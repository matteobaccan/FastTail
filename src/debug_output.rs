// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Windows debug output (`OutputDebugString`) as a stream: `dbwin://local` (this session)
//! or `dbwin://global` (services in session 0 too, administrators only), with an optional
//! process filter (`?include=app.exe,1234&exclude=svchost.exe`).
//!
//! A tracing program waits up to 10 seconds for the monitor, so the reader thread only
//! copies each message into a bounded queue and hands the buffer back; the copier of the
//! standard-input spool (`StdinStream`) formats the queued messages into lines
//! (`<local time> [<pid> <process>] <message>`) and writes them.
//!
//! The URI is recognised on every platform (so it is never taken for a file); opening it
//! anywhere but Windows says it is Windows only. No crate is added: the few kernel32
//! calls are declared here.

use crate::stdin_source::{Settings, StdinStream};
use crate::tail_engine::{TailEngine, WakeFn};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

const SCHEME: &str = "dbwin://";
/// Messages the reader may queue ahead of the copier; beyond, they are counted dropped.
pub const QUEUE: usize = 65_536;
/// How long a process name is trusted (process ids are reused).
const NAME_TTL: Duration = Duration::from_secs(60);
/// How often the threads look whether the stream was closed.
const POLL: Duration = Duration::from_millis(500);
/// The text of a message after its process id in the 4096-byte buffer.
#[cfg_attr(not(windows), allow(dead_code))]
const MAX_TEXT: usize = 4092;

/// True when `path` is written as a debug-output URI (valid or not).
pub fn is_dbwin_path(path: &Path) -> bool {
    path.to_str().is_some_and(|s| s.starts_with(SCHEME))
}

/// Which processes are kept: by name (`*` / `?` wildcards, any case) or by process id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
}

fn matches_entry(entry: &str, pid: u32, name: &str) -> bool {
    match entry.parse::<u32>() {
        Ok(n) => n == pid,
        Err(_) => crate::wildcard::wildcard_match(&entry.to_lowercase(), &name.to_lowercase()),
    }
}

impl Filter {
    /// Whether a message of `pid` (`name`) is kept. FastTail's own messages (`own_pid`)
    /// are left out unless the include list names its process id.
    pub fn keeps(&self, pid: u32, name: &str, own_pid: u32) -> bool {
        if pid == own_pid && !self.include.iter().any(|e| e.parse() == Ok(pid)) {
            return false;
        }
        if self.exclude.iter().any(|e| matches_entry(e, pid, name)) {
            return false;
        }
        self.include.is_empty() || self.include.iter().any(|e| matches_entry(e, pid, name))
    }
}

/// A `dbwin://` URI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbwinSpec {
    pub global: bool,
    pub filter: Filter,
}

fn list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .map(str::to_string)
        .collect()
}

impl DbwinSpec {
    /// `None` for a path that is no debug-output URI, else the spec or why it is not one
    /// (an i18n key).
    pub fn parse(path: &Path) -> Option<Result<Self, &'static str>> {
        let s = path.to_str()?.strip_prefix(SCHEME)?;
        let (scope, query) = s.split_once('?').unwrap_or((s, ""));
        let global = match scope.trim_end_matches('/') {
            "local" => false,
            "global" => true,
            _ => return Some(Err("dbwin_bad_url")),
        };
        let mut filter = Filter::default();
        for part in query.split('&').filter(|p| !p.is_empty()) {
            match part.split_once('=') {
                Some(("include", v)) => filter.include = list(v),
                Some(("exclude", v)) => filter.exclude = list(v),
                _ => return Some(Err("dbwin_bad_url")),
            }
        }
        Some(Ok(Self { global, filter }))
    }

    pub fn to_uri(&self) -> String {
        let mut uri = format!("{SCHEME}{}", if self.global { "global" } else { "local" });
        let mut sep = '?';
        for (key, entries) in [
            ("include", &self.filter.include),
            ("exclude", &self.filter.exclude),
        ] {
            if !entries.is_empty() {
                uri.push(sep);
                uri.push_str(key);
                uri.push('=');
                uri.push_str(&entries.join(","));
                sep = '&';
            }
        }
        uri
    }

    /// The tab title.
    pub fn title(&self) -> String {
        if self.global {
            "debug output (global)".to_string()
        } else {
            "debug output".to_string()
        }
    }
}

