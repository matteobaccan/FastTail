## 1. Engine

- [ ] 1.1 `src/operations.rs`: `OpSource`, name source, `OpStat`, map with the 100,000 cap and the untracked counter, merge of partial maps
- [ ] 1.2 `JobSpec::Operations` over the visible lines (timing first when needed), partial results every 250 ms, Cancel
- [ ] 1.3 Engine state per stream: incremental update on appended visible lines, rebuild on filter / source change, truncation and rotation
- [ ] 1.4 Tests: interleaved requests fixture (start, end, duration, worst level), continuation lines follow their entry, out-of-order timestamps, cap, append while following, synchronous and job results identical

## 2. UI

- [ ] 2.1 Operations tab: source and name pickers, virtualised table, sort, id / name box, minimum duration
- [ ] 2.2 Gantt column: shared axis, zoom and pan, level colours, ticks, time window shading, hover with the first line
- [ ] 2.3 Actions: go to first line, "Filter to this operation", "Set as time window", "Copy as CSV"; menu entries in the stream menu and the column header menu

## 3. Texts and documentation

- [ ] 3.1 New i18n keys (tab, pickers, columns, actions, notes) in all 16 languages; add them to the exhaustive i18n test
- [ ] 3.2 README (operations section, comparison table) and CHANGELOG `[Unreleased]`

## 4. Wrap-up

- [ ] 4.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PR with Linux and Windows CI green
- [ ] 4.2 Local preview exe for the maintainer before the release
- [ ] 4.3 After the release, archive the change so the `operation-timeline` capability is created
