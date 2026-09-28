## 1. Pattern core

- [ ] 1.1 `src/patterns.rs`: normalisation (collapse Numbers masks plus IPv4 / IPv6), tokenisation (≤ 4,096 bytes, ≤ 128 tokens), Drain tree (depth 4, similarity 0.5, 100 templates per leaf), 2,000-template cap with the "other patterns" bucket
- [ ] 1.2 `PatternSnapshot` (immutable, `classify` without insertion) and per-pattern statistics (count, first / last line and timestamp, 64 sparkline buckets rebinned by pairs)
- [ ] 1.3 Unit tests: templates for known fixtures (ids, durations, IPs masked), merge on generalisation, cap, classify equals the learnt assignment
- [ ] 1.4 Bench: learning throughput on the synthetic 200 MB log

## 2. Engine and jobs

- [ ] 2.1 `JobSpec::Patterns` fed the visible lines under every filter but the pattern filter; progress; snapshots every 500 ms
- [ ] 2.2 Engine pattern state: start on opening the tab, relearn on filter change, incremental feed of appended lines, discard on reload
- [ ] 2.3 `FilterSpec.patterns` (included and excluded ids plus snapshot); evaluation in `included` / `excluded`; dropped when a new snapshot replaces the one it holds
- [ ] 2.4 Tests: filtered count equals the pattern's count (synchronous and job paths, thresholds at 0); continuation lines follow a filtered entry; relearn on another filter change; truncation relearns

## 3. UI

- [ ] 3.1 Patterns dock tab (not saved in the layout): virtualized sortable table, sparkline, text box narrowing templates, progress while learning
- [ ] 3.2 Click, `CTRL + click`, row menu (Hide this pattern, Copy template, Search fixed words); `⧉ N patterns` chip with `✖` and tooltip
- [ ] 3.3 Stream menu entry and `CTRL + SHIFT + G` on the focused stream

## 4. Texts and documentation

- [ ] 4.1 New i18n keys (tab title, columns, chip, menu entries, hints) in every language, added to the exhaustive i18n test
- [ ] 4.2 Help dialog entry; README section with a screenshot; CHANGELOG `[Unreleased]`

## 5. Wrap-up

- [ ] 5.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PR with Linux and Windows CI green
- [ ] 5.2 Local preview exe for the maintainer before the release
- [ ] 5.3 After the release, archive the change so the `pattern-grouping` capability is created