/// Appends the lines of one message to `out`: the first with the time and the process,
/// the following ones (a message with line breaks) indented by two spaces; a trailing
/// line break is dropped, control characters other than tabs become spaces.
pub fn format_message(out: &mut Vec<u8>, time: &str, pid: u32, name: &str, text: &str) {
    let text = text.trim_end_matches(['\r', '\n']);
    let mut parts = text.split(['\n', '\r']).filter(|p| !p.is_empty());
    let push = |out: &mut Vec<u8>, part: &str| {
        for c in part.chars() {
            let c = if c.is_control() && c != '\t' { ' ' } else { c };
            let mut buf = [0u8; 4];
            out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
        }
        out.push(b'\n');
    };
    out.extend_from_slice(format!("{time} [{pid} {name}] ").as_bytes());
    push(out, parts.next().unwrap_or(""));
    for part in parts {
        out.extend_from_slice(b"  ");
        push(out, part);
    }
}

/// A message as the reader queued it.
struct Message {
    at: SystemTime,
    pid: u32,
    text: Vec<u8>,
}

#[derive(Default)]
struct Shared {
    closed: AtomicBool,
    received: AtomicU64,
    dropped: AtomicU64,
}

/// The copier's input: formats the queued messages into lines.
struct Input {
    rx: Receiver<Message>,
    shared: Arc<Shared>,
    filter: Filter,
    own_pid: u32,
    names: HashMap<u32, (String, Instant)>,
    /// Drops already said in the stream.
    noted: u64,
    pending: Vec<u8>,
    pos: usize,
}

impl Input {
    fn name(&mut self, pid: u32) -> String {
        let now = Instant::now();
        if let Some((name, at)) = self.names.get(&pid) {
            if now.duration_since(*at) < NAME_TTL {
                return name.clone();
            }
        }
        if self.names.len() > 4096 {
            self.names
                .retain(|_, (_, at)| now.duration_since(*at) < NAME_TTL);
        }
        let name = platform::process_name(pid).unwrap_or_else(|| "?".to_string());
        self.names.insert(pid, (name.clone(), now));
        name
    }

    fn take(&mut self, m: Message) {
        let name = self.name(m.pid);
        if !self.filter.keeps(m.pid, &name, self.own_pid) {
            return;
        }
        let time = crate::syslog::local_time(m.at).0;
        let text = platform::decode(&m.text);
        format_message(&mut self.pending, &time, m.pid, &name, &text);
    }

    /// The marker of messages dropped since the last one, if any.
    fn note_drops(&mut self) {
        let dropped = self.shared.dropped.load(Ordering::Relaxed);
        if dropped > self.noted {
            let time = crate::syslog::receive_time().0;
            self.pending.extend_from_slice(
                format!(
                    "{time} [fasttail] {} debug messages dropped (the queue was full)\n",
                    dropped - self.noted
                )
                .as_bytes(),
            );
            self.noted = dropped;
        }
    }
}

impl Read for Input {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        loop {
            if self.pos < self.pending.len() {
                let n = buf.len().min(self.pending.len() - self.pos);
                buf[..n].copy_from_slice(&self.pending[self.pos..self.pos + n]);
                self.pos += n;
                return Ok(n);
            }
            self.pending.clear();
            self.pos = 0;
            match self.rx.recv_timeout(POLL) {
                Ok(m) => {
                    self.take(m);
                    while self.pending.len() < buf.len() {
                        match self.rx.try_recv() {
                            Ok(m) => self.take(m),
                            Err(_) => break,
                        }
                    }
                    self.note_drops();
                }
                Err(RecvTimeoutError::Timeout) => {
                    if self.shared.closed.load(Ordering::Acquire) {
                        return Ok(0);
                    }
                    self.note_drops();
                }
                Err(RecvTimeoutError::Disconnected) => return Ok(0),
            }
        }
    }
}

