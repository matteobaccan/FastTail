// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Windows Event Log channels and `.evtx` files as streams.
//!
//! A channel is `eventlog://<channel>` (`/` in the name percent-encoded) with an optional
//! pre-filter `?levels=1,2&ids=7031,7034-7040`, turned into the XPath query Windows
//! filters with. A worker thread reads the newest `event_log_initial_events` events
//! (newest first, written oldest first), bookmarks the newest, then subscribes after the
//! bookmark, so nothing is lost or repeated between history and live. Each event becomes
//! one text line the level and timestamp detectors read:
//! `2026-09-28T14:02:05.123+02:00 ERROR [Service Control Manager] 7031: The Print Spooler
//! service terminated unexpectedly.` (continuation lines indented by two spaces).
//!
//! The lines go to a spool through the standard-input copier (`StdinStream`), like the
//! other non-file sources. The URI is recognised on every platform; opening it anywhere
//! but Windows says it is Windows only. No crate is added: the `wevtapi` calls are
//! declared here.

use crate::stdin_source::{Settings, StdinStream};
use crate::tail_engine::{TailEngine, WakeFn};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const SCHEME: &str = "eventlog://";
/// Bytes of an event message kept (longer ones are cut with `…`).
pub const MAX_MESSAGE: usize = 64 * 1024;
/// Event IDs or ranges of a pre-filter at most.
pub const MAX_IDS: usize = 32;
/// Events read per call while following.
#[cfg_attr(not(windows), allow(dead_code))]
const BATCH: usize = 64;
/// How often the worker looks whether the stream was closed.
const POLL: Duration = Duration::from_millis(500);
/// The first bytes of an `.evtx` file.
pub const EVTX_MAGIC: &[u8; 8] = b"ElfFile\0";

/// True when `path` is written as an Event Log URI (valid or not).
pub fn is_eventlog_path(path: &Path) -> bool {
    path.to_str().is_some_and(|s| s.starts_with(SCHEME))
}

/// Whether the file at `path` is an `.evtx` export (by its magic bytes).
pub fn is_evtx_file(path: &Path) -> bool {
    let mut head = [0u8; 8];
    std::fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut head))
        .is_ok()
        && &head == EVTX_MAGIC
}

/// An event ID or an inclusive range of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdRange(pub u32, pub u32);

/// The pre-filter of a channel: the levels kept (1 critical … 5 verbose; empty: all) and
/// the event IDs (empty: all).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PreFilter {
    pub levels: Vec<u8>,
    pub ids: Vec<IdRange>,
}

impl PreFilter {
    /// `7031, 7034-7040` (an invalid item is an error; at most `MAX_IDS`).
    pub fn parse_ids(text: &str) -> Result<Vec<IdRange>, &'static str> {
        let mut out = Vec::new();
        for item in text.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let range = match item.split_once('-') {
                Some((a, b)) => {
                    let (a, b) = (a.trim().parse::<u32>(), b.trim().parse::<u32>());
                    match (a, b) {
                        (Ok(a), Ok(b)) if a <= b => IdRange(a, b),
                        _ => return Err("eventlog_bad_ids"),
                    }
                }
                None => {
                    let n = item.parse::<u32>().map_err(|_| "eventlog_bad_ids")?;
                    IdRange(n, n)
                }
            };
            out.push(range);
        }
        if out.len() > MAX_IDS {
            return Err("eventlog_too_many_ids");
        }
        Ok(out)
    }

    pub fn ids_text(&self) -> String {
        self.ids
            .iter()
            .map(|r| {
                if r.0 == r.1 {
                    r.0.to_string()
                } else {
                    format!("{}-{}", r.0, r.1)
                }
            })
            .collect::<Vec<_>>()
            .join(",")
    }

    /// The XPath query Windows filters the channel with (`*` without a filter). Level 4
    /// also takes level 0 (LogAlways), as Event Viewer's "Information" does.
    pub fn xpath(&self) -> String {
        let mut parts = Vec::new();
        if !self.levels.is_empty() {
            let mut levels: Vec<u8> = self.levels.clone();
            if levels.contains(&4) && !levels.contains(&0) {
                levels.push(0);
            }
            levels.sort_unstable();
            let terms: Vec<String> = levels.iter().map(|l| format!("Level={l}")).collect();
            parts.push(format!("({})", terms.join(" or ")));
        }
        if !self.ids.is_empty() {
            let terms: Vec<String> = self
                .ids
                .iter()
                .map(|r| {
                    if r.0 == r.1 {
                        format!("EventID={}", r.0)
                    } else {
                        format!("(EventID>={} and EventID<={})", r.0, r.1)
                    }
                })
                .collect();
            parts.push(format!("({})", terms.join(" or ")));
        }
        if parts.is_empty() {
            "*".to_string()
        } else {
            format!("*[System[{}]]", parts.join(" and "))
        }
    }
}

/// What an Event Log stream reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// A live channel (`Application`, `Microsoft-Windows-PowerShell/Operational`).
    Channel(String),
    /// An exported `.evtx` file, read once.
    File(PathBuf),
}

/// An Event Log stream: its source and pre-filter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventLogSpec {
    pub source: Source,
    pub filter: PreFilter,
    /// Another machine whose channel is read (`eventlog://host/Channel`), through a
    /// remote session with the current user's credentials; `None`: this machine.
    pub host: Option<String>,
}

/// A host name or address a remote session takes (`srv01`, `srv01.corp.example`,
/// `10.0.0.5`, `[fe80::1]`).
fn valid_host(host: &str) -> bool {
    (1..=255).contains(&host.len())
        && !host.starts_with('-')
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._:[]".contains(&b))
}

fn pct_encode(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '/' | '?' | '&' | '%' | '#' | ' ' => out.push_str(&format!("%{:02X}", c as u8)),
            c => out.push(c),
        }
    }
    out
}

fn pct_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hex = |c: u8| (c as char).to_digit(16);
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

