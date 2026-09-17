use crate::config::push_search_history;
use crate::i18n::{t, Language};
use crate::paths::{paths_equal, paths_equal_fast};
use crate::tail_engine::{HighlightRule, TailEngine};
use crate::theme::CyberTheme;
use egui::{Color32, RichText, ScrollArea, Stroke, Ui, WidgetText};
use egui_dock::TabViewer;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// How long to wait after the last keystroke before rescanning the file for matches.
const SEARCH_DEBOUNCE: Duration = Duration::from_millis(150);

/// Finds the engine backing `path`: exact match first, then a cheap case-insensitive
/// comparison, then a canonicalized one, so tabs restored from an older layout with a
/// differently-cased path still resolve to their engine.
fn find_engine_index(engines: &[TailEngine], path: &Path) -> Option<usize> {
    engines
        .iter()
        .position(|e| e.path == path)
        .or_else(|| engines.iter().position(|e| paths_equal_fast(&e.path, path)))
        .or_else(|| engines.iter().position(|e| paths_equal(&e.path, path)))
}

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FastTailTab {
    LogStream(PathBuf),
    Filters,
    Highlights,
    Settings,
}

pub struct DockContext<'a> {
    pub engines: &'a mut Vec<TailEngine>,
    pub open_files: &'a mut Vec<PathBuf>,
    pub theme: &'a mut CyberTheme,
    pub language: &'a mut Language,
    pub global_rules: &'a mut Vec<HighlightRule>,
    pub screensaver_enabled: &'a mut bool,
    pub screensaver_timeout_mins: &'a mut u32,
    pub telemetry_enabled: &'a mut bool,
    pub sound_enabled: &'a mut bool,
    pub borderless: &'a mut bool,
    pub show_line_numbers: &'a mut bool,
    pub font_size: &'a mut f32,
    pub size_unit: &'a mut crate::tail_engine::SizeUnit,
    pub search_history: &'a mut Vec<String>,
    pub tab_closed: &'a mut bool,
    pub test_screensaver: &'a mut bool,
    /// Stream shown in the focused dock leaf: the only one that handles search shortcuts.
    pub focused_stream: Option<PathBuf>,
}

pub struct FastTailTabViewer<'a> {
    pub ctx: DockContext<'a>,
}

impl<'a> TabViewer for FastTailTabViewer<'a> {
    type Tab = FastTailTab;

    fn id(&mut self, tab: &mut Self::Tab) -> egui::Id {
        egui::Id::new(&*tab)
    }

    fn title(&mut self, tab: &mut Self::Tab) -> WidgetText {
        match tab {
            FastTailTab::LogStream(path) => {
                let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("log");
                if let Some((idx, engine)) =
                    find_engine_index(self.ctx.engines, path).map(|i| (i, &self.ctx.engines[i]))
                {
                    let watch_icon = if engine.is_watching { "▶" } else { "■" };
                    let data_dot = if engine.has_new_data { "●" } else { "○" };
                    let title_text =
                        format!("[#{}] {} {} {}", idx + 1, watch_icon, file_name, data_dot);
                    if engine.has_new_data {
                        WidgetText::from(
                            RichText::new(title_text)
                                .monospace()
                                .strong()
                                .color(self.ctx.theme.warn_color()),
                        )
                    } else if !engine.is_watching {
                        WidgetText::from(
                            RichText::new(title_text)
                                .monospace()
                                .color(self.ctx.theme.warn_color()),
                        )
                    } else {
                        // Normal tabs: do not override text color, allowing egui_dock
                        // to apply dock_style.tab.active.text_color (neon accent) vs inactive (dim)
                        WidgetText::from(RichText::new(title_text).monospace().strong())
                    }
                } else {
                    WidgetText::from(
                        RichText::new(format!(
                            "{} ({})",
                            file_name,
                            t(*self.ctx.language, "closed")
                        ))
                        .monospace(),
                    )
                }
            }
            FastTailTab::Filters => WidgetText::from(
                RichText::new(format!("🔍 {}", t(*self.ctx.language, "filters")))
                    .monospace()
                    .strong(),
            ),
            FastTailTab::Highlights => WidgetText::from(
                RichText::new(format!("⚡ {}", t(*self.ctx.language, "highlight_rules")))
                    .monospace()
                    .strong(),
            ),
            FastTailTab::Settings => WidgetText::from(
                RichText::new(format!("⚙️ {}", t(*self.ctx.language, "settings")))
                    .monospace()
                    .strong(),
            ),
        }
    }

    fn ui(&mut self, ui: &mut Ui, tab: &mut Self::Tab) {
        match tab {
            FastTailTab::LogStream(path) => {
                let mut new_size_unit = None;
                let engine_idx = find_engine_index(self.ctx.engines, path);
                let is_focused = self
                    .ctx
                    .focused_stream
                    .as_deref()
                    .map(|f| paths_equal_fast(f, path))
                    .unwrap_or(false);
                if let Some(engine) = engine_idx.map(|i| &mut self.ctx.engines[i]) {
                    // Mark new data as viewed/cleared
                    engine.has_new_data = false;
                    // Each engine owns its own search query so the find box is per-tab
                    let mut search_query = std::mem::take(&mut engine.search_query);
                    render_log_stream(
                        ui,
                        engine,
                        self.ctx.theme,
                        *self.ctx.language,
                        &mut search_query,
                        self.ctx.search_history,
                        *self.ctx.sound_enabled,
                        self.ctx.show_line_numbers,
                        *self.ctx.font_size,
                        &mut new_size_unit,
                        is_focused,
                    );
                    engine.search_query = search_query;
                } else {
                    ui.label(t(*self.ctx.language, "no_file_open"));
                }
                if let Some(unit) = new_size_unit {
                    *self.ctx.size_unit = unit;
                    for eng in self.ctx.engines.iter_mut() {
                        eng.size_unit = unit;
                    }
                }
            }
            FastTailTab::Filters => {
                render_filters_content(ui, self.ctx.engines, self.ctx.theme, *self.ctx.language);
            }
            FastTailTab::Highlights => {
                render_highlights_content(
                    ui,
                    self.ctx.global_rules,
                    self.ctx.engines,
                    self.ctx.theme,
                    *self.ctx.language,
                );
            }
            FastTailTab::Settings => {
                render_settings_content(
                    ui,
                    self.ctx.theme,
                    self.ctx.language,
                    self.ctx.screensaver_enabled,
                    self.ctx.screensaver_timeout_mins,
                    self.ctx.test_screensaver,
                    self.ctx.telemetry_enabled,
                    self.ctx.sound_enabled,
                    self.ctx.borderless,
                    self.ctx.show_line_numbers,
                    self.ctx.font_size,
                );
            }
        }
    }

    fn on_close(&mut self, tab: &mut Self::Tab) -> egui_dock::tab_viewer::OnCloseResponse {
        match tab {
            FastTailTab::LogStream(path) => {
                self.ctx.open_files.retain(|p| !paths_equal_fast(p, path));
                self.ctx
                    .engines
                    .retain(|e| !paths_equal_fast(&e.path, path));
                *self.ctx.tab_closed = true;
            }
            _ => {
                *self.ctx.tab_closed = true;
            }
        }
        egui_dock::tab_viewer::OnCloseResponse::Close
    }
}