/// Why a capture could not start: an i18n key and a detail.
#[derive(Debug)]
pub struct StartError {
    pub key: &'static str,
    pub detail: String,
}

/// A debug-output stream, owned by its engine (`TailEngine::debug_output`).
pub struct DebugStream {
    pub spec: DbwinSpec,
    shared: Arc<Shared>,
    reader: Option<std::thread::JoinHandle<()>>,
    /// The copier; declared last so the buffer is released before the spool goes.
    pub copier: StdinStream,
}

impl DebugStream {
    pub fn start(
        spec: DbwinSpec,
        spool: &Settings,
        wake: Option<WakeFn>,
    ) -> Result<Self, StartError> {
        let shared = Arc::new(Shared::default());
        let (tx, rx) = std::sync::mpsc::sync_channel::<Message>(QUEUE);
        // The objects first: another monitor or missing rights fail before any thread.
        let buffer = platform::Buffer::create(spec.global)?;
        let input = Input {
            rx,
            shared: shared.clone(),
            filter: spec.filter.clone(),
            own_pid: std::process::id(),
            names: HashMap::new(),
            noted: 0,
            pending: Vec::new(),
            pos: 0,
        };
        let copier =
            StdinStream::start_named(input, spool, wake, "dbwin").map_err(|e| StartError {
                key: "dbwin_failed",
                detail: e.to_string(),
            })?;
        let reader_shared = shared.clone();
        let reader = std::thread::Builder::new()
            .name("fasttail-dbwin".to_string())
            .spawn(move || buffer.run(&tx, &reader_shared))
            .map_err(|e| StartError {
                key: "dbwin_failed",
                detail: e.to_string(),
            })?;
        Ok(Self {
            spec,
            shared,
            reader: Some(reader),
            copier,
        })
    }

    /// Messages received, dropped ones included.
    pub fn received(&self) -> u64 {
        self.shared.received.load(Ordering::Relaxed)
    }

    /// Messages dropped because the queue was full.
    pub fn dropped(&self) -> u64 {
        self.shared.dropped.load(Ordering::Relaxed)
    }
}

impl Drop for DebugStream {
    fn drop(&mut self) {
        // The reader sees it within one wait and releases the buffer, so another monitor
        // (or this stream opened again) can take it at once.
        self.shared.closed.store(true, Ordering::Release);
        if let Some(t) = self.reader.take() {
            let _ = t.join();
        }
    }
}

/// Opens the stream of `spec`: `path` is its URI, follow is on.
pub fn open_engine(
    spec: DbwinSpec,
    spool: &Settings,
    wake: Option<WakeFn>,
) -> Result<TailEngine, StartError> {
    let stream = DebugStream::start(spec, spool, wake.clone())?;
    let opened = match wake {
        Some(wake) => TailEngine::open_with_wake(stream.copier.spool_path(), wake),
        None => TailEngine::open(stream.copier.spool_path()),
    };
    let mut engine = opened.map_err(|e| StartError {
        key: "dbwin_failed",
        detail: e.to_string(),
    })?;
    engine.path = PathBuf::from(stream.spec.to_uri());
    engine.follow_tail = true;
    engine.debug_output = Some(stream);
    Ok(engine)
}

#[cfg(windows)]
mod platform {
    use super::{Message, Shared, StartError, MAX_TEXT, POLL};
    use std::ffi::c_void;
    use std::sync::atomic::Ordering;
    use std::sync::mpsc::{SyncSender, TrySendError};

    type Handle = *mut c_void;

