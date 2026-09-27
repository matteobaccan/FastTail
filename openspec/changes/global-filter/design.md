## Context

Each stream filters on its own `FilterSpec` (`src/scan_job.rs`): up to 8 include terms
(ANDed) and 8 exclude terms (ORed) compiled with the stream's case and regex toggles, the
minimum level, the unknown-level toggle and the stack-trace continuation rule. The same
`FilterSpec` is cloned into the background filter and search jobs (files above 16 MB) and
into the Find results jobs (`find_all.rs`), and the time window is applied on top of it.
Filter presets can copy one stream's state to all streams, but only once. There is no
state shared by every stream.

## Goals / Non-Goals

**Goals:**
- One set of include / exclude terms that every stream, including those opened later,
  combines with its own filter, with a single on / off switch.
- Exactly the same visibility in the synchronous path, the background jobs, the Find
  results and the counters: one evaluation function, not a second filter pass.
- Changing the global filter never blocks the UI thread on a large file.

**Non-Goals:**
- Per-stream opt-out, linked stream filters, a global level or time filter, and saving
  the global filter in sessions or presets (see the proposal).

## Decisions

1. **Global terms live inside `FilterSpec`.** A new `global: Option<Arc<FilterSpec>>`
   field: the global set is itself a `FilterSpec` (terms, per-term regexes, its own case
   and regex flags, no level), so no separate type is needed. `excluded(line)` is true
   when a stream *or* global exclude term matches, `included(line)` when the stream's
   *and* the global include terms all match; `matches` and `visible_in_sequence` build on
   them, so a continuation line follows its entry unless a stream or global exclude term
   matches it. Alternative rejected: a second filtering pass over `filtered_lines` — it
   would double the work on large files and break the continuation rule.
2. **One compiled set shared by all streams.** The app compiles the global terms once per
   edit into an `Arc<FilterSpec>` and hands the same `Arc` to every engine
   (`TailEngine::set_global_filter(Option<Arc<FilterSpec>>)`), so N streams cost one
   compilation and 8 bytes each. `None` when the filter is off or has no non-empty term.
3. **Recomputation per engine uses the existing path.** `set_global_filter` rebuilds the
   engine's `FilterSpec` and calls `refresh_filters`, which recomputes synchronously up to
   16 MB and starts a background filter job above; the search refresh follows as it does
   for a stream filter change. Edits in the bar are debounced by 300 ms before they are
   pushed to the engines, so typing a term does not restart N background jobs per key.
4. **Streams opened later** receive the current `Arc` in the same frame, before the dock
   draws them: every frame the app hands the current set to each engine, a no-op when it
   already holds the same `Arc`, so every way a stream appears (open, workspace or
   session restore, standard input, zip entry) is covered in one place.
5. **UI.** A `🌐` toggle in the toolbar (and `CTRL + SHIFT + H`, consumed before the dock
   is drawn, as `CTRL + SHIFT + F` is, because egui matches shortcuts with extra Shift
   ignored) shows or hides the global filter bar under the menu bar: an on / off switch,
   the include and exclude term rows (the `+` / `✖` rows of the Filters window, reused),
   `Aa` and `.*`. Hiding the bar does not switch the filter off. While the filter is on
   and has a term, each stream bar shows a `🌐` badge (tooltip: the global terms), and
   the Filters window lists the global terms above the streams.
6. **Persistence.** `[global_filter]` in `fasttail.ini`: `enabled`, `case_sensitive`,
   `regex`, `bar_open`, `include.N`, `exclude.N` (N = 1..8), values through
   `filter_preset::ini_value`. Written only when it differs from the default (off, empty),
   so an untouched installation keeps its ini unchanged.

Threads and memory: compiling the terms and pushing the `Arc` run on the UI thread (≤ 16
regexes, microseconds); recomputation follows the stream filter rules (UI thread up to
16 MB, worker above). Memory: one global `FilterSpec` for the whole app plus one `Arc` per stream;
no per-line cost beyond the existing `filtered_lines`. Growing files filter their appended
lines with the same `FilterSpec`; a rotated or truncated file rebuilds with it; nothing
touches the file itself, so Windows sharing is unchanged.

## Risks / Trade-offs

- [A global term hides everything in a stream that never contains it] → the `🌐` badge
  and the existing empty-view message ("No lines match the active filters") make the cause
  visible; the switch turns it off in one click.
- [N large streams each start a background job when the global filter changes] → the
  300 ms debounce; jobs are already bounded to one per stream and cancelled by the next
  change.
- [`CTRL + SHIFT + H` collides with a future `CTRL + H`] → consumed by the app first, as for
  `CTRL + SHIFT + F`; noted in the help.
