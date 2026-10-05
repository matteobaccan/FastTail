## 1. Derived stream engine

- [x] 1.1 `src/filter_tab.rs`: `DerivedSpec`, initial fill through a filter scan job, follow with rotation / truncation handling, spool writing and the source line-number map
- [x] 1.2 `TailEngine` source line-number map: gutter, go-to line (bookmarks stay by derived line: the tab is not saved)
- [x] 1.3 Spool bound `stdin_spool_max_mb`: the tab stops following, with a notice (no restart, no free-space margin, no large-copy warning yet)
- [x] 1.4 Tests: fill equals the source's filtered lines; appended matching and non-matching lines; truncation and rotation rebuild; source filter changed afterwards leaves the derived tab unchanged

## 2. UI

- [x] 2.1 "Open filter as new tab" in the stream menu and row menu, enabled only with an active filter
- [x] 2.2 Tab title, tooltip with the frozen filter, icon; "source closed" state in the stream bar
- [x] 2.3 Show in context and `CTRL + K` redirected to the source stream (reopened if closed)

## 3. Persistence

- [x] 3.1 `filter:` identity; workspace and session entries (source path, frozen filter, own filters and options); excluded from recent files
- [x] 3.2 Rebuild on restore; tests for the round trip

## 4. Texts and documentation

- [x] 4.1 New i18n keys (menu item, tooltip, states, warning) in all 16 languages; add them to the exhaustive i18n test
- [x] 4.2 README (filters section, comparison table) and CHANGELOG `[Unreleased]`

## 5. Wrap-up

- [x] 5.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PR with Linux and Windows CI green
- [x] 5.2 Local preview exe for the maintainer before the 0.13.0 release
- [x] 5.3 After the release, archive the change so `filters-and-highlighting` gains the new requirements
