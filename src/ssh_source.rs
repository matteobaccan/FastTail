// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Remote files over SSH: `ssh://[user@]host[:port]/path` as a followed stream.
//!
//! FastTail runs the system OpenSSH client (`ssh_program`) in batch mode with a one-line
//! POSIX script that prints a marker (`FASTTAIL1 <inode> <size> <start>`) on standard
//! error and then `tail -c +<start> -F` the file; standard output is copied into a spool
//! by the standard-input copier (`stdin_source`), so the engine tails it like any file.
//! Authentication is the client's: keys, the agent, `~/.ssh/config`, `known_hosts`;
//! FastTail never asks for or keeps a credential.
//!
//! `SshReader` is the copier's input: one endless `Read` that connects, follows, and on a
//! lost connection connects again with back-off (1, 2, 5, 10 s, then every 30 s), asking
//! the server to go on after the last byte received when the file is still the same one
//! (same inode, at least as long); otherwise the spool restarts from empty with the
//! initial content of the new file. A refused password, an unknown host key or a missing
//! file stop the stream until the user reconnects. At most 32 `ssh` processes run at once.

use crate::stdin_source::{InputState, Settings, StdinStream};
use crate::tail_engine::{TailEngine, WakeFn};
use std::io::{BufRead, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

pub const DEFAULT_SSH_PROGRAM: &str = "ssh";
pub const DEFAULT_REMOTE_SPOOL_MAX_MB: u32 = 2048;
pub const DEFAULT_REMOTE_INITIAL_MB: u32 = 64;
pub const MAX_REMOTE_INITIAL_MB: u32 = 4096;
/// `ssh` processes running at once across FastTail.
pub const MAX_CHILDREN: usize = 32;

const SCHEME: &str = "ssh://";
/// The URL fragment of a stream fetched from the first byte.
const WHOLE: &str = "#whole";
/// How long the marker may take after `ssh` starts (its own connect timeout is 10 s).
const MARKER_TIMEOUT: Duration = Duration::from_secs(30);
/// Back-off between connection attempts, the last one repeated.
const BACKOFF_SECS: [u64; 5] = [1, 2, 5, 10, 30];
/// Bytes of the client's standard error kept for the messages.
const STDERR_KEPT: usize = 4096;

static RUNNING: AtomicUsize = AtomicUsize::new(0);

/// A remote file, as its URL names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshSpec {
    pub user: Option<String>,
    pub host: String,
    pub port: Option<u16>,
    /// The absolute remote path, percent-decoded.
    pub path: String,
    /// Fetch from the first byte instead of the last `remote_initial_mb`.
    pub whole: bool,
}

/// True when `path` is written as an `ssh://` URL (valid or not).
pub fn is_ssh_path(path: &Path) -> bool {
    path.to_str().is_some_and(|s| s.starts_with(SCHEME))
}

fn valid_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 255
        && !host.starts_with('-')
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:[]-".contains(&b))
}

fn valid_user(user: &str) -> bool {
    !user.is_empty()
        && user.len() <= 64
        && !user.starts_with('-')
        && user
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

/// `%XX` sequences decoded; `None` for a malformed one or bytes that are not UTF-8.
fn percent_decode(text: &str) -> Option<String> {
    let b = text.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// `%`, `#`, space and control characters encoded, the rest as it is.
fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch == '%' || ch == '#' || ch == ' ' || ch.is_control() {
            let mut buf = [0u8; 4];
            for b in ch.encode_utf8(&mut buf).bytes() {
                out.push_str(&format!("%{b:02X}"));
            }
        } else {
            out.push(ch);
        }
    }
    out
}

