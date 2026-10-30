## Context

`crate::system_source` runs `docker logs` / `journalctl` with a configurable command and turns its output into a stream.

## Goals / Non-Goals

**Goals:** what the proposal lists, over the existing engine and files.

**Non-Goals:** Talking to the Kubernetes API without kubectl.

## Decisions

### D1. kubectl, not an API client
No new crate: the user's kubectl, contexts and auth are reused.

## Risks / Trade-offs

- [kubectl missing] → the stream says the command was not found.

## Open Questions

- None.
