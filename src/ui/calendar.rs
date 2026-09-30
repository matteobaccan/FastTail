// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Month calendar of the time range popup: a 7×6 grid of days starting on Monday
//! (ISO 8601), with arrows for the previous and next month and year. It works on day
//! counts since the Unix epoch, through the civil-date functions of `crate::timestamp`,
//! so it needs no date library.

use crate::i18n::{t, Language};
use crate::theme::CyberTheme;
use crate::timestamp::{date_to_days, days_to_date, weekday};
use egui::{Align2, FontId, Id, RichText, Sense, Stroke, Ui, Vec2};

/// Size of one day cell.
pub const CELL: Vec2 = Vec2::new(26.0, 18.0);

/// Smallest width of the month and year between the arrows. The title is as wide as the
/// longest month of the language, so the arrows do not move from month to month.
const TITLE_WIDTH: f32 = 124.0;

/// Size of the month and year title.
const TITLE_SIZE: f32 = 11.5;

/// The month a calendar shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Month {
    pub year: i64,
    pub month: u32,
}

impl Month {
    /// The month holding a day count since the epoch.
    pub fn of_days(days: i64) -> Self {
        let (year, month, _) = days_to_date(days);
        Self { year, month }
    }

    /// The month holding an instant in milliseconds.
    pub fn of_millis(millis: i64) -> Self {
        Self::of_days(millis.div_euclid(86_400_000))
    }

    /// The month `months` later (earlier when negative), kept within the years 1..=9999.
    pub fn shifted(self, months: i64) -> Self {
        let index = (self.year * 12 + i64::from(self.month) - 1 + months).clamp(12, 9999 * 12 + 11);
        Self {
            year: index.div_euclid(12),
            month: (index.rem_euclid(12) + 1) as u32,
        }
    }

    /// Day count of the first day of the month.
    pub fn first_day(self) -> i64 {
        date_to_days(self.year, self.month, 1).unwrap_or(0)
    }

    /// Day count of the first cell of the grid: the Monday on or before the 1st.
    pub fn grid_start(self) -> i64 {
        let first = self.first_day();
        first - i64::from(weekday(first))
    }
}

/// What the calendar marks: the day the side holds, the days the log spans (inclusive)
/// and today, all as day counts since the epoch.
pub struct Marks {
    pub selected: Option<i64>,
    pub span: Option<(i64, i64)>,
    pub today: i64,
}

/// Id of the cell of `days` in the calendar `id`, for the tests.
pub fn day_id(id: Id, days: i64) -> Id {
    id.with(("day", days))
}

