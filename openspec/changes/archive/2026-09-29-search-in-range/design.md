## Context

The stream search (`refresh_search_from`, `start_search_job`) walks the visible lines from
a start line to the end, on the UI thread up to 16 MB and in `JobSpec::Search` above,
storing at most 1,000,000 hits. The selection is a `BTreeSet<usize>` of line indices (or
"all"). The time range popup reads text with `timestamp::parse_user_time` and
`end_of_typed_time`, and holds a window while the stream is timed (`PendingWindow`). The
Find results tab runs one `Search` job per stream (`src/find_all.rs`).

## Goals / Non-Goals

**Goals:** narrow hits and navigation to a range without hiding lines; faster searches
when the range is small; the same parsing as the time range.

**Non-Goals:** disjoint ranges, scoped filters, persistence.

## Decisions

### D1. Scope model
`SearchScope::All | Lines { first, last: Option<usize> } | Time { from, to }` per stream.
Selection, From here and Up to here are converted to `Lines` when set (the selection may
change later; the scope does not follow it). The chip reads `⌖ 1,200–5,000`,
`⌖ 1,200–end`, `⌖ 14:00–14:10`.

### D2. Search cost
A `Lines` scope starts the walk at `first` and stops after `last`, so a 5,000-line scope
of a 10 GB file searches in milliseconds on the UI thread; the 16 MB job threshold applies
to the bytes of the scope, not of the file. A `Time` scope walks every visible line and
tests the timestamp cache (timestamps are not guaranteed monotonic); it runs in the job
above 16 MB like today.

### D3. Hits outside the scope
They are not hits: no marker, no tint, not counted. *Alternative:* dim them — rejected,
two kinds of hits confuse the counter and `F3`.

### D4. Growing files and reloads
`Lines` with an open end and `Time` scopes pick up appended lines; a closed `Lines` scope
does not. A truncation, rotation or reload resets the scope to Whole view (line numbers no
longer mean the same lines) and says so in the search box.

### D5. Find results time scope
Two optional fields beside the query; each stream's job skips lines outside the bounds
using that stream's timestamp cache (timing a stream first when needed, as the time window
does). A stream without usable timestamps is reported as skipped for the time scope.

## Risks / Trade-offs

- [User forgets a scope is set and misses hits] → the chip is tinted with the accent
  colour and the counter says "in range".
- [Time scope on an untimed 5 GB log] → held with `⏳` until timing ends, as the window.

## Migration Plan

Additive; nothing is persisted.

## Open Questions

- Should the scope survive a restart in session files? Proposed: no.
- Shortcut for "Search in selection"? Proposed: none at first; the row menu and the chip.
