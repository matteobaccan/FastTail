## Why

The window searches every open stream at once (`Ctrl+Shift+F`, the Find results tab). The
terminal interface reached GUI parity in 0.20.0 except for this feature, which the
`terminal-interface` spec left out; the maintainer was asked whether it should come to the
terminal. On a server, looking for a request id across several logs is the common case.

## What Changes

- `Ctrl+Shift+F` (where the terminal reports it), an alternative key that every terminal
  delivers, and the palette entry "Search all streams" open a **Find results** window in the terminal dock with the focused stream's query.
- `Enter` searches every open stream with the same rules as the window (each stream's
  filters and the global filter, ANSI stripped where the stream strips it, HEX and ASM
  streams skipped), one background job per stream, at most four at a time.
- Results are grouped by stream (name, count, progress), collapsible; the arrows and the
  pages walk them; `Enter` focuses the stream on the line (follow paused) without changing
  that stream's own search.
- `r` refreshes, `Esc` / `[x]` closes and cancels; a stream reloaded since the search is
  marked stale.
- Up to 100,000 hits listed per stream, the true total counted, as in the window.

Target release: **0.24.0** (candidates of the 2026-10-05 competitor scan, `docs/competitor-analysis.md` section 6, assigned by the maintainer on 2026-10-05), per the release plan in section 8. Priority: **medium**. Effort: **M (1–3 weeks)**.

### Non-goals

- Searching files that are not open (a grep over a folder).
- Regex options beyond the window's search.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `terminal-interface`: Search all streams and the Find results window.

## Impact

- `src/find_all.rs` is already UI-agnostic (jobs, grouping, stale detection): reused.
- `src/tui/`: a Find results window kind in the dock (`src/tui/dock.rs`), its drawing and
  keys (`src/tui/app.rs`), the palette entry (`src/tui/palette.rs`).
- `src/i18n_tui.rs`, `docs/tui.md`, the parity checklist, CHANGELOG, tests.
