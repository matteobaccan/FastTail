## 1. Engine

- [ ] 1.1 `src/spike.rs`: template normalisation (collapse Numbers mode plus IP masks, 4,096-byte cut), 64-bit hashing, per-side maps with the 50,000 cap and "other"
- [ ] 1.2 Level counts, field-pair counts with the 1,000-distinct-values cap per key (with `structured-fields`)
- [ ] 1.3 Smoothed log-ratio score, filtering (`p > q`, `a ≥ 3`), top 20 per section, "new" flag
- [ ] 1.4 `JobSpec::Explain` over the timestamp cache (timing first when needed), baseline sampling above 1,000,000 entries, partial results every 250 ms, Cancel
- [ ] 1.5 Tests: a fixture with an injected burst ranks the burst message first; new templates flagged; sampling scales counts; filtered scope; preceding-span baseline; synchronous and job results identical

## 2. UI

- [ ] 2.1 Right-click menu on the histogram strip ("Explain this bar", "Explain the time window"); "Explain" button in the time range popup
- [ ] 2.2 Explain tab: span and baseline header, baseline choice, "Only filtered lines", Volume / Levels / Messages / Fields sections, progress, Recompute, stale marker
- [ ] 2.3 Actions: go to first line, "Search this message", add field term, "Set as time window"

## 3. Texts and documentation

- [ ] 3.1 New i18n keys (menu, tab, sections, actions, notes) in all 16 languages; add them to the exhaustive i18n test
- [ ] 3.2 README (timeline section, comparison table) and CHANGELOG `[Unreleased]`

## 4. Wrap-up

- [ ] 4.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PR with Linux and Windows CI green
- [ ] 4.2 Local preview exe for the maintainer before the release
- [ ] 4.3 After the release, archive the change so the `spike-explanation` capability is created
