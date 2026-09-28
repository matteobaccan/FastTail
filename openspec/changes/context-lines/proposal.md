## Why

A filter shows the lines that matter and hides what happened around them. Today the only
way to read the lines before and after a match is "Show in context" (`CTRL + K`), one line
at a time: the stream leaves the filtered view, the user reads, comes back, and does it
again for the next match. When a filter keeps 300 `payment failed` lines, the question
"what came right before each failure" means 300 round trips. `grep -C 3` answers it in one
command, LogFusion offers "lines before / after a filter match", and the post-0.12.0
competitor scan ranked it the highest-value gap for its size (gap 3).

## What Changes

- A per-stream **context lines** setting `N` (0 to 100, default 0 = off), set from a
  `±N` control in the stream bar. With `N > 0` and an active filter, every line that
  passes the filters (a **match**) is shown together with the `N` file lines before it
  and the `N` file lines after it, like `grep -C N`.
- Context lines are taken from the file as it is, **ignoring every filter** (stream
  terms, level, time range, global filter): that is what they are for. A line that is
  both a match and within `N` of another match is shown once, as a match.
- Context rows are drawn **dimmed** (the theme's dim text colour, highlight rules and
  level colours at reduced opacity); matches keep their normal style.
- Non-adjacent groups are separated by a **thin separator rule** drawn between the two
  rows, with a tooltip saying how many lines are hidden there. The separator is drawn,
  not a row: line numbers, selection, copy and navigation are never offset by it.
- **No filter recomputation** when `N` changes or the view scrolls: the matches stay in
  the existing filtered line list; a list of merged line ranges is derived from it in
  one pass and the row mapping uses it (binary search per row, as today).
- Works with the **global filter** (its matches get context like any other), with
  **collapse** (runs are formed over the rows shown, context included), with
  **show in context** (the context view shows every line anyway; leaving it restores the
  view with context), with **follow** (lines appended within `N` of the last match show
  as context at once), and with **background jobs** on files above 16 MB (ranges grow as
  filter batches arrive).
- Search, bookmarks, selection, copy, export, the time delta column and the overview
  strip work on the rows shown, context rows included.
- Saved per stream in the workspace and in session files as `context_lines=N`, written
  only when `N > 0`. No new key in the global `fasttail.ini` settings.

Target release: **0.13.0** (basics and quick wins), per the release plan in
`docs/competitor-analysis.md` section 8. Effort: **S–M**.

### Non-goals

- Different counts before and after a match (`grep -B` / `-A`): one symmetric `N` in this
  change; see the open questions in the design.
- Context in the Find results tab (it keeps listing matching lines only) and in the HEX
  and rendered Markdown views.
- A `--` separator line in exported files: export writes lines, as today.
- Context around search hits of an unfiltered stream: without a filter every line is
  already visible.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `filters-and-highlighting`: adds the Context Lines Around Matches requirement (setting,
  what is shown, dimming, separators, persistence) and Context Lines on Large and Growing
  Files.
- `search-and-navigation`: adds how search, bookmarks, go to line, show in context,
  collapse and the overview strip treat context rows.
- `selection-and-export`: adds what selection, copy and export produce with context rows.

## Impact

- `src/context_lines.rs` (new): `ContextRanges` (merged `[first, last]` line ranges with
  a row prefix sum), built from `filtered_lines` in one pass and extended incrementally.
- `src/tail_engine.rs`: `context_lines: u8` per engine; `visible_lines`, `line_at`,
  `pos_of_line`, `row_of_line_or_next` go through the ranges when `N > 0` and rows are
  filtered; `is_context_row(line)`; ranges extended on filter batches and appends, reset
  with `filtered_lines`.
- `src/scan_job.rs`: `JobSpec::Search` and `JobSpec::Collapse` accept the ranges as the
  visible set (`VisibleSet::Ranges`).
- `src/ui/dock.rs`: `±N` control in the stream bar, dimmed context rows, separator rule
  and tooltip in the normal and wrap layouts, overview strip.
- `src/session.rs` / `src/config.rs`: `context_lines` in `StreamEntry`.
- `src/i18n.rs` (16 languages), help dialog, README, CHANGELOG, tests.