/// Background of the text of the current search hit.
const SEARCH_ACTIVE_BG: Color32 = Color32::from_rgb(0, 255, 230);
/// Background of the text of the other search hits.
const SEARCH_MATCH_BG: Color32 = Color32::from_rgb(255, 230, 0);

/// The marker column shown while a search is active: `▶` on the current hit, `●` on the
/// other hits, and a blank of the same width elsewhere so rows never shift.
fn search_marker_label(
    ui: &mut Ui,
    theme: &CyberTheme,
    font_size: f32,
    matches: bool,
    is_active: bool,
) {
    let (glyph, color) = if is_active {
        ("▶", theme.accent_color())
    } else if matches {
        ("●", theme.warn_color())
    } else {
        ("\u{2007}", theme.text_dim()) // figure space: same advance as a digit
    };
    ui.label(
        RichText::new(format!("{glyph} "))
            .monospace()
            .strong()
            .size(font_size)
            .color(color),
    );
}

/// Tints the whole row of a search hit, drawing behind the widgets laid out in `row_rect`.
fn paint_search_row_background(
    ui: &Ui,
    slot: egui::layers::ShapeIdx,
    row_rect: egui::Rect,
    theme: &CyberTheme,
    matches: bool,
    is_active: bool,
) {
    if !matches && !is_active {
        return;
    }
    let base = if is_active {
        theme.accent_color()
    } else {
        theme.warn_color()
    };
    let alpha = if is_active { 70 } else { 40 };
    let fill = Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), alpha);
    let full = egui::Rect::from_min_max(
        egui::pos2(ui.max_rect().left(), row_rect.top()),
        egui::pos2(
            ui.max_rect().right().max(row_rect.right()),
            row_rect.bottom(),
        ),
    );
    ui.painter()
        .set(slot, egui::Shape::rect_filled(full, 0.0, fill));
}

