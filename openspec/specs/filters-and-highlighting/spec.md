# Filters and Highlighting Specification

## Purpose
Enables real-time filtering (include/exclude) and multi-rule visual and acoustic highlighting for streamed log lines with priority ordering.
## Requirements
### Requirement: Live Include and Exclude Filters
The application SHALL provide real-time filtering directly in the stream control bar and filter management tabs, supporting case-sensitive and case-insensitive plain text and regular expressions. Empty include and exclude filter fields SHALL represent no filter constraint, allowing lines to pass unfiltered unless an active criterion excludes or includes them.

#### Scenario: User applies include and exclude filters
- **WHEN** the user inputs an include regex ERROR|WARN and an exclude regex healthcheck
- **THEN** only lines containing ERROR or WARN that do not contain healthcheck are displayed in the viewport.

#### Scenario: Empty include and exclude filters show all lines
- **WHEN** both the include filter and exclude filter are empty
- **THEN** all lines in the log stream are displayed in the viewport without omission.

#### Scenario: Empty include filter with non-empty exclude filter
- **WHEN** the include filter is empty and the exclude filter contains a pattern
- **THEN** all lines not matching the exclude pattern are displayed in the viewport.

#### Scenario: Highlight rules do not bypass the include filter
- **WHEN** a highlight rule for ERROR is enabled and the include filter is `payment`
- **THEN** ERROR lines that do not contain `payment` stay hidden; highlight rules only style the lines that pass the filters.

#### Scenario: Filters applied while the file grows
- **WHEN** a filter is active and the writer appends lines
- **THEN** only the appended lines are evaluated against the filters, and matching ones appear at the bottom without re-scanning the whole file.

### Requirement: Automatic Filter Activation
Filtering SHALL activate automatically whenever either the include or exclude filter input contains text, without a manual mode switch button.

#### Scenario: User types in filter box
- **WHEN** the user enters a string in the include or exclude filter field
- **THEN** filtering immediately applies to the stream, and clearing both fields restores all lines.

### Requirement: Multi-Rule Highlighting with Font Styles and Sound Alerts
The application SHALL allow defining multiple highlight rules with custom foreground color, background color, bold text toggle, italic text toggle, and sound alert preset (None, Beep, Chime, Warning, Critical). Configured sound alerts SHALL be persistently saved in the application configuration file (`fasttail.ini`) and fully restored upon application restart.

#### Scenario: Rule match triggers style and sound alert
- **WHEN** an incoming log line matches a rule configured with red foreground, yellow background, bold font, and Critical sound alert
- **THEN** the line is rendered in bold with the specified colors and the system plays the critical stop audio alert.

#### Scenario: Sound alerts persisted across restarts
- **WHEN** a highlight rule is configured with a sound alert preset and the application is restarted
- **THEN** the loaded configuration preserves the exact sound alert preset for the rule.

### Requirement: Top-Down Rule Priority and First-Match Evaluation
Highlight rules SHALL be evaluated strictly in order from top to bottom. Once a rule matches a line, styling from that rule is applied and evaluation for that line terminates. The UI SHALL provide buttons to move rules up (⬆) and down (⬇) to easily adjust priority.

#### Scenario: Reordering rules changes line styling
- **WHEN** Rule A (Green) is placed above Rule B (Red) and a line matches both
- **THEN** Rule A is applied, rendering the line in Green. When the user moves Rule B above Rule A, the line immediately re-renders in Red.

### Requirement: Clean Initialization of New Rules
When the user creates a new filter or highlight rule, the text and regex fields SHALL initialize empty without prefilled sample text.

#### Scenario: Adding a new highlight rule
- **WHEN** the user clicks the add-rule button in the Highlights dialog
- **THEN** a new rule row appears with an empty pattern field, ready for typing, and no placeholder text is saved to the configuration.

### Requirement: Capture-Only Highlighting
A regex highlight rule with capture groups MAY set "highlight captures only"; then only the captured spans of a matching row SHALL be painted with the rule's style while the rest of the row keeps its normal style. Rules without the option SHALL keep colouring the whole row. At most 64 spans per row SHALL be painted, first rule winning per byte. The 64-span budget SHALL be shared, in priority order, by user rules, quick labels and then the ANSI colour spans of a stream in render mode (see the ansi-escape-codes capability); once the budget is used, the remaining bytes of the row keep their normal style.

