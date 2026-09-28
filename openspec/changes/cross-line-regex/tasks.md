## 1. Engine

- [ ] 1.1 `SearchMode { Text, Regex, Multiline }` and the compiled pattern per stream; `Aa` for the regex modes; invalid pattern keeps the last valid hits
- [ ] 1.2 Single-line regex search on the synchronous path and in `JobSpec::Search`
- [ ] 1.3 Chunked multi-line scanner (1 MB chunks, 64-line / 64 KB overlap, deferred hits, dropped long matches counted); `search_spans` beside `search_matches`
- [ ] 1.4 Append overlap and replacement of hits; truncation and rotation rerun
- [ ] 1.5 Tests: hits across chunk boundaries neither lost nor doubled; `^`/`$` per line; a hit across a filter gap; 64-line bound; cap and counting past it; synchronous and job results identical; bench case

## 2. UI and persistence

- [ ] 2.1 Mode selector and `Aa` in the search box, error tint and message
- [ ] 2.2 Bracket marker and lighter tint on continuation rows (normal and wrap layouts), overview strip marks, `+N lines` in the results pane
- [ ] 2.3 `search_mode` in `StreamEntry`; history entries with `re:` / `mre:`; round-trip and old-file tests

## 3. Texts and documentation

- [ ] 3.1 New i18n keys (modes, errors, skipped matches, `+N lines`) in all 16 languages; add them to the exhaustive i18n test
- [ ] 3.2 README (search section, comparison table) and CHANGELOG `[Unreleased]`

## 4. Wrap-up

- [ ] 4.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PR with Linux and Windows CI green
- [ ] 4.2 Local preview exe for the maintainer before the release
- [ ] 4.3 After the release, archive the change so `search-and-navigation` gains the new requirements
