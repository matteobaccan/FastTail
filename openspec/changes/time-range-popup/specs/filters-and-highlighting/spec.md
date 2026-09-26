## ADDED Requirements

### Requirement: Time range button and popup
The filter bar SHALL show the time range as a single button instead of inline fields. The button label SHALL be `🕘` followed by the window that is set, built from the two texts (`2026-09-18 14:02 → 16:30` when both sides share the date, `from 14:02` or `to 2026-09-18` when one side is empty), or by a translated "all times" when no window is set; a label longer than 40 characters SHALL be shortened with the full window in the tooltip. The button SHALL be tinted with the accent colour while a window narrows the view, use the warning colour while a side cannot be read, and end with `⏳` while the window waits for the background timing. Clicking the button SHALL open a popup anchored under it holding, for the "from" and the "to" side, a text field at least 170 px wide that accepts every format the time range accepts, and a clear button, together with the invalid-time, no-usable-timestamps and pending hints. Edits SHALL apply as they are made, with the same rules as typed text. The popup SHALL close on `Esc`, on a click outside it, or on a second click on the button. Nothing new SHALL be stored in `fasttail.ini`.

#### Scenario: Label shows the window
- **WHEN** the user sets from `2026-09-18 14:02` to `2026-09-18 16:30` in the popup and closes it
- **THEN** the button reads `🕘 2026-09-18 14:02 → 16:30` in the accent colour and only the lines in that window are visible

#### Scenario: No window
- **WHEN** both sides are empty
- **THEN** the button reads `🕘` and the translated "all times", in the normal colour

#### Scenario: Invalid text
- **WHEN** the "to" field is left holding `14:6x`
- **THEN** the popup shows the invalid-time hint and the button uses the warning colour until the text is fixed or cleared

#### Scenario: Clearing from the popup
- **WHEN** a window is set and the user presses the clear button in the popup
- **THEN** both fields are empty, every line is visible again, the button reads "all times", and the fields stay editable

### Requirement: Calendar and time spinners
Each side of the popup SHALL offer a calendar of one month (a 7×6 grid, weeks starting on Monday, translated month and weekday names, arrows for the previous / next month and year) and hour, minute and second spinners. The calendar SHALL open on the month of the side's current value if it can be read, otherwise on the month of the stream's first timestamp, otherwise on the current month. Days between the stream's first and last timestamp SHALL be tinted and the current day outlined. Picking a day SHALL keep the side's time if it has one; otherwise the side SHALL become the bare date, meaning 00:00:00 on the "from" side and through 23:59:59.999 on the "to" side. Changing a spinner SHALL write the side as `YYYY-MM-DD HH:MM:SS`. The calendar and spinners SHALL only rewrite the side's text, which stays what is applied and saved.

#### Scenario: Picking a day
- **WHEN** both sides are empty and the user picks 18 on the "from" calendar and 18 on the "to" calendar of September 2026
- **THEN** the fields read `2026-09-18` and `2026-09-18` and every line stamped on that day is visible

#### Scenario: Picking a day keeps the time
- **WHEN** the "from" side holds `2026-09-18 14:02:00` and the user picks 19 on its calendar
- **THEN** the side reads `2026-09-19 14:02:00`

#### Scenario: Calendar opens where the log is
- **WHEN** the popup is opened with empty sides on a log whose first line is stamped 2025-07-31 09:26:03
- **THEN** both calendars show July 2025 with the days the log spans tinted

### Requirement: Time range shortcuts
The popup SHALL offer shortcut buttons: "Whole log" clears the window; "First day" and "Last day" set both sides to the date of the stream's first or last timestamp; "Last hour" sets the window to the hour ending at the stream's last timestamp. The shortcuts that need timestamps SHALL be disabled until the stream has been timed and when it has no usable timestamps.

#### Scenario: Last day of a multi-day log
- **WHEN** a log runs from 2026-05-25 to 2026-05-29 and the user presses "Last day"
- **THEN** both sides read `2026-05-29` and only the lines of that day are visible

#### Scenario: Last hour
- **WHEN** the last timestamp of the log is 2026-05-29 23:38:12 and the user presses "Last hour"
- **THEN** the sides read `2026-05-29 22:38:12` and `2026-05-29 23:38:12`
