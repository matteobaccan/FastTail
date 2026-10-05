## Why

Editors, IDEs, CI pages and terminal tools print log locations as `path:line` (`app.log:1204`)
and link to them. FastTail opens the file but always at its end (follow) or at its saved
place, so the user has to press `Ctrl+G` and type the line again. LogExpert 1.50 added
opening at a line from the command line (#58); Chipmunk has "Open with" at a position.

## What Changes

- `fasttail app.log:1204` and `fasttail --line 1204 app.log` open the file with line 1204
  selected and centred, follow paused. The same for `fasttail-tui` and `fasttail --tui`.
- `path:line:column` (compiler style) is accepted; the column is ignored.
- A `path:N` argument is read as a line only when `path` exists and `path:N` does not, so a
  file really named `x.log:12` and Windows drive letters (`C:\logs\a.log`) keep working.
- A line past the end of a growing file is waited for while the file is indexed, then the
  view lands on the last line with a notice.
- `--line` with several files applies to the first one; `path:line` applies to its own file.
- With `--print`, `--line N` starts printing at line N.
- When FastTail is already running and single-instance hand-off is on, the line is passed
  with the path.

Target release: **0.24.0** (candidates of the 2026-10-05 competitor scan, `docs/competitor-analysis.md` section 6, assigned by the maintainer on 2026-10-05), per the release plan in section 8. Priority: **medium**. Effort: **S (under a week)**.

### Non-goals

- Byte offsets (`--offset`) and times (`--at 14:02`): go to already takes a time; a
  follow-up if asked.
- URL schemes (`fasttail://`) registered with the system.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `command-line`: a line target in the paths and the `--line` option.

## Impact

- `src/cli.rs`: parsing `path:line[:col]` and `--line`; tests for drive letters and real
  names containing `:`.
- `src/main.rs`, `src/ui/app.rs`, `src/tui/app.rs`, `src/handoff.rs`: the target reaches the
  stream after it opens (the engine's go-to and `pending_jump`).
- `src/print_mode.rs`: the first printed line.
- `--help` text in every language, README, `docs/tui.md`, CHANGELOG, tests.
