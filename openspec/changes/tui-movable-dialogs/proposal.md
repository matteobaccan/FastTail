## Why

In the terminal interface the Settings dialog (`,`) and the Keys dialog (`?` / `F1`) open
centred at a fixed size and cannot be moved or resized: the Settings dialog covers the
stream the user is tuning (theme, colours, time delta apply live, but the lines they change
are hidden under it), and the Keys dialog hides the windows it describes. Both lack the
`[x]` close button that the terminal's floating windows already have in their top border,
so a mouse user must find `[ OK ]`, `[ Cancel ]` or `Esc`. The floating dock windows
already move by their title and resize by their borders; these two dialogs should behave
the same way.

## What Changes

- The Settings and Keys dialogs move when their title row is dragged and resize when a
  border or corner is dragged, with the same hit zones and clamping as the floating dock
  windows; a double click on the title row brings back the centred default size.
- Both dialogs show `[x]` right-aligned in their top border; a click on it closes the
  dialog as `Esc` does (Settings brings back the values it opened with).
- Their content follows the new size: Settings scrolls its fields in the rows it has, the
  Keys dialog lays out its columns (1 to 3) for the dialog's width instead of the screen's.
- The position and size are remembered while the interface runs (reopening a dialog finds
  it where it was left) and kept inside the screen when the terminal shrinks. Nothing is
  saved to `fasttail.ini`: no new key.
- A drag that starts on the dialog and ends outside it does not close the dialog.

Target release: the next **nightly patch (0.20.x)**, requested by the maintainer on
2026-10-06. Priority: **high** (usability of the terminal interface). Effort: **S (days)**.

### Non-goals

- Moving or resizing the other dialogs (search, prompts, open file, rule editor, tools,
  lock screen); the lock dialog in particular stays full screen and fixed.
- Keyboard move / resize of dialogs.
- Saving dialog geometry in `fasttail.ini` or in session files.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `terminal-interface`: a new requirement for the movable, resizable Settings and Keys
  dialogs with a `[x]` close button, and Terminal Mouse (a dialog drag released outside it
  does not close it).

## Impact

- `src/tui/app.rs`: `dialog()` takes an optional stored geometry and draws `[x]`; the
  Settings and Keys draw functions size their content from the dialog's inner area; new
  drag states for a dialog move / resize; the Keys layout computed from the dialog width.
- `src/tui/mouse.rs`: `DialogHit` gains the title row, the `[x]` cell and the border edges
  (reusing `edges_at`); `src/tui/dock.rs` `resized` / `clamp_into` reused for the clamp.
- `src/i18n_tui.rs` (no new text expected: `[x]` is already drawn by floating windows),
  `docs/tui.md`, `docs/ui-design.md`, CHANGELOG, tests.
