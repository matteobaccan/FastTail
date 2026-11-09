## Context

The engine has `set_filter_expr_mode`, rule-set reading and writing
(`config::read_rule_set`), the formats library (`log_format::library`, save / reload) and
the editor presets of `source_links`.

## Goals / Non-Goals

**Goals:** the window's feature in the terminal, over the same engine and files.

**Non-Goals:** A full regex IDE in the formats dialog (the window's preview is richer).

## Decisions

### D1. Reuse the window's files
Rule sets and formats are the same files the window reads and writes.

### D2. One form per format
The formats dialog edits the fields of `FormatSpec` as a form, with the samples matched
shown under it, instead of the window's live table.

## Risks / Trade-offs

- [A format saved in the terminal while the window has it open] → the last writer wins, as for `fasttail.ini`.

## Open Questions

- Ask editor or FastTail on every `Ctrl+O`? Proposed: only when the location is also a file FastTail can open.