#[allow(clippy::too_many_arguments)]
fn render_log_stream(
    ui: &mut Ui,
    engine: &mut TailEngine,
    theme: &CyberTheme,
    lang: Language,
    search_query: &mut String,
    search_history: &mut Vec<String>,
    sound_enabled: bool,
    show_line_numbers: &mut bool,
    font_size: f32,
    new_size_unit: &mut Option<crate::tail_engine::SizeUnit>,
    is_focused: bool,
) {
    let font_id = egui::FontId::monospace(font_size);
    let hex_row_height = ui.ctx().fonts_mut(|f| f.row_height(&font_id));
    let row_height = (hex_row_height * 1.25).max(18.0).ceil();
    let viewport_height = ui.available_height().max(200.0);
    let bytes_per_row = engine.hex_columns.max(8);

    // Synchronize search matches with the current query, debounced while the user is typing
    match engine.search_edited_at {
        Some(edited) if edited.elapsed() < SEARCH_DEBOUNCE => {
            ui.ctx()
                .request_repaint_after(SEARCH_DEBOUNCE - edited.elapsed());
        }
        _ => {
            engine.search_edited_at = None;
            engine.update_search(search_query);
        }
    }

    // Helper to scroll to a search target (a line index, or a byte offset in HEX view),
    // centering it in the viewport
    let scroll_to_target = |engine: &mut TailEngine, target: usize| {
        let row_y = if engine.view_mode == crate::tail_engine::ViewMode::Hex {
            Some((target / bytes_per_row) as f32 * hex_row_height)
        } else {
            engine.scroll_to_line = Some(target);
            engine
                .get_visible_row_of_line(target)
                .map(|v_row| v_row as f32 * row_height)
        };
        if let Some(row_y) = row_y {
            engine.requested_scroll_y = Some((row_y - viewport_height / 2.0).max(0.0));
            engine.requested_scroll_x = Some(0.0);
            engine.follow_tail = false;
        }
    };

    // F3 and Shift+F3 shortcuts: only the stream in the focused dock leaf reacts
    let f3_pressed = is_focused && ui.input(|i| i.key_pressed(egui::Key::F3));
    let shift_f3 = f3_pressed && ui.input(|i| i.modifiers.shift);
    let next_f3 = f3_pressed && !ui.input(|i| i.modifiers.shift);

    if next_f3 {
        if let Some(target) = engine.search_next(sound_enabled) {
            scroll_to_target(engine, target);
            push_search_history(search_history, search_query);
            ui.ctx().request_repaint();
        }
    } else if shift_f3 {
        if let Some(target) = engine.search_prev(sound_enabled) {
            scroll_to_target(engine, target);
            push_search_history(search_history, search_query);
            ui.ctx().request_repaint();
        }
    }

    // Stream status bar
    ui.horizontal(|ui| {
        let follow_text = if engine.follow_tail {
            RichText::new("▶ Follow")
                .color(theme.accent_color())
                .monospace()
                .strong()
        } else {
            RichText::new("■ Follow")
                .color(theme.warn_color())
                .monospace()
        };

        if ui
            .button(follow_text)
            .on_hover_text(t(lang, "tip_follow_tail"))
            .clicked()
        {
            engine.follow_tail = !engine.follow_tail;
            ui.ctx().request_repaint();
        }

        ui.separator();

        // Monitor disk reading toggle (no attivo/sospeso text, using ▶ and ■ with color)
        let monitor_text = if engine.is_watching {
            RichText::new("▶ Monitor")
                .color(theme.accent_color())
                .monospace()
                .strong()
        } else {
            RichText::new("■ Monitor")
                .color(theme.warn_color())
                .monospace()
        };
        if ui
            .button(monitor_text)
            .on_hover_text(t(lang, "tip_monitor"))
            .clicked()
        {
            engine.is_watching = !engine.is_watching;
            ui.ctx().request_repaint();
        }

        ui.separator();

        // Line numbers toggle
        let lines_text = if *show_line_numbers {
            RichText::new("# 123")
                .color(theme.accent_color())
                .monospace()
        } else {
            RichText::new("# ---").color(theme.text_dim()).monospace()
        };
        if ui
            .button(lines_text)
            .on_hover_text(t(lang, "show_lines"))
            .clicked()
        {
            *show_line_numbers = !*show_line_numbers;
            ui.ctx().request_repaint();
        }

        ui.separator();

        // Mode Switcher (TXT, HEX, MD)
        let current_mode = engine.view_mode;
        let is_txt = current_mode == crate::tail_engine::ViewMode::Text
            || current_mode == crate::tail_engine::ViewMode::Filtered;
        let is_hex = current_mode == crate::tail_engine::ViewMode::Hex;
        let is_md = current_mode == crate::tail_engine::ViewMode::Markdown;

        let txt_style = if is_txt {
            RichText::new("🔤 TXT")
                .color(theme.accent_color())
                .monospace()
                .strong()
        } else {
            RichText::new("🔤 TXT").color(theme.text_dim()).monospace()
        };
        if ui
            .button(txt_style)
            .on_hover_text(t(lang, "tip_mode_txt"))
            .clicked()
        {
            engine.set_view_mode(crate::tail_engine::ViewMode::Text);
            ui.ctx().request_repaint();
        }

        let hex_style = if is_hex {
            RichText::new("🔢 HEX")
                .color(theme.secondary_accent())
                .monospace()
                .strong()
        } else {
            RichText::new("🔢 HEX").color(theme.text_dim()).monospace()
        };
        if ui
            .button(hex_style)
            .on_hover_text(t(lang, "tip_mode_hex"))
            .clicked()
        {
            engine.set_view_mode(crate::tail_engine::ViewMode::Hex);
            ui.ctx().request_repaint();
        }

        let md_style = if is_md {
            RichText::new("📝 MD")
                .color(theme.warn_color())
                .monospace()
                .strong()
        } else {
            RichText::new("📝 MD").color(theme.text_dim()).monospace()
        };
        if ui
            .button(md_style)
            .on_hover_text(t(lang, "tip_mode_md"))
            .clicked()
        {
            engine.set_view_mode(crate::tail_engine::ViewMode::Markdown);
            ui.ctx().request_repaint();
        }

        // Encoding selector (relevant in Text & Markdown modes)
        if engine.view_mode != crate::tail_engine::ViewMode::Hex {
            let mut curr_enc = engine.encoding;
            egui::ComboBox::from_id_salt(format!("enc_sel_{}", engine.path.display()))
                .selected_text(RichText::new(curr_enc.name()).monospace().size(11.0))
                .width(90.0)
                .show_ui(ui, |ui| {
                    for enc in crate::tail_engine::FileEncoding::all() {
                        if ui
                            .selectable_value(&mut curr_enc, *enc, enc.name())
                            .clicked()
                        {
                            engine.set_encoding(*enc);
                            ui.ctx().request_repaint();
                        }
                    }
                });
        }

        // Hex column count selector (multiples of 8: 8, 16, 24, 32...)
        if engine.view_mode == crate::tail_engine::ViewMode::Hex {
            ui.separator();
            ui.label(
                RichText::new(format!("{}:", t(lang, "hex_columns")))
                    .monospace()
                    .size(11.0),
            );
            if ui
                .button(" -8 ")
                .on_hover_text(t(lang, "hex_cols_dec"))
                .clicked()
                && engine.hex_columns > 8
            {
                engine.hex_columns -= 8;
                ui.ctx().request_repaint();
            }
            ui.label(
                RichText::new(format!("{}", engine.hex_columns))
                    .monospace()
                    .strong(),
            );
            if ui
                .button(" +8 ")
                .on_hover_text(t(lang, "hex_cols_inc"))
                .clicked()
                && engine.hex_columns < 64
            {
                engine.hex_columns += 8;
                ui.ctx().request_repaint();
            }
        }

        ui.separator();

        // Lines count stat (shows filtered count vs total when filtering is active)
        let lines_stat = match engine.view_mode {
            crate::tail_engine::ViewMode::Text | crate::tail_engine::ViewMode::Filtered => {
                if engine.is_filter_active() {
                    format!(
                        "{}: {} / {}",
                        t(lang, "lines"),
                        engine.visible_line_count(),
                        engine.total_lines()
                    )
                } else {
                    format!("{}: {}", t(lang, "lines"), engine.total_lines())
                }
            }
            crate::tail_engine::ViewMode::Hex => {
                format!(
                    "{}: {}",
                    t(lang, "lines"),
                    engine.total_hex_rows(engine.hex_columns)
                )
            }
            crate::tail_engine::ViewMode::Markdown => {
                format!("{}: {}", t(lang, "lines"), engine.total_lines())
            }
        };
        ui.label(
            RichText::new(lines_stat)
                .monospace()
                .color(theme.text_dim()),
        );

        // Clickable File Size toggle (Bytes -> MB -> GB -> Hex -> Bytes)
        let size_str = engine.format_size();
        let size_btn = ui
            .button(
                RichText::new(format!("📦 {}", size_str))
                    .monospace()
                    .color(theme.secondary_accent()),
            )
            .on_hover_text(t(lang, "tip_size_unit"));
        if size_btn.clicked() {
            engine.next_size_unit();
            *new_size_unit = Some(engine.size_unit);
            ui.ctx().request_repaint();
        }

        if engine.throughput_bps > 0.0 {
            ui.separator();
            ui.label(
                RichText::new(format!(
                    "{}: {:.1} KB/s",
                    t(lang, "throughput"),
                    engine.throughput_bps / 1024.0
                ))
                .monospace()
                .color(theme.secondary_accent()),
            );
        }

        ui.separator();
        ui.label(RichText::new("🔍").monospace());
        let search_id = egui::Id::new("log_search_input").with(&engine.path);
        let has_query = !search_query.trim().is_empty();
        let match_count = engine.active_match_count();

        let extra_controls_w = if has_query { 220.0 } else { 70.0 };
        let box_w = (ui.available_width() - extra_controls_w).clamp(160.0, 360.0);
        let search_edit = egui::TextEdit::singleline(search_query)
            .hint_text(t(lang, "search_placeholder"))
            .desired_width(box_w)
            .id(search_id);
        let search_resp = ui.add(search_edit).on_hover_text(t(lang, "tip_search_box"));
        if search_resp.changed() {
            engine.search_edited_at = Some(Instant::now());
        }

        // Ctrl+F in the focused stream: requests focus on its search input and selects all text
        let ctrl_f = egui::KeyboardShortcut::new(egui::Modifiers::CTRL, egui::Key::F);
        if is_focused && ui.input_mut(|i| i.consume_shortcut(&ctrl_f)) {
            ui.ctx().memory_mut(|m| m.request_focus(search_id));
            let mut state =
                egui::text_edit::TextEditState::load(ui.ctx(), search_id).unwrap_or_default();
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::two(
                    egui::text::CCursor::new(0),
                    egui::text::CCursor::new(search_query.chars().count()),
                )));
            state.store(ui.ctx(), search_id);
            ui.ctx().request_repaint();
        }

        // Escape inside the search input box: releases focus and restores log navigation
        if search_resp.has_focus()
            && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            ui.ctx().memory_mut(|m| m.stop_text_input());
            ui.ctx().request_repaint();
        }

        // ArrowUp / ArrowDown / PageUp / PageDown inside search input box for seamless navigation
        if search_resp.has_focus() {
            if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown)) {
                if let Some(target) = engine.search_next(sound_enabled) {
                    scroll_to_target(engine, target);
                    push_search_history(search_history, search_query);
                } else {
                    engine.follow_tail = false;
                    engine.requested_scroll_y = Some(engine.current_scroll_y + row_height);
                }
                ui.ctx().request_repaint();
            }
            if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp)) {
                if let Some(target) = engine.search_prev(sound_enabled) {
                    scroll_to_target(engine, target);
                    push_search_history(search_history, search_query);
                } else {
                    engine.follow_tail = false;
                    engine.requested_scroll_y =
                        Some((engine.current_scroll_y - row_height).max(0.0));
                }
                ui.ctx().request_repaint();
            }
            if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::PageDown)) {
                let visible_lines = ((viewport_height / row_height).floor() as usize).max(1);
                let current_line = (engine.current_scroll_y / row_height).round() as usize;
                let target_line = current_line + visible_lines;
                engine.follow_tail = false;
                engine.requested_scroll_y = Some(target_line as f32 * row_height);
                ui.ctx().request_repaint();
            }
            if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::PageUp)) {
                let visible_lines = ((viewport_height / row_height).floor() as usize).max(1);
                let current_line = (engine.current_scroll_y / row_height).round() as usize;
                let target_line = current_line.saturating_sub(visible_lines);
                engine.follow_tail = false;
                engine.requested_scroll_y = Some(target_line as f32 * row_height);
                ui.ctx().request_repaint();
            }
        }

        // Enter / Shift+Enter inside the search input box
        if search_resp.has_focus() {
            let enter_pressed =
                ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
            let shift_enter_pressed =
                ui.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::Enter));
            if enter_pressed || shift_enter_pressed {
                // Enter must act on the query as typed, even inside the debounce window
                engine.search_edited_at = None;
                engine.update_search(search_query);
                let target_line = if shift_enter_pressed {
                    engine.search_prev(sound_enabled)
                } else {
                    engine.search_next(sound_enabled)
                };
                if let Some(target) = target_line {
                    scroll_to_target(engine, target);
                    push_search_history(search_history, search_query);
                    ui.ctx().request_repaint();
                }
            }
        }

        // Match counter & Next/Prev navigation buttons
        if has_query {
            if match_count == 0 {
                ui.label(
                    RichText::new("[0 / 0]")
                        .monospace()
                        .color(theme.warn_color()),
                );
            } else {
                let current_1based = engine.current_match_idx.map(|i| i + 1).unwrap_or(0);
                ui.label(
                    RichText::new(format!("[{} / {}]", current_1based, match_count))
                        .monospace()
                        .strong()
                        .color(theme.accent_color()),
                );
                // Prev button (Shift+F3)
                if ui
                    .button(RichText::new("▲").monospace())
                    .on_hover_text(t(lang, "search_prev"))
                    .clicked()
                {
                    if let Some(target) = engine.search_prev(sound_enabled) {
                        scroll_to_target(engine, target);
                        push_search_history(search_history, search_query);
                        ui.ctx().request_repaint();
                    }
                }
                // Next button (F3)
                if ui
                    .button(RichText::new("▼").monospace())
                    .on_hover_text(t(lang, "search_next"))
                    .clicked()
                {
                    if let Some(target) = engine.search_next(sound_enabled) {
                        scroll_to_target(engine, target);
                        push_search_history(search_history, search_query);
                        ui.ctx().request_repaint();
                    }
                }
            }
        }

        // Search History dropdown menu (last 10 searches)
        ui.menu_button("🕒", |ui| {
            ui.set_max_width(280.0);
            if search_history.is_empty() {
                ui.label(
                    RichText::new(t(lang, "no_recent_files"))
                        .monospace()
                        .color(theme.text_dim()),
                );
            } else {
                let mut selected = None;
                for query_item in search_history.iter() {
                    if ui.button(RichText::new(query_item).monospace()).clicked() {
                        selected = Some(query_item.clone());
                        ui.close();
                    }
                }
                if let Some(chosen) = selected {
                    *search_query = chosen;
                    engine.update_search(search_query);
                    if let Some(target) = engine.current_search_line() {
                        scroll_to_target(engine, target);
                    }
                    ui.ctx().request_repaint();
                }
            }
        })
        .response
        .on_hover_text(t(lang, "search_history"));

        if !search_query.is_empty()
            && ui
                .button("✖")
                .on_hover_text(t(lang, "clear_search"))
                .clicked()
        {
            search_query.clear();
            engine.update_search(search_query);
            ui.ctx().memory_mut(|m| m.request_focus(search_id));
            ui.ctx().request_repaint();
        }

        // Keyboard navigation shortcuts for the focused stream when no input has the keyboard
        if is_focused && !ui.ctx().egui_wants_keyboard_input() {
            ui.input(|i| {
                // Ctrl + Home: Jump to top
                if i.modifiers.ctrl && i.key_pressed(egui::Key::Home) {
                    engine.follow_tail = false;
                    engine.requested_scroll_y = Some(0.0);
                }
                // Ctrl + End: Jump to bottom & follow
                if i.modifiers.ctrl && i.key_pressed(egui::Key::End) {
                    engine.follow_tail = true;
                    let max_y = (engine.visible_line_count() as f32 * row_height).max(0.0);
                    engine.requested_scroll_y = Some(max_y);
                }
                // Home (horizontal start)
                if !i.modifiers.ctrl && i.key_pressed(egui::Key::Home) {
                    engine.requested_scroll_x = Some(0.0);
                }
                // End (horizontal end)
                if !i.modifiers.ctrl && i.key_pressed(egui::Key::End) {
                    let target_x =
                        (engine.max_detected_width - ui.available_width() + 100.0).max(0.0);
                    engine.requested_scroll_x = Some(target_x);
                }
                // Arrow navigation
                // Arrow navigation (Ctrl + Left/Right moves by 5x)
                let h_step = if i.modifiers.ctrl { 200.0 } else { 40.0 };
                if i.key_pressed(egui::Key::ArrowUp) {
                    engine.follow_tail = false;
                    engine.requested_scroll_y =
                        Some((engine.current_scroll_y - row_height).max(0.0));
                }
                if i.key_pressed(egui::Key::ArrowDown) {
                    engine.follow_tail = false;
                    engine.requested_scroll_y = Some(engine.current_scroll_y + row_height);
                }
                if i.key_pressed(egui::Key::ArrowLeft) {
                    engine.requested_scroll_x = Some((engine.current_scroll_x - h_step).max(0.0));
                }
                if i.key_pressed(egui::Key::ArrowRight) {
                    engine.requested_scroll_x = Some(engine.current_scroll_x + h_step);
                }
                // PageUp / PageDown: screen-based paging
                let visible_lines = ((ui.available_height() / row_height).floor() as usize).max(1);
                let current_line = (engine.current_scroll_y / row_height).round() as usize;
                if i.key_pressed(egui::Key::PageUp) {
                    engine.follow_tail = false;
                    let target_line = current_line.saturating_sub(visible_lines);
                    engine.requested_scroll_y = Some(target_line as f32 * row_height);
                }
                if i.key_pressed(egui::Key::PageDown) {
                    engine.follow_tail = false;
                    let target_line = current_line + visible_lines;
                    engine.requested_scroll_y = Some(target_line as f32 * row_height);
                }
            });
        }
    });

    ui.separator();

    // If Hex streaming mode is active, render the binary hex stream
    if engine.view_mode == crate::tail_engine::ViewMode::Hex {
        render_hex_stream(ui, engine, theme, lang, font_size);
        return;
    }

    // Markdown mode renders the formatted document; with an active search it falls back to
    // the source lines so hits get the same marker column and row highlight as text mode.
    let search_active = !engine.last_searched_query.is_empty();
    if engine.view_mode == crate::tail_engine::ViewMode::Markdown {
        if !search_active {
            render_markdown_stream(ui, engine, theme, lang);
            return;
        }
        ui.label(
            RichText::new(format!("📝 {}", t(lang, "md_search_source")))
                .monospace()
                .size(11.0)
                .color(theme.warn_color()),
        );
    }

    // Quick Filter Row directly above log buffer
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("⚡ {}:", t(lang, "filter_include")))
                .monospace()
                .size(11.0)
                .color(theme.accent_color()),
        );
        let mut inc = engine.include_filter.clone();
        if ui
            .add(
                egui::TextEdit::singleline(&mut inc)
                    .hint_text("ERROR|CRITICAL|Exception...")
                    .desired_width(180.0),
            )
            .changed()
        {
            engine.set_include_filter(&inc);
        }
        if !engine.include_filter.is_empty()
            && ui
                .button("✖")
                .on_hover_text(t(lang, "clear_filter"))
                .clicked()
        {
            engine.set_include_filter("");
        }

        ui.separator();

        ui.label(
            RichText::new(format!("🚫 {}:", t(lang, "filter_exclude")))
                .monospace()
                .size(11.0)
                .color(theme.warn_color()),
        );
        let mut exc = engine.exclude_filter.clone();
        if ui
            .add(
                egui::TextEdit::singleline(&mut exc)
                    .hint_text("healthcheck|ping|DEBUG...")
                    .desired_width(180.0),
            )
            .changed()
        {
            engine.set_exclude_filter(&exc);
        }
        if !engine.exclude_filter.is_empty()
            && ui
                .button("✖")
                .on_hover_text(t(lang, "clear_filter"))
                .clicked()
        {
            engine.set_exclude_filter("");
        }

        ui.separator();

        // Match Case (Aa) toggle
        let case_text = if engine.filter_case_sensitive {
            RichText::new("Aa").strong().color(theme.accent_color())
        } else {
            RichText::new("Aa").color(theme.text_dim())
        };
        if ui
            .button(case_text)
            .on_hover_text(t(lang, "case_sensitive_tip"))
            .clicked()
        {
            engine.filter_case_sensitive = !engine.filter_case_sensitive;
            engine.refresh_filters();
        }

        // Regex (.*) toggle
        let regex_text = if engine.filter_is_regex {
            RichText::new(".*").strong().color(theme.accent_color())
        } else {
            RichText::new(".*").color(theme.text_dim())
        };
        if ui
            .button(regex_text)
            .on_hover_text(t(lang, "tip_regex_checkbox"))
            .clicked()
        {
            engine.filter_is_regex = !engine.filter_is_regex;
            engine.refresh_filters();
        }
    });

    ui.separator();

    let visible_lines = engine.visible_line_count();
    if visible_lines == 0 {
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new(t(lang, "no_file_open"))
                    .monospace()
                    .color(theme.text_dim()),
            );
        });
        return;
    }

    let has_search = !engine.last_searched_query.is_empty();
    let active_search_line = engine.current_search_line();

    let mut toggle_json = None;
    let mut clear_scroll_to_line = false;
    let mut scroll_area = ScrollArea::both()
        .auto_shrink([false, false])
        .stick_to_bottom(engine.follow_tail);

    if let Some(x) = engine.requested_scroll_x.take() {
        scroll_area = scroll_area.horizontal_scroll_offset(x);
    }
    if let Some(y) = engine.requested_scroll_y.take() {
        scroll_area = scroll_area.vertical_scroll_offset(y);
    }

    ui.spacing_mut().item_spacing.y = 0.0;
    let scroll_output = scroll_area.show_rows(ui, row_height, visible_lines, |ui, row_range| {
        ui.set_min_width(engine.max_detected_width);
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
        ui.spacing_mut().item_spacing.y = 0.0;
        for row_idx in row_range {
            let actual_line_idx = match engine.get_actual_line_idx(row_idx) {
                Some(idx) => idx,
                None => continue,
            };

            if let Some(raw_line) = engine.get_line(actual_line_idx) {
                let is_json = TailEngine::is_json_line(&raw_line);
                let is_expanded = engine.expanded_json_lines.contains(&actual_line_idx);
                let highlight = engine.match_highlight(&raw_line);
                let matches_search = has_search
                    && engine
                        .search_matches
                        .binary_search(&actual_line_idx)
                        .is_ok();
                let is_active_search = active_search_line == Some(actual_line_idx);

                // Background painted after layout, behind the row (see SearchRowMark)
                let row_bg = ui.painter().add(egui::Shape::Noop);
                let row_resp = ui
                    .horizontal(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.set_min_height(row_height);
                        ui.set_max_height(row_height);

                        // Search marker column: ▶ current hit, ● other hits, blank otherwise
                        if has_search {
                            search_marker_label(
                                ui,
                                theme,
                                font_size,
                                matches_search,
                                is_active_search,
                            );
                        }

                        // Line number
                        if *show_line_numbers {
                            let line_num_str = format!("{:>6} │", actual_line_idx + 1);
                            let num_color = if is_active_search {
                                theme.accent_color()
                            } else {
                                theme.text_dim().gamma_multiply(0.6)
                            };
                            ui.label(
                                RichText::new(line_num_str)
                                    .monospace()
                                    .strong()
                                    .size(font_size)
                                    .color(num_color),
                            );
                        }

                        // JSON toggle button
                        if is_json {
                            let btn_label = if is_expanded { "[-] JSON" } else { "[+] JSON" };
                            let btn = ui.button(
                                RichText::new(btn_label)
                                    .monospace()
                                    .size((font_size - 2.0).max(9.0))
                                    .color(theme.secondary_accent()),
                            );
                            if btn.clicked() {
                                toggle_json = Some((actual_line_idx, is_expanded));
                            }
                        }

                        // Content text
                        let mut text = RichText::new(&*raw_line).monospace().size(font_size);
                        if is_active_search {
                            text = text
                                .color(Color32::BLACK)
                                .background_color(SEARCH_ACTIVE_BG)
                                .strong();
                        } else if matches_search {
                            text = text.color(Color32::BLACK).background_color(SEARCH_MATCH_BG);
                        } else if let Some(hl) = highlight {
                            text = text.color(hl.fg).background_color(hl.bg);
                            if hl.bold {
                                text = text.strong();
                            }
                            if hl.italic {
                                text = text.italics();
                            }
                        } else {
                            text = text.color(theme.text_primary());
                        }
                        ui.add(egui::Label::new(text).wrap_mode(egui::TextWrapMode::Extend));
                    })
                    .response;
                paint_search_row_background(
                    ui,
                    row_bg,
                    row_resp.rect,
                    theme,
                    matches_search,
                    is_active_search,
                );

                if is_active_search && engine.scroll_to_line == Some(actual_line_idx) {
                    row_resp.scroll_to_me(Some(egui::Align::Center));
                    clear_scroll_to_line = true;
                }

                // Render expanded pretty JSON
                if is_json && is_expanded {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw_line) {
                        if let Ok(pretty) = serde_json::to_string_pretty(&val) {
                            egui::Frame::NONE
                                .fill(theme.panel_bg().linear_multiply(1.3))
                                .stroke(Stroke::new(
                                    1.0_f32,
                                    theme.border_color().gamma_multiply(0.4),
                                ))
                                .inner_margin(egui::Margin::same(6))
                                .show(ui, |ui| {
                                    for line in pretty.lines() {
                                        ui.label(
                                            RichText::new(format!("        {}", line))
                                                .monospace()
                                                .size(11.0)
                                                .color(theme.secondary_accent()),
                                        );
                                    }
                                });
                        }
                    }
                }
            }
        }
    });
    if clear_scroll_to_line {
        engine.scroll_to_line = None;
    }
    let measured_width = scroll_output.content_size.x;
    if measured_width > engine.max_detected_width {
        engine.max_detected_width = measured_width;
    }
    engine.current_scroll_x = scroll_output.state.offset.x;
    engine.current_scroll_y = scroll_output.state.offset.y;

    if let Some((idx, was_expanded)) = toggle_json {
        if was_expanded {
            engine.expanded_json_lines.remove(&idx);
        } else {
            engine.expanded_json_lines.insert(idx);
        }
    }
}

