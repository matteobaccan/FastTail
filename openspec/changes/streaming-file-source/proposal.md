## Why

`TailEngine` reads the whole file into a `Vec<u8>` at open and appends every new byte to it. Resident memory equals the file size, can reach twice the file size through vector growth, and is never released after a truncation. The README and the `stream-engine` spec promise memory-mapped access to files larger than 50 GB with a small footprint; today a 20 GB log needs 20 GB of RAM. The `memmap2` dependency is declared but unused. Continuously growing logs, and logs that are reset and refilled, are the normal case for this tool, so the engine must stop holding the file in RAM.

## What Changes

- **On-demand reading.** The engine keeps an open read handle and a small block cache (fixed number of 256 KB blocks, LRU, ~8 MB per stream) and reads line bytes, hex rows and search windows from the file when needed. No copy of the file lives in memory.
- **Index-only resident state.** Per stream the engine holds the line index (one 64-bit offset per line), the filtered-line and match lists, selection and bookmarks. Line text is decoded from cached blocks on access.
- **Background scans with progress.** Indexing at open, include/exclude filtering and search run synchronously for files up to 64 MB and on a worker thread above that, in chunks, reporting progress in the stream bar and cancellable by any newer request. Appends keep the incremental paths (only new bytes are read and indexed).
- **Reset detection without a buffer.** The engine remembers 64-byte fingerprints of the file head and of the last indexed position and compares them against the file on growth; a mismatch reloads from the start. Truncation frees the index and the cache.
- **Markdown mode cap.** Rendered Markdown needs the whole text; files above 32 MB open in text mode with a notice instead of being read whole.
- `memmap2` is removed from the dependencies; README and the `stream-engine` spec describe the real behaviour and its limits (index cost of 8 bytes per line).

## Capabilities

### New Capabilities
- (none)

### Modified Capabilities
- `stream-engine`: the large-file requirement changes from memory-mapped access to streaming access with a bounded footprint; background scans and the Markdown cap are added.

## Impact

- `src/tail_engine.rs`: `buffer: Vec<u8>` replaced by a `FileSource` (handle + block cache); `get_line`, `get_bytes`, `refresh_file`, `rebuild_line_index_from`, `find_byte_matches_from`, `ensure_markdown_text` rewritten on top of it; scan jobs (`index`, `filter`, `search`) with a worker thread, progress and cancellation; `release_mmap` removed.
- `src/ui/dock.rs`: progress indicator in the stream bar, Markdown cap notice; `render_markdown_stream` no longer touches the buffer.
- `Cargo.toml`: `memmap2` removed.
- `benches/filter_bench.rs`: phases for open (indexing) on 1 GB, scrolling reads through the cache, and a resident-memory report.
- `tests/integration_tests.rs`: behaviour is unchanged for the existing 91 tests; new tests for cache reads across block boundaries, truncation freeing memory, reset detection, background scans completing and being cancelled, Markdown cap.
- README: memory section replaces the "memory-mapped" wording.
