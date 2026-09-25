//! Standard input as a stream: `command | fasttail -`.
//!
//! A pipe cannot be read twice, and the engine relies on random access, so a background
//! thread copies standard input into a spool file (see `spool`) that the engine tails
//! like any followed log. Nothing in the engine learns about pipes.
//!
//! - `classify` tells whether standard input is piped (a pipe, a socket or a redirected
//!   file), a terminal, or absent. On Windows the handle passed by the shell is read
//!   whatever the subsystem of the executable: `#![windows_subsystem = "windows"]` only
//!   decides whether a console is created, not the standard handles.
//! - `StdinStream::start` spawns the copier: 64 KB reads, each written and flushed at
//!   once so a line-buffered producer shows up immediately, the wake callback after each
//!   write. At `stdin_spool_max_mb`, or when the spool volume keeps less than 512 MB
//!   free, the spool restarts from empty, which the engine handles as a truncation.
//! - `open_engine` wires it into an engine whose `path` is the pseudo-path `<stdin>`;
//!   the engine owns the stream, so closing the tab deletes the spool.
//!
//! The copier thread is detached: a blocked read cannot be interrupted portably, and at
//! exit the process ends regardless. When the stream is closed the thread stops after the
//! read in progress and closes the input, so the producer gets a broken pipe.

use crate::spool::SpoolFile;
use crate::tail_engine::{TailEngine, WakeFn};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Identity of the standard-input stream: never a real file (`<` is not even valid in a
/// Windows file name), skipped by the workspace, recent files and sessions.
pub const STDIN_PATH: &str = "<stdin>";
/// Title of the stream's tab.
pub const STDIN_TITLE: &str = "stdin";
/// Bytes read from standard input per step.
pub const READ_BYTES: usize = 64 * 1024;
/// Default and bounds of `stdin_spool_max_mb`.
pub const DEFAULT_MAX_MB: u32 = 2048;
pub const MIN_MAX_MB: u32 = 64;
pub const MAX_MAX_MB: u32 = 65536;

/// True for the pseudo-path of the standard-input stream.
pub fn is_stdin_path(path: &Path) -> bool {
    path.as_os_str() == STDIN_PATH
}

/// What standard input is attached to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StdinKind {
    /// A pipe, a socket or a redirected regular file: there is something to read.
    Piped,
    /// A terminal or a console.
    Terminal,
    /// No standard input, or a device such as `/dev/null`.
    Absent,
}

/// Classifies the standard input of this process.
pub fn classify() -> StdinKind {
    #[cfg(unix)]
    {
        use std::os::fd::AsFd;
        classify_fd(std::io::stdin().as_fd())
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        // `Stdin::as_raw_handle` is `GetStdHandle(STD_INPUT_HANDLE)`.
        classify_handle(std::io::stdin().as_raw_handle())
    }
    #[cfg(not(any(unix, windows)))]
    {
        StdinKind::Absent
    }
}

/// `fstat` on a descriptor: FIFO, socket and regular file are piped input, a terminal is
/// a terminal, a closed descriptor and other character devices (`/dev/null`) are none.
#[cfg(unix)]
fn classify_fd(fd: std::os::fd::BorrowedFd<'_>) -> StdinKind {
    use std::io::IsTerminal;
    use std::os::unix::fs::FileTypeExt;
    if fd.is_terminal() {
        return StdinKind::Terminal;
    }
    // Duplicated so the metadata call cannot close the real descriptor.
    let Ok(owned) = fd.try_clone_to_owned() else {
        return StdinKind::Absent;
    };
    let Ok(metadata) = File::from(owned).metadata() else {
        return StdinKind::Absent;
    };
    let kind = metadata.file_type();
    if kind.is_fifo() || kind.is_socket() || kind.is_file() {
        StdinKind::Piped
    } else {
        StdinKind::Absent
    }
}

