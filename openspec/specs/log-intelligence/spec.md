# Log Intelligence Specification

## Purpose
Adds structure awareness to raw log lines: inline JSON detection with expandable pretty-printing, grouping of multiline stack traces with their parent entry, log level detection with colouring, a minimum-level filter and counters, and leading-timestamp detection used by the time range filter and go-to-time.
## Requirements
### Requirement: Inline JSON auto-detection and expansion
The application SHALL treat a line as a JSON payload when, trimmed, it starts with `{` and ends with `}` or starts with `[` and ends with `]`, and SHALL render an inline toggle (`[+] JSON`, `[-] JSON` once expanded) that expands the payload into an indented, pretty-printed block below the row. A line that only carries JSON after other text (a timestamp, a level) is not detected.

#### Scenario: Line containing valid JSON payload
- **WHEN** a log line is a JSON object or array
- **THEN** the viewer displays a `[+] JSON` toggle next to the line, leaving the collapsed line unchanged by default.

#### Scenario: User clicks expand JSON
- **WHEN** the user clicks the inline `[+] JSON` toggle
- **THEN** the line expands inline into indented, pretty-printed key-value pairs without breaking the overall scroll position, and the toggle reads `[-] JSON` until it is clicked again.

### Requirement: Multiline stack trace grouping
The engine SHALL recognize multiline exceptions and stack traces (such as Java, .NET, Python, and Go tracebacks) and associate contiguous trace lines with their parent log entry.

#### Scenario: Filtering logs containing stack traces
- **WHEN** the user applies an include filter matching an error message that has a subsequent multiline stack trace
- **THEN** the entire stack trace is preserved and displayed alongside the matched error header rather than being severed by line-by-line filters.

### Requirement: Log Level Detection
The engine SHALL detect a level for each line among FATAL, ERROR, WARN, INFO, DEBUG, TRACE or unknown, by matching the first level token within the first 96 bytes as a whole word (optionally bracketed or followed by a colon, case-insensitive) or a syslog `<n>` severity. Detection SHALL not allocate and SHALL be cached per line.

#### Scenario: Common layouts
- **WHEN** the lines `2026-09-18 12:00:00 [ERROR] boom`, `WARN  main - slow` and `<3>kernel: oops` are appended
- **THEN** their levels are ERROR, WARN and ERROR.

#### Scenario: Level word in the message body only
- **WHEN** the line is `INFO user typed "error" in the search box`
- **THEN** the level is INFO.

### Requirement: Level Colouring Fallback
Rows with a detected level SHALL be coloured with the theme's level palette when no whole-row user highlight rule matches them, on the bytes that no capture-only rule, quick label or ANSI colour span colours. The feature SHALL be on by default and switchable in Settings.

#### Scenario: User rule keeps priority
- **WHEN** a user rule colours lines containing `payment` green and an ERROR line contains `payment`
- **THEN** the row is green, not the ERROR colour.

### Requirement: Minimum Level Filter and Counters
The stream bar SHALL offer a minimum-level selector combined with the include/exclude filters, with a toggle for lines of unknown level, and the stream status bar SHALL show per-level counts updated live.

#### Scenario: Filtering to warnings and above
- **WHEN** the user selects `>= WARN` on a stream with 100 INFO, 5 WARN and 2 ERROR lines
- **THEN** 7 rows are visible and the counters read WARN 5, ERROR 2.

### Requirement: Timestamp Detection
The engine SHALL detect a leading timestamp per line among ISO 8601 (with `T` or space, optional fraction and zone), syslog (`Mon DD HH:MM:SS`, the current year assumed), Apache/nginx (`[DD/Mon/YYYY:HH:MM:SS zone]`) and epoch seconds (10 digits) or milliseconds (13 digits), within the first 64 bytes, without allocation, trying the format that matched last first, and SHALL cache the result per line. Timestamps SHALL be compared on the clock the log printed: a zone suffix (`Z`, `+02:00`, `+0200`) is read past and not applied, and epoch values are read as UTC. Lines without a timestamp SHALL inherit the previous line's timestamp, so a stack trace stays with its entry. The cache SHALL be reset from the first changed line when the file is truncated or rewritten. The cache is built when the time range or the time jump is first used, as a background scan with progress (see the stream-engine capability) when more than 16 MB remain to be timed; the result SHALL be identical to timing the same lines synchronously. Lines appended afterwards SHALL be timed incrementally.

