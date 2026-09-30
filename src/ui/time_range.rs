// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The time range control of the stream bar and its popup.
//!
//! The control shows the time span of the visible lines and opens a popup that edits the
//! stream's time window: for each side a text field, a calendar and hour / minute /
//! second spinners, plus shortcuts. Everything in the popup edits a draft; OK (or Enter in
//! a field) writes the draft into the stream's time texts through
//! `TailEngine::apply_time_range_text`, so the window is applied, held while the stream is
//! timed and saved exactly as typed text always was. Cancel, Esc or a click outside drop
//! the draft.

use crate::i18n::{t, Language};
use crate::tail_engine::TailEngine;
use crate::theme::CyberTheme;
pub use crate::time_range_text::{
    clock_of, control_label, last_hour, pick_day, side_readable, span_text, whole_day, with_clock,
    ControlState, Side, RELATIVE_SHORTCUTS,
};
use crate::time_range_text::{day_of, parse};
use crate::timestamp::format_millis;
use crate::ui::calendar::{self, Marks, Month};
use egui::{Id, Key, Modifiers, Response, RichText, Ui};

/// Width of the "from" and "to" fields: a full timestamp fits.
pub const FIELD_WIDTH: f32 = 180.0;

/// What the popup edits until OK: the two texts and the month each calendar shows.
#[derive(Clone, Debug, PartialEq)]
pub struct TimeRangeDraft {
    pub from: String,
    pub to: String,
    pub from_month: Month,
    pub to_month: Month,
    /// Today on the local clock, as a day count, read once when the popup opens.
    pub today: i64,
}

impl TimeRangeDraft {
    /// The draft a popup opens with: the window that is set, each calendar on the month
    /// of its side, else of the log's first timestamp, else of today.
    fn open(engine: &TailEngine) -> Self {
        // Everything the popup shows and reads is on the stream's display clock.
        let reference = engine.to_display_clock(engine.time_reference());
        let first = engine.first_timestamp().map(|t| engine.to_display_clock(t));
        let now = engine.now_on_display_clock();
        Self {
            from: engine.time_from_text.clone(),
            to: engine.time_to_text.clone(),
            from_month: opening_month(&engine.time_from_text, reference, first, now),
            to_month: opening_month(&engine.time_to_text, reference, first, now),
            today: day_of(now),
        }
    }

    fn side_mut(&mut self, side: Side) -> (&mut String, &mut Month) {
        match side {
            Side::From => (&mut self.from, &mut self.from_month),
            Side::To => (&mut self.to, &mut self.to_month),
        }
    }

    /// Fills both sides (a shortcut) and moves each calendar to its side.
    fn set(&mut self, (from, to): (String, String), reference: i64) {
        if let Some(millis) = parse(&from, reference) {
            self.from_month = Month::of_millis(millis);
        }
        if let Some(millis) = parse(&to, reference) {
            self.to_month = Month::of_millis(millis);
        }
        self.from = from;
        self.to = to;
    }
}

/// Id of the popup of a stream (and, with `"draft"`, of its draft in egui temp memory).
pub fn popup_id(engine: &TailEngine) -> Id {
    Id::new("time_range_popup").with(&engine.path)
}

/// Id of the control in the stream bar.
pub fn control_id(engine: &TailEngine) -> Id {
    popup_id(engine).with("control")
}

/// Id of a side's text field.
pub fn field_id(engine: &TailEngine, side: Side) -> Id {
    popup_id(engine).with(("field", side))
}

/// Id of a side's calendar; its day cells are `calendar::day_id(calendar_id, days)`.
pub fn calendar_id(engine: &TailEngine, side: Side) -> Id {
    popup_id(engine).with(("calendar", side))
}

/// Id of the OK button.
pub fn ok_id(engine: &TailEngine) -> Id {
    popup_id(engine).with("ok")
}

/// Id of the Cancel button.
pub fn cancel_id(engine: &TailEngine) -> Id {
    popup_id(engine).with("cancel")
}

/// Id of a shortcut button, named by its translation key (`time_range_whole`,
/// `time_range_first_day`, `time_range_last_day`, `time_range_last_hour`).
pub fn shortcut_id(engine: &TailEngine, key: &str) -> Id {
    popup_id(engine).with(key)
}

