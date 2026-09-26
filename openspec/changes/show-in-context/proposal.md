## Why

A filtered stream (include / exclude terms, minimum level, time range, and in 0.12.0 the
global filter) shows the interesting line but hides what happened just before and after
it. Today the user must clear the filter, find the line again in the full log (by
remembering its number and pressing `CTRL + G`), read around it, then retype or reload the
filter and scroll back to where they were; on a file above 16 MB each step recomputes the
filter in the background. Grafana ("log context") and klogg offer this in one click; the
competitor scan after 0.11.0 listed it as a gap.

## What Changes

- **Show in context** on a row of a filtered stream: a row context-menu item and
  `CTRL + K` (on the selected row) switch the stream to a **context view** that shows every
  line of the file, unfiltered, with that line centred, selected and marked.
- A **banner** above the rows says the filters are suspended and offers "Back to filtered
  view"; `Esc` (when no text field has the keyboard) does the same. Returning restores the
  filtered view exactly: same top row, same selection, same follow state, with no filter
  recomputation.
- The stream's filter terms, level, time range and the global filter are **kept, not
  cleared**: the filtered line list stays in memory and keeps growing with appended lines
  while the context view is shown.
- The **Find results** tab gains the same action on a result (context menu and
  `CTRL + K`), which brings the stream to the front in context view on that line.
- Bookmarks, selection, copy and export work in the context view on the lines it shows;
  follow mode is paused on entry. The action is unavailable in HEX and rendered Markdown
  views and when the stream has no active filter.
- No new key in `fasttail.ini`: the context view is transient and never persisted.

Target release: **0.12.0**.

### Non-goals

- A ±N lines peek panel or tooltip (rejected in the design).
- Showing context around a hit of a stream's unfiltered search: without a filter every
  line is already visible.
- Context across streams (a merged timeline of several files).
- Saving the context view in sessions or the workspace.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `search-and-navigation`: adds the Show In Context requirement (entry from a row, the
  banner and return, behaviour with follow, search, bookmarks, selection, large files) and
  the Show In Context from Find Results requirement.

## Impact

- `src/tail_engine.rs`: a transient context state per stream; the row mapping
  (`visible_line_count`, `get_actual_line_idx`, `get_visible_row_of_line`) reads "rows
  filtered" instead of "filter active", while `filtered_lines` keeps being maintained.
- `src/ui/dock.rs`: row context-menu item, `CTRL + K`, banner, `Esc`, restore of the
  scroll position; stream bar filter fields end the context view when edited.
- `src/ui/find_results.rs`, `src/ui/app.rs`: result context menu and shortcut, jump in
  context view.
- `src/i18n.rs`: new strings in all 16 languages; help dialog entry.
- README, CHANGELOG, tests.
