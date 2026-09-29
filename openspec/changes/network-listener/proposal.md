## Why

Network devices, embedded boards, containers without a mounted log volume and many
appliances do not write a file the user can open: they send syslog over UDP or TCP, or
write lines to a socket. Today the user has to run a separate syslog server or
`nc -lk 5140 | fasttail -` (no `nc` on stock Windows), and the result is an anonymous
`stdin` tab that is not saved and loses the sender's identity. LogViewPlus has a TCP
listener and Chipmunk reads TCP / UDP streams; the post-0.12.0 competitor scan ranks a
TCP / UDP / syslog listener gap 13 (value medium).

## What Changes

- **Listener streams**: a new **Listen…** entry opens a stream that receives log lines on
  a local port: `tcp://127.0.0.1:5140`, `udp://127.0.0.1:5140` or
  `syslog://127.0.0.1:514` (UDP and TCP on the same port). Received data is copied into a
  spool file tailed like the stdin stream, so every stream feature works on it.
- **Framing**: TCP by newline; syslog over TCP by newline or octet counting (RFC 6587),
  detected per connection; one UDP datagram is one message (several lines if it holds
  line breaks).
- **Syslog parsing** (RFC 5424 and RFC 3164, auto-detected per message): the line is
  written as `<timestamp> <LEVEL> <host> <app>[<pid>]: <message>` with severity mapped to
  FastTail's level words (0–2 → `FATAL`, 3 → `ERROR`, 4 → `WARN`, 5–6 → `INFO`,
  7 → `DEBUG`); RFC 5424 structured data is kept in brackets. Unparseable messages are
  kept as they arrived.
- **Sender prefix** (optional, on by default for UDP and syslog): `[10.0.0.7]` before
  each line, so several senders on one listener stay apart and can be filtered.
- **Safe by default**: bound to `127.0.0.1` (and `::1`) unless the user picks another
  address in the dialog, which then warns that other machines can send to it; no reply is
  ever sent to a sender.
- **Bounds**: at most 64 TCP connections per listener, a 64 KB line / message cap (longer
  ones cut with the long-line marker), a rate limit per listener (default 20 000
  lines/s and 16 MB/s, excess dropped and counted in the stream bar), and the spool cap
  and free-space margin of the stdin stream.
- **State in the stream bar**: `listening on 127.0.0.1:5140 · 3 connections ·
  1 204 lines/s · 0 dropped`; a port already in use or not allowed (below 1024 without
  privileges on Linux and macOS) is shown with the reason and a Retry button.
- **Persistence**: sessions and the workspace keep the listener URL and options and
  re-bind on restore; `fasttail --listen syslog://127.0.0.1:5514` opens one from the
  command line. New `fasttail.ini` keys: `listener_spool_max_mb` (default 2048,
  64–65536), `listener_max_lines_per_sec` (default 20000, 100–1000000),
  `listener_max_mb_per_sec` (default 16, 1–1024).
- **OTLP** is not part of this change: the `otlp-receiver` change is its phase 2 and
  reuses these listener streams, bounds and settings.

Target release: **0.22.0** (planned for 0.15.0, moved after the terminal interface of 0.20.0; sources and integrations), per the release plan in
`docs/competitor-analysis.md` section 8. Priority: **medium**. Effort: **M (about 2 weeks)**.

### Non-goals

- TLS (syslog over TLS, RFC 5425), authentication of senders, and relaying or forwarding
  messages elsewhere.
- Acting as a durable log server: when FastTail is closed, messages are not received.
- Binary protocols (GELF, Fluent Forward, DLT), and OTLP (see `otlp-receiver`).
- One stream per sender created automatically (open question 1).

## Capabilities

### New Capabilities

- `network-listener`: TCP, UDP and syslog listener streams, framing, syslog rendering,
  bind policy, bounds, state and persistence.

### Modified Capabilities

_None._

## Impact

- New `src/net_listener.rs`: `ListenSpec` (parsed URL), `Listener` (acceptor and UDP
  threads, per-connection readers, rate limiter, writer into the shared spool feed),
  `syslog` parser module (`src/syslog.rs`); reuses `log_level` words.
- Shared spool feed (`spool_feed`, split from `stdin_source` by the first source change)
  and non-file identity helpers; `src/session.rs`, workspace and recent files store
  `tcp://`, `udp://`, `syslog://` identities.
- `src/cli.rs`: `--listen <url>` (repeatable). `src/ui/app.rs`: Listen dialog;
  `src/ui/dock.rs`: listener state in the stream bar. `src/config.rs`: three keys.
- `src/i18n.rs`: new strings in all 16 languages; README and CHANGELOG.
- No new crate (`std::net` and threads).
