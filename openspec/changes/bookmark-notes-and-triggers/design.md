## Context

Bookmarks live in `TailEngine::bookmarks: BTreeSet<usize>` with `bookmarks_dirty` /
`bookmarks_generation`; `app.rs` copies a dirty set into `FastTailConfig::set_bookmarks`
(`[bookmarks]` `file_<i>` / `lines_<i>`, capped by `MAX_BOOKMARKS_PER_FILE = 1000` and
`MAX_BOOKMARK_FILES = 50`). Sessions store `bookmarks=1,2,3` per stream section
(`StreamEntry::bookmarks`). Compressed streams hold restored bookmarks in
`pending_bookmarks` until the index covers them. `reload_from_start` (truncation,
rotation, rewrite) clears them. The row context menu (`row_context_menu` in `dock.rs`)
only opens when there are external tools or a time anchor.

Highlight rules (`HighlightRule` → `CompiledHighlight`) are global, persisted in
`[highlight_<n>]`. Appended lines are checked by `check_sound_alerts` (throttled to one
sound per 250 ms, stops at the first match) and `collect_tool_hits` (every appended line,
bounded queue). Files above `JOB_THRESHOLD_BYTES` (16 MB) scan on a `ScanJob` worker.

## Goals / Non-Goals

**Goals:** single-line notes on bookmarks, persisted backward-compatibly; rules that
bookmark existing and appended matching lines, visibly different from manual ones, bounded
in memory and never blocking the UI on large files.

**Non-Goals:** tags, multi-line notes, a bookmark panel, persisted auto-bookmarks.

## Decisions

### D1. Notes: `BTreeMap<usize, String>` beside the bookmark set
`TailEngine::bookmark_notes` holds notes only for bookmarked lines (invariant: key ⊂
`bookmarks`). A note is trimmed, newlines and tabs replaced by spaces, cut to 200
characters (chars, not bytes). Worst case 1,000 × ~800 bytes ≈ 0.8 MB per file; typical is
a few KB. Any note change sets `bookmarks_dirty` and bumps `bookmarks_generation`.
*Alternative:* `BTreeMap<usize, Option<String>>` replacing the set — rejected, it churns
every caller of `bookmarks`.

### D2. Persistence keys
- `fasttail.ini` `[bookmarks]`: `note_<i>_<line>=<text>` written with
  `filter_preset::ini_value` (keeps edge spaces and quotes). Loader collects them after
  `lines_<i>`, drops notes whose line is not in `lines_<i>`. `set_bookmarks` takes
  `&[(usize, Option<String>)]`; `config.bookmarks` becomes `Vec<(PathBuf, Vec<usize>,
  BTreeMap<usize, String>)>` (or a small struct).
- Session stream section: `bookmark_note.<line>=<text>`; `StreamEntry` gains
  `bookmark_notes: BTreeMap<usize, String>`.
Old FastTail builds ignore the unknown keys and keep reading `lines_<i>` / `bookmarks`;
the next save by an old build drops the notes (accepted).
*Alternative:* one `notes_<i>` key with an escaped list — rejected, a single escaping bug
loses every note of the file.

### D3. Auto-bookmarks are a separate, derived set
`TailEngine::auto_bookmarks: BTreeSet<usize>` and `dismissed_auto: BTreeSet<usize>`.
`is_bookmarked` = manual ∪ (auto − dismissed); F2 navigation and the overview strip use the
union; the marker glyph is `★` for manual, `☆` for auto-only. Auto-bookmarks are never
written to `fasttail.ini` or sessions (they are recomputed from the rules), so they do not
count toward the 1,000 saved per file. Cap: `MAX_AUTO_BOOKMARKS = 10_000` per stream
(16 bytes/entry in a BTreeSet ≈ 0.4 MB worst case with node overhead). Past the cap no new
auto-bookmark is added (the first 10,000 in file order are kept, later appends are
ignored) and the stream bar shows "auto-bookmarks capped at 10,000".
*Alternative:* insert auto matches into `bookmarks` — rejected: they would be saved, hit
the 1,000 cap, and could not be told apart or recomputed when a rule is turned off.
*Alternative:* ring buffer keeping the newest 10,000 — rejected, jumps from the start of
the file would silently lose marks; a stable set is easier to explain.

