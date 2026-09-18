## ADDED Requirements

### Requirement: Timestamp Range Filter
The filter panel SHALL offer "from" and "to" time inputs accepting `HH:MM[:SS]`, `YYYY-MM-DD HH:MM[:SS]` or ISO 8601, either side optional. Only lines whose detected timestamp (or inherited timestamp for continuation lines) lies inside the range SHALL be visible, combined with include/exclude filters. The stream status bar SHALL show the time span of the visible lines. The controls SHALL be disabled with a hint when fewer than half of the sampled lines carry a recognised timestamp.

#### Scenario: Three-minute window
- **WHEN** the user enters from `14:02` to `14:05` on a log of the current day
- **THEN** only lines timestamped between 14:02:00.000 and 14:05:59.999 are visible, including stack-trace lines that follow an entry in that window.
