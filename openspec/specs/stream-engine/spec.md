# Stream Engine Specification

## Purpose
Defines the tail engine: on-demand streaming access to very large files without holding them in memory, real-time follow with pause, rotation and truncation handling, background scans with progress, multi-encoding decoding, and the Text, Hex and Markdown view modes.
## Requirements
### Requirement: Streaming access to large files
The tail engine SHALL access files through an open read handle and a bounded block cache (at most 64 blocks of 64 KB per stream) and SHALL NOT hold a copy of the file in memory. Resident state per stream SHALL be limited to the line index (one 64-bit offset per line), the level and timestamp caches once scanned (1 and 8 bytes per line), the per-4096-line error counts, the filtered-line list, the match list (at most 1,000,000 hits), selection and bookmarks. Opening a file SHALL show its content immediately; the line index is built synchronously for files up to 256 MB and in the background above, with progress shown. Memory-mapping SHALL NOT be used, because a mapped file blocks the writer's rotation on Windows.

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
The engine SHALL decode log files according to the selected encoding: ASCII, ANSI (Windows-1252), UTF-8 (with or without BOM), Unicode LE (UTF-16 Little Endian), and Unicode BE (UTF-16 Big Endian), with automatic BOM and binary heuristic detection upon opening, run again once when a stream opened with fewer than 512 bytes first holds 512 bytes (or when the decompression of a compressed stream ends).

#### Scenario: Log created empty and filled later
- **WHEN** a log is opened while empty and the writer then fills it with UTF-16 LE text
- **THEN** the encoding is detected again once the file holds 512 bytes, and the lines are shown as UTF-16 LE instead of UTF-8.

#### Scenario: Opening a UTF-16 log file
- **WHEN** a user opens a log file formatted in UTF-16 LE
- **THEN** the engine automatically detects the encoding and renders lines as valid UTF-8 strings in the viewport.

### Requirement: View Modes: Text, Hex, and Markdown
The engine SHALL support three view modes selectable per stream:
1. **TXT (Text Mode)**: Displays log lines as text with highlight rules and JSON toggles. Include/exclude filters apply automatically whenever their text is non-empty; there is no separate filtered mode.
2. **HEX (Binary Hex Mode)**: Displays file bytes in hexadecimal and ASCII dump columns. Columns SHALL be configurable in multiples of 8 (starting at 16, incrementing or decrementing by 8 columns). Files detected as binary open in this mode (gzip and zip files are decompressed instead, see Compressed Log Input).
3. **MD (Markdown Mode)**: Renders the file as formatted Markdown. Files with a `.md` / `.markdown` extension open in this mode; content that contains real HTML markup is converted to Markdown before rendering, and the converted text is cached until the file changes.

#### Scenario: Opening an HTML report in Markdown mode
- **WHEN** the user switches a stream holding an HTML document to MD view
- **THEN** headings, paragraphs, lists, links, tables and code blocks are rendered as Markdown, with `<pre>` contents kept verbatim.

#### Scenario: Markdown file containing generics in code
- **WHEN** a README with `Vec<i32>` inside a fenced code block is opened
- **THEN** the file is rendered as plain Markdown and the generic type is preserved, because code blocks and bare `<name>` tokens are not treated as HTML.

### Requirement: Background Scans with Progress and Cancellation
Include/exclude filtering and search SHALL run synchronously for files up to 16 MB and on a worker thread above, delivering results in order as they are found. The per-line timestamp scan (see the log-intelligence capability) SHALL follow the same rule, measured on the bytes not timed yet: synchronously when they are at most 16 MB, on a worker thread above. The stream bar SHALL show the scan kind, the progress percentage and the count so far. A newer filter, search or reload request SHALL cancel the running scan, and results of a cancelled scan SHALL never reach the view; the exceptions are a timestamp scan that a time window is waiting on, during which filter and search requests SHALL be deferred until it completes, and a level scan, which a timestamp request SHALL preempt. A cancelled timestamp or level scan SHALL resume from the first line it had not reached, not from the start of the file. Lines appended during a scan SHALL be evaluated by the incremental paths once the scan completes. A time window SHALL be applied to the lines returned by a background filter or search scan, so a window never forces a filter or search onto the interface thread.

#### Scenario: Typing a filter on a large file
- **WHEN** the user types an include filter on a 400 MB stream
- **THEN** the keystroke is not blocked, matching rows appear progressively with `filtering 37%` in the stream bar, and typing another character cancels the previous scan.

