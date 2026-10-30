## Context

The terminal sees key repeats as presses; a press within 120 ms of the previous one is a repeat (key repeats come every 30 to 50 ms locally, later over SSH).

## Goals / Non-Goals

**Goals:** what the proposal lists, over the existing engine and files.

**Non-Goals:** Acceleration of the arrow keys.

## Decisions

### D1. Time-based
Acceleration follows how long the key has been held, not the press count, so slow and fast terminals feel the same.

## Risks / Trade-offs

- [Terminals without repeat] → nothing changes for them.

## Open Questions

- None.
