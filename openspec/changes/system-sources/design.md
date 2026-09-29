## Context

Standard input already shows how a non-file source becomes a stream: `stdin_source`
copies any `impl Read` into a spool file (`spool::SpoolFile`, in `spool_dir`, named after
the owning pid and swept at start after a crash) on a detached thread, 64 KB reads,
flushed at once, bounded by `stdin_spool_max_mb` and a 512 MB free-space margin (restart
from empty, seen by the engine as a truncation); `open_engine` gives the engine a
pseudo-path (`<stdin>`) and `TailEngine.stdin` owns the copier so closing the tab deletes
the spool. The workspace and sessions store `StreamEntry.path`; `session::source_exists`
checks files and pattern directories; `cli::resolve` joins relative paths with the current
directory. Compressed streams use the same spool machinery (`TailEngine.compressed`).
External tools start processes with `CREATE_NO_WINDOW` on Windows
(`external_tools.rs`) so no console flashes. The minimum Rust version is 1.88, so
`std::io::pipe` (1.87) is available.

## Goals / Non-Goals

**Goals:**
- journald and Docker as first-class streams: named, reopened, saved, followed, with
  every stream feature.
- No new dependency and no new engine path: the source is a spool, like standard input.
- The user's own permissions, contexts and configuration apply, as when they type the
  command.

**Non-Goals:**
- SSH in this change, native clients, Kubernetes, Event Log, listeners (see the proposal).

## Decisions

1. **Processes, not native APIs.** A source runs `journalctl` or `docker` as a child
   process and copies its output. *Rejected:* `sd-journal` through `libsystemd` — a C
   library to link or `dlopen`, Linux-only code paths in the build, and journal file
   format and permission handling that `journalctl` already does (including
   `systemd-journal` group membership and `--user`). *Rejected:* the Docker Engine API —
   HTTP over a Unix socket or a Windows named pipe, a multiplexed stdout / stderr frame
   format, TLS and contexts for remote hosts; clients such as `bollard` bring `tokio` and
   `hyper` into a codebase that has no async runtime. The command approach costs a
   process per stream (a few MB), works with Podman by changing one setting, and honours
   `DOCKER_HOST`, Docker contexts and sudo-less group setups exactly as the terminal does.
   No strong reason for native APIs was found; a later change can swap the back end behind
   the same URI.
2. **Command lines**, arguments passed as separate argv entries, never through a shell:
   - `journald://[user/]<unit>`: `journalctl --follow --no-pager --output=short-iso-precise
     --lines=<backlog> [--user] --unit=<unit>`; `journald://` without a unit omits
     `--unit`. `short-iso-precise` starts every line with an ISO 8601 timestamp with
     microseconds and zone, which the timestamp parser reads, and keeps the
     `host unit[pid]: message` layout that level detection handles.
   - `docker://<container>`: `docker logs --follow --timestamps --tail <backlog> -- <container>`.
     `--timestamps` prefixes RFC 3339 nanosecond timestamps, read as ISO 8601.
   `<backlog>` is `system_source_backlog_lines` (default 10,000, range 0 to 1,000,000;
   `all` is not offered because a year of journal would fill the spool).
   *Rejected:* `--output=json` for journald: structured, but one JSON object per line is
   unreadable without the `structured-fields` change and loses the familiar layout.
3. **One pipe for stdout and stderr.** The child gets the two ends of one anonymous pipe
   (`std::io::pipe()`, the writer cloned) as its standard output and error, like `2>&1`,
   so `docker logs` output from the container's stderr is not lost and lines arrive in
   the order the child wrote them; the reader end goes to the existing copier
   (`StdinStream::start(reader, …)`, generalised as `PipedSource`). *Rejected:* two pipes
   merged by a thread — interleaving by read chunk breaks lines unless a line assembler is
   added, for no benefit.
4. **Validation.** A unit must match `[A-Za-z0-9@_.:\\-]{1,256}` and must not start with
   `-`; a container must match `[A-Za-z0-9][A-Za-z0-9_.-]{0,254}` or be 12 to 64 hex
   digits. The container is also placed after `--`. The executable names come only from
   `fasttail.ini` (`docker_command`, `journalctl_command`), never from a session file or
   a URI, so opening a session can run only those two programs with a validated name.
   *Rejected:* allowing any name and relying on argv separation: `docker logs` and
   `journalctl` would still parse a leading `-` as an option.
5. **Lifecycle.** On Windows the child is started with `CREATE_NO_WINDOW`. Closing the
   stream kills the child (`Child::kill`, then `wait` on a detached thread) and deletes
   the spool; at exit every child is killed. After a crash the child gets a broken pipe on
   its next write and exits; the orphaned spool is swept at the next start as today.
   When the child exits, the copier sees the end of the pipe: the stream stays open with
   its lines, the stream bar says the source ended with the exit code and the last line
   the child wrote to the pipe before exiting (the error of `docker: Error response from
   daemon: No such container` or `journalctl: … No journal files were found`), and offers
   **Reconnect**.
   *Rejected:* restarting the command automatically in a loop: a missing container would
   spin; the user decides (see the open questions).
