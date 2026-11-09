## Context

`crate::filter_tab` describes a filter tab (path, filters) and the window opens it as another engine on the same file.

## Goals / Non-Goals

**Goals:** what the proposal lists, over the existing engine and files.

**Non-Goals:** Filter tabs of several files at once.

## Decisions

### D1. Same description
The terminal opens a `FilterTab` as a new engine on the file, so the workspace round-trips between the interfaces.

## Risks / Trade-offs

- [Many tabs of one big file] → each is an engine with its own index: the message says so on files over 1 GB.

## Open Questions

- None.