fn render_hex_stream(
    ui: &mut Ui,
    engine: &mut TailEngine,
    theme: &CyberTheme,
    lang: Language,
    font_size: f32,
) {
    let has_search = !engine.last_searched_query.is_empty();
    let active_byte = engine.current_search_byte();
    let file_size = engine.file_size as usize;
    if file_size == 0 {
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new(t(lang, "no_file_open"))
                    .monospace()
                    .color(theme.text_dim()),
            );
        });
        return;
    }

    let bytes_per_row = engine.hex_columns.max(8);
    let total_rows = engine.total_hex_rows(bytes_per_row);
    let font_id = egui::FontId::monospace(font_size);
    let row_height = ui.ctx().fonts_mut(|f| f.row_height(&font_id));

    // Dynamic Hex column header
    let offset_header = "OFFSET    ";
    let mut hex_header = String::with_capacity(bytes_per_row * 3 + 8);
    for i in 0..bytes_per_row {
        use std::fmt::Write;
        let _ = write!(&mut hex_header, "{:02X} ", i);
        if (i + 1) % 8 == 0 && (i + 1) < bytes_per_row {
            hex_header.push(' ');
        }
    }
    let mut ascii_header = String::with_capacity(bytes_per_row + 2);
    ascii_header.push('|');
    for _ in 0..bytes_per_row {
        ascii_header.push('.');
    }
    ascii_header.push('|');

    // Ensure max_detected_width accommodates full hex line so horizontal scroll works
    let total_chars = offset_header.len() + hex_header.len() + ascii_header.len();
    let expected_hex_width = (total_chars as f32) * (font_size * 0.65) + 60.0;
    if engine.max_detected_width < expected_hex_width {
        engine.max_detected_width = expected_hex_width;
    }

    let header_height = row_height + 4.0;
    ScrollArea::horizontal()
        .id_salt("hex_header_scroll")
        .auto_shrink([false, true])
        .max_height(header_height)
        .horizontal_scroll_offset(engine.current_scroll_x)
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .show(ui, |ui| {
            ui.set_min_width(engine.max_detected_width);
            ui.horizontal(|ui| {
                if has_search {
                    // Keep the header aligned with the marker column of the rows
                    search_marker_label(ui, theme, font_size, false, false);
                }
                ui.label(
                    RichText::new(offset_header)
                        .monospace()
                        .size(font_size)
                        .color(theme.accent_color())
                        .strong(),
                );
                ui.label(
                    RichText::new(&hex_header)
                        .monospace()
                        .size(font_size)
                        .color(theme.accent_color())
                        .strong(),
                );
                ui.label(
                    RichText::new(&ascii_header)
                        .monospace()
                        .size(font_size)
                        .color(theme.accent_color())
                        .strong(),
                );
            });
        });
    ui.separator();

    let mut scroll_to_byte = engine.scroll_to_byte;
    let mut scroll_area = ScrollArea::both()
        .id_salt("hex_rows_scroll")
        .auto_shrink([false, false])
        .stick_to_bottom(engine.follow_tail);

    if let Some(x) = engine.requested_scroll_x.take() {
        scroll_area = scroll_area.horizontal_scroll_offset(x);
    }
    if let Some(y) = engine.requested_scroll_y.take() {
        scroll_area = scroll_area.vertical_scroll_offset(y);
    }

    let scroll_output = scroll_area.show_rows(ui, row_height, total_rows, |ui, row_range| {
        ui.set_min_width(engine.max_detected_width);
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
        ui.spacing_mut().item_spacing.y = 0.0;
        for row_idx in row_range {
            let offset = row_idx * bytes_per_row;
            let chunk = match engine.get_bytes(offset, bytes_per_row) {
                Some(b) => b,
                None => continue,
            };

            // Search state of this row: any byte hit overlapping it, and whether the current hit is here
            let row_end = offset + chunk.len();
            let matches_search = has_search && engine.hex_row_matches(offset, row_end);
            let is_active_search = matches_search
                && active_byte
                    .map(|(off, len)| off < row_end && off + len > offset)
                    .unwrap_or(false);

            let row_bg = ui.painter().add(egui::Shape::Noop);
            let row_resp = ui
                .horizontal(|ui| {
                    if has_search {
                        search_marker_label(ui, theme, font_size, matches_search, is_active_search);
                    }

                    // Offset label (8 uppercase hex digits)
                    ui.label(
                        RichText::new(format!("{:08X}  ", offset))
                            .monospace()
                            .size(font_size)
                            .color(theme.text_dim().gamma_multiply(0.75)),
                    );

                    // Format hex bytes in groups of 8
                    let mut hex_str = String::with_capacity(bytes_per_row * 3 + 8);
                    for i in 0..bytes_per_row {
                        if i > 0 && i % 8 == 0 {
                            hex_str.push(' ');
                        }
                        if i < chunk.len() {
                            use std::fmt::Write;
                            let _ = write!(&mut hex_str, "{:02X} ", chunk[i]);
                        } else {
                            hex_str.push_str("   ");
                        }
                    }

                    // Format ASCII representation
                    let mut ascii_str = String::with_capacity(bytes_per_row + 4);
                    ascii_str.push('|');
                    for &b in chunk {
                        if (0x20..=0x7E).contains(&b) {
                            ascii_str.push(b as char);
                        } else {
                            ascii_str.push('·');
                        }
                    }
                    for _ in chunk.len()..bytes_per_row {
                        ascii_str.push(' ');
                    }
                    ascii_str.push('|');

                    let mut hex_text = RichText::new(hex_str).monospace().size(font_size);
                    let mut ascii_text = RichText::new(ascii_str).monospace().size(font_size);

                    if is_active_search {
                        hex_text = hex_text
                            .color(Color32::BLACK)
                            .background_color(SEARCH_ACTIVE_BG)
                            .strong();
                        ascii_text = ascii_text
                            .color(Color32::BLACK)
                            .background_color(SEARCH_ACTIVE_BG)
                            .strong();
                    } else if matches_search {
                        hex_text = hex_text
                            .color(Color32::BLACK)
                            .background_color(SEARCH_MATCH_BG);
                        ascii_text = ascii_text
                            .color(Color32::BLACK)
                            .background_color(SEARCH_MATCH_BG);
                    } else {
                        hex_text = hex_text.color(theme.text_primary());
                        ascii_text = ascii_text.color(theme.secondary_accent());
                    }

                    ui.label(hex_text);
                    ui.label(ascii_text);
                })
                .response;
            paint_search_row_background(
                ui,
                row_bg,
                row_resp.rect,
                theme,
                matches_search,
                is_active_search,
            );

            if is_active_search
                && scroll_to_byte
                    .map(|b| b >= offset && b < row_end)
                    .unwrap_or(false)
            {
                row_resp.scroll_to_me(Some(egui::Align::Center));
                scroll_to_byte = None;
            }
        }
    });
    engine.scroll_to_byte = scroll_to_byte;
    let measured_width = scroll_output.content_size.x;
    if measured_width > engine.max_detected_width {
        engine.max_detected_width = measured_width;
    }
    engine.current_scroll_x = scroll_output.state.offset.x;
    engine.current_scroll_y = scroll_output.state.offset.y;
}