#### Scenario: Same result as the synchronous path
- **WHEN** the same filter is applied to a file below and above the threshold
- **THEN** the set of visible lines is identical.

#### Scenario: First time range on a multi-gigabyte log
- **WHEN** the user types a "from" time on a 5 GB stream that has not been timed yet
- **THEN** the interface stays responsive, the stream bar shows `timing lines 12%` rising to 100%, and the window is applied when the scan completes.

#### Scenario: Timestamp scan resumed after a search
- **WHEN** a timestamp scan started by a go-to-time request is 40% through a large stream and the user types a search query
- **THEN** the search runs first, and the timestamp scan then continues from the line it had reached rather than from line 1.

#### Scenario: Filter with a time window on a large file
- **WHEN** a time window is set on a fully timed 2 GB stream and the user types an include filter
- **THEN** the filter runs on the worker thread with progress in the stream bar, and only lines inside the window appear.

### Requirement: Markdown Mode Size Cap
Rendered Markdown SHALL be available only for files up to a size limit of 1 MB by default, adjustable from 1 to 100 MB in Settings (`markdown_max_mb` in `fasttail.ini`, overridden by the `FASTTAIL_MARKDOWN_MAX_MB` environment variable). A `.md` / `.markdown` file above 1 MB SHALL open in text mode; selecting MD on a file above the configured limit SHALL show a notice naming the limit instead of reading the whole file.

#### Scenario: Large HTML report
- **WHEN** the limit is the default 1 MB and the user switches a 5 MB HTML file to MD view
- **THEN** the stream stays in text view and a notice explains the size limit.

#### Scenario: Raising the limit
- **WHEN** the user sets the Markdown limit to 10 MB in Settings and switches the same 5 MB file to MD view
- **THEN** the document is rendered as Markdown.

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

### Requirement: Compressed Log Input
The engine SHALL open, read-only and whatever their extension, gzip files (magic bytes `1f 8b`, including multi-member files), bzip2 files (`BZh` and a digit `1`–`9` followed by the block magic `31 41 59 26 53 59` or the end-of-stream magic `17 72 45 38 50 90`, including concatenated streams), xz files (`FD 37 7A 58 5A 00`, including concatenated streams), zstd files (`28 B5 2F FD`, or a skippable frame `50`–`5F 2A 4D 18`, including several frames), zip archives (magic bytes `50 4b 03 04`) and tar archives (`ustar` at byte offset 257), plain or inside a gzip, bzip2, xz or zstd file, by decompressing them on a background thread into a temporary spool file that is then accessed like any other file, so that no decompressed copy is held in memory and every text, HEX and Markdown feature works on the result. The format SHALL be decided from at most the first 512 bytes of the file and, for a gzip, bzip2, xz or zstd file, at most the first 512 decompressed bytes. Content SHALL become visible while decompression runs, and the stream bar SHALL show the decompression progress with a cancel action. A zip with one file entry SHALL open that entry directly; a zip with several SHALL show an entry picker from which one or more entries are opened, each as its own stream; a tar archive SHALL show the entry picker described in Tar Archive Entry Picker; a zip without file entries SHALL be reported with a notice, and a file starting with the zip magic bytes whose central directory cannot be read SHALL open as a plain file. An archive entry (zip or tar) whose first bytes are gzip, bzip2, xz or zstd SHALL be decompressed once more into the spool; a zip or tar inside an entry SHALL NOT be unpacked. Zip entries that are encrypted or use a method other than stored or deflate, tar entries that are symbolic links, hard links, character or block devices, FIFOs or sparse files, and entries of either kind whose name is absolute, has a drive prefix or climbs out of the archive (`../`), SHALL be refused with a message naming the reason, and no file SHALL ever be created at a path taken from an archive. When several entries have the same stream path, the first in archive order SHALL open and the others SHALL be refused. A compressed stream SHALL NOT follow the archive on disk: follow mode is disabled for it with an explanation, and neither `--follow` nor a restored follow state turns it on; a re-extract button in the stream bar SHALL decompress the archive again. The stream's identity in the tab, the footer, the workspace, sessions, recent files and bookmarks SHALL be the archive path for a single compressed file and the entry path `<archive>/<entry>` for a zip or tar entry (a session stores it as the archive path plus `entry=`), never the spool path; the tab title of an entry SHALL be `<archive name> › <entry>`. A restored compressed stream SHALL be decompressed again in the background, its bookmarks being applied once the index covers them; a restored tar entry that the archive no longer holds SHALL stop with a message saying so.

