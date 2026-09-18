## ADDED Requirements

### Requirement: Timestamp Detection
The engine SHALL detect a leading timestamp per line among ISO 8601 (with `T` or space, optional fraction and zone), syslog (`Mon DD HH:MM:SS`), Apache/nginx (`[DD/Mon/YYYY:HH:MM:SS zone]`) and epoch seconds or milliseconds, within the first 64 bytes, without allocation, cached per line. Lines without a timestamp SHALL inherit the previous line's timestamp.

#### Scenario: Mixed entry with continuation lines
- **WHEN** the lines `2026-09-18T14:02:05.123Z ERROR boom`, `    at Foo.bar(Foo.java:10)` are appended
- **THEN** both lines carry the timestamp 14:02:05.123 of 2026-09-18.

### Requirement: Go To Time
The go-to popup SHALL accept a time in the same formats and SHALL scroll to the first line whose timestamp is at or after it, using a binary search when timestamps are non-decreasing and a linear scan otherwise.

#### Scenario: Jump inside an ordered log
- **WHEN** the user enters `14:03:30` in the go-to popup
- **THEN** the viewport centres on the first line at or after 14:03:30.
