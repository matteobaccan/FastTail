## Why

Most logs worth tailing live on servers, not on the desktop running FastTail. Today the user
opens a terminal, runs `ssh web1 tail -F /var/log/app.log | fasttail -`, and gets a `stdin`
tab that is not saved in the workspace or a session, cannot be reopened after a restart,
has no host in its title, and must be rebuilt by hand for every host and file. Following
the same file on three web servers means three terminals and three anonymous tabs. lnav
(`ssh` paths), nerdlog (many hosts over SSH, no agent on the server), LogExpert (SFTP
plugin) and LogViewPlus (SFTP / SCP) all open remote files directly; SnakeTail's issue #21
asks for it. The post-0.12.0 competitor scan ranks remote and system sources as gap 5
(value medium–high); journald and Docker are covered by the separate `system-sources`
change, this change is the full SSH part.

## What Changes

- **Remote file streams**: a stream can be opened from `ssh://[user@]host[:port]/path`
  (the open dialog's new **Remote…** entry, the command line, drag and drop of such a URL
  as text, the recent files list). FastTail runs the system OpenSSH client (`ssh`), which
  runs `tail` on the server; the output is copied into a local spool file that the engine
  tails like any followed log, exactly as the stdin stream does. Every stream feature
  (filters, search, rules, levels, time range, bookmarks, export, collapse) works on it.
- **Initial content**: by default the last 64 MB of the remote file are fetched, then the
  stream follows; the **Whole file** option fetches it all, bounded by the spool cap.
- **Rotation and truncation**: `tail -F` on the server follows the file by name; a
  truncation or a replaced file is shown as it is for a local file (the engine sees a
  truncation of the spool).
- **Globs**: `ssh://host/var/log/app-*.log` behaves like a local pattern stream (newest
  match, rescanned every 10 s on the server, switch with a notice). All matches at once is
  the `folder-source` change's mode, available for remote paths when both have landed.
- **Several hosts**: the Remote dialog accepts a list of hosts for one path
  (`web1, web2, web3`), opening one stream per host, titled `web2:app.log`; combined with
  `merged-timeline-view` they read as one timeline.
- **Authentication** is whatever the user's `ssh` already does: keys, the SSH agent
  (OpenSSH agent, Pageant through the Windows OpenSSH agent), `~/.ssh/config` aliases,
  `ProxyJump`, certificates. FastTail never asks for, stores or passes a password or a
  passphrase: `ssh` runs with `BatchMode=yes`, and a host that needs a password is
  reported with a hint to set up a key or the agent.
- **Host keys**: the user's `known_hosts` is authoritative; an unknown or changed host key
  stops the stream with the fingerprint `ssh` printed and a hint, and FastTail never
  accepts a key on the user's behalf.
- **Connection state** in the stream bar: `connecting`, `following`, `reconnecting (n)`,
  `disconnected: <reason>`, with a **Reconnect** button. A dropped connection is retried
  with back-off (1 s, 2 s, 5 s, 10 s, then every 30 s) and resumes after the last byte
  received, so no line is duplicated or lost while the file was not rotated.
- **Sessions and workspace** keep the source URL (`ssh://deploy@web1:2222/var/log/app.log`)
  and the initial-content choice, never a credential; a restored remote stream reconnects
  in the background.
- New `fasttail.ini` keys: `ssh_program` (default `ssh`, a full path allowed),
  `remote_spool_max_mb` (default 2048, 64–65536) and `remote_initial_mb` (default 64,
  0–4096; 0 = only new lines).

Target release: **0.16.0** (moved from 0.15.0 when 0.14.0 shipped early; sources and integrations), per the release plan in
`docs/competitor-analysis.md` section 8. Priority: **medium–high**. Effort: **M (2–3 weeks)**.

### Non-goals

- A built-in SSH implementation or an SFTP client (design D1), and password or passphrase
  prompts (open question 1).
- Remote hosts without a POSIX shell and `tail` (Windows servers with OpenSSH, network
  appliances); a clear error is shown instead.
- Browsing remote directories in a file dialog; the path is typed or pasted.
- Opening remote compressed files or archives (`.gz` over SSH): a remote file is text as
  it is; a later change may pipe it through the decompression job.
- Writing to the remote host, running arbitrary remote commands, port forwarding.
- Agent or daemon on the server; nothing is installed remotely.

## Capabilities

### New Capabilities

- `ssh-sources`: remote file streams over the system `ssh` client, their identity, state,
  reconnection, persistence and bounds.

### Modified Capabilities

_None_ (the command-line syntax is specified inside `ssh-sources`; `command-line`,
`stream-engine` and the session requirements are extended there so this change does not
collide with `system-sources`).

## Impact

- New `src/ssh_source.rs`: `SshSpec` (parsed URL), `SshStream` (child process, copier,
  state, reconnect), `open_engine` like `stdin_source::open_engine`.
- The stdin copier (`stdin_source::Copier`, `Limits`, `RestartReason`) is generalised into
  a spool feed shared by stdin, SSH and later sources (`src/spool_feed.rs`); if
  `system-sources` lands first, that change does the split and this one reuses it.
- Non-file stream identity (`SourceUri`, `is_remote_path`) shared with `system-sources`:
  `src/session.rs` (`StreamEntry.path` holds the URL, no `rel`), recent files, workspace,
  `src/cli.rs` (URLs accepted as paths), `src/ui/app.rs` (`open_log_file` dispatch, Remote
  dialog), `src/ui/dock.rs` (stream bar state and Reconnect).
- `src/config.rs`: the three keys; Settings → Remote.
- `src/i18n.rs`: new strings in all 16 languages. README (remote section, FAQ on keys and
  agents per platform), CHANGELOG.
- No new crate.
