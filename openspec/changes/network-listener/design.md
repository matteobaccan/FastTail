## Context

The stdin stream (`src/stdin_source.rs`) shows how a non-seekable source becomes a stream:
a background copier writes flushed chunks into a `SpoolFile` (`src/spool.rs`), wakes the
engine, restarts the spool from empty at its cap or when the volume keeps less than
512 MB, and the engine tails the spool under a pseudo-path identity. `src/log_level.rs`
recognises `FATAL`…`TRACE` words and syslog `<n>` priorities; `src/timestamp.rs` reads ISO
8601 and syslog timestamps. FastTail has no async runtime; heavy work runs on threads.

## Goals / Non-Goals

**Goals:** TCP, UDP and syslog as streams; safe default binding; bounded threads, memory,
rate and disk; readable syslog lines with correct levels; persistence. **Non-goals:** see
proposal.

## Decisions

### D1. Blocking `std::net` with a thread per connection, capped

One acceptor thread per TCP listener (`TcpListener`, non-blocking accept polled every
200 ms so closing is observed), one reader thread per TCP connection (at most 64: the
65th connection is accepted and closed at once, counted as refused), one thread per UDP
socket (`recv_from` with a 200 ms read timeout, 65 536-byte buffer). All readers hand
complete messages to one writer thread per listener through a bounded channel of 1 024
messages; the writer applies the rate limit, renders syslog, prefixes the sender and
writes into the spool feed, so the spool has a single writer and lines from different
connections are never interleaved mid-line.

Rejected: an async runtime (`tokio` / `mio`): 64 connections do not justify a runtime
the rest of the application does not use, and blocking reads with timeouts keep the code
like the stdin copier's. Rejected: one stream per connection: senders reconnect often
(every syslog relay restart), which would scatter the log across tabs.

### D2. Framing and caps

- TCP plain: split on `\n` (a trailing `\r` removed); a line over 64 KB is cut with the
  engine's truncation marker and the rest up to the next `\n` discarded.
- Syslog over TCP: if the first byte of a connection is a digit and the bytes up to the
  first space parse as a length ≤ 65 536, the connection uses octet counting (RFC 6587
  §3.4.1), else newline framing, decided once per connection.
- UDP: one datagram, one message; embedded `\n` split it into lines.
- The per-connection read buffer is 64 KB; a partial line pending when the connection
  closes is written as a line.

### D3. Syslog rendering

The parser tries RFC 5424 (`<PRI>1 TIMESTAMP HOST APP PROCID MSGID [SD] MSG`), then RFC
3164 (`<PRI>Mmm dd hh:mm:ss HOST TAG[pid]: MSG`, year taken from the receive time, and
the receive time used when the timestamp is missing or unparseable). Output line:
`<ISO 8601 timestamp with offset> <LEVEL> <host> <app>[<pid>]: <msg>`, with RFC 5424
structured data appended as `[sd-id k="v" …]` and a UTF-8 BOM in MSG removed. Severity →
level: 0 emerg, 1 alert, 2 crit → `FATAL`; 3 → `ERROR`; 4 → `WARN`; 5 notice, 6 info →
`INFO`; 7 → `DEBUG`. A message that parses as neither is written unchanged. Facility is
not shown (open question 2).

### D4. Bind policy

The dialog proposes `127.0.0.1`; `syslog://` and `udp://` with host `localhost` bind both
`127.0.0.1` and `::1`. Any other address (a LAN address, `0.0.0.0`, `::`) needs the user to
pick it explicitly, and the dialog shows "other machines on the network can send lines
to this stream"; the same warning shows once in the stream bar after a restore. The
command line accepts any address (the user typed it). Ports below 1024 fail on Linux and
macOS without privileges: the error names the reason and suggests a port above 1024 with
the sender pointed at it. On Windows the first bind to a non-loopback address may raise
the firewall prompt; nothing is done to bypass it.

### D5. Rate limit and accounting

A token bucket per listener: `listener_max_lines_per_sec` lines and
`listener_max_mb_per_sec` MB per second, burst of one second. Messages over the limit are
dropped at the writer and counted; the stream bar shows `N dropped` and a line
`-- fasttail: 1 523 messages dropped (rate limit) --` is written once per second while
dropping, so the gap is visible in the log itself. When the writer's channel is full,
readers block (TCP back-pressure to the sender) or, for UDP, drop and count.

### D6. Identity, persistence, threads and memory

Identity: `tcp://addr:port`, `udp://addr:port`, `syslog://addr:port` plus
`?prefix=on|off`. Title `syslog :514`; the tooltip shows the full address. Sessions,
workspace and recent files store the identity; a restore re-binds in the background and
shows the error if the port is taken. Bookmarks are not persisted (content differs at
every run). Threads per listener: 1 acceptor, ≤ 64 readers, 1 UDP reader, 1 writer.
Memory per listener: ≤ 64 × 64 KB read buffers (4 MB worst case), the 1 024-message
channel (≤ 64 MB worst case at the 64 KB cap, typically under 1 MB), the engine index
(8 bytes per line); content lives in the spool, bounded by `listener_spool_max_mb` and
the 512 MB free-space margin, with the stdin restart-from-empty rule. The UI thread only
reads counters. Closing the stream closes the sockets (connections see a reset) and
deletes the spool.

## Risks / Trade-offs

- [A LAN-bound listener lets anyone write lines into the view] → loopback by default,
  explicit choice and warning; lines are only text (no escape sequence is interpreted
  unless the user's ANSI mode renders colours, which cannot run anything).
- [UDP loss under load is silent at the OS level] → documented; drops FastTail makes are
  counted.
- [Thread count with 64 connections × several listeners] → caps per listener; readers
  are idle-blocking threads with 64 KB stacks.

## Open Questions

1. Should a listener optionally split senders into one stream each (auto-created tabs)?
2. Show the syslog facility (`daemon`, `local0`) in the line, or only as a future field
   of `structured-fields`?
