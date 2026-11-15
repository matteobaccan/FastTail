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

### D2. Rows as logfmt
Each message is `<ISO time> <LEVEL> ecu=… apid=… ctid=… tms=… type=… msg="…"`: the
timestamp and level detectors read the start, the logfmt parser (detected on its own) gives
the columns, so no new field parser is needed.

### D3. Resynchronise on damage
A record whose header does not hold is skipped up to the next `DLT\x01`; a verbose
argument of an unsupported type (arrays, structs, fixed point) ends the decoding of that
message, and its rest is written as hex.

## Open Questions

None (the maintainer asked for the change on 2026-11-15).
