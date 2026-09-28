## 1. Derived stream engine

- [ ] 1.1 `src/filter_tab.rs`: `DerivedSpec`, initial fill through a filter scan job, follow with rotation / truncation handling, spool writing and the source line-number map
- [ ] 1.2 `TailEngine` source line-number map: gutter, go-to line, bookmarks by source number
- [ ] 1.3 Spool bounds (`stdin_spool_max_mb`, free-space margin) and restart notice; large-copy warning
- [ ] 1.4 Tests: fill equals the source's filtered lines; appended matching and non-matching lines; truncation and rotation rebuild; source filter changed afterwards leaves the derived tab unchanged

## 2. UI

- [ ] 2.1 "Open filter as new tab" in the stream menu and row menu, enabled only with an active filter
- [ ] 2.2 Tab title, tooltip with the frozen filter, icon; "source closed" state in the stream bar
- [ ] 2.3 Show in context and `CTRL + K` redirected to the source stream (reopened if closed)

## 3. Persistence

- [ ] 3.1 `filter:` identity; workspace and session entries (source path, frozen filter, own filters and options); excluded from recent files
- [ ] 3.2 Rebuild on restore; tests for the round trip

## 4. Texts and documentation

- [ ] 4.1 New i18n keys (menu item, tooltip, states, warning) in all 16 languages; add them to the exhaustive i18n test
- [ ] 4.2 README (filters section, comparison table) and CHANGELOG `[Unreleased]`

## 5. Wrap-up

- [ ] 5.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PR with Linux and Windows CI green
- [ ] 5.2 Local preview exe for the maintainer before the 0.13.0 release
- [ ] 5.3 After the release, archive the change so `filters-and-highlighting` gains the new requirements
