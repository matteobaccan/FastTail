## Context

`crate::time_histogram` gives the counts per slice, `ValueStats::series` the counts of a value per slice, `PatternStat::buckets` a pattern's volume.

## Goals / Non-Goals

**Goals:** what the proposal lists, over the existing engine and files.

**Non-Goals:** Multi-row charts with axes.

## Decisions

### D1. One sparkline drawer
The `spark` module of the plotted field row draws every series (eight levels `▁`–`█`, `.:-=+*#%@` in ASCII).

## Risks / Trade-offs

- [Small windows] → the volume row is the first to go below 8 rows.

## Open Questions

- Turn the volume row on by default? Decided: off, and it is the window's timeline strip flag (`timeline=` per stream), so both interfaces show or hide it together.