#### Scenario: Mixed entry with continuation lines
- **WHEN** the lines `2026-09-18T14:02:05.123Z ERROR boom`, `    at Foo.bar(Foo.java:10)` are appended
- **THEN** both lines carry the timestamp 14:02:05.123 of 2026-09-18.

#### Scenario: Zone suffix is not applied
- **WHEN** a line starts with `[18/Sep/2026:14:02:05 +0200]`
- **THEN** its timestamp is 2026-09-18 14:02:05, the clock the log printed, and a range from `14:02` includes it.

#### Scenario: Background and synchronous timing agree
- **WHEN** the same log is timed once below the 16 MB threshold and once, padded above it, by the background scan
- **THEN** the cached timestamps of the common lines, the share of lines with a timestamp of their own and the out-of-order flag are identical.

### Requirement: Time Delta Column
The text view SHALL offer an optional time delta column, available on streams with usable timestamps. The column SHALL be switched per stream from that stream's toolbar (`Δt`), and switching it SHALL NOT change any other stream. A newly opened stream SHALL start from the `show_time_delta` default in `fasttail.ini`, which Settings SHALL present as the default for new streams; changing it SHALL NOT change the streams already open. The switch SHALL be saved with the stream in the workspace and in session files as `time_delta=true` or `time_delta=false`, written only when it differs from the default, and a file without the key SHALL take the default. For each visible row whose line carries a timestamp of its own, the column SHALL show the difference between that line's timestamp and the effective timestamp of the previous visible row, so that it follows the active filters; continuation lines, the first visible row and rows without a known time SHALL show no value. Values SHALL be signed with millisecond precision, formatted as seconds below one minute, `M:SS.mmm` below one hour, `H:MM:SS` below one day and days plus hours and minutes above, and values at or above a gap threshold (default 1 s), one preference shared by every stream, SHALL be drawn in the accent colour. Timestamps SHALL be those of the timestamp cache, compared on the clock the log printed. Enabling the column SHALL NOT pause the interface: the cache is filled progressively and rows not timed yet show a pending mark, and only the streams that show the column SHALL be timed for it. On a stream without usable timestamps the column SHALL be hidden and the toolbar SHALL say why.

#### Scenario: Spotting a stall
- **WHEN** the column is on and three consecutive entries are stamped 14:02:05.100, 14:02:05.225 and 14:02:07.580
- **THEN** the second row shows `+0.125` and the third `+2.355` in the accent colour.

#### Scenario: Delta follows the filter
- **WHEN** the include filter `payment` shows the entries stamped 14:02:05.100 and 14:02:09.100 and hides the entries between them
- **THEN** the second visible row shows `+4.000`.

#### Scenario: Stack trace lines stay blank
- **WHEN** an ERROR entry is followed by five `at …` continuation lines
- **THEN** only the ERROR row shows a delta and the continuation rows show none.

#### Scenario: Large file does not freeze
- **WHEN** the column is turned on for a 20 GB log that has not been timed yet and the user jumps to its end
- **THEN** the interface stays responsive and the rows show a pending mark until their timestamps are known.

#### Scenario: One stream only
- **WHEN** two streams are open side by side and the user clicks `Δt` in the first one's toolbar
- **THEN** the first stream shows the column, the second does not and is not timed for it.

#### Scenario: Saved with the stream
- **WHEN** the default is off, the user turns `Δt` on in one of two streams, saves a session and loads it later
- **THEN** that stream shows the column, its section holds `time_delta=true`, and the other stream's section has no `time_delta` key.

#### Scenario: Old files follow the default
- **WHEN** a workspace or session file written before this change is opened with `show_time_delta=true` in `fasttail.ini`
- **THEN** every stream it opens shows the column.

### Requirement: Time Anchor
The row context menu SHALL offer to set the row as the stream's time anchor and to clear it. While an anchor is set, the time delta column SHALL show, for every row with a timestamp of its own, the signed difference between its timestamp and the anchor's, the anchor row SHALL be marked, and the gap tint SHALL not apply. The anchor SHALL survive filter changes, SHALL be cleared when the file is truncated or rewritten, and SHALL NOT be persisted.

#### Scenario: Measuring from a request start
- **WHEN** the user sets the anchor on a line stamped 14:02:05.100
- **THEN** a later line stamped 14:02:06.350 shows `+1.250` and an earlier line stamped 14:02:04.600 shows `-0.500`.

