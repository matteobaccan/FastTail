## Why

The time range sits inline in the filter bar as two 74-pixel fields between the exclude
filter and the case toggle: a full `2026-09-18 14:02:05` does not fit, the bar gets crowded,
and the only way to pick a day is to type it in the exact format. Testing 0.10.1, the user
asked for a calendar to pick dates and for the window to live in a popup behind a button
whose label shows the window that is set.

## What Changes

- The two inline fields are replaced by one **time range button** in the filter bar. Its
  label is `🕘` plus the window that is set (`🕘 2026-09-18 14:02 → 16:30`, `🕘 from 14:02`,
  `🕘 to 2026-09-18`), or `🕘` plus "all times" when none is. It is tinted when a window
  narrows the view, shows the warning colour when a side cannot be read and a pending mark
  while the window waits for the background timing.
- Clicking it opens a **popup** anchored under the button with, for each side ("from" and
  "to"): a text field wide enough for a full timestamp (every format accepted today still
  works, the date alone included), a **calendar** of the month to pick the day, and hour /
  minute / second spinners. A ✖ clears the window; the hints (invalid time, no usable
  timestamps, pending) are shown inside the popup.
- The calendar opens on the month of the log's first timestamp (or of the side's current
  value), marks the days the log spans and today, and navigates by month and year. Picking
  a day on the "from" side sets 00:00:00 unless a time was already chosen; on the "to"
  side 23:59:59.
- Shortcut buttons in the popup: **Whole log** (clear), **First day** and **Last day** of
  the log, **Last hour** of the log.
- Changes apply as they are made, as today; the popup closes with `Esc`, a click outside,
  or the button again. Nothing new is stored in `fasttail.ini`; the window itself stays
  per stream and in sessions as today.
- No new dependency: the calendar uses the civil-date functions already in
  `src/timestamp.rs`.

## Capabilities

### New Capabilities
<!-- none -->

### Modified Capabilities
- `filters-and-highlighting`: the time range inputs move from the filter bar into a popup
  behind a button labelled with the current window, and gain a calendar, time spinners and
  shortcuts.

## Impact

- `src/ui/dock.rs` (`render_time_range` becomes the button + popup), a new
  `src/ui/calendar.rs` widget, `src/timestamp.rs` (public civil-date helpers: days in a
  month, weekday, day start), `src/i18n.rs` (new keys in every language: all times,
  month and weekday names, shortcuts), README, CHANGELOG, spec.
- Engine untouched: the popup writes the same `time_from_text` / `time_to_text` through
  `apply_time_range_text`, so pending windows, sessions and go-to-time behave as today.
- Target release: **0.11.0**. Builds on #96 (date alone accepted, fields never disabled).

## Non-goals

- Time zones: a picked time is the clock written in the log, as with typed times.
- Picking ranges by dragging on a timeline (that belongs to `timeline-histogram`).
- A global time range across streams (that belongs to `merged-timeline-view`).