/// `GetFileType` on a handle: pipe and disk file are piped input, a console (character
/// device) is a terminal, a NULL or invalid handle is none.
#[cfg(windows)]
fn classify_handle(handle: std::os::windows::io::RawHandle) -> StdinKind {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetFileType(file: *mut std::ffi::c_void) -> u32;
    }
    const FILE_TYPE_DISK: u32 = 0x0001;
    const FILE_TYPE_CHAR: u32 = 0x0002;
    const FILE_TYPE_PIPE: u32 = 0x0003;
    const INVALID_HANDLE_VALUE: isize = -1;
    if handle.is_null() || handle as isize == INVALID_HANDLE_VALUE {
        return StdinKind::Absent;
    }
    // SAFETY: plain Win32 query on a handle value; an unknown handle gives
    // FILE_TYPE_UNKNOWN, classified as absent.
    match unsafe { GetFileType(handle) } {
        FILE_TYPE_PIPE | FILE_TYPE_DISK => StdinKind::Piped,
        FILE_TYPE_CHAR => StdinKind::Terminal,
        _ => StdinKind::Absent,
    }
}

/// Takes ownership of the process's standard input as a `File`, reading raw bytes (no
/// console decoding, no buffering). Dropping it closes the input, which is how the
/// producer learns that nobody reads any more. `None` when there is no standard input.
pub fn take_stdin() -> Option<File> {
    #[cfg(unix)]
    {
        use std::os::fd::FromRawFd;
        // SAFETY: descriptor 0 is valid (checked by the caller through `classify`) and
        // nothing else in the process reads standard input: the copier becomes its
        // only owner.
        Some(unsafe { File::from_raw_fd(0) })
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::{AsRawHandle, FromRawHandle};
        let handle = std::io::stdin().as_raw_handle();
        if handle.is_null() || handle as isize == -1 {
            return None;
        }
        // SAFETY: as on Unix; the handle is the standard input handle passed by the
        // shell, owned from here on by the copier.
        Some(unsafe { File::from_raw_handle(handle) })
    }
    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}

/// Bounds of the spool (see the "Standard Input Stream" requirement).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Size at which the spool restarts from empty (`stdin_spool_max_mb`).
    pub max_bytes: u64,
    /// Free space the spool volume must keep.
    pub min_free: u64,
    /// Bytes written between two free-space checks.
    pub check_every: u64,
}

impl Limits {
    pub fn with_cap_mb(mb: u32) -> Self {
        Self {
            max_bytes: mb.clamp(MIN_MAX_MB, MAX_MAX_MB) as u64 * 1024 * 1024,
            min_free: crate::compressed::FREE_SPACE_MARGIN,
            check_every: crate::compressed::FREE_SPACE_CHECK_EVERY,
        }
    }
}

impl Default for Limits {
    fn default() -> Self {
        Self::with_cap_mb(DEFAULT_MAX_MB)
    }
}

/// Where the copy goes and how far it may grow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub spool_dir: PathBuf,
    pub limits: Limits,
}

impl Settings {
    /// Settings from `fasttail.ini` values (`spool_dir`, `stdin_spool_max_mb`).
    pub fn from_config(spool_dir: Option<&Path>, max_mb: u32) -> Self {
        Self {
            spool_dir: crate::spool::spool_dir(spool_dir),
            limits: Limits::with_cap_mb(max_mb),
        }
    }
}

/// State of the input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputState {
    Reading,
    /// The producer closed its end (read returned 0).
    Ended,
    /// Reading or writing the spool failed.
    Failed(String),
}

/// Why the spool was last restarted from empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestartReason {
    /// The spool reached the cap (bytes).
    CapReached(u64),
    /// The spool volume kept less than the margin.
    LowDisk { volume: String },
}

