## 1. Engine

- [ ] 1.1 Add `detect_timestamp(&str) -> Option<i64>` for the listed formats, allocation-free, with per-stream "last format" hint
- [ ] 1.2 Add the per-line timestamp cache with inheritance for continuation lines, lazy fill, reset on truncation; ordered/unordered detection
- [ ] 1.3 Fold `time_range` into `is_line_visible`; add `goto_time(millis)` with `partition_point` / linear fallback; visible span computation
- [ ] 1.4 Tests: each format, inheritance, range filter with open ends, go-to-time ordered and unordered, disabled state under 50% parse rate

## 2. UI

- [ ] 2.1 From/to inputs with validation and the disabled hint in the filter panel; span in the stream status bar
- [ ] 2.2 Time form in the Ctrl+G popup (depends on `goto-line`)
- [ ] 2.3 i18n keys in five languages; i18n test

## 3. Bench and docs

- [ ] 3.1 Add a time-range phase to `benches/filter_bench.rs` (the generator already writes timestamps)
- [ ] 3.2 README feature list and comparison table
