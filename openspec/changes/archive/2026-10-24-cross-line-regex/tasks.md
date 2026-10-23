## 1. Engine

- [x] 1.1 `SearchMode { Text, Regex, Multiline }` and the compiled pattern per stream; `Aa` for the regex modes; invalid pattern keeps the last valid hits
- [x] 1.2 Single-line regex search on the synchronous path and in `JobSpec::Search`
- [x] 1.3 Chunked multi-line scanner (1 MB chunks, 64-line / 64 KB overlap, deferred hits, dropped long matches counted); `search_ends` (last line of each hit) beside `search_matches`
- [x] 1.4 Append overlap and replacement of hits; truncation and rotation rerun
- [x] 1.5 Tests: hits across chunk boundaries neither lost nor doubled; `^`/`$` per line; a hit across a filter gap; 64-line bound; cap and counting past it; synchronous and job results identical; bench case

## 2. UI and persistence

- [x] 2.1 Mode selector and `Aa` in the search box, error tint and message
- [x] 2.2 Bracket marker and lighter tint on continuation rows (normal and wrap layouts), overview strip marks, `+N lines` in the results pane
- [x] 2.3 (`search_mode` / `search_case`; the history keeps the text, the terminal reads `re:` / `mre:` prefixes) `search_mode` in `StreamEntry`; history entries with `re:` / `mre:`; round-trip and old-file tests

## 3. Texts and documentation

- [x] 3.1 New i18n keys (modes, errors, skipped matches, `+N lines`) in all 16 languages; add them to the exhaustive i18n test
- [x] 3.2 README (search section, comparison table) and CHANGELOG `[Unreleased]`

## 4. Wrap-up

- [x] 4.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PR with Linux and Windows CI green
- [x] 4.2 Local preview exe for the maintainer before the release
- [x] 4.3 After the release, archive the change so `search-and-navigation` gains the new requirements
