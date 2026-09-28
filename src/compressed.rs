//! Compressed log input: gzip, bzip2, xz and zstd files, zip and 7z entries and tar entries
//! (plain or compressed tar) are decompressed on a background thread into a spool file
//! (see `spool`) that the tail engine opens like any other log, so every feature works on
//! the result and nothing decompressed is held in memory.
//!
//! - `sniff` recognises the format by its first 512 bytes, whatever the extension;
//!   `classify` also peeks at the first 512 decompressed bytes of a codec file to tell a
//!   compressed tar from a compressed log.
//! - `open_decoder` is the codec layer shared by single files and archive entries: every
//!   decoder reads concatenated members, streams or frames as one stream.
//! - `list_zip_entries` reads the central directory once, for the entry picker; a tar has
//!   no directory, so `TarScan` walks its headers on a worker thread while the picker
//!   shows the rows as they are found. `list_7z_entries` reads a 7z header once too,
//!   within bounds checked before the 7z reader allocates anything.
//! - `DecompressJob` inflates into the spool in 1 MB chunks (flushed, so the engine's
//!   normal poll indexes them as they land), reports progress by compressed bytes
//!   consumed, stops on cancel, at the output cap, when the spool volume runs low, or on a
//!   tar archive inside a single compressed file. Whatever was written before a stop
//!   stays readable.
//! - `open_engine` wires it together: an engine on the empty spool whose `path` is the
//!   stream identity (the archive, or `archive/entry` for an archive entry) and whose
//!   `compressed` field owns the job and the spool, so closing the stream stops the job
//!   and deletes the spool.

use crate::spool::SpoolFile;
use crate::tail_engine::{TailEngine, WakeFn};
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::SystemTime;

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
/// Bytes a format is decided from, raw or decompressed: one tar header.
pub const SNIFF_BYTES: usize = 512;
/// Largest dictionary (xz) or window (zstd) a decoder may allocate: a file declaring more
/// is refused before the memory is taken.
pub const MAX_DECODER_WINDOW: u64 = 256 * 1024 * 1024;
/// Offset and magic of a tar header (`ustar`).
const TAR_MAGIC_OFFSET: usize = 257;
const TAR_MAGIC: &[u8] = b"ustar";
/// Size of a tar block: headers and data are padded to it.
const TAR_BLOCK: u64 = 512;

/// A single-stream compression format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Codec {
    Gzip,
    Bzip2,
    Xz,
    Zstd,
}

/// Container format of a file, from its first bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Compressed(Codec),
    Zip,
    /// A zip holding nothing (end-of-central-directory record only).
    EmptyZip,
    /// A plain (uncompressed) tar archive.
    Tar,
    /// A 7z archive.
    SevenZ,
    /// Anything else: opened as a plain file.
    Plain,
}

/// Signature at the start of every 7z archive.
const SEVENZ_MAGIC: [u8; 6] = [0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C];

/// bzip2 block magic (pi) and end-of-stream magic (sqrt pi), after `BZh1`..`BZh9`.
const BZIP2_BLOCK: [u8; 6] = [0x31, 0x41, 0x59, 0x26, 0x53, 0x59];
const BZIP2_END: [u8; 6] = [0x17, 0x72, 0x45, 0x38, 0x50, 0x90];

/// True when `head` starts with a bzip2 stream header. `BZh1` alone is too weak (a text
/// file may start with it), so the block or end-of-stream magic after it is checked too.
fn is_bzip2(head: &[u8]) -> bool {
    head.len() >= 10
        && head.starts_with(b"BZh")
        && (b'1'..=b'9').contains(&head[3])
        && (head[4..10] == BZIP2_BLOCK || head[4..10] == BZIP2_END)
}

/// True when `head` starts with a zstd frame or a zstd skippable frame.
fn is_zstd(head: &[u8]) -> bool {
    head.starts_with(&[0x28, 0xB5, 0x2F, 0xFD])
        || (head.len() >= 4 && head[0] & 0xF0 == 0x50 && head[1..4] == [0x2A, 0x4D, 0x18])
}

/// True when `head` is (the start of) a tar header.
fn is_tar_head(head: &[u8]) -> bool {
    head.len() >= TAR_MAGIC_OFFSET + TAR_MAGIC.len()
        && &head[TAR_MAGIC_OFFSET..TAR_MAGIC_OFFSET + TAR_MAGIC.len()] == TAR_MAGIC
}

/// Format of the data starting with `head` (at most `SNIFF_BYTES` are looked at).
pub fn sniff_bytes(head: &[u8]) -> Format {
    if head.starts_with(&[0x1f, 0x8b]) {
        Format::Compressed(Codec::Gzip)
    } else if is_bzip2(head) {
        Format::Compressed(Codec::Bzip2)
    } else if head.starts_with(&[0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00]) {
        Format::Compressed(Codec::Xz)
    } else if is_zstd(head) {
        Format::Compressed(Codec::Zstd)
    } else if head.starts_with(b"PK\x03\x04") {
        Format::Zip
    } else if head.starts_with(b"PK\x05\x06") {
        Format::EmptyZip
    } else if head.starts_with(&SEVENZ_MAGIC) {
        Format::SevenZ
    } else if is_tar_head(head) {
        Format::Tar
    } else {
        Format::Plain
    }
}

/// The first `SNIFF_BYTES` bytes `reader` gives (fewer at its end); read errors end it.
fn read_head(reader: &mut impl Read) -> Vec<u8> {
    let mut head = vec![0u8; SNIFF_BYTES];
    let filled = read_full(reader, &mut head).unwrap_or(0);
    head.truncate(filled);
    head
}

/// Format of the file at `path` (`Plain` when it cannot be read).
pub fn sniff(path: &Path) -> Format {
    match crate::file_source::open_file_shared(path) {
        Ok(mut file) => sniff_bytes(&read_head(&mut file)),
        Err(_) => Format::Plain,
    }
}

/// Window a decoder may allocate for the UI-thread peek of `codec_holds_tar`: enough for
/// `xz -6` and `zstd -19` (8 MiB). The xz decoder zero-fills its whole dictionary up front,
/// so a larger one is not decoded on the UI thread at all.
const PEEK_WINDOW: u64 = 8 * 1024 * 1024;

/// True for a name that says tar: `.tar.gz`, `.tgz`, `.tar.xz`, `.txz`...
fn has_tar_name(path: &Path) -> bool {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let stem = name.rsplit_once('.').map_or("", |(stem, _)| stem);
    stem.ends_with(".tar")
        || [".tgz", ".tbz", ".tbz2", ".txz", ".tzst"]
            .iter()
            .any(|s| name.ends_with(s))
}

/// The first `SNIFF_BYTES` decompressed bytes of the `codec` file at `path` (fewer when it
/// is shorter, damaged or unreadable), with the decoder window capped at `PEEK_WINDOW`;
/// `None` when the file needs a larger window.
fn peek_decoded(path: &Path, codec: Codec) -> Option<Vec<u8>> {
    let Ok(file) = crate::file_source::open_file_shared(path) else {
        return Some(Vec::new());
    };
    let mut decoder = open_decoder_capped(codec, BufReader::new(file), PEEK_WINDOW);
    let mut head = vec![0u8; SNIFF_BYTES];
    let mut filled = 0;
    while filled < head.len() {
        match decoder.read(&mut head[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) if is_window_too_large(&e) => return None,
            Err(_) => break,
        }
    }
    head.truncate(filled);
    Some(head)
}

/// Whether the `codec` file at `path` holds a tar. It runs on the UI thread (opening a
/// file), so it costs at most one small decoder: a name that says tar is believed, other
/// files are peeked at with `PEEK_WINDOW`, one needing more is taken for a single log, and
/// the answer is kept per (path, size, modification time).
fn codec_holds_tar(path: &Path, codec: Codec) -> bool {
    if has_tar_name(path) {
        return true;
    }
    let Some(when) = stamp(path) else {
        return false;
    };
    if let Some(known) = tar_peeks().lock().ok().and_then(|mut p| p.get(path, when)) {
        return known;
    }
    let holds = peek_decoded(path, codec).is_some_and(|head| is_tar_head(&head));
    if let Ok(mut p) = tar_peeks().lock() {
        p.insert(path, when, holds);
    }
    holds
}

/// The answers of `codec_holds_tar`, per (path, size, modification time).
fn tar_peeks() -> &'static Mutex<StampedLru<bool>> {
    static PEEKS: OnceLock<Mutex<StampedLru<bool>>> = OnceLock::new();
    PEEKS.get_or_init(|| Mutex::new(StampedLru::new(64)))
}

/// A decompression job found a tar the UI-thread peek could not see (a window over
/// `PEEK_WINDOW` and no tar name): opening the file again must show the entry picker,
/// not stop on the same tar once more.
fn remember_holds_tar(path: &Path) {
    if let (Some(when), Ok(mut p)) = (stamp(path), tar_peeks().lock()) {
        p.insert(path, when, true);
    }
}

/// A small most-recently-used map keyed by path, whose values hold while the file's size
/// and modification time do.
struct StampedLru<V> {
    items: Vec<(PathBuf, Stamp, V)>,
    capacity: usize,
}

impl<V: Clone> StampedLru<V> {
    fn new(capacity: usize) -> Self {
        Self {
            items: Vec::new(),
            capacity,
        }
    }

    /// The value for `path` if its stamp still matches (it becomes the most recent).
    fn get(&mut self, path: &Path, when: Stamp) -> Option<V> {
        let i = self.items.iter().position(|(p, _, _)| p == path)?;
        let item = self.items.remove(i);
        let value = (item.1 == when).then(|| item.2.clone());
        if value.is_some() {
            self.items.push(item);
        }
        value
    }

    /// Forgets `path` when its value passes `test`.
    fn remove_if(&mut self, path: &Path, test: impl Fn(&V) -> bool) {
        self.items.retain(|(p, _, v)| p != path || !test(v));
    }

    /// Stores `value` as the most recent, evicting the least recently used past capacity.
    fn insert(&mut self, path: &Path, when: Stamp, value: V) {
        self.items.retain(|(p, _, _)| p != path);
        self.items.push((path.to_path_buf(), when, value));
        if self.items.len() > self.capacity {
            self.items.remove(0);
        }
    }
}

/// A decoder that would need more memory than `MAX_DECODER_WINDOW`: the stream is refused
/// before that memory is allocated.
#[derive(Debug)]
pub struct WindowTooLarge;

impl std::fmt::Display for WindowTooLarge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the decoder window is larger than {} MiB",
            MAX_DECODER_WINDOW / (1024 * 1024)
        )
    }
}

impl std::error::Error for WindowTooLarge {}

/// True when `e` says a decoder refused its window (zstd) or dictionary (xz, which
/// reports it as out of memory).
fn is_window_too_large(e: &std::io::Error) -> bool {
    e.kind() == std::io::ErrorKind::OutOfMemory
        || e.get_ref()
            .is_some_and(|inner| inner.is::<WindowTooLarge>())
}

/// Decoder of `codec` reading `input`: concatenated gzip members, bzip2 streams (pbzip2
/// output), xz streams and zstd frames all come out as one stream, and zstd skippable
/// frames are skipped. The xz dictionary and the zstd window are capped at
/// `MAX_DECODER_WINDOW`.
pub fn open_decoder<'a, R: BufRead + 'a>(codec: Codec, input: R) -> Box<dyn Read + 'a> {
    open_decoder_capped(codec, input, MAX_DECODER_WINDOW)
}

/// `open_decoder` with the xz dictionary and the zstd window capped at `max_window`.
fn open_decoder_capped<'a, R: BufRead + 'a>(
    codec: Codec,
    input: R,
    max_window: u64,
) -> Box<dyn Read + 'a> {
    match codec {
        Codec::Gzip => Box::new(flate2::bufread::MultiGzDecoder::new(input)),
        Codec::Bzip2 => Box::new(bzip2::bufread::MultiBzDecoder::new(input)),
        Codec::Xz => {
            // The limit counts the dictionary plus the decoder's own buffers (under
            // 1 MiB): a dictionary of `max_window` still fits, anything larger does not.
            let limit_kib = (max_window / 1024 + 1024) as u32;
            Box::new(lzma_rust2::XzReader::new_mem_limit(input, true, limit_kib))
        }
        Codec::Zstd => Box::new(MultiZstd::new(input, max_window)),
    }
}

/// zstd decoder over every frame of the input (ruzstd's own reader stops after one).
struct MultiZstd<R> {
    input: R,
    frame: ruzstd::decoding::FrameDecoder,
    in_frame: bool,
}

impl<R: BufRead> MultiZstd<R> {
    fn new(input: R, max_window: u64) -> Self {
        let mut frame = ruzstd::decoding::FrameDecoder::new();
        frame.set_max_window_size(max_window);
        Self {
            input,
            frame,
            in_frame: false,
        }
    }
}

fn zstd_error(e: ruzstd::decoding::errors::FrameDecoderError) -> std::io::Error {
    use ruzstd::decoding::errors::FrameDecoderError;
    match e {
        FrameDecoderError::WindowSizeTooBig { .. } => std::io::Error::other(WindowTooLarge),
        e => std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()),
    }
}

impl<R: BufRead> Read for MultiZstd<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        use ruzstd::decoding::errors::{FrameDecoderError, ReadFrameHeaderError};
        use ruzstd::decoding::BlockDecodingStrategy;
        if buf.is_empty() {
            return Ok(0);
        }
        loop {
            if self.in_frame {
                // Decoded bytes become collectable only once they leave the window: keep
                // decoding until the buffer can be filled or the frame ends.
                while self.frame.can_collect() < buf.len() && !self.frame.is_finished() {
                    let wanted = buf.len() - self.frame.can_collect();
                    self.frame
                        .decode_blocks(&mut self.input, BlockDecodingStrategy::UptoBytes(wanted))
                        .map_err(zstd_error)?;
                }
                let n = self.frame.read(buf)?;
                if n > 0 {
                    return Ok(n);
                }
                self.in_frame = false;
            }
            if self.input.fill_buf()?.is_empty() {
                return Ok(0);
            }
            match self.frame.reset(&mut self.input) {
                Ok(()) => self.in_frame = true,
                Err(FrameDecoderError::ReadFrameHeaderError(ReadFrameHeaderError::SkipFrame {
                    length,
                    ..
                })) => {
                    let skipped = std::io::copy(
                        &mut (&mut self.input).take(length as u64),
                        &mut std::io::sink(),
                    )?;
                    if skipped < length as u64 {
                        return Err(std::io::ErrorKind::UnexpectedEof.into());
                    }
                }
                Err(e) => return Err(zstd_error(e)),
            }
        }
    }
}

/// Why an archive entry cannot be opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryRefusal {
    Encrypted,
    /// Compression method other than stored or deflate (its name).
    Method(String),
    /// A name that climbs out of the archive (`../x`), is absolute or has a drive prefix.
    UnsafeName,
    /// Another entry earlier in the archive has the same stream path (`a/b.log` and
    /// `a\b.log`, or names differing only by case on Windows): only the first opens.
    DuplicateName,
    /// A tar symbolic link, hard link, character or block device, or FIFO.
    LinkOrSpecial,
    /// A GNU sparse tar entry.
    Sparse,
    /// A 7z entry whose LZMA / LZMA2 dictionary or PPMd model is larger than
    /// `MAX_DECODER_WINDOW`: refused before that memory is allocated.
    DictionaryTooLarge,
}

/// One file entry of a zip, tar or 7z archive, as listed by the entry picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveEntryInfo {
    pub name: String,
    pub size: u64,
    /// Compressed size of a zip entry; a tar stores its entries as they are (`size`); a
    /// 7z entry of a solid block gets its share of the block.
    pub compressed_size: u64,
    /// 7z only: the unpacked size of the solid block holding the entry when the block
    /// holds other entries too (they are decoded, and dropped, to reach it).
    pub block_size: Option<u64>,
    /// Plain tar only: where the headers of this entry start in the archive (the end of
    /// the previous entry, so GNU long-name and pax headers are read again), so its
    /// extraction seeks there instead of reading the archive from the start.
    pub offset: Option<u64>,
    /// Set when the entry cannot be opened; the picker shows it disabled with the reason.
    pub refusal: Option<EntryRefusal>,
}

/// True for a name that is absolute, has a drive prefix or climbs out of the archive.
fn is_unsafe_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    name.starts_with(['/', '\\'])
        || (bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic())
        || name.split(['/', '\\']).any(|part| part == "..")
}

/// The checks every archive makes last, for an entry that can otherwise be opened: an
/// unsafe name is refused, and so is a name whose stream path an earlier openable entry
/// took (`keys`), so each tab reads one entry.
fn claim_name(name: &str, keys: &mut HashSet<String>) -> Option<EntryRefusal> {
    if is_unsafe_name(name) {
        Some(EntryRefusal::UnsafeName)
    } else if !keys.insert(entry_key(name)) {
        Some(EntryRefusal::DuplicateName)
    } else {
        None
    }
}

/// The file entries of the zip at `path` (directories skipped), in archive order.
pub fn list_zip_entries(path: &Path) -> std::io::Result<Vec<ArchiveEntryInfo>> {
    let file = crate::file_source::open_file_shared(path)?;
    let mut archive = zip::ZipArchive::new(BufReader::new(file)).map_err(zip_err)?;
    let mut out = Vec::with_capacity(archive.len());
    let mut opened_keys = HashSet::new();
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
        } else {
            claim_name(entry.name(), &mut opened_keys)
        };
        out.push(ArchiveEntryInfo {
            name: entry.name().to_string(),
            size: entry.size(),
            compressed_size: entry.compressed_size(),
            block_size: None,
            offset: None,
            refusal,
        });
    }
    Ok(out)
}

fn zip_err(e: zip::result::ZipError) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
}

/// Largest 7z header read to list an archive: the header as stored, and the header an
/// encoded (compressed) header decodes to.
pub const MAX_7Z_HEADER: u64 = 64 * 1024 * 1024;
/// Entries a 7z listing shows at most; beyond, the picker says the list is partial.
pub const MAX_7Z_ENTRIES: usize = 100_000;
/// Files, blocks, packed streams or block sub-streams a 7z header may declare: the 7z
/// reader allocates a record for each before any listing bound applies, so a header
/// declaring more is refused before it is handed over.
pub const MAX_7Z_ITEMS: u64 = 250_000;
/// Coders (and streams of one coder) a 7z block may chain.
const MAX_7Z_CODERS: u64 = 32;

