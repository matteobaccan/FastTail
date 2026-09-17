## Purpose
Enables real-time filtering, search match traversal with F3/Shift+F3, active match highlighting, wrap-around audio alerts, and search history persistence.

## Requirements

### Requirement: Automatic Filter Activation
Filtering SHALL activate automatically whenever either the include or exclude filter input contains text, without requiring a manual mode switch button.

#### Scenario: User types in filter box
- **WHEN** the user enters a string in the include or exclude filter field
- **THEN** filtering immediately applies to the stream, and clearing both fields restores all lines.

### Requirement: Search Match Traversal with F3 and Shift+F3
The application SHALL support navigating search matches forward with `F3` (or Enter) and backward with `Shift+F3` (or Shift+Enter), automatically scrolling the viewport to center the matching line.

#### Scenario: User presses F3 to jump between matches
- **WHEN** the user has entered a search query and presses `F3`
- **THEN** the viewport scrolls to center the next match in the buffer, and the match counter increments.

### Requirement: Active Search Match Line Highlighting
The line corresponding to the current search match SHALL be distinctly highlighted from other matching lines.

#### Scenario: Active search match is visually distinct
- **WHEN** the search advances to a match on line N
- **THEN** line N is rendered with an accent border, background glow, and an indicator arrow (`▶`) beside the line number.

### Requirement: Wrap-Around Search Audio Alert
When search traversal reaches the end of the buffer and wraps around to the beginning, or reaches the beginning and wraps around to the end, the system SHALL emit an audio alert.

#### Scenario: Search wraps around buffer boundaries
- **WHEN** the user is at the last match and presses `F3`
- **THEN** the active match wraps to match 1 and the system emits an audio beep.

### Requirement: Persistent Search History (Last 10 Queries)
The application SHALL maintain a list of the 10 most recent search queries, persisted in `fasttail.ini`, accessible via a dropdown menu in the search toolbar.

#### Scenario: Re-running a previous search from history
- **WHEN** the user clicks the search history button (`🕒`) and selects a previous query
- **THEN** the search box is populated with that query and search matches are immediately highlighted.
