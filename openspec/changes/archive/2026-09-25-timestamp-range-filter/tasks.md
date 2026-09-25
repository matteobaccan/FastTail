## 1. Engine

- [x] 1.1 Add `detect_timestamp(&str) -> Option<i64>` for the listed formats, allocation-free, with per-stream "last format" hint
- [x] 1.2 Add the per-line timestamp cache with inheritance for continuation lines, lazy fill, reset on truncation; ordered/unordered detection
- [x] 1.3 Fold `time_range` into `is_line_visible`; add `goto_time(millis)` with `partition_point` / linear fallback; visible span computation
- [x] 1.4 Tests: each format, inheritance, range filter with open ends, go-to-time ordered and unordered, disabled state under 50% parse rate

## 2. UI

- [x] 2.1 From/to inputs with validation and the disabled hint in the filter panel; span in the stream status bar
- [x] 2.2 Time form in the Ctrl+G popup (depends on `goto-line`)
- [x] 2.3 i18n keys in five languages; i18n test

## 3. Bench and docs

- [x] 3.1 Add a time-range phase to `benches/filter_bench.rs` (the generator already writes timestamps)
- [x] 3.2 README feature list and comparison table