/// Why a whole 7z archive cannot be listed (its entries are never shown).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SevenZRefusal {
    /// The header is encrypted: without a password not even the names can be read.
    EncryptedHeader,
    /// The header is larger than `MAX_7Z_HEADER` (or its decoder would need more than
    /// `MAX_DECODER_WINDOW`).
    HeaderTooLarge,
    /// The header declares more than `MAX_7Z_ITEMS` files, blocks or streams.
    TooManyEntries,
}

impl std::fmt::Display for SevenZRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SevenZRefusal::EncryptedHeader => write!(f, "the 7z header is encrypted"),
            SevenZRefusal::HeaderTooLarge => write!(
                f,
                "the 7z header is larger than {} MB",
                MAX_7Z_HEADER / (1024 * 1024)
            ),
            SevenZRefusal::TooManyEntries => {
                write!(
                    f,
                    "the 7z archive declares more than {MAX_7Z_ITEMS} entries"
                )
            }
        }
    }
}

impl std::error::Error for SevenZRefusal {}

/// The `SevenZRefusal` an error of `list_7z_entries` or `open_engine` carries, if any.
pub fn sevenz_refusal(e: &std::io::Error) -> Option<&SevenZRefusal> {
    e.get_ref()?.downcast_ref::<SevenZRefusal>()
}

fn sevenz_refused(refusal: SevenZRefusal) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, refusal)
}

/// The entries of a 7z archive, as the picker lists them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SevenZListing {
    pub entries: Vec<ArchiveEntryInfo>,
    /// More than `MAX_7Z_ENTRIES` file entries: only the first ones are listed.
    pub partial: bool,
}

/// The coders an entry may use: copy, LZMA, LZMA2, BZip2, Deflate, PPMd, and the BCJ
/// family (x86, BCJ2, ARM and the other branch filters the reader decodes) and delta
/// filters.
fn sevenz_coder_supported(id: &[u8]) -> bool {
    use sevenz_rust2::EncoderMethod as M;
    [
        M::ID_COPY,
        M::ID_LZMA,
        M::ID_LZMA2,
        M::ID_BZIP2,
        M::ID_DEFLATE,
        M::ID_PPMD,
        M::ID_DELTA,
        M::ID_BCJ_X86,
        M::ID_BCJ2,
        M::ID_BCJ_ARM,
        M::ID_BCJ_ARM64,
        M::ID_BCJ_ARM_THUMB,
        M::ID_BCJ_PPC,
        M::ID_BCJ_IA64,
        M::ID_BCJ_SPARC,
        M::ID_BCJ_RISCV,
    ]
    .contains(&id)
}

/// Memory the decoder of a 7z coder allocates for its dictionary (LZMA, LZMA2) or model
/// (PPMd), read from the coder properties; `None` for any other coder.
fn sevenz_coder_window(id: &[u8], props: &[u8]) -> Option<u64> {
    use sevenz_rust2::EncoderMethod as M;
    let le32 = || {
        props
            .get(1..5)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as u64)
    };
    if id == M::ID_LZMA || id == M::ID_PPMD {
        // A property block too short to hold the size is damage: the reader says so.
        le32()
    } else if id == M::ID_LZMA2 {
        let bits = (*props.first()? & 0x3F) as u64;
        Some(if bits >= 40 {
            u32::MAX as u64
        } else {
            (2 | (bits & 1)) << (bits / 2 + 11)
        })
    } else {
        None
    }
}

/// Why the coders `(id, properties)` of a 7z block keep its entries from opening: an
/// AES coder (whatever else the block holds), another coder, or dictionaries that
/// together exceed `MAX_DECODER_WINDOW` (every coder of a chain holds its own).
fn sevenz_coders_refusal<'c>(
    coders: impl IntoIterator<Item = (&'c [u8], &'c [u8])>,
) -> Option<EntryRefusal> {
    let mut refusal = None;
    let mut windows = 0u64;
    for (id, props) in coders {
        if id == sevenz_rust2::EncoderMethod::ID_AES256_SHA256 {
            return Some(EntryRefusal::Encrypted);
        }
        if !sevenz_coder_supported(id) {
            let name = sevenz_rust2::EncoderMethod::by_id(id).map_or_else(
                || id.iter().map(|b| format!("{b:02X}")).collect::<String>(),
                |m| m.name().to_string(),
            );
            refusal.get_or_insert(EntryRefusal::Method(name));
        } else if let Some(window) = sevenz_coder_window(id, props) {
            windows = windows.saturating_add(window);
        }
    }
    refusal.or((windows > MAX_DECODER_WINDOW).then_some(EntryRefusal::DictionaryTooLarge))
}

fn sevenz_block_refusal(block: &sevenz_rust2::Block) -> Option<EntryRefusal> {
    sevenz_coders_refusal(
        block
            .coders
            .iter()
            .map(|c| (c.encoder_method_id(), c.properties())),
    )
}

/// What stops a 7z header walk: damage, or a bound FastTail refuses to go past.
#[derive(Debug)]
enum HeaderIssue {
    Damaged(&'static str),
    Refused(SevenZRefusal),
}

impl From<HeaderIssue> for std::io::Error {
    fn from(issue: HeaderIssue) -> Self {
        match issue {
            HeaderIssue::Damaged(what) => std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("damaged 7z archive: {what}"),
            ),
            HeaderIssue::Refused(refusal) => sevenz_refused(refusal),
        }
    }
}

type Walk<T> = Result<T, HeaderIssue>;

/// `count` when it is within `MAX_7Z_ITEMS`.
fn bounded_items(count: u64) -> Walk<u64> {
    if count > MAX_7Z_ITEMS {
        Err(HeaderIssue::Refused(SevenZRefusal::TooManyEntries))
    } else {
        Ok(count)
    }
}

/// `count` when it is within `MAX_7Z_CODERS`.
fn bounded_coders(count: u64) -> Walk<u64> {
    if count > MAX_7Z_CODERS {
        Err(HeaderIssue::Damaged("too many coders in a block"))
    } else {
        Ok(count)
    }
}

/// Reads the bytes of a 7z header: its numbers are 1 to 9 bytes long, the count of
/// leading one bits of the first byte saying how many bytes follow.
struct HeaderBytes<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> HeaderBytes<'a> {
    fn take(&mut self, n: u64) -> Walk<&'a [u8]> {
        let truncated = HeaderIssue::Damaged("truncated header");
        let Some(end) = usize::try_from(n)
            .ok()
            .and_then(|n| self.pos.checked_add(n))
        else {
            return Err(truncated);
        };
        let bytes = self.data.get(self.pos..end).ok_or(truncated)?;
        self.pos = end;
        Ok(bytes)
    }

    fn byte(&mut self) -> Walk<u8> {
        self.take(1).map(|b| b[0])
    }

    fn number(&mut self) -> Walk<u64> {
        let first = self.byte()? as u64;
        let mut mask = 0x80u64;
        let mut value = 0u64;
        for i in 0..8 {
            if first & mask == 0 {
                return Ok(value | ((first & (mask - 1)) << (8 * i)));
            }
            value |= (self.byte()? as u64) << (8 * i);
            mask >>= 1;
        }
        Ok(value)
    }

    /// A digest list of `count` items (a bounded count): all-defined byte or bit field,
    /// then the CRCs of the defined ones.
    fn digests(&mut self, count: u64) -> Walk<Vec<Option<u32>>> {
        let defined: Vec<bool> = if self.byte()? != 0 {
            vec![true; count as usize]
        } else {
            let bits = self.take(count.div_ceil(8))?;
            (0..count as usize)
                .map(|i| bits[i / 8] & (0x80 >> (i % 8)) != 0)
                .collect()
        };
        defined
            .into_iter()
            .map(|d| {
                d.then(|| {
                    self.take(4)
                        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                })
                .transpose()
            })
            .collect()
    }
}

/// A block ("folder") of a 7z streams info, as far as the walk needs it.
#[derive(Default)]
struct SzFolder {
    coders: Vec<(Vec<u8>, Vec<u8>)>,
    outputs: u64,
    unpack_sizes: Vec<u64>,
    crc: Option<u32>,
    /// Files (sub-streams) the block holds.
    streams: u64,
}

#[derive(Default)]
struct SzStreams {
    pack_pos: u64,
    pack_sizes: Vec<u64>,
    folders: Vec<SzFolder>,
}

/// Walks a 7z streams info (the one of the main header, or the one saying how an
/// encoded header is packed), refusing any count past `MAX_7Z_ITEMS` before the 7z reader
/// would allocate for it.
fn walk_streams_info(h: &mut HeaderBytes) -> Walk<SzStreams> {
    let mut s = SzStreams::default();
    let mut id = h.byte()?;
    if id == 0x06 {
        s.pack_pos = h.number()?;
        let count = bounded_items(h.number()?)?;
        loop {
            match h.byte()? {
                0x00 => break,
                0x09 => {
                    for _ in 0..count {
                        s.pack_sizes.push(h.number()?);
                    }
                }
                0x0A => {
                    h.digests(count)?;
                }
                _ => return Err(HeaderIssue::Damaged("pack info")),
            }
        }
        id = h.byte()?;
    }
    if id == 0x07 {
        if h.byte()? != 0x0B {
            return Err(HeaderIssue::Damaged("unpack info"));
        }
        let count = bounded_items(h.number()?)?;
        if h.byte()? != 0 {
            return Err(HeaderIssue::Damaged("external folders"));
        }
        for _ in 0..count {
            let mut folder = SzFolder {
                streams: 1,
                ..SzFolder::default()
            };
            let (mut ins, mut outs) = (0u64, 0u64);
            let coders = bounded_coders(h.number()?)?;
            if coders == 0 {
                return Err(HeaderIssue::Damaged("block without coders"));
            }
            for _ in 0..coders {
                let flags = h.byte()?;
                if flags & 0x80 != 0 {
                    return Err(HeaderIssue::Damaged("alternative coder methods"));
                }
                let coder = h.take((flags & 0x0F) as u64)?.to_vec();
                let (i, o) = if flags & 0x10 != 0 {
                    (bounded_coders(h.number()?)?, bounded_coders(h.number()?)?)
                } else {
                    (1, 1)
                };
                let props = if flags & 0x20 != 0 {
                    let n = h.number()?;
                    h.take(n)?.to_vec()
                } else {
                    Vec::new()
                };
                ins += i;
                outs += o;
                folder.coders.push((coder, props));
            }
            let binds = outs
                .checked_sub(1)
                .ok_or(HeaderIssue::Damaged("block without output"))?;
            for _ in 0..binds {
                h.number()?;
                h.number()?;
            }
            let packed = ins
                .checked_sub(binds)
                .ok_or(HeaderIssue::Damaged("block streams"))?;
            if packed > 1 {
                for _ in 0..packed {
                    h.number()?;
                }
            }
            folder.outputs = outs;
            s.folders.push(folder);
        }
        if h.byte()? != 0x0C {
            return Err(HeaderIssue::Damaged("unpack sizes"));
        }
        for folder in &mut s.folders {
            for _ in 0..folder.outputs {
                folder.unpack_sizes.push(h.number()?);
            }
        }
        loop {
            match h.byte()? {
                0x00 => break,
                0x0A => {
                    for (folder, crc) in s.folders.iter_mut().zip(h.digests(count)?) {
                        folder.crc = crc;
                    }
                }
                _ => return Err(HeaderIssue::Damaged("unpack info")),
            }
        }
        id = h.byte()?;
    }
    if id == 0x08 {
        id = h.byte()?;
        if id == 0x0D {
            let mut total = 0u64;
            for folder in &mut s.folders {
                folder.streams = bounded_items(h.number()?)?;
                total = bounded_items(total + folder.streams)?;
            }
            id = h.byte()?;
        }
        if id == 0x09 {
            for folder in &s.folders {
                for _ in 1..folder.streams {
                    h.number()?;
                }
            }
            id = h.byte()?;
        }
        while id != 0x00 {
            if id != 0x0A {
                return Err(HeaderIssue::Damaged("sub-streams info"));
            }
            let count = s
                .folders
                .iter()
                .filter(|f| !(f.streams == 1 && f.crc.is_some()))
                .map(|f| f.streams)
                .sum();
            h.digests(count)?;
            id = h.byte()?;
        }
        id = h.byte()?;
    }
    if id != 0x00 {
        return Err(HeaderIssue::Damaged("streams info"));
    }
    Ok(s)
}

/// Walks a plain 7z header up to its file count, refusing any count past
/// `MAX_7Z_ITEMS`.
fn walk_header(data: &[u8]) -> Walk<()> {
    let mut h = HeaderBytes { data, pos: 0 };
    if h.byte()? != 0x01 {
        return Err(HeaderIssue::Damaged("no header"));
    }
    let mut id = h.byte()?;
    if id == 0x02 {
        // Archive properties: skipped.
        while h.byte()? != 0x00 {
            let size = h.number()?;
            h.take(size)?;
        }
        id = h.byte()?;
    }
    if id == 0x03 {
        return Err(HeaderIssue::Damaged("additional streams"));
    }
    if id == 0x04 {
        walk_streams_info(&mut h)?;
        id = h.byte()?;
    }
    if id == 0x05 {
        bounded_items(h.number()?)?;
    }
    Ok(())
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = flate2::Crc::new();
    crc.update(bytes);
    crc.sum()
}

/// Decodes the encoded header described by `info` (the streams info after its `0x17` id)
/// from `file` (`len` bytes), within `MAX_7Z_HEADER` and `MAX_DECODER_WINDOW`. 7-Zip packs
/// headers with one LZMA coder (LZMA2 and copy are read too); an AES coder means the
/// header is encrypted.
fn decode_encoded_header(
    file: &mut (impl Read + Seek),
    len: u64,
    info: &[u8],
) -> std::io::Result<Vec<u8>> {
    use sevenz_rust2::EncoderMethod as M;
    let streams = walk_streams_info(&mut HeaderBytes { data: info, pos: 0 })?;
    let [folder] = streams.folders.as_slice() else {
        return Err(HeaderIssue::Damaged("encoded header blocks").into());
    };
    let coders = folder
        .coders
        .iter()
        .map(|(id, props)| (id.as_slice(), props.as_slice()));
    match sevenz_coders_refusal(coders) {
        Some(EntryRefusal::Encrypted) => {
            return Err(sevenz_refused(SevenZRefusal::EncryptedHeader))
        }
        Some(EntryRefusal::DictionaryTooLarge) => {
            return Err(sevenz_refused(SevenZRefusal::HeaderTooLarge))
        }
        _ => {}
    }
    let size = folder.unpack_sizes.last().copied().unwrap_or(0);
    if size > MAX_7Z_HEADER {
        return Err(sevenz_refused(SevenZRefusal::HeaderTooLarge));
    }
    let [(id, props)] = folder.coders.as_slice() else {
        return Err(HeaderIssue::Damaged("encoded header coders").into());
    };
    let packed = streams.pack_sizes.first().copied().unwrap_or(0);
    let at = 32u64
        .checked_add(streams.pack_pos)
        .filter(|at| at.saturating_add(packed) <= len)
        .ok_or(HeaderIssue::Damaged("encoded header past the end"))?;
    file.seek(SeekFrom::Start(at))?;
    let input = BufReader::new(file.take(packed));
    // The dictionary never needs to be larger than what it decodes.
    let dict = sevenz_coder_window(id, props)
        .unwrap_or(0)
        .min(size.max(4096)) as u32;
    let mut decoder: Box<dyn Read + '_> = if id.as_slice() == M::ID_LZMA {
        let lc_lp_pb = *props
            .first()
            .ok_or(HeaderIssue::Damaged("LZMA properties"))?;
        Box::new(lzma_rust2::LzmaReader::new_with_props(
            input, size, lc_lp_pb, dict, None,
        )?)
    } else if id.as_slice() == M::ID_LZMA2 {
        Box::new(lzma_rust2::Lzma2Reader::new(input, dict, None))
    } else if id.as_slice() == M::ID_COPY {
        Box::new(input)
    } else {
        return Err(HeaderIssue::Damaged("encoded header coder").into());
    };
    let mut header = Vec::new();
    (&mut decoder).take(size).read_to_end(&mut header)?;
    if header.len() as u64 != size {
        return Err(HeaderIssue::Damaged("truncated encoded header").into());
    }
    if folder.crc.is_some_and(|crc| crc != crc32(&header)) {
        return Err(HeaderIssue::Damaged("header CRC mismatch").into());
    }
    Ok(header)
}

/// The 7z file as the 7z reader sees it: the start header is rewritten to point at the
/// plain header FastTail decoded and checked, served after the end of the file, so the
/// reader never decodes a header itself nor guesses where one is. Packed data is read
/// from the file at its own offsets.
struct SevenZView<R> {
    file: R,
    len: u64,
    start: [u8; 32],
    header: Vec<u8>,
    pos: u64,
}

impl<R: Read + Seek> SevenZView<R> {
    fn new(file: R, len: u64, version: [u8; 2], header: Vec<u8>) -> Self {
        let mut start = [0u8; 32];
        start[..6].copy_from_slice(&SEVENZ_MAGIC);
        start[6..8].copy_from_slice(&version);
        start[12..20].copy_from_slice(&(len - 32).to_le_bytes());
        start[20..28].copy_from_slice(&(header.len() as u64).to_le_bytes());
        start[28..32].copy_from_slice(&crc32(&header).to_le_bytes());
        let crc = crc32(&start[12..32]);
        start[8..12].copy_from_slice(&crc.to_le_bytes());
        Self {
            file,
            len,
            start,
            header,
            pos: 0,
        }
    }
}

impl<R: Read + Seek> Read for SevenZView<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = if self.pos < 32 {
            let from = &self.start[self.pos as usize..];
            let n = from.len().min(buf.len());
            buf[..n].copy_from_slice(&from[..n]);
            n
        } else if self.pos < self.len {
            let room = ((self.len - self.pos).min(buf.len() as u64)) as usize;
            self.file.seek(SeekFrom::Start(self.pos))?;
            self.file.read(&mut buf[..room])?
        } else {
            let from = self
                .header
                .get((self.pos - self.len) as usize..)
                .unwrap_or(&[]);
            let n = from.len().min(buf.len());
            buf[..n].copy_from_slice(&from[..n]);
            n
        };
        self.pos += n as u64;
        Ok(n)
    }
}