impl EventLogSpec {
    /// The spec of an `eventlog://` URI; `None` when `path` is not one, an error key when
    /// it is but does not read.
    pub fn parse(path: &Path) -> Option<Result<Self, &'static str>> {
        let s = path.to_str()?;
        let rest = s.strip_prefix(SCHEME)?;
        let (name, query) = rest.split_once('?').unwrap_or((rest, ""));
        // A literal `/` (a channel's own is written `%2F`) separates a remote host.
        let (host, name) = match name.split_once('/') {
            Some((h, n)) if valid_host(h) => (Some(h.to_string()), n),
            Some(_) => return Some(Err("eventlog_bad_host")),
            None => (None, name),
        };
        let name = pct_decode(name);
        if name.trim().is_empty() {
            return Some(Err("eventlog_no_channel"));
        }
        let mut filter = PreFilter::default();
        for pair in query.split('&').filter(|p| !p.is_empty()) {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            match k {
                "levels" => {
                    for l in v.split(',').filter(|l| !l.is_empty()) {
                        match l.parse::<u8>() {
                            Ok(n) if (1..=5).contains(&n) => filter.levels.push(n),
                            _ => return Some(Err("eventlog_bad_levels")),
                        }
                    }
                }
                "ids" => match PreFilter::parse_ids(&pct_decode(v)) {
                    Ok(ids) => filter.ids = ids,
                    Err(e) => return Some(Err(e)),
                },
                _ => {}
            }
        }
        filter.levels.sort_unstable();
        filter.levels.dedup();
        Some(Ok(Self {
            source: Source::Channel(name),
            filter,
            host,
        }))
    }

    /// The URI of a channel stream, or the path of a file.
    pub fn to_path(&self) -> PathBuf {
        match &self.source {
            Source::File(p) => p.clone(),
            Source::Channel(name) => {
                let host = self
                    .host
                    .as_deref()
                    .map(|h| format!("{h}/"))
                    .unwrap_or_default();
                let mut uri = format!("{SCHEME}{host}{}", pct_encode(name));
                let mut q = Vec::new();
                if !self.filter.levels.is_empty() {
                    let l: Vec<String> = self.filter.levels.iter().map(u8::to_string).collect();
                    q.push(format!("levels={}", l.join(",")));
                }
                if !self.filter.ids.is_empty() {
                    q.push(format!("ids={}", self.filter.ids_text()));
                }
                if !q.is_empty() {
                    uri.push('?');
                    uri.push_str(&q.join("&"));
                }
                PathBuf::from(uri)
            }
        }
    }
}

/// The level word of an event: `Level` 1 critical … 5 verbose; 0 (LogAlways) is INFO, or
/// WARN / INFO for an audit failure / success (`Keywords`).
pub fn level_word(level: u8, keywords: u64) -> &'static str {
    const AUDIT_FAILURE: u64 = 0x0010_0000_0000_0000;
    const AUDIT_SUCCESS: u64 = 0x0020_0000_0000_0000;
    match level {
        1 => "FATAL",
        2 => "ERROR",
        3 => "WARN",
        5 => "DEBUG",
        0 if keywords & AUDIT_FAILURE != 0 => "WARN",
        0 if keywords & AUDIT_SUCCESS != 0 => "INFO",
        _ => "INFO",
    }
}

/// Appends the line of an event: `<time> <LEVEL> [<provider>] <id>: <message>`, the
/// message's further lines indented by two spaces, cut at `MAX_MESSAGE` bytes.
pub fn format_event(
    out: &mut Vec<u8>,
    time: &str,
    level: &str,
    provider: &str,
    id: u32,
    msg: &str,
) {
    out.extend_from_slice(format!("{time} {level} [{provider}] {id}: ").as_bytes());
    let mut end = msg.len().min(MAX_MESSAGE);
    while !msg.is_char_boundary(end) {
        end -= 1;
    }
    let cut = end < msg.len();
    let mut first = true;
    for line in msg[..end].trim_end().split('\n') {
        let line = line.trim_end_matches('\r');
        if !first {
            if line.trim().is_empty() {
                continue;
            }
            out.extend_from_slice(b"\n  ");
        }
        out.extend_from_slice(line.as_bytes());
        first = false;
    }
    if cut {
        out.extend_from_slice("…".as_bytes());
    }
    out.push(b'\n');
}

/// Where a stream is: reading the history, following, resubscribing after the channel
/// was cleared, done (a file), or failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    History,
    Live,
    Resubscribing,
    Done,
    Failed,
}

impl State {
    fn from_u8(v: u8) -> Self {
        match v {
            0 => State::History,
            1 => State::Live,
            2 => State::Resubscribing,
            3 => State::Done,
            _ => State::Failed,
        }
    }
}

#[derive(Default)]
pub(crate) struct Shared {
    closed: AtomicBool,
    state: AtomicU8,
    /// Events written.
    events: AtomicU64,
    error: Mutex<Option<String>>,
}