/// The draft of the open popup, `None` while the popup is closed.
pub fn draft(ctx: &egui::Context, engine: &TailEngine) -> Option<TimeRangeDraft> {
    ctx.data(|d| d.get_temp(popup_id(engine).with("draft")))
}

/// The month a side's calendar opens on: the side's own, else the log's first timestamp,
/// else the current month.
pub fn opening_month(text: &str, reference: i64, first: Option<i64>, now: i64) -> Month {
    Month::of_millis(parse(text, reference).or(first).unwrap_or(now))
}

/// Draws a widget inside a scope with a fixed id, so its rect can be read back (tests).
fn tagged(ui: &mut Ui, id: Id, add: impl FnOnce(&mut Ui) -> Response) -> Response {
    ui.scope_builder(egui::UiBuilder::new().id(id), add).inner
}

/// The time range control of the stream bar, and its popup while open.
pub fn control(ui: &mut Ui, engine: &mut TailEngine, theme: &CyberTheme, lang: Language) {
    let progress = match engine.scan_progress() {
        Some((crate::scan_job::ScanKind::Timestamps, p, _)) => Some(p),
        _ => None,
    };
    let state = ControlState {
        span: engine
            .visible_time_span()
            .map(|(a, b)| (engine.to_display_clock(a), engine.to_display_clock(b))),
        timed: engine.timestamps_complete(),
        usable: engine.timestamps_usable(),
        filtered: engine.is_time_filtered(),
        pending: engine.time_range_pending(),
        progress,
        live: engine.time_window_live(),
    };
    if state.live {
        // The engine re-reads the window every few seconds; wake up to show it.
        ui.ctx()
            .request_repaint_after(crate::tail_engine::LIVE_WINDOW_PERIOD);
    }
    if state.pending || progress.is_some() {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(100));
    }
    let color = if engine.time_range_error {
        theme.warn_color()
    } else if state.filtered {
        theme.accent_color()
    } else if state.no_timestamps() {
        theme.text_dim()
    } else {
        theme.text_primary()
    };
    let hovered = ui
        .ctx()
        .read_response(control_id(engine))
        .is_some_and(|r| r.contains_pointer());
    let mut text = RichText::new(control_label(&state, t(lang, "time_range_unavailable")))
        .monospace()
        .size(11.0)
        .color(color);
    if hovered {
        text = text.underline();
    }
    let resp = tagged(ui, control_id(engine), |ui| {
        ui.add(egui::Button::new(text).frame(false))
    })
    .on_hover_cursor(egui::CursorIcon::PointingHand)
    .on_hover_text(control_tooltip(engine, &state, lang));

    let draft_id = popup_id(engine).with("draft");
    let mut draft: Option<TimeRangeDraft> = ui.ctx().data(|d| d.get_temp(draft_id));
    if resp.clicked() {
        if draft.is_some() {
            draft = None;
        } else {
            // The calendar marks and the shortcuts need the stream timed.
            engine.request_timeline();
            draft = Some(TimeRangeDraft::open(engine));
        }
    }

    // Some(true): apply the draft; Some(false): drop it.
    let mut outcome: Option<bool> = None;
    if let Some(d) = draft.as_mut() {
        if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            outcome = Some(false);
        } else {
            let mut open = true;
            egui::Popup::from_response(&resp)
                .id(popup_id(engine))
                .open_bool(&mut open)
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                .gap(4.0)
                .show(|ui| {
                    outcome = popup_contents(ui, engine, theme, lang, d);
                });
            if !open && outcome.is_none() {
                outcome = Some(false);
            }
        }
    }
    match outcome {
        Some(true) => {
            if let Some(d) = draft.take() {
                let (from_ok, to_ok) = engine.apply_time_range_text(d.from.trim(), d.to.trim());
                engine.time_range_error = !from_ok || !to_ok;
            }
            ui.ctx().memory_mut(|m| m.stop_text_input());
        }
        Some(false) => {
            draft = None;
            ui.ctx().memory_mut(|m| m.stop_text_input());
        }
        None => {}
    }
    ui.ctx().data_mut(|data| match draft {
        Some(d) => {
            data.insert_temp(draft_id, d);
        }
        None => data.remove::<TimeRangeDraft>(draft_id),
    });
}

