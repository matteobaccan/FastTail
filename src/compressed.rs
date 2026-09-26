//! Compressed log input: gzip files and zip entries are decompressed on a background
//! thread into a spool file (see `spool`) that the tail engine opens like any other log,
//! so every feature works on the result and nothing decompressed is held in memory.
//!
//! - `sniff` recognises the format by its magic bytes, whatever the extension.
//! - `list_zip_entries` reads the central directory once, for the entry picker.
//! - `DecompressJob` inflates into the spool in 1 MB chunks (flushed, so the engine's
//!   normal poll indexes them as they land), reports progress by compressed bytes
//!   consumed, stops on cancel, at the output cap, when the spool volume runs low, or on a
//!   tar archive inside a gzip file. Whatever was written before a stop stays readable.
//! - `open_engine` wires it together: an engine on the empty spool whose `path` is the
//!   stream identity (the archive, or `archive/entry` for a zip entry) and whose
//!   `compressed` field owns the job and the spool, so closing the stream stops the job
//!   and deletes the spool.

use crate::spool::SpoolFile;
use crate::tail_engine::{TailEngine, WakeFn};
use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

/// Bytes inflated and written per step: the first screen appears after the first one.
pub const CHUNK_BYTES: usize = 1024 * 1024;
/// Free space the spool volume keeps after an extraction.
pub const FREE_SPACE_MARGIN: u64 = 512 * 1024 * 1024;
/// Output written between two free-space checks.
pub const FREE_SPACE_CHECK_EVERY: u64 = 64 * 1024 * 1024;
/// Default and bounds of `compressed_max_gb` (output cap, in GB).
pub const DEFAULT_MAX_GB: u32 = 20;
pub const MIN_MAX_GB: u32 = 1;
pub const MAX_MAX_GB: u32 = 1024;
/// Offset and magic of a tar header (`ustar`), checked on the decompressed data.
const TAR_MAGIC_OFFSET: usize = 257;
const TAR_MAGIC: &[u8] = b"ustar";

/// Container format of a file, from its first bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Gzip,
    Zip,
    /// A zip holding nothing (end-of-central-directory record only).
    EmptyZip,
    /// Anything else: opened as a plain file.
    Plain,
}

/// Format of the data starting with `head`.
pub fn sniff_bytes(head: &[u8]) -> Format {
    if head.starts_with(&[0x1f, 0x8b]) {
        Format::Gzip
    } else if head.starts_with(b"PK\x03\x04") {
        Format::Zip
    } else if head.starts_with(b"PK\x05\x06") {
        Format::EmptyZip
    } else {
        Format::Plain
    }
}

/// Format of the file at `path` (`Plain` when it cannot be read).
pub fn sniff(path: &Path) -> Format {
    let Ok(mut file) = crate::file_source::open_file_shared(path) else {
        return Format::Plain;
    };
    let mut head = [0u8; 4];
    let mut filled = 0;
    while filled < head.len() {
        match file.read(&mut head[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Format::Plain,
        }
    }
    sniff_bytes(&head[..filled])
}

/// Why a zip entry cannot be opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryRefusal {
    Encrypted,
    /// Compression method other than stored or deflate (its name).
    Method(String),
    /// A name that climbs out of the archive (`../x`) or is absolute.
    UnsafeName,
    /// Another entry earlier in the archive has the same stream path (`a/b.log` and
    /// `a\b.log`, or names differing only by case on Windows): only the first opens.
    DuplicateName,
}

/// One file entry of a zip archive, as listed by the entry picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZipEntryInfo {
    pub name: String,
    pub size: u64,
    pub compressed_size: u64,
    /// Set when the entry cannot be opened; the picker shows it disabled with the reason.
    pub refusal: Option<EntryRefusal>,
}

/// The file entries of the zip at `path` (directories skipped), in archive order.
pub fn list_zip_entries(path: &Path) -> std::io::Result<Vec<ZipEntryInfo>> {
    let file = crate::file_source::open_file_shared(path)?;
    let mut archive = zip::ZipArchive::new(BufReader::new(file)).map_err(zip_err)?;
    let mut out = Vec::with_capacity(archive.len());
    // Stream paths of the entries that open: a later entry with the same one is refused,
    // so each tab reads one entry.
    let mut opened_keys = std::collections::HashSet::new();
    for i in 0..archive.len() {
        // Raw access reads the metadata without decrypting or decompressing anything.
        let entry = archive.by_index_raw(i).map_err(zip_err)?;
        if entry.is_dir() {
            continue;
        }
        let method = entry.compression();
        let refusal = if entry.encrypted() {
            Some(EntryRefusal::Encrypted)
        } else if !matches!(
            method,
            zip::CompressionMethod::Stored | zip::CompressionMethod::Deflated
        ) {
            Some(EntryRefusal::Method(format!("{method:?}")))
        } else if entry.enclosed_name().is_none() {
            Some(EntryRefusal::UnsafeName)
        } else if !opened_keys.insert(entry_key(entry.name())) {
            Some(EntryRefusal::DuplicateName)
        } else {
            None
        };
        out.push(ZipEntryInfo {
            name: entry.name().to_string(),
            size: entry.size(),
            compressed_size: entry.compressed_size(),
            refusal,
        });
    }
    Ok(out)
}

fn zip_err(e: zip::result::ZipError) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
}

/// The parts of a zip entry name: split on `/` and on the `\` some Windows tools write,
/// without the empty and `.` parts a path would drop anyway.
fn entry_parts(entry: &str) -> impl Iterator<Item = &str> {
    entry
        .split(['/', '\\'])
        .filter(|p| !p.is_empty() && *p != ".")
}

/// The name a zip entry is known by in FastTail (its stream identity and the value
/// sessions save): `dir\./file.log` is `dir/file.log`.
pub fn normalize_entry(entry: &str) -> String {
    entry_parts(entry).collect::<Vec<_>>().join("/")
}

/// Key of the stream path of an entry: two entries with the same key would be one tab.
/// Paths compare without case on Windows (see `paths::paths_equal_fast`).
fn entry_key(entry: &str) -> String {
    let name = normalize_entry(entry);
    if cfg!(windows) {
        name.to_ascii_lowercase()
    } else {
        name
    }
}

/// Stream identity of the zip entry `entry` of `archive`: `archive/entry`, one path
/// component per part of the name. It names the stream in tabs, the workspace and the
/// bookmarks; nothing is ever written at that path.
pub fn entry_path(archive: &Path, entry: &str) -> PathBuf {
    let mut path = archive.to_path_buf();
    for part in entry_parts(entry) {
        path.push(part);
    }
    path
}

/// The archive of the entry path `path` holding `entry` (the inverse of `entry_path`).
pub fn archive_of(path: &Path, entry: &str) -> PathBuf {
    let parts = entry_parts(entry).count();
    let mut archive = path.to_path_buf();
    for _ in 0..parts {
        archive.pop();
    }
    archive
}

