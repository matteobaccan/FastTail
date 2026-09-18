## MODIFIED Requirements

### Requirement: Memory-mapped streaming of large files
The tail engine SHALL access files through an open read handle and a bounded block cache (at most 64 blocks of 64 KB per stream) and SHALL NOT hold a copy of the file in memory. Resident state per stream SHALL be limited to the line index (one 64-bit offset per line), the filtered-line and match lists, selection and bookmarks. Opening a file SHALL show its content immediately; the line index is built synchronously for files up to 256 MB and in the background above, with progress shown. Memory-mapping SHALL NOT be used, because a mapped file blocks the writer's rotation on Windows.

#### Scenario: Ten growing files
- **WHEN** ten 50 MB logs with 500-byte lines are open and all of them grow on every frame
- **THEN** the process holds under 60 MB for the ten streams, each frame's polling of all ten costs under 5 ms, and the views stay responsive.

#### Scenario: Opening a multi-gigabyte log file
- **WHEN** the user opens a 20 GB log file
- **THEN** the window shows the file within a second, indexing proceeds in the background with progress, and resident memory stays bounded by the index (8 bytes per line) plus the cache instead of the file size.

#### Scenario: Truncation returns memory
- **WHEN** a 500 MB stream is truncated to zero by the writer
- **THEN** the index, cache and derived lists are dropped and their memory returned to the allocator.

## ADDED Requirements

### Requirement: Background Scans with Progress and Cancellation
Include/exclude filtering and search SHALL run synchronously for files up to 16 MB and on a worker thread above, delivering results in order as they are found. The stream bar SHALL show the scan kind, the progress percentage and the count so far. A newer filter, search or reload request SHALL cancel the running scan, and results of a cancelled scan SHALL never reach the view. Lines appended during a scan SHALL be evaluated by the incremental paths once the scan completes.

#### Scenario: Typing a filter on a large file
- **WHEN** the user types an include filter on a 400 MB stream
- **THEN** the keystroke is not blocked, matching rows appear progressively with `filtering 37%` in the stream bar, and typing another character cancels the previous scan.

#### Scenario: Same result as the synchronous path
- **WHEN** the same filter is applied to a file below and above the threshold
- **THEN** the set of visible lines is identical.

### Requirement: Markdown Mode Size Cap
Rendered Markdown SHALL be available only for files up to 32 MB. Larger files SHALL open in text mode and selecting MD SHALL show a notice instead of reading the whole file.

#### Scenario: Large HTML report
- **WHEN** the user switches a 100 MB HTML file to MD view
- **THEN** the stream stays in text view and a notice explains the size limit.

### Requirement: Long Line Cap
A single line longer than 1 MB SHALL be displayed truncated to 1 MB with a visible marker, so that one pathological line cannot exhaust the cache or the layout.

#### Scenario: One-megabyte JSON line
- **WHEN** a log contains a 3 MB single-line JSON payload
- **THEN** the row shows the first 1 MB followed by a truncation marker and the rest of the file stays navigable.
