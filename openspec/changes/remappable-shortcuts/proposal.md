## Why

FastTail has about forty shortcuts and none can be changed. They are hard-coded where they
are handled (`app.rs`, `dock.rs`, the find results and global filter modules), repeated as
text in menu labels (`(CTRL + C)`, `(CTRL + K)`), in the help dialog and in the README, so
they drift apart. Users coming from BareTail, klogg, less or an IDE expect their own keys;
a laptop without `F2` / `F3` or a keyboard layout where `CTRL + Shift + 1..9` produces
other symbols leaves features unreachable. External tools can take a shortcut, but nothing
stops one from silently shadowing a built-in key. klogg, LogViewPlus and lnav let users
remap keys; the post-0.12.0 competitor scan ranks the gap 14 (value Medium, effort M), and
the terminal interface planned for 0.20.0 needs rebindable keys too.

## What Changes

- **One action table**: every keyboard action has a stable id (`search.focus`,
  `search.next`, `bookmark.toggle`, `view.follow`, `tab.activate.3`, `label.toggle.2`, …),
  a localized name, a scope (window, stream, list) and its default bindings. Every handler,
  menu label, tooltip, the help dialog and the README table read the bindings from it.
- **Rebinding in Settings** → "Keyboard shortcuts": a searchable list of actions with their
  bindings; click a binding and press the new keys to record it; add a second binding;
  remove one; **reset one** or **reset all** to the defaults.
- **Conflict detection**: a binding already used by another action in an overlapping scope
  (or by an external tool) is shown in the recorder with the action it belongs to; saving
  it asks to move it (the other action loses it) or cancel. Conflicts found when loading
  `fasttail.ini` are reported once and the first action in table order keeps the key.
- **External tools share the table**: their shortcuts appear in the list under "External
  tools" and go through the same recorder and conflict check; they stay stored in their
  `[tool.N]` section.
- **Persistence**: new `fasttail.ini` section `[shortcuts]`, one key per action that
  differs from its default, e.g. `bookmark.toggle=Ctrl+B`, `search.next=F3, Ctrl+N`, or an
  empty value for an unbound action. Unknown ids and unparseable values are ignored and
  reported.
- A few keys stay **fixed** because text editing depends on them: `Esc`, `Enter`, `Tab`,
  the arrows and `Home` / `End` inside text fields and lists, `CTRL + A / C / V / X / Z`
  inside text fields.

Target release: **0.22.0** (planned for 0.15.0, moved after the terminal interface of 0.20.0; sources and integrations), per the release plan in
`docs/competitor-analysis.md` section 8. Priority: **Medium**. Effort: **M**.

### Non-goals

- Key sequences / chords (`Ctrl+K Ctrl+S`), vim-style modes and counts in the GUI.
- Mouse-button bindings.
- Per-stream or per-session key maps; import / export of a key map file (the
  `[shortcuts]` section can be copied).
- Preset key maps ("BareTail", "less") — a possible follow-up once the table exists.

## Capabilities

### New Capabilities

- `keyboard-shortcuts`: the action table, rebinding in Settings, conflict detection,
  persistence in `[shortcuts]`, fixed keys, and shortcuts shown from the table.

### Modified Capabilities

None. The default bindings stay those of Keyboard Navigation & Hotkeys (`cyber-ui-docking`)
and of the other specs; the requirement of that spec is left unchanged and the new
capability says the defaults can be rebound.

## Impact

- `src/actions.rs` (created by `command-palette` in 0.13.0; extended here with key bindings): `ActionId` enum with string ids, names (i18n keys), scopes,
  default `Vec<KeyBinding>`; `KeyMap` resolved from defaults + `[shortcuts]` + tools;
  `consume(ctx, ActionId)` helper; label formatting per platform (`Ctrl` / `⌘`).
- `src/external_tools.rs`: `Shortcut` becomes `KeyBinding` in `actions.rs` (extended to
  plain keys for stream actions such as `Space`, `F2`); tools keep the at-least-one-modifier
  rule.
- `src/ui/app.rs`, `src/ui/dock.rs`, `src/ui/find_results.rs`,
  `src/ui/global_filter_bar.rs`, `src/ui/hit_list.rs`: every `consume_key` /
  `key_pressed` for an action replaced by the key map; menu labels built from it.
- `src/config.rs`: `[shortcuts]`.
- Settings page, help dialog generated from the table; README table generated or checked
  by a test against the defaults.
- `src/i18n.rs` (16 languages: action names, recorder texts), CHANGELOG.
- `command-palette` uses the same table.
- **TUI (0.20.0, PR #132)**: the TUI adds its own default bindings (vim / less) for the
  same ids in a separate section, e.g. `[tui_shortcuts]`, so a GUI rebinding does not
  break terminal muscle memory; the palette and help screen there read the same table.
