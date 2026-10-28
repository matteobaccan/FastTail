// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The Operations tab of a stream (see `crate::operations`): the id and name sources, then
//! one row per operation (id, name, start, duration, lines, worst level) with a Gantt bar
//! on a time axis shared by the rows. The wheel zooms the axis around the pointer, a drag
//! pans it, a double click resets it. A click goes to the operation's first line; its
//! menu filters to it, sets its span as the time window or copies the table. Not saved in
//! the dock layout.

use crate::field_stats::StatsSource;
use crate::i18n::{t, Language};
use crate::log_level::LogLevel;
use crate::operations::{NameSource, OpSort, Operation};
use crate::tail_engine::TailEngine;
use crate::theme::CyberTheme;
use crate::ui::dock::FastTailTab;
use egui::{Color32, RichText, Ui};
use egui_dock::{DockState, NodeIndex, NodePath, SurfaceIndex};
use std::path::Path;

/// Width of the bar column at least.
const MIN_BAR_W: f32 = 160.0;
/// Width of a bar at least (a one-instant operation is a tick this wide).
const MIN_BAR_PX: f32 = 2.0;

/// Brings the Operations tab of `path` to the front, adding it below the first leaf.
pub fn open_tab(dock: &mut DockState<FastTailTab>, path: &Path) {
    let tab = FastTailTab::Operations(path.to_path_buf());
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

/// The tab title: the stream it groups.
pub fn title(engine: Option<&TailEngine>, lang: Language) -> String {
    let name = engine.map(crate::find_all::stream_name).unwrap_or_default();
    format!("⏱ {} · {name}", t(lang, "ops_title"))
}

/// A duration in ms as the table shows it.
pub fn duration_text(ms: i64) -> String {
    match ms {
        ms if ms < 1_000 => format!("{ms} ms"),
        ms if ms < 60_000 => format!("{:.2} s", ms as f64 / 1000.0),
        ms if ms < 3_600_000 => format!("{}m {:02}s", ms / 60_000, ms / 1000 % 60),
        ms => format!("{}h {:02}m", ms / 3_600_000, ms / 60_000 % 60),
    }
}

/// The time of day of `ms` (`HH:MM:SS.mmm`).
fn clock(ms: i64) -> String {
    let full = crate::timestamp::format_millis(ms);
    match full.find('T').or_else(|| full.find(' ')) {
        Some(at) => full[at + 1..].trim_end_matches('Z').to_string(),
        None => full,
    }
}

fn sort_name(sort: OpSort, lang: Language) -> &'static str {
    match sort {
        OpSort::Start => t(lang, "ops_sort_start"),
        OpSort::Duration => t(lang, "ops_sort_duration"),
        OpSort::Lines => t(lang, "ops_sort_lines"),
        OpSort::Level => t(lang, "ops_sort_level"),
    }
}

/// The view's settings kept between frames (not saved).
#[derive(Clone)]
struct View {
    sort: OpSort,
    needle: String,
    min_secs: f64,
    /// The axis shown (ms), `None` for the span of the operations.
    axis: Option<(i64, i64)>,
    /// The regex typed, and whether the regex box shows.
    regex: String,
    regex_mode: bool,
}

/// What a row asked for this frame.
enum RowAction {
    Goto(usize),
    Filter(usize),
    Window(usize),
    CopyId(usize),
    CopyCsv,
}

/// Draws the tab of `engine` (`None` once the stream is closed).
pub fn render(ui: &mut Ui, engine: Option<&mut TailEngine>, theme: &CyberTheme, lang: Language) {
    let Some(engine) = engine else {
        ui.label(RichText::new(t(lang, "stats_stream_closed")).color(theme.text_dim()));
        return;
    };
    let id = ui.id().with(("fasttail_ops", engine.path.clone()));
    let mut view: View = ui.data(|d| d.get_temp(id)).unwrap_or_else(|| View {
        sort: OpSort::Start,
        needle: String::new(),
        min_secs: 0.0,
        axis: None,
        regex: match engine.operations.as_ref().map(|o| &o.source) {
            Some(StatsSource::Capture(re)) => re.as_str().to_string(),
            _ => String::new(),
        },
        regex_mode: false,
    });
    source_row(ui, engine, &mut view, theme, lang);
    if engine.operations.is_none() {
        ui.label(RichText::new(t(lang, "stats_pick_source")).color(theme.text_dim()));
        ui.data_mut(|d| d.insert_temp(id, view));
        return;
    }
    let action = table(ui, engine, &mut view, id, theme, lang);
    ui.data_mut(|d| d.insert_temp(id, view.clone()));
    if let Some(action) = action {
        apply(engine, action, ui, lang);
    }
    if engine.operations.as_ref().is_some_and(|o| !o.done()) {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(250));
    }
}

