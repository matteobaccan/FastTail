## 1. Parser core (phase 1)

- [x] 1.1 `src/fields.rs`: `FieldParser { Json, Logfmt, Regex(Arc<Regex>) }` plus the `apache` and `syslog` presets as named `Regex` patterns; `FieldSpan`, reused `FieldSpans`; `scan(parser, line, &mut spans)` for each kind (JSON flattening to depth 3, logfmt quoting and `_prefix`, regex named groups); value unescaping on demand
- [x] 1.2 Detection over the first 200 non-continuation lines (≤ 256 KB): JSON at 80 % of lines scanning cleanly, logfmt at 80 % with 3 or more pairs, else none; re-run once when a stream opened with fewer than 200 lines reaches 200
- [x] 1.3 Unit tests: every scanner on fixtures (nested JSON, escapes, arrays, broken JSON, logfmt quoting, bare keys, Apache combined, RFC 3164), detection thresholds, no allocation per line after warm-up (counting allocator test)
- [x] 1.4 `benches/fields.rs`: JSON and logfmt scan throughput on a 100 MB fixture

## 2. Column view (phase 1)

- [ ] 2.1 Engine: parser per stream (done: `set_field_choice`, `parser_generation`), field catalogue (≤ 256 keys) and the row field cache (≤ 1,024 rows, cleared on `reload_generation` or parser change)
- [ ] 2.2 (chip and menu done; `▦ Columns` open) `dock.rs`: parser chip and menu in the stream bar (auto result, JSON, logfmt, regex with a pattern field and invalid flag, Apache, syslog, off); `▦ Columns` toggle, Text view only
- [ ] 2.3 Column header (drag reorder, width drag in character cells, right-click menu: hide, show all, move left / right, reset), cells drawn after the marker / number / delta columns, unparsed and continuation lines across the columns, last column wrapping in wrap mode
- [ ] 2.4 (parser and regex done) Persistence: `fields_parser`, `fields_regex`, `fields_view`, `fields_columns`, `fields_width.<field>` in `StreamEntry`, workspace and session files; round-trip and old-file tests

## 3. Field terms (phase 2)

- [ ] 3.1 `FieldTerm` grammar and evaluation (`=`, `!=`, `~=`, `>`, `>=`, `<`, `<=`, `|` alternatives, quoted literal terms, `Aa` and `.*` toggles, missing field fails); `FilterTerm.field`, `FilterSpec.fields`, one field scan per line per call, text pre-check for `=` / `~=`
- [ ] 3.2 Engine: rebuild the `FilterSpec` and refresh filters when the parser changes; the global filter's terms compiled per stream with that stream's parser
- [ ] 3.3 `ƒ` badge on field terms in the stream bar and the Filters window; tooltip with the parsed key, operator and values
- [ ] 3.4 Tests: each operator; alternatives; missing field on include and exclude sides; quoted literal; stream without a parser keeps text semantics (`a && !b` scenario unchanged); background filter job equal to the synchronous path (thresholds at 0); presets and Find results with field terms; global field term on streams with and without a parser

## 4. Level and timestamp from fields (phase 3)

- [ ] 4.1 Level from the first level field (`level`, `lvl`, `severity`, `log.level`) through `log_level::token_level` (made `pub(crate)`), falling back to `detect_level`; time from the first time field (`ts`, `time`, `timestamp`, `@timestamp`, `t`) through the existing parsers, falling back to `detect_timestamp`
- [ ] 4.2 `JobSpec::Levels` / `JobSpec::Timestamps` carry the parser; parser change resets the level and timestamp caches without rebuilding the line index
- [ ] 4.3 Tests: JSON log with the time inside the object works with the time range, the histogram and the delta column; background and synchronous caches identical

## 5. Search, rules and copy in the column view (phase 3)

- [ ] 5.1 Hit tints and rule spans mapped to cells by byte range (split across cells), whole-row styles on the row, 64-span budget unchanged
- [ ] 5.2 "Copy as shown" writes the shown cells tab-separated in the column view; copy and export keep writing raw lines
- [ ] 5.3 Tests: span mapping across cell boundaries; copy as shown TSV; export unchanged

## 6. Texts and documentation

- [ ] 6.1 New i18n keys (parser names and menu, chip tooltips, column menu, `ƒ` tooltip, regex error, help entries) in all 16 languages; add them to the exhaustive i18n test
- [ ] 6.2 README (feature list, a "Structured logs" section with the field-term syntax, comparison table, FAQ) and CHANGELOG `[Unreleased]`, one entry per phase merged

## 7. Wrap-up

- [ ] 7.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; one PR per phase with Linux and Windows CI green
- [ ] 7.2 Local preview exe for the maintainer before the 0.14.0 release
- [ ] 7.3 If phase 3 is not merged by the 0.14.0 release: move its requirements ("Level and Timestamp from Fields", the cell-span part of "Search, Rules, Copy and Export with Fields") into a follow-up change targeting 0.15.0 before archiving
- [ ] 7.4 After the release, archive the change so `structured-fields` is created and `filters-and-highlighting` gains the modified Combined Filter Terms