/// Tooltip of the control: the full span, the window that is set, its state, and what a
/// click does.
fn control_tooltip(engine: &TailEngine, state: &ControlState, lang: Language) -> String {
    let mut lines = Vec::new();
    if let Some((from, to)) = state.span {
        lines.push(format!(
            "{}: {} → {}",
            t(lang, "time_span"),
            format_millis(from),
            format_millis(to)
        ));
    }
    // The clock the span and the typed times are on.
    let display = engine.time_display();
    if display != crate::timestamp::TimeDisplay::Written {
        lines.push(format!(
            "🌐 {}",
            t(lang, "time_range_zone").replace("{zone}", &display.to_config())
        ));
    }
    let (from, to) = (engine.time_from_text.trim(), engine.time_to_text.trim());
    if !from.is_empty() || !to.is_empty() {
        let side = |s: &str| {
            if s.is_empty() {
                "…".to_string()
            } else {
                s.to_string()
            }
        };
        lines.push(format!(
            "{}: {} → {}",
            t(lang, "time_range"),
            side(from),
            side(to)
        ));
    }
    if state.live {
        if let Some((from, to)) = engine.time_window() {
            let side = |side: Option<i64>| {
                side.map_or_else(
                    || "…".to_string(),
                    |ms| format_millis(engine.to_display_clock(ms)),
                )
            };
            let key = if engine.time_window_slides_in_place() {
                "time_range_live"
            } else {
                "time_range_live_minute"
            };
            lines.push(format!(
                "⟳ {}",
                t(lang, key)
                    .replace("{from}", &side(from))
                    .replace("{to}", &side(to))
            ));
            // An empty live window: the log has nothing that recent.
            if engine.visible_line_count() == 0 {
                if let (Some(from), Some(last)) = (from, engine.last_timestamp()) {
                    if last < from {
                        lines.push(format!(
                            "ⓘ {}",
                            t(lang, "time_range_live_empty")
                                .replace("{from}", &side(Some(from)))
                                .replace("{last}", &side(Some(last)))
                        ));
                    }
                }
            }
        }
    }
    if state.pending {
        lines.push(format!("⏳ {}", t(lang, "time_range_pending")));
    }
    if engine.time_range_error {
        lines.push(format!("⚠ {}", t(lang, "time_range_invalid")));
    }
    if state.no_timestamps() {
        lines.push(format!("ⓘ {}", t(lang, "time_range_unavailable_tip")));
    }
    lines.push(t(lang, "time_range_click").to_string());
    lines.join("\n")
}