/// The id source (a field or a regex), the name source, the sort, the narrowing box, the
/// minimum duration and the progress.
fn source_row(
    ui: &mut Ui,
    engine: &mut TailEngine,
    view: &mut View,
    theme: &CyberTheme,
    lang: Language,
) {
    let keys: Vec<String> = engine.field_catalogue().keys().to_vec();
    let current = engine.operations.as_ref().map(|o| o.source.clone());
    let name = engine
        .operations
        .as_ref()
        .map_or(NameSource::None, |o| o.name.clone());
    view.regex_mode |= keys.is_empty() || matches!(current, Some(StatsSource::Capture(_)));
    let mut open: Option<(StatsSource, NameSource)> = None;
    ui.horizontal_wrapped(|ui| {
        ui.label(t(lang, "ops_source"));
        let shown = match &current {
            Some(StatsSource::Field(k)) => k.clone(),
            Some(StatsSource::Capture(_)) | None => t(lang, "stats_regex").to_string(),
        };
        egui::ComboBox::from_id_salt(("ops_source", engine.path.clone()))
            .selected_text(RichText::new(shown).monospace())
            .show_ui(ui, |ui| {
                for k in &keys {
                    let on = matches!(&current, Some(StatsSource::Field(c)) if c == k);
                    if ui
                        .selectable_label(on, RichText::new(k).monospace())
                        .clicked()
                    {
                        view.regex_mode = false;
                        open = Some((StatsSource::Field(k.clone()), name.clone()));
                    }
                }
                if ui
                    .selectable_label(
                        matches!(current, Some(StatsSource::Capture(_))),
                        t(lang, "stats_regex"),
                    )
                    .clicked()
                {
                    view.regex_mode = true;
                    if let Ok(re) = regex::Regex::new(&view.regex) {
                        if !view.regex.is_empty() {
                            open = Some((StatsSource::Capture(re), name.clone()));
                        }
                    }
                }
            });
        if view.regex_mode {
            let valid = regex::Regex::new(&view.regex);
            let resp = ui.add(
                egui::TextEdit::singleline(&mut view.regex)
                    .hint_text(r"req=(\w+)")
                    .desired_width(200.0)
                    .font(egui::TextStyle::Monospace)
                    .text_color(if valid.is_ok() {
                        theme.text_primary().into()
                    } else {
                        theme.error_color().into()
                    }),
            );
            let resp = match &valid {
                Ok(_) => resp.on_hover_text(t(lang, "ops_regex_tip")),
                Err(e) => resp.on_hover_text(e.to_string()),
            };
            if resp.changed() {
                if let Ok(re) = valid {
                    if !view.regex.is_empty() {
                        open = Some((StatsSource::Capture(re), name.clone()));
                    }
                }
            }
        }
        ui.separator();
        ui.label(t(lang, "ops_name"));
        let name_text = |n: &NameSource| match n {
            NameSource::None => t(lang, "ops_name_none").to_string(),
            NameSource::Message => t(lang, "ops_name_message").to_string(),
            NameSource::Field(k) => k.clone(),
        };
        egui::ComboBox::from_id_salt(("ops_name", engine.path.clone()))
            .selected_text(RichText::new(name_text(&name)).monospace())
            .show_ui(ui, |ui| {
                let choices = [NameSource::None, NameSource::Message]
                    .into_iter()
                    .chain(keys.iter().map(|k| NameSource::Field(k.clone())));
                for c in choices {
                    if ui
                        .selectable_label(c == name, RichText::new(name_text(&c)).monospace())
                        .clicked()
                    {
                        if let Some(src) = current.clone() {
                            open = Some((src, c));
                        }
                    }
                }
            });
        if engine.operations.is_none() {
            return;
        }
        ui.separator();
        ui.label(t(lang, "ops_sort"));
        egui::ComboBox::from_id_salt(("ops_sort", engine.path.clone()))
            .selected_text(sort_name(view.sort, lang))
            .show_ui(ui, |ui| {
                for s in OpSort::ALL {
                    ui.selectable_value(&mut view.sort, s, sort_name(s, lang));
                }
            });
        ui.add(
            egui::TextEdit::singleline(&mut view.needle)
                .hint_text(t(lang, "ops_narrow"))
                .desired_width(140.0),
        );
        ui.label(t(lang, "ops_min_duration"));
        ui.add(
            egui::DragValue::new(&mut view.min_secs)
                .range(0.0..=86_400.0)
                .speed(0.1)
                .suffix(" s"),
        );
        if let Some(o) = engine.operations.as_ref() {
            if !o.done() {
                ui.label(
                    RichText::new(
                        t(lang, "stats_counting")
                            .replace("{0}", &format!("{:.0}", o.progress * 100.0)),
                    )
                    .color(theme.text_dim()),
                );
            }
            if o.failed {
                ui.label(RichText::new(t(lang, "stats_failed")).color(theme.warn_color()));
            }
        }
    });
    if let Some((source, name)) = open {
        view.axis = None;
        engine.open_operations(source, name);
    }
}

