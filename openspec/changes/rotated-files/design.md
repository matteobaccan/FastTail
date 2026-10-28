## Context

A stream reads one file through `file_source` (with the block cache) or a spool (compressed
files, stdin, system sources). Pattern streams switch to the newest match but never read
two files as one; merged views interleave streams by timestamp.

## Goals / Non-Goals

**Goals:** yesterday's lines and today's in one stream, in file order, without copying
uncompressed pieces; the live file still followed.

**Non-Goals:** timestamp ordering, rotations elsewhere than next to the file.

## Decisions

### D1. Recognised names
`<name>.<n>`, `<name>.<n>.<gz|xz|zst|bz2>`, `<name>-<YYYYMMDD>[.<ext>]`,
`<stem>.<YYYY-MM-DD>.<ext>` (logback, log4j `DailyRollingFileAppender`). Ordered by number
descending, dates ascending; the live file last. At most 256 pieces.

### D2. A chained source
One logical byte range: the pieces back to back, each compressed piece decompressed once
into the spool (as a compressed file is today), plain ones read in place. The line index
records each piece's start offset; a line never spans two pieces (a piece without a final
line break gets one).

### D3. Rotation while following
When the live file shrinks or its identity changes and a new `app.log.1` has the old
identity, the chain appends the new live file instead of reloading.

### D4. Ask by default
A first open shows the offer; `always` and `never` remember the choice per setting, not per
file. Sessions keep `rotations=1` for the stream.

## Risks / Trade-offs

- [A folder with thousands of dated pieces] → the 256 cap, newest kept, a notice.
- [Decompression time of many `.gz` pieces] → the existing background job with progress;
  the plain pieces are shown first.

## Open Questions

- Include rotations of a pattern stream (`app-*.log`)? Proposed: no, its newest file only.
