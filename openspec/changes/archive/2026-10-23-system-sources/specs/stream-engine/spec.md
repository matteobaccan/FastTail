## ADDED Requirements

### Requirement: System Log Sources
The application SHALL open two kinds of system log source as streams, named by a URI: `journald://` for the whole systemd journal, `journald://<unit>` for one system unit and `journald://user/<unit>` for one user unit, on Linux only; and `docker://<container>` for a container name or id, on every platform. A journald source SHALL run the command configured as `journalctl_command` (default `journalctl`) with the arguments `--follow --no-pager --output=short-iso-precise --lines=<backlog>`, `--user` for a user unit and `--unit=<unit>` for a unit; a Docker source SHALL run the command configured as `docker_command` (default `docker`) with the arguments `logs --follow --timestamps --tail <backlog> -- <container>`, where `<backlog>` is `system_source_backlog_lines` from `fasttail.ini` (default 10,000, from 0 to 1,000,000). Arguments SHALL be passed to the program directly, never through a shell. A unit name SHALL be 1 to 256 characters among letters, digits and `@ _ . : \ -` and SHALL NOT start with `-`; a container SHALL be a name of 1 to 255 characters among letters, digits and `_ . -` starting with a letter or digit, or 12 to 64 hexadecimal digits; any other value SHALL be refused with a message. The standard output and the standard error of the command SHALL be copied, in the order the command wrote them, into a temporary spool file tailed as a followed stream, under the same size cap (`stdin_spool_max_mb`), free-space margin and lifecycle as the standard-input spool, so that every stream feature works on it and no copy of the output is held in memory. A line written by the command SHALL appear in the stream within 100 milliseconds. When the command ends, the stream SHALL stay open with its lines and the stream bar SHALL say that the source ended, with the exit code and the last line the command wrote. A Reconnect action SHALL run the command again, appending to the same stream only the lines received after the last line already in it. A `journald://` URI on a platform other than Linux SHALL be refused with a message.

#### Scenario: Following a unit
- **WHEN** the user opens `journald://nginx.service` and nginx logs a request
- **THEN** a stream titled `journal: nginx.service` shows the last 10,000 journal lines of the unit followed by the new line, each starting with its ISO 8601 timestamp, and the time range works on it.

#### Scenario: Container stderr is kept
- **WHEN** the user opens `docker://api` and the container writes one line to its standard output and then one to its standard error
- **THEN** both lines appear in the `docker: api` stream, in that order.

#### Scenario: Container stops and starts again
- **WHEN** the `api` container stops, the stream bar says the source ended with the exit code, the container is started again and the user presses Reconnect
- **THEN** the new lines are appended to the same stream and no line already shown appears twice.

#### Scenario: Option injection refused
- **WHEN** the user opens `docker://--help` or `journald://-x`
- **THEN** no command is run and a message says the name is not valid.

#### Scenario: journald on Windows
- **WHEN** the user runs `fasttail journald://sshd.service` on Windows
- **THEN** no stream opens and the message says journald sources are available on Linux only.

### Requirement: System Source Process Lifecycle
The command of a system source SHALL be started without showing a console window, SHALL be ended when its stream is closed and when the application exits, and its spool SHALL then be deleted; after a crash, the spool SHALL be deleted at the next start like any orphaned spool, and the command SHALL end on its next write to the closed pipe. The command SHALL be one of the two programs configured in `fasttail.ini`; a URI in a session or workspace file SHALL NOT be able to name another program or pass other arguments.

#### Scenario: Closing the tab
- **WHEN** the user closes the `docker: api` tab
- **THEN** the `docker logs` process of that stream is no longer running and its spool file is removed from disk.

#### Scenario: No console flash on Windows
- **WHEN** the user opens `docker://api` on Windows
- **THEN** no console window appears while the `docker` command runs.
