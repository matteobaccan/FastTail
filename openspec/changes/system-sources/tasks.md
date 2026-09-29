## 1. Source core

- [ ] 1.1 `src/system_source.rs`: `SystemUri { Journald { user: bool, unit: Option<String> }, Docker { container: String } }` with `parse` / `to_uri` / `title`; validation of units and container names; refusal of `journald://` outside Linux
- [ ] 1.2 Command lines for both kinds from `journalctl_command`, `docker_command` and `system_source_backlog_lines`; child spawned with one `std::io::pipe()` for standard output and error, `CREATE_NO_WINDOW` on Windows
- [ ] 1.3 Generalise the standard-input copier as a piped source (`StdinStream::start` already takes `impl Read`): spool name per source, reopening the same spool in append mode for a reconnect, end state with exit code and last line
- [ ] 1.4 Unit tests: URI parsing and validation (option injection, empty names, hex ids), command lines per kind and platform, a fake child (a test binary printing lines to stdout and stderr) copied in order into the spool, end state and exit code

## 2. Engine and lifecycle

- [ ] 2.1 `TailEngine.system: Option<SystemStream>` next to `stdin` and `compressed` (declared last so the file handle closes before the spool is deleted); pseudo-path is the URI; follow on
- [ ] 2.2 Close kills the child and deletes the spool; exit kills every child; crash leaves an orphaned spool swept at the next start
- [ ] 2.3 Reconnect: last received timestamp kept, command restarted with `--since`, leading duplicates dropped, appended to the same spool
- [ ] 2.4 Tests with the fake child: stream grows live, child exit shows the end state, reconnect appends without duplicates, spool cap restart handled as a truncation

## 3. UI, command line and persistence

- [ ] 3.1 `cli.rs`: `journald://` and `docker://` recognised before path resolution; tests
- [ ] 3.2 "Open system source…" dialog: kind selector (journald on Linux only, Docker when the command is found), background listing (`docker ps`, `systemctl list-units`, with `--user`), errors shown, free text field
- [ ] 3.3 Stream bar: source status (running, ended with exit code and last line), Reconnect button; tab titles `docker: <name>` / `journal: <unit>`
- [ ] 3.4 Workspace, recent files and sessions store the URI unchanged; `source_exists` true for URIs; bookmarks and notes not saved for system sources; reopen at start runs the command again; round-trip tests
- [ ] 3.5 Settings and `fasttail.ini`: `system_source_backlog_lines` (0–1,000,000, default 10,000), `docker_command` (default `docker`), `journalctl_command` (default `journalctl`); round-trip test

## 4. Texts and documentation

- [ ] 4.1 New i18n keys (menu entry, dialog, kinds, status texts, Reconnect, Settings labels, refusal messages, help entry) in all 16 languages; add them to the exhaustive i18n test
- [ ] 4.2 README (feature list, "System sources" section with journald / Docker / Podman examples, comparison table, FAQ replacing the `docker logs | fasttail -` advice), CHANGELOG `[Unreleased]`
- [ ] 4.3 Manual check on Linux (journald system and user units, Docker, Podman) and on Windows with Docker Desktop, recorded in the PR

## 5. Wrap-up

- [ ] 5.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PR with Linux and Windows CI green
- [ ] 5.2 Local preview exe for the maintainer before the 0.22.0 release
- [ ] 5.3 After the release, archive the change so `stream-engine`, `command-line` and `cyber-ui-docking` gain the new requirements
- [ ] 5.4 Check that the `ssh-sources` proposal (phase 2) covers the points listed in the design and reuses the piped source
