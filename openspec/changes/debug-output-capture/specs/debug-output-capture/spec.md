## ADDED Requirements

### Requirement: Debug Output Stream
On Windows the application SHALL open the debug output of the current session (`OutputDebugString` of processes without a debugger attached) as a followed stream titled `debug output`, from an open-menu entry or from the command-line argument `dbwin://local`. Each message SHALL be written as one line `<local time in ISO 8601 with milliseconds> [<pid> <process name>] <message>`, the process name being `?` when it cannot be read; a trailing line break SHALL be dropped and further lines of a message SHALL follow as continuation lines indented by two spaces. Messages SHALL be decoded from the system ANSI code page. The capture SHALL release each message to the writing process without waiting for disk I/O, SHALL keep at most 65 536 messages queued, and SHALL report the number of messages dropped beyond that bound. The lines SHALL be written into a temporary spool bounded like the standard input stream (`stdin_spool_max_mb` and a 512 MB free-space margin, restart from empty with a notice). The entry SHALL NOT be offered on Linux and macOS.

#### Scenario: Tracing application
- **WHEN** the debug output stream is open and `app.exe` with pid 4242 calls `OutputDebugStringW(L"ERROR cache miss\n")`
- **THEN** within 1 second the stream shows a line like `2026-09-28T14:02:05.123 [4242 app.exe] ERROR cache miss`, counted as an ERROR line.

#### Scenario: Multi-line message
- **WHEN** a process writes one message holding three lines
- **THEN** the stream shows the first line with the prefix and two continuation lines indented by two spaces.

#### Scenario: Not offered elsewhere
- **WHEN** FastTail runs on Linux
- **THEN** the open menu has no debug output entry and `dbwin://local` on the command line is reported as unsupported on this platform.

### Requirement: Debug Output Capture Scope and Filter
The dialog SHALL offer global capture (`dbwin://global`, including services in session 0), enabled only when FastTail runs with administrator rights and otherwise disabled with the reason; the application SHALL NOT ask for elevation. The dialog SHALL offer include and exclude lists of process names (with `*` and `?` wildcards, case-insensitive) and pids, applied before a message is written; FastTail's own process SHALL be excluded by default. When another debug monitor already owns the debug output buffer, the stream SHALL end with a message saying so and offer a Retry action, and SHALL NOT share the buffer.

#### Scenario: Only one program
- **WHEN** the include list is `myapp*.exe` and both `myapp.exe` and `other.exe` write debug output
- **THEN** only the messages of `myapp.exe` appear.

#### Scenario: DebugView already running
- **WHEN** Sysinternals DebugView is capturing and the user opens the debug output stream
- **THEN** the stream says that another debug monitor is running, captures nothing, and Retry opens the capture once DebugView is closed.

### Requirement: Debug Output Persistence
The workspace and session files SHALL store the stream as `dbwin://local` or `dbwin://global` with its include and exclude lists, and SHALL restart the capture on restore with an empty stream; earlier messages SHALL NOT be restored. The stream SHALL NOT be added to the recent files, and its spool SHALL be deleted when the stream is closed and at exit.

#### Scenario: Restored after restart
- **WHEN** FastTail is closed with a debug output stream filtered on `myapp*.exe` and started again
- **THEN** the debug output stream is open again, empty, with the same include list, and new messages of `myapp.exe` appear.
