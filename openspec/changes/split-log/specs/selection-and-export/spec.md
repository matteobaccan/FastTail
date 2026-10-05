## ADDED Requirements

### Requirement: Split a Log into Files
The user SHALL be able to split a stream's file into several files from the stream menu and the command palette, in both interfaces: every N lines, every N megabytes (each piece ending at a line end; a longer line becomes its own piece), or by time (every hour, every day or a chosen span, from the line timestamps, lines without a timestamp staying with the line before them). A dialog SHALL preview the number, names and sizes of the pieces and the folder they go to before writing. The pieces SHALL be named `<name>.part-NNN<extension>` by lines and by size, and after the start of their time span by time. An option SHALL limit the split to the visible lines, as Export visible lines does. Writing SHALL run in the background with progress and Cancel, read the file through the block cache without holding it in memory, check the free space first, and never overwrite an existing file without confirmation.

#### Scenario: By lines
- **WHEN** the user splits a 1,000,000-line file every 250,000 lines
- **THEN** four files `app.part-001.log` to `app.part-004.log` are written, each with 250,000 lines.

#### Scenario: By size at line ends
- **WHEN** the user splits a 2 GB file every 500 MB
- **THEN** each piece is at most 500 MB and ends at the end of a line.

#### Scenario: By hour
- **WHEN** the user splits a log covering 14:00 to 16:30 by hour
- **THEN** three files are written, starting with the lines of 14:00, 15:00 and 16:00.