impl SshSpec {
    /// The remote file `path` names: `None` when it is no `ssh://` URL, `Some(Err)` with
    /// the reason (an i18n key) when it is one that cannot be used.
    pub fn parse(path: &Path) -> Option<Result<Self, &'static str>> {
        let text = path.to_str()?.strip_prefix(SCHEME)?;
        Some(Self::parse_rest(text))
    }

    fn parse_rest(text: &str) -> Result<Self, &'static str> {
        let (text, whole) = match text.strip_suffix(WHOLE) {
            Some(t) => (t, true),
            None => (text, false),
        };
        let slash = text.find('/').ok_or("ssh_bad_path")?;
        let (authority, raw_path) = text.split_at(slash);
        let (user, hostport) = match authority.rsplit_once('@') {
            Some((u, h)) => (Some(u), h),
            None => (None, authority),
        };
        if user.is_some_and(|u| !valid_user(u)) {
            return Err("ssh_bad_user");
        }
        // `[::1]:2222`, `host:2222`, or a bare host (an IPv6 address only in brackets).
        let (host, port) = if let Some(rest) = hostport.strip_prefix('[') {
            let (inner, after) = rest.split_once(']').ok_or("ssh_bad_host")?;
            let port = match after.strip_prefix(':') {
                Some(p) => Some(p.parse::<u16>().map_err(|_| "ssh_bad_port")?),
                None if after.is_empty() => None,
                None => return Err("ssh_bad_host"),
            };
            (inner.to_string(), port)
        } else {
            match hostport.rsplit_once(':') {
                Some((h, p)) => (
                    h.to_string(),
                    Some(p.parse::<u16>().map_err(|_| "ssh_bad_port")?),
                ),
                None => (hostport.to_string(), None),
            }
        };
        if !valid_host(&host) || port == Some(0) {
            return Err(if port == Some(0) {
                "ssh_bad_port"
            } else {
                "ssh_bad_host"
            });
        }
        let path = percent_decode(raw_path).ok_or("ssh_bad_path")?;
        if path.len() < 2 || path.ends_with('/') || path.contains('\0') {
            return Err("ssh_bad_path");
        }
        Ok(Self {
            user: user.map(String::from),
            host,
            port,
            path,
            whole,
        })
    }

    pub fn to_uri(&self) -> String {
        let user = self
            .user
            .as_ref()
            .map(|u| format!("{u}@"))
            .unwrap_or_default();
        let host = if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        let port = self.port.map(|p| format!(":{p}")).unwrap_or_default();
        let whole = if self.whole { WHOLE } else { "" };
        format!(
            "{SCHEME}{user}{host}{port}{}{whole}",
            percent_encode(&self.path)
        )
    }

    /// The tab title: `web1:app.log`.
    pub fn title(&self) -> String {
        let name = self.path.rsplit('/').next().unwrap_or(&self.path);
        format!("{}:{name}", self.host)
    }
}

/// The client program and how much a first fetch reads (`fasttail.ini`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshSettings {
    pub program: String,
    /// Bytes from the end a first fetch starts at (0: only new lines).
    pub initial_bytes: u64,
}

impl Default for SshSettings {
    fn default() -> Self {
        Self {
            program: DEFAULT_SSH_PROGRAM.to_string(),
            initial_bytes: u64::from(DEFAULT_REMOTE_INITIAL_MB) << 20,
        }
    }
}

/// Where the next connection asks the server to start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Start {
    /// A first fetch: the whole file, or its last `bytes`.
    Initial { whole: bool, bytes: u64 },
    /// After `offset` bytes of the file whose inode is `inode`, when it is still that
    /// file and at least that long; otherwise a first fetch of `bytes`.
    Resume {
        inode: String,
        offset: u64,
        bytes: u64,
        whole: bool,
    },
}

/// The remote script: the marker on standard error, then `tail` from the start chosen on
/// the server (so the size and the start come from the same moment).
pub fn remote_script(path: &str, start: &Start) -> String {
    // A tail fetch asks for one byte more than `bytes` (the client drops through the
    // first line break), so a fetch that starts on a line start keeps that line.
    let initial = |whole: bool, bytes: u64| {
        if whole {
            "b=1; m=I".to_string()
        } else {
            format!("if [ \"$s\" -gt {bytes} ]; then b=$((s-{bytes})); else b=1; fi; m=I")
        }
    };
    let choose = match start {
        Start::Initial { whole, bytes } => initial(*whole, *bytes),
        Start::Resume {
            inode,
            offset,
            bytes,
            whole,
        } => format!(
            "if [ \"$i\" = {} ] && [ \"$s\" -ge {offset} ]; then b=$(({offset}+1)); m=R; else {}; fi",
            crate::external_tools::quote_sh_arg(inode),
            initial(*whole, *bytes)
        ),
    };
    // `set -f`: the fields of `ls` are not globbed; a C locale and no block size keep the
    // size a plain number. `m` tells the client whether it resumed (R) or started (I).
    format!(
        "set -f; f={}; [ -r \"$f\" ] || {{ printf 'FASTTAIL1 - - - -\\n' >&2; exit 2; }}; \
         unset BLOCK_SIZE LS_BLOCK_SIZE; set -- $(LC_ALL=C ls -iLnd -- \"$f\" 2>/dev/null); i=$1; s=$6; \
         if [ -z \"$i\" ]; then printf 'FASTTAIL1 - - - -\\n' >&2; exit 2; fi; \
         {choose}; printf 'FASTTAIL1 %s %s %s %s\\n' \"$i\" \"$s\" \"$b\" \"$m\" >&2; \
         exec tail -c +\"$b\" -F -- \"$f\"",
        crate::external_tools::quote_sh_arg(path)
    )
}

