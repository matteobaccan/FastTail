# Command Line Specification

## Purpose
Defines the startup arguments and options of the fasttail executable: files to open, workspace and filter options, renderer and configuration overrides, help and version output.
## Requirements
### Requirement: Command Line Paths and Options
The executable SHALL accept `fasttail [OPTIONS] [PATH...]`. Each PATH SHALL be opened as a stream after the workspace is restored, skipping paths already open and reporting missing files on stderr; relative paths SHALL be resolved against the current directory at startup. Options: `--fresh` (empty workspace), `--filter <text>`, `--exclude <text>` (applied to the streams opened from the command line), `--follow` / `--no-follow`, `--renderer <auto|glow|wgpu|software>`, `--config <path>`, `--session <file>` (load this session file at startup, replacing the restored workspace without confirmation), `--version`, `--help`, and `--` to end option parsing. `--gui` SHALL be accepted and ignored: FastTail has been GUI-only since 0.7.1, and the flag is kept so existing shortcuts and scripts keep working. Unknown options or missing values SHALL print usage to stderr and exit with code 2; `--help` and `--version` SHALL print to the parent console and exit 0.

#### Scenario: Opening two files from a shell
- **WHEN** the user runs `fasttail app.log err.log` with a saved workspace holding `other.log`
- **THEN** the window opens with `other.log`, `app.log` and `err.log` as streams.

#### Scenario: Clean start with a filter
- **WHEN** the user runs `fasttail --fresh --filter ERROR app.log`
- **THEN** only `app.log` is open and its include filter is `ERROR`.

#### Scenario: Version from a terminal on Windows
- **WHEN** the user runs `fasttail --version` in PowerShell
- **THEN** the version string is printed in that console and no window opens.

#### Scenario: Session from the command line
- **WHEN** the user runs `fasttail --session incident.fasttail-session.ini`
- **THEN** the window opens with that session's streams and layout, and the title bar shows `incident`.

