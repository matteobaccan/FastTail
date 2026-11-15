// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! AUTOSAR DLT storage files (`.dlt`, the binary logs of automotive ECUs) as streams.
//!
//! Each stored message (storage header `DLT\x01` + time + ECU id, then the standard
//! header, the optional extended header and the payload) becomes one text line the level
//! and timestamp detectors and the logfmt field parser read:
//! `2026-11-15T10:00:00.123456Z INFO ecu=ECU1 apid=APP1 ctid=CTX1 tms=12.3456 type=log
//! msg="engine started 42"`.
//!
//! Verbose payloads are decoded argument by argument (booleans, integers, floats, strings,
//! raw data); non-verbose ones (without the FIBEX description) are written as their
//! message id and the payload in hex. A damaged record is skipped up to the next storage
//! header. The file is read once, in blocks, through the standard-input copier
//! (`StdinStream`) like the other decoded sources: it is never held in memory.

use crate::stdin_source::{Settings, StdinStream};
use crate::tail_engine::{TailEngine, WakeFn};
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

/// The first bytes of every stored message.
pub const MAGIC: &[u8; 4] = b"DLT\x01";
/// Bytes of a payload written as hex at most (longer ones end with `…`).
const MAX_HEX: usize = 256;
/// Bytes of decoded text kept per message (longer ones are cut with `…`).
const MAX_TEXT: usize = 16 * 1024;

/// Whether the file at `path` is a DLT storage file (by its magic bytes).
pub fn is_dlt_file(path: &Path) -> bool {
    let mut head = [0u8; 4];
    std::fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut head))
        .is_ok()
        && &head == MAGIC
}

/// The level word of a log message's type info (`MTIN`).
fn level_word(mtin: u8) -> &'static str {
    match mtin {
        1 => "FATAL",
        2 => "ERROR",
        3 => "WARN",
        4 => "INFO",
        5 => "DEBUG",
        6 => "VERBOSE",
        _ => "-",
    }
}

/// The message type (`MSTP`) as written in `type=`.
fn type_word(mstp: u8) -> &'static str {
    match mstp {
        0 => "log",
        1 => "app_trace",
        2 => "nw_trace",
        3 => "control",
        _ => "unknown",
    }
}

/// A 4-byte id as text, without its trailing NULs.
fn id_text(id: &[u8]) -> String {
    let end = id.iter().position(|&b| b == 0).unwrap_or(id.len());
    String::from_utf8_lossy(&id[..end]).into_owned()
}

/// Reads integers in the payload's byte order.
struct Cursor<'a> {
    b: &'a [u8],
    pos: usize,
    big: bool,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.b.get(self.pos..self.pos.checked_add(n)?)?;
        self.pos += n;
        Some(s)
    }

    fn uint(&mut self, n: usize) -> Option<u128> {
        let s = self.take(n)?;
        let mut v = 0u128;
        if self.big {
            for &b in s {
                v = (v << 8) | b as u128;
            }
        } else {
            for &b in s.iter().rev() {
                v = (v << 8) | b as u128;
            }
        }
        Some(v)
    }

    fn u16(&mut self) -> Option<u16> {
        self.uint(2).map(|v| v as u16)
    }

    fn u32(&mut self) -> Option<u32> {
        self.uint(4).map(|v| v as u32)
    }

    fn rest(&self) -> &'a [u8] {
        &self.b[self.pos.min(self.b.len())..]
    }
}

/// The bytes of a type info's length field (`TYLE`).
fn type_len(tyle: u32) -> Option<usize> {
    match tyle {
        1 => Some(1),
        2 => Some(2),
        3 => Some(4),
        4 => Some(8),
        5 => Some(16),
        _ => None,
    }
}