### D4. Manual interaction with auto rows
- `CTRL + F2` on an auto-only row adds it to `dismissed_auto` (in memory, cleared on
  reload or rule change). On a row that is both manual and auto, it removes the manual one
  and the row falls back to `☆`.
- Adding a note to an auto-only row inserts it into the manual set (becomes `★`, saved,
  counts toward 1,000).
- "Clear bookmarks" clears manual bookmarks and notes and dismisses every current
  auto-bookmark; lines appended later can still be auto-bookmarked.

### D5. Finding auto-bookmarks
`CompiledHighlight` gains `auto_bookmark: bool`; the engine keeps
`has_auto_bookmark_rules`. Matching uses the same test as `check_sound_alerts`
(regex / case-sensitive contains / lower-case contains) extracted into
`CompiledHighlight::is_match(&str)`, on the raw line regardless of filters.
- **≤ 16 MB**: on open, reload and `set_highlight_rules`, scan synchronously on the UI
  thread with `scan_lines` (same budget as today's synchronous filter).
- **> 16 MB**: new `JobSpec::AutoBookmarks { rules: Vec<CompiledHighlight> }` (only the
  flagged, enabled ones; `Regex` is `Clone + Send`) and `ScanKind::AutoBookmarks`,
  emitting `ScanBatch::Lines` in file order; the worker stops once it has sent 10,000
  lines. Progress shows in the stream bar like other scans; a new rule set or reload bumps
  the job generation so stale batches are dropped. The job starts after the index job
  (`index_pending` false) and covers `[0, indexed_lines)`.
- **Appended lines**: `collect_auto_bookmarks(prev_lines_count)` runs on each append next
  to `collect_tool_hits` on the UI thread, for every new line (no throttle), only when a
  flagged rule exists. Lines beyond a running job's range are handled here, so none is
  checked twice or missed. Cost per append is one match per flagged rule per new line,
  the same order as the tool-hit check.
- Compressed streams run the scan once extraction and indexing settle, like pending
  bookmarks. Standard input: appended-line check only; nothing persisted.

### D6. Truncation, rotation, rewrite
`reload_from_start` already clears manual bookmarks; it also clears notes, auto-bookmarks
and dismissals, then reruns D5 on the new content. Restored bookmarks whose file shrank
below the largest saved index are discarded with their notes (existing rule).

### D7. UI
- Row context menu always opens on a Text-view row: "Bookmark note…" (opens a one-line
  text field, 200-char limit, `Enter` saves, `ESC` cancels) and "Remove bookmark" on a
  bookmarked row. The existing tool/anchor items follow a separator.
- Marker column: `✎` replaces `★` when a note exists; hovering the marker shows the note.
  Search match `▶` keeps priority as today.
- Overview strip: auto marks in the bookmark colour at 50 % alpha; hovering a manual mark
  with a note shows the note.
- Rule editor: checkbox "Bookmark matching lines" beside the sound alert combo.

## Risks / Trade-offs

- [A broad rule (e.g. `INFO`) bookmarks everything] → 10,000 cap plus the capped notice;
  auto marks are dimmer so manual ones stay visible.
- [UI-thread cost of the synchronous scan for files just under 16 MB] → same budget
  already accepted for filters; skipped entirely when no rule has the option.
- [Old builds drop notes on save] → documented in CHANGELOG; bookmarks themselves survive.
- [Line indices shift if a file is rewritten with the same size] → already handled:
  `reload_from_start` drops everything.
- Windows file sharing is unaffected: the worker opens the file with the same shared
  read mode as other scan jobs.

## Migration Plan

No migration: new keys default to empty / `false`. Rollback to an older build keeps
bookmarks and rules and loses notes and the `bookmark` rule flag.

## Open Questions

- Should notes also be included when copying rows or exporting (non-goal for now)?
- Is 10,000 the right auto-bookmark cap, or should it be a setting?
