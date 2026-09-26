## 1. Engine and filter evaluation

- [ ] 1.1 Factor the compiled include / exclude terms of `FilterSpec` (`src/scan_job.rs`) into a `TermSet` (terms, per-term regexes, case and regex flags, invalid-term flags) with `included(line)` / `excluded(line)`
- [ ] 1.2 Add `global: Option<Arc<TermSet>>` to `FilterSpec`; `matches` and `visible_in_sequence` combine stream and global terms (either exclude hides, both includes must pass, then the level); `is_active` counts a global set
- [ ] 1.3 `TailEngine::set_global_filter(Option<Arc<TermSet>>)`: store it, rebuild the `FilterSpec`, `refresh_filters` (synchronous up to 16 MB, background job above) and the search refresh, no-op when the same `Arc`
- [ ] 1.4 Unit tests: combination rules, continuation line under a global exclude, background job result equal to the synchronous path on a file above the job threshold (thresholds at 0), Find results job honouring the global set

## 2. Application state, UI and persistence

- [ ] 2.1 `GlobalFilter` state in the app (enabled, bar open, terms, toggles); compile to one `Arc<TermSet>` per edit, debounced 300 ms, and push it to every engine; push the current one to each stream right after it is created (open, workspace / session restore, standard input, zip entry)
- [ ] 2.2 Global filter bar under the menu bar: on / off switch, include and exclude term rows (reuse the Filters window rows, up to 8 each), `Aa`, `.*`, invalid-regex flag per row; `🌐` toolbar toggle and `CTRL + SHIFT + H` consumed before the dock
- [ ] 2.3 `🌐` badge in each stream bar while the global filter is on and has a term, tooltip listing the global terms; the global terms listed above the streams in the Filters window
- [ ] 2.4 `[global_filter]` in `fasttail.ini` (`enabled`, `case_sensitive`, `regex`, `bar_open`, `include.N`, `exclude.N` through `filter_preset::ini_value`), written only when not default; round-trip test including quotes and edge spaces
- [ ] 2.5 Integration tests: three streams hide the same exclude term; global include combined with a stream include; a stream opened later is filtered at once; switched off keeps the terms across a restart; the shortcut does not trigger `CTRL + H` handlers

## 3. Texts and documentation

- [ ] 3.1 New i18n keys (bar title, switch, badge tooltip, toolbar tooltip, help entry) in all 16 languages; add them to the exhaustive i18n test
- [ ] 3.2 Help dialog: `CTRL + SHIFT + H` entry (modifiers in capitals)
- [ ] 3.3 README (filters section and FAQ), CHANGELOG `[Unreleased]`

## 4. Wrap-up

- [ ] 4.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PR with Linux and Windows CI green
- [ ] 4.2 Local preview exe for the maintainer before the 0.12.0 release
- [ ] 4.3 After the release, archive the change so `filters-and-highlighting` gains the Global Filter requirement