impl<R: Read + Seek> Seek for SevenZView<R> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let end = self.len + self.header.len() as u64;
        let to = match pos {
            SeekFrom::Start(n) => Some(n),
            SeekFrom::End(d) => end.checked_add_signed(d),
            SeekFrom::Current(d) => self.pos.checked_add_signed(d),
        };
        self.pos = to.ok_or_else(|| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
        Ok(self.pos)
    }
}

fn sevenz_err(e: sevenz_rust2::Error) -> std::io::Error {
    use sevenz_rust2::Error as E;
    match e {
        E::PasswordRequired | E::MaybeBadPassword(_) => {
            sevenz_refused(SevenZRefusal::EncryptedHeader)
        }
        E::Io(e, context) if context.is_empty() => e,
        E::MaxMemLimited { .. } => sevenz_refused(SevenZRefusal::HeaderTooLarge),
        e => std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()),
    }
}

/// Reads the header of the 7z archive at `path` within the listing bounds. The start
/// header must be intact (its CRC is checked): a damaged one is refused, never guessed
/// from the end of the file. The header (decoded here when it is encoded) is checked and
/// walked for its counts before the 7z reader parses it.
fn read_7z_archive(path: &Path) -> std::io::Result<sevenz_rust2::Archive> {
    let file = crate::file_source::open_file_shared(path)?;
    let len = file.metadata()?.len();
    let mut reader = BufReader::new(file);
    let mut start = [0u8; 32];
    reader
        .read_exact(&mut start)
        .map_err(|_| HeaderIssue::Damaged("shorter than its start header"))?;
    if start[..6] != SEVENZ_MAGIC || start[6] != 0 {
        return Err(HeaderIssue::Damaged("unknown signature or version").into());
    }
    let field = |at: usize| {
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&start[at..at + 8]);
        u64::from_le_bytes(bytes)
    };
    let word =
        |at: usize| u32::from_le_bytes([start[at], start[at + 1], start[at + 2], start[at + 3]]);
    if word(8) != crc32(&start[12..32]) {
        return Err(HeaderIssue::Damaged("start header CRC mismatch").into());
    }
    let (offset, size, header_crc) = (field(12), field(20), word(28));
    if size == 0 {
        // An archive holding nothing.
        return Ok(sevenz_rust2::Archive::default());
    }
    if size > MAX_7Z_HEADER {
        return Err(sevenz_refused(SevenZRefusal::HeaderTooLarge));
    }
    let at = 32u64
        .checked_add(offset)
        .filter(|at| at.saturating_add(size) <= len)
        .ok_or(HeaderIssue::Damaged("header past the end of the file"))?;
    reader.seek(SeekFrom::Start(at))?;
    let mut header = vec![0u8; size as usize];
    reader.read_exact(&mut header)?;
    if crc32(&header) != header_crc {
        return Err(HeaderIssue::Damaged("header CRC mismatch").into());
    }
    let header = match header[0] {
        0x17 => decode_encoded_header(&mut reader, len, &header[1..])?,
        0x01 => header,
        _ => return Err(HeaderIssue::Damaged("no header").into()),
    };
    walk_header(&header)?;
    let mut view = SevenZView::new(reader, len, [start[6], start[7]], header);
    sevenz_rust2::Archive::read(&mut view, &sevenz_rust2::Password::empty()).map_err(sevenz_err)
}

/// The last 7z archives read, by path, reused while their size and date hold: the
/// listing, the open and the extraction job of an entry read the header once.
fn sevenz_cache() -> &'static Mutex<StampedLru<Arc<sevenz_rust2::Archive>>> {
    static CACHE: OnceLock<Mutex<StampedLru<Arc<sevenz_rust2::Archive>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(StampedLru::new(4)))
}

/// The parsed header of the 7z archive at `path`, from the cache or read now.
fn load_7z(path: &Path) -> std::io::Result<Arc<sevenz_rust2::Archive>> {
    let when = stamp(path);
    if let Some(when) = when {
        if let Some(archive) = sevenz_cache()
            .lock()
            .ok()
            .and_then(|mut c| c.get(path, when))
        {
            return Ok(archive);
        }
    }
    let archive = Arc::new(read_7z_archive(path)?);
    if let (Some(when), Ok(mut cache)) = (when, sevenz_cache().lock()) {
        cache.insert(path, when, archive.clone());
    }
    Ok(archive)
}

/// True for a 7z entry listed as a file: not a directory, not an anti-item (a deletion
/// marker of an update), and holding data.
fn is_7z_file(file: &sevenz_rust2::ArchiveEntry) -> bool {
    !file.is_directory && !file.is_anti_item && file.has_stream
}

/// The block holding the data of file `index`, if any.
fn sevenz_block_of(archive: &sevenz_rust2::Archive, index: usize) -> Option<usize> {
    archive
        .stream_map
        .file_block_index
        .get(index)
        .copied()
        .flatten()
        .filter(|&b| b < archive.blocks.len())
}

/// The file entries of the 7z archive at `path` (directories, anti-items and empty
/// entries skipped), in archive order, at most `MAX_7Z_ENTRIES`.
pub fn list_7z_entries(path: &Path) -> std::io::Result<SevenZListing> {
    list_7z(path, MAX_7Z_ENTRIES)
}

fn list_7z(path: &Path, max_entries: usize) -> std::io::Result<SevenZListing> {
    let archive = load_7z(path)?;
    let mut per_block = vec![0usize; archive.blocks.len()];
    for index in 0..archive.files.len() {
        if let Some(b) = sevenz_block_of(&archive, index) {
            per_block[b] += 1;
        }
    }
    let first_pack = archive.stream_map.block_first_pack_stream_index();
    let mut keys = HashSet::new();
    let mut entries = Vec::new();
    for (index, file) in archive.files.iter().enumerate() {
        if !is_7z_file(file) {
            continue;
        }
        if entries.len() >= max_entries {
            return Ok(SevenZListing {
                entries,
                partial: true,
            });
        }
        let block = sevenz_block_of(&archive, index);
        let (refusal, compressed_size, block_size) = match block {
            Some(b) => {
                let unpacked = archive.blocks[b].get_unpack_size();
                let packed = first_pack
                    .get(b)
                    .and_then(|&p| archive.pack_sizes().get(p))
                    .copied()
                    .unwrap_or(0);
                let solid = per_block[b] > 1;
                let share = if solid && unpacked > 0 {
                    (packed as u128 * file.size as u128 / unpacked as u128) as u64
                } else {
                    packed
                };
                (
                    sevenz_block_refusal(&archive.blocks[b]),
                    share,
                    solid.then_some(unpacked),
                )
            }
            None => (None, 0, None),
        };
        let refusal = refusal.or_else(|| claim_name(&file.name, &mut keys));
        entries.push(ArchiveEntryInfo {
            name: file.name.clone(),
            size: file.size,
            compressed_size,
            block_size,
            offset: None,
            refusal,
        });
    }
    Ok(SevenZListing {
        entries,
        partial: false,
    })
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

/// The kind of archive whose entries are chosen in the picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveKind {
    Zip,
    /// A tar, plain (`None`) or inside a codec file.
    Tar(Option<Codec>),
    SevenZ,
}

/// The archive kind of the file at `path`: a zip, a 7z, a plain tar, or a codec file
/// whose first decompressed bytes are a tar header. `None` for anything else.
pub fn archive_kind(path: &Path) -> Option<ArchiveKind> {
    match sniff(path) {
        Format::Zip => Some(ArchiveKind::Zip),
        Format::SevenZ => Some(ArchiveKind::SevenZ),
        Format::Tar => Some(ArchiveKind::Tar(None)),
        Format::Compressed(codec) if codec_holds_tar(path, codec) => {
            Some(ArchiveKind::Tar(Some(codec)))
        }
        _ => None,
    }
}

/// The archive kind of `path` as the ancestor of an entry path, from its raw magic bytes
/// alone (nothing is decompressed): any codec file may hold a tar, and the extraction job
/// finds out. Session saves and the config load call this for every stream.
fn entry_ancestor_kind(path: &Path) -> Option<ArchiveKind> {
    match sniff(path) {
        Format::Zip => Some(ArchiveKind::Zip),
        Format::SevenZ => Some(ArchiveKind::SevenZ),
        Format::Tar => Some(ArchiveKind::Tar(None)),
        Format::Compressed(codec) => Some(ArchiveKind::Tar(Some(codec))),
        _ => None,
    }
}

/// Splits an entry path into the archive and the entry name: the nearest ancestor of
/// `path` that is a regular file must be a zip, a 7z or a tar (plain or compressed).
/// `None` for any other path.
pub fn split_entry_path(path: &Path) -> Option<(PathBuf, String)> {
    split_entry_path_kind(path).map(|(archive, entry, _)| (archive, entry))
}

fn split_entry_path_kind(path: &Path) -> Option<(PathBuf, String, ArchiveKind)> {
    if path.is_file() {
        return None;
    }
    let archive = path.ancestors().skip(1).find(|a| a.is_file())?;
    let kind = entry_ancestor_kind(archive)?;
    let rel = path.strip_prefix(archive).ok()?;
    let entry = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/");
    if entry.is_empty() {
        return None;
    }
    Some((archive.to_path_buf(), entry, kind))
}

/// True when `path` is an existing file or the entry path of an existing archive.
pub fn source_exists(path: &Path) -> bool {
    path.is_file() || split_entry_path(path).is_some()
}

/// What opening `path` means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A plain file (or anything the engine opens as it is).
    Plain,
    /// A single compressed file (not a tar).
    Compressed(Codec),
    /// A zip archive: its entries still have to be chosen.
    ZipArchive,
    EmptyZip,
    /// A tar archive, plain or compressed: its entries still have to be chosen.
    TarArchive(Option<Codec>),
    /// A 7z archive: its entries still have to be chosen.
    SevenZArchive,
    /// One entry of a zip, tar or 7z archive.
    Entry {
        archive: PathBuf,
        entry: String,
        kind: ArchiveKind,
    },
}

/// Classifies `path` by its content (for a file) or as an archive entry path.
pub fn classify(path: &Path) -> Target {
    if path.is_file() {
        return match sniff(path) {
            Format::Compressed(codec) if codec_holds_tar(path, codec) => {
                Target::TarArchive(Some(codec))
            }
            Format::Compressed(codec) => Target::Compressed(codec),
            // A text file that merely starts with `PK\x03\x04` is not a zip: when the
            // central directory does not parse, the file opens as it is.
            Format::Zip if list_zip_entries(path).is_ok() => Target::ZipArchive,
            Format::Zip => Target::Plain,
            Format::EmptyZip => Target::EmptyZip,
            Format::Tar => Target::TarArchive(None),
            // The signature holds bytes no text starts with: a 7z that does not list is
            // reported as such, not opened as text.
            Format::SevenZ => Target::SevenZArchive,
            Format::Plain => Target::Plain,
        };
    }
    match split_entry_path_kind(path) {
        Some((archive, entry, kind)) => Target::Entry {
            archive,
            entry,
            kind,
        },
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
    /// A single compressed file turned out to hold a tar archive (a fallback: `classify`
    /// sends such a file to the entry picker).
    Tar,
    /// The tar archive no longer holds the entry.
    NoSuchEntry,
    /// The tar entry is one that cannot be opened (a link, a device, a sparse file).
    Refused(EntryRefusal),
    /// The xz dictionary or the zstd window is larger than `MAX_DECODER_WINDOW`.
    WindowTooLarge,
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
    /// A single gzip, bzip2, xz or zstd file.
    Compressed(PathBuf, Codec),
    ZipEntry {
        archive: PathBuf,
        entry: String,
    },
    /// An entry of a tar, plain or inside `codec`. `offset` is where its headers start
    /// in a plain tar, when a scan indexed it.
    TarEntry {
        archive: PathBuf,
        codec: Option<Codec>,
        entry: String,
        offset: Option<u64>,
    },
    /// An entry of a 7z archive: its block is decoded from the start, the entries before
    /// it dropped.
    SevenZEntry {
        archive: PathBuf,
        entry: String,
    },
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

/// Counts the bytes read through it into `counter` (a seek sets it to the new position),
/// and fails every read once `cancel` is set, so a long walk over tar data that nobody
/// reads (the entries before the wanted one) stops at once.
struct CountingReader<'c, R> {
    inner: R,
    counter: &'c AtomicU64,
    cancel: Option<&'c AtomicBool>,
}

impl<'c, R> CountingReader<'c, R> {
    fn new(inner: R, counter: &'c AtomicU64, cancel: Option<&'c AtomicBool>) -> Self {
        Self {
            inner,
            counter,
            cancel,
        }
    }
}

impl<R: Read> Read for CountingReader<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
            return Err(std::io::Error::other("cancelled"));
        }
        let n = self.inner.read(buf)?;
        self.counter.fetch_add(n as u64, Ordering::Relaxed);
        Ok(n)
    }
}

impl<R: Seek> Seek for CountingReader<'_, R> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let at = self.inner.seek(pos)?;
        self.counter.store(at, Ordering::Relaxed);
        Ok(at)
    }
}

/// Opens `path` for a job and records its size as the progress total.
fn open_counted(path: &Path, shared: &Shared) -> std::io::Result<File> {
    let file = crate::file_source::open_file_shared(path)?;
    let total = file.metadata().map(|m| m.len()).unwrap_or(0);
    shared.total.store(total, Ordering::Relaxed);
    Ok(file)
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
        JobSource::Compressed(path, codec) => {
            let file = match open_counted(path, shared) {
                Ok(f) => f,
                Err(e) => return failed(&e),
            };
            let counted = CountingReader::new(file, &shared.consumed, None);
            // Concatenated members (`cat a.gz b.gz`, some rotators) are one stream.
            let decoder = open_decoder(*codec, BufReader::new(counted));
            let state = pump(decoder, out, env, true);
            if state == JobState::Stopped(StopReason::Tar) {
                remember_holds_tar(path);
            }
            state
        }
        JobSource::TarEntry {
            archive,
            codec,
            entry,
            offset,
        } => run_tar_entry(archive, *codec, entry, *offset, out, env),
        JobSource::SevenZEntry { archive, entry } => run_7z_entry(archive, entry, out, env),
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
            pump_entry(scaled, out, env)
        }
    }
}

/// True for a tar entry type that holds file data FastTail can read.
fn is_regular(kind: tar::EntryType) -> bool {
    kind.is_file() || kind.is_contiguous()
}

/// Why a tar entry (not a directory) cannot be opened, before the name checks.
fn tar_type_refusal(kind: tar::EntryType) -> Option<EntryRefusal> {
    if kind.is_gnu_sparse() {
        Some(EntryRefusal::Sparse)
    } else if !is_regular(kind) {
        Some(EntryRefusal::LinkOrSpecial)
    } else {
        None
    }
}

/// True for a tar member that is not a listed entry: a directory (also an old-style one,
/// a regular entry named `dir/`) or a GNU volume label.
fn is_tar_structure(kind: tar::EntryType, name: &str) -> bool {
    kind.is_dir() || kind == tar::EntryType::new(b'V') || name.ends_with('/')
}

/// Largest GNU long-name / long-link or pax extension header read. The `tar` crate reads
/// them whole into memory with no bound, so a header declaring gigabytes would abort the
/// process: FastTail walks the headers itself and treats a larger one as damage.
pub const MAX_TAR_EXTENSION: u64 = 64 * 1024;

fn damaged(what: impl Into<String>) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, what.into())
}

/// How a tar walk moves past data it does not read.
trait Skip: Read {
    fn skip(&mut self, bytes: u64) -> std::io::Result<()>;
}

/// A decompressed tar: skipped data is read and dropped (and must all be there).
struct ReadSkip<R>(R);

impl<R: Read> Read for ReadSkip<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(buf)
    }
}

impl<R: Read> Skip for ReadSkip<R> {
    fn skip(&mut self, bytes: u64) -> std::io::Result<()> {
        let skipped = std::io::copy(&mut (&mut self.0).take(bytes), &mut std::io::sink())?;
        if skipped < bytes {
            return Err(damaged("the tar archive is truncated"));
        }
        Ok(())
    }
}

/// A plain tar file: skipped data is seeked over, never read. A seek past the end of the
/// file is truncation (a seek alone would not notice it).
struct SeekSkip<R> {
    inner: R,
    len: u64,
}

impl<R: Read> Read for SeekSkip<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.inner.read(buf)
    }
}

impl<R: Read + Seek> Skip for SeekSkip<R> {
    fn skip(&mut self, bytes: u64) -> std::io::Result<()> {
        let at = self.inner.seek(SeekFrom::Current(bytes as i64))?;
        if at > self.len {
            return Err(damaged("the tar archive is truncated"));
        }
        Ok(())
    }
}

/// One member of a tar, its extension headers resolved.
struct TarMember {
    name: String,
    kind: tar::EntryType,
    /// Bytes of data stored in the archive (for a sparse file, not its expanded size).
    size: u64,
    /// Where the headers of this member start (its first extension header), relative to
    /// where the walk started.
    start: u64,
}

/// A tar walk that reads every header itself: GNU long names and pax paths are resolved
/// with `MAX_TAR_EXTENSION` as the bound, pax sizes and GNU sparse extension blocks are
/// honoured, and a checksum mismatch or data cut short is an error (damage).
struct TarWalker<R> {
    reader: R,
    /// Position in the tar stream, relative to where the walk started.
    pos: u64,
    /// Padded data of the last member not read yet (skipped before the next header).
    pending: u64,
}

fn padded(size: u64) -> u64 {
    size.div_ceil(TAR_BLOCK) * TAR_BLOCK
}

impl<R: Skip> TarWalker<R> {
    fn new(reader: R) -> Self {
        Self {
            reader,
            pos: 0,
            pending: 0,
        }
    }

