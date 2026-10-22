## Context

The renderer, filters and search all talk to `TailEngine` through a small read-side surface: `total_lines`, `get_line`, `is_line_visible`, `filtered_lines`, `search_matches`. A merged view needs the same surface over a different index space.

## Goals / Non-Goals

**Goals:** correct ordering by timestamp with live updates, same features as a normal stream, clear attribution of every row.

**Non-Goals:** merging files without timestamps; clock-skew correction between hosts; merging more than 16 sources.

## Decisions

- **A spool, not a second read-side** (changed at implementation, 2026-10-22): instead of
  extracting a `StreamView` trait from the 9,000-line engine, a merged view is an ordinary
  `TailEngine` on a spool file, as the derived streams of "Open filter as new tab" are.
  `MergedFeeder` (`src/merged_stream.rs`) writes the sources' lines into it in time order,
  so filters, search, highlight rules, bookmarks, export, the column view and the time
  features work unchanged. `map: Vec<(source, line)>` (9 bytes a row) gives the chip and
  the source line number of every row.
- **Streaming k-way merge**: each source's next line is read with its timestamp (text, or
  the time field with a field parser; a line without one inherits the entry's); the
  smallest is written. A source with no next line holds the merge while it is growing
  and stops holding once quiet for `SETTLED` (2 s), so a late line of a writing source
  is not overtaken, at most 10 s (`MAX_HOLD`: a line that grows without ever ending must
  not stop the view). On equal timestamps the source written last goes on, which keeps a
  stack trace with its entry. A reloaded source, a source turned on or off, and a line
  written before its newline whose source then grew, write the spool again from the
  start; that step writes nothing more, so the merged engine sees the spool empty first.
  A view that stopped (full, every source closed) follows again after a restart. Lines
  of a source closed in the meantime are not in the rewritten view.
- **Attribution** via a `▌` chip in the source's colour (fixed 8-colour palette) before
  the line number, the source name in its tooltip, and the source's own line numbers in
  the gutter; one check box per source in the stream bar turns it on or off.
- **Persistence** (next part): the merged view is not saved yet; it is rebuilt from its
  source paths in a following patch.

## Risks / Trade-offs

- [Sources with skewed clocks interleave wrongly] → out of scope; documented.
- [Memory: 12 bytes per merged row] → fine at tens of millions of rows.
