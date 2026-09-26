//! Timeline histogram strip: the stream's lines per level over time, drawn above the rows
//! from the engine's `TimeHistogram`, with the current time window shaded and an optional
//! lane marking where the search hits fall. A click selects one column's span, a drag the
//! span between two columns; the selection becomes the time range, written into the
//! from/to fields exactly as if the user had typed it.
//!
//! Buckets are grouped into at most one column per pixel (`TimeHistogram::columns`), or
//! spread over several pixels when there are fewer buckets than pixels. Columns and lane
//! are cached per stream in egui memory and rebuilt when the histogram, the width or the
//! hit list change, at most four times a second while the stream grows.

use std::time::{Duration, Instant};

use crate::i18n::{t, Language};
use crate::log_level::LogLevel;
use crate::tail_engine::TailEngine;
use crate::theme::CyberTheme;
use crate::time_histogram::{column_of_bucket, selection_texts, Column, TimeHistogram};
use egui::{Stroke, Ui};

/// Height of the strip in points (bars, lane and labels).
pub const STRIP_HEIGHT: f32 = 56.0;
/// Height of the search lane above the bars.
const LANE_HEIGHT: f32 = 5.0;
/// Minimum time between two rebuilds caused by the stream growing.
const REBUILD_INTERVAL: Duration = Duration::from_millis(250);

/// Level groups stacked bottom-up: the levels each segment sums.
pub const GROUPS: [&[LogLevel]; 5] = [
    &[LogLevel::Error, LogLevel::Fatal],
    &[LogLevel::Warn],
    &[LogLevel::Info],
    &[LogLevel::Debug, LogLevel::Trace],
    &[LogLevel::Unknown],
];

/// Lines of `column` in each of the `GROUPS`.
pub fn group_counts(column: &Column) -> [u64; 5] {
    let mut out = [0u64; 5];
    for (slot, levels) in out.iter_mut().zip(GROUPS) {
        *slot = levels.iter().map(|&l| column.counts[l as usize]).sum();
    }
    out
}

/// Which columns hold at least one of the `hits` (line indices), from each line's
/// effective timestamp. Lines not timed yet, or outside the histogram's span, mark nothing.
pub fn search_lane(
    histogram: &TimeHistogram,
    columns: usize,
    hits: &[usize],
    timestamp: impl Fn(usize) -> Option<i64>,
) -> Vec<bool> {
    let k = histogram.len().min(columns);
    let mut lane = vec![false; k];
    if k == 0 {
        return lane;
    }
    for &line in hits {
        if let Some(bucket) = timestamp(line).and_then(|ts| histogram.bucket_of(ts)) {
            lane[column_of_bucket(bucket, histogram.len(), columns)] = true;
        }
    }
    lane
}

/// What the cached columns and lane were built from; any change asks for a rebuild.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CacheKey {
    width: usize,
    histogram_generation: u64,
    lane: bool,
    search_generation: u64,
    buffer_generation: u64,
}

/// Per-stream cache kept in egui memory, plus the column a drag started from.
#[derive(Debug, Clone, Default)]
pub struct TimelineCache {
    key: Option<CacheKey>,
    built_at: Option<Instant>,
    pub columns: Vec<Column>,
    pub lane: Vec<bool>,
    /// Tallest column, the scale of the bars.
    pub peak: u64,
    drag_from: Option<usize>,
}

impl TimelineCache {
    /// Brings columns and lane up to date for a strip `width` pixels wide. A width change
    /// or a toggled lane rebuilds at once; the rest moves while the stream grows or is
    /// timed and rebuilds at most every `REBUILD_INTERVAL`, and not at all while `hold`
    /// (a press or drag on the strip, whose column indices must keep their meaning).
    /// Returns the time after which a throttled rebuild is due, if one is waiting.
    pub fn refresh(
        &mut self,
        engine: &TailEngine,
        width: usize,
        lane: bool,
        hold: bool,
    ) -> Option<Duration> {
        let key = CacheKey {
            width,
            histogram_generation: engine.histogram_generation,
            lane,
            search_generation: engine.search_generation,
            buffer_generation: engine.buffer_generation,
        };
        if self.key == Some(key) {
            return None;
        }
        let urgent = self
            .key
            .is_none_or(|old| old.width != key.width || old.lane != key.lane);
        if !urgent && hold && self.key.is_some() {
            return Some(REBUILD_INTERVAL);
        }
        if !urgent {
            if let Some(since) = self.built_at.map(|at| at.elapsed()) {
                if since < REBUILD_INTERVAL {
                    return Some(REBUILD_INTERVAL - since);
                }
            }
        }
        let histogram = engine.time_histogram();
        self.columns = histogram.columns(width);
        self.peak = self.columns.iter().map(Column::total).max().unwrap_or(0);
        self.lane = if lane && !engine.last_searched_query.is_empty() {
            search_lane(histogram, width, &engine.search_matches, |line| {
                engine.line_timestamp(line)
            })
        } else {
            Vec::new()
        };
        self.key = Some(key);
        self.built_at = Some(Instant::now());
        None
    }
}

