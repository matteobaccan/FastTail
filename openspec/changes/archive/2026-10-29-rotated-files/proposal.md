## Why

Log rotation splits one log into `app.log`, `app.log.1`, `app.log.2.gz`, ... Reading
yesterday's incident means opening the pieces one by one or building a merged view by hand,
and a merged view orders by timestamp, which is wrong for lines without one and slower than
needed: rotated pieces are already in order, one after the other. logana 0.8.2 (2026-10-26)
opens the numbered rotations of a file as one stream; lnav and hl read them when given a glob.
The post-0.27.0 scan (`docs/competitor-analysis.md`, gap 22) ranks it medium-high.

## What Changes

- Opening a file whose folder holds its **rotations** (`app.log.1`, `app.log.2.gz`,
  `app.log-20261027`, `app.2026-10-27.log`, ... recognised by name) offers **"Include
  rotations (N files, 1.4 GB)"** in the stream bar; the setting `rotations=ask|always|never`
  (default `ask`) decides.
- With rotations the stream reads the pieces **as one file, oldest first**, compressed
  pieces decompressed through the existing spool, the live file last and followed.
- The gutter and the overview strip mark where each piece starts; a tooltip names it;
  `Go to piece…` in the stream menu jumps to the first line of a piece.
- When the live file rotates while it is followed, the stream keeps its lines and goes on
  with the new file (the old one is now `app.log.1`, already read).
- `--rotations` on the command line, and `rotations=1` in the workspace and sessions.

Target release: **0.28.0** (proposed after the post-0.27.0 scan of 2026-10-28; planned when the maintainer agrees), per the release plan in `docs/competitor-analysis.md` section 8. Priority: **Medium**. Effort: **M**.

### Non-goals

- Rotations that are not in the same folder, or named by a scheme FastTail cannot recognise
  (a pattern stream or a merged view covers those).
- Reordering lines by timestamp (that is the merged timeline).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `stream-engine`: the requirements below are added.

## Impact

- New `src/rotation.rs`: sibling discovery and ordering by the rotation number or date.
- `src/file_source.rs`: a chained source (one logical offset space over several files,
  compressed ones through the spool); the line index and block cache unchanged above it.
- `src/tail_engine.rs`: piece boundaries, rotation while following; `src/ui/dock.rs`,
  overview strip, the stream bar offer; the TUI the same chip and key.
- `src/config.rs` `rotations=`; `src/cli.rs` `--rotations`; workspace and sessions.
- README, CHANGELOG, `docs/ui-design.md`, i18n in every language.