/// The popup: both sides, the shortcuts, the hints and OK / Cancel. Returns `Some(true)`
/// to apply the draft, `Some(false)` to drop it.
fn popup_contents(
    ui: &mut Ui,
    engine: &mut TailEngine,
    theme: &CyberTheme,
    lang: Language,
    draft: &mut TimeRangeDraft,
) -> Option<bool> {
    let reference = engine.to_display_clock(engine.time_reference());
    let timed = engine.timestamps_complete();
    let usable = engine.timestamps_usable();
    // Both are read from the ends of the timestamp cache: nothing here walks the lines.
    let first = engine.first_timestamp().map(|t| engine.to_display_clock(t));
    let last = if timed && usable {
        engine.last_timestamp().map(|t| engine.to_display_clock(t))
    } else {
        None
    };
    let today = draft.today;
    // On a log that goes back in time the first and last timestamps are not its bounds,
    // and finding those would mean reading every line: the span is left unshaded.
    let span = first
        .zip(last)
        .filter(|_| !engine.timestamps_unordered())
        .map(|(a, b)| (day_of(a.min(b)), day_of(a.max(b))));

    let mut enter = false;
    ui.horizontal_top(|ui| {
        for side in [Side::From, Side::To] {
            // A gap, not a separator: a vertical separator takes the height the popup
            // had last frame, and the popup would grow a little on every frame.
            if side == Side::To {
                ui.add_space(18.0);
            }
            ui.vertical(|ui| {
                let log_day = match side {
                    Side::From => first,
                    Side::To => last.or(first),
                }
                .map(day_of);
                let marks = Marks {
                    selected: None,
                    span,
                    today,
                };
                enter |= side_ui(
                    ui, engine, theme, lang, draft, side, reference, log_day, marks,
                );
            });
        }
    });

    ui.separator();
    // Shortcuts: they fill the draft, like any edit.
    let stamps = timed && usable;
    let base = popup_id(engine);
    ui.horizontal(|ui| {
        if tagged(ui, base.with("time_range_whole"), |ui| {
            ui.button(t(lang, "time_range_whole"))
        })
        .on_hover_text(t(lang, "time_range_clear"))
        .clicked()
        {
            draft.set((String::new(), String::new()), reference);
        }
        let shortcut = |ui: &mut Ui, key: &str, tip: &str, at: Option<i64>| {
            tagged(ui, base.with(key), |ui| {
                ui.add_enabled(stamps && at.is_some(), egui::Button::new(t(lang, key)))
            })
            .on_hover_text(t(lang, tip))
            .on_disabled_hover_text(t(lang, "time_range_unavailable_tip"))
            .clicked()
            .then_some(at)
            .flatten()
        };
        if let Some(first) = shortcut(
            ui,
            "time_range_first_day",
            "time_range_first_day_tip",
            first,
        ) {
            draft.set(whole_day(first), reference);
        }
        if let Some(last) = shortcut(ui, "time_range_last_day", "time_range_last_day_tip", last) {
            draft.set(whole_day(last), reference);
        }
        if let Some(last) = shortcut(ui, "time_range_last_hour", "time_range_last_hour_tip", last) {
            draft.set(last_hour(last), reference);
        }
    });
    // Relative to now: the window slides with the clock.
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("⟳ {}:", t(lang, "time_range_relative")))
                .monospace()
                .size(10.5),
        )
        .on_hover_text(t(lang, "time_range_relative_tip"));
        for (label, from) in RELATIVE_SHORTCUTS {
            if tagged(ui, base.with(("relative", *label)), |ui| {
                ui.add_enabled(!timed || usable, egui::Button::new(*label))
            })
            .on_hover_text(t(lang, "time_range_relative_tip"))
            .on_disabled_hover_text(t(lang, "time_range_unavailable_tip"))
            .clicked()
            {
                draft.set((from.to_string(), String::new()), reference);
            }
        }
    });

    // Where the stream stands: a window waiting for the timing, the timing itself, or a
    // stream whose lines cannot be placed in time.
    if engine.time_range_pending() {
        ui.label(
            RichText::new(format!("⏳ {}", t(lang, "time_range_pending")))
                .monospace()
                .size(10.5)
                .color(theme.warn_color()),
        );
    }
    if !timed {
        let progress = match engine.scan_progress() {
            Some((crate::scan_job::ScanKind::Timestamps, p, _)) => {
                format!(" {:.0}%", (p * 100.0).clamp(0.0, 100.0))
            }
            _ => String::new(),
        };
        ui.label(
            RichText::new(format!("⏳ {}{progress}", t(lang, "scan_timestamps")))
                .monospace()
                .size(10.5)
                .color(theme.text_dim()),
        );
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(100));
    } else if !usable {
        ui.label(
            RichText::new(format!("ⓘ {}", t(lang, "time_range_unavailable")))
                .monospace()
                .size(10.5)
                .color(theme.text_dim()),
        )
        .on_hover_text(t(lang, "time_range_unavailable_tip"));
    }

    let readable = side_readable(&draft.from, reference) && side_readable(&draft.to, reference);
    let mut outcome = None;
    ui.horizontal(|ui| {
        let ok = tagged(ui, ok_id(engine), |ui| {
            ui.add_enabled(readable, egui::Button::new(t(lang, "session_ok")))
        })
        .on_disabled_hover_text(t(lang, "time_range_invalid"));
        if ok.clicked() || (enter && readable) {
            outcome = Some(true);
        }
        if tagged(ui, cancel_id(engine), |ui| {
            ui.button(t(lang, "session_cancel"))
        })
        .clicked()
        {
            outcome = Some(false);
        }
    });
    outcome
}

