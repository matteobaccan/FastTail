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
`list` walks breadth-first, stops at 10 000 directory entries per walk (notice "folder
too large, scan limited") and does not follow symbolic links or junctions to folders
(loops, and surprise network shares). A flat folder is read whole, as before, and keeps
the 2 s rescan. With subfolders, the watcher stays on the folder itself (a recursive
`notify` watch walks the whole tree, links included, inside `watch()` and takes one
inotify watch per directory); a file or folder appearing there triggers a rescan (at most
every 2 s) and a full walk runs every 10 s.

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
The pattern path names the whole spec: patterns separated by `;` (`dir/*.log;*.txt`) and
a `**` folder before them for subfolders (`dir/**/app.log`). A single pattern without
subfolders is the path pattern streams always had, so old files and command lines are
unchanged, and no extra stream keys are needed (an older build reads a multi-pattern or
recursive path as a pattern that matches nothing, or a folder that does not exist).
Open-all groups are stored as `[folder_groups] group_N=<pattern path>` in `fasttail.ini`
and as `[folder_N]` sections (`path`, `rel`) in sessions, only while a tab of the group is
open; the cap is the global `folder_max_streams`. On restore the group rescans and opens
the current matches, and each file's saved state (filters, bookmarks) applies by file path
as for any stream. A group ends when its last tab is closed.

## Risks / Trade-offs

- [A folder of hundreds of logs opens hundreds of tabs] → the per-group cap (32 by
  default) and the notice.
- [Recursive watching of a huge tree costs handles / inotify watches] → no recursive
  watch: the folder itself is watched, the subfolders are walked on the 10 s timer.
- [Bookmarks on a follow-newest stream are still reset on switch] → unchanged behaviour,
  documented.

## Open Questions

- Should the command line get `--all` to open every match of a pattern argument as a
  group? Proposed: yes, small, useful for scripts.
- Once `merged-timeline-view` ships, should "Open all" offer "as one merged stream"?
  Proposed: yes, as a third mode in that change.
