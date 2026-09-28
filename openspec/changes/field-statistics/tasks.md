## 1. Statistics core

- [ ] 1.1 `src/field_stats.rs`: `StatsSource { Field(key), Capture(Regex) }`, value extraction per line, bounded counter (100,000 values, 256-byte values, "other values"), per-level counts
- [ ] 1.2 Numeric detection (90 %, unit suffixes) and log-scale histogram with percentiles within 1 %; min, max, mean
- [ ] 1.3 Time slices (auto width ≤ 240 slices, user choice), top-5 series and per-slice p50 / p95
- [ ] 1.4 Merge of partial results; unit tests on fixtures (counts, cap, percentiles against sorted reference, slices)

## 2. Engine and jobs

- [ ] 2.1 `JobSpec::FieldStats` over the visible entry lines, partial results every 250 ms, progress
- [ ] 2.2 Engine state per stream: restart on filter change, incremental count of appended lines, reset on reload; timing the stream when a time breakdown is opened
- [ ] 2.3 Tests: job result equal to a synchronous count (thresholds at 0); filter change recomputes; append updates counts; truncation resets

## 3. UI

- [ ] 3.1 Statistics dock tab (not saved in the layout): source picker (field list from the catalogue, or regex with invalid flag), N selector, table with bars, numeric summary
- [ ] 3.2 Per-level columns; time-slice line chart with slice picker and hover tooltips, drawn with the egui painter in theme colours
- [ ] 3.3 Click / `CTRL + click` to add include / exclude terms (field term or regex term), notice when 8 terms are used
- [ ] 3.4 Entries in the column header menu, the cell menu and the stream menu; Copy as CSV

## 4. Texts and documentation

- [ ] 4.1 New i18n keys in every language, added to the exhaustive i18n test
- [ ] 4.2 README section (with an example on an access log), help dialog, CHANGELOG `[Unreleased]`

## 5. Wrap-up

- [ ] 5.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PR with Linux and Windows CI green
- [ ] 5.2 Local preview exe for the maintainer before the release
- [ ] 5.3 After the release, archive the change so the `field-statistics` capability is created
