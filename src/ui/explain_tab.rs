// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The Explain tab of a stream (see `crate::spike`): a span of the timeline compared with
//! a baseline, by volume, level, message template and `field=value` pair. A click on a
//! message shows its first line in the span, a click on a field value filters on it.
//! Not saved in the dock layout.

use crate::i18n::{t, Language};
use crate::log_level::LogLevel;
use crate::spike::{Baseline, Explanation, LEVELS};
use crate::tail_engine::{TailEngine, NO_TIMESTAMP};
use crate::theme::CyberTheme;
use crate::ui::dock::FastTailTab;
use egui::{Color32, RichText, Ui};
use egui_dock::{DockState, NodeIndex, NodePath, SurfaceIndex};
use std::path::Path;

/// Brings the Explain tab of `path` to the front, adding it below the first leaf.
pub fn open_tab(dock: &mut DockState<FastTailTab>, path: &Path) {
    let tab = FastTailTab::Explain(path.to_path_buf());
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

/// The tab title: the stream it explains.
pub fn title(engine: Option<&TailEngine>, lang: Language) -> String {
    let name = engine.map(crate::find_all::stream_name).unwrap_or_default();
    format!("🔬 {} · {name}", t(lang, "explain_title"))
}

/// A share as a percentage.
fn pct(n: u64, total: u64) -> String {
    if total == 0 {
        "-".to_string()
    } else {
        format!("{:.1}%", n as f64 * 100.0 / total as f64)
    }
}

/// The ratio column: `×12.3`, or the new mark.
fn ratio_text(ratio: Option<f64>, lang: Language) -> String {
    match ratio {
        Some(r) if r >= 100.0 => format!("×{r:.0}"),
        Some(r) => format!("×{r:.1}"),
        None => t(lang, "explain_new").to_string(),
    }
}

/// What a click in the tab asks for.
enum Action {
    Goto(usize),
    Search(String),
    FieldTerm(String, String),
    SetWindow,
    Recompute(Baseline, bool),
}

/// Draws the tab of `engine` (`None` once the stream is closed).
pub fn render(ui: &mut Ui, engine: Option<&mut TailEngine>, theme: &CyberTheme, lang: Language) {
    let Some(engine) = engine else {
        ui.label(RichText::new(t(lang, "stats_stream_closed")).color(theme.text_dim()));
        return;
    };
    let Some(state) = engine.explain.as_ref() else {
        ui.label(RichText::new(t(lang, "explain_none")).color(theme.text_dim()));
        return;
    };
    let mut action: Option<Action> = None;
    let (a, b) = state.sides.span;
    let clock = |ms: i64| crate::timestamp::format_millis(engine.to_display_clock(ms));
    let mut baseline = state.sides.baseline;
    let mut only = state.only_filtered;
    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new(
                t(lang, "explain_span").replace("{0}", &format!("{} – {}", clock(a), clock(b))),
            )
            .monospace()
            .strong(),
        );
        ui.separator();
        ui.label(t(lang, "explain_baseline"));
        let name = |b: Baseline| match b {
            Baseline::Rest => t(lang, "explain_baseline_rest"),
            Baseline::Preceding => t(lang, "explain_baseline_preceding"),
        };
        egui::ComboBox::from_id_salt(("explain_baseline", engine.path.clone()))
            .selected_text(name(baseline))
            .show_ui(ui, |ui| {
                for b in [Baseline::Rest, Baseline::Preceding] {
                    if ui.selectable_label(b == baseline, name(b)).clicked() && b != baseline {
                        baseline = b;
                        action = Some(Action::Recompute(baseline, only));
                    }
                }
            });
        if ui
            .checkbox(&mut only, t(lang, "explain_only_filtered"))
            .on_hover_text(t(lang, "explain_only_filtered_tip"))
            .changed()
        {
            action = Some(Action::Recompute(baseline, only));
        }
        if ui.button(t(lang, "explain_recompute")).clicked() {
            action = Some(Action::Recompute(baseline, only));
        }
        if ui.button(t(lang, "explain_set_window")).clicked() {
            action = Some(Action::SetWindow);
        }
        if state.running() {
            ui.label(
                RichText::new(
                    t(lang, "stats_counting")
                        .replace("{0}", &format!("{:.0}", state.progress * 100.0)),
                )
                .color(theme.text_dim()),
            );
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(250));
        }
        if state.failed {
            ui.label(RichText::new(t(lang, "stats_failed")).color(theme.warn_color()));
        }
        if state.stale {
            ui.label(RichText::new(t(lang, "explain_stale")).color(theme.warn_color()));
        }
    });
    if let Some(r) = state.result.as_ref() {
        if r.sampled() {
            ui.label(
                RichText::new(
                    t(lang, "explain_sampled").replace("{0}", &r.sides.sample.to_string()),
                )
                .color(theme.text_dim()),
            );
        }
        ui.separator();
        egui::ScrollArea::vertical()
            .id_salt(("fasttail_explain", engine.path.clone()))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                volume(ui, r, theme, lang);
                ui.add_space(6.0);
                levels(ui, r, theme, lang);
                ui.add_space(6.0);
                messages(ui, r, theme, lang, &mut action);
                if !r.fields.is_empty() || !r.dropped.is_empty() {
                    ui.add_space(6.0);
                    fields(ui, r, theme, lang, &mut action);
                }
            });
    }
    match action {
        Some(Action::Goto(line)) => {
            let _ = engine.request_jump(line, lang);
        }
        Some(Action::Search(words)) => {
            engine.set_search_mode(crate::tail_engine::SearchMode::Text, false);
            engine.search_query = words;
            engine.search_edited_at = None;
        }
        Some(Action::FieldTerm(key, value)) => {
            let source = crate::field_stats::StatsSource::Field(key);
            let term = crate::field_stats::filter_term(&source, &value, engine.filter_is_regex);
            if !engine.add_filter_term(&term, true) {
                engine.view_notice = Some(t(lang, "stats_terms_full").to_string());
            }
        }
        Some(Action::SetWindow) => engine.set_time_range(Some(a), Some(b)),
        Some(Action::Recompute(baseline, only)) => engine.recompute_explain(baseline, only),
        None => {}
    }
}

