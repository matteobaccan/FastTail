// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The ASM view (openspec/changes/disassembly-view): x86 machine code of any file, read
//! in place. A screen is decoded from an offset forward, at most `rows * 15` bytes, so a
//! file of any size costs the same; moving up re-synchronises from up to 64 bytes before
//! (`previous_start`). PE and ELF executables are recognised from their header: the view
//! then shows virtual addresses, starts at the entry point and decodes only the
//! executable sections, the others as `db` data. Invalid bytes are `db 0xNN` rows, so the
//! view never stops.

use iced_x86::{Decoder, DecoderOptions, Formatter, Instruction, IntelFormatter};

/// The longest x86 instruction.
pub const MAX_INSTRUCTION: usize = 15;
/// How far back `previous_start` looks for a chain of instructions.
pub const RESYNC_WINDOW: usize = 64;
/// Rows a copy holds at most.
pub const MAX_COPY_ROWS: usize = 100_000;
/// Bytes of a data row (a non-executable section).
const DATA_ROW: usize = 8;

/// The decoding mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Arch {
    X86_16,
    X86_32,
    #[default]
    X86_64,
}

impl Arch {
    pub const ALL: [Arch; 3] = [Arch::X86_16, Arch::X86_32, Arch::X86_64];

    fn bitness(self) -> u32 {
        match self {
            Arch::X86_16 => 16,
            Arch::X86_32 => 32,
            Arch::X86_64 => 64,
        }
    }

    /// The name shown and saved (`disasm_arch=`).
    pub fn name(self) -> &'static str {
        match self {
            Arch::X86_16 => "x86-16",
            Arch::X86_32 => "x86-32",
            Arch::X86_64 => "x86-64",
        }
    }

    pub fn from_name(name: &str) -> Option<Arch> {
        Arch::ALL.into_iter().find(|a| a.name() == name.trim())
    }

    /// The next one, for a key that cycles them.
    pub fn next(self) -> Arch {
        match self {
            Arch::X86_16 => Arch::X86_32,
            Arch::X86_32 => Arch::X86_64,
            Arch::X86_64 => Arch::X86_16,
        }
    }
}

/// A section of an executable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub name: String,
    /// Where its bytes are in the file, and how many.
    pub file_offset: u64,
    pub size: u64,
    /// Its address once loaded.
    pub address: u64,
    pub executable: bool,
}

/// What the header of a PE or ELF file says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Executable {
    /// `PE` or `ELF`.
    pub format: &'static str,
    pub arch: Arch,
    /// The file offset of the entry point, when a section holds it.
    pub entry_offset: Option<u64>,
    pub sections: Vec<Section>,
}

impl Executable {
    /// The section holding file offset `offset`.
    pub fn section_at(&self, offset: u64) -> Option<&Section> {
        self.sections
            .iter()
            .find(|s| offset >= s.file_offset && offset < s.file_offset + s.size)
    }

    /// The address of file offset `offset`, inside a section.
    pub fn address_of(&self, offset: u64) -> Option<u64> {
        self.section_at(offset)
            .map(|s| s.address + (offset - s.file_offset))
    }

    /// The file offset of address `address`, inside a section.
    pub fn offset_of(&self, address: u64) -> Option<u64> {
        self.sections
            .iter()
            .find(|s| address >= s.address && address < s.address + s.size)
            .map(|s| s.file_offset + (address - s.address))
    }
}

/// One row of the view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsmRow {
    pub offset: u64,
    /// The address shown: the virtual address in an executable, else the offset.
    pub address: u64,
    pub bytes: Vec<u8>,
    /// The instruction in Intel syntax, or `db 0xNN, ...`.
    pub text: String,
    /// Data (`db`): an invalid byte, or bytes of a section that is not code.
    pub data: bool,
    /// The section name, on the first row of a section.
    pub section: Option<String>,
}

impl AsmRow {
    /// The row as copied: `address  bytes  instruction`.
    pub fn copy_text(&self) -> String {
        format!(
            "{:08X}  {:<30}{}",
            self.address,
            hex_bytes(&self.bytes),
            self.text
        )
    }
}

