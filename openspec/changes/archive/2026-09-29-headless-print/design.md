## Context

`src/main.rs` is `#![windows_subsystem = "windows"]`; `parse_command_line` calls
`attach_parent_console()` (`AttachConsole(ATTACH_PARENT_PROCESS)`) before printing
`--help`, `--version` or a usage error, then exits. `src/cli.rs` is a hand-written parser
(`CliArgs`, `USAGE`); `--filter` / `--exclude` hold one value each. The matching logic is
UI-free already: `FilterSpec` (`build`, `with_levels`, `visible_in_sequence`) in
`src/scan_job.rs`, `timestamp::detect_timestamp` / `parse_user_time` /
`end_of_typed_time` / `local_now_millis`, `log_level::detect_level`, `ansi::strip` /
`strip_and_style`, `compressed::sniff` / `open_decoder`, `wildcard` for patterns,
`file_source::open_file_shared`, and `CompiledHighlight` for rules. `TailEngine` is the
window's model: it builds a line index (8 bytes per line) before anything is shown and
carries view state. `stdin_source::classify` tells a pipe from a terminal on both
platforms. The `tui-interface` proposal (PR #132, not merged) plans a `gui` / `tui` Cargo
feature split and, on Windows, a console-subsystem `fasttail-tui.exe` next to the GUI
`fasttail.exe`.

## Goals / Non-Goals

**Goals:**
- One command that prints exactly the lines the window would show for the same filters,
  on any file size, in constant memory, starting to print at once.
- Usable in pipes on every platform, with colours only when wanted.
- Code the 0.20.0 terminal interface can reuse unchanged.

**Non-Goals:**
- Archives with entries, output formats, a new console executable in 0.13.0 (see the
  proposal).

## Decisions

1. **A streaming pipeline, not a `TailEngine`.** `print_mode::run(args) -> ExitCode`
   reads each input once, front to back, through a `BufRead` (a shared-read file, a
   decoder from `compressed::open_decoder`, or standard input), splits lines with
   `memchr`, decodes them with the stream's detected encoding (the same 512-byte sniff),
   strips ANSI sequences for matching (render-mode semantics) and evaluates
   `FilterSpec::visible_in_sequence`, the level stage and the time window on the fly. The
   timestamp of a line without one is inherited from the previous line, as
   `JobSpec::Timestamps` does, so a stack trace follows its entry through `--since`. A
   bare `--since 14:02` refers to the day of the input's first timestamped line, whether
   or not that line passes the filters, as in the popup; a relative time (`now`, `-15m`,
   `-1h30m`, `-1w`: `timestamp::parse_relative`, shared with `relative-time-windows`) is
   an exact instant on both sides, a relative "to" not being widened to a unit's end;
   lines before the first timestamped line are dropped while a window is set, as in the
   window. *Rejected:* opening a `TailEngine` per input — it indexes the whole file before
   the first line can be printed (minutes on a 20 GB log, 8 bytes per line of memory),
   drags in view state and egui types, and its incremental paths are built around a UI
   frame loop. The matching functions are the same, which is what "the same engine" must
   mean for the results to agree; a test compares both on fixtures.
2. **Context lines** use a ring buffer of the last `N` lines and a countdown of `N` lines
   after each match, printing `--` between non-adjacent groups (the grep convention, since
   this is text output). `N` is capped at 100 as in the window.
   *Rejected:* printing context without separators: in a text stream the reader could
   not tell where one group ends, which the window shows with its separator rule.
3. **Output.** Each line is written as the stripped text (`--color never`) or with colour:
   level colours from the configured theme (`CyberTheme::level_style`), then highlight
   rules from `fasttail.ini` (`CompiledHighlight`, first rule wins, captures-only spans),
   then the line's own ANSI colours below them, re-emitted as SGR sequences. 24-bit colour
   when `COLORTERM` is `truecolor` / `24bit` or on Windows 10 and later, otherwise the
   nearest xterm-256 index (`ansi::xterm_color` inverted). With several inputs each line
   is prefixed with the input's display name and `:` (grep's `-H`), unless `--no-prefix`;
   `--line-numbers` adds the 1-based line number and `:`. Output goes through a
   `BufWriter` of 64 KB, flushed after each batch in follow mode. A broken pipe
   (`| head`) ends the program quietly with code 0.
   *Rejected:* colouring only the level: rules are what users tuned, and the output
   would not look like the window.
4. **Several inputs** are printed one after the other in the order given (no merge by
   time: that is the `merged-timeline-view` change). With `--follow`, after the existing
   content of every input, appended lines are printed as they arrive, prefixed with their
   input when there are several.
   *Rejected:* interleaving the inputs by time: that is the merged timeline, with its own
   ordering rules; printing in order is what `grep file1 file2` does.
