## 1. Action registry

- [x] 1.1 `src/actions.rs`: `ActionId`, `ActionMeta` (i18n key, category, scope, shortcut label), `enabled`, deferred `run` queue drained in `FastTailApp::update`
- [x] 1.2 Register window, stream, search, bookmark, session and view actions; expose the `dock.rs` stream actions as functions
- [x] 1.3 Generated setting commands: booleans as toggles, enums with a value step
- [x] 1.4 Tests: every menu i18n key has a registry entry; shortcut labels match the consumed keys; enabled conditions (no stream, no search, HEX view)

## 2. Palette UI

- [x] 2.1 `src/ui/palette.rs`: popup, text box, rows with name, category, shortcut and disabled reason
- [x] 2.2 Fuzzy scorer on localized and English names with case and accent folding; ranking tests
- [x] 2.3 Keys (arrows, pages, Enter, Esc, CTRL + SHIFT + P toggle), value step, no opening while locked
- [x] 2.4 Recent commands and `palette_recent` in `[general]`; title-bar button ⌨ (tooltip "Command palette…")

## 3. Texts and documentation

- [x] 3.1 New i18n keys (action names without a menu label, categories, palette texts) in all 16 languages; add them to the exhaustive i18n test
- [x] 3.2 Help dialog entry, README (shortcuts table, feature list) and CHANGELOG `[Unreleased]`

## 4. Wrap-up

- [x] 4.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PR with Linux and Windows CI green
- [x] 4.2 Local preview exe for the maintainer before the 0.13.0 release
- [x] 4.3 After the release, archive the change so the `command-palette` capability is created
