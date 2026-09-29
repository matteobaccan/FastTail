## ADDED Requirements

### Requirement: Relative Time Windows
Each side of the time range SHALL also accept a relative time: `now`, or `-` followed by one or more number-and-unit pairs with the units `s`, `m`, `h`, `d` and `w` (for example `-15m`, `-3h`, `-1h30m`), meaning the current time minus that duration. The current time SHALL be the local clock, or the current time in the stream's source zone when one is set. A relative "to" side SHALL be the exact instant, without widening to the end of a unit. While a side of the window is relative the window SHALL be live: its bounds SHALL be re-evaluated at least every 5 seconds and when lines are appended, lines that leave the window SHALL disappear and appended lines inside it SHALL appear, without recomputing the filter over the whole file when the stream's timestamps never decrease, and with a full recomputation at most once a minute otherwise. The time span control SHALL begin with `⟳` while the window is live, and its tooltip SHALL give the typed text and the bounds in force. When a live window holds no line because the stream's last timestamp is before its start, the control SHALL say so and give the last line's time. A filter preset saved with a relative window SHALL keep the relative text.

#### Scenario: The last 15 minutes of a live log
- **WHEN** the local time is 14:30:00, the user enters from `-15m` with an empty "to" on a followed log and confirms
- **THEN** the lines stamped from 14:15:00 are visible, the control begins with `⟳`, and at 14:31 the lines stamped before 14:16:00 are no longer shown while the lines appended meanwhile are.

#### Scenario: Relative text that is not valid
- **WHEN** the "from" field holds `-15x`
- **THEN** the popup shows the invalid-time hint and OK is disabled.

#### Scenario: A log that stopped writing
- **WHEN** the window is from `-15m` and the log's last line is stamped two hours ago
- **THEN** no line is visible and the control says there are no lines in the window and gives the time of the last line.

### Requirement: Time Window Sliding Cost
Re-evaluating a live window on a stream whose timestamps never decrease SHALL only remove lines from the start of the visible lines and add lines after the last visible one, so that the cost is proportional to the lines that enter or leave the window and not to the size of the file.

#### Scenario: A 3 GB followed log
- **WHEN** a live window `-1h` is set on a followed 3 GB log with ordered timestamps and one minute passes
- **THEN** no background filter job is started and the interface stays responsive.

## MODIFIED Requirements

### Requirement: Time range shortcuts
The popup SHALL offer shortcut buttons that fill the draft: "Whole log" empties both sides; "First day" and "Last day" set both sides to the date of the stream's first or last timestamp; "Last hour of the log" sets the window to the hour ending at the stream's last timestamp. A "Relative to now" row SHALL offer 5 min, 15 min, 1 h, 6 h, 24 h and 7 d, each setting the "from" side to the matching relative time (`-5m`, `-15m`, `-1h`, `-6h`, `-24h`, `-7d`) and emptying the "to" side. The shortcuts that need the stream's timestamps ("First day", "Last day", "Last hour of the log") SHALL be disabled until the stream has been timed and when it has no usable timestamps; the relative shortcuts SHALL be disabled only when the stream has no usable timestamps. Like any edit, a shortcut SHALL take effect when OK is pressed.

#### Scenario: Last day of a multi-day log
- **WHEN** a log runs from 2026-05-25 to 2026-05-29 and the user presses "Last day" and then OK
- **THEN** only the lines of 2026-05-29 are visible

#### Scenario: Last hour of the log
- **WHEN** the last timestamp of the log is 2026-05-29 23:38:12 and the user presses "Last hour of the log"
- **THEN** the sides read `2026-05-29 22:38:12` and `2026-05-29 23:38:12`

#### Scenario: Relative shortcut
- **WHEN** the user presses "15 min" in the "Relative to now" row and then OK
- **THEN** the "from" side reads `-15m`, the "to" side is empty, and the window is live
