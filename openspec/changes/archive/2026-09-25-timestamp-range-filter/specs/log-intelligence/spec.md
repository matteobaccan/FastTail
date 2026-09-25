## ADDED Requirements

### Requirement: Timestamp Detection
The engine SHALL detect a leading timestamp per line among ISO 8601 (with `T` or space, optional fraction and zone), syslog (`Mon DD HH:MM:SS`, the current year assumed), Apache/nginx (`[DD/Mon/YYYY:HH:MM:SS zone]`) and epoch seconds (10 digits) or milliseconds (13 digits), within the first 64 bytes, without allocation, trying the format that matched last first, and SHALL cache the result per line. Timestamps SHALL be compared on the clock the log printed: a zone suffix (`Z`, `+02:00`, `+0200`) is read past and not applied, and epoch values are read as UTC. Lines without a timestamp SHALL inherit the previous line's timestamp, so a stack trace stays with its entry. The cache SHALL be reset from the first changed line when the file is truncated or rewritten. The cache is built when the time range or the time jump is first used; that first scan currently runs synchronously on the interface thread.

#### Scenario: Mixed entry with continuation lines
- **WHEN** the lines `2026-09-18T14:02:05.123Z ERROR boom`, `    at Foo.bar(Foo.java:10)` are appended
- **THEN** both lines carry the timestamp 14:02:05.123 of 2026-09-18.

#### Scenario: Zone suffix is not applied
- **WHEN** a line starts with `[18/Sep/2026:14:02:05 +0200]`
- **THEN** its timestamp is 2026-09-18 14:02:05, the clock the log printed, and a range from `14:02` includes it.