#### Scenario: Rotated gzip log
- **WHEN** the user opens `app.log.1.gz`, a 300 MB gzip of a 3 GB text log
- **THEN** the first lines appear within a second, the stream bar shows `decompressing` with a rising percentage, filters and search work on the lines already available, and the follow toggle is disabled with a tooltip explaining that the file is a compressed snapshot.

#### Scenario: Support bundle with several logs
- **WHEN** the user opens `bundle.zip` holding `server.log`, `worker.log` and `config/`
- **THEN** an entry picker lists `server.log` and `worker.log` with their sizes, and choosing both opens two streams titled `bundle.zip › server.log` and `bundle.zip › worker.log`.

#### Scenario: Unsupported zip entry
- **WHEN** a zip entry is compressed with zstd or is encrypted
- **THEN** the entry is shown disabled in the picker with the reason, and nothing is written to the spool for it.

#### Scenario: Compressed file without the usual extension
- **WHEN** a gzip file named `trace.dat` is opened
- **THEN** it is recognised by its magic bytes and opened decompressed instead of in HEX view.

#### Scenario: xz and zstd rotated logs
- **WHEN** the user opens `syslog.2.xz` and `journal.zst`, the second made of 3 concatenated zstd frames
- **THEN** both open decompressed with progress, and the zstd stream contains the lines of all 3 frames in order.

#### Scenario: Text that looks like bzip2
- **WHEN** a text file starting with `BZh1 is a label` is opened
- **THEN** it opens as a plain text file, because the bytes after `BZh1` are not a bzip2 block magic.

#### Scenario: Nested gzip in a tarball
- **WHEN** the user opens the entry `logs/app.log.1.gz` of `bundle.tgz`
- **THEN** the stream titled `bundle.tgz › logs/app.log.1.gz` shows the decompressed text of that gzip, not its binary bytes.

#### Scenario: Tar entry session round trip
- **WHEN** a session saved with the stream `bundle.tar.gz › var/log/server.log` and 2 bookmarks is loaded
- **THEN** the session stores the archive path plus `entry=var/log/server.log`, the entry is extracted again in the background without showing the picker, and both bookmarks appear once the lines they point to are indexed.

#### Scenario: Unsafe and special tar entries
- **WHEN** a tar holds `../../etc/passwd`, `/abs.log`, a symbolic link `current.log` and a regular file `app.log`
- **THEN** only `app.log` can be opened, the other 3 entries are shown disabled with their reason, and no file other than the spool is created.

### Requirement: Decompression Space Guard
Before decompressing a zip entry, or a tar entry whose size the picker's scan already read, the engine SHALL check that the spool volume has room for the entry's uncompressed size (at most the output cap) plus a 512 MB margin and refuse otherwise. During any decompression it SHALL re-check the free space at least every 64 MB written and stop when less than 512 MB would remain, and it SHALL stop when the output reaches the cap `compressed_max_gb` (default 20 GB, 1 to 1024, configurable in `fasttail.ini` and Settings). An xz or zstd stream whose declared dictionary or window exceeds 256 MiB SHALL be refused before that memory is allocated. When decompression stops early, the lines already written SHALL stay readable and the stream SHALL say that its content is partial and why.

#### Scenario: Not enough disk space
- **WHEN** the user opens a zip entry of 40 GB uncompressed and the spool volume has 10 GB free
- **THEN** the entry is refused with a message naming the volume and the required size, and no spool file is created.

#### Scenario: Archive larger than the cap
- **WHEN** a gzip, bzip2, xz or zstd file inflates past the 20 GB cap
- **THEN** decompression stops at the cap, the first 20 GB of lines remain browsable, and the stream bar says the content is partial because the cap was reached.

#### Scenario: Oversized decoder window
- **WHEN** a zstd file declares a 2 GiB window (`--long=31`)
- **THEN** it is refused with a message naming the 256 MiB limit, and no spool file is written beyond the empty one.

### Requirement: Temporary Spool Lifecycle
Spool files SHALL be created in a `fasttail-spool` subfolder of the directory `spool_dir` from `fasttail.ini` when set, otherwise of the system temporary directory (readable by the owner only on Unix), named `<pid>-<counter>-<name>` after the owning process id. A spool file SHALL be deleted when its stream is closed and emptied and refilled when the stream is re-extracted, all spool files of the process SHALL be deleted at normal exit, and at startup the application SHALL delete the spool files in the current spool directory whose owning process is no longer running, so that a crash does not leave decompressed copies behind. Closing a stream while it is being decompressed SHALL cancel the decompression.

