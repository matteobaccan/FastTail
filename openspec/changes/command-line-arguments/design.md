## Context

`main.rs` is compiled with `windows_subsystem = "windows"`, so there is no console by default; `--help` output must attach to the parent console (`AttachConsole(ATTACH_PARENT_PROCESS)`) to be visible when run from a terminal.

## Goals / Non-Goals

**Goals:** make the documented behaviour true, cover the handful of options a tail tool needs, keep startup unchanged when no arguments are given.

**Non-Goals:** a full CLI mode without a window; reading from stdin (a GUI tail over a pipe is a separate feature).

## Decisions

- **Hand-written parser** over `clap`: six options and positional paths do not justify a dependency that adds seconds of compile time and hundreds of KB.
- **Paths are opened in addition to the restored workspace** by default, deduplicated against already open files; `--fresh` is the explicit way to start clean. This matches how users double-click a file while FastTail already holds their layout.
- **Relative paths are resolved against the current directory at startup**, before anything else changes it.
- **Exit codes**: 0 for `--help`/`--version`, 2 for usage errors, otherwise the GUI runs.

## Risks / Trade-offs

- [Console attach on Windows prints after the shell prompt returned] → known Windows behaviour for GUI-subsystem apps; acceptable for `--help`.
