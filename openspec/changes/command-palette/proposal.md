## Why

FastTail has about forty shortcuts, a stream menu, a row context menu, a title-bar menu,
a Settings page with dozens of switches and a help dialog that lists keys. Finding "where
is the collapse toggle" or "what was the key for the time delta" means opening menus one
by one or reading `F1`. Editors (VS Code, JetBrains, Sublime) solved this with a command
palette; lnav has `:` commands, and the post-0.12.0 competitor scan lists a palette among
the discoverability lessons for the terminal interface (section 7). The same action list
is what `remappable-shortcuts` needs, so building it once serves both.

## What Changes

- **CTRL + SHIFT + P** (and a title-bar menu item "Command palette…") opens a floating
  palette over the window: a text box and a list of commands, filtered as the user types.
- **Every action is listed**: window and stream actions (follow, wrap, collapse, search
  all, go to line, export, bookmark, next / previous match, open file, sessions…), every
  menu item that is not a file path, and every **boolean or enumerated setting** of the
  Settings page as a "Toggle …" / "Theme…" style command. Each row shows the localized
  name, its category (Stream, View, Search, Bookmarks, Session, Settings, Window) and the
  **current shortcut** when it has one.
- **Fuzzy search**: subsequence match on the localized name and on the English name (so
  "wrap" finds "Zeilenumbruch" in German), ranked by consecutive runs, word starts and
  recent use; the last 8 commands used appear first when the box is empty.
- **Context aware**: stream commands act on the focused stream and are greyed, with the
  reason, when no stream is focused or the action does not apply (for example "Next
  match" without a search).
- **Keyboard only**: `Up` / `Down` / `PgUp` / `PgDown` move, `Enter` runs, `Esc` closes;
  commands with a value (theme, language, renderer, time display) open a second step in
  the same palette listing the values.
- **Action registry**: a new `src/actions.rs` holding each action's stable id, i18n name,
  category, scope, enabled condition and shortcut label. The palette dispatches through
  it. `remappable-shortcuts` (0.15.0) extends the same registry with key bindings and
  rebinding; in 0.13.0 handlers keep their current key code and only the palette goes
  through the registry.
- New `fasttail.ini` key `[general] palette_recent` (the last 8 command ids, comma
  separated).

Target release: **0.13.0** (basics and quick wins), per the release plan in
`docs/competitor-analysis.md` section 8. Priority: **Medium**. Effort: **M**.

### Non-goals

- Rebinding keys from the palette (that is `remappable-shortcuts`).
- Typed commands with arguments (`:filter ERROR`, `:goto 1200`); a later change can add a
  `:` prefix mode.
- Listing open files, recent files or lines (a "go to file / go to anything" palette).
- Entries for external tools and filter presets (a possible follow-up).

## Capabilities

### New Capabilities

- `command-palette`: the palette, the action registry it lists, fuzzy ranking, context
  enabling and recent commands.

### Modified Capabilities

None. The shortcut list of Keyboard Navigation & Hotkeys (`cyber-ui-docking`) is left
unchanged; the new capability defines CTRL + SHIFT + P (not used by 0.12.0, which uses
CTRL + SHIFT with `1..9`, `D`, `F` and `H`). The `pattern-grouping` proposal (0.14.0)
opens its Patterns tab with CTRL + SHIFT + G, so the palette keeps CTRL + SHIFT + P (the
convention of every editor).

## Impact

- `src/actions.rs` (new): `ActionId` (string ids such as `view.wrap.toggle`,
  `search.next`, `settings.level_colors.toggle`), `ActionMeta` (i18n key, category, scope,
  shortcut label), `enabled(&state)`, a deferred `run` queue.
- `src/ui/palette.rs` (new): popup, fuzzy scorer (no new crate), value step.
- `src/ui/app.rs`: CTRL + SHIFT + P, title-bar menu item, draining the action queue,
  window actions; `src/ui/dock.rs`: stream actions exposed as functions callable from the
  registry instead of only from menu closures and key handlers.
- `src/config.rs`: `palette_recent`.
- `src/i18n.rs` (16 languages): names for actions that have no menu label yet, category
  names, palette texts; help dialog lists CTRL + SHIFT + P.
- README, CHANGELOG.
- `remappable-shortcuts` builds on `src/actions.rs` instead of creating it.
- **TUI (0.20.0, PR #132)**: the registry is UI-free; the TUI's `:` palette lists the
  same ids with its own key labels.
