## MODIFIED Requirements

### Requirement: Time Delta Column
The text view SHALL offer an optional time delta column, available on streams with usable timestamps. The column SHALL be switched per stream from that stream's toolbar (`Δt`), and switching it SHALL NOT change any other stream. A newly opened stream SHALL start from the `show_time_delta` default in `fasttail.ini`, which Settings SHALL present as the default for new streams; changing it SHALL NOT change the streams already open. The switch SHALL be saved with the stream in the workspace and in session files as `time_delta=true` or `time_delta=false`, written for every stream so that a saved stream never depends on the default; only a file written by an older version, without the key, SHALL take the default. Changing the default SHALL NOT mark an open named session as modified. For each visible row whose line carries a timestamp of its own, the column SHALL show the difference between that line's timestamp and the effective timestamp of the previous visible row, so that it follows the active filters; continuation lines, the first visible row and rows without a known time SHALL show no value. Values SHALL be signed with millisecond precision, formatted as seconds below one minute, `M:SS.mmm` below one hour, `H:MM:SS` below one day and days plus hours and minutes above, and values at or above a gap threshold (default 1 s), one preference shared by every stream, SHALL be drawn in the accent colour. Timestamps SHALL be those of the timestamp cache, compared on the clock the log printed. Enabling the column SHALL NOT pause the interface: the cache is filled progressively and rows not timed yet show a pending mark, and only the streams that show the column SHALL be timed for it. On a stream without usable timestamps the column SHALL be hidden and the toolbar SHALL say why.

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
- **THEN** that stream shows the column, its section holds `time_delta=true`, and the other stream's section holds `time_delta=false`.

#### Scenario: A saved session ignores a later default
- **WHEN** a session is saved while the default is off with a stream whose column is off, the default is then turned on, and the session is loaded
- **THEN** that stream opens without the column.

#### Scenario: Old files follow the default
- **WHEN** a workspace or session file written before this change is opened with `show_time_delta=true` in `fasttail.ini`
- **THEN** every stream it opens shows the column.
