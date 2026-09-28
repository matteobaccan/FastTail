## ADDED Requirements

### Requirement: Scratchpad Tab
FastTail SHALL offer one Scratchpad tab per session, opened from the title-bar menu, that docks like the other tabs and holds a plain-text editor with undo, redo and a find box. Its content SHALL be limited to 4 MB; an edit or a send that would exceed it SHALL be refused with a message. "Save as…" SHALL write the content to a chosen file and "Clear" SHALL empty it after confirmation.

#### Scenario: Free notes
- **WHEN** the user opens the Scratchpad and types a sentence
- **THEN** the text stays in the tab while the user switches between streams.

### Requirement: Send Lines to the Scratchpad
The row context menu item "Send to scratchpad" and CTRL + SHIFT + N in the focused stream SHALL append the selected rows to the end of the scratchpad in file order, every underlying line of a collapsed row included, preceded by a reference line `── <file name>:<first line number> ──`. A variant of the menu item SHALL append the lines without the reference line. The Find results tab SHALL offer the same item for its selected hits, with one reference line per stream.

#### Scenario: Two streams into one pad
- **WHEN** the user sends line 1204 of `app.log` and then lines 88–90 of `db.log`
- **THEN** the scratchpad ends with `── app.log:1204 ──`, that line, `── db.log:88 ──` and the three lines.

### Requirement: Jump from a Reference Line
CTRL + click on a reference line, or ALT + Enter with the cursor on it, SHALL focus the referenced stream and show the referenced line as "Show in context" does, opening the file first when it is not open and its path is known. When the file cannot be found, a message SHALL say so and the scratchpad SHALL be unchanged.

#### Scenario: Back to the source
- **WHEN** the user CTRL + clicks `── app.log:1204 ──`
- **THEN** the `app.log` stream is focused with line 1204 selected and in view.

### Requirement: Scratchpad Persistence
The scratchpad SHALL be saved as a UTF-8 text file next to the session file, named after it with the suffix `.scratch.txt`, or as `scratchpad.txt` in the configuration directory for the default workspace. It SHALL be written within one second after a change and on exit, and loaded when the session is opened; a missing file SHALL mean an empty scratchpad. "Save session as…" SHALL copy the scratchpad to the new session.

#### Scenario: Survives a restart
- **WHEN** the user writes notes in the scratchpad of session `incident`, closes FastTail and reopens the session
- **THEN** the notes are back and `incident.fasttail-session.scratch.txt` exists beside the session file.