/// One verbose argument as text, or `None` when it cannot be read (an unknown type, a
/// short payload): the caller writes the rest as hex.
fn argument(c: &mut Cursor) -> Option<String> {
    let info = c.u32()?;
    let tyle = info & 0x0f;
    let (bool_, sint, uint, float) = (
        info & 0x10 != 0,
        info & 0x20 != 0,
        info & 0x40 != 0,
        info & 0x80 != 0,
    );
    let (array, string, raw) = (info & 0x100 != 0, info & 0x200 != 0, info & 0x400 != 0);
    let (vari, fixp, strct) = (info & 0x800 != 0, info & 0x1000 != 0, info & 0x4000 != 0);
    if array || fixp || strct {
        return None;
    }
    // A named argument: its name (and, for numbers, its unit) come first.
    let mut name = String::new();
    let mut unit = String::new();
    if vari {
        let name_len = c.u16()? as usize;
        let unit_len = if string || raw || bool_ {
            0
        } else {
            c.u16()? as usize
        };
        name = id_text(c.take(name_len)?);
        unit = id_text(c.take(unit_len)?);
    }
    let value = if string {
        let len = c.u16()? as usize;
        id_text(c.take(len)?)
    } else if raw {
        let len = c.u16()? as usize;
        hex(c.take(len)?)
    } else if bool_ {
        (c.take(1)?[0] != 0).to_string()
    } else if uint {
        let n = type_len(tyle)?;
        c.uint(n)?.to_string()
    } else if sint {
        let n = type_len(tyle)?;
        let v = c.uint(n)?;
        let shift = 128 - 8 * n as u32;
        (((v << shift) as i128) >> shift).to_string()
    } else if float {
        match type_len(tyle)? {
            4 => f32::from_bits(c.u32()?).to_string(),
            8 => f64::from_bits(c.uint(8)? as u64).to_string(),
            _ => return None,
        }
    } else {
        return None;
    };
    Some(match (name.is_empty(), unit.is_empty()) {
        (true, _) => value,
        (false, true) => format!("{name}={value}"),
        (false, false) => format!("{name}={value}{unit}"),
    })
}

/// Bytes as `0a 1b 2c`, at most `MAX_HEX` of them.
fn hex(b: &[u8]) -> String {
    let mut out = String::with_capacity(b.len().min(MAX_HEX) * 3);
    for (i, byte) in b.iter().take(MAX_HEX).enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(&format!("{byte:02x}"));
    }
    if b.len() > MAX_HEX {
        out.push_str(" …");
    }
    out
}

/// `text` as a logfmt value: quoted, with `"` and `\` escaped and line breaks as `\n`.
fn quoted(text: &str, out: &mut String) {
    out.push('"');
    let mut kept = 0;
    for ch in text.chars() {
        if kept >= MAX_TEXT {
            out.push('…');
            break;
        }
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
        kept += ch.len_utf8();
    }
    out.push('"');
}

/// The text line of one message: `stored` is the storage header's time (seconds and
/// microseconds) and ECU id, `msg` the standard header onwards. `None` when the standard
/// header is shorter than it says.
pub fn format_message(secs: u32, micros: u32, storage_ecu: &[u8], msg: &[u8]) -> Option<String> {
    let htyp = *msg.first()?;
    let (ueh, msbf, weid, wsid, wtms) = (
        htyp & 0x01 != 0,
        htyp & 0x02 != 0,
        htyp & 0x04 != 0,
        htyp & 0x08 != 0,
        htyp & 0x10 != 0,
    );
    // The standard header's own fields are always big-endian.
    let mut c = Cursor {
        b: msg,
        pos: 4,
        big: true,
    };
    let ecu = if weid {
        id_text(c.take(4)?)
    } else {
        id_text(storage_ecu)
    };
    if wsid {
        c.take(4)?;
    }
    let tms = if wtms { Some(c.u32()?) } else { None };
    let (mut verbose, mut mstp, mut mtin, mut noar) = (false, 0u8, 0u8, 0u8);
    let (mut apid, mut ctid) = (String::new(), String::new());
    if ueh {
        let msin = c.take(1)?[0];
        verbose = msin & 0x01 != 0;
        mstp = (msin >> 1) & 0x07;
        mtin = (msin >> 4) & 0x0f;
        noar = c.take(1)?[0];
        apid = id_text(c.take(4)?);
        ctid = id_text(c.take(4)?);
    }
    let level = if ueh && mstp == 0 {
        level_word(mtin)
    } else {
        "-"
    };
    let mut line = String::with_capacity(96 + msg.len());
    let stamp = crate::timestamp::format_millis(secs as i64 * 1000).replacen(' ', "T", 1);
    line.push_str(&format!("{stamp}.{:06}Z {level}", micros.min(999_999)));
    for (key, value) in [("ecu", &ecu), ("apid", &apid), ("ctid", &ctid)] {
        if !value.is_empty() {
            line.push_str(&format!(" {key}="));
            let plain = value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_');
            if plain {
                line.push_str(value);
            } else {
                quoted(value, &mut line);
            }
        }
    }
    if let Some(tms) = tms {
        line.push_str(&format!(" tms={}.{:04}", tms / 10_000, tms % 10_000));
    }
    line.push_str(" type=");
    line.push_str(if ueh { type_word(mstp) } else { "log" });
    let mut payload = Cursor {
        b: c.rest(),
        pos: 0,
        big: msbf,
    };
    let text = if ueh && verbose {
        let mut parts = Vec::with_capacity(noar as usize);
        for _ in 0..noar {
            let start = payload.pos;
            match argument(&mut payload) {
                Some(arg) => parts.push(arg),
                None => {
                    payload.pos = start;
                    parts.push(format!("[{}]", hex(payload.rest())));
                    break;
                }
            }
        }
        parts.join(" ")
    } else {
        // Non-verbose: the message id, then the arguments as hex.
        match payload.u32() {
            Some(id) => {
                let rest = payload.rest();
                if rest.is_empty() {
                    format!("[id {id}]")
                } else {
                    format!("[id {id}] {}", hex(rest))
                }
            }
            None => hex(payload.rest()),
        }
    };
    line.push_str(" msg=");
    quoted(&text, &mut line);
    Some(line)
}