#### Scenario: Colouring request ids
- **WHEN** the rule `req=(\d+)` with captures-only and a cyan foreground is enabled
- **THEN** only the digits after `req=` are cyan on each matching row.

#### Scenario: Captures inside a coloured row
- **WHEN** the same rule is enabled and a row in ANSI render mode shows `req=42` inside a yellow ANSI span
- **THEN** the digits `42` are cyan and the rest of the yellow span stays yellow.

### Requirement: Quick Colour Labels
Ctrl+Shift+1..9 SHALL create or toggle a quick label for the current search text of the focused stream (row selection is whole-row only, so the search text is the text source; without a current search hit the stream bar SHALL show a notice) using preset colour N, applied across all streams, listed in a strip above the stream with a remove button, and not persisted across restarts. Pressing the same digit again SHALL remove the label, another digit SHALL recolour it. Quick labels SHALL rank below user rules.

#### Scenario: Labelling a session id
- **WHEN** the user searches `sess-8f3a` and presses Ctrl+Shift+2
- **THEN** every occurrence of `sess-8f3a` in every stream is painted with preset colour 2 until the label is removed or the application restarts.

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

### Requirement: Combined Filter Terms
A stream SHALL accept up to 8 include terms and up to 8 exclude terms. The stream bar SHALL show the first term of each side in its include and exclude fields and SHALL indicate how many further terms are active; all terms SHALL be editable in the Filters window. A line SHALL pass the text filter when it matches every non-empty include term and none of the non-empty exclude terms; the minimum-level and time filters SHALL then apply as before, and a stack-trace continuation line SHALL still follow its parent entry unless an exclude term matches it. Every term SHALL be plain text or a regular expression according to the stream's case-sensitive and regex toggles, with no operator syntax: text typed in a term is matched as typed. A term whose regular expression does not compile SHALL be flagged in its row. Include and exclude terms SHALL be persisted per stream in the workspace and in session files, the first term of each side under the existing keys.

#### Scenario: Two conditions on the same line
- **WHEN** the include terms are `payment` and `timeout` and the exclude terms are `healthcheck` and `retry=0`
- **THEN** only lines containing both `payment` and `timeout` and containing neither `healthcheck` nor `retry=0` are visible.

#### Scenario: Operator characters are literal
- **WHEN** the include term is the plain text `a && !b`
- **THEN** only lines containing the literal text `a && !b` are visible.

#### Scenario: Extra terms are never hidden
- **WHEN** a stream has three include terms
- **THEN** the stream bar shows the first term in the include field with a `+2` indicator.

### Requirement: Filter Presets
The user SHALL be able to save the filter state of a stream as a named preset holding its include and exclude terms, the case-sensitive and regex toggles, the minimum level and the unknown-level toggle, and, when chosen at save time, the time range as typed. Presets SHALL be stored in `fasttail.ini`, SHALL NOT be part of session files, and SHALL have names unique without regard to case. A presets drop-down in the stream bar SHALL apply a preset to that stream or to all open streams, replacing their filter state in one recomputation; a preset without a time range SHALL leave the stream's time range unchanged. The drop-down SHALL show the name of the preset the stream's filter state equals, marked as modified once the state has been edited after applying it. Presets SHALL be renamed, deleted after confirmation, reordered and updated from a stream in the Filters window.

#### Scenario: Applying a saved preset
- **WHEN** the user saved `payment errors` as include `payment`, exclude `healthcheck`, `>= WARN`, and applies it to another stream
- **THEN** that stream's include field reads `payment`, its exclude field `healthcheck`, its level selector `>= WARN`, and the drop-down shows `payment errors`.

#### Scenario: Editing after applying
- **WHEN** the user applies `payment errors` and then adds the include term `timeout`
- **THEN** the drop-down shows `payment errors *` and offers to update the preset from the stream.

#### Scenario: Preset with a bare time range
- **WHEN** a preset saved with the time range from `14:02` to `14:05` is applied to a log whose first entry is dated 2026-09-20
- **THEN** the window covers 2026-09-20 14:02:00.000 to 14:05:59.999 on that log.

#### Scenario: Presets survive a restart
- **WHEN** the user saves two presets and restarts FastTail
- **THEN** both presets are listed in the drop-down in the same order.

