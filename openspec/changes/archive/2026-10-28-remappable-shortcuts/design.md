## Context

Shortcuts are checked in place: `app.rs` consumes `F1`, `CTRL + Shift + T`, `CTRL + L`,
`Alt + 1..9`, `CTRL + Shift + 1..9`, the tool shortcuts, `CTRL + Shift + F` (through
`find_results::consume_find_all_shortcut`) and `CTRL + Shift + H` (through
`global_filter_bar::consume_shortcut`); `dock.rs` checks `F3`, `CTRL + F`, `Alt + W`,
`CTRL + K`, `CTRL + G`, `CTRL + A`, `CTRL + C`, `CTRL + Shift + D`, the `F2` family, `Space`,
arrows and pages for the focused stream. Order matters: window-level keys are consumed
before the dock is drawn so a stream does not also see them. Tool shortcuts are parsed by
`external_tools::Shortcut::parse` (`Ctrl+Shift+F9`, at least one modifier). Menu labels
embed key text (`"{}  (CTRL + C)"`).

## Goals / Non-Goals

**Goals:** a single source of truth for bindings; rebinding without restart; no silent
conflicts; defaults identical to 0.12.0; cheap per frame.

**Non-Goals:** chords, modes, mouse bindings, presets.

## Decisions

### D1. Action table in code, overrides in the ini
`ActionId` is an enum; each variant has a stable string id (never renamed: it is the ini
key), an i18n name key, a scope and default bindings. Families are expanded into distinct
actions (`tab.activate.1` … `.9`, `label.toggle.1` … `.9`) so each can be rebound. The
table holds about 60 actions. `KeyMap` = defaults, then `[shortcuts]` overrides, then tool
shortcuts; rebuilt only when settings change, not per frame.
*Alternative:* a data file of defaults shipped with the binary — rejected, the code already
needs a handler per action, and a data file can drift from it.

### D2. Scopes and dispatch order
Scopes: **window** (always, even with no stream: help, always-on-top, lock, find all,
global filter bar, tabs, zoom, palette), **stream** (the focused stream's view: search,
bookmarks, follow, wrap, collapse, context, go to, copy, select all, navigation keys),
**list** (Find results list and search results pane: walking keys). Conflicts are checked
between window and every other scope, and within a scope; stream and list keys may
coincide (they are active in different places, as `↑` is today). Dispatch keeps today's
order: window actions and tools consumed before the dock, then the focused widget's scope.
Stream-scope bindings without modifiers (`Space`, `F2`, arrows) are ignored while a text
field has the keyboard, as today.

### D3. Binding syntax
`KeyBinding { modifiers, key }` parsed from `Ctrl+Shift+F9`, `Alt+W`, `F3`, `Space`,
`Shift+F2`, case-insensitive, `+` separated (the existing parser's `-` separator is kept for
tools); `Cmd` and `Ctrl` both mean egui's `COMMAND` (so a map moves between Windows and
macOS). Several bindings are separated by `,`. Written back in canonical form
(`Ctrl+Shift+F9`). Labels show `⌘` / `⌥` / `⇧` on macOS and `CTRL` / `ALT` / `SHIFT`
elsewhere, matching today's label style.

### D4. Fixed keys
Not in the table and not bindable: `Esc`, `Enter`, `Tab`, `SHIFT + Tab`; arrows, `Home`,
`End`, `PgUp`, `PgDown` and `CTRL + A / C / V / X / Z` while a text field has focus. Stream
navigation keys (arrows, pages, `CTRL + Home` / `End`) are in the table (they can be moved,
e.g. to `J` / `K`) but cannot be left unbound: removing the last binding of a navigation
action is refused. Recording a fixed key shows "reserved".

### D5. Conflicts
On record: if the chord is bound to another action in a conflicting scope (D2) or to a
tool, the recorder names it and offers "Move here" or "Cancel". On load: overrides are
applied in table order; a later action taking a key already taken is left without that
binding and listed in a one-time notice in Settings; tools conflicting with built-ins are
flagged in the tool row and do not fire (the built-in wins), as before the key map a tool
could silently win — this is the behaviour change to call out in the CHANGELOG.

### D6. Performance
Per frame, dispatch walks the pressed keys of `egui::InputState` (usually zero or one
event) and looks each up in a `HashMap<KeyBinding, ActionId>` per scope: O(events), not
O(actions). No allocation in the steady state.

### D7. Recorder
Clicking a binding opens a capture field that consumes the next non-modifier key press
with its modifiers (the key is not dispatched to the app); `Esc` cancels (so `Esc` cannot be
bound, D4). Keys egui cannot see (e.g. `Print`) are not offered.

## Risks / Trade-offs

- [Refactor touches every shortcut site] → one PR per area (window, stream, lists), each
  with tests that the default map produces identical dispatch to 0.12.0.
- [Keyboard layouts: egui reports logical keys; `CTRL + Shift + 1` may arrive as `!` on some
  layouts] → the recorder records what egui reports, which is what dispatch will see, so a
  recorded binding always works on that machine.
- [macOS system shortcuts (`⌘ + H`, `⌘ + Q`)] → not capturable; documented.

## Migration Plan

No `[shortcuts]` section means the 0.12.0 defaults. Tools keep their `shortcut` key.
Rolling back to an older build ignores `[shortcuts]`.

## Open Questions

- Should the README table be generated from the table at build time (a test that fails
  when it drifts) or kept by hand with a check? Proposed: a test that compares the default
  bindings with the README table rows.
- Should the TUI share the GUI's `[shortcuts]` or have its own section? Proposed: its own
  (`[tui_shortcuts]`), same ids — to settle in PR #132.
