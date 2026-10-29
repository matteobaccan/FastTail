## ADDED Requirements

### Requirement: Terminal Analysis Tabs
The terminal interface SHALL offer the Statistics, Query, Patterns, Explain and Operations tabs over the same engine jobs as the window, as dockable panels with sortable, scrollable lists, and an Enter on a row SHALL filter on its value or go to its line as the window's click does.

#### Scenario: Statistics of a field
- **WHEN** a logfmt stream is open in the terminal and the user opens the Statistics of `status`
- **THEN** the panel lists `200`, `404` and `500` with their counts and shares, and Enter on `500` adds the include term `status=500`.

#### Scenario: Query
- **WHEN** the user runs `where status>=500 | stats count() by path` in the Query tab
- **THEN** the result is a table of paths and counts, sortable with `s`.