struct Shared {
    /// Bytes in the spool now (back to the chunk size after a restart).
    written: AtomicU64,
    /// Bytes read from the input since the start, restarts included.
    total: AtomicU64,
    /// Number of restarts: the engine rebuilds from byte 0 when it changes.
    generation: AtomicU64,
    state: Mutex<InputState>,
    restart: Mutex<Option<RestartReason>>,
    /// The stream was closed: stop after the read in progress.
    closed: AtomicBool,
}

/// Standard input being copied into a spool. Owned by the engine (`TailEngine::stdin`)
/// once the stream exists, by the app while it waits for the first byte.
pub struct StdinStream {
    shared: Arc<Shared>,
    spool: SpoolFile,
    /// Restart generation the engine has rebuilt for.
    seen_generation: u64,
    /// The end of the input has been seen by the engine (last bytes indexed, encoding
    /// detection settled).
    finalized: bool,
}

impl StdinStream {
    /// Creates the spool in `settings.spool_dir` and starts copying `input` into it.
    pub fn start(
        input: impl Read + Send + 'static,
        settings: &Settings,
        wake: Option<WakeFn>,
    ) -> std::io::Result<Self> {
        let (spool, out) = SpoolFile::create(&settings.spool_dir, "stdin")?;
        let shared = Arc::new(Shared {
            written: AtomicU64::new(0),
            total: AtomicU64::new(0),
            generation: AtomicU64::new(0),
            state: Mutex::new(InputState::Reading),
            restart: Mutex::new(None),
            closed: AtomicBool::new(false),
        });
        let copier = Copier {
            shared: shared.clone(),
            spool: spool.path().to_path_buf(),
            spool_dir: settings.spool_dir.clone(),
            limits: settings.limits,
            wake,
        };
        std::thread::Builder::new()
            .name("fasttail-stdin".to_string())
            .spawn(move || copier.run(input, out))?;
        Ok(Self {
            shared,
            spool,
            seen_generation: 0,
            finalized: false,
        })
    }

    pub fn state(&self) -> InputState {
        self.shared
            .state
            .lock()
            .map(|s| s.clone())
            .unwrap_or(InputState::Failed("poisoned".into()))
    }

    pub fn is_reading(&self) -> bool {
        self.state() == InputState::Reading
    }

    /// Bytes in the spool now.
    pub fn written(&self) -> u64 {
        self.shared.written.load(Ordering::Acquire)
    }

    /// Bytes received since the start, restarts included (0 = nothing arrived yet).
    pub fn received(&self) -> u64 {
        self.shared.total.load(Ordering::Acquire)
    }

    /// Why earlier input was discarded, if it was.
    pub fn restart_reason(&self) -> Option<RestartReason> {
        self.shared.restart.lock().ok().and_then(|r| r.clone())
    }

    pub fn spool_path(&self) -> &Path {
        self.spool.path()
    }

    /// True while the spool holds bytes the engine has not indexed yet.
    pub fn has_unindexed(&self, indexed: u64) -> bool {
        self.written() != indexed || self.pending_restart()
    }

    fn pending_restart(&self) -> bool {
        self.shared.generation.load(Ordering::Acquire) != self.seen_generation
    }
}

impl Drop for StdinStream {
    fn drop(&mut self) {
        // The copier stops after its read in progress and closes the input; the spool
        // (deleted right after) is removed from disk once its write handle is closed.
        self.shared.closed.store(true, Ordering::Release);
    }
}

/// The copier thread's side.
struct Copier {
    shared: Arc<Shared>,
    spool: PathBuf,
    spool_dir: PathBuf,
    limits: Limits,
    wake: Option<WakeFn>,
}

impl Copier {
    fn run(self, mut input: impl Read, out: File) {
        let outcome = self.pump(&mut input, out);
        // Closing the input here, before the wake, so a producer writing again gets a
        // broken pipe even while the window stays open.
        drop(input);
        if let Some(outcome) = outcome {
            if let Ok(mut state) = self.shared.state.lock() {
                *state = outcome;
            }
        }
        self.wake();
    }

    fn wake(&self) {
        if let Some(wake) = &self.wake {
            wake();
        }
    }

