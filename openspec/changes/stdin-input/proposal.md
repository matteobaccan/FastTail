## Why

Many logs never exist as files: `kubectl logs -f pod`, `docker compose logs -f`, `journalctl -f`, `ssh host tail -f /var/log/app.log`, a build script's output. Today the user has to redirect them to a file and open it. `less`, lnav and klogg read standard input; `some-command | fasttail -` should work too, with every FastTail feature on the result.

## What Changes

- `fasttail -` reads standard input into a temporary spool file that is tailed like any stream (follow on, filters, search, levels, timestamps, highlight rules, bookmarks, export). The tab is titled `stdin`.
- Without `-`, piped or redirected input is detected (stdin is a pipe or a regular file, not a terminal or a console) and opens the same stream, created only once the first byte arrives, so a launcher that hands the process a silent pipe does not get an empty tab.
- When the producer ends, the stream stays open with an "input ended" notice.
- `-` given twice is a usage error; `-` with stdin attached to a terminal reports on stderr that nothing is piped and opens no stdin stream. A file literally named `-` is opened as `./-`.
- The spool is bounded: at `stdin_spool_max_mb` (default 2048) or when the spool volume falls under 512 MB free, the spool is restarted from empty, which the engine handles as a truncation, with a notice in the stream bar.
- The stdin stream is not saved in the workspace, recent files or sessions; its spool is deleted when the tab closes, at exit, and swept after a crash.
- The command-line requirement documents `-`.
- Windows: the executable is built with `#![windows_subsystem = "windows"]` (a GUI-subsystem program without a console). Redirected standard handles are still passed to such a process by the shell, so pipes are expected to work from `cmd.exe`, PowerShell and Git Bash, with caveats stated in the design and a manual verification matrix in the tasks.

## Capabilities

### New Capabilities
- (none)

### Modified Capabilities
- `command-line`: `-` as a PATH means standard input; piped input detected without `-`.
- `stream-engine`: adds the standard-input stream spooled to a temporary file.

## Impact

- `src/cli.rs`: `-` is recorded as `stdin: bool` instead of being resolved to `<cwd>/-` (today's behaviour); a second `-` is a usage error; USAGE text updated.
- New `src/stdin_source.rs`: stdin classification (Windows: `GetFileType` on `GetStdHandle(STD_INPUT_HANDLE)`; Unix: `fstat` on fd 0 plus `IsTerminal`), the reader thread copying stdin to the spool in chunks, cap and free-space handling, EOF state, wake callback.
- `src/spool.rs` (spool directory, naming, cleanup, crash sweep): shared with the `compressed-files` change; whichever lands first adds it.
- `src/tail_engine.rs`: origin `Stdin` (title, footer text, no workspace persistence); encoding and binary detection re-run once when a stream opened empty first holds 512 bytes (also needed by `compressed-files`).
- `src/ui/app.rs`: create the stdin stream at startup (immediately with `-`, on first byte when detected); skip it when saving workspace and sessions.
- `src/config.rs`: `stdin_spool_max_mb`.
- i18n: new keys in all 16 languages. README (usage examples per shell) and CHANGELOG.
- No new dependency (`sysinfo` for free space is already used; the Win32 calls are declared by hand as `AttachConsole` is in `main.rs`).
