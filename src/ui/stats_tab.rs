// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The Statistics tab of a stream (see `crate::field_stats`): the source picker, the most
//! frequent values with bars and their counts per level, the numeric summary, and a line
//! chart per time slice. A click on a value filters on it. Not saved in the dock layout.

use crate::field_stats::{SliceChoice, StatsSource, SLICE_WIDTHS};
use crate::i18n::{t, Language};
use crate::log_level::LogLevel;
use crate::tail_engine::TailEngine;
use crate::theme::CyberTheme;
use crate::ui::dock::FastTailTab;
use egui::{Color32, RichText, Ui};
use egui_dock::{DockState, NodeIndex, NodePath, SurfaceIndex};
use std::path::Path;

/// Values shown in the chart at most.
const CHART_SERIES: usize = 5;
/// Height of the chart.
const CHART_HEIGHT: f32 = 160.0;

/// Brings the Statistics tab of `path` to the front, adding it below the first leaf.
pub fn open_tab(dock: &mut DockState<FastTailTab>, path: &Path) {
    let tab = FastTailTab::Statistics(path.to_path_buf());
    if let Some(at) = dock.find_tab(&tab) {
        let _ = dock.set_active_tab(at);
        dock.set_focused_node_and_surface(at.node_path());
        return;
    }
    if dock.iter_all_tabs().count() == 0 {
        *dock = DockState::new(vec![tab]);
        return;
    }
    let surface = dock.main_surface_mut();
    match surface.iter().position(|n| n.is_leaf()) {
        Some(leaf) => {
            let [_, new] = surface.split_below(NodeIndex(leaf), 0.55, vec![tab]);
            dock.set_focused_node_and_surface(NodePath::new(SurfaceIndex::main(), new));
        }
        None => surface.push_to_first_leaf(tab),
    }
}

/// The tab title: the stream it counts.
pub fn title(engine: Option<&TailEngine>, lang: Language) -> String {
    let name = engine.map(crate::find_all::stream_name).unwrap_or_default();
    format!("📊 {} · {name}", t(lang, "stats_title"))
}

/// A number as the table and the summary show it.
fn number(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{v:.0}")
    } else if v.abs() >= 100.0 {
        format!("{v:.1}")
    } else {
        format!("{v:.3}")
    }
}

/// The colours of the chart series, then the "other values" series.
fn series_colors(theme: &CyberTheme) -> [Color32; CHART_SERIES + 1] {
    [
        theme.accent_color().into(),
        theme.warn_color().into(),
        theme.error_color().into(),
        Color32::from_rgb(120, 200, 120),
        Color32::from_rgb(190, 140, 255),
        theme.text_dim().into(),
    ]
}

fn slice_name(width: i64) -> String {
    match width {
        w if w % 86_400_000 == 0 => format!("{} d", w / 86_400_000),
        w if w % 3_600_000 == 0 => format!("{} h", w / 3_600_000),
        w if w % 60_000 == 0 => format!("{} min", w / 60_000),
        w => format!("{} s", w / 1000),
    }
}

/// Draws the tab of `engine` (`None` once the stream is closed).
pub fn render(ui: &mut Ui, engine: Option<&mut TailEngine>, theme: &CyberTheme, lang: Language) {
    let Some(engine) = engine else {
        ui.label(RichText::new(t(lang, "stats_stream_closed")).color(theme.text_dim()));
        return;
    };
    egui::ScrollArea::vertical()
        .id_salt(("fasttail_stats", engine.path.clone()))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            source_row(ui, engine, theme, lang);
            if engine.stats.is_none() {
                ui.label(RichText::new(t(lang, "stats_pick_source")).color(theme.text_dim()));
                return;
            }
            ui.add_space(4.0);
            summary(ui, engine, theme, lang);
            ui.add_space(6.0);
            chart(ui, engine, theme, lang);
            ui.add_space(6.0);
            table(ui, engine, theme, lang);
        });
    if engine.stats.as_ref().is_some_and(|s| !s.done()) {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(250));
    }
}

