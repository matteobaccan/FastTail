## MODIFIED Requirements

### Requirement: Go To Line
Ctrl+G SHALL open a go-to popup for the focused stream. Entering a 1-based line number and pressing Enter SHALL scroll that line to the middle of the viewport and pause follow mode. Numbers past the end SHALL clamp to the last line. Under active filters the first visible line at or after the target SHALL be used and the popup SHALL say so. The forms `+N` and `-N` SHALL jump relative to the current top line. An input containing `:` SHALL be read as a time instead, in the forms the timestamp range filter accepts (a bare `HH:MM[:SS]` belonging to the day of the stream's first timestamped line, a `YYYY-MM-DD HH:MM[:SS]`, or a timestamp copied out of a line), and SHALL jump to the first line whose timestamp is at or after it, building the timestamp cache first when it has not been built yet. The search SHALL be a binary search when the timestamps never go back in time and a linear scan otherwise. Anything else, including a bare epoch number, is a line number.

#### Scenario: Jumping to a hidden line
- **WHEN** an include filter hides line 500 and the user goes to line 500
- **THEN** the viewport centres on the first visible line after 500 and the popup reports the substitution.

#### Scenario: Number beyond the file
- **WHEN** the file has 1,000 lines and the user enters 5000
- **THEN** the viewport shows line 1,000 and follow mode is paused.

#### Scenario: Jump to a time on a freshly opened log
- **WHEN** the user opens an ordered log, presses Ctrl+G without touching the time range and enters `14:03:30`
- **THEN** the viewport centres on the first line stamped at or after 14:03:30 on the day of the log's first entry.

#### Scenario: Time past the end of the log
- **WHEN** the user enters a time later than every timestamp in the stream
- **THEN** the popup reports that the input cannot be resolved and the viewport does not move.