/// One side of the popup: its label, field, invalid-time hint, calendar and spinners.
/// Returns whether Enter was pressed in the field.
#[allow(clippy::too_many_arguments)]
fn side_ui(
    ui: &mut Ui,
    engine: &TailEngine,
    theme: &CyberTheme,
    lang: Language,
    draft: &mut TimeRangeDraft,
    side: Side,
    reference: i64,
    log_day: Option<i64>,
    mut marks: Marks,
) -> bool {
    let (label, hint) = match side {
        Side::From => (t(lang, "time_from_hint"), "2026-09-18 14:02:05"),
        Side::To => (t(lang, "time_to_hint"), "2026-09-18 16:30"),
    };
    let (text, month) = draft.side_mut(side);
    ui.label(
        RichText::new(label)
            .monospace()
            .size(11.0)
            .color(theme.accent_color()),
    );
    let field = ui.add(
        egui::TextEdit::singleline(text)
            .id(field_id(engine, side))
            .hint_text(hint)
            .desired_width(FIELD_WIDTH),
    );
    if field.changed() {
        if let Some(millis) = parse(text, reference) {
            *month = Month::of_millis(millis);
        }
    }
    let enter = field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
    // A side is judged once the user leaves it: `14:0` on the way to `14:02` is no error.
    if !side_readable(text, reference) && !field.has_focus() {
        ui.label(
            RichText::new(format!("⚠ {}", t(lang, "time_range_invalid")))
                .monospace()
                .size(10.5)
                .color(theme.warn_color()),
        );
    }

    marks.selected = parse(text, reference).map(day_of);
    if let Some(days) = calendar::show(ui, calendar_id(engine, side), month, &marks, theme, lang) {
        *text = pick_day(text, reference, days);
    }

    // Spinners: a change writes the side as `YYYY-MM-DD HH:MM:SS`, on the side's day, or
    // on the log's (or today's) day in the month shown, else on its first day.
    let before = clock_of(text, reference, side);
    let (mut hour, mut minute, mut second) = before;
    ui.horizontal(|ui| {
        ui.label(RichText::new("🕘").monospace().color(theme.text_dim()));
        let spin = |ui: &mut Ui, value: &mut u32, max: u32| {
            ui.add(
                egui::DragValue::new(value)
                    .range(0..=max)
                    .speed(0.1)
                    .custom_formatter(|v, _| format!("{:02}", v as u32)),
            )
            .on_hover_text(t(lang, "time_clock_tip"));
        };
        spin(ui, &mut hour, 23);
        ui.label(":");
        spin(ui, &mut minute, 59);
        ui.label(":");
        spin(ui, &mut second, 59);
    });
    if (hour, minute, second) != before {
        let fallback = [log_day, Some(marks.today)]
            .into_iter()
            .flatten()
            .find(|&days| Month::of_days(days) == *month)
            .unwrap_or_else(|| month.first_day());
        *text = with_clock(text, reference, fallback, (hour, minute, second));
    }
    enter
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timestamp::parse_user_time;

    fn ms(text: &str) -> i64 {
        parse_user_time(text, 0).unwrap()
    }

    #[test]
    fn calendars_open_where_the_side_or_the_log_is() {
        let first = ms("2025-07-31 09:26:03");
        let now = ms("2026-09-28 12:00:00");
        let july = Month {
            year: 2025,
            month: 7,
        };
        assert_eq!(opening_month("", first, Some(first), now), july);
        assert_eq!(
            opening_month("2026-01-05", first, Some(first), now),
            Month {
                year: 2026,
                month: 1
            }
        );
        // A bare time is on the log's day.
        assert_eq!(opening_month("14:02", first, Some(first), now), july);
        assert_eq!(
            opening_month("", now, None, now),
            Month {
                year: 2026,
                month: 9
            }
        );
        assert_eq!(opening_month("14:6x", first, Some(first), now), july);
    }
}
