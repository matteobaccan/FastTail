## Context

`crate::compare` aligns lines and regions; the window keeps the scratchpad text in a file
beside the session (`*.fasttail-session.scratch.txt`).

## Goals / Non-Goals

**Goals:** the window's feature in the terminal, over the same engine and files.

**Non-Goals:** Three-way compare.

## Decisions

### D1. Same file
The terminal edits the same scratchpad file, so the window and the terminal share it.

## Risks / Trade-offs

- [Editing a multi-line pad in a terminal] → a simple editor (insert, delete, lines), no undo history beyond the last change.

## Open Questions

- Undo in the scratchpad editor? Proposed: one level.
