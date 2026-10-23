// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Listener streams: log lines received on a local port, `tcp://127.0.0.1:5140`,
//! `udp://127.0.0.1:5140` or `syslog://127.0.0.1:514` (UDP and TCP on one port).
//!
//! An acceptor thread per TCP socket (at most 64 connections, a reader thread each), a
//! thread per UDP socket; readers cut messages (newlines, or RFC 6587 octet counting for
//! syslog over TCP, decided per connection; one datagram per UDP message), render syslog
//! (`syslog`), put the sender in front when asked, and hand complete lines through a
//! bounded channel to the copier of `stdin_source`, whose input (`ListenerInput`)
//! applies the rate limit, so the spool has one writer and lines never mix. Nothing is
//! ever sent back to a sender.

use crate::stdin_source::{Settings, StdinStream};
use crate::tail_engine::{TailEngine, WakeFn};
use std::io::Read;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const DEFAULT_SPOOL_MAX_MB: u32 = 2048;
pub const DEFAULT_MAX_LINES_PER_SEC: u32 = 20_000;
pub const MIN_MAX_LINES_PER_SEC: u32 = 100;
pub const MAX_MAX_LINES_PER_SEC: u32 = 1_000_000;
pub const DEFAULT_MAX_MB_PER_SEC: u32 = 16;
pub const MAX_MAX_MB_PER_SEC: u32 = 1024;
/// TCP connections a listener serves at once.
pub const MAX_CONNECTIONS: usize = 64;
/// Longest message kept; the rest is cut.
pub const MAX_MESSAGE: usize = 64 * 1024;
const CHANNEL: usize = 1024;
const POLL: Duration = Duration::from_millis(200);

/// What a listener receives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Proto {
    Tcp,
    Udp,
    /// UDP and TCP on the same port, messages rendered as syslog.
    Syslog,
}

impl Proto {
    fn scheme(self) -> &'static str {
        match self {
            Proto::Tcp => "tcp",
            Proto::Udp => "udp",
            Proto::Syslog => "syslog",
        }
    }
}

/// A listener, as its URL names it: `syslog://127.0.0.1:514?prefix=off`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListenSpec {
    pub proto: Proto,
    /// As written: an address, or `localhost` (both loopback addresses).
    pub host: String,
    pub port: u16,
    /// The sender's address before each line; `None`: on for UDP and syslog.
    pub prefix: Option<bool>,
}

/// True when `path` is written as a listener URL (valid or not).
pub fn is_listen_path(path: &Path) -> bool {
    path.to_str().is_some_and(|s| {
        s.starts_with("tcp://") || s.starts_with("udp://") || s.starts_with("syslog://")
    })
}

