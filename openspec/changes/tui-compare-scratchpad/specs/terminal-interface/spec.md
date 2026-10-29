## ADDED Requirements

### Requirement: Terminal Compare and Scratchpad
The terminal interface SHALL compare two lines or two regions side by side with the window's alignment and ignore options, and SHALL offer the session's scratchpad as an editable panel that the window shares.

#### Scenario: Compare two lines
- **WHEN** the user selects two rows and presses `=`
- **THEN** a Compare panel shows them side by side with the differing tokens highlighted.

#### Scenario: Send to scratchpad
- **WHEN** the user sends a row with `Ctrl+N`
- **THEN** the pad gets a `── app.log:N ──` reference and the line, and the window shows the same text.