    /// One 512-byte block; `None` at a clean end of the stream.
    fn block(&mut self) -> std::io::Result<Option<[u8; 512]>> {
        let mut block = [0u8; 512];
        match read_full(&mut self.reader, &mut block)? {
            0 => Ok(None),
            512 => {
                self.pos += TAR_BLOCK;
                Ok(Some(block))
            }
            _ => Err(damaged("the tar archive is truncated")),
        }
    }

    /// The data of an extension header, at most `MAX_TAR_EXTENSION` bytes.
    fn extension(&mut self, size: u64) -> std::io::Result<Vec<u8>> {
        if size > MAX_TAR_EXTENSION {
            return Err(damaged(format!(
                "tar extension header of {size} bytes (at most {MAX_TAR_EXTENSION})"
            )));
        }
        let mut data = vec![0u8; padded(size) as usize];
        if read_full(&mut self.reader, &mut data)? < data.len() {
            return Err(damaged("the tar archive is truncated"));
        }
        self.pos += data.len() as u64;
        data.truncate(size as usize);
        Ok(data)
    }

    /// The next member, skipping the data of the previous one; `None` at the end.
    fn next_member(&mut self) -> std::io::Result<Option<TarMember>> {
        if self.pending > 0 {
            self.reader.skip(self.pending)?;
            self.pos += self.pending;
            self.pending = 0;
        }
        let start = self.pos;
        let mut long_name: Option<Vec<u8>> = None;
        let mut pax_path: Option<Vec<u8>> = None;
        let mut pax_size: Option<u64> = None;
        loop {
            let Some(block) = self.block()? else {
                return if long_name.is_some() || pax_path.is_some() || pax_size.is_some() {
                    Err(damaged("the tar archive is truncated"))
                } else {
                    Ok(None)
                };
            };
            // Two zero blocks end the archive; one is enough to stop reading.
            if block.iter().all(|&b| b == 0) {
                return Ok(None);
            }
            let header = tar::Header::from_byte_slice(&block);
            let sum: u32 = block[..148]
                .iter()
                .chain(&block[156..])
                .map(|&b| b as u32)
                .sum::<u32>()
                + 8 * 32;
            if header.cksum()? != sum {
                return Err(damaged("tar header checksum mismatch"));
            }
            let kind = header.entry_type();
            let size = header.entry_size()?;
            if kind.is_gnu_longname() {
                let mut name = self.extension(size)?;
                if let Some(end) = name.iter().position(|&b| b == 0) {
                    name.truncate(end);
                }
                long_name = Some(name);
                continue;
            }
            if kind.is_gnu_longlink() || kind.is_pax_global_extensions() {
                self.extension(size)?;
                continue;
            }
            if kind.is_pax_local_extensions() {
                let records = self.extension(size)?;
                for record in tar::PaxExtensions::new(&records) {
                    let record = record.map_err(|_| damaged("damaged pax header"))?;
                    match record.key_bytes() {
                        b"path" => pax_path = Some(record.value_bytes().to_vec()),
                        b"size" => {
                            pax_size = std::str::from_utf8(record.value_bytes())
                                .ok()
                                .and_then(|v| v.parse().ok());
                        }
                        _ => {}
                    }
                }
                continue;
            }
            let mut stored = pax_size.unwrap_or(size);
            // An old-style GNU sparse file is followed by extension blocks of its map
            // before the data; its size field already is the data stored.
            if kind.is_gnu_sparse() {
                let mut extended = block[482] != 0;
                while extended {
                    let Some(ext) = self.block()? else {
                        return Err(damaged("the tar archive is truncated"));
                    };
                    extended = ext[504] != 0;
                }
                stored = size;
            }
            let name = long_name
                .filter(|n| !n.is_empty())
                .or(pax_path)
                .unwrap_or_else(|| header.path_bytes().to_vec());
            self.pending = padded(stored);
            return Ok(Some(TarMember {
                name: String::from_utf8_lossy(&name).to_string(),
                kind,
                size: stored,
                start,
            }));
        }
    }

    /// The data of the member `next_member` just returned.
    fn data(&mut self, member: &TarMember) -> impl Read + '_ {
        self.pending = 0;
        (&mut self.reader).take(member.size)
    }
}

/// Extracts the tar entry `entry` of `archive`. A plain tar whose scan indexed the entry
/// seeks to it and checks that the header there still names it; otherwise (a compressed
/// tar, a restored stream, a changed archive) the archive is read from the start and the
/// data of the entries before it is skipped.
fn run_tar_entry(
    archive: &Path,
    codec: Option<Codec>,
    entry: &str,
    offset: Option<u64>,
    out: File,
    env: &JobEnv,
) -> JobState {
    let key = entry_key(entry);
    let shared = env.shared;
    if let (None, Some(offset)) = (codec, offset) {
        if let Ok(file) = crate::file_source::open_file_shared(archive) {
            let len = file.metadata().map(|m| m.len()).unwrap_or(0);
            let mut reader = BufReader::new(file);
            if reader.seek(SeekFrom::Start(offset)).is_ok() {
                let mut walker = TarWalker::new(SeekSkip { inner: reader, len });
                if let Ok(Some(found)) = walker.next_member() {
                    if entry_key(&found.name) == key
                        && is_regular(found.kind)
                        && !is_unsafe_name(&found.name)
                    {
                        // Progress is the share of the entry copied.
                        shared.total.store(found.size.max(1), Ordering::Relaxed);
                        let data = walker.data(&found);
                        let counted = CountingReader::new(data, &shared.consumed, None);
                        return pump_entry(counted, out, env);
                    }
                }
            }
        }
    }
    shared.consumed.store(0, Ordering::Relaxed);
    let file = match open_counted(archive, shared) {
        Ok(f) => f,
        Err(e) => return JobState::Stopped(StopReason::Failed(e.to_string())),
    };
    let len = shared.total.load(Ordering::Relaxed);
    let counted = CountingReader::new(BufReader::new(file), &shared.consumed, Some(env.cancel));
    match codec {
        None => walk_to_entry(
            TarWalker::new(SeekSkip {
                inner: counted,
                len,
            }),
            &key,
            out,
            env,
        ),
        Some(codec) => walk_to_entry(
            TarWalker::new(ReadSkip(open_decoder(codec, BufReader::new(counted)))),
            &key,
            out,
            env,
        ),
    }
}

/// Walks the tar headers to the first openable entry whose stream path is `key` and
/// extracts it. The first match wins, as in the picker; a match that cannot be opened is
/// reported only when no later one can.
fn walk_to_entry<R: Skip>(
    mut walker: TarWalker<R>,
    key: &str,
    out: File,
    env: &JobEnv,
) -> JobState {
    let mut refused = None;
    loop {
        let found = match walker.next_member() {
            Ok(Some(found)) => found,
            Ok(None) => break,
            Err(e) => return read_failure(e, env),
        };
        if is_tar_structure(found.kind, &found.name) || entry_key(&found.name) != key {
            continue;
        }
        let refusal = if is_unsafe_name(&found.name) {
            Some(EntryRefusal::UnsafeName)
        } else {
            tar_type_refusal(found.kind)
        };
        match refusal {
            None => return pump_entry(walker.data(&found), out, env),
            Some(refusal) => {
                refused.get_or_insert(refusal);
            }
        }
    }
    if env.cancel.load(Ordering::Relaxed) {
        return JobState::Stopped(StopReason::Cancelled);
    }
    JobState::Stopped(match refused {
        Some(refusal) => StopReason::Refused(refusal),
        None => StopReason::NoSuchEntry,
    })
}

/// Extracts the 7z entry `entry` of `archive`: the first openable entry with that stream
/// path, as in the picker. Its block is decoded from the start; the entries before it are
/// decoded and dropped, never written, and the progress is the share of the block
/// decoded, so it moves while they are skipped.
fn run_7z_entry(path: &Path, entry: &str, out: File, env: &JobEnv) -> JobState {
    let shared = env.shared;
    let opened = load_7z(path).and_then(|archive| {
        let file = crate::file_source::open_file_shared(path)?;
        Ok((archive, BufReader::new(file)))
    });
    let (archive, mut reader) = match opened {
        Ok(opened) => opened,
        Err(e) => {
            return JobState::Stopped(match sevenz_refusal(&e) {
                Some(SevenZRefusal::EncryptedHeader) => {
                    StopReason::Refused(EntryRefusal::Encrypted)
                }
                _ => StopReason::Failed(e.to_string()),
            })
        }
    };
    let key = entry_key(entry);
    let mut refused = None;
    let mut found = None;
    for (index, file) in archive.files.iter().enumerate() {
        if !is_7z_file(file) || entry_key(&file.name) != key {
            continue;
        }
        let block = sevenz_block_of(&archive, index);
        let refusal = if is_unsafe_name(&file.name) {
            Some(EntryRefusal::UnsafeName)
        } else {
            block.and_then(|b| sevenz_block_refusal(&archive.blocks[b]))
        };
        match refusal {
            None => {
                found = Some((index, block));
                break;
            }
            Some(refusal) => {
                refused.get_or_insert(refusal);
            }
        }
    }
    let Some((index, block)) = found else {
        return JobState::Stopped(match refused {
            Some(refusal) => StopReason::Refused(refusal),
            None => StopReason::NoSuchEntry,
        });
    };
    let Some(block) = block else {
        // No data stream: nothing to write.
        return JobState::Done;
    };
    shared.consumed.store(0, Ordering::Relaxed);
    shared.total.store(
        archive.blocks[block].get_unpack_size().max(1),
        Ordering::Relaxed,
    );
    let target: *const sevenz_rust2::ArchiveEntry = &archive.files[index];
    let password = sevenz_rust2::Password::empty();
    let mut out = Some(out);
    let mut state = None;
    // One decoder thread: an LZMA2 block written by several threads would otherwise
    // decode on as many.
    let decoded = sevenz_rust2::BlockDecoder::new(1, block, &archive, &password, &mut reader)
        .for_each_entries(&mut |file, data| {
            let mut counted = CountingReader::new(data, &shared.consumed, Some(env.cancel));
            if std::ptr::eq(file, target) {
                if let Some(out) = out.take() {
                    state = Some(pump_entry(counted, out, env));
                }
                return Ok(false);
            }
            std::io::copy(&mut counted, &mut std::io::sink())?;
            Ok(true)
        });
    match (state, decoded) {
        (Some(state), _) => state,
        (None, Err(e)) => read_failure(sevenz_err(e), env),
        (None, Ok(_)) => JobState::Stopped(StopReason::NoSuchEntry),
    }
}

/// The state a read error ends a job in: a cancel shows as such, not as a failure.
fn read_failure(e: std::io::Error, env: &JobEnv) -> JobState {
    JobState::Stopped(if env.cancel.load(Ordering::Relaxed) {
        StopReason::Cancelled
    } else if is_window_too_large(&e) {
        StopReason::WindowTooLarge
    } else {
        StopReason::Failed(e.to_string())
    })
}

/// `reader`, decompressed once more when its first bytes are gzip, bzip2, xz or zstd (a
/// rotated `app.log.1.gz` inside a bundle). A zip, tar or 7z inside is left as it is.
fn nested<'a, R: Read + 'a>(mut reader: R) -> std::io::Result<Box<dyn Read + 'a>> {
    let mut head = vec![0u8; SNIFF_BYTES];
    let n = read_full(&mut reader, &mut head)?;
    head.truncate(n);
    let codec = match sniff_bytes(&head) {
        Format::Compressed(codec) => Some(codec),
        _ => None,
    };
    let whole = std::io::Cursor::new(head).chain(reader);
    Ok(match codec {
        Some(codec) => open_decoder(codec, BufReader::new(whole)),
        None => Box::new(whole),
    })
}

/// Extracts an archive entry (zip, tar or 7z) into the spool, decompressing a nested codec.
fn pump_entry(reader: impl Read, out: File, env: &JobEnv) -> JobState {
    match nested(reader) {
        Ok(reader) => pump(reader, out, env, false),
        Err(e) => read_failure(e, env),
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
            Err(e) => return read_failure(e, env),
        };
        if n == 0 {
            return JobState::Done;
        }
        if first {
            first = false;
            if detect_tar && is_tar_head(&buf[..n]) {
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
    /// The archive on disk and, for an archive entry, the entry: its `/`-separated
    /// identity (`normalize_entry`), the name `path` and saved sessions know it by.
    pub archive: PathBuf,
    pub entry: Option<String>,
    /// The entry name as the archive spells it, which the job looks up.
    pub real_entry: Option<String>,
    pub settings: Settings,
    /// What the job (and every reload) reads.
    source: JobSource,
    job: DecompressJob,
    spool: SpoolFile,
    wake: Option<WakeFn>,
    /// Bookmarks to restore once the index covers them (a restored stream starts empty),
    /// with their notes.
    pub pending_bookmarks: Vec<usize>,
    pub pending_bookmark_notes: std::collections::BTreeMap<usize, String>,
    /// The end of the job has been seen by the engine: final size indexed, encoding
    /// detection settled, pending bookmarks applied.
    finalized: bool,
}

impl CompressedStream {
    /// `entry`: for an archive entry, its identity and its name as the archive spells it.
    fn start(
        archive: &Path,
        entry: Option<(String, String)>,
        source: JobSource,
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
            source,
            job: DecompressJob::idle(),
            spool,
            wake,
            pending_bookmarks: Vec::new(),
            pending_bookmark_notes: std::collections::BTreeMap::new(),
            finalized: false,
        };
        stream.job = stream.new_job(out);
        stream
    }

    fn new_job(&self, out: File) -> DecompressJob {
        DecompressJob::start(
            self.source.clone(),
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
    /// The archive entry does not fit on the spool volume (volume, bytes needed).
    NotEnoughSpace {
        volume: String,
        needed: u64,
    },
    Refused(EntryRefusal),
    /// The archive holds no such entry.
    NoSuchEntry,
}

impl From<std::io::Error> for OpenError {
    fn from(e: std::io::Error) -> Self {
        OpenError::Io(e)
    }
}

/// Spool name for a compressed file or entry: its last name without the codec suffix, so
/// `notes.md.gz` or `notes.md.zst` still opens in the Markdown view.
fn inner_name(name: &str) -> String {
    let name = name
        .rsplit(['/', '\\'])
        .find(|p| !p.is_empty())
        .unwrap_or(name);
    let lower = name.to_ascii_lowercase();
    for suffix in [".gz", ".gzip", ".bz2", ".xz", ".zst", ".zstd"] {
        if lower.ends_with(suffix) && name.len() > suffix.len() {
            return name[..name.len() - suffix.len()].to_string();
        }
    }
    name.to_string()
}

/// The entry of `entries` whose stream path is the one of `entry` (however either spells
/// its separators): the one that opens when several share it, else the first.
fn find_entry<'e>(entries: &'e [ArchiveEntryInfo], entry: &str) -> Option<&'e ArchiveEntryInfo> {
    let key = entry_key(entry);
    let mut matching = entries
        .iter()
        .filter(|e| e.refusal != Some(EntryRefusal::DuplicateName) && entry_key(&e.name) == key);
    let first = matching.next()?;
    if first.refusal.is_none() {
        return Some(first);
    }
    Some(matching.find(|e| e.refusal.is_none()).unwrap_or(first))
}

/// Refuses up front an entry whose (exact) size cannot fit on the spool volume, before
/// any spool is created.
fn check_space(settings: &Settings, size: u64) -> Result<(), OpenError> {
    crate::spool::ensure_dir(&settings.spool_dir)?;
    if let Some((free, volume)) = free_space(&settings.spool_dir) {
        let needed = size.min(settings.limits.max_output);
        if free < needed.saturating_add(settings.limits.min_free) {
            return Err(OpenError::NotEnoughSpace { volume, needed });
        }
    }
    Ok(())
}

/// Opens a decompressed stream: `entry` is `None` for a single compressed file, the entry
/// name for a zip, tar or 7z (any spelling of its stream path). The engine starts on an empty
/// spool with follow off; the job fills the spool in the background and the engine's poll
/// indexes it as it grows.
pub fn open_engine(
    archive: &Path,
    entry: Option<&str>,
    settings: &Settings,
    wake: Option<WakeFn>,
) -> Result<TailEngine, OpenError> {
    // The entry name as the archive spells it, for the job.
    let mut real_entry = None;
    let source = match entry {
        Some(entry) => match entry_ancestor_kind(archive) {
            Some(ArchiveKind::Zip) => {
                let entries = list_zip_entries(archive)?;
                let info = find_entry(&entries, entry).ok_or(OpenError::NoSuchEntry)?;
                if let Some(refusal) = &info.refusal {
                    return Err(OpenError::Refused(refusal.clone()));
                }
                // Zip sizes are exact.
                check_space(settings, info.size)?;
                real_entry = Some(info.name.clone());
                JobSource::ZipEntry {
                    archive: archive.to_path_buf(),
                    entry: info.name.clone(),
                }
            }
            Some(ArchiveKind::SevenZ) => {
                // Every entry, not only the picker's first `MAX_7Z_ENTRIES`: a restored
                // stream may name one further down.
                let listing = list_7z(archive, usize::MAX)?;
                let info = find_entry(&listing.entries, entry).ok_or(OpenError::NoSuchEntry)?;
                if let Some(refusal) = &info.refusal {
                    return Err(OpenError::Refused(refusal.clone()));
                }
                // 7z sizes are exact.
                check_space(settings, info.size)?;
                real_entry = Some(info.name.clone());
                JobSource::SevenZEntry {
                    archive: archive.to_path_buf(),
                    entry: info.name.clone(),
                }
            }
            Some(ArchiveKind::Tar(codec)) => {
                // A scan of this archive (the picker's, running or finished) gives the
                // size, the refusal and, for a plain tar, where the entry is.
                let info = cached_tar_entry(archive, entry)?;
                if let Some(info) = &info {
                    if let Some(refusal) = &info.refusal {
                        return Err(OpenError::Refused(refusal.clone()));
                    }
                    check_space(settings, info.size)?;
                }
                let name = info
                    .as_ref()
                    .map_or_else(|| entry.to_string(), |i| i.name.clone());
                real_entry = Some(name.clone());
                JobSource::TarEntry {
                    archive: archive.to_path_buf(),
                    codec,
                    entry: name,
                    offset: info.and_then(|i| i.offset),
                }
            }
            None => return Err(OpenError::NoSuchEntry),
        },
        None => {
            // A file that is not compressed at all fails in the job with the gzip
            // decoder's message, as it always did.
            let codec = match sniff(archive) {
                Format::Compressed(codec) => codec,
                _ => Codec::Gzip,
            };
            JobSource::Compressed(archive.to_path_buf(), codec)
        }
    };
    let spool_name = match &real_entry {
        Some(name) => inner_name(name),
        None => inner_name(&archive.file_name().unwrap_or_default().to_string_lossy()),
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
        archive, entry, source, settings, spool, out, wake,
    ));
    Ok(engine)
}

