## Context

The time range popup (`src/ui/time_range.rs`) edits a draft of two texts; OK calls
`TailEngine::apply_time_range_text`, which stores the texts, parses them with
`timestamp::parse_user_time` / `end_of_typed_time` against the stream's first timestamp,
and applies the window, holding it (`PendingWindow::Texts`) while the stream is timed in
the background. Timestamps are cached as the log's own clock read as if it were UTC;
`local_now_millis` gives the current time in that convention. The window is not saved in
the workspace; filter presets can save it as typed text. `headless-print` specifies
`--since` / `--until` with `-<N>m|h|d` for print mode only.

## Goals / Non-Goals

**Goals:** relative windows that slide on a followed log at a cost close to zero on
ordered logs; the same syntax in the popup, in presets and on the command line.

**Non-Goals:** named zones, end-anchored syntax, a global window.

## Decisions

### D1. Syntax
`-<N><unit>[<N><unit>…]` with units `s`, `m`, `h`, `d`, `w` (case-insensitive, no spaces
inside), and `now`. Parsed before the other forms in `parse_user_time`, so nothing that
reads today changes meaning (no current form starts with `-` or is `now`). On the "to"
side a relative time is the exact instant (no "end of unit" widening).

### D2. Which "now"
`local_now_millis()`, the convention of the cache. With `quick-wins-0-13`'s
`time_source_zone` set to `utc` or an offset, now is converted to that zone first, so a
UTC log is compared with UTC now. Without it, a UTC log on a machine at UTC+2 shows an
empty "last 15 minutes"; the empty-window hint (D4) names the last line's time, which
makes the cause visible.

### D3. Sliding
While a side is relative the engine re-evaluates the bounds every 5 seconds (and on each
append batch). When the stream's timestamps are non-decreasing (tracked while the cache is
filled: one flag, cleared at the first step back), a later "from" drops a prefix of
`filtered_lines` (binary search on the cached timestamps) and a later "to" admits the
lines after the last visible one up to the bound; both are O(changed lines). Otherwise the
filter is recomputed, in the background above 16 MB, at most once a minute, and the
tooltip says the window refreshes every minute. The search, collapse, context lines and
the histogram shading follow through the existing "filtered lines changed" path.
*Alternative:* anchor the window when applied (hl semantics) — rejected for the window: a
tailing viewer is watched, and a stale "last 15 minutes" misleads; the command line still
gets hl-like values at startup since the window then slides from there.

### D4. Display
The control's label is `⟳` plus the span of the visible lines; the tooltip gives the typed
text and the absolute bounds now in force. A live window with no visible line because the
last timestamp is older than "from" shows "no lines since HH:MM; last line at …".

### D5. Command line
`--since` / `--until` without `--print` call `apply_time_range_text` on each stream opened
from the command line, after its filter options; an unreadable value is a usage error
(exit 2), checked with the same parser before any window opens. With `--print` the
`headless-print` semantics apply (anchored at start, the process being short-lived).

### D6. Shortcuts
Relative row: 5 min, 15 min, 1 h, 6 h, 24 h, 7 d → `from = -5m…-7d`, `to` empty. Enabled
unless the stream is known to have no usable timestamps; before timing ends the window is
held like any typed one. "Last hour" becomes "Last hour of the log", same behaviour.

## Risks / Trade-offs

- [Re-evaluation cost on unordered multi-GB logs] → at most one background refilter per
  minute, and none while the bounds do not move past any line.
- [Clock / zone mismatch empties the view] → the empty-window hint with the last line's
  time; the source zone of `quick-wins-0-13`.
- [A preset saved with `-1h` behaves differently from one saved with `13:00`] → by design;
  the preset list shows the time text.

## Migration Plan

Additive. Texts starting with `-` or equal to `now` were invalid before, so no saved
preset changes meaning.

## Open Questions

- Add an end-anchored form (`end-15m`) for archived logs? Proposed: not now; the "Last hour
  of the log" shortcut covers the common case.
- Is 5 seconds the right refresh period? Proposed: yes; lower costs repaints for little.
