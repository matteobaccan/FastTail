## Context

`structured-fields` parses each line with a detected or chosen parser (JSON, logfmt, regex,
Apache, syslog) and draws a column view with chosen columns and widths. Rows are lines;
multiline grouping exists for stack traces.

## Goals / Non-Goals

**Goals:** read CSV / TSV exports as tables with names from the header, correct quoting,
without loading the file.

**Non-Goals:** sorting, editing, spreadsheets.

## Decisions

1. **Detection** on the first 64 lines: the separator whose count is the same (and above
   zero) on at least 90 % of them, quotes taken into account; the extension breaks ties.
2. **Header** when the first row's cells are all non-numeric, distinct and not repeated
   in the next rows; the user can turn it off.
3. **Quoted newlines:** a record whose quotes are not closed at the end of a line
   continues on the next one. The engine joins such lines into one row through the same
   mechanism as stack-trace grouping, so the line index stays per physical line and
   nothing is held in memory. A record that does not close within 1,000 lines is cut
   there (a broken file must not swallow the rest).
4. **Cells are parsed on demand** for the rows on screen and in the filter pass, without
   allocation per line in the hot path (cell spans into the line buffer).

## Risks / Trade-offs

- [Misdetected separator] → the selector lets the user pick it; saved per stream.
- [Huge single records] → capped at 1,000 lines.

## Open Questions

- Should a CSV open in the column view by default, or stay as text with the parser
  ready? Proposed: column view by default for `.csv` / `.tsv`.
