## 1. Units

- [x] 1.1 `src/units.rs`: families, suffixes, Go durations, formatting; allocation-free parsing; unit tests
- [x] 1.2 Detection per field (D2) and `unit.<field>=` in log formats; round trip of the format file (as built: the family most values have, over all of them; see design, implementation notes)

## 2. Users of numbers

- [x] 2.1 Field statistics and percentiles in the base unit, results formatted
- [x] 2.2 Sparklines (window and terminal) with unit labels
- [x] 2.3 Query language literals and results, field filter terms; error for mixed families
- [x] 2.4 Tests: `900ms` < `1.2s`, p95 of mixed `ms` / `s`, `latency>=500ms`, sizes KB vs KiB

## 3. Texts and documentation

- [x] 3.1 i18n keys in every language; README (fields, Query), CHANGELOG, `docs/ui-design.md`

## 4. Wrap-up

- [x] 4.1 `cargo fmt`, `cargo clippy --all-targets`, focused tests; PR with Linux and Windows CI green
- [x] 4.2 Local preview exe for the maintainer before the release (skipped: the maintainer asked for the release directly, 2026-10-29)
- [x] 4.3 After the release, archive the change
