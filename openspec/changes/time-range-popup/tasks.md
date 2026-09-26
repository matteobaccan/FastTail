## 1. Civil-date helpers

- [ ] 1.1 Make `days_from_civil` / `civil_from_days` usable from the UI (public wrappers in `src/timestamp.rs`) and add `days_in_month`, `weekday` (Monday = 0) and `day_start`, with unit tests (leap years, month ends, 1970 and 2100)
- [ ] 1.2 Add `TailEngine::last_timestamp()` (last non-`NO_TIMESTAMP` entry of the cache) with a test

## 2. Calendar widget

- [ ] 2.1 New `src/ui/calendar.rs`: month state (year, month), 7×6 grid starting Monday, month / year arrows, tinted log span, outlined today; returns the picked day
- [ ] 2.2 Hour / minute / second spinners (`DragValue`, clamped 0-23 / 0-59) that rewrite the side as `YYYY-MM-DD HH:MM:SS`
- [ ] 2.3 Text rewriting rules as pure functions (pick a day keeping the time, bare date otherwise; spinner change), with unit tests for every scenario of the spec

## 3. Button and popup

- [ ] 3.1 Replace the inline fields in `render_time_range` with the button: label from the two texts (shared-date collapse, one-sided, "all times", 40-character cut with tooltip), accent / warning colours, `⏳` when pending; unit-test the label builder
- [ ] 3.2 Popup anchored under the button: two sides (field ≥ 170 px, calendar, spinners), ✖, hints; closes on `Esc`, click outside, second click
- [ ] 3.3 Shortcuts Whole log / First day / Last day / Last hour, disabled until the stream is timed or when it has no usable timestamps
- [ ] 3.4 Opening month: side value, else the log's first timestamp, else today

## 4. Translations and docs

- [ ] 4.1 i18n keys in every language: "all times", from / to labels, month names, weekday abbreviations, shortcut labels and tooltips (the translation test must pass)
- [ ] 4.2 README (time range paragraphs and feature highlights), CHANGELOG `[Unreleased]` (Added), screenshots if any show the old fields

## 5. Verification

- [ ] 5.1 `cargo fmt`, `cargo test`, green Linux and Windows CI
- [ ] 5.2 Manual check on `tests/logs/application.log` and a large log (> 16 MB, pending window shown on the button)
- [ ] 5.3 Archive the change after the 0.11.0 release