/// Splits a zip entry path into the archive and the entry name: the nearest ancestor of
/// `path` that is a regular file must be a zip. `None` for any other path.
pub fn split_entry_path(path: &Path) -> Option<(PathBuf, String)> {
    if path.is_file() {
        return None;
    }
    let archive = path.ancestors().skip(1).find(|a| a.is_file())?;
    if sniff(archive) != Format::Zip {
        return None;
    }
    let rel = path.strip_prefix(archive).ok()?;
    let entry = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/");
    if entry.is_empty() {
        return None;
    }
    Some((archive.to_path_buf(), entry))
}

/// True when `path` is an existing file or the entry path of an existing zip.
pub fn source_exists(path: &Path) -> bool {
    path.is_file() || split_entry_path(path).is_some()
}

/// What opening `path` means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A plain file (or anything the engine opens as it is).
    Plain,
    Gzip,
    /// A zip archive: its entries still have to be chosen.
    ZipArchive,
    EmptyZip,
    /// One entry of a zip archive.
    ZipEntry {
        archive: PathBuf,
        entry: String,
    },
}

/// Classifies `path` by its content (for a file) or as a zip entry path.
pub fn classify(path: &Path) -> Target {
    if path.is_file() {
        return match sniff(path) {
            Format::Gzip => Target::Gzip,
            // A text file that merely starts with `PK\x03\x04` is not a zip: when the
            // central directory does not parse, the file opens as it is.
            Format::Zip if list_zip_entries(path).is_ok() => Target::ZipArchive,
            Format::Zip => Target::Plain,
            Format::EmptyZip => Target::EmptyZip,
            Format::Plain => Target::Plain,
        };
    }
    match split_entry_path(path) {
        Some((archive, entry)) => Target::ZipEntry { archive, entry },
        None => Target::Plain,
    }
}

/// Bounds of one extraction (see the "Decompression Space Guard" requirement).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Output cap in bytes (`compressed_max_gb`).
    pub max_output: u64,
    /// Free space the spool volume must keep.
    pub min_free: u64,
    /// Output written between two free-space checks.
    pub check_every: u64,
}

impl Limits {
    pub fn with_cap_gb(gb: u32) -> Self {
        Self {
            max_output: gb.clamp(MIN_MAX_GB, MAX_MAX_GB) as u64 * 1024 * 1024 * 1024,
            min_free: FREE_SPACE_MARGIN,
            check_every: FREE_SPACE_CHECK_EVERY,
        }
    }
}

impl Default for Limits {
    fn default() -> Self {
        Self::with_cap_gb(DEFAULT_MAX_GB)
    }
}

/// Where spools go and how far an extraction may run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub spool_dir: PathBuf,
    pub limits: Limits,
}

impl Settings {
    /// Settings from `fasttail.ini` values (`spool_dir`, `compressed_max_gb`).
    pub fn from_config(spool_dir: Option<&Path>, max_gb: u32) -> Self {
        Self {
            spool_dir: crate::spool::spool_dir(spool_dir),
            limits: Limits::with_cap_gb(max_gb),
        }
    }
}

/// Free bytes and mount point of the volume holding `dir` (the disk whose mount point is
/// the longest prefix of the directory). `None` when it cannot be told.
pub fn free_space(dir: &Path) -> Option<(u64, String)> {
    let dir = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    let text = dir.to_string_lossy().to_string();
    // `canonicalize` gives `\\?\C:\...` on Windows; mount points are `C:\`.
    let text = text.strip_prefix(r"\\?\").unwrap_or(&text).to_string();
    let disks = sysinfo::Disks::new_with_refreshed_list();
    disks
        .list()
        .iter()
        .filter_map(|d| {
            let mount = d.mount_point().to_string_lossy().to_string();
            let matches = if cfg!(windows) {
                text.to_ascii_lowercase()
                    .starts_with(&mount.to_ascii_lowercase())
            } else {
                Path::new(&text).starts_with(&mount)
            };
            matches.then(|| (mount.len(), d.available_space(), mount))
        })
        .max_by_key(|(len, _, _)| *len)
        .map(|(_, free, mount)| (free, mount))
}

/// Why an extraction stopped before the end of the data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    Cancelled,
    /// The output reached the cap (bytes).
    CapReached(u64),
    /// The spool volume would keep less than the margin.
    DiskFull {
        volume: String,
    },
    /// The gzip holds a tar archive.
    Tar,
    /// Corrupt data or an I/O error.
    Failed(String),
}

/// State of a decompression job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobState {
    Running,
    Done,
    Stopped(StopReason),
}

/// What a job decompresses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobSource {
    Gzip(PathBuf),
    ZipEntry { archive: PathBuf, entry: String },
}

struct Shared {
    /// Compressed bytes consumed and their total, for the progress.
    consumed: AtomicU64,
    total: AtomicU64,
    /// Decompressed bytes written (and flushed) to the spool.
    written: AtomicU64,
    state: Mutex<JobState>,
}

/// A decompression running on its own thread. Dropping it cancels and joins the thread,
/// so the spool's write handle is closed before the spool is deleted.
pub struct DecompressJob {
    shared: Arc<Shared>,
    cancel: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl DecompressJob {
    /// Starts decompressing `source` into `out`, a spool in `spool_dir` (whose volume the
    /// free-space check watches). `wake` is called after every chunk and at the end.
    pub fn start(
        source: JobSource,
        out: File,
        spool_dir: PathBuf,
        limits: Limits,
        wake: Option<WakeFn>,
    ) -> Self {
        let shared = Arc::new(Shared {
            consumed: AtomicU64::new(0),
            total: AtomicU64::new(0),
            written: AtomicU64::new(0),
            state: Mutex::new(JobState::Running),
        });
        let cancel = Arc::new(AtomicBool::new(false));
        let (thread_shared, thread_cancel) = (shared.clone(), cancel.clone());
        let handle = std::thread::Builder::new()
            .name("fasttail-decompress".to_string())
            .spawn(move || {
                let job = JobEnv {
                    spool_dir: &spool_dir,
                    limits: &limits,
                    shared: &thread_shared,
                    cancel: &thread_cancel,
                    wake: &wake,
                };
                let outcome = run_job(&source, out, &job);
                if let Ok(mut state) = thread_shared.state.lock() {
                    *state = outcome;
                }
                if let Some(wake) = &wake {
                    wake();
                }
            })
            .ok();
        if handle.is_none() {
            if let Ok(mut state) = shared.state.lock() {
                *state = JobState::Stopped(StopReason::Failed("cannot start thread".into()));
            }
        }
        Self {
            shared,
            cancel,
            handle,
        }
    }

    /// A job that has nothing to do (a placeholder while the real one is set up).
    fn idle() -> Self {
        Self {
            shared: Arc::new(Shared {
                consumed: AtomicU64::new(0),
                total: AtomicU64::new(0),
                written: AtomicU64::new(0),
                state: Mutex::new(JobState::Done),
            }),
            cancel: Arc::new(AtomicBool::new(false)),
            handle: None,
        }
    }

    pub fn state(&self) -> JobState {
        self.shared
            .state
            .lock()
            .map(|s| s.clone())
            .unwrap_or(JobState::Stopped(StopReason::Failed("poisoned".into())))
    }

    pub fn is_running(&self) -> bool {
        self.state() == JobState::Running
    }

    /// Share of the compressed input consumed, 0.0 to 1.0.
    pub fn progress(&self) -> f32 {
        let total = self.shared.total.load(Ordering::Relaxed);
        if total == 0 {
            return 0.0;
        }
        (self.shared.consumed.load(Ordering::Relaxed) as f64 / total as f64).clamp(0.0, 1.0) as f32
    }

    /// Decompressed bytes written to the spool so far.
    pub fn written(&self) -> u64 {
        self.shared.written.load(Ordering::Acquire)
    }

    /// Asks the job to stop after the chunk in progress.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// Waits for the thread to end (tests, and before the spool is rewritten).
    pub fn wait(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for DecompressJob {
    fn drop(&mut self) {
        self.cancel();
        self.wait();
    }
}

/// Counts the bytes read through it into `counter`.
struct CountingReader<R> {
    inner: R,
    counter: Arc<Shared>,
}

impl<R: Read> Read for CountingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.counter.consumed.fetch_add(n as u64, Ordering::Relaxed);
        Ok(n)
    }
}

/// What the job thread works with besides the input and the output.
struct JobEnv<'a> {
    spool_dir: &'a Path,
    limits: &'a Limits,
    shared: &'a Arc<Shared>,
    cancel: &'a AtomicBool,
    wake: &'a Option<WakeFn>,
}

