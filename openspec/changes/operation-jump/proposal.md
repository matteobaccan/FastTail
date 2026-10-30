## Why

lnav binds `o` / `O` to the next / previous message of the cursor row's operation id; the
Operations view (FastTail) lists operations but a reader following one request through
a log still searches its id by hand.

## What Changes

- *Next / previous line of this operation* (`Alt+]` / `Alt+[` in the window, `o` / `O` in the
  terminal moved to `Alt+.` / `Alt+,` if taken): the next line whose id field (or the
  operations source) equals the cursor row's.
- Without an operations source, the fields `trace_id`, `request_id`, `req_id`, `opid`,
  `correlation_id` and `thread` are tried in that order. Both interfaces: the window and the terminal (`fasttail-tui`) get it in the same change.

Target release: **0.32.0** (plan of 2026-10-30, after the competitor scan against 0.30.0; see `docs/competitor-analysis.md` section 8). Priority: **Medium**. Effort: **S**.

### Non-goals

- Grouping by several ids at once.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `operation-timeline`: the requirement below is added.

## Impact

`src/tail_engine.rs` a walk like the rule walk; keys in both interfaces; texts; docs.