/// The client's arguments: batch mode (no prompt ever), timeouts and keep-alives, the
/// user and port, then `--`, the host and the script.
pub fn ssh_args(spec: &SshSpec, start: &Start) -> Vec<String> {
    let mut args: Vec<String> = [
        "-T",
        "-n",
        "-o",
        "BatchMode=yes",
        "-o",
        "ConnectTimeout=10",
        "-o",
        "ServerAliveInterval=15",
        "-o",
        "ServerAliveCountMax=3",
        // A persisted master of the user's config could keep the pipes open, and a
        // RemoteCommand would replace the script.
        "-o",
        "ControlMaster=no",
        "-o",
        "ControlPath=none",
        "-o",
        "RemoteCommand=none",
    ]
    .map(String::from)
    .to_vec();
    if let Some(user) = &spec.user {
        args.extend(["-l".to_string(), user.clone()]);
    }
    if let Some(port) = spec.port {
        args.extend(["-p".to_string(), port.to_string()]);
    }
    args.extend([
        "--".to_string(),
        spec.host.clone(),
        remote_script(&spec.path, start),
    ]);
    args
}

/// Why a remote stream stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Disconnect {
    /// The server wants a password (or the key was refused).
    Password,
    /// The host key is unknown or changed; the client's message.
    HostKey(String),
    /// No such remote file (or not readable).
    Missing,
    /// The user pressed Disconnect.
    Stopped,
    /// Anything else, with the client's last message.
    Failed(String),
}

/// The state shown in the stream bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SshState {
    /// Waiting for one of the `MAX_CHILDREN` slots.
    Queued,
    Connecting,
    Following,
    /// Attempt `n` after a lost connection.
    Reconnecting(u32),
    Disconnected(Disconnect),
}

struct Shared {
    state: Mutex<SshState>,
    /// The stream was closed: no new connection, the reader ends.
    closed: AtomicBool,
    /// Disconnect was pressed.
    stop: AtomicBool,
    /// The running client, killed on close and Disconnect.
    child: Mutex<Option<Child>>,
    /// Bytes of the remote file the spool holds up to (the next start is this + 1).
    offset: AtomicU64,
    /// Inode of the remote file followed.
    inode: Mutex<Option<String>>,
    /// The client's last messages.
    stderr: Mutex<String>,
    /// `tail` said the file was truncated or replaced while connected: the next
    /// connection starts again instead of resuming.
    changed: AtomicBool,
}

impl Shared {
    fn set_state(&self, state: SshState) {
        if let Ok(mut s) = self.state.lock() {
            *s = state;
        }
    }

    fn kill_child(&self) {
        if let Some(mut child) = self.child.lock().ok().and_then(|mut c| c.take()) {
            let _ = child.kill();
            let _ = std::thread::Builder::new()
                .name("fasttail-reap".to_string())
                .spawn(move || {
                    let _ = child.wait();
                });
        }
    }

    fn ended(&self) -> bool {
        self.closed.load(Ordering::Acquire) || self.stop.load(Ordering::Acquire)
    }
}

/// A slot among the `MAX_CHILDREN`, given back when dropped.
struct Slot;

