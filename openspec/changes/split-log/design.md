## Context

Export visible lines writes the filtered lines to one file through a background job.
Timestamps per line are available once the timing pass ran (the histogram and the time
range use it).

## Goals / Non-Goals

**Goals:** cut a large file into pieces by lines, size or time in one action, streaming.

**Non-Goals:** per-field splitting, compression.

## Decisions

1. **One background job** reading the file sequentially through the block cache and
   writing one piece at a time with a buffered writer; memory is bounded by the buffers.
2. **Cut at line ends:** by size, a piece ends at the last full line before the limit (a
   single line longer than the limit becomes its own piece).
3. **By time:** buckets from the line timestamps; lines without a timestamp stay with the
   previous line's piece; the timing pass runs first if needed, with its progress.
4. **Names:** `<stem>.part-NNN<ext>` by lines and size, `<stem>.<bucket start><ext>` by
   time (`2026-10-05T14`); the dialog previews them and refuses to overwrite unless the
   user confirms.
5. **Filters:** off by default (a split is about the file); "visible lines only" uses the
   view's filters as Export does.

## Risks / Trade-offs

- [Disk space] → the preview shows the total size; free space is checked first.
- [Growing file] → the split covers the size at the start; later lines are not included.

## Open Questions

- A command-line form (`fasttail --split-lines 100000 app.log`) for scripts? Proposed as
  part of this change if cheap.
