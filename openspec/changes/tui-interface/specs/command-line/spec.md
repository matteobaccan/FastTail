## MODIFIED Requirements

### Requirement: Command Line Paths and Options
The executable SHALL accept `fasttail [OPTIONS] [PATH...]`. Each PATH SHALL be opened as a stream after the workspace is restored, skipping paths already open and reporting missing files on stderr; relative paths SHALL be resolved against the current directory at startup. A PATH of exactly `-` SHALL mean standard input (see the stream-engine capability), before or after `--`; it SHALL be accepted at most once, a second `-` being a usage error, and a file named `-` SHALL be reachable as `./-`. When `-` is given but standard input is not a pipe or a redirected file, the application SHALL report it on stderr and open no standard-input stream. Without `-`, standard input that is a pipe or a redirected file SHALL be read the same way, the stream being created only once the first byte arrives. Options: `--fresh` (empty workspace), `--filter <text>`, `--exclude <text>` (applied to the streams opened from the command line, the standard-input stream included), `--follow` / `--no-follow`, `--renderer <auto|glow|wgpu|software>`, `--config <path>`, `--session <file>` (load this session file at startup, replacing the restored workspace without confirmation), `--tui` (start the terminal interface, see the terminal-interface capability), `--gui` (start the graphical interface even when `interface=tui` is set in `fasttail.ini`), `--version`, `--help`, and `--` to end option parsing. `--tui` and `--gui` together SHALL be a usage error. The terminal options `--ascii`, `--no-mouse`, `--split`, `--search <text>` and `--theme <tron|matrix|blade|light>` SHALL be accepted by every FastTail executable, used by the terminal interface and ignored by the graphical one; on Windows `fasttail.exe` SHALL pass them on to `fasttail-tui.exe`. Unknown options or missing values SHALL print usage to stderr and exit with code 2; `--help` and `--version` SHALL print to the parent console and exit 0.

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

#### Scenario: Piping a command into FastTail
- **WHEN** the user runs `kubectl logs -f pod-7 | fasttail --filter ERROR -`
- **THEN** a `stdin` stream opens and follows the command's output, showing only lines containing `ERROR`.

#### Scenario: Standard input given twice
- **WHEN** the user runs `fasttail - -`
- **THEN** usage is printed to stderr and the process exits with code 2.

#### Scenario: Dash without piped input
- **WHEN** the user runs `fasttail - app.log` from a terminal without redirecting standard input
- **THEN** stderr says that standard input is not a pipe, and the window opens with `app.log` and no `stdin` stream.

#### Scenario: --gui overrides interface=tui
- **WHEN** `fasttail.ini` holds `interface=tui` and the user runs `fasttail --gui app.log`
- **THEN** the graphical window opens with `app.log`, and `fasttail.ini` still holds `interface=tui`.

#### Scenario: Both interface options
- **WHEN** the user runs `fasttail --tui --gui`
- **THEN** usage is printed to stderr and the process exits with code 2.

#### Scenario: Terminal options passed on by the Windows hand-off
- **WHEN** the user runs `fasttail.exe --tui --ascii --filter ERROR app.log` with `fasttail-tui.exe` next to it
- **THEN** `fasttail-tui.exe` starts in a new console with `--ascii --filter ERROR app.log`, draws ASCII borders and shows only `ERROR` rows of `app.log`.

#### Scenario: Terminal options ignored by the GUI
- **WHEN** the user runs `fasttail --no-mouse app.log` with `interface=gui`
- **THEN** the graphical window opens with `app.log` and the mouse works as usual.

## ADDED Requirements

### Requirement: Terminal Executable
Where `fasttail-tui` is built, it SHALL accept the same paths and options as `fasttail`, SHALL always start the terminal interface, SHALL ignore `interface` in `fasttail.ini` and accept `--tui` as a no-op, and SHALL reject `--gui` with exit code 2 and a message naming `fasttail` (`fasttail.exe` on Windows) as the graphical executable. `--renderer` SHALL be accepted and ignored. Its `--help` SHALL list the terminal options and the environment variables `FASTTAIL_TUI_COLORS` and `FASTTAIL_TUI_ASCII`. On Windows it SHALL be a console-subsystem executable, so a shell waits for it.

#### Scenario: Help of the terminal executable
- **WHEN** the user runs `fasttail-tui.exe --help` in cmd
- **THEN** usage with `--ascii`, `--no-mouse`, `--split`, `--search`, `--theme`, `FASTTAIL_TUI_COLORS` and `FASTTAIL_TUI_ASCII` is printed in that console, the exit code is 0, and cmd shows its prompt only afterwards.

#### Scenario: --gui given to the terminal executable
- **WHEN** the user runs `fasttail-tui.exe --gui app.log`
- **THEN** stderr says to run `fasttail.exe` for the graphical interface and the exit code is 2.
