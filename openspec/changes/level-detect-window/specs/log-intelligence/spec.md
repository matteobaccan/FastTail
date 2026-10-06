## MODIFIED Requirements

### Requirement: Log Level Detection
The engine SHALL detect a level for each line among FATAL, ERROR, WARN, INFO, DEBUG, TRACE or unknown, by matching the first level token within the first N bytes as a whole word (optionally bracketed or followed by a colon, case-insensitive) or a syslog `<n>` severity; a word cut by the N-byte limit SHALL NOT match. N SHALL be the stream's override when set, otherwise the `level_detect_bytes` setting (default 96, range 16 to 4,096 bytes), editable in the Settings of both interfaces; the override SHALL be set from the stream's level menu (window) or level chip (terminal) with the same range and saved with the stream as `level_bytes` only when it differs from the setting. `fasttail --print` SHALL use the setting, or `--level-bytes N` when given. Changing N for a stream SHALL clear its level cache and counters and detect every line again, on a background job with progress for files above 16 MB, before the level filter, colours and counters use the new levels. Detection SHALL not allocate and SHALL be cached per line (1 byte per line).

#### Scenario: Common layouts
- **WHEN** the lines `2026-09-18 12:00:00 [ERROR] boom`, `WARN  main - slow` and `<3>kernel: oops` are appended
- **THEN** their levels are ERROR, WARN and ERROR.

#### Scenario: Level word in the message body only
- **WHEN** the line is `INFO user typed "error" in the search box`
- **THEN** the level is INFO.

#### Scenario: Level far into the line
- **WHEN** a stream's lines carry `ERROR` starting at byte 150 and the stream uses the default of 96 bytes
- **THEN** their level is unknown; after the user sets the stream's override to 256 bytes, they are ERROR, the ERROR counter counts them and `>= WARN` shows them.

#### Scenario: Override saved only when different
- **WHEN** `level_detect_bytes=96` and the user sets one stream to 256 and leaves another unchanged
- **THEN** `fasttail.ini` holds `level_bytes=256` for the first stream and no `level_bytes` for the second.

#### Scenario: Out of range
- **WHEN** the user types 8 in the Settings field
- **THEN** the field shows the range 16 to 4,096 and nothing is saved.
