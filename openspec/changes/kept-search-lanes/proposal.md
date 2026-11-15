## Why

Kept searches (0.32.0, after lnav 0.15's named searches) left out one part of lnav's design:
a row per search in the timeline, which shows at a glance when each search hits. The
timeline only marks the current search.

## What Changes

- The window's timeline strip draws one thin lane per kept search, under the search lane,
  in the search's colour, with a mark in every column holding one of its hits.
- The lanes follow the kept searches: added, removed and recounted while the stream grows,
  rebuilt with the strip's cache.

Target release: **0.34.0** (asked by the maintainer on 2026-11-15). Priority: **Low**. Effort: **S**.

### Non-goals

- The terminal interface: it has no timeline strip (its volume row is one character high).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `search-and-navigation`: the requirement below is added.

## Impact

`src/ui/timeline_strip.rs` (cache key, lanes, drawing); docs.
