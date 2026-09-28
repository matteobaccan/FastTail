## ADDED Requirements

### Requirement: Timestamp Display Zone
Each text stream SHALL offer a time display among "as written" (the default), UTC, local time and a fixed offset from `-14:00` to `+14:00`, and a source zone among local time (the default), UTC and a fixed offset, which says what a timestamp without a zone suffix means. With a display other than "as written", the leading timestamp of each drawn row SHALL be shown as `YYYY-MM-DD HH:MM:SS` followed by the fraction digits the log printed (at most 3), converted from the line's own zone suffix when it has one, from UTC for epoch seconds and milliseconds, and from the source zone otherwise, with `Z` appended for UTC and the offset appended for a fixed offset; local time SHALL use the offset in force at that instant, daylight saving included. The original text SHALL be shown in the tooltip of the converted timestamp. Filters, search, highlight rules, copy, export and external tools SHALL see the line as written. The time span control, the time range popup (for what it shows and for the times typed into it), the timeline histogram axis, go to time and the time delta tooltip SHALL use the display zone. Changing the display or the source zone SHALL NOT re-time the stream. Both SHALL be saved with the stream in the workspace and in session files as `time_display` and `time_source_zone`, written only when not the default.

#### Scenario: UTC log read in local time
- **WHEN** a line starts with `2026-09-28T14:02:05.123Z` and the display is local time on a machine in Central European Summer Time
- **THEN** the row shows `2026-09-28 16:02:05.123` and its tooltip shows `2026-09-28T14:02:05.123Z`.

#### Scenario: Epoch milliseconds as a date
- **WHEN** a line starts with `1790604125123` and the display is UTC
- **THEN** the row shows `2026-09-28 14:02:05.123Z`.

#### Scenario: Search sees the text as written
- **WHEN** the display is local time and the user searches `14:02:05`
- **THEN** the line starting with `2026-09-28T14:02:05.123Z` is a hit, although its row shows `16:02:05`.

#### Scenario: Time range in the display zone
- **WHEN** the display is local time as above and the user sets the time range from `16:02` to `16:03`
- **THEN** the line stamped `2026-09-28T14:02:05.123Z` is visible.

#### Scenario: As written by default
- **WHEN** a stream has no `time_display` key
- **THEN** every row shows its timestamp exactly as the log printed it.
