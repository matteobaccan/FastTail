## ADDED Requirements

### Requirement: Terminal Charts
The terminal interface SHALL draw a one-row volume sparkline of the stream over time and SHALL show a sparkline per value in the Statistics view and per pattern in the Patterns view when the stream is timed.

#### Scenario: Volume row
- **WHEN** a timed stream is open and the user presses `Alt+V`
- **THEN** a row of blocks above the lines shows how many lines each time slice has.
