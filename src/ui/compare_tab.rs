// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The Compare tab: two lines, or two lists of lines, side by side with their
//! differences (see `crate::compare`). Not saved in the dock layout.

use crate::compare::{
    canonical_json, diff_regions, unified, CompareOptions, RegionDiff, RowKind, MAX_REGION_LINES,
};
use crate::i18n::{t, Language};
use crate::tail_engine::TailEngine;
use crate::theme::CyberTheme;
use crate::ui::dock::FastTailTab;
use egui::{Color32, RichText, Ui};
use egui_dock::{DockState, NodeIndex, NodePath, SurfaceIndex};
use std::path::PathBuf;

/// Rows laid out in full (wrapping); above this only the visible rows are.
const WRAPPED_ROWS: usize = 300;

/// One side of a compare: the stream, its lines and their text when they were taken.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompareSide {
    pub name: String,
    pub path: PathBuf,
    pub lines: Vec<usize>,
    pub texts: Vec<String>,
    /// More lines were selected than a side takes.
    pub capped: bool,
}

impl CompareSide {
    /// Lines `lines` of `engine` (the first `MAX_REGION_LINES`), as the view shows them.
    pub fn of(engine: &TailEngine, mut lines: Vec<usize>) -> Self {
        let capped = lines.len() > MAX_REGION_LINES;
        lines.truncate(MAX_REGION_LINES);
        let texts = lines
            .iter()
            .map(|&l| {
                engine
                    .get_line(l)
                    .map(|t| t.into_owned())
                    .unwrap_or_default()
            })
            .collect();
        Self {
            name: crate::find_all::stream_name(engine),
            path: engine.path.clone(),
            lines,
            texts,
            capped,
        }
    }

    /// `app.log:12`, `app.log:12-40` or `app.log (5 lines)`.
    pub fn label(&self) -> String {
        match (self.lines.first(), self.lines.last()) {
            (Some(&a), Some(&b)) if a == b => format!("{}:{}", self.name, a + 1),
            (Some(&a), Some(&b)) if b >= a && b - a + 1 == self.lines.len() => {
                format!("{}:{}-{}", self.name, a + 1, b + 1)
            }
            _ => format!("{} ({} lines)", self.name, self.lines.len()),
        }
    }
}

pub struct CompareView {
    pub left: CompareSide,
    pub right: CompareSide,
    pub opts: CompareOptions,
    /// The diff and the options and texts it was computed on.
    result: Option<(CompareOptions, std::sync::Arc<RegionDiff>)>,
    /// Both sides are one line holding JSON (computed once).
    json_ok: bool,
    left_texts: Vec<String>,
    right_texts: Vec<String>,
    /// The change the arrows / `F7` last moved to, and a row to scroll to.
    current: Option<usize>,
    scroll_to: Option<usize>,
    /// A line double-clicked in the tab: `(stream, line)`, applied by the app.
    pub jump: Option<(PathBuf, usize)>,
    notice: Option<String>,
}

impl CompareView {
    pub fn new(left: CompareSide, right: CompareSide) -> Self {
        Self {
            left,
            right,
            opts: CompareOptions::default(),
            result: None,
            left_texts: Vec::new(),
            right_texts: Vec::new(),
            current: None,
            scroll_to: None,
            jump: None,
            notice: None,
            json_ok: false,
        }
        .with_json_check()
    }

    fn with_json_check(mut self) -> Self {
        self.json_ok = self.left.texts.len() == 1
            && self.right.texts.len() == 1
            && canonical_json(&self.left.texts[0]).is_some()
            && canonical_json(&self.right.texts[0]).is_some();
        self
    }

    /// Both sides are one line holding JSON: the JSON option can apply.
    pub fn json_possible(&self) -> bool {
        self.json_ok
    }

    /// The diff for the current options, computed again when they changed.
    pub fn diff(&mut self) -> std::sync::Arc<RegionDiff> {
        if self
            .result
            .as_ref()
            .is_none_or(|(opts, _)| *opts != self.opts)
        {
            let json = self.opts.json && self.json_possible();
            let (left, right) = if json {
                (
                    canonical_json(&self.left.texts[0]).unwrap_or_default(),
                    canonical_json(&self.right.texts[0]).unwrap_or_default(),
                )
            } else {
                (self.left.texts.clone(), self.right.texts.clone())
            };
            let diff = diff_regions(&left, &right, &self.opts);
            self.left_texts = left;
            self.right_texts = right;
            self.current = None;
            self.result = Some((self.opts, std::sync::Arc::new(diff)));
        }
        self.result.as_ref().expect("computed").1.clone()
    }

