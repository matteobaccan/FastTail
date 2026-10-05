## 1. Window kind

- [ ] 1.1 `src/tui/dock.rs`: a Find results window kind, not saved in the layout
- [ ] 1.2 Keys (`Ctrl+Shift+F`, the alternative key, the palette entry) and the query box

## 2. Results

- [ ] 2.1 Jobs through `find_all` (four at a time), progress, stale marks, caps
- [ ] 2.2 Grouped, collapsible, virtualized rows; arrows, pages, Home / End
- [ ] 2.3 `Enter` focuses the stream on the line (follow paused, its search unchanged); `r` refresh; `Esc` closes and cancels

## 3. Texts, docs, wrap-up

- [ ] 3.1 Terminal texts in every language; `docs/tui.md`, the parity checklist, CHANGELOG
- [ ] 3.2 Capture tests at 80 and 200 columns; a test that walks results across two streams
- [ ] 3.3 `cargo fmt`, clippy (GUI and TUI builds), focused tests; PR with Linux and Windows CI green
