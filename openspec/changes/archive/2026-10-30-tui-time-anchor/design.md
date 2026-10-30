## Context

`TailEngine::toggle_time_anchor`, `time_anchor`, `row_time_delta` and the selection's elapsed time are engine features.

## Goals / Non-Goals

**Goals:** what the proposal lists, over the existing engine and files.

**Non-Goals:** Several anchors.

## Decisions

### D1. The engine's anchor
The key toggles the engine's anchor; the Δt cell already draws `TimeDelta::Anchor`.

## Risks / Trade-offs

- [`@` taken by a remapped key] → the action is remappable (`time_anchor`).

## Open Questions

- None.