pub fn render_filters_content(
    ui: &mut Ui,
    engines: &mut [TailEngine],
    theme: &CyberTheme,
    lang: Language,
) {
    let active_streams = engines
        .iter()
        .filter(|e| !e.include_filter.is_empty() || !e.exclude_filter.is_empty())
        .count();

    ui.horizontal(|ui| {
        ui.heading(
            RichText::new(format!("🔍 {}", t(lang, "filters")))
                .monospace()
                .color(theme.accent_color()),
        );
        if active_streams > 0 {
            ui.label(
                RichText::new(format!("({} {})", active_streams, t(lang, "active_count")))
                    .monospace()
                    .color(theme.accent_color())
                    .strong(),
            );
        }
    });

    ui.label(
        RichText::new(t(lang, "visibility_filters_desc"))
            .monospace()
            .size(11.0)
            .color(theme.text_dim()),
    );

    ui.add_space(8.0);

    for engine in engines.iter_mut() {
        let file_name = engine
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Log")
            .to_string();

        ui.group(|ui| {
            ui.label(
                RichText::new(format!("📄 {}", file_name))
                    .monospace()
                    .strong(),
            );

            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{}:", t(lang, "filter_include"))).monospace());
                let mut inc = engine.include_filter.clone();
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut inc)
                            .hint_text("ERROR|CRITICAL|Exception..."),
                    )
                    .changed()
                {
                    engine.set_include_filter(&inc);
                }
                if !engine.include_filter.is_empty()
                    && ui
                        .button("✖")
                        .on_hover_text(t(lang, "clear_filter"))
                        .clicked()
                {
                    engine.set_include_filter("");
                }

                ui.separator();

                // Match Case toggle
                let case_text = if engine.filter_case_sensitive {
                    RichText::new("Aa").strong().color(theme.accent_color())
                } else {
                    RichText::new("Aa").color(theme.text_dim())
                };
                if ui
                    .button(case_text)
                    .on_hover_text(t(lang, "case_sensitive_tip"))
                    .clicked()
                {
                    engine.filter_case_sensitive = !engine.filter_case_sensitive;
                    engine.refresh_filters();
                }

                // Regex toggle
                let regex_text = if engine.filter_is_regex {
                    RichText::new(".*").strong().color(theme.accent_color())
                } else {
                    RichText::new(".*").color(theme.text_dim())
                };
                if ui
                    .button(regex_text)
                    .on_hover_text(t(lang, "tip_regex_checkbox"))
                    .clicked()
                {
                    engine.filter_is_regex = !engine.filter_is_regex;
                    engine.refresh_filters();
                }
            });

            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{}:", t(lang, "filter_exclude"))).monospace());
                let mut exc = engine.exclude_filter.clone();
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut exc).hint_text("healthcheck|ping|DEBUG..."),
                    )
                    .changed()
                {
                    engine.set_exclude_filter(&exc);
                }
                if !engine.exclude_filter.is_empty()
                    && ui
                        .button("✖")
                        .on_hover_text(t(lang, "clear_filter"))
                        .clicked()
                {
                    engine.set_exclude_filter("");
                }
            });
        });
        ui.add_space(4.0);
    }
}

