// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The Patterns tab of a stream (see `crate::patterns`): its entries grouped by template,
//! with count, share, a sparkline of the volume, first and last seen. A click filters the
//! stream to a pattern, `CTRL + click` adds or removes one, the row menu hides one. Not
//! saved in the dock layout.

use crate::i18n::{t, Language};
use crate::patterns::{PatternStat, BUCKETS, WILDCARD};
use crate::tail_engine::{TailEngine, NO_TIMESTAMP};
use crate::theme::CyberTheme;
use crate::ui::dock::FastTailTab;
use egui::{Color32, RichText, Ui};
use egui_dock::{DockState, NodeIndex, NodePath, SurfaceIndex};
use std::path::Path;
use std::sync::Arc;

/// Width of the sparkline column.
const SPARK_W: f32 = 96.0;

/// Brings the Patterns tab of `path` to the front, adding it below the first leaf.
pub fn open_tab(dock: &mut DockState<FastTailTab>, path: &Path) {
    let tab = FastTailTab::Patterns(path.to_path_buf());
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
    format!("⧉ {} · {name}", t(lang, "patterns_title"))
}

/// The column the table is sorted by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortBy {
    Count,
    Template,
    First,
    Last,
}

/// What the tab keeps between frames: the sort, and the template texts of a tree.
#[derive(Clone)]
struct TabState {
    sort: SortBy,
    descending: bool,
    texts: Option<(u64, Arc<Vec<String>>)>,
}

impl Default for TabState {
    fn default() -> Self {
        Self {
            sort: SortBy::Count,
            descending: true,
            texts: None,
        }
    }
}

/// Where an entry was first or last seen: its time when timed, else its line.
fn seen(line: usize, ts: i64) -> String {
    if ts != NO_TIMESTAMP {
        crate::timestamp::format_millis(ts)
    } else {
        format!("#{}", line + 1)
    }
}

