## 1. Server

- [ ] 1.1 `src/http_local.rs` (shared with `mcp-server`): loopback or all-interfaces bind, token and cookie, Origin check, connection cap, `no-store` and CSP headers
- [ ] 1.2 Stream snapshot published only while clients are connected; worker pool reading lines with shared-read handles
- [ ] 1.3 Endpoints `/api/streams`, `/api/lines` (tail and pages of 500), `/api/search` (next / previous match), `/api/events` (SSE `lines`, `reset`, `streams`)
- [ ] 1.4 Style runs from the row span computation; CSS generated from the theme and rules
- [ ] 1.5 Tests: token and cookie flow; foreign Origin refused; paging; SSE batches after appends; reset on truncation and filter change; escaping of `<script>` in lines; connection cap

## 2. Page

- [ ] 2.1 Single embedded HTML file: stream list, one or two panes, follow and Pause, load earlier lines, bookmarks, line numbers, interface language texts
- [ ] 2.2 Search box over loaded lines and server next / previous; responsive single column

## 3. UI and settings

- [ ] 3.1 Settings page "Web view": enable, port, allow other computers (confirmation, LAN addresses), token (copy, regenerate), Open in browser, Copy link
- [ ] 3.2 Title-bar indicator with the number of browsers; `[web]` section in `fasttail.ini`

## 4. Texts and documentation

- [ ] 4.1 New i18n keys (settings and page texts) in all 16 languages; add them to the exhaustive i18n test
- [ ] 4.2 README (web view section, security notes, SSH forwarding) and CHANGELOG `[Unreleased]`

## 5. Wrap-up

- [ ] 5.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PR with Linux and Windows CI green
- [ ] 5.2 Local preview exe for the maintainer before the release
- [ ] 5.3 After the release, archive the change so the `web-ui` capability is created
