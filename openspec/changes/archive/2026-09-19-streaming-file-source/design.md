## Context

`TailEngine` owns `buffer: Vec<u8>` holding the entire file. Every reader (`get_line`, `get_bytes` for HEX, `find_byte_matches_from`, `ensure_markdown_text`, the line index scan, the append and truncation paths) slices that vector. The UI never touches the buffer except `render_markdown_stream`, which checks whether it is empty. Filters, search and highlight evaluation all go through `get_line`.

Reference workload from the maintainer: ten files of 50 MB each, all growing continuously, some reset periodically; BareTail handles it without stalls. Today this costs 500 MB to 1 GB of resident memory and, before the incremental index fix, tens of milliseconds per frame per file.

Measured costs on this machine (thin LTO): sequential scan with memchr ~1.2 GB/s; plain-text case-insensitive filter over 100 MB ~300 ms; regex filter ~120 ms; reading a 256 KB block from the OS cache ~50 µs.

## Goals / Non-Goals

**Goals:**
- Resident memory per stream bounded by the line index plus a small fixed cache, independent of file size. Target for 10 × 50 MB (about 1 million lines): under 60 MB total, versus 500 MB or more today.
- No per-frame work proportional to file size. Ten growing streams SHALL cost under 5 ms per frame together on this machine.
- Opening, filtering and searching large files never freeze the interface; long scans show progress and can be cancelled by a newer request.
- Behaviour of every existing feature (filters, search in all views, HEX, Markdown, bookmarks, selection, export, encodings, rotation, truncation) unchanged for files that fit today's tests.

**Non-Goals:**
- Memory-mapping. On Windows a mapped file cannot be truncated or replaced by the writer (sharing violation), and reading a mapped region past a shrunk EOF raises an uncatchable access violation; log rotation does both. Explicit reads are the only safe primitive on the platform the tool targets first.
- A sparse (compressed) line index. One 64-bit offset per line costs 8 MB per million lines; that is acceptable for the workloads above and keeps random access O(1). A block-sampled index is a later change if 100 million-line files appear.
- Changing the filter or search semantics, or the 20,000-hit cap.

## Decisions

**D1. `FileSource`: one read handle, a block cache, no buffer.**
A stream keeps a `File` opened with full sharing (already the case for polls) and an LRU cache of at most 64 blocks of 64 KB (4 MB; 16 × 256 KB was measured first and made random jumps four times more expensive for no gain in scrolling). `read_at(offset, len)` returns bytes from cached blocks, reading missing blocks with `seek + read_exact` (or `read_at` on Unix). Visible rows come from one or two blocks, so scrolling and follow mode hit the cache; a filter scan streams the file sequentially through the same cache with a read-ahead of one block. The handle is reopened when the file is replaced (rotation) or when a read fails.

**D2. Line text is decoded on access.** `get_line(idx)` reads `[offset(idx), offset(idx+1))` through the cache and decodes it as today (UTF-8 lossless, ANSI/ASCII mapping, UTF-16 LE/BE). Lines longer than one block are assembled from consecutive blocks; a cap of 1 MB per line protects the UI from pathological input (the rest of the line is not shown, and a marker says so).

**D3. The index stays as it is: `Vec<u64>` offsets, incremental on append.** The recent fix already rescans only from the last known line. Under the new source the scan reads the new bytes in 256 KB chunks with memchr and never keeps them.

**D4. Scans run in jobs: synchronous when small, on a worker thread when large.**
Indexing at open runs synchronously up to 256 MB (about 200 ms) and in the background above. Filter and search scans run synchronously up to 16 MB and in the background above, so typing a filter on a 50 MB file never blocks a keystroke. A job owns its own `File` handle and a snapshot of the parameters (query, filter strings, case, regex, encoding), reads sequential chunks, and sends `ScanBatch { generation, range, results, progress }` messages through an `mpsc` channel; `poll_updates` drains them and appends to `filtered_lines` / `search_matches`. Every job carries a generation number; results of an older generation are dropped, and the worker checks an `AtomicU64` to stop early. Appends that arrive while a job runs are queued and evaluated by the incremental paths once the job ends, so the derived state is never inconsistent. Results are appended in order (jobs scan forward), which keeps `filtered_lines` sorted and `get_actual_line_idx` valid at every step.

