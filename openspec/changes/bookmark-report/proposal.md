## Why

After an incident the user has bookmarked the lines that matter and written a note on each
(`first OOM`, `deploy start`, `retry storm`). To share the finding they still copy every
line by hand, look up its timestamp, paste the note next to it and add a few surrounding
lines so the reader understands it. FastTail keeps the notes only in its own tooltip and
settings; nothing turns them into a write-up. logana exports bookmarks as Markdown and Jira
text, and lnav lets the user comment and tag lines and search by tag. The post-0.12.0
competitor scan ranks this gap 16 (value Medium, effort S).

Notes are also flat text: with twenty bookmarks on three streams there is no way to say
"these four are the deploy" and find them again.

## What Changes

- **Bookmark report**: "Bookmark report…" in the stream menu (this stream) and in the main
  menu (all open streams) opens a small dialog, then writes a **Markdown incident report**
  of the manual bookmarks: per stream, each bookmark with its line number, timestamp (when
  the line has one), note, tags and the bookmarked line with **N context lines** before and
  after in a fenced code block. The report goes to the **clipboard** or to a **`.md` file**.
- Dialog options: context lines (0 to 20, default 3), include automatic bookmarks (off by
  default), a tag filter (only bookmarks carrying the chosen tags), order by stream or by
  time across streams. The last choices are remembered.
- **Bookmark tags**: every `#word` in a note (`#deploy`, `#db-pool`, letters, digits, `-`,
  `_`, `.`, up to 32 characters) is a tag. Tags are shown as chips in the note tooltip,
  listed in the report and usable to **filter the report** and to **jump**: the go-to popup
  (`CTRL + G`) accepts `#deploy` and jumps to the next bookmark carrying that tag, across
  the focused stream, wrapping around.
- New `fasttail.ini` keys in `[general]`: `report_context` (default 3), `report_auto`
  (default `false`), `report_order` (`stream` or `time`, default `stream`).

Target release: **0.13.0** (basics and quick wins), per the release plan in
`docs/competitor-analysis.md` section 8. Priority: **Medium**. Effort: **S**.

### Non-goals

- Jira wiki markup, HTML or PDF reports (Jira and GitHub both accept Markdown; HTML export
  of lines is `export-formats`).
- Multi-line notes, a tag editor or tags stored separately from the note text: a tag is
  just text in the note, so it is saved, restored and edited exactly as notes are today.
- A bookmark list panel.
- Changing what copy and export write: the 0.12.0 rule that notes are not written by copy
  or export stays; the report is a separate, explicit output.
- Tag search across all streams in the Find results tab (a possible follow-up).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `search-and-navigation`: new requirement Bookmark Tags (tags in notes, tooltip chips,
  `#tag` in the go-to popup).
- `selection-and-export`: new requirement Bookmark Report.

## Impact

- `src/bookmark_report.rs` (new): report model, Markdown writer with fence escaping, tag
  parser shared with the UI.
- `src/tail_engine.rs`: `bookmark_tags(idx)`, `bookmark_next_tagged(from, tag)`; reading
  context lines through the existing `get_line`.
- `src/ui/dock.rs` (stream menu item, tag chips in the tooltip, `#tag` in the go-to popup),
  `src/ui/app.rs` (main menu item, report dialog, clipboard, save dialog, worker hand-off).
- `src/config.rs`: `report_context`, `report_auto`, `report_order`.
- `src/i18n.rs` (16 languages), README (Bookmarks and notes section), CHANGELOG.
- **TUI (0.20.0, PR #132)**: the report builder is UI-free and can back a `:report` command
  that writes the file; `#tag` works in the TUI's go-to prompt with the same parser.
