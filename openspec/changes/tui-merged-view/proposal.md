## Why

The window merges several streams into one timeline by timestamp (`merged-timeline-view`),
and saves the merged view in the workspace and sessions, but the terminal only keeps such
a view in the layout without drawing it. On a server with an `app.log`, a `db.log` and a
`proxy.log` the merged timeline is the view that answers "what happened at 14:02".

## What Changes

- **Merged view in the terminal**: the palette's *Merged view of streams…* lists the open
  streams with check boxes; `Enter` opens a stream window whose rows are the chosen
  streams' lines interleaved by time, each prefixed by a coloured source tag (`[app]`).
- The window's merged views saved in the workspace or a session open as such in the terminal.
- Search, filters (per source and over the merge), bookmarks, go to time, copy and export
  work as in the window's merged view.

Target release: **0.31.0** (plan of 2026-10-30, after the competitor scan against 0.30.0; see `docs/competitor-analysis.md` section 8). Priority: **High**. Effort: **M**.

### Non-goals

- A merged view of streams the terminal cannot open (none: every source opens in both).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `terminal-interface`: the requirement below is added.

## Impact

`src/tui/` a merge dialog and the merged stream's rows over `crate::merged_stream`; texts, `docs/tui.md`, README, CHANGELOG.