    /// Moves to the next (or previous) block of changes, wrapping around.
    pub fn step_change(&mut self, forward: bool) {
        let changes = self.diff().changes.clone();
        if changes.is_empty() {
            return;
        }
        let next = match (self.current, forward) {
            (None, true) => 0,
            (None, false) => changes.len() - 1,
            (Some(i), true) => (i + 1) % changes.len(),
            (Some(i), false) => (i + changes.len() - 1) % changes.len(),
        };
        self.current = Some(next);
        self.scroll_to = Some(changes[next]);
    }

    /// The compare as `diff -u` text (on the texts compared, JSON pretty-printed when on).
    pub fn unified_text(&mut self) -> String {
        self.diff();
        unified(
            &self.left.label(),
            &self.right.label(),
            &self.left_texts,
            &self.right_texts,
        )
    }
}

/// Where the app keeps the label of the marked side, for the row menu (egui temp data).
pub fn mark_id() -> egui::Id {
    egui::Id::new("fasttail_compare_mark")
}

/// Brings the Compare tab to the front, adding it below the first leaf.
pub fn open_tab(dock: &mut DockState<FastTailTab>) {
    let tab = FastTailTab::Compare;
    if let Some(path) = dock.find_tab(&tab) {
        let _ = dock.set_active_tab(path);
        dock.set_focused_node_and_surface(path.node_path());
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

/// Text with the byte ranges in `changed` on a tinted background.
fn highlighted(
    text: &str,
    changed: &[std::ops::Range<usize>],
    font: &egui::FontId,
    color: Color32,
    tint: Color32,
    wrap_width: f32,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = wrap_width;
    let plain = egui::TextFormat {
        font_id: font.clone(),
        color,
        ..Default::default()
    };
    let marked = egui::TextFormat {
        background: tint,
        ..plain.clone()
    };
    let mut at = 0;
    for r in changed {
        let (s, e) = (r.start.min(text.len()), r.end.min(text.len()));
        if s > at {
            job.append(&text[at..s], 0.0, plain.clone());
        }
        if e > s {
            job.append(&text[s..e], 0.0, marked.clone());
        }
        at = at.max(e);
    }
    if at < text.len() {
        job.append(&text[at..], 0.0, plain);
    }
    job
}

/// The body of the Compare tab.
pub fn render(
    ui: &mut Ui,
    view: &mut Option<CompareView>,
    theme: &CyberTheme,
    lang: Language,
    font_size: f32,
) {
    let Some(view) = view.as_mut() else {
        ui.label(
            RichText::new(t(lang, "compare_empty"))
                .monospace()
                .color(theme.text_dim()),
        );
        return;
    };
    let json_possible = view.json_possible();
    let diff = view.diff();
    let has_changes = !diff.changes.is_empty();

    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(t(lang, "compare_ignore")).monospace());
        ui.checkbox(&mut view.opts.timestamp, t(lang, "compare_opt_timestamp"));
        ui.checkbox(&mut view.opts.numbers, t(lang, "compare_opt_numbers"));
        ui.checkbox(&mut view.opts.ids, t(lang, "compare_opt_ids"));
        ui.checkbox(&mut view.opts.whitespace, t(lang, "compare_opt_whitespace"));
        ui.checkbox(&mut view.opts.case, t(lang, "compare_opt_case"));
        ui.separator();
        ui.add_enabled(
            json_possible,
            egui::Checkbox::new(&mut view.opts.json, t(lang, "compare_opt_json")),
        )
        .on_hover_text(t(lang, "compare_opt_json"))
        .on_disabled_hover_text(t(lang, "compare_selected_tip"));
        ui.separator();
        if ui
            .add_enabled(has_changes, egui::Button::new("▲"))
            .on_hover_text(t(lang, "compare_prev"))
            .on_disabled_hover_text(t(lang, "compare_identical"))
            .clicked()
        {
            view.step_change(false);
        }
        if ui
            .add_enabled(has_changes, egui::Button::new("▼"))
            .on_hover_text(t(lang, "compare_next"))
            .on_disabled_hover_text(t(lang, "compare_identical"))
            .clicked()
        {
            view.step_change(true);
        }
        if ui
            .button(format!("📋 {}", t(lang, "compare_copy_unified")))
            .clicked()
        {
            let text = view.unified_text();
            ui.ctx().copy_text(text);
            view.notice = Some(t(lang, "report_copied").to_string());
        }
    });
    // F7 / SHIFT + F7 while the tab is shown.
    if ui.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::F7)) {
        view.step_change(false);
    } else if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F7)) {
        view.step_change(true);
    }
    let summary = if diff.changes.is_empty() {
        t(lang, "compare_identical").to_string()
    } else {
        t(lang, "compare_changes").replace("{n}", &diff.changes.len().to_string())
    };
    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new(summary)
                .monospace()
                .strong()
                .color(theme.accent_color()),
        );
        if diff.coarse {
            ui.label(
                RichText::new(t(lang, "compare_coarse"))
                    .monospace()
                    .size(11.0)
                    .color(theme.warn_color()),
            );
        }
        if view.left.capped || view.right.capped {
            ui.label(
                RichText::new(t(lang, "compare_capped"))
                    .monospace()
                    .size(11.0)
                    .color(theme.warn_color()),
            );
        }
        if let Some(notice) = &view.notice {
            ui.label(RichText::new(notice).monospace().size(11.0));
        }
    });

    let half = (ui.available_width() / 2.0 - 8.0).max(80.0);
    ui.horizontal(|ui| {
        for side in [&view.left, &view.right] {
            ui.add_sized(
                [half, 18.0],
                egui::Label::new(
                    RichText::new(side.label())
                        .monospace()
                        .strong()
                        .color(theme.secondary_accent()),
                )
                .truncate(),
            );
        }
    });
    ui.separator();

    let font = egui::FontId::monospace(font_size);
    let removed_bg = Color32::from_rgba_unmultiplied(220, 60, 60, 40);
    let added_bg = Color32::from_rgba_unmultiplied(60, 200, 90, 40);
    let changed_bg = Color32::from_rgba_unmultiplied(230, 170, 40, 30);
    let token_left = Color32::from_rgba_unmultiplied(220, 60, 60, 110);
    let token_right = Color32::from_rgba_unmultiplied(60, 200, 90, 110);
    let text_color = theme.text_primary();
    let scroll_to = view.scroll_to.take();
    let current_row = view.current.map(|i| diff.changes[i]);
    let mut jump: Option<(PathBuf, usize)> = None;
    // Up to `WRAPPED_ROWS` rows every row is laid out and long lines wrap; above, only
    // the rows on screen are, one text line each (clipped).
    let wrap = diff.rows.len() <= WRAPPED_ROWS;
    let row_height = font_size + 6.0;
    let mut draw_row = |ui: &mut Ui, idx: usize| {
        let row = &diff.rows[idx];
        let bg = match row.kind {
            _ if !wrap && current_row == Some(idx) => {
                theme.accent_color().gamma_multiply(0.25).into()
            }
            RowKind::Equal => Color32::TRANSPARENT,
            RowKind::Changed => changed_bg,
            RowKind::Removed => removed_bg,
            RowKind::Added => added_bg,
        };
        let response = egui::Frame::NONE
            .fill(bg)
            .stroke(if wrap && current_row == Some(idx) {
                egui::Stroke::new(1.0, theme.accent_color())
            } else {
                egui::Stroke::NONE
            })
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    for (is_left, line) in [(true, row.left), (false, row.right)] {
                        let (texts, side) = if is_left {
                            (&view.left_texts, &view.left)
                        } else {
                            (&view.right_texts, &view.right)
                        };
                        ui.allocate_ui(egui::vec2(half, 0.0), |ui| {
                            ui.set_width(half);
                            if !wrap {
                                ui.set_min_height(row_height);
                                ui.set_max_height(row_height);
                            }
                            let Some(line) = line else {
                                return;
                            };
                            let text = texts.get(line).map(String::as_str).unwrap_or("");
                            let ranges = row
                                .words
                                .as_ref()
                                .map(|w| {
                                    if is_left {
                                        w.left.clone()
                                    } else {
                                        w.right.clone()
                                    }
                                })
                                .unwrap_or_default();
                            let tint = if is_left { token_left } else { token_right };
                            let width = if wrap { half } else { f32::INFINITY };
                            let job =
                                highlighted(text, &ranges, &font, text_color.into(), tint, width);
                            let mut label = egui::Label::new(job).sense(egui::Sense::click());
                            if !wrap {
                                label = label.truncate();
                            }
                            let resp = ui.add(label);
                            if resp.double_clicked() {
                                // In JSON mode the rows are not the stream's lines.
                                let source = if side.lines.len() == texts.len() {
                                    side.lines.get(line).copied()
                                } else {
                                    side.lines.first().copied()
                                };
                                if let Some(source) = source {
                                    jump = Some((side.path.clone(), source));
                                }
                            }
                            resp.on_hover_text(t(lang, "compare_jump_tip"));
                        });
                    }
                });
            })
            .response;
        if wrap && scroll_to == Some(idx) {
            response.scroll_to_me(Some(egui::Align::Center));
        }
    };
    if wrap {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                for idx in 0..diff.rows.len() {
                    draw_row(ui, idx);
                }
            });
    } else {
        let mut area = egui::ScrollArea::vertical().auto_shrink([false, false]);
        if let Some(target) = scroll_to {
            let spacing = ui.spacing().item_spacing.y;
            let offset = target as f32 * (row_height + spacing) - ui.available_height() / 2.0;
            area = area.vertical_scroll_offset(offset.max(0.0));
        }
        area.show_rows(ui, row_height, diff.rows.len(), |ui, range| {
            ui.set_width(ui.available_width());
            for idx in range {
                draw_row(ui, idx);
            }
        });
    }
    if jump.is_some() {
        view.jump = jump;
    }
}