/// Turns a DLT storage file into text lines, block by block: the copier's input.
pub struct DltText<R: Read> {
    input: BufReader<R>,
    out: Vec<u8>,
    pos: usize,
    done: bool,
    /// Messages decoded and damaged records skipped so far.
    pub messages: u64,
    pub skipped: u64,
}

impl<R: Read> DltText<R> {
    pub fn new(input: R) -> Self {
        Self {
            input: BufReader::with_capacity(256 * 1024, input),
            out: Vec::new(),
            pos: 0,
            done: false,
            messages: 0,
            skipped: 0,
        }
    }

    /// Drops bytes until a storage header starts the buffer. Returns false at the end of
    /// the input. A header split across two reads of a damaged region may be missed.
    fn sync(&mut self) -> std::io::Result<bool> {
        let mut dropped = false;
        loop {
            let buf = self.input.fill_buf()?;
            if buf.is_empty() {
                return Ok(false);
            }
            if let Some(i) = memchr::memmem::find(buf, MAGIC) {
                self.input.consume(i);
                if i > 0 || dropped {
                    self.skipped += 1;
                }
                return Ok(true);
            }
            let n = buf.len().saturating_sub(MAGIC.len() - 1).max(1);
            self.input.consume(n);
            dropped = true;
        }
    }

    /// Reads the next message as a text line into `out`. Returns false at the end.
    fn next_line(&mut self) -> std::io::Result<bool> {
        loop {
            if !self.sync()? {
                return Ok(false);
            }
            let mut storage = [0u8; 16];
            let mut start = [0u8; 4];
            if read_full(&mut self.input, &mut storage)? < storage.len()
                || read_full(&mut self.input, &mut start)? < start.len()
            {
                return Ok(false);
            }
            let len = u16::from_be_bytes([start[2], start[3]]) as usize;
            if len < start.len() {
                self.skipped += 1;
                continue;
            }
            let mut msg = vec![0u8; len];
            msg[..4].copy_from_slice(&start);
            let got = read_full(&mut self.input, &mut msg[4..])?;
            msg.truncate(4 + got);
            let secs = u32::from_le_bytes([storage[4], storage[5], storage[6], storage[7]]);
            let micros = u32::from_le_bytes([storage[8], storage[9], storage[10], storage[11]]);
            match format_message(secs, micros, &storage[12..16], &msg) {
                Some(line) => {
                    self.out.extend_from_slice(line.as_bytes());
                    self.out.push(b'\n');
                    self.messages += 1;
                    return Ok(true);
                }
                None => self.skipped += 1,
            }
        }
    }
}

/// Fills `buf` as far as the input goes; the bytes read.
fn read_full(input: &mut impl Read, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match input.read(&mut buf[n..]) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(n)
}

