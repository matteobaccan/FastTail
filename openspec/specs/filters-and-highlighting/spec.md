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
