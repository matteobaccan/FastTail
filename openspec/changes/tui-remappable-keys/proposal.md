## Why

The window's shortcuts are remappable (`[shortcuts]` in `fasttail.ini`) but the terminal
keys are fixed, and the terminal's palette lacks the window's setting toggles. Users of
vim-like or Emacs-like terminals asked other tools for both (klogg, lnav).

## What Changes

- The terminal reads `[shortcuts]` for the actions it has, with terminal key names
  (`ctrl+o`, `alt+/`, `g g`); a **Keys** dialog lists the bindings, records a new key,
  warns on a conflict and resets one or all.
- The terminal palette gets the window's **setting toggles and choices** that apply to a
  terminal (line numbers, level colours, auto highlighting, theme, language...).

Target release: the next minor after 0.28.1 (terminal parity with the window, proposed on 2026-10-29 at the maintainer's request; planned when the maintainer agrees), per `docs/competitor-analysis.md` sections 7 and 8. Priority: **Low–Med**. Effort: **M**.

### Non-goals

- Mouse button remapping.
- Chords longer than two keys.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `terminal-interface`: the requirements below are added.

## Impact

- `src/tui/keys.rs` a keymap over the registry ids, `src/keymap.rs` terminal key names;
  a Keys dialog; `src/tui/palette.rs` toggles; texts, `docs/tui.md`, README, CHANGELOG.
