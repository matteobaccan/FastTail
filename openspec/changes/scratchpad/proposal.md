## Why

While investigating, users collect the few lines that matter from several files, add a
sentence or two, and paste the result into a ticket. FastTail has bookmarks with notes,
but they stay attached to one file each and cannot be reordered, edited or mixed with
free text; today the user keeps a Notepad window open beside FastTail. klogg has a
scratchpad for exactly this, and lnav has comments; the post-0.12.0 competitor scan lists
it under klogg's strengths.

## What Changes

- A **Scratchpad** tab, one per session, opened from the title-bar menu or the command
  palette; it docks like the other tabs (Filters, Highlights, Find results).
- It is a **plain-text editor** (monospace, the stream font, undo / redo, find with
  CTRL + F inside it) that the user can type in freely.
- **Send to scratchpad**: the row context menu item "Send to scratchpad" and
  **CTRL + SHIFT + N** append the selected rows of the focused stream (every underlying
  line of collapsed rows, as copy does) to the end of the scratchpad, preceded by a
  reference line `── app.log:1204 ──` (file name and first line number). An option in the
  menu sends the lines without the reference line. The Find results tab offers the same
  item for its selected hits.
- **Jump back**: CTRL + click on a reference line (or ALT + Enter on it) focuses that stream
  and shows the line, as "Show in context" does; if the file is not open, it is opened.
- **Persistence**: the scratchpad is saved next to the session as a plain text file
  (`incident.fasttail-session.scratch.txt`; for the default workspace `scratchpad.txt` in
  the config directory), written at most once a second after a change and on exit. "Save
  as…" writes a copy anywhere; "Clear" asks for confirmation.
- Size cap: 4 MB; sending more is refused with a message.

Target release: **0.13.0** (basics and quick wins), per the release plan in
`docs/competitor-analysis.md` section 8. Priority: **Low–Medium**. Effort: **S**.

### Non-goals

- Rich text, Markdown rendering or colours from the source lines in the scratchpad (the
  `export-formats` HTML export keeps colours; `bookmark-report` writes a Markdown report).
- Several scratchpads per session, or one scratchpad shared between sessions.
- Treating the scratchpad as a stream (filters, search hits, highlight rules).
- Syncing edits back to bookmarks or notes.

## Capabilities

### New Capabilities

- `scratchpad`: the tab, sending lines, reference lines and jumping back, persistence.

### Modified Capabilities

None.

## Impact

- `src/ui/dock.rs`: `FastTailTab::Scratchpad` (saved in the dock layout), row menu item,
  CTRL + SHIFT + N in the stream key handler.
- `src/ui/scratchpad.rs` (new): `egui::TextEdit` multiline over a `String`, reference-line
  detection and jump, debounced save.
- `src/ui/find_results.rs`: "Send to scratchpad" for selected hits.
- `src/session.rs`: sidecar file path derived from the session path and the
  `[scratchpad]` paths map; `src/config.rs` / `src/paths.rs`: the same for the default
  workspace and `scratchpad.txt`.
- `src/i18n.rs` (16 languages), help dialog, README, CHANGELOG.
- With `command-palette`: "Open scratchpad" and "Send to scratchpad" actions.
- **TUI (0.20.0, PR #132)**: the TUI can append to the same sidecar file (`y` to
  scratchpad) and open it in `$EDITOR`; no in-terminal editor is planned.
