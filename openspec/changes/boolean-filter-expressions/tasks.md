## 1. Expression core

- [ ] 1.1 `src/filter_expr.rs`: tokenizer (bare words, `"…"` phrases, `/…/` regexes, parentheses, upper-case `AND` / `OR` / `NOT`), recursive-descent parser to a flat post-order `Vec<Node>`, implicit AND, precedence `NOT` > `AND` > `OR`
- [ ] 1.2 Limits (1,024 characters, 32 operands, depth 16) and errors with byte position and message key
- [ ] 1.3 Operand compilation: text / regex per the stream toggles, `/…/` always regex, field terms through the `structured-fields` grammar when the stream has a parser
- [ ] 1.4 Evaluation without allocation per line, short-circuit, one field scan per evaluation
- [ ] 1.5 Unit tests: precedence, implicit AND, lower-case keywords as words, quoting and escapes, limits, error positions, field and text operands mixed

## 2. Engine and filter pipeline

- [ ] 2.1 `FilterSpec.expr: Option<Arc<FilterExpr>>`; `included` evaluates it in place of the include terms; `is_active` counts it
- [ ] 2.2 Engine: filter mode, stored terms and expression, `last_valid_expr`, 150 ms debounce, `refresh_filters` on change
- [ ] 2.3 Global filter: mode and expression in `GlobalFilter`, compiled into the shared global `FilterSpec`
- [ ] 2.4 Tests: background filter job equal to the synchronous path (thresholds at 0); continuation line under an exclude term in expression mode; Find results honour a stream expression and a global expression; collapse and context lines over an expression-filtered view
- [ ] 2.5 Bench: 4 text operands against 4 terms in `benches/filter_bench.rs`

## 3. UI, presets, command line and persistence

- [ ] 3.1 `ƒx` toggle beside the Include field; expression field with error caret and `ƒx ⚠` badge; syntax tooltip; Filters window shows the expression in place of the include rows
- [ ] 3.2 Mode switch filling the expression from the terms the first time, restoring the terms on the way back
- [ ] 3.3 Global filter bar: `ƒx` toggle and expression field
- [ ] 3.4 Presets: `mode` and `expression` keys; applying a preset switches the mode
- [ ] 3.5 `--filter-expr` in `cli.rs` (usage error with `--filter`, syntax error exits 2 before the window opens); CLI parse tests
- [ ] 3.6 Persistence: `filter_mode` / `filter_expr` per stream (workspace, sessions), `[global_filter] mode` / `expression`; round-trip and old-file tests

## 4. Texts and documentation

- [ ] 4.1 New i18n keys (toggle, tooltip with syntax, error messages, badge) in every language, added to the exhaustive i18n test
- [ ] 4.2 Help dialog entry; README filters section and FAQ; `--filter-expr` in the command-line section; CHANGELOG `[Unreleased]`

## 5. Wrap-up

- [ ] 5.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PR with Linux and Windows CI green
- [ ] 5.2 Local preview exe for the maintainer before the release
- [ ] 5.3 After the release, archive the change so `filters-and-highlighting` and `command-line` gain the new requirements
