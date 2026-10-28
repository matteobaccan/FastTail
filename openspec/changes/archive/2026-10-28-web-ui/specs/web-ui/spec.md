## ADDED Requirements

### Requirement: Web View Enabling and Access
FastTail SHALL include a read-only web view that is off by default and is enabled in Settings, with the state kept in `fasttail.ini` `[web]` (`enabled`, `port` default `47820`, `token`, `allow_remote` default `false`). It SHALL bind `127.0.0.1` only, unless "Allow other computers" is on, which SHALL ask for confirmation before binding all interfaces and SHALL list the machine's addresses. Every request SHALL require the token, given once as a `token` query parameter (which sets an HttpOnly, SameSite=Strict cookie) or by that cookie; requests without a valid token SHALL be answered with 401. At most 8 browser connections SHALL be served. While the web view is on, the title bar SHALL show an indicator with the number of connected browsers.

#### Scenario: Off by default
- **WHEN** FastTail starts with no `[web]` section
- **THEN** no port is bound for the web view.

#### Scenario: Wrong token
- **WHEN** a browser opens `http://127.0.0.1:47820/` without the token
- **THEN** the server answers 401 and no stream data is sent.

#### Scenario: Loopback only
- **WHEN** the web view is on with `allow_remote=false` and another computer connects to the machine's LAN address on port 47820
- **THEN** the connection is refused.

### Requirement: Read-Only Stream Page
The web page SHALL list the open streams and show the selected stream's lines as FastTail shows them: only the lines visible under its filters and the global filter, with line numbers, bookmarks, level colours and highlight rule colours of the current theme. Log text SHALL be inserted as text, never as markup, and the page SHALL load no external resource. Nothing done in the browser SHALL change FastTail's filters, bookmarks, streams or files.

#### Scenario: Same view as the window
- **WHEN** `app.log` excludes `healthcheck` in FastTail and a rule paints `ERROR` red
- **THEN** the page shows no `healthcheck` line and shows `ERROR` in red.

#### Scenario: Markup in a log line
- **WHEN** a line contains `<img src=x onerror=alert(1)>`
- **THEN** the page shows that text literally and no script runs.

### Requirement: Live Updates and Paging
The page SHALL receive newly visible lines through Server-Sent Events, in batches at most 5 times per second, and SHALL follow the end of the stream unless the user pauses or scrolls up. Scrolling to the top SHALL load earlier lines in pages of 500. A truncation, rotation or filter change in FastTail SHALL make the page reload the stream's tail. A search box SHALL highlight matches in the loaded lines and SHALL ask the server for the next or previous match in the whole visible stream.

#### Scenario: Watching from a browser
- **WHEN** the page shows `app.log` following and the writer appends 3 lines that pass the filters
- **THEN** the 3 lines appear at the bottom of the page within one second.

#### Scenario: Filter changed in the window
- **WHEN** the user adds the include term `ERROR` to `app.log` in FastTail
- **THEN** the page reloads and shows only the `ERROR` lines.