5. **Follow loop.** Per input: the offset reached and the fingerprint of the first
   64 bytes (the engine's `head_fingerprint` rule); a `notify` watcher plus a 250 ms size
   check. Growth: read from the offset to the last complete line. Shrink or fingerprint
   change: a notice on standard error (`fasttail: app.log truncated, reading from the
   start`) and read from byte 0. Pattern inputs rescan the directory every 2 s and switch
   to a newer match with a notice. Before either restart the last line still waiting for
   its newline goes through the filters as complete, and the notice is written only once
   the file to read is open (a file that cannot be opened yet is tried again at the next
   check, silently). Standard input ends the program when it ends; a
   compressed input is not followed (notice). `CTRL + C` ends the program with the code
   earned so far: a console control handler / `SIGINT` handler sets a flag the follow
   loop checks, which flushes standard output and returns. Files are opened with `file_source::open_file_shared`, so the writer is
   never locked out on Windows.
   *Rejected:* polling only: `notify` gives sub-100 ms latency on local disks, the
   250 ms size check covers network shares where events do not arrive.
6. **Configuration read-only.** The configuration is loaded (honouring `--config` and
   `FASTTAIL_CONFIG`) for the theme and the rules only; nothing is saved, no workspace is
   restored, no spool is created or swept, and the crash handler writes its log as usual.
   *Rejected:* ignoring the configuration: colours would not match the user's rules,
   and `--config` would mean nothing in print mode.
7. **Windows console.** Before any output, `print_mode` checks `GetStdHandle(STD_OUTPUT_HANDLE)`:
   when it is a valid pipe or file (redirected output), it is used as it is; when it is
   missing, `attach_parent_console()` is called and the console output reopened
   (`CONOUT$`). Standard error gets the same treatment. With colour on and a console
   output, `SetConsoleMode(ENABLE_VIRTUAL_TERMINAL_PROCESSING)` is set, falling back to
   no colour when it fails. Standard input is read from the inherited handle, as
   `stdin_source` already does.
   *Rejected:* calling `AttachConsole` unconditionally as `--help` does: when output is
   redirected, re-binding the standard handles to the console would send the lines to
   the screen instead of the file or pipe.
8. **Reuse by the 0.20.0 terminal interface.** `src/print_mode.rs` imports nothing from
   `ui`, `egui` or `eframe`, so the `tui-interface` feature split puts it in the shared
   core without edits. Planned reuse: `fasttail-tui --print …` is the same code in a
   console-subsystem executable (the clean path on Windows, see risks); when
   `fasttail-tui -` finds that its standard output is not a terminal, it can fall back to
   print mode like `less` does; the ANSI writer (truecolor → 256 → 16 degradation) is the
   colour back end the terminal interface needs anyway. The level and rule colour mapping
   moves with it. This is noted in PR #132 before it is merged.
   *Rejected:* waiting for 0.20.0 and building print mode inside the terminal
   interface: the feature is small and useful now, and building it UI-agnostic first
   is exactly the decoupling the terminal interface needs.

Threads and memory: one thread; memory is the read and write buffers (64 KB each), one
line (at most 1 MB, the engine's long-line cap applies with the same truncation marker),
`N` context lines and the compiled filters and rules; nothing grows with the file size.

## Risks / Trade-offs

- [An interactive `cmd.exe` or PowerShell does not wait for a GUI-subsystem program whose
  output is the console: the prompt comes back while lines are still being printed] →
  when the output is redirected or piped the shell waits, which covers scripts; the
  README documents `start /wait fasttail --print …` for the interactive console case;
  0.20.0's console `fasttail-tui.exe --print` removes the problem. See the open question.
- [Results differ from the window] → the same `FilterSpec`, timestamp and level code, and
  an integration test that compares `--print` output with the engine's
  `export_visible` for the same filters on the fixture logs.
- [`--since -3h` on a log written in UTC by a machine in another zone] → relative times
  are computed on the local clock, and timestamps are compared on the clock the log
  printed, as the window does; the README says so, and the `quick-wins-0-13` source zone
  can later be offered here as `--source-zone`.

## Migration Plan

Additive: without `--print` the command line behaves as before (`--filter` / `--exclude`
given more than once: the last one counts, no limit on how many times). `--since` /
`--until` without `--print` set the window's time range; a relative value is turned into
the instant it names at start, with its milliseconds, until `relative-time-windows` makes
it slide. The print-only options
(`--level`, `--context`, `--color`, `--line-numbers`, `--no-prefix`,
`--regex`, `--case-sensitive`) are usage errors without `--print`.

## Open Questions

- Ship a small console-subsystem `fasttail-print.exe` in the 0.13.0 Windows zip (the same
  crate, a second `[[bin]]` calling `print_mode::run`), instead of waiting for
  `fasttail-tui.exe` in 0.20.0? It avoids the prompt problem at the cost of about 4 MB
  more in the zip.
- Should the GUI's `--filter` / `--exclude` become repeatable too, for symmetry?
- `--print` with no FILE and a terminal as standard input: usage error (proposed) or read
  the files of the saved workspace?
