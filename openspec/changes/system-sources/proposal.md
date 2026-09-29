## Why

On Linux servers many services no longer write log files: they log to the systemd journal,
or to Docker, which keeps the container's output behind `docker logs`. To see them in
FastTail today the user types `journalctl -fu nginx | fasttail -` or
`docker logs -f api | fasttail -`: it works, but the stream is called `stdin`, is not in
the recent files, cannot be saved in a session, loses stderr unless the user adds `2>&1`,
and stops for good when the container restarts. journald and Docker sources are what
lnav, lazyjournal and nerdlog users ask for first (nerdlog's most requested feature was
journald), and the post-0.12.0 competitor scan ranks remote and system sources gap 5.

## What Changes

- Two new **stream sources**, named by a URI wherever a path is accepted today:
  - `journald://` (the whole system journal), `journald://<unit>` (one systemd unit, such
    as `journald://nginx.service`) and `journald://user/<unit>` (a user unit); Linux only.
  - `docker://<container>` (a container name or id) on every platform where the `docker`
    command is available; the command can be changed to `podman` in Settings.
- Each source runs the platform's own command (`journalctl -f …`, `docker logs -f …`)
  with its standard output **and** standard error copied into a temporary **spool**, as
  standard input is today; the spool is tailed as a normal followed stream, so filters,
  search, levels, time range, collapse, context lines, bookmarks and export work on it.
- Opened from a new **"Open system source…"** dialog (kind, a list of running containers
  or units fetched in the background, a free text field) and from the **command line**
  (`fasttail docker://api journald://nginx.service`); the tab is titled `docker: api` /
  `journal: nginx.service`.
- **Followed live**; the stream bar says when the source ended (the container stopped,
  the command failed, with its last error line) and offers **Reconnect**, which resumes
  after the last line received instead of repeating it.
- **Saved** in the workspace, the recent files and session files by their URI, and
  reopened at start by running the command again; the view state (filters, columns,
  wrap, collapse…) is restored, bookmarks are not (line numbers depend on what the source
  returns).
- The backlog read at open is bounded by a new `fasttail.ini` key
  `system_source_backlog_lines` (default 10,000); the spool reuses the
  `stdin_spool_max_mb` cap. `docker_command` (default `docker`) and `journalctl_command`
  (default `journalctl`) are new keys in `fasttail.ini` too.
- **SSH** (`ssh://host/path`) is **phase 2**, a later release: see the design for why.

Target release: **0.16.0** (moved from 0.15.0 when 0.14.0 shipped early; sources and integrations), per the release plan in
`docs/competitor-analysis.md` section 8. Effort: **M–L**.

### Non-goals

- SSH, SFTP and SCP sources in this change (phase 2 for `ssh://` with the same process
  approach; SFTP random access is not planned).
- Kubernetes (`kubectl logs`), Windows Event Log, syslog / TCP / UDP listeners: other
  sources for later changes, which the source abstraction here makes cheaper.
- Native journal or Docker Engine API clients (see the design).
- Remote Docker hosts beyond what the user's `docker` command already reaches through its
  own context or `DOCKER_HOST`.
- Following a container across a re-creation with a new id; a restarted container with
  the same name is reconnected by the user.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `stream-engine`: adds System Log Sources (URIs, commands, spool, live follow, end of
  source and reconnect, limits) and System Source Process Lifecycle.
- `command-line`: adds System Source URIs on the Command Line.
- `cyber-ui-docking`: adds the Open System Source dialog and System Sources in the
  Workspace and Sessions.

## Impact

- `src/system_source.rs` (new): URI parsing and validation, command lines, the child
  process with one pipe for its standard output and error, reconnect cursor, the
  container and unit listing jobs.
- `src/stdin_source.rs`: the copier and spool reused for any `Read` (renamed or wrapped
  as a generic piped source); `StdinStream::start` already takes `impl Read`.
- `src/tail_engine.rs`: `system: Option<SystemStream>` next to `stdin` and `compressed`,
  pseudo-path identity, status for the stream bar.
- `src/cli.rs`: URIs recognised before path resolution.
- `src/ui/app.rs` / `src/ui/dock.rs`: the dialog, open dispatch, stream bar status and
  Reconnect, titles.
- `src/session.rs` / `src/config.rs`: URIs in `StreamEntry.path`, `source_exists` true for
  URIs, bookmarks not saved for them; the three new `fasttail.ini` keys and Settings.
- `src/i18n.rs` (16 languages), help dialog, README, CHANGELOG, tests.
