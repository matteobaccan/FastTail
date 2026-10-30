## ADDED Requirements

### Requirement: Terminal Merged Timeline
The terminal interface SHALL open a merged view of several streams interleaved by timestamp, with a source tag per row, and SHALL restore the merged views saved by the window.

#### Scenario: Merged at 14:02
- **WHEN** `app.log` and `db.log` are open and the user merges them
- **THEN** the rows alternate by time and each starts with `[app]` or `[db]`.
