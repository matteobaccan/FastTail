## Why

*Open filter as new tab* turns the current filter into a stream of its own in the window;
the terminal keeps filter tabs made in the window in the layout but does not show them,
and cannot make one.

## What Changes

- `Alt+T` (and the palette's *Open filter as new tab*): the focused stream's filters become a
  new tab of the same file, the filters of the original stream cleared.
- Filter tabs saved by the window open as tabs in the terminal.

Target release: **0.31.0** (plan of 2026-10-30, after the competitor scan against 0.30.0; see `docs/competitor-analysis.md` section 8). Priority: **Medium**. Effort: **S**.

### Non-goals

- Filter tabs of several files at once.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `terminal-interface`: the requirement below is added.

## Impact

`src/tui/app.rs` over `crate::filter_tab`; texts, `docs/tui.md`, CHANGELOG.
