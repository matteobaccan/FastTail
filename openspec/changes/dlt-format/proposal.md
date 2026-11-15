## Why

Chipmunk and logana read AUTOSAR DLT, the binary log format of automotive ECUs; FastTail
reads text only. A DLT reader opens FastTail to that niche without a plugin.

## What Changes

- `.dlt` files (storage header `DLT\x01`) open as streams: each message a row with its time,
  ECU, application and context ids, level and payload (verbose arguments decoded, non-verbose
  as hex), as fields for the column view, filters, statistics and queries.
- Read through a block-based decoder, never the whole file in memory. Both interfaces: the window and the terminal (`fasttail-tui`) get it in the same change.

Target release: **0.34.0** (asked by the maintainer on 2026-11-15; first planned for 0.33.0). Priority: **Low**. Effort: **L**.

### Non-goals

- DLT over TCP / serial (a source plugin can feed it).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `log-formats`: the requirement below is added.

## Impact

A new `src/dlt.rs` decoder producing text rows (logfmt fields); detection in `workspace::open_target`; the stream information kind `info_kind_dlt`; docs. `--print` does not read DLT files.
