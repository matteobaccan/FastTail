## Context

A stream's visible rows are `filtered_lines` (line indices in file order) whenever
`is_filter_active()` is true, and every line otherwise; `visible_line_count`,
`get_actual_line_idx` and `get_visible_row_of_line` switch on that flag. `filtered_lines`
costs 8 bytes per visible line and, above 16 MB, is rebuilt by a background filter job
after every filter change. Jumps go through `request_jump` (select, pause follow, centre
via `pending_jump`); the row context menu (`row_context_menu` in `dock.rs`) holds the time
anchor and external tools; Find results jump through `apply_find_jump`.

## Goals / Non-Goals

**Goals:**
- One action from a filtered row to the same line in the full log, and one action back
  to the filtered view exactly as it was.
- Entering and leaving never recompute the filter or the search, on any file size.

**Non-Goals:**
- Peek panel, cross-stream context, persistence of the context view (see the proposal).

## Decisions

1. **UX: a temporary unfiltered view, not a peek panel.** Entering switches the stream's
   own rows to every line, centres the chosen line, selects it and draws a context marker
   (`◆` in the marker column) on it; a banner above the rows reads "Filters suspended:
   showing line N in context" with a "Back to filtered view" button. The user gets the
   full tools of the main view (scroll as far as needed, search tint, bookmarks, copy,
   external tools). *Rejected:* a ±N lines peek panel or tooltip (Grafana's first
   version): a fixed N is always too few or too many, it needs its own renderer,
   selection and scroll, and on wide lines it duplicates the main view in a smaller box.
2. **Transient state in the engine.** `context: Option<ContextView>` with `line` (the
   anchor), `saved_top_line`, `saved_follow`, `saved_selection` (the `BTreeSet` and
   anchor, moved, not copied). A new `rows_filtered()` = `is_filter_active() &&
   context.is_none()` replaces `is_filter_active()` in the three row-mapping functions,
   the overview strip and export / copy of visible lines; filter maintenance
   (`recompute_filtered_lines_from`, appended lines, background jobs) keeps using
   `is_filter_active()`, so `filtered_lines` stays exact and keeps growing while hidden.
   `filter_generation` is bumped on enter and leave so the row caches rebuild.
3. **Leaving restores, never recomputes.** `leave_context()` restores the selection and
   follow state and sets a pending top-row scroll to `saved_top_line` (or the nearest
   visible line after it if it was dropped by a truncation). If follow was on at entry,
   return goes back to the tail.
4. **What ends the context view.** The banner button, `Esc` while the rows have the
   keyboard (a text field, popup or the Go-to box consumes `Esc` first), `CTRL + K` again,
   and any change of the stream's filter (terms, toggles, level, time range, preset,
   global filter): the new filter then applies as usual and the anchor line is kept
   centred when it is still visible. A reload, truncation or rotation leaves it without
   restoring (the saved rows no longer mean anything). Switching to HEX or Markdown view
   leaves it first.
5. **Search stays as computed.** The stream search is not re-run on enter or leave:
   `F3` / `SHIFT + F3` and the results pane walk the hits of the filtered view, which are
   all shown in the context view; the query tint is drawn on every row as today. Typing a
   new query in the context view runs it over the rows shown (all lines), and leaving
   re-runs it over the filtered rows (a normal search refresh, background above 16 MB).
6. **Shortcut `CTRL + K`.** Free in `src/ui` (used: A C F G L O T W, Num1..9, F1..F9);
   egui also matches it on `CTRL + SHIFT + K`, which is unused. Acts on the selection
   anchor, or the row under the context menu. A user external tool bound to `CTRL + K`
   would clash: the built-in action wins on a filtered stream, and the help and README
   list the key.
7. **Find results.** A result's context menu offers "Show in context" and `CTRL + K` on
   the selected result does the same: `apply_find_jump` brings the tab forward and calls
   `enter_context(line)` instead of `request_jump`, so a result hidden by the stream's
   current filter is shown exactly rather than as "the next visible line".

Threads and memory: everything runs on the UI thread in O(1) plus one binary search; no
worker is started. Memory: one `ContextView` per stream (a few words plus the moved
selection); `filtered_lines` is not duplicated. Files above 16 MB: entering while a
filter job is still running is allowed (the context view needs only the line index,
unavailable while `index_pending`); the job keeps filling `filtered_lines` in the
background and the return shows whatever it has found. Growing files: appended lines show
at the bottom of the context view and are filtered into `filtered_lines` as usual; follow
is paused on entry. Nothing opens or locks the file, so Windows sharing is unchanged.

## Risks / Trade-offs

- [Code reading `is_filter_active()` for "what rows are shown" is missed] → grep every
  call site and classify it (row mapping vs filter maintenance); an integration test
  compares visible rows, export and overview in context view to an unfiltered stream.
- [The user forgets they are in context view] → the banner stays while it lasts, the
  stream bar filter fields are drawn dimmed with the "suspended" tooltip.
- [A 30-million-line filtered view: restoring the top row] → by line index with one
  binary search in `filtered_lines`, no scan.
