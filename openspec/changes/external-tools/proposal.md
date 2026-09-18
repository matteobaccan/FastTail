## Why

A log line often points somewhere else: a request id to paste into a dashboard, a file to open in the editor, a host to ssh into. SnakeTail lets the user configure external tools with placeholders and shortcuts, and even launch them when a keyword matches. FastTail has no way out of the window except the clipboard (once `copy-lines-and-export` lands).

## What Changes

- Settings gain an "External tools" list: name, command, arguments with placeholders (`{line}`, `{file}`, `{lineno}`, `{selection}`, `{match}` for the first regex capture of a tool-specific pattern), optional shortcut, and "run on rule match" binding to a highlight rule.
- Tools appear in the row context menu and in the stream menu; shortcuts run them on the current row.
- A tool bound to a highlight rule runs when the rule matches an appended line, throttled to once per second per tool.

## Capabilities

### New Capabilities
- `external-tools`: user-configured commands launched from a row or on rule match.

### Modified Capabilities
- (none)

## Impact

- `src/config.rs`: `[tool.N]` sections; `src/external_tools.rs`: placeholder expansion, quoting, `std::process::Command` spawn without waiting; `src/tail_engine.rs`: rule-match hook (next to the sound alert) with throttle.
- `src/ui/app.rs` / `dock.rs`: settings editor, context menu, shortcuts; `src/i18n.rs`: keys.
