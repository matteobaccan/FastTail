## ADDED Requirements

### Requirement: Go To Line
Ctrl+G SHALL open a go-to popup for the focused stream. Entering a 1-based line number and pressing Enter SHALL scroll that line to the middle of the viewport and pause follow mode. Numbers past the end SHALL clamp to the last line. Under active filters the first visible line at or after the target SHALL be used and the popup SHALL say so. The forms `+N` and `-N` SHALL jump relative to the current top line.

#### Scenario: Jumping to a hidden line
- **WHEN** an include filter hides line 500 and the user goes to line 500
- **THEN** the viewport centres on the first visible line after 500 and the popup reports the substitution.

#### Scenario: Number beyond the file
- **WHEN** the file has 1,000 lines and the user enters 5000
- **THEN** the viewport shows line 1,000 and follow mode is paused.
