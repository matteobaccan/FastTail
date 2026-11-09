## Context

`crate::web` and the MCP server run on threads fed by the window's engines each frame
(`web_view.serve`, `serve_mcp`); `crate::plugins` holds the approvals by hash. The terminal
has the same engines in its tabs.

## Goals / Non-Goals

**Goals:** the window's feature in the terminal, over the same engine and files.

**Non-Goals:** A plugin marketplace or download.

## Decisions

### D1. Same servers
The terminal feeds the same servers from its tick, so the pages and tools are identical.

### D2. Approval shows the hash
Approving a plugin in the terminal shows its executable path and SHA-256 and asks for
`y`, as the window's dialog does.

## Risks / Trade-offs

- [Port already taken by a window on the same machine] → the second interface reports it and does not start its server.

## Open Questions

- Start the web view by default in the terminal-only build? Proposed: no, as in the window.
