## Context

`render_time_range` (src/ui/dock.rs) draws the label, two 74 px `TextEdit`s, a ✖ and the
hints inline in the horizontal filter bar. It writes `engine.time_from_text` /
`time_to_text` through `apply_time_range_text`, which parses them (`parse_user_time`,
`end_of_typed_time`), holds the window while a background timing scan runs, and applies it.
After #96 a bare `YYYY-MM-DD` is accepted and the fields are never disabled. The project has
no date library: `src/timestamp.rs` has private `days_from_civil` / `civil_from_days`.

## Goals / Non-Goals

**Goals:**
- Free the filter bar: one compact button whose label says what window is set.
- Pick dates with the mouse (calendar) and times without typing format (spinners), while
  typing any accepted format keeps working.
- Zero change to the engine and to what is saved.

**Non-Goals:**
- Time zones, drag-to-select on a timeline, cross-stream windows (see proposal).
- A reusable general-purpose date picker crate.

## Decisions

- **The text stays the source of truth.** The popup edits `time_from_text` /
  `time_to_text` and calls `apply_time_range_text` exactly as the inline fields do; the
  calendar and spinners only rewrite that text in the canonical shape
  (`YYYY-MM-DD` or `YYYY-MM-DD HH:MM:SS`). Sessions, pending windows and the invalid-time
  logic keep working unchanged. Alternative: keep instants in the engine and format them
  for display — rejected, it forks the parsing rules and loses what the user typed.
- **Own calendar widget, no dependency.** `egui_extras::DatePickerButton` needs `chrono`
  and its own look; the calendar is a 7×6 grid of day buttons on top of the civil-date
  functions already present (made public with a weekday and days-in-month helper). Weeks
  start on Monday (ISO 8601) in every language; month and weekday names come from
  `src/i18n.rs`. Cost: one grid per open popup, nothing when closed.
- **Which month opens.** The side's current value if it parses, else the day of the log's
  first timestamp (`time_reference`), else today. Days between the log's first and last
  timestamp are tinted, so the user sees where the data is; today gets an outline.
- **Picking a day keeps the time.** If the side already has a time it is kept on the new
  day; otherwise "from" becomes the bare date (00:00:00) and "to" the bare date (through
  23:59:59.999, per `end_of_typed_time`). Spinners on a bare date turn it into
  `YYYY-MM-DD HH:MM:SS`.
- **Popup = `egui::Popup` anchored to the button**, closed on `Esc`, click outside or a
  second click on the button; the text fields get a width of ~170 px so a full timestamp
  fits. Changes apply live, as the inline fields did; an unfinished text is judged when its
  field loses focus (the rule from #93).
- **Button label** from the two texts, shortened: equal dates collapse (`2026-09-18
  14:02 → 16:30`), a side left empty reads "from … / to …", and long labels are cut with
  the full window in the tooltip. Colours: accent tint when `is_time_filtered()`, warning
  colour when `time_range_error`, a `⏳` suffix while `time_range_pending()`.
- **Shortcuts** read the log's first and last timestamp from the cache (`time_reference`
  and the last non-`NO_TIMESTAMP` entry, O(1) from the end); they are disabled until the
  stream has been timed.
- Where it runs: all on the UI thread; parsing a typed text is microseconds, the grid is
  42 buttons. No per-line memory is added.

## Risks / Trade-offs

- [An extra click to change the window] → the label shows the window without opening it,
  and `Ctrl+G` time jumps stay one keystroke away.
- [Logs spanning years make month-by-month navigation slow] → year arrows next to the month
  ones, and the shortcuts jump to the first / last day directly.
- [Monday-first is unusual for English users] → accepted for now; a per-language first
  weekday can follow if asked.
- [Popup covers the rows while open] → it is anchored under the filter bar and closes on a
  click outside.