pub fn render_highlights_content(
    ui: &mut Ui,
    global_rules: &mut Vec<HighlightRule>,
    engines: &mut [TailEngine],
    theme: &CyberTheme,
    lang: Language,
) {
    let total_rules = global_rules.len();
    let active_rules = global_rules
        .iter()
        .filter(|r| r.enabled && !r.pattern.is_empty())
        .count();

    ui.horizontal(|ui| {
        ui.heading(
            RichText::new(format!("⚡ {}", t(lang, "highlight_rules")))
                .monospace()
                .color(theme.warn_color()),
        );
        ui.label(
            RichText::new(format!(
                "({}/{} {})",
                active_rules,
                total_rules,
                t(lang, "active_count")
            ))
            .monospace()
            .color(theme.accent_color())
            .strong(),
        );
    });

    ui.label(
        RichText::new(t(lang, "color_filters_desc"))
            .monospace()
            .size(11.0)
            .color(theme.text_dim()),
    );

    ui.label(
        RichText::new(t(lang, "rules_order_hint"))
            .monospace()
            .size(11.0)
            .color(theme.secondary_accent()),
    );

    ui.add_space(8.0);

    let mut rules_changed = false;
    let mut to_remove = None;
    let mut to_move_up = None;
    let mut to_move_down = None;

    ScrollArea::vertical().show(ui, |ui| {
        let rules_len = global_rules.len();
        for (i, rule) in global_rules.iter_mut().enumerate() {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut rule.enabled, "").changed() {
                        rules_changed = true;
                    }

                    // Move Up / Move Down buttons for priority reordering (disabled when at boundary)
                    let up_btn = ui
                        .add_enabled(i > 0, egui::Button::new("⬆"))
                        .on_hover_text(t(lang, "move_up"));
                    if up_btn.clicked() {
                        to_move_up = Some(i);
                    }
                    let down_btn = ui
                        .add_enabled(i + 1 < rules_len, egui::Button::new("⬇"))
                        .on_hover_text(t(lang, "move_down"));
                    if down_btn.clicked() {
                        to_move_down = Some(i);
                    }

                    ui.label(RichText::new(format!("#{}:", i + 1)).monospace());
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut rule.pattern)
                                .hint_text(t(lang, "hint_rule_pattern")),
                        )
                        .changed()
                    {
                        rules_changed = true;
                    }
                    if ui
                        .checkbox(&mut rule.is_regex, "Regex")
                        .on_hover_text(t(lang, "tip_regex_checkbox"))
                        .changed()
                    {
                        rules_changed = true;
                    }

                    // Match case toggle for highlight rule
                    let case_text = if rule.case_sensitive {
                        RichText::new("Aa").strong().color(theme.accent_color())
                    } else {
                        RichText::new("Aa").color(theme.text_dim())
                    };
                    if ui
                        .button(case_text)
                        .on_hover_text(t(lang, "case_sensitive_tip"))
                        .clicked()
                    {
                        rule.case_sensitive = !rule.case_sensitive;
                        rules_changed = true;
                    }

                    // Bold and Italic toggles
                    if ui
                        .checkbox(&mut rule.bold, "B")
                        .on_hover_text(t(lang, "bold"))
                        .changed()
                    {
                        rules_changed = true;
                    }
                    if ui
                        .checkbox(&mut rule.italic, "I")
                        .on_hover_text(t(lang, "italic"))
                        .changed()
                    {
                        rules_changed = true;
                    }

                    // Sound Alert Preset Selector & Test Button
                    let mut curr_alert = rule.sound_alert;
                    egui::ComboBox::from_id_salt(format!("sound_alert_{}", i))
                        .selected_text(RichText::new(curr_alert.name()).monospace().size(11.0))
                        .width(75.0)
                        .show_ui(ui, |ui| {
                            for alert in crate::audio::SoundAlertPreset::all() {
                                if ui
                                    .selectable_value(&mut curr_alert, *alert, alert.name())
                                    .clicked()
                                {
                                    rule.sound_alert = *alert;
                                    rules_changed = true;
                                }
                            }
                        });

                    if rule.sound_alert != crate::audio::SoundAlertPreset::None
                        && ui
                            .button("▶")
                            .on_hover_text(t(lang, "sound_test_tip"))
                            .clicked()
                    {
                        rule.sound_alert.play();
                    }

                    ui.label(RichText::new("FG:").monospace().size(11.0));
                    if ui.color_edit_button_srgb(&mut rule.fg_color).changed() {
                        rules_changed = true;
                    }

                    ui.label(RichText::new("BG:").monospace().size(11.0));
                    if ui.color_edit_button_srgb(&mut rule.bg_color).changed() {
                        rules_changed = true;
                    }

                    // Sample preview
                    let fg =
                        Color32::from_rgb(rule.fg_color[0], rule.fg_color[1], rule.fg_color[2]);
                    let bg =
                        Color32::from_rgb(rule.bg_color[0], rule.bg_color[1], rule.bg_color[2]);
                    let mut preview = RichText::new(format!(" {} ", t(lang, "preview")))
                        .color(fg)
                        .background_color(bg)
                        .monospace();
                    if rule.bold {
                        preview = preview.strong();
                    }
                    if rule.italic {
                        preview = preview.italics();
                    }
                    ui.label(preview);

                    if ui
                        .button("🗑")
                        .on_hover_text(t(lang, "delete_rule"))
                        .clicked()
                    {
                        to_remove = Some(i);
                        rules_changed = true;
                    }
                });
            });
        }
    });

    if let Some(i) = to_move_up {
        global_rules.swap(i, i - 1);
        rules_changed = true;
    }
    if let Some(i) = to_move_down {
        global_rules.swap(i, i + 1);
        rules_changed = true;
    }
    if let Some(idx) = to_remove {
        global_rules.remove(idx);
    }

    if ui
        .button(RichText::new(t(lang, "add_rule")).monospace())
        .on_hover_text(t(lang, "tip_add_rule"))
        .clicked()
    {
        global_rules.push(HighlightRule::new(
            "",
            [255, 255, 255],
            [0, 100, 200],
            false,
        ));
        rules_changed = true;
    }

    if rules_changed {
        for engine in engines {
            engine.set_highlight_rules(global_rules.clone());
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn render_settings_content(
    ui: &mut Ui,
    theme: &mut CyberTheme,
    lang: &mut Language,
    screensaver_enabled: &mut bool,
    screensaver_timeout_mins: &mut u32,
    test_screensaver: &mut bool,
    telemetry_enabled: &mut bool,
    sound_enabled: &mut bool,
    borderless: &mut bool,
    show_line_numbers: &mut bool,
    font_size: &mut f32,
) {
    ui.heading(RichText::new(format!("⚙ {}", t(*lang, "settings").to_uppercase())).monospace());
    ui.add_space(10.0);

    // Theme selector
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{}:", t(*lang, "theme"))).monospace());
        ui.selectable_value(theme, CyberTheme::Tron, "Tron")
            .clicked();
        ui.selectable_value(theme, CyberTheme::Matrix, "Matrix")
            .clicked();
        ui.selectable_value(theme, CyberTheme::Blade, "Blade")
            .clicked();
        if ui
            .selectable_value(theme, CyberTheme::Light, t(*lang, "theme_light"))
            .clicked()
        {}
    });

    ui.add_space(6.0);

    // Language selector
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{}:", t(*lang, "language"))).monospace());
        ui.selectable_value(lang, Language::En, "English").clicked();
        ui.selectable_value(lang, Language::It, "Italiano")
            .clicked();
        ui.selectable_value(lang, Language::Fr, "Français")
            .clicked();
        ui.selectable_value(lang, Language::Es, "Español").clicked();
        if ui.selectable_value(lang, Language::Zh, "中文").clicked() {}
    });

    ui.add_space(6.0);

    // Font size selector
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{}:", t(*lang, "font_size"))).monospace());
        if ui
            .button(" - ")
            .on_hover_text(t(*lang, "font_dec_tip"))
            .clicked()
        {
            *font_size = (*font_size - 1.0).max(8.0);
        }
        ui.label(
            RichText::new(format!("{:.0} pt", *font_size))
                .monospace()
                .strong(),
        );
        if ui
            .button(" + ")
            .on_hover_text(t(*lang, "font_inc_tip"))
            .clicked()
        {
            *font_size = (*font_size + 1.0).min(32.0);
        }
        if ui
            .button("100%")
            .on_hover_text(t(*lang, "font_reset_tip"))
            .clicked()
        {
            *font_size = 13.0;
        }
    });

    ui.add_space(6.0);
    ui.separator();
    ui.add_space(6.0);

    // Screensaver controls
    ui.checkbox(screensaver_enabled, t(*lang, "screensaver"));
    if *screensaver_enabled {
        ui.horizontal(|ui| {
            ui.label(t(*lang, "screensaver_timeout"));
            ui.add(egui::DragValue::new(screensaver_timeout_mins).range(1..=120));
            ui.add_space(8.0);
            if ui.button(t(*lang, "test_screensaver")).clicked() {
                *test_screensaver = true;
            }
        });
    }

    ui.add_space(6.0);
    ui.checkbox(telemetry_enabled, t(*lang, "telemetry"));
    ui.checkbox(sound_enabled, t(*lang, "sound_fx"));
    ui.checkbox(borderless, t(*lang, "borderless"));
    ui.checkbox(show_line_numbers, t(*lang, "show_lines"));
}