impl Slot {
    fn try_take() -> Option<Slot> {
        RUNNING
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < MAX_CHILDREN).then_some(n + 1)
            })
            .ok()
            .map(|_| Slot)
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        RUNNING.fetch_sub(1, Ordering::AcqRel);
    }
}

/// One connection being followed.
struct Connection {
    out: ChildStdout,
    /// The partial first line of a fetch that starts mid-file is still to drop.
    skip_line: bool,
    _slot: Slot,
}

/// What ended a connection.
enum Outcome {
    /// Retry after the back-off.
    Retry,
    /// Stop until the user reconnects.
    Stop(Disconnect),
}

/// The copier's input: connects, follows, and connects again.
struct SshReader {
    spec: SshSpec,
    settings: SshSettings,
    shared: Arc<Shared>,
    wake: Option<WakeFn>,
    conn: Option<Connection>,
    /// Failed attempts since the last byte received.
    attempt: u32,
    /// Closed when the stderr thread of the last client finished.
    stderr_done: Option<mpsc::Receiver<()>>,
}

impl Drop for SshReader {
    fn drop(&mut self) {
        // The copy stopped (closed, or a spool error): the client goes with it.
        self.shared.kill_child();
    }
}

impl SshReader {
    fn wake(&self) {
        if let Some(w) = &self.wake {
            w();
        }
    }

    fn set_state(&self, state: SshState) {
        self.shared.set_state(state);
        self.wake();
    }