fn section(ui: &mut Ui, key: &str, theme: &CyberTheme, lang: Language) {
    ui.label(
        RichText::new(t(lang, key))
            .strong()
            .color(theme.accent_color()),
    );
}

/// Entries per minute on both sides, and their ratio.
fn volume(ui: &mut Ui, r: &Explanation, theme: &CyberTheme, lang: Language) {
    section(ui, "explain_volume", theme, lang);
    let (a, b) = r.sides.span;
    let span_ms = b - a + 1;
    let base_ms = match r.sides.baseline {
        Baseline::Preceding => span_ms,
        Baseline::Rest => {
            let ends = [r.time[0], r.time[1]];
            let lo = ends
                .iter()
                .map(|t| t.0)
                .filter(|&t| t != NO_TIMESTAMP)
                .min();
            let hi = ends
                .iter()
                .map(|t| t.1)
                .filter(|&t| t != NO_TIMESTAMP)
                .max();
            match (lo, hi) {
                (Some(lo), Some(hi)) => (hi - lo + 1 - span_ms).max(1),
                _ => 1,
            }
        }
    };
    let span_rate = r.per_minute(0, span_ms);
    let base_rate = r.per_minute(1, base_ms);
    let ratio = if base_rate > 0.0 {
        format!("×{:.1}", span_rate / base_rate)
    } else {
        "-".to_string()
    };
    ui.label(
        RichText::new(
            t(lang, "explain_volume_line")
                .replace("{0}", &format!("{span_rate:.1}"))
                .replace("{1}", &format!("{base_rate:.1}"))
                .replace("{2}", &ratio)
                .replace(
                    "{3}",
                    &crate::ui::dock::group_thousands(r.entries[0] as usize),
                ),
        )
        .monospace(),
    );
}

/// Count and share per level on both sides.
fn levels(ui: &mut Ui, r: &Explanation, theme: &CyberTheme, lang: Language) {
    section(ui, "explain_levels", theme, lang);
    let scale = r.sides.sample.max(1);
    let totals = [r.entries[0], r.entries[1] * scale];
    egui::Grid::new(("explain_levels", r.sides.span))
        .striped(true)
        .spacing([14.0, 2.0])
        .show(ui, |ui| {
            ui.label("");
            ui.label(RichText::new(t(lang, "explain_in_span")).strong());
            ui.label("");
            ui.label(RichText::new(t(lang, "explain_in_baseline")).strong());
            ui.label("");
            ui.end_row();
            for l in (0..LEVELS).rev() {
                let (n, m) = (r.levels[0][l], r.levels[1][l] * scale);
                if n == 0 && m == 0 {
                    continue;
                }
                let (name, color): (&str, Color32) = match l {
                    0 => (t(lang, "stats_no_level"), theme.text_dim().into()),
                    l => (
                        LogLevel::ALL[l - 1].name(),
                        theme.level_color(LogLevel::ALL[l - 1]).into(),
                    ),
                };
                ui.label(RichText::new(name).monospace().color(color));
                ui.label(RichText::new(crate::ui::dock::group_thousands(n as usize)).monospace());
                ui.label(RichText::new(pct(n, totals[0])).monospace());
                ui.label(RichText::new(crate::ui::dock::group_thousands(m as usize)).monospace());
                ui.label(
                    RichText::new(pct(m, totals[1]))
                        .monospace()
                        .color(theme.text_dim()),
                );
                ui.end_row();
            }
        });
}

