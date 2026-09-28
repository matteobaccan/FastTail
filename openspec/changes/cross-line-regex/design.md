## Context

The stream search is plain text compared without regard to case (`query.to_lowercase()`),
line by line over the visible lines, on the UI thread up to 16 MB and in a
`JobSpec::Search` job above, storing at most 1,000,000 line hits (`search_matches`) and
counting past the cap. The marker column, the results pane (`src/ui/hit_list.rs`), the
overview strip and `F3` all read `search_matches`. The `regex` crate is a dependency.

## Goals / Non-Goals

**Goals:** regex and multi-line regex search with the same cap, background behaviour and
navigation as text search; memory independent of the file size; no change for text
search.

**Non-Goals:** multi-line filters or rules, backreferences (see the proposal).

## Decisions

### D1. Modes in the search box
A small selector (`ab` / `.*` / `.*⤶`) and an `Aa` toggle (regex modes only; text stays
case-insensitive as today). An invalid pattern tints the box and shows the error; the last
valid hits stay until it is fixed.

### D2. What the multi-line pattern sees
The visible lines, each without its line end, joined with `\n`; the pattern is compiled
with `multi_line(true)` so `^` / `$` work per line, and `(?s)` is left to the user. Hidden
lines are skipped, so a hit crosses a filter gap. *Alternative:* the raw file bytes —
rejected, hits would span lines the user cannot see, and CRLF / encodings would leak into
patterns.

### D3. Chunked scanning with a bound
Lines are appended to a chunk buffer until 1 MB; `find_iter` runs over it; a hit whose
end lies within the last 64 lines or 64 KB of a chunk that is not the last is deferred:
the next chunk starts at the deferred hit's first line (or at the overlap start), so hits
are neither lost nor doubled. A match longer than 64 lines or 64 KB is dropped and
counted. Non-overlapping, leftmost-first, as `find_iter`. Worst case per byte is the
regex engine's linear cost times the overlap ratio (≤ 1.07).

### D4. Hit storage
`search_matches` keeps the first line of each hit (so navigation, cap and counting stay
the same); `search_spans: Vec<u8>` holds the extra line count (0–63) per hit. Row marking
for a continuation row is a binary search for the last hit starting at or before it.

### D5. Growth
On append, the scan restarts at the first line of the last hit or 64 lines before the old
end, whichever is earlier, and replaces hits from there. Truncation and rotation rerun the
search as today.

### D6. Persistence and history
`search_mode` in `StreamEntry` (written only when not text). History entries are stored as
today; a regex entry is prefixed with its mode (`re:` / `mre:`) so older builds show it as
text instead of failing.

## Risks / Trade-offs

- [`.*` with `(?s)` matches to the end of the chunk] → the 64-line bound drops it and the
  box reports the skipped matches, which tells the user to make the pattern lazy.
- [Regex search slower than text search] → it runs in the background above 16 MB as text
  search does; the bench tracks MB/s.

## Migration Plan

Additive; streams without `search_mode` stay in text mode.

## Open Questions

- Should the 64-line bound be configurable? Proposed: no until someone asks.
- Should `Enter` in the multi-line box insert `\n` literally? Proposed: no; `\n` is typed as
  the two characters, `Enter` keeps searching.