/// Draws the calendar of `month`, whose arrows move `month`. Returns the day picked, as a
/// day count; a day of the previous or next month also moves the calendar there.
pub fn show(
    ui: &mut Ui,
    id: Id,
    month: &mut Month,
    marks: &Marks,
    theme: &CyberTheme,
    lang: Language,
) -> Option<i64> {
    let accent = theme.accent_color();
    ui.spacing_mut().item_spacing = Vec2::new(2.0, 2.0);

    // Header: « ‹ September 2026 › »
    ui.horizontal(|ui| {
        let arrow = |ui: &mut Ui, glyph: &str, tip: &str| {
            ui.add(egui::Button::new(RichText::new(glyph).monospace()).small())
                .on_hover_text(tip)
                .clicked()
        };
        if arrow(ui, "«", t(lang, "cal_prev_year")) {
            *month = month.shifted(-12);
        }
        if arrow(ui, "‹", t(lang, "cal_prev_month")) {
            *month = month.shifted(-1);
        }
        let names = t(lang, "cal_months");
        let name = names.split('|').nth(month.month as usize - 1).unwrap_or("");
        // Measured on the longest name with a four-digit year; egui caches the layout.
        let longest = names
            .split('|')
            .max_by_key(|n| n.chars().count())
            .unwrap_or("");
        let width = ui
            .painter()
            .layout_no_wrap(
                format!("{longest} 9999"),
                FontId::monospace(TITLE_SIZE),
                accent.into(),
            )
            .size()
            .x
            + 8.0;
        ui.add_sized(
            [width.max(TITLE_WIDTH), CELL.y],
            egui::Label::new(
                RichText::new(format!("{name} {}", month.year))
                    .monospace()
                    .size(TITLE_SIZE)
                    .color(accent),
            ),
        );
        if arrow(ui, "›", t(lang, "cal_next_month")) {
            *month = month.shifted(1);
        }
        if arrow(ui, "»", t(lang, "cal_next_year")) {
            *month = month.shifted(12);
        }
    });

    // Weekday names, Monday first.
    let font = FontId::monospace(11.0);
    ui.horizontal(|ui| {
        for name in t(lang, "cal_weekdays").split('|') {
            let (_, rect) = ui.allocate_space(CELL);
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                name,
                font.clone(),
                theme.text_dim().into(),
            );
        }
    });

    let start = month.grid_start();
    let mut picked = None;
    for week in 0..6 {
        ui.horizontal(|ui| {
            for dow in 0..7 {
                let days = start + week * 7 + dow;
                let (_, rect) = ui.allocate_space(CELL);
                let resp = ui.interact(rect, day_id(id, days), Sense::click());
                let (_, cell_month, cell_day) = days_to_date(days);
                let in_month = cell_month == month.month;
                let in_span = marks.span.is_some_and(|(lo, hi)| (lo..=hi).contains(&days));
                let selected = marks.selected == Some(days);
                let painter = ui.painter();
                if selected {
                    painter.rect_filled(rect, 3.0, accent.gamma_multiply(0.55));
                } else if in_span {
                    painter.rect_filled(rect, 3.0, accent.gamma_multiply(0.18));
                }
                if resp.hovered() {
                    painter.rect_stroke(
                        rect,
                        3.0,
                        Stroke::new(1.0, theme.text_dim()),
                        egui::StrokeKind::Inside,
                    );
                }
                if days == marks.today {
                    painter.rect_stroke(
                        rect,
                        3.0,
                        Stroke::new(1.0, accent),
                        egui::StrokeKind::Inside,
                    );
                }
                let color = if selected {
                    theme.text_primary()
                } else if in_month {
                    theme.text_primary().gamma_multiply(0.9)
                } else {
                    theme.text_dim().gamma_multiply(0.6)
                };
                painter.text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    cell_day.to_string(),
                    font.clone(),
                    color.into(),
                );
                if resp.clicked() {
                    picked = Some(days);
                }
                resp.on_hover_cursor(egui::CursorIcon::PointingHand);
            }
        });
    }
    if let Some(days) = picked {
        *month = Month::of_days(days);
    }
    picked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn months_shift_across_years_and_stay_in_range() {
        let sep = Month {
            year: 2026,
            month: 9,
        };
        assert_eq!(
            sep.shifted(4),
            Month {
                year: 2027,
                month: 1
            }
        );
        assert_eq!(
            sep.shifted(-9),
            Month {
                year: 2025,
                month: 12
            }
        );
        assert_eq!(sep.shifted(-12).year, 2025);
        let first = Month { year: 1, month: 1 };
        assert_eq!(first.shifted(-1), first);
        let last = Month {
            year: 9999,
            month: 12,
        };
        assert_eq!(last.shifted(1), last);
    }

    #[test]
    fn the_grid_starts_on_the_monday_before_the_first() {
        // September 2026 starts on a Tuesday: the grid opens on Monday 31 August.
        let sep = Month {
            year: 2026,
            month: 9,
        };
        assert_eq!(days_to_date(sep.grid_start()), (2026, 8, 31));
        assert_eq!(weekday(sep.grid_start()), 0);
        // June 2026 starts on a Monday: no day of May.
        let june = Month {
            year: 2026,
            month: 6,
        };
        assert_eq!(june.grid_start(), june.first_day());
        // 42 cells cover every month, even a 31-day month starting on a Sunday.
        let march = Month {
            year: 2026,
            month: 3,
        };
        assert_eq!(weekday(march.first_day()), 6);
        let last_cell = march.grid_start() + 41;
        assert!(last_cell >= date_to_days(2026, 3, 31).unwrap());
        assert_eq!(Month::of_millis(1_789_740_125_123), sep);
    }
}
