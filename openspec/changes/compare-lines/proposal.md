## Why

"Why did this request fail when the one before succeeded?" Two long log lines (a JSON
payload, a SQL statement, a stack of key=value pairs) differ in one field buried in 400
characters; two runs of the same job differ in a few lines out of a thousand. Today the
user copies both into a diff tool. LogExpert, Chipmunk and IDE log consoles offer a
compare; the post-0.12.0 competitor scan found no desktop tailer that makes it one
click, which makes it a cheap differentiator next to FastTail's selection and bookmarks.

## What Changes

- **Compare two lines**: with exactly two rows selected in a stream, the row menu offers
  "Compare selected lines"; across streams, "Mark for compare" on a row of one stream and
  "Compare with marked line" on a row of another (or the same) stream.
- **Compare two regions**: with a contiguous or non-contiguous selection in one stream
  marked, "Compare selection with marked selection" in another stream (or a later part
  of the same stream) compares the two lists of lines. Each side is capped at 20,000
  lines.
- A **Compare tab** (docked, not saved in the layout) shows the two sides side by side:
  for two lines, a word-level diff with changed tokens highlighted, and long lines
  wrapped; for regions, a line diff (added, removed, changed rows aligned) with
  word-level highlights inside changed rows, a change count, and `F7` / `SHIFT + F7` to
  jump between changes. Each side's header names the stream and line range; double-click
  on a row focuses that line in its stream.
- **Ignore options** in the tab bar, applied before diffing: the leading timestamp (on by
  default), numbers, hex ids / UUIDs, whitespace amount, case.
- **JSON aware**: when both lines are (or end with) a JSON object, a "Compare as JSON"
  toggle pretty-prints both with sorted keys before the diff, reusing the inline JSON
  detection.
- "Copy as unified diff" copies the result as `diff -u` text.

Target release: **0.13.0** (basics and quick wins), per the release plan in
`docs/competitor-analysis.md` section 8. Priority: **Low–Medium**. Effort: **M**.

### Non-goals

- Comparing whole files of any size (a region is capped; a file diff tool does that
  better).
- Three-way compare, merging, or editing either side.
- Persisting compare tabs in the workspace or sessions.
- Structured field-by-field compare (possible once `structured-fields` exists).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `selection-and-export`: new requirements Compare Lines, Compare Regions and Compare
  Ignore Options.

## Impact

- `src/compare.rs` (new, UI-free): normalisation for the ignore options, tokeniser,
  line and word diff through the `similar` crate (MIT / Apache-2.0, no dependencies),
  JSON canonicalisation through `serde_json`, unified diff output.
- `src/ui/compare_tab.rs` (new): `FastTailTab::Compare(id)`, side-by-side virtualized
  rows, change navigation, options bar.
- `src/ui/dock.rs`: row menu items, the compare mark (one per window, shown as a marker
  in the gutter), `F7` / `SHIFT + F7` in the Compare tab.
- `src/tail_engine.rs`: text of selected lines (reuses `copy_selection_text` logic, ANSI
  stripped per mode).
- `Cargo.toml`: `similar`.
- `src/i18n.rs` (16 languages), README, CHANGELOG.
- **TUI (0.20.0, PR #132)**: `compare.rs` is UI-free; the TUI can render the unified
  diff in a pane.
