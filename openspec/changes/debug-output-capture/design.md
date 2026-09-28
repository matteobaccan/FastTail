## Context

`OutputDebugStringA/W` in a process without a debugger writes to a monitor through
named objects: the event `DBWIN_BUFFER_READY` (set by the monitor when it can take a
message), the event `DBWIN_DATA_READY` (set by the writer) and the 4096-byte file mapping
`DBWIN_BUFFER` (a `u32` pid followed by up to 4092 bytes of ANSI text, NUL-terminated).
The writer waits on `BUFFER_READY` for up to 10 s, so a slow monitor stalls every
tracing process. The `Global\` prefix of the same names reaches session 0 services and
needs `SeCreateGlobalPrivilege` (administrators). `OutputDebugStringW` converts to the
ANSI code page before writing; Windows 10+ also has a Unicode path only used when a
debugger is attached.

FastTail's standard-input stream copies bytes into a spool that the engine tails
(`stdin_source.rs`); `ssh-sources` design D3 splits that copier into a shared
`spool_feed`.

## Goals / Non-Goals

**Goals:** DebugView-equivalent capture for the local session (and global with admin);
never slow down the traced programs; the same stream features as any file.

**Non-Goals:** kernel output, ETW, non-Windows platforms.

## Decisions

### D1. Two threads, never block the writer
The **reader** thread loops: set `BUFFER_READY`, wait `DATA_READY` (500 ms timeout to
check for close), copy pid and bytes into a queue entry with the arrival time, repeat.
It does no I/O and no allocation beyond the entry, so the writer is released within
microseconds. The **writer** thread drains the queue (bounded at 65 536 messages; excess
counted and reported as "N messages dropped"), formats lines and appends them to the
spool feed, flushing per batch and waking the UI.

### D2. Single owner
The monitor creates the objects with `CreateEventW` / `CreateFileMappingW`; when
`GetLastError()` is `ERROR_ALREADY_EXISTS` right after creating `DBWIN_BUFFER_READY`,
another monitor is active: the objects are closed and the stream ends with "another debug
monitor is running (for example DebugView)". Two monitors on the same buffer would each
see a random half of the messages, which is worse than none.

### D3. Line format and decoding
`YYYY-MM-DDTHH:MM:SS.mmm [pid name] message` in local time, the form the timestamp
detector already reads. The text is decoded from the ANSI code page
(`MultiByteToWideChar(CP_ACP)`) into UTF-8. The process name comes from
`QueryFullProcessImageNameW` with `PROCESS_QUERY_LIMITED_INFORMATION`, cached per pid for
60 s (pids are reused); `?` when the process has already exited or cannot be opened.
Messages are split on CR/LF; the first part gets the prefix, the rest are indented by two
spaces so multi-line grouping keeps them together.

### D4. Process filter before the spool
Include / exclude lists of names (case-insensitive, `*` / `?` wildcards through
`wildcard_match`) and pids, checked on the writer thread. FastTail's own pid is excluded
by default: GPU drivers and runtimes may trace into it.

### D5. Identity and persistence
`dbwin://local` and `dbwin://global`, plus `dbwin_include=` / `dbwin_exclude=` in the
stream entry. On restore the capture restarts empty; if another monitor owns the buffer,
the stream shows D2's message and a Retry button.

## Risks / Trade-offs

- [A debugger attached to a process receives its messages instead] → documented; nothing
  to do.
- [High-rate tracing floods the spool] → spool cap and restart as for stdin; queue bound
  and dropped counter.
- [Global capture silently misses services without admin] → the dialog disables the
  checkbox with the reason when not elevated.

## Open Questions

- Should the stream offer a pause button that stops taking messages (DebugView's
  "capture" toggle) without closing the tab? Proposed: yes, it releases the buffer so
  another monitor can take it.
- Should the process name column be optional to keep lines short? Proposed: no, the
  structured-fields regex parser can hide it once it ships.
