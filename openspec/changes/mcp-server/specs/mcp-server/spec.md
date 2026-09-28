## ADDED Requirements

### Requirement: MCP Server Enabling
FastTail SHALL include a Model Context Protocol server that is off by default and is enabled in Settings, with the state kept in `fasttail.ini` `[mcp]` (`enabled`, `http`, `port`, `token`, `allow_write`, `open_roots`). Enabling it SHALL show a warning that log content becomes readable by the connected assistants. While it is on, the title bar SHALL show an indicator with the number of requests in the last minute, and the Settings page SHALL list the last 200 requests with tool name, arguments and response size.

#### Scenario: Off by default
- **WHEN** FastTail starts with no `[mcp]` section
- **THEN** no port is bound and `fasttail --mcp` reports that the server is disabled in the running window.

#### Scenario: Indicator
- **WHEN** the server is on and an assistant calls `search` three times
- **THEN** the title-bar indicator shows 3 requests and the log lists the three calls.

### Requirement: MCP Transports
The server SHALL offer a stdio transport through `fasttail --mcp`, which connects to the running FastTail window, and, when no window is running, serves the files given on its command line read-only without opening a window. When `[mcp] http=true` the window SHALL also serve MCP over HTTP bound to `127.0.0.1` only, on the configured port (default `47821`). Every HTTP request SHALL carry the bearer token of `[mcp] token`, generated randomly on first enable; a request without the right token, or with an `Origin` header that is not a loopback origin, SHALL be rejected with status 401 or 403.

#### Scenario: Missing token
- **WHEN** a client posts to the MCP endpoint without an Authorization header
- **THEN** the server answers 401 and no tool runs.

#### Scenario: Bridge without a window
- **WHEN** no FastTail window runs and a client starts `fasttail --mcp app.log`
- **THEN** `list_streams` returns `app.log` and no window opens.

### Requirement: Read-Only Tools
The server SHALL offer the tools `list_streams`, `read_lines`, `search`, `get_context`, `list_bookmarks`, `get_filters` and `level_summary`, and each open stream as a resource. They SHALL answer from the state shown in the window: `read_lines` and `search` SHALL use the stream's visible lines unless the call asks for all lines, and `list_bookmarks` SHALL include the notes. Line text SHALL be returned as data fields with the line number, cut at the long line cap. A result SHALL hold at most 2,000 lines and 1 MB, and `search` at most 500 hits plus the total count, with a flag saying when it was truncated. Tool calls SHALL run off the UI thread.

#### Scenario: Assistant reads an error's context
- **WHEN** the assistant calls `get_context` for line 812 of `app.log` with `before=5, after=5`
- **THEN** it receives lines 807 to 817 with their numbers.

#### Scenario: Filters respected
- **WHEN** the stream excludes `DEBUG` and the assistant calls `read_lines` with `last=100`
- **THEN** the 100 lines returned contain no line hidden by that filter.

### Requirement: Write Tools Require Permission
The tools `add_bookmark`, `set_search` and `open_file` SHALL be listed and callable only when `[mcp] allow_write=true` (default `false`). `open_file` SHALL open only paths under one of the folders in `open_roots`. Every change made by a write tool SHALL be shown in the stream bar as made by the assistant.

#### Scenario: Write refused by default
- **WHEN** `allow_write` is false and a client calls `add_bookmark`
- **THEN** the call fails with an error saying write tools are disabled and no bookmark is added.

#### Scenario: Path outside the roots
- **WHEN** `allow_write` is true, `open_roots` is `D:\logs` and a client calls `open_file` with `C:\Windows\win.ini`
- **THEN** the call is refused and no stream opens.
