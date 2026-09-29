## Context

All stream state lives on the UI thread: `TailEngine` per stream (line index, filters,
search hits, bookmarks with notes, level counts, time span), the global filter, the
workspace. Scan jobs already read files on workers with a shared-read handle. The
`headless-print` change (0.13.0) adds a UI-free line pipeline (`print_mode.rs`). MCP
clients start stdio servers as child processes, or connect to HTTP servers by URL.

## Goals / Non-Goals

**Goals:** an assistant sees what the user sees (visible lines, filters, bookmarks) with
zero copy-paste; nothing is exposed unless the user turns it on; the UI never waits for
a client.

**Non-Goals:** AI inside FastTail, remote access, write access by default.

## Decisions

### D1. Snapshot, not shared engines
The UI thread publishes an `Arc<StreamSnapshot>` per stream when its state changes
(generation counter): path, file length, visible-line ranges or the filter spec to
recompute them, bookmarks and notes, level counts, time span. Tool calls run on a worker
pool (2 threads), read line bytes through their own shared-read handle, and answer from
the snapshot. *Alternative:* lock the engines — rejected, would stall frames.

### D2. Two transports, one core
stdio is what most clients support best; HTTP lets several clients connect to a running
window. `fasttail --mcp` reads `mcp.runtime` (port and token, written by the window when
the server is on, removed on exit) and proxies stdio to HTTP; without a window it serves
the files on its command line using the `headless-print` pipeline (filters from the
arguments, no bookmarks). The window listens only when `[mcp] http=true` or a bridge
connects (the bridge path always uses the token). *Alternative:* stdio only in bridge
mode, no HTTP server — rejected, stdio needs some local IPC anyway.

### D3. Loopback and token
The listener binds `127.0.0.1` only. Every request needs `Authorization: Bearer <token>`
(32 random bytes, hex). The `Origin` header, when present, must be absent or loopback, to
stop DNS-rebinding from a browser page. The token is regenerated with a button.

### D4. Log text is untrusted data
Log lines can contain text written to steer an assistant ("ignore previous
instructions…"). Tool results return lines as data fields (`{"line": 812, "text": …}`),
never as instructions, and the README warns that assistants may be misled by log
content. Write tools are off by default for this reason.

### D5. Hand-written JSON-RPC, no async runtime
The used MCP surface (initialize, tools, resources, ping, notifications) is small and
serde_json covers it; FastTail has no tokio today. *Alternative:* the `rmcp` crate —
see open questions.

### D6. Limits
2,000 lines and 1 MB per result; `search` returns at most 500 hits plus the total count;
at most 20 requests/s per client, excess answered with a JSON-RPC error. Long lines are
cut at the long line cap with the marker.

## Risks / Trade-offs

- [Secrets in logs reach a cloud model] → off by default; enabling shows a warning; the
  request log shows what left.
- [Protocol churn in MCP] → pin the protocol version negotiated in `initialize`; tests
  against recorded client handshakes.
- [Antivirus / firewall prompts for the listener] → loopback only; Windows does not prompt
  for loopback binds.

## Migration Plan

None: `[mcp]` absent means off.

## Open Questions

- Adopt the official `rmcp` crate (pulls tokio) instead of hand-written JSON-RPC?
  Proposed: no for 0.22.0.
- Should `read_lines` expose lines hidden by the stream's filters? Proposed: yes, with an
  explicit `scope: "all"` argument; default `visible`.
- Should the HTTP listener be shared with `web-ui` on one port? Proposed: one server,
  separate paths and separate enable switches.