/// The source (a field of the parser or a regex), the number of values, the time slices,
/// the progress and Copy as CSV.
fn source_row(ui: &mut Ui, engine: &mut TailEngine, theme: &CyberTheme, lang: Language) {
    let keys: Vec<String> = engine.field_catalogue().keys().to_vec();
    let id = ui.id().with(("stats_regex", engine.path.clone()));
    let mut regex_text: String = ui.data(|d| d.get_temp(id)).unwrap_or_else(|| {
        match engine.stats.as_ref().map(|s| &s.source) {
            Some(StatsSource::Capture(re)) => re.as_str().to_string(),
            _ => String::new(),
        }
    });
    let current = engine.stats.as_ref().map(|s| s.source.clone());
    // The regex field shows once Regex… was picked, or with no field to pick.
    let mode_id = id.with("mode");
    let mut regex_mode: bool = keys.is_empty()
        || matches!(current, Some(StatsSource::Capture(_)))
        || ui.data(|d| d.get_temp(mode_id)).unwrap_or(false);
    let slice_choice = engine
        .stats
        .as_ref()
        .map_or(SliceChoice::Auto, |s| s.slice_choice);
    let mut open: Option<(StatsSource, SliceChoice)> = None;
    ui.horizontal_wrapped(|ui| {
        ui.label(t(lang, "stats_source"));
        let shown = match &current {
            Some(StatsSource::Field(k)) => k.clone(),
            Some(StatsSource::Capture(_)) | None => t(lang, "stats_regex").to_string(),
        };
        egui::ComboBox::from_id_salt(("stats_source", engine.path.clone()))
            .selected_text(RichText::new(shown).monospace())
            .show_ui(ui, |ui| {
                for k in &keys {
                    let on = matches!(&current, Some(StatsSource::Field(c)) if c == k);
                    if ui
                        .selectable_label(on, RichText::new(k).monospace())
                        .clicked()
                    {
                        regex_mode = false;
                        open = Some((StatsSource::Field(k.clone()), slice_choice));
                    }
                }
                if ui
                    .selectable_label(
                        matches!(current, Some(StatsSource::Capture(_))),
                        t(lang, "stats_regex"),
                    )
                    .clicked()
                {
                    regex_mode = true;
                    if let Ok(re) = regex::Regex::new(&regex_text) {
                        if !regex_text.is_empty() {
                            open = Some((StatsSource::Capture(re), slice_choice));
                        }
                    }
                }
            });
        if regex_mode {
            let valid = regex::Regex::new(&regex_text);
            let resp = ui.add(
                egui::TextEdit::singleline(&mut regex_text)
                    .hint_text(r"took (\d+)ms")
                    .desired_width(220.0)
                    .font(egui::TextStyle::Monospace)
                    .text_color(if valid.is_ok() {
                        theme.text_primary().into()
                    } else {
                        theme.error_color().into()
                    }),
            );
            let resp = match &valid {
                Ok(_) => resp.on_hover_text(t(lang, "stats_regex_tip")),
                Err(e) => resp.on_hover_text(e.to_string()),
            };
            if resp.changed() {
                ui.data_mut(|d| d.insert_temp(id, regex_text.clone()));
                if let Ok(re) = valid {
                    if !regex_text.is_empty() {
                        open = Some((StatsSource::Capture(re), slice_choice));
                    }
                }
            }
        }
        ui.separator();
        ui.label(t(lang, "stats_slices"));
        let slice_text = |c: SliceChoice| match c {
            SliceChoice::Off => t(lang, "stats_slices_off").to_string(),
            SliceChoice::Auto => t(lang, "stats_slices_auto").to_string(),
            SliceChoice::Width(w) => slice_name(w),
        };
        egui::ComboBox::from_id_salt(("stats_slices", engine.path.clone()))
            .selected_text(slice_text(slice_choice))
            .show_ui(ui, |ui| {
                let choices = [SliceChoice::Off, SliceChoice::Auto]
                    .into_iter()
                    .chain(SLICE_WIDTHS.iter().map(|w| SliceChoice::Width(*w)));
                for c in choices {
                    if ui
                        .selectable_label(c == slice_choice, slice_text(c))
                        .clicked()
                    {
                        if let Some(src) = current.clone() {
                            open = Some((src, c));
                        }
                    }
                }
            });
        if let Some(s) = engine.stats.as_mut() {
            ui.separator();
            ui.label(t(lang, "stats_top"));
            ui.add(egui::DragValue::new(&mut s.top_n).range(5..=1000));
            if !s.done() {
                ui.label(
                    RichText::new(
                        t(lang, "stats_counting")
                            .replace("{0}", &format!("{:.0}", s.progress * 100.0)),
                    )
                    .color(theme.text_dim()),
                );
            }
            if s.failed {
                ui.label(RichText::new(t(lang, "stats_failed")).color(theme.warn_color()));
            }
            if ui
                .button(t(lang, "stats_copy_csv"))
                .on_hover_text(t(lang, "stats_copy_csv_tip"))
                .clicked()
            {
                let names = level_names(lang);
                let csv = s.result.to_csv(s.top_n, &names);
                ui.ctx().copy_text(csv);
            }
        }
    });
    ui.data_mut(|d| d.insert_temp(mode_id, regex_mode));
    if let Some((source, slices)) = open {
        engine.open_stats(source, slices);
    }
}

