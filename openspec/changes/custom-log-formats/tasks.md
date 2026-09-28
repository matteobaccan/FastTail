## 1. Format model

- [ ] 1.1 `src/log_format.rs`: `LogFormat` with patterns (`RegexSet` + `Regex`es), field roles, level map, entry start, globs, columns, samples; load and validate the `formats` folder (256 formats max), errors per file
- [ ] 1.2 Writer for `*.fasttail-format.ini` through `filter_preset::ini_value`; round-trip test
- [ ] 1.3 `timestamp::parse_with_format` for the strftime subset and `epoch_s` / `epoch_ms`; tests per directive, year-less formats, invalid input
- [ ] 1.4 Built-in Apache / nginx and syslog formats replacing the `structured-fields` presets; `fields_parser=apache|syslog` mapped to them

## 2. Engine integration

- [ ] 2.1 `FieldParser::Format(Arc<LogFormat>)` in `src/fields.rs`; captures, level map and timestamp format in the level and time field paths
- [ ] 2.2 `EntryRule { Heuristic, Pattern }` per stream replacing the static `is_stacktrace_continuation` in the filter, timestamp, level and collapse paths and their `scan_job` specs; rebuild on format change
- [ ] 2.3 Detection: glob-matched formats, then user formats, then JSON / logfmt; 80 % rule and tie-breaks; skipped when `fields_parser` is saved; fallback when a saved format is missing
- [ ] 2.4 Tests: detection order and ties, entry start on a multi-line fixture (filter keeps whole entries, timestamps inherited, collapse groups entries), level map, custom timestamp feeding the time range and histogram, synchronous and job paths identical

## 3. UI and persistence

- [ ] 3.1 Log formats dialog: list (built-in locked, invalid files with errors), new / duplicate / delete / import / export
- [ ] 3.2 Pattern editor with live preview (captures, match ratio, unmatched lines, time and level read, µs per line, entry count warning)
- [ ] 3.3 Row menu "Create format from these lines…"; parser menu and chip list formats by name
- [ ] 3.4 `fields_parser=format:<name>` in `StreamEntry`; round-trip and missing-format tests

## 4. Texts and documentation

- [ ] 4.1 New i18n keys (dialog, editor, errors, menu entries) in all 16 languages; add them to the exhaustive i18n test
- [ ] 4.2 README (log formats section with an example file, comparison table), a sample format under `docs/`, CHANGELOG `[Unreleased]`

## 5. Wrap-up

- [ ] 5.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PR with Linux and Windows CI green
- [ ] 5.2 Local preview exe for the maintainer before the release
- [ ] 5.3 After the release, archive the change so the `log-formats` capability is created
