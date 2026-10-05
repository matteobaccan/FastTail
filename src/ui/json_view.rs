// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The tree of an expanded JSON row (`[+] JSON`), drawn under the row as one galley of
//! one line per tree row: a click on an object or array folds it, the context menu of a
//! node copies its value or path and expands or collapses everything below it. Shared by
//! the fixed-height and the wrapped row views.

use crate::i18n::{t, Language};
use crate::json_tree::{Fold, Kind, NoTree, Row, Tree};
use crate::tail_engine::JsonTreeResult;
use crate::theme::CyberTheme;
use egui::text::LayoutJob;
use egui::{Color32, FontId, TextFormat, Ui};
use std::collections::HashSet;
use std::sync::Arc;

/// The laid-out tree of a row.
pub struct JsonBlock {
    pub galley: Arc<egui::Galley>,
    /// What each line of the galley shows: a tree row, or a notice (`None`).
    rows: Vec<Option<Row>>,
}

/// The summary of a container: `{3 keys}`, `[12 items]`.
pub fn summary(tree: &Tree, id: usize, lang: Language) -> String {
    let node = tree.node(id);
    let n = node.children.to_string();
    match node.kind {
        Kind::Object => format!("{{{}}}", t(lang, "json_keys").replace("{n}", &n)),
        _ => format!("[{}]", t(lang, "json_items").replace("{n}", &n)),
    }
}

/// Lays out `tree` under the open nodes `open`, with `font`. `cut` adds the line saying
/// "expand all" stopped early.
pub fn layout(
    ctx: &egui::Context,
    theme: &CyberTheme,
    lang: Language,
    tree: &JsonTreeResult,
    open: &HashSet<usize>,
    font: &FontId,
) -> JsonBlock {
    let mut job = LayoutJob::default();
    let mut rows = Vec::new();
    let format = |color: Color32| TextFormat {
        font_id: font.clone(),
        color,
        ..Default::default()
    };
    let dim: Color32 = theme.text_dim().into();
    let warn: Color32 = theme.warn_color().into();
    let key_color: Color32 = theme.text_primary().into();
    let string_color: Color32 = theme.secondary_accent().into();
    let number_color: Color32 = theme.accent_color().into();
    let line = |job: &mut LayoutJob, parts: &[(String, Color32)]| {
        if !job.text.is_empty() {
            job.append("\n", 0.0, format(dim));
        }
        for (text, color) in parts {
            job.append(text, 0.0, format(*color));
        }
    };
    match tree {
        Err(NoTree::TooLarge) => {
            line(&mut job, &[(t(lang, "json_too_large").to_string(), warn)]);
            rows.push(None);
        }
        Err(NoTree::NotJson) => {}
        Ok(tree) => {
            for row in tree.rows(open) {
                match row {
                    Row::Node { id, depth } => {
                        let node = tree.node(id);
                        let indent = "  ".repeat(depth);
                        let marker = match (node.kind.is_container(), open.contains(&id)) {
                            (false, _) => "  ",
                            (true, true) => "▾ ",
                            (true, false) => "▸ ",
                        };
                        let mut parts = vec![(format!("{indent}{marker}"), dim)];
                        if let Some(key) = tree.key(id) {
                            parts.push((key, key_color));
                            parts.push((": ".into(), dim));
                        }
                        let value = match node.kind {
                            Kind::Object | Kind::Array => (summary(tree, id, lang), dim),
                            Kind::String => (tree.display_value(id), string_color),
                            Kind::Number => (tree.display_value(id), number_color),
                            Kind::Bool | Kind::Null => (tree.display_value(id), warn),
                        };
                        parts.push(value);
                        line(&mut job, &parts);
                    }
                    Row::More { depth, hidden } => {
                        let text = t(lang, "json_more").replace("{n}", &hidden.to_string());
                        line(
                            &mut job,
                            &[(format!("{}  {text}", "  ".repeat(depth)), dim)],
                        );
                    }
                }
                rows.push(Some(row));
            }
            if let Some(at) = tree.error() {
                let text = t(lang, "json_invalid").replace("{n}", &at.to_string());
                line(&mut job, &[(format!("⚠ {text}"), warn)]);
                rows.push(None);
            }
        }
    }
    // One galley line per tree row: no wrapping (long values run past the frame, clipped).
    job.wrap.max_width = f32::INFINITY;
    let galley = ctx.fonts_mut(|f| f.layout_job(job));
    JsonBlock { galley, rows }
}

impl JsonBlock {
    /// The tree row under the y coordinate `y` of a block drawn with its top at `top`.
    fn row_at(&self, top: f32, y: f32) -> Option<Row> {
        let n = self.rows.len().max(1);
        let h = self.galley.size().y / n as f32;
        let i = ((y - top) / h.max(1.0)).floor();
        (i >= 0.0).then(|| self.rows.get(i as usize).copied().flatten())?
    }
}

/// Clicks and the context menu on a block drawn at `rect` for the row of `line`. Copies
/// go to the clipboard at once; a fold is returned for the caller to apply.
pub fn interact(
    ui: &mut Ui,
    rect: egui::Rect,
    block: &JsonBlock,
    tree: &JsonTreeResult,
    line: usize,
    lang: Language,
) -> Option<Fold> {
    let Ok(tree) = tree else {
        return None;
    };
    let id = ui.id().with(("json_tree", line));
    let response = ui.interact(rect, id, egui::Sense::click());
    let node_at = |pos: Option<egui::Pos2>| match pos.and_then(|p| block.row_at(rect.top(), p.y)) {
        Some(Row::Node { id, .. }) => Some(id),
        _ => None,
    };
    let hovered = node_at(response.hover_pos());
    if hovered.is_some_and(|n| tree.node(n).kind.is_container()) {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let mut fold = None;
    if response.clicked() {
        if let Some(node) = node_at(response.interact_pointer_pos()) {
            if tree.node(node).kind.is_container() {
                fold = Some(Fold::Toggle(node));
            }
        }
    }
    if response.secondary_clicked() {
        if let Some(node) = node_at(response.interact_pointer_pos()) {
            ui.data_mut(|d| d.insert_temp(id, node));
        }
    }
    response.context_menu(|ui| {
        let Some(node) = ui.data(|d| d.get_temp::<usize>(id)) else {
            ui.close();
            return;
        };
        if ui.button(t(lang, "json_copy_value")).clicked() {
            ui.ctx().copy_text(tree.value_text(node));
            ui.close();
        }
        if ui.button(t(lang, "json_copy_path")).clicked() {
            ui.ctx().copy_text(tree.path(node));
            ui.close();
        }
        if tree.node(node).kind.is_container() {
            ui.separator();
            if ui.button(t(lang, "json_expand_all")).clicked() {
                fold = Some(Fold::ExpandAll(node));
                ui.close();
            }
            if ui.button(t(lang, "json_collapse_all")).clicked() {
                fold = Some(Fold::CollapseAll(node));
                ui.close();
            }
        }
    });
    fold
}
