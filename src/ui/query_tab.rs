// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The Query tab of a stream (see `crate::query`): the query editor with Run / Stop and
//! the history, the error of a query that does not run, and the result table (sortable
//! by its headers, Copy as CSV, a double-click on a row result shows the line). Not saved
//! in the dock layout.

use crate::i18n::{t, Language};
use crate::tail_engine::TailEngine;
use crate::theme::CyberTheme;
use crate::ui::dock::FastTailTab;
use egui::{Color32, RichText, Ui};
use egui_dock::{DockState, NodeIndex, NodePath, SurfaceIndex};
use std::path::Path;

/// Widest column, in characters.
const MAX_COL_CHARS: usize = 60;

/// Brings the Query tab of `path` to the front, adding it below the first leaf.
pub fn open_tab(dock: &mut DockState<FastTailTab>, path: &Path) {
    let tab = FastTailTab::Query(path.to_path_buf());
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

pub fn title(engine: Option<&TailEngine>, lang: Language) -> String {
    let name = engine.map(crate::find_all::stream_name).unwrap_or_default();
    format!("🧮 {} · {name}", t(lang, "query_title"))
}

/// What the tab asks the app for.
#[derive(Debug, Default)]
pub struct QueryOutput {
    /// A query was run: remember it in the history.
    pub ran: Option<String>,
}

/// Draws the tab of `engine` (`None` once the stream is closed).
pub fn render(
    ui: &mut Ui,
    engine: Option<&mut TailEngine>,
    history: &[String],
    theme: &CyberTheme,
    lang: Language,
) -> QueryOutput {
    let mut out = QueryOutput::default();
    let Some(engine) = engine else {
        ui.label(RichText::new(t(lang, "stats_stream_closed")).color(theme.text_dim()));
        return out;
    };
    let id = egui::Id::new("fasttail_query").with(&engine.path);
    let mut text: String = ui
        .data(|d| d.get_temp(id))
        .or_else(|| engine.query.as_ref().map(|q| q.text.clone()))
        .unwrap_or_default();
    let running = engine.query.as_ref().is_some_and(|q| q.running());
    let edit = ui.add(
        egui::TextEdit::multiline(&mut text)
            .font(egui::TextStyle::Monospace)
            .desired_rows(3)
            .desired_width(f32::INFINITY)
            .hint_text(t(lang, "query_hint")),
    );
    if edit.changed() {
        ui.data_mut(|d| d.insert_temp(id, text.clone()));
    }
    let ctrl_enter = edit.has_focus()
        && ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter));
    let mut run = ctrl_enter && !running;
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(
                !running,
                egui::Button::new(format!("▶ {}", t(lang, "query_run"))),
            )
            .on_hover_text(t(lang, "query_run_tip"))
            .clicked()
        {
            run = true;
        }
        if ui
            .add_enabled(
                running,
                egui::Button::new(format!("■ {}", t(lang, "query_stop"))),
            )
            .clicked()
        {
            engine.stop_query();
        }
        egui::ComboBox::from_id_salt(id.with("history"))
            .selected_text(format!("🕒 {}", t(lang, "query_history")))
            .width(160.0)
            .show_ui(ui, |ui| {
                if history.is_empty() {
                    ui.label(RichText::new("—").color(theme.text_dim()));
                }
                for h in history {
                    if ui
                        .selectable_label(false, RichText::new(h).monospace())
                        .clicked()
                    {
                        text = h.clone();
                        ui.data_mut(|d| d.insert_temp(id, text.clone()));
                    }
                }
            });
        let table = engine
            .query
            .as_ref()
            .and_then(|q| q.result.as_ref())
            .and_then(|r| r.as_ref().ok());
        if ui
            .add_enabled(
                table.is_some(),
                egui::Button::new(t(lang, "query_copy_csv")),
            )
            .on_disabled_hover_text(t(lang, "query_none"))
            .clicked()
        {
            if let Some(table) = table {
                ui.ctx().copy_text(table.to_csv());
            }
        }
        if let Some(q) = engine.query.as_ref().filter(|q| q.running()) {
            ui.label(
                RichText::new(
                    t(lang, "query_running").replace("{0}", &format!("{:.0}", q.progress * 100.0)),
                )
                .color(theme.text_dim()),
            );
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(200));
        }
    });
    if run && !text.trim().is_empty() {
        engine.run_query(&text);
        out.ran = Some(text.clone());
    }
    ui.collapsing(t(lang, "query_syntax"), |ui| {
        ui.label(
            RichText::new(t(lang, "query_syntax_help"))
                .monospace()
                .size(11.5),
        );
    });
    ui.separator();
    result(ui, engine, theme, lang, id);
    out
}

