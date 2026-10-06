## 1. Window

- [ ] 1.1 `src/ui/dock.rs`: Follow and Auto-update buttons with `☑` / `☐` and translated labels, tinted fill kept; glyphs checked in the bundled fonts
- [ ] 1.2 Tooltip, status texts and palette entry renamed (keys unchanged, "monitor" kept as a palette alias)

## 2. Terminal

- [ ] 2.1 `src/tui/bars.rs`: `[x]` / `[ ]` Follow and Auto-update chips (also in ASCII mode)
- [ ] 2.2 `src/tui/app.rs`: "Auto-update on" / "Auto-update off" messages; update tests that match `[▶ Monitor]`

## 3. Texts, docs, tests

- [ ] 3.1 New and changed texts in every language (`src/i18n.rs`, `src/i18n_tui.rs`)
- [ ] 3.2 Tests: chip text and state for both toggles, click toggles only its own feature, 80-column capture in German and Japanese
- [ ] 3.3 README, `docs/ui-design.md` (diagram, toggle list, icon table), `docs/tui.md`, CHANGELOG `[Unreleased]`
- [ ] 3.4 `cargo fmt`, clippy (GUI and `--no-default-features --features tui`), focused tests; PR with Linux and Windows CI green
- [ ] 3.5 Archive the change after the release that ships it