    /// Sleeps `secs`, waking early when the stream ends.
    fn sleep(&self, secs: u64) {
        for _ in 0..secs * 10 {
            if self.shared.ended() {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// Waits for a free slot (`Queued` meanwhile); `None` when the stream ended.
    fn slot(&self) -> Option<Slot> {
        let mut queued = false;
        loop {
            if self.shared.ended() {
                return None;
            }
            if let Some(slot) = Slot::try_take() {
                return Some(slot);
            }
            if !queued {
                self.set_state(SshState::Queued);
                queued = true;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    /// Connects once. `Ok(Some(replaced))` on success (the file changed since the last
    /// connection), `Ok(None)` when the stream ended meanwhile.
    fn connect(&mut self) -> Result<Option<bool>, Outcome> {
        let Some(slot) = self.slot() else {
            return Ok(None);
        };
        self.set_state(if self.attempt == 0 {
            SshState::Connecting
        } else {
            SshState::Reconnecting(self.attempt)
        });
        let mut inode = self.shared.inode.lock().ok().and_then(|i| i.clone());
        let had_file = inode.is_some();
        if self.shared.changed.swap(false, Ordering::AcqRel) {
            inode = None;
        }
        let offset = self.shared.offset.load(Ordering::Acquire);
        let start = match inode {
            Some(inode) => Start::Resume {
                inode,
                offset,
                bytes: self.settings.initial_bytes,
                whole: self.spec.whole,
            },
            None => Start::Initial {
                whole: self.spec.whole,
                bytes: self.settings.initial_bytes,
            },
        };
        let mut cmd = Command::new(&self.settings.program);
        cmd.args(ssh_args(&self.spec, &start))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let mut child = cmd.spawn().map_err(|e| {
            Outcome::Stop(Disconnect::Failed(format!(
                "{}: {e}",
                self.settings.program
            )))
        })?;
        let out = child.stdout.take().expect("piped");
        let err = child.stderr.take().expect("piped");
        let (tx, rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel::<()>();
        self.stderr_done = Some(done_rx);
        let shared = self.shared.clone();
        let _ = std::thread::Builder::new()
            .name("fasttail-ssh-err".to_string())
            .spawn(move || {
                drain_stderr(err, tx, shared);
                drop(done_tx);
            });
        if let Ok(mut c) = self.shared.child.lock() {
            *c = Some(child);
        }
        if self.shared.ended() {
            self.shared.kill_child();
            return Ok(None);
        }
        let marker = match rx.recv_timeout(MARKER_TIMEOUT) {
            Ok(m) => m,
            Err(_) => {
                // No marker: the client failed (or hangs); its exit says why.
                self.shared.kill_child_if_hung();
                return Err(self.classify(true));
            }
        };
        let Some((inode, start_at, resumed)) = marker else {
            self.shared.kill_child();
            return Err(Outcome::Stop(Disconnect::Missing));
        };
        // A stream that had a file and did not resume it starts again.
        let replaced = had_file && !resumed;
        if let Ok(mut i) = self.shared.inode.lock() {
            *i = Some(inode);
        }
        self.shared
            .offset
            .store(start_at.saturating_sub(1), Ordering::Release);
        // A first fetch after byte 1 starts one byte early: dropped through the first
        // line break, it leaves whole lines (just that byte when it is a line break).
        let first_fetch_mid_file = !resumed && start_at > 1;
        self.conn = Some(Connection {
            out,
            skip_line: first_fetch_mid_file,
            _slot: slot,
        });
        self.set_state(SshState::Following);
        Ok(Some(replaced))
    }

    /// Why the client ended: waits for it and reads its messages.
    /// `before_marker`: the client ended before the script ran, so its messages are the
    /// client's own (a refused key, a host key); after it, they are `tail`'s.
    fn classify(&mut self, before_marker: bool) -> Outcome {
        let status = self
            .shared
            .child
            .lock()
            .ok()
            .and_then(|mut c| c.take())
            .and_then(|mut c| {
                if matches!(c.try_wait(), Ok(None)) && !before_marker {
                    // Output ended but the client still runs (a read error): end it.
                    let _ = c.kill();
                }
                c.wait().ok()
            });
        // The stderr thread stores the last lines before it ends.
        if let Some(done) = self.stderr_done.take() {
            let _ = done.recv_timeout(Duration::from_secs(2));
        }
        let text = self
            .shared
            .stderr
            .lock()
            .map(|s| s.clone())
            .unwrap_or_default();
        let last = text
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or_default()
            .trim()
            .to_string();
        if before_marker
            && (text.contains("Host key verification failed")
                || text.contains("REMOTE HOST IDENTIFICATION HAS CHANGED"))
        {
            let shown: Vec<&str> = text
                .lines()
                .filter(|l| !l.trim().is_empty())
                .rev()
                .take(4)
                .collect();
            return Outcome::Stop(Disconnect::HostKey(
                shown.into_iter().rev().collect::<Vec<_>>().join("\n"),
            ));
        }
        if before_marker && text.contains("Permission denied (") {
            return Outcome::Stop(Disconnect::Password);
        }
        match status.and_then(|s| s.code()) {
            // The connection: lost, refused, or the host did not answer.
            Some(255) | None => Outcome::Retry,
            Some(0) => Outcome::Retry,
            Some(127) => Outcome::Stop(Disconnect::Failed(if last.is_empty() {
                "tail: command not found".to_string()
            } else {
                last
            })),
            Some(code) => Outcome::Stop(Disconnect::Failed(if last.is_empty() {
                format!("exit code {code}")
            } else {
                last
            })),
        }
    }
}

impl Shared {
    /// Ends a client that printed nothing within the marker timeout.
    fn kill_child_if_hung(&self) {
        if let Ok(mut guard) = self.child.lock() {
            if let Some(child) = guard.as_mut() {
                if matches!(child.try_wait(), Ok(None)) {
                    let _ = child.kill();
                }
            }
        }
    }
}

/// Reads the client's standard error: the first marker goes to `tx` (`None` inside for a
/// missing file), every other line to the kept messages.
fn drain_stderr(
    err: std::process::ChildStderr,
    tx: mpsc::Sender<Option<(String, u64, bool)>>,
    shared: Arc<Shared>,
) {
    let mut tx = Some(tx);
    for line in std::io::BufReader::new(err).split(b'\n') {
        let Ok(line) = line else {
            break;
        };
        let line = String::from_utf8_lossy(&line);
        let line = line.trim_end_matches('\r');
        if let Some(rest) = line.strip_prefix("FASTTAIL1 ") {
            if let Some(tx) = tx.take() {
                let parts: Vec<&str> = rest.split_whitespace().collect();
                let marker = match parts.as_slice() {
                    [inode, _size, start, mode] if *inode != "-" => start
                        .parse::<u64>()
                        .ok()
                        .map(|s| (inode.to_string(), s.max(1), *mode == "R")),
                    _ => None,
                };
                let _ = tx.send(marker);
            }
            continue;
        }
        // GNU and BusyBox `tail -F` on a truncation or a new file of that name: the
        // position no longer matches, so the next connection starts again.
        if tx.is_none()
            && (line.contains("file truncated")
                || line.contains("has been replaced")
                || line.contains("following new file")
                || line.contains("has appeared"))
        {
            shared.changed.store(true, Ordering::Release);
        }
        if let Ok(mut kept) = shared.stderr.lock() {
            kept.push_str(line);
            kept.push('\n');
            if kept.len() > STDERR_KEPT {
                let mut cut = kept.len() - STDERR_KEPT;
                while !kept.is_char_boundary(cut) {
                    cut += 1;
                }
                kept.drain(..cut);
            }
        }
    }
}

impl Read for SshReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        loop {
            if self.shared.ended() {
                self.shared.kill_child();
                if self.shared.stop.load(Ordering::Acquire) {
                    self.set_state(SshState::Disconnected(Disconnect::Stopped));
                }
                return Ok(0);
            }
            if let Some(conn) = self.conn.as_mut() {
                let n = match conn.out.read(buf) {
                    Ok(n) => n,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => 0,
                };
                if n > 0 {
                    self.shared.offset.fetch_add(n as u64, Ordering::AcqRel);
                    self.attempt = 0;
                    if conn.skip_line {
                        match buf[..n].iter().position(|&b| b == b'\n') {
                            Some(nl) => {
                                conn.skip_line = false;
                                buf.copy_within(nl + 1..n, 0);
                                let left = n - nl - 1;
                                if left == 0 {
                                    continue;
                                }
                                return Ok(left);
                            }
                            None => continue,
                        }
                    }
                    return Ok(n);
                }
                // The connection ended.
                self.conn = None;
                if self.shared.ended() {
                    continue;
                }
                match self.classify(false) {
                    // No byte since: back off like a failed connection.
                    Outcome::Retry => self.attempt += 1,
                    Outcome::Stop(reason) => {
                        self.set_state(SshState::Disconnected(reason));
                        return Ok(0);
                    }
                }
            }
            if self.attempt > 0 {
                let i = (self.attempt as usize - 1).min(BACKOFF_SECS.len() - 1);
                self.set_state(SshState::Reconnecting(self.attempt));
                self.sleep(BACKOFF_SECS[i]);
                if self.shared.ended() {
                    continue;
                }
            }
            if let Ok(mut kept) = self.shared.stderr.lock() {
                kept.clear();
            }
            match self.connect() {
                Ok(None) => continue,
                Ok(Some(true)) => return Err(crate::stdin_source::restart_request()),
                Ok(Some(false)) => {}
                Err(Outcome::Retry) => self.attempt += 1,
                Err(Outcome::Stop(reason)) => {
                    self.set_state(SshState::Disconnected(reason));
                    return Ok(0);
                }
            }
        }
    }
}

/// A remote stream, owned by its engine (`TailEngine::ssh`).
pub struct SshStream {
    pub spec: SshSpec,
    shared: Arc<Shared>,
    settings: SshSettings,
    spool_settings: Settings,
    wake: Option<WakeFn>,
    /// Reconnect / Disconnect clicked (applied by the front end).
    pub reconnect_requested: bool,
    pub disconnect_requested: bool,
    /// The output copier; declared last so the client ends before the spool goes.
    pub copier: StdinStream,
}

impl SshStream {
    pub fn start(
        spec: SshSpec,
        settings: &SshSettings,
        spool_settings: &Settings,
        wake: Option<WakeFn>,
    ) -> std::io::Result<Self> {
        if !crate::system_source::valid_program(&settings.program) {
            return Err(std::io::Error::other(
                "the program must be a name on PATH or an absolute path",
            ));
        }
        let shared = Arc::new(Shared {
            state: Mutex::new(SshState::Connecting),
            closed: AtomicBool::new(false),
            stop: AtomicBool::new(false),
            child: Mutex::new(None),
            offset: AtomicU64::new(0),
            inode: Mutex::new(None),
            stderr: Mutex::new(String::new()),
            changed: AtomicBool::new(false),
        });
        let reader = SshReader {
            spec: spec.clone(),
            settings: settings.clone(),
            shared: shared.clone(),
            wake: wake.clone(),
            conn: None,
            attempt: 0,
            stderr_done: None,
        };
        let copier = StdinStream::start_named(reader, spool_settings, wake.clone(), "ssh")?;
        Ok(Self {
            spec,
            shared,
            settings: settings.clone(),
            spool_settings: spool_settings.clone(),
            wake,
            reconnect_requested: false,
            disconnect_requested: false,
            copier,
        })
    }

    pub fn state(&self) -> SshState {
        self.shared
            .state
            .lock()
            .map(|s| s.clone())
            .unwrap_or(SshState::Disconnected(Disconnect::Failed(
                "poisoned".into(),
            )))
    }

    /// The client's last messages.
    pub fn messages(&self) -> String {
        self.shared
            .stderr
            .lock()
            .map(|s| s.clone())
            .unwrap_or_default()
    }

    /// Stops following: the client ends, the lines stay.
    pub fn disconnect(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        self.shared.kill_child();
    }

    /// Connects again once the copy stopped (Disconnect, or a stop that is not retried),
    /// going on after the last byte received.
    pub fn reconnect(&mut self) -> std::io::Result<()> {
        if self.copier.state() == InputState::Reading {
            return Err(std::io::Error::other("the stream is still connected"));
        }
        self.shared.stop.store(false, Ordering::Release);
        self.shared.set_state(SshState::Connecting);
        let reader = SshReader {
            spec: self.spec.clone(),
            settings: self.settings.clone(),
            shared: self.shared.clone(),
            wake: self.wake.clone(),
            conn: None,
            attempt: 0,
            stderr_done: None,
        };
        self.copier
            .resume(reader, &self.spool_settings, self.wake.clone())
    }

    /// For tests: drops the connection as a network failure would.
    pub fn drop_connection_for_test(&self) {
        if let Ok(mut guard) = self.shared.child.lock() {
            if let Some(child) = guard.as_mut() {
                let _ = child.kill();
            }
        }
    }
}

impl Drop for SshStream {
    fn drop(&mut self) {
        self.shared.closed.store(true, Ordering::Release);
        self.shared.kill_child();
    }
}

/// Opens the stream of `spec`: `path` is its URL, follow is on.
pub fn open_engine(
    spec: SshSpec,
    settings: &SshSettings,
    spool_settings: &Settings,
    wake: Option<WakeFn>,
) -> std::io::Result<TailEngine> {
    let stream = SshStream::start(spec, settings, spool_settings, wake.clone())?;
    let mut engine = match wake {
        Some(wake) => TailEngine::open_with_wake(stream.copier.spool_path(), wake)?,
        None => TailEngine::open(stream.copier.spool_path())?,
    };
    engine.path = PathBuf::from(stream.spec.to_uri());
    engine.follow_tail = true;
    engine.ssh = Some(stream);
    Ok(engine)
}

/// The stream bar text of `state` in `lang`, and whether it is a warning.
pub fn state_text(state: &SshState, lang: crate::i18n::Language) -> (String, bool) {
    use crate::i18n::t;
    match state {
        SshState::Queued => (t(lang, "ssh_queued").to_string(), false),
        SshState::Connecting => (t(lang, "ssh_connecting").to_string(), false),
        SshState::Following => (t(lang, "ssh_following").to_string(), false),
        SshState::Reconnecting(n) => (
            t(lang, "ssh_reconnecting").replace("{0}", &n.to_string()),
            true,
        ),
        SshState::Disconnected(reason) => {
            let why = match reason {
                Disconnect::Password => t(lang, "ssh_password").to_string(),
                Disconnect::HostKey(_) => t(lang, "ssh_hostkey").to_string(),
                Disconnect::Missing => t(lang, "ssh_missing").to_string(),
                Disconnect::Stopped => t(lang, "ssh_stopped").to_string(),
                Disconnect::Failed(text) => text.clone(),
            };
            (t(lang, "ssh_disconnected").replace("{0}", &why), true)
        }
    }
}

impl SshStream {
    /// True once the copy stopped: Reconnect can run.
    pub fn is_stopped(&self) -> bool {
        self.copier.state() != InputState::Reading
    }
}

impl TailEngine {
    /// Applies a Reconnect or Disconnect asked for from the stream bar; the error, if any.
    pub fn apply_ssh_request(&mut self) -> Option<String> {
        let s = self.ssh.as_mut()?;
        if std::mem::take(&mut s.disconnect_requested) {
            s.disconnect();
        }
        if std::mem::take(&mut s.reconnect_requested) {
            if let Err(e) = s.reconnect() {
                return Some(e.to_string());
            }
            self.follow_tail = true;
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Option<Result<SshSpec, &'static str>> {
        SshSpec::parse(Path::new(s))
    }

    #[test]
    fn urls_are_parsed_and_written_back() {
        let s = parse("ssh://deploy@web1:2222/var/log/app.log")
            .unwrap()
            .unwrap();
        assert_eq!(s.user.as_deref(), Some("deploy"));
        assert_eq!((s.host.as_str(), s.port), ("web1", Some(2222)));
        assert_eq!(s.path, "/var/log/app.log");
        assert!(!s.whole);
        assert_eq!(s.to_uri(), "ssh://deploy@web1:2222/var/log/app.log");
        assert_eq!(s.title(), "web1:app.log");
        let v6 = parse("ssh://[::1]:22/tmp/my%20log.txt#whole")
            .unwrap()
            .unwrap();
        assert_eq!((v6.host.as_str(), v6.port), ("::1", Some(22)));
        assert_eq!(v6.path, "/tmp/my log.txt");
        assert!(v6.whole);
        assert_eq!(v6.to_uri(), "ssh://[::1]:22/tmp/my%20log.txt#whole");
        assert_eq!(parse("/var/log/x"), None);
        assert!(is_ssh_path(Path::new("ssh://h/x")));
    }

    #[test]
    fn bad_urls_are_refused_before_anything_runs() {
        for (url, why) in [
            ("ssh://-oProxyCommand=calc/tmp/x", "ssh_bad_host"),
            ("ssh://a b/tmp/x", "ssh_bad_host"),
            ("ssh://h;id/tmp/x", "ssh_bad_host"),
            ("ssh://-l@h/tmp/x", "ssh_bad_user"),
            ("ssh://h:99999/tmp/x", "ssh_bad_port"),
            ("ssh://h:0/tmp/x", "ssh_bad_port"),
            ("ssh://h", "ssh_bad_path"),
            ("ssh://h/", "ssh_bad_path"),
            ("ssh://h/tmp/", "ssh_bad_path"),
            ("ssh://h/bad%zz", "ssh_bad_path"),
        ] {
            assert_eq!(parse(url), Some(Err(why)), "{url}");
        }
    }

    #[test]
    fn the_command_line_keeps_the_host_and_the_path_out_of_option_reach() {
        let s = parse("ssh://deploy@web1:2222/var/log/it's.log")
            .unwrap()
            .unwrap();
        let args = ssh_args(
            &s,
            &Start::Initial {
                whole: false,
                bytes: 1024,
            },
        );
        assert!(args.contains(&"BatchMode=yes".to_string()));
        let dash = args.iter().position(|a| a == "--").unwrap();
        assert_eq!(args[dash + 1], "web1");
        assert_eq!(args[dash - 4..dash], ["-l", "deploy", "-p", "2222"]);
        let script = &args[dash + 2];
        assert!(
            script.starts_with(r"set -f; f='/var/log/it'\''s.log';"),
            "{script}"
        );
        assert!(script.contains("b=$((s-1024))"), "{script}");
        assert!(script.contains(r#"[ -r "$f" ] ||"#), "{script}");
        assert!(args.contains(&"ControlMaster=no".to_string()));
        assert!(
            script.ends_with(r#"exec tail -c +"$b" -F -- "$f""#),
            "{script}"
        );
        let resume = remote_script(
            "/x",
            &Start::Resume {
                inode: "42".into(),
                offset: 900,
                bytes: 10,
                whole: true,
            },
        );
        assert!(
            resume.contains(r#"if [ "$i" = '42' ] && [ "$s" -ge 900 ]; then b=$((900+1)); m=R; else b=1; m=I; fi"#),
            "{resume}"
        );
        assert!(remote_script(
            "/x",
            &Start::Initial {
                whole: true,
                bytes: 1
            }
        )
        .contains("b=1; m=I"));
    }
}
