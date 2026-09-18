## 1. File Source

- [x] 1.1 Add `FileSource` in `src/file_source.rs`: open handle with full sharing, `len()`, `read_at(offset, len) -> Cow<[u8]>` through an LRU of 16 × 256 KB blocks, `invalidate_from(offset)`, `clear()`, `reopen()`
- [x] 1.2 Unit tests: reads inside a block, across block boundaries, past EOF, after invalidation and after the file grew

## 2. Engine on the Source

- [x] 2.1 Replace `buffer: Vec<u8>` with `source: FileSource`; rewrite `get_line` (decode from `read_at`, 1 MB cap with marker), `get_bytes`, `find_byte_matches_from` (chunked with overlap), `ensure_markdown_text` (32 MB cap)
- [x] 2.2 Rewrite the index scan to stream 256 KB chunks with memchr (initial and incremental); keep `max_line_bytes` and `max_detected_width` semantics
- [x] 2.3 Rewrite `refresh_file`: growth reads only new bytes through the source; fingerprints of the head and of the indexed end replace the buffer prefix/tail comparison; shrink drops index, cache and derived state and shrinks vectors
- [x] 2.4 Remove `release_mmap`, the `memmap2` dependency and every remaining `buffer` reference (`render_markdown_stream` uses `total_lines()`)
- [x] 2.5 Run the existing suite unchanged: all 91 integration tests and the unit tests pass

## 3. Scan Jobs

- [x] 3.1 Add `ScanJob` (kind: Index | Filter | Search; generation; own handle; parameter snapshot) running on a worker thread, sending ordered `ScanBatch` messages with progress; `AtomicU64` generation for cancellation
- [x] 3.2 Route indexing (> 256 MB), filtering and search (> 16 MB) through jobs; drain batches in `poll_updates`; queue appends during a job and replay them incrementally at the end
- [x] 3.3 Tests: background filter equals synchronous filter, cancellation on a newer query, appends during a job, index job on a generated 300 MB file

## 4. UI

- [x] 4.1 Progress text (`indexing 34%`, `filtering 58%`, `searching 12%`, count so far) in the stream bar while a job runs
- [x] 4.2 Markdown cap notice; long-line truncation marker; i18n keys in five languages and the coverage test

## 5. Measurement and Docs

- [x] 5.1 `filter_bench`: `open` on 1 GB, `scroll` (random `get_line`), the 10 × 50 MB scenario with per-frame polling, and a peak resident memory report
- [x] 5.2 Record before/after numbers in `design.md` (memory for 10 × 50 MB, per-frame poll cost, open time for 1 GB)
- [x] 5.3 README: replace the memory-mapped wording with the real model and its limits; `stream-engine` spec synced at archive
