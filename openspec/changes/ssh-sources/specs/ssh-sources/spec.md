## ADDED Requirements

### Requirement: Remote File Streams over SSH
The application SHALL open a stream from a URL `ssh://[user@]host[:port]/absolute/path`, given in the Remote dialog, on the command line, or from the recent files, workspace or a session, by running the configured OpenSSH client (`ssh_program`, default `ssh`) in batch mode with a remote `tail` command and copying its output into a temporary spool file that is tailed as a followed stream, so that every stream feature works on it and the remote content is never held in memory. By default the last `remote_initial_mb` MB (default 64, 0 to 4096, 0 meaning only new lines) of the remote file SHALL be fetched before following; the Whole file option SHALL fetch it from the first byte. When the fetch starts after the first byte, the partial first line SHALL be dropped. A host that starts with `-` or contains characters other than letters, digits, `.`, `_`, `-`, `:`, `[` and `]` SHALL be refused before any process is started, and the remote path SHALL reach the remote shell as a single quoted word. On Windows no console window SHALL appear. The stream SHALL be titled `host:file-name` and its full URL SHALL be shown in the tab tooltip and the footer.

#### Scenario: Following a remote log
- **WHEN** the user opens `ssh://deploy@web1/var/log/app.log`, a 3 GB file, with the default settings and a working key in the agent
- **THEN** the stream `web1:app.log` shows the last 64 MB of the file starting at a whole line, then each line appended on the server appears within 1 second, and filters and highlight rules apply to it.

#### Scenario: Whole file
- **WHEN** the user opens a 20 MB remote file with Whole file selected
- **THEN** its first row is line 1 of the remote file and the line count equals the file's.

#### Scenario: Option injection refused
- **WHEN** the URL `ssh://-oProxyCommand=calc/tmp/x` is opened
- **THEN** it is refused with an "invalid host" message and no `ssh` process is started.

### Requirement: SSH Credentials Stay with the SSH Client
Authentication SHALL be performed entirely by the OpenSSH client with the user's own configuration (keys, agent, `~/.ssh/config`, jump hosts, `known_hosts`). The application SHALL NOT prompt for, store, log or pass a password or passphrase, and SHALL NOT accept an unknown or changed host key on the user's behalf. A connection refused because a password is required, or because the host key is unknown or changed, SHALL stop the stream with the reason and the text printed by the client, and SHALL NOT be retried automatically.

#### Scenario: Password-only server
- **WHEN** the user opens a remote file on a host that accepts only passwords
- **THEN** the stream shows `disconnected: password authentication is not supported` with a hint to use a key or the agent, and no further connection attempt is made until the user presses Reconnect.

#### Scenario: Unknown host key
- **WHEN** the host is not in the user's `known_hosts`
- **THEN** the stream stops with the host key fingerprint printed by `ssh` and a hint to connect once from a terminal, and `known_hosts` is left unchanged.

#### Scenario: No credential in the session file
- **WHEN** a session holding two remote streams is saved and the file is inspected
- **THEN** it contains the two `ssh://` URLs and their initial-content choice, and no password, passphrase, key material or agent socket path.

### Requirement: Remote Stream Connection State and Reconnection
The stream bar of a remote stream SHALL show its state: `queued`, `connecting`, `following`, `reconnecting (n)` or `disconnected: <reason>`, with a Reconnect action when disconnected and a Disconnect action otherwise. A connection lost for a network reason SHALL be retried after 1, 2, 5 and 10 seconds and then every 30 seconds until it succeeds, the user disconnects or the stream is closed. On reconnection, when the remote file has the same identity and is at least as long as the bytes already received, the stream SHALL resume after the last byte received, without duplicating or losing a line; otherwise the spool SHALL restart from empty, the initial content SHALL be fetched again, and the stream bar SHALL say the remote file was replaced. At most 32 SSH client processes SHALL run at the same time; further remote streams SHALL wait in the `queued` state. Closing the stream SHALL end its SSH process and delete its spool.

#### Scenario: Network drop
- **WHEN** the network is lost for 20 seconds while `web1:app.log` is followed and the server writes 50 lines meanwhile
- **THEN** the stream bar shows `reconnecting (n)`, and after the network returns the 50 lines appear once each, in order, after the last line received before the drop.

#### Scenario: Rotated while disconnected
- **WHEN** the remote file is rotated during a disconnection
- **THEN** on reconnection the stream restarts with the initial content of the new file and says the remote file was replaced.

#### Scenario: Many hosts
- **WHEN** the user opens the same path on 40 hosts
- **THEN** 32 streams connect and 8 show `queued`, each queued stream connecting as soon as another one ends.

### Requirement: Remote Pattern Streams and Several Hosts
A remote URL whose file name holds `*` or `?` SHALL follow the newest matching file on the server, listing the directory every 10 seconds and switching to a newer match as a local pattern stream does (filters, highlight rules, search, wrap and encoding kept; buffer, bookmarks and selection reset; a switch notice shown); only `*` and `?` SHALL be interpreted by the remote shell. The Remote dialog SHALL accept a comma- or newline-separated list of hosts for one path and open one independent stream per host.

#### Scenario: Remote daily rotation
- **WHEN** the stream was opened as `ssh://web1/var/log/app-*.log` and the server creates `app-2026-09-29.log`
- **THEN** within 10 seconds the stream tails the new file and the stream bar reports the switch.

#### Scenario: Three web servers
- **WHEN** the user enters the hosts `web1, web2, web3` and the path `/var/log/nginx/error.log`
- **THEN** three streams titled `web1:error.log`, `web2:error.log` and `web3:error.log` open, and a failure on `web2` does not affect the other two.

### Requirement: Remote Streams in Sessions and on the Command Line
A remote stream SHALL be saved in the workspace, the recent files and session files by its URL and initial-content choice, and restored by reconnecting in the background. Its bookmarks SHALL be saved only when it was fetched as a whole file. Arguments of the form `ssh://…` on the command line SHALL open remote streams, and `--filter`, `--exclude`, `--follow` and `--no-follow` SHALL apply to them. The spool of a remote stream SHALL be bounded by `remote_spool_max_mb` (default 2048 MB, 64 to 65536) and a 512 MB free-space margin with the same restart-from-empty behaviour and notice as the standard input stream.

#### Scenario: Session restored after a restart
- **WHEN** FastTail is started with a session holding `ssh://web1/var/log/app.log` with an include filter `ERROR`
- **THEN** the stream reappears with the filter set, shows `connecting` and then the remote lines, without any dialog.

#### Scenario: Command line
- **WHEN** FastTail is started as `fasttail --filter timeout ssh://web1/var/log/app.log`
- **THEN** one remote stream opens with the include filter `timeout`.
