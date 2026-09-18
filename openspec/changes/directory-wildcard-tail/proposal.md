## Why

Rotating loggers write `app-2026-09-18.log`, `app-2026-09-19.log`, ... and the interesting file is always the newest one. SnakeTail tails a directory with a wildcard and switches to the newest match; in FastTail the user has to reopen the new file every day.

## What Changes

- Open dialog and drag & drop accept a pattern such as `C:\logs\app-*.log`; a "pattern stream" tails the newest file matching it and switches automatically when a newer match appears.
- The stream bar shows the pattern and the current file; the switch keeps filters, highlight rules and search, resets the buffer and bookmarks.
- Patterns are persisted in the workspace and recent files.

## Capabilities

### New Capabilities
- (none)

### Modified Capabilities
- `stream-engine`: adds pattern streams that follow the newest matching file.

## Impact

- `src/tail_engine.rs`: `source: Source::File(PathBuf) | Source::Pattern { dir, glob }`, periodic directory scan (every 2 s, reusing the poll timer), `switch_to(path)`.
- `src/config.rs`: pattern entries in open files and recent list; `src/ui/dock.rs` and `app.rs`: pattern display, open dialog support; `src/i18n.rs`: keys.
- New dependency: `glob` or a small in-house matcher (`*` and `?` only).
