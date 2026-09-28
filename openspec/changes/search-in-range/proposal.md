## Why

A search covers every visible line of the stream. On a day-long log the user often knows
where to look: "after the restart at line 1,204,000", "inside the lines I selected",
"between 14:00 and 14:10", and wants `F3` and the counter to stay there instead of
wrapping into the morning. The time window can narrow the view, but it hides the rest of
the log, which the user still wants to read around the hits. klogg limits a search to a
part of the file (start and end marks); the post-0.12.0 scan lists it among klogg's
strengths. It is small: the search already walks a line range.

## What Changes

- A **search scope** per stream, shown as a chip in the search box, default **Whole
  view** (today's behaviour). Other scopes:
  - **Selection**: from the first to the last selected line when the scope is set;
  - **Lines**: a typed range `1200-5000`, `1200-` (to the end, growing with the file) or
    `-5000`;
  - **Time**: a from / to pair typed as in the time range popup (same formats, same "end
    of the unit typed" rule, the relative forms of `relative-time-windows` when shipped);
  - **From here** / **Up to here**: from the row menu, the clicked line to the end or
    from the start.
- With a scope, only visible lines **inside the scope** are hits: the counter reads
  `[3 / 17 in range]`, `F3` / `SHIFT + F3` wrap within the scope, the results pane and the
  histogram's search lane list only those hits. The **rest of the view stays visible**;
  the overview strip shades the scope.
- The row menu offers "Search in selection" (when rows are selected), "Search from here"
  and "Search up to here"; the chip's `✖` returns to Whole view.
- A Time scope needs timestamps: it is held while the stream is being timed, like the
  time window, and is unavailable on streams without usable timestamps.
- The **Find results tab** gets an optional **time scope** (from / to) applied to every
  stream, so one query can be limited to "14:00–14:10 in all logs".
- Not persisted (a scope is part of an investigation, not of the workspace); no new key
  in `fasttail.ini`.

Target release: **0.13.0** (basics and quick wins), per the release plan in
`docs/competitor-analysis.md` section 8. Priority: **Medium**. Effort: **S**.

### Non-goals

- Several disjoint ranges in one scope, or a scope per bookmark pair.
- Scoping the filters (that is what the time window and filters are for) or highlight
  rules.
- Line scopes in the Find results tab (line numbers differ per stream).
- Scoped search in HEX view (byte offsets) and the rendered Markdown view.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `search-and-navigation`: adds Search Scope and Find Results Time Scope.

## Impact

- `src/tail_engine.rs`: `SearchScope { All, Lines(first, Option<last>), Time(from, to) }`
  per stream; the synchronous search starts at the scope's first line and stops at its
  last (line index lookup), a time scope tests the timestamp cache; held scope while
  timing; counter note.
- `src/scan_job.rs`: `JobSpec::Search` gains the line bounds and the optional time
  bounds.
- `src/find_all.rs`: time bounds for each stream's job; `src/ui/find_results.rs`: the two
  fields.
- `src/ui/dock.rs`: scope chip and editor in the search box, row menu entries;
  `src/ui/overview_strip.rs`: scope shading; `src/ui/timeline_strip.rs`: lane limited to
  the scope.
- Reuses `timestamp::parse_user_time` / `end_of_typed_time`.
- `src/i18n.rs` (16 languages), help dialog, README, CHANGELOG, tests.
- **TUI (0.20.0, PR #132)**: the scope is engine state; the TUI can set it with a
  `:range 1200-5000` command and show it in the status line.
