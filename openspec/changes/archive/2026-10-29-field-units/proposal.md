## Why

Fields such as `latency=12ms`, `took 1.2s`, `size=3.4MB` compare and plot wrongly when
read as bare numbers: today `parse_number` keeps the digits, so `1.2s` counts as 1.2 and
`900ms` as 900 in statistics, sparklines, percentiles and query comparisons. lnav 0.15.0
added units to format fields (suffix and divisor, `measure_value()`), so durations and sizes
compare and show as people read them. Gap 24 (post-0.27.0 scan), value medium.

## What Changes

- Numeric field values with a **unit suffix** are read in a base unit: durations (`ns`,
  `us`/`µs`, `ms`, `s`, `m`, `h`, Go-style `1m30s`) as milliseconds, sizes (`B`, `KB`/`KiB`,
  `MB`/`MiB`, `GB`/`GiB`, `TB`) as bytes, rates (`/s`).
- **Field statistics**, **sparklines**, the **Query tab** (`where latency > 1s`,
  `stats p95(latency) by route`) and **field filter terms** (`latency>=500ms`) compare in the
  base unit and show results with the best unit (`1.2 s`, `3.4 MiB`).
- A field's unit is detected from its values (the most common suffix family) or set in a
  log format (`unit.latency = ms`, for bare numbers that are milliseconds).

Target release: **0.28.0** (proposed after the post-0.27.0 scan of 2026-10-28; planned when the maintainer agrees), per the release plan in `docs/competitor-analysis.md` section 8. Priority: **Medium**. Effort: **M**.

### Non-goals

- Currency or arbitrary custom units.
- Converting the text of the line itself.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `structured-fields`: the requirements below are added.

## Impact

- New `src/units.rs`: suffix parsing, families, formatting; unit tests.
- `src/field_stats.rs` `parse_number` → unit-aware value; `src/plot.rs` series; `src/query.rs`
  literals with units and result formatting; `src/fields.rs` field terms.
- `src/log_format.rs` `unit.<field>=`; Log formats dialog field.
- README, CHANGELOG, `docs/ui-design.md` (Statistics, Query, sparkline labels), i18n.
