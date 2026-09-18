## Context

`TailEngine::open` binds a single path and `poll_updates` watches its size and identity. The rotation handling already reopens the same path when the inode changes; a pattern stream generalises that to "the newest path among matches".

## Goals / Non-Goals

**Goals:** follow a rotating file set without user action, keep the per-stream configuration across switches, persist patterns.

**Non-Goals:** concatenating old and new files into one view (see `merged-timeline-view`); recursive patterns; watching remote shares specially.

## Decisions

- **Newest = latest modification time**, ties broken by name; a scan every 2 seconds is enough for daily/hourly rotation and costs one `read_dir` per stream.
- **In-house matcher for `*` and `?`** on the file name only, directory taken literally, avoiding a dependency and surprising recursive behaviour.
- **Switch semantics**: keep filters, rules, search query, wrap, encoding; reset buffer, line index, bookmarks, selection, unseen counters; play no sound. The stream status bar shows "switched to <name>" for 5 seconds.
- **Persist the pattern, not the resolved file**, so the next start resolves again.

## Risks / Trade-offs

- [A file being rewritten in place with a newer mtime] → the same path stays selected; only a different newest path triggers a switch.
- [Large directories] → `read_dir` on thousands of entries every 2 s is still cheap; scanning is skipped while the app is idle (screensaver).
