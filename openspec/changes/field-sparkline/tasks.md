## 1. Aggregates

- [ ] 1.1 Per-bucket count, sum, min, max, quantile sketch; unit tests on known series
- [ ] 1.2 Numeric reading with units (time, size), mixed families detected
- [ ] 1.3 Filled by the background timing pass for the plotted fields, following the filters

## 2. Drawing

- [ ] 2.1 Window: Plot over time from the column header and the palette; overlay line, scale, legend with `✖`, hover values; avg / max / min / p95
- [ ] 2.2 Terminal interface: one field as a sparkline row under the histogram
- [ ] 2.3 `plot=` per stream; round-trip test

## 3. Texts, docs, wrap-up

- [ ] 3.1 i18n keys in every language; README, `docs/ui-design.md`, `docs/tui.md`, CHANGELOG
- [ ] 3.2 Integration tests (latency series, units, growing file, filters)
- [ ] 3.3 `cargo fmt`, clippy, focused tests; PR with Linux and Windows CI green
