## Why

FastTail's filters, level detection, time parsing and highlight rules only exist inside
the window. To get "the ERROR lines of the last hour of these three logs" into a script,
a CI job, a ticket or another tool, the user leaves FastTail and rebuilds the same logic
with `grep`, `awk` and `date`, with different matching rules and no knowledge of the
formats FastTail already reads (syslog, Apache, epoch, compressed files). hl, tailspin
and lnav all work as filters on a pipe; the competitor scan lists a headless mode as
gap 7, and the planned terminal interface (0.20.0) needs the same thing for
`cmd | fasttail-tui -` when its output is not a terminal.

## What Changes

- `fasttail --print [OPTIONS] FILE…|-` writes the lines that pass the filters to standard
  output and exits, **without opening a window** and without reading or writing the
  workspace.
- Options, all optional:
  - `--filter <TEXT>` and `--exclude <TEXT>`, each repeatable up to 8 times, with the
    same semantics as the stream terms (includes ANDed, excludes ORed); `--regex` and
    `--case-sensitive` as the stream toggles;
  - `--level <LEVEL>` (minimum level, as the `≥ level` selector);
  - `--since <TIME>` / `--until <TIME>`: anything the time range popup accepts
    (`14:02`, `2026-09-28 14:02`, a timestamp from a line) plus relative times
    (`now`, `-15m`, `-3h`, `-1h30m`, `-2d`, `-1w`, measured back from now, with the
    `relative-time-windows` syntax shared through `timestamp::parse_relative`);
  - `--context <N>`: `N` lines before and after each match (as the `context-lines`
    change defines it);
  - `--follow`: after the existing content, keep printing appended lines (rotation and
    truncation handled as in the window) until interrupted;
  - `--color <auto|always|never>`: level colours and highlight rules from `fasttail.ini`
    as ANSI escape sequences (auto = standard output is a terminal and `NO_COLOR` is not
    set);
  - `--line-numbers`, and `--no-prefix` to drop the `file:` prefix printed when more than
    one input is given.
- Inputs: plain files, directory patterns (`app-*.log`, the newest match), single-file
  compressed logs (`.gz`, `.bz2`, `.xz`, `.zst`, read as a stream, nothing written to
  disk) and standard input (`-`, or piped input without any FILE).
- The **same matching code** as the window: `FilterSpec` (terms, levels, continuation
  lines), the timestamp parsers with inheritance for continuation lines, level detection,
  ANSI stripping, compiled highlight rules. Constant memory: no line index is built.
- Exit codes: 0 when at least one line was printed, 1 when none matched, 2 for a usage
  error, 3 when an input could not be read (the others are still printed).
- On **Windows** the GUI-subsystem executable attaches to the parent console only when
  standard output is not already redirected, and turns on virtual-terminal processing
  for colours.
- No new key in `fasttail.ini`; the file is only read (theme, rules), never written.

Target release: **0.13.0** (basics and quick wins), per the release plan in
`docs/competitor-analysis.md` section 8. Effort: **S**.

### Non-goals

- Zip and tar archives (their entries need the picker); a clear message says so.
- The `journald://` and `docker://` sources of the `system-sources` change: in a shell,
  `journalctl` and `docker logs` can already be piped into `fasttail --print -`.
- Output formats other than lines (JSON, CSV) and field projections: the
  `structured-fields` change can add them later.
- Repeatable `--filter` / `--exclude` in the GUI mode, where they stay one term each.
- A separate console-subsystem executable in 0.13.0 (see the design's open questions;
  0.20.0 brings `fasttail-tui.exe`).
- Paging (`less`) or interactive keys: that is the terminal interface.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `command-line`: adds Headless Print Mode, Print Mode Output and Colours, and Print Mode
  Follow.

## Impact

- `src/print_mode.rs` (new, UI-agnostic: no `egui` / `eframe` import): argument handling,
  input readers, the line pipeline, the ANSI writer, the follow loop.
- `src/cli.rs`: `--print` and its options, repeatable terms, validation (print-only
  options without `--print` are a usage error), `USAGE` text.
- `src/main.rs`: branch to `print_mode::run` before any window or workspace code; console
  attach only when standard output is not redirected; virtual-terminal mode for colours.
- `src/scan_job.rs`, `src/timestamp.rs`, `src/log_level.rs`, `src/ansi.rs`,
  `src/compressed.rs`, `src/wildcard.rs`, `src/file_source.rs`: reused as they are, a
  shared line reader extracted if needed.
- README (command-line section, FAQ), CHANGELOG, tests (integration tests running the
  binary with `assert_cmd`-style `std::process::Command`).
