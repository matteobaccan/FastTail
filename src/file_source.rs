//! On-demand file access for the tail engine: one shared read handle and a small LRU
//! cache of fixed-size blocks. The file is never held in memory as a whole.
//!
//! Two access paths:
//! - `read_with` serves random access (visible rows, HEX rows, byte windows) through the
//!   block cache; a screen of rows lives in one or two blocks.
//! - `read_direct` serves sequential scans (indexing, filtering, searching) straight from
//!   the handle, so a scan never evicts the blocks the view is using.

use std::cell::{Cell, RefCell};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Size of one cached block. A screen of rows fits in one block; a miss costs one read of
/// this size, so smaller blocks make random jumps (F3, go-to, bookmarks) cheaper.
pub const BLOCK_SIZE: usize = 64 * 1024;
/// Blocks kept per source (4 MB).
pub const MAX_BLOCKS: usize = 64;

pub fn open_file_shared(path: &Path) -> std::io::Result<File> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(7); // FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE
    }
    options.open(path)
}

struct Block {
    index: u64,
    data: Vec<u8>,
    last_used: u64,
}

#[derive(Default)]
struct BlockCache {
    blocks: Vec<Block>,
    tick: u64,
}

pub struct FileSource {
    path: PathBuf,
    file: RefCell<Option<File>>,
    len: Cell<u64>,
    cache: RefCell<BlockCache>,
}

impl FileSource {
    pub fn open(path: &Path) -> std::io::Result<Self> {
        let file = open_file_shared(path)?;
        let len = file.metadata()?.len();
        Ok(Self {
            path: path.to_path_buf(),
            file: RefCell::new(Some(file)),
            len: Cell::new(len),
            cache: RefCell::new(BlockCache::default()),
        })
    }

    /// Length the source currently believes the file has (kept in sync by the engine).
    pub fn len(&self) -> u64 {
        self.len.get()
    }

    pub fn is_empty(&self) -> bool {
        self.len.get() == 0
    }

    /// Records a new file length. Growth invalidates the (partial) block that held the old
    /// end; shrinking drops the whole cache.
    pub fn set_len(&self, new_len: u64) {
        let old = self.len.get();
        if new_len == old {
            return;
        }
        if new_len < old {
            self.clear();
        } else {
            self.invalidate_from(old);
        }
        self.len.set(new_len);
    }

    /// Drops every cached block that contains bytes at or after `offset`.
    pub fn invalidate_from(&self, offset: u64) {
        let first = offset / BLOCK_SIZE as u64;
        self.cache.borrow_mut().blocks.retain(|b| b.index < first);
    }

    /// Drops the cache and returns its memory.
    pub fn clear(&self) {
        let mut cache = self.cache.borrow_mut();
        cache.blocks = Vec::new();
    }

    /// Reopens the handle (after a rotation or a failed read) and drops the cache.
    pub fn reopen(&self) -> std::io::Result<()> {
        let file = open_file_shared(&self.path)?;
        let len = file.metadata()?.len();
        *self.file.borrow_mut() = Some(file);
        self.len.set(len);
        self.clear();
        Ok(())
    }

    /// Bytes currently held by the cache.
    pub fn cached_bytes(&self) -> usize {
        self.cache
            .borrow()
            .blocks
            .iter()
            .map(|b| b.data.len())
            .sum()
    }

    /// Calls `f` with the bytes in `[offset, offset + len)`, clamped to the known length.
    /// Returns `None` when `offset` is past the end or the file cannot be read.
    pub fn read_with<R>(&self, offset: u64, len: usize, f: impl FnOnce(&[u8]) -> R) -> Option<R> {
        let total = self.len.get();
        if offset >= total && !(offset == 0 && total == 0) {
            return None;
        }
        let end = offset.saturating_add(len as u64).min(total);
        let len = (end - offset) as usize;
        if len == 0 {
            return Some(f(&[]));
        }
        let first_block = offset / BLOCK_SIZE as u64;
        let last_block = (end - 1) / BLOCK_SIZE as u64;
        if first_block == last_block {
            let mut cache = self.cache.borrow_mut();
            let block = self.block(&mut cache, first_block)?;
            let start = (offset - first_block * BLOCK_SIZE as u64) as usize;
            let stop = (start + len).min(block.len());
            return Some(f(&block[start..stop]));
        }
        // Spanning blocks (a line across a boundary, a long HEX window): assemble a copy.
        let mut out = Vec::with_capacity(len);
        {
            let mut cache = self.cache.borrow_mut();
            for idx in first_block..=last_block {
                let block = self.block(&mut cache, idx)?;
                let block_start = idx * BLOCK_SIZE as u64;
                let from = offset.saturating_sub(block_start) as usize;
                let to = (end - block_start).min(block.len() as u64) as usize;
                if from < to {
                    out.extend_from_slice(&block[from..to]);
                }
            }
        }
        Some(f(&out))
    }

    /// Owned copy of `[offset, offset + len)`, clamped to the known length.
    pub fn read_to_vec(&self, offset: u64, len: usize) -> Vec<u8> {
        self.read_with(offset, len, |b| b.to_vec())
            .unwrap_or_default()
    }