fn render_markdown_stream(
    ui: &mut Ui,
    engine: &mut TailEngine,
    theme: &CyberTheme,
    lang: Language,
) {
    if engine.buffer.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new(t(lang, "no_file_open"))
                    .monospace()
                    .color(theme.text_dim()),
            );
        });
        return;
    }

    engine.ensure_markdown_text();
    let text: &str = engine
        .markdown_text_cache
        .as_ref()
        .map(|(_, s)| s.as_str())
        .unwrap_or("");

    let mut scroll_area = ScrollArea::both()
        .auto_shrink([false, false])
        .stick_to_bottom(engine.follow_tail);

    if let Some(y) = engine.requested_scroll_y.take() {
        scroll_area = scroll_area.vertical_scroll_offset(y);
    }
    if let Some(x) = engine.requested_scroll_x.take() {
        scroll_area = scroll_area.horizontal_scroll_offset(x);
    }

    let html_renderer = |ui: &mut egui::Ui, html: &str| {
        let converted = crate::html_converter::html_to_markdown(html);
        ui.label(converted);
    };

    let scroll_output = scroll_area.show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        egui_commonmark::CommonMarkViewer::new()
            .render_html_fn(Some(&html_renderer))
            .show(ui, &mut engine.markdown_cache, text);
    });

    engine.current_scroll_y = scroll_output.state.offset.y;
    engine.current_scroll_x = scroll_output.state.offset.x;
}
