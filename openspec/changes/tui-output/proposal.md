## Why

Getting lines out of the terminal interface is limited to `y` (copy). The window also
has the search results pane, search in a range, Export (text, CSV, HTML), Copy as shown and
the bookmark report. Over SSH these are often the reason to open a log at all: hand the
errors of the last hour to someone, or write an incident note.

## What Changes

- **Search results pane** (`Alt+R`, the palette): the matching lines in a list over
  the stream (as the Find results window),
  Enter goes to a hit; the hit counter says `[3 / 17 in range]` with a scope.
- **Search in a range** (palette, `R` in the search prompt's history): Selection, Lines
  (`1200-5000`) or Time; `F3`-style `n` / `N` stay inside it.
- **Export** (palette, `e`): visible lines, selected lines or matches as text, CSV or
  HTML, with the CSV options of the window; the path is typed with completion, existing
  files are confirmed.
- **Copy as shown** (`Y`): the rows as they are drawn (cells separated by tabs in the
  column view).
- **Bookmark report** (palette): the Markdown report of this stream or of all of them,
  saved to a file or copied.

Target release: the next minor after 0.28.1 (terminal parity with the window, proposed on 2026-10-29 at the maintainer's request; planned when the maintainer agrees), per `docs/competitor-analysis.md` sections 7 and 8. Priority: **Medium**. Effort: **M**.

### Non-goals

- A file picker beyond the existing path field with completion.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `terminal-interface`: the requirements below are added.

## Impact

- `src/tui/` search pane and scope, an export dialog over `crate::export`, the report over
  `crate::bookmark_report`; texts, `docs/tui.md`, README, CHANGELOG.
