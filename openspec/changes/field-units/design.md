## Context

Numbers are read by `field_stats::parse_number` (digits only) in statistics, sparklines and
queries; field terms compare numbers through the same path.

## Goals / Non-Goals

**Goals:** `900ms < 1.2s`, percentiles and plots in one unit, readable results.

**Non-Goals:** custom units, rewriting lines.

## Decisions

### D1. Families and base units
Duration → milliseconds (f64), size → bytes, count rates → per second. Decimal (`KB` =
1000) and binary (`KiB` = 1024) prefixes kept apart; a bare `K`/`M` after a size word is
decimal.

### D2. Detection per field
The first 1,000 values of a field decide its family (the most common suffix family, at
least 80 %); values of another family or none are counted as "not numeric", as non-numbers
are today. A format's `unit.<field>` overrides detection.

### D3. Literals with units
The query language and field terms accept `500ms`, `1.5s`, `10MiB` as literals; comparing a
literal of one family to a field of another is an error message, not a silent false.

## Risks / Trade-offs

- [`5m` as minutes or metres] → only in a duration-family field; elsewhere it is not a
  number.
- [Hot paths] → parsing is allocation-free on the scan workers, like `parse_number`.

## Open Questions

- Show the unit in the column header of the column view? Proposed: yes, `latency (ms)`.
