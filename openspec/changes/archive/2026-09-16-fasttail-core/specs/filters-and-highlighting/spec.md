## ADDED Requirements

### Requirement: Live Include and Exclude filter tray
The application SHALL provide a filter bar supporting simultaneous include (whitelist) and exclude (blacklist) patterns with regex and literal string options, filtering log streams in real-time.

#### Scenario: User enters include pattern
- **WHEN** the user types `ERROR` in the Include filter input
- **THEN** only lines matching `ERROR` are shown in the active stream view.

#### Scenario: User combines include and exclude patterns
- **WHEN** the user sets Include to `ERROR` and Exclude to `healthcheck`
- **THEN** lines containing `ERROR` but not containing `healthcheck` are displayed in the view.

### Requirement: Multi-pattern color highlighting with priority
The application SHALL allow users to define multiple text or regex highlighting rules, each with custom foreground and background colors, evaluated in ordered priority.

#### Scenario: Multiple rules match the same line
- **WHEN** a log line matches two highlight rules (e.g. `ERROR` and `CRITICAL`)
- **THEN** the rule positioned higher in the priority list determines the styling for that line or token.

### Requirement: Incremental search and match navigation
The application SHALL support incremental search as the user types, highlighting occurrences and providing keyboard shortcuts (`F3` / `Shift+F3`) to jump forward and backward between matches.

#### Scenario: Pressing F3 to jump to next match
- **WHEN** the user presses `F3` while a search query is active
- **THEN** the viewport scrolls to bring the next matching line into focus and highlights the search term.
