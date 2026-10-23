## 1. Groundwork

- [x] 1.1 Reuse the shared spool feed and non-file identity helpers; add them if no earlier source change did
- [x] 1.2 `ListenSpec::parse` for `tcp://`, `udp://`, `syslog://` with IPv4, IPv6 in brackets, `localhost`, `?prefix=`; unit tests

## 2. Listener

- [x] 2.1 TCP acceptor (non-blocking accept, 200 ms poll), readers capped at 64, refused-connection counter
- [x] 2.2 UDP reader (65 536-byte buffer, 200 ms timeout), `localhost` binding both loopback addresses
- [x] 2.3 Framing (design D2): newline, octet counting detected per connection, 64 KB cap with truncation marker, partial line on close; unit tests
- [x] 2.4 Syslog parser (RFC 5424, RFC 3164, fallback unchanged), rendering and severity mapping; tests with sample messages from rsyslog, syslog-ng, systemd-journald forwarding, a Cisco device and a BOM-prefixed message
- [x] 2.5 Single writer thread, bounded channel of 1 024, token-bucket rate limit, drop counter and the once-per-second dropped marker line; sender prefix
- [x] 2.6 Integration tests on 127.0.0.1 with an ephemeral port: TCP lines from 3 clients not interleaved mid-line; UDP datagrams; syslog over TCP with octet counting; rate limit drops counted; port in use reported

## 3. UI, command line, persistence

- [x] 3.1 Listen dialog: protocol, address (loopback preselected, warning for others), port, sender prefix
- [ ] 3.2 Stream bar (listening state, connections, lines/s, dropped, refused and the LAN warning done; a bind error is reported at open, without a Retry button): listening state, connections, lines/s, dropped, bind error with Retry; one-time LAN warning after a restore
- [x] 3.3 `--listen <url>` (repeatable) in `src/cli.rs` and the usage text
- [x] 3.4 Sessions, workspace and recent files store the identity; restore re-binds; round-trip test
- [x] 3.5 `listener_spool_max_mb`, `listener_max_lines_per_sec`, `listener_max_mb_per_sec` in `fasttail.ini` and Settings, with bounds tests

## 4. Texts and documentation

- [x] 4.1 New i18n keys in all 16 languages; exhaustive i18n test updated
- [x] 4.2 README section "Network listener" (examples for rsyslog, `logger -n`, Docker's syslog driver, Windows firewall note), command line, comparison table
- [x] 4.3 CHANGELOG `[Unreleased]`: bold opening sentence, before / after

## 5. Wrap-up

- [ ] 5.1 Manual check: `logger -n 127.0.0.1 -P 5514 -d` (UDP) and `--tcp`, a Docker container with `--log-driver syslog`, 50 000 lines/s burst dropped and counted (left to the maintainer: not run in the agent sessions)
- [x] 5.2 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green on Linux and Windows CI
- [x] 5.3 After the release, archive the change so `network-listener` becomes a spec