    /// Sequential read bypassing the cache: fills `buf` from `offset` with as many bytes as
    /// the file provides (looping over partial reads) and returns the count.
    pub fn read_direct(&self, offset: u64, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut guard = self.file.borrow_mut();
        let file = match guard.as_mut() {
            Some(f) => f,
            None => {
                *guard = Some(open_file_shared(&self.path)?);
                guard.as_mut().unwrap()
            }
        };
        file.seek(SeekFrom::Start(offset))?;
        let mut filled = 0;
        while filled < buf.len() {
            match file.read(&mut buf[filled..]) {
                Ok(0) => break,
                Ok(n) => filled += n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
        Ok(filled)
    }

    /// Returns the cached block `idx`, reading it when absent (evicting the least
    /// recently used one when the cache is full).
    fn block<'c>(&self, cache: &'c mut BlockCache, idx: u64) -> Option<&'c [u8]> {
        cache.tick = cache.tick.wrapping_add(1);
        let tick = cache.tick;
        if let Some(pos) = cache.blocks.iter().position(|b| b.index == idx) {
            cache.blocks[pos].last_used = tick;
            return Some(&cache.blocks[pos].data);
        }
        let mut data = vec![0u8; BLOCK_SIZE];
        let n = match self.read_direct(idx * BLOCK_SIZE as u64, &mut data) {
            Ok(n) => n,
            Err(_) => {
                // The handle may point at a replaced file: reopen once and retry.
                self.reopen().ok()?;
                self.read_direct(idx * BLOCK_SIZE as u64, &mut data).ok()?
            }
        };
        if n == 0 {
            return None;
        }
        data.truncate(n);
        if cache.blocks.len() >= MAX_BLOCKS {
            if let Some((pos, _)) = cache
                .blocks
                .iter()
                .enumerate()
                .min_by_key(|(_, b)| b.last_used)
            {
                cache.blocks.swap_remove(pos);
            }
        }
        cache.blocks.push(Block {
            index: idx,
            data,
            last_used: tick,
        });
        Some(&cache.blocks.last().unwrap().data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn pattern(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    fn temp_file(bytes: &[u8]) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("src.bin");
        File::create(&path).unwrap().write_all(bytes).unwrap();
        (dir, path)
    }

    #[test]
    fn reads_inside_and_across_blocks() {
        let data = pattern(BLOCK_SIZE * 2 + 1000);
        let (_dir, path) = temp_file(&data);
        let src = FileSource::open(&path).unwrap();
        assert_eq!(src.len(), data.len() as u64);

        // Inside one block.
        let got = src.read_to_vec(100, 50);
        assert_eq!(got, &data[100..150]);
        // Across the first boundary.
        let got = src.read_to_vec(BLOCK_SIZE as u64 - 10, 30);
        assert_eq!(got, &data[BLOCK_SIZE - 10..BLOCK_SIZE + 20]);
        // Across two boundaries (spanning three blocks).
        let got = src.read_to_vec(BLOCK_SIZE as u64 - 5, BLOCK_SIZE + 20);
        assert_eq!(got, &data[BLOCK_SIZE - 5..2 * BLOCK_SIZE + 15]);
        // The partial last block.
        let got = src.read_to_vec(2 * BLOCK_SIZE as u64, 5000);
        assert_eq!(got, &data[2 * BLOCK_SIZE..]);
        assert!(src.cached_bytes() <= MAX_BLOCKS * BLOCK_SIZE);
    }

    #[test]
    fn past_end_and_empty_reads() {
        let data = pattern(1000);
        let (_dir, path) = temp_file(&data);
        let src = FileSource::open(&path).unwrap();
        assert!(src.read_with(1000, 10, |b| b.len()).is_none());
        assert!(src.read_with(5000, 10, |b| b.len()).is_none());
        assert_eq!(src.read_with(990, 100, |b| b.len()), Some(10));
        assert_eq!(src.read_with(10, 0, |b| b.len()), Some(0));
    }

    #[test]
    fn growth_invalidates_the_old_last_block_and_shrink_clears() {
        let data = pattern(BLOCK_SIZE + 100);
        let (_dir, path) = temp_file(&data);
        let src = FileSource::open(&path).unwrap();
        assert_eq!(src.read_to_vec(BLOCK_SIZE as u64, 100).len(), 100);
        assert!(src.cached_bytes() > 0);

        // The writer appends; the source is told the new length and re-reads the tail block.
        let extra = pattern(500);
        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(&extra)
            .unwrap();
        src.set_len(data.len() as u64 + 500);
        let got = src.read_to_vec(BLOCK_SIZE as u64 + 100, 500);
        assert_eq!(got, extra);

        // Truncation: everything cached is dropped.
        File::create(&path).unwrap().write_all(b"tiny").unwrap();
        src.set_len(4);
        assert_eq!(src.cached_bytes(), 0);
        assert_eq!(src.read_to_vec(0, 10), b"tiny");
    }

    #[test]
    fn lru_keeps_the_cache_bounded() {
        let data = pattern(BLOCK_SIZE * (MAX_BLOCKS + 4));
        let (_dir, path) = temp_file(&data);
        let src = FileSource::open(&path).unwrap();
        for i in 0..(MAX_BLOCKS + 4) {
            let off = (i * BLOCK_SIZE) as u64 + 7;
            assert_eq!(
                src.read_to_vec(off, 3),
                &data[off as usize..off as usize + 3]
            );
        }
        assert_eq!(src.cached_bytes(), MAX_BLOCKS * BLOCK_SIZE);
        // Evicted blocks are read again transparently.
        assert_eq!(src.read_to_vec(7, 3), &data[7..10]);
    }

    #[test]
    fn read_direct_is_sequential_and_uncached() {
        let data = pattern(BLOCK_SIZE + 10);
        let (_dir, path) = temp_file(&data);
        let src = FileSource::open(&path).unwrap();
        let mut buf = vec![0u8; 64 * 1024];
        let n = src.read_direct(BLOCK_SIZE as u64 - 20, &mut buf).unwrap();
        assert_eq!(n, 30);
        assert_eq!(&buf[..n], &data[BLOCK_SIZE - 20..]);
        assert_eq!(src.cached_bytes(), 0);
    }
}
