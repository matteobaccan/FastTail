## 1. Splitter

- [ ] 1.1 `src/split.rs`: background job by lines, size and time; line-end cuts; names; free-space check; unit tests
- [ ] 1.2 Visible-lines option through the view's filters
- [ ] 1.3 Cancel, progress, no overwrite without confirmation

## 2. Interfaces

- [ ] 2.1 Window: Split file... in the stream menu and palette; dialog with preview
- [ ] 2.2 Terminal interface: the same dialog from the palette

## 3. Texts, docs, wrap-up

- [ ] 3.1 i18n keys in every language; README, `docs/ui-design.md`, `docs/tui.md`, CHANGELOG
- [ ] 3.2 Integration tests (by lines, by size with a long line, by hour, visible only, cancel)
- [ ] 3.3 `cargo fmt`, clippy, focused tests; PR with Linux and Windows CI green
