## 1. Engine and filter evaluation

- [x] 1.1 Reuse `FilterSpec` (`src/scan_job.rs`) as the compiled global set (its terms, per-term regexes, case and regex flags, invalid-term flags); no separate type needed
- [x] 1.2 Add `global: Option<Arc<FilterSpec>>` to `FilterSpec` (`with_global`, `has_terms`); `excluded` / `included` combine stream and global terms, so `matches` and `visible_in_sequence` follow; `is_active` counts a global set
- [x] 1.3 `TailEngine::set_global_filter(Option<Arc<FilterSpec>>)`: store it, rebuild the `FilterSpec`, `refresh_filters` (synchronous up to 16 MB, background job above) and the search refresh, no-op when the same `Arc`
- [x] 1.4 Unit tests: combination rules, continuation line under a global exclude, background job result equal to the synchronous path on a file above the job threshold (thresholds at 0), Find results job honouring the global set

## 2. Application state, UI and persistence

- [x] 2.1 `GlobalFilter` state in the app (enabled, bar open, terms, toggles); compile to one `Arc<FilterSpec>` per edit, debounced 300 ms, and push it to every engine; every stream, new ones included, gets the current set each frame before it is drawn (a no-op when already the same `Arc`)
- [x] 2.2 Global filter bar under the menu bar: on / off switch, include and exclude term rows (reuse the Filters window rows, up to 8 each), `Aa`, `.*`, invalid-regex flag per row; `🌐` toolbar toggle and `CTRL + SHIFT + H` consumed before the dock
- [x] 2.3 `🌐` badge in each stream bar while the global filter is on and has a term, tooltip listing the global terms; the global terms listed above the streams in the Filters window
- [x] 2.4 `[global_filter]` in `fasttail.ini` (`enabled`, `case_sensitive`, `regex`, `bar_open`, `include.N`, `exclude.N` through `filter_preset::ini_value`), written only when not default; round-trip test including quotes and edge spaces
- [x] 2.5 Integration tests: three streams hide the same exclude term; global include combined with a stream include; a stream opened later is filtered at once; switched off keeps the terms across a restart; the shortcut does not trigger `CTRL + H` handlers

## 3. Texts and documentation

- [x] 3.1 New i18n keys (bar title, switch, badge tooltip, toolbar tooltip, help entry) in all 16 languages; add them to the exhaustive i18n test
- [x] 3.2 Help dialog: `CTRL + SHIFT + H` entry (modifiers in capitals)
- [x] 3.3 README (filters section and FAQ), CHANGELOG `[Unreleased]`

## 4. Wrap-up

- [x] 4.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PR with Linux and Windows CI green
- [x] 4.2 Local preview exe for the maintainer before the 0.12.0 release
- [x] 4.3 After the release, archive the change so `filters-and-highlighting` gains the Global Filter requirement
