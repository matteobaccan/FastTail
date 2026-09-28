## Context

`src/wildcard.rs` matches one `*` / `?` pattern against the file names of one folder,
never recursively, and `resolve_newest` picks the newest match. `TailEngine::open_pattern`
keeps `pattern: Option<String>` and rescans every 2 s (`pattern_scan_interval`),
switching file and resetting buffer, bookmarks and selection. A dropped folder opens the
pattern prompt (`pattern_prompt`, `default_pattern_for` → `dir/*.log`). Sessions and the
workspace store pattern paths; `session.rs` treats a pattern whose folder exists as
present. Each stream already has its own `notify` watcher for its file.

## Goals / Non-Goals

**Goals:** follow a whole folder of logs, including files that appear later; subfolders
and several patterns; no behaviour change for existing pattern streams and saved
workspaces; bounded scan cost on large trees.

**Non-Goals:** merged view, remote folders, regex file-name patterns.

## Decisions

### D1. One spec for both modes
`FolderSpec { dir, patterns: Vec<String>, recursive: bool }`. A name matches when it
matches any pattern; with `recursive`, the pattern applies to the file name and files in
subfolders up to depth 8 count. A single pattern without subfolders serialises to the
existing `dir/pattern` path, so today's files and command lines are unchanged.

### D2. Bounded walk
`list_matches` walks breadth-first, stops at 10 000 directory entries per scan (notice
"folder too large, scan limited") and does not follow symbolic links or junctions to
folders (loops, and surprise network shares). Flat folders keep the 2 s rescan. With
subfolders, a recursive `notify` watcher on `dir` triggers a rescan on create / rename
events (debounced 250 ms) and a full walk runs at most every 10 s to catch what the
watcher misses (network drives, overflowed event queues).

### D3. Open all as a folder group
`FolderGroup` owns the spec and the set of files it has opened. On open it lists the
matches, sorts them by name, takes the `folder_max_streams` newest by modification time
when there are more, and asks the app to open each one into a new dock node (tabs of one
group). Later scans report new files; the app opens them as tabs of the same node — or,
when the user has moved or closed that node, of the node holding the group's most recent
tab — without changing the active tab. A file the user closes is not reopened by the
group until the group is reopened. Compressed files are skipped unless a pattern ends in
their extension, since `compressed::sniff` would start a decompression per file.

### D4. Deleted files
When a file of a group, or the file of a plain stream, is removed, the engine keeps its
indexed lines and marks the stream "file removed" in the stream bar; if a file of that
name reappears it is tailed from the start as a rotation. Follow-newest streams keep
switching to the next newest match, as today.

### D5. Persistence
Stream entries gain `folder_patterns=` (the `;` list) and `folder_recursive=` only when
they differ from the single-pattern form. Open-all groups are stored as
`[folder_N]` sections (`dir`, `patterns`, `recursive`, `max_streams`) in the workspace and
in sessions, paths relative to the session file as for streams; on restore the group
rescans and opens the current matches, and each file's saved state (filters, bookmarks)
applies by file path as for any stream.

## Risks / Trade-offs

- [A folder of hundreds of logs opens hundreds of tabs] → the per-group cap (32 by
  default) and the notice.
- [Recursive watching of a huge tree costs handles / inotify watches] → the watcher is
  optional: if it cannot be created, the timed walk alone is used and nothing fails.
- [Bookmarks on a follow-newest stream are still reset on switch] → unchanged behaviour,
  documented.

## Open Questions

- Should the command line get `--all` to open every match of a pattern argument as a
  group? Proposed: yes, small, useful for scripts.
- Once `merged-timeline-view` ships, should "Open all" offer "as one merged stream"?
  Proposed: yes, as a third mode in that change.
