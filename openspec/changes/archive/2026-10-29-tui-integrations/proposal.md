## Why

A server running the terminal interface is where the read-only web view and the MCP
server would help most, yet only the window starts them; `fasttail-tui --mcp` serves the
files given on the command line, not the streams of a running interface. Plugins run in
the terminal only after they were enabled and approved in the window, which a
terminal-only Linux build does not have.

## What Changes

- The terminal interface starts the **web view** when `[web]` is on, as the window does,
  and shows its address in the status bar; a Settings section turns it on or off and sets
  the port.
- The terminal interface serves its open streams to the **MCP server** when `[mcp]` is on
  (the same tools and write opt-in as the window), with a Settings section.
- A **Plugins** dialog (palette): the installed plugins, enable / disable, approve a new
  or changed hash (showing it), the parameters of a source plugin, its last error.

Target release: the next minor after 0.28.1 (terminal parity with the window, proposed on 2026-10-29 at the maintainer's request; planned when the maintainer agrees), per `docs/competitor-analysis.md` sections 7 and 8. Priority: **Medium**. Effort: **M**.

### Non-goals

- A plugin marketplace or download.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `terminal-interface`: the requirements below are added.

## Impact

- `src/tui/` start / stop of `crate::web` and the MCP bridge, a plugins dialog over
  `crate::plugins`; Settings rows; texts, `docs/tui.md`, README, CHANGELOG.