/// Column under the horizontal offset `x` of a strip `width` points wide with `columns`
/// columns laid out evenly.
pub fn column_at(x: f32, width: f32, columns: usize) -> usize {
    if columns == 0 || width <= 0.0 {
        return 0;
    }
    let frac = (x / width).clamp(0.0, 1.0);
    ((frac * columns as f32) as usize).min(columns - 1)
}

/// Horizontal fraction of the strip at which instant `ms` sits: the column holding it,
/// and the share of that column's time before it. Reads the columns alone, so a cache a
/// little behind the histogram still places the window consistently with its bars.
fn x_frac_of(ms: i64, columns: &[Column]) -> f32 {
    let (Some(first), Some(last)) = (columns.first(), columns.last()) else {
        return 0.0;
    };
    if ms <= first.start_ms {
        return 0.0;
    }
    if ms >= last.end_ms {
        return 1.0;
    }
    let c = columns.partition_point(|col| col.end_ms <= ms);
    let col = &columns[c.min(columns.len() - 1)];
    let within = (ms - col.start_ms) as f32 / (col.end_ms - col.start_ms).max(1) as f32;
    (c as f32 + within.clamp(0.0, 1.0)) / columns.len() as f32
}

/// `2024-03-05 14:02:00 – 14:02:59`: the span of the columns `a..=b`, the date once.
fn span_text(columns: &[Column], a: usize, b: usize) -> String {
    let Some((from, to)) = selection_texts(columns, a, b) else {
        return String::new();
    };
    let to_short = if from.get(..10) == to.get(..10) {
        to.get(11..).unwrap_or(&to).to_string()
    } else {
        to
    };
    format!("{from} – {to_short}")
}

/// Interaction id of a stream's strip.
pub fn strip_id(engine: &TailEngine) -> egui::Id {
    egui::Id::new("timeline_strip").with(&engine.path)
}