impl<R: Read> Read for DltText<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        while self.pos >= self.out.len() && !self.done {
            self.out.clear();
            self.pos = 0;
            // About 64 KiB of lines per refill.
            while self.out.len() < 64 * 1024 {
                if !self.next_line()? {
                    self.done = true;
                    break;
                }
            }
        }
        let n = buf.len().min(self.out.len() - self.pos);
        buf[..n].copy_from_slice(&self.out[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

/// The copier of a DLT stream, owned by its engine (`TailEngine::dlt`).
pub struct DltStream {
    pub copier: StdinStream,
}

/// Opens the DLT file at `path` as a stream: its messages decoded into a spool, read once.
pub fn open_engine(
    path: &Path,
    settings: &Settings,
    wake: Option<WakeFn>,
) -> std::io::Result<TailEngine> {
    let file = std::fs::File::open(path)?;
    let copier = StdinStream::start_named(DltText::new(file), settings, wake.clone(), "dlt")?;
    let mut engine = match wake {
        Some(wake) => TailEngine::open_with_wake(copier.spool_path(), wake)?,
        None => TailEngine::open(copier.spool_path())?,
    };
    engine.path = path.to_path_buf();
    engine.follow_tail = false;
    engine.dlt = Some(DltStream { copier });
    Ok(engine)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stored verbose log message with `args` (type info + data, little-endian).
    fn message(level: u8, args: &[(u32, &[u8])]) -> Vec<u8> {
        let mut payload = Vec::new();
        for (info, data) in args {
            payload.extend_from_slice(&info.to_le_bytes());
            payload.extend_from_slice(data);
        }
        // HTYP: extended header, ECU id, timestamp, version 1.
        let htyp = 0x01 | 0x04 | 0x10 | (1 << 5);
        let mut msg = vec![htyp, 7, 0, 0];
        msg.extend_from_slice(b"ECU1");
        msg.extend_from_slice(&123_456u32.to_be_bytes());
        // MSIN: verbose, log, level.
        msg.push(0x01 | (level << 4));
        msg.push(args.len() as u8);
        msg.extend_from_slice(b"APP1");
        msg.extend_from_slice(b"CT\0\0");
        msg.extend_from_slice(&payload);
        let len = msg.len() as u16;
        msg[2..4].copy_from_slice(&len.to_be_bytes());
        let mut out = MAGIC.to_vec();
        out.extend_from_slice(&1_763_200_800u32.to_le_bytes());
        out.extend_from_slice(&250_000u32.to_le_bytes());
        out.extend_from_slice(b"ECU1");
        out.extend_from_slice(&msg);
        out
    }

    fn string_arg(text: &str) -> Vec<u8> {
        let mut data = ((text.len() + 1) as u16).to_le_bytes().to_vec();
        data.extend_from_slice(text.as_bytes());
        data.push(0);
        data
    }

    fn decode(bytes: &[u8]) -> (String, u64) {
        let mut text = DltText::new(bytes);
        let mut out = String::new();
        text.read_to_string(&mut out).unwrap();
        (out, text.skipped)
    }

    #[test]
    fn verbose_arguments_become_the_message_text() {
        let s = string_arg("engine started");
        let bytes = message(
            4,
            &[
                (0x200, &s),
                (0x43, &42u32.to_le_bytes()),
                (0x22, &(-3i16).to_le_bytes()),
                (0x10 | 1, &[1]),
                (0x84, &1.5f64.to_le_bytes()),
            ],
        );
        let (out, skipped) = decode(&bytes);
        assert_eq!(skipped, 0);
        assert_eq!(
            out,
            "2025-11-15T10:00:00.250000Z INFO ecu=ECU1 apid=APP1 ctid=CT tms=12.3456 \
             type=log msg=\"engine started 42 -3 true 1.5\"\n"
        );
    }

    #[test]
    fn levels_and_types_map_to_words() {
        let (out, _) = decode(&message(2, &[]));
        assert!(out.contains(" ERROR ecu=ECU1"), "{out}");
        assert_eq!(level_word(6), "VERBOSE");
        assert_eq!(type_word(3), "control");
    }

    #[test]
    fn non_verbose_payloads_are_written_as_id_and_hex() {
        let mut bytes = message(4, &[]);
        // Clear the verbose bit and append a message id and two bytes.
        let msin = MAGIC.len() + 12 + 4 + 4 + 4;
        bytes[msin] &= !0x01;
        bytes.extend_from_slice(&77u32.to_le_bytes());
        bytes.extend_from_slice(&[0xab, 0x01]);
        let len = (bytes.len() - 16) as u16;
        bytes[16 + 2..16 + 4].copy_from_slice(&len.to_be_bytes());
        let (out, _) = decode(&bytes);
        assert!(out.ends_with("msg=\"[id 77] ab 01\"\n"), "{out}");
    }

    #[test]
    fn damaged_bytes_are_skipped_to_the_next_message() {
        let mut bytes = b"garbage".to_vec();
        bytes.extend(message(3, &[(0x200, &string_arg("a \"q\""))]));
        bytes.extend_from_slice(b"\xff\xff");
        bytes.extend(message(5, &[(0x200, &string_arg("b"))]));
        let (out, skipped) = decode(&bytes);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains(" WARN ") && lines[0].ends_with(r#"msg="a \"q\"""#));
        assert!(lines[1].contains(" DEBUG ") && lines[1].ends_with(r#"msg="b""#));
        assert_eq!(skipped, 2);
    }

    #[test]
    fn a_truncated_last_message_still_gives_its_header() {
        let mut bytes = message(4, &[(0x200, &string_arg("cut here"))]);
        bytes.truncate(bytes.len() - 4);
        let (out, _) = decode(&bytes);
        assert!(out.starts_with("2025-11-15T10:00:00.250000Z INFO"), "{out}");
    }

    #[test]
    fn files_are_recognised_by_their_magic() {
        let dir = tempfile::tempdir().unwrap();
        let dlt = dir.path().join("trace.bin");
        std::fs::write(&dlt, message(4, &[])).unwrap();
        let text = dir.path().join("app.dlt");
        std::fs::write(&text, b"plain text\n").unwrap();
        assert!(is_dlt_file(&dlt));
        assert!(!is_dlt_file(&text));
    }
}
