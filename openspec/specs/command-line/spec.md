# Command Line Specification

## Purpose
Defines the startup arguments and options of the fasttail executable: files to open, workspace and filter options, renderer and configuration overrides, help and version output.
## Requirements
### Requirement: Command Line Paths and Options
The executable SHALL accept `fasttail [OPTIONS] [PATH...]`. Each PATH SHALL be opened as a stream after the workspace is restored, skipping paths already open and reporting missing files on stderr; relative paths SHALL be resolved against the current directory at startup. A PATH of exactly `-` SHALL mean standard input (see the stream-engine capability), before or after `--`; it SHALL be accepted at most once, a second `-` being a usage error, and a file named `-` SHALL be reachable as `./-`. When `-` is given but standard input is not a pipe or a redirected file, the application SHALL report it on stderr and open no standard-input stream. Without `-`, standard input that is a pipe or a redirected file SHALL be read the same way, the stream being created only once the first byte arrives. Options: `--fresh` (empty workspace), `--filter <text>`, `--exclude <text>` (applied to the streams opened from the command line, the standard-input stream included), `--follow` / `--no-follow`, `--renderer <auto|glow|wgpu|software>`, `--config <path>`, `--session <file>` (load this session file at startup, replacing the restored workspace without confirmation), `--version`, `--help`, and `--` to end option parsing. `--gui` SHALL be accepted and ignored: FastTail has been GUI-only since 0.7.1, and the flag is kept so existing shortcuts and scripts keep working. Unknown options or missing values SHALL print usage to stderr and exit with code 2; `--help` and `--version` SHALL print to the parent console and exit 0.

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

### Requirement: Headless Print Mode
The executable SHALL accept `fasttail --print [OPTIONS] [FILE...]`, which writes to standard output the lines of the inputs that pass the given filters and exits, without opening a window, restoring or saving the workspace, or writing `fasttail.ini`; the configuration SHALL only be read, for the theme and the highlight rules. Each input SHALL be a file, a directory pattern (read from its newest matching file), a single compressed file in gzip, bzip2, xz or zstd format (decompressed as it is read, nothing written to disk), or `-` for standard input; with no FILE, piped standard input SHALL be read, and a terminal as standard input SHALL be a usage error. A zip, tar or 7z archive SHALL be reported on standard error as not supported in print mode. `--filter <TEXT>` and `--exclude <TEXT>` SHALL each be accepted up to 8 times and SHALL combine as a stream's include and exclude terms, with `--regex` and `--case-sensitive` as the stream's toggles; `--level <LEVEL>` SHALL set the minimum level; `--since <TIME>` and `--until <TIME>` SHALL accept everything the time range popup accepts, with the same rules for a bare time and for the end of the unit typed, and relative times (`now`, or `-` followed by number-and-unit pairs with the units `s`, `m`, `h`, `d` and `w`, combined as in `-1h30m`, the syntax of `relative-time-windows`) counted back from the current local time, a relative time being an exact instant on both sides (a relative "to" is not widened to the end of a unit); `--context <N>` (0 to 100) SHALL print `N` lines before and after each match with a `--` line between groups that are not consecutive. The lines printed SHALL be exactly the lines the window shows for the same filters, stack-trace continuation lines following their entry. Inputs SHALL be printed one after the other in the order given; with more than one input each line SHALL be prefixed with the input's name and `:` unless `--no-prefix` is given, and `--line-numbers` SHALL prefix the line number and `:`. Memory use SHALL NOT grow with the size of the inputs: no line index SHALL be built, and the first matching line SHALL be printed as soon as it is read. The exit code SHALL be 0 when at least one line was printed, 1 when none was, 2 for a usage error, and 3 when an input could not be read, the others being printed. The options `--level`, `--context`, `--color`, `--line-numbers`, `--no-prefix`, `--regex` and `--case-sensitive` without `--print` SHALL be a usage error; `--since` and `--until` without `--print` SHALL set the time window of the streams opened in the window with the same syntax, a relative time being turned into the instant it names at start (with its milliseconds, exact on both sides) until `relative-time-windows` makes it slide. Without `--print`, `--filter` and `--exclude` SHALL keep their previous behaviour: given more than once, the last one SHALL count, and the limit of 8 terms SHALL NOT apply.