    /// Copies until the end of the input; `None` when the stream was closed first.
    fn pump(&self, input: &mut impl Read, mut out: File) -> Option<InputState> {
        let shared = &self.shared;
        let mut buf = vec![0u8; READ_BYTES];
        let mut written: u64 = 0;
        let mut next_check = self.limits.check_every;
        loop {
            let n = match input.read(&mut buf) {
                Ok(n) => n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                // Windows reports the writer's end closing as a broken pipe.
                Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => 0,
                Err(e) => return Some(InputState::Failed(e.to_string())),
            };
            if shared.closed.load(Ordering::Acquire) {
                return None;
            }
            if n == 0 {
                return Some(InputState::Ended);
            }
            let low_disk = if written >= next_check {
                next_check = written + self.limits.check_every;
                crate::compressed::free_space(&self.spool_dir)
                    .filter(|(free, _)| *free < self.limits.min_free)
                    .map(|(_, volume)| volume)
            } else {
                None
            };
            let reason = match low_disk {
                Some(volume) => Some(RestartReason::LowDisk { volume }),
                None if written + n as u64 > self.limits.max_bytes => {
                    Some(RestartReason::CapReached(self.limits.max_bytes))
                }
                None => None,
            };
            if let Some(reason) = reason {
                // Start over from empty: the engine sees the size drop (and the new
                // generation) and rebuilds from byte 0.
                out = match restart(&self.spool) {
                    Ok(file) => file,
                    Err(e) => return Some(InputState::Failed(e.to_string())),
                };
                written = 0;
                next_check = self.limits.check_every;
                if let Ok(mut r) = shared.restart.lock() {
                    *r = Some(reason);
                }
                shared.written.store(0, Ordering::Release);
                shared.generation.fetch_add(1, Ordering::AcqRel);
            }
            if let Err(e) = out.write_all(&buf[..n]).and_then(|_| out.flush()) {
                return Some(InputState::Failed(e.to_string()));
            }
            written += n as u64;
            shared.written.store(written, Ordering::Release);
            shared.total.fetch_add(n as u64, Ordering::AcqRel);
            self.wake();
        }
    }
}

/// Empties the spool at `path` and returns a new write handle at offset 0.
fn restart(path: &Path) -> std::io::Result<File> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(7); // FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE
    }
    let file = options.open(path)?;
    file.set_len(0)?;
    Ok(file)
}

/// Opens the engine of the standard-input stream on `stream`'s spool: `path` is
/// `<stdin>`, follow is on, and the engine owns the stream.
pub fn open_engine(stream: StdinStream, wake: Option<WakeFn>) -> std::io::Result<TailEngine> {
    let mut engine = match wake {
        Some(wake) => TailEngine::open_with_wake(stream.spool_path(), wake)?,
        None => TailEngine::open(stream.spool_path())?,
    };
    engine.path = PathBuf::from(STDIN_PATH);
    engine.follow_tail = true;
    engine.stdin = Some(stream);
    Ok(engine)
}

impl TailEngine {
    /// True for the standard-input stream.
    pub fn is_stdin(&self) -> bool {
        self.stdin.is_some()
    }

    /// Called from `poll_updates` before the file is checked: after a spool restart the
    /// index is rebuilt from byte 0, even when the spool has already grown back past the
    /// indexed size (an append check would take it for more of the old content).
    pub(crate) fn poll_stdin_restart(&mut self) {
        let Some(s) = self.stdin.as_mut() else {
            return;
        };
        let generation = s.shared.generation.load(Ordering::Acquire);
        if generation != s.seen_generation {
            s.seen_generation = generation;
            self.restart_from_empty();
        }
    }

