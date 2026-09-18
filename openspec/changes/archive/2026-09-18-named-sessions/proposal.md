## Why

FastTail restores one workspace: the last one. Users who alternate between projects (production incident, local dev, a customer's bundle) rebuild their tab set each time. SnakeTail saves and loads named sessions, also from the command line.

## What Changes

- "Session" menu: Save session as..., Load session, Recent sessions, Save current as default.
- A session file (`.fasttail-session.ini`) stores open files and patterns, dock layout, floating windows, per-stream filters, search queries, wrap, encoding and bookmarks; global settings (theme, language, rules) stay in `fasttail.ini`.
- Loading a session replaces the current workspace after confirmation; unsaved changes to a named session are indicated in the title bar with `*`.
- `command-line-arguments` will accept a session file path to open at startup.

## Capabilities

### New Capabilities
- (none)

### Modified Capabilities
- `cyber-ui-docking`: adds named sessions to workspace persistence.

## Impact

- `src/config.rs`: split workspace state into a `Session` struct with its own INI serialisation, reused by the default workspace.
- `src/ui/app.rs`: session menu, dirty tracking, load/replace flow, recent sessions in `fasttail.ini`; `src/i18n.rs`: keys.
