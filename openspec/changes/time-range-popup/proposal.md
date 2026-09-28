## Why

Every stream shows its time range twice, in two places that do not talk to each other:
the stream bar shows the span of the lines on screen (`🕘 2026-09-18 14:02:05 →
2026-09-18 16:30:12`), read-only, while the filter row holds the time range filter as two
74-pixel fields where a full timestamp does not fit and a day can only be typed in the exact
format. Testing 0.10.1 the maintainer asked for a calendar and a popup; testing 0.12.0 they
asked that the range shown be the range edited: "the time range should be clickable, open a
popup where I can change the dates, and confirming it should change the visible range. Now
viewing and changing the range are two different things."

## What Changes

- The **time span in the stream bar becomes the time range control**. It keeps showing the
  span of what is visible (`🕘 2026-09-18 14:02 → 16:30`, shortened when both ends share the
  date), tinted with the accent colour while a time window narrows the view, in the warning
  colour while a side of the window cannot be read, with `⏳` while the window waits for the
  background timing, and `🕘` plus "no timestamps" (dimmed) on a stream without usable
  timestamps. It is always shown in Text view, looks clickable, and a click opens the popup.
- The **inline time range fields leave the filter row**; the timeline (`📊`) and go-to-time
  (`🔍`) buttons stay where they are.
- The **popup**, anchored under the control, holds for each side ("from", "to"): a text
  field wide enough for a full timestamp (every format accepted today still works, the date
  alone included), a **calendar** of the month to pick the day and hour / minute / second
  spinners; shortcuts **Whole log**, **First day**, **Last day**, **Last hour**; the
  invalid-time and no-usable-timestamps hints; and **OK** / **Cancel**.
- **Edits are a draft until confirmed.** OK (or `Enter` in a field) applies the window and
  closes the popup; Cancel, `Esc` or a click outside close it and leave the window as it
  was. OK is disabled while a side cannot be read. The popup opens prefilled with the
  window that is set, or empty when none is.
- The calendar opens on the month of the side's value, else of the log's first timestamp,
  else today, marks the days the log spans and today, and navigates by month and year.
  Picking a day on the "from" side gives 00:00:00 unless a time was set; on the "to" side
  through 23:59:59.
- Nothing new is stored in `fasttail.ini`; the window stays per stream and in sessions as
  today. No new dependency: the calendar uses the civil-date functions of
  `src/timestamp.rs`.

## Capabilities

### New Capabilities
<!-- none -->

### Modified Capabilities
- `filters-and-highlighting`: the time range filter is edited from the stream bar's time
  span through a popup with a calendar, time spinners, shortcuts and an explicit
  confirmation, instead of inline fields in the filter row.

## Impact

- `src/ui/dock.rs` (the stream bar's span becomes the control; `render_time_range` loses its
  fields and becomes the popup), a new `src/ui/calendar.rs` widget, `src/timestamp.rs`
  (public civil-date helpers: days in a month, weekday), `src/tail_engine.rs`
  (`last_timestamp()`), `src/i18n.rs` (new keys in every language), README, CHANGELOG,
  `docs/ui-design.md`, spec.
- Engine behaviour untouched: OK writes `time_from_text` / `time_to_text` and calls
  `apply_time_range_text`, so pending windows, sessions and go-to-time behave as today.
- Target release: **0.12.0** (before the release is cut).

## Non-goals

- Time zones: a picked time is the clock written in the log, as with typed times.
- Picking ranges by dragging on the timeline histogram.
- A global time range across streams (that belongs to `merged-timeline-view`).