fn level_names(lang: Language) -> [&'static str; LogLevel::COUNT] {
    [
        t(lang, "stats_no_level"),
        LogLevel::Trace.name(),
        LogLevel::Debug.name(),
        LogLevel::Info.name(),
        LogLevel::Warn.name(),
        LogLevel::Error.name(),
        LogLevel::Fatal.name(),
    ]
}

/// Lines counted, values, distinct values and the numeric summary.
fn summary(ui: &mut Ui, engine: &TailEngine, theme: &CyberTheme, lang: Language) {
    let Some(s) = engine.stats.as_ref() else {
        return;
    };
    let r = &s.result;
    let distinct = if r.capped {
        format!("≥ {}", crate::ui::dock::group_thousands(r.distinct()))
    } else {
        crate::ui::dock::group_thousands(r.distinct())
    };
    ui.label(
        RichText::new(
            t(lang, "stats_summary")
                .replace("{0}", &crate::ui::dock::group_thousands(r.lines as usize))
                .replace(
                    "{1}",
                    &crate::ui::dock::group_thousands(r.with_value as usize),
                )
                .replace("{2}", &distinct),
        )
        .monospace()
        .color(theme.text_dim()),
    );
    if r.with_value > 1 && r.distinct() as u64 == r.with_value && !r.is_numeric() {
        ui.label(RichText::new(t(lang, "stats_all_unique")).color(theme.warn_color()));
    }
    if r.is_numeric() {
        let h = &r.hist;
        let p = |q: f64| h.percentile(q).map_or_else(|| "-".to_string(), number);
        ui.label(
            RichText::new(format!(
                "min {} · max {} · {} {} · p50 {} · p90 {} · p95 {} · p99 {}",
                number(h.min),
                number(h.max),
                t(lang, "stats_mean"),
                h.mean().map_or_else(|| "-".to_string(), number),
                p(0.5),
                p(0.9),
                p(0.95),
                p(0.99)
            ))
            .monospace()
            .color(theme.text_primary()),
        );
    }
}

