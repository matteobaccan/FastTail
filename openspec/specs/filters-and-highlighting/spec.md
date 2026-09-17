# Filters and Highlighting Specification

## Purpose
Enables real-time filtering (include/exclude) and multi-rule visual and acoustic highlighting for streamed log lines with priority ordering.

## Requirements

### Requirement: Live Include and Exclude Filters
The application SHALL provide real-time filtering directly in the stream control bar and filter management tabs, supporting case-sensitive and case-insensitive plain text and regular expressions.

#### Scenario: User applies include and exclude filters
- **WHEN** the user inputs an include regex ERROR|WARN and an exclude regex healthcheck
- **THEN** only lines containing ERROR or WARN that do not contain healthcheck are displayed in the viewport.

### Requirement: Multi-Rule Highlighting with Font Styles and Sound Alerts
The application SHALL allow defining multiple highlight rules with custom foreground color, background color, bold text toggle, italic text toggle, and sound alert preset (None, Beep, Chime, Warning, Critical).

#### Scenario: Rule match triggers style and sound alert
- **WHEN** an incoming log line matches a rule configured with red foreground, yellow background, bold font, and Critical sound alert
- **THEN** the line is rendered in bold with the specified colors and the system plays the critical stop audio alert.

### Requirement: Top-Down Rule Priority and First-Match Evaluation
Highlight rules SHALL be evaluated strictly in order from top to bottom. Once a rule matches a line, styling from that rule is applied and evaluation for that line terminates. The UI SHALL provide buttons to move rules up (⬆) and down (⬇) to easily adjust priority.

#### Scenario: Reordering rules changes line styling
- **WHEN** Rule A (Green) is placed above Rule B (Red) and a line matches both
- **THEN** Rule A is applied, rendering the line in Green. When the user moves Rule B above Rule A, the line immediately re-renders in Red.

### Requirement: Clean Initialization of New Rules
When the user creates a new filter or highlight rule, the text and regex fields SHALL initialize empty without prefilled sample text.