6. **Reconnect without duplicates.** The engine keeps the timestamp of the last line
   received. Reconnect starts the command again, appending to the same spool, with
   `--since <last timestamp>` (docker: RFC 3339; journalctl: `--since "YYYY-MM-DD
   HH:MM:SS.ffffff"` in the journal's local time) and no backlog limit, and drops the
   leading lines whose timestamp is not after the last one received (lines with exactly
   the same timestamp and text are skipped). The stream simply grows; bookmarks and
   filters stay valid. Automatic reconnection is not done in this change (see the open
   questions).
   *Rejected:* restarting from an empty spool on reconnect: bookmarks, selection and the
   lines already read would be lost for a container that only restarted.
7. **Identity and titles.** The URI is the stream's path (`PathBuf::from("docker://api")`;
   on Windows `docker:` is not a drive prefix), stored as is in the workspace, the recent
   files and sessions, never made relative. `session::source_exists` returns true for a
   URI; `cli::parse` recognises `journald://` and `docker://` before `resolve`. Tabs are
   titled `docker: api` and `journal: nginx.service` (`journal: system` without a unit).
   Bookmarks and notes are not saved for these streams, because a reopened source starts
   from a different backlog; everything else in `StreamEntry` is.
   *Rejected:* a `[system_source]` section apart from the streams: the workspace, the
   recent list and sessions already carry a path per stream; a URI fits there.
8. **Open dialog.** "Open system source…" in the File menu and next to the `📂*` prompt:
   a kind selector (journald shown only on Linux, Docker when the command is found on
   `PATH` or configured), a list filled by a background job (`docker ps --format
   "{{.Names}}\t{{.Image}}\t{{.Status}}"`; `systemctl list-units --type=service
   --no-legend --plain`, and `--user` for user units), a text field, and Open. Listing
   errors (daemon not running, permission denied) are shown in the dialog; the text field
   still works.
   *Rejected:* only a text prompt as for patterns: nobody remembers container names, and
   listing them is one background command.
9. **Platforms.** journald on Linux only (a `journald://` URI elsewhere is refused with a
   message). Docker on Linux, macOS and Windows (Docker Desktop, Podman).
   *Rejected:* hiding the Docker kind on Windows and macOS: Docker Desktop and Podman
   are common there and `docker logs` behaves the same.

Threads and memory: one child process and one copier thread per source, plus one listing
job while the dialog is open; the UI thread only reads the spool through the engine.
Memory per stream: the engine's usual state (line index at 8 bytes per line) and the
copier's 64 KB buffer; the spool is on disk, bounded by `stdin_spool_max_mb` (default
2048 MB) with the 512 MB free-space margin, restarted from empty (a truncation for the
engine) when a limit is reached, as for standard input. Windows file sharing: the spool is
opened for shared reading exactly as the standard-input spool.

### SSH: phase 2, not in 0.22.0

`ssh://host/path` fits the same process design (`ssh -T -o BatchMode=yes host -- tail -n
<backlog> -F -- '<path>'`), which is why the source abstraction is generic. It is left for
a later release because:
- **Authentication without a console**: a GUI-subsystem program cannot answer a password
  or host-key prompt; only `BatchMode=yes` with keys or an agent works, and the failure
  modes (unknown host key, agent not running, Windows OpenSSH agent service disabled) need
  their own messages and documentation.
- **Remote quoting**: the remote command goes through the remote user's shell, so the path
  must be quoted for POSIX `sh` and validated; a mistake there is a command injection on
  the remote host, which deserves its own review.
- **No random access**: `tail -F` over SSH gives a stream, not the file, so multi-GB remote
  files would be copied whole into the spool; users expect SFTP-style browsing, which is a
  different (larger) feature.
- **Pattern and rotation semantics** of the remote side (`tail -F` follows by name) differ
  from the local ones and need their own tests.

## Risks / Trade-offs

- [`docker logs` of a busy container at 10,000 lines per second] → the copier writes
  64 KB chunks as today's standard input does, and the engine indexes appended bytes
  incrementally; the spool cap bounds disk use.
- [journald permissions: a user outside `systemd-journal` sees only their own units] →
  `journalctl` prints a hint on stderr, which reaches the stream through the shared pipe
  and the stream bar.
- [A session from someone else opens `docker://prod-db`] → only the configured `docker`
  command runs, with a validated name, read-only (`docker logs`); the same trust as
  opening their file paths.
- [Clock of the Docker host differs from the local one] → timestamps are compared on the
  clock printed (UTC `Z` from Docker), as for files.

## Migration Plan

Additive. Older builds reading a workspace with a `docker://` path report it as a missing
file and skip it. Rollback: remove the source kinds; saved URIs become missing streams.

## Open Questions

- Automatic reconnect when a container restarts (poll `docker inspect` every 2 s while
  ended), or only the Reconnect button (proposed for 0.22.0)?
- `kubectl logs -f` (`k8s://namespace/pod`) as a third kind in the same release, since it
  is the same design?
- Should `docker://` also list stopped containers in the dialog (`docker ps -a`)?
