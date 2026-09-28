## Context

`FilterSpec` (`src/scan_job.rs`) holds up to 8 include and 8 exclude `FilterTerm`s compiled
with the stream's case and regex toggles, the minimum level, and an optional shared global
`FilterSpec`. `excluded(line)` is true when any exclude term (stream or global) matches;
`included(line)` when every include term (stream and global) matches; `matches` and
`visible_in_sequence` (the stack-trace continuation rule) are built on the two. The same
spec is cloned into the `Filter` and `Search` scan jobs above 16 MB and into the Find
results jobs. The `structured-fields` change adds `FieldTerm` operands to `FilterTerm`
when the stream has a field parser.

## Goals / Non-Goals

**Goals:**
- OR, NOT and grouping across conditions, text and field conditions mixed.
- One evaluation path: the expression lives inside `FilterSpec`, so every consumer of the
  filter (synchronous path, jobs, Find results, collapse, context lines) sees it.
- Nothing changes for a user who never turns expression mode on.

**Non-Goals:**
- Expressions outside the include side of filters (see the proposal).

## Decisions

1. **The expression replaces the include side only.** Exclude terms keep their list and
   their role in `visible_in_sequence`: a continuation line follows its parent unless an
   exclude term matches it. *Rejected:* one expression for both sides — the continuation
   rule needs to know which conditions "exclude"; with `NOT` buried in an expression there
   is no sound way to tell, and a stack trace would vanish or leak depending on how the
   user happened to write the negation.
2. **Upper-case keywords, implicit AND.** `AND`, `OR`, `NOT` are operators only in upper
   case, so `not found` and `error or warning` in lower case stay words; two operands side
   by side are ANDed, so `payment timeout` means what the term UI means today. Precedence
   `NOT` > `AND` > `OR`, left-associative. *Rejected:* `&&` / `||` / `!` — they occur in
   log text (shell lines, C++ asserts) and would need quoting far more often; SQL `WHERE`
   syntax — heavier, and `query-language` owns the SQL-or-pipeline decision.
3. **Operands.** A bare word runs until whitespace, a parenthesis or the end; `"…"` is a
   phrase with `\"` and `\\` escapes; `/…/` is a regex with `\/` escape, compiled with the
   stream's case toggle; on a stream with a field parser a bare token matching the
   `structured-fields` field-term grammar is a field term. Bare and quoted operands are text
   or regex according to the `.*` toggle, exactly like a term, so a user switching mode
   sees the same matching.
4. **Parser and evaluation.** Hand-written tokenizer and recursive-descent parser in
   `src/filter_expr.rs` producing a flat `Vec<Node>` (post-order, indices instead of
   boxes); evaluation walks it with short-circuit on a small stack (≤ 32 entries), no
   allocation per line. Leaves reuse `FilterTerm::matches` and `FieldTerm` evaluation; the
   field scan of a line happens at most once per evaluation (shared stack-local
   `FieldSpans`, as in `structured-fields`). Operands are evaluated in written order;
   cheap text operands are not reordered (keeps the behaviour predictable when debugging).
   *Rejected:* a parser crate (`pest`, `nom`) — a new dependency for a grammar of five
   rules.
5. **Limits and errors.** ≤ 1,024 characters, ≤ 32 operands, depth ≤ 16; beyond them a
   parse error. An error (syntax, limit, invalid regex) is shown under the field with a
   caret at the byte position and a short message; the engine keeps the last valid
   compiled expression (`last_valid_expr`), so typing `(payment OR` does not blank the view
   halfway. An expression that has never been valid filters nothing and the field is
   flagged. This differs from an invalid regex term (which matches nothing) on purpose:
   an expression is typed character by character in one field.
6. **Mode switch without loss.** Each stream stores both the include terms and the
   expression. The first switch to expression mode with an empty expression builds it from
   the non-empty include terms (`"a b" AND c`, quoting where needed); switching back shows
   the stored terms. The exclude terms are untouched in both directions.
7. **Global filter.** `GlobalFilter` gains the same mode and expression; the compiled
   global `FilterSpec` carries its own `expr`, so `included` becomes
   `stream_include(line) && global.included(line)` as today. Edits reach the streams after
   the existing 300 ms debounce.
8. **Presets and command line.** Presets gain `mode` and `expression`; applying a preset in
   expression mode switches the stream to it. `--filter-expr` is parsed as a string by
   `cli.rs` and compiled when the stream is created; a syntax error is reported on stderr
   before the window opens and exits with code 2, like an unknown option.

Threads and memory: parsing and compiling run on the UI thread on each (debounced, 150 ms)
edit, microseconds for 32 operands. Evaluation runs where filters run today: UI thread up
to 16 MB, `Filter` job above; the expression is shared through the `Arc` in `FilterSpec`.
Memory: one compiled expression per stream (≤ 32 compiled terms), nothing per line.
Growing files evaluate appended lines with the same spec; truncation and rotation rebuild
with it. Windows file sharing is unchanged (no new file access).

Performance target: an expression of 4 text operands filters at no less than 80 % of the
speed of the same 4 terms in term mode (bench case in `benches/filter_bench.rs`).

## Risks / Trade-offs

- [A user types `ERROR OR WARN` in term mode and expects OR] → term mode keeps literal
  matching (the existing scenario "Operator characters are literal" stays); the `ƒx`
  toggle's tooltip and the README explain the difference; nothing guesses.
- [Keeping the last valid expression hides an error] → the field is flagged in the
  warning colour and the stream bar shows `ƒx ⚠` until the error is fixed.
- [Implicit AND surprises users who expect a phrase] → quoting gives a phrase; the
  tooltip shows `"circuit open"` as the first example.

## Migration Plan

Additive: without `filter_mode=expr` a stream is in term mode as before. Rollback: remove
the mode; stored include terms are still there and older builds read them.

## Open Questions

- Should the per-stream search box accept the same expressions later (search with OR)?
- Should `-word` be accepted as shorthand for `NOT word` (Loki / search-engine style)?
