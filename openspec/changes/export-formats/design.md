## Context

`TailEngine::export_lines` streams line text to a `Write` sink on the UI thread;
`export_visible` and `export_search_matches` choose the indices (visible lines include the
lines inside collapsed groups; matches are the stored hits, at most 1,000,000). The level
and the timestamp of a line are cached by the engine. A row's paint spans come from the
rule, label, level, ANSI and hit layers in `dock.rs`. `structured-fields` (proposed for
0.14.0) adds a per-stream parser and a column order.

## Goals / Non-Goals

**Goals:** CSV that opens correctly in Excel and LibreOffice in any locale; HTML that
looks like the screen; no change for text export users; memory independent of the export
size.

**Non-Goals:** other formats, HTML with scripts, Find results export.

## Decisions

### D1. One dialog, the old items kept
The dialog has "What" (visible / selected / search matches; selected is disabled with no
selection) and "Format". The two old menu items open it preset to their scope and text.
*Alternative:* six menu items — rejected, the menu is already long.

### D2. CSV columns
Without a parser: `line,time,level,text`. `time` is ISO 8601 with the precision the line
has (`2026-09-28T14:02:11.123`), the zone suffix only when the line carries one; when
`quick-wins-0-13` time display is set to a zone, the converted time is written, matching
the screen. With a parser: `line`, the shown fields in view order, `message`. A cell is
quoted when it holds the separator, `"`, CR or LF; `"` is doubled. Lines end with CRLF
(RFC 4180, and what Excel expects).

### D3. Formula protection on by default
A cell starting with `=`, `+`, `-`, `@`, TAB or CR is prefixed with `'` (OWASP CSV
injection guidance): a log line is attacker-influenced text and a spreadsheet may execute
it. Numeric-looking fields such as `-5` also get the prefix; the user can turn protection
off in the dialog. *Alternative:* off by default — rejected, the safe default matters more
than a stray `'` in negative numbers.

### D4. HTML
A single file: `<style>` with one class per distinct style (rules, labels, levels, ANSI
colours, hits) so the size grows with lines, not with styles; a `<pre>`-like `<div>` per
line with the line number in a separate, non-selectable span. All text is HTML-escaped;
ANSI sequences are never copied. The background and text colours come from the current
theme. Lines are cut at the long line cap used on screen. 200,000 lines ≈ 30 to 60 MB at
typical line lengths, which browsers still open; above that the dialog says the export is
limited and writes the first 200,000.

### D5. Threads and memory
CSV and text keep today's model: the UI thread writes through a `BufWriter`, one line at a
time; the timestamp and level come from the engine caches, so the extra cost per line is
formatting. For more than 1,000,000 lines, or for HTML (style computation per line), the
export runs on a worker that opens the file with its own handle in the shared read mode of
scan jobs (Windows writers keep writing), with the line byte ranges handed over in chunks
of 100,000 from the UI thread, progress in the stream bar and Cancel. The worker gets a
snapshot of the compiled rules, labels and theme colours, so later edits do not change a
running export. Memory: one chunk of ranges (1.6 MB) plus the writer buffer.
A file that grows during the export: the export covers the lines present when it started.
A file truncated during the export: the export stops and reports it.

## Risks / Trade-offs

- [HTML spans differ from the screen when styles change mid-export] → snapshot (D5).
- [BOM confuses some Unix tools] → the BOM is an option in the dialog, remembered.
- [Field CSV on lines that do not parse] → the fields are empty and `message` holds the
  whole line.

## Migration Plan

No migration: new keys default as listed; the old menu items keep their behaviour.

## Open Questions

- Should selected-lines export also be offered from the row context menu ("Export
  selection…")? Proposed: yes, one item.
- Should the HTML include the bookmark `★` markers and notes? Proposed: markers yes,
  notes no (the 0.12.0 rule that notes are not exported; the bookmark report covers them).
