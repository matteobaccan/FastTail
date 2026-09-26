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
The filter panel SHALL offer "from" and "to" time inputs, either side optional (an empty side is an open end). Each input SHALL accept a bare `HH:MM` or `HH:MM:SS`, a `YYYY-MM-DD HH:MM[:SS]` (space or `T` separator), a bare `YYYY-MM-DD` (00:00:00.000 of that day on the "from" side), or any timestamp the line parser recognises, so a timestamp copied out of a line works. A bare time SHALL belong to the day of the stream's first timestamped line, not to the current day. The "to" side SHALL cover the whole unit typed: a date alone covers the whole day (through 23:59:59.999), a time without seconds the whole minute, a time with seconds the whole second. Only lines whose detected timestamp (or the timestamp inherited by a continuation line) lies inside the range SHALL be visible, combined with the include/exclude and level filters; while a window is set, the lines before the first timestamped line cannot be placed in time and SHALL be hidden. An input that cannot be read SHALL be flagged next to the fields, and a clear button SHALL remove the window. The stream status bar SHALL show the time span of the visible lines. A hint SHALL say the stream has no usable timestamps when fewer than half of the timed lines can be placed in time (a recognised timestamp of their own, or one inherited from the entry they continue); the fields SHALL stay editable in every case, so a window can always be typed, corrected or cleared. When the window is entered before the stream has been fully timed and the timing runs in the background, the window SHALL be kept as pending: the view SHALL keep showing the lines it showed, a hint next to the fields SHALL say that the window applies when timing finishes, and the window SHALL be applied automatically once every line is timed. Editing the fields while pending SHALL replace the pending window, and clearing them SHALL drop it.

#### Scenario: Three-minute window
- **WHEN** the user enters from `14:02` to `14:05` on a log whose first entry is dated 2026-09-18
- **THEN** only lines stamped between 2026-09-18 14:02:00.000 and 14:05:59.999 are visible, including the stack-trace lines that follow an entry in that window.

#### Scenario: Banner lines before the first timestamp
- **WHEN** a log starts with a banner line without a timestamp and the user sets a "from" time
- **THEN** the banner line is hidden, and clearing the window shows it again.

#### Scenario: Open-ended window
- **WHEN** the user enters only from `14:02`
- **THEN** every line stamped at or after 14:02:00.000 is visible, up to the end of the log.

#### Scenario: A log that cannot be timed
- **WHEN** fewer than half of the lines of a stream can be placed in time
- **THEN** a hint next to the fields says the stream has no usable timestamps, and the fields stay editable.

#### Scenario: A log made mostly of stack traces
- **WHEN** a log has one timestamped entry every four lines, each followed by a three-line stack trace
- **THEN** no hint is shown, since every line inherits the timestamp of its entry, and after a window is cleared with the clear button both fields are still editable.

#### Scenario: A date alone
- **WHEN** the user enters from `2026-09-18` to `2026-09-18`
- **THEN** every line stamped between 2026-09-18 00:00:00.000 and 23:59:59.999 is visible, and no "invalid time" warning is shown.

#### Scenario: Partial input is not an error while typing
- **WHEN** the user types `14:0` in the "from" field on the way to `14:02`
- **THEN** no "invalid time" warning is shown while the field has focus; it is shown if the field is left holding text that cannot be read, together with the clear button.

#### Scenario: Window typed while the log is being timed
- **WHEN** the user enters from `14:02` on a 3 GB stream that has never been timed
- **THEN** every line stays visible, the hint says the window applies when timing finishes, the stream bar shows the timing progress, and when it reaches 100% only the lines from 14:02 on remain visible.

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

