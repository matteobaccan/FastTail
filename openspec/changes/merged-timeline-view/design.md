## Context

The renderer, filters and search all talk to `TailEngine` through a small read-side surface: `total_lines`, `get_line`, `is_line_visible`, `filtered_lines`, `search_matches`. A merged view needs the same surface over a different index space.

## Goals / Non-Goals

**Goals:** correct ordering by timestamp with live updates, same features as a normal stream, clear attribution of every row.

**Non-Goals:** merging files without timestamps; clock-skew correction between hosts; merging more than 16 sources.

## Decisions

- **Extract a `StreamView` trait** from `TailEngine`'s read side and implement it for both the engine and the merged stream, so `show_rows`, filters and search code paths are shared rather than duplicated.
- **Order vector of (source, line) pairs**, rebuilt incrementally: on append, new lines from a source are merged into the tail of the order vector (logs are near-ordered, so the insertion point is found by a backward scan bounded to 10,000 entries, then binary search). Truncation of a source rebuilds the order from scratch.
- **Filters and search run on the merged view's own state**, evaluating each row's text through the source engine's `get_line`; highlight rules come from the global rules.
- **Attribution** via a 2-character colour chip per source in the marker column plus the short name in a tooltip; the source colour follows a fixed 8-colour palette per theme.
- **Persistence** stores the source paths; on restart the merged view is recreated once all sources are open.

## Risks / Trade-offs

- [Sources with skewed clocks interleave wrongly] → out of scope; documented.
- [Memory: 12 bytes per merged row] → fine at tens of millions of rows.
