## 1. Detection core

- [x] 1.1 `timestamp::leading_span(line, hint) -> Option<usize>`: byte length of the leading timestamp, same parsers and hint as `detect_timestamp`; unit tests for each supported format
- [x] 1.2 `src/collapse.rs`: `CollapseMode { Off, Exact, Numbers }` with `name` / `from_name`; normalisation into a reused buffer (timestamp prefix, trailing whitespace, Numbers masking of UUID, `0x` hex, hex words of 8+ characters with a digit, decimal runs); unit tests for each token class and non-masked words
- [x] 1.3 Entry splitting with `is_stacktrace_continuation`, run detection with a byte comparison against the previous entry (entry caps 256 lines / 64 KiB, `count` saturating at `u32::MAX`), `Group { pos, row, entry_len, count }`, prefix `row` rebuild, `row → visible position` and `visible position → row` mapping; unit tests including traces, orphan continuation lines and caps

## 2. Engine integration

- [x] 2.1 `TailEngine` holds `CollapseState`; `visible_line_count`, `get_actual_line_idx`, `get_visible_row_of_line` go through it when the mode is not Off (today's path unchanged when Off); add `row_span(row)` (first and last line of a row)
- [x] 2.2 Recompute after every final filter result and on mode change; incremental resume from the last entry on append; clear on `reload_generation` bump; keep open groups whose head still heads a group after a filter change
- [x] 2.3 `JobSpec::Collapse` / `ScanKind::Collapse` / `ScanBatch::Groups` for streams above 16 MB, queued after filter and search jobs, cancelled by a new filter or mode, progress in the stream bar, scroll anchored on the top line index
- [x] 2.4 `row_time_delta` uses the last line of the previous row's group; selection of a group row selects its lines; `copy_selection_text`, `export_visible` and selection elapsed time use every underlying line
- [x] 2.5 Search navigation: hit on a hidden line lands on the group row and the next step skips that group's hits; `request_jump`, go to line, bookmark navigation and timeline jumps expand the group of a hidden target line
- [x] 2.6 Engine tests: synchronous and background detection give identical groups (thresholds at 0); filter change regroups; follow append grows the last count without new rows; truncation resets; copy and export of a collapsed row; delta after a group

## 3. UI and persistence

- [x] 3.0 Row context menu "Copy as shown" (first entry of a collapsed row plus ` ×N`, one line per selected row) with a test
- [x] 3.1 Stream toolbar selector (Off / Exact / Numbers) and `CTRL + SHIFT + D` cycling, ignored while a text field has focus; hidden in HEX and Markdown views
- [x] 3.2 `×N` badge (thousands grouped, `×1.2M` above 999,999) before the text of the head row in normal and wrap layouts, tooltip with line range and time span, click toggles expansion; match and bookmark markers on group rows; line number of the head line
- [x] 3.3 Overview strip and scrollbar over collapsed rows, marks of hidden lines at their group row
- [x] 3.4 `StreamEntry.collapse` read and written as `collapse=exact|numbers` (only when not Off) in workspace and session files; round-trip test and old-file test (no key → Off)

## 4. Texts and documentation

- [x] 4.1 New i18n keys (mode names, toolbar tooltip, badge tooltip, progress label, help entry) in all 16 languages; add them to the exhaustive i18n test
- [x] 4.2 Help dialog entry for `CTRL + SHIFT + D`
- [x] 4.3 README (feature list, view options) and CHANGELOG `[Unreleased]` entry

## 5. Wrap-up

- [x] 5.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PR with Linux and Windows CI green
- [ ] 5.2 Local preview exe for the maintainer before the 0.12.0 release
- [ ] 5.3 After the release, archive the change so `log-intelligence`, `search-and-navigation` and `selection-and-export` gain the new requirements
