# external-tools Specification

## Purpose
Defines user-configured external tools: commands launched from a row (context menu, stream menu, shortcut) with placeholders expanded per argument and no shell by default, and tools bound to highlight rules that run on matching appended lines under a throttle and a children cap.

## Requirements
### Requirement: Configurable External Tools
Settings SHALL let the user define external tools with a name, a command, an argument list with the placeholders `{line}` (the row text), `{file}` (the tailed file, the resolved file of a pattern stream, the archive of a compressed stream), `{dir}` (its directory), `{lineno}` (1-based), `{selection}` (the selected rows as text, or the row itself when nothing is selected) and `{match}` (the first capture group of the tool's own optional regex applied to the row, the whole match when the regex has no group, empty when it does not match), an optional keyboard shortcut (at least one modifier) and an optional "run via shell" flag (off by default). The argument list SHALL be split like a command line (quotes group words) and each placeholder SHALL be expanded once per argument, so a value is never expanded again. Without the flag every expanded argument SHALL reach the program as its own argv entry, with no shell involved. With the flag the program SHALL be run through `cmd /c` (Windows) or `sh -c` with every expanded argument quoted for that shell, so shell operators written in the argument list are quoted as well and a pipeline belongs in a script; the Windows quoting is weaker than the POSIX one, so the flag SHALL be documented as meant for trusted logs only. Tools SHALL run with stdin, stdout and stderr detached, so a tool reads its input from its arguments. Tools SHALL be listed in the row context menu and the stream menu, and SHALL be persisted in `fasttail.ini`.

#### Scenario: Opening the editor on the current row
- **WHEN** a tool `code -g {file}:{lineno}` is defined and the user runs it on row 120 of `app.log`
- **THEN** the editor is launched with `app.log:120` as one argument.

#### Scenario: Hostile line content
- **WHEN** the row text is `x; rm -rf /` and a tool with `{line}` is run without the shell flag
- **THEN** the text is passed as a single argument and no shell command is executed.

#### Scenario: Match without a capture group
- **WHEN** a tool's regex is `https?://\S+` and the row contains `see https://example.com/x`
- **THEN** `{match}` expands to `https://example.com/x`.

#### Scenario: The settings say what tools are for
- **WHEN** the user opens the External tools section
- **THEN** the placeholder list is accompanied by a link to the external tools cookbook (`docs/external-tools-cookbook.md`), which holds worked recipes and the safety habits that go with them.

### Requirement: Tools Bound to Highlight Rules
A tool MAY be bound to a highlight rule; it SHALL run when the rule matches an appended line, at most once per second per tool and with at most 10 concurrent child processes, dropped runs being counted and shown in the tool's settings row.

#### Scenario: Notify on critical error
- **WHEN** a tool `notify-send "{line}"` is bound to the rule `FATAL` and three FATAL lines arrive within a second
- **THEN** the tool runs once and the settings row shows two dropped runs.

