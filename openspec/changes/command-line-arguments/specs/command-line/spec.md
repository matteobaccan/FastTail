## ADDED Requirements

### Requirement: Command Line Paths and Options
The executable SHALL accept `fasttail [OPTIONS] [PATH...]`. Each PATH (a file or a directory plus wildcard pattern) SHALL be opened as a stream after the workspace is restored, skipping paths already open. Options: `--fresh` (empty workspace), `--session <file>`, `--filter <text>`, `--exclude <text>` (applied to the streams opened from the command line), `--follow` / `--no-follow`, `--config <path>`, `--version`, `--help`. Unknown options SHALL print usage to stderr and exit with code 2; `--help` and `--version` SHALL print to the parent console and exit 0.

#### Scenario: Opening two files from a shell
- **WHEN** the user runs `fasttail app.log err.log` with a saved workspace holding `other.log`
- **THEN** the window opens with `other.log`, `app.log` and `err.log` as streams.

#### Scenario: Clean start with a filter
- **WHEN** the user runs `fasttail --fresh --filter ERROR app.log`
- **THEN** only `app.log` is open and its include filter is `ERROR`.

#### Scenario: Version from a terminal on Windows
- **WHEN** the user runs `fasttail --version` in PowerShell
- **THEN** the version string is printed in that console and no window opens.