### Requirement: Selection Elapsed Time
When two or more rows are selected in a stream with usable timestamps, the stream status bar SHALL show the time from the first to the last selected row in file order, using their effective timestamps, together with the number of selected rows; with every visible row selected it SHALL use the first and last visible rows. The value SHALL be omitted when either end has no known timestamp.

#### Scenario: Duration of a request
- **WHEN** the user clicks the row stamped 14:02:05.100 and Shift+clicks the row stamped 14:02:07.457, selecting 14 rows
- **THEN** the status bar shows `Δ +2.357` and `14 rows`.

### Requirement: Timeline Histogram
A stream SHALL offer a timeline histogram, toggled from the time range controls, that shows the number of lines over the stream's time span as bars stacked by detected level (ERROR and FATAL, WARN, INFO, DEBUG and TRACE, unknown) in the theme's level colours, counting every timed line of the stream regardless of the active filters. Opening the histogram SHALL build the timestamp cache if needed, in the background for large files, and the bars SHALL fill in as lines are timed. The histogram SHALL be maintained incrementally from the timestamp and level caches, with at most 2,048 buckets whose width starts at one second and doubles as the span grows, and SHALL stay consistent when the file grows, is truncated or is rewritten. Clicking a bar SHALL set the time range filter to that bar's span and dragging across bars SHALL set it to the dragged span, writing the bounds into the from/to fields so the window behaves exactly like a typed one; the current window SHALL be shaded on the histogram. Hovering a bar SHALL show its time span and its counts per level. An optional lane SHALL mark where the lines matching the current search fall. The histogram SHALL be unavailable, with the same hint as the time fields, when the stream has no usable timestamps. The histogram SHALL be opened and closed per stream, the toggle of one stream leaving the others unchanged and untimed; its shown state SHALL be saved with the stream in the workspace and in sessions, and the search lane option SHALL be a global preference persisted in `fasttail.ini`.

#### Scenario: Spotting an error burst
- **WHEN** a day-long log has a steady volume of INFO lines and 180 ERROR lines between 14:02 and 14:04, and the user opens the histogram
- **THEN** the bars covering 14:02 to 14:04 carry a visible ERROR segment in the error colour, and hovering one of them shows its span and its ERROR count.

#### Scenario: Selecting a window by dragging
- **WHEN** the user drags across the bars from the one starting at 14:02:00 to the one ending at 14:04:59
- **THEN** the from field reads the 14:02:00 bound, the to field reads the 14:04:59 bound, only lines stamped from 14:02:00.000 to 14:04:59.999 are visible, and the selected span is shaded on the histogram.

#### Scenario: Histogram on a large untimed log
- **WHEN** the user opens the histogram on a 6 GB stream that has never been timed
- **THEN** the interface stays responsive, the stream bar shows the timing progress, and the bars grow as the lines are timed.

#### Scenario: One stream's histogram
- **WHEN** three streams are open and the user presses `📊` on one of them
- **THEN** only that stream shows the histogram and is timed for it, and after a restart it is still shown on that stream alone.

#### Scenario: Filters do not hide the histogram's data
- **WHEN** an include filter `payment` is active and the user opens the histogram
- **THEN** the bars count every timed line of the stream, not only the lines containing `payment`.

#### Scenario: Log rewritten
- **WHEN** the writer truncates the log to zero and writes new lines
- **THEN** the histogram is emptied and then shows only the new lines.

