## 1. Units

- [ ] 1.1 `src/units.rs`: families, suffixes, Go durations, formatting; allocation-free parsing; unit tests
- [ ] 1.2 Detection per field (D2) and `unit.<field>=` in log formats; round trip of the format file

## 2. Users of numbers

- [ ] 2.1 Field statistics and percentiles in the base unit, results formatted
- [ ] 2.2 Sparklines (window and terminal) with unit labels
- [ ] 2.3 Query language literals and results, field filter terms; error for mixed families
- [ ] 2.4 Tests: `900ms` < `1.2s`, p95 of mixed `ms` / `s`, `latency>=500ms`, sizes KB vs KiB

## 3. Texts and documentation

- [ ] 3.1 i18n keys in every language; README (fields, Query), CHANGELOG, `docs/ui-design.md`

## 4. Wrap-up

- [ ] 4.1 `cargo fmt`, `cargo clippy --all-targets`, focused tests; PR with Linux and Windows CI green
- [ ] 4.2 Local preview exe for the maintainer before the release
- [ ] 4.3 After the release, archive the change