/// Bounds of a tar scan (see the "Tar Archive Entry Picker" requirement).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanLimits {
    /// Entries listed at most (about 100 bytes each in the picker).
    pub max_entries: usize,
    /// Decompressed bytes read at most (a compressed tar is decoded to be listed).
    pub max_decoded: u64,
}

impl Default for ScanLimits {
    fn default() -> Self {
        Self {
            max_entries: 100_000,
            max_decoded: MAX_MAX_GB as u64 * 1024 * 1024 * 1024,
        }
    }
}

/// State of a tar scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanState {
    Running,
    /// Every header was read.
    Done,
    /// A bound of `ScanLimits` was reached: the list is partial.
    LimitReached,
    /// A damaged header or truncated data ended the scan: the list stops there.
    Damaged(String),
    /// The scan was stopped by the user or the picker closed.
    Cancelled,
    /// The archive could not be read at all.
    Failed(String),
}

impl ScanState {
    /// True once the scan will not list anything more.
    pub fn is_over(&self) -> bool {
        *self != ScanState::Running
    }
}

struct ScanShared {
    entries: Mutex<Vec<ArchiveEntryInfo>>,
    /// Archive bytes read (compressed for a compressed tar) and the archive size.
    consumed: AtomicU64,
    total: AtomicU64,
    state: Mutex<ScanState>,
}

impl ScanShared {
    fn state(&self) -> ScanState {
        self.state
            .lock()
            .map(|s| s.clone())
            .unwrap_or(ScanState::Failed("poisoned".into()))
    }
}

/// Size and modification time of a file: a cached scan is reused while they hold.
type Stamp = (u64, Option<SystemTime>);

fn stamp(path: &Path) -> Option<Stamp> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.len(), meta.modified().ok()))
}

/// Scans kept: a picker list is at most about 10 MB, so the cache stays bounded.
const CACHED_SCANS: usize = 8;

/// The last `CACHED_SCANS` scans by archive path: a finished scan is reused (reopening the
/// picker does not scan again) and a running one tells `open_engine` where the entries
/// found so far are.
fn scan_cache() -> &'static Mutex<StampedLru<Arc<ScanShared>>> {
    static CACHE: OnceLock<Mutex<StampedLru<Arc<ScanShared>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(StampedLru::new(CACHED_SCANS)))
}

/// The scan of `archive` in the cache, if its size and modification time still match.
fn cached_scan(archive: &Path) -> Option<Arc<ScanShared>> {
    let now = stamp(archive)?;
    scan_cache().lock().ok()?.get(archive, now)
}

/// What a scan of `archive` knows about `entry`: `Ok(None)` when no scan has listed it
/// (yet), `NoSuchEntry` when a complete scan did not find it.
fn cached_tar_entry(archive: &Path, entry: &str) -> Result<Option<ArchiveEntryInfo>, OpenError> {
    let Some(shared) = cached_scan(archive) else {
        return Ok(None);
    };
    let state = shared.state();
    let entries = shared.entries.lock().map_err(|_| OpenError::NoSuchEntry)?;
    match find_entry(&entries, entry) {
        Some(info) => Ok(Some(info.clone())),
        None if state == ScanState::Done => Err(OpenError::NoSuchEntry),
        None => Ok(None),
    }
}

/// The header walk of a tar archive on its own thread, feeding the entry picker. Dropping
/// it stops a scan that is still running (the thread notices at its next read).
pub struct TarScan {
    shared: Arc<ScanShared>,
    cancel: Arc<AtomicBool>,
}

impl TarScan {
    /// Starts listing the tar `archive` (plain, or inside `codec`), or reuses a finished
    /// scan of the same, unchanged archive.
    pub fn start(archive: &Path, codec: Option<Codec>, limits: ScanLimits) -> Self {
        let cancel = Arc::new(AtomicBool::new(false));
        if let Some(shared) = cached_scan(archive) {
            if !matches!(shared.state(), ScanState::Running | ScanState::Cancelled) {
                return Self { shared, cancel };
            }
        }
        let shared = Arc::new(ScanShared {
            entries: Mutex::new(Vec::new()),
            consumed: AtomicU64::new(0),
            total: AtomicU64::new(0),
            state: Mutex::new(ScanState::Running),
        });
        if let (Some(when), Ok(mut cache)) = (stamp(archive), scan_cache().lock()) {
            cache.insert(archive, when, shared.clone());
        }
        let (thread_shared, thread_cancel) = (shared.clone(), cancel.clone());
        let path = archive.to_path_buf();
        let spawned = std::thread::Builder::new()
            .name("fasttail-tar-scan".to_string())
            .spawn(move || {
                let state = run_scan(&path, codec, limits, &thread_shared, &thread_cancel);
                if matches!(state, ScanState::Cancelled | ScanState::Failed(_)) {
                    // Only a finished listing is worth keeping.
                    if let Ok(mut cache) = scan_cache().lock() {
                        cache.remove_if(&path, |s| Arc::ptr_eq(s, &thread_shared));
                    }
                }
                if let Ok(mut s) = thread_shared.state.lock() {
                    *s = state;
                }
            });
        if spawned.is_err() {
            if let Ok(mut s) = shared.state.lock() {
                *s = ScanState::Failed("cannot start thread".into());
            }
        }
        Self { shared, cancel }
    }

    /// The entries listed from index `from` on (the picker pulls only the new ones).
    pub fn entries_from(&self, from: usize) -> Vec<ArchiveEntryInfo> {
        self.shared
            .entries
            .lock()
            .map(|e| e.get(from..).map(<[_]>::to_vec).unwrap_or_default())
            .unwrap_or_default()
    }

    pub fn state(&self) -> ScanState {
        self.shared.state()
    }

    /// Share of the archive read, 0.0 to 1.0.
    pub fn progress(&self) -> f32 {
        let total = self.shared.total.load(Ordering::Relaxed);
        if total == 0 {
            return 0.0;
        }
        (self.shared.consumed.load(Ordering::Relaxed) as f64 / total as f64).clamp(0.0, 1.0) as f32
    }

    /// Asks the scan to stop; the entries found so far stay listed.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Drop for TarScan {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Fails once more than `max` decompressed bytes went through it (`hit` records it), so
/// a small archive that inflates without end cannot keep a scan busy forever.
struct DecodedLimit<'a, R> {
    inner: R,
    read: u64,
    max: u64,
    hit: &'a AtomicBool,
}

impl<R: Read> Read for DecodedLimit<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.read >= self.max {
            self.hit.store(true, Ordering::Relaxed);
            return Err(std::io::Error::other("scan limit reached"));
        }
        let room = (self.max - self.read).min(buf.len() as u64) as usize;
        let n = self.inner.read(&mut buf[..room])?;
        self.read += n as u64;
        Ok(n)
    }
}

fn run_scan(
    archive: &Path,
    codec: Option<Codec>,
    limits: ScanLimits,
    shared: &ScanShared,
    cancel: &AtomicBool,
) -> ScanState {
    let file = match crate::file_source::open_file_shared(archive) {
        Ok(f) => f,
        Err(e) => return ScanState::Failed(e.to_string()),
    };
    let total = file.metadata().map(|m| m.len()).unwrap_or(0);
    shared.total.store(total, Ordering::Relaxed);
    let counted = CountingReader::new(BufReader::new(file), &shared.consumed, Some(cancel));
    let hit = AtomicBool::new(false);
    let result = match codec {
        // A plain tar is listed by seeking over the entry data: only the headers are read.
        None => list_tar(
            TarWalker::new(SeekSkip {
                inner: counted,
                len: total,
            }),
            true,
            limits,
            shared,
        ),
        Some(codec) => {
            let decoded = DecodedLimit {
                inner: open_decoder(codec, BufReader::new(counted)),
                read: 0,
                max: limits.max_decoded,
                hit: &hit,
            };
            list_tar(TarWalker::new(ReadSkip(decoded)), false, limits, shared)
        }
    };
    match result {
        Ok(state) => state,
        Err(_) if cancel.load(Ordering::Relaxed) => ScanState::Cancelled,
        Err(_) if hit.load(Ordering::Relaxed) => ScanState::LimitReached,
        Err(e) if is_window_too_large(&e) => ScanState::Failed(WindowTooLarge.to_string()),
        Err(e) => ScanState::Damaged(e.to_string()),
    }
}

/// Lists the entries of a tar into `shared` as they are found. `seekable`: offsets are
/// positions in the archive file (a plain tar), worth recording. The walk ends `Done` only
/// after the last member's data was seen whole (a truncated tar is damaged).
fn list_tar<R: Skip>(
    mut walker: TarWalker<R>,
    seekable: bool,
    limits: ScanLimits,
    shared: &ScanShared,
) -> std::io::Result<ScanState> {
    let mut keys = HashSet::new();
    let mut listed = 0usize;
    while let Some(member) = walker.next_member()? {
        if is_tar_structure(member.kind, &member.name) {
            continue;
        }
        let refusal = if is_unsafe_name(&member.name) {
            Some(EntryRefusal::UnsafeName)
        } else {
            tar_type_refusal(member.kind).or_else(|| claim_name(&member.name, &mut keys))
        };
        let info = ArchiveEntryInfo {
            size: member.size,
            compressed_size: member.size,
            block_size: None,
            offset: seekable.then_some(member.start),
            refusal,
            name: member.name,
        };
        if let Ok(mut list) = shared.entries.lock() {
            list.push(info);
        }
        listed += 1;
        if listed >= limits.max_entries {
            return Ok(ScanState::LimitReached);
        }
    }
    Ok(ScanState::Done)
}

