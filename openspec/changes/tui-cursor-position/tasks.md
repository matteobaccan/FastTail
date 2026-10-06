## 1. Border

- [ ] 1.1 `draw_stream` / `counts_text`: `Ln N` from `cursor_line()`, `Col N` from `hscroll` when > 0; not in HEX / ASM
- [ ] 1.2 Drop order when the border is too narrow (`Col`, then `Ln`, before the counts)
- [ ] 1.3 Click on `Ln` opens Go to for that stream

## 2. Texts, docs, tests

- [ ] 2.1 `Ln {0}` and `Col {0}` in every language in `src/i18n_tui.rs`
- [ ] 2.2 Tests: `Ln` follows the cursor with a filter on (file line, not view row), `Col` appears after `→` and goes with `0`, a narrow window drops `Col` first, HEX unchanged, a click opens Go to
- [ ] 2.3 `docs/tui.md`, `docs/ui-design.md`, CHANGELOG `[Unreleased]`
- [ ] 2.4 `cargo fmt`, clippy (GUI and `--no-default-features --features tui`), focused tests; PR with Linux and Windows CI green
- [ ] 2.5 Archive the change after the release that ships it
