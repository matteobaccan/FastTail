## Context

`FastTailConfig` holds both global preferences and the workspace (open files, dock layout, geometry). The default workspace is exactly a session that is loaded and saved implicitly.

## Goals / Non-Goals

**Goals:** switch between prepared tab sets in one action, share a session file with a colleague, keep global preferences out of it.

**Non-Goals:** auto-switching sessions per project directory; cloud sync.

## Decisions

- **`Session` struct extracted from `FastTailConfig`**, serialised as INI with the same helpers; `fasttail.ini` embeds the default session under `[session]` sections so the current behaviour is unchanged for users who never name a session.
- **Paths inside a session are stored absolute and, when under the session file's directory, also relative**, so a session saved next to a log bundle still opens after the bundle moves.
- **Dirty tracking** compares the serialised current session with the last saved one at most once per second.
- **Loading replaces** the workspace: streams are closed, then opened from the session; missing files are reported in a summary and skipped.

## Risks / Trade-offs

- [Sessions referencing huge files reopen slowly] → opening is already lazy and mmap-based; no change.
- [Relative vs absolute ambiguity] → relative wins when it exists, otherwise absolute, otherwise skipped with a notice.
