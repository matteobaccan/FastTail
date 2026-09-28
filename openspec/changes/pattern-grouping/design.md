## Context

`collapse.rs` already normalises a line (leading timestamp removed with
`timestamp::leading_span`, trailing whitespace trimmed, and in Numbers mode UUIDs, `0x`
values, long hex words and digit runs masked) into a reused buffer, and groups consecutive
equal entries in a `Collapse` scan job fed with the visible lines. The engine keeps
`filtered_lines`, the timestamp cache (`Vec<i64>`, when the stream is timed) and the level
cache. `FilterSpec` is the single visibility function shared by the synchronous path and
every job.

## Goals / Non-Goals

**Goals:**
- A bounded summary of "what kinds of lines are here" for a file of any size, without
  blocking the UI thread and without per-line memory beyond what exists.
- Clicking a pattern filters to exactly the entries counted for it.

**Non-Goals:**
- Cross-stream patterns, persistence, field-aware clustering (see the proposal).

## Decisions

1. **Drain, fixed depth.** Each entry's first line is normalised (collapse's Numbers masks
   plus IPv4 / IPv6), split on whitespace into at most 128 tokens (first 4,096 bytes), and
   routed through a tree: level 1 by token count, level 2 by the first token (or `<*>` when
   it holds a digit), level 3 by the second token; a leaf holds up to 100 templates. The
   entry joins the template of equal length with the highest share of equal tokens when
   that share is ≥ 0.5 (tokens that differ become `<*>`), otherwise it creates one. These
   are Drain's published defaults (depth 4, similarity 0.5, 100 children). *Rejected:*
   edit-distance clustering (O(n·k) comparisons of whole strings, too slow on millions of
   lines); exact normalised-text grouping (what collapse does — splits
   `user alice logged in` from `user bob logged in`).
2. **Learning and classification are separate.** Learning mutates the tree; the pattern
   filter needs a stable answer. When learning completes (or every 500 ms while it runs)
   the engine publishes an `Arc<PatternSnapshot>`: the templates and the routing tables,
   immutable. `PatternSnapshot::classify(line)` walks it without inserting and returns the
   template id; the same function computes the counts shown in the tab at the end of a
   learning pass (a second, read-only pass is not needed: the learning pass records the
   final id of each template and merges counts when a template generalises). The pattern
   filter holds the snapshot, so the filtered view shows exactly the entries counted for
   the clicked pattern. *Rejected:* filtering by a regex built from the template — lines
   matching the regex but clustered elsewhere would make counts and view disagree.
3. **What is an entry.** As in collapse: a visible line that is not a stack-trace
   continuation, with its continuation lines. Only the first line is clustered;
   continuation lines follow their entry through `visible_in_sequence`, so filtering to a
   pattern shows whole stack traces.
4. **Scope and relearning.** The job is fed the visible lines under every filter except
   the pattern filter itself (the engine passes a `FilterSpec` without `patterns`).
   Changing another filter cancels the job and relearns; setting or clearing the pattern
   filter does not. The pattern filter is dropped when a relearn produces a new snapshot
   (ids are not stable across snapshots) and the chip says so.
5. **Per-pattern statistics.** Count, first and last line, first and last timestamp, and
   64 sparkline buckets over the stream's time span (or over line numbers when untimed):
   buckets are rebinned by pairs when the span doubles, like the histogram. About 600
   bytes per pattern including the template; 2,000 patterns ≈ 1.2 MB per stream, only while
   the tab exists. Beyond 2,000 templates new entries are counted in one "other patterns"
   row.
6. **UI.** A dock tab per stream ("Patterns · app.log"), not saved in the layout, like
   Find results. Virtualized table; columns count, share, sparkline, template, first seen,
   last seen; click on a header sorts. Click on a row sets the pattern filter to that
   pattern, `CTRL + click` toggles it in the set, the row menu offers "Hide this pattern",
   "Copy template" and "Search this template's fixed words". The chip `⧉ N patterns` in the
   stream bar lists them in its tooltip.

Threads and memory: learning runs on a `Patterns` scan job for every file size (the tab is
opened on demand, so there is no reason to block the UI thread even on small files);
classification for the filter runs where filters run (UI thread up to 16 MB, `Filter` job
above). Memory: the tree and statistics above, nothing per line. Growing files: appended
visible lines are fed to the engine's tree incrementally on the UI thread (bounded by the
append batch), and a new snapshot is published at most every 500 ms. Truncation, rotation
and rewrite discard the tree and relearn. The job opens its own read handle with the
engine's sharing flags, so Windows writers are not blocked.

Performance target: at least 50 MB/s per core on a typical 120-byte-per-line log (bench on
the synthetic 200 MB log).

## Risks / Trade-offs

- [Over-generalised templates (`<*> <*> <*>`)] → the first token is only masked when it
  holds a digit, and templates of different token counts never merge; a template made only
  of `<*>` is shown last with a hint.
- [Patterns change while the tab is read on a growing log] → snapshots at most every
  500 ms, and the list keeps its sort and scroll position across snapshots.
- [Pattern filter dropped on relearn] → announced in the chip tooltip and the tab header.

## Open Questions

- Should the similarity threshold be a setting (0.4 to 0.7)?
- Should "other patterns" be clickable (filter to entries in no listed pattern)?
