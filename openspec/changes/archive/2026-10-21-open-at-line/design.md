## Context

`src/cli.rs` turns arguments into paths and options shared by the window, the terminal
interface and `--print`. The engine already resolves go-to targets (`resolve_goto`) and
shows a line through `pending_jump` / `scroll_to_line`; a large file is indexed in the
background, so a line may not exist yet when the stream opens.

## Goals / Non-Goals

**Goals:** open a file at a line from any tool that prints `path:line`; the same
behaviour in the window, the terminal interface and print mode.

**Non-Goals:** offsets, times, system URL handlers.

## Decisions

1. **`path:line` only when it is unambiguous.** The suffix `:N` or `:N:M` is split off
   only when the full argument is not an existing file and the part before it is. This
   keeps `C:\a.log` (the drive colon is followed by `\`, not digits) and a file literally
   named `a.log:12` (it exists, so it is opened as is). *Rejected:* always splitting
   (breaks real names), a separate syntax such as `+1204 file` (less common in tool output).
2. **`--line N` targets the first path**; per-path suffixes win over it. Lines are
   1-based as shown in the gutter.
3. **Landing** reuses go-to: the line is selected and centred, follow is paused, and a
   line hidden by a saved filter is revealed as go-to does today (the filters stay).
4. **Waiting for the index:** the target is kept on the stream until the index reaches it
   or the initial indexing ends; then the view lands on the line, or on the last line with
   a notice (`Line 9000 is past the end (8123 lines)`).
5. **Hand-off:** the single-instance message carries `path` and `line`; an older running
   instance that does not know the field opens the file as before.

## Risks / Trade-offs

- [A path ending in `:digits` that does not exist] → reported as "file not found" with
  the full argument, as today.
- [Huge file, line near the end] → the landing waits for the background index; the notice
  says it is indexing.

## Open Questions

- Should a relative line (`:+10` from the end) be supported? Not planned.
