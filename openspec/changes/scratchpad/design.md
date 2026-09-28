## Context

`FastTailTab` has `LogStream`, `Filters`, `Highlights`, `Settings` and `FindResults`.
Copy (CTRL + C) builds text from the selection with `copy_selection_text`, which already
expands collapsed rows. Sessions are `*.fasttail-session.ini` files; the default session
is embedded in `fasttail.ini`. INI values are single-line, escaped by `ini_value`.

## Goals / Non-Goals

**Goals:** collect lines from several streams plus free text in one place, keep it with
the session, get back to the source line in one click.

**Non-Goals:** a rich editor, colours, several pads.

## Decisions

### D1. Sidecar text file, not INI
Scratch content is multi-line and can reach megabytes; escaping it into one INI value
would make session files unreadable. The pad lives in `<session>.scratch.txt` beside the
session file (UTF-8, LF). "Save session as…" copies it; opening a session loads it.
*Alternative:* a `[scratchpad]` section with one key per line — rejected, fragile edits.

### D2. Reference lines are plain text
`── <file name>:<line> ──` is ordinary text the user can edit or delete. On CTRL + click
the line is parsed; the file name is resolved against open streams first, then against a
small map `name → full path` recorded at each send and kept in the session file as
`[scratchpad] path.N=app.log|D:\logs\app.log` (values through `ini_value`), so a
reference still works after restart without putting paths into the pad text. Unresolvable
references show "file not found". *Alternative:* hidden anchors
in a rich model — rejected, it breaks copy-paste into tickets.

### D3. Editor
`egui::TextEdit::multiline` is adequate up to a few MB; above that it becomes slow, hence
the 4 MB cap. Undo uses the widget's history. Find inside the pad is a simple search box
over the string.

### D4. Key
CTRL + SHIFT + N is unused in 0.12.0 (CTRL + SHIFT is used with `1..9`, `D`, `F`, `H`).
With `remappable-shortcuts` it becomes `scratchpad.send`.

## Risks / Trade-offs

- [Data loss on crash] → debounced save every second after a change; save on exit.
- [Large paste freezes the editor] → 4 MB cap checked before appending.
- [Relative references break when files move] → the paths map in D2; otherwise the text
  stays and only the jump fails.

## Migration Plan

None: no pad file means an empty pad.

## Open Questions

- Should "Send to scratchpad" include the line numbers on each line (`1204│ text`)?
  Proposed: no, only the reference line, so the text pastes cleanly.
- Should the default key be CTRL + SHIFT + N or CTRL + SHIFT + S? Proposed: N (S is
  commonly "save as").
