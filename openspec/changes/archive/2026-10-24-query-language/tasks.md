## 1. Language

- [x] 1.1 `src/query/`: lexer and parser for `where`, `parse`, `fields`, `stats … by …`, `timeslice`, `sort`, `head`; `where` delegates to `filter_expr`; limits (64 KB, 16 stages); errors with stage and position
- [x] 1.2 Executors: streaming stages over a reused row buffer, blocking `stats` and `sort`, early stop for `head`
- [x] 1.3 Aggregators: `count`, `sum`, `avg`, `min`, `max`, `dc` (100,000 cap), `p50`…`p99` (reuse `field_stats` histogram), `first`, `last`; 100,000-group error
- [x] 1.4 Unit tests per stage and aggregator; example queries on fixtures with expected tables

## 2. Engine and UI

- [x] 2.1 `JobSpec::Query` over the visible lines, progress, cancel, snapshot semantics
- [x] 2.2 Query dock tab: editor with the error's stage and character (no caret drawn in the editor), Run / Stop, history (20, `[query] history.N`), virtualized sortable table, Copy as CSV, double-click to jump for row results
- [x] 2.3 Tests: job equal to a synchronous run (thresholds at 0); truncation cancels; history round-trip

## 3. Texts and documentation

- [x] 3.1 New i18n keys in every language, added to the exhaustive i18n test
- [x] 3.2 README section with a syntax reference and examples; CHANGELOG `[Unreleased]`

## 4. Wrap-up

- [x] 4.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PR with Linux and Windows CI green
- [x] 4.2 Local preview exe for the maintainer before the release
- [x] 4.3 After the release, archive the change so the `query-language` capability is created
