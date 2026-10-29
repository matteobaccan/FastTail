## Context

`src/keymap.rs` maps registry actions to egui keys; `src/tui/keys.rs::map_key` matches
crossterm events in code. `src/tui/palette.rs::terminal_action` maps registry ids to
terminal actions.

## Goals / Non-Goals

**Goals:** the window's feature in the terminal, over the same engine and files.

**Non-Goals:** Mouse button remapping. Chords longer than two keys.

## Decisions

### D1. One `[shortcuts]` section
Terminal bindings live beside the window's under a `tui.` prefix
(`tui.stream.follow.toggle=space`), so each interface keeps its own keys.

### D2. Defaults unchanged
Without an override the terminal keys stay as documented in `docs/tui.md`.

## Risks / Trade-offs

- [Terminals that do not report a key] → the Keys dialog says when a recorded key cannot be told apart (Ctrl+Shift letters without the Kitty protocol).

## Open Questions

- Share bindings between the window and the terminal when the key exists in both? Proposed: no, separate prefixes.
