## 1. Aggregates

- [x] 1.1 Per-bucket count, sum, min, max, quantile sketch; unit tests on known series
- [x] 1.2 Numeric reading with units (time, size), mixed families detected
- [x] 1.3 Filled by the background timing pass for the plotted fields, following the filters

## 2. Drawing

- [x] 2.1 (no palette entry: a field has to be picked, the column header does it) Window: Plot over time from the column header and the palette; overlay line, scale, legend with `✖`, hover values; avg / max / min / p95
- [x] 2.2 Terminal interface: one field as a sparkline row (done on 2026-10-24 at the maintainer's request: the terminal has no histogram, so the row is the last of the stream window, `src/tui/spark.rs`; block characters, no braille; `p` / `a` in the `[Fields...]` list)
- [x] 2.3 `plot=` per stream; round-trip test

## 3. Texts, docs, wrap-up

- [x] 3.1 i18n keys in every language; README, `docs/ui-design.md`, CHANGELOG (`docs/tui.md` with 2.2)
- [x] 3.2 Integration tests (latency series, units, growing file, filters)
- [x] 3.3 `cargo fmt`, clippy, focused tests; PR with Linux and Windows CI green