/// The error, or the summary and the table.
fn result(ui: &mut Ui, engine: &mut TailEngine, theme: &CyberTheme, lang: Language, id: egui::Id) {
    let Some(q) = engine.query.as_mut() else {
        ui.label(RichText::new(t(lang, "query_none")).color(theme.text_dim()));
        return;
    };
    let generation = q.generation;
    // Error positions are bytes of the query; shown as characters.
    let char_at = |pos: usize| {
        q.text
            .get(..pos.min(q.text.len()))
            .map_or(pos, |s| s.chars().count())
    };
    let error_char = q
        .result
        .as_ref()
        .and_then(|r| r.as_ref().err())
        .map(|e| char_at(e.pos));
    let Some(res) = q.result.as_mut() else {
        return;
    };
    let table = match res {
        Err(e) => {
            let msg = t(lang, e.key);
            let text = t(lang, "query_error")
                .replace("{0}", &e.stage.to_string())
                .replace("{1}", &(error_char.unwrap_or(e.pos) + 1).to_string())
                .replace("{2}", msg);
            let text = if e.detail.is_empty() {
                text
            } else {
                format!("{text}: {}", e.detail)
            };
            ui.label(RichText::new(format!("⚠ {text}")).color(theme.warn_color()));
            return;
        }
        Ok(table) => table,
    };
    let mut info = t(lang, "query_summary")
        .replace("{0}", &crate::ui::dock::group_thousands(table.rows.len()))
        .replace(
            "{1}",
            &crate::ui::dock::group_thousands(table.lines_read as usize),
        );
    if table.truncated {
        info.push_str(" · ");
        info.push_str(t(lang, "query_truncated"));
    }
    if table.skipped_values > 0 {
        info.push_str(" · ");
        info.push_str(&t(lang, "query_skipped").replace("{0}", &table.skipped_values.to_string()));
    }
    ui.label(RichText::new(info).monospace().color(theme.text_dim()))
        .on_hover_text(t(lang, "query_snapshot"));
    if table.columns.is_empty() {
        return;
    }
    // Column widths from the header and the first rows, in characters.
    let font = egui::FontId::monospace(12.5);
    let char_w = ui.fonts_mut(|f| f.glyph_width(&font, '0')).max(1.0);
    let widths: Vec<f32> = (0..table.columns.len())
        .map(|c| {
            let chars = table
                .rows
                .iter()
                .take(200)
                .map(|r| r[c].chars().count())
                .chain(std::iter::once(table.columns[c].chars().count() + 2))
                .max()
                .unwrap_or(4)
                .clamp(4, MAX_COL_CHARS);
            chars as f32 * char_w + 12.0
        })
        .collect();
    let sort_id = id.with(("sort", generation));
    let mut sort_click: Option<usize> = None;
    let row_h = ui.text_style_height(&egui::TextStyle::Monospace) + 4.0;
    egui::ScrollArea::horizontal()
        .id_salt(id.with("h"))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                for (c, name) in table.columns.iter().enumerate() {
                    let sorted: Option<(usize, bool)> = ui.data(|d| d.get_temp(sort_id));
                    let arrow = match sorted {
                        Some((s, true)) if s == c => " ▼",
                        Some((s, false)) if s == c => " ▲",
                        _ => "",
                    };
                    let b = egui::Button::new(
                        RichText::new(format!("{name}{arrow}")).monospace().strong(),
                    )
                    .frame(false)
                    .min_size(egui::vec2(widths[c], row_h));
                    if ui.add(b).on_hover_text(t(lang, "query_sort_tip")).clicked() {
                        sort_click = Some(c);
                    }
                }
            });
            let jump_lines = table.lines.as_deref();
            let mut jump = None;
            egui::ScrollArea::vertical()
                .id_salt(id.with("v"))
                .auto_shrink([false, false])
                .show_rows(ui, row_h, table.rows.len(), |ui, range| {
                    for r in range {
                        let resp = ui
                            .horizontal(|ui| {
                                for (c, cell) in table.rows[r].iter().enumerate() {
                                    let (rect, _) = ui.allocate_exact_size(
                                        egui::vec2(widths[c], row_h),
                                        egui::Sense::hover(),
                                    );
                                    let shown: String = cell.chars().take(MAX_COL_CHARS).collect();
                                    ui.painter().with_clip_rect(rect).text(
                                        rect.left_center() + egui::vec2(2.0, 0.0),
                                        egui::Align2::LEFT_CENTER,
                                        shown,
                                        font.clone(),
                                        Color32::from(theme.text_primary()),
                                    );
                                }
                            })
                            .response;
                        let resp =
                            ui.interact(resp.rect, id.with(("row", r)), egui::Sense::click());
                        if jump_lines.is_some() {
                            let resp = resp.on_hover_text(t(lang, "query_jump_tip"));
                            if resp.double_clicked() {
                                jump = jump_lines.and_then(|l| l.get(r).copied());
                            }
                        }
                    }
                });
            if let Some(line) = jump {
                engine_jump(ui, line);
            }
        });
    if let Some(c) = sort_click {
        let desc = matches!(
            ui.data(|d| d.get_temp::<(usize, bool)>(sort_id)),
            Some((s, false)) if s == c
        );
        table.sort_by(c, desc);
        ui.data_mut(|d| d.insert_temp(sort_id, (c, desc)));
    }
    if let Some(line) = ui.data_mut(|d| d.remove_temp::<usize>(jump_id())) {
        engine.query_jump = Some(line);
    }
}

fn jump_id() -> egui::Id {
    egui::Id::new("fasttail_query_jump")
}

/// Hands the double-clicked line to the stream (applied after the table is drawn).
fn engine_jump(ui: &Ui, line: usize) {
    ui.data_mut(|d| d.insert_temp(jump_id(), line));
}