impl ListenSpec {
    /// The listener `path` names: `None` when it is no listener URL, `Some(Err)` with the
    /// reason (an i18n key) when it cannot be used.
    pub fn parse(path: &Path) -> Option<Result<Self, &'static str>> {
        let text = path.to_str()?;
        let (proto, rest) = [Proto::Tcp, Proto::Udp, Proto::Syslog]
            .into_iter()
            .find_map(|p| {
                text.strip_prefix(&format!("{}://", p.scheme()))
                    .map(|r| (p, r))
            })?;
        Some(Self::parse_rest(proto, rest))
    }

    fn parse_rest(proto: Proto, rest: &str) -> Result<Self, &'static str> {
        let (addr, query) = match rest.split_once('?') {
            Some((a, q)) => (a, Some(q)),
            None => (rest, None),
        };
        let prefix = match query {
            None | Some("") => None,
            Some("prefix=on") => Some(true),
            Some("prefix=off") => Some(false),
            Some(_) => return Err("listen_bad_url"),
        };
        let (host, port) = if let Some(inner) = addr.strip_prefix('[') {
            let (h, p) = inner.split_once("]:").ok_or("listen_bad_url")?;
            (h, p)
        } else {
            addr.rsplit_once(':').ok_or("listen_bad_url")?
        };
        let port: u16 = port.parse().map_err(|_| "listen_bad_port")?;
        if port == 0 {
            return Err("listen_bad_port");
        }
        let host = if host.eq_ignore_ascii_case("localhost") {
            "localhost"
        } else {
            host
        };
        if host != "localhost" && host.parse::<IpAddr>().is_err() {
            return Err("listen_bad_address");
        }
        Ok(Self {
            proto,
            host: host.to_string(),
            port,
            prefix,
        })
    }

    pub fn to_uri(&self) -> String {
        let host = if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        let query = match self.prefix {
            None => "",
            Some(true) => "?prefix=on",
            Some(false) => "?prefix=off",
        };
        format!("{}://{host}:{}{query}", self.proto.scheme(), self.port)
    }

    /// The tab title: `syslog :514`.
    pub fn title(&self) -> String {
        format!("{} :{}", self.proto.scheme(), self.port)
    }

    /// Whether lines carry the sender's address.
    pub fn prefixed(&self) -> bool {
        self.prefix.unwrap_or(self.proto != Proto::Tcp)
    }

    /// True when other machines can reach it (not a loopback address).
    pub fn is_exposed(&self) -> bool {
        self.host != "localhost"
            && self
                .host
                .parse::<IpAddr>()
                .is_ok_and(|ip| !ip.is_loopback())
    }

    /// The addresses it binds: both loopback addresses for `localhost` (the IPv6 one
    /// only when available).
    fn addresses(&self) -> Vec<(SocketAddr, bool)> {
        if self.host == "localhost" {
            vec![
                (
                    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), self.port),
                    true,
                ),
                (
                    SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), self.port),
                    false,
                ),
            ]
        } else {
            let ip: IpAddr = self.host.parse().expect("checked when parsed");
            vec![(SocketAddr::new(ip, self.port), true)]
        }
    }
}

/// The rate limit of a listener (`fasttail.ini`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub lines_per_sec: u32,
    pub bytes_per_sec: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            lines_per_sec: DEFAULT_MAX_LINES_PER_SEC,
            bytes_per_sec: u64::from(DEFAULT_MAX_MB_PER_SEC) << 20,
        }
    }
}

/// Counters the stream bar reads.
#[derive(Default)]
struct Shared {
    closed: AtomicBool,
    connections: AtomicUsize,
    refused: AtomicU64,
    dropped: AtomicU64,
    /// UDP lines dropped by a reader, not yet noted in the stream.
    dropped_unnoted: AtomicU64,
    received: AtomicU64,
}

/// A line ready for the spool, ending with `\n`.
type Line = Vec<u8>;

/// `text` cut at `MAX_MESSAGE` (on a character boundary, with the engine's marker).
fn capped(mut text: String) -> String {
    if text.len() > MAX_MESSAGE {
        let mut cut = MAX_MESSAGE;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.truncate(cut);
        text.push_str(" [...]");
    }
    text
}

/// The spool lines of one message from `from`: each of its lines rendered as syslog when
/// it is one, the sender in front.
fn lines_of(spec: &ListenSpec, from: SocketAddr, raw: &[u8]) -> Vec<Line> {
    let text = String::from_utf8_lossy(raw);
    let text = text.trim_end_matches(['\r', '\n']);
    if text.is_empty() {
        return Vec::new();
    }
    let prefix = if spec.prefixed() {
        format!("[{}] ", from.ip())
    } else {
        String::new()
    };
    capped(text.to_string())
        .split('\n')
        .map(|l| {
            let l = l.trim_end_matches('\r');
            let shown = if spec.proto == Proto::Syslog {
                // The receive time is read only for a message that needs it.
                crate::syslog::render(l, &crate::syslog::receive_time)
                    .unwrap_or_else(|| l.to_string())
            } else {
                l.to_string()
            };
            let mut line = format!("{prefix}{shown}").into_bytes();
            line.push(b'\n');
            line
        })
        .collect()
}

/// Digits that can hold a message length (`MAX_MESSAGE + 1`).
const LENGTH_DIGITS: usize = 5;

