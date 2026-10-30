## Why

lnav 0.15 shows per-file indexing time and memory; users comparing FastTail with other
viewers ask how big the index is and why a file reads as it does (encoding, parser,
format, rotation pieces).

## What Changes

- *Stream information* (stream menu, palette, `Alt+I` in the terminal): path and size,
  lines, indexing time, index and cache memory, encoding (and how it was chosen), field
  parser or log format, timestamp format and zone, rotation pieces, source kind. Both interfaces: the window and the terminal (`fasttail-tui`) get it in the same change.

Target release: **0.32.0** (plan of 2026-10-30, after the competitor scan against 0.30.0; see `docs/competitor-analysis.md` section 8). Priority: **Low–Med**. Effort: **S**.

### Non-goals

- A memory profiler.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `stream-engine`: the requirement below is added.

## Impact

`src/tail_engine.rs` counters; a small dialog in each interface; texts; docs.
