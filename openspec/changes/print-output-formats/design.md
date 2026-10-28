## Context

Print mode streams lines through the window's filters without an index; fields are parsed
by `fields` / `log_format` in the window and the scan workers.

## Goals / Non-Goals

**Goals:** machine-readable output of exactly the lines `text` would print.

**Non-Goals:** new formats beyond the five.

## Decisions

### D1. Same lines, other shape
The writer is chosen after the filters: the set of lines never depends on `--output`.

### D2. JSON shape
Stable keys (`file`, `line`, `level`, `time`, `text`, `fields`, `context`), `time` in UTC
ISO 8601 with milliseconds, absent keys left out; one object per line (NDJSON).

### D3. Template
`{name}` is a field, `{line}`, `{file}`, `{time}`, `{level}`, `{text}` built-ins, `{{` a
brace; an unknown name prints empty.

## Risks / Trade-offs

- [CSV columns differ between files] → the header comes from the first file's parser; other
  files map by name.

## Open Questions

- Colour in `template` output on a terminal? Proposed: no colour for non-text formats.
