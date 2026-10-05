## 1. Detection

- [ ] 1.1 Timing pass records out-of-order lines with their jump (tolerance, cap, reverse-order notice); unit tests
- [ ] 1.2 Appends checked as timed; reload re-runs
- [ ] 1.3 `out_of_order_tolerance_ms` setting; round-trip test

## 2. Interfaces

- [ ] 2.1 Window: gutter `↶` with tooltip, `N out of order` chip with next / previous, histogram bucket marks, time range popup count
- [ ] 2.2 Terminal interface: gutter mark, bar count, walk keys from the palette

## 3. Texts, docs, wrap-up

- [ ] 3.1 i18n keys in every language; README, `docs/ui-design.md`, `docs/tui.md`, CHANGELOG
- [ ] 3.2 Integration tests (a 5-minute jump back, tolerance, stack-trace lines, reverse-order file)
- [ ] 3.3 `cargo fmt`, clippy, focused tests; PR with Linux and Windows CI green
