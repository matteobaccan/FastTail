# external-tools Specification

## Purpose
Defines user-configured external tools: commands launched from a row (context menu, stream menu, shortcut) with placeholders expanded per argument and no shell by default, and tools bound to highlight rules that run on matching appended lines under a throttle and a children cap.

## Requirements
### Requirement: Configurable External Tools
Settings SHALL let the user define external tools with a name, a command, an argument list with the placeholders `{line}`, `{file}`, `{lineno}`, `{selection}` and `{match}`, an optional keyboard shortcut and an optional "run via shell" flag (off by default). Placeholders SHALL be expanded per argument and passed without shell interpretation unless the flag is set. Tools SHALL be listed in the row context menu and the stream menu, and SHALL be persisted in `fasttail.ini`.

#### Scenario: Opening the editor on the current row
- **WHEN** a tool `code -g {file}:{lineno}` is defined and the user runs it on row 120 of `app.log`
- **THEN** the editor is launched with `app.log:120` as one argument.

#### Scenario: Hostile line content
- **WHEN** the row text is `x; rm -rf /` and a tool with `{line}` is run without the shell flag
- **THEN** the text is passed as a single argument and no shell command is executed.

### Requirement: Tools Bound to Highlight Rules
A tool MAY be bound to a highlight rule; it SHALL run when the rule matches an appended line, at most once per second per tool and with at most 10 concurrent child processes, dropped runs being counted and shown in the tool's settings row.

#### Scenario: Notify on critical error
- **WHEN** a tool `notify-send "{line}"` is bound to the rule `FATAL` and three FATAL lines arrive within a second
- **THEN** the tool runs once and the settings row shows two dropped runs.

