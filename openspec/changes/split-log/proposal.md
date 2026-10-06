## Why

A multi-gigabyte log often has to be cut to attach a piece to a ticket, send it through a
size-limited channel or open it in a tool that cannot take it whole. Today FastTail exports
one piece at a time (a time range plus **Export visible lines**). lnav 0.15 added
`file split`.

## What Changes

- **Split file...** in the stream menu and the palette opens a dialog: split **by lines**
  (every N lines), **by size** (every N MB, cut at a line end) or **by time** (every hour,
  day, or a chosen span, from the line timestamps).
- The pieces are written next to the file or in a chosen folder, named
  `app.part-001.log`, ... (or `app.2026-10-05T14.log` by time), with a preview of the
  piece count and sizes before writing.
- **Visible lines only** is an option: the current filters apply, as for Export.
- Writing streams the file in the background with progress and Cancel; nothing is held
  in memory; existing files are never overwritten without asking.
- The terminal interface offers the same dialog from the palette.

Target release: **0.22.0** (re-planned by the maintainer on 2026-10-06, from 0.24.0: about one large, two medium and three small changes per release) (candidates of the 2026-10-05 competitor scan, `docs/competitor-analysis.md` section 6, assigned by the maintainer on 2026-10-05), per the release plan in section 8. Priority: **low**. Effort: **S (under a week)**.

### Non-goals

- Splitting by a field value (one file per host): a follow-up if asked.
- Compressing the pieces.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `selection-and-export`: splitting a stream into files.

## Impact

- `src/split.rs` (new): the splitter as a background job over the block cache, line
  aware, with time buckets from the timestamp index.
- `src/ui/` dialog, `src/tui/` dialog, `src/actions.rs` action.
- i18n, README, `docs/ui-design.md`, `docs/tui.md`, CHANGELOG, tests.
