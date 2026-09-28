## 1. Tags

- [ ] 1.1 Tag parser (`#` + 1..32 of letters, digits, `-`, `_`, `.`; at least one letter; trailing `.` dropped; case-insensitive) shared by engine and UI
- [ ] 1.2 `TailEngine::bookmark_tags(idx)`, `bookmark_next_tagged(from, tag)` with filters and wrap-around
- [ ] 1.3 Tag chips in the marker and overview-strip tooltips; `#tag` in the go-to popup with suggestions and the "no bookmark tagged" message
- [ ] 1.4 Tests: parser edge cases (`#42`, `#db.`, `a#b`, 33 characters, non-ASCII letters); tagged navigation under a filter and wrap-around

## 2. Report

- [ ] 2.1 `src/bookmark_report.rs`: model (stream, bookmark, note, tags, timestamp, context), Markdown writer with dynamic fence length, merged overlapping context, 2,000-char cut, ANSI handling as export
- [ ] 2.2 Orders `stream` and `time`; tag filter; automatic bookmarks optional, capped at 1,000 per stream with a "left out" line
- [ ] 2.3 UI-thread path up to 20,000 context lines; worker path above it with its own shared-read handle, progress and Cancel
- [ ] 2.4 Dialog: context 0..20, automatic bookmarks, tag filter, order, Copy (disabled above 4 MB) and Save (`.md`, native dialog); entries in the stream menu and main menu
- [ ] 2.5 `report_context`, `report_auto`, `report_order` in `[general]`
- [ ] 2.6 Tests: golden Markdown for one and two streams; fence with backticks in a line; overlapping context; bookmark without timestamp in time order; line past end of a rewritten file; compressed stream

## 3. Texts and documentation

- [ ] 3.1 New i18n keys (menu items, dialog labels, messages) in all 16 languages; add them to the exhaustive i18n test
- [ ] 3.2 README (Bookmarks and notes: tags and the report; go-to popup; comparison table) and CHANGELOG `[Unreleased]`

## 4. Wrap-up

- [ ] 4.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PR with Linux and Windows CI green
- [ ] 4.2 Local preview exe for the maintainer before the release
- [ ] 4.3 After the release, archive the change so `search-and-navigation` and `selection-and-export` gain the new requirements
