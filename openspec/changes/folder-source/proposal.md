## Why

FastTail already follows the newest file of a pattern (`C:\logs\app-*.log`), and
dropping a folder asks for such a pattern. Two common cases are still missing. First, a
service that writes **several logs at once** in one folder (`api.log`, `worker.log`,
`scheduler.log`, a new `job-1234.log` per job): the user opens each file by hand and
misses the ones created later. Second, applications that write into **dated
subfolders** (`logs/2026-09-28/app.log`) or need **more than one pattern** (`*.log` and
`*.txt`): the pattern stream only looks at one folder with one pattern. Tailviewer has
folder data sources, LogFusion watched folders and SnakeTail a folder tail; the
post-0.12.0 competitor scan lists folder sources among the desktop features FastTail
lacks (section 2).

## What Changes

- The **Open pattern** dialog (also shown when a folder is dropped) becomes **Open
  folder**: folder, one or more file-name patterns separated by `;` (default `*.log`),
  **include subfolders** (off by default, depth at most 8) and the mode:
  - **Follow newest** (today's pattern stream): one stream that switches to the newest
    matching file, now across all the patterns and, when chosen, the subfolders.
  - **Open all**: every matching file opens as its own stream in **one new tab group**
    (sorted by name), and a matching file **created later** opens automatically as a new
    tab of that group, without taking the focus (the `[N]` badge shows its new lines).
- **Open all** limits: at most `folder_max_streams` streams per folder (default 32,
  1–256); beyond, the newest by modification time open and a notice names how many were
  skipped. Compressed files (`.gz`, `.zip`, ...) are skipped unless the pattern names
  them explicitly (for example `*.gz`), so a folder of rotated archives does not start
  thirty decompressions.
- A file of an open-all group that is deleted keeps its tab and lines; its stream bar says
  the file was removed.
- Scans: the existing 2-second rescan of the folder for a flat folder; with subfolders, a
  file-system watcher triggers a rescan at once and a full walk runs every 10 seconds at
  most, bounded at 10 000 entries; symbolic links and junctions to folders are not
  followed.
- Persistence: a follow-newest stream keeps its `dir/pattern` path (unchanged for one
  pattern without subfolders) plus `folder_patterns=` and `folder_recursive=` when used;
  an open-all group is stored in the workspace and in sessions as a folder entry (folder,
  patterns, subfolders) and rescanned on restore, the per-file state (bookmarks, filters)
  being kept per file as today.
- New `fasttail.ini` key `folder_max_streams` in `[general]`.

Target release: **0.15.0** (structured logs and analysis, continued; planned for 0.14.0, moved when 0.14.0 shipped early), per the release plan in
`docs/competitor-analysis.md` section 8. Priority: **Medium**. Effort: **M**.

### Non-goals

- A single merged stream of the folder's files by timestamp (that is
  `merged-timeline-view`; an open-all group is its natural input later).
- `**` or directory wildcards inside the folder part of a path; regex file-name patterns.
- Remote folders (SSH, SMB-specific handling beyond what the OS mounts).
- Closing tabs automatically when their file disappears.

## Capabilities

### New Capabilities

- `folder-source`: the Open folder dialog, multi-pattern and subfolder matching for
  follow-newest streams, open-all folder groups with auto-open of new files, limits and
  persistence.

### Modified Capabilities

None. `stream-engine` "Pattern Streams Follow the Newest Matching File" keeps holding for
a single pattern in one folder; the new capability extends it.

## Impact

- `src/wildcard.rs`: `FolderSpec { dir, patterns, recursive }`, `list_matches(spec,
  limit)` (depth 8, 10 000 entries, no link following), `resolve_newest` over a spec;
  the module note on non-recursion updated.
- New `src/folder_source.rs`: an open-all group (`FolderGroup`): known files, rescan with
  a `notify` watcher (already a dependency) plus the timed walk, new-file events for the
  app.
- `src/tail_engine.rs`: pattern streams take a `FolderSpec` instead of `(dir, glob)`;
  a "file removed" state for a stream whose file is deleted.
- `src/ui/app.rs`: the Open folder dialog replacing the pattern prompt (menu entry, folder
  drop, `default_pattern_for`); opening a group into a new dock node; auto-opened tabs
  without focus change.
- `src/session.rs`, workspace in `src/config.rs`: folder entries, `folder_patterns`,
  `folder_recursive`, `folder_max_streams`; old files without them load as today.
- `src/i18n.rs` (16 languages), README, CHANGELOG.
- **TUI (0.20.0, PR #132)**: `FolderSpec` and `FolderGroup` are UI-free; the TUI can open
  a group as a list of streams and receive the new-file events.