/// Draws the tab of `engine` (`None` once the stream is closed).
pub fn render(ui: &mut Ui, engine: Option<&mut TailEngine>, theme: &CyberTheme, lang: Language) {
    let Some(engine) = engine else {
        ui.label(RichText::new(t(lang, "stats_stream_closed")).color(theme.text_dim()));
        return;
    };
    if engine.patterns.is_none() {
        engine.open_patterns();
    }
    let id = egui::Id::new("fasttail_patterns").with(&engine.path);
    let mut state: TabState = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    let narrow_id = id.with("narrow");
    let mut narrow: String = ui.data(|d| d.get_temp(narrow_id)).unwrap_or_default();
    let Some(p) = engine.patterns.as_ref() else {
        return;
    };
    let running = p.running();
    let generation = p.generation;
    // The template texts of the tree shown, made once per change.
    let texts = match (&state.texts, p.snapshot.as_ref().map(|_| p.tree())) {
        (Some((g, texts)), _) if *g == generation => texts.clone(),
        (_, Some(drain)) => {
            let texts: Arc<Vec<String>> = Arc::new(
                (0..drain.len() as u32)
                    .map(|i| drain.template(i).map(|t| t.text()).unwrap_or_default())
                    .collect(),
            );
            state.texts = Some((generation, texts.clone()));
            texts
        }
        _ => Arc::new(Vec::new()),
    };
    // Header: progress, counts, the narrowing box, the filter in place.
    ui.horizontal_wrapped(|ui| {
        if running {
            let key = if p.counting {
                "patterns_counting"
            } else {
                "patterns_learning"
            };
            ui.label(
                RichText::new(t(lang, key).replace("{0}", &format!("{:.0}", p.progress * 100.0)))
                    .color(theme.text_dim()),
            );
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(250));
        } else {
            ui.label(
                RichText::new(
                    t(lang, "patterns_summary")
                        .replace(
                            "{0}",
                            &crate::ui::dock::group_thousands(p.counts.ids().count()),
                        )
                        .replace(
                            "{1}",
                            &crate::ui::dock::group_thousands(p.counts.entries as usize),
                        ),
                )
                .monospace()
                .color(theme.text_dim()),
            );
        }
        if p.failed {
            ui.label(RichText::new(t(lang, "stats_failed")).color(theme.warn_color()));
        }
        ui.separator();
        let resp = ui.add(
            egui::TextEdit::singleline(&mut narrow)
                .hint_text(t(lang, "patterns_narrow"))
                .desired_width(200.0),
        );
        if resp.changed() {
            ui.data_mut(|d| d.insert_temp(narrow_id, narrow.clone()));
        }
        if engine.pattern_filter_dropped {
            ui.label(RichText::new(t(lang, "patterns_filter_dropped")).color(theme.warn_color()));
        }
    });
    ui.separator();
    let Some(p) = engine.patterns.as_ref() else {
        return;
    };
    let total = p.counts.entries.max(1) as f64;
    let needle = narrow.trim().to_lowercase();
    let mut rows: Vec<u32> = p
        .counts
        .ids()
        .filter(|&i| {
            needle.is_empty()
                || texts
                    .get(i as usize)
                    .is_some_and(|t| t.to_lowercase().contains(&needle))
        })
        .collect();
    let stat = |i: u32| &p.counts.per[i as usize];
    let all_wild = |i: u32| p.tree().template(i).is_some_and(|t| t.is_all_wildcards());
    rows.sort_by(|&a, &b| {
        // A template of wildcards only says little: always last.
        let wild = all_wild(a).cmp(&all_wild(b));
        let by = match state.sort {
            SortBy::Count => stat(a).count.cmp(&stat(b).count),
            SortBy::Template => texts.get(a as usize).cmp(&texts.get(b as usize)),
            SortBy::First => stat(a).first_line.cmp(&stat(b).first_line),
            SortBy::Last => stat(a).last_line.cmp(&stat(b).last_line),
        };
        let by = if state.descending { by.reverse() } else { by };
        wild.then(by).then(a.cmp(&b))
    });
    let filter = engine.pattern_filter();
    let included: Vec<u32> = filter.map(|f| f.include.clone()).unwrap_or_default();
    let excluded: Vec<u32> = filter.map(|f| f.exclude.clone()).unwrap_or_default();
    let font = egui::FontId::monospace(12.5);
    let row_h = ui.text_style_height(&egui::TextStyle::Monospace) + 6.0;
    let char_w = ui.fonts_mut(|f| f.glyph_width(&font, '0')).max(1.0);
    let count_w = char_w * 10.0;
    let share_w = char_w * 7.0;
    let seen_w = char_w * 24.0;
    // Header buttons sort; a second click reverses.
    let mut sort_click: Option<SortBy> = None;
    ui.horizontal(|ui| {
        let mut head = |ui: &mut Ui, label: &str, w: f32, by: Option<SortBy>| {
            let arrow = match by {
                Some(b) if b == state.sort => {
                    if state.descending {
                        " ▼"
                    } else {
                        " ▲"
                    }
                }
                _ => "",
            };
            let b = egui::Button::new(RichText::new(format!("{label}{arrow}")).strong())
                .frame(false)
                .min_size(egui::vec2(w, row_h));
            let resp = ui.add_sized([w, row_h], b);
            if let Some(by) = by {
                if resp.on_hover_text(t(lang, "query_sort_tip")).clicked() {
                    sort_click = Some(by);
                }
            }
        };
        head(ui, t(lang, "stats_count"), count_w, Some(SortBy::Count));
        head(ui, t(lang, "stats_share"), share_w, None);
        head(ui, t(lang, "patterns_volume"), SPARK_W, None);
        head(ui, t(lang, "patterns_first"), seen_w, Some(SortBy::First));
        head(ui, t(lang, "patterns_last"), seen_w, Some(SortBy::Last));
        head(
            ui,
            t(lang, "patterns_template"),
            200.0,
            Some(SortBy::Template),
        );
    });
    let mut action: Option<RowAction> = None;
    let other = p.counts.other;
    egui::ScrollArea::both()
        .id_salt(id.with("rows"))
        .auto_shrink([false, false])
        .show_rows(
            ui,
            row_h,
            rows.len() + usize::from(other > 0),
            |ui, range| {
                for r in range {
                    let Some(&pid) = rows.get(r) else {
                        // The entries in no listed pattern.
                        ui.horizontal(|ui| {
                            cell(
                                ui,
                                count_w,
                                row_h,
                                &crate::ui::dock::group_thousands(other as usize),
                                theme,
                            );
                            cell(
                                ui,
                                share_w,
                                row_h,
                                &format!("{:.1}%", other as f64 * 100.0 / total),
                                theme,
                            );
                            ui.add_space(SPARK_W + 2.0 * seen_w);
                            ui.label(
                                RichText::new(t(lang, "patterns_other"))
                                    .italics()
                                    .color(theme.text_dim()),
                            )
                            .on_hover_text(t(lang, "patterns_other_tip"));
                        });
                        continue;
                    };
                    let s = stat(pid);
                    let on = included.contains(&pid);
                    let hidden = excluded.contains(&pid);
                    let resp = ui
                        .horizontal(|ui| {
                            cell(
                                ui,
                                count_w,
                                row_h,
                                &crate::ui::dock::group_thousands(s.count as usize),
                                theme,
                            );
                            cell(
                                ui,
                                share_w,
                                row_h,
                                &format!("{:.1}%", s.count as f64 * 100.0 / total),
                                theme,
                            );
                            sparkline(ui, s, row_h, theme);
                            cell(ui, seen_w, row_h, &seen(s.first_line, s.first_ts), theme);
                            cell(ui, seen_w, row_h, &seen(s.last_line, s.last_ts), theme);
                            let text = texts.get(pid as usize).map(String::as_str).unwrap_or("");
                            ui.label(template_text(text, theme, hidden));
                        })
                        .response;
                    let resp = ui.interact(resp.rect, id.with(("row", pid)), egui::Sense::click());
                    if on {
                        ui.painter().rect_filled(
                            resp.rect,
                            0.0,
                            Color32::from(theme.accent_color()).gamma_multiply(0.18),
                        );
                    }
                    if resp.clicked() {
                        let toggle = ui.input(|i| i.modifiers.command);
                        action = Some(if toggle {
                            RowAction::Toggle(pid)
                        } else {
                            RowAction::Only(pid)
                        });
                    }
                    let resp = resp.on_hover_text(t(lang, "patterns_row_tip"));
                    resp.context_menu(|ui| {
                        if ui.button(t(lang, "patterns_hide")).clicked() {
                            action = Some(RowAction::Hide(pid));
                            ui.close();
                        }
                        if ui.button(t(lang, "patterns_copy")).clicked() {
                            if let Some(text) = texts.get(pid as usize) {
                                ui.ctx().copy_text(text.clone());
                            }
                            ui.close();
                        }
                        if ui.button(t(lang, "patterns_search")).clicked() {
                            action = Some(RowAction::Search(pid));
                            ui.close();
                        }
                        if ui.button(t(lang, "patterns_goto_first")).clicked() {
                            action = Some(RowAction::Goto(s.first_line));
                            ui.close();
                        }
                    });
                }
            },
        );
    if let Some(by) = sort_click {
        if state.sort == by {
            state.descending = !state.descending;
        } else {
            state.sort = by;
            state.descending = matches!(by, SortBy::Count | SortBy::Last);
        }
    }
    ui.data_mut(|d| d.insert_temp(id, state));
    if let Some(a) = action {
        apply(engine, a, included, excluded, lang);
    }
}

