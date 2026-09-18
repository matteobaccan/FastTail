## ADDED Requirements

### Requirement: Background Tab Activity Badge
A stream tab that is not currently displayed SHALL show a badge with the number of lines appended since it was last displayed, capped at `999+`, coloured by the most severe highlight (or log level, when detected) among those lines. The badge SHALL clear when the tab is displayed again.

#### Scenario: Lines arrive in a hidden tab
- **WHEN** stream B is behind stream A in the same tab group and 42 lines are appended to B, one matching a rule with a Critical sound preset
- **THEN** B's tab shows `42` in the critical colour, and clicking B clears it.

### Requirement: Window Attention on Background Alerts
When enabled in Settings, a highlight rule with a sound preset matching in a stream that is not displayed while the window is unfocused SHALL request user attention from the OS (taskbar/dock flash).

#### Scenario: Alert while the window is in the background
- **WHEN** the option is on, the window is not focused, and a Critical rule matches
- **THEN** the OS attention request is sent once, and not again until the window has been focused.
