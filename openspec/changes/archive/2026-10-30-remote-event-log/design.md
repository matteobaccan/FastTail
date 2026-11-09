## Context

`crate::eventlog` uses `EvtSubscribe` on a local session.

## Goals / Non-Goals

**Goals:** what the proposal lists, over the existing engine and files.

**Non-Goals:** Event forwarding subscriptions (WEF).

## Decisions

### D1. EvtOpenSession
A remote session handle is opened and passed to the same subscription code.

## Risks / Trade-offs

- [Firewalls] → the error text names the RPC port to open.

## Open Questions

- None.
