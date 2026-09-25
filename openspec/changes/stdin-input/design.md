## Context

The engine only opens regular files: `open_impl` refuses anything else ("prevents opening … FIFOs/named pipes"), and every feature relies on random access through `FileSource` and 8-byte line offsets. A pipe is sequential and cannot be re-read. `cli.rs` currently treats `-` as a positional path and resolves it to `<cwd>/-`. `main.rs` starts with `#![windows_subsystem = "windows"]` unconditionally, so on Windows FastTail is a GUI-subsystem program in every build; `attach_parent_console()` (`AttachConsole(ATTACH_PARENT_PROCESS)`) is called only for `--help`, `--version` and usage errors so their output reaches the terminal.

## Goals / Non-Goals

**Goals:**
- `cmd | fasttail -` works on Linux, macOS and Windows with every stream feature.
- Piped input without `-` works when it is unambiguous.
- Bounded disk use, no leftovers, no empty tabs from launchers.

**Non-Goals:**
- Writing to stdout (FastTail is not a filter in a pipeline).
- Several stdin streams, or reading named pipes / FIFOs given as paths (still refused).
- Restoring a stdin stream after restart.
- Interactive input from a terminal.

## Decisions

### D1. Spool stdin to a temporary file and tail it
A reader thread copies stdin to a spool file (64 KB reads, each written and flushed immediately so line-buffered producers appear at once) and calls the app's wake callback after each write. The engine opens the spool as a regular file and follows it through its normal poll path: appended bytes are indexed incrementally, a partial last line is re-scanned on the next append, filters and search run on appended lines. Nothing in the engine learns about pipes.

*Alternatives:* (a) an in-memory ring buffer — breaks the rule that the engine does not hold content in memory, and every scan job, HEX view and export would need a second source type; (b) teaching `FileSource` to read a pipe — impossible to seek back.

The spool directory, naming (`<pid>-<counter>-stdin`), delete-on-close and the startup sweep of spools whose process is gone are the ones of `src/spool.rs` introduced for compressed files (same decisions; whichever change is implemented first adds the module).

### D2. What counts as piped input
Classification of standard input at startup:

| Platform | Pipe / redirected file → **piped** | Terminal / console → **terminal** | Absent → **none** |
|---|---|---|---|
| Windows | `GetFileType(h)` = `FILE_TYPE_PIPE` or `FILE_TYPE_DISK` | `FILE_TYPE_CHAR` | handle NULL or `INVALID_HANDLE_VALUE` |
| Unix | `fstat(0)`: `S_IFIFO`, `S_IFSOCK` or `S_IFREG` | `isatty(0)` | fd 0 closed; `/dev/null` and other character devices count as none |

- `-` and piped → stream created immediately (empty until data arrives).
- `-` and terminal/none → `fasttail: standard input is not a pipe; nothing to read` on stderr (after `attach_parent_console` on Windows) and no stdin stream; the rest of the startup continues, as for a missing file.
- no `-` and piped → reader started, the stream (tab) created when the first byte arrives; EOF with zero bytes creates nothing.
- no `-` and terminal/none → nothing.

The lazy creation in the auto case protects against launchers (IDE run configurations, some desktop environments, service managers) that give the process a pipe nobody writes to: the only cost is one blocked reader thread.

### D3. End of input and producer lifetime
EOF (read returns 0) marks the stream `input ended`, shown in the stream bar with the line count; follow stays on so the last lines remain in view. A read error is shown the same way with the error text. Closing the stdin tab closes our read end: the producer gets a broken pipe on its next write (EPIPE/SIGPIPE on Unix, `ERROR_NO_DATA` on Windows), which is the normal way a pager ends a pipeline. The reader thread is detached; at exit the process ends regardless of a blocked read.

### D4. Bounded spool
`stdin_spool_max_mb` (default 2048, 64–65536, `fasttail.ini` and Settings). When the spool reaches the cap, or free space on its volume falls below 512 MB (checked every 64 MB via `sysinfo::Disks`), the writer truncates the spool to zero and continues. The engine detects the size drop as a truncation — an existing, tested path — and resets its index, caches, selection and bookmarks; the stream bar says that earlier input was discarded because of the limit. Blocking the pipe instead was rejected: it would stall `kubectl logs -f` or a CI job behind a log viewer.