/// Counts per time slice of the top values (or p50 / p95 of a numeric source).
fn chart(ui: &mut Ui, engine: &mut TailEngine, theme: &CyberTheme, lang: Language) {
    let Some(s) = engine.stats.as_mut() else {
        return;
    };
    let top = s.top();
    let s = &*s;
    let r = &s.result;
    let Some(slices) = r.slices else {
        if s.slice_choice != SliceChoice::Off && s.done() {
            ui.label(RichText::new(t(lang, "stats_no_time")).color(theme.text_dim()));
        }
        return;
    };
    let colors = series_colors(theme);
    // The series: name, colour, value per slice.
    let mut series: Vec<(String, Color32, Vec<f64>)> = Vec::new();
    if r.is_numeric() {
        for (i, (name, q)) in [("p50", 0.5), ("p95", 0.95)].into_iter().enumerate() {
            let values = (0..slices.count)
                .map(|k| r.slice_percentile(k, q).unwrap_or(f64::NAN))
                .collect();
            series.push((name.to_string(), colors[i], values));
        }
    } else {
        let mut rest: Vec<f64> = r.slice_totals().iter().map(|&n| n as f64).collect();
        for (i, (value, v)) in top.iter().take(CHART_SERIES).enumerate() {
            let values: Vec<f64> = v.series(slices.count).iter().map(|&n| n as f64).collect();
            for (a, b) in rest.iter_mut().zip(&values) {
                *a -= b;
            }
            series.push((value.to_string(), colors[i], values));
        }
        if rest.iter().any(|&n| n > 0.0) {
            series.push((
                t(lang, "stats_other").to_string(),
                colors[CHART_SERIES],
                rest,
            ));
        }
    }
    let max = series
        .iter()
        .flat_map(|(_, _, v)| v.iter().copied())
        .filter(|v| v.is_finite())
        .fold(0.0f64, f64::max);
    // Legend.
    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new(t(lang, "stats_per_slice").replace("{0}", &slice_name(slices.width)))
                .color(theme.text_dim()),
        );
        for (name, color, _) in &series {
            ui.label(RichText::new(format!("━ {name}")).monospace().color(*color));
        }
    });
    let width = ui.available_width().max(100.0);
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(width, CHART_HEIGHT), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_stroke(
        rect,
        2.0,
        egui::Stroke::new(1.0, Color32::from(theme.text_dim()).gamma_multiply(0.4)),
        egui::StrokeKind::Inside,
    );
    if max <= 0.0 || slices.count == 0 {
        return;
    }
    let x_of = |k: usize| {
        if slices.count == 1 {
            rect.center().x
        } else {
            rect.left() + 4.0 + (rect.width() - 8.0) * k as f32 / (slices.count - 1) as f32
        }
    };
    let y_of = |v: f64| rect.bottom() - 4.0 - (rect.height() - 8.0) * (v / max) as f32;
    for (_, color, values) in &series {
        let mut run: Vec<egui::Pos2> = Vec::new();
        for (k, v) in values.iter().enumerate() {
            if v.is_finite() {
                run.push(egui::pos2(x_of(k), y_of(*v)));
            } else if !run.is_empty() {
                painter.add(egui::Shape::line(
                    std::mem::take(&mut run),
                    egui::Stroke::new(1.5, *color),
                ));
            }
        }
        if run.len() == 1 {
            painter.circle_filled(run[0], 2.0, *color);
        } else if !run.is_empty() {
            painter.add(egui::Shape::line(run, egui::Stroke::new(1.5, *color)));
        }
    }
    painter.text(
        rect.left_top() + egui::vec2(4.0, 2.0),
        egui::Align2::LEFT_TOP,
        number(max),
        egui::FontId::monospace(10.0),
        theme.text_dim().into(),
    );
    if let Some(pos) = resp.hover_pos() {
        let k = if slices.count == 1 {
            0
        } else {
            (((pos.x - rect.left() - 4.0) / (rect.width() - 8.0)) * (slices.count - 1) as f32)
                .round()
                .clamp(0.0, (slices.count - 1) as f32) as usize
        };
        painter.vline(
            x_of(k),
            rect.y_range(),
            egui::Stroke::new(1.0, Color32::from(theme.text_dim()).gamma_multiply(0.6)),
        );
        let start = slices.start + slices.width * k as i64;
        let mut tip = crate::timestamp::format_millis(start);
        for (name, _, values) in &series {
            let v = values[k];
            let shown = if v.is_finite() {
                number(v)
            } else {
                "-".to_string()
            };
            tip.push_str(&format!("\n{name}: {shown}"));
        }
        resp.on_hover_text_at_pointer(tip);
    }
}

