## ADDED Requirements

### Requirement: Terminal Filter Tabs
The terminal interface SHALL open the focused stream's filters as a new tab of the same file and SHALL restore the filter tabs saved by the window.

#### Scenario: Errors as a tab
- **WHEN** the include filter is `ERROR` and the user presses `Alt+T`
- **THEN** a new tab shows only the ERROR lines and the original stream shows every line again.
