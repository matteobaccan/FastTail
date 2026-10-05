## 1. Engine

- [ ] 1.1 Hidden ranges (merge, split on Show, cap 1,000) applied in the filter pass and the background scan; unit tests
- [ ] 1.2 Search, counters, overview strip, export and histogram follow the hidden lines; Show in context lists them
- [ ] 1.3 Cleared on truncation, rotation and rewrite with a notice; kept on append

## 2. Interfaces

- [ ] 2.1 Window: context menu, `Ctrl+H`, palette action, `N hidden` chip with the list (Show, Show all)
- [ ] 2.2 Terminal interface: `H`, the bar chip, the list dialog
- [ ] 2.3 `hidden=` per stream in `fasttail.ini` and sessions; round-trip test

## 3. Texts, docs, wrap-up

- [ ] 3.1 i18n keys in every language; README, `docs/ui-design.md`, `docs/tui.md`, CHANGELOG
- [ ] 3.2 Integration tests (hide, show, persistence, growing file, filters combined)
- [ ] 3.3 `cargo fmt`, clippy, focused tests; PR with Linux and Windows CI green