    /// Called from `poll_updates` after the file is checked: once the input ended and
    /// its last bytes are indexed, settles the encoding detection of an input that
    /// stayed under the sample size.
    pub(crate) fn poll_stdin_end(&mut self) {
        let file_size = self.file_size;
        let Some(s) = self.stdin.as_mut() else {
            return;
        };
        if !s.finalized && !s.is_reading() && s.written() == file_size {
            s.finalized = true;
            self.finish_encoding_detection();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    /// A producer fed through a channel: each message is one write, the channel closing
    /// is the end of input.
    struct FakeProducer(mpsc::Receiver<Vec<u8>>, Vec<u8>);

    impl Read for FakeProducer {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.1.is_empty() {
                match self.0.recv() {
                    Ok(data) => self.1 = data,
                    Err(_) => return Ok(0),
                }
            }
            let n = self.1.len().min(buf.len());
            buf[..n].copy_from_slice(&self.1[..n]);
            self.1.drain(..n);
            Ok(n)
        }
    }

    fn producer() -> (mpsc::Sender<Vec<u8>>, FakeProducer) {
        let (tx, rx) = mpsc::channel();
        (tx, FakeProducer(rx, Vec::new()))
    }

    fn settings(dir: &Path, limits: Limits) -> Settings {
        Settings {
            spool_dir: dir.join("spool"),
            limits,
        }
    }

    fn wait_for(what: &str, mut cond: impl FnMut() -> bool) {
        let start = Instant::now();
        while !cond() {
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "timed out waiting for {what}"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn poll_until(engine: &mut TailEngine, what: &str, mut cond: impl FnMut(&TailEngine) -> bool) {
        engine.size_check_interval = Duration::ZERO;
        let start = Instant::now();
        loop {
            engine.poll_updates();
            if cond(engine) {
                return;
            }
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "timed out waiting for {what}"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn pseudo_path_is_recognised() {
        assert!(is_stdin_path(Path::new(STDIN_PATH)));
        assert!(!is_stdin_path(Path::new("stdin")));
        assert!(!is_stdin_path(&std::env::temp_dir().join(STDIN_PATH)));
    }

    #[test]
    fn limits_are_clamped() {
        assert_eq!(Limits::default().max_bytes, 2048 * 1024 * 1024);
        assert_eq!(Limits::with_cap_mb(1).max_bytes, 64 * 1024 * 1024);
        assert_eq!(Limits::with_cap_mb(u32::MAX).max_bytes, 65536 * 1024 * 1024);
    }

    #[cfg(unix)]
    #[test]
    fn classification_of_pipe_file_and_dev_null() {
        use std::os::fd::AsFd;
        let (reader, _writer) = std::io::pipe().unwrap();
        assert_eq!(classify_fd(reader.as_fd()), StdinKind::Piped);
        let file = tempfile::tempfile().unwrap();
        assert_eq!(classify_fd(file.as_fd()), StdinKind::Piped);
        let null = File::open("/dev/null").unwrap();
        assert_eq!(classify_fd(null.as_fd()), StdinKind::Absent);
    }

    #[cfg(windows)]
    #[test]
    fn classification_of_pipe_file_and_nul() {
        use std::os::windows::io::AsRawHandle;
        let (reader, _writer) = std::io::pipe().unwrap();
        assert_eq!(classify_handle(reader.as_raw_handle()), StdinKind::Piped);
        let file = tempfile::tempfile().unwrap();
        assert_eq!(classify_handle(file.as_raw_handle()), StdinKind::Piped);
        // NUL is a character device, as a console is.
        let nul = std::fs::OpenOptions::new().read(true).open("NUL").unwrap();
        assert_eq!(classify_handle(nul.as_raw_handle()), StdinKind::Terminal);
        assert_eq!(classify_handle(std::ptr::null_mut()), StdinKind::Absent);
        assert_eq!(classify_handle(-1isize as _), StdinKind::Absent);
    }

    #[test]
    fn copies_a_producer_and_reports_the_end() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, input) = producer();
        let stream =
            StdinStream::start(input, &settings(dir.path(), Limits::default()), None).unwrap();
        let spool = stream.spool_path().to_path_buf();
        assert!(spool.is_file());
        assert_eq!(stream.received(), 0);
        let mut engine = open_engine(stream, None).unwrap();
        assert!(engine.is_stdin());
        assert!(is_stdin_path(&engine.path));
        assert!(engine.follow_tail);

        tx.send(b"first line\nsecond ".to_vec()).unwrap();
        poll_until(&mut engine, "the first write", |e| e.total_lines() >= 1);
        // A partial last line is completed by the next write.
        tx.send(b"line\nthird line\n".to_vec()).unwrap();
        poll_until(&mut engine, "the second write", |e| e.total_lines() >= 3);
        assert_eq!(engine.get_line(1).unwrap(), "second line");
        assert!(engine.stdin.as_ref().unwrap().is_reading());

        drop(tx);
        poll_until(&mut engine, "the end of input", |e| {
            e.stdin.as_ref().unwrap().state() == InputState::Ended
        });
        assert_eq!(engine.total_lines(), 3);
        assert_eq!(engine.get_line(2).unwrap(), "third line");
        // The stream stays open on its spool after the end.
        assert!(spool.is_file());
        drop(engine);
        assert!(!spool.exists());
    }

    #[test]
    fn a_short_input_settles_the_encoding_at_the_end() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, input) = producer();
        let stream =
            StdinStream::start(input, &settings(dir.path(), Limits::default()), None).unwrap();
        let mut engine = open_engine(stream, None).unwrap();
        assert!(engine.encoding_pending);
        tx.send(b"only\n".to_vec()).unwrap();
        drop(tx);
        poll_until(&mut engine, "the end of input", |e| {
            e.stdin.as_ref().unwrap().finalized
        });
        assert!(!engine.encoding_pending);
        assert_eq!(engine.total_lines(), 1);
    }

    #[test]
    fn read_error_is_reported() {
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("device gone"))
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let stream =
            StdinStream::start(Broken, &settings(dir.path(), Limits::default()), None).unwrap();
        wait_for("the error", || !stream.is_reading());
        assert!(matches!(stream.state(), InputState::Failed(m) if m.contains("device gone")));
    }

