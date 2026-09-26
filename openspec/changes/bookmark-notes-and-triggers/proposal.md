## Why

A bookmark today is a bare `★`: after an hour of reading a log the user no longer knows why
row 48,213 was marked, and a colleague opening the same session sees only stars. Marking
every line of a recurring event (each `OutOfMemoryError`, each deploy start) means pressing
`CTRL + F2` on every one, and lines appended later are never marked. LogExpert (bookmark
comments and triggers), lnav (comments and tags) and Chipmunk (comments) all cover this;
it is the gap found by the post-0.11.0 competitor scan.

## What Changes

- **Bookmark notes**: a bookmark can carry a single-line note of at most 200 characters,
  added or edited from the row context menu ("Bookmark note…"), shown as the tooltip of the
  row's `★` marker and of the bookmark mark in the overview strip, and marked with `✎` in
  the marker column. Writing a note on a row without a bookmark bookmarks it; clearing the
  note keeps the bookmark; removing the bookmark removes the note.
- Notes are **persisted with the bookmarks**: in `fasttail.ini` section `[bookmarks]` new
  keys `note_<i>_<line>` beside the existing `file_<i>` / `lines_<i>`, and in session files
  new keys `bookmark_note.<line>` in each stream section. Older versions ignore the new keys
  and still read the bookmarks.
- **Auto-bookmarks**: a highlight rule gains a "Bookmark matching lines" option. Every line
  matching an enabled rule with the option — lines already in the file and lines appended
  later — is bookmarked automatically, shown with `☆` (manual bookmarks keep `★`), included
  in `F2` / `SHIFT + F2` navigation and in the overview strip with a dimmer mark.
- Auto-bookmarks are **derived, not stored**: they are recomputed from the rules when a
  file is opened, reloaded or the rules change, capped at 10,000 per stream (a stream-bar
  notice says so), and never count toward the 1,000 saved bookmarks per file. `CTRL + F2`
  on an auto-bookmarked row dismisses it for the session; adding a note turns it into a
  manual bookmark, which is saved.
- Files above 16 MB find their existing auto-bookmarks on a worker thread with progress in
  the stream bar; appended lines are checked as they arrive, like sound alerts.
- New `fasttail.ini` key in each `[highlight_<n>]` rule section: `bookmark` (`true` /
  `false`, default `false`).

Target release: **0.12.0**.

### Non-goals

- Multi-line notes, tags or categories of bookmarks (lnav tags).
- A separate bookmark list panel; notes are edited from the row context menu.
- Triggers other than bookmarking (running tools is already covered by external tools bound
  to rules).
- Persisting auto-bookmarks or dismissals across restarts.
- Exporting notes into copied or saved text.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `search-and-navigation`: Line Bookmarks gain notes and automatic bookmarks with their own
  glyph and dismissal; Bookmark Persistence per File saves notes; new requirements for
  Bookmark Notes and Automatic Bookmarks from Rules.
- `filters-and-highlighting`: highlight rules gain the "Bookmark matching lines" option,
  persisted as `bookmark`.

## Impact

- `src/tail_engine.rs`: `HighlightRule` / `CompiledHighlight` gain `auto_bookmark`; engine
  gains notes map, auto-bookmark set, dismissed set and incremental check on append;
  `reload_from_start` and `set_highlight_rules` recompute.
- `src/scan_job.rs`: new `JobSpec::AutoBookmarks` / `ScanKind::AutoBookmarks` for files
  above 16 MB.
- `src/config.rs`, `src/session.rs`, `src/compressed.rs` (pending bookmarks carry notes).
- `src/ui/dock.rs` (context menu, marker glyphs, tooltip, rule editor checkbox),
  `src/ui/overview_strip.rs`, `src/ui/app.rs` (persisting notes).
- `src/i18n.rs` (all 16 languages), README, CHANGELOG.
