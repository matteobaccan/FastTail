## ADDED Requirements

### Requirement: System Source URIs on the Command Line
A PATH argument that starts with `journald://` or `docker://` SHALL be opened as the corresponding system log source (see the stream-engine capability) after the workspace is restored, skipping a source already open, and SHALL NOT be resolved against the current directory. An invalid source SHALL be reported on standard error without preventing the other paths from opening. `--filter`, `--exclude` and `--follow` SHALL apply to these streams as to files.

#### Scenario: Two sources and a file
- **WHEN** the user runs `fasttail --filter ERROR docker://api journald://nginx.service app.log`
- **THEN** the window opens with the streams `docker: api`, `journal: nginx.service` and `app.log`, each with the include term `ERROR`.

#### Scenario: Invalid container name
- **WHEN** the user runs `fasttail docker://-v app.log`
- **THEN** standard error says that `-v` is not a valid container name and the window opens with `app.log`.