    #[test]
    fn spool_restart_at_the_cap_is_a_truncation_for_the_engine() {
        let dir = tempfile::tempdir().unwrap();
        let limits = Limits {
            max_bytes: 90,
            min_free: 0,
            check_every: u64::MAX,
        };
        let (tx, input) = producer();
        let stream = StdinStream::start(input, &settings(dir.path(), limits), None).unwrap();
        let mut engine = open_engine(stream, None).unwrap();
        // 6 lines of 14 bytes: 84 bytes, under the cap.
        for i in 0..6 {
            tx.send(format!("old line {i:04}\n").into_bytes()).unwrap();
        }
        poll_until(&mut engine, "the old lines", |e| e.total_lines() == 6);
        engine.set_bookmarks(vec![1, 4]);
        assert!(engine.stdin.as_ref().unwrap().restart_reason().is_none());

        // The next write would pass 90 bytes: the spool starts over with it.
        tx.send(b"new line 0000\n".to_vec()).unwrap();
        poll_until(&mut engine, "the restart", |e| {
            e.stdin.as_ref().unwrap().restart_reason().is_some() && e.total_lines() == 1
        });
        assert_eq!(engine.get_line(0).unwrap(), "new line 0000");
        assert!(engine.bookmarks.is_empty());
        assert_eq!(
            engine.stdin.as_ref().unwrap().restart_reason(),
            Some(RestartReason::CapReached(90))
        );
        assert_eq!(engine.stdin.as_ref().unwrap().received(), 84 + 14);
        tx.send(b"new line 0001\n".to_vec()).unwrap();
        poll_until(&mut engine, "lines after the restart", |e| {
            e.total_lines() == 2
        });
    }