### Requirement: Collapse Repeated Lines
The text view of a stream SHALL offer a collapse mode among Off (default), Exact and Numbers, chosen from the stream toolbar and cycled with `CTRL + SHIFT + D` while the stream has the keyboard and no text field does. An entry SHALL be a visible line that is not a stack-trace continuation line together with the visible continuation lines that follow it; a visible continuation line without a visible parent SHALL be an entry of its own. Each line SHALL be normalised by removing its leading timestamp, when the stream's timestamp detection finds one at the start of the line, and its trailing whitespace; in Numbers mode each maximal UUID (8-4-4-4-12 hexadecimal digits), `0x`-prefixed hexadecimal number, hexadecimal word of 8 or more characters containing a digit, and run of decimal digits SHALL further be replaced by one placeholder. Two consecutive entries SHALL be equal when they have the same number of lines and their normalised lines are byte-for-byte equal in order. Each run of 2 or more consecutive equal entries SHALL be shown as a group: the first entry's lines, with a `×N` badge on its first row, where N is the number of entries in the run (thousands grouped, `×1.2M` style above 999,999). Entries longer than 256 lines or 64 KiB of normalised text SHALL never be grouped. Runs SHALL be formed over the lines left visible by every active filter and SHALL be recomputed when the filters change. Clicking the badge SHALL expand the group into all its lines and clicking it again SHALL collapse it; the badge tooltip SHALL give the first and last line numbers of the group and, when known, its first and last timestamps. The mode SHALL NOT affect the HEX and rendered Markdown views, and with mode Off the view SHALL behave exactly as without this feature. The mode SHALL be saved with the stream in the workspace and in session files as `collapse=exact` or `collapse=numbers`, written only when not Off; expanded groups SHALL NOT be saved.

#### Scenario: Retry loop in Exact mode
- **WHEN** mode is Exact and lines 10 to 509 read `12:00:0x.xxx WARN connection refused, retrying` with 500 different timestamps, and line 510 is `INFO connected`
- **THEN** the view shows one row for line 10 with the badge `×500`, followed by the row for line 510.

#### Scenario: Numbers mode masks ids
- **WHEN** mode is Numbers and three consecutive lines read `user 41 fetched order 0x1f3a`, `user 42 fetched order 0x1f3b` and `user 43 fetched order 0x2000`
- **THEN** they are shown as one row with the badge `×3`, while in Exact mode they are shown as three rows.

#### Scenario: Identical stack traces collapse as whole entries
- **WHEN** mode is Exact and the same ERROR line followed by the same 12 `at …` continuation lines is logged 40 times in a row
- **THEN** the view shows the first ERROR line with the badge `×40` followed by its 12 continuation lines, and no other row of that run.

#### Scenario: Runs follow the filter
- **WHEN** mode is Exact, lines `A`, `B`, `A` are consecutive and an exclude filter hides `B`
- **THEN** the two `A` lines are shown as one row with `×2`, and removing the filter shows three rows again.

#### Scenario: Expanding a group
- **WHEN** the user clicks the `×500` badge
- **THEN** all 500 lines are shown as rows, and clicking the badge again shows one row.

#### Scenario: Mode restored with the workspace
- **WHEN** a stream is set to Numbers and FastTail is restarted
- **THEN** the stream reopens in Numbers mode with its groups collapsed, and the other streams keep their own mode.

### Requirement: Collapse on Large and Growing Files
On a stream larger than 16 MB, run detection SHALL run on a worker thread after the filter pass, with its progress in the stream bar, and the view SHALL stay usable meanwhile, collapsing the rows already scanned while keeping the line at the top of the view in place. The memory kept per stream for collapsing SHALL be at most 24 bytes per group plus the expanded set, and no memory per visible line SHALL be kept once detection is complete. While the file grows, appended lines SHALL be compared with the last entry, so a repetition at the end increases the last group's count instead of adding a row, and in follow mode the view SHALL stay on the last row. A truncation, rotation, rewrite or re-decode SHALL discard the groups and the expanded set and detect the runs again.

#### Scenario: Large log stays responsive
- **WHEN** mode Numbers is chosen on a 6 GB stream
- **THEN** the interface stays responsive, the stream bar shows the collapse progress, and the rows become collapsed as the scan advances.

#### Scenario: Count grows while following
- **WHEN** follow mode is on, the last row shows `heartbeat ok` with `×12` and the writer appends 3 more `heartbeat ok` lines
- **THEN** the last row shows `×15`, the number of rows does not change and the view stays at the bottom.

#### Scenario: Rotation resets the groups
- **WHEN** a group is expanded and the file is truncated and rewritten
- **THEN** the groups are detected again on the new content and no group is expanded.

### Requirement: Time Delta Across Collapsed Groups
When the time delta column is shown and a group is collapsed, the delta of the row that follows the group SHALL be computed from the effective timestamp of the group's last line, and the delta of the group's head row SHALL be computed from the previous row as for any row.

#### Scenario: Delta after a collapsed run
- **WHEN** a group of 100 lines stamped 14:00:00.000 to 14:00:09.900 is collapsed and the next line is stamped 14:00:10.400
- **THEN** the row after the group shows `+0.500`.

