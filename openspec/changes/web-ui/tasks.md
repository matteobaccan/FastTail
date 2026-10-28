## 1. Server

- [x] 1.1 `src/http_local.rs` (shared with `mcp-server`): loopback or all-interfaces bind, token and cookie, Origin check, connection cap, `no-store` and CSP headers (as built: `src/web.rs`; `mcp-server` will reuse it)
- [x] 1.2 Stream snapshot published only while clients are connected; worker pool reading lines with shared-read handles (as built: no snapshot: each request goes to the window's thread through a channel and is answered from the engines, at most 32 per frame; a thread per connection)
- [x] 1.3 Endpoints `/api/streams`, `/api/lines` (tail and pages of 500), `/api/search` (next / previous match), `/api/events` (SSE `lines`, `reset`, `streams`) (SSE events `lines`, `reset`, `gone`)
- [x] 1.4 Style runs from the row span computation; CSS generated from the theme and rules (`export_html::row_runs`)
- [x] 1.5 Tests: token and cookie flow; foreign Origin refused; paging; SSE batches after appends; reset on truncation and filter change; escaping of `<script>` in lines; connection cap

## 2. Page

- [x] 2.1 Single embedded HTML file: stream list, one or two panes, follow and Pause, load earlier lines, bookmarks, line numbers, interface language texts (an HTML file, its script and its CSS, all embedded)
- [x] 2.2 Search box over loaded lines and server next / previous; responsive single column

## 3. UI and settings

- [x] 3.1 Settings page "Web view": enable, port, allow other computers (confirmation, LAN addresses), token (copy, regenerate), Open in browser, Copy link (its own window from Settings; the LAN address of the default route)
- [x] 3.2 Title-bar indicator with the number of browsers; `[web]` section in `fasttail.ini`

## 4. Texts and documentation

- [x] 4.1 New i18n keys (settings and page texts) in all 16 languages; add them to the exhaustive i18n test
- [x] 4.2 README (web view section, security notes, SSH forwarding) and CHANGELOG `[Unreleased]`

## 5. Wrap-up

- [ ] 5.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PR with Linux and Windows CI green
- [ ] 5.2 Local preview exe for the maintainer before the release
- [ ] 5.3 After the release, archive the change so the `web-ui` capability is created
