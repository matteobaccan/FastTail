## Why

Five small things other viewers do and FastTail does not, each a few days of work, all
from the post-0.12.0 competitor scan (gaps 8 to 11):

- **Where else does this id appear?** Seeing one request id or IP in a row, the user has
  to type it into the search box to see its other occurrences. LogViewPlus highlights the
  selected text everywhere in the view.
- **Next line of this rule.** `F3` walks the search hits and `F2` the bookmarks, but there
  is no way to jump to the next line a given highlight rule paints (the next red ERROR
  row, the next yellow slow query) without turning that rule into a search.
- **Sharing rules.** A team that tuned twenty highlight rules cannot hand them to a
  colleague except by copying `[highlight_N]` sections out of `fasttail.ini` by hand.
  LogFusion and LogViewPlus import and export rule sets.
- **Time zones.** Servers log in UTC, users think in local time, and some logs print epoch
  seconds or milliseconds (`1790604125123`) that nobody can read. FastTail parses them all
  for the time range but always shows the text as printed. hl and LogViewPlus convert
  (LogExpert's third most-voted request is readable Unix timestamps, #139).
- **Zero-configuration colour.** tailspin (8k stars) colours IPs, UUIDs, URLs, durations
  and paths with no setup. FastTail colours only levels and user rules.

## What Changes

- **Selection highlight**: double-clicking a word in a row (a token such as an id, an IP,
  a UUID, a path) outlines every occurrence of that exact token in the rows of that
  stream, without changing the search; `Esc`, a double-click on empty space or on the
  same token again clears it. The row menu offers the same ("Highlight 'token'").
- **Next / previous line of a rule**: the row menu lists the highlight rules that match
  the row ("Next line of rule ▸"); `F4` / `SHIFT + F4` then go to the next / previous
  shown line that rule matches, wrapping around with the search's beep. Without a chosen
  rule, `F4` uses the first rule that matches the selected row.
- **Rule-set import / export**: "Export rules…" and "Import rules…" in the Highlights
  dialog write and read a `*.fasttail-rules.ini` file with the same keys as
  `[highlight_N]` in `fasttail.ini`; import appends (skipping duplicates) or replaces
  (after confirmation).
- **Time display**: a per-stream choice to show the leading timestamp of each row **as
  written** (default), in **UTC**, in **local** time or at a **fixed offset**, and to show
  epoch seconds / milliseconds as dates when a zone is chosen. A per-stream **source
  zone** says what a timestamp without a zone suffix means (local by default). Only the
  display changes: filters, search, copy and export see the original text; the time span,
  the time range popup and the histogram follow the chosen zone.
- **Automatic highlighting** of IPv4 / IPv6 addresses, UUIDs, URLs, durations
  (`250ms`, `1.5s`, `2m30s`) and file paths, with a colour per kind from each theme,
  switchable in Settings (off by default) with a toggle per kind. It ranks below user
  rules, quick labels and the log's own ANSI colours, above level colouring.
- New keys: `[general] auto_highlight=true|false` and `auto_highlight_kinds=ip,uuid,url,duration,path`
  in `fasttail.ini`; per stream in the workspace and session files `time_display` and
  `time_source_zone` (written only when not the default).

Target release: **0.13.0** (basics and quick wins), per the release plan in
`docs/competitor-analysis.md` section 8. Effort: **M (five S items)**.

### Non-goals

- Partial-line text selection with the mouse (gap 19): the selection highlight works on a
  double-clicked token, not on a dragged range.
- Named time zones with daylight-saving rules (`Europe/Rome`): local, UTC and fixed
  offsets only, without a time-zone database.
- Rewriting timestamps inside the line (only the leading timestamp is converted) or in
  exported files.
- User-defined token kinds for the automatic highlighting (that is what highlight rules
  are for), and automatic highlighting in the HEX and Markdown views.
- Importing rule formats of other viewers (BareTail already has its bridge).

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `filters-and-highlighting`: adds Selection Highlight, Rule Set Import and Export and
  Automatic Token Highlighting.
- `search-and-navigation`: adds Rule Navigation (`F4` / `SHIFT + F4`).
- `log-intelligence`: adds Timestamp Display Zone.
- `cyber-themes`: adds the token colours of each theme.

## Impact

- `src/tail_engine.rs`: selection token per stream and its spans, rule cursor and rule
  seek, auto-token spans in `match_highlight_spans_with` (after ANSI), display timestamp
  substitution in `RowText`.
- `src/auto_highlight.rs` (new): hand-written single-pass token scanner (no regex).
- `src/timestamp.rs`: zone offset and span returned with the timestamp
  (`detect_timestamp_zoned`), `format_in_zone`.
- `src/config.rs`: `auto_highlight`, `auto_highlight_kinds`; rule-set file read / write
  sharing the `[highlight_N]` code.
- `src/theme.rs`: `token_color(kind)` per theme.
- `src/ui/dock.rs`: double-click token pick, outline drawing, row menu entries, `F4`,
  time display menu, Highlights dialog buttons, Settings toggles.
- `src/session.rs`: `time_display`, `time_source_zone` in `StreamEntry`.
- `src/i18n.rs` (16 languages), help dialog, README, CHANGELOG, tests.