#### Scenario: Errors of the last hour
- **WHEN** the user runs `fasttail --print --level error --since -1h app.log`
- **THEN** standard output receives the ERROR and FATAL lines of `app.log` stamped in the last hour, with their stack-trace lines, no window opens and the exit code is 0.

#### Scenario: Nothing matches
- **WHEN** the user runs `fasttail --print --filter no-such-text app.log`
- **THEN** nothing is written to standard output and the exit code is 1.

#### Scenario: Several terms and inputs
- **WHEN** the user runs `fasttail --print --filter payment --filter timeout --exclude healthcheck gateway.log payment.log.1.gz`
- **THEN** the lines containing both `payment` and `timeout` and not `healthcheck` are printed, first those of `gateway.log`, then those of the decompressed `payment.log.1.gz`, each prefixed with its file name and `:`.

#### Scenario: In a pipe
- **WHEN** the user runs `kubectl logs pod-7 | fasttail --print --exclude DEBUG | wc -l`
- **THEN** the count of the non-DEBUG lines is printed and FastTail exits when its input ends.

#### Scenario: Same result as the window
- **WHEN** a log is printed with `--filter ERROR --since 14:02 --until 14:05` and the same log is opened in the window with the same include term and time range and exported with "Export visible lines..."
- **THEN** the two outputs are identical.

#### Scenario: Multi-gigabyte input
- **WHEN** the user runs `fasttail --print --filter ERROR` on a 20 GB log
- **THEN** the first matching line is printed before the rest of the file is read and the memory of the process stays below 64 MB.

#### Scenario: Print-only option without print
- **WHEN** the user runs `fasttail --level error app.log`
- **THEN** usage is printed to standard error and the exit code is 2.

### Requirement: Print Mode Output and Colours
`--color <auto|always|never>` SHALL control colour in print mode, `auto` being the default and meaning colour only when standard output is a terminal and the `NO_COLOR` environment variable is not set. Without colour, each line SHALL be written without any escape sequence, the log's own ANSI sequences removed. With colour, each line SHALL be written with the level colour of the configured theme, the highlight rules of `fasttail.ini` in priority order (captures-only rules colouring only their groups) and the log's own ANSI colours below them, as SGR sequences in 24-bit colour when the terminal announces it through `COLORTERM` and on Windows 10 and later, and in the nearest of the 256 xterm colours otherwise. A closed standard output (a reader such as `head` exiting) SHALL end the program quietly with exit code 0. On Windows, output SHALL go to the redirected standard output when there is one, and otherwise to the console of the parent process, attached as for `--help`, with virtual-terminal processing enabled for colour and colour turned off when it cannot be enabled.

#### Scenario: Colours in a terminal
- **WHEN** a rule paints `timeout` red and the user runs `fasttail --print app.log` in a terminal
- **THEN** the lines containing `timeout` are printed in red and the ERROR lines in the theme's error colour.

#### Scenario: No colours in a file
- **WHEN** the user runs `fasttail --print app.log > out.txt`
- **THEN** `out.txt` contains no escape sequence, even for lines that were coloured in the log.

#### Scenario: Windows console
- **WHEN** the user runs `fasttail --print --filter ERROR app.log | findstr payment` in `cmd.exe`
- **THEN** the matching lines reach `findstr` through the pipe and no window opens.

### Requirement: Print Mode Follow
With `--follow`, print mode SHALL print the existing content of every input and then keep printing lines appended to them, within 500 milliseconds of their being written, until it is interrupted (`CTRL + C` SHALL end it with the exit code earned so far) or, for standard input, until the input ends. A file that is truncated or rewritten SHALL be read again from its start with a notice on standard error, a directory pattern SHALL switch to a newer matching file with a notice; in both cases the last line of the old content still waiting for its newline SHALL first go through the filters as a complete line, and the notice SHALL be written only once the file to read could be opened (a file that cannot be opened yet SHALL be tried again at the next check without a new notice), and a compressed input SHALL not be followed, which is said on standard error. Inputs SHALL be opened without preventing the writer from appending, renaming or deleting them, on every platform.

#### Scenario: Following a growing log
- **WHEN** `fasttail --print --follow --filter ERROR app.log` is running and the writer appends an ERROR line
- **THEN** the line is printed within 500 milliseconds and FastTail keeps running.

#### Scenario: Rotation while following
- **WHEN** `fasttail --print --follow app.log` is running and the logger truncates `app.log` and starts writing it again
- **THEN** standard error says that the file was truncated and the new lines are printed from the start of the file.

