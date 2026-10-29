## ADDED Requirements

### Requirement: Terminal Time Delta and Time Display
The terminal interface SHALL draw the time delta column when it is on for the stream, SHALL show timestamps as written, in UTC or in local time as chosen, and its listener prompt SHALL mention every listener scheme it accepts.

#### Scenario: Time delta
- **WHEN** Δt is on and two lines are 1.2 s apart
- **THEN** the second row shows `+1.2 s` before its text.

#### Scenario: UTC display
- **WHEN** a stream's lines are written in local time and the user chooses UTC
- **THEN** the rows show the timestamps converted to UTC.
