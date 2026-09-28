## Why

A log that repeats itself (a retry loop, a poller, the same warning every 100 ms, the same
stack trace thrown on every request) buries the lines that differ: the user scrolls past
thousands of identical rows, and a filter cannot help because the noise is also the thing
being watched. Today FastTail shows every visible line as its own row. Grafana's dedup
modes (None / Exact / Numbers / Signature) show the demand; the post-0.11.0 competitor
scan picked this for 0.12.0.

## What Changes

- A per-stream **Collapse repeated lines** option with three modes: **Off** (default),
  **Exact** and **Numbers**, chosen from the stream toolbar and cycled with
  `CTRL + SHIFT + D` while the stream has the keyboard.
- Runs of **consecutive visible entries** (a line plus its stack-trace continuation lines)
  that are equal are shown as **one group**: the first entry, with a `×N` badge on its
  first row. *Exact* compares the text after the leading timestamp; *Numbers* also masks
  decimal numbers, `0x` hex, hex words of 8 or more characters and UUIDs. Identical stack
  traces therefore collapse as whole entries.
- Clicking the badge **expands** the group into its lines and collapses it again.
- Runs are formed **over the visible lines**, after the filters (stream, global, level,
  time window); a filter change recomputes them. Search, bookmarks, go to line, selection,
  copy, export, line numbers, the time delta column, the overview strip, wrap and follow
  mode are all defined over collapsed rows (see the specs). While following a growing
  file, a repetition at the end increases the last group's count instead of adding rows.
- Files above 16 MB detect runs on a worker thread after the filter pass, with progress in
  the stream bar; the view stays usable, uncollapsed, meanwhile.
- The mode is saved per stream in the workspace and in session files as
  `collapse=exact|numbers` (written only when not off). No new key in the global
  `fasttail.ini` settings.

Target release: **0.12.0**.

### Non-goals

- Grafana's *Signature* mode (collapse lines that share a shape with different words):
  possible later, needs a tokenizer and a stricter definition.
- Collapsing non-consecutive repetitions (a "top repeated messages" table).
- Collapse in the HEX and rendered Markdown views, and in the Find results tab.
- A user-defined mask (regex of parts to ignore).

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `log-intelligence`: adds the Collapse Repeated Lines requirement (modes, entry
  comparison, badge, expansion, persistence, large and growing files) and how the time
  delta column treats a group.
- `search-and-navigation`: adds how search hits, bookmarks, go to line, the search results
  pane and the overview strip treat lines inside a collapsed group.
- `selection-and-export`: adds what selecting, copying and exporting a collapsed row
  produce.

## Impact

- `src/collapse.rs` (new): entry normalisation (timestamp prefix, number masking), run
  detection state, group table and row mapping.
- `src/tail_engine.rs`: collapse mode per engine; `visible_line_count`,
  `get_actual_line_idx`, `get_visible_row_of_line`, `row_time_delta`, copy and export go
  through the group table; recomputation after the filter and on append, reset on reload.
- `src/scan_job.rs`: `ScanKind::Collapse` / `JobSpec::Collapse` and a `Groups` batch.
- `src/timestamp.rs`: the byte length of the leading timestamp.
- `src/ui/dock.rs`: toolbar mode selector, `×N` badge and tooltip, expansion, markers on
  group rows, overview strip, wrap layout.
- `src/session.rs` / `src/config.rs`: `collapse` key in `StreamEntry`.
- `src/i18n.rs` (16 languages), help dialog, README, CHANGELOG, tests.