/// The copier's input: the chunks of lines the worker sends.
struct Input {
    rx: Receiver<Vec<u8>>,
    shared: Arc<Shared>,
    pending: Vec<u8>,
    pos: usize,
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
            match self.rx.recv_timeout(POLL) {
                Ok(chunk) => {
                    self.pending = chunk;
                    self.pos = 0;
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

/// Why a stream could not start: an i18n key and a detail.
#[derive(Debug)]
pub struct StartError {
    pub key: &'static str,
    pub detail: String,
}

/// An Event Log stream, owned by its engine (`TailEngine::event_log`).
pub struct EventLogStream {
    pub spec: EventLogSpec,
    shared: Arc<Shared>,
    worker: Option<std::thread::JoinHandle<()>>,
    /// The copier; declared last so the worker is gone before the spool.
    pub copier: StdinStream,
}

impl EventLogStream {
    pub fn start(
        spec: EventLogSpec,
        initial: u32,
        spool: &Settings,
        wake: Option<WakeFn>,
    ) -> Result<Self, StartError> {
        let shared = Arc::new(Shared::default());
        let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(256);
        // The channel is opened first: a missing one or missing rights fail at once.
        let reader = platform::Reader::open(&spec)?;
        let input = Input {
            rx,
            shared: shared.clone(),
            pending: Vec::new(),
            pos: 0,
        };
        let copier =
            StdinStream::start_named(input, spool, wake, "eventlog").map_err(|e| StartError {
                key: "eventlog_failed",
                detail: e.to_string(),
            })?;
        let worker_shared = shared.clone();
        let worker = std::thread::Builder::new()
            .name("fasttail-eventlog".to_string())
            .spawn(move || reader.run(initial, &tx, &worker_shared))
            .map_err(|e| StartError {
                key: "eventlog_failed",
                detail: e.to_string(),
            })?;
        Ok(Self {
            spec,
            shared,
            worker: Some(worker),
            copier,
        })
    }

    pub fn state(&self) -> State {
        State::from_u8(self.shared.state.load(Ordering::Relaxed))
    }

    /// Events written so far.
    pub fn events(&self) -> u64 {
        self.shared.events.load(Ordering::Relaxed)
    }

    /// The error that stopped the stream, if any.
    pub fn error(&self) -> Option<String> {
        self.shared.error.lock().ok().and_then(|e| e.clone())
    }
}

impl Drop for EventLogStream {
    fn drop(&mut self) {
        self.shared.closed.store(true, Ordering::Release);
        if let Some(t) = self.worker.take() {
            let _ = t.join();
        }
    }
}

/// Opens the stream of `spec` (a channel followed, a file read once).
pub fn open_engine(
    spec: EventLogSpec,
    initial: u32,
    spool: &Settings,
    wake: Option<WakeFn>,
) -> Result<TailEngine, StartError> {
    let file = matches!(spec.source, Source::File(_));
    let stream = EventLogStream::start(spec, initial, spool, wake.clone())?;
    let opened = match wake {
        Some(wake) => TailEngine::open_with_wake(stream.copier.spool_path(), wake),
        None => TailEngine::open(stream.copier.spool_path()),
    };
    let mut engine = opened.map_err(|e| StartError {
        key: "eventlog_failed",
        detail: e.to_string(),
    })?;
    engine.path = stream.spec.to_path();
    engine.follow_tail = !file;
    engine.event_log = Some(stream);
    Ok(engine)
}

/// The channels this user can list (empty off Windows, or when listing fails).
pub fn channels() -> Vec<String> {
    platform::channels()
}

#[cfg(windows)]
mod platform {
    use super::{
        format_event, level_word, EventLogSpec, Shared, Source, StartError, State, BATCH, POLL,
    };
    use std::collections::HashMap;
    use std::ffi::c_void;
    use std::sync::atomic::Ordering;
    use std::sync::mpsc::SyncSender;

    type Handle = isize;

    /// `EVT_RPC_LOGIN`: the machine of a remote session; null user, domain and password
    /// mean the current user's credentials.
    #[repr(C)]
    struct RpcLogin {
        server: *mut u16,
        user: *mut u16,
        domain: *mut u16,
        password: *mut u16,
        flags: u32,
    }

    const EVT_RPC_LOGIN_CLASS: u32 = 1;
    const RPC_S_SERVER_UNAVAILABLE: u32 = 1722;

    const ERROR_ACCESS_DENIED: u32 = 5;
    const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
    const ERROR_NO_MORE_ITEMS: u32 = 259;
    const ERROR_EVT_CHANNEL_NOT_FOUND: u32 = 15007;
    const EVT_QUERY_CHANNEL_PATH: u32 = 0x1;
    const EVT_QUERY_FILE_PATH: u32 = 0x2;
    const EVT_QUERY_REVERSE_DIRECTION: u32 = 0x200;
    const EVT_SUBSCRIBE_TO_FUTURE_EVENTS: u32 = 1;
    const EVT_SUBSCRIBE_START_AFTER_BOOKMARK: u32 = 3;
    const EVT_RENDER_CONTEXT_SYSTEM: u32 = 1;
    const EVT_RENDER_CONTEXT_USER: u32 = 2;
    const EVT_RENDER_EVENT_VALUES: u32 = 0;
    const EVT_FORMAT_MESSAGE_EVENT: u32 = 1;
    const INFINITE: u32 = 0xFFFF_FFFF;
    const RPC_S_INVALID_BOUND: u32 = 1734;
    // EvtFormatMessage filled the buffer, but some inserts were not resolved.
    const ERROR_EVT_UNRESOLVED_VALUE_INSERT: u32 = 15029;
    const ERROR_EVT_UNRESOLVED_PARAMETER_INSERT: u32 = 15030;
    const ERROR_EVT_MAX_INSERTS_REACHED: u32 = 15031;
    // EVT_SYSTEM_PROPERTY_ID
    const PROP_PROVIDER_NAME: usize = 0;
    const PROP_EVENT_ID: usize = 2;
    const PROP_LEVEL: usize = 4;
    const PROP_KEYWORDS: usize = 7;
    const PROP_TIME_CREATED: usize = 8;
    // EVT_VARIANT_TYPE
    const TYPE_STRING: u32 = 1;
    const TYPE_BYTE: u32 = 4;
    const TYPE_UINT16: u32 = 6;
    const TYPE_UINT32: u32 = 8;
    const TYPE_UINT64: u32 = 10;
    const TYPE_FILETIME: u32 = 17;
    const TYPE_HEXINT64: u32 = 21;
    /// Publisher metadata handles kept per stream.
    const PUBLISHERS: usize = 256;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Variant {
        value: u64,
        count: u32,
        kind: u32,
    }

    #[link(name = "wevtapi")]
    extern "system" {
        fn EvtQuery(session: Handle, path: *const u16, query: *const u16, flags: u32) -> Handle;
        fn EvtNext(
            result_set: Handle,
            size: u32,
            events: *mut Handle,
            timeout: u32,
            flags: u32,
            returned: *mut u32,
        ) -> i32;
        fn EvtClose(object: Handle) -> i32;
        fn EvtOpenSession(
            login_class: u32,
            login: *const RpcLogin,
            timeout: u32,
            flags: u32,
        ) -> Handle;
        fn EvtCreateRenderContext(count: u32, paths: *const *const u16, flags: u32) -> Handle;
        fn EvtRender(
            context: Handle,
            fragment: Handle,
            flags: u32,
            buffer_size: u32,
            buffer: *mut c_void,
            used: *mut u32,
            property_count: *mut u32,
        ) -> i32;
        fn EvtOpenPublisherMetadata(
            session: Handle,
            publisher: *const u16,
            log_file: *const u16,
            locale: u32,
            flags: u32,
        ) -> Handle;
        fn EvtFormatMessage(
            publisher: Handle,
            event: Handle,
            message_id: u32,
            value_count: u32,
            values: *const c_void,
            flags: u32,
            buffer_size: u32,
            buffer: *mut u16,
            used: *mut u32,
        ) -> i32;
        fn EvtCreateBookmark(xml: *const u16) -> Handle;
        fn EvtUpdateBookmark(bookmark: Handle, event: Handle) -> i32;
        fn EvtSubscribe(
            session: Handle,
            signal: Handle,
            channel: *const u16,
            query: *const u16,
            bookmark: Handle,
            context: *mut c_void,
            callback: *mut c_void,
            flags: u32,
        ) -> Handle;
        fn EvtOpenChannelEnum(session: Handle, flags: u32) -> Handle;
        fn EvtNextChannelPath(
            enumerator: Handle,
            size: u32,
            buffer: *mut u16,
            used: *mut u32,
        ) -> i32;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetLastError() -> u32;
        fn CreateEventW(
            attributes: *const c_void,
            manual: i32,
            initial: i32,
            name: *const u16,
        ) -> *mut c_void;
        fn ResetEvent(event: *mut c_void) -> i32;
        fn WaitForSingleObject(handle: *mut c_void, millis: u32) -> u32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// An Evt handle closed when dropped.
    struct Evt(Handle);

    impl Drop for Evt {
        fn drop(&mut self) {
            if self.0 != 0 {
                unsafe { EvtClose(self.0) };
            }
        }
    }

    /// The query of a stream, opened by `open` so a failure is reported before any thread.
    pub struct Reader {
        spec: EventLogSpec,
        /// The remote session of `spec.host` (0: this machine), closed with the reader.
        session: Evt,
    }

    /// A session on `host` with the current user's credentials.
    fn open_session(host: &str) -> Result<Evt, u32> {
        let mut server = wide(host);
        let login = RpcLogin {
            server: server.as_mut_ptr(),
            user: std::ptr::null_mut(),
            domain: std::ptr::null_mut(),
            password: std::ptr::null_mut(),
            flags: 0,
        };
        let h = unsafe { EvtOpenSession(EVT_RPC_LOGIN_CLASS, &login, 0, 0) };
        if h == 0 {
            Err(last_error())
        } else {
            Ok(Evt(h))
        }
    }

    fn last_error() -> u32 {
        unsafe { GetLastError() }
    }

    fn query(spec: &EventLogSpec, session: Handle, reverse: bool) -> Result<Evt, u32> {
        let (path, flags) = match &spec.source {
            Source::Channel(c) => (c.clone(), EVT_QUERY_CHANNEL_PATH),
            Source::File(p) => (p.display().to_string(), EVT_QUERY_FILE_PATH),
        };
        let flags = flags
            | if reverse {
                EVT_QUERY_REVERSE_DIRECTION
            } else {
                0
            };
        let (p, q) = (wide(&path), wide(&spec.filter.xpath()));
        let h = unsafe { EvtQuery(session, p.as_ptr(), q.as_ptr(), flags) };
        if h == 0 {
            Err(last_error())
        } else {
            Ok(Evt(h))
        }
    }

    impl Reader {
        pub fn open(spec: &EventLogSpec) -> Result<Self, StartError> {
            let session = match spec.host.as_deref() {
                Some(host) => match open_session(host) {
                    Ok(s) => s,
                    Err(code) => {
                        return Err(StartError {
                            key: "eventlog_remote_failed",
                            detail: format!("{host}: error {code}"),
                        })
                    }
                },
                None => Evt(0),
            };
            match query(spec, session.0, true) {
                Ok(_) => Ok(Self {
                    spec: spec.clone(),
                    session,
                }),
                Err(RPC_S_SERVER_UNAVAILABLE) => Err(StartError {
                    key: "eventlog_remote_failed",
                    detail: format!(
                        "{}: RPC server unavailable (open Remote Event Log Management in its firewall)",
                        spec.host.as_deref().unwrap_or("")
                    ),
                }),
                Err(ERROR_ACCESS_DENIED) => Err(StartError {
                    key: "eventlog_denied",
                    detail: String::new(),
                }),
                Err(ERROR_EVT_CHANNEL_NOT_FOUND) => Err(StartError {
                    key: "eventlog_not_found",
                    detail: String::new(),
                }),
                Err(code) => Err(StartError {
                    key: "eventlog_failed",
                    detail: format!("error {code}"),
                }),
            }
        }

        pub fn run(self, initial: u32, tx: &SyncSender<Vec<u8>>, shared: &Shared) {
            let mut r = Renderer::new(self.session.0);
            let closed = || shared.closed.load(Ordering::Acquire);
            let set = |s: State| shared.state.store(s as u8, Ordering::Relaxed);
            let fail = |msg: String| {
                if let Ok(mut e) = shared.error.lock() {
                    *e = Some(msg);
                }
                shared.state.store(State::Failed as u8, Ordering::Relaxed);
            };
            let send = |chunk: Vec<u8>, n: u64| -> bool {
                shared.events.fetch_add(n, Ordering::Relaxed);
                tx.send(chunk).is_ok()
            };
            if let Source::File(_) = self.spec.source {
                // A file: every event, oldest first, then done.
                let q = match query(&self.spec, self.session.0, false) {
                    Ok(q) => q,
                    Err(code) => return fail(format!("error {code}")),
                };
                let mut buf = Vec::new();
                let mut n = 0;
                let mut gone = false;
                let read = next_events(&q, true, |ev| {
                    r.render(ev, &mut buf);
                    n += 1;
                    if buf.len() >= 1 << 16 {
                        if closed() || !send(std::mem::take(&mut buf), n) {
                            gone = true;
                            return false;
                        }
                        n = 0;
                    }
                    true
                });
                if !gone && !buf.is_empty() {
                    send(buf, n);
                }
                match read {
                    Ok(()) => set(State::Done),
                    Err(code) => fail(format!("error {code}")),
                }
                return;
            }
            set(State::History);
            // History: the newest `initial` events, newest first, kept as lines in
            // chunks (spilled to a temporary file past the memory budget), then written
            // oldest first.
            let bookmark = Evt(unsafe { EvtCreateBookmark(std::ptr::null()) });
            let mut have_bookmark = false;
            if initial > 0 {
                let q = match query(&self.spec, self.session.0, true) {
                    Ok(q) => q,
                    Err(code) => return fail(format!("error {code}")),
                };
                let mut history = History::new();
                let mut left = initial;
                let read = next_events(&q, true, |ev| {
                    if !have_bookmark {
                        unsafe { EvtUpdateBookmark(bookmark.0, ev) };
                        have_bookmark = true;
                    }
                    let mut line = Vec::new();
                    r.render(ev, &mut line);
                    history.push(line);
                    left -= 1;
                    left > 0 && !closed()
                });
                for (chunk, n) in history.oldest_first() {
                    if closed() || !send(chunk, n) {
                        return;
                    }
                }
                if let Err(code) = read {
                    // What was read is shown; the rest of the history is not.
                    if let Ok(mut e) = shared.error.lock() {
                        *e = Some(format!("history cut short: error {code}"));
                    }
                }
            }
            // Live: after the bookmark (no gap, no repeat), or from now on. A failure
            // (the channel cleared, the Event Log service restarted) subscribes again
            // from the bookmark after a pause.
            let channel = match &self.spec.source {
                Source::Channel(c) => wide(c),
                Source::File(_) => return,
            };
            let xpath = wide(&self.spec.filter.xpath());
            loop {
                if closed() {
                    return;
                }
                let signal = unsafe { CreateEventW(std::ptr::null(), 1, 1, std::ptr::null()) };
                let (mark, flags) = if have_bookmark {
                    (bookmark.0, EVT_SUBSCRIBE_START_AFTER_BOOKMARK)
                } else {
                    (0, EVT_SUBSCRIBE_TO_FUTURE_EVENTS)
                };
                let sub = Evt(unsafe {
                    EvtSubscribe(
                        self.session.0,
                        signal as Handle,
                        channel.as_ptr(),
                        xpath.as_ptr(),
                        mark,
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        flags,
                    )
                });
                let mut broken: Option<u32> = None;
                if sub.0 == 0 {
                    broken = Some(last_error());
                } else {
                    set(State::Live);
                }
                while broken.is_none() && !closed() {
                    // Drained on the signal and on every timeout alike: an event signalled
                    // while the last batch was sent is never left waiting.
                    unsafe {
                        WaitForSingleObject(signal, POLL.as_millis() as u32);
                        ResetEvent(signal);
                    }
                    let mut buf = Vec::new();
                    let mut n = 0;
                    let read = next_events(&sub, false, |ev| {
                        unsafe { EvtUpdateBookmark(bookmark.0, ev) };
                        have_bookmark = true;
                        r.render(ev, &mut buf);
                        n += 1;
                        true
                    });
                    if !buf.is_empty() && (closed() || !send(buf, n)) {
                        unsafe { CloseHandle(signal) };
                        return;
                    }
                    if let Err(code) = read {
                        broken = Some(code);
                    }
                }
                unsafe { CloseHandle(signal) };
                drop(sub);
                let Some(code) = broken else {
                    return;
                };
                if code == ERROR_ACCESS_DENIED {
                    return fail(format!("error {code}"));
                }
                set(State::Resubscribing);
                if let Ok(mut e) = shared.error.lock() {
                    *e = Some(format!("error {code}"));
                }
                for _ in 0..10 {
                    if closed() {
                        return;
                    }
                    std::thread::sleep(POLL);
                }
                if let Ok(mut e) = shared.error.lock() {
                    *e = None;
                }
            }
        }
    }

    /// Calls `f` with each event of `results` until it returns false or none is left;
    /// `Err(code)` when reading stopped on an error other than the end. A query result set
    /// is waited on (`wait`), a subscription is not. Events too large for one call are
    /// read in smaller batches.
    fn next_events(
        results: &Evt,
        wait: bool,
        mut f: impl FnMut(Handle) -> bool,
    ) -> Result<(), u32> {
        let mut events = [0 as Handle; BATCH];
        let mut batch = BATCH;
        let timeout = if wait { INFINITE } else { 0 };
        loop {
            let mut got = 0u32;
            let ok = unsafe {
                EvtNext(
                    results.0,
                    batch as u32,
                    events.as_mut_ptr(),
                    timeout,
                    0,
                    &mut got,
                )
            };
            if ok == 0 {
                match last_error() {
                    ERROR_NO_MORE_ITEMS => return Ok(()),
                    RPC_S_INVALID_BOUND if batch > 1 => {
                        batch /= 2;
                        continue;
                    }
                    code => return Err(code),
                }
            }
            let mut go = true;
            for &ev in &events[..got as usize] {
                if go {
                    go = f(ev);
                }
                unsafe { EvtClose(ev) };
            }
            if !go {
                return Ok(());
            }
        }
    }

    /// The history lines, newest first, in chunks of at most 10,000 lines; the older
    /// chunks spill to a temporary file.
    struct History {
        chunks: Vec<HistoryChunk>,
        current: Vec<Vec<u8>>,
    }

    enum HistoryChunk {
        Memory(Vec<Vec<u8>>),
        Disk(tempfile_path::TempLines),
    }

    mod tempfile_path {
        use std::io::{BufRead, Write};

        /// Lines written to a temporary file, read back in reverse.
        pub struct TempLines {
            path: std::path::PathBuf,
        }

        impl TempLines {
            pub fn write(lines: &[Vec<u8>]) -> Option<Self> {
                let path = std::env::temp_dir().join(format!(
                    "fasttail-eventlog-{}-{}.tmp",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_nanos())
                        .unwrap_or(0)
                ));
                // Security: use create_export_file to ensure the temporary file is verified as
                // a regular file on the opened OS handle before writing or truncating.
                let file = crate::paths::create_export_file(&path).ok()?;
                let mut f = std::io::BufWriter::new(file);
                for l in lines {
                    f.write_all(l).ok()?;
                }
                f.flush().ok()?;
                Some(Self { path })
            }

            /// The events newest last, and how many there are.
            pub fn read_reversed(&self) -> (Vec<u8>, u64) {
                let Ok(f) = std::fs::File::open(&self.path) else {
                    return (Vec::new(), 0);
                };
                // An event is one line plus its indented continuation lines.
                let mut events: Vec<Vec<u8>> = Vec::new();
                for line in std::io::BufReader::new(f)
                    .split(b'\n')
                    .map_while(Result::ok)
                {
                    if line.starts_with(b"  ") {
                        if let Some(last) = events.last_mut() {
                            last.extend_from_slice(&line);
                            last.push(b'\n');
                            continue;
                        }
                    }
                    let mut l = line;
                    l.push(b'\n');
                    events.push(l);
                }
                let n = events.len() as u64;
                (events.into_iter().rev().flatten().collect(), n)
            }
        }

        impl Drop for TempLines {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.path);
            }
        }
    }

    impl History {
        const CHUNK: usize = 10_000;

        fn new() -> Self {
            Self {
                chunks: Vec::new(),
                current: Vec::new(),
            }
        }

        fn push(&mut self, line: Vec<u8>) {
            self.current.push(line);
            if self.current.len() >= Self::CHUNK {
                let lines = std::mem::take(&mut self.current);
                // The first chunk stays in memory, the older ones go to disk.
                let chunk = if self.chunks.is_empty() {
                    HistoryChunk::Memory(lines)
                } else {
                    match tempfile_path::TempLines::write(&lines) {
                        Some(t) => HistoryChunk::Disk(t),
                        None => HistoryChunk::Memory(lines),
                    }
                };
                self.chunks.push(chunk);
            }
        }

        /// The lines oldest first, a chunk at a time.
        /// The events oldest first, a chunk at a time, with the number of events in it.
        fn oldest_first(mut self) -> impl Iterator<Item = (Vec<u8>, u64)> {
            if !self.current.is_empty() {
                let rest = std::mem::take(&mut self.current);
                self.chunks.push(HistoryChunk::Memory(rest));
            }
            self.chunks.into_iter().rev().map(|c| match c {
                HistoryChunk::Memory(lines) => {
                    let n = lines.len() as u64;
                    (lines.into_iter().rev().flatten().collect(), n)
                }
                HistoryChunk::Disk(t) => t.read_reversed(),
            })
        }
    }

    /// Renders events to lines: the system values, the formatted message (publisher
    /// metadata cached), else the event data.
    struct Renderer {
        /// The session the publishers' messages are read in (0: this machine).
        session: Handle,
        system: Evt,
        user: Evt,
        buf: Vec<u8>,
        text: Vec<u16>,
        publishers: HashMap<String, Evt>,
        order: Vec<String>,
    }

    impl Renderer {
        fn new(session: Handle) -> Self {
            Self {
                session,
                system: Evt(unsafe {
                    EvtCreateRenderContext(0, std::ptr::null(), EVT_RENDER_CONTEXT_SYSTEM)
                }),
                user: Evt(unsafe {
                    EvtCreateRenderContext(0, std::ptr::null(), EVT_RENDER_CONTEXT_USER)
                }),
                buf: vec![0; 64 * 1024],
                text: vec![0; 16 * 1024],
                publishers: HashMap::new(),
                order: Vec::new(),
            }
        }

        /// The values of `context` for `event`, in `self.buf`.
        fn values(&mut self, context: Handle, event: Handle) -> Option<usize> {
            loop {
                let (mut used, mut count) = (0u32, 0u32);
                let ok = unsafe {
                    EvtRender(
                        context,
                        event,
                        EVT_RENDER_EVENT_VALUES,
                        self.buf.len() as u32,
                        self.buf.as_mut_ptr() as *mut c_void,
                        &mut used,
                        &mut count,
                    )
                };
                if ok != 0 {
                    return Some(count as usize);
                }
                if last_error() == ERROR_INSUFFICIENT_BUFFER && (used as usize) > self.buf.len() {
                    self.buf.resize(used as usize, 0);
                    continue;
                }
                return None;
            }
        }

        fn variant(&self, i: usize) -> Variant {
            let size = std::mem::size_of::<Variant>();
            let at = i * size;
            let mut v = Variant {
                value: 0,
                count: 0,
                kind: 0,
            };
            if at + size <= self.buf.len() {
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        self.buf.as_ptr().add(at),
                        &mut v as *mut Variant as *mut u8,
                        size,
                    )
                };
            }
            v
        }

