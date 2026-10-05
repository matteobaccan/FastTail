// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The month grid of the time range dialog: a cursor day (days since 1970-01-01), moved
//! by days, weeks and months, and the weeks of the cursor's month, Monday first.

use crate::i18n_tui::en;

use crate::timestamp::{date_to_days, days_to_date};

const MONTHS: [&str; 12] = [
    en("January"),
    en("February"),
    en("March"),
    en("April"),
    en("May"),
    en("June"),
    en("July"),
    en("August"),
    en("September"),
    en("October"),
    en("November"),
    en("December"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Calendar {
    /// The day under the cursor, in days since 1970-01-01.
    pub cursor: i64,
}

impl Calendar {
    pub fn new(cursor: i64) -> Self {
        Self { cursor }
    }

    pub fn move_days(&mut self, n: i64) {
        self.cursor += n;
    }

    /// The same day `n` months later (the last day of a shorter month).
    pub fn move_months(&mut self, n: i32) {
        let (y, m, d) = days_to_date(self.cursor);
        let total = y * 12 + (m as i64 - 1) + n as i64;
        let (y, m) = (total.div_euclid(12), total.rem_euclid(12) as u32 + 1);
        let last = days_in_month(y, m);
        if let Some(days) = date_to_days(y, m, d.min(last)) {
            self.cursor = days;
        }
    }

    /// "September 2026".
    pub fn title(&self, lang: crate::i18n::Language) -> String {
        let (y, m, _) = days_to_date(self.cursor);
        format!(
            "{} {y}",
            crate::i18n_tui::tx(lang, MONTHS[(m - 1) as usize])
        )
    }

    /// The weeks of the cursor's month, Monday first; `None` outside the month.
    pub fn weeks(&self) -> Vec<[Option<i64>; 7]> {
        let (y, m, _) = days_to_date(self.cursor);
        let first = date_to_days(y, m, 1).unwrap_or(self.cursor);
        let len = days_in_month(y, m) as i64;
        // 1970-01-01 was a Thursday: Monday is 0.
        let lead = (first + 3).rem_euclid(7);
        let mut weeks = Vec::with_capacity(6);
        let mut week = [None; 7];
        for i in 0..(lead + len) {
            let col = (i % 7) as usize;
            if i >= lead {
                week[col] = Some(first + i - lead);
            }
            if col == 6 {
                weeks.push(week);
                week = [None; 7];
            }
        }
        if week.iter().any(Option::is_some) {
            weeks.push(week);
        }
        weeks
    }
}

/// Days of month `m` (1-12) of year `y`.
pub fn days_in_month(y: i64, m: u32) -> u32 {
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    match (date_to_days(y, m, 1), date_to_days(ny, nm, 1)) {
        (Some(a), Some(b)) => (b - a) as u32,
        _ => 30,
    }
}

/// The day of the month of `days`.
pub fn day_of_month(days: i64) -> u32 {
    days_to_date(days).2
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i64, m: u32, d: u32) -> i64 {
        date_to_days(y, m, d).unwrap()
    }

    #[test]
    fn the_grid_starts_on_monday_and_holds_the_month() {
        // 1 September 2026 is a Tuesday.
        let c = Calendar::new(day(2026, 9, 18));
        assert_eq!(c.title(crate::i18n::Language::En), "September 2026");
        let weeks = c.weeks();
        assert_eq!(weeks[0][0], None, "Monday 31 August is outside");
        assert_eq!(weeks[0][1], Some(day(2026, 9, 1)));
        let last = weeks.last().unwrap();
        assert!(last.contains(&Some(day(2026, 9, 30))));
        assert_eq!(weeks.iter().flatten().flatten().count(), 30);
    }

    #[test]
    fn moving_by_days_and_months_keeps_a_valid_day() {
        let mut c = Calendar::new(day(2026, 1, 31));
        c.move_months(1);
        assert_eq!(c.cursor, day(2026, 2, 28), "the last day of February");
        c.move_months(-2);
        assert_eq!(c.cursor, day(2025, 12, 28));
        c.move_days(7);
        assert_eq!(c.cursor, day(2026, 1, 4));
        assert_eq!(day_of_month(c.cursor), 4);
        assert_eq!(days_in_month(2024, 2), 29);
    }
}
