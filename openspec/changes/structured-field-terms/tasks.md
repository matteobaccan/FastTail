## 1. Field terms

- [ ] 1.1 `FieldTerm` grammar and evaluation (`=`, `!=`, `~=`, `>`, `>=`, `<`, `<=`, `|` alternatives, quoted literal terms, `Aa` and `.*` toggles, missing field fails); `FilterTerm.field`, `FilterSpec.fields`, one field scan per line per call, text pre-check for `=` / `~=`
- [ ] 1.2 Engine: rebuild the `FilterSpec` and refresh filters when the parser changes; the global filter's terms compiled per stream with that stream's parser
- [ ] 1.3 `ƒ` badge on field terms in the stream bar and the Filters window; tooltip with the parsed key, operator and values
- [ ] 1.4 Tests: each operator; alternatives; missing field on include and exclude sides; quoted literal; stream without a parser keeps text semantics (`a && !b` scenario unchanged); background filter job equal to the synchronous path (thresholds at 0); presets and Find results with field terms; global field term on streams with and without a parser

## 2. Level and timestamp from fields

- [ ] 2.1 Level from the first level field (`level`, `lvl`, `severity`, `log.level`) through `log_level::token_level` (made `pub(crate)`), falling back to `detect_level`; time from the first time field (`ts`, `time`, `timestamp`, `@timestamp`, `t`) through the existing parsers, falling back to `detect_timestamp`
- [ ] 2.2 `JobSpec::Levels` / `JobSpec::Timestamps` carry the parser; parser change resets the level and timestamp caches without rebuilding the line index
- [ ] 2.3 Tests: JSON log with the time inside the object works with the time range, the histogram and the delta column; background and synchronous caches identical

## 3. Search, rules and copy in the column view

- [ ] 3.1 Hit tints and rule spans mapped to cells by byte range (split across cells), whole-row styles on the row, 64-span budget unchanged
- [ ] 3.2 "Copy as shown" writes the shown cells tab-separated in the column view; copy and export keep writing raw lines
- [ ] 3.3 Tests: span mapping across cell boundaries; copy as shown TSV; export unchanged
- [ ] 3.4 Character selection kept inside the cell it starts in (from `partial-line-selection` task 2.6)
- [ ] 3.5 Wrap mode: the last column wraps, the other cells on the row's first line; scroll and jumps through the wrap layout

## 4. Texts and documentation

- [ ] 4.1 New i18n keys (`ƒ` tooltip, copy as shown in columns, help entries) in all 16 languages; exhaustive i18n test
- [ ] 4.2 README "Structured logs" section with the field-term syntax, FAQ; CHANGELOG `[Unreleased]`

## 5. Wrap-up

- [ ] 5.1 `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test` green; Linux and Windows CI green
- [ ] 5.2 Local preview exe for the maintainer before the 0.15.0 release
- [ ] 5.3 After the release, archive the change so `structured-fields` and `filters-and-highlighting` gain the requirements
