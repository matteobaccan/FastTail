## ADDED Requirements

### Requirement: Terminal Time Anchor
The terminal interface SHALL set and clear the time anchor of the time delta column on the cursor row, and SHALL show the elapsed time of a multi-row selection.

#### Scenario: From the anchor
- **WHEN** the anchor is on line 10 and line 12 is 1.25 s later
- **THEN** line 12's Δt cell reads `+1.250`.
