## 1. Civil-date helpers

- [ ] 1.1 Make `days_from_civil` / `civil_from_days` usable from the UI (public wrappers in `src/timestamp.rs`) and add `days_in_month`, `weekday` (Monday = 0), with unit tests (leap years, month ends, 1970 and 2100)
- [ ] 1.2 Add `TailEngine::last_timestamp()` (last non-`NO_TIMESTAMP` entry of the cache) with a test

## 2. Calendar widget and draft

- [ ] 2.1 New `src/ui/calendar.rs`: month state (year, month), 7×6 grid starting Monday, month / year arrows, tinted log span, outlined today; returns the picked day
- [ ] 2.2 Hour / minute / second spinners (`DragValue`, clamped 0-23 / 0-59) that rewrite the draft side as `YYYY-MM-DD HH:MM:SS`
- [ ] 2.3 Draft text rules as pure functions (pick a day keeping the time, bare date otherwise; spinner change; shortcuts), with unit tests for every scenario of the spec

## 3. Control and popup

- [ ] 3.1 Stream bar: the time span becomes a clickable control (label builder: shared-date collapse, 40-character cut, "no timestamps", timing progress; accent / warning colours, `⏳` when pending; tooltip with the window and "click to set the time range"); unit-test the label builder
- [ ] 3.2 Remove the inline fields from the filter row, keeping `📊` and `🔍`
- [ ] 3.3 Popup anchored under the control: two sides (field ≥ 170 px, calendar, spinners), hints, shortcuts, OK / Cancel; draft in egui temp memory; OK and `Enter` apply through `apply_time_range_text`; Cancel, `Esc`, click outside discard; OK disabled while a side does not parse
- [ ] 3.4 Opening month: side value, else the log's first timestamp, else today
- [ ] 3.5 GUI tests (egui `Context` + `run_ui`): click opens the popup prefilled; OK applies and filters; `Esc` leaves the window unchanged; OK disabled on invalid text

## 4. Translations and docs

- [ ] 4.1 i18n keys in every language: "no timestamps", "click to set the time range", from / to labels, OK / Cancel if not already present, month names, weekday abbreviations, shortcut labels and tooltips (the translation test must pass)
- [ ] 4.2 README (time range paragraphs, feature highlights, shortcuts), CHANGELOG `[Unreleased]`, `docs/ui-design.md` (stream bar, filter row, popup, glyphs)

## 5. Verification

- [ ] 5.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test`, green Linux and Windows CI
- [ ] 5.2 Preview exe for the maintainer (with the 0.12.0 preview round)
- [ ] 5.3 Archive the change before the 0.12.0 release
