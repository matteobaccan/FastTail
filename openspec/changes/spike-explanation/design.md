## Context

The histogram (`src/time_histogram.rs`, drawn by `src/ui/timeline_strip.rs`) is built from
the timestamp and level caches and counts every timed line regardless of the filters; a
click or drag sets the time window. `collapse::normalize_line` drops the leading
timestamp and, in `Numbers` mode, masks numbers, hex values and ids into a reused buffer.
`structured-fields` (0.14.0) scans a line's fields as byte spans without allocation.
`pattern-grouping` (0.14.0) will classify lines into Drain patterns from a frozen
snapshot.

## Goals / Non-Goals

**Goals:** a useful "what is different" answer in a few seconds on a multi-GB log; no
change to the histogram's existing gestures; memory bounded by distinct templates, not by
lines.

**Non-Goals:** anomaly detection, cross-stream comparison, significance testing.

## Decisions

### D1. Entry points
Right-click on the strip opens a menu (the strip has none today): "Explain this bar"
(the column under the pointer, its exact bucket span) and "Explain the time window" when a
window is set. The popup's "Explain" button uses the draft window when it is readable.
*Alternative:* Shift+drag — rejected, invisible and easy to confuse with the drag that
sets the window.

### D2. Baseline
"Rest of the log" (every timed line outside the span, default) or "Preceding span" (the
same duration right before; disabled when the span starts at the first timestamp).
Baseline entries above 1,000,000 are sampled every k-th entry and counts scaled by k; the
tab says so.

### D3. Templates
An entry's first line (continuation lines follow their entry and are not counted apart)
normalised with `normalize_line(…, Numbers, hint)` plus IPv4 / IPv6 masking, cut at
4,096 bytes, hashed to 64 bits into a `HashMap<u64, TemplateStat>` keeping the first
normalised text and the first line in the span. Cap: 50,000 templates per side, the rest
counted as "other". With `pattern-grouping` shipped, the pattern id replaces the hash.

### D4. Score
For an item with counts `a` in the span (of `A` entries) and `b` in the baseline (of `B`):
`p = (a + 0.5) / (A + 0.5·V)`, `q = (b + 0.5) / (B + 0.5·V)` (`V` distinct items), score
`a · ln(p / q)`; only items with `p > q` and `a ≥ 3` are listed, ordered by score. The
ratio shown is `p / q`; `b = 0` shows **new**. This favours items both frequent and
over-represented, which is what a person scanning a spike needs.
*Alternative:* raw count difference — rejected, it lists INFO heartbeats first in any long
span.

### D5. Fields
With a parser, each entry contributes its `key=value` pairs (values cut at 256 bytes).
Per key, distinct values are counted up to 1,000 in the span; a key over the cap is
dropped (ids, timestamps) and named in a footnote. Pairs are scored as templates.

### D6. Job
`JobSpec::Explain` walks the line range once, using the timestamp cache to assign each
entry to span, baseline or neither (timing it first when needed, like the histogram);
partial results merge every 250 ms, so the tab fills in. Memory: at most 2 × 50,000
templates × ~300 bytes and the field maps, well under 64 MB. The tab is a snapshot: new
lines appended after the job do not change it; a "Recompute" button reruns it. Truncation
or rotation marks the tab stale.

## Risks / Trade-offs

- [Masking too little, so one message kind becomes many templates] → same masks as
  collapse's Numbers mode plus IPs; with `pattern-grouping` Drain handles it better.
- [A spike of one new message drowns the others] → each section is ranked separately and
  the Levels and Fields sections still show their own top items.
- [Slow on a 10 GB log] → one pass, sampling of the baseline, progress and Cancel.

## Migration Plan

Additive; nothing is persisted.

## Open Questions

- Should "Explain" also be offered from a selection of rows (span = the selection's
  timestamps)? Proposed: yes, in the row menu, if cheap.
- Should the baseline option be remembered across runs? Proposed: per session only.