### Requirement: Global Filter
The application SHALL offer one global filter of up to 8 include terms and up to 8 exclude terms, with case-sensitive and regex toggles of its own, edited in a global filter bar that is shown or hidden from the toolbar and with `CTRL + SHIFT + H`. While the global filter is switched on and has at least one non-empty term, a line of any stream SHALL be visible only when it passes that stream's own filters (terms, minimum level, time range) and also contains every non-empty global include term and none of the non-empty global exclude terms; a stack-trace continuation line SHALL follow its parent entry unless a stream or global exclude term matches it. The global filter SHALL apply to every open stream and to every stream opened afterwards, including standard input and compressed streams, and SHALL NOT change the HEX view. Switching it off SHALL restore each stream's own filtering without losing the global terms; hiding the bar SHALL NOT switch it off. While it is on, every stream bar SHALL show a global filter badge whose tooltip lists the global terms. The Find results, line counters, overview strip and search results pane SHALL see the lines as filtered by both. A change of the global terms SHALL be applied to the streams at most 300 ms after the last keystroke, and on a stream above 16 MB the recomputation SHALL run in the background as for a stream filter. The global filter SHALL be persisted in the `[global_filter]` section of `fasttail.ini` (`enabled`, `case_sensitive`, `regex`, `bar_open`, `include.1`…`include.8`, `exclude.1`…`exclude.8`) and SHALL NOT be written to session files or filter presets.

#### Scenario: Hiding the same noise everywhere
- **WHEN** three streams are open and the user adds the global exclude term `healthcheck`
- **THEN** no line containing `healthcheck` is visible in any of the three streams, each stream bar shows the global filter badge, and each stream keeps its own include and exclude fields unchanged.

#### Scenario: Combined with a stream's own terms
- **WHEN** the global include term is `req-7f3a` and a stream's own include term is `ERROR`
- **THEN** that stream shows only the lines containing both `req-7f3a` and `ERROR`, while a stream without terms of its own shows every line containing `req-7f3a`.

#### Scenario: A stream opened later
- **WHEN** the global exclude term `DEBUG` is on and the user opens a further log
- **THEN** the new stream opens with its `DEBUG` lines already hidden and shows the badge.

#### Scenario: Switched off, not lost
- **WHEN** the user switches the global filter off and restarts FastTail
- **THEN** every stream shows its own filtering only, and switching the global filter on again brings back the same terms.

#### Scenario: Find results under the global filter
- **WHEN** the global exclude term is `healthcheck` and the user searches `req-7f3a` across all streams
- **THEN** no result is a line containing `healthcheck`.

#### Scenario: Large file
- **WHEN** a 2 GB stream is open and the user adds a global include term
- **THEN** the interface stays responsive, the stream bar shows the background filter progress, and the stream shows the matching lines once the scan completes.

### Requirement: Bookmark Matching Lines Rule Option
Each highlight rule SHALL have a "Bookmark matching lines" option, off by default and off for new rules, edited in the rule editor beside the sound alert. It SHALL be independent of the rule's colours, font styles, sound alert and capture-only mode: a rule lower in the list whose colours are hidden by a higher rule under first-match evaluation SHALL still bookmark the lines it matches. The option SHALL be persisted in the rule's `[highlight_<n>]` section of `fasttail.ini` as `bookmark` (`true` / `false`); a section without the key SHALL load with the option off. Changing the option, the pattern or the enabled state of a rule with the option SHALL recompute the automatic bookmarks of every open stream.

#### Scenario: Option persisted
- **WHEN** the user turns on "Bookmark matching lines" for the rule `FATAL` and restarts FastTail
- **THEN** `fasttail.ini` holds `bookmark=true` in that rule's section and the option is still on.

#### Scenario: Rule hidden by a higher rule
- **WHEN** rule 1 `ERROR` has no bookmark option, rule 2 `timeout` has it on, and a line contains both `ERROR` and `timeout`
- **THEN** the line is painted with rule 1's colours and carries an automatic bookmark.

#### Scenario: Older settings file
- **WHEN** `fasttail.ini` has a `[highlight_0]` section without a `bookmark` key
- **THEN** the rule loads with "Bookmark matching lines" off.

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

