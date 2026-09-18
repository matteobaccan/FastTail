## ADDED Requirements

### Requirement: Pattern Streams Follow the Newest Matching File
A stream MAY be opened from a directory path plus a file-name pattern with `*` and `?` wildcards. The engine SHALL resolve the newest matching file by modification time (name as tie-break), rescan the directory every 2 seconds, and switch to a newer match automatically, keeping filters, highlight rules, search query, wrap and encoding while resetting the buffer, bookmarks and selection. The pattern SHALL be what is persisted in the workspace and in the recent files list.

#### Scenario: Daily rotation
- **WHEN** the stream was opened as `logs/app-*.log` while `app-2026-09-18.log` was newest and the writer creates `app-2026-09-19.log`
- **THEN** within 2 seconds the stream tails `app-2026-09-19.log`, the include filter still applies, and the status bar reports the switch.

#### Scenario: No match yet
- **WHEN** the pattern matches no file at open time
- **THEN** the stream opens empty with a "waiting for a matching file" notice and starts tailing the first file that appears.
