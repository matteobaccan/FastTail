## 1. Core

- [ ] 1.1 `StreamSnapshot` published by the UI thread with a generation counter; worker pool reading lines with a shared-read handle
- [ ] 1.2 `src/mcp/`: JSON-RPC 2.0 framing, `initialize` with protocol version, `tools/list`, `tools/call`, `resources/list`, `resources/read`, `ping`
- [ ] 1.3 Read-only tools: `list_streams`, `read_lines`, `search`, `get_context`, `list_bookmarks`, `get_filters`, `level_summary`
- [ ] 1.4 Write tools behind `allow_write`: `add_bookmark`, `set_search`, `open_file` restricted to `open_roots`; applied through the UI command queue
- [ ] 1.5 Limits: lines, bytes, hits, rate; truncation flags in results
- [ ] 1.6 Tests: recorded handshake, each tool on fixture files, limits, write tools refused when not allowed, `open_file` outside the roots refused

## 2. Transports

- [ ] 2.1 `src/http_local.rs`: loopback HTTP/1.1 server, bearer token, Origin check, body size limit (shared with `web-ui`)
- [ ] 2.2 MCP Streamable HTTP endpoint on it
- [ ] 2.3 `fasttail --mcp`: runtime file discovery and stdio proxy; headless fallback over the `headless-print` pipeline
- [ ] 2.4 Tests: wrong or missing token rejected; foreign Origin rejected; bridge with and without a running window

## 3. UI and settings

- [ ] 3.1 Settings page "AI assistants (MCP)": enable, HTTP, port, token (copy, regenerate), allow write, open roots, client snippet, warning
- [ ] 3.2 Title-bar indicator with request rate; request log (last 200); "by the assistant" notice in the stream bar
- [ ] 3.3 `[mcp]` section in `fasttail.ini`

## 4. Texts and documentation

- [ ] 4.1 New i18n keys in all 16 languages; add them to the exhaustive i18n test
- [ ] 4.2 README (MCP setup for common clients, prompt-injection note) and CHANGELOG `[Unreleased]`

## 5. Wrap-up

- [ ] 5.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PR with Linux and Windows CI green
- [ ] 5.2 Local preview exe for the maintainer before the release
- [ ] 5.3 After the release, archive the change so the `mcp-server` capability is created
