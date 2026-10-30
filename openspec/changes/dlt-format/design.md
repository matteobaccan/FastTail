## Context

Every stream reads text lines; binary sources (Event Log `.evtx`) are decoded into text rows by a reader in front of the index.

## Goals / Non-Goals

**Goals:** what the proposal lists, over the existing engine and files.

**Non-Goals:** DLT over TCP / serial (a source plugin can feed it).

## Decisions

### D1. Like `.evtx`
A decoder turns messages into text rows in a spool, the way `.evtx` files are read, so the engine stays text-only.

## Risks / Trade-offs

- [Non-verbose messages without the FIBEX description] → shown as message id and hex payload.

## Open Questions

- Is the automotive niche worth it now? The maintainer may drop this change.
