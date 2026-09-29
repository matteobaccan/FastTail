## Context

A filtered stream shows `filtered_lines` (sorted line indices, 8 bytes per visible line),
maintained by `recompute_filtered_lines_from` up to 16 MB and by a background `Filter`
`ScanJob` above, with appended lines filtered incrementally. Every row consumer goes
through four functions of `TailEngine`: `visible_lines`, `line_at(pos)`,
`pos_of_line(line)` and `row_of_line_or_next`, which switch on `rows_filtered()`
(`is_filter_active() && context.is_none()`, so "Show in context" suspends the filter
without touching `filtered_lines`). Collapse (`src/collapse.rs`) adds one more mapping,
visible position → row, on top of those four. Search (`JobSpec::Search { filter, .. }`)
and collapse (`JobSpec::Collapse { visible, .. }`) jobs receive the visible set as a
`FilterSpec` or as a snapshot of `filtered_lines`.

## Goals / Non-Goals

**Goals:**
- `grep -C N` in the viewer: every match with its `N` neighbours, groups separated.
- Changing `N`, scrolling and following never re-run the filter; the extra state is
  bounded by the number of groups, not the number of lines.
- One more layer under the existing row mapping, so collapse, search, bookmarks,
  selection, the delta column and the overview strip need no parallel logic.

**Non-Goals:**
- Asymmetric before / after counts, context in Find results, HEX and Markdown (see the
  proposal).

## Decisions

1. **Context is derived from the matches, never filtered.** The matches are
   `filtered_lines`, unchanged. A new `ContextRanges { n, ranges: Vec<Range>, rows: usize }`
   in `src/context_lines.rs` holds merged ranges `Range { first: usize, last: usize,
   row: usize }` (24 bytes): `first = match − N` (clamped at 0), `last = match + N`
   (not clamped: the end of the file is applied when reading), `row` the prefix sum of the
   rows of the ranges before it. Two ranges merge when they touch or overlap
   (`next.first <= prev.last + 1`). Building is one pass over `filtered_lines`, O(matches).
   *Rejected:* materialising the shown lines as a second `Vec<usize>` — up to
   `(2N + 1) × 8` bytes per match (1.6 GB for 10 million matches at `N = 10`) and a full
   rebuild on every change of `N`.
2. **Row mapping.** When `N > 0` and `rows_filtered()`, the visible set is the union of
   the ranges: `visible_lines()` = rows of all ranges (the last range clipped to
   `total_lines`); `line_at(pos)` = binary search on `row`, then `first + (pos − row)`;
   `pos_of_line(line)` = binary search on `first`, then a bounds check. Both are
   O(log groups). Collapse keeps working on visible positions, so it sits on top without
   change. `is_match(line)` (for dimming) is a binary search in `filtered_lines`, only for
   the rows drawn.
   *Rejected:* a flag per shown line telling match from context: memory per row for
   what one binary search answers.
3. **What a context line is.** Any file line within `N` lines of a match, whatever the
   filters say about it: stream terms, level, time window and global filter are ignored for
   context, as `grep -C` ignores the pattern for its context lines. A stack-trace
   continuation line of a match is already a match (`visible_in_sequence`), so the `N`
   lines after it count from the end of the entry. `N` counts file lines, not entries.
   *Rejected:* context limited to lines the level or time filter would show (with
   `>= ERROR` it would add nothing), and counting entries instead of lines (a 200-line
   stack trace would make `N = 1` unreadable).
4. **Drawing.** Context rows use `theme.text_dim()` for the text and draw rule, label,
   ANSI and level colours at 55 % opacity; match rows are unchanged. Between two ranges
   that are not adjacent in the file, a 1 px rule in `theme.border_color()` at 40 %
   opacity is drawn along the top edge of the first row of the later range, in the normal
   and the wrap layouts; hovering it shows "N lines hidden". *Rejected:* a separator row
   (`--` as grep prints): it would need a row that maps to no line, which breaks
   `get_actual_line_idx`, selection, copy, the delta column and wrap anchoring.
