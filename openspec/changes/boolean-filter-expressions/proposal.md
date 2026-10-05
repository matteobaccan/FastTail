## Why

A stream's include terms are ANDed and its exclude terms ORed. That covers "payment and
timeout, without health checks", but not "(payment or refund) and timeout", "error, unless
it comes from the retry worker and is a timeout", or "status 500 or above, or any line
mentioning `circuit open`". Today the only OR is a regex alternation inside one term
(`timeout|refused`), which cannot mix with other conditions, and the Combined Filter Terms
requirement says outright that there is no operator syntax. klogg has AND / OR / NOT
between patterns, hl has boolean `-q` queries, Seq and Loki have filter expressions, and
OR filters are among Tailviewer's most-voted requests (#368, #139). The post-0.12.0
competitor scan ranks field-scoped and boolean filters as gap 4 (value high); the
`structured-fields` change covers the field-scoped half and explicitly leaves the boolean
half to its own change (its design, decision 6).

## What Changes

- An **expression mode** for the include side of a stream's filter, switched per stream
  with an `ƒx` toggle beside the Include field. In expression mode the include terms are
  replaced by **one expression**; the exclude terms stay as they are and keep hiding
  noise, stack-trace continuation lines included, exactly as today.
- **Syntax**: operands joined by `AND`, `OR`, `NOT` (upper case only) and parentheses,
  precedence `NOT` > `AND` > `OR`; two operands side by side mean `AND`. An operand is a
  bare word, a double-quoted phrase (`"circuit open"`), a `/regex/`, or, on a stream with
  an active field parser (`structured-fields`), a field term (`status>=500`,
  `level=error|fatal`). Bare and quoted operands follow the stream's `Aa` and `.*`
  toggles like a term; `/…/` is always a regex.
- **Limits**: 1,024 characters, 32 operands, nesting depth 16. A syntax error or an
  invalid regex is flagged under the field with its position, and the view keeps showing
  the result of the **last valid expression** until the text is fixed.
- The **term UI stays the default**. Turning expression mode on for the first time fills
  the expression from the current include terms (`payment AND timeout`); turning it off
  brings the terms back as they were; neither direction loses the other side's text.
- The **global filter** gets the same `ƒx` toggle and expression for its include side,
  combined with each stream's filter as today (stream AND global).
- **Filter presets** save the mode and the expression. The Find results tab, the line
  counters, the overview strip, collapse and context lines see the lines as filtered, with
  no change of their own, because the expression is evaluated inside `FilterSpec`.
- **Command line**: `--filter-expr <expr>` sets expression mode with that expression on
  the streams opened from the command line (the standard-input stream included); it
  cannot be combined with `--filter` (usage error, exit code 2).
- Persistence, per stream in the workspace and session files: `filter_mode=expr` (written
  only in expression mode) and `filter_expr=` (through `filter_preset::ini_value`); in
  `fasttail.ini`: `[global_filter] mode=expr` and `expression=`, and the same two keys in
  each `[filter_preset_N]` section. Older builds ignore the new keys and fall back to the
  saved include terms.

Target release: **0.23.0** (re-planned by the maintainer on 2026-10-05, from 0.21.0) (structured logs and analysis, continued; planned for 0.14.0, moved after the terminal interface of 0.20.0), per the release plan in
`docs/competitor-analysis.md` section 8. Priority: **high**. Effort: **M (1–3 weeks)**.

### Non-goals

- Replacing the exclude side with the expression: exclude terms keep their own list so
  the continuation-line rule stays unchanged (see the design).
- Expressions in highlight rules, quick labels or the per-stream search box.
- Pipes, aggregation or sorting: that is the `query-language` change.
- A visual query builder (tree of AND / OR groups).
- Lower-case operators, or the symbols `&&`, `||`, `!`.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `filters-and-highlighting`: adds the Filter Expression Mode requirement (syntax,
  limits, errors, switching, global filter, presets, persistence) and the expression
  counterpart of the global filter.
- `command-line`: adds the `--filter-expr` option.

## Impact

- `src/filter_expr.rs` (new): tokenizer, recursive-descent parser to a flat AST
  (`Vec<Node>` with operand leaves holding a `FilterTerm` or a `FieldTerm`), evaluation
  with short-circuit, error positions.
- `src/scan_job.rs`: `FilterSpec` gains `expr: Option<Arc<FilterExpr>>`; `included`
  evaluates it instead of the include terms when set, so `matches`,
  `visible_in_sequence`, the background filter and search jobs and the Find results jobs
  follow with no other change.
- `src/tail_engine.rs`: filter mode and expression per engine, last valid expression kept,
  `is_filter_active` accounts for it.
- `src/global_filter.rs`, `src/ui/global_filter_bar.rs`: expression mode for the global set.
- `src/filter_preset.rs`, `src/session.rs`, `src/config.rs`: the new keys.
- `src/ui/dock.rs`: `ƒx` toggle, expression field with error caret, tooltip with the
  syntax; the Filters window shows the expression in place of the include rows.
- `src/cli.rs`: `--filter-expr`.
- `src/i18n.rs` (every language), help dialog, README, CHANGELOG, tests, a bench case.