        fn string(v: Variant) -> String {
            if v.kind != TYPE_STRING || v.value == 0 {
                return String::new();
            }
            let p = v.value as usize as *const u16;
            let mut len = 0;
            unsafe {
                while *p.add(len) != 0 && len < 1 << 20 {
                    len += 1;
                }
                String::from_utf16_lossy(std::slice::from_raw_parts(p, len))
            }
        }

        /// A value of the event data as text (`?` for the kinds not read).
        fn value_text(v: Variant) -> String {
            match v.kind {
                0 => String::new(),
                TYPE_STRING => Self::string(v),
                // SByte, Int16, Int32, Int64.
                3 => (v.value as u8 as i8).to_string(),
                5 => (v.value as u16 as i16).to_string(),
                7 => (v.value as u32 as i32).to_string(),
                9 => (v.value as i64).to_string(),
                // Boolean (a 32-bit BOOL).
                13 => ((v.value as u32) != 0).to_string(),
                // HexInt32, HexInt64.
                20 => format!("0x{:x}", v.value as u32),
                TYPE_HEXINT64 => format!("0x{:x}", v.value),
                TYPE_BYTE | TYPE_UINT16 | TYPE_UINT32 | TYPE_UINT64 | TYPE_FILETIME => {
                    Self::number(v).to_string()
                }
                _ => "?".to_string(),
            }
        }

