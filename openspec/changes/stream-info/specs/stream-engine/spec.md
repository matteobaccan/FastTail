## ADDED Requirements

### Requirement: Stream Information
Both interfaces SHALL show a stream's path, size, lines, indexing time, index and cache memory, encoding, parser or format, timestamp format and source kind.

#### Scenario: A big file
- **WHEN** a 2 GB file was indexed
- **THEN** the information says how many lines, how long the index took and how much memory it uses.