    const ERROR_ACCESS_DENIED: u32 = 5;
    const ERROR_ALREADY_EXISTS: u32 = 183;
    const PAGE_READWRITE: u32 = 0x04;
    const FILE_MAP_READ: u32 = 0x04;
    const WAIT_OBJECT_0: u32 = 0;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const CP_ACP: u32 = 0;
    const BUFFER_SIZE: u32 = 4096;

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateEventW(
            attrs: *const c_void,
            manual: i32,
            initial: i32,
            name: *const u16,
        ) -> Handle;
        fn CreateFileMappingW(
            file: Handle,
            attrs: *const c_void,
            protect: u32,
            size_high: u32,
            size_low: u32,
            name: *const u16,
        ) -> Handle;
        fn MapViewOfFile(
            mapping: Handle,
            access: u32,
            offset_high: u32,
            offset_low: u32,
            bytes: usize,
        ) -> *mut c_void;
        fn UnmapViewOfFile(base: *const c_void) -> i32;
        fn SetEvent(event: Handle) -> i32;
        fn WaitForSingleObject(handle: Handle, millis: u32) -> u32;
        fn CloseHandle(handle: Handle) -> i32;
        fn GetLastError() -> u32;
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
        fn QueryFullProcessImageNameW(
            process: Handle,
            flags: u32,
            name: *mut u16,
            size: *mut u32,
        ) -> i32;
        fn MultiByteToWideChar(
            code_page: u32,
            flags: u32,
            text: *const u8,
            len: i32,
            wide: *mut u16,
            wide_len: i32,
        ) -> i32;
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// The DBWIN objects, owned by this monitor.
    pub struct Buffer {
        ready: Handle,
        data: Handle,
        mapping: Handle,
        view: *const u8,
    }

    // The handles and the view belong to the reader thread once it starts.
    unsafe impl Send for Buffer {}

    impl Drop for Buffer {
        fn drop(&mut self) {
            // SAFETY: each handle and the view were returned by the calls in `create`
            // and are released once, here.
            unsafe {
                if !self.view.is_null() {
                    UnmapViewOfFile(self.view.cast());
                }
                for h in [self.mapping, self.data, self.ready] {
                    if !h.is_null() {
                        CloseHandle(h);
                    }
                }
            }
        }
    }

    fn refused(global: bool, error: u32) -> StartError {
        StartError {
            key: if global && error == ERROR_ACCESS_DENIED {
                "dbwin_needs_admin"
            } else {
                "dbwin_failed"
            },
            detail: std::io::Error::from_raw_os_error(error as i32).to_string(),
        }
    }

    impl Buffer {
        pub fn create(global: bool) -> Result<Self, StartError> {
            let prefix = if global { "Global\\" } else { "" };
            let name = |n: &str| wide(&format!("{prefix}{n}"));
            let mut b = Buffer {
                ready: std::ptr::null_mut(),
                data: std::ptr::null_mut(),
                mapping: std::ptr::null_mut(),
                view: std::ptr::null(),
            };
            // SAFETY: plain kernel32 calls with NUL-terminated names that outlive them;
            // every handle is kept in `b`, which closes it on any early return.
            unsafe {
                b.ready = CreateEventW(std::ptr::null(), 0, 0, name("DBWIN_BUFFER_READY").as_ptr());
                let error = GetLastError();
                if b.ready.is_null() {
                    return Err(refused(global, error));
                }
                if error == ERROR_ALREADY_EXISTS {
                    // Another monitor (DebugView, another FastTail) owns the buffer.
                    return Err(StartError {
                        key: "dbwin_busy",
                        detail: String::new(),
                    });
                }
                b.data = CreateEventW(std::ptr::null(), 0, 0, name("DBWIN_DATA_READY").as_ptr());
                if b.data.is_null() {
                    return Err(refused(global, GetLastError()));
                }
                b.mapping = CreateFileMappingW(
                    -1isize as Handle,
                    std::ptr::null(),
                    PAGE_READWRITE,
                    0,
                    BUFFER_SIZE,
                    name("DBWIN_BUFFER").as_ptr(),
                );
                if b.mapping.is_null() {
                    return Err(refused(global, GetLastError()));
                }
                b.view = MapViewOfFile(b.mapping, FILE_MAP_READ, 0, 0, BUFFER_SIZE as usize)
                    as *const u8;
                if b.view.is_null() {
                    return Err(refused(global, GetLastError()));
                }
            }
            Ok(b)
        }

