# Stream Engine Specification

## Purpose
Defines the tail engine: on-demand streaming access to very large files without holding them in memory, real-time follow with pause, rotation and truncation handling, background scans with progress, multi-encoding decoding, and the Text, Hex and Markdown view modes.
## Requirements
### Requirement: Streaming access to large files
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

### Requirement: Real-time file follow with pause and resume
The engine SHALL monitor file modifications in real-time (`tail -f`) and append newly written lines to the view automatically while follow mode is active.

#### Scenario: New log lines appended to file
- **WHEN** an external process writes new lines to a monitored log file
- **THEN** the engine reads the new bytes and dispatches them to the UI viewport within 50 milliseconds.

#### Scenario: User pauses follow
- **WHEN** the user toggles follow off or scrolls up into the buffer
- **THEN** the viewport remains fixed on the current lines while background streaming continues without moving the scroll position.

#### Scenario: Continuous appends on a large file
- **WHEN** a 100 MB log receives new lines on every frame
- **THEN** each poll costs time proportional to the appended bytes, not to the file size: the line index keeps the offsets of unchanged lines and rescans only from the previously last (possibly partial) line, so the interface stays responsive.

#### Scenario: File reset and regrown with the same header
- **WHEN** the writer truncates the file and refills it past its previous size with lines that start with the same 64 bytes as before
- **THEN** the engine detects the rewrite by comparing the bytes where the old data ended, reloads the file from the start, and never shows old and new content spliced together.

### Requirement: Log rotation and truncation handling
The engine SHALL detect when a monitored file is truncated or rotated (e.g., via logrotate) and reopen or reset the byte stream automatically without crashing or losing data.

#### Scenario: File size decreases due to truncation
- **WHEN** a log file is truncated to a smaller size
- **THEN** the engine resets its read offset to zero and streams subsequent lines from the beginning of the truncated file.

#### Scenario: File rotated to new inode or renamed
- **WHEN** a monitored log file is renamed and replaced with a new empty file of the same name
- **THEN** the engine switches tracking to the new file seamlessly and notifies the user via the status bar.

### Requirement: Multi-Encoding Support
The engine SHALL decode log files according to the selected encoding: ASCII, ANSI (Windows-1252), UTF-8 (with or without BOM), Unicode LE (UTF-16 Little Endian), and Unicode BE (UTF-16 Big Endian), with automatic BOM and binary heuristic detection upon opening.

#### Scenario: Opening a UTF-16 log file
- **WHEN** a user opens a log file formatted in UTF-16 LE
- **THEN** the engine automatically detects the encoding and renders lines as valid UTF-8 strings in the viewport.

### Requirement: View Modes: Text, Hex, and Markdown
The engine SHALL support three view modes selectable per stream:
1. **TXT (Text Mode)**: Displays log lines as text with highlight rules and JSON toggles. Include/exclude filters apply automatically whenever their text is non-empty; there is no separate filtered mode.
2. **HEX (Binary Hex Mode)**: Displays file bytes in hexadecimal and ASCII dump columns. Columns SHALL be configurable in multiples of 8 (starting at 16, incrementing or decrementing by 8 columns). Files detected as binary open in this mode.
3. **MD (Markdown Mode)**: Renders the file as formatted Markdown. Files with a `.md` / `.markdown` extension open in this mode; content that contains real HTML markup is converted to Markdown before rendering, and the converted text is cached until the file changes.

#### Scenario: Opening an HTML report in Markdown mode
- **WHEN** the user switches a stream holding an HTML document to MD view
- **THEN** headings, paragraphs, lists, links, tables and code blocks are rendered as Markdown, with `<pre>` contents kept verbatim.

#### Scenario: Markdown file containing generics in code
- **WHEN** a README with `Vec<i32>` inside a fenced code block is opened
- **THEN** the file is rendered as plain Markdown and the generic type is preserved, because code blocks and bare `<name>` tokens are not treated as HTML.

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

### Requirement: Pattern Streams Follow the Newest Matching File
A stream MAY be opened from a directory path plus a file-name pattern with `*` and `?` wildcards. The engine SHALL resolve the newest matching file by modification time (name as tie-break), rescan the directory every 2 seconds, and switch to a newer match automatically, keeping filters, highlight rules, search query, wrap and encoding while resetting the buffer, bookmarks and selection. The pattern SHALL be what is persisted in the workspace and in the recent files list.

#### Scenario: Daily rotation
- **WHEN** the stream was opened as `logs/app-*.log` while `app-2026-09-18.log` was newest and the writer creates `app-2026-09-19.log`
- **THEN** within 2 seconds the stream tails `app-2026-09-19.log`, the include filter still applies, and the status bar reports the switch.

#### Scenario: No match yet
- **WHEN** the pattern matches no file at open time
- **THEN** the stream opens empty with a "waiting for a matching file" notice and starts tailing the first file that appears.