#### Scenario: Closing the tab
- **WHEN** the user closes the tab of a decompressed `app.log.1.gz`
- **THEN** the decompression, if still running, stops and the spool file is removed from disk.

#### Scenario: Restart after a crash
- **WHEN** FastTail crashed with two decompressed streams open and is started again
- **THEN** the two orphaned spool files are deleted at startup, and the restored streams are decompressed into new spool files.

### Requirement: Standard Input Stream
Standard input SHALL be copied by a background thread into a temporary spool file, flushed as it arrives, and the spool SHALL be tailed as a regular followed stream, so that every stream feature works on it and no copy of the input is held in memory. The stream SHALL be titled `stdin`, SHALL identify itself in the footer as standard input, and SHALL NOT be saved in the workspace, the recent files or session files. When the input ends, the stream SHALL stay open and say that the input ended. The spool SHALL be bounded by `stdin_spool_max_mb` (default 2048 MB, configurable in `fasttail.ini` and Settings) and by a 512 MB free-space margin on its volume: on reaching either limit the spool SHALL restart from empty, which the stream handles as a truncation, and the stream bar SHALL say that earlier input was discarded. The spool file SHALL be deleted when the stream is closed and at exit, and a spool left by a crashed process SHALL be deleted at the next start. On Windows this SHALL work with the GUI-subsystem executable, reading the standard-input handle passed by the shell.

#### Scenario: Following a container
- **WHEN** `docker logs -f api | fasttail -` runs and the container writes a line every second
- **THEN** each line appears in the `stdin` stream within 50 milliseconds of arriving, and search and highlight rules apply to it.

#### Scenario: Producer finishes
- **WHEN** `cat build.log | fasttail -` has copied the whole file and the pipe closes
- **THEN** the stream keeps all lines and its stream bar says the input ended.

#### Scenario: Spool limit reached
- **WHEN** a producer has written 2048 MB into the stdin stream with the default limit
- **THEN** the spool restarts from empty, new lines keep arriving, and the stream bar says earlier input was discarded because of the limit.

#### Scenario: Not restored after restart
- **WHEN** FastTail is closed with a `stdin` stream open and started again without piped input
- **THEN** no `stdin` stream is restored and no spool file remains on disk.

### Requirement: Tar Archive Entry Picker
Opening a tar archive, plain or inside a gzip, bzip2, xz or zstd file, SHALL show the entry picker at once while a background thread reads the entry headers, without writing any decompressed data to disk; the picker SHALL list each entry as it is found (name, size, and the reason when it cannot be opened), show the scan progress as a percentage of the archive bytes read with a stop action, and allow opening the listed entries before the scan ends. Directories SHALL NOT be listed. The scan SHALL stop after 100 000 entries or after 1024 GB of decompressed data, whichever comes first, and the picker SHALL then say that the list is partial; a damaged header SHALL end the scan with the entries found so far and a notice. When the scan ends with exactly one entry that can be opened and the user has neither opened nor checked an entry, that entry SHALL open directly and the picker SHALL close; when it ends with none, the picker SHALL say that the archive holds no file that can be opened. Closing the picker SHALL stop the scan. A finished scan SHALL be reused while the archive's size and modification time are unchanged, for the lifetime of the process. Each chosen entry SHALL be extracted by its own background job into its own spool, reading a compressed tar from the start and skipping the data of the entries before it.

#### Scenario: Large compressed support bundle
- **WHEN** the user opens `sosreport.tar.xz`, 800 MB compressed with 4 000 entries
- **THEN** the picker appears within 500 ms with `scanning` and a rising percentage, entries appear as they are found, and choosing `var/log/messages` while the scan runs opens a stream titled `sosreport.tar.xz › var/log/messages`.

#### Scenario: Plain tar lists by seeking
- **WHEN** the user opens `logs.tar`, 10 GB uncompressed with 1 000 entries
- **THEN** the list is complete after reading only the 1 000 headers (the entry data is skipped by seeking, not read), and an opened entry is copied into its spool with progress as a percentage of its size.

#### Scenario: Tarball with one log
- **WHEN** the user opens `app.tgz` holding the directory `logs/` and the single file `logs/app.log`
- **THEN** once the scan ends, `logs/app.log` opens directly as `app.tgz › logs/app.log` and the picker closes.

#### Scenario: Scan bound on an archive bomb
- **WHEN** a 10 MB `.tar.gz` would decompress to 2 TB
- **THEN** the scan stops after 1024 GB decompressed, the picker says the list is partial and keeps the entries found, and nothing of that data is written to disk.

