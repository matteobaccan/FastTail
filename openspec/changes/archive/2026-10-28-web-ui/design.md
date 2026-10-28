## Context

Stream state lives on the UI thread in `TailEngine`s; lines are read from files by byte
range. `mcp-server` (also 0.22.0) proposes a `StreamSnapshot` published by the UI thread
and a loopback HTTP server (`src/http_local.rs`) with a token. Rows are painted from
rule, label, level, ANSI and hit spans computed in `dock.rs`; `export-formats` exposes
that span computation for its HTML writer.

## Goals / Non-Goals

**Goals:** watch the same filtered, coloured streams from a browser, live; safe by
default; no runtime dependencies (no Node, no CDN).

**Non-Goals:** control from the browser, a log server, TLS.

## Decisions

### D1. Shared server with `mcp-server`
One `http_local` server per process, with routes `/mcp` and `/` + `/api/*`, each behind
its own enable switch and token. When only one feature is on, only its routes answer. If
the two use different ports in `fasttail.ini`, two listeners run. *Alternative:* a crate
such as `tiny_http` — acceptable fallback; std-only first.

### D2. Server-Sent Events, not WebSockets
SSE is plain HTTP, one direction (all the page needs), reconnects on its own and needs
no handshake code. Events: `lines` (a batch of appended visible lines with spans),
`reset` (truncation, rotation, filter change: the page reloads the tail), `streams`
(opened / closed). Batches are sent at most 5 times a second.

### D3. Spans computed server-side
Lines are sent as text plus style class runs from the same span computation as the
screen and the HTML export, so colours match. The page's CSS is generated from the
current theme and rules when it loads and on `reset`.

### D4. Security
Loopback by default; remote needs an explicit switch with a warning. Token of 32 random
bytes; `?token=` sets an `HttpOnly`, `SameSite=Strict` cookie and redirects to strip it
from the URL. JSON endpoints reject requests with a foreign `Origin`. All log text is
escaped by the page (`textContent`, never `innerHTML`), and a strict
Content-Security-Policy forbids scripts other than the page's own. Responses carry
`Cache-Control: no-store`.

### D5. Limits
8 browser connections; a page of 500 lines per request, lines cut at the long line cap;
server search returns the next match only (no bulk dump). Worker pool of 2 threads.

## Risks / Trade-offs

- [Exposing logs on the LAN] → off by default, loopback by default, token, warning,
  indicator with the number of connections.
- [Plain HTTP on the LAN can be sniffed] → documented; SSH forwarding recommended.
- [UI cost of snapshots] → snapshots only when something is connected.

## Migration Plan

None: `[web]` absent means off.

## Open Questions

- Should the page allow its own local (browser-side) filter over the loaded lines?
  Proposed: yes, a simple text filter that never reaches FastTail.
- Should the web view be enabled from the command line (`--web`) for headless machines?
  Proposed: not in 0.22.0; `headless-print --follow` plus existing tools cover scripts.
