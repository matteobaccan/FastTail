## Context

The timeline histogram is built in the background from the line timestamps, in buckets
whose width adapts to the span. `structured-fields` reads named fields per line;
`field-statistics` (0.23.0) adds numeric reading and top-N.

## Goals / Non-Goals

**Goals:** see a numeric field move over time beside the line counts, cheaply, for files of
any size.

**Non-Goals:** a metrics product.

## Decisions

1. **Aggregates per bucket**, not per line: count, sum, min, max and a fixed-size
   quantile sketch (e.g. a 64-centroid t-digest or log buckets) for p95. Memory is
   per bucket (a few hundred bytes × at most a few thousand buckets).
2. **Filled by the timing pass** that already reads every line for the histogram, when a
   field is plotted; adding a field re-runs the pass in the background with progress.
3. **Own scale** per field (min to max of the shown buckets), with the values written at
   the strip's edge; lines with no value or a non-numeric value are skipped and counted.
4. **Units:** a number followed by `ns` `µs` `ms` `s` `m` `h` or `B` `KB` `MB` `GB` is
   normalised when all values of the field use the same family.
5. **Terminal:** one field, one row of block characters (`▁▂▃▄▅▆▇█`), braille when the
   terminal supports it.

## Risks / Trade-offs

- [Mixed units] → a field with mixed families is plotted raw with a notice.
- [Depends on field-statistics] → implemented after it.

## Open Questions

- Should the plot follow the filters (only visible lines) or the whole file? Proposed:
  the visible lines, as the histogram.
