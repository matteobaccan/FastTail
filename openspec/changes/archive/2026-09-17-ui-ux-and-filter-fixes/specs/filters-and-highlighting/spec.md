## MODIFIED Requirements

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

### Requirement: Multi-Rule Highlighting with Font Styles and Sound Alerts
The application SHALL allow defining multiple highlight rules with custom foreground color, background color, bold text toggle, italic text toggle, and sound alert preset (None, Beep, Chime, Warning, Critical). Configured sound alerts SHALL be persistently saved in the application configuration file (`fasttail.ini`) and fully restored upon application restart.

#### Scenario: Rule match triggers style and sound alert
- **WHEN** an incoming log line matches a rule configured with red foreground, yellow background, bold font, and Critical sound alert
- **THEN** the line is rendered in bold with the specified colors and the system plays the critical stop audio alert.

#### Scenario: Sound alerts persisted across restarts
- **WHEN** a highlight rule is configured with a sound alert preset and the application is restarted
- **THEN** the loaded configuration preserves the exact sound alert preset for the rule.
