## ADDED Requirements

### Requirement: Listener Streams
The application SHALL open a stream that receives log lines on a local address: `tcp://addr:port` (newline-framed lines), `udp://addr:port` (one datagram per message) or `syslog://addr:port` (UDP and TCP on the same port, TCP framed by newline or by octet counting as in RFC 6587, detected once per connection). Received lines SHALL be written by a single writer into a temporary spool file tailed as a followed stream, so that every stream feature works on it and lines from different connections are never interleaved within a line. A line or message longer than 64 KB SHALL be cut with the long-line marker. An optional sender prefix `[<address>]`, on by default for `udp://` and `syslog://`, SHALL precede each line. Closing the stream SHALL close its sockets and delete its spool.

#### Scenario: Three TCP senders
- **WHEN** three clients connect to `tcp://127.0.0.1:5140` and each writes 1 000 lines concurrently
- **THEN** the stream holds 3 000 whole lines, none mixing text of two clients, each shown within 1 second of arriving.

#### Scenario: Syslog over TCP with octet counting
- **WHEN** a sender writes `87 <34>1 2026-09-28T14:02:05.003Z web1 app 42 ID7 - payment failed` messages to `syslog://127.0.0.1:5514` over TCP
- **THEN** each message becomes one line, whatever line breaks it contains.

### Requirement: Syslog Message Rendering
Messages received by a `syslog://` listener SHALL be parsed as RFC 5424 or, failing that, RFC 3164, and written as `<timestamp> <LEVEL> <host> <app>[<pid>]: <message>`, the timestamp in ISO 8601 with its offset (the receive time when the message has none or it cannot be parsed, and the receive year for RFC 3164), RFC 5424 structured data kept in brackets after the message, and a leading byte order mark removed. Severities SHALL map to levels as 0, 1 and 2 → `FATAL`, 3 → `ERROR`, 4 → `WARN`, 5 and 6 → `INFO`, 7 → `DEBUG`. A message that parses as neither format SHALL be written as received.

#### Scenario: BSD syslog from a router
- **WHEN** the UDP datagram `<28>Sep 28 14:02:05 edge-rtr ospfd[311]: neighbor 10.0.0.2 down` arrives from 10.0.0.1
- **THEN** the line reads `[10.0.0.1] 2026-09-28T14:02:05+02:00 WARN edge-rtr ospfd[311]: neighbor 10.0.0.2 down` (local offset), is counted as WARN, and the time range can select it.

#### Scenario: Unparseable message
- **WHEN** a datagram holds `hello world`
- **THEN** the line reads `[<sender>] hello world`.

### Requirement: Listener Binding Safety
A listener SHALL bind to the loopback address unless the user explicitly chooses another address, in which case the dialog SHALL warn that other machines can send lines to the stream, and a restored listener bound to a non-loopback address SHALL show that warning once in its stream bar. The application SHALL never send data to a sender. When the address cannot be bound (port in use, port below 1024 without privileges, address not local), the stream SHALL show the reason and a Retry action and SHALL NOT fall back to another port or address by itself.

#### Scenario: Default binding
- **WHEN** the user opens a listener from the dialog without changing the address
- **THEN** it binds to `127.0.0.1`, and a connection attempt from another machine is refused by the operating system.

#### Scenario: Port already in use
- **WHEN** a listener on UDP port 514 is opened while a syslog daemon already uses it
- **THEN** the stream shows that the port is in use with a Retry action and binds nothing else.

### Requirement: Listener Bounds and Persistence
A TCP listener SHALL accept at most 64 simultaneous connections, closing further ones at once and counting them. Each listener SHALL accept at most `listener_max_lines_per_sec` lines (default 20 000, 100 to 1 000 000) and `listener_max_mb_per_sec` MB (default 16, 1 to 1024) per second with a one-second burst; messages above the limit SHALL be dropped and counted, the stream bar SHALL show the count, and while dropping a marker line stating the number dropped SHALL be written at most once per second. The spool SHALL be bounded by `listener_spool_max_mb` (default 2048 MB, 64 to 65536) and a 512 MB free-space margin with the standard input stream's restart-from-empty behaviour and notice. The stream bar SHALL show the address, the open connections, the lines per second and the dropped count. Listener streams SHALL be saved in the workspace, recent files and sessions by their URL and options and SHALL bind again when restored, without their bookmarks; `--listen <url>` on the command line SHALL open one per occurrence.

#### Scenario: Burst over the limit
- **WHEN** a sender pushes 50 000 lines within one second to a listener with the default limit
- **THEN** 20 000 lines are written, the stream bar shows 30 000 dropped, and a marker line in the log states the number dropped.

#### Scenario: Restored after a restart
- **WHEN** FastTail is started with a session holding `syslog://127.0.0.1:5514`
- **THEN** the listener binds again without any dialog and its stream bar shows `listening on 127.0.0.1:5514`.
