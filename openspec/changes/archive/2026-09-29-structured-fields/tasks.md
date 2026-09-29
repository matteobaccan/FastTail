## 1. Parser core (phase 1)

- [x] 1.1 `src/fields.rs`: `FieldParser { Json, Logfmt, Regex(Arc<Regex>) }` plus the `apache` and `syslog` presets as named `Regex` patterns; `FieldSpan`, reused `FieldSpans`; `scan(parser, line, &mut spans)` for each kind (JSON flattening to depth 3, logfmt quoting and `_prefix`, regex named groups); value unescaping on demand
- [x] 1.2 Detection over the first 200 non-continuation lines (≤ 256 KB): JSON at 80 % of lines scanning cleanly, logfmt at 80 % with 3 or more pairs, else none; re-run once when a stream opened with fewer than 200 lines reaches 200
- [x] 1.3 Unit tests: every scanner on fixtures (nested JSON, escapes, arrays, broken JSON, logfmt quoting, bare keys, Apache combined, RFC 3164), detection thresholds, no allocation per line after warm-up (counting allocator test)
- [x] 1.4 `benches/fields.rs`: JSON and logfmt scan throughput on a 100 MB fixture

## 2. Column view (phase 1)

- [x] 2.1 Engine: parser per stream (`set_field_choice`, `parser_generation`), field catalogue (≤ 256 keys) and the row field cache (≤ 1,024 rows, cleared on `reload_generation` or parser change)
- [x] 2.2 `dock.rs`: parser chip and menu in the stream bar (auto result, JSON, logfmt, regex with a pattern field and invalid flag, Apache, syslog, off); `▦ Columns` toggle, Text view only
- [x] 2.3 (wrap: one row per line in 0.14.0; the wrapping last column moved to `structured-field-terms`) Column header (drag reorder, width drag in character cells, right-click menu: hide, show all, move left / right, reset), cells drawn after the marker / number / delta columns, unparsed and continuation lines across the columns, last column wrapping in wrap mode
- [x] 2.4 Persistence: `fields_parser`, `fields_regex`, `fields_view`, `fields_columns`, `fields_width.<field>` in `StreamEntry`, workspace and session files; round-trip and old-file tests

Sections 3 to 5 of the original plan (field terms; level and timestamp from fields;
search, rules and copy in the column view) moved to `structured-field-terms` (0.15.0).

## 6. Texts and documentation

- [x] 6.1 New i18n keys (parser names and menu, chip tooltips, column menu, regex error) in all 16 languages; add them to the exhaustive i18n test
- [x] 6.2 README (feature list, a "Structured logs" section with the field-term syntax, comparison table, FAQ) and CHANGELOG `[Unreleased]`, one entry per phase merged

## 7. Wrap-up

- [x] 7.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; one PR per phase with Linux and Windows CI green
- [x] 7.2 Local preview exe for the maintainer before the 0.14.0 release
- [x] 7.3 (done: `structured-field-terms`) If phase 3 is not merged by the 0.14.0 release: move its requirements ("Level and Timestamp from Fields", the cell-span part of "Search, Rules, Copy and Export with Fields") into a follow-up change targeting 0.15.0 before archiving
- [x] 7.4 After the release, archive the change so `structured-fields` is created and `filters-and-highlighting` gains the modified Combined Filter Terms
