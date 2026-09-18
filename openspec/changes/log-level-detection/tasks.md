## 1. Engine

- [ ] 1.1 Add `LogLevel` and `detect_level(&str) -> LogLevel` with the fixed token table, syslog `<n>` mapping and the first-96-bytes / whole-word rules
- [ ] 1.2 Add the per-line level cache (`Vec<u8>`), filled on append and lazily, reset on truncation; per-level counters
- [ ] 1.3 Fold `min_level` and `show_unknown_levels` into `is_line_visible` after exclude/include; recompute filtered lines when they change
- [ ] 1.4 Tests: detection table (positive and negative cases), cache reset, min-level filter with unknown toggle, counters

## 2. Theme and UI

- [ ] 2.1 Level palette per theme in `src/theme.rs`
- [ ] 2.2 Fallback colouring in `match_highlight` when no user rule matches; Settings toggle persisted in `fasttail.ini`
- [ ] 2.3 Level selector and unknown toggle in the stream bar, counters in the stream status bar; i18n keys in five languages; i18n test

## 3. Bench and docs

- [ ] 3.1 Add a min-level filter phase to `benches/filter_bench.rs`
- [ ] 3.2 README feature list and comparison table
