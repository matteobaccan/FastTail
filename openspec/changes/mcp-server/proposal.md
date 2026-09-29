## Why

Users increasingly debug with an AI assistant (Claude, Copilot, Cursor, local models)
next to the log viewer. Today they copy lines out of FastTail and paste them into a chat,
losing the filters, the levels, the bookmarks and the notes they already built, and the
assistant cannot look further ("show me the 20 lines before that error", "how many
timeouts in the last hour?"). The Model Context Protocol (MCP) is the standard way to
give an assistant tools and data; in 2026 Seq, gonzo, logana and LogViewPlus added MCP
servers or AI prompts, and the post-0.12.0 competitor scan ranks the gap 17 (value
Medium, effort M).

## What Changes

- An **optional MCP server** inside FastTail, **off by default**, enabled in Settings →
  "AI assistants (MCP)". Two transports:
  - **stdio**: `fasttail --mcp` is a small bridge the assistant launches; it connects to
    the running FastTail window, or, when none runs, serves the files given on its
    command line headlessly (read-only, with the `headless-print` line pipeline).
  - **localhost HTTP** (MCP Streamable HTTP) on `127.0.0.1`, port `47821` by default,
    requiring a bearer **token** generated on first enable and shown with a Copy button
    and a ready-to-paste client configuration snippet.
- **Read-only tools** (the default): `list_streams` (path, size, lines, follow state,
  level counts, time span, active filters), `read_lines` (a range, or the last N, of the
  visible or all lines), `search` (text or regex, optional level and time range, returns
  line numbers and text), `get_context` (N lines around a line), `list_bookmarks` (with
  notes), `get_filters` (terms, level, time range, global filter), `level_summary`.
  Resources: each open stream as `fasttail://stream/<n>`.
- **Write tools, off unless allowed** in Settings: `add_bookmark` (with note),
  `set_search`, `open_file` (only paths under folders the user lists). Every write is
  shown in the stream bar as done "by the assistant".
- **Limits**: at most 2,000 lines and 1 MB per response (truncation says so), lines cut at
  the long line cap, requests served on a worker that reads with the shared-read mode of
  scan jobs, never blocking the UI.
- **Visibility**: a title-bar indicator while the server is on, with the number of
  requests in the last minute; a log of the last 200 requests (tool, arguments, size) in
  the MCP settings page.
- New `fasttail.ini` section `[mcp]`: `enabled` (default `false`), `http` (default
  `false`), `port`, `token`, `allow_write` (default `false`), `open_roots`.

Target release: **0.22.0** (planned for 0.15.0, moved after the terminal interface of 0.20.0; sources and integrations), per the release plan in
`docs/competitor-analysis.md` section 8. Priority: **Low–Medium**. Effort: **M**.

### Non-goals

- An AI chat or a built-in model inside FastTail, or FastTail calling any AI service.
- Listening on a non-loopback address, TLS, multi-user access (see `web-ui` for a
  browser view).
- MCP prompts and sampling; tools that run external tools or change highlight rules.
- Structured field queries (`structured-fields`, `query-language` can add tools later).

## Capabilities

### New Capabilities

- `mcp-server`: enabling, transports, token, the read-only and write tool sets, limits,
  request log.

### Modified Capabilities

None.

## Impact

- `src/mcp/` (new): JSON-RPC 2.0 framing (serde_json), MCP `initialize`, `tools/list`,
  `tools/call`, `resources/list`, `resources/read`; tool implementations over a read-only
  `StreamSnapshot` (path, visible-line index, filters, bookmarks) published by the UI
  thread each frame it changes; write tools as commands queued to the UI thread.
- `src/http_local.rs` (new, shared with `web-ui`): a minimal loopback HTTP/1.1 server on
  `std::net` threads, token check, request size limits.
- `src/cli.rs`, `src/main.rs`: `--mcp` bridge mode (no window); discovery of the running
  instance through a runtime file (`mcp.port`, token) in the config directory.
- `src/ui/app.rs`: Settings page, indicator, request log; `src/config.rs`: `[mcp]`.
- `src/i18n.rs` (16 languages), README (setup for common clients), CHANGELOG.
- No async runtime; no new crate unless the design's open question on `rmcp` is
  answered yes.
- **TUI (0.20.0, PR #132)**: the server core is UI-free; the TUI can host it with the
  same snapshot interface.