fn run_job(source: &JobSource, out: File, env: &JobEnv) -> JobState {
    let failed = |e: &dyn std::fmt::Display| JobState::Stopped(StopReason::Failed(e.to_string()));
    let shared = env.shared;
    match source {
        JobSource::Gzip(path) => {
            let file = match crate::file_source::open_file_shared(path) {
                Ok(f) => f,
                Err(e) => return failed(&e),
            };
            let total = file.metadata().map(|m| m.len()).unwrap_or(0);
            shared.total.store(total, Ordering::Relaxed);
            let counted = CountingReader {
                inner: file,
                counter: shared.clone(),
            };
            // Concatenated members (`cat a.gz b.gz`, some rotators) are one stream.
            let decoder = flate2::read::MultiGzDecoder::new(BufReader::new(counted));
            pump(decoder, out, env, true)
        }
        JobSource::ZipEntry { archive, entry } => {
            let file = match crate::file_source::open_file_shared(archive) {
                Ok(f) => f,
                Err(e) => return failed(&e),
            };
            let mut zip = match zip::ZipArchive::new(BufReader::new(file)) {
                Ok(z) => z,
                Err(e) => return failed(&e),
            };
            let reader = match zip.by_name(entry) {
                Ok(r) => r,
                Err(e) => return failed(&e),
            };
            shared
                .total
                .store(reader.compressed_size().max(1), Ordering::Relaxed);
            // The entry reader pulls the compressed bytes: count what it consumes by the
            // decompressed side instead, scaled to the compressed size.
            let size = reader.size();
            let compressed = reader.compressed_size();
            let scaled = ScaledProgress {
                inner: reader,
                size,
                compressed,
                read: 0,
                counter: shared.clone(),
            };
            pump(scaled, out, env, false)
        }
    }
}

/// Progress of a zip entry: decompressed bytes read, scaled to the compressed size (the
/// zip reader does not expose how much of the compressed data it consumed).
struct ScaledProgress<R> {
    inner: R,
    size: u64,
    compressed: u64,
    read: u64,
    counter: Arc<Shared>,
}

impl<R: Read> Read for ScaledProgress<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.read += n as u64;
        let consumed = if self.size == 0 {
            self.compressed
        } else {
            (self.read as u128 * self.compressed as u128 / self.size as u128) as u64
        };
        self.counter.consumed.store(consumed, Ordering::Relaxed);
        Ok(n)
    }
}

/// Fills `buf` from `reader` as far as it goes; returns the count (0 at the end).
fn read_full(reader: &mut impl Read, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(filled)
}

/// Copies `reader` into `out` chunk by chunk, enforcing the limits.
fn pump(mut reader: impl Read, mut out: File, env: &JobEnv, detect_tar: bool) -> JobState {
    let JobEnv {
        spool_dir,
        limits,
        shared,
        cancel,
        wake,
    } = env;
    let mut buf = vec![0u8; CHUNK_BYTES];
    let mut written: u64 = 0;
    let mut next_check = limits.check_every;
    let mut first = true;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return JobState::Stopped(StopReason::Cancelled);
        }
        let n = match read_full(&mut reader, &mut buf) {
            Ok(n) => n,
            Err(e) => return JobState::Stopped(StopReason::Failed(e.to_string())),
        };
        if n == 0 {
            return JobState::Done;
        }
        if first {
            first = false;
            if detect_tar
                && n >= TAR_MAGIC_OFFSET + TAR_MAGIC.len()
                && &buf[TAR_MAGIC_OFFSET..TAR_MAGIC_OFFSET + TAR_MAGIC.len()] == TAR_MAGIC
            {
                return JobState::Stopped(StopReason::Tar);
            }
        }
        let room = limits.max_output.saturating_sub(written);
        let take = (n as u64).min(room) as usize;
        if let Err(e) = out.write_all(&buf[..take]).and_then(|_| out.flush()) {
            return JobState::Stopped(StopReason::Failed(e.to_string()));
        }
        written += take as u64;
        shared.written.store(written, Ordering::Release);
        if let Some(wake) = wake {
            wake();
        }
        if take < n {
            return JobState::Stopped(StopReason::CapReached(limits.max_output));
        }
        if written >= next_check {
            next_check = written + limits.check_every;
            if let Some(volume) = low_on_space(spool_dir, limits) {
                return JobState::Stopped(StopReason::DiskFull { volume });
            }
        }
    }
}

/// The volume of the spool when it keeps less free space than the margin.
fn low_on_space(spool_dir: &Path, limits: &Limits) -> Option<String> {
    let (free, volume) = free_space(spool_dir)?;
    (free < limits.min_free).then_some(volume)
}

/// A stream decompressed into a spool: owned by the engine (`TailEngine::compressed`).
/// Fields drop in order: the job (cancelled and joined) before the spool (deleted).
pub struct CompressedStream {
    /// The archive on disk and, for a zip, the entry: its `/`-separated identity
    /// (`normalize_entry`), the name `path` and saved sessions know it by.
    pub archive: PathBuf,
    pub entry: Option<String>,
    /// The entry name as the archive spells it, which the job looks up.
    pub real_entry: Option<String>,
    pub settings: Settings,
    job: DecompressJob,
    spool: SpoolFile,
    wake: Option<WakeFn>,
    /// Bookmarks to restore once the index covers them (a restored stream starts empty).
    pub pending_bookmarks: Vec<usize>,
    /// The end of the job has been seen by the engine: final size indexed, encoding
    /// detection settled, pending bookmarks applied.
    finalized: bool,
}

