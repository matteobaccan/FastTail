## Context

The engine knows its line offsets, cache size and the time its index took; it does not keep the latter yet.

## Goals / Non-Goals

**Goals:** what the proposal lists, over the existing engine and files.

**Non-Goals:** A memory profiler.

## Decisions

### D1. Counted, not measured
Memory is computed from the index and cache sizes, not read from the allocator.

## Risks / Trade-offs

- None.

## Open Questions

- None.
