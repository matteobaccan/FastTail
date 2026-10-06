## Why

Users who know Seq, Grafana Loki or Kibana expect to watch logs in a browser: from a
second screen, a laptop on the couch while the build machine tails, or a colleague's
browser during an incident call. FastTail's streams, filters and highlights exist only in
its window. A full log server is out of scope, but a small read-only web view of what
FastTail already shows covers "let me see it from over there" without installing
anything else. The post-0.12.0 competitor scan notes that server tools set this
expectation (section 3).

## What Changes

- An **optional web view**, **off by default**, enabled in Settings → "Web view", served
  by FastTail itself on `127.0.0.1`, port `47820` by default. Opening
  `http://127.0.0.1:47820/?token=…` shows it; "Open in browser" and "Copy link" buttons
  in Settings give the URL with the token.
- **Read-only**: the page lists the open streams and shows one at a time (or two side by
  side on wide screens) with the lines **as filtered in FastTail**, the level colours and
  the highlight rule colours of the current theme, line numbers and bookmarks (★).
- **Live**: new lines arrive through Server-Sent Events and the page follows the end,
  with a Pause button; scrolling up loads earlier lines in pages of 500.
- **Search in the page**: a search box highlights and walks matches in the lines loaded,
  and can ask the server for the next / previous match in the whole visible stream.
- Nothing typed in the browser changes FastTail (no filter, bookmark or file changes).
- **Access**: bound to loopback unless the user enables "Allow other computers", which
  asks for confirmation, binds `0.0.0.0` and shows the LAN addresses; every request
  needs the **token** (random, regenerable; kept in a cookie after the first visit); at
  most 8 browser connections.
- **Visibility**: a title-bar indicator while the web view is on, with the number of
  connected browsers.
- New `fasttail.ini` section `[web]`: `enabled` (default `false`), `port`, `token`,
  `allow_remote` (default `false`).

Target release: **0.27.0** (re-planned by the maintainer on 2026-10-06, from 0.22.0: about one large, two medium and three small changes per release) (planned for 0.15.0, moved after the terminal interface of 0.20.0; sources and integrations), per the release plan in
`docs/competitor-analysis.md` section 8. Priority: **Low**. Effort: **M**.

### Non-goals

- Controlling FastTail from the browser (filters, opening files, bookmarks).
- HTTPS, user accounts, or a multi-user log server; for remote access over untrusted
  networks the README points to SSH port forwarding.
- Serving files that are not open in FastTail, or lines hidden by its filters.
- A mobile-specific layout beyond a responsive single column.

## Capabilities

### New Capabilities

- `web-ui`: enabling, binding and token, the read-only stream view, live updates,
  search, limits.

### Modified Capabilities

None.

## Impact

- `src/http_local.rs` (new or shared with `mcp-server`): minimal HTTP/1.1 server on
  `std::net` threads, token and cookie check, Server-Sent Events, connection cap.
- `src/web/` (new): JSON endpoints (`/api/streams`, `/api/lines`, `/api/search`,
  `/api/events`), the page (one HTML file with inline CSS and JavaScript embedded with
  `include_str!`, no external resource), CSS classes per style from the theme and rules.
- A read-only stream snapshot published by the UI thread (the same `StreamSnapshot` as
  `mcp-server`); lines read by workers with shared-read handles.
- `src/ui/app.rs`: Settings page, indicator; `src/config.rs`: `[web]`.
- `src/i18n.rs` (16 languages) for the Settings texts and the page (the page uses the
  interface language), README, CHANGELOG.
- **TUI (0.20.0, PR #132)**: the server is UI-free and could be hosted by the TUI; not
  planned for 0.20.0.