impl CompressedStream {
    /// `entry`: for a zip, the entry's identity and its name as the archive spells it.
    fn start(
        archive: &Path,
        entry: Option<(String, String)>,
        settings: &Settings,
        spool: SpoolFile,
        out: File,
        wake: Option<WakeFn>,
    ) -> Self {
        let (entry, real_entry) = entry.unzip();
        let mut stream = Self {
            archive: archive.to_path_buf(),
            entry,
            real_entry,
            settings: settings.clone(),
            job: DecompressJob::idle(),
            spool,
            wake,
            pending_bookmarks: Vec::new(),
            finalized: false,
        };
        stream.job = stream.new_job(out);
        stream
    }

    fn new_job(&self, out: File) -> DecompressJob {
        let source = match &self.real_entry {
            Some(entry) => JobSource::ZipEntry {
                archive: self.archive.clone(),
                entry: entry.clone(),
            },
            None => JobSource::Gzip(self.archive.clone()),
        };
        DecompressJob::start(
            source,
            out,
            self.settings.spool_dir.clone(),
            self.settings.limits,
            self.wake.clone(),
        )
    }

    /// Stops the job, empties the spool and extracts again from the start ("Reload").
    pub fn restart(&mut self) -> std::io::Result<()> {
        self.job.cancel();
        self.job.wait();
        let out = self.spool.rewrite()?;
        self.job = self.new_job(out);
        self.finalized = false;
        Ok(())
    }

    pub fn state(&self) -> JobState {
        self.job.state()
    }

    pub fn progress(&self) -> f32 {
        self.job.progress()
    }

    pub fn is_running(&self) -> bool {
        self.job.is_running()
    }

    /// Stops the extraction; what was spooled stays readable.
    pub fn cancel(&self) {
        self.job.cancel();
    }

    pub fn spool_path(&self) -> &Path {
        self.spool.path()
    }

    /// Decompressed bytes in the spool.
    pub fn written(&self) -> u64 {
        self.job.written()
    }

    /// True while the spool holds bytes the engine has not indexed yet.
    pub fn has_unindexed(&self, indexed: u64) -> bool {
        self.job.written() != indexed
    }

    /// True once the job ended and the engine indexed everything it wrote.
    pub fn is_finalized(&self) -> bool {
        self.finalized
    }

    /// Title of the stream: `app.log.1.gz`, `bundle.zip › server.log`.
    pub fn title(&self) -> String {
        let archive = self
            .archive
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        match &self.entry {
            Some(entry) => format!("{archive} › {entry}"),
            None => archive,
        }
    }
}

/// Why a compressed stream could not be opened.
#[derive(Debug)]
pub enum OpenError {
    Io(std::io::Error),
    /// The zip entry does not fit on the spool volume (volume, bytes needed).
    NotEnoughSpace {
        volume: String,
        needed: u64,
    },
    Refused(EntryRefusal),
    /// The zip holds no such entry.
    NoSuchEntry,
}

impl From<std::io::Error> for OpenError {
    fn from(e: std::io::Error) -> Self {
        OpenError::Io(e)
    }
}

/// Spool name for a gzip: the archive name without its `.gz` suffix, so `notes.md.gz`
/// still opens in the Markdown view.
fn gzip_inner_name(archive: &Path) -> String {
    let name = archive
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let lower = name.to_ascii_lowercase();
    for suffix in [".gz", ".gzip"] {
        if lower.ends_with(suffix) && name.len() > suffix.len() {
            return name[..name.len() - suffix.len()].to_string();
        }
    }
    name
}

/// The entry of `entries` whose stream path is the one of `entry` (however either spells
/// its separators): the one that opens when several share it, else the first.
fn find_entry(entries: Vec<ZipEntryInfo>, entry: &str) -> Result<ZipEntryInfo, OpenError> {
    let key = entry_key(entry);
    let mut matching = entries
        .into_iter()
        .filter(|e| e.refusal != Some(EntryRefusal::DuplicateName) && entry_key(&e.name) == key);
    let first = matching.next().ok_or(OpenError::NoSuchEntry)?;
    if first.refusal.is_none() {
        return Ok(first);
    }
    Ok(matching.find(|e| e.refusal.is_none()).unwrap_or(first))
}

/// Opens a decompressed stream: `entry` is `None` for a gzip file, the entry name for a
/// zip (any spelling of its stream path). The engine starts on an empty spool with follow off; the job fills the spool
/// in the background and the engine's poll indexes it as it grows.
pub fn open_engine(
    archive: &Path,
    entry: Option<&str>,
    settings: &Settings,
    wake: Option<WakeFn>,
) -> Result<TailEngine, OpenError> {
    // The entry name as the archive spells it, for the job.
    let mut real_entry = None;
    let spool_name = match entry {
        Some(entry) => {
            let info = find_entry(list_zip_entries(archive)?, entry)?;
            if let Some(refusal) = info.refusal {
                return Err(OpenError::Refused(refusal));
            }
            // Zip sizes are exact: refuse up front what cannot fit, before any spool.
            crate::spool::ensure_dir(&settings.spool_dir)?;
            if let Some((free, volume)) = free_space(&settings.spool_dir) {
                let needed = info.size.min(settings.limits.max_output);
                if free < needed.saturating_add(settings.limits.min_free) {
                    return Err(OpenError::NotEnoughSpace { volume, needed });
                }
            }
            real_entry = Some(info.name.clone());
            info.name
        }
        None => gzip_inner_name(archive),
    };
    let (spool, out) = SpoolFile::create(&settings.spool_dir, &spool_name)?;
    let mut engine = match &wake {
        Some(wake) => TailEngine::open_with_wake(spool.path(), wake.clone())?,
        None => TailEngine::open(spool.path())?,
    };
    engine.path = match entry {
        Some(entry) => entry_path(archive, entry),
        None => archive.to_path_buf(),
    };
    // Nothing is appended once the job ends, and jumping to the bottom on every chunk
    // while it runs would make the view unreadable: follow stays off.
    engine.follow_tail = false;
    let entry = entry
        .zip(real_entry)
        .map(|(entry, real)| (normalize_entry(entry), real));
    engine.compressed = Some(CompressedStream::start(
        archive, entry, settings, spool, out, wake,
    ));
    Ok(engine)
}

impl TailEngine {
    /// True for a stream read from a gzip file or a zip entry.
    pub fn is_compressed(&self) -> bool {
        self.compressed.is_some()
    }