### D5. Identity and persistence
The stream's identity is the pseudo-path `<stdin>`: tab title `stdin`, footer `standard input (spooled to <spool path>)`. It is skipped when the workspace, recent files and sessions are written (saving a session with a stdin stream open names it in the summary of streams not saved). Export works, and is how a user keeps the content. `--filter`, `--exclude`, `--follow` / `--no-follow` apply to it like to the other streams opened from the command line.

### D6. Windows: GUI subsystem and pipes — what is known and what is not
**What holds from the Win32 model.** The subsystem decides whether Windows creates or attaches a console at start-up; it does not decide the standard handles. A shell that runs `producer | fasttail -` creates an anonymous pipe and starts `fasttail.exe` with `STARTF_USESTDHANDLES` and the pipe's read end as `hStdInput`; this works the same for GUI-subsystem executables, and `GetStdHandle(STD_INPUT_HANDLE)` returns that pipe. Rust's `std::io::stdin()` reads a non-console handle with `ReadFile`, returning raw bytes (the UTF-8 validation it applies is for console handles only), and reports a missing handle as end of file rather than an error. So reading a pipe from a `windows_subsystem = "windows"` binary is expected to work without code specific to the subsystem. `AttachConsole` is not needed for reading and must not be relied on for it; it is still used only to print diagnostics.

**What is uncertain and must be verified on real shells (task 5):**
- *cmd.exe*: whether cmd waits for a GUI-subsystem program at the end of a pipeline or returns to the prompt at once. Either way the pipe stays connected and the producer keeps writing; only the prompt behaviour differs.
- *Windows PowerShell 5.1 and PowerShell 7.x*: PowerShell decides itself whether to redirect a native program's input and whether to wait for GUI programs; whether `kubectl logs -f pod | fasttail -` feeds stdin to a GUI-subsystem program has to be tested, not assumed. In Windows PowerShell 5.1 text piped to a native program is re-encoded with `$OutputEncoding` (US-ASCII by default), so non-ASCII characters can arrive as `?`; PowerShell 7.4+ passes the bytes between two native programs unchanged. If PowerShell does not connect the pipe, the documented fallback is `cmd /c "producer | fasttail -"`.
- *Git Bash / MSYS2*: its pipes are Windows anonymous pipes, expected to behave like cmd's.
- *Windows Terminal / ConPTY*: without redirection stdin is a console handle (`FILE_TYPE_CHAR`) or absent, classified as terminal/none by D2 either way.
- *Ctrl+C* in the console reaches the producer, not FastTail (which is not attached to that console); the producer exits, FastTail sees EOF and keeps the window open. Expected, to be confirmed.

The README will state what was verified per shell, and anything not verified will be listed as not guaranteed.

*Alternatives considered:* building FastTail as a console-subsystem program and calling `FreeConsole` — a console window flashes when launched from Explorer or a shortcut, which the GUI-only design since 0.7.1 avoided; shipping a separate console-subsystem launcher (`fasttail-pipe.exe`) that forwards its stdin — a second binary in the release archive for a case the plain binary is expected to cover. Both stay available if verification fails.

### D7. `-` in the parser
`CliArgs` gains `stdin: bool`; `-` sets it (before and after `--`, as in `cat`), and a second `-` returns `CliError::Usage("standard input given twice")`, exit code 2. A file literally named `-` is reachable as `./-`. `paths` no longer contains `<cwd>/-`.

## Risks / Trade-offs

- [Unverified PowerShell behaviour] → verification matrix before release; README states the results and the `cmd /c` fallback.
- [Spool restart loses earlier input] → only at the cap or on low disk, announced in the stream bar; the cap is configurable.
- [Silent pipe from a launcher] → lazy tab creation in auto mode; one idle thread.
- [Binary data piped in] → the binary heuristic, re-run once 512 bytes exist, switches the stream to HEX as for files.
- [Very high input rate] → writing is sequential and the engine indexes appended bytes incrementally; tested at 50 MB/s with a synthetic producer.
