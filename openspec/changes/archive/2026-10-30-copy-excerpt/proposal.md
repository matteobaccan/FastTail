## Why

gonzo and logana share log excerpts to Slack or Jira; FastTail's bookmark report writes
Markdown, but copying a few lines into a ticket still loses their source and time.

## What Changes

- *Copy as excerpt* (row menu, palette; `Alt+Y` in the terminal): the selected rows as a
  fenced block with a header (`app.log, lines 1204-1210, 2026-10-29 14:02:05 → 14:02:07`),
  in Markdown, Jira (`{code}`) or Slack flavour (Settings: *Excerpt format*). Both interfaces: the window and the terminal (`fasttail-tui`) get it in the same change.

Target release: **0.32.0** (plan of 2026-10-30, after the competitor scan against 0.30.0; see `docs/competitor-analysis.md` section 8). Priority: **Medium**. Effort: **S**.

### Non-goals

- Posting to a service.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `selection-and-export`: the requirement below is added.

## Impact

`src/export.rs` an excerpt builder; both interfaces; a setting; texts; docs.
