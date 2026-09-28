## ADDED Requirements

### Requirement: Open System Source Dialog
The File menu SHALL offer "Open system source…", which opens a dialog with a kind selector (journald, shown on Linux only, and Docker), a list of the sources available and a text field. The list SHALL be filled by a background task, without blocking the interface: the running containers with their image and status for Docker, and the system services, or the user services when chosen, for journald. An error while listing (the daemon not running, a permission denied, the command not found) SHALL be shown in the dialog, and the text field SHALL still accept a name. Choosing an entry or typing a name and confirming SHALL open the source as a stream titled `docker: <container>` or `journal: <unit>` (`journal: system` for the whole journal). Settings SHALL offer `system_source_backlog_lines`, `docker_command` and `journalctl_command`.

#### Scenario: Picking a container
- **WHEN** Docker runs the containers `api` and `db` and the user opens the dialog and chooses Docker
- **THEN** the list shows `api` and `db` with their image and status, and choosing `api` opens the stream `docker: api`.

#### Scenario: Docker not running
- **WHEN** the Docker daemon is stopped and the user chooses Docker in the dialog
- **THEN** the dialog shows the error reported by the `docker` command, the interface stays responsive, and typing `api` still opens the source.

### Requirement: System Sources in the Workspace and Sessions
A system source stream SHALL be saved in the workspace, the recent files and session files by its URI, written unchanged and never made relative, together with its view state (filters, presets applied, level, time range text, wrap, ANSI mode, collapse, line numbers, time delta, and the other per-stream keys); its bookmarks and bookmark notes SHALL NOT be saved. At start, or when a session is loaded, the source SHALL be opened again by running its command. A URI that cannot be opened on this platform SHALL be reported like a missing file.

#### Scenario: Restored at start
- **WHEN** FastTail is closed with the stream `docker: api` filtered by `ERROR` and started again
- **THEN** the stream `docker: api` is open again, its `docker logs` command is running and its include term is `ERROR`.

#### Scenario: Session opened on another platform
- **WHEN** a session saved on Linux with `journald://nginx.service` is loaded on Windows
- **THEN** that stream is listed as not available and the session's other streams open.