/// The colour of a worst level.
fn level_color(level: u8, theme: &CyberTheme) -> Color32 {
    match level {
        0 => theme.text_dim().into(),
        l => theme.level_color(LogLevel::ALL[l as usize - 1]).into(),
    }
}

/// The summary line, the header with the axis, and the rows.
fn table(
    ui: &mut Ui,
    engine: &mut TailEngine,
    view: &mut View,
    id: egui::Id,
    theme: &CyberTheme,
    lang: Language,
) -> Option<RowAction> {
    let o = engine.operations.as_ref()?;
    let map = &o.map;
    let rows = crate::operations::listed(
        map,
        &view.needle,
        (view.min_secs * 1000.0) as i64,
        view.sort,
    );
    // Summary and notes.
    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new(
                t(lang, "ops_summary")
                    .replace("{0}", &crate::ui::dock::group_thousands(map.len()))
                    .replace("{1}", &crate::ui::dock::group_thousands(rows.len())),
            )
            .monospace()
            .color(theme.text_dim()),
        );
        if map.untracked() > 0 {
            ui.label(
                RichText::new(
                    t(lang, "ops_untracked")
                        .replace("{0}", &crate::ui::dock::group_thousands(map.untracked()))
                        .replace(
                            "{1}",
                            &crate::ui::dock::group_thousands(crate::operations::MAX_OPERATIONS),
                        ),
                )
                .color(theme.warn_color()),
            );
        }
        let single = map.ops.iter().filter(|o| o.lines == 1).count();
        if map.len() >= 20 && single * 10 >= map.len() * 9 {
            ui.label(RichText::new(t(lang, "ops_mostly_single")).color(theme.warn_color()));
        }
    });
    let span = map.span();
    let (mut a, mut b) = view.axis.or(span).unwrap_or((0, 1));
    if b <= a {
        b = a + 1;
    }
    let font = egui::FontId::monospace(12.5);
    let row_h = ui.text_style_height(&egui::TextStyle::Monospace) + 6.0;
    let char_w = ui.fonts_mut(|f| f.glyph_width(&font, '0')).max(1.0);
    let id_w = char_w * 18.0;
    let name_w = char_w * 28.0;
    let start_w = char_w * 13.0;
    let dur_w = char_w * 10.0;
    let lines_w = char_w * 8.0;
    let level_w = char_w * 7.0;
    let fixed = id_w + name_w + start_w + dur_w + lines_w + level_w;
    let width = ui.available_width();
    let bar_w = (width - fixed - 8.0).max(MIN_BAR_W);
    let window = engine.time_window();
    let dim: Color32 = theme.text_dim().into();
    // Header: column names, then the axis with its ends; the axis takes the wheel and
    // drags (zoom, pan, double click resets).
    let (head, head_resp) = ui.allocate_exact_size(
        egui::vec2(fixed + bar_w, row_h),
        egui::Sense::click_and_drag(),
    );
    let painter = ui.painter_at(head);
    let mut x = head.left();
    for (key, w) in [
        ("ops_id", id_w),
        ("ops_name", name_w),
        ("ops_sort_start", start_w),
        ("ops_sort_duration", dur_w),
        ("ops_sort_lines", lines_w),
        ("ops_level", level_w),
    ] {
        painter.text(
            egui::pos2(x + 2.0, head.center().y),
            egui::Align2::LEFT_CENTER,
            t(lang, key),
            font.clone(),
            theme.text_primary().into(),
        );
        x += w;
    }
    let bar_x = x;
    let bar_range = bar_x..=(bar_x + bar_w);
    if span.is_some() {
        painter.text(
            egui::pos2(bar_x + 2.0, head.center().y),
            egui::Align2::LEFT_CENTER,
            clock(a),
            egui::FontId::monospace(10.5),
            dim,
        );
        painter.text(
            egui::pos2(bar_x + bar_w - 2.0, head.center().y),
            egui::Align2::RIGHT_CENTER,
            clock(b),
            egui::FontId::monospace(10.5),
            dim,
        );
    }
    let ms_per_px = (b - a) as f64 / bar_w as f64;
    let over_bar = |p: egui::Pos2| bar_range.contains(&p.x);
    // Zoom and pan over the header or the rows' bar column.
    let rows_rect = egui::Rect::from_min_max(
        egui::pos2(bar_x, head.bottom()),
        egui::pos2(bar_x + bar_w, ui.max_rect().bottom()),
    );
    let pointer = ui.input(|i| i.pointer.hover_pos());
    let hovering_bars =
        pointer.is_some_and(|p| (head.contains(p) || rows_rect.contains(p)) && over_bar(p));
    if hovering_bars && span.is_some() {
        // Taken before the rows' scroll area sees it: over the bars the wheel zooms.
        let scroll = ui.input_mut(|i| std::mem::take(&mut i.smooth_scroll_delta.y));
        if scroll != 0.0 {
            let p = pointer.unwrap_or_default();
            let at = a as f64 + (p.x - bar_x) as f64 * ms_per_px;
            let factor = (-scroll as f64 * 0.003).exp();
            let na = at - (at - a as f64) * factor;
            let nb = at + (b as f64 - at) * factor;
            if nb - na >= 1.0 {
                a = na as i64;
                b = nb as i64;
                view.axis = Some((a, b));
            }
        }
    }
    if head_resp.dragged() && over_bar(head_resp.interact_pointer_pos().unwrap_or_default()) {
        let dx = head_resp.drag_delta().x as f64 * ms_per_px;
        a -= dx as i64;
        b -= dx as i64;
        view.axis = Some((a, b));
    }
    if head_resp.double_clicked() {
        view.axis = None;
    }
    let head_resp = head_resp.on_hover_text(t(lang, "ops_axis_tip"));
    let _ = head_resp;
    let to_x = |ms: i64| bar_x + ((ms - a) as f64 / ms_per_px) as f32;
    let mut action: Option<RowAction> = None;
    let mut drag_dx = 0.0f32;
    let mut reset = false;
    egui::ScrollArea::vertical()
        .id_salt(id.with("rows"))
        .auto_shrink([false, false])
        .show_rows(ui, row_h, rows.len(), |ui, range| {
            for r in range {
                let i = rows[r];
                let op: &Operation = &map.ops[i];
                let (rect, resp) = ui.allocate_exact_size(
                    egui::vec2(fixed + bar_w, row_h),
                    egui::Sense::click_and_drag(),
                );
                let painter = ui.painter_at(rect);
                if r % 2 == 1 {
                    painter.rect_filled(rect, 0.0, dim.gamma_multiply(0.06));
                }
                let mut x = rect.left();
                let mut text = |s: &str, w: f32, color: Color32| {
                    let galley = painter.layout_no_wrap(s.to_string(), font.clone(), color);
                    let clip = egui::Rect::from_min_size(
                        egui::pos2(x, rect.top()),
                        egui::vec2(w - 4.0, row_h),
                    );
                    painter.with_clip_rect(clip).galley(
                        egui::pos2(x + 2.0, rect.center().y - galley.size().y / 2.0),
                        galley,
                        color,
                    );
                    x += w;
                };
                let primary: Color32 = theme.text_primary().into();
                text(&op.id, id_w, primary);
                text(&op.name, name_w, dim);
                let untimed = t(lang, "ops_untimed");
                text(
                    &if op.timed() {
                        clock(op.start)
                    } else {
                        untimed.to_string()
                    },
                    start_w,
                    dim,
                );
                text(
                    &op.duration().map_or_else(String::new, duration_text),
                    dur_w,
                    primary,
                );
                text(
                    &crate::ui::dock::group_thousands(op.lines as usize),
                    lines_w,
                    primary,
                );
                let level = match op.worst {
                    0 => "",
                    l => LogLevel::ALL[l as usize - 1].name(),
                };
                text(level, level_w, level_color(op.worst, theme));
                // The time window, shaded under the bars.
                if let Some((from, to)) = window {
                    let l = from.map_or(bar_x, to_x).max(bar_x);
                    let rr = to.map_or(bar_x + bar_w, to_x).min(bar_x + bar_w);
                    if rr > l {
                        painter.rect_filled(
                            egui::Rect::from_x_y_ranges(l..=rr, rect.y_range()),
                            0.0,
                            Color32::from(theme.accent_color()).gamma_multiply(0.08),
                        );
                    }
                }
                if op.timed() {
                    let l = to_x(op.start);
                    let rr = to_x(op.end).max(l + MIN_BAR_PX);
                    let bar = egui::Rect::from_x_y_ranges(
                        l.max(bar_x)..=rr.min(bar_x + bar_w),
                        (rect.top() + 3.0)..=(rect.bottom() - 3.0),
                    );
                    if bar.width() > 0.0 {
                        painter.rect_filled(bar, 1.0, level_color(op.worst.max(3), theme));
                    }
                }
                if resp.dragged() && over_bar(resp.interact_pointer_pos().unwrap_or_default()) {
                    drag_dx += resp.drag_delta().x;
                }
                if resp.double_clicked()
                    && over_bar(resp.interact_pointer_pos().unwrap_or_default())
                {
                    reset = true;
                } else if resp.clicked() {
                    action = Some(RowAction::Goto(op.first_line));
                }
                let first = engine.get_line(op.first_line).unwrap_or_default();
                let tip = format!(
                    "{}\n{}",
                    first.chars().take(300).collect::<String>(),
                    t(lang, "ops_row_tip")
                );
                let resp = resp.on_hover_text(tip);
                resp.context_menu(|ui| {
                    if ui.button(t(lang, "patterns_goto_first")).clicked() {
                        action = Some(RowAction::Goto(op.first_line));
                        ui.close();
                    }
                    if ui.button(t(lang, "ops_filter_to")).clicked() {
                        action = Some(RowAction::Filter(i));
                        ui.close();
                    }
                    if ui
                        .add_enabled(op.timed(), egui::Button::new(t(lang, "explain_set_window")))
                        .clicked()
                    {
                        action = Some(RowAction::Window(i));
                        ui.close();
                    }
                    if ui.button(t(lang, "ops_copy_id")).clicked() {
                        action = Some(RowAction::CopyId(i));
                        ui.close();
                    }
                    if ui.button(t(lang, "stats_copy_csv")).clicked() {
                        action = Some(RowAction::CopyCsv);
                        ui.close();
                    }
                });
            }
        });
    if drag_dx != 0.0 {
        let dx = drag_dx as f64 * ms_per_px;
        view.axis = Some((a - dx as i64, b - dx as i64));
    }
    if reset {
        view.axis = None;
    }
    if let Some(RowAction::CopyCsv) = action {
        ui.ctx().copy_text(crate::operations::to_csv(map, &rows));
        return None;
    }
    action
}