        fn number(v: Variant) -> u64 {
            match v.kind {
                TYPE_BYTE => v.value & 0xff,
                TYPE_UINT16 => v.value & 0xffff,
                TYPE_UINT32 => v.value & 0xffff_ffff,
                TYPE_UINT64 | TYPE_HEXINT64 | TYPE_FILETIME => v.value,
                _ => 0,
            }
        }

        fn publisher(&mut self, name: &str) -> Handle {
            if let Some(h) = self.publishers.get(name) {
                return h.0;
            }
            if self.order.len() >= PUBLISHERS {
                let old = self.order.remove(0);
                self.publishers.remove(&old);
            }
            let w = wide(name);
            let h = unsafe {
                EvtOpenPublisherMetadata(self.session, w.as_ptr(), std::ptr::null(), 0, 0)
            };
            self.publishers.insert(name.to_string(), Evt(h));
            self.order.push(name.to_string());
            h
        }

        fn message(&mut self, publisher: Handle, event: Handle) -> Option<String> {
            if publisher == 0 {
                return None;
            }
            loop {
                let mut used = 0u32;
                let ok = unsafe {
                    EvtFormatMessage(
                        publisher,
                        event,
                        0,
                        0,
                        std::ptr::null(),
                        EVT_FORMAT_MESSAGE_EVENT,
                        self.text.len() as u32,
                        self.text.as_mut_ptr(),
                        &mut used,
                    )
                };
                if ok != 0 {
                    let n = (used as usize).saturating_sub(1).min(self.text.len());
                    return Some(String::from_utf16_lossy(&self.text[..n]));
                }
                match last_error() {
                    ERROR_INSUFFICIENT_BUFFER if (used as usize) > self.text.len() => {
                        self.text.resize(used as usize, 0);
                        continue;
                    }
                    // Some inserts were not resolved: the text is there, as Event Viewer
                    // shows it.
                    ERROR_EVT_UNRESOLVED_VALUE_INSERT
                    | ERROR_EVT_UNRESOLVED_PARAMETER_INSERT
                    | ERROR_EVT_MAX_INSERTS_REACHED
                        if used > 0 && (used as usize) <= self.text.len() =>
                    {
                        let n = (used as usize).saturating_sub(1);
                        return Some(String::from_utf16_lossy(&self.text[..n]));
                    }
                    _ => return None,
                }
            }
        }

