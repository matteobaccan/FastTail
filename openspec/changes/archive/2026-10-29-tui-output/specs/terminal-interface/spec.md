## ADDED Requirements

### Requirement: Terminal Search Pane, Scope and Export
The terminal interface SHALL offer the search results pane, search in a range, Export of visible lines, selected lines or matches as text, CSV or HTML through the window's writers, Copy as shown, and the bookmark report.

#### Scenario: CSV export over SSH
- **WHEN** the user exports the matches of `ERROR` as CSV to `errors.csv` in the terminal
- **THEN** the file holds the same rows and columns as the window's export of the same stream and search.

#### Scenario: Search in a range
- **WHEN** the user limits the search to lines `100-200`
- **THEN** the counter shows only the hits in that range and `n` wraps inside it.