    /// The file a user would name for this stream: the archive of a compressed stream,
    /// else the file being tailed (the resolved file of a pattern stream).
    pub fn source_file(&self) -> PathBuf {
        match &self.compressed {
            Some(c) => c.archive.clone(),
            None => self
                .current_file
                .clone()
                .unwrap_or_else(|| self.path.clone()),
        }
    }

    /// Re-extracts a compressed stream from the start, keeping its bookmarks.
    pub fn reload_compressed(&mut self) -> std::io::Result<()> {
        let bookmarks: Vec<usize> = self.bookmarks.iter().copied().collect();
        let Some(c) = self.compressed.as_mut() else {
            return Ok(());
        };
        c.restart()?;
        if !bookmarks.is_empty() {
            c.pending_bookmarks = bookmarks;
        }
        Ok(())
    }

    /// Follows the job of a compressed stream (called from `poll_updates`): once the job
    /// ended and its last bytes are indexed, settles the encoding detection of a stream
    /// that stayed under the sample size, and applies the bookmarks of a restored stream
    /// as soon as the index covers them.
    pub(crate) fn poll_compressed(&mut self) {
        let file_size = self.file_size;
        let Some(c) = self.compressed.as_mut() else {
            return;
        };
        if c.finalized {
            return;
        }
        // While a background index job runs, `file_size` is ahead of the index and
        // `total_lines` is partial: nothing is settled until the index catches up.
        if self.index_pending {
            return;
        }
        let finished = !c.is_running() && !c.has_unindexed(file_size);
        if finished {
            c.finalized = true;
            self.finish_encoding_detection();
        }
        let total = self.total_lines();
        let settled = !self.encoding_pending;
        let Some(c) = self.compressed.as_mut() else {
            return;
        };
        let fits = c
            .pending_bookmarks
            .iter()
            .max()
            .is_some_and(|&max| max < total);
        if settled && (fits || finished) && !c.pending_bookmarks.is_empty() {
            let bookmarks = std::mem::take(&mut c.pending_bookmarks);
            if fits {
                self.set_bookmarks(bookmarks);
                // A reload rebuilt the index and saved the bookmarks as gone: save
                // them again.
                self.bookmarks_dirty = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log_level::LogLevel;
    use std::io::Write;
    use std::time::{Duration, Instant};
    use zip::write::SimpleFileOptions;

    fn gzip(data: &[u8]) -> Vec<u8> {
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        enc.write_all(data).unwrap();
        enc.finish().unwrap()
    }

    /// Writes a zip holding `entries` (`None` data = a directory) into `path`.
    fn write_zip(path: &Path, entries: &[(&str, Option<&[u8]>, zip::CompressionMethod)]) {
        let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
        for (name, data, method) in entries {
            let options = SimpleFileOptions::default().compression_method(*method);
            match data {
                Some(data) => {
                    zip.start_file(*name, options).unwrap();
                    zip.write_all(data).unwrap();
                }
                None => {
                    zip.add_directory(*name, options).unwrap();
                }
            }
        }
        zip.finish().unwrap();
    }

    /// Rewrites the 16-bit field at `offset` of every zip header with signature `sig`.
    fn patch_zip_headers(path: &Path, sig: &[u8; 4], offset: usize, patch: impl Fn(u16) -> u16) {
        let mut bytes = std::fs::read(path).unwrap();
        let mut i = 0;
        while i + 4 <= bytes.len() {
            if &bytes[i..i + 4] == sig {
                let at = i + offset;
                let value = u16::from_le_bytes([bytes[at], bytes[at + 1]]);
                bytes[at..at + 2].copy_from_slice(&patch(value).to_le_bytes());
            }
            i += 1;
        }
        std::fs::write(path, bytes).unwrap();
    }

    fn log_text(lines: usize) -> Vec<u8> {
        let mut out = Vec::new();
        for i in 0..lines {
            let level = ["INFO", "WARN", "ERROR", "DEBUG"][i % 4];
            writeln!(
                out,
                "2026-09-25T10:{:02}:{:02}Z {level} line {i}",
                (i / 60) % 60,
                i % 60
            )
            .unwrap();
        }
        out
    }

    fn test_settings(dir: &Path) -> Settings {
        Settings {
            spool_dir: dir.join("spool"),
            limits: Limits {
                max_output: u64::MAX,
                min_free: 0,
                check_every: FREE_SPACE_CHECK_EVERY,
            },
        }
    }

    fn spooled_files(settings: &Settings) -> usize {
        std::fs::read_dir(&settings.spool_dir)
            .map(|d| d.count())
            .unwrap_or(0)
    }

    fn wait_job(job: &DecompressJob) -> JobState {
        let start = Instant::now();
        while job.is_running() {
            assert!(start.elapsed() < Duration::from_secs(20), "job never ended");
            std::thread::sleep(Duration::from_millis(2));
        }
        job.state()
    }

    /// Polls the engine until its decompression is over and indexed.
    fn settle(engine: &mut TailEngine) {
        let start = Instant::now();
        loop {
            engine.poll_updates();
            if engine.compressed.as_ref().unwrap().is_finalized() {
                return;
            }
            assert!(
                start.elapsed() < Duration::from_secs(20),
                "stream never settled"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn run_to_spool(source: JobSource, limits: Limits, dir: &Path) -> (JobState, Vec<u8>, u64) {
        let (spool, out) = SpoolFile::create(dir, "out.log").unwrap();
        let mut job = DecompressJob::start(source, out, dir.to_path_buf(), limits, None);
        let state = wait_job(&job);
        job.wait();
        (state, std::fs::read(spool.path()).unwrap(), job.written())
    }

    #[test]
    fn formats_are_told_by_their_magic_bytes() {
        assert_eq!(sniff_bytes(&[0x1f, 0x8b, 8, 0]), Format::Gzip);
        assert_eq!(sniff_bytes(b"PK\x03\x04rest"), Format::Zip);
        assert_eq!(sniff_bytes(b"PK\x05\x06"), Format::EmptyZip);
        assert_eq!(sniff_bytes(b"2026-09-25 INFO"), Format::Plain);
        assert_eq!(sniff_bytes(b""), Format::Plain);
        // The extension plays no part: a gzip named `.dat` is still a gzip.
        let dir = tempfile::tempdir().unwrap();
        let dat = dir.path().join("trace.dat");
        std::fs::write(&dat, gzip(b"hello\n")).unwrap();
        assert_eq!(sniff(&dat), Format::Gzip);
        assert_eq!(classify(&dat), Target::Gzip);
        let log = dir.path().join("plain.log");
        std::fs::write(&log, b"hello\n").unwrap();
        assert_eq!(classify(&log), Target::Plain);
        let fake = dir.path().join("fake.log");
        std::fs::write(&fake, b"PK\x03\x04 is how this line starts\n").unwrap();
        assert_eq!(sniff(&fake), Format::Zip);
        assert_eq!(classify(&fake), Target::Plain);
    }

    #[test]
    fn gzip_round_trip_and_multi_member() {
        let dir = tempfile::tempdir().unwrap();
        let data = log_text(5000);
        let gz = dir.path().join("app.log.1.gz");
        std::fs::write(&gz, gzip(&data)).unwrap();
        let (state, out, written) =
            run_to_spool(JobSource::Gzip(gz.clone()), Limits::default(), dir.path());
        assert_eq!(state, JobState::Done);
        assert_eq!(out, data);
        assert_eq!(written, data.len() as u64);

        // `cat a.gz b.gz`: both members come out, in order.
        let mut two = gzip(b"first member\n");
        two.extend(gzip(b"second member\n"));
        std::fs::write(&gz, two).unwrap();
        let (state, out, _) = run_to_spool(JobSource::Gzip(gz), Limits::default(), dir.path());
        assert_eq!(state, JobState::Done);
        assert_eq!(out, b"first member\nsecond member\n");
    }

    #[test]
    fn a_corrupt_gzip_stops_with_a_reason() {
        let dir = tempfile::tempdir().unwrap();
        let gz = dir.path().join("bad.gz");
        let mut bytes = gzip(&log_text(2000));
        let len = bytes.len();
        bytes.truncate(len / 2);
        std::fs::write(&gz, bytes).unwrap();
        let (state, _, _) = run_to_spool(JobSource::Gzip(gz), Limits::default(), dir.path());
        assert!(
            matches!(state, JobState::Stopped(StopReason::Failed(_))),
            "{state:?}"
        );
    }

    #[test]
    fn zip_entries_are_listed_and_extracted() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("bundle.zip");
        let server = log_text(300);
        let worker = b"worker started\nworker done\n".to_vec();
        write_zip(
            &bundle,
            &[
                ("config/", None, zip::CompressionMethod::Stored),
                (
                    "server.log",
                    Some(&server),
                    zip::CompressionMethod::Deflated,
                ),
                (
                    "logs/worker.log",
                    Some(&worker),
                    zip::CompressionMethod::Stored,
                ),
            ],
        );
        assert_eq!(classify(&bundle), Target::ZipArchive);
        let entries = list_zip_entries(&bundle).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["server.log", "logs/worker.log"]);
        assert_eq!(entries[0].size, server.len() as u64);
        assert!(entries[0].compressed_size < entries[0].size);
        assert!(entries.iter().all(|e| e.refusal.is_none()));

        for (entry, data) in [("server.log", &server), ("logs/worker.log", &worker)] {
            let (state, out, _) = run_to_spool(
                JobSource::ZipEntry {
                    archive: bundle.clone(),
                    entry: entry.to_string(),
                },
                Limits::default(),
                dir.path(),
            );
            assert_eq!(state, JobState::Done);
            assert_eq!(&out, data);
        }

        let empty = dir.path().join("empty.zip");
        write_zip(&empty, &[]);
        assert_eq!(classify(&empty), Target::EmptyZip);
    }

    #[test]
    fn entry_paths_name_the_archive_and_the_entry() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("bundle.zip");
        write_zip(
            &bundle,
            &[(
                "logs/worker.log",
                Some(b"x\n"),
                zip::CompressionMethod::Stored,
            )],
        );
        let path = entry_path(&bundle, "logs/worker.log");
        assert_eq!(path, bundle.join("logs").join("worker.log"));
        assert_eq!(archive_of(&path, "logs/worker.log"), bundle);
        assert_eq!(
            split_entry_path(&path),
            Some((bundle.clone(), "logs/worker.log".to_string()))
        );
        assert!(source_exists(&path));
        assert_eq!(
            classify(&path),
            Target::ZipEntry {
                archive: bundle.clone(),
                entry: "logs/worker.log".to_string()
            }
        );
        // Under a plain file or a missing path, nothing is an entry.
        let plain = dir.path().join("plain.log");
        std::fs::write(&plain, b"x").unwrap();
        assert_eq!(split_entry_path(&plain.join("x.log")), None);
        assert_eq!(
            split_entry_path(&dir.path().join("none").join("x.log")),
            None
        );
        assert!(!source_exists(&dir.path().join("none.log")));
    }

    #[test]
    fn unsupported_and_encrypted_entries_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let settings = test_settings(dir.path());
        let odd = dir.path().join("odd.zip");
        write_zip(
            &odd,
            &[("a.log", Some(b"hello\n"), zip::CompressionMethod::Stored)],
        );
        // Method 93 (zstd) in the local and central headers.
        patch_zip_headers(&odd, b"PK\x03\x04", 8, |_| 93);
        patch_zip_headers(&odd, b"PK\x01\x02", 10, |_| 93);
        let entries = list_zip_entries(&odd).unwrap();
        assert!(
            matches!(entries[0].refusal, Some(EntryRefusal::Method(_))),
            "{entries:?}"
        );
        assert!(matches!(
            open_engine(&odd, Some("a.log"), &settings, None),
            Err(OpenError::Refused(EntryRefusal::Method(_)))
        ));

        let locked = dir.path().join("locked.zip");
        write_zip(
            &locked,
            &[("a.log", Some(b"hello\n"), zip::CompressionMethod::Stored)],
        );
        // General-purpose flag bit 0: encrypted.
        patch_zip_headers(&locked, b"PK\x03\x04", 6, |f| f | 1);
        patch_zip_headers(&locked, b"PK\x01\x02", 8, |f| f | 1);
        let entries = list_zip_entries(&locked).unwrap();
        assert_eq!(entries[0].refusal, Some(EntryRefusal::Encrypted));
        assert!(matches!(
            open_engine(&locked, Some("a.log"), &settings, None),
            Err(OpenError::Refused(EntryRefusal::Encrypted))
        ));
        // Nothing was spooled for either.
        assert_eq!(spooled_files(&settings), 0);
    }

    #[test]
    fn a_tar_inside_gzip_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let mut tar = vec![0u8; 1024];
        tar[..8].copy_from_slice(b"app.log\0");
        tar[TAR_MAGIC_OFFSET..TAR_MAGIC_OFFSET + 6].copy_from_slice(b"ustar\0");
        let tgz = dir.path().join("logs.tgz");
        std::fs::write(&tgz, gzip(&tar)).unwrap();
        let (state, out, _) = run_to_spool(JobSource::Gzip(tgz), Limits::default(), dir.path());
        assert_eq!(state, JobState::Stopped(StopReason::Tar));
        assert!(out.is_empty());
    }

    #[test]
    fn the_cap_leaves_a_partial_spool() {
        let dir = tempfile::tempdir().unwrap();
        let data = log_text(100_000);
        assert!(data.len() > 3 * CHUNK_BYTES);
        let gz = dir.path().join("big.gz");
        std::fs::write(&gz, gzip(&data)).unwrap();
        let cap = CHUNK_BYTES as u64 + 12345;
        let limits = Limits {
            max_output: cap,
            ..Limits::default()
        };
        let (state, out, written) = run_to_spool(JobSource::Gzip(gz), limits, dir.path());
        assert_eq!(state, JobState::Stopped(StopReason::CapReached(cap)));
        assert_eq!(written, cap);
        assert_eq!(out, &data[..cap as usize]);
    }

    #[test]
    fn low_free_space_stops_the_job_and_refuses_a_zip_entry() {
        let dir = tempfile::tempdir().unwrap();
        if free_space(dir.path()).is_none() {
            // No disk information on this machine: the guard cannot be exercised.
            return;
        }
        let gz = dir.path().join("a.gz");
        std::fs::write(&gz, gzip(&log_text(1000))).unwrap();
        let limits = Limits {
            max_output: u64::MAX,
            min_free: u64::MAX,
            check_every: 1,
        };
        let (state, out, _) = run_to_spool(JobSource::Gzip(gz), limits, dir.path());
        assert!(
            matches!(state, JobState::Stopped(StopReason::DiskFull { .. })),
            "{state:?}"
        );
        // What was written before the stop stays.
        assert!(!out.is_empty());

        let bundle = dir.path().join("b.zip");
        write_zip(
            &bundle,
            &[("a.log", Some(b"hello\n"), zip::CompressionMethod::Deflated)],
        );
        let mut settings = test_settings(dir.path());
        settings.limits = limits;
        assert!(matches!(
            open_engine(&bundle, Some("a.log"), &settings, None),
            Err(OpenError::NotEnoughSpace { .. })
        ));
        assert_eq!(
            spooled_files(&settings),
            0,
            "no spool is created for a refused entry"
        );
    }

    #[test]
    fn cancel_stops_the_job_and_keeps_what_was_spooled() {
        let dir = tempfile::tempdir().unwrap();
        let data = log_text(200_000);
        let gz = dir.path().join("big.gz");
        std::fs::write(&gz, gzip(&data)).unwrap();
        // The wake callback reports each chunk and waits for the test to let it go on.
        let (chunk_tx, chunk_rx) = std::sync::mpsc::channel::<()>();
        let (go_tx, go_rx) = std::sync::mpsc::channel::<()>();
        let go_rx = Mutex::new(go_rx);
        let wake: WakeFn = Arc::new(move || {
            let _ = chunk_tx.send(());
            let _ = go_rx.lock().unwrap().recv_timeout(Duration::from_secs(5));
        });
        let (spool, out) = SpoolFile::create(dir.path(), "big.log").unwrap();
        let mut job = DecompressJob::start(
            JobSource::Gzip(gz),
            out,
            dir.path().to_path_buf(),
            Limits::default(),
            Some(wake),
        );
        chunk_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        job.cancel();
        // One release for the chunk wake, one for the final wake.
        let _ = go_tx.send(());
        let _ = go_tx.send(());
        let state = wait_job(&job);
        job.wait();
        assert_eq!(state, JobState::Stopped(StopReason::Cancelled));
        assert_eq!(job.written(), CHUNK_BYTES as u64);
        assert_eq!(std::fs::read(spool.path()).unwrap(), &data[..CHUNK_BYTES]);
    }

    #[test]
    fn an_engine_on_a_gzip_matches_the_plain_file() {
        let dir = tempfile::tempdir().unwrap();
        let data = log_text(40_000);
        let plain_path = dir.path().join("app.log");
        std::fs::write(&plain_path, &data).unwrap();
        let gz = dir.path().join("app.log.1.gz");
        std::fs::write(&gz, gzip(&data)).unwrap();
        let settings = test_settings(dir.path());

        let mut plain = TailEngine::open(&plain_path).unwrap();
        let mut packed = open_engine(&gz, None, &settings, None).unwrap();
        assert_eq!(packed.path, gz);
        assert!(packed.is_compressed());
        assert!(!packed.follow_tail);
        assert_eq!(packed.source_file(), gz);
        let spool = packed
            .compressed
            .as_ref()
            .unwrap()
            .spool_path()
            .to_path_buf();
        assert_eq!(packed.current_file.as_deref(), Some(spool.as_path()));
        assert!(spool
            .file_name()
            .unwrap()
            .to_string_lossy()
            .ends_with("-app.log.1"));
        settle(&mut packed);
        assert_eq!(packed.compressed.as_ref().unwrap().state(), JobState::Done);

        assert_eq!(packed.total_lines(), plain.total_lines());
        assert_eq!(packed.get_line(12_345), plain.get_line(12_345));
        for level in LogLevel::ALL {
            assert_eq!(packed.level_count(level), plain.level_count(level));
        }
        for engine in [&mut plain, &mut packed] {
            engine.set_include_filter("ERROR");
            engine.set_exclude_filter("line 1");
            engine.update_search("line 3");
            engine.toggle_bookmark(7);
            engine.toggle_bookmark(39_999);
        }
        assert_eq!(packed.visible_line_count(), plain.visible_line_count());
        assert_eq!(packed.filtered_lines, plain.filtered_lines);
        assert_eq!(packed.search_matches, plain.search_matches);
        assert_eq!(packed.bookmarks, plain.bookmarks);

        // Closing the stream deletes its spool.
        drop(packed);
        assert!(!spool.exists());
    }

    #[test]
    fn a_utf16_entry_is_detected_after_the_first_chunk() {
        let dir = tempfile::tempdir().unwrap();
        let text = "первая строка\nsecond line\n";
        let mut utf16 = vec![0xFF, 0xFE];
        for unit in text.encode_utf16() {
            utf16.extend_from_slice(&unit.to_le_bytes());
        }
        let bundle = dir.path().join("wide.zip");
        write_zip(
            &bundle,
            &[("wide.log", Some(&utf16), zip::CompressionMethod::Deflated)],
        );
        let settings = test_settings(dir.path());
        let mut engine = open_engine(&bundle, Some("wide.log"), &settings, None).unwrap();
        // Opened on the empty spool: nothing could be detected yet.
        assert!(engine.encoding_pending);
        settle(&mut engine);
        assert!(!engine.encoding_pending);
        assert_eq!(engine.encoding, crate::tail_engine::FileEncoding::UnicodeLe);
        assert_eq!(engine.total_lines(), 2);
        assert_eq!(engine.get_line(0).as_deref(), Some("первая строка"));
        assert_eq!(engine.path, entry_path(&bundle, "wide.log"));
        assert_eq!(
            engine.compressed.as_ref().unwrap().title(),
            "wide.zip › wide.log"
        );
    }

    #[test]
    fn a_large_utf16_stream_is_detected_at_the_sample_size() {
        let dir = tempfile::tempdir().unwrap();
        let mut utf16 = Vec::new();
        for unit in "ab\n".repeat(200_000).encode_utf16() {
            utf16.extend_from_slice(&unit.to_le_bytes());
        }
        let gz = dir.path().join("wide.log.gz");
        std::fs::write(&gz, gzip(&utf16)).unwrap();
        let mut engine = open_engine(&gz, None, &test_settings(dir.path()), None).unwrap();
        settle(&mut engine);
        assert_eq!(engine.encoding, crate::tail_engine::FileEncoding::UnicodeLe);
        assert_eq!(engine.total_lines(), 200_000);
        assert_eq!(engine.get_line(199_999).as_deref(), Some("ab"));
    }

    #[test]
    fn colours_past_the_first_sample_of_a_chunk_are_detected() {
        let dir = tempfile::tempdir().unwrap();
        let mut data = log_text(20_000);
        assert!(data.len() > 4 * crate::ansi::DETECT_SAMPLE_BYTES);
        data.extend_from_slice(b"\x1b[31mERROR\x1b[0m payment failed\n");
        let gz = dir.path().join("docker.log.gz");
        std::fs::write(&gz, gzip(&data)).unwrap();
        let mut engine = open_engine(&gz, None, &test_settings(dir.path()), None).unwrap();
        settle(&mut engine);
        assert_eq!(engine.ansi_effective(), crate::ansi::AnsiMode::Render);
        assert_eq!(
            engine.get_line(20_000).as_deref(),
            Some("ERROR payment failed")
        );
    }

    #[test]
    fn reload_extracts_again_and_restored_bookmarks_come_back() {
        let dir = tempfile::tempdir().unwrap();
        let data = log_text(3000);
        let gz = dir.path().join("app.gz");
        std::fs::write(&gz, gzip(&data)).unwrap();
        let mut engine = open_engine(&gz, None, &test_settings(dir.path()), None).unwrap();
        // A restored stream: the bookmarks wait for the index to reach them.
        engine.compressed.as_mut().unwrap().pending_bookmarks = vec![5, 2999];
        settle(&mut engine);
        assert_eq!(
            engine.bookmarks.iter().copied().collect::<Vec<_>>(),
            [5, 2999]
        );

        engine.reload_compressed().unwrap();
        assert!(!engine.compressed.as_ref().unwrap().is_finalized());
        settle(&mut engine);
        assert_eq!(engine.total_lines(), 3000);
        assert_eq!(
            engine.bookmarks.iter().copied().collect::<Vec<_>>(),
            [5, 2999]
        );
    }

    #[test]
    fn a_backslash_entry_keeps_the_slash_name_as_its_identity() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("win.zip");
        write_zip(
            &bundle,
            &[(
                "dir\\file.log",
                Some(b"from windows\n"),
                zip::CompressionMethod::Deflated,
            )],
        );
        let settings = test_settings(dir.path());
        let mut engine = open_engine(&bundle, Some("dir/file.log"), &settings, None).unwrap();
        let c = engine.compressed.as_ref().unwrap();
        // The identity (and what sessions save) is the `/` name; the job reads the entry
        // as the archive spells it.
        assert_eq!(c.entry.as_deref(), Some("dir/file.log"));
        assert_eq!(c.real_entry.as_deref(), Some("dir\\file.log"));
        assert_eq!(engine.path, entry_path(&bundle, "dir/file.log"));
        assert_eq!(archive_of(&engine.path, "dir/file.log"), bundle);
        // A value saved by 0.10.0 still names the right archive and entry path.
        assert_eq!(archive_of(&engine.path, "dir\\file.log"), bundle);
        assert_eq!(entry_path(&bundle, "dir\\file.log"), engine.path);
        settle(&mut engine);
        assert_eq!(engine.get_line(0).as_deref(), Some("from windows"));
    }