        /// The event data values joined by `; `, for an event without a message.
        fn data(&mut self, event: Handle) -> String {
            let Some(n) = self.values(self.user.0, event) else {
                return String::new();
            };
            (0..n)
                .map(|i| Self::value_text(self.variant(i)))
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("; ")
        }

        fn render(&mut self, event: Handle, out: &mut Vec<u8>) {
            let Some(n) = self.values(self.system.0, event) else {
                return;
            };
            if n <= PROP_TIME_CREATED {
                return;
            }
            let provider = Self::string(self.variant(PROP_PROVIDER_NAME));
            let id = Self::number(self.variant(PROP_EVENT_ID)) as u32;
            let level = Self::number(self.variant(PROP_LEVEL)) as u8;
            let keywords = Self::number(self.variant(PROP_KEYWORDS));
            let ft = Self::number(self.variant(PROP_TIME_CREATED));
            // FILETIME: 100 ns since 1601.
            let ms = (ft / 10_000) as i64 - 11_644_473_600_000;
            let at = std::time::UNIX_EPOCH + std::time::Duration::from_millis(ms.max(0) as u64);
            let time = crate::syslog::local_time(at).0;
            let publisher = self.publisher(&provider);
            let msg = match self.message(publisher, event) {
                Some(m) if !m.trim().is_empty() => m,
                _ => format!("(no message; data: {})", self.data(event)),
            };
            format_event(out, &time, level_word(level, keywords), &provider, id, &msg);
        }
    }

    pub fn channels() -> Vec<String> {
        let e = Evt(unsafe { EvtOpenChannelEnum(0, 0) });
        if e.0 == 0 {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut buf = vec![0u16; 512];
        loop {
            let mut used = 0u32;
            let ok =
                unsafe { EvtNextChannelPath(e.0, buf.len() as u32, buf.as_mut_ptr(), &mut used) };
            if ok == 0 {
                match last_error() {
                    ERROR_INSUFFICIENT_BUFFER => {
                        buf.resize(used as usize, 0);
                        continue;
                    }
                    _ => break,
                }
            }
            let n = (used as usize).saturating_sub(1).min(buf.len());
            out.push(String::from_utf16_lossy(&buf[..n]));
        }
        out.sort_by_key(|c| c.to_lowercase());
        out
    }
}