        /// Hands the buffer to the writers and copies each message into the queue until
        /// the stream closes.
        pub fn run(self, tx: &SyncSender<Message>, shared: &Shared) {
            while !shared.closed.load(Ordering::Acquire) {
                // SAFETY: the handles are open for the life of `self`; the view is 4096
                // readable bytes: a u32 then the text, NUL-terminated by the writer (the
                // scan stops at MAX_TEXT anyway).
                let message = unsafe {
                    SetEvent(self.ready);
                    if WaitForSingleObject(self.data, POLL.as_millis() as u32) != WAIT_OBJECT_0 {
                        continue;
                    }
                    let pid = std::ptr::read_unaligned(self.view.cast::<u32>());
                    let text = std::slice::from_raw_parts(self.view.add(4), MAX_TEXT);
                    let len = text.iter().position(|&b| b == 0).unwrap_or(MAX_TEXT);
                    Message {
                        at: std::time::SystemTime::now(),
                        pid,
                        text: text[..len].to_vec(),
                    }
                };
                shared.received.fetch_add(1, Ordering::Relaxed);
                match tx.try_send(message) {
                    Ok(()) => {}
                    Err(TrySendError::Full(_)) => {
                        shared.dropped.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(TrySendError::Disconnected(_)) => return,
                }
            }
        }
    }

    /// The file name of process `pid`'s executable.
    pub fn process_name(pid: u32) -> Option<String> {
        // SAFETY: the handle is checked and closed; the buffer and its size match.
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                return None;
            }
            let mut buf = [0u16; 1024];
            let mut size = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut size);
            CloseHandle(h);
            if ok == 0 {
                return None;
            }
            let path = String::from_utf16_lossy(&buf[..size as usize]);
            Some(path.rsplit(['\\', '/']).next().unwrap_or(&path).to_string())
        }
    }

    /// The text of a message, from the ANSI code page.
    pub fn decode(bytes: &[u8]) -> String {
        if bytes.is_ascii() {
            return String::from_utf8_lossy(bytes).into_owned();
        }
        let mut out = vec![0u16; bytes.len()];
        // SAFETY: the lengths passed are the slices' own; one byte never gives more than
        // one UTF-16 unit in an ANSI code page.
        let n = unsafe {
            MultiByteToWideChar(
                CP_ACP,
                0,
                bytes.as_ptr(),
                bytes.len() as i32,
                out.as_mut_ptr(),
                out.len() as i32,
            )
        };
        if n <= 0 {
            return String::from_utf8_lossy(bytes).into_owned();
        }
        String::from_utf16_lossy(&out[..n as usize])
    }
}

#[cfg(not(windows))]
mod platform {
    use super::{Message, Shared, StartError};
    use std::sync::mpsc::SyncSender;

    pub struct Buffer;

    impl Buffer {
        pub fn create(_global: bool) -> Result<Self, StartError> {
            Err(StartError {
                key: "dbwin_windows_only",
                detail: String::new(),
            })
        }

        pub fn run(self, _tx: &SyncSender<Message>, _shared: &Shared) {}
    }

    pub fn process_name(_pid: u32) -> Option<String> {
        None
    }

    pub fn decode(bytes: &[u8]) -> String {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Option<Result<DbwinSpec, &'static str>> {
        DbwinSpec::parse(Path::new(s))
    }

