## Why

Windows programs, drivers' user-mode parts, Delphi / C++ / .NET apps and many installers
trace through `OutputDebugString`. Without a debugger attached, that output is visible
only in a debug monitor such as Sysinternals DebugView, which does not have FastTail's
rules, filters, bookmarks or search. LogFusion captures OutputDebugString as a log
source, and SnakeTail-style users run DebugView next to their tailer. The post-0.12.0
competitor scan lists it among the small gaps (gap 20). FastTail already turns a byte
source into a followed stream through a spool (standard input), so a debug-output
capture is a small, self-contained source on Windows.

## What Changes

- **Debug Output** entry in the open menu (Windows only) and `fasttail dbwin://local` on
  the command line: opens a live stream titled `debug output` with every
  `OutputDebugString` message of the current session.
- Each message is **one line**: `<local ISO 8601 time with ms> [<pid> <process name>]
  <message>`; a message with embedded line breaks gives continuation lines indented by
  two spaces; a trailing line break is dropped. Level detection, rules, filters, time
  range and bookmarks work on these lines.
- **Global capture** (`dbwin://global`, services in session 0) as a checkbox in the
  dialog; it needs administrator rights and says so when they are missing, never asking
  for elevation.
- **Process filter** in the dialog: include or exclude processes by name or pid, applied
  before writing; FastTail's own process is excluded by default.
- **One capture at a time**: when another debug monitor (DebugView, another FastTail)
  already owns the buffer, the stream says so instead of fighting for it.
- The stream is spooled and bounded like standard input (`stdin_spool_max_mb`, 512 MB
  free-space margin); workspace and sessions store `dbwin://local` or `dbwin://global`
  with the process filter and restart the capture on restore (earlier messages are not
  restored).

Target release: **0.23.0** (re-planned by the maintainer on 2026-10-06, from 0.22.0: about one large, two medium and three small changes per release) (planned for 0.15.0, moved after the terminal interface of 0.20.0; sources and integrations), per the release plan in
`docs/competitor-analysis.md` section 8. Priority: **Low**. Effort: **S**.

### Non-goals

- Kernel `DbgPrint` output (needs a driver) and ETW / TraceLogging sessions.
- Capturing on Linux or macOS (no equivalent facility; the entry is not offered).
- Capturing the output of a process FastTail launches as a debugger.
- Remote machines.

## Capabilities

### New Capabilities

- `debug-output-capture`: the Windows debug-output stream, its line format, process
  filter, global capture, single-owner rule and persistence.

### Modified Capabilities

None.

## Impact

- New `src/debug_output.rs` (`#[cfg(windows)]`): the DBWIN listener (shared memory and
  the two events), a reader thread and a writer thread feeding a spool through the spool
  feed shared with standard input (`stdin_source` copier split into `spool_feed` by the
  first source change that lands, see `ssh-sources` design D3).
- `Cargo.toml`: `windows-sys` as a direct Windows-only dependency (already in the tree
  through `winit` / `wgpu`) with `Win32_System_Memory`, `Win32_System_Threading`,
  `Win32_Foundation`, `Win32_Globalization`, `Win32_Security` features; no C code.
- `src/cli.rs`: `dbwin://` arguments; `src/ui/app.rs`: menu entry, dialog, open
  dispatch; `src/session.rs` and workspace: the `dbwin://` identity with its filter;
  `stdin_source::is_stdin_path`-style checks extended so the stream is never treated as
  a file (recent files, rotation, follow toggle).
- `src/i18n.rs` (16 languages), README, CHANGELOG.
- **TUI (0.20.0, PR #132)**: the listener is UI-free and produces a spool, so a TUI
  stream opens `dbwin://local` like any other source; only on Windows.