#[cfg(not(windows))]
mod platform {
    use super::{EventLogSpec, Shared, StartError};
    use std::sync::mpsc::SyncSender;

    pub struct Reader;

    impl Reader {
        pub fn open(_spec: &EventLogSpec) -> Result<Self, StartError> {
            Err(StartError {
                key: "eventlog_windows_only",
                detail: String::new(),
            })
        }

        pub fn run(self, _initial: u32, _tx: &SyncSender<Vec<u8>>, _shared: &Shared) {}
    }

    pub fn channels() -> Vec<String> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uris_parse_and_round_trip() {
        let spec = EventLogSpec::parse(Path::new(
            "eventlog://Microsoft-Windows-PowerShell%2FOperational?levels=2,1&ids=4104,7034-7040",
        ))
        .unwrap()
        .unwrap();
        assert_eq!(
            spec.source,
            Source::Channel("Microsoft-Windows-PowerShell/Operational".into())
        );
        assert_eq!(spec.filter.levels, vec![1, 2]);
        assert_eq!(
            spec.filter.ids,
            vec![IdRange(4104, 4104), IdRange(7034, 7040)]
        );
        assert_eq!(
            spec.to_path(),
            PathBuf::from(
                "eventlog://Microsoft-Windows-PowerShell%2FOperational?levels=1,2&ids=4104,7034-7040"
            )
        );
        assert_eq!(
            EventLogSpec::parse(Path::new("eventlog://Application")).unwrap(),
            Ok(EventLogSpec {
                source: Source::Channel("Application".into()),
                filter: PreFilter::default(),
                host: None,
            })
        );
        assert!(EventLogSpec::parse(Path::new("eventlog://"))
            .unwrap()
            .is_err());
        assert!(EventLogSpec::parse(Path::new("eventlog://System?levels=9"))
            .unwrap()
            .is_err());
        assert!(EventLogSpec::parse(Path::new("/var/log/syslog")).is_none());
        assert!(is_eventlog_path(Path::new("eventlog://System")));
        // Non-ASCII names round-trip; a `%` before a multi-byte character is kept as is.
        let spec = EventLogSpec {
            source: Source::Channel("Applicazioni è/Operativo".into()),
            filter: PreFilter::default(),
            host: None,
        };
        // A remote machine: `host/Channel`, the channel's own `/` still encoded.
        let remote = EventLogSpec::parse(Path::new(
            "eventlog://srv01/Microsoft-Windows-PowerShell%2FOperational",
        ))
        .unwrap()
        .unwrap();
        assert_eq!(remote.host.as_deref(), Some("srv01"));
        assert_eq!(
            remote.source,
            Source::Channel("Microsoft-Windows-PowerShell/Operational".into())
        );
        assert_eq!(
            EventLogSpec::parse(&remote.to_path()).unwrap().unwrap(),
            remote
        );
        assert_eq!(
            EventLogSpec::parse(Path::new("eventlog://-x/System")).unwrap(),
            Err("eventlog_bad_host")
        );
        assert_eq!(EventLogSpec::parse(&spec.to_path()).unwrap().unwrap(), spec);
        let odd = EventLogSpec::parse(Path::new("eventlog://%aé%4"))
            .unwrap()
            .unwrap();
        assert_eq!(odd.source, Source::Channel("%aé%4".into()));
    }

