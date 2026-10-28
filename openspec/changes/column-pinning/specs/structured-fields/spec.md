## ADDED Requirements

### Requirement: Pinned Columns
In the column view of the window and of the terminal, the user SHALL be able to pin columns, which then stay at the left while the other columns scroll horizontally; the timestamp column SHALL be pinned by default (`pin_time_column=true`), and the pins SHALL be kept per stream in the workspace and sessions.

#### Scenario: Scrolling a wide JSON log
- **WHEN** a JSON stream shows twenty columns with `ts` pinned and the user scrolls right to `user_agent`
- **THEN** `ts` stays visible at the left of every row, aligned with its row.
