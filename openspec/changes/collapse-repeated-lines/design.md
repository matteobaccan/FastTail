## Context

The text view draws `visible_line_count()` rows and maps each row to a file line with
`get_actual_line_idx(row)`: the identity without filters, `filtered_lines[row]` (sorted
line indices, 8 bytes per visible line) with filters. Filters above 16 MB run as a
background `ScanJob` (`src/scan_job.rs`) whose batches stream into `filtered_lines`;
the time window is applied by index when batches are drained. Continuation lines of a
stack trace follow their entry (`is_stacktrace_continuation`, `visible_in_sequence`).
Selection, bookmarks, search hits and the time anchor are stored as file line indices,
so they do not depend on the row mapping. Per-stream view flags (`wrap`, `ansi`,
`timeline`) live in `StreamEntry` (`src/session.rs`), saved in the workspace section of
`fasttail.ini` and in session files.

## Goals / Non-Goals

**Goals:**
- One more mapping layer, *visible lines → rows*, that every row consumer already goes
  through, so filters, search, bookmarks and the delta column need no parallel logic.
- Zero cost when the mode is Off; memory bounded by the number of groups, not lines.
- Never block the UI thread: above 16 MB detection is a background pass.

**Non-Goals:**
- Signature mode, non-consecutive repetitions, collapse in HEX / Markdown / Find results
  (see the proposal).

## Decisions

1. **Compare entries, not lines.** The unit is a visible *entry*: a visible line that is
   not a continuation line plus the visible continuation lines that follow it (a visible
   continuation line with no visible parent is an entry of its own). Two consecutive
   entries are equal when they have the same number of lines and their normalised lines
   are equal pairwise. So N identical stack traces collapse into one trace with `×N`,
   instead of never collapsing (lines alternate header / `at …`) or collapsing the
   `at …` lines inside one trace. Entries longer than 256 lines or 64 KiB of normalised
   text are never collapsed. Alternative rejected: line-by-line runs — they break traces
   apart and make the count meaningless for multi-line entries.
2. **Normalisation.** Both modes drop the leading timestamp (a new
   `timestamp::leading_span(line, hint) -> Option<usize>` returns its byte length, reusing
   the stream's format hint) and trailing whitespace; otherwise every log with a
   timestamp would never repeat. *Numbers* then replaces with one `#` each maximal token
   that is a UUID (8-4-4-4-12 hex), `0x` + hex digits, a hex word of 8 or more characters
   with at least one digit, or a run of decimal digits. Normalisation writes into a
   reused buffer: no allocation per line. Comparison is a byte comparison with the
   previous entry, kept in a second buffer (at most 64 KiB), not a hash: no collisions.
3. **Group table.** `CollapseState { mode, groups: Vec<Group>, open: BTreeSet<usize>,
   tail: RunCursor }` in `src/collapse.rs`, with
   `Group { pos: usize, row: usize, entry_len: u32, count: u32 }` (24 bytes): `pos` is the
   visible position of the first line of the first entry, `row` its row index in the
   collapsed view, `count ≥ 2`. Only groups are stored, so memory is 24 bytes per group,
   at most 12 bytes per visible line in the worst case (every entry repeated twice) and
   near zero on a log with no repetition. A group hides `entry_len × (count − 1)` visible
   lines unless its head line index is in `open` (expanded). `row` is a prefix sum
   rebuilt in O(groups) when a group is opened or closed. `get_actual_line_idx(row)`
   binary-searches `groups` by `row`, then maps the visible position through the existing
   filtered / identity mapping; `get_visible_row_of_line` does the reverse and returns the
   group's head row for a hidden line (`row_of_line_or_group`). With mode Off the engine
   keeps today's code path. `count` saturates at `u32::MAX` and a further repetition
   starts a new group.
4. **When detection runs.** After every filter result is final (synchronous filter, drained
   filter job, append refresh), and on a mode change. Up to 16 MB it runs on the UI thread
   with the same chunked `scan_lines` reader as the filter. Above 16 MB it is a
   `JobSpec::Collapse { mode, hint, visible: Option<Arc<[usize]>> }` job queued after the
   filter and search jobs (the engine runs one job at a time, as today): `None` means
   every line is visible, otherwise it gets a snapshot of `filtered_lines` (a transient
   8 bytes per visible line while the job runs). It emits `ScanBatch::Groups(Vec<Group>)`
   in file order with progress in the stream bar; rows before the first unscanned line are
   collapsed as batches arrive and the view keeps its top line index stable
   (the scroll anchor is the line, not the row). A new filter or mode cancels the job.
   Alternative rejected: detection inside the filter job — the time window is applied
   after that job, by index, so runs would be formed over the wrong lines.
5. **Growing files and follow.** `RunCursor` keeps the visible position of the last
   complete entry, its normalised text and the open group, if any. An append re-runs
   detection from the start of the last entry (it may gain continuation lines), so the
   last group's `count` grows and, in follow mode, the view stays on the last row without
   adding rows. The new lines still count as unseen activity for background tabs.
6. **Reload, truncation, rotation, re-decode.** Whatever bumps `reload_generation` clears
   `groups`, `open` and the cursor and runs detection again. A filter change clears
   `groups` and keeps `open` entries whose head line still heads a group.
7. **Consumers.**
   - Badge: `×N` (thousands grouped; `×1.2M` above 999,999) drawn before the text of the
     head row in every row layout, wrap included; tooltip: lines first–last and, when
     timed, first and last timestamp. Clicking it toggles `open`.
   - Search: the hit list is unchanged (file lines). A row is marked when any line it
     stands for matches. F3 / SHIFT + F3 step through hits; a hit in a hidden line lands on
     the group row and the next step skips the other hits of that closed group, while the
     counter still counts every hit. A jump that must show one exact hidden line (search
     results pane, Find results, go to line, bookmark navigation, time anchor) opens the
     group first.
   - Selection is kept in line indices: selecting a closed group row selects the lines of
     the whole group; copy and export write every underlying line, never the badge.
   - Time delta: for a group row the reference is the previous row's last line; for the
     row after a closed group, the reference is the group's last line.
   - Overview strip and scrollbar work on collapsed rows; marks of hidden lines are drawn
     at their group row.
   - Line numbers show the head line's number.
8. **Persistence.** `StreamEntry.collapse: Option<String>` (`exact` / `numbers`), written
   as `collapse=` only when not off, read by older builds as an unknown key; the open
   groups are not persisted. The toolbar selector and `CTRL + SHIFT + D` set the mode.

## Risks / Trade-offs

- [Rows shift while a large-file job streams groups in] → the scroll anchor is the top line
  index, so the content under the user does not move; only the scrollbar length changes.
- [A long run of distinct lines costs a full scan per mode change on a 20 GB file] → it is
  a background job with progress and cancel on the next change, like a filter.
- [Numbers mode can merge lines the user considers different (`port 80` / `port 443`)] →
  the badge tooltip gives the line range and expanding shows every line; Exact is there.
- [Snapshot of `filtered_lines` doubles that vector during a job] → only while the job runs
  and only with a filter active; without filters the job needs no list.
- [Windows file sharing] → unchanged: the job reads through the same shared-read source as
  the filter job.

## Migration Plan

Additive. Streams load with mode Off unless `collapse=` is present; older builds ignore
the key. Rollback: remove the option; saved keys are ignored.

## Decisions (maintainer, 2026-09-27)

- Besides the normal copy (every underlying line), the row context menu offers
  **"Copy as shown"**: one line per selected row, a collapsed row written as its first
  entry followed by ` ×N`.
- The mode is set both from the toolbar selector and with `CTRL + SHIFT + D`.
