## 1. Action table and key map

- [ ] 1.1 `src/actions.rs` (from `command-palette`): extend `ActionId` with stable ids, i18n names, scopes, default bindings equal to 0.12.0 (families expanded)
- [ ] 1.2 `KeyBinding` parser and canonical writer (`Cmd` = `Ctrl`, several bindings with `,`), replacing `external_tools::Shortcut` (tools keep the modifier rule)
- [ ] 1.3 `KeyMap` from defaults + `[shortcuts]` + tools; conflict rules by scope; load-time conflict notice; unknown ids / bad values reported
- [ ] 1.4 Tests: parse / write round-trip; default map equals 0.12.0 bindings; conflicts within and across scopes; tool vs built-in; unbinding a navigation action refused

## 2. Dispatch

- [ ] 2.1 Window scope in `app.rs` (help, always-on-top, lock, find all, global filter, tabs, labels, zoom, tools) through the key map, same consume order
- [ ] 2.2 Stream scope in `dock.rs` (search, bookmarks, follow, wrap, collapse, context, go to, copy, select all, navigation), ignored for plain keys while a text field has focus
- [ ] 2.3 List scope in `find_results.rs` and `hit_list.rs`
- [ ] 2.4 Menu labels, tooltips and the help dialog built from the key map (platform labels)
- [ ] 2.5 Tests: rebinding `bookmark.toggle` to `Ctrl+B` fires it and `Ctrl+F2` no longer does; plain-key action ignored in a text field

## 3. Settings

- [ ] 3.1 "Keyboard shortcuts" page: searchable list grouped by scope and tools, recorder, second binding, remove, reset one, reset all
- [ ] 3.2 Conflict prompt (move / cancel); reserved-key message
- [ ] 3.3 `[shortcuts]` written with non-default entries only

## 4. Texts and documentation

- [ ] 4.1 i18n names for every action and the recorder texts in all 16 languages; add them to the exhaustive i18n test
- [ ] 4.2 README (shortcuts table notes rebinding; configuration section `[shortcuts]`), a test checking the README table against the defaults; CHANGELOG `[Unreleased]` noting that a tool no longer overrides a built-in shortcut

## 5. Wrap-up

- [ ] 5.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PRs with Linux and Windows CI green
- [ ] 5.2 Local preview exe for the maintainer before the release
- [ ] 5.3 After the release, archive the change so `keyboard-shortcuts` is created