/// Cuts a TCP byte stream into messages: newline framing, or octet counting (`123 <msg>`,
/// RFC 6587) when `syslog` and the connection starts with a length. The buffer never holds
/// more than one message over the cap, and every byte is scanned once.
struct Framer {
    syslog: bool,
    octet: Option<bool>,
    buf: Vec<u8>,
    /// Discarding the rest of a line that went over the cap.
    skipping: bool,
    /// Bytes of `buf` already searched for a line break.
    scanned: usize,
}

impl Framer {
    fn new(syslog: bool) -> Self {
        Self {
            syslog,
            octet: None,
            buf: Vec::new(),
            skipping: false,
            scanned: 0,
        }
    }

    /// The length prefix at the start of `b`: its digits, and the length when a space
    /// follows them.
    fn length_prefix(b: &[u8]) -> (usize, Option<usize>) {
        let digits = b
            .iter()
            .take(LENGTH_DIGITS + 1)
            .take_while(|c| c.is_ascii_digit())
            .count();
        let len = (digits > 0 && digits <= LENGTH_DIGITS && b.get(digits) == Some(&b' '))
            .then(|| {
                std::str::from_utf8(&b[..digits])
                    .ok()?
                    .parse::<usize>()
                    .ok()
            })
            .flatten()
            .filter(|l| *l <= MAX_MESSAGE + 1);
        (digits, len)
    }

    /// The complete messages in `data` (the rest is kept for the next call).
    fn push(&mut self, data: &[u8]) -> Vec<Vec<u8>> {
        self.buf.extend_from_slice(data);
        if self.octet.is_none() {
            let (digits, len) = Self::length_prefix(&self.buf);
            if digits == self.buf.len() && digits <= LENGTH_DIGITS {
                return Vec::new(); // too early to tell
            }
            self.octet = Some(self.syslog && len.is_some());
        }
        let mut out = Vec::new();
        let mut start = 0;
        if self.octet == Some(true) {
            loop {
                // Some senders end each frame with a line break: skipped.
                while self
                    .buf
                    .get(start)
                    .is_some_and(|b| matches!(b, b'\r' | b'\n'))
                {
                    start += 1;
                }
                let rest = &self.buf[start..];
                let (digits, len) = Self::length_prefix(rest);
                if digits == rest.len() && digits <= LENGTH_DIGITS {
                    break; // the length is still arriving
                }
                let Some(len) = len else {
                    // Lost framing: line breaks for the rest of the connection.
                    self.octet = Some(false);
                    self.scanned = 0;
                    break;
                };
                if rest.len() < digits + 1 + len {
                    break;
                }
                out.push(rest[digits + 1..digits + 1 + len].to_vec());
                start += digits + 1 + len;
            }
        }
        if self.octet == Some(false) {
            let mut from = start.max(self.scanned);
            while let Some(rel) = self.buf[from..].iter().position(|&b| b == b'\n') {
                let nl = from + rel;
                if self.skipping {
                    self.skipping = false;
                } else {
                    out.push(self.buf[start..=nl].to_vec());
                }
                start = nl + 1;
                from = start;
            }
            if self.buf.len() - start > MAX_MESSAGE {
                // Over the cap: keep its head, drop the rest up to the next line break.
                if !self.skipping {
                    out.push(self.buf[start..].to_vec());
                }
                start = self.buf.len();
                self.skipping = true;
            }
        }
        self.buf.drain(..start);
        self.scanned = if self.octet == Some(false) {
            self.buf.len()
        } else {
            0
        };
        out
    }

    /// What is left when the connection closes, as a last message (without the length
    /// of a cut frame).
    fn finish(&mut self) -> Option<Vec<u8>> {
        let mut rest = std::mem::take(&mut self.buf);
        if self.octet == Some(true) {
            if let (digits, Some(_)) = Self::length_prefix(&rest) {
                rest.drain(..=digits);
            }
        }
        (!rest.is_empty() && !self.skipping).then_some(rest)
    }
}

/// Gives a connection slot back when its reader ends, however it ends.
struct ConnectionSlot(Arc<Shared>);

impl Drop for ConnectionSlot {
    fn drop(&mut self) {
        self.0.connections.fetch_sub(1, Ordering::AcqRel);
    }
}

