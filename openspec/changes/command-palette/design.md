## Context

Actions are implemented inline where their key or menu item is handled: window actions
in `app.rs` (title bar, `F1`, zoom, `CTRL + SHIFT + 1..9` labels), stream actions in
`dock.rs` (follow, wrap `ALT + W`, collapse `CTRL + SHIFT + D`, bookmarks, search), lists
in `find_results.rs`, `hit_list.rs` and `global_filter_bar.rs`. Settings are `Config`
fields edited in the Settings page. `remappable-shortcuts` (0.15.0) proposes
`src/actions.rs` with ids, names, scopes and bindings.

## Goals / Non-Goals

**Goals:** every action reachable by name with one shortcut and a few letters; one
registry that both changes share; no behaviour change for existing keys and menus.

**Non-Goals:** rebinding, typed commands, file / line navigation.

## Decisions

### D1. The palette creates the registry, shortcuts extend it
0.13.0 ships `src/actions.rs` with id, name, category, scope, `enabled` and `run`, plus a
static shortcut label. Handlers keep their `consume_key` calls; a test checks that each
label matches the key its handler consumes. `remappable-shortcuts` then replaces the
static label with its `KeyMap` and routes handlers through the registry.
*Alternative:* wait for `remappable-shortcuts` — rejected, it is 0.15.0 and larger.

### D2. Deferred execution
The palette is drawn inside the frame, while many actions need `&mut` access to engines
or the dock state. `run` pushes an `ActionId` into a queue that `FastTailApp::update`
drains after the dock is drawn, as menu clicks already do with flags. Stream actions
target the stream focused when the palette opened.

### D3. Settings as generated commands
A table maps setting ids to `Config` accessors: booleans become "Toggle <name>" with the
current state shown, enums (theme, language, renderer, size unit) open a value step.
Numeric settings (poll interval, font size) are not listed; "Open Settings" is.

### D4. Fuzzy scorer
Hand-written subsequence scorer (fzf v1 style): bonuses for consecutive matches, word
starts and the name start, penalty for gaps; case folding plus an accent-folding table
for Latin scripts; CJK compared by characters. Both localized and English names are
scored, the best wins; a recent command gets a fixed bonus. About 150 actions, so scoring
per keystroke is negligible. *Alternative:* `nucleo` / `fuzzy-matcher` crates — rejected
for size; revisit if ranking complaints appear.

### D5. Keyboard ownership
While open, the palette takes the keyboard: stream keys (`Space`, `F3`) do not reach the
stream. It does not open while the window lock is armed (the lock filters input already).

## Risks / Trade-offs

- [New actions forget the registry] → a test walks the menu i18n keys and fails when one
  has no registry entry (allow-list for file and session paths).
- [Label drift before `remappable-shortcuts`] → the D1 test.
- [Long translated names crowd the row] → name elided; category and key right-aligned.

## Migration Plan

None: `palette_recent` defaults to empty.

## Open Questions

- Should the empty palette also offer recent files? Proposed: no in 0.13.0; a "go to
  file" mode later.
- `pattern-grouping` (0.14.0) first proposed CTRL + SHIFT + P for "Patterns…"; it now uses
  CTRL + SHIFT + G (unused), so the palette keeps CTRL + SHIFT + P.
- Should CTRL + P alone be an alias? Proposed: no, keep it for a future "go to file".
