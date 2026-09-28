## 1. Engine

- [ ] 1.1 `SearchScope` per stream; synchronous search bounded by the line range; time scope over the timestamp cache; held while timing; reset on truncation, rotation and reload
- [ ] 1.2 `JobSpec::Search` line and time bounds; 16 MB threshold on the scope's bytes
- [ ] 1.3 `find_all` time bounds per stream; streams without timestamps reported as skipped
- [ ] 1.4 Tests: hits and counter within a line scope; wrap within the scope; open-ended scope grows with appends, closed one does not; time scope with a continuation line; held time scope; Find results time scope; synchronous and job results identical

## 2. UI

- [ ] 2.1 Scope chip and editor (Selection, Lines, Time) in the search box, `✖`, accent tint, "in range" counter
- [ ] 2.2 Row menu: "Search in selection", "Search from here", "Search up to here"
- [ ] 2.3 Overview strip shading of the scope; histogram search lane limited to the scope; results pane lists only scoped hits
- [ ] 2.4 Find results tab: from / to fields with the invalid-time hint

## 3. Texts and documentation

- [ ] 3.1 New i18n keys (scopes, chip, menu entries, notes) in all 16 languages; add them to the exhaustive i18n test
- [ ] 3.2 README (search section) and CHANGELOG `[Unreleased]`

## 4. Wrap-up

- [ ] 4.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PR with Linux and Windows CI green
- [ ] 4.2 Local preview exe for the maintainer before the 0.13.0 release
- [ ] 4.3 After the release, archive the change so `search-and-navigation` gains the new requirements
