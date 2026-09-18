## Why

The README promises that file paths can be passed on the command line, but `main.rs` never reads `std::env::args()`: `fasttail app.log` opens an empty workspace. Shell users, scripts and "Open with" integrations all rely on this, and SnakeTail also accepts a session file at startup.

## What Changes

- `fasttail [OPTIONS] [PATH...]`: each PATH (file or `dir/pattern`) is opened as a stream in addition to the restored workspace; `--fresh` starts with an empty workspace instead.
- `--session <file>` loads a named session (depends on `named-sessions`).
- `--filter <text>` / `--exclude <text>` apply to the streams opened from the command line; `--follow` / `--no-follow` set follow mode.
- `--config <path>` overrides `FASTTAIL_CONFIG`; `--version` and `--help` print and exit (on Windows the console is attached when present).
- Unknown options print usage to stderr and exit with code 2.

## Capabilities

### New Capabilities
- `command-line`: startup arguments and options.

### Modified Capabilities
- (none)

## Impact

- `src/cli.rs`: a hand-written parser (no `clap`, to keep the binary and compile time small) returning a `CliArgs` struct; unit tests.
- `src/main.rs`: parse before building the viewport, attach console on Windows for `--help`/`--version`, pass `CliArgs` to `FastTailApp::new`.
- `src/ui/app.rs`: open the requested streams after restoring the workspace; `README.md`: usage section.
