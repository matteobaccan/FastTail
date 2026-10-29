## ADDED Requirements

### Requirement: Terminal Rule Sets, Formats and Filter Expressions
The terminal interface SHALL import and export rule sets, walk the lines of a rule, clear all bookmarks, edit log formats in a dialog, offer the editor presets and Open in FastTail for a source location, and toggle a stream's include side between terms and an expression.

#### Scenario: Rule set import
- **WHEN** the user imports `team.fasttail-rules.ini` in the terminal rule editor
- **THEN** its rules are added and colour the rows, as the window's import does.

#### Scenario: Expression toggle
- **WHEN** the user toggles ƒx on a stream with include terms `a` and `b`
- **THEN** the include side becomes the expression `a AND b`, editable in the prompt.