/// Draws the strip across the available width and returns the `(from, to)` texts of a
/// finished click or drag, for the caller to apply through `apply_time_range_text`.
pub fn show(
    ui: &mut Ui,
    engine: &TailEngine,
    theme: &CyberTheme,
    lang: Language,
    lane_on: bool,
) -> Option<(String, String)> {
    let width = ui.available_width().max(1.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, STRIP_HEIGHT), egui::Sense::hover());
    let response = ui.interact(rect, strip_id(engine), egui::Sense::click_and_drag());
    let id = egui::Id::new("timeline_cache").with(&engine.path);
    let mut cache: TimelineCache = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    let hold = response.is_pointer_button_down_on() || response.dragged();
    let width_px = rect.width().round().max(1.0) as usize;
    if let Some(wait) = cache.refresh(engine, width_px, lane_on, hold) {
        ui.ctx().request_repaint_after(wait);
    }

    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, theme.panel_bg().gamma_multiply(0.85));
    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(1.0, theme.border_color().gamma_multiply(0.4)),
        egui::StrokeKind::Inside,
    );
    let small = egui::FontId::monospace(10.0);
    let histogram = engine.time_histogram();
    let columns = &cache.columns;
    let k = columns.len();
    let timing = engine
        .scan_progress()
        .filter(|(kind, _, _)| *kind == crate::scan_job::ScanKind::Timestamps)
        .map(|(_, progress, _)| progress);
    if timing.is_some() || !engine.timestamps_complete() {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(100));
    }

    let lane_rect = egui::Rect::from_min_max(
        rect.left_top() + egui::vec2(0.0, 2.0),
        egui::pos2(rect.right(), rect.top() + 2.0 + LANE_HEIGHT),
    );
    let bars = egui::Rect::from_min_max(
        egui::pos2(rect.left(), lane_rect.bottom() + 2.0),
        rect.right_bottom() - egui::vec2(0.0, 1.0),
    );
    let col_w = if k > 0 { rect.width() / k as f32 } else { 0.0 };
    let colours = [
        theme.level_color(LogLevel::Error),
        theme.level_color(LogLevel::Warn),
        theme.level_color(LogLevel::Info),
        theme.level_color(LogLevel::Debug),
        theme.text_dim().gamma_multiply(0.45),
    ];

    // Current window, under the bars.
    if k > 0 && engine.is_time_filtered() {
        let x0 = engine
            .time_from
            .map(|ms| x_frac_of(ms, columns))
            .unwrap_or(0.0);
        let x1 = engine
            .time_to
            .map(|ms| x_frac_of(ms + 1, columns))
            .unwrap_or(1.0);
        let shade = egui::Rect::from_min_max(
            egui::pos2(rect.left() + x0 * rect.width(), rect.top()),
            egui::pos2(
                (rect.left() + x1 * rect.width()).max(rect.left() + x0 * rect.width() + 2.0),
                rect.bottom(),
            ),
        );
        painter.rect_filled(shade, 0.0, theme.accent_color().gamma_multiply(0.18));
        painter.rect_stroke(
            shade,
            0.0,
            Stroke::new(1.0, theme.accent_color().gamma_multiply(0.7)),
            egui::StrokeKind::Inside,
        );
    }

    // Stacked bars, linear up to the tallest column.
    if cache.peak > 0 {
        let scale = bars.height() / cache.peak as f32;
        for (c, column) in columns.iter().enumerate() {
            let x0 = rect.left() + c as f32 * col_w;
            let x1 = (x0 + col_w - if col_w >= 4.0 { 1.0 } else { 0.0 }).max(x0 + 1.0);
            let mut y = bars.bottom();
            for (n, colour) in group_counts(column).into_iter().zip(colours) {
                if n == 0 {
                    continue;
                }
                // At least a point tall so a single error stays visible.
                let h = (n as f32 * scale).max(1.0);
                painter.rect_filled(
                    egui::Rect::from_min_max(egui::pos2(x0, y - h), egui::pos2(x1, y)),
                    0.0,
                    colour,
                );
                y -= h;
            }
        }
        painter.text(
            rect.left_top() + egui::vec2(3.0, LANE_HEIGHT + 3.0),
            egui::Align2::LEFT_TOP,
            t(lang, "timeline_peak").replace("{n}", &cache.peak.to_string()),
            small.clone(),
            theme.text_dim(),
        );
    }

    // Search lane: one mark per column holding a hit.
    for (c, _) in cache.lane.iter().enumerate().filter(|(_, &hit)| hit) {
        let x0 = rect.left() + c as f32 * col_w;
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(x0, lane_rect.top()),
                egui::pos2(x0 + col_w.max(1.5), lane_rect.bottom()),
            ),
            0.0,
            theme.warn_color(),
        );
    }

    let status = if let Some(progress) = timing {
        Some(format!(
            "⏳ {} {:.0}%",
            t(lang, "scan_timestamps"),
            (progress * 100.0).clamp(0.0, 100.0)
        ))
    } else if k == 0 {
        Some(t(lang, "timeline_empty").to_string())
    } else {
        None
    };
    if let Some(text) = status {
        painter.text(
            rect.right_top() + egui::vec2(-4.0, LANE_HEIGHT + 3.0),
            egui::Align2::RIGHT_TOP,
            text,
            small,
            theme.warn_color(),
        );
    }

    // Drag in progress: the prospective selection, outlined.
    let pointer_col = response
        .interact_pointer_pos()
        .or(response.hover_pos())
        .map(|pos| column_at(pos.x - rect.left(), rect.width(), k));
    if response.drag_started() && k > 0 {
        // The drag is recognised a few points after the press: it starts where the
        // button went down.
        cache.drag_from = ui
            .input(|i| i.pointer.press_origin())
            .map(|pos| column_at(pos.x - rect.left(), rect.width(), k))
            .or(pointer_col);
    }
    if let (Some(a), Some(b)) = (cache.drag_from, pointer_col) {
        if response.dragged() {
            let (a, b) = (a.min(b), a.max(b));
            painter.rect_stroke(
                egui::Rect::from_min_max(
                    egui::pos2(rect.left() + a as f32 * col_w, rect.top()),
                    egui::pos2(rect.left() + (b + 1) as f32 * col_w, rect.bottom()),
                ),
                0.0,
                Stroke::new(1.5, theme.text_primary().gamma_multiply(0.8)),
                egui::StrokeKind::Inside,
            );
        }
    }

    let mut selected = None;
    if k > 0 {
        if response.drag_stopped() {
            if let (Some(a), Some(b)) = (cache.drag_from.take(), pointer_col) {
                selected = selection_texts(columns, a, b);
            }
        } else if response.clicked() {
            if let Some(c) = pointer_col {
                selected = selection_texts(columns, c, c);
            }
        }
    }

    if let Some(c) = pointer_col.filter(|_| response.hovered() && k > 0) {
        let column = &columns[c];
        let mut tip = span_text(columns, c, c);
        let names = ["ERROR/FATAL", "WARN", "INFO", "DEBUG/TRACE"];
        for (i, n) in group_counts(column).into_iter().enumerate() {
            if n > 0 {
                let name = names
                    .get(i)
                    .copied()
                    .unwrap_or(t(lang, "timeline_no_level"));
                tip.push_str(&format!("\n{name}: {n}"));
            }
        }
        if histogram.untimed() > 0 {
            tip.push_str("\n\n");
            tip.push_str(
                &t(lang, "timeline_untimed").replace("{n}", &histogram.untimed().to_string()),
            );
        }
        if lane_on && !engine.last_searched_query.is_empty() {
            tip.push_str("\n\n");
            tip.push_str(t(lang, "timeline_lane_note"));
        }
        response.clone().on_hover_text_at_pointer(tip);
    }

    ui.data_mut(|d| d.insert_temp(id, cache));
    selected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lane_marks_the_columns_of_the_hits() {
        let mut h = TimeHistogram::default();
        let stamps: Vec<i64> = (0..100).map(|s| s * 1_000).collect();
        for &ts in &stamps {
            h.add(ts, LogLevel::Info as u8);
        }
        let at = |line: usize| stamps.get(line).copied();
        // 100 buckets on 10 columns: lines 5 and 95 mark the first and last column.
        let lane = search_lane(&h, 10, &[5, 95, 1_000], at);
        assert_eq!(lane.len(), 10);
        assert_eq!(
            lane.iter()
                .enumerate()
                .filter(|(_, &m)| m)
                .map(|(c, _)| c)
                .collect::<Vec<_>>(),
            vec![0, 9],
            "an untimed hit marks nothing"
        );
        // More columns than buckets: one per bucket.
        let lane = search_lane(&h, 400, &[42], at);
        assert_eq!(lane.len(), 100);
        assert!(lane[42]);
        assert!(search_lane(&TimeHistogram::default(), 10, &[1], at).is_empty());
    }

    #[test]
    fn a_press_on_the_strip_holds_the_columns_while_the_stream_grows() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("live.log");
        let line = |s: u32| {
            format!(
                "2026-09-18T14:02:{s:02}Z INFO tick
"
            )
        };
        std::fs::write(&path, (0..10).map(line).collect::<String>()).unwrap();
        let mut engine = TailEngine::open(&path).unwrap();
        engine.size_check_interval = Duration::ZERO;
        engine.request_timeline();
        let mut cache = TimelineCache::default();
        assert!(cache.refresh(&engine, 200, false, false).is_none());
        let before = cache.columns.clone();

        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        f.write_all((10..40).map(line).collect::<String>().as_bytes())
            .unwrap();
        drop(f);
        let started = Instant::now();
        while engine.histogram_lines() < 40 {
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "append not seen"
            );
            engine.poll_updates();
            std::thread::sleep(Duration::from_millis(5));
        }
        // Past the throttle: held while pressed, rebuilt once released.
        cache.built_at = Some(Instant::now() - REBUILD_INTERVAL * 2);
        assert!(cache.refresh(&engine, 200, false, true).is_some());
        assert_eq!(cache.columns, before, "the drag keeps its columns");
        assert!(cache.refresh(&engine, 200, false, false).is_none());
        assert_ne!(cache.columns, before);
    }

    #[test]
    fn groups_stack_error_warn_info_debug_unknown() {
        let mut h = TimeHistogram::default();
        for level in [
            LogLevel::Fatal,
            LogLevel::Error,
            LogLevel::Warn,
            LogLevel::Trace,
            LogLevel::Unknown,
        ] {
            h.add(0, level as u8);
        }
        let cols = h.columns(1);
        assert_eq!(group_counts(&cols[0]), [2, 1, 0, 1, 1]);
    }

    #[test]
    fn pointer_maps_to_columns_and_instants_to_positions() {
        assert_eq!(column_at(0.0, 100.0, 10), 0);
        assert_eq!(column_at(55.0, 100.0, 10), 5);
        assert_eq!(column_at(100.0, 100.0, 10), 9);
        assert_eq!(column_at(-5.0, 100.0, 10), 0);
        let mut h = TimeHistogram::default();
        for s in 0..10i64 {
            h.add(s * 1_000, LogLevel::Info as u8);
        }
        let cols = h.columns(10);
        assert_eq!(x_frac_of(0, &cols), 0.0);
        assert!((x_frac_of(5_000, &cols) - 0.5).abs() < 1e-6);
        assert!((x_frac_of(5_500, &cols) - 0.55).abs() < 1e-6);
        assert_eq!(x_frac_of(99_000, &cols), 1.0);
        assert_eq!(span_text(&cols, 2, 3), "1970-01-01 00:00:02 – 00:00:03");
    }
}
