## ADDED Requirements

### Requirement: Memory-mapped streaming of large files
The tail engine SHALL open files using memory-mapped I/O (`memmap2`) with 64-bit offsets, enabling immediate viewing of files larger than 50 GB without loading the entire content into RAM.

#### Scenario: Opening a multi-gigabyte log file
- **WHEN** the user opens a 20 GB log file
- **THEN** the application opens the file within 100 milliseconds and displays the tail lines using less than 50 MB of resident memory.

### Requirement: Real-time file follow with pause and resume
The engine SHALL monitor file modifications in real-time (`tail -f`) and append newly written lines to the view automatically while follow-tail mode is active.

#### Scenario: New log lines appended to file
- **WHEN** an external process writes new lines to a monitored log file
- **THEN** the engine reads the new bytes and dispatches them to the UI viewport within 50 milliseconds.

#### Scenario: User pauses follow-tail
- **WHEN** the user toggles follow-tail off or scrolls up into the buffer
- **THEN** the viewport remains fixed on the current lines while background streaming continues without moving the scroll position.

### Requirement: Log rotation and truncation handling
The engine SHALL detect when a monitored file is truncated or rotated (e.g., via `logrotate`) and reopen or reset the byte stream automatically without crashing or losing data.

#### Scenario: File size decreases due to truncation
- **WHEN** a log file is truncated to a smaller size
- **THEN** the engine resets its read offset to zero and streams subsequent lines from the beginning of the truncated file.

#### Scenario: File rotated to new inode or renamed
- **WHEN** a monitored log file is renamed and replaced with a new empty file of the same name
- **THEN** the engine switches tracking to the new file seamlessly and notifies the user via the status bar.