/// Applies what a row asked for.
fn apply(engine: &mut TailEngine, action: RowAction, ui: &Ui, lang: Language) {
    let Some(o) = engine.operations.as_ref() else {
        return;
    };
    match action {
        RowAction::Goto(line) => {
            let _ = engine.request_jump(line, lang);
        }
        RowAction::Filter(i) => {
            let Some(op) = o.map.ops.get(i) else {
                return;
            };
            let term = match &o.source {
                // As text, what the regex matched on the first line (`req=7f3a`): the value
                // in the pattern's context.
                StatsSource::Capture(re) if !engine.filter_is_regex => engine
                    .get_line(op.first_line)
                    .and_then(|l| re.find(&l).map(|m| m.as_str().to_string()))
                    .unwrap_or_else(|| op.id.to_string()),
                source => crate::field_stats::filter_term(source, &op.id, engine.filter_is_regex),
            };
            if !engine.add_filter_term(&term, true) {
                engine.view_notice = Some(t(lang, "stats_terms_full").to_string());
            }
        }
        RowAction::Window(i) => {
            if let Some(op) = o.map.ops.get(i).filter(|o| o.timed()) {
                let (from, to) = (op.start, op.end);
                engine.set_time_range(Some(from), Some(to));
            }
        }
        RowAction::CopyId(i) => {
            if let Some(op) = o.map.ops.get(i) {
                ui.ctx().copy_text(op.id.to_string());
            }
        }
        RowAction::CopyCsv => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_read_in_their_unit() {
        assert_eq!(duration_text(250), "250 ms");
        assert_eq!(duration_text(2_900), "2.90 s");
        assert_eq!(duration_text(125_000), "2m 05s");
        assert_eq!(duration_text(7_500_000), "2h 05m");
    }
}
