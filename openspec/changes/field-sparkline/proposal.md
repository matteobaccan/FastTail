## Why

The timeline histogram shows how many lines arrive per bucket, not what they say. When a
log carries a numeric field (latency, memory, queue length), the question is how that
number moves over time, next to the error spikes. lnav 0.15 draws metrics as sparklines on
its timeline; gonzo draws charts. FastTail parses fields (`structured-fields`) and will
compute their statistics (`field-statistics`, 0.23.0).

## What Changes

- From a numeric column's header menu (or the palette), **Plot over time** draws a line
  of that field over the timeline histogram, on its own scale (min and max shown).
- Per bucket the plotted value is the **average**, or the **max**, **min** or **p95**,
  chosen in the strip's menu.
- Hovering a bucket shows the value next to the line count; a click or a drag selects the
  time range as the histogram does today.
- Up to three fields at once, each in a theme colour; removed with `✖` on its legend.
- The plotted fields are saved per stream (`plot=latency_ms:avg`); older builds ignore it.
- The terminal interface draws one plotted field as a braille or block sparkline row under
  its histogram row.

Target release: **0.25.0** (re-planned by the maintainer on 2026-10-06, from 0.24.0: about one large, two medium and three small changes per release) (candidates of the 2026-10-05 competitor scan, `docs/competitor-analysis.md` section 6, assigned by the maintainer on 2026-10-05), per the release plan in section 8. Depends on `field-statistics` (0.23.0) for numeric field reading. Priority:
**medium**. Effort: **M (1–3 weeks)**.

### Non-goals

- Dashboards, several panels, alerts on thresholds.
- Fields without timestamps on their lines (nothing to plot against).

## Capabilities

### New Capabilities

- `field-sparkline`: a numeric field plotted over the timeline histogram.

### Modified Capabilities

(none; the histogram of `log-intelligence` gains an overlay described here)

## Impact

- `src/time_histogram.rs`: per-bucket aggregates of a field (count, sum, min, max, a small
  quantile sketch for p95), filled by the same background timing pass.
- `src/fields.rs`: numeric reading of a field (units stripped: `12ms`, `1.5s` normalised
  when the unit is consistent).
- `src/ui/timeline_strip.rs`, `src/tui/`: drawing; `src/session.rs`: `plot=`.
- i18n, README, `docs/ui-design.md`, `docs/tui.md`, CHANGELOG, tests.
