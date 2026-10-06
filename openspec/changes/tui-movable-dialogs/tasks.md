## 1. Dialog frame

- [ ] 1.1 `App::dialog()`: optional stored geometry (`settings_rect`, `help_rect` in `App`), clamped into the screen with the minimum sizes, `[x]` in the top border
- [ ] 1.2 `DialogHit` gains the `[x]` cell and the movable flag; `mouse.rs` maps the title row, the edges (via `edges_at`) and `[x]` to new targets

## 2. Mouse

- [ ] 2.1 `DockDrag::DialogMove` / `DialogResize` follow the pointer (`dock::resized`, `dock::clamp_into`); release never closes the dialog
- [ ] 2.2 `[x]` closes as `Esc` (Settings restores the values it opened with); double click on the title restores the centred default

## 3. Content

- [ ] 3.1 Settings: rows and scrolling from the dialog's inner height, focused field kept in view
- [ ] 3.2 Keys: 1 to 3 columns chosen from the dialog's inner width and height; `help_half` and the selection follow

## 4. Tests, docs, wrap-up

- [ ] 4.1 Tests in `src/tui/app.rs`: move, resize (Keys 3 to 2 columns), `[x]` on Settings restores the theme, geometry kept on reopen, clamp after a terminal resize, drag released outside keeps the dialog, double click resets
- [ ] 4.2 Terminal texts: none new expected; if any, a row in every language in `src/i18n_tui.rs`
- [ ] 4.3 `docs/tui.md`, `docs/ui-design.md`, README if it lists TUI mouse actions, CHANGELOG `[Unreleased]`
- [ ] 4.4 `cargo fmt`, clippy (GUI and `--no-default-features --features tui` builds), focused tests; PR with Linux and Windows CI green
- [ ] 4.5 Archive the change after the release that ships it
