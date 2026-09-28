## ADDED Requirements

### Requirement: Time span control and time range popup
In Text view the stream bar SHALL show the time span of the visible lines as a clickable control, and the filter row SHALL NOT hold time range fields. The control label SHALL be `🕘` followed by the first and last timestamp of the visible lines, with the date written once when both share it (`2026-09-18 14:02 → 16:30`); a label longer than 40 characters SHALL be shortened with the full span and the window that is set in the tooltip. The control SHALL be tinted with the accent colour while a time window narrows the view, use the warning colour while a side of the window cannot be read, end with `⏳` while the window waits for the background timing, and read `🕘` plus a translated "no timestamps", dimmed, on a stream without usable timestamps. Clicking the control SHALL open a popup anchored under it holding, for the "from" and the "to" side, a text field at least 170 px wide that accepts every format the time range accepts, together with the invalid-time and no-usable-timestamps hints and **OK** and **Cancel** buttons. The popup SHALL open with the window that is set (empty sides when none is). Edits in the popup SHALL be a draft: **OK** or `Enter` in a field SHALL apply the window with the same rules as typed text and close the popup; **Cancel**, `Esc` or a click outside SHALL close it and leave the window unchanged. **OK** SHALL be disabled while a side cannot be read. Nothing new SHALL be stored in `fasttail.ini`.

#### Scenario: The span is the control
- **WHEN** a stream shows lines from `2026-09-18 14:02:05` to `2026-09-18 16:30:12` and no window is set
- **THEN** the stream bar reads `🕘 2026-09-18 14:02:05 → 16:30:12` in the normal colour, and clicking it opens the popup with both sides empty

#### Scenario: Confirming a window changes the visible range
- **WHEN** the user types from `2026-09-18 15:00` to `2026-09-18 15:30` in the popup and presses OK
- **THEN** the popup closes, only the lines in that window are visible, and the control reads the span of those lines in the accent colour

#### Scenario: Cancelling keeps the window
- **WHEN** a window is set, the user opens the popup, changes both sides and presses `Esc`
- **THEN** the popup closes and the window, the visible lines and the control are as before

#### Scenario: Invalid text
- **WHEN** the "to" field holds `14:6x`
- **THEN** the popup shows the invalid-time hint and OK is disabled until the text is fixed or cleared

#### Scenario: No usable timestamps
- **WHEN** the stream has been timed and none of its lines carries a timestamp
- **THEN** the control reads `🕘` and the translated "no timestamps", dimmed, and the popup shows the no-usable-timestamps hint

### Requirement: Calendar and time spinners
Each side of the popup SHALL offer a calendar of one month (a 7×6 grid, weeks starting on Monday, translated month and weekday names, arrows for the previous / next month and year) and hour, minute and second spinners. The calendar SHALL open on the month of the side's current value if it can be read, otherwise on the month of the stream's first timestamp, otherwise on the current month. Days between the stream's first and last timestamp SHALL be tinted and the current day outlined. Picking a day SHALL keep the side's time if it has one; otherwise the side SHALL become the bare date, meaning 00:00:00 on the "from" side and through 23:59:59.999 on the "to" side. Changing a spinner SHALL write the side as `YYYY-MM-DD HH:MM:SS`. The calendar and spinners SHALL only rewrite the side's draft text, which is what OK applies and what is saved.

#### Scenario: Picking a day
- **WHEN** both sides are empty and the user picks 18 on the "from" calendar and 18 on the "to" calendar of September 2026 and presses OK
- **THEN** the window is `2026-09-18` to `2026-09-18` and every line stamped on that day is visible

#### Scenario: Picking a day keeps the time
- **WHEN** the "from" side holds `2026-09-18 14:02:00` and the user picks 19 on its calendar
- **THEN** the side reads `2026-09-19 14:02:00`

#### Scenario: Calendar opens where the log is
- **WHEN** the popup is opened with empty sides on a log whose first line is stamped 2025-07-31 09:26:03
- **THEN** both calendars show July 2025 with the days the log spans tinted

### Requirement: Time range shortcuts
The popup SHALL offer shortcut buttons that fill the draft: "Whole log" empties both sides; "First day" and "Last day" set both sides to the date of the stream's first or last timestamp; "Last hour" sets the window to the hour ending at the stream's last timestamp. The shortcuts that need timestamps SHALL be disabled until the stream has been timed and when it has no usable timestamps. Like any edit, a shortcut SHALL take effect when OK is pressed.