/// `55 48 89 E5`.
pub fn hex_bytes(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn data_row(offset: u64, address: u64, bytes: &[u8]) -> AsmRow {
    let list = bytes
        .iter()
        .map(|b| format!("0x{b:02X}"))
        .collect::<Vec<_>>()
        .join(", ");
    AsmRow {
        offset,
        address,
        bytes: bytes.to_vec(),
        text: format!("db {list}"),
        data: true,
        section: None,
    }
}

/// Decodes up to `max_rows` rows of `bytes`, which start at file offset `offset`. The
/// last instruction is kept only when all its bytes are there, unless `at_end` (the end
/// of the file), where its bytes become `db` rows.
pub fn decode(
    bytes: &[u8],
    offset: u64,
    arch: Arch,
    exe: Option<&Executable>,
    max_rows: usize,
    at_end: bool,
) -> Vec<AsmRow> {
    let mut rows = Vec::new();
    let mut formatter = IntelFormatter::new();
    let mut text = String::new();
    let mut pos = 0usize;
    let mut last_section: Option<String> = None;
    while pos < bytes.len() && rows.len() < max_rows {
        let at = offset + pos as u64;
        let section = exe.and_then(|e| e.section_at(at));
        let address = exe.and_then(|e| e.address_of(at)).unwrap_or(at);
        // A section starting inside the bytes stops the decoding of the one before it.
        let limit = exe
            .and_then(|e| {
                e.sections
                    .iter()
                    .map(|s| s.file_offset)
                    .filter(|&s| s > at)
                    .min()
            })
            .map_or(bytes.len(), |next| {
                ((next - offset) as usize).min(bytes.len())
            });
        let named = section.map(|s| s.name.clone());
        let first_of_section = named.is_some() && named != last_section;
        last_section = named.clone();
        let mut row = if section.is_some_and(|s| !s.executable) {
            let end = (pos + DATA_ROW).min(limit);
            data_row(at, address, &bytes[pos..end])
        } else {
            let window = &bytes[pos..limit];
            let mut decoder =
                Decoder::with_ip(arch.bitness(), window, address, DecoderOptions::NONE);
            let mut instruction = Instruction::default();
            decoder.decode_out(&mut instruction);
            let len = instruction.len();
            if instruction.is_invalid() || len == 0 {
                // Cut short at the end of what was read: wait for the rest, unless the
                // file ends here.
                if window.len() < MAX_INSTRUCTION && !at_end && limit == bytes.len() {
                    break;
                }
                data_row(at, address, &bytes[pos..pos + 1])
            } else {
                text.clear();
                formatter.format(&instruction, &mut text);
                AsmRow {
                    offset: at,
                    address,
                    bytes: window[..len].to_vec(),
                    text: text.clone(),
                    data: false,
                    section: None,
                }
            }
        };
        if first_of_section {
            row.section = named;
        }
        pos += row.bytes.len();
        rows.push(row);
    }
    rows
}

/// The start of the row before `offset`: from each candidate in the `RESYNC_WINDOW`
/// bytes before it, the instruction chain that lands exactly on `offset` with the fewest
/// invalid bytes (and, between equals, the longest chain), else one byte back. `before`
/// holds the bytes that precede `offset`, at most `RESYNC_WINDOW`.
pub fn previous_start(before: &[u8], offset: u64, arch: Arch) -> u64 {
    if before.is_empty() {
        return offset;
    }
    let start = offset - before.len() as u64;
    let mut best: Option<(usize, usize, u64)> = None; // (invalid, -rows, last start)
    for skip in 0..before.len() {
        let mut decoder =
            Decoder::with_ip(arch.bitness(), &before[skip..], 0, DecoderOptions::NONE);
        let mut instruction = Instruction::default();
        let (mut pos, mut invalid, mut rows, mut last) = (skip, 0usize, 0usize, skip);
        while pos < before.len() {
            decoder.set_position(pos - skip).ok();
            decoder.decode_out(&mut instruction);
            last = pos;
            if instruction.is_invalid() || instruction.len() == 0 {
                invalid += 1;
                pos += 1;
            } else {
                pos += instruction.len();
            }
            rows += 1;
        }
        if pos == before.len() {
            let key = (invalid, usize::MAX - rows);
            if best.is_none_or(|(i, r, _)| key < (i, r)) {
                best = Some((key.0, key.1, start + last as u64));
            }
        }
    }
    best.map_or(offset.saturating_sub(1), |(_, _, at)| at)
}

/// Bytes read from the start of a file to recognise an executable.
pub const HEADER_BYTES: usize = 4096;

/// The ASM view of a stream: where it is and what it found. Reads go through `read`
/// (offset, length -> bytes, fewer at the end of the file), so nothing of the file is
/// kept but the header of an executable.
#[derive(Debug, Clone, Default)]
pub struct AsmView {
    /// The architecture chosen for the stream (`disasm_arch=`); a recognised executable
    /// overrides it while it is open.
    pub arch: Arch,
    /// The offset of the first row.
    pub top: u64,
    /// The selected rows, as the offsets of the first and the last (in click order).
    pub selection: Option<(u64, u64)>,
    /// Whether the view was placed since it was entered (see `enter`).
    placed: bool,
    /// The header read, with the file size it was read at (a smaller size later means
    /// the file was truncated or replaced: read it again).
    header: Option<(u64, Option<Executable>)>,
}

impl AsmView {
    /// The executable the file is, read on first use.
    pub fn executable(
        &mut self,
        read: &dyn Fn(u64, usize) -> Vec<u8>,
        size: u64,
    ) -> Option<&Executable> {
        if self.header.as_ref().is_none_or(|(at, _)| size < *at) {
            let head = read(0, HEADER_BYTES);
            self.header = Some((size, executable(&head)));
        }
        self.header.as_ref().and_then(|(_, e)| e.as_ref())
    }

    /// The executable already read, without reading.
    pub fn known_executable(&self) -> Option<&Executable> {
        self.header.as_ref().and_then(|(_, e)| e.as_ref())
    }

    /// The architecture decoding uses: the executable's, else the stream's.
    pub fn arch_in_use(&self) -> Arch {
        self.known_executable().map_or(self.arch, |e| e.arch)
    }

    /// Where the view starts on a file: its entry point, else 0.
    pub fn home(&mut self, read: &dyn Fn(u64, usize) -> Vec<u8>, size: u64) -> u64 {
        self.executable(read, size)
            .and_then(|e| e.entry_offset)
            .filter(|&o| o < size)
            .unwrap_or(0)
    }

    /// Enters the view at `at` (a search hit, the HEX position), else at the entry point
    /// or 0, unless it is already placed and `at` is none.
    pub fn enter(&mut self, at: Option<u64>, read: &dyn Fn(u64, usize) -> Vec<u8>, size: u64) {
        let home = self.home(read, size);
        match at {
            Some(at) => self.top = at.min(size.saturating_sub(1)),
            None if !self.placed || self.top >= size => self.top = home,
            None => {}
        }
        self.placed = true;
        self.selection = None;
    }

    /// The rows of a screen `rows` high from `top`. A file that shrank under `top` puts
    /// the view back at its start (the entry point, or 0).
    pub fn rows(
        &mut self,
        read: &dyn Fn(u64, usize) -> Vec<u8>,
        size: u64,
        rows: usize,
    ) -> Vec<AsmRow> {
        if size == 0 {
            return Vec::new();
        }
        self.executable(read, size);
        if self.top >= size {
            self.top = self.home(read, size);
            self.selection = None;
        }
        self.rows_from(self.top, read, size, rows)
    }

    fn rows_from(
        &self,
        from: u64,
        read: &dyn Fn(u64, usize) -> Vec<u8>,
        size: u64,
        rows: usize,
    ) -> Vec<AsmRow> {
        let bytes = read(from, rows.max(1) * MAX_INSTRUCTION);
        let at_end = from + bytes.len() as u64 >= size;
        decode(
            &bytes,
            from,
            self.arch_in_use(),
            self.known_executable(),
            rows,
            at_end,
        )
    }

    /// Moves down `n` rows (a page is `n` = rows of the screen), never past the last row.
    pub fn down(&mut self, n: usize, read: &dyn Fn(u64, usize) -> Vec<u8>, size: u64) {
        let rows = self.rows_from(self.top, read, size, n + 1);
        if let Some(row) = rows.get(n.min(rows.len().saturating_sub(1))) {
            self.top = row.offset;
        }
    }

    /// Moves up `n` rows, re-synchronising each one.
    pub fn up(&mut self, n: usize, read: &dyn Fn(u64, usize) -> Vec<u8>, size: u64) {
        self.executable(read, size);
        for _ in 0..n {
            if self.top == 0 {
                break;
            }
            self.top = self.previous_row(self.top, read);
        }
    }

    fn previous_row(&self, at: u64, read: &dyn Fn(u64, usize) -> Vec<u8>) -> u64 {
        // In a data section rows are `DATA_ROW` bytes from the section start; above the
        // first byte of a section, the row before belongs to the one before it.
        if let Some(s) = self.known_executable().and_then(|e| e.section_at(at - 1)) {
            if !s.executable {
                let into = at - 1 - s.file_offset;
                return s.file_offset + into / DATA_ROW as u64 * DATA_ROW as u64;
            }
        }
        let floor = self
            .known_executable()
            .and_then(|e| e.section_at(at - 1))
            .map_or(0, |s| s.file_offset);
        let from = at.saturating_sub(RESYNC_WINDOW as u64).max(floor);
        let before = read(from, (at - from) as usize);
        previous_start(&before, from + before.len() as u64, self.arch_in_use()).max(from)
    }

    /// Puts the end of the file on the last row of a screen `rows` high (follow).
    pub fn bottom(&mut self, rows: usize, read: &dyn Fn(u64, usize) -> Vec<u8>, size: u64) {
        if size == 0 {
            return;
        }
        self.executable(read, size);
        self.top = size;
        self.up(rows.max(1), read, size);
    }

    /// Goes to `target`: `entry`, a `0x` address (a file offset when the file is not an
    /// executable, or the address is outside its sections), or a decimal offset. False
    /// when it is none of them or past the end.
    pub fn go_to(&mut self, target: &str, read: &dyn Fn(u64, usize) -> Vec<u8>, size: u64) -> bool {
        let target = target.trim();
        let exe = self.executable(read, size).cloned();
        let offset = if target.eq_ignore_ascii_case("entry") {
            exe.and_then(|e| e.entry_offset)
        } else if let Some(hex) = target
            .strip_prefix("0x")
            .or_else(|| target.strip_prefix("0X"))
        {
            u64::from_str_radix(hex, 16)
                .ok()
                .map(|v| exe.and_then(|e| e.offset_of(v)).unwrap_or(v))
        } else {
            target.parse::<u64>().ok()
        };
        match offset.filter(|&o| o < size) {
            Some(o) => {
                self.top = o;
                self.selection = None;
                true
            }
            None => false,
        }
    }

    /// The selected rows as copied, one `address  bytes  instruction` line each (at most
    /// `MAX_COPY_ROWS`).
    pub fn selection_text(
        &self,
        read: &dyn Fn(u64, usize) -> Vec<u8>,
        size: u64,
    ) -> Option<String> {
        let (a, b) = self.selection?;
        let (from, to) = (a.min(b), a.max(b));
        let mut out = String::new();
        let (mut at, mut count) = (from, 0usize);
        while at < size && count < MAX_COPY_ROWS {
            let rows = self.rows_from(at, read, size, 256);
            let Some(last) = rows.last() else { break };
            at = last.offset + last.bytes.len() as u64;
            for row in &rows {
                if row.offset > to {
                    return Some(out);
                }
                out.push_str(&row.copy_text());
                out.push('\n');
                count += 1;
            }
        }
        Some(out)
    }

    /// The architecture shown in the view's bar: the executable's format with it, or the
    /// stream's choice.
    pub fn label(&self) -> String {
        match self.known_executable() {
            Some(e) => format!("{} {}", e.format, e.arch.name()),
            None => self.arch.name().to_string(),
        }
    }
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn u64_at(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

/// Reads the header of a PE or ELF executable (little endian, 32 or 64 bit) from the
/// first bytes of a file; `None` for anything else or a malformed header.
pub fn executable(head: &[u8]) -> Option<Executable> {
    if head.starts_with(b"MZ") {
        pe(head)
    } else if head.starts_with(b"\x7FELF") {
        elf(head)
    } else {
        None
    }
}

fn pe(b: &[u8]) -> Option<Executable> {
    let pe = u32_at(b, 0x3C)? as usize;
    if b.get(pe..pe + 4)? != b"PE\0\0" {
        return None;
    }
    let machine = u16_at(b, pe + 4)?;
    let arch = match machine {
        0x14C => Arch::X86_32,
        0x8664 => Arch::X86_64,
        _ => return None,
    };
    let sections = u16_at(b, pe + 6)? as usize;
    let optional_size = u16_at(b, pe + 20)? as usize;
    let optional = pe + 24;
    let entry_rva = u32_at(b, optional + 16)? as u64;
    let image_base = match u16_at(b, optional)? {
        0x10B => u32_at(b, optional + 28)? as u64,
        0x20B => u64_at(b, optional + 24)?,
        _ => return None,
    };
    let table = optional + optional_size;
    let mut list = Vec::new();
    for i in 0..sections.min(96) {
        let at = table + i * 40;
        let raw = b.get(at..at + 8)?;
        let name = String::from_utf8_lossy(raw)
            .trim_end_matches('\0')
            .to_string();
        let virtual_size = u32_at(b, at + 8)? as u64;
        let rva = u32_at(b, at + 12)? as u64;
        let raw_size = u32_at(b, at + 16)? as u64;
        let raw_offset = u32_at(b, at + 20)? as u64;
        let flags = u32_at(b, at + 36)?;
        if raw_size == 0 {
            continue;
        }
        list.push(Section {
            name,
            file_offset: raw_offset,
            size: raw_size.min(if virtual_size == 0 {
                raw_size
            } else {
                virtual_size.max(raw_size)
            }),
            address: image_base + rva,
            executable: flags & 0x2000_0000 != 0 || flags & 0x20 != 0,
        });
    }
    let mut exe = Executable {
        format: "PE",
        arch,
        entry_offset: None,
        sections: list,
    };
    exe.entry_offset = exe.offset_of(image_base + entry_rva);
    Some(exe)
}

fn elf(b: &[u8]) -> Option<Executable> {
    let class = *b.get(4)?;
    if *b.get(5)? != 1 {
        return None; // big endian
    }
    let machine = u16_at(b, 18)?;
    let (arch, entry, shoff, shentsize, shnum, shstrndx) = match (class, machine) {
        (1, 3) => (
            Arch::X86_32,
            u32_at(b, 24)? as u64,
            u32_at(b, 32)? as usize,
            u16_at(b, 46)? as usize,
            u16_at(b, 48)? as usize,
            u16_at(b, 50)? as usize,
        ),
        (2, 62) => (
            Arch::X86_64,
            u64_at(b, 24)?,
            u64_at(b, 40)? as usize,
            u16_at(b, 58)? as usize,
            u16_at(b, 60)? as usize,
            u16_at(b, 62)? as usize,
        ),
        _ => return None,
    };
    let header = |i: usize| -> Option<(u32, u64, u64, u64, u64)> {
        let at = shoff.checked_add(i.checked_mul(shentsize)?)?;
        Some(if class == 1 {
            (
                u32_at(b, at)?,
                u32_at(b, at + 8)? as u64,
                u32_at(b, at + 12)? as u64,
                u32_at(b, at + 16)? as u64,
                u32_at(b, at + 20)? as u64,
            )
        } else {
            (
                u32_at(b, at)?,
                u64_at(b, at + 8)?,
                u64_at(b, at + 16)?,
                u64_at(b, at + 24)?,
                u64_at(b, at + 32)?,
            )
        })
    };
    let names = header(shstrndx).map(|(_, _, _, off, size)| (off as usize, size as usize));
    let mut list = Vec::new();
    for i in 0..shnum.min(256) {
        let Some((name, flags, address, offset, size)) = header(i) else {
            break;
        };
        let kind = u32_at(b, shoff + i * shentsize + 4).unwrap_or(0);
        // Sections with bytes in the file only (not NOBITS = 8, not NULL).
        if kind == 0 || kind == 8 || size == 0 || address == 0 {
            continue;
        }
        let name = names
            .and_then(|(at, len)| {
                let start = at.checked_add(name as usize)?;
                let rest = b.get(start..at.checked_add(len)?)?;
                let end = rest.iter().position(|&c| c == 0).unwrap_or(rest.len());
                Some(String::from_utf8_lossy(&rest[..end]).to_string())
            })
            .unwrap_or_default();
        list.push(Section {
            name,
            file_offset: offset,
            size,
            address,
            executable: flags & 0x4 != 0,
        });
    }
    let mut exe = Executable {
        format: "ELF",
        arch,
        entry_offset: None,
        sections: list,
    };
    exe.entry_offset = exe.offset_of(entry);
    Some(exe)
}

#[cfg(test)]
mod tests {
    use super::*;

    // push rbp; mov rbp, rsp; sub rsp, 0x10; mov eax, 0; leave; ret
    const PROLOGUE: [u8; 15] = [
        0x55, 0x48, 0x89, 0xE5, 0x48, 0x83, 0xEC, 0x10, 0xB8, 0x00, 0x00, 0x00, 0x00, 0xC9, 0xC3,
    ];

    #[test]
    fn a_prologue_decodes_row_by_row() {
        let rows = decode(&PROLOGUE, 0x400, Arch::X86_64, None, 10, true);
        let text: Vec<&str> = rows.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(
            text,
            [
                "push rbp",
                "mov rbp,rsp",
                "sub rsp,10h",
                "mov eax,0",
                "leave",
                "ret"
            ]
        );
        assert_eq!(rows[1].offset, 0x401);
        assert_eq!(rows[2].bytes, [0x48, 0x83, 0xEC, 0x10]);
        assert!(rows[0].copy_text().starts_with("00000400  55"));
    }

    #[test]
    fn invalid_bytes_are_data_and_decoding_goes_on() {
        let bytes = [0x0F, 0xFF, 0x90, 0xC3];
        let rows = decode(&bytes, 0, Arch::X86_64, None, 10, true);
        assert!(rows[0].data && rows[0].text.starts_with("db 0x0F"));
        assert_eq!(rows.last().unwrap().text, "ret");
    }

    #[test]
    fn a_cut_instruction_waits_unless_the_file_ends() {
        let rows = decode(&PROLOGUE[..10], 0, Arch::X86_64, None, 10, false);
        assert_eq!(rows.len(), 3, "mov eax,0 is cut: it waits");
        let rows = decode(&PROLOGUE[..10], 0, Arch::X86_64, None, 10, true);
        assert!(rows.last().unwrap().data);
    }

    #[test]
    fn moving_up_lands_on_the_instruction_before() {
        // From `leave` (offset 13) back: `mov eax,0` starts at 8.
        assert_eq!(previous_start(&PROLOGUE[..13], 13, Arch::X86_64), 8);
        assert_eq!(previous_start(&PROLOGUE[..1], 1, Arch::X86_64), 0);
        assert_eq!(previous_start(&[], 0, Arch::X86_64), 0);
    }

    #[test]
    fn the_architecture_changes_the_decoding() {
        let rows = decode(&[0x48, 0x90], 0, Arch::X86_32, None, 4, true);
        assert_eq!(rows[0].text, "dec eax");
        assert_eq!(Arch::from_name("x86-16"), Some(Arch::X86_16));
        assert_eq!(Arch::X86_64.next(), Arch::X86_16);
    }

    fn reader(bytes: &[u8]) -> impl Fn(u64, usize) -> Vec<u8> + '_ {
        move |at, len| {
            let at = (at as usize).min(bytes.len());
            bytes[at..(at + len).min(bytes.len())].to_vec()
        }
    }

    #[test]
    fn the_view_moves_by_rows_both_ways() {
        let read = reader(&PROLOGUE);
        let size = PROLOGUE.len() as u64;
        let mut view = AsmView::default();
        view.enter(None, &read, size);
        assert_eq!(view.top, 0);
        view.down(3, &read, size);
        assert_eq!(view.top, 8, "three rows down: mov eax,0");
        view.up(2, &read, size);
        assert_eq!(view.top, 1, "two rows up: mov rbp,rsp");
        view.bottom(2, &read, size);
        let rows = view.rows(&read, size, 2);
        assert_eq!(
            rows.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(),
            ["leave", "ret"]
        );
        view.down(100, &read, size);
        assert_eq!(view.top, 14, "never past the last row");
    }

    #[test]
    fn go_to_takes_entry_addresses_and_offsets() {
        let b = tiny_pe();
        let read = reader(&b);
        let size = b.len() as u64;
        let mut view = AsmView::default();
        view.enter(None, &read, size);
        assert_eq!(view.top, 0x200, "an executable starts at its entry point");
        assert_eq!(view.label(), "PE x86-64");
        assert!(view.go_to("0x140002000", &read, size));
        assert_eq!(view.top, 0x300);
        assert!(view.go_to("16", &read, size));
        assert_eq!(view.top, 16);
        assert!(view.go_to("entry", &read, size));
        assert_eq!(view.top, 0x200);
        assert!(!view.go_to("99999", &read, size));
        assert!(!view.go_to("nowhere", &read, size));
        // Up from the first data row lands on the last 8-byte row of the code before.
        view.top = 0x308;
        view.up(1, &read, size);
        assert_eq!(view.top, 0x300);
    }

    #[test]
    fn the_selection_copies_its_rows() {
        let read = reader(&PROLOGUE);
        let view = AsmView {
            selection: Some((8, 1)),
            ..Default::default()
        };
        let text = view.selection_text(&read, 15).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].starts_with("00000001  48 89 E5") && lines[0].ends_with("mov rbp,rsp"));
        assert!(lines[2].ends_with("mov eax,0"));
    }

    #[test]
    fn a_truncated_file_puts_the_view_back_at_its_start() {
        let read = reader(&PROLOGUE);
        let mut view = AsmView::default();
        view.enter(Some(13), &read, 15);
        assert_eq!(view.top, 13);
        let rows = view.rows(&read, 8, 10);
        assert_eq!(view.top, 0);
        assert_eq!(rows[0].text, "push rbp");
    }

    /// A minimal PE32+: headers, a `.text` section with the prologue at the entry point
    /// and a `.data` section.
    fn tiny_pe() -> Vec<u8> {
        let mut b = vec![0u8; 0x400];
        b[0..2].copy_from_slice(b"MZ");
        b[0x3C..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        b[0x80..0x84].copy_from_slice(b"PE\0\0");
        b[0x84..0x86].copy_from_slice(&0x8664u16.to_le_bytes());
        b[0x86..0x88].copy_from_slice(&2u16.to_le_bytes());
        b[0x94..0x96].copy_from_slice(&0xF0u16.to_le_bytes()); // optional header size
        let opt = 0x98;
        b[opt..opt + 2].copy_from_slice(&0x20Bu16.to_le_bytes());
        b[opt + 16..opt + 20].copy_from_slice(&0x1000u32.to_le_bytes()); // entry RVA
        b[opt + 24..opt + 32].copy_from_slice(&0x1_4000_0000u64.to_le_bytes());
        let table = opt + 0xF0;
        let section = |b: &mut Vec<u8>, i: usize, name: &[u8], rva: u32, raw: u32, flags: u32| {
            let at = table + i * 40;
            b[at..at + name.len()].copy_from_slice(name);
            b[at + 8..at + 12].copy_from_slice(&0x100u32.to_le_bytes());
            b[at + 12..at + 16].copy_from_slice(&rva.to_le_bytes());
            b[at + 16..at + 20].copy_from_slice(&0x100u32.to_le_bytes());
            b[at + 20..at + 24].copy_from_slice(&raw.to_le_bytes());
            b[at + 36..at + 40].copy_from_slice(&flags.to_le_bytes());
        };
        section(&mut b, 0, b".text", 0x1000, 0x200, 0x6000_0020);
        section(&mut b, 1, b".data", 0x2000, 0x300, 0xC000_0040);
        b[0x200..0x200 + PROLOGUE.len()].copy_from_slice(&PROLOGUE);
        b[0x300..0x304].copy_from_slice(b"ABCD");
        b
    }

    #[test]
    fn a_pe_gives_the_entry_point_addresses_and_sections() {
        let b = tiny_pe();
        let exe = executable(&b).unwrap();
        assert_eq!(exe.format, "PE");
        assert_eq!(exe.arch, Arch::X86_64);
        assert_eq!(exe.entry_offset, Some(0x200));
        assert_eq!(exe.sections.len(), 2);
        let rows = decode(&b[0x200..], 0x200, exe.arch, Some(&exe), 400, true);
        assert_eq!(rows[0].text, "push rbp");
        assert_eq!(rows[0].address, 0x1_4000_1000);
        assert_eq!(rows[0].section.as_deref(), Some(".text"));
        let data = rows.iter().find(|r| r.offset == 0x300).unwrap();
        assert!(data.data && data.text.starts_with("db 0x41, 0x42"));
        assert_eq!(data.section.as_deref(), Some(".data"));
        assert_eq!(exe.offset_of(0x1_4000_2000), Some(0x300));
    }

    #[test]
    fn a_malformed_header_is_raw_bytes() {
        assert_eq!(executable(b"MZ\0\0"), None);
        assert_eq!(executable(b"\x7FELF\x02\x01"), None);
        assert_eq!(executable(b"plain text"), None);
    }

    /// A minimal ELF64: a `.text` section holding the entry point.
    #[test]
    fn an_elf_gives_its_entry_point_and_text_section() {
        let mut b = vec![0u8; 0x300];
        b[0..4].copy_from_slice(b"\x7FELF");
        b[4] = 2;
        b[5] = 1;
        b[18..20].copy_from_slice(&62u16.to_le_bytes());
        b[24..32].copy_from_slice(&0x401000u64.to_le_bytes());
        b[40..48].copy_from_slice(&0x200u64.to_le_bytes()); // section headers
        b[58..60].copy_from_slice(&64u16.to_le_bytes());
        b[60..62].copy_from_slice(&3u16.to_le_bytes());
        b[62..64].copy_from_slice(&2u16.to_le_bytes()); // .shstrtab index
        b[0x100..0x100 + PROLOGUE.len()].copy_from_slice(&PROLOGUE);
        let names = b"\0.text\0.shstrtab\0";
        b[0x180..0x180 + names.len()].copy_from_slice(names);
        let sh = |b: &mut Vec<u8>,
                  i: usize,
                  name: u32,
                  kind: u32,
                  flags: u64,
                  addr: u64,
                  off: u64,
                  size: u64| {
            let at = 0x200 + i * 64;
            b.resize(b.len().max(at + 64), 0);
            b[at..at + 4].copy_from_slice(&name.to_le_bytes());
            b[at + 4..at + 8].copy_from_slice(&kind.to_le_bytes());
            b[at + 8..at + 16].copy_from_slice(&flags.to_le_bytes());
            b[at + 16..at + 24].copy_from_slice(&addr.to_le_bytes());
            b[at + 24..at + 32].copy_from_slice(&off.to_le_bytes());
            b[at + 32..at + 40].copy_from_slice(&size.to_le_bytes());
        };
        sh(&mut b, 1, 1, 1, 0x6, 0x401000, 0x100, PROLOGUE.len() as u64);
        sh(&mut b, 2, 7, 3, 0, 0, 0x180, names.len() as u64);
        let exe = executable(&b).unwrap();
        assert_eq!(exe.format, "ELF");
        assert_eq!(exe.entry_offset, Some(0x100));
        assert_eq!(exe.sections[0].name, ".text");
        assert!(exe.sections[0].executable);
    }
}
