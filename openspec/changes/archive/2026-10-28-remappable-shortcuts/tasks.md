## 1. Action table and key map

- [x] 1.1 `src/actions.rs` (from `command-palette`): extend `ActionId` with stable ids, i18n names, scopes, default bindings equal to 0.12.0 (families expanded) (as built: the registry's actions with their shortcut labels as defaults, plus `window.shortcuts`; the families `Alt + 1..9` / `Ctrl + Shift + 1..9` and the row navigation keys stay fixed)
- [x] 1.2 `KeyBinding` parser and canonical writer (`Cmd` = `Ctrl`, several bindings with `,`), replacing `external_tools::Shortcut` (tools keep the modifier rule) (as built: `keymap::KeyBinding` over `external_tools::KeyName` / `Mods`; tools keep `Shortcut` for their modifier rule)
- [x] 1.3 `KeyMap` from defaults + `[shortcuts]` + tools; conflict rules by scope; load-time conflict notice; unknown ids / bad values reported
- [x] 1.4 Tests: parse / write round-trip; default map equals 0.12.0 bindings; conflicts within and across scopes; tool vs built-in; unbinding a navigation action refused (navigation keys are not in the table)

## 2. Dispatch

- [x] 2.1 Window scope in `app.rs` (help, always-on-top, lock, find all, global filter, tabs, labels, zoom, tools) through the key map, same consume order (as built: unchanged defaults keep their handlers; changed keys are taken before every handler and run through `run_action`, retired defaults are dropped; zoom stays with egui)
- [x] 2.2 Stream scope in `dock.rs` (search, bookmarks, follow, wrap, collapse, context, go to, copy, select all, navigation), ignored for plain keys while a text field has focus (same mechanism; navigation keys fixed)
- [x] 2.3 List scope in `find_results.rs` and `hit_list.rs` (their walking keys stay fixed; no list action has a default key)
- [x] 2.4 Menu labels, tooltips and the help dialog built from the key map (platform labels)
- [x] 2.5 Tests: rebinding `bookmark.toggle` to `Ctrl+B` fires it and `Ctrl+F2` no longer does; plain-key action ignored in a text field

## 3. Settings

- [x] 3.1 "Keyboard shortcuts" page: searchable list grouped by scope and tools, recorder, second binding, remove, reset one, reset all (as built: its own window from Settings and the palette, grouped by the palette's categories; tools are flagged in their own Settings rows)
- [x] 3.2 Conflict prompt (move / cancel); reserved-key message
- [x] 3.3 `[shortcuts]` written with non-default entries only

## 4. Texts and documentation

- [x] 4.1 i18n names for every action and the recorder texts in all 16 languages; add them to the exhaustive i18n test
- [x] 4.2 README (shortcuts table notes rebinding; configuration section `[shortcuts]`), a test checking the README table against the defaults; CHANGELOG `[Unreleased]` noting that a tool no longer overrides a built-in shortcut (the README check is the existing `shortcut_labels_match_the_handlers` test on the registry labels)

## 5. Wrap-up

- [x] 5.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PRs with Linux and Windows CI green
- [x] 5.2 Local preview exe for the maintainer before the release
- [x] 5.3 After the release, archive the change so `keyboard-shortcuts` is created