    #[test]
    fn the_pre_filter_becomes_an_xpath_query() {
        assert_eq!(PreFilter::default().xpath(), "*");
        let f = PreFilter {
            levels: vec![2, 1],
            ids: vec![IdRange(7031, 7031), IdRange(7034, 7040)],
        };
        assert_eq!(
            f.xpath(),
            "*[System[(Level=1 or Level=2) and (EventID=7031 or (EventID>=7034 and EventID<=7040))]]"
        );
        // Information takes LogAlways too.
        let f = PreFilter {
            levels: vec![4],
            ids: vec![],
        };
        assert_eq!(f.xpath(), "*[System[(Level=0 or Level=4)]]");
        assert!(PreFilter::parse_ids("1,2-1").is_err());
        assert!(PreFilter::parse_ids(
            &(0..40).map(|i| i.to_string()).collect::<Vec<_>>().join(",")
        )
        .is_err());
        assert_eq!(PreFilter::parse_ids(" 5 , 7-9 ").unwrap().len(), 2);
    }

    #[test]
    fn event_lines_read_as_their_level_and_time() {
        assert_eq!(level_word(1, 0), "FATAL");
        assert_eq!(level_word(2, 0), "ERROR");
        assert_eq!(level_word(3, 0), "WARN");
        assert_eq!(level_word(4, 0), "INFO");
        assert_eq!(level_word(5, 0), "DEBUG");
        assert_eq!(level_word(0, 0x0010_0000_0000_0000), "WARN");
        assert_eq!(level_word(0, 0x0020_0000_0000_0000), "INFO");
        let mut out = Vec::new();
        format_event(
            &mut out,
            "2026-09-28T14:02:05.123+02:00",
            "ERROR",
            "Service Control Manager",
            7031,
            "The Print Spooler service terminated unexpectedly.\r\nIt has done this 1 time(s).\r\n",
        );
        let text = String::from_utf8(out).unwrap();
        assert_eq!(
            text,
            "2026-09-28T14:02:05.123+02:00 ERROR [Service Control Manager] 7031: The Print Spooler service terminated unexpectedly.\n  It has done this 1 time(s).\n"
        );
        let first = text.lines().next().unwrap();
        assert_eq!(
            crate::log_level::detect_level_in(first, 4096),
            crate::log_level::LogLevel::Error
        );
        assert!(
            crate::timestamp::leading_span(first, crate::timestamp::FormatHint::default())
                .is_some()
        );
        // A long message is cut.
        let mut out = Vec::new();
        format_event(&mut out, "t", "INFO", "p", 1, &"x".repeat(MAX_MESSAGE + 10));
        assert!(out.len() < MAX_MESSAGE + 100);
        assert!(String::from_utf8(out).unwrap().trim_end().ends_with('…'));
    }

    #[test]
    fn evtx_files_are_known_by_their_magic() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.evtx");
        std::fs::write(&a, b"ElfFile\0rest").unwrap();
        let b = dir.path().join("b.evtx");
        std::fs::write(&b, b"plain text").unwrap();
        assert!(is_evtx_file(&a));
        assert!(!is_evtx_file(&b));
    }
}
