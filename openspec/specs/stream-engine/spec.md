# Stream Engine Specification

## Purpose
Defines the tail engine: memory-mapped access to very large files, real-time follow with pause, rotation and truncation handling, multi-encoding decoding, and the Text, Hex and Markdown view modes.

## Requirements

### Requirement: Memory-mapped streaming of large files
The tail engine SHALL open files using memory-mapped I/O (memmap2) with 64-bit offsets, enabling immediate viewing of files larger than 50 GB without loading the entire content into RAM.

#### Scenario: Opening a multi-gigabyte log file
- **WHEN** the user opens a 20 GB log file
- **THEN** the application opens the file within 100 milliseconds and displays the tail lines using less than 50 MB of resident memory.

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
