## 1. Tags

- [x] 1.1 Tag parser (`#` + 1..32 of letters, digits, `-`, `_`, `.`; at least one letter; trailing `.` dropped; case-insensitive) shared by engine and UI
- [x] 1.2 `TailEngine::bookmark_tags(idx)`, `bookmark_next_tagged(from, tag)` with filters and wrap-around
- [x] 1.3 Tag chips in the marker and overview-strip tooltips; `#tag` in the go-to popup with suggestions and the "no bookmark tagged" message
- [x] 1.4 Tests: parser edge cases (`#42`, `#db.`, `a#b`, 33 characters, non-ASCII letters); tagged navigation under a filter and wrap-around

## 2. Report

- [x] 2.1 `src/bookmark_report.rs`: model (stream, bookmark, note, tags, timestamp, context), Markdown writer with dynamic fence length, merged overlapping context, 2,000-char cut, ANSI handling as export
- [x] 2.2 Orders `stream` and `time`; tag filter; automatic bookmarks optional, capped at 1,000 per stream with a "left out" line
- [x] 2.3 Context read in bounded per-frame steps with progress and Cancel (see design D3, "as implemented")
- [x] 2.4 Dialog: context 0..20, automatic bookmarks, tag filter, order, Copy (disabled above 4 MB) and Save (`.md`, native dialog); entries in the stream menu and main menu
- [x] 2.5 `report_context`, `report_auto`, `report_order` in `[general]`
- [x] 2.6 Tests: golden Markdown for one and two streams; fence with backticks in a line; overlapping context; bookmark without timestamp in time order; line past end of a rewritten file; compressed stream

## 3. Texts and documentation

- [x] 3.1 New i18n keys (menu items, dialog labels, messages) in all 16 languages; add them to the exhaustive i18n test
- [x] 3.2 README (Bookmarks and notes: tags and the report; go-to popup; comparison table) and CHANGELOG `[Unreleased]`

## 4. Wrap-up

- [ ] 4.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PR with Linux and Windows CI green
- [ ] 4.2 Local preview exe for the maintainer before the release
- [ ] 4.3 After the release, archive the change so `search-and-navigation` and `selection-and-export` gain the new requirements
