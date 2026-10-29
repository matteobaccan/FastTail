## Why

Several smaller tools of the window are missing in the terminal: rule sets import /
export, the next / previous match of a rule, clearing all bookmarks, the Log formats
dialog (formats load only at start), the editor presets and Open in FastTail from a source
location, the per-stream filter expression toggle and editing the global filter's
expression. Each is small; together they are the everyday friction of using the terminal.

## What Changes

- Rule editor: **Import / Export** a rule set (`.fasttail-rules.ini`), and `F4` / `Shift+F4`
  for the next / previous line of the selected rule.
- Bookmarks: **Clear all** (palette, confirmed).
- **Log formats** dialog (palette): the list (built-in locked), edit a format's fields with
  the samples' match count as a preview, save, import, export; the streams are refreshed.
- Source locations: **editor presets** in Settings and **Open in FastTail at that line**
  (`Ctrl+O` asks editor or FastTail when both apply).
- Filters: the **ƒx** toggle per stream (include terms ↔ expression) and the global
  filter's expression editable.

Target release: the next minor after 0.28.1 (terminal parity with the window, proposed on 2026-10-29 at the maintainer's request; planned when the maintainer agrees), per `docs/competitor-analysis.md` sections 7 and 8. Priority: **Low–Med**. Effort: **M**.

### Non-goals

- A full regex IDE in the formats dialog (the window's preview is richer).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `terminal-interface`: the requirements below are added.

## Impact

- `src/tui/rules.rs`, a formats dialog over `crate::log_format`, `src/tui/settings.rs`
  presets, the include prompt's expression mode; texts, `docs/tui.md`, README, CHANGELOG.
