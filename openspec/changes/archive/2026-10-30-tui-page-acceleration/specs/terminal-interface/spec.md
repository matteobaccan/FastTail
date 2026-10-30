## ADDED Requirements

### Requirement: Terminal Page Acceleration
The terminal interface SHALL move further per press while `PgUp` or `PgDn` is held, doubling each second up to 8 pages, unless `page_acceleration` is off.

#### Scenario: Held for 2 s
- **WHEN** the user holds `PgDn` for two seconds
- **THEN** each press then moves four pages.
