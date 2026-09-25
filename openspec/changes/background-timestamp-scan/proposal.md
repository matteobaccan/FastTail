## Why

0.9.0 shipped the time range filter and go-to-time with a known issue written into the CHANGELOG: the first use of either control times every line of the stream on the interface thread (`ensure_timestamps` loops `fill_timestamps` until the cache is complete). On a multi-GB log the window freezes until the whole file has been read. Every other whole-file pass — index, filter, search, levels — already runs on a `scan_job` worker with progress in the stream bar. The maintainer scheduled the fix for 0.9.1.

## What Changes

- New `ScanKind::Timestamps` / `JobSpec::Timestamps` in `scan_job`: the worker detects the timestamp of every line with the same `detect_timestamp` + format hint + inheritance rules as `fill_timestamps`, and sends ordered batches that the engine appends to its `timestamps` cache. The stream bar shows `timing lines 37% (n)` like the other scans.
- The cache is still built on demand (time range, go-to-time), synchronously when the untimed remainder is below the 16 MB job threshold and on the worker above it. Appended lines keep being timed incrementally.
- The time range applies once the cache is complete: while the scan runs, the typed window is kept as pending, the view keeps showing what it showed, and a hint next to the fields says the window will apply when timing finishes.
- Go-to-time on an untimed large stream waits in the Ctrl+G popup with the scan progress and jumps when the cache is complete; closing the popup drops the pending jump, not the scan.
- The scan is resumable: the cache is a prefix of the line index, so a Timestamps job preempted by a filter or search job restarts from the first untimed line instead of from zero. It preempts a Levels job, and it is preempted by nothing while a time window waits on it (see design).
- Filter and search jobs on a stream with a time window stop falling back to the interface thread: the engine applies the window to the lines a job returns, by index, when it drains the batch.
- The "timestamp scan is currently synchronous" caveats are removed from the stream-engine, log-intelligence and search-and-navigation specs, and the CHANGELOG known issue is closed.

## Capabilities

### New Capabilities
- (none)

### Modified Capabilities
- `stream-engine`: Background Scans covers the timestamp scan; the synchronous exception is dropped.
- `log-intelligence`: Timestamp Detection no longer says the first scan runs on the interface thread.
- `filters-and-highlighting`: Timestamp Range Filter describes the pending window while the cache is built.
- `search-and-navigation`: Go To Line waits for the timestamp cache instead of building it synchronously.

## Impact

- `src/scan_job.rs`: `ScanKind::Timestamps`, `JobSpec::Timestamps { inherited, hint }`, `ScanBatch::Timestamps { values, parsed, unordered, hint }`.
- `src/tail_engine.rs`: `request_timestamps()` replacing the synchronous calls in `set_time_range`, `apply_time_range_text`, `resolve_goto` and the append path; pending time window / pending go-to state; drain and finish handling; time-window post-filter of Filter/Search job batches; job priority rules. `ensure_timestamps()` stays as the synchronous API for tests and `benches/filter_bench.rs`.
- `src/ui/dock.rs`: `scan_kind_key` entry, pending hint in `render_time_range`, progress line in the Ctrl+G popup.
- `src/i18n.rs`: new keys in all 16 languages.
- No config or file format change. Target release 0.9.1.