impl TailEngine {
    /// True for a stream read from a compressed file or an archive entry.
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
        let notes = self.bookmark_notes.clone();
        let Some(c) = self.compressed.as_mut() else {
            return Ok(());
        };
        c.restart()?;
        if !bookmarks.is_empty() {
            c.pending_bookmarks = bookmarks;
            c.pending_bookmark_notes = notes;
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
            let notes = std::mem::take(&mut c.pending_bookmark_notes);
            if fits {
                // Merged: rows bookmarked (or annotated) during the extraction stay.
                self.merge_bookmarks_with_notes(bookmarks, notes);
                // A reload rebuilt the index and saved the bookmarks as gone: save
                // them again.
                self.bookmarks_dirty = true;
            }
        }
        if finished {
            // The automatic bookmarks waited for the whole entry.
            self.run_auto_scan_if_due();
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
        assert_eq!(
            sniff_bytes(&[0x1f, 0x8b, 8, 0]),
            Format::Compressed(Codec::Gzip)
        );
        assert_eq!(sniff_bytes(b"PK\x03\x04rest"), Format::Zip);
        assert_eq!(sniff_bytes(b"PK\x05\x06"), Format::EmptyZip);
        assert_eq!(sniff_bytes(b"2026-09-25 INFO"), Format::Plain);
        assert_eq!(sniff_bytes(b""), Format::Plain);
        // The extension plays no part: a gzip named `.dat` is still a gzip.
        let dir = tempfile::tempdir().unwrap();
        let dat = dir.path().join("trace.dat");
        std::fs::write(&dat, gzip(b"hello\n")).unwrap();
        assert_eq!(sniff(&dat), Format::Compressed(Codec::Gzip));
        assert_eq!(classify(&dat), Target::Compressed(Codec::Gzip));
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
        let (state, out, written) = run_to_spool(
            JobSource::Compressed(gz.clone(), Codec::Gzip),
            Limits::default(),
            dir.path(),
        );
        assert_eq!(state, JobState::Done);
        assert_eq!(out, data);
        assert_eq!(written, data.len() as u64);

        // `cat a.gz b.gz`: both members come out, in order.
        let mut two = gzip(b"first member\n");
        two.extend(gzip(b"second member\n"));
        std::fs::write(&gz, two).unwrap();
        let (state, out, _) = run_to_spool(
            JobSource::Compressed(gz, Codec::Gzip),
            Limits::default(),
            dir.path(),
        );
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
        let (state, _, _) = run_to_spool(
            JobSource::Compressed(gz, Codec::Gzip),
            Limits::default(),
            dir.path(),
        );
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
            Target::Entry {
                archive: bundle.clone(),
                entry: "logs/worker.log".to_string(),
                kind: ArchiveKind::Zip,
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
        let (state, out, _) = run_to_spool(
            JobSource::Compressed(tgz, Codec::Gzip),
            Limits::default(),
            dir.path(),
        );
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
        let (state, out, written) =
            run_to_spool(JobSource::Compressed(gz, Codec::Gzip), limits, dir.path());
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
        let (state, out, _) =
            run_to_spool(JobSource::Compressed(gz, Codec::Gzip), limits, dir.path());
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
            JobSource::Compressed(gz, Codec::Gzip),
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
    fn restored_notes_and_automatic_bookmarks_wait_for_the_extraction() {
        let dir = tempfile::tempdir().unwrap();
        let gz = dir.path().join("app.gz");
        std::fs::write(&gz, gzip(&log_text(3000))).unwrap();
        let mut engine = open_engine(&gz, None, &test_settings(dir.path()), None).unwrap();
        let mut rule = crate::tail_engine::HighlightRule::new("line 2", [0; 3], [0; 3], false);
        rule.auto_bookmark = true;
        engine.set_highlight_rules(vec![rule]);
        let c = engine.compressed.as_mut().unwrap();
        c.pending_bookmarks = vec![5, 2999];
        c.pending_bookmark_notes = [(2999, "last one".to_string())].into_iter().collect();
        settle(&mut engine);
        assert_eq!(engine.bookmark_note(2999), Some("last one"));
        // "line 2", "line 20".."line 29", "line 200".."line 299", "line 2000".."line 2999".
        assert_eq!(engine.auto_bookmarks().len(), 1 + 10 + 100 + 1000);

        engine.reload_compressed().unwrap();
        settle(&mut engine);
        assert_eq!(engine.bookmark_note(2999), Some("last one"));
        assert_eq!(engine.auto_bookmarks().len(), 1111);
    }

    #[test]
    fn bookmarks_added_during_the_extraction_survive_the_restore() {
        let dir = tempfile::tempdir().unwrap();
        let gz = dir.path().join("app.gz");
        std::fs::write(&gz, gzip(&log_text(3000))).unwrap();
        let mut engine = open_engine(&gz, None, &test_settings(dir.path()), None).unwrap();
        let c = engine.compressed.as_mut().unwrap();
        c.pending_bookmarks = vec![1, 2999];
        c.pending_bookmark_notes = [(1, "saved".to_string()), (2999, "last".to_string())]
            .into_iter()
            .collect();
        let start = Instant::now();
        while engine.total_lines() < 3 {
            assert!(start.elapsed() < Duration::from_secs(20));
            engine.poll_updates();
            std::thread::sleep(Duration::from_millis(1));
        }
        // Rows annotated while the rest is still being extracted.
        engine.set_bookmark_note(0, "mine");
        engine.set_bookmark_note(1, "newer");
        settle(&mut engine);
        assert_eq!(
            engine.bookmarks.iter().copied().collect::<Vec<_>>(),
            [0, 1, 2999]
        );
        assert_eq!(engine.bookmark_note(0), Some("mine"));
        assert_eq!(
            engine.bookmark_note(1),
            Some("newer"),
            "the user's note wins"
        );
        assert_eq!(engine.bookmark_note(2999), Some("last"));
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
        let Target::Entry { archive, entry, .. } = classify(&path) else {
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

    // ----- bzip2, xz, zstd and tar ---------------------------------------------------

    const CODECS: [Codec; 4] = [Codec::Gzip, Codec::Bzip2, Codec::Xz, Codec::Zstd];

    fn encode(codec: Codec, data: &[u8]) -> Vec<u8> {
        match codec {
            Codec::Gzip => gzip(data),
            Codec::Bzip2 => {
                let mut enc = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::fast());
                enc.write_all(data).unwrap();
                enc.finish().unwrap()
            }
            Codec::Xz => {
                let options = lzma_rust2::XzOptions::with_preset(1);
                let mut enc = lzma_rust2::XzWriter::new(Vec::new(), options).unwrap();
                enc.write_all(data).unwrap();
                enc.finish().unwrap()
            }
            Codec::Zstd => {
                ruzstd::encoding::compress_to_vec(data, ruzstd::encoding::CompressionLevel::Fastest)
            }
        }
    }

    fn decode_all(codec: Codec, data: &[u8]) -> std::io::Result<Vec<u8>> {
        let mut out = Vec::new();
        open_decoder(codec, data).read_to_end(&mut out)?;
        Ok(out)
    }

    /// One member of a tar fixture.
    enum Item<'a> {
        File(&'a str, &'a [u8]),
        Dir(&'a str),
        /// A header whose name is written as it is (the builder refuses `..` and `/x`).
        Raw(&'a str, tar::EntryType, &'a [u8]),
        /// A regular file whose name only a pax `path` record carries.
        Pax(&'a str, &'a [u8]),
    }

    fn raw_header(name: &str, kind: tar::EntryType, size: u64) -> tar::Header {
        let mut header = tar::Header::new_gnu();
        header.set_size(size);
        header.set_entry_type(kind);
        header.set_mode(0o644);
        header.as_old_mut().name[..name.len()].copy_from_slice(name.as_bytes());
        header.set_cksum();
        header
    }

    fn tar_bytes(items: &[Item]) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for item in items {
            match item {
                Item::File(name, data) => {
                    let mut header = tar::Header::new_gnu();
                    header.set_size(data.len() as u64);
                    header.set_mode(0o644);
                    builder.append_data(&mut header, name, *data).unwrap();
                }
                Item::Dir(name) => {
                    let mut header = tar::Header::new_gnu();
                    header.set_entry_type(tar::EntryType::Directory);
                    header.set_size(0);
                    header.set_mode(0o755);
                    builder
                        .append_data(&mut header, name, std::io::empty())
                        .unwrap();
                }
                Item::Raw(name, kind, data) => {
                    let header = raw_header(name, *kind, data.len() as u64);
                    builder.append(&header, *data).unwrap();
                }
                Item::Pax(name, data) => {
                    // A pax record is `<len> path=<name>\n`, its length counting itself.
                    let body = format!(" path={name}\n");
                    let mut len = body.len() + 1;
                    while format!("{len}{body}").len() != len {
                        len += 1;
                    }
                    let record = format!("{len}{body}");
                    let pax = raw_header("PaxHeader", tar::EntryType::XHeader, record.len() as u64);
                    builder.append(&pax, record.as_bytes()).unwrap();
                    let file = raw_header("short-name", tar::EntryType::Regular, data.len() as u64);
                    builder.append(&file, *data).unwrap();
                }
            }
        }
        builder.into_inner().unwrap()
    }

    fn wait_scan(scan: &TarScan) -> ScanState {
        let start = Instant::now();
        while !scan.state().is_over() {
            assert!(
                start.elapsed() < Duration::from_secs(20),
                "scan never ended"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        scan.state()
    }

    fn scan_all(
        path: &Path,
        codec: Option<Codec>,
        limits: ScanLimits,
    ) -> (ScanState, Vec<ArchiveEntryInfo>) {
        let scan = TarScan::start(path, codec, limits);
        let state = wait_scan(&scan);
        (state, scan.entries_from(0))
    }

    #[test]
    fn every_codec_round_trips_and_reads_concatenated_streams() {
        let data = log_text(3000);
        for codec in CODECS {
            let packed = encode(codec, &data);
            assert_eq!(sniff_bytes(&packed), Format::Compressed(codec), "{codec:?}");
            assert_eq!(decode_all(codec, &packed).unwrap(), data, "{codec:?}");
            // `cat a b`: both streams come out, in order.
            let mut two = encode(codec, b"first stream\n");
            two.extend(encode(codec, b"second stream\n"));
            assert_eq!(
                decode_all(codec, &two).unwrap(),
                b"first stream\nsecond stream\n",
                "{codec:?}"
            );
        }
        // zstd skippable frames (metadata some tools write) are skipped.
        let mut skippable = vec![0x5A, 0x2A, 0x4D, 0x18, 3, 0, 0, 0, b'x', b'y', b'z'];
        skippable.extend(encode(Codec::Zstd, b"after the skippable frame\n"));
        skippable.extend([0x50, 0x2A, 0x4D, 0x18, 0, 0, 0, 0]);
        assert_eq!(sniff_bytes(&skippable), Format::Compressed(Codec::Zstd));
        assert_eq!(
            decode_all(Codec::Zstd, &skippable).unwrap(),
            b"after the skippable frame\n"
        );
    }

    #[test]
    fn new_formats_are_told_by_their_content() {
        let dir = tempfile::tempdir().unwrap();
        // A label that starts like a bzip2 header stays text: no block magic follows.
        let label = dir.path().join("label.txt");
        std::fs::write(&label, b"BZh1 is a label\n").unwrap();
        assert_eq!(classify(&label), Target::Plain);
        // Whatever the name, the content decides.
        for codec in [Codec::Bzip2, Codec::Xz, Codec::Zstd] {
            let dat = dir.path().join("trace.dat");
            std::fs::write(&dat, encode(codec, b"hello\n")).unwrap();
            assert_eq!(classify(&dat), Target::Compressed(codec), "{codec:?}");
            let (state, out, _) = run_to_spool(
                JobSource::Compressed(dat, codec),
                Limits::default(),
                dir.path(),
            );
            assert_eq!(state, JobState::Done);
            assert_eq!(out, b"hello\n");
        }
        // An empty bzip2 stream (end-of-stream magic right after the header).
        assert_eq!(
            sniff_bytes(&encode(Codec::Bzip2, b"")),
            Format::Compressed(Codec::Bzip2)
        );
        // Tars, plain and compressed.
        let tar = tar_bytes(&[Item::File("a.log", b"a\n")]);
        let plain = dir.path().join("logs.tar");
        std::fs::write(&plain, &tar).unwrap();
        assert_eq!(classify(&plain), Target::TarArchive(None));
        for codec in CODECS {
            let packed = dir.path().join("logs.tar.x");
            std::fs::write(&packed, encode(codec, &tar)).unwrap();
            assert_eq!(
                classify(&packed),
                Target::TarArchive(Some(codec)),
                "{codec:?}"
            );
            assert_eq!(archive_kind(&packed), Some(ArchiveKind::Tar(Some(codec))));
        }
    }

    #[test]
    fn a_zstd_window_over_the_cap_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        // Frame header: no single segment, window descriptor exponent 21 = 2 GiB
        // (`zstd --long=31`), then a last raw block of 6 bytes.
        let mut frame = vec![0x28, 0xB5, 0x2F, 0xFD, 0x00, 21 << 3];
        frame.extend([0x31, 0x00, 0x00]);
        frame.extend(b"hello\n");
        let long = dir.path().join("long.zst");
        std::fs::write(&long, &frame).unwrap();
        assert_eq!(classify(&long), Target::Compressed(Codec::Zstd));
        let (state, out, _) = run_to_spool(
            JobSource::Compressed(long, Codec::Zstd),
            Limits::default(),
            dir.path(),
        );
        assert_eq!(state, JobState::Stopped(StopReason::WindowTooLarge));
        assert!(out.is_empty());
        // The same frame with a 1 KiB window decodes.
        frame[5] = 0;
        assert_eq!(decode_all(Codec::Zstd, &frame).unwrap(), b"hello\n");
    }

    #[test]
    fn cap_and_cancel_stop_every_codec() {
        let dir = tempfile::tempdir().unwrap();
        let data = log_text(60_000);
        assert!(data.len() > 2 * CHUNK_BYTES);
        let cap = CHUNK_BYTES as u64 + 777;
        for codec in [Codec::Bzip2, Codec::Xz, Codec::Zstd] {
            let path = dir.path().join("big.bin");
            std::fs::write(&path, encode(codec, &data)).unwrap();
            let limits = Limits {
                max_output: cap,
                ..Limits::default()
            };
            let (state, out, written) = run_to_spool(
                JobSource::Compressed(path.clone(), codec),
                limits,
                dir.path(),
            );
            assert_eq!(
                state,
                JobState::Stopped(StopReason::CapReached(cap)),
                "{codec:?}"
            );
            assert_eq!(written, cap);
            assert_eq!(out, &data[..cap as usize]);

            // Cancelled after the first chunk: that chunk stays.
            let (chunk_tx, chunk_rx) = std::sync::mpsc::channel::<()>();
            let (go_tx, go_rx) = std::sync::mpsc::channel::<()>();
            let go_rx = Mutex::new(go_rx);
            let wake: WakeFn = Arc::new(move || {
                let _ = chunk_tx.send(());
                let _ = go_rx.lock().unwrap().recv_timeout(Duration::from_secs(5));
            });
            let (spool, out) = SpoolFile::create(dir.path(), "big.log").unwrap();
            let mut job = DecompressJob::start(
                JobSource::Compressed(path, codec),
                out,
                dir.path().to_path_buf(),
                Limits::default(),
                Some(wake),
            );
            chunk_rx.recv_timeout(Duration::from_secs(10)).unwrap();
            job.cancel();
            let _ = go_tx.send(());
            let _ = go_tx.send(());
            let state = wait_job(&job);
            job.wait();
            assert_eq!(state, JobState::Stopped(StopReason::Cancelled), "{codec:?}");
            assert_eq!(std::fs::read(spool.path()).unwrap(), &data[..CHUNK_BYTES]);
        }
    }

    #[test]
    fn a_tar_lists_its_entries_and_refuses_the_unsafe_and_special_ones() {
        let dir = tempfile::tempdir().unwrap();
        let long_name = format!("{}/deep.log", "d".repeat(120));
        let tar = tar_bytes(&[
            Item::Dir("logs/"),
            Item::File("logs/app.log", b"app\n"),
            Item::Raw("../../etc/passwd", tar::EntryType::Regular, b"root\n"),
            Item::Raw("/abs.log", tar::EntryType::Regular, b"abs\n"),
            Item::Raw("current.log", tar::EntryType::Symlink, b""),
            Item::Raw("hard.log", tar::EntryType::Link, b""),
            Item::Raw("pipe", tar::EntryType::Fifo, b""),
            Item::File("logs/app.log", b"second copy\n"),
            Item::File(&long_name, b"long\n"),
            Item::Pax("pax/named.log", b"pax\n"),
        ]);
        let path = dir.path().join("bundle.tar");
        std::fs::write(&path, &tar).unwrap();
        let (state, entries) = scan_all(&path, None, ScanLimits::default());
        assert_eq!(state, ScanState::Done);
        let rows: Vec<(&str, Option<EntryRefusal>)> = entries
            .iter()
            .map(|e| (e.name.as_str(), e.refusal.clone()))
            .collect();
        assert_eq!(
            rows,
            [
                ("logs/app.log", None),
                ("../../etc/passwd", Some(EntryRefusal::UnsafeName)),
                ("/abs.log", Some(EntryRefusal::UnsafeName)),
                ("current.log", Some(EntryRefusal::LinkOrSpecial)),
                ("hard.log", Some(EntryRefusal::LinkOrSpecial)),
                ("pipe", Some(EntryRefusal::LinkOrSpecial)),
                ("logs/app.log", Some(EntryRefusal::DuplicateName)),
                (long_name.as_str(), None),
                ("pax/named.log", None),
            ]
        );
        // A plain tar is indexed: every row knows where its headers start.
        assert!(entries.iter().all(|e| e.offset.is_some()));

        let settings = test_settings(dir.path());
        // The first of two same-named entries opens; the long and pax names too.
        for (entry, text) in [
            ("logs/app.log", "app"),
            (long_name.as_str(), "long"),
            ("pax/named.log", "pax"),
        ] {
            let mut engine = open_engine(&path, Some(entry), &settings, None).unwrap();
            settle(&mut engine);
            assert_eq!(engine.compressed.as_ref().unwrap().state(), JobState::Done);
            assert_eq!(engine.get_line(0).as_deref(), Some(text), "{entry}");
            assert_eq!(engine.path, entry_path(&path, entry));
        }
        // The refused ones do not open, and nothing is spooled for them.
        for entry in ["current.log", "pipe"] {
            assert!(matches!(
                open_engine(&path, Some(entry), &settings, None),
                Err(OpenError::Refused(EntryRefusal::LinkOrSpecial))
            ));
        }
        assert_eq!(spooled_files(&settings), 0);
        // Nothing was ever written next to the archive.
        let beside: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert!(
            beside.iter().all(|n| n == "bundle.tar" || n == "spool"),
            "{beside:?}"
        );
    }

    #[test]
    fn compressed_tars_list_and_extract_their_entries() {
        let dir = tempfile::tempdir().unwrap();
        let server = log_text(2000);
        let tar = tar_bytes(&[
            Item::Dir("var/log/"),
            Item::File("var/log/server.log", &server),
            Item::File("var/log/app.log.1.gz", &gzip(b"rotated line\n")),
            Item::File("notes.md.zst", &encode(Codec::Zstd, b"# Notes\n")),
        ]);
        let settings = test_settings(dir.path());
        for codec in CODECS {
            let path = dir.path().join(format!("bundle-{codec:?}.tar.pack"));
            std::fs::write(&path, encode(codec, &tar)).unwrap();
            let (state, entries) = scan_all(&path, Some(codec), ScanLimits::default());
            assert_eq!(state, ScanState::Done, "{codec:?}");
            let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
            assert_eq!(
                names,
                ["var/log/server.log", "var/log/app.log.1.gz", "notes.md.zst"]
            );
            // A compressed tar cannot be seeked into.
            assert!(entries.iter().all(|e| e.offset.is_none()));
            assert_eq!(entries[0].size, server.len() as u64);

            let mut engine =
                open_engine(&path, Some("var/log/server.log"), &settings, None).unwrap();
            settle(&mut engine);
            assert_eq!(engine.total_lines(), 2000, "{codec:?}");
            assert_eq!(
                engine.compressed.as_ref().unwrap().title(),
                format!("bundle-{codec:?}.tar.pack › var/log/server.log")
            );
            // A gzip inside the tarball is decompressed once more.
            let mut nested =
                open_engine(&path, Some("var/log/app.log.1.gz"), &settings, None).unwrap();
            settle(&mut nested);
            assert_eq!(nested.get_line(0).as_deref(), Some("rotated line"));
            // The spool name drops the codec suffix: Markdown stays Markdown.
            let notes = open_engine(&path, Some("notes.md.zst"), &settings, None).unwrap();
            let spool = notes
                .compressed
                .as_ref()
                .unwrap()
                .spool_path()
                .to_path_buf();
            assert!(spool.to_string_lossy().ends_with("-notes.md"), "{spool:?}");
        }
    }

    #[test]
    fn a_nested_gzip_in_a_zip_entry_is_decompressed() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("bundle.zip");
        let inner = encode(Codec::Xz, b"xz inside zip\n");
        write_zip(
            &bundle,
            &[("app.log.xz", Some(&inner), zip::CompressionMethod::Stored)],
        );
        let mut engine = open_engine(
            &bundle,
            Some("app.log.xz"),
            &test_settings(dir.path()),
            None,
        )
        .unwrap();
        settle(&mut engine);
        assert_eq!(engine.get_line(0).as_deref(), Some("xz inside zip"));
    }

    #[test]
    fn a_tar_entry_opens_without_a_scan_and_reports_a_missing_one() {
        let dir = tempfile::tempdir().unwrap();
        let tar = tar_bytes(&[
            Item::File("first.log", &log_text(500)),
            Item::File("second.log", b"second\n"),
        ]);
        let path = dir.path().join("restored.tgz");
        std::fs::write(&path, gzip(&tar)).unwrap();
        let settings = test_settings(dir.path());
        // A restored session: no scan ran, the job walks to the entry.
        let entry = entry_path(&path, "second.log");
        let Target::Entry {
            archive,
            entry,
            kind,
        } = classify(&entry)
        else {
            panic!("not an entry path");
        };
        assert_eq!(kind, ArchiveKind::Tar(Some(Codec::Gzip)));
        let mut engine = open_engine(&archive, Some(&entry), &settings, None).unwrap();
        settle(&mut engine);
        assert_eq!(engine.get_line(0).as_deref(), Some("second"));

        // The archive no longer holds the entry: the job says so.
        let mut gone = open_engine(&archive, Some("gone.log"), &settings, None).unwrap();
        settle(&mut gone);
        assert_eq!(
            gone.compressed.as_ref().unwrap().state(),
            JobState::Stopped(StopReason::NoSuchEntry)
        );
        // Once a complete scan is known, the open is refused up front.
        let (state, _) = scan_all(&archive, Some(Codec::Gzip), ScanLimits::default());
        assert_eq!(state, ScanState::Done);
        assert!(matches!(
            open_engine(&archive, Some("gone.log"), &settings, None),
            Err(OpenError::NoSuchEntry)
        ));
    }

    #[test]
    fn a_plain_tar_entry_is_found_again_when_its_offset_is_stale() {
        let dir = tempfile::tempdir().unwrap();
        let tar = tar_bytes(&[
            Item::File("a.log", &log_text(100)),
            Item::File("b.log", b"bee\n"),
        ]);
        let path = dir.path().join("logs.tar");
        std::fs::write(&path, &tar).unwrap();
        let source = |offset| JobSource::TarEntry {
            archive: path.clone(),
            codec: None,
            entry: "b.log".to_string(),
            offset,
        };
        let (_, entries) = scan_all(&path, None, ScanLimits::default());
        let (state, out, _) =
            run_to_spool(source(entries[1].offset), Limits::default(), dir.path());
        assert_eq!(state, JobState::Done);
        assert_eq!(out, b"bee\n");
        // An offset pointing at another entry: the header does not match, the job scans.
        let (state, out, _) = run_to_spool(source(Some(0)), Limits::default(), dir.path());
        assert_eq!(state, JobState::Done);
        assert_eq!(out, b"bee\n");
    }

    #[test]
    fn a_scan_stops_at_its_bounds_and_on_a_damaged_header() {
        let dir = tempfile::tempdir().unwrap();
        let tar = tar_bytes(&[
            Item::File("1.log", b"one\n"),
            Item::File("2.log", &log_text(20_000)),
            Item::File("3.log", b"three\n"),
        ]);
        let plain = dir.path().join("three.tar");
        std::fs::write(&plain, &tar).unwrap();
        let few = ScanLimits {
            max_entries: 2,
            ..ScanLimits::default()
        };
        let (state, entries) = scan_all(&plain, None, few);
        assert_eq!(state, ScanState::LimitReached);
        assert_eq!(entries.len(), 2);

        // A compressed tar that inflates past the decoded bound: the list is partial and
        // nothing is written to disk.
        let packed = dir.path().join("three.tar.zst");
        std::fs::write(&packed, encode(Codec::Zstd, &tar)).unwrap();
        let small = ScanLimits {
            max_decoded: 64 * 1024,
            ..ScanLimits::default()
        };
        let (state, entries) = scan_all(&packed, Some(Codec::Zstd), small);
        assert_eq!(state, ScanState::LimitReached);
        assert_eq!(entries.len(), 2);

        // A header with a bad checksum ends the scan with what came before it.
        let mut damaged = tar.clone();
        let second = 512 + 512; // header of 2.log, after 1.log's header and data block
        damaged[second + 148] ^= 0x01;
        let bad = dir.path().join("damaged.tar");
        std::fs::write(&bad, &damaged).unwrap();
        let (state, entries) = scan_all(&bad, None, ScanLimits::default());
        assert!(matches!(state, ScanState::Damaged(_)), "{state:?}");
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn a_finished_scan_is_reused_until_the_archive_changes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cached.tar");
        std::fs::write(&path, tar_bytes(&[Item::File("a.log", b"a\n")])).unwrap();
        let first = TarScan::start(&path, None, ScanLimits::default());
        wait_scan(&first);
        let again = TarScan::start(&path, None, ScanLimits::default());
        assert!(Arc::ptr_eq(&first.shared, &again.shared));
        // A different size is a different archive.
        std::fs::write(
            &path,
            tar_bytes(&[Item::File("a.log", b"a\n"), Item::File("b.log", b"b\n")]),
        )
        .unwrap();
        let changed = TarScan::start(&path, None, ScanLimits::default());
        assert!(!Arc::ptr_eq(&first.shared, &changed.shared));
        assert_eq!(wait_scan(&changed), ScanState::Done);
        assert_eq!(changed.entries_from(0).len(), 2);
        // A cancelled scan is not kept.
        let cancelled_path = dir.path().join("cancelled.tar");
        std::fs::write(&cancelled_path, tar_bytes(&[Item::File("a.log", b"a\n")])).unwrap();
        let scan = TarScan::start(&cancelled_path, None, ScanLimits::default());
        scan.cancel();
        let state = wait_scan(&scan);
        if state == ScanState::Cancelled {
            assert!(cached_scan(&cancelled_path).is_none());
        }
    }

    /// A GNU long-name header declaring `size` bytes, with none of them behind it.
    fn huge_extension(kind: tar::EntryType, size: u64) -> Vec<u8> {
        let mut header = raw_header("././@LongLink", kind, 0);
        header.set_size(size);
        header.set_cksum();
        let mut tar = header.as_bytes().to_vec();
        tar.extend(tar_bytes(&[Item::File("after.log", b"after\n")]));
        tar
    }

    #[test]
    fn a_huge_extension_header_is_damage_not_an_allocation() {
        let dir = tempfile::tempdir().unwrap();
        let settings = test_settings(dir.path());
        // 64 GiB of long name, 64 GiB of pax records: nothing is allocated for them.
        for kind in [tar::EntryType::GNULongName, tar::EntryType::XHeader] {
            let tar = huge_extension(kind, 64 << 30);
            for (name, bytes, codec) in [
                ("huge.tar", tar.clone(), None),
                ("huge.tgz", gzip(&tar), Some(Codec::Gzip)),
            ] {
                let path = dir.path().join(name);
                std::fs::write(&path, bytes).unwrap();
                let (state, entries) = scan_all(&path, codec, ScanLimits::default());
                assert!(matches!(state, ScanState::Damaged(_)), "{state:?}");
                assert!(entries.is_empty());
                let mut engine = open_engine(&path, Some("after.log"), &settings, None).unwrap();
                settle(&mut engine);
                assert!(
                    matches!(
                        engine.compressed.as_ref().unwrap().state(),
                        JobState::Stopped(StopReason::Failed(_))
                    ),
                    "{name}"
                );
            }
        }
        // Within the bound, a long name still resolves.
        let tar = huge_extension(tar::EntryType::GNULongName, 0);
        let path = dir.path().join("fine.tar");
        std::fs::write(&path, tar).unwrap();
        let (state, entries) = scan_all(&path, None, ScanLimits::default());
        assert_eq!(state, ScanState::Done);
        assert_eq!(entries[0].name, "after.log");
    }

    #[test]
    fn a_truncated_plain_tar_is_damaged_not_done() {
        let dir = tempfile::tempdir().unwrap();
        let mut tar = tar_bytes(&[
            Item::File("first.log", b"first\n"),
            Item::File("cut.log", &log_text(200)),
        ]);
        // Keep the header of `cut.log` and a few bytes of its data.
        tar.truncate(1024 + 512 + 100);
        let path = dir.path().join("cut.tar");
        std::fs::write(&path, &tar).unwrap();
        let (state, entries) = scan_all(&path, None, ScanLimits::default());
        assert!(matches!(state, ScanState::Damaged(_)), "{state:?}");
        assert_eq!(entries.len(), 2);
        // An incomplete list does not answer "no such entry" with authority.
        assert!(matches!(cached_tar_entry(&path, "later.log"), Ok(None)));
    }

    #[test]
    fn the_entry_after_a_sparse_file_keeps_its_offset() {
        let dir = tempfile::tempdir().unwrap();
        // A GNU sparse header whose map continues in one extension block, 512 bytes of
        // stored data (the expanded size is larger), then a regular file.
        let mut sparse = raw_header("sparse.dat", tar::EntryType::GNUSparse, 512);
        sparse.as_gnu_mut().unwrap().set_real_size(1 << 20);
        sparse.as_gnu_mut().unwrap().set_is_extended(true);
        sparse.set_cksum();
        let mut tar = sparse.as_bytes().to_vec();
        tar.extend([0u8; 512]); // extension block, not extended further
        tar.extend([7u8; 512]); // stored data
        tar.extend(tar_bytes(&[Item::File("after.log", b"after\n")]));
        let path = dir.path().join("sparse.tar");
        std::fs::write(&path, &tar).unwrap();
        let (state, entries) = scan_all(&path, None, ScanLimits::default());
        assert_eq!(state, ScanState::Done);
        assert_eq!(entries[0].refusal, Some(EntryRefusal::Sparse));
        assert_eq!(entries[1].name, "after.log");
        assert_eq!(entries[1].offset, Some(3 * 512));
        let (state, out, _) = run_to_spool(
            JobSource::TarEntry {
                archive: path.clone(),
                codec: None,
                entry: "after.log".to_string(),
                offset: entries[1].offset,
            },
            Limits::default(),
            dir.path(),
        );
        assert_eq!(state, JobState::Done);
        assert_eq!(out, b"after\n");
    }

    /// `xz` with the LZMA2 dictionary declared in its first block header replaced by the
    /// property byte `prop` (dictionary `(2 | prop & 1) << (prop / 2 + 11)`), the header's
    /// CRC32 fixed up. Nothing but the declaration changes.
    fn xz_with_dict_prop(data: &[u8], prop: u8) -> Vec<u8> {
        let mut xz = encode(Codec::Xz, data);
        let start = 12; // after the stream header
        let len = (xz[start] as usize + 1) * 4;
        let header = &mut xz[start..start + len];
        let at = header
            .windows(2)
            .position(|w| w == [0x21, 0x01])
            .expect("LZMA2 filter flags")
            + 2;
        header[at] = prop;
        let mut crc = flate2::Crc::new();
        crc.update(&header[..len - 4]);
        header[len - 4..].copy_from_slice(&crc.sum().to_le_bytes());
        xz
    }

    #[test]
    fn the_ui_thread_does_not_decode_a_large_xz_dictionary() {
        let dir = tempfile::tempdir().unwrap();
        let tar = tar_bytes(&[Item::File("a.log", b"a\n"), Item::File("b.log", b"b\n")]);
        // A small dictionary is peeked at: the content says tar, whatever the name.
        let small = dir.path().join("small.bin");
        std::fs::write(&small, encode(Codec::Xz, &tar)).unwrap();
        assert_eq!(classify(&small), Target::TarArchive(Some(Codec::Xz)));
        // A 16 MiB dictionary is over the peek bound: without a tar name it is taken for
        // a single log, with one it is a tar; either way the job still decodes it.
        let big = xz_with_dict_prop(&tar, 24);
        let unnamed = dir.path().join("big.bin");
        std::fs::write(&unnamed, &big).unwrap();
        assert_eq!(classify(&unnamed), Target::Compressed(Codec::Xz));
        // The job decodes it, finds the tar and says so; opening it again then shows the
        // entry picker instead of stopping on the same tar in a loop.
        let (state, _, _) = run_to_spool(
            JobSource::Compressed(unnamed.clone(), Codec::Xz),
            Limits::default(),
            dir.path(),
        );
        assert_eq!(state, JobState::Stopped(StopReason::Tar));
        assert_eq!(classify(&unnamed), Target::TarArchive(Some(Codec::Xz)));
        let named = dir.path().join("big.tar.xz");
        std::fs::write(&named, &big).unwrap();
        assert_eq!(classify(&named), Target::TarArchive(Some(Codec::Xz)));
        let (state, entries) = scan_all(&named, Some(Codec::Xz), ScanLimits::default());
        assert_eq!(state, ScanState::Done);
        assert_eq!(entries.len(), 2);
        // A dictionary over 256 MiB is refused by the job before it is allocated.
        let huge = dir.path().join("huge.xz");
        std::fs::write(&huge, xz_with_dict_prop(b"hello\n", 34)).unwrap();
        let (state, out, _) = run_to_spool(
            JobSource::Compressed(huge, Codec::Xz),
            Limits::default(),
            dir.path(),
        );
        assert_eq!(state, JobState::Stopped(StopReason::WindowTooLarge));
        assert!(out.is_empty());
    }

    #[test]
    fn entry_paths_are_told_from_the_raw_magic_alone() {
        let dir = tempfile::tempdir().unwrap();
        let tgz = dir.path().join("bundle.bin");
        std::fs::write(&tgz, gzip(&tar_bytes(&[Item::File("a.log", b"a\n")]))).unwrap();
        let entry = entry_path(&tgz, "a.log");
        assert_eq!(
            split_entry_path(&entry),
            Some((tgz.clone(), "a.log".into()))
        );
        assert!(source_exists(&entry));
        let plain = dir.path().join("plain.log");
        std::fs::write(&plain, b"text\n").unwrap();
        assert!(!source_exists(&plain.join("a.log")));
    }

    #[test]
    fn the_stamped_cache_keeps_the_most_recent_entries() {
        let mut lru = StampedLru::new(2);
        let when = (1, None);
        lru.insert(Path::new("a"), when, 1);
        lru.insert(Path::new("b"), when, 2);
        assert_eq!(lru.get(Path::new("a"), when), Some(1));
        lru.insert(Path::new("c"), when, 3);
        // `b` was the least recently used.
        assert_eq!(lru.get(Path::new("b"), when), None);
        assert_eq!(lru.get(Path::new("a"), when), Some(1));
        // A changed file is a miss.
        assert_eq!(lru.get(Path::new("c"), (2, None)), None);
        lru.remove_if(Path::new("a"), |v| *v == 1);
        assert_eq!(lru.get(Path::new("a"), when), None);
    }

    /// Writes a 7z holding `files` (`None` data = a directory) with `method`: one block
    /// per file, or every file in one solid block.
    fn write_7z(
        path: &Path,
        files: &[(&str, Option<&[u8]>)],
        method: sevenz_rust2::EncoderMethod,
        solid: bool,
    ) {
        use sevenz_rust2::{ArchiveEntry, ArchiveWriter, SourceReader};
        let mut writer = ArchiveWriter::new(File::create(path).unwrap()).unwrap();
        writer.set_content_methods(vec![method.into()]);
        let mut solid_entries = Vec::new();
        let mut solid_readers = Vec::new();
        for (name, data) in files {
            match data {
                None => {
                    writer
                        .push_archive_entry::<&[u8]>(ArchiveEntry::new_directory(name), None)
                        .unwrap();
                }
                Some(data) if solid => {
                    solid_entries.push(ArchiveEntry::new_file(name));
                    solid_readers.push(SourceReader::new(*data));
                }
                Some(data) => {
                    writer
                        .push_archive_entry(ArchiveEntry::new_file(name), Some(*data))
                        .unwrap();
                }
            }
        }
        if !solid_entries.is_empty() {
            writer
                .push_archive_entries(solid_entries, solid_readers)
                .unwrap();
        }
        writer.finish().unwrap();
    }

    /// A 7z number in its 9-byte form (all length bits set, 8 bytes little-endian).
    fn sz_number(out: &mut Vec<u8>, value: u64) {
        out.push(0xFF);
        out.extend(value.to_le_bytes());
    }

    /// Rewrites the start-header CRC of the 7z `bytes` after a test patched its fields.
    fn reseal_start_header(bytes: &mut [u8]) {
        let crc = crc32(&bytes[12..32]);
        bytes[8..12].copy_from_slice(&crc.to_le_bytes());
    }

    /// A 7z with `header` (stored as it is) after `packed`, behind a valid signature
    /// header.
    fn sz_container(packed: &[u8], header: &[u8]) -> Vec<u8> {
        let mut start = Vec::new();
        start.extend((packed.len() as u64).to_le_bytes());
        start.extend((header.len() as u64).to_le_bytes());
        start.extend(crc32(header).to_le_bytes());
        let mut out = SEVENZ_MAGIC.to_vec();
        out.extend([0, 4]);
        out.extend(crc32(&start).to_le_bytes());
        out.extend(start);
        out.extend(packed);
        out.extend(header);
        out
    }

    /// One file of `raw_7z`: name, coder id, coder properties, stored data.
    type RawFile<'a> = (&'a str, &'a [u8], &'a [u8], &'a [u8]);

    /// A hand-built 7z with a plain (not encoded) header: one block per file, each with
    /// the single coder `(id, properties)` and its data stored as given, so any coder id
    /// and property can be tried (the copy coder `[0x00]` gives the data back).
    fn raw_7z(files: &[RawFile]) -> Vec<u8> {
        let chains: Vec<[(&[u8], &[u8]); 1]> = files
            .iter()
            .map(|(_, id, props, _)| [(*id, *props)])
            .collect();
        let files: Vec<RawChain> = files
            .iter()
            .zip(&chains)
            .map(|((name, _, _, data), chain)| (*name, chain.as_slice(), *data))
            .collect();
        let (packed, header) = raw_7z_parts(&files);
        sz_container(&packed, &header)
    }

    /// One file of `raw_7z_parts`: name, chained coders (id, properties), stored data.
    type RawChain<'a> = (&'a str, &'a [(&'a [u8], &'a [u8])], &'a [u8]);

    /// The packed data and the plain header of a hand-built 7z: one block per file, its
    /// coders chained in order (each coder's output bound to the input of the one
    /// before), every coder's unpacked size the size of the stored data.
    fn raw_7z_parts(files: &[RawChain]) -> (Vec<u8>, Vec<u8>) {
        let mut packed = Vec::new();
        // Header, main streams info, pack info.
        let mut h = vec![0x01, 0x04, 0x06];
        sz_number(&mut h, 0);
        sz_number(&mut h, files.len() as u64);
        h.push(0x09);
        for (_, _, data) in files {
            sz_number(&mut h, data.len() as u64);
            packed.extend(*data);
        }
        h.push(0x00);
        // Unpack info: one folder per file, its coders chained.
        h.extend([0x07, 0x0B]);
        sz_number(&mut h, files.len() as u64);
        h.push(0x00);
        for (_, coders, _) in files {
            sz_number(&mut h, coders.len() as u64);
            for (id, props) in coders.iter() {
                let attributes = if props.is_empty() { 0 } else { 0x20 };
                h.push(id.len() as u8 | attributes);
                h.extend(*id);
                if !props.is_empty() {
                    sz_number(&mut h, props.len() as u64);
                    h.extend(*props);
                }
            }
            for i in 1..coders.len() as u64 {
                sz_number(&mut h, i - 1);
                sz_number(&mut h, i);
            }
        }
        h.push(0x0C);
        for (_, coders, data) in files {
            for _ in coders.iter() {
                sz_number(&mut h, data.len() as u64);
            }
        }
        // End of unpack info, empty substreams info, end of streams info.
        h.extend([0x00, 0x08, 0x00, 0x00]);
        // Files info: the names.
        h.push(0x05);
        sz_number(&mut h, files.len() as u64);
        let mut names = vec![0x00];
        for (name, _, _) in files {
            for unit in name.encode_utf16().chain([0]) {
                names.extend(unit.to_le_bytes());
            }
        }
        h.push(0x11);
        sz_number(&mut h, names.len() as u64);
        h.extend(names);
        h.extend([0x00, 0x00]);
        (packed, h)
    }

    /// A 7z whose encoded header is packed with `coders` (id, properties, unpacked size),
    /// chained in order; its data is never read.
    fn encoded_header_7z(coders: &[(&[u8], &[u8], u64)]) -> Vec<u8> {
        let packed = vec![0u8; 16];
        let mut h = vec![0x17, 0x06];
        sz_number(&mut h, 0);
        sz_number(&mut h, 1);
        h.push(0x09);
        sz_number(&mut h, packed.len() as u64);
        h.extend([0x00, 0x07, 0x0B]);
        sz_number(&mut h, 1);
        h.push(0x00);
        sz_number(&mut h, coders.len() as u64);
        for (id, props, _) in coders {
            h.push(id.len() as u8 | 0x20);
            h.extend(*id);
            sz_number(&mut h, props.len() as u64);
            h.extend(*props);
        }
        for i in 1..coders.len() as u64 {
            sz_number(&mut h, i - 1);
            sz_number(&mut h, i);
        }
        h.push(0x0C);
        for (_, _, size) in coders {
            sz_number(&mut h, *size);
        }
        h.extend([0x00, 0x00]);
        sz_container(&packed, &h)
    }

    /// A 7z holding `packed`, its plain `header` LZMA-compressed after it as 7-Zip writes
    /// it: an encoded header (`0x17`) saying where the packed header is, its coder and
    /// its CRC.
    fn lzma_header_7z(packed: &[u8], header: &[u8]) -> Vec<u8> {
        let options = lzma_rust2::LzmaOptions::with_preset(1);
        let mut writer = lzma_rust2::LzmaWriter::new(
            Vec::new(),
            &options,
            false,
            false,
            Some(header.len() as u64),
        )
        .unwrap();
        writer.write_all(header).unwrap();
        let lc_lp_pb = writer.props();
        let compressed = writer.finish().unwrap();
        let mut props = vec![lc_lp_pb];
        props.extend(options.dict_size.to_le_bytes());
        let mut h = vec![0x17, 0x06];
        sz_number(&mut h, packed.len() as u64);
        sz_number(&mut h, 1);
        h.push(0x09);
        sz_number(&mut h, compressed.len() as u64);
        h.extend([0x00, 0x07, 0x0B]);
        sz_number(&mut h, 1);
        h.push(0x00);
        sz_number(&mut h, 1);
        h.push(SZ_LZMA.len() as u8 | 0x20);
        h.extend(SZ_LZMA);
        sz_number(&mut h, props.len() as u64);
        h.extend(props);
        h.push(0x0C);
        sz_number(&mut h, header.len() as u64);
        h.extend([0x0A, 0x01]);
        h.extend(crc32(header).to_le_bytes());
        h.extend([0x00, 0x00]);
        let mut data = packed.to_vec();
        data.extend(compressed);
        sz_container(&data, &h)
    }

    const SZ_COPY: &[u8] = &[0x00];
    const SZ_LZMA: &[u8] = &[0x03, 0x01, 0x01];
    const SZ_LZMA2: &[u8] = &[0x21];
    const SZ_AES: &[u8] = &[0x06, 0xF1, 0x07, 0x01];
    const SZ_ZSTD: &[u8] = &[0x04, 0xF7, 0x11, 0x01];

    #[test]
    fn sevenz_entries_are_listed_and_extracted() {
        let dir = tempfile::tempdir().unwrap();
        let logs = dir.path().join("logs.bin");
        let server = log_text(300);
        let worker = b"worker started\nworker done\n".to_vec();
        write_7z(
            &logs,
            &[
                ("config", None),
                ("server.log", Some(&server)),
                ("logs/worker.log", Some(&worker)),
            ],
            sevenz_rust2::EncoderMethod::LZMA2,
            false,
        );
        // Told by its signature, whatever the extension.
        assert_eq!(sniff(&logs), Format::SevenZ);
        assert_eq!(classify(&logs), Target::SevenZArchive);
        assert_eq!(archive_kind(&logs), Some(ArchiveKind::SevenZ));
        let listing = list_7z_entries(&logs).unwrap();
        assert!(!listing.partial);
        let names: Vec<&str> = listing.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["server.log", "logs/worker.log"]);
        assert_eq!(listing.entries[0].size, server.len() as u64);
        assert!(listing.entries[0].compressed_size < listing.entries[0].size);
        // One block per entry: nothing else is decoded to reach it.
        assert!(listing
            .entries
            .iter()
            .all(|e| e.refusal.is_none() && e.block_size.is_none()));

        for (entry, data) in [("server.log", &server), ("logs/worker.log", &worker)] {
            let (state, out, _) = run_to_spool(
                JobSource::SevenZEntry {
                    archive: logs.clone(),
                    entry: entry.to_string(),
                },
                Limits::default(),
                dir.path(),
            );
            assert_eq!(state, JobState::Done);
            assert_eq!(&out, data);
        }

        // The entry path names the archive and the entry, and opens a titled stream.
        let path = entry_path(&logs, "logs/worker.log");
        assert_eq!(
            classify(&path),
            Target::Entry {
                archive: logs.clone(),
                entry: "logs/worker.log".to_string(),
                kind: ArchiveKind::SevenZ,
            }
        );
        let mut engine = open_engine(
            &logs,
            Some("logs/worker.log"),
            &test_settings(dir.path()),
            None,
        )
        .unwrap();
        assert_eq!(engine.path, path);
        assert_eq!(
            engine.compressed.as_ref().unwrap().title(),
            "logs.bin › logs/worker.log"
        );
        settle(&mut engine);
        assert_eq!(engine.total_lines(), 2);
    }