/// A connection silent this long is closed, so idle ones cannot hold every slot.
const IDLE: Duration = Duration::from_secs(600);

fn serve_tcp(
    mut stream: TcpStream,
    from: SocketAddr,
    spec: &ListenSpec,
    tx: &SyncSender<Line>,
    shared: &Shared,
    wake: &Option<WakeFn>,
) {
    let _ = stream.set_read_timeout(Some(POLL));
    let mut framer = Framer::new(spec.proto == Proto::Syslog);
    let mut last_data = Instant::now();
    let mut buf = vec![0u8; 64 * 1024];
    let send = |msg: &[u8]| -> bool {
        for line in lines_of(spec, from, msg) {
            shared.received.fetch_add(1, Ordering::Relaxed);
            // Blocking: a full channel slows the sender down (TCP back-pressure).
            loop {
                match tx.try_send(line.clone()) {
                    Ok(()) => break,
                    Err(TrySendError::Full(_)) => {
                        if shared.closed.load(Ordering::Acquire) {
                            return false;
                        }
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(TrySendError::Disconnected(_)) => return false,
                }
            }
        }
        if let Some(w) = wake {
            w();
        }
        true
    };
    loop {
        if shared.closed.load(Ordering::Acquire) {
            return;
        }
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                last_data = Instant::now();
                for msg in framer.push(&buf[..n]) {
                    if !send(&msg) {
                        return;
                    }
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::Interrupted
                ) =>
            {
                if last_data.elapsed() > IDLE {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    if let Some(rest) = framer.finish() {
        send(&rest);
    }
}

fn accept_loop(
    listener: TcpListener,
    spec: ListenSpec,
    tx: SyncSender<Line>,
    shared: Arc<Shared>,
    wake: Option<WakeFn>,
) {
    let _ = listener.set_nonblocking(true);
    loop {
        if shared.closed.load(Ordering::Acquire) {
            return;
        }
        match listener.accept() {
            Ok((stream, from)) => {
                // Taken first, so two acceptors (`localhost`) never pass the cap.
                if shared.connections.fetch_add(1, Ordering::AcqRel) >= MAX_CONNECTIONS {
                    shared.connections.fetch_sub(1, Ordering::AcqRel);
                    shared.refused.fetch_add(1, Ordering::Relaxed);
                    drop(stream);
                    continue;
                }
                let _ = stream.set_nonblocking(false);
                let slot = ConnectionSlot(shared.clone());
                let (spec, tx, shared, wake) =
                    (spec.clone(), tx.clone(), shared.clone(), wake.clone());
                // A thread that cannot start gives the slot back as the closure drops.
                let _ = std::thread::Builder::new()
                    .name("fasttail-listen-tcp".to_string())
                    .stack_size(256 * 1024)
                    .spawn(move || {
                        let _slot = slot;
                        serve_tcp(stream, from, &spec, &tx, &shared, &wake);
                        if let Some(w) = &wake {
                            w();
                        }
                    });
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(POLL),
            Err(_) => std::thread::sleep(POLL),
        }
    }
}

fn udp_loop(
    socket: UdpSocket,
    spec: ListenSpec,
    tx: SyncSender<Line>,
    shared: Arc<Shared>,
    wake: Option<WakeFn>,
) {
    let _ = socket.set_read_timeout(Some(POLL));
    let mut buf = vec![0u8; 65_536];
    loop {
        if shared.closed.load(Ordering::Acquire) {
            return;
        }
        // A timeout (or a reset reported by Windows) just polls again.
        let received = socket.recv_from(&mut buf);
        if let Err(e) = &received {
            if !matches!(
                e.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ) {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        if let Ok((n, from)) = received {
            {
                for line in lines_of(&spec, from, &buf[..n]) {
                    shared.received.fetch_add(1, Ordering::Relaxed);
                    match tx.try_send(line) {
                        Ok(()) => {}
                        // UDP has no back-pressure: what does not fit is dropped.
                        Err(TrySendError::Full(_)) => {
                            shared.dropped.fetch_add(1, Ordering::Relaxed);
                            shared.dropped_unnoted.fetch_add(1, Ordering::Relaxed);
                        }
                        Err(TrySendError::Disconnected(_)) => return,
                    }
                }
                if let Some(w) = &wake {
                    w();
                }
            }
        }
    }
}

/// The copier's input: the lines the readers send, within the rate limit; a marker line
/// once a second while lines are dropped.
struct ListenerInput {
    rx: Receiver<Line>,
    shared: Arc<Shared>,
    limits: Limits,
    pending: Vec<u8>,
    pos: usize,
    window: Instant,
    lines: u32,
    bytes: u64,
    dropped_in_window: u64,
}

impl ListenerInput {
    /// A new one-second window; the marker of what the last one dropped, if anything.
    fn roll(&mut self) -> Option<Vec<u8>> {
        if self.window.elapsed() < Duration::from_secs(1) {
            return None;
        }
        self.window = Instant::now();
        self.lines = 0;
        self.bytes = 0;
        let dropped = std::mem::take(&mut self.dropped_in_window)
            + self.shared.dropped_unnoted.swap(0, Ordering::Relaxed);
        (dropped > 0).then(|| {
            format!("-- fasttail: {dropped} messages dropped (rate limit) --\n").into_bytes()
        })
    }
}

impl ListenerInput {
    /// `line` into the pending bytes, or dropped and counted when over the limit.
    fn take(&mut self, line: Line) {
        if self.lines >= self.limits.lines_per_sec
            || self.bytes + line.len() as u64 > self.limits.bytes_per_sec
        {
            self.dropped_in_window += 1;
            self.shared.dropped.fetch_add(1, Ordering::Relaxed);
            return;
        }
        self.lines += 1;
        self.bytes += line.len() as u64;
        self.pending.extend_from_slice(&line);
    }
}

impl Read for ListenerInput {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        loop {
            if self.pos < self.pending.len() {
                let n = buf.len().min(self.pending.len() - self.pos);
                buf[..n].copy_from_slice(&self.pending[self.pos..self.pos + n]);
                self.pos += n;
                return Ok(n);
            }
            if let Some(marker) = self.roll() {
                self.pending = marker;
                self.pos = 0;
                continue;
            }
            match self.rx.recv_timeout(POLL) {
                Ok(line) => {
                    self.pending.clear();
                    self.pos = 0;
                    self.take(line);
                    // More lines already waiting go in the same write.
                    while self.pending.len() < buf.len() {
                        match self.rx.try_recv() {
                            Ok(line) => self.take(line),
                            Err(_) => break,
                        }
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    if self.shared.closed.load(Ordering::Acquire) {
                        return Ok(0);
                    }
                }
                Err(RecvTimeoutError::Disconnected) => return Ok(0),
            }
        }
    }
}

/// A listener stream, owned by its engine (`TailEngine::listener`).
pub struct ListenerStream {
    pub spec: ListenSpec,
    shared: Arc<Shared>,
    /// The acceptor and UDP threads, joined on close so the port is free at once.
    threads: Vec<std::thread::JoinHandle<()>>,
    /// The last lines-per-second reading: when, the count then, the rate.
    rate: std::sync::Mutex<(Instant, u64, u64)>,
    /// What each socket is bound to, for the stream bar.
    pub bound: Vec<SocketAddr>,
    /// The output copier; declared last so the sockets close before the spool goes.
    pub copier: StdinStream,
}

/// Why binding failed: the address and the error.
#[derive(Debug)]
pub struct BindError {
    pub addr: SocketAddr,
    pub error: std::io::Error,
}

impl ListenerStream {
    pub fn start(
        spec: ListenSpec,
        limits: Limits,
        spool: &Settings,
        wake: Option<WakeFn>,
    ) -> Result<Self, BindError> {
        let shared = Arc::new(Shared::default());
        let (tx, rx) = sync_channel::<Line>(CHANNEL);
        // Bind everything first, so a port in use fails before any thread starts.
        let mut tcp = Vec::new();
        let mut udp = Vec::new();
        for (addr, required) in spec.addresses() {
            if matches!(spec.proto, Proto::Tcp | Proto::Syslog) {
                match TcpListener::bind(addr) {
                    Ok(l) => tcp.push(l),
                    Err(error) if required => return Err(BindError { addr, error }),
                    Err(_) => {}
                }
            }
            if matches!(spec.proto, Proto::Udp | Proto::Syslog) {
                match UdpSocket::bind(addr) {
                    Ok(s) => udp.push(s),
                    Err(error) if required => return Err(BindError { addr, error }),
                    Err(_) => {}
                }
            }
        }
        let mut bound: Vec<SocketAddr> = tcp
            .iter()
            .filter_map(|l| l.local_addr().ok())
            .chain(udp.iter().filter_map(|s| s.local_addr().ok()))
            .collect();
        bound.sort();
        bound.dedup();
        let input = ListenerInput {
            rx,
            shared: shared.clone(),
            limits,
            pending: Vec::new(),
            pos: 0,
            window: Instant::now(),
            lines: 0,
            bytes: 0,
            dropped_in_window: 0,
        };
        let copier =
            StdinStream::start_named(input, spool, wake.clone(), "listen").map_err(|error| {
                BindError {
                    addr: bound.first().copied().unwrap_or_else(|| {
                        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), spec.port)
                    }),
                    error,
                }
            })?;
        let mut threads = Vec::new();
        for l in tcp {
            let (spec, tx, shared, wake) = (spec.clone(), tx.clone(), shared.clone(), wake.clone());
            threads.extend(
                std::thread::Builder::new()
                    .name("fasttail-listen-accept".to_string())
                    .spawn(move || accept_loop(l, spec, tx, shared, wake))
                    .ok(),
            );
        }
        for s in udp {
            let (spec, tx, shared, wake) = (spec.clone(), tx.clone(), shared.clone(), wake.clone());
            threads.extend(
                std::thread::Builder::new()
                    .name("fasttail-listen-udp".to_string())
                    .spawn(move || udp_loop(s, spec, tx, shared, wake))
                    .ok(),
            );
        }
        Ok(Self {
            spec,
            shared,
            threads,
            rate: std::sync::Mutex::new((Instant::now(), 0, 0)),
            bound,
            copier,
        })
    }

    pub fn connections(&self) -> usize {
        self.shared.connections.load(Ordering::Acquire)
    }

    /// Lines dropped (rate limit, a full channel for UDP).
    pub fn dropped(&self) -> u64 {
        self.shared.dropped.load(Ordering::Relaxed)
    }

    /// TCP connections refused at the cap.
    pub fn refused(&self) -> u64 {
        self.shared.refused.load(Ordering::Relaxed)
    }

    /// Lines received, dropped ones included.
    pub fn received(&self) -> u64 {
        self.shared.received.load(Ordering::Relaxed)
    }

    /// Lines received per second, measured over the last second or more.
    pub fn lines_per_sec(&self) -> u64 {
        let Ok(mut r) = self.rate.lock() else {
            return 0;
        };
        let elapsed = r.0.elapsed();
        if elapsed >= Duration::from_secs(1) {
            let now = self.received();
            r.2 = ((now - r.1) as f64 / elapsed.as_secs_f64()).round() as u64;
            *r = (Instant::now(), now, r.2);
        }
        r.2
    }
}

impl Drop for ListenerStream {
    fn drop(&mut self) {
        // The threads see it within one poll and close their sockets; waiting for them
        // frees the port for a listener opened again right after (a session reloaded).
        self.shared.closed.store(true, Ordering::Release);
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

/// Opens the stream of `spec`: `path` is its URL, follow is on.
pub fn open_engine(
    spec: ListenSpec,
    limits: Limits,
    spool: &Settings,
    wake: Option<WakeFn>,
) -> Result<TailEngine, BindError> {
    let stream = ListenerStream::start(spec, limits, spool, wake.clone())?;
    let opened = match wake {
        Some(wake) => TailEngine::open_with_wake(stream.copier.spool_path(), wake),
        None => TailEngine::open(stream.copier.spool_path()),
    };
    let mut engine =
        opened.map_err(|error| BindError {
            addr: stream.bound.first().copied().unwrap_or_else(|| {
                SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), stream.spec.port)
            }),
            error,
        })?;
    engine.path = PathBuf::from(stream.spec.to_uri());
    engine.follow_tail = true;
    engine.listener = Some(stream);
    Ok(engine)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Option<Result<ListenSpec, &'static str>> {
        ListenSpec::parse(Path::new(s))
    }

    #[test]
    fn urls() {
        let s = parse("syslog://127.0.0.1:514").unwrap().unwrap();
        assert_eq!(
            (s.proto, s.host.as_str(), s.port),
            (Proto::Syslog, "127.0.0.1", 514)
        );
        assert!(s.prefixed() && !s.is_exposed());
        assert_eq!(s.title(), "syslog :514");
        let t = parse("tcp://[::1]:5140?prefix=on").unwrap().unwrap();
        assert_eq!((t.host.as_str(), t.prefix), ("::1", Some(true)));
        assert_eq!(t.to_uri(), "tcp://[::1]:5140?prefix=on");
        assert!(parse("udp://0.0.0.0:5140").unwrap().unwrap().is_exposed());
        assert!(!parse("udp://localhost:5140").unwrap().unwrap().is_exposed());
        assert!(!parse("tcp://localhost:9").unwrap().unwrap().prefixed());
        for (bad, why) in [
            ("tcp://127.0.0.1", "listen_bad_url"),
            ("tcp://127.0.0.1:0", "listen_bad_port"),
            ("tcp://127.0.0.1:99999", "listen_bad_port"),
            ("tcp://example.com:80", "listen_bad_address"),
            ("udp://127.0.0.1:5?x=1", "listen_bad_url"),
        ] {
            assert_eq!(parse(bad), Some(Err(why)), "{bad}");
        }
        assert_eq!(parse("/var/log/x"), None);
    }

    #[test]
    fn newline_and_octet_framing() {
        let mut f = Framer::new(false);
        assert!(f.push(b"one\ntw").len() == 1);
        assert_eq!(f.push(b"o\n"), [b"two\n".to_vec()]);
        assert_eq!(f.finish(), None);

        let mut s = Framer::new(true);
        let msgs = s.push(b"11 <13>1 - a b11 <13>1 - c d\n11 <13>1 - e f");
        assert_eq!(
            msgs,
            [
                b"<13>1 - a b".to_vec(),
                b"<13>1 - c d".to_vec(),
                b"<13>1 - e f".to_vec()
            ]
        );
        // A cut frame at the close loses its length.
        assert!(s.push(b"20 <13>1 - g").is_empty());
        assert_eq!(s.finish(), Some(b"<13>1 - g".to_vec()));
        // Endless digits never grow the buffer: framing is lost, then the line cap.
        let mut d = Framer::new(true);
        d.push(b"1 x");
        for _ in 0..100 {
            d.push(&[b'9'; 4096]);
        }
        assert!(d.buf.len() <= MAX_MESSAGE + 4096);
        // A syslog connection that starts with text uses newlines.
        let mut n = Framer::new(true);
        assert_eq!(n.push(b"<13>hello\n"), [b"<13>hello\n".to_vec()]);
        // Partial line at the close.
        let mut p = Framer::new(false);
        assert!(p.push(b"tail").is_empty());
        assert_eq!(p.finish(), Some(b"tail".to_vec()));
    }

    #[test]
    fn a_long_line_is_cut_and_its_rest_dropped() {
        let mut f = Framer::new(false);
        let long = vec![b'x'; MAX_MESSAGE + 10];
        let first = f.push(&long);
        assert_eq!(first.len(), 1);
        assert!(f.push(b"yyy\nnext\n") == [b"next\n".to_vec()]);
        let from: SocketAddr = "10.0.0.7:5000".parse().unwrap();
        let spec = parse("udp://127.0.0.1:5140").unwrap().unwrap();
        let lines = lines_of(&spec, from, b"a\nb");
        assert_eq!(
            lines,
            [b"[10.0.0.7] a\n".to_vec(), b"[10.0.0.7] b\n".to_vec()]
        );
    }
}
