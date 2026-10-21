## ADDED Requirements

### Requirement: Terminal Follow and Auto-update Check Boxes
In the terminal stream bar, follow and auto-update SHALL be two chips drawn as check boxes, `[x] Follow` / `[ ] Follow` and `[x] Auto-update` / `[ ] Auto-update` (translated labels, the same in ASCII mode), and SHALL NOT use `▶` / `■`. A click on a chip SHALL toggle only its own feature. Toggling auto-update SHALL show "Auto-update on" or "Auto-update off" in the status bar. Auto-update on SHALL read the lines appended to the file as it grows (reopening it when truncated or rotated); off SHALL stop reading it, leaving the stream as it is. No text of the terminal interface SHALL call this feature "Monitor".

#### Scenario: Read while staying in place
- **WHEN** a stream shows `[x] Follow` and `[x] Auto-update` and the user clicks `[x] Follow`
- **THEN** the chips read `[ ] Follow` and `[x] Auto-update`, the window stays on the rows it shows, and the line count keeps growing as the file grows.

#### Scenario: Frozen snapshot
- **WHEN** the user clicks `[x] Auto-update` and lines are then appended to the file
- **THEN** the chip reads `[ ] Auto-update`, the status bar says "Auto-update off", and the line count does not change.

#### Scenario: ASCII terminal
- **WHEN** the terminal interface runs in ASCII mode
- **THEN** the chips still read `[x] Follow` and `[x] Auto-update`.
