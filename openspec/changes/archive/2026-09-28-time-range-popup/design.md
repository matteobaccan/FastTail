## Context

The stream bar (`src/ui/dock.rs`) shows `🕘 from → to`, the span of the visible lines from
`TailEngine::visible_time_span()`, read-only, tinted while `is_time_filtered()`. The filter
row holds `render_time_range`: a label, two 74 px `TextEdit`s, a ✖, the hints, then the
`📊` timeline toggle and the `🔍` go-to-time button. The fields write
`engine.time_from_text` / `time_to_text` through `apply_time_range_text`, which parses
them (`parse_user_time`, `end_of_typed_time`), holds the window while a background timing
scan runs, and applies it. A bare `YYYY-MM-DD` is accepted and the fields are never
disabled (#96). The project has no date library: `src/timestamp.rs` has private
`days_from_civil` / `civil_from_days`.

The maintainer's request (2026-09-28): the range shown should be the range edited, through
a popup, and a confirmation should change the visible range.

## Goals / Non-Goals

**Goals:**
- One place for the time range: the span in the stream bar, which shows and edits it.
- Pick dates with the mouse (calendar) and times without typing a format (spinners), while
  typing any accepted format keeps working.
- An explicit confirmation: nothing changes until OK.
- Zero change to the engine's filtering and to what is saved.

**Non-Goals:**
- Time zones, drag-to-select on the timeline, cross-stream windows.
- A reusable general-purpose date picker crate.

## Decisions

- **The span is the control.** The existing span label becomes a `Button` (frameless, same
  text, pointer cursor, underline on hover) that toggles the popup; it is shown in Text view
  even before the stream is timed (`🕘 …` with the timing progress) and on a stream without
  timestamps (`🕘 no timestamps`, dimmed), so the time range is always reachable. The label
  is the visible span, not the window typed: with a window set it is the span of the
  lines that fall in it, which is what the maintainer wants to see; the window itself is in
  the tooltip. Colours: accent while `is_time_filtered()`, warning while
  `time_range_error`, `⏳` suffix while `time_range_pending()`.
- **Inline fields removed from the filter row;** `📊` and `🔍` stay there, next to the
  level selector, as they are about time but not about the window.
- **Draft, then confirm.** The popup keeps its own `TimeRangeDraft { from, to, from_month,
  to_month }` in egui temp memory keyed by the stream, created from `time_from_text` /
  `time_to_text` when the popup opens. Calendar, spinners, shortcuts and typing edit the
  draft only. OK (or `Enter` in a field) copies the draft into the engine's texts and calls
  `apply_time_range_text`; Cancel, `Esc` or a click outside drop the draft. OK is disabled
  while a draft side does not parse (`parse_user_time`), with the invalid-time hint under
  that side. Alternative (live apply, as the old fields): rejected by the maintainer's
  request, and it refilters a large file on every calendar click.
- **The text stays the source of truth.** Calendar and spinners rewrite the draft text in
  the canonical shape (`YYYY-MM-DD` or `YYYY-MM-DD HH:MM:SS`); sessions, pending windows
  and the invalid-time logic keep working unchanged.
- **Own calendar widget, no dependency.** A 7×6 grid of day buttons on top of the
  civil-date functions (made public with `weekday` and `days_in_month`). Weeks start on
  Monday (ISO 8601) in every language; month and weekday names come from `src/i18n.rs`.
- **Which month opens.** The side's draft value if it parses, else the log's first
  timestamp (`time_reference`), else today. Days between the log's first and last
  timestamp are tinted; today gets an outline.
- **Picking a day keeps the time**; otherwise "from" becomes the bare date and "to" the bare
  date (through 23:59:59.999, per `end_of_typed_time`). Spinners on a bare date turn it into
  `YYYY-MM-DD HH:MM:SS`.
- **Shortcuts** fill the draft from the log's first and last timestamp
  (`time_reference` and a new `last_timestamp()`, O(1) from the end of the cache); disabled
  until the stream has been timed or when it has no usable timestamps.
- **Popup = `egui::Popup` anchored to the control**, closed on `Esc` or a click outside
  (both cancel); the fields are ~170 px wide. It never blocks: parsing a text is
  microseconds, the grids are 2 × 42 buttons.

## Risks / Trade-offs

- [Users of the old inline fields look for them in the filter row] → the CHANGELOG and README
  say where the range moved; the span's tooltip says "click to set the time range".
- [An extra click to change the window] → the label shows the range without opening it, and
  `Ctrl+G` time jumps stay one keystroke away.
- [Logs spanning years] → year arrows next to the month ones, and the shortcuts jump to the
  first / last day directly.
- [Monday-first is unusual for English users] → accepted; a per-language first weekday can
  follow if asked.
