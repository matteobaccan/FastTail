## ADDED Requirements

### Requirement: Open Folder Dialog
The application SHALL offer an Open folder dialog, from the open menu and when a folder is dropped on the window, with a folder, one or more file-name patterns with `*` and `?` wildcards separated by `;` (default `*.log`), an "include subfolders" option (off by default) and a mode, Follow newest or Open all. A file SHALL match when its name matches any of the patterns; with subfolders included, files in subfolders up to a depth of 8 SHALL match as well. A scan SHALL read at most 10 000 directory entries, saying so when the limit is reached, and SHALL NOT follow symbolic links or junctions to folders. A single pattern without subfolders in Follow newest mode SHALL behave exactly as the existing pattern stream.

#### Scenario: Several patterns
- **WHEN** the user opens the folder `C:\logs` with the patterns `*.log;*.txt` in Follow newest mode and the newest matching file is `trace.txt`
- **THEN** the stream tails `trace.txt`, and switches to `app.log` when that file becomes the newest.

#### Scenario: Dated subfolders
- **WHEN** the user opens `logs` with `app.log`, subfolders included, and the application creates `logs/2026-09-29/app.log` after `logs/2026-09-28/app.log`
- **THEN** within 10 seconds the stream tails `logs/2026-09-29/app.log` and the status bar reports the switch.

#### Scenario: Symbolic link loop
- **WHEN** a subfolder contains a symbolic link pointing back to the opened folder
- **THEN** the scan does not enter the link and ends normally.

### Requirement: Open All Folder Group
In Open all mode every matching file SHALL open as its own stream in one new tab group, sorted by name. At most `folder_max_streams` streams (default 32, 1 to 256, in `fasttail.ini` `[general]`) SHALL open per folder; when more files match, the newest by modification time SHALL open and a notice SHALL say how many were skipped. Compressed files and archives SHALL be skipped unless a pattern names their extension. A matching file created later SHALL open automatically as a new tab of the group within 2 seconds for a flat folder and within 10 seconds with subfolders, without changing the active tab; a file the user closed SHALL NOT be reopened by the group while it stays open. A file of the group that is deleted SHALL keep its tab and lines, and its stream bar SHALL say that the file was removed.

#### Scenario: Several services in one folder
- **WHEN** the user opens `/var/log/myapp` in Open all mode and it holds `api.log`, `worker.log` and `scheduler.log`
- **THEN** three streams open as tabs of one new group, in that name order.

#### Scenario: A job log appears
- **WHEN** the group is open and the service creates `job-1234.log`
- **THEN** a tab for `job-1234.log` is added to the group without taking the focus, and its new-lines badge counts its lines.

#### Scenario: Too many files
- **WHEN** the folder holds 120 matching files and `folder_max_streams` is 32
- **THEN** the 32 most recently modified files open and a notice says that 88 files were skipped.

#### Scenario: Rotated archives skipped
- **WHEN** the patterns are `*` and the folder holds `app.log` and `app.log.1.gz`
- **THEN** only `app.log` opens; with the pattern `*.gz` added, `app.log.1.gz` opens decompressed as well.

### Requirement: Folder Source Persistence
A Follow newest stream SHALL keep being stored as its `dir/pattern` path, with `folder_patterns` and `folder_recursive` added only when it uses several patterns or subfolders. An Open all group SHALL be stored in the workspace and in session files as a folder entry holding its folder, patterns, subfolder option and cap, with the path relative to the session file when possible; on restore the group SHALL rescan the folder and open the current matches, each file getting back its saved stream state. Workspace and session files written before this change SHALL load unchanged.

#### Scenario: Group restored after restart
- **WHEN** FastTail is closed with an Open all group on `C:\logs` and a file `new.log` is created there before the next start
- **THEN** at the next start the group reopens with its previous files and `new.log`, and the bookmarks of `api.log` are restored.

#### Scenario: Old workspace
- **WHEN** a workspace written by FastTail 0.12.0 with the pattern stream `C:\logs\app-*.log` is loaded
- **THEN** the stream opens as the same Follow newest stream.
