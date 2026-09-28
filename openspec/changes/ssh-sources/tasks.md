## 1. Shared groundwork

- [ ] 1.1 Split the stdin copier into `src/spool_feed.rs` (`SpoolFeed`, `Limits`, `InputState`, `RestartReason`), `StdinStream` wrapping it; stdin tests unchanged and green (skip if `system-sources` already did it)
- [ ] 1.2 `SourceUri` / `is_remote_path` for non-file identities, used by the workspace, recent files, sessions and `source_exists` (skip the parts `system-sources` already added)

## 2. SSH stream

- [ ] 2.1 `SshSpec::parse`: `ssh://[user@]host[:port]/path`, percent-decoding, absolute path, host charset and no leading `-`, IPv6 in brackets; unit tests including `ssh://-oProxyCommand=x/p` refused
- [ ] 2.2 Command builder (design D2): fixed options, `-l` / `-p`, `--`, ControlMaster options off Windows, `CREATE_NO_WINDOW` on Windows, remote script with `quote_sh_arg`; unit tests on the argv produced
- [ ] 2.3 `SshStream`: supervisor loop, `FASTTAIL1` marker parsing, drop to the first `\n` on a mid-file start, stderr ring of 4 KB, states `queued` / `connecting` / `following` / `reconnecting(n)` / `disconnected(reason)`
- [ ] 2.4 Reconnect with back-off 1, 2, 5, 10, 30 s; resume by inode and size; refetch and spool restart when the file was replaced; no retry on authentication, host key or missing file errors
- [ ] 2.5 Global limit of 32 concurrent `ssh` children with a queue
- [ ] 2.6 Remote pattern streams: glob quoting per D4, 10 s rescans, switch like local pattern streams
- [ ] 2.7 Tests with a fake `ssh_program` (a test binary that echoes the marker and streams a fixture, can exit 255, can print "Permission denied"): initial tail fetch, whole-file fetch, resume without duplicates, replaced file, auth failure without retry, cap restart

## 3. UI, command line, sessions

- [ ] 3.1 Open dialog **Remote…**: URL or host list + path, initial content (last N MB / whole file); one stream per host
- [ ] 3.2 Stream bar: connection state, Reconnect / Disconnect, "remote file replaced" notice; title `host:name`, full URL in the tooltip and footer
- [ ] 3.3 `src/cli.rs`: `ssh://` arguments open remote streams; `--filter`, `--exclude`, `--follow` apply
- [ ] 3.4 Sessions and workspace: `path=<url>`, no `rel`, `remote_initial=`; bookmarks persisted only for whole-file streams; round-trip test; restored streams reconnect in the background
- [ ] 3.5 Settings → Remote: `ssh_program`, `remote_spool_max_mb`, `remote_initial_mb`, detected `ssh -V`; keys in `fasttail.ini` with bounds tests

## 4. Texts and documentation

- [ ] 4.1 New i18n keys (dialog, states, errors: password required, unknown host key, no `tail`, file missing, file replaced, queued) in all 16 languages; exhaustive i18n test updated
- [ ] 4.2 README: "Remote files over SSH" section (requirements per platform, agent setup on Windows, what is never stored), command line, FAQ; comparison table row
- [ ] 4.3 CHANGELOG `[Unreleased]`: bold opening sentence, before / after

## 5. Wrap-up

- [ ] 5.1 Manual check against a Linux server (GNU), an Alpine container (BusyBox) and a macOS host, from Windows 11 and Linux; a dropped Wi-Fi reconnects without duplicate lines
- [ ] 5.2 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green on Linux and Windows CI
- [ ] 5.3 After the release, archive the change so `ssh-sources` becomes a spec