/// What a click on a row asks for.
enum RowAction {
    Only(u32),
    Toggle(u32),
    Hide(u32),
    Search(u32),
    Goto(usize),
}

fn apply(
    engine: &mut TailEngine,
    action: RowAction,
    mut include: Vec<u32>,
    mut exclude: Vec<u32>,
    lang: Language,
) {
    match action {
        RowAction::Only(id) => engine.set_pattern_filter(vec![id], Vec::new()),
        RowAction::Toggle(id) => {
            if let Some(i) = include.iter().position(|&x| x == id) {
                include.remove(i);
            } else {
                include.push(id);
            }
            exclude.retain(|&x| x != id);
            engine.set_pattern_filter(include, exclude);
        }
        RowAction::Hide(id) => {
            include.retain(|&x| x != id);
            if !exclude.contains(&id) {
                exclude.push(id);
            }
            engine.set_pattern_filter(include, exclude);
        }
        RowAction::Search(id) => {
            let words = engine
                .patterns
                .as_ref()
                .and_then(|p| p.tree().template(id))
                .map(|t| t.fixed_words())
                .unwrap_or_default();
            if !words.is_empty() {
                engine.set_search_mode(crate::tail_engine::SearchMode::Text, false);
                engine.search_query = words;
                engine.search_edited_at = None;
            }
        }
        RowAction::Goto(line) => {
            let _ = engine.request_jump(line, lang);
        }
    }
}

/// A right-aligned monospace cell `w` wide.
fn cell(ui: &mut Ui, w: f32, h: f32, text: &str, theme: &CyberTheme) {
    ui.add_sized(
        [w, h],
        egui::Label::new(RichText::new(text).monospace().color(theme.text_primary())),
    );
}

/// The template with its `<*>` dimmed (struck through when the pattern is hidden).
fn template_text(text: &str, theme: &CyberTheme, hidden: bool) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let font = egui::FontId::monospace(12.5);
    let fixed = egui::TextFormat {
        font_id: font.clone(),
        color: if hidden {
            theme.text_dim().into()
        } else {
            theme.text_primary().into()
        },
        strikethrough: if hidden {
            egui::Stroke::new(1.0, Color32::from(theme.text_dim()))
        } else {
            egui::Stroke::NONE
        },
        ..Default::default()
    };
    let wild = egui::TextFormat {
        color: Color32::from(theme.text_dim()).gamma_multiply(0.7),
        ..fixed.clone()
    };
    let mut rest = text;
    while let Some(at) = rest.find(WILDCARD) {
        job.append(&rest[..at], 0.0, fixed.clone());
        job.append(WILDCARD, 0.0, wild.clone());
        rest = &rest[at + WILDCARD.len()..];
    }
    job.append(rest, 0.0, fixed);
    job
}

/// The volume of a pattern over the stream's span.
fn sparkline(ui: &mut Ui, s: &PatternStat, h: f32, theme: &CyberTheme) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(SPARK_W, h), egui::Sense::hover());
    let max = s.buckets.iter().copied().max().unwrap_or(0).max(1) as f32;
    let bar_w = rect.width() / BUCKETS as f32;
    let color = Color32::from(theme.accent_color()).gamma_multiply(0.8);
    let painter = ui.painter_at(rect);
    for (i, &n) in s.buckets.iter().enumerate() {
        if n == 0 {
            continue;
        }
        let bh = ((rect.height() - 4.0) * n as f32 / max).max(1.0);
        let x = rect.left() + i as f32 * bar_w;
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(x, rect.bottom() - 2.0 - bh),
                egui::pos2(x + bar_w.max(1.0), rect.bottom() - 2.0),
            ),
            0.0,
            color,
        );
    }
}
