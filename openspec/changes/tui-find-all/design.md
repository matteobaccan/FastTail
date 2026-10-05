## Context

`src/find_all.rs` runs one background search job per stream, groups hits, detects stale
streams and caps the listed hits; the window draws it in a dock tab
(`src/ui/find_results.rs`). The terminal dock (`src/tui/dock.rs`) holds stream windows and
dialogs; the `terminal-interface` spec excluded Find results from 0.20.0.

## Goals / Non-Goals

**Goals:** the window's Search all streams in the terminal, same results, same limits.

**Non-Goals:** folder search, new search syntax.

## Decisions

1. **Reuse `find_all`** unchanged; the terminal adds a window kind that is not a stream,
   so the dock gains a variant for it (it is not saved in the layout, as in the window).
2. **Keys:** `Ctrl+Shift+F` when the terminal reports it, plus a palette entry and a key
   that works everywhere: `Alt+/` (to be confirmed against the key map; `F` is the global
   filter editor).
3. **Rows:** a group header (`▼ app.log — 57 matches 40%`), then `line  text` rows with the
   query highlighted, virtualized like the stream rows.
4. **Enter** focuses the stream's window and puts its cursor on the line, follow paused,
   without touching its search; the results window keeps the keyboard so the arrows walk
   on, as in the window.
5. **Narrow terminals:** below 60 columns the line numbers are dropped first.

## Risks / Trade-offs

- [Key not delivered by some terminals] → the palette entry and the alternative key.

## Open Questions

- Should the results window open beside the focused window or replace it? Proposed:
  beside (split right), like `s`.