    #[test]
    fn uris() {
        let local = parse("dbwin://local").unwrap().unwrap();
        assert!(!local.global && local.filter == Filter::default());
        assert_eq!(local.to_uri(), "dbwin://local");
        assert_eq!(local.title(), "debug output");
        let g = parse("dbwin://global?include=app.exe, 42&exclude=svc*")
            .unwrap()
            .unwrap();
        assert!(g.global);
        assert_eq!(g.filter.include, ["app.exe", "42"]);
        assert_eq!(g.filter.exclude, ["svc*"]);
        assert_eq!(g.to_uri(), "dbwin://global?include=app.exe,42&exclude=svc*");
        assert_eq!(parse(&g.to_uri()).unwrap().unwrap(), g);
        assert_eq!(
            parse("dbwin://local?exclude=x").unwrap().unwrap().to_uri(),
            "dbwin://local?exclude=x"
        );
        assert_eq!(parse("dbwin://remote"), Some(Err("dbwin_bad_url")));
        assert_eq!(parse("dbwin://local?pid=1"), Some(Err("dbwin_bad_url")));
        assert_eq!(parse("/var/log/x"), None);
        assert!(is_dbwin_path(Path::new("dbwin://whatever")));
    }

    #[test]
    fn filter() {
        let none = Filter::default();
        assert!(none.keeps(10, "app.exe", 99));
        assert!(!none.keeps(99, "fasttail.exe", 99));
        let f = Filter {
            include: vec!["App*.EXE".into(), "77".into()],
            exclude: vec!["apptest.exe".into()],
        };
        assert!(f.keeps(10, "appmain.exe", 99));
        assert!(f.keeps(77, "other.exe", 99));
        assert!(!f.keeps(11, "other.exe", 99));
        assert!(!f.keeps(12, "AppTest.exe", 99));
        // Its own process id named in the include list brings FastTail's messages back.
        let own = Filter {
            include: vec!["99".into()],
            exclude: Vec::new(),
        };
        assert!(own.keeps(99, "fasttail.exe", 99));
    }

    #[test]
    fn message_lines() {
        let mut out = Vec::new();
        format_message(&mut out, "T", 42, "app.exe", "hello\r\n");
        format_message(&mut out, "T", 7, "?", "first\r\nsecond\nthird\x07\n");
        format_message(&mut out, "T", 8, "x", "");
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "T [42 app.exe] hello\nT [7 ?] first\n  second\n  third \nT [8 x] \n"
        );
    }

    #[test]
    fn the_copier_input_formats_and_filters() {
        let (tx, rx) = std::sync::mpsc::sync_channel(4);
        let shared = Arc::new(Shared::default());
        let mut input = Input {
            rx,
            shared: shared.clone(),
            filter: Filter {
                include: Vec::new(),
                exclude: vec!["5".into()],
            },
            own_pid: 1,
            names: HashMap::new(),
            noted: 0,
            pending: Vec::new(),
            pos: 0,
        };
        let at = SystemTime::now();
        for (pid, text) in [(4, "kept"), (5, "excluded"), (1, "own")] {
            tx.send(Message {
                at,
                pid,
                text: text.as_bytes().to_vec(),
            })
            .unwrap();
        }
        shared.dropped.store(3, Ordering::Relaxed);
        drop(tx);
        let mut text = String::new();
        input.read_to_string(&mut text).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2, "{text}");
        assert!(lines[0].ends_with(" [4 ?] kept"), "{text}");
        assert!(lines[1].ends_with("3 debug messages dropped (the queue was full)"));
    }

    #[cfg(not(windows))]
    #[test]
    fn elsewhere_it_is_windows_only() {
        let spec = parse("dbwin://local").unwrap().unwrap();
        let settings = Settings {
            spool_dir: std::env::temp_dir(),
            limits: crate::stdin_source::Limits::with_cap_mb(64),
        };
        let err = DebugStream::start(spec, &settings, None).err().unwrap();
        assert_eq!(err.key, "dbwin_windows_only");
    }
}