/// The most frequent values: count, share, a bar and the counts per level; a click
/// filters on a value, `CTRL` + click leaves it out.
fn table(ui: &mut Ui, engine: &mut TailEngine, theme: &CyberTheme, lang: Language) {
    let Some(s) = engine.stats.as_mut() else {
        return;
    };
    let top = s.top();
    let s = &*s;
    let r = &s.result;
    let total = r.with_value.max(1) as f64;
    let max = top.first().map_or(1, |(_, v)| v.count.max(1)) as f32;
    // Only the levels some value has.
    let levels: Vec<usize> = (0..LogLevel::COUNT)
        .filter(|&l| top.iter().any(|(_, v)| v.levels[l] > 0))
        .collect();
    let names = level_names(lang);
    let level_color = |l: usize| -> Color32 {
        match l {
            0 => theme.text_dim().into(),
            l => theme.level_color(LogLevel::ALL[l - 1]).into(),
        }
    };
    let mut clicked: Option<(String, bool)> = None;
    egui::Grid::new(("fasttail_stats_table", engine.path.clone()))
        .striped(true)
        .spacing([12.0, 3.0])
        .show(ui, |ui| {
            ui.label(RichText::new(t(lang, "stats_value")).strong());
            ui.label(RichText::new(t(lang, "stats_count")).strong());
            ui.label(RichText::new(t(lang, "stats_share")).strong());
            ui.label("");
            for &l in &levels {
                ui.label(RichText::new(names[l]).strong().color(level_color(l)));
            }
            ui.end_row();
            for (value, v) in top.iter() {
                let shown = if value.is_empty() { "\"\"" } else { value };
                let resp = ui
                    .add(
                        egui::Label::new(RichText::new(shown).monospace())
                            .truncate()
                            .sense(egui::Sense::click()),
                    )
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .on_hover_text(format!("{value}\n{}", t(lang, "stats_value_tip")));
                if resp.clicked() {
                    let exclude = ui.input(|i| i.modifiers.command);
                    clicked = Some((value.to_string(), !exclude));
                }
                ui.label(
                    RichText::new(crate::ui::dock::group_thousands(v.count as usize)).monospace(),
                );
                ui.label(
                    RichText::new(format!("{:.1}%", v.count as f64 * 100.0 / total)).monospace(),
                );
                let (bar, _) =
                    ui.allocate_exact_size(egui::vec2(140.0, 12.0), egui::Sense::hover());
                let w = bar.width() * v.count as f32 / max;
                ui.painter().rect_filled(
                    egui::Rect::from_min_size(bar.min, egui::vec2(w.max(1.0), bar.height())),
                    1.0,
                    Color32::from(theme.accent_color()).gamma_multiply(0.7),
                );
                for &l in &levels {
                    let n = v.levels[l];
                    let text = if n == 0 {
                        String::new()
                    } else {
                        crate::ui::dock::group_thousands(n as usize)
                    };
                    ui.label(RichText::new(text).monospace().color(level_color(l)));
                }
                ui.end_row();
            }
            let shown: u64 = top.iter().map(|(_, v)| v.count).sum();
            let rest = r.with_value.saturating_sub(shown);
            if rest > 0 {
                ui.label(
                    RichText::new(t(lang, "stats_other"))
                        .italics()
                        .color(theme.text_dim()),
                );
                ui.label(
                    RichText::new(crate::ui::dock::group_thousands(rest as usize)).monospace(),
                );
                ui.label(RichText::new(format!("{:.1}%", rest as f64 * 100.0 / total)).monospace());
                ui.end_row();
            }
        });
    if let Some((value, include)) = clicked {
        let term = crate::field_stats::filter_term(&s.source, &value, engine.filter_is_regex);
        if !engine.add_filter_term(&term, include) {
            engine.view_notice = Some(t(lang, "stats_terms_full").to_string());
        }
    }
}
