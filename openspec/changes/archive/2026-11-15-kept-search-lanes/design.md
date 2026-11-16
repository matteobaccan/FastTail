## Context

The timeline strip caches its columns, the search lane and the out-of-order marks per
stream, rebuilt when a cache key changes. Kept searches hold their hits in
`NamedSearch::hits`.

## Goals / Non-Goals

**Goals:** a lane per kept search in the window's timeline. **Non-Goals:** the terminal.

## Decisions

### D1. Same lanes as the search lane
`kept_lanes` reuses `search_lane` for each kept search, so a hit lands in the same column
as in the search lane.

### D2. The key follows the kept searches
`kept_key` folds the colours and hit counts into the cache key, so adding, removing or
recounting a kept search rebuilds the strip (still at most every 250 ms while the stream
grows).

### D3. Lanes take height from the bars
Each lane is 3 px plus a 1 px gap under the search lane; the strip keeps its 56 px and the
bars get the rest (eight kept searches leave about 20 px of bars). The peak label moves to
the top of the bars.

## Risks / Trade-offs

- [Eight lanes squeeze the bars] → eight is the most a stream keeps; the bars still show
  the shape of the volume.