/// The message templates over-represented in the span.
fn messages(
    ui: &mut Ui,
    r: &Explanation,
    theme: &CyberTheme,
    lang: Language,
    action: &mut Option<Action>,
) {
    section(ui, "explain_messages", theme, lang);
    let list = r.messages();
    if list.is_empty() {
        ui.label(RichText::new(t(lang, "explain_nothing")).color(theme.text_dim()));
        return;
    }
    egui::Grid::new(("explain_messages", r.sides.span))
        .striped(true)
        .spacing([14.0, 2.0])
        .show(ui, |ui| {
            ui.label(RichText::new(t(lang, "explain_in_span")).strong());
            ui.label(RichText::new(t(lang, "explain_in_baseline")).strong());
            ui.label(RichText::new(t(lang, "explain_ratio")).strong());
            ui.label(RichText::new(t(lang, "patterns_template")).strong());
            ui.end_row();
            for (item, first) in &list {
                let Some(tpl) = r.drain.template(item.item) else {
                    continue;
                };
                let text = tpl.text();
                ui.label(
                    RichText::new(crate::ui::dock::group_thousands(item.span as usize)).monospace(),
                );
                ui.label(
                    RichText::new(crate::ui::dock::group_thousands(item.base as usize)).monospace(),
                );
                let ratio = RichText::new(ratio_text(item.ratio, lang)).monospace();
                ui.label(if item.ratio.is_none() {
                    ratio.strong().color(theme.warn_color())
                } else {
                    ratio
                });
                let resp = ui
                    .add(
                        egui::Label::new(RichText::new(&text).monospace())
                            .truncate()
                            .sense(egui::Sense::click()),
                    )
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .on_hover_text(format!("{text}\n{}", t(lang, "explain_message_tip")));
                if resp.clicked() && *first != usize::MAX {
                    *action = Some(Action::Goto(*first));
                }
                let words = tpl.fixed_words();
                resp.context_menu(|ui| {
                    if ui
                        .add_enabled(
                            !words.is_empty(),
                            egui::Button::new(t(lang, "explain_search")),
                        )
                        .clicked()
                    {
                        *action = Some(Action::Search(words.clone()));
                        ui.close();
                    }
                    if ui.button(t(lang, "patterns_copy")).clicked() {
                        ui.ctx().copy_text(text.clone());
                        ui.close();
                    }
                });
                ui.end_row();
            }
        });
}

/// The `field=value` pairs over-represented in the span.
fn fields(
    ui: &mut Ui,
    r: &Explanation,
    theme: &CyberTheme,
    lang: Language,
    action: &mut Option<Action>,
) {
    section(ui, "explain_fields", theme, lang);
    let list = r.field_pairs();
    if list.is_empty() {
        ui.label(RichText::new(t(lang, "explain_nothing")).color(theme.text_dim()));
    } else {
        egui::Grid::new(("explain_fields", r.sides.span))
            .striped(true)
            .spacing([14.0, 2.0])
            .show(ui, |ui| {
                ui.label(RichText::new(t(lang, "explain_in_span")).strong());
                ui.label(RichText::new(t(lang, "explain_in_baseline")).strong());
                ui.label(RichText::new(t(lang, "explain_ratio")).strong());
                ui.label(RichText::new(t(lang, "explain_pair")).strong());
                ui.end_row();
                for item in &list {
                    let (k, v) = &item.item;
                    ui.label(
                        RichText::new(crate::ui::dock::group_thousands(item.span as usize))
                            .monospace(),
                    );
                    ui.label(
                        RichText::new(crate::ui::dock::group_thousands(item.base as usize))
                            .monospace(),
                    );
                    let ratio = RichText::new(ratio_text(item.ratio, lang)).monospace();
                    ui.label(if item.ratio.is_none() {
                        ratio.strong().color(theme.warn_color())
                    } else {
                        ratio
                    });
                    if ui
                        .add(
                            egui::Label::new(RichText::new(format!("{k}={v}")).monospace())
                                .truncate()
                                .sense(egui::Sense::click()),
                        )
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text(t(lang, "explain_field_tip"))
                        .clicked()
                    {
                        *action = Some(Action::FieldTerm(k.clone(), v.clone()));
                    }
                    ui.end_row();
                }
            });
    }
    if !r.dropped.is_empty() {
        let names: Vec<&str> = r.dropped.iter().map(|d| &**d).collect();
        ui.label(
            RichText::new(t(lang, "explain_fields_dropped").replace("{0}", &names.join(", ")))
                .color(theme.text_dim()),
        );
    }
}