    #[test]
    fn a_solid_7z_block_is_decoded_up_to_its_entry() {
        for method in [
            sevenz_rust2::EncoderMethod::LZMA2,
            sevenz_rust2::EncoderMethod::BZIP2,
        ] {
            let dir = tempfile::tempdir().unwrap();
            let solid = dir.path().join("solid.7z");
            let first = log_text(4000);
            let second = log_text(2000);
            let last = b"the last entry\n".to_vec();
            write_7z(
                &solid,
                &[
                    ("first.log", Some(&first)),
                    ("second.log", Some(&second)),
                    ("last.log", Some(&last)),
                ],
                method,
                true,
            );
            let listing = list_7z_entries(&solid).unwrap();
            let block = (first.len() + second.len() + last.len()) as u64;
            assert!(
                listing
                    .entries
                    .iter()
                    .all(|e| e.refusal.is_none() && e.block_size == Some(block)),
                "{method:?}: {listing:?}"
            );
            // Only the chosen entry reaches the spool; the progress ran over the block.
            let (spool, out) = SpoolFile::create(dir.path(), "last.log").unwrap();
            let mut job = DecompressJob::start(
                JobSource::SevenZEntry {
                    archive: solid.clone(),
                    entry: "last.log".to_string(),
                },
                out,
                dir.path().to_path_buf(),
                Limits::default(),
                None,
            );
            assert_eq!(wait_job(&job), JobState::Done, "{method:?}");
            job.wait();
            assert_eq!(std::fs::read(spool.path()).unwrap(), last);
            assert_eq!(job.progress(), 1.0);
            // An entry in the middle stops at its own end.
            let (state, out, _) = run_to_spool(
                JobSource::SevenZEntry {
                    archive: solid,
                    entry: "second.log".to_string(),
                },
                Limits::default(),
                dir.path(),
            );
            assert_eq!(state, JobState::Done);
            assert_eq!(out, second);
        }
    }