    #[test]
    fn a_dot_slash_entry_opens_by_its_stream_path() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("dot.zip");
        write_zip(
            &bundle,
            &[("./x.log", Some(b"dot\n"), zip::CompressionMethod::Stored)],
        );
        let path = entry_path(&bundle, "./x.log");
        assert_eq!(path, bundle.join("x.log"));
        let Target::ZipEntry { archive, entry } = classify(&path) else {
            panic!("not an entry path");
        };
        let settings = test_settings(dir.path());
        let mut engine = open_engine(&archive, Some(&entry), &settings, None).unwrap();
        assert_eq!(engine.path, path);
        assert_eq!(
            engine.compressed.as_ref().unwrap().entry.as_deref(),
            Some("x.log")
        );
        settle(&mut engine);
        assert_eq!(engine.get_line(0).as_deref(), Some("dot"));
    }

    #[test]
    fn entries_sharing_a_stream_path_are_refused_after_the_first() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("twins.zip");
        write_zip(
            &bundle,
            &[
                (
                    "a\\b.log",
                    Some(b"backslash\n"),
                    zip::CompressionMethod::Stored,
                ),
                ("a/b.log", Some(b"slash\n"), zip::CompressionMethod::Stored),
                ("c.log", Some(b"c\n"), zip::CompressionMethod::Stored),
            ],
        );
        let entries = list_zip_entries(&bundle).unwrap();
        let refusals: Vec<_> = entries.iter().map(|e| e.refusal.clone()).collect();
        assert_eq!(refusals, [None, Some(EntryRefusal::DuplicateName), None]);
        // The stream path opens the first of the two, whichever spelling asks for it.
        let settings = test_settings(dir.path());
        for asked in ["a/b.log", "a\\b.log"] {
            let mut engine = open_engine(&bundle, Some(asked), &settings, None).unwrap();
            assert_eq!(
                engine.compressed.as_ref().unwrap().real_entry.as_deref(),
                Some("a\\b.log")
            );
            settle(&mut engine);
            assert_eq!(engine.get_line(0).as_deref(), Some("backslash"));
        }
        if cfg!(windows) {
            // Names differing only by case are one stream path on Windows.
            let cased = dir.path().join("cased.zip");
            write_zip(
                &cased,
                &[
                    ("App.log", Some(b"upper\n"), zip::CompressionMethod::Stored),
                    ("app.log", Some(b"lower\n"), zip::CompressionMethod::Stored),
                ],
            );
            let entries = list_zip_entries(&cased).unwrap();
            assert_eq!(entries[1].refusal, Some(EntryRefusal::DuplicateName));
        }
    }

    #[test]
    fn restored_bookmarks_survive_a_background_index() {
        let dir = tempfile::tempdir().unwrap();
        let data = log_text(50_000);
        let gz = dir.path().join("app.gz");
        std::fs::write(&gz, gzip(&data)).unwrap();
        let mut engine = open_engine(&gz, None, &test_settings(dir.path()), None).unwrap();
        // Index every append on a worker thread, as a large stream does.
        engine.index_job_threshold_bytes = 0;
        engine.compressed.as_mut().unwrap().pending_bookmarks = vec![5, 49_999];
        settle(&mut engine);
        assert_eq!(engine.total_lines(), 50_000);
        assert_eq!(
            engine.bookmarks.iter().copied().collect::<Vec<_>>(),
            [5, 49_999]
        );
    }
}