5. **Changing `N`.** Rebuilds the ranges from `filtered_lines` on the UI thread (one pass,
   at most 30 ms for 10 million matches, measured in a unit test with a time bound of
   100 ms) and bumps `filter_generation` so row caches rebuild; the filter is not re-run.
   The top line of the view stays in place (`view_top_line` / `pending_top_row`, as for
   collapse). Above 10 million matches the rebuild runs on a worker thread
   (`std::thread::spawn`, the result swapped in on the next poll), so the frame never
   blocks; the previous ranges stay in use meanwhile.
   *Rejected:* a `ScanJob` kind for the rebuild: it reads no file, and queuing it
   behind a filter or timestamp job would delay a cheap in-memory pass.
6. **Large files.** While a background filter job streams `ScanBatch::Lines`, each batch
   extends the ranges incrementally (`ContextRanges::extend(&new_matches)`: the first new
   match may merge into the last range), so context appears with the matches. The job
   itself is unchanged. Memory: 24 bytes per group, at most 24 bytes per match and at most
   `24 / (2N + 2)` bytes per file line; freed when `N` goes back to 0.
   *Rejected:* a separate context pass after the filter job ends: on a multi-GB file
   context would only appear at 100 %.
7. **Growing files.** Appended lines are filtered as today; new matches extend the ranges.
   Because the last range's `last` is not clamped, lines appended within `N` after the last
   match become visible as context without any recomputation. Follow stays on the last row.
   *Rejected:* clamping `last` to the file end and re-extending it on append: a special
   case for exactly the lines a follower watches.
8. **Search and collapse jobs.** `JobSpec::Search.filter` and `JobSpec::Collapse.visible`
   become a `VisibleSet` enum: `All`, `Filter(FilterSpec)`, `Lines(Arc<[usize]>)` and the
   new `Ranges(Arc<[Range]>)`. With `Ranges` the worker reads the lines in order and
   evaluates only those inside a range (a cursor over the sorted ranges, O(1) per line),
   so a search covers the rows shown, context included, and collapse forms runs over them.
   Changing `N` therefore refreshes the search (background above 16 MB, as for any filter
   change) but never the filter. *Rejected:* searching the matches only — a hit drawn on a
   context row that `F3` skips is confusing.
9. **Show in context.** `rows_filtered()` is false in the context view, so the ranges are
   bypassed and every line shows; they are kept and apply again, unchanged, on leave.
   *Rejected:* dimming context rows inside the context view: every line is shown
   there, so the distinction would only add noise.
10. **Reload, truncation, rotation, re-decode.** Whatever clears `filtered_lines` clears
    the ranges; they are rebuilt as the filter result arrives.
    *Rejected:* adjusting the ranges in place: after a truncation line indices no
    longer mean the same lines.
11. **Persistence and UI.** `StreamEntry.context_lines: u8`, written as `context_lines=N`
    only when `N > 0`; older builds ignore the key. The `±N` control is a small drag value
    in the stream bar next to the collapse selector, enabled in Text view only, with a
    tooltip; it is shown dimmed (and has no effect) while the stream has no active filter.
    *Rejected:* a global default in `[general]`: context belongs to the question being
    asked, like the filter it goes with.

Threads and memory: range building and mapping run on the UI thread (one pass on a change
of `N`, O(log groups) per drawn row); the filter, search and collapse jobs keep running on
their worker. Memory per stream: 24 bytes per group. Windows file sharing is unchanged:
context lines are read through the same shared-read `FileSource` and block cache.

## Risks / Trade-offs

- [Dense matches make the view nearly the whole file] → intended; the separator only
  appears where lines are hidden, and `N = 0` restores the plain filtered view instantly.
- [A context line outside the time window looks like a filter bug] → context rows are
  dimmed and the `±N` tooltip says that context ignores the filters.
- [Collapse over context rows merges a context line with a match] → collapse compares
  text, not roles; the group's head keeps the style of its first line. Documented.
- [Row count changes while a background filter job streams] → the scroll anchor is the
  top line index, as for collapse, so the content under the user does not move.

## Migration Plan

Additive. Streams load with `N = 0` unless `context_lines=` is present; older builds ignore
the key. Rollback: remove the control; saved keys are ignored.

## Open Questions

- Asymmetric `B` / `A` counts (`grep -B 2 -A 10`)? The range structure supports it at no
  cost; the question is only the UI (two drag values or `2/10` in one field).
- A keyboard shortcut to step `N` (`ALT + [` / `ALT + ]` are free)? Not included until
  the maintainer picks one.
- Should `N` also apply in the headless print mode (`--context N`)? The `headless-print`
  change proposes it with the same definition.
