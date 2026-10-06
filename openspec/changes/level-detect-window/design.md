## Context

`detect_level(line)` scans the first `DETECT_WINDOW` (96) bytes for the first whole-word
level token, plus a syslog `<n>` prefix; a word cut by the window is not a match. The
engine fills a per-line level cache (1 byte per line) on the UI thread for small files and
in `scan_job` on a worker above 16 MB, and keeps per-level counters. The filter and the
colours read the cache; `level_of` detects on the fly for lines not cached yet.

## Goals / Non-Goals

**Goals:** let the user widen (or narrow) the window, per stream when one log needs it.

**Non-Goals:** positional or field-based level extraction, new tokens, the timestamp window.

## Decisions

1. **`detect_level(line, window)`**, `log_level::DEFAULT_WINDOW = 96`. The scan stays a
   byte loop without allocation; its cost grows with the window only for lines that have
   no level token early (the first token found stops the scan).
2. **Range 16 to 4,096 bytes.** Below 16 even `<n>` plus a short date does not fit; above
   4 KB the "first token" rule would pick level words from message bodies too often, and
   the scan would cost more than reading the row. Longer lines are scanned only up to the
   window.
3. **Global plus per-stream override.** The global key is the default for new streams and
   for streams without an override; the stream key is written only when it differs, so
   older builds and unchanged streams see nothing new. Alternative: global only; rejected
   because widening it for one log raises the chance of false matches in all others.
4. **Changing the value** truncates the stream's level cache to 0 and resets the counters,
   then refills them as on open: on the UI thread under 16 MB, otherwise as a `Levels`
   background job with progress (the existing `ScanKind::Levels`). A growing file keeps
   appending with the new window; a truncated or rotated file restarts as today. Memory per
   line is unchanged (1 byte). Windows file sharing is unaffected (the same reads).
5. **Where the setting lives:** Settings > Streams (window and terminal), next to the
   stream defaults; the override in the level selector's menu (window) and in the level
   chip's menu (terminal), as a number field with the same range check.
6. **`--print`** reads the global value and accepts `--level-bytes N`.

## Risks / Trade-offs

- [A wide window matches a level word in the message (`... user error ...`)] → the
  default stays 96; the setting's tooltip says so; the override is per stream.
- [Recount on a multi-GB file takes time] → background job with progress, as on open;
  the old levels are cleared first so the filter never mixes two windows.
- [Hot-path cost] → benchmark at 96 and 1,024 bytes on lines without a level token.

## Migration Plan

None: new keys with defaults equal to today's behaviour.

## Open Questions

- Offer an "auto" mode that finds the column where levels sit in the first lines?
  Proposed: not now; `custom-log-formats` is the precise answer.
