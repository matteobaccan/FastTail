## Why

Investigating a log means going back and forth between a handful of interesting lines. FastTail only remembers the current search match. Tailviewer, SnakeTail and klogg all let the user mark lines and jump between them; klogg even shows marks together with matches in the filtered view.

## What Changes

- Ctrl+F2 toggles a bookmark on the row under the search cursor or the selected row; F2 / Shift+F2 jump to the next / previous bookmark of the focused stream, wrapping around.
- Bookmarked rows show a `★` in the marker column (the search marker takes precedence on the current match) and a subtle tint across the row.
- Bookmarks are per stream, kept as line indices, survive filter changes, are dropped on truncation, and are persisted per file path in `fasttail.ini` so reopening a file restores them while the file still has those lines.
- "Clear bookmarks" in the stream menu.

## Capabilities

### New Capabilities
- (none)

### Modified Capabilities
- `search-and-navigation`: adds bookmark toggling, navigation and persistence.

## Impact

- `src/tail_engine.rs`: `bookmarks: BTreeSet<usize>`, toggle/next/prev helpers, pruning on truncation.
- `src/config.rs`: `[bookmarks]` section keyed by file path (list of line indices), size-capped.
- `src/ui/dock.rs`: shortcuts, marker glyph, tint, menu entry; `src/i18n.rs`: keys.
- `tests/integration_tests.rs`: toggle, navigation with wrap, persistence round-trip, pruning.
