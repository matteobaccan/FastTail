## ADDED Requirements

### Requirement: Time Delta Column
The text view SHALL offer an optional time delta column, toggled from the stream toolbar and persisted in `fasttail.ini`, available on streams with usable timestamps. For each visible row whose line carries a timestamp of its own, the column SHALL show the difference between that line's timestamp and the effective timestamp of the previous visible row, so that it follows the active filters; continuation lines, the first visible row and rows without a known time SHALL show no value. Values SHALL be signed with millisecond precision, formatted as seconds below one minute, `M:SS.mmm` below one hour, `H:MM:SS` below one day and days plus hours and minutes above, and values at or above a configurable gap threshold (default 1 s) SHALL be drawn in the accent colour. Timestamps SHALL be those of the timestamp cache, compared on the clock the log printed. Enabling the column SHALL NOT pause the interface: the cache is filled progressively and rows not timed yet show a pending mark. On a stream without usable timestamps the column SHALL be hidden and the toolbar SHALL say why.

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