    #[test]
    fn refused_7z_entries_are_listed_with_their_reason() {
        let dir = tempfile::tempdir().unwrap();
        let settings = test_settings(dir.path());
        let mixed = dir.path().join("mixed.7z");
        // LZMA2 property 37: a 1536 MiB dictionary.
        std::fs::write(
            &mixed,
            raw_7z(&[
                ("secret.log", SZ_AES, &[0x00], b"garbage"),
                ("app.log", SZ_COPY, &[], b"app started\n"),
                ("../../evil.log", SZ_COPY, &[], b"evil\n"),
                ("big.log", SZ_LZMA2, &[37], b"garbage"),
                ("odd.log", SZ_ZSTD, &[], b"garbage"),
                ("APP.log", SZ_COPY, &[], b"twin\n"),
            ]),
        )
        .unwrap();
        let entries = list_7z_entries(&mixed).unwrap().entries;
        let refusals: Vec<_> = entries.iter().map(|e| e.refusal.clone()).collect();
        let duplicate = if cfg!(windows) {
            Some(EntryRefusal::DuplicateName)
        } else {
            None
        };
        assert_eq!(
            refusals,
            [
                Some(EntryRefusal::Encrypted),
                None,
                Some(EntryRefusal::UnsafeName),
                Some(EntryRefusal::DictionaryTooLarge),
                Some(EntryRefusal::Method("ZSTD".to_string())),
                duplicate,
            ]
        );
        for (entry, refusal) in [
            ("secret.log", EntryRefusal::Encrypted),
            ("big.log", EntryRefusal::DictionaryTooLarge),
            ("odd.log", EntryRefusal::Method("ZSTD".to_string())),
        ] {
            match open_engine(&mixed, Some(entry), &settings, None) {
                Err(OpenError::Refused(r)) => assert_eq!(r, refusal),
                other => panic!("{entry}: {:?}", other.err()),
            }
        }
        // Nothing was spooled for them, and a job asked for one writes nothing.
        assert_eq!(spooled_files(&settings), 0);
        let (state, out, _) = run_to_spool(
            JobSource::SevenZEntry {
                archive: mixed.clone(),
                entry: "big.log".to_string(),
            },
            Limits::default(),
            dir.path(),
        );
        assert_eq!(
            state,
            JobState::Stopped(StopReason::Refused(EntryRefusal::DictionaryTooLarge))
        );
        assert!(out.is_empty());
        let (state, out, _) = run_to_spool(
            JobSource::SevenZEntry {
                archive: mixed.clone(),
                entry: "../../evil.log".to_string(),
            },
            Limits::default(),
            dir.path(),
        );
        assert_eq!(
            state,
            JobState::Stopped(StopReason::Refused(EntryRefusal::UnsafeName))
        );
        assert!(out.is_empty());
        // The entry beside them opens.
        let (state, out, _) = run_to_spool(
            JobSource::SevenZEntry {
                archive: mixed,
                entry: "app.log".to_string(),
            },
            Limits::default(),
            dir.path(),
        );
        assert_eq!(state, JobState::Done);
        assert_eq!(out, b"app started\n");
    }

    #[test]
    fn a_7z_header_that_is_encrypted_or_too_large_refuses_the_archive() {
        let dir = tempfile::tempdir().unwrap();
        let refusal = |bytes: Vec<u8>| {
            let path = dir.path().join("x.7z");
            std::fs::write(&path, bytes).unwrap();
            assert_eq!(classify(&path), Target::SevenZArchive);
            let err = list_7z_entries(&path).unwrap_err();
            let refusal = sevenz_refusal(&err).cloned();
            let opened = open_engine(&path, Some("a.log"), &test_settings(dir.path()), None);
            assert!(matches!(opened, Err(OpenError::Io(_))));
            refusal
        };
        // AES, then LZMA: the names cannot be read without the password.
        assert_eq!(
            refusal(encoded_header_7z(&[
                (SZ_AES, &[0x00], 64),
                (SZ_LZMA, &[0x5D, 0, 0, 1, 0], 64),
            ])),
            Some(SevenZRefusal::EncryptedHeader)
        );
        // A header decoding to 1 GB, or needing a 1536 MiB dictionary.
        assert_eq!(
            refusal(encoded_header_7z(&[(
                SZ_LZMA,
                &[0x5D, 0, 0, 1, 0],
                1 << 30
            )])),
            Some(SevenZRefusal::HeaderTooLarge)
        );
        assert_eq!(
            refusal(encoded_header_7z(&[(SZ_LZMA2, &[37], 64)])),
            Some(SevenZRefusal::HeaderTooLarge)
        );
        // A stored header larger than 64 MB is refused before it is read.
        let mut big = raw_7z(&[("a.log", SZ_COPY, &[], b"a\n")]);
        big[20..28].copy_from_slice(&(MAX_7Z_HEADER + 1).to_le_bytes());
        reseal_start_header(&mut big);
        assert_eq!(refusal(big), Some(SevenZRefusal::HeaderTooLarge));
        // Damage is reported as damage, not as a refusal.
        let mut broken = raw_7z(&[("a.log", SZ_COPY, &[], b"a\n")]);
        let len = broken.len();
        broken.truncate(len - 4);
        assert_eq!(refusal(broken), None);
        // Two 192 MiB LZMA2 coders need 384 MiB together: over the decoder bound.
        assert_eq!(
            refusal(encoded_header_7z(&[
                (SZ_LZMA2, &[31], 64),
                (SZ_LZMA2, &[31], 64)
            ])),
            Some(SevenZRefusal::HeaderTooLarge)
        );
    }

    #[test]
    fn a_7z_with_a_damaged_start_header_is_damaged_not_guessed() {
        let dir = tempfile::tempdir().unwrap();
        let good = raw_7z(&[("a.log", SZ_COPY, &[], b"a\n")]);
        // Zeroed (what an interrupted 7-Zip leaves), or one CRC bit flipped: the header
        // is never looked for near the end of the file.
        let mut zeroed = good.clone();
        zeroed[8..32].fill(0);
        let mut flipped = good.clone();
        flipped[8] ^= 0x01;
        for (name, bytes) in [("zeroed.7z", zeroed), ("flipped.7z", flipped)] {
            let path = dir.path().join(name);
            std::fs::write(&path, bytes).unwrap();
            let err = list_7z_entries(&path).unwrap_err();
            assert_eq!(sevenz_refusal(&err), None, "{name}");
            assert_eq!(err.kind(), std::io::ErrorKind::InvalidData, "{name}");
            assert!(err.to_string().contains("start header"), "{name}: {err}");
        }
        // An intact start header declaring no header: an empty archive.
        let empty = dir.path().join("empty.7z");
        std::fs::write(&empty, sz_container(&[], &[])).unwrap();
        let listing = list_7z_entries(&empty).unwrap();
        assert!(listing.entries.is_empty());
        assert!(!listing.partial);
    }

    #[test]
    fn a_7z_declaring_too_many_entries_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        // A header declaring 300,000 files, in a few bytes.
        let mut header = vec![0x01, 0x05];
        sz_number(&mut header, 300_000);
        header.extend([0x00, 0x00]);
        for (name, bytes) in [
            ("plain.7z", sz_container(&[], &header)),
            ("encoded.7z", lzma_header_7z(&[], &header)),
        ] {
            let path = dir.path().join(name);
            std::fs::write(&path, bytes).unwrap();
            let err = list_7z_entries(&path).unwrap_err();
            assert_eq!(
                sevenz_refusal(&err),
                Some(&SevenZRefusal::TooManyEntries),
                "{name}: {err}"
            );
        }
        // As many blocks declared by the streams info.
        let mut blocks = vec![0x01, 0x04, 0x07, 0x0B];
        sz_number(&mut blocks, 300_000);
        blocks.push(0x00);
        let path = dir.path().join("blocks.7z");
        std::fs::write(&path, sz_container(&[], &blocks)).unwrap();
        let err = list_7z_entries(&path).unwrap_err();
        assert_eq!(sevenz_refusal(&err), Some(&SevenZRefusal::TooManyEntries));
    }

    #[test]
    fn a_7z_with_an_lzma_encoded_header_opens() {
        let dir = tempfile::tempdir().unwrap();
        let (packed, header) = raw_7z_parts(&[
            ("a.log", &[(SZ_COPY, &[])], b"first\n"),
            ("b.log", &[(SZ_COPY, &[])], b"second\n"),
        ]);
        let path = dir.path().join("encoded.7z");
        std::fs::write(&path, lzma_header_7z(&packed, &header)).unwrap();
        let listing = list_7z_entries(&path).unwrap();
        let names: Vec<&str> = listing.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["a.log", "b.log"]);
        let (state, out, _) = run_to_spool(
            JobSource::SevenZEntry {
                archive: path.clone(),
                entry: "b.log".to_string(),
            },
            Limits::default(),
            dir.path(),
        );
        assert_eq!(state, JobState::Done);
        assert_eq!(out, b"second\n");
        // A damaged byte in the packed header fails its CRC: damage, not a refusal.
        let mut bytes = std::fs::read(&path).unwrap();
        let at = 32 + packed.len() + 2;
        bytes[at] ^= 0x40;
        let broken = dir.path().join("broken.7z");
        std::fs::write(&broken, bytes).unwrap();
        let err = list_7z_entries(&broken).unwrap_err();
        assert_eq!(sevenz_refusal(&err), None, "{err}");
    }

    #[test]
    fn a_7z_block_whose_coders_together_need_too_much_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("chain.7z");
        let (packed, header) = raw_7z_parts(&[
            ("one.log", &[(SZ_LZMA2, &[31])], b"garbage"),
            (
                "two.log",
                &[(SZ_LZMA2, &[31]), (SZ_LZMA2, &[31])],
                b"garbage",
            ),
        ]);
        std::fs::write(&path, sz_container(&packed, &header)).unwrap();
        let refusals: Vec<_> = list_7z_entries(&path)
            .unwrap()
            .entries
            .into_iter()
            .map(|e| e.refusal)
            .collect();
        // 192 MiB alone fits; two chained need 384 MiB.
        assert_eq!(refusals, [None, Some(EntryRefusal::DictionaryTooLarge)]);
    }

    #[test]
    fn a_long_7z_listing_is_partial() {
        let dir = tempfile::tempdir().unwrap();
        let many = dir.path().join("many.7z");
        let files: Vec<(String, &[u8])> = (0..5)
            .map(|i| (format!("{i}.log"), b"x\n".as_slice()))
            .collect();
        let files: Vec<RawFile> = files
            .iter()
            .map(|(name, data)| (name.as_str(), SZ_COPY, [].as_slice(), *data))
            .collect();
        std::fs::write(&many, raw_7z(&files)).unwrap();
        let listing = list_7z(&many, 3).unwrap();
        assert!(listing.partial);
        assert_eq!(listing.entries.len(), 3);
        assert!(!list_7z(&many, 5).unwrap().partial);
        // An entry past the listed ones still opens (a restored stream).
        let (state, out, _) = run_to_spool(
            JobSource::SevenZEntry {
                archive: many,
                entry: "4.log".to_string(),
            },
            Limits::default(),
            dir.path(),
        );
        assert_eq!(state, JobState::Done);
        assert_eq!(out, b"x\n");
    }

    #[test]
    fn a_compressed_7z_entry_is_decompressed_once_more() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("bundle.7z");
        let data = log_text(500);
        let gz = gzip(&data);
        write_7z(
            &bundle,
            &[("app.log.1.gz", Some(&gz)), ("notes.txt", Some(b"n\n"))],
            sevenz_rust2::EncoderMethod::LZMA2,
            true,
        );
        let mut engine = open_engine(
            &bundle,
            Some("app.log.1.gz"),
            &test_settings(dir.path()),
            None,
        )
        .unwrap();
        // The spool is named without the codec suffix.
        assert!(engine
            .compressed
            .as_ref()
            .unwrap()
            .spool_path()
            .to_string_lossy()
            .ends_with("app.log.1"));
        settle(&mut engine);
        assert_eq!(engine.total_lines(), 500);
        assert_eq!(
            engine.get_line(499).as_deref(),
            std::str::from_utf8(&data).unwrap().lines().nth(499)
        );
    }

    #[test]
    fn a_restored_7z_entry_is_extracted_again_with_its_bookmarks() {
        let dir = tempfile::tempdir().unwrap();
        let logs = dir.path().join("logs.7z");
        let server = log_text(3000);
        write_7z(
            &logs,
            &[("server.log", Some(&server)), ("worker.log", Some(b"w\n"))],
            sevenz_rust2::EncoderMethod::LZMA2,
            true,
        );
        // What a session saves (archive + entry) resolves back to the stream path.
        let path = entry_path(&logs, "server.log");
        assert_eq!(
            split_entry_path(&path),
            Some((logs.clone(), "server.log".to_string()))
        );
        assert!(source_exists(&path));
        let mut engine =
            open_engine(&logs, Some("server.log"), &test_settings(dir.path()), None).unwrap();
        engine.compressed.as_mut().unwrap().pending_bookmarks = vec![5, 2999];
        settle(&mut engine);
        assert_eq!(
            engine.bookmarks.iter().copied().collect::<Vec<_>>(),
            [5, 2999]
        );
        engine.reload_compressed().unwrap();
        settle(&mut engine);
        assert_eq!(engine.total_lines(), 3000);
        assert_eq!(
            engine.bookmarks.iter().copied().collect::<Vec<_>>(),
            [5, 2999]
        );
    }
}