    #[test]
    fn restart_regrowing_past_the_old_size_before_a_poll_still_rebuilds() {
        let dir = tempfile::tempdir().unwrap();
        let limits = Limits {
            max_bytes: 40,
            min_free: 0,
            check_every: u64::MAX,
        };
        let (tx, input) = producer();
        let stream = StdinStream::start(input, &settings(dir.path(), limits), None).unwrap();
        let mut engine = open_engine(stream, None).unwrap();
        tx.send(b"aaaaaaaaa\n".to_vec()).unwrap();
        poll_until(&mut engine, "the first line", |e| e.total_lines() == 1);
        // Without polling: the cap restarts the spool, which grows past the 10 bytes
        // the engine indexed.
        tx.send(b"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\n".to_vec())
            .unwrap();
        tx.send(b"cccccccccccccccccccccccc\n".to_vec()).unwrap();
        let s = engine.stdin.as_ref().unwrap();
        wait_for("the restart", || s.written() == 25);
        poll_until(&mut engine, "the rebuild", |e| e.total_lines() == 1);
        assert_eq!(engine.get_line(0).unwrap(), "c".repeat(24));
    }

    #[test]
    fn low_disk_restarts_the_spool() {
        let dir = tempfile::tempdir().unwrap();
        // No volume keeps u64::MAX free bytes: the first check restarts the spool.
        let limits = Limits {
            max_bytes: u64::MAX,
            min_free: u64::MAX,
            check_every: 10,
        };
        let (tx, input) = producer();
        let stream = StdinStream::start(input, &settings(dir.path(), limits), None).unwrap();
        tx.send(b"0123456789\n".to_vec()).unwrap();
        tx.send(b"abc\n".to_vec()).unwrap();
        wait_for("the restart", || stream.restart_reason().is_some());
        if crate::compressed::free_space(&dir.path().join("spool")).is_some() {
            assert!(matches!(
                stream.restart_reason(),
                Some(RestartReason::LowDisk { .. })
            ));
            wait_for("the second write", || stream.written() == 4);
            assert_eq!(std::fs::read(stream.spool_path()).unwrap(), b"abc\n");
        }
    }

    #[test]
    fn closing_the_stream_stops_the_copier_and_deletes_the_spool() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, input) = producer();
        let stream =
            StdinStream::start(input, &settings(dir.path(), Limits::default()), None).unwrap();
        let spool = stream.spool_path().to_path_buf();
        tx.send(b"x\n".to_vec()).unwrap();
        wait_for("the first write", || stream.written() == 2);
        let shared = stream.shared.clone();
        drop(stream);
        // The next read returns, the copier sees the stream closed and stops writing.
        tx.send(b"y\n".to_vec()).unwrap();
        wait_for("the copier to stop", || Arc::strong_count(&shared) == 1);
        assert_eq!(shared.written.load(Ordering::Acquire), 2);
        assert!(!spool.exists());
    }

    #[test]
    fn fifty_megabytes_per_second_keep_up() {
        // A synthetic producer of 50 MB in 64 KB writes: copying it must take well under
        // a second even in a debug build, so a 50 MB/s producer is never held back.
        let dir = tempfile::tempdir().unwrap();
        let (tx, input) = producer();
        let stream =
            StdinStream::start(input, &settings(dir.path(), Limits::default()), None).unwrap();
        let line = b"2026-09-25T10:00:00Z INFO synthetic producer line with some payload\n";
        let chunk: Vec<u8> = line.iter().copied().cycle().take(READ_BYTES).collect();
        let total = 50 * 1024 * 1024 / READ_BYTES;
        let start = Instant::now();
        for _ in 0..total {
            tx.send(chunk.clone()).unwrap();
        }
        drop(tx);
        wait_for("the end of input", || !stream.is_reading());
        let elapsed = start.elapsed();
        assert_eq!(stream.written(), (total * READ_BYTES) as u64);
        assert!(
            elapsed < Duration::from_secs(5),
            "50 MB copied in {elapsed:?}"
        );
    }
}
