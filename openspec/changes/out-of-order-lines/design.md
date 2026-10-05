## Context

The background timing pass reads every line's timestamp for the histogram, the time range
and the time delta column, keeping one timestamp per line.

## Goals / Non-Goals

**Goals:** make backward clock jumps visible without changing what filters do.

**Non-Goals:** reordering, forward gaps.

## Decisions

1. **Detected in the timing pass**: compare with the previous timed line; record
   `(line, jump_ms)` when the jump exceeds the tolerance. The list is sparse (usually
   empty), so memory follows the number of jumps, capped at 1,000,000 entries (then
   counted only).
2. **Previous timed line**, not previous line: lines without a timestamp (stack traces)
   are skipped.
3. **Mark, do not move:** filters, search and the histogram keep each line's own time;
   the histogram only marks affected buckets.
4. **Tolerance** default 1 s, setting in `[general]`; 0 disables detection.
5. **Appends** are checked as they are timed; a reload re-runs the pass.

## Risks / Trade-offs

- [Interleaved writers with sub-second skew] → the tolerance absorbs them.
- [Logs written in reverse order] → every line would be marked: above 50 % of timed lines
  the marks are replaced by one notice ("timestamps go backwards").

## Open Questions

- Should the merged timeline view reuse the list to warn per file? Likely, when it lands.