**D5. Progress in the stream bar.** While a job runs the stream bar shows `indexing 34%`, `filtering 58%` or `searching 12%` with the count so far; the view works on what is already available (rows appear as they are found).

**D6. Reset detection by fingerprints.** The engine stores the first 64 bytes of the file and the 64 bytes preceding the end of the indexed region. On growth it reads both windows again; a mismatch means the file was rewritten and triggers a full reload. On shrink the index, caches and derived state are dropped and memory is returned (`Vec::shrink_to_fit`, cache cleared).

**D7. Markdown cap.** `ensure_markdown_text` needs the whole text; it reads it through the source only when the file is at most 32 MB. Larger files open in text mode with a notice; the MD button stays available and shows the notice again.

**D8. HEX and byte search.** `get_bytes(offset, len)` becomes a cache read. Byte-level search scans chunks with an overlap of `query_len - 1` bytes, exactly like the current `find_byte_matches_from` but sourced from the file.

**D9. Export and copy stream through the source**, unchanged in API: `export_lines` reads each line on demand; a 12 million-line export costs one sequential pass.

**D10. Tests and benchmark.** All existing tests must pass unchanged (they only use the public API). New tests: lines spanning block boundaries, a 1 MB line, truncation freeing the cache, reset-with-same-header detection through fingerprints, background filter job delivering the same result as the synchronous path, cancellation on a newer query, Markdown cap. `filter_bench` gains an `open 1 GB` phase, a `scroll` phase (random `get_line` over the file) and prints peak resident memory (Windows `GetProcessMemoryInfo`, Linux `/proc/self/status`), with the 10 × 50 MB scenario as a dedicated phase.

## Measurements (2026-09-19, this machine, thin LTO, `cargo bench`)

100 MB generated log, 1.13 million lines, one stream:

| Phase | In-memory buffer (after the incremental-index fix) | File source, 64 KB blocks |
|---|---|---|
| open + index | 80 ms | 57 ms |
| 20 append polls (200 lines each) | 213 ms | 26 ms |
| include filter, case-insensitive | 272 ms | 284 ms |
| include + exclude | 865 ms | 893 ms |
| include regex | 122 ms | 132 ms |
| search | 82 ms | 132 ms |
| 20 append polls with search + filter | 174 ms | 29 ms |
| 20,000 random `get_line` (cache misses) | n/a (in RAM) | 470 ms, 23 µs each (1489 ms with 256 KB blocks) |
| process memory, current / peak | ~130 MB / ~230 MB | 20 MB / 24 MB |

Maintainer scenario, ten 50 MB logs (5.6 million lines of ~88 bytes) growing on every frame:

| Measure | Before | After |
|---|---|---|
| memory for the ten streams | ~500 MB (up to 1 GB after growth) | 60 MB (45 MB of index + cache) |
| open all ten | not measured | 232 ms |
| poll of all ten per frame, 500 new lines per frame | tens of ms | 3.5 ms |

With 500-byte lines the index is five times smaller, so the same scenario lands near 50 MB total. Sequential scans (filters, search) stay within noise of the in-memory version because they stream 1 MB chunks and borrow UTF-8 text from the chunk; search is ~50 ms slower on 100 MB because the byte-level HEX search now also streams the file.

## Risks / Trade-offs

- [Disk reads instead of memory reads while scrolling] → one 256 KB block per ~500 rows, served from the OS page cache in practice; measured before merging.
- [Network drives and antivirus make small reads slow] → the block size and read-ahead keep reads few and sequential; the handle stays open.
- [Worker thread and UI thread both read the file] → separate handles; results only flow through the channel; no shared mutable state.
- [Filter results arrive progressively] → the counter shows "n so far" with the progress percentage; navigation works on the partial list.
- [The writer replaces the file while a job runs] → the job's `read_exact` fails or the fingerprint check fails; the job result is dropped by generation and a reload starts.
- [A line longer than the cache] → 1 MB cap with a visible marker.

## Migration Plan

1. Land behind no flag: the public API is unchanged. Run the full suite and the benchmark before and after; record resident memory for 10 × 50 MB.
2. Ship in 0.3.0 together with block 1.
3. Rollback: revert the commit; the incremental-index fix stays valid on its own.

## Open Questions

- Cache size per stream: 4 MB (16 blocks) is the starting point; with ten streams that is 40 MB, still under the target. Halve if the measurement says the hit rate is unchanged.
