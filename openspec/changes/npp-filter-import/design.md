## Context

`config::read_rule_set` reads FastTail rule sets; the BareTail bridge reads BareTail's.

## Goals / Non-Goals

**Goals:** what the proposal lists, over the existing engine and files.

**Non-Goals:** Notepad++ themes.

## Decisions

### D1. Detected by content
The import looks at the file's first bytes and picks the reader.

## Risks / Trade-offs

- None.

## Open Questions

- None.
