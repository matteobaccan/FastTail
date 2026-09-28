## Context

Manual bookmarks live in `TailEngine::bookmarks` (at most 1,000 saved per file) with notes
in `bookmark_notes: BTreeMap<usize, String>` (one line, at most 200 characters); automatic
bookmarks come from rules, capped by `auto_bookmark_max` (default 10,000). Timestamps are
detected per line and cached by the engine (`timestamp.rs`). `get_line(idx)` reads a line
through the block cache. The go-to popup (`CTRL + G`) accepts a line number, `+N` / `-N`
and a time. Today's export writes on the UI thread, streaming to the file.

## Goals / Non-Goals

**Goals:** a readable Markdown write-up of the bookmarks with context, in two clicks; tags
that need no new storage; bounded cost for every combination of streams and bookmarks.

**Non-Goals:** other report formats, a tag database, bookmark panels.

## Decisions

### D1. Tags are parsed from the note text
A tag is `#` followed by 1 to 32 characters from letters, digits, `-`, `_`, `.`, at the
start of the note or after a space, compared case-insensitively (`#Deploy` = `#deploy`).
Nothing new is stored: tags are saved, restored, persisted per file and in sessions exactly
as the notes are. A trailing `.` is not part of the tag (`see #db.` → `#db`).
*Alternative:* a separate `tags` key per bookmark — rejected: a new persistence format, an
editor, and old builds would drop them silently; the note already survives every path.

### D2. Report format
~~~markdown
# FastTail bookmark report
Generated 2026-09-28 14:30 by FastTail 0.14.0 · 2 streams · 5 bookmarks · 14:02:05 – 14:09:41
Tags: #deploy (2), #oom (1)

## app.log
`D:\logs\app.log`

### Line 1,204 · 14:02:11.123 · retry storm starts here #deploy
```text
  1201 | ...
> 1204 | ERROR payment failed
  1207 | ...
```
~~~
- Line numbers are 1-based, as shown in the view; the bookmarked line is marked `>`.
- The fence is three backticks, lengthened to one more than the longest backtick run found
  in the block, so a log line holding three backticks cannot close it.
- Lines are written without ANSI escapes when the stream's ANSI mode is render or strip,
  as export does, and cut at 2,000 characters with `…`.
- Overlapping context of neighbouring bookmarks is merged into one block with several `>`
  lines, so no line is written twice.
- Order `stream`: streams in dock order, bookmarks in line order. Order `time`: one flat
  list sorted by timestamp across streams, each heading carrying the stream name; bookmarks
  without a timestamp go last, in stream and line order.
- The stream heading uses the tab title; the path is the file (the entry path for an
  archive entry, `stdin` for standard input).

### D3. Where the work runs
The UI thread collects, per stream, the bookmark lines, notes and timestamps (already in
memory) and reads the context lines through `get_line`: at most
`bookmarks × (2N + 1)` reads, e.g. 1,000 × 7 = 7,000 per stream at the default N = 3.
When the total exceeds 20,000 lines (many streams, many automatic bookmarks or N = 20) the
UI thread only collects the byte ranges of the needed lines from each engine's line index
and a worker reads them with its own handle, opened in the same shared read mode as scan
jobs (Windows writers keep writing), and builds the text; the dialog shows progress and a
Cancel button. Compressed streams read their spool; standard input its spool too.
Automatic bookmarks, when included, are capped at 1,000 per stream in the report (the
first ones in line order), and the report says how many were left out.

### D4. Size and destinations
The clipboard takes the report when it is at most 4 MB; above that the Copy button is
disabled with a tooltip and only Save is offered. Save opens the native dialog with
`fasttail-report-<date>.md` and writes UTF-8 without BOM, `\n` line ends.

### D5. `#tag` in the go-to popup
A go-to input starting with `#` is a tag: it jumps to the next bookmark after the current
row carrying that tag (only manual bookmarks have notes, so only they have tags), visible under the active filters, wrapping around with the search's beep; with no such
bookmark the popup shows "no bookmark tagged #x". Typing `#` lists the stream's tags as
suggestions.

## Risks / Trade-offs

- [A `#` word that is not meant as a tag, e.g. `issue #42`] → tags must contain a letter
  (`#42` is not a tag); worst case an extra tag appears in the summary.
- [File rewritten between bookmarking and the report] → bookmarks are already dropped on
  truncation; the worker checks the file length and writes `(line unavailable)` for a
  range past the end.
- [Large context on huge files] → bounded by D3 and the 1,000 cap per stream.

## Migration Plan

No migration: tags are note text; new settings default as listed. Older builds show the
tags as plain text in notes.

## Open Questions

- Should the report also list the time delta between consecutive bookmarks (useful for
  "3 minutes between deploy and OOM")? Proposed: yes, in the `time` order only.
- Should the main-menu entry be "all streams" or "streams with bookmarks" (skipping empty
  ones)? Proposed: skip empty streams and say how many were skipped.
