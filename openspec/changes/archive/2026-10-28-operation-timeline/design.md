## Context

Each stream has a timestamp cache (the effective timestamp per line, continuation lines
inheriting their entry's), a level cache, and the visible line list under its filters.
`structured-fields` (0.14.0) extracts fields per line without allocation and builds
`FieldTerm`s; `field-statistics` (0.14.0) proposes a source picker "field or regex with
one capture group". Background jobs (`src/scan_job.rs`) walk line ranges and send batches.

## Goals / Non-Goals

**Goals:** a Gantt answer for "what ran when and how long" over a multi-GB log in one
pass; memory bounded by the number of operations; live extension while following.

**Non-Goals:** span trees, cross-stream operations, automatic id detection.

## Decisions

### D1. Operation model
An operation is the set of visible entries whose id source yields the same value. Per id:
first and last line, first and last timestamp (min and max, since lines of one id may be
out of order across threads), line count, worst level, the name (from the name source on
the first entry). Ids are cut at 128 bytes. ~200 bytes per operation: 100,000 operations
≈ 20 MB. *Alternative:* keep every line index per id — rejected, memory grows with lines.

### D2. Sources
Id: a parser field (`structured-fields`) or a regex with exactly one capture group, run
on the entry's first line. Name: none, a field, or the first line's text after the
timestamp (cut at 120 characters). The picker is the `field-statistics` one when present.

### D3. Drawing
A virtualised table (egui rows, as the Find results tab) with the bar column on the right.
Axis: from the earliest start to the latest end of the listed operations; wheel zooms
around the pointer, drag pans, double-click resets. Bars coloured by worst level with the
theme's level colours; minimum width 2 px; untimed operations are listed without a bar.
Sorting and narrowing run on the in-memory list (100,000 rows sort in a few ms).

### D4. Job and growth
`JobSpec::Operations { source, name, filter }` over the visible lines (timing the stream
first if needed); partial maps merge every 250 ms. While the file grows, appended visible
lines update their operation in place (the engine keeps the map). A filter change, a
source change, a truncation or a rotation rebuilds. Past 100,000 ids new ids are counted
("n more operations not tracked") and ignored.

### D5. From an operation to the lines
"Filter to this operation" adds `key=value` (field source) or a regex include term
`\Q…\E`-escaped around the captured value in the pattern's context (regex source).
*Alternative:* a hidden per-stream "operation filter" — rejected, a visible term is easier
to understand and remove.

## Risks / Trade-offs

- [High-cardinality ids such as per-line UUIDs produce 100,000 one-line operations] → the
  cap, the minimum duration box, and a hint when most operations have one line.
- [Clock skew between threads makes an operation look longer] → min / max timestamps are
  shown as logged; no correction.

## Migration Plan

Additive; nothing is persisted.

## Open Questions

- Should the tab offer "operations still open" (no line for N minutes while following)?
  Proposed: later, after feedback.
- Share one "source picker" component with `field-statistics` regardless of which lands
  first? Proposed: yes.