#### Scenario: Last day of a multi-day log
- **WHEN** a log runs from 2026-05-25 to 2026-05-29 and the user presses "Last day" and then OK
- **THEN** only the lines of 2026-05-29 are visible

#### Scenario: Last hour
- **WHEN** the last timestamp of the log is 2026-05-29 23:38:12 and the user presses "Last hour"
- **THEN** the sides read `2026-05-29 22:38:12` and `2026-05-29 23:38:12`

## MODIFIED Requirements

### Requirement: Timestamp Range Filter
The time range popup opened from the stream bar's time span (see "Time span control and time range popup") SHALL offer "from" and "to" time inputs, either side optional (an empty side is an open end). Each input SHALL accept a bare `HH:MM` or `HH:MM:SS`, a `YYYY-MM-DD HH:MM[:SS]` (space or `T` separator), a bare `YYYY-MM-DD` (00:00:00.000 of that day on the "from" side), or any timestamp the line parser recognises, so a timestamp copied out of a line works. A bare time SHALL belong to the day of the stream's first timestamped line, not to the current day. The "to" side SHALL cover the whole unit typed: a date alone covers the whole day (through 23:59:59.999), a time without seconds the whole minute, a time with seconds the whole second. Only lines whose detected timestamp (or the timestamp inherited by a continuation line) lies inside the range SHALL be visible, combined with the include/exclude and level filters; while a window is set, the lines before the first timestamped line cannot be placed in time and SHALL be hidden. An input that cannot be read SHALL be flagged in the popup, and emptying both sides (or the "Whole log" shortcut) and confirming SHALL remove the window. The stream bar SHALL show the time span of the visible lines. A hint SHALL say the stream has no usable timestamps when fewer than half of the timed lines can be placed in time (a recognised timestamp of their own, or one inherited from the entry they continue); the fields SHALL stay editable in every case, so a window can always be typed, corrected or cleared. When the window is entered before the stream has been fully timed and the timing runs in the background, the window SHALL be kept as pending: the view SHALL keep showing the lines it showed, the time span control and the popup SHALL say that the window applies when timing finishes, and the window SHALL be applied automatically once every line is timed. Confirming another window while pending SHALL replace the pending window, and confirming empty sides SHALL drop it.

#### Scenario: Three-minute window
- **WHEN** the user enters from `14:02` to `14:05` in the popup and confirms, on a log whose first entry is dated 2026-09-18
- **THEN** only lines stamped between 2026-09-18 14:02:00.000 and 14:05:59.999 are visible, including the stack-trace lines that follow an entry in that window.

#### Scenario: Banner lines before the first timestamp
- **WHEN** a log starts with a banner line without a timestamp and the user sets a "from" time
- **THEN** the banner line is hidden, and clearing the window shows it again.

#### Scenario: Open-ended window
- **WHEN** the user enters only from `14:02`
- **THEN** every line stamped at or after 14:02:00.000 is visible, up to the end of the log.

#### Scenario: A log that cannot be timed
- **WHEN** fewer than half of the lines of a stream can be placed in time
- **THEN** the popup says the stream has no usable timestamps, and its fields stay editable.

#### Scenario: A log made mostly of stack traces
- **WHEN** a log has one timestamped entry every four lines, each followed by a three-line stack trace
- **THEN** no hint is shown, since every line inherits the timestamp of its entry, and after a window is cleared both fields of the popup are still editable.

#### Scenario: A date alone
- **WHEN** the user enters from `2026-09-18` to `2026-09-18`
- **THEN** every line stamped between 2026-09-18 00:00:00.000 and 23:59:59.999 is visible, and no "invalid time" warning is shown.

#### Scenario: Partial input is not an error while typing
- **WHEN** the user types `14:0` in the popup's "from" field on the way to `14:02`
- **THEN** no "invalid time" warning is shown while the field has focus; it is shown, and OK is disabled, if the field is left holding text that cannot be read.

#### Scenario: Window typed while the log is being timed
- **WHEN** the user enters from `14:02` on a 3 GB stream that has never been timed
- **THEN** every line stays visible, the time span control shows `⏳` and says the window applies when timing finishes, the stream bar shows the timing progress, and when it reaches 100% only the lines from 14:02 on remain visible.
