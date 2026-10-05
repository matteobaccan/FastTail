## Context

The engine builds the filtered view from the include and exclude terms, the level, the
time range and the global filter, in one pass (in the background above 16 MB), and keeps
the visible line index. Context lines and collapse are derived from that index.

## Goals / Non-Goals

**Goals:** hide arbitrary selected lines cheaply, reversibly, persistently per stream.

**Non-Goals:** pattern-based hiding, partial lines.

## Decisions

1. **Ranges of line numbers**, merged and sorted (`Vec<(u64, u64)>`): membership is a
   binary search, so the filter pass stays without allocation per line. *Rejected:* a
   bitmap per line (8 MB per 64 M lines) and storing hidden text (ambiguous with repeated
   lines).
2. **A filter like the others:** hidden ranges are applied in the same pass as the terms,
   so search, counters, the overview strip, export and the time histogram agree with the
   view. Show in context lists them like the other filtered lines.
3. **No hidden line is ever lost silently:** the bar chip stays while any range exists,
   and the list shows each range with its first line's text.
4. **Invalidation:** a reload that changes line numbers (truncation, rotation, rewrite)
   clears the ranges with a notice; appends keep them.
5. **Persistence** as `hidden=a-b,c` per stream, at most 1,000 ranges (more are merged
   to the nearest).

## Risks / Trade-offs

- [Users forget hidden lines] → the always-visible chip and the help in Show in context.
- [Many tiny ranges] → capped and merged.

## Open Questions

- Should hiding also be offered on search results (hide every hit)? Left out.
