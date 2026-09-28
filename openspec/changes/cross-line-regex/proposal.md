## Why

Some questions are about a sequence of lines, not one line: "a `BEGIN TRANSACTION` followed
within five lines by `deadlock`", "an exception whose third frame is in `PaymentGateway`",
"a request line immediately followed by another request line (the response is missing)".
The stream search matches one line at a time, as plain case-insensitive text, so these
need an export and `grep -Pzo` or a script. klogg's open request #333 asks for regex that
spans lines; the post-0.12.0 scan lists cross-line regex among the low-value, S–L gaps
(gap 20). It is small in the UI and self-contained in the engine.

## What Changes

- The stream search box gets a **mode** selector: **Text** (today's behaviour, the
  default), **Regex** (a regular expression matched against each visible line, case
  following an `Aa` toggle in the box), and **Multi-line regex**.
- In **Multi-line regex** mode the pattern is matched against the **visible lines joined
  with `\n`**: `\n` in the pattern crosses a line end, `(?s)` lets `.` cross them, `^` /
  `$` match at line starts and ends. A hit may start and end anywhere in a line; it
  covers the lines from its first to its last byte.
- A hit is limited to **64 lines and 64 KB**; a match that would be longer is not
  reported (the box says how many were skipped).
- Hits are shown as today for their **first row** (`▶` / `●`, tint) and with a thinner
  **bracket** in the marker column and a lighter tint on the rows the hit continues on.
  The counter counts **hits**, `F3` / `SHIFT + F3` move hit to hit, and the search results
  pane lists each hit's first line with `+N lines`.
- Hidden lines are not part of the text: with a filter active, a hit spans the rows the
  user sees as consecutive (matching what is on screen and what copy writes).
- Runs where the search runs today: UI thread up to 16 MB, a background job above, with
  the same 1,000,000-hit cap; appended lines are searched with a 64-line overlap so a hit
  that spans the old end is found.
- The mode is saved per stream as `search_mode=regex|multiline` (only when not text);
  the search history stores the query with its mode.

Target release: **0.14.0** (structured logs and analysis), per the release plan in
`docs/competitor-analysis.md` section 8. Priority: **Low**. Effort: **M**.

### Non-goals

- Multi-line regex in the include / exclude filter, highlight rules, the Find results tab
  or `headless-print` (possible follow-ups once the search engine supports it).
- Backreferences and look-around (the `regex` crate does not support them; the linear-time
  guarantee is worth more here).
- Multi-line matching in the HEX and rendered Markdown views.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `search-and-navigation`: adds Search Modes and Multi-Line Regex Search.

## Impact

- `src/tail_engine.rs`: `SearchMode { Text, Regex, Multiline }` and the compiled pattern
  per stream; hits as `(first_line, line_count)` in multi-line mode (`search_matches`
  stays the first lines; a parallel `Vec<u8>` of extra line counts, capped at 64); row
  marking for continuation rows; append overlap.
- `src/scan_job.rs`: `JobSpec::Search` gains the mode; a chunked multi-line scanner over
  the visible lines (1 MB chunks with a 64-line / 64 KB overlap).
- `src/ui/dock.rs`: mode selector and `Aa` in the search box, bracket marker and tint;
  `src/ui/hit_list.rs`: `+N lines` in the results pane; `src/ui/overview_strip.rs`:
  marks for the whole hit.
- `src/session.rs`: `search_mode` in `StreamEntry`; `src/config.rs`: history entries with
  their mode.
- `src/i18n.rs` (16 languages), help dialog, README, CHANGELOG, tests, a bench case.
- **TUI (0.20.0, PR #132)**: the scanner is in the engine, so `/` in the TUI can offer the
  same modes (a `\n` in the typed pattern switching to multi-line).
