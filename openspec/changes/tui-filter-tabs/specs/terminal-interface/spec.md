## ADDED Requirements

### Requirement: Terminal Filter Tabs
The terminal interface SHALL open the focused stream's filters as a new tab of the same file and SHALL restore the filter tabs saved by the window.

#### Scenario: Errors as a tab
- **WHEN** the include filter is `ERROR` and the user presses `Alt+T`
- **THEN** a new tab shows only the ERROR lines, numbered by their lines in the source, and the original stream keeps its filter.

#### Scenario: Back to the source
- **WHEN** the cursor is on a row of a filter tab and the user presses `Ctrl+K`
- **THEN** the source stream is focused and shows that line in context.
