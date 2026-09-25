use crate::config::push_search_history;
use crate::external_tools::{ExternalTool, ToolContext, ToolRunner};
use crate::i18n::{t, Language};
use crate::paths::{paths_equal, paths_equal_fast};
use crate::tail_engine::{
    HighlightRule, HighlightSpan, HighlightStyle, QuickLabel, SpanStyle, TailEngine, TimeDelta,
    MAX_LINE_BYTES,
};
use crate::theme::CyberTheme;
use crate::wrap_layout::{
    anchor_center, anchor_to_bottom, fill_from, layout_slice, walk_anchor, WrapAnchor, WrapScroll,
};
use egui::{Color32, RichText, ScrollArea, Stroke, Ui, WidgetText};
use egui_dock::TabViewer;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// How long to wait after the last keystroke before rescanning the file for matches.
const SEARCH_DEBOUNCE: Duration = Duration::from_millis(150);

/// Virtual width of the horizontal scroll canvas, far beyond the widest line the renderer
/// can produce (lines are capped at MAX_LINE_BYTES). Keeps the right edge of the scroll
/// content beyond any real line so horizontal scrolling is effectively unlimited.
const HORIZONTAL_SCROLL_EXTENT: f32 = MAX_LINE_BYTES as f32 * 16.0;

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

use crate::log_level::LogLevel;
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
    /// Colour rows by detected log level when no highlight rule matches (Settings).
    pub level_colors: &'a mut bool,
    pub size_unit: &'a mut crate::tail_engine::SizeUnit,
    pub search_history: &'a mut Vec<String>,
    pub tab_closed: &'a mut bool,
    pub test_screensaver: &'a mut bool,
    /// PIN lock (Settings): master switch, scrambled PIN and a "lock right now" request.
    /// Follow the OS language instead of the stored one (Settings language row).
    pub language_auto: &'a mut bool,
    pub lock_enabled: &'a mut bool,
    pub lock_pin: &'a mut String,
    pub lock_now: &'a mut bool,
    /// Quick colour labels (Ctrl+Shift+1..9), shared by every stream; `labels_changed` asks
    /// the app to push them to the engines again.
    pub quick_labels: &'a mut Vec<QuickLabel>,
    pub labels_changed: &'a mut bool,
    /// External tools (Settings editor, row context menu, stream menu) and the runner
    /// that spawns them and counts dropped rule-bound runs.
    pub external_tools: &'a mut Vec<ExternalTool>,
    pub tool_runner: &'a mut ToolRunner,
    /// Stream shown in the focused dock leaf: the only one that handles search shortcuts.
    pub focused_stream: Option<PathBuf>,
    /// Search results pane and overview strip preferences, shared by every stream.
    pub search_view: &'a mut SearchViewPrefs,
    /// Time delta column switch and gap threshold, shared by every stream.
    pub time_delta: &'a mut TimeDeltaPrefs,
}

/// Search results pane (open flag, height) and overview strip switch, global preferences
/// persisted in `fasttail.ini` and applied to every stream.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SearchViewPrefs {
    pub search_pane: bool,
    pub search_pane_height: f32,
    pub overview_strip: bool,
}

impl Default for SearchViewPrefs {
    fn default() -> Self {
        Self {
            search_pane: false,
            search_pane_height: DEFAULT_SEARCH_PANE_HEIGHT,
            overview_strip: true,
        }
    }
}

/// Time delta column (stream toolbar `Δt`, Settings): shown or not, and the gap from
/// which a delta is drawn in the accent colour. Global preferences in `fasttail.ini`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeDeltaPrefs {
    pub show: bool,
    /// Milliseconds; 0 turns the tint off.
    pub gap_ms: u64,
}

impl Default for TimeDeltaPrefs {
    fn default() -> Self {
        Self {
            show: false,
            gap_ms: DEFAULT_TIME_DELTA_GAP_MS,
        }
    }
}

/// Gap tint threshold of the time delta column until the user changes it.
pub const DEFAULT_TIME_DELTA_GAP_MS: u64 = 1000;
/// Width of the time delta column, in characters: `-59:59.999` and a space.
const TIME_DELTA_CHARS: f32 = 11.0;

/// Height of the search results pane until the user drags it.
pub const DEFAULT_SEARCH_PANE_HEIGHT: f32 = 180.0;
/// The results pane never gets smaller than this, nor leaves the rows less than this.
const MIN_SEARCH_PANE_HEIGHT: f32 = 60.0;
const MIN_ROWS_HEIGHT: f32 = 80.0;

/// Keyboard target of a stream's search results pane (see `hit_list`).
pub fn search_pane_id(engine: &TailEngine) -> egui::Id {
    egui::Id::new("search_pane").with(&engine.path)
}

/// "first 1,000,000 listed": the note of a search that matched more lines than it keeps.
fn capped_note(lang: Language) -> String {
    t(lang, "search_capped").replace(
        "{n}",
        &group_thousands(crate::tail_engine::MAX_SEARCH_MATCHES),
    )
}

/// `1234567` as `1,234,567`.
pub fn group_thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Row data handed to an external tool: the row text, the file being tailed (the resolved
/// file of a pattern stream, the archive of a compressed one, never its spool), the
/// 1-based line number and the selection text when the row is part of a selection.
pub fn tool_context_for_row(engine: &TailEngine, row: usize) -> Option<ToolContext> {
    let line = engine.get_line(row)?.into_owned();
    let file = engine.source_file();
    let selection = if engine.has_selection() && engine.is_selected(row) {
        engine.copy_selection_text()
    } else {
        None
    };
    Some(ToolContext::for_row(
        &file,
        row + 1,
        &line,
        selection.as_deref(),
    ))
}

/// Creates or truncates a regular export file safely.
/// Opens without truncating, validates that the handle points to a regular file,
/// and truncates to 0 bytes only after confirming it is not a directory or non-regular file.
pub fn create_export_file(target: &Path) -> std::io::Result<std::fs::File> {
    if target.exists() {
        let meta = std::fs::metadata(target)?;
        if !meta.is_file() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Target path is not a regular file",
            ));
        }
    }
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .open(target)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Target path is not a regular file",
        ));
    }
    file.set_len(0)?;
    Ok(file)
}

/// Runs a tool on a row from a user gesture; a spawn failure becomes a stream notice.
pub fn run_tool_on_row(
    engine: &mut TailEngine,
    tool: &ExternalTool,
    runner: &mut ToolRunner,
    row: usize,
    lang: Language,
) {
    let Some(ctx) = tool_context_for_row(engine, row) else {
        return;
    };
    if let Err(err) = runner.run_manual(tool, &ctx) {
        engine.view_notice = Some(format!("{}: {err}", t(lang, "ext_tool_run_failed")));
    }
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
                    // Pattern stream: the tab names the pattern and the file it resolves to;
                    // a zip entry names the archive and the entry.
                    let file_name = match engine.current_file_name() {
                        Some(current) if engine.is_pattern() => {
                            format!("{file_name} ▸ {current}")
                        }
                        _ => match &engine.compressed {
                            Some(c) => c.title(),
                            None => file_name.to_string(),
                        },
                    };
                    // Background-tab activity badge: lines appended since the tab was last shown
                    let badge = if !engine.displayed && engine.unseen_lines > 0 {
                        if engine.unseen_lines > 999 {
                            " [999+]".to_string()
                        } else {
                            format!(" [{}]", engine.unseen_lines)
                        }
                    } else {
                        String::new()
                    };
                    let title_text = format!(
                        "[#{}] {} {} {}{}",
                        idx + 1,
                        watch_icon,
                        file_name,
                        data_dot,
                        badge
                    );
                    if !badge.is_empty() {
                        let color = match engine.unseen_severity {
                            2 => self.ctx.theme.warn_color(),
                            1 => self.ctx.theme.accent_color(),
                            _ => self.ctx.theme.secondary_accent(),
                        };
                        return WidgetText::from(
                            RichText::new(title_text).monospace().strong().color(color),
                        );
                    }
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
                    // Mark new data as viewed/cleared; the tab is on screen this frame
                    engine.has_new_data = false;
                    engine.mark_seen();
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
                        *self.ctx.level_colors,
                        &mut new_size_unit,
                        is_focused,
                        self.ctx.quick_labels,
                        self.ctx.labels_changed,
                        self.ctx.external_tools,
                        self.ctx.tool_runner,
                        self.ctx.search_view,
                        self.ctx.time_delta,
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
                    self.ctx.level_colors,
                    self.ctx.external_tools,
                    self.ctx.global_rules,
                    self.ctx.tool_runner,
                    self.ctx.language_auto,
                    self.ctx.lock_enabled,
                    self.ctx.lock_pin,
                    self.ctx.lock_now,
                    &mut self.ctx.search_view.overview_strip,
                    self.ctx.time_delta,
                );
            }
        }
    }

    fn on_tab_button(&mut self, tab: &mut Self::Tab, response: &egui::Response) {
        // A compressed stream: the tab tooltip names the archive (and the entry).
        if let FastTailTab::LogStream(path) = tab {
            if let Some(c) = find_engine_index(self.ctx.engines, path)
                .and_then(|i| self.ctx.engines[i].compressed.as_ref())
            {
                let mut tip = c.archive.display().to_string();
                if let Some(entry) = &c.entry {
                    tip.push_str(&format!("\n› {entry}"));
                }
                response.clone().on_hover_text(tip);
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
    bookmarked: bool,
) {
    let (glyph, color) = marker_glyph(theme, matches, is_active, bookmarked);
    ui.label(
        RichText::new(format!("{glyph} "))
            .monospace()
            .strong()
            .size(font_size)
            .color(color),
    );
}

/// Row tint for search hits, the selection and bookmarks; search tints win over the
/// selection tint, which wins over the bookmark tint.
fn row_tint(
    theme: &CyberTheme,
    matches: bool,
    is_active: bool,
    selected: bool,
    bookmarked: bool,
) -> Option<Color32> {
    if !matches && !is_active && !selected && !bookmarked {
        return None;
    }
    let (base, alpha) = if is_active {
        (theme.accent_color(), 70)
    } else if matches {
        (theme.warn_color(), 40)
    } else if selected {
        (theme.secondary_accent(), 60)
    } else {
        (theme.secondary_accent(), 28)
    };
    Some(Color32::from_rgba_unmultiplied(
        base.r(),
        base.g(),
        base.b(),
        alpha,
    ))
}

/// Marker glyph and colour of a row: ▶ current hit, ● other hits, ★ bookmark, blank otherwise.
fn marker_glyph(
    theme: &CyberTheme,
    matches: bool,
    is_active: bool,
    bookmarked: bool,
) -> (&'static str, Color32) {
    if is_active {
        ("▶", theme.accent_color())
    } else if matches {
        ("●", theme.warn_color())
    } else if bookmarked {
        ("★", theme.secondary_accent())
    } else {
        ("\u{2007}", theme.text_dim()) // figure space: same advance as a digit
    }
}

/// Tints the whole row of a search hit, drawing behind the widgets laid out in `row_rect`.
#[allow(clippy::too_many_arguments)]
fn paint_search_row_background(
    ui: &Ui,
    slot: egui::layers::ShapeIdx,
    row_rect: egui::Rect,
    theme: &CyberTheme,
    matches: bool,
    is_active: bool,
    selected: bool,
    bookmarked: bool,
) {
    let Some(fill) = row_tint(theme, matches, is_active, selected, bookmarked) else {
        return;
    };
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
/// The "from / to" time window of a stream, next to the text filters.
///
/// The fields take `14:02`, `14:02:05`, a full date and time, or a timestamp copied out
/// of the log, or a date alone (the whole day); a bare time belongs to the day of the log,
/// not to today. A hint says when most lines cannot be placed in time, since a window over
/// such a log hides them; the fields stay usable, so a window can always be typed, fixed
/// or cleared. A window typed while the stream is still being timed in the background is
/// held, with a hint, until timing finishes.
fn render_time_range(ui: &mut Ui, engine: &mut TailEngine, theme: &CyberTheme, lang: Language) {
    let usable = engine.timestamps_usable() || !engine.timestamps_complete();
    let show_clear = time_range_clear_shown(
        !engine.time_from_text.trim().is_empty() || !engine.time_to_text.trim().is_empty(),
        engine.is_time_filtered() || engine.time_range_pending(),
    );
    ui.label(
        RichText::new(format!("🕘 {}:", t(lang, "time_range")))
            .monospace()
            .size(11.0)
            .color(if usable {
                theme.accent_color()
            } else {
                theme.text_dim()
            }),
    );

    let mut from = engine.time_from_text.clone();
    let mut to = engine.time_to_text.clone();
    let mut changed = false;
    let field = |ui: &mut Ui, text: &mut String, hint: &str| {
        ui.add(
            egui::TextEdit::singleline(text)
                .hint_text(hint)
                .desired_width(74.0),
        )
    };

    let from_resp = field(ui, &mut from, t(lang, "time_from_hint"));
    changed |= from_resp.changed();
    ui.label(RichText::new("→").monospace().color(theme.text_dim()));
    let to_resp = field(ui, &mut to, t(lang, "time_to_hint"));
    // A side is judged once the user leaves it: `1` on the way to `14:02` is no error.
    let typing = from_resp.has_focus() || to_resp.has_focus();
    changed |= to_resp.changed();

    if changed {
        let (from_ok, to_ok) = engine.apply_time_range_text(&from, &to);
        engine.time_range_error = !from_ok || !to_ok;
    }

    if show_clear
        && ui
            .button("✖")
            .on_hover_text(t(lang, "time_range_clear"))
            .clicked()
    {
        engine.clear_time_range();
        engine.time_range_error = false;
    }

    if engine.time_range_error && !typing {
        ui.label(
            RichText::new(format!("⚠ {}", t(lang, "time_range_invalid")))
                .monospace()
                .size(10.5)
                .color(theme.warn_color()),
        );
    } else if engine.time_range_pending() {
        // The stream is still being timed in the background (progress in the stream
        // bar): the window is held and applies by itself once every line is timed.
        ui.label(
            RichText::new(format!("⏳ {}", t(lang, "time_range_pending")))
                .monospace()
                .size(10.5)
                .color(theme.warn_color()),
        );
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(100));
    } else if !usable {
        ui.label(
            RichText::new(format!("ⓘ {}", t(lang, "time_range_unavailable")))
                .monospace()
                .size(10.5)
                .color(theme.text_dim()),
        )
        .on_hover_text(t(lang, "time_range_unavailable_tip"));
    }
}

/// Whether the ✖ that clears both time fields and the window is shown: as soon as there
/// is anything to clear, an unparsable text included, from whether either field holds
/// text and whether a window is applied or held.
fn time_range_clear_shown(has_text: bool, windowed: bool) -> bool {
    has_text || windowed
}

/// Toolbar toggle (Follow, Monitor, line numbers, Wrap, TXT/HEX/MD): an active one gets
/// a tinted fill and an accent border on top of the coloured label. On the light theme
/// the label colour alone was too close to the inactive grey to read as "on".
fn toggle_button(
    ui: &mut Ui,
    theme: &CyberTheme,
    label: &str,
    active: bool,
    accent: Color32,
) -> egui::Response {
    let text = if active {
        RichText::new(label).color(accent).monospace().strong()
    } else {
        RichText::new(label).color(theme.text_dim()).monospace()
    };
    let mut button = egui::Button::new(text);
    if active {
        button = button
            .fill(accent.gamma_multiply(0.20))
            .stroke(Stroke::new(1.0, accent.gamma_multiply(0.85)));
    }
    ui.add(button)
}

/// Toolbar selector of the stream's ANSI mode: `ANSI: auto → render` shows what auto
/// resolved to; picking a mode applies it at once (filters and search run again).
fn render_ansi_mode_selector(ui: &mut Ui, engine: &mut TailEngine, lang: Language) {
    use crate::ansi::AnsiMode;
    let mode_name = |mode: AnsiMode| match mode {
        AnsiMode::Auto => t(lang, "ansi_auto"),
        AnsiMode::Render => t(lang, "ansi_render"),
        AnsiMode::Strip => t(lang, "ansi_strip"),
        AnsiMode::Raw => t(lang, "ansi_raw"),
    };
    let selected = match engine.ansi_mode {
        AnsiMode::Auto => format!(
            "ANSI: {} → {}",
            mode_name(AnsiMode::Auto),
            mode_name(engine.ansi_effective())
        ),
        mode => format!("ANSI: {}", mode_name(mode)),
    };
    let mut current = engine.ansi_mode;
    egui::ComboBox::from_id_salt(format!("ansi_sel_{}", engine.path.display()))
        .selected_text(RichText::new(selected).monospace().size(11.0))
        .show_ui(ui, |ui| {
            for mode in AnsiMode::ALL {
                if ui
                    .selectable_value(&mut current, mode, mode_name(mode))
                    .clicked()
                {
                    engine.set_ansi_mode(mode);
                    ui.ctx().request_repaint();
                }
            }
        })
        .response
        .on_hover_text(t(lang, "tip_ansi"));
}

/// Stream bar part of a compressed stream: `decompressing N%` with a cancel button while
/// the job runs, then why the content is partial if it stopped early, and a button that
/// extracts the archive again.
fn render_compressed_status(
    ui: &mut Ui,
    engine: &mut TailEngine,
    theme: &CyberTheme,
    lang: Language,
) {
    use crate::compressed::{JobState, StopReason};
    let Some(c) = engine.compressed.as_ref() else {
        return;
    };
    ui.separator();
    let mut tip = c.archive.display().to_string();
    if let Some(entry) = &c.entry {
        tip.push_str(&format!("\n› {entry}"));
    }
    let state = c.state();
    let mut cancel = false;
    let mut reload = false;
    match &state {
        JobState::Running => {
            ui.label(
                RichText::new(format!(
                    "🗜 {} {:.0}%",
                    t(lang, "compressed_decompressing"),
                    c.progress() * 100.0
                ))
                .monospace()
                .color(theme.warn_color()),
            )
            .on_hover_text(tip);
            cancel = ui
                .button(RichText::new("✖").monospace())
                .on_hover_text(t(lang, "compressed_cancel"))
                .clicked();
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
        JobState::Done => {
            ui.label(
                RichText::new("🗜")
                    .monospace()
                    .color(theme.secondary_accent()),
            )
            .on_hover_text(tip);
        }
        JobState::Stopped(reason) => {
            let why = match reason {
                StopReason::Cancelled => t(lang, "compressed_cancelled").to_string(),
                StopReason::CapReached(cap) => t(lang, "compressed_cap_reached")
                    .replace("{size}", &crate::ui::zip_picker::human_size(*cap)),
                StopReason::DiskFull { volume } => {
                    t(lang, "compressed_disk_full").replace("{volume}", volume)
                }
                StopReason::Tar => t(lang, "compressed_tar").to_string(),
                StopReason::Failed(err) => format!("{}: {err}", t(lang, "compressed_failed")),
            };
            // Nothing was written for a refused tar: the content is not "partial".
            let text = if matches!(reason, StopReason::Tar) || c.written() == 0 {
                format!("🗜 {why}")
            } else {
                format!("🗜 {} ({why})", t(lang, "compressed_partial"))
            };
            ui.label(RichText::new(text).monospace().color(theme.warn_color()))
                .on_hover_text(tip);
        }
    }
    if !matches!(state, JobState::Running)
        && ui
            .button(RichText::new("⟳").monospace())
            .on_hover_text(t(lang, "compressed_reload"))
            .clicked()
    {
        reload = true;
    }
    if cancel {
        c.cancel();
    }
    if reload {
        if let Err(err) = engine.reload_compressed() {
            engine.view_notice = Some(format!("{}: {err}", t(lang, "compressed_failed")));
        }
        ui.ctx().request_repaint();
    }
}

/// Translation key naming a background scan, as shown in the stream bar.
fn scan_kind_key(kind: crate::scan_job::ScanKind) -> &'static str {
    match kind {
        crate::scan_job::ScanKind::Index => "scan_indexing",
        crate::scan_job::ScanKind::Filter => "scan_filtering",
        crate::scan_job::ScanKind::Search => "scan_searching",
        crate::scan_job::ScanKind::Levels => "scan_levels",
        crate::scan_job::ScanKind::Timestamps => "scan_timestamps",
    }
}

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
    level_colors: bool,
    new_size_unit: &mut Option<crate::tail_engine::SizeUnit>,
    is_focused: bool,
    quick_labels: &mut Vec<QuickLabel>,
    labels_changed: &mut bool,
    external_tools: &[ExternalTool],
    tool_runner: &mut ToolRunner,
    search_view: &mut SearchViewPrefs,
    time_delta: &mut TimeDeltaPrefs,
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
    // The results pane shows while a query is active; the rows get what it leaves, so
    // centring a target measures against that.
    let pane_visible = search_view.search_pane && !engine.last_searched_query.is_empty();
    let viewport_height = if pane_visible {
        (viewport_height - search_view.search_pane_height).max(MIN_ROWS_HEIGHT)
    } else {
        viewport_height
    };

    // Viewport movement helpers. Extend mode maps lines to pixels through the constant row
    // height; wrap mode hands the wrap renderer a request that it resolves through the real
    // row heights (see `wrap_layout`), so every jump stays anchored to a line index.
    let wrap_view = |engine: &TailEngine| {
        engine.wrap_lines && engine.view_mode != crate::tail_engine::ViewMode::Hex
    };
    let top_row = |engine: &TailEngine| -> usize {
        if wrap_view(engine) {
            engine.wrap_anchor.row
        } else {
            (engine.current_scroll_y / row_height).round() as usize
        }
    };
    let scroll_lines = |engine: &mut TailEngine, n: i64| {
        engine.follow_tail = false;
        if wrap_view(engine) {
            engine.wrap_request = Some(WrapScroll::Lines(n));
        } else {
            engine.requested_scroll_y =
                Some((engine.current_scroll_y + n as f32 * row_height).max(0.0));
        }
    };
    let scroll_pages = |engine: &mut TailEngine, n: i32| {
        engine.follow_tail = false;
        if wrap_view(engine) {
            engine.wrap_request = Some(WrapScroll::Pages(n));
        } else {
            let visible_lines = ((viewport_height / row_height).floor() as usize).max(1);
            let current_line = (engine.current_scroll_y / row_height).round() as usize;
            let target_line = if n < 0 {
                current_line.saturating_sub(visible_lines * n.unsigned_abs() as usize)
            } else {
                current_line + visible_lines * n as usize
            };
            engine.requested_scroll_y = Some(target_line as f32 * row_height);
        }
    };
    let scroll_top = |engine: &mut TailEngine| {
        engine.follow_tail = false;
        if wrap_view(engine) {
            engine.wrap_request = Some(WrapScroll::Top);
        } else {
            engine.requested_scroll_y = Some(0.0);
        }
    };
    let scroll_bottom = |engine: &mut TailEngine| {
        engine.follow_tail = !engine.is_compressed();
        if wrap_view(engine) {
            engine.wrap_request = Some(WrapScroll::Bottom);
        } else {
            let max_y = (engine.visible_line_count() as f32 * row_height).max(0.0);
            engine.requested_scroll_y = Some(max_y);
        }
    };
    // Scroll to a search target (a line index, or a byte offset in HEX view), centering it
    // in the viewport
    let scroll_to_target = |engine: &mut TailEngine, target: usize| {
        if engine.view_mode == crate::tail_engine::ViewMode::Hex {
            let row_y = (target / bytes_per_row) as f32 * hex_row_height;
            engine.requested_scroll_y = Some((row_y - viewport_height / 2.0).max(0.0));
            engine.requested_scroll_x = Some(0.0);
            engine.follow_tail = false;
        } else if wrap_view(engine) {
            engine.scroll_to_line = None;
            engine.wrap_request = Some(WrapScroll::CenterLine(target));
            engine.follow_tail = false;
        } else {
            engine.scroll_to_line = Some(target);
            if let Some(row_y) = engine
                .get_visible_row_of_line(target)
                .map(|v_row| v_row as f32 * row_height)
            {
                engine.requested_scroll_y = Some((row_y - viewport_height / 2.0).max(0.0));
                engine.requested_scroll_x = Some(0.0);
                engine.follow_tail = false;
            }
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
        let follow_label = if engine.follow_tail {
            "▶ Follow"
        } else {
            "■ Follow"
        };
        if engine.is_compressed() {
            // A compressed stream is a static snapshot of the archive: nothing to follow.
            ui.add_enabled_ui(false, |ui| {
                toggle_button(ui, theme, follow_label, false, theme.accent_color())
            })
            .inner
            .on_disabled_hover_text(t(lang, "compressed_follow_tip"));
        } else if toggle_button(
            ui,
            theme,
            follow_label,
            engine.follow_tail,
            theme.accent_color(),
        )
        .on_hover_text(t(lang, "tip_follow_tail"))
        .clicked()
        {
            engine.follow_tail = !engine.follow_tail;
            ui.ctx().request_repaint();
        }

        ui.separator();

        // Monitor disk reading toggle (no attivo/sospeso text, using ▶ and ■ with color)
        let monitor_label = if engine.is_watching {
            "▶ Monitor"
        } else {
            "■ Monitor"
        };
        if toggle_button(
            ui,
            theme,
            monitor_label,
            engine.is_watching,
            theme.accent_color(),
        )
        .on_hover_text(t(lang, "tip_monitor"))
        .clicked()
        {
            engine.is_watching = !engine.is_watching;
            ui.ctx().request_repaint();
        }

        ui.separator();

        // Mode Switcher (TXT, HEX, MD)
        let current_mode = engine.view_mode;
        let is_txt = current_mode == crate::tail_engine::ViewMode::Text
            || current_mode == crate::tail_engine::ViewMode::Filtered;
        let is_hex = current_mode == crate::tail_engine::ViewMode::Hex;
        let is_md = current_mode == crate::tail_engine::ViewMode::Markdown;

        if toggle_button(ui, theme, "🔤 TXT", is_txt, theme.accent_color())
            .on_hover_text(t(lang, "tip_mode_txt"))
            .clicked()
        {
            engine.set_view_mode(crate::tail_engine::ViewMode::Text);
            ui.ctx().request_repaint();
        }

        if toggle_button(ui, theme, "🔢 HEX", is_hex, theme.secondary_accent())
            .on_hover_text(t(lang, "tip_mode_hex"))
            .clicked()
        {
            engine.set_view_mode(crate::tail_engine::ViewMode::Hex);
            ui.ctx().request_repaint();
        }

        let md_too_large = engine.markdown_too_large();
        let md_limit_mb = (engine.markdown_max_bytes / (1024 * 1024)).max(1);
        let md_msg = t(lang, "md_too_large").replace("{limit}", &md_limit_mb.to_string());
        let md_tip = if md_too_large {
            md_msg.as_str()
        } else {
            t(lang, "tip_mode_md")
        };
        if toggle_button(ui, theme, "📝 MD", is_md, theme.warn_color())
            .on_hover_text(md_tip)
            .clicked()
        {
            if md_too_large {
                engine.view_notice = Some(md_msg);
            } else {
                engine.set_view_mode(crate::tail_engine::ViewMode::Markdown);
            }
            ui.ctx().request_repaint();
        }

        // Line numbers & Line wrap toggle, meaningful in the text views only (hidden in Hex mode)
        if engine.view_mode != crate::tail_engine::ViewMode::Hex {
            ui.separator();

            // Line numbers toggle
            let lines_label = if *show_line_numbers { "# 123" } else { "# ---" };
            if toggle_button(
                ui,
                theme,
                lines_label,
                *show_line_numbers,
                theme.accent_color(),
            )
            .on_hover_text(t(lang, "show_lines"))
            .clicked()
            {
                *show_line_numbers = !*show_line_numbers;
                ui.ctx().request_repaint();
            }

            // Time delta column toggle (global, like the line numbers). A stream whose
            // timestamps we cannot read keeps the column hidden and says why.
            let delta_tip = if timestamps_unreadable(engine) {
                t(lang, "time_delta_unusable")
            } else {
                t(lang, "tip_time_delta")
            };
            if toggle_button(ui, theme, "Δt", time_delta.show, theme.accent_color())
                .on_hover_text(delta_tip)
                .clicked()
            {
                time_delta.show = !time_delta.show;
                ui.ctx().request_repaint();
            }

            // Line wrap toggle (per stream, Alt+W), meaningful in the text views only
            let alt_w = egui::KeyboardShortcut::new(egui::Modifiers::ALT, egui::Key::W);
            let toggled =
                toggle_button(ui, theme, "↩ Wrap", engine.wrap_lines, theme.accent_color())
                    .on_hover_text(t(lang, "tip_wrap"))
                    .clicked()
                    || (is_focused && ui.input_mut(|i| i.consume_shortcut(&alt_w)));
            if toggled {
                let row = top_row(engine);
                engine.set_wrap_lines(!engine.wrap_lines, row);
                if !engine.wrap_lines {
                    // Extend mode maps rows to pixels itself: keep the same top row.
                    engine.requested_scroll_y = Some(row as f32 * row_height);
                }
                ui.ctx().request_repaint();
            }
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
            render_ansi_mode_selector(ui, engine, lang);
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

        // Time span of what is on screen: the answer to "which minutes am I looking at".
        if let Some((from, to)) = engine.visible_time_span() {
            ui.separator();
            let span = if from == to {
                crate::timestamp::format_millis(from)
            } else {
                format!(
                    "{} → {}",
                    crate::timestamp::format_millis(from),
                    crate::timestamp::format_millis(to)
                )
            };
            ui.label(
                RichText::new(format!("🕘 {span}"))
                    .monospace()
                    .size(11.0)
                    .color(if engine.is_time_filtered() {
                        theme.accent_color()
                    } else {
                        theme.text_dim()
                    }),
            )
            .on_hover_text(t(lang, "time_span"));
        }

        // Elapsed time of a selection of two or more timed rows.
        if let Some((millis, rows)) = engine.selection_elapsed() {
            ui.separator();
            let text = t(lang, "selection_elapsed")
                .replace("{delta}", &crate::timestamp::format_delta(millis))
                .replace("{n}", &group_thousands(rows));
            ui.label(
                RichText::new(text)
                    .monospace()
                    .size(11.0)
                    .color(theme.accent_color()),
            )
            .on_hover_text(t(lang, "selection_elapsed_tip"));
        }

        // Per-level counters (most severe first), only the levels seen in the file
        if engine.view_mode != crate::tail_engine::ViewMode::Hex {
            let counts: Vec<(LogLevel, u64)> = LogLevel::ALL
                .iter()
                .rev()
                .map(|l| (*l, engine.level_count(*l)))
                .filter(|(_, n)| *n > 0)
                .collect();
            if !counts.is_empty() {
                ui.separator();
                for (level, n) in counts {
                    ui.label(
                        RichText::new(format!("{} {}", level.short(), n))
                            .monospace()
                            .size(11.0)
                            .color(theme.level_color(level)),
                    )
                    .on_hover_text(t(lang, "level_counts_tip"));
                }
            }
        }

        // Background scan in progress: kind, percentage and hits so far
        if let Some((kind, progress, hits)) = engine.scan_progress() {
            let key = scan_kind_key(kind);
            ui.label(
                RichText::new(format!(
                    "⏳ {} {:.0}% ({})",
                    t(lang, key),
                    (progress * 100.0).clamp(0.0, 100.0),
                    hits
                ))
                .monospace()
                .color(theme.warn_color()),
            );
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
        if let Some(notice) = &engine.view_notice {
            ui.label(
                RichText::new(format!("ⓘ {notice}"))
                    .monospace()
                    .color(theme.warn_color()),
            );
        }
        if engine.ansi_switch_notice() {
            ui.label(
                RichText::new(format!("ⓘ {}", t(lang, "ansi_switched")))
                    .monospace()
                    .color(theme.warn_color()),
            );
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(500));
        }
        render_compressed_status(ui, engine, theme, lang);

        // Pattern stream: the pattern, the file being tailed, and the switch notice
        if let Some(glob) = engine.pattern.clone() {
            ui.separator();
            let label = match engine.current_file_name() {
                Some(name) => RichText::new(format!("📂* {glob} ▸ {name}"))
                    .monospace()
                    .color(theme.secondary_accent()),
                None => RichText::new(format!("📂* {glob} ▸ {}", t(lang, "pattern_waiting")))
                    .monospace()
                    .color(theme.warn_color()),
            };
            let tip = match &engine.current_file {
                Some(file) => format!("{}\n{}", engine.path.display(), file.display()),
                None => engine.path.display().to_string(),
            };
            ui.label(label).on_hover_text(tip);
            if let Some(name) = engine.active_switch_notice() {
                ui.label(
                    RichText::new(format!("ⓘ {} {name}", t(lang, "pattern_switched")))
                        .monospace()
                        .color(theme.warn_color()),
                );
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(500));
            }
        }

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

        let extra_controls_w = if has_query { 250.0 } else { 100.0 };
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
                    scroll_lines(engine, 1);
                }
                ui.ctx().request_repaint();
            }
            if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp)) {
                if let Some(target) = engine.search_prev(sound_enabled) {
                    scroll_to_target(engine, target);
                    push_search_history(search_history, search_query);
                } else {
                    scroll_lines(engine, -1);
                }
                ui.ctx().request_repaint();
            }
            if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::PageDown)) {
                scroll_pages(engine, 1);
                ui.ctx().request_repaint();
            }
            if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::PageUp)) {
                scroll_pages(engine, -1);
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
                // The true total: past the cap the hits are counted, not listed.
                let counter = ui.label(
                    RichText::new(format!(
                        "[{} / {}]",
                        current_1based,
                        group_thousands(engine.search_total())
                    ))
                    .monospace()
                    .strong()
                    .color(theme.accent_color()),
                );
                if engine.search_capped() {
                    let note = capped_note(lang);
                    counter.on_hover_text(&note);
                    ui.label(
                        RichText::new(format!("({note})"))
                            .monospace()
                            .size(11.0)
                            .color(theme.warn_color()),
                    );
                }
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

        // Search results pane toggle (a global preference, shown while a query is active)
        if toggle_button(
            ui,
            theme,
            "☰",
            search_view.search_pane,
            theme.accent_color(),
        )
        .on_hover_text(t(lang, "tip_search_pane"))
        .clicked()
        {
            search_view.search_pane = !search_view.search_pane;
            ui.ctx().request_repaint();
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

        // Go to line (Ctrl+G): inline box, Enter jumps, Esc closes
        let ctrl_g = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::G);
        let goto_id = egui::Id::new("goto_line_input").with(&engine.path);
        if is_focused && ui.input_mut(|i| i.consume_shortcut(&ctrl_g)) {
            engine.goto_open = true;
            engine.goto_input.clear();
            engine.goto_notice = None;
            engine.cancel_goto_time();
            ui.ctx().memory_mut(|m| m.request_focus(goto_id));
        }
        if engine.goto_open {
            ui.separator();
            ui.label(
                RichText::new(format!("⇢ {}:", t(lang, "goto_label")))
                    .monospace()
                    .size(11.0)
                    .color(theme.accent_color()),
            );
            let resp = ui.add(
                egui::TextEdit::singleline(&mut engine.goto_input)
                    .hint_text(t(lang, "goto_hint"))
                    .desired_width(90.0)
                    .id(goto_id),
            );
            let enter = resp.has_focus()
                && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
            let esc = resp.has_focus()
                && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
            // Where to land: the target entered now, or a time jump that waited for the
            // background timing and has just been resolved.
            let mut landed = engine.take_goto_time_result();
            if enter {
                let current_line = engine.get_actual_line_idx(top_row(engine)).unwrap_or(0);
                match engine.resolve_goto(&engine.goto_input.clone(), current_line) {
                    Some(target) if target.waiting => engine.goto_notice = None,
                    result => landed = Some(result),
                }
            }
            match landed {
                Some(Some(target)) => {
                    engine.goto_notice = if target.hidden {
                        Some(format!(
                            "{} {} {}",
                            target.requested + 1,
                            t(lang, "goto_hidden"),
                            target.line + 1
                        ))
                    } else {
                        None
                    };
                    scroll_to_target(engine, target.line);
                    engine.select_row(target.line);
                    if !target.hidden {
                        engine.goto_open = false;
                        ui.ctx().memory_mut(|m| m.stop_text_input());
                    }
                    ui.ctx().request_repaint();
                }
                Some(None) => {
                    engine.goto_notice = Some(t(lang, "goto_invalid").to_string());
                }
                None => {}
            }
            if esc {
                // Drops a time jump still waiting; the timing itself goes on.
                engine.cancel_goto_time();
                engine.goto_open = false;
                engine.goto_notice = None;
                ui.ctx().memory_mut(|m| m.stop_text_input());
            }
            if engine.goto_time_waiting() {
                // The jump happens when the stream is timed: show how far that is.
                let waiting = match engine.scan_progress() {
                    Some((crate::scan_job::ScanKind::Timestamps, progress, _)) => format!(
                        "⏳ {} {:.0}%",
                        t(lang, "scan_timestamps"),
                        (progress * 100.0).clamp(0.0, 100.0)
                    ),
                    _ => format!("⏳ {}...", t(lang, "scan_timestamps")),
                };
                ui.label(
                    RichText::new(waiting)
                        .monospace()
                        .size(11.0)
                        .color(theme.warn_color()),
                );
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(100));
            } else if let Some(notice) = &engine.goto_notice {
                ui.label(
                    RichText::new(notice)
                        .monospace()
                        .size(11.0)
                        .color(theme.warn_color()),
                );
            }
        }

        // Export menu: visible lines, or the search matches, to a text file
        ui.menu_button("💾", |ui| {
            ui.set_max_width(260.0);
            let export = |engine: &TailEngine, title: &str, matches_only: bool| {
                let suggested = format!(
                    "{}-{}.txt",
                    engine
                        .path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("fasttail"),
                    if matches_only { "matches" } else { "export" }
                );
                if let Some(target) = rfd::FileDialog::new()
                    .set_title(title)
                    .set_file_name(suggested)
                    .add_filter("Text (*.txt, *.log)", &["txt", "log"])
                    .save_file()
                {
                    let result = create_export_file(&target).and_then(|f| {
                        let mut w = std::io::BufWriter::new(f);
                        if matches_only {
                            engine.export_search_matches(&mut w)
                        } else {
                            engine.export_visible(&mut w)
                        }
                    });
                    if let Err(err) = result {
                        eprintln!("fasttail: export to {} failed: {err}", target.display());
                    }
                }
            };
            if ui
                .button(RichText::new(t(lang, "export_visible")).monospace())
                .clicked()
            {
                export(engine, t(lang, "export_visible"), false);
                ui.close();
            }
            if has_query
                && ui
                    .button(RichText::new(t(lang, "export_matches")).monospace())
                    .clicked()
            {
                export(engine, t(lang, "export_matches"), true);
                ui.close();
            }
            if engine.has_bookmarks() {
                ui.separator();
                if ui
                    .button(RichText::new(t(lang, "clear_bookmarks")).monospace())
                    .clicked()
                {
                    engine.clear_bookmarks();
                    ui.close();
                }
            }
            // External tools on the current row (last clicked, search hit, or last line)
            if !external_tools.is_empty() {
                ui.separator();
                ui.label(
                    RichText::new(t(lang, "ext_tools_menu"))
                        .monospace()
                        .small()
                        .color(theme.text_dim()),
                );
                let mut run: Option<usize> = None;
                for (ti, tool) in external_tools.iter().enumerate() {
                    if ui
                        .button(RichText::new(format!("▶ {}", tool.name)).monospace())
                        .clicked()
                    {
                        run = Some(ti);
                        ui.close();
                    }
                }
                if let Some(ti) = run {
                    if let Some(row) = engine.current_row() {
                        run_tool_on_row(engine, &external_tools[ti], tool_runner, row, lang);
                    }
                }
            }
        })
        .response
        .on_hover_text(t(lang, "export_tip"));

        // Stream shortcuts of the focused stream: when no input has the keyboard, and also
        // while its search results pane has it (the pane is part of the stream's panel).
        let keyboard_free = !ui.ctx().egui_wants_keyboard_input();
        let pane_focused =
            crate::ui::hit_list::HitList::has_focus(ui.ctx(), search_pane_id(engine));
        if is_focused && (keyboard_free || pane_focused) {
            // Ctrl+A selects every visible row, Ctrl+C copies the selection (or the
            // current search hit) as plain text.
            let ctrl_a = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::A);
            let ctrl_c = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::C);
            if ui.input_mut(|i| i.consume_shortcut(&ctrl_a)) {
                engine.select_all_visible();
                ui.ctx().request_repaint();
            }
            if ui.input_mut(|i| i.consume_shortcut(&ctrl_c)) {
                if let Some(text) = engine.copy_selection_text() {
                    ui.ctx().copy_text(text);
                }
            }
            // Bookmarks: Ctrl+F2 toggles on the current row, F2 / Shift+F2 navigate.
            let ctrl_f2 = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::F2);
            let shift_f2 = egui::KeyboardShortcut::new(egui::Modifiers::SHIFT, egui::Key::F2);
            let plain_f2 = egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::F2);
            let current_row = || -> usize {
                let top = top_row(engine);
                engine
                    .selection
                    .iter()
                    .next()
                    .copied()
                    .or(engine.current_search_line())
                    .or_else(|| engine.get_actual_line_idx(top))
                    .unwrap_or(0)
            };
            if ui.input_mut(|i| i.consume_shortcut(&ctrl_f2)) {
                let row = current_row();
                engine.toggle_bookmark(row);
                ui.ctx().request_repaint();
            } else if ui.input_mut(|i| i.consume_shortcut(&shift_f2)) {
                let row = current_row();
                if let Some(target) = engine.bookmark_prev(row) {
                    scroll_to_target(engine, target);
                    engine.select_row(target);
                    ui.ctx().request_repaint();
                }
            } else if ui.input_mut(|i| i.consume_shortcut(&plain_f2)) {
                let row = current_row();
                if let Some(target) = engine.bookmark_next(row) {
                    scroll_to_target(engine, target);
                    engine.select_row(target);
                    ui.ctx().request_repaint();
                }
            }
        }
        // Main-view navigation keys: not while the results pane (or an input) has them.
        if is_focused && keyboard_free {
            ui.input(|i| {
                // Ctrl + Home: Jump to top
                if i.modifiers.ctrl && i.key_pressed(egui::Key::Home) {
                    scroll_top(engine);
                }
                // Ctrl + End: Jump to bottom & follow
                if i.modifiers.ctrl && i.key_pressed(egui::Key::End) {
                    scroll_bottom(engine);
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
                    scroll_lines(engine, -1);
                }
                if i.key_pressed(egui::Key::ArrowDown) {
                    scroll_lines(engine, 1);
                }
                if i.key_pressed(egui::Key::ArrowLeft) {
                    engine.requested_scroll_x = Some((engine.current_scroll_x - h_step).max(0.0));
                }
                if i.key_pressed(egui::Key::ArrowRight) {
                    engine.requested_scroll_x = Some(engine.current_scroll_x + h_step);
                }
                // PageUp / PageDown: screen-based paging
                if i.key_pressed(egui::Key::PageUp) {
                    scroll_pages(engine, -1);
                }
                if i.key_pressed(egui::Key::PageDown) {
                    scroll_pages(engine, 1);
                }
            });
        }
    });

    ui.separator();

    // If Hex streaming mode is active, render the binary hex stream (the results pane
    // lists line hits: in HEX view it only says so)
    if engine.view_mode == crate::tail_engine::ViewMode::Hex {
        if pane_visible {
            render_search_pane(
                ui,
                engine,
                theme,
                lang,
                font_size,
                level_colors,
                search_view,
            );
        }
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

        render_time_range(ui, engine, theme, lang);

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

        ui.separator();

        // Minimum log level selector (third filter stage) and the unknown-level toggle
        let level_active = engine.min_level != LogLevel::Unknown;
        let level_text = if level_active {
            RichText::new(format!("≥ {}", engine.min_level.name()))
                .monospace()
                .size(11.0)
                .strong()
                .color(theme.level_color(engine.min_level))
        } else {
            RichText::new(t(lang, "min_level_off"))
                .monospace()
                .size(11.0)
                .color(theme.text_dim())
        };
        let mut new_level: Option<LogLevel> = None;
        egui::ComboBox::from_id_salt(egui::Id::new("min_level").with(&engine.path))
            .selected_text(level_text)
            .width(110.0)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(!level_active, t(lang, "min_level_off"))
                    .clicked()
                {
                    new_level = Some(LogLevel::Unknown);
                }
                for level in LogLevel::ALL {
                    let label = RichText::new(format!("≥ {}", level.name()))
                        .monospace()
                        .color(theme.level_color(level));
                    if ui
                        .selectable_label(engine.min_level == level, label)
                        .clicked()
                    {
                        new_level = Some(level);
                    }
                }
            })
            .response
            .on_hover_text(t(lang, "min_level_tip"));
        if let Some(level) = new_level {
            engine.set_min_level(level);
        }
        if engine.min_level > LogLevel::Trace {
            let unknown_text = if engine.show_unknown_levels {
                RichText::new("?").strong().color(theme.accent_color())
            } else {
                RichText::new("?").color(theme.text_dim())
            };
            if ui
                .button(unknown_text)
                .on_hover_text(t(lang, "show_unknown_levels_tip"))
                .clicked()
            {
                let show = !engine.show_unknown_levels;
                engine.set_show_unknown_levels(show);
            }
        }
    });

    ui.separator();

    // Quick colour labels strip: one chip per label with a remove button.
    if !quick_labels.is_empty() {
        let mut remove = None;
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(format!("🏷 {}:", t(lang, "quick_labels")))
                    .monospace()
                    .size(11.0)
                    .color(theme.text_dim()),
            );
            for (i, label) in quick_labels.iter().enumerate() {
                let (fg, bg) = theme.label_style(label.color);
                ui.label(
                    RichText::new(format!(" {} {} ", label.color, label.text))
                        .monospace()
                        .size(11.0)
                        .color(fg)
                        .background_color(bg),
                );
                if ui
                    .small_button("✕")
                    .on_hover_text(t(lang, "tip_remove_label"))
                    .clicked()
                {
                    remove = Some(i);
                }
            }
        });
        if let Some(i) = remove {
            quick_labels.remove(i);
            *labels_changed = true;
            ui.ctx().request_repaint();
        }
        ui.separator();
    }

    // Search results pane: a resizable region at the bottom of the stream's panel, laid
    // out before the rows so they take what is left. A committed hit becomes the current
    // match and the main view centres it, exactly like F3.
    if pane_visible {
        let committed = render_search_pane(
            ui,
            engine,
            theme,
            lang,
            font_size,
            level_colors,
            search_view,
        );
        if let Some(target) = committed.and_then(|hit| engine.select_match(hit)) {
            scroll_to_target(engine, target);
            push_search_history(search_history, search_query);
            ui.ctx().request_repaint();
        }
    }

    let visible_lines = engine.visible_line_count();
    if visible_lines == 0 {
        // While the index or the filter is still being built on the worker, "empty" and
        // "nothing matches" would both be premature: say what is running instead.
        let building = engine
            .scan_progress()
            .map(|(kind, _, _)| kind)
            .filter(|kind| {
                matches!(
                    kind,
                    crate::scan_job::ScanKind::Index | crate::scan_job::ScanKind::Filter
                )
            });
        let empty_msg = if let Some(kind) = building {
            format!("⏳ {}...", t(lang, scan_kind_key(kind)))
        } else if engine.total_lines() == 0 {
            t(lang, "file_empty").to_string()
        } else {
            t(lang, "no_matching_lines").to_string()
        };
        ui.centered_and_justified(|ui| {
            ui.label(RichText::new(empty_msg).monospace().color(theme.text_dim()));
        });
        return;
    }

    let has_search = !engine.last_searched_query.is_empty();
    let active_search_line = engine.current_search_line();
    // The marker column appears when there is anything to mark: search hits or bookmarks.
    let show_markers = has_search || engine.has_bookmarks();
    let time_delta = time_delta_column(ui, engine, *time_delta);

    // Overview strip beside the scroll bar: the right edge of the rows area, when the
    // setting is on and there is anything to mark.
    // Clipped to what is on screen: a stream bar wider than the panel widens the ui
    // beyond its right edge, and the strip must stay visible.
    let area = ui.available_rect_before_wrap().intersect(ui.clip_rect());
    let strip_marks = if search_view.overview_strip {
        overview_marks(ui, engine, area.height())
            .filter(|marks| !marks.is_empty() || engine.current_search_line().is_some())
    } else {
        None
    };
    let rows_rect = if strip_marks.is_some() {
        area.with_max_x(area.max.x - crate::ui::overview_strip::STRIP_WIDTH)
    } else {
        area
    };

    let (row_click, toggle_json, tool_run) = ui
        .scope_builder(egui::UiBuilder::new().max_rect(rows_rect), |ui| {
            render_rows(
                ui,
                engine,
                theme,
                lang,
                font_size,
                row_height,
                *show_line_numbers,
                time_delta,
                show_markers,
                level_colors,
                has_search,
                active_search_line,
                external_tools,
            )
        })
        .inner;

    if let Some(marks) = strip_marks {
        let strip_rect =
            egui::Rect::from_min_max(egui::pos2(rows_rect.max.x, area.min.y), area.max);
        let per_row = if engine.wrap_lines {
            engine.wrap_avg_row_height.max(1.0)
        } else {
            row_height
        };
        let viewport = crate::ui::overview_strip::Viewport {
            top_row: top_row(engine),
            rows_on_screen: (rows_rect.height() / per_row).ceil() as usize,
        };
        if let Some(row) =
            crate::ui::overview_strip::paint(ui, strip_rect, engine, &marks, viewport, theme, lang)
        {
            if let Some(line) = engine.get_actual_line_idx(row) {
                scroll_to_target(engine, line);
                ui.ctx().request_repaint();
            }
        }
    }

    if let Some((tool_idx, row)) = tool_run {
        if let Some(tool) = external_tools.get(tool_idx) {
            run_tool_on_row(engine, tool, tool_runner, row, lang);
        }
    }

    if let Some((idx, mods)) = row_click {
        if mods.shift {
            engine.extend_selection_to(idx);
        } else if mods.ctrl || mods.command {
            engine.toggle_row(idx);
        } else {
            engine.select_row(idx);
        }
        ui.ctx().request_repaint();
    }

    if let Some((idx, was_expanded)) = toggle_json {
        if was_expanded {
            engine.expanded_json_lines.remove(&idx);
        } else {
            engine.expanded_json_lines.insert(idx);
        }
    }
}

/// The time delta column for this frame: `Some(gap_ms)` when it is shown. While the
/// column or a selection's elapsed time needs the stream timed, asks for it - never on the
/// UI thread for a large file, which a background scan times - and repaints until done.
fn time_delta_column(ui: &Ui, engine: &mut TailEngine, prefs: TimeDeltaPrefs) -> Option<u64> {
    let multi_select = engine.selection_all || engine.selection.len() >= 2;
    if (prefs.show || multi_select)
        && !engine.timestamps_complete()
        && !timestamps_unreadable(engine)
    {
        engine.want_timestamps();
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(100));
    }
    (prefs.show && engine.timestamps_usable()).then_some(prefs.gap_ms)
}

/// Whether enough of the stream has been timed to tell that we cannot read its times.
fn timestamps_unreadable(engine: &TailEngine) -> bool {
    engine
        .timestamp_rate()
        .is_some_and(|rate| rate < crate::tail_engine::MIN_TIMESTAMP_RATE)
}

/// Text and colour of a row's time delta cell, `None` when blank (continuation lines, no
/// known time): `…` until the row is timed, `⚓` on the anchor, and the accent colour on
/// a gap of at least `gap_ms` - in previous-row mode only, since large values are the
/// point of an anchor.
fn time_delta_cell(
    engine: &TailEngine,
    row: usize,
    gap_ms: u64,
    theme: &CyberTheme,
) -> Option<(String, Color32)> {
    let dim = theme.text_dim().gamma_multiply(0.8);
    match engine.row_time_delta(row) {
        TimeDelta::Blank => None,
        TimeDelta::Pending => Some(("…".to_string(), dim.gamma_multiply(0.6))),
        TimeDelta::Anchor => Some(("⚓".to_string(), theme.accent_color())),
        TimeDelta::Millis(millis) => {
            let gap = gap_ms > 0
                && engine.time_anchor().is_none()
                && millis >= i64::try_from(gap_ms).unwrap_or(i64::MAX);
            let color = if gap { theme.accent_color() } else { dim };
            Some((crate::timestamp::format_delta(millis), color))
        }
    }
}

/// The rows of the Text view, wrapped or not.
#[allow(clippy::too_many_arguments)]
fn render_rows(
    ui: &mut Ui,
    engine: &mut TailEngine,
    theme: &CyberTheme,
    lang: Language,
    font_size: f32,
    row_height: f32,
    show_line_numbers: bool,
    time_delta: Option<u64>,
    show_markers: bool,
    level_colors: bool,
    has_search: bool,
    active_search_line: Option<usize>,
    external_tools: &[ExternalTool],
) -> RowInteractions {
    if engine.wrap_lines {
        render_wrapped_rows(
            ui,
            engine,
            theme,
            lang,
            font_size,
            row_height,
            show_line_numbers,
            time_delta,
            show_markers,
            level_colors,
            has_search,
            active_search_line,
            external_tools,
        )
    } else {
        render_extended_rows(
            ui,
            engine,
            theme,
            lang,
            font_size,
            row_height,
            show_line_numbers,
            time_delta,
            show_markers,
            level_colors,
            has_search,
            active_search_line,
            external_tools,
        )
    }
}

/// Marks of the overview strip for a strip `height` points tall, from the per-stream cache
/// in egui memory (rebuilt only when its inputs change).
fn overview_marks(
    ui: &Ui,
    engine: &TailEngine,
    height: f32,
) -> Option<crate::ui::overview_strip::Marks> {
    use crate::ui::overview_strip::StripCache;
    let height = height.round().max(0.0) as usize;
    if height == 0 {
        return None;
    }
    let id = egui::Id::new("overview_cache").with(&engine.path);
    let mut cache: StripCache = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    if let Some(wait) = cache.refresh(engine, height) {
        ui.ctx().request_repaint_after(wait);
    }
    let marks = cache.marks.clone();
    ui.data_mut(|d| d.insert_temp(id, cache));
    Some(marks)
}

/// The search results pane of a stream: a header with the query, the true total, the
/// capped note and the search progress, then the virtualized hit list (see `hit_list`),
/// or a notice in HEX view. Returns the hit committed by a click or `Enter`.
fn render_search_pane(
    ui: &mut Ui,
    engine: &TailEngine,
    theme: &CyberTheme,
    lang: Language,
    font_size: f32,
    level_colors: bool,
    search_view: &mut SearchViewPrefs,
) -> Option<usize> {
    use crate::ui::hit_list::HitList;
    let id = search_pane_id(engine);
    let panel_id = id.with("panel");
    let max_height = (ui.available_height() - MIN_ROWS_HEIGHT).max(MIN_SEARCH_PANE_HEIGHT);
    let height = search_view
        .search_pane_height
        .clamp(MIN_SEARCH_PANE_HEIGHT, max_height);
    // The height is one preference for every stream: hand it to the panel each frame, and
    // take the panel's back while the user drags its edge.
    ui.ctx().data_mut(|d| {
        d.insert_persisted(
            panel_id,
            egui::containers::panel::PanelState {
                outer_rect: egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(ui.available_width(), height),
                ),
            },
        )
    });
    let mut committed = None;
    let panel = egui::Panel::bottom(panel_id)
        .resizable(true)
        .size_range(MIN_SEARCH_PANE_HEIGHT..=max_height)
        .frame(
            egui::Frame::NONE
                .fill(theme.panel_bg())
                .inner_margin(egui::Margin::symmetric(4, 2)),
        )
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!(
                        "☰ {} \"{}\": {}",
                        t(lang, "search_pane_header"),
                        engine.last_searched_query,
                        group_thousands(engine.search_total())
                    ))
                    .monospace()
                    .size(11.0)
                    .color(theme.accent_color()),
                );
                if engine.search_capped() {
                    ui.label(
                        RichText::new(format!("({})", capped_note(lang)))
                            .monospace()
                            .size(11.0)
                            .color(theme.warn_color()),
                    );
                }
                if let Some((crate::scan_job::ScanKind::Search, progress, _)) =
                    engine.scan_progress()
                {
                    ui.label(
                        RichText::new(format!(
                            "⏳ {} {:.0}%",
                            t(lang, "scan_searching"),
                            (progress * 100.0).clamp(0.0, 100.0)
                        ))
                        .monospace()
                        .size(11.0)
                        .color(theme.warn_color()),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button("✖")
                        .on_hover_text(t(lang, "tip_search_pane"))
                        .clicked()
                    {
                        search_view.search_pane = false;
                    }
                });
            });
            if engine.view_mode == crate::tail_engine::ViewMode::Hex {
                ui.centered_and_justified(|ui| {
                    ui.label(
                        RichText::new(t(lang, "search_pane_hex"))
                            .monospace()
                            .color(theme.text_dim()),
                    );
                });
                return;
            }
            let output = HitList::new(
                id,
                &engine.search_matches,
                engine.current_match_idx,
                &engine.last_searched_query,
            )
            .font_size(font_size)
            .level_colors(level_colors)
            .show(ui, engine, theme, lang);
            committed = output.committed;
        });
    let resize = ui.ctx().read_response(panel_id.with("__resize"));
    if resize.is_some_and(|r| r.dragged() || r.drag_stopped()) {
        search_view.search_pane_height = panel.response.rect.height();
    }
    committed
}

/// Text view in extend mode: every row has the same height, so the scroll offset maps to
/// a line index arithmetically and `show_rows` lays out only the visible range.
#[allow(clippy::too_many_arguments)]
fn render_extended_rows(
    ui: &mut Ui,
    engine: &mut TailEngine,
    theme: &CyberTheme,
    lang: Language,
    font_size: f32,
    row_height: f32,
    show_line_numbers: bool,
    time_delta: Option<u64>,
    show_markers: bool,
    level_colors: bool,
    has_search: bool,
    active_search_line: Option<usize>,
    external_tools: &[ExternalTool],
) -> RowInteractions {
    let mut toggle_json = None;
    let mut row_click: Option<(usize, egui::Modifiers)> = None;
    let mut tool_run: Option<(usize, usize)> = None;
    let mut anchor_pick: Option<usize> = None;
    let mut clear_scroll_to_line = false;
    let mut max_row_natural_width = 0.0_f32;
    let visible_lines = engine.visible_line_count();
    let font_id = egui::FontId::monospace(font_size);
    let char_w = ui.ctx().fonts_mut(|f| f.glyph_width(&font_id, '0'));
    let delta_w = char_w * TIME_DELTA_CHARS;
    let span_rules = engine.has_span_rules();
    let scroll_content_width = engine.max_detected_width.max(HORIZONTAL_SCROLL_EXTENT);
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
        ui.set_min_width(scroll_content_width);
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
        ui.spacing_mut().item_spacing.y = 0.0;
        for row_idx in row_range {
            let actual_line_idx = match engine.get_actual_line_idx(row_idx) {
                Some(idx) => idx,
                None => continue,
            };

            if let Some(row) = engine.get_row(actual_line_idx) {
                let raw_line: &str = &row.line;
                let is_json = TailEngine::is_json_line(raw_line);
                let is_expanded = engine.expanded_json_lines.contains(&actual_line_idx);
                let matches_search = has_search
                    && engine
                        .search_matches
                        .binary_search(&actual_line_idx)
                        .is_ok();
                let is_active_search = active_search_line == Some(actual_line_idx);
                // User rules first; the level palette only colours rows no rule matched.
                // Span rules (captures-only, quick labels, ANSI colours) only run when one
                // exists, and never on search hits, which keep their own colours.
                let spans =
                    if (span_rules || !row.ansi.is_empty()) && !matches_search && !is_active_search
                    {
                        Some(engine.match_row_spans(&row))
                    } else {
                        None
                    };
                let highlight = match &spans {
                    Some(s) => s
                        .rest
                        .or_else(|| level_fallback(engine, theme, level_colors, actual_line_idx)),
                    None => row_highlight(engine, theme, level_colors, actual_line_idx, raw_line),
                };
                // Raw mode draws ESC as ␛, with the spans moved past the wider glyphs.
                let (shown, spans) = row.display(spans);
                let is_selected = engine.is_selected(actual_line_idx);
                let is_bookmarked = engine.is_bookmarked(actual_line_idx);

                // Background painted after layout, behind the row (see SearchRowMark)
                let row_bg = ui.painter().add(egui::Shape::Noop);
                let row_resp = ui
                    .horizontal(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.set_min_height(row_height);
                        ui.set_max_height(row_height);

                        // Search marker column: ▶ current hit, ● other hits, blank otherwise
                        if show_markers {
                            search_marker_label(
                                ui,
                                theme,
                                font_size,
                                matches_search,
                                is_active_search,
                                is_bookmarked,
                            );
                        }

                        // Line number
                        if show_line_numbers {
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

                        // Time delta, right-aligned in a fixed-width cell
                        if let Some(gap_ms) = time_delta {
                            let (cell, _) = ui.allocate_exact_size(
                                egui::vec2(delta_w, row_height),
                                egui::Sense::hover(),
                            );
                            if let Some((text, color)) =
                                time_delta_cell(engine, row_idx, gap_ms, theme)
                            {
                                ui.painter().text(
                                    egui::pos2(cell.right() - char_w, cell.center().y),
                                    egui::Align2::RIGHT_CENTER,
                                    text,
                                    font_id.clone(),
                                    color,
                                );
                            }
                        }

                        // JSON toggle button
                        if is_json {
                            let btn_label = if is_expanded { "[-] JSON" } else { "[+] JSON" };
                            let tooltip = if is_expanded {
                                t(lang, "json_collapse")
                            } else {
                                t(lang, "json_expand")
                            };
                            let btn = ui
                                .button(
                                    RichText::new(btn_label)
                                        .monospace()
                                        .size((font_size - 2.0).max(9.0))
                                        .color(theme.secondary_accent()),
                                )
                                .on_hover_text(tooltip);
                            if btn.clicked() {
                                toggle_json = Some((actual_line_idx, is_expanded));
                            }
                        }

                        // Content text: per-span formats when a captures-only rule or a
                        // quick label painted something, the plain label otherwise.
                        if let Some(spans) = spans.as_ref().filter(|s| !s.spans.is_empty()) {
                            let base = egui::TextFormat {
                                font_id: font_id.clone(),
                                color: highlight.map(|h| h.fg).unwrap_or(theme.text_primary()),
                                background: highlight.map(|h| h.bg).unwrap_or(Color32::TRANSPARENT),
                                italics: highlight.map(|h| h.italic).unwrap_or(false),
                                ..Default::default()
                            };
                            let job = span_layout_job(&shown, &font_id, base, &spans.spans, theme);
                            ui.add(egui::Label::new(job).wrap_mode(egui::TextWrapMode::Extend));
                            return;
                        }
                        let mut text = RichText::new(&*shown).monospace().size(font_size);
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
                max_row_natural_width = max_row_natural_width.max(row_resp.rect.width());
                paint_search_row_background(
                    ui,
                    row_bg,
                    row_resp.rect,
                    theme,
                    matches_search,
                    is_active_search,
                    is_selected,
                    is_bookmarked,
                );

                // Row selection: click, Shift+click (range), Ctrl+click (toggle)
                let click_rect = egui::Rect::from_min_max(
                    egui::pos2(ui.max_rect().left(), row_resp.rect.top()),
                    egui::pos2(
                        ui.max_rect().right().max(row_resp.rect.right()),
                        row_resp.rect.bottom(),
                    ),
                );
                let click = ui.interact(
                    click_rect,
                    ui.id().with(("row_select", actual_line_idx)),
                    egui::Sense::click(),
                );
                if click.clicked() {
                    row_click = Some((actual_line_idx, ui.input(|i| i.modifiers)));
                }
                row_context_menu(
                    &click,
                    actual_line_idx,
                    lang,
                    time_delta.map(|_| engine.time_anchor()),
                    external_tools,
                    &mut tool_run,
                    &mut anchor_pick,
                );

                if is_active_search && engine.scroll_to_line == Some(actual_line_idx) {
                    row_resp.scroll_to_me(Some(egui::Align::Center));
                    clear_scroll_to_line = true;
                }

                // Render expanded pretty JSON
                if is_json && is_expanded {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(raw_line) {
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
    if let Some(line) = anchor_pick {
        engine.toggle_time_anchor(line);
    }
    if max_row_natural_width > engine.max_detected_width {
        engine.max_detected_width = max_row_natural_width;
    }
    engine.current_scroll_x = scroll_output.state.offset.x;
    engine.current_scroll_y = scroll_output.state.offset.y;
    (row_click, toggle_json, tool_run)
}

/// What the user did on the rows this frame: a row click with its modifiers, a JSON
/// expander toggle `(line, was_expanded)`, and an external tool picked from the row
/// context menu `(tool index, line)`.
type RowInteractions = (
    Option<(usize, egui::Modifiers)>,
    Option<(usize, bool)>,
    Option<(usize, usize)>,
);

/// Row context menu: the time anchor entries while the time delta column is shown
/// (`anchor` is `Some(current anchor)` then), recorded in `anchor_pick` as the line to
/// toggle, and the external tools, recorded in `tool_run`.
#[allow(clippy::too_many_arguments)]
fn row_context_menu(
    click: &egui::Response,
    line: usize,
    lang: Language,
    anchor: Option<Option<usize>>,
    tools: &[ExternalTool],
    tool_run: &mut Option<(usize, usize)>,
    anchor_pick: &mut Option<usize>,
) {
    if tools.is_empty() && anchor.is_none() {
        return;
    }
    click.context_menu(|ui| {
        ui.set_min_width(160.0);
        if let Some(current) = anchor {
            if current != Some(line)
                && ui
                    .button(RichText::new(format!("⚓ {}", t(lang, "time_anchor_set"))).monospace())
                    .clicked()
            {
                *anchor_pick = Some(line);
                ui.close();
            }
            if let Some(current) = current {
                if ui
                    .button(RichText::new(t(lang, "time_anchor_clear")).monospace())
                    .clicked()
                {
                    // Toggling the anchor line clears it.
                    *anchor_pick = Some(current);
                    ui.close();
                }
            }
            if !tools.is_empty() {
                ui.separator();
            }
        }
        for (ti, tool) in tools.iter().enumerate() {
            if ui
                .button(RichText::new(format!("▶ {}", tool.name)).monospace())
                .clicked()
            {
                *tool_run = Some((ti, line));
                ui.close();
            }
        }
    });
}

/// Level-palette fallback for a row no user rule matched (user rules keep priority).
fn row_highlight(
    engine: &TailEngine,
    theme: &CyberTheme,
    level_colors: bool,
    line_idx: usize,
    raw_line: &str,
) -> Option<HighlightStyle> {
    engine
        .match_highlight(raw_line)
        .or_else(|| level_fallback(engine, theme, level_colors, line_idx))
}

/// The level palette entry of a row, when level colouring is on.
fn level_fallback(
    engine: &TailEngine,
    theme: &CyberTheme,
    level_colors: bool,
    line_idx: usize,
) -> Option<HighlightStyle> {
    if !level_colors {
        return None;
    }
    theme
        .level_style(engine.level_of(line_idx))
        .map(|s| HighlightStyle {
            fg: s.fg,
            bg: s.bg,
            bold: s.bold,
            italic: false,
        })
}

/// Lays out `text` as sections: `base` for the bytes no span claimed, and per span the
/// rule's colours, the theme's preset for a quick label, or the theme's reading of an
/// ANSI style (drawn over the row's own colours, with underline). Spans are sorted and
/// non-overlapping (see `TailEngine::match_highlight_spans`); ranges past the end of the
/// (possibly capped) text are dropped.
fn span_layout_job(
    text: &str,
    font_id: &egui::FontId,
    base: egui::TextFormat,
    spans: &[HighlightSpan],
    theme: &CyberTheme,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let mut pos = 0;
    for sp in spans {
        let start = sp.start.min(text.len());
        let end = sp.end.min(text.len());
        if start < pos
            || start >= end
            || !text.is_char_boundary(start)
            || !text.is_char_boundary(end)
        {
            continue;
        }
        if start > pos {
            job.append(&text[pos..start], 0.0, base.clone());
        }
        let (fg, bg, italics, underline) = match sp.style {
            SpanStyle::Rule(s) => (s.fg, s.bg, s.italic, false),
            SpanStyle::Label(n) => {
                let (fg, bg) = theme.label_style(n);
                (fg, bg, false, false)
            }
            SpanStyle::Ansi(s) => {
                // The wrapped view leaves the row colour to the painter (placeholder).
                let base_fg = if base.color == Color32::PLACEHOLDER {
                    theme.text_primary()
                } else {
                    base.color
                };
                let (fg, bg) = theme.ansi_colors(&s, base_fg, base.background);
                let plain = s.fg.is_none() && !s.bold && !s.dim && !s.inverse && s.bg.is_none();
                let fg = if plain { base.color } else { fg };
                (fg, bg, s.italic || base.italics, s.underline)
            }
        };
        job.append(
            &text[start..end],
            0.0,
            egui::TextFormat {
                font_id: font_id.clone(),
                color: fg,
                background: bg,
                italics,
                underline: if underline {
                    Stroke::new(
                        1.0,
                        if fg == Color32::PLACEHOLDER {
                            theme.text_primary()
                        } else {
                            fg
                        },
                    )
                } else {
                    Stroke::NONE
                },
                ..Default::default()
            },
        );
        pos = end;
    }
    if pos < text.len() {
        job.append(&text[pos..], 0.0, base);
    }
    job
}

/// A row laid out for the wrapped view: its line, galleys and total height.
struct WrappedRow {
    line: usize,
    galley: std::sync::Arc<egui::Galley>,
    pretty: Option<std::sync::Arc<egui::Galley>>,
    height: f32,
    is_json: bool,
    expanded: bool,
    highlight: Option<HighlightStyle>,
}

/// Text view in wrap mode: rows soft-wrap at the viewport width and have their own
/// heights, so the viewport is anchored to a row (`engine.wrap_anchor`, see
/// `wrap_layout`) instead of being mapped arithmetically. Only the rows in view are laid
/// out; the scroll bar sees an estimated total height (rows × average row height) and an
/// offset derived from the anchor, which makes its thumb approximate on files with very
/// uneven line lengths while scrolling by line stays exact.
#[allow(clippy::too_many_arguments)]
fn render_wrapped_rows(
    ui: &mut Ui,
    engine: &mut TailEngine,
    theme: &CyberTheme,
    lang: Language,
    font_size: f32,
    row_height: f32,
    show_line_numbers: bool,
    time_delta: Option<u64>,
    show_markers: bool,
    level_colors: bool,
    has_search: bool,
    active_search_line: Option<usize>,
    external_tools: &[ExternalTool],
) -> RowInteractions {
    use std::collections::HashMap;

    let ctx = ui.ctx().clone();
    let font_id = egui::FontId::monospace(font_size);
    let font_row_h = ctx.fonts_mut(|f| f.row_height(&font_id));
    let char_w = ctx.fonts_mut(|f| f.glyph_width(&font_id, '0'));
    let pad = (row_height - font_row_h).max(0.0);
    let rows = engine.visible_line_count();
    if engine.wrap_avg_row_height <= 0.0 {
        engine.wrap_avg_row_height = row_height;
    }
    let span_rules = engine.has_span_rules();

    let mut scroll_area = ScrollArea::vertical().auto_shrink([false, false]);
    let handed = engine.wrap_virtual_offset;
    if let Some(v) = handed {
        scroll_area = scroll_area.vertical_scroll_offset(v);
    }
    engine.requested_scroll_y = None;

    let mut row_click: Option<(usize, egui::Modifiers)> = None;
    let mut toggle_json: Option<(usize, bool)> = None;
    let mut tool_run: Option<(usize, usize)> = None;
    let mut anchor_pick: Option<usize> = None;

    let output = scroll_area.show_viewport(ui, |ui, viewport| {
        let origin = ui.max_rect().min;
        let content_w = ui.available_width().max(50.0);
        let vh = viewport.height().max(1.0);
        let top = origin.y + viewport.min.y;
        let left_pad = 4.0;
        let marker_w = if show_markers { char_w * 2.0 } else { 0.0 };
        let num_w = if show_line_numbers { char_w * 9.0 } else { 0.0 };
        let delta_w = if time_delta.is_some() {
            char_w * TIME_DELTA_CHARS
        } else {
            0.0
        };
        let text_x = origin.x + left_pad + marker_w + num_w + delta_w;
        let json_w = char_w * 9.0;
        let text_w = (origin.x + content_w - text_x - left_pad).max(40.0);
        let pretty_font = egui::FontId::monospace(11.0);

        // Rows laid out this frame, keyed by visible row. Heights come from the real
        // wrapped galleys; the layout is capped to `WRAP_LAYOUT_CAP` bytes per row.
        let eng: &TailEngine = engine;
        let mut laid: HashMap<usize, WrappedRow> = HashMap::new();
        let mut measure = |row: usize| -> f32 {
            if let Some(r) = laid.get(&row) {
                return r.height;
            }
            let Some(line) = eng.get_actual_line_idx(row) else {
                return row_height;
            };
            let Some(row_text) = eng.get_row(line) else {
                return row_height;
            };
            let raw: &str = &row_text.line;
            let is_json = TailEngine::is_json_line(raw);
            let expanded = is_json && eng.expanded_json_lines.contains(&line);
            // Search hits keep their own colours; other rows take the span path only when
            // a captures-only rule, a quick label or ANSI colours exist.
            let is_hit = active_search_line == Some(line)
                || (has_search && eng.search_matches.binary_search(&line).is_ok());
            let spans = if (span_rules || !row_text.ansi.is_empty()) && !is_hit {
                Some(eng.match_row_spans(&row_text))
            } else {
                None
            };
            let highlight = match &spans {
                Some(s) => s
                    .rest
                    .or_else(|| level_fallback(eng, theme, level_colors, line)),
                None => row_highlight(eng, theme, level_colors, line, raw),
            };
            let (shown, spans) = row_text.display(spans);
            let format = egui::TextFormat {
                font_id: font_id.clone(),
                color: Color32::PLACEHOLDER,
                italics: highlight.map(|h| h.italic).unwrap_or(false),
                ..Default::default()
            };
            let mut job = match spans.filter(|s| !s.spans.is_empty()) {
                Some(s) => span_layout_job(layout_slice(&shown), &font_id, format, &s.spans, theme),
                None => {
                    egui::text::LayoutJob::single_section(layout_slice(&shown).to_owned(), format)
                }
            };
            job.wrap.max_width = if is_json { text_w - json_w } else { text_w }.max(20.0);
            let galley = ctx.fonts_mut(|f| f.layout_job(job));
            let mut height = galley.size().y.max(font_row_h) + pad;
            let pretty = if expanded {
                serde_json::from_str::<serde_json::Value>(raw)
                    .ok()
                    .and_then(|v| serde_json::to_string_pretty(&v).ok())
                    .map(|pretty| {
                        let format = egui::TextFormat {
                            font_id: pretty_font.clone(),
                            color: Color32::PLACEHOLDER,
                            ..Default::default()
                        };
                        let mut job = egui::text::LayoutJob::single_section(pretty, format);
                        job.wrap.max_width = (text_w - 12.0).max(20.0);
                        let g = ctx.fonts_mut(|f| f.layout_job(job));
                        height += g.size().y + 16.0;
                        g
                    })
            } else {
                None
            };
            laid.insert(
                row,
                WrappedRow {
                    line,
                    galley,
                    pretty,
                    height,
                    is_json,
                    expanded,
                    highlight,
                },
            );
            height
        };

        // 1. Resolve the anchor: pending request, then the user's scrolling during the
        //    last frame (measured below against the offset handed to the scroll area),
        //    then follow mode.
        let avg = eng.wrap_avg_row_height;
        let prev_offset = eng.wrap_virtual_offset;
        let delta = eng.wrap_scroll_delta;
        let mut anchor = eng.wrap_anchor;
        anchor.row = anchor.row.min(rows.saturating_sub(1));
        let mut at_bottom = eng.wrap_at_bottom;
        if let Some(request) = eng.wrap_request {
            at_bottom = false;
            anchor = match request {
                WrapScroll::Top => WrapAnchor::TOP,
                WrapScroll::Bottom => {
                    at_bottom = true;
                    anchor_to_bottom(rows, vh, &mut measure)
                }
                WrapScroll::Lines(n) => WrapAnchor {
                    row: (anchor.row as i64 + n).clamp(0, rows.saturating_sub(1) as i64) as usize,
                    within: 0.0,
                },
                WrapScroll::Pages(n) => walk_anchor(anchor, n as f32 * vh, rows, &mut measure),
                WrapScroll::CenterLine(line) => {
                    // A line hidden by the filters resolves to the next visible row.
                    let row = if eng.is_filter_active() {
                        eng.filtered_lines.partition_point(|&l| l < line)
                    } else {
                        line
                    };
                    anchor_center(row, rows, vh, &mut measure)
                }
            };
        }
        if delta.abs() > 0.5 {
            at_bottom = false;
            anchor = if delta.abs() > 4.0 * vh {
                // Scroll bar dragged far: jump through the estimate instead of walking.
                let row = (eng.wrap_scroll_abs / avg.max(1.0)).floor();
                WrapAnchor {
                    row: (row.max(0.0) as usize).min(rows.saturating_sub(1)),
                    within: 0.0,
                }
            } else {
                walk_anchor(anchor, delta, rows, &mut measure)
            };
        }
        if eng.follow_tail && at_bottom {
            anchor = anchor_to_bottom(rows, vh, &mut measure);
        }

        // 2. Lay out the rows in view; when they run out before the viewport is full, pull
        //    the last row to the bottom edge instead of leaving a gap.
        let (mut count, mut covered) = fill_from(anchor, rows, vh, &mut measure);
        if covered < vh && anchor.row + count >= rows && anchor != WrapAnchor::TOP {
            anchor = anchor_to_bottom(rows, vh, &mut measure);
            (count, covered) = fill_from(anchor, rows, vh, &mut measure);
        }
        let reached_end = anchor.row + count >= rows && covered <= vh + 0.5;

        // 3. Estimated total height for the scroll bar, refreshed from the rows measured
        //    this frame, and the offset that represents the anchor on that scale.
        let mean = if count > 0 {
            (anchor.row..anchor.row + count)
                .map(|r| laid.get(&r).map(|l| l.height).unwrap_or(row_height))
                .sum::<f32>()
                / count as f32
        } else {
            avg
        };
        let new_avg = if prev_offset.is_none() {
            mean
        } else {
            avg * 0.8 + mean * 0.2
        }
        .max(1.0);
        let total_h = (rows as f32 * new_avg).max(vh);
        let max_offset = (total_h - vh).max(0.0);
        let virtual_offset = if reached_end {
            max_offset
        } else {
            (anchor.row as f32 * new_avg + anchor.within).clamp(0.0, max_offset)
        };
        ui.allocate_rect(
            egui::Rect::from_min_size(origin, egui::vec2(content_w, total_h)),
            egui::Sense::hover(),
        );

        // 4. Paint the rows from the anchor: tints and gutter first, the text galley, the
        //    expanded JSON below it, then the click targets.
        let painter = ui.painter().clone();
        let mut y = top - anchor.within;
        for row in anchor.row..anchor.row + count {
            let Some(r) = laid.get(&row) else {
                continue;
            };
            let line = r.line;
            let row_rect = egui::Rect::from_min_max(
                egui::pos2(origin.x, y),
                egui::pos2(origin.x + content_w, y + r.height),
            );
            let matches_search = has_search && eng.search_matches.binary_search(&line).is_ok();
            let is_active = active_search_line == Some(line);
            let is_selected = eng.is_selected(line);
            let is_bookmarked = eng.is_bookmarked(line);
            if let Some(fill) =
                row_tint(theme, matches_search, is_active, is_selected, is_bookmarked)
            {
                painter.rect_filled(row_rect, 0.0, fill);
            }
            let text_top = y + pad / 2.0;
            if show_markers {
                let (glyph, color) = marker_glyph(theme, matches_search, is_active, is_bookmarked);
                painter.text(
                    egui::pos2(origin.x + left_pad, text_top),
                    egui::Align2::LEFT_TOP,
                    glyph,
                    font_id.clone(),
                    color,
                );
            }
            if show_line_numbers {
                let num_color = if is_active {
                    theme.accent_color()
                } else {
                    theme.text_dim().gamma_multiply(0.6)
                };
                painter.text(
                    egui::pos2(origin.x + left_pad + marker_w, text_top),
                    egui::Align2::LEFT_TOP,
                    format!("{:>6} │", line + 1),
                    font_id.clone(),
                    num_color,
                );
            }
            if let Some((text, color)) =
                time_delta.and_then(|gap_ms| time_delta_cell(eng, row, gap_ms, theme))
            {
                painter.text(
                    egui::pos2(text_x - char_w, text_top),
                    egui::Align2::RIGHT_TOP,
                    text,
                    font_id.clone(),
                    color,
                );
            }
            let (color, text_bg) = if is_active {
                (Color32::BLACK, Some(SEARCH_ACTIVE_BG))
            } else if matches_search {
                (Color32::BLACK, Some(SEARCH_MATCH_BG))
            } else if let Some(hl) = r.highlight {
                (hl.fg, Some(hl.bg).filter(|c| c.a() > 0))
            } else {
                (theme.text_primary(), None)
            };
            let text_pos = egui::pos2(text_x + if r.is_json { json_w } else { 0.0 }, text_top);
            if let Some(bg) = text_bg {
                painter.rect_filled(
                    egui::Rect::from_min_size(text_pos, r.galley.size()),
                    0.0,
                    bg,
                );
            }
            painter.galley(text_pos, r.galley.clone(), color);
            if let Some(pretty) = &r.pretty {
                let frame = egui::Rect::from_min_size(
                    egui::pos2(text_x, text_top + r.galley.size().y + 4.0),
                    egui::vec2(text_w, pretty.size().y + 12.0),
                );
                painter.rect(
                    frame,
                    0.0,
                    theme.panel_bg().linear_multiply(1.3),
                    Stroke::new(1.0_f32, theme.border_color().gamma_multiply(0.4)),
                    egui::StrokeKind::Inside,
                );
                painter.galley(
                    frame.min + egui::vec2(6.0, 6.0),
                    pretty.clone(),
                    theme.secondary_accent(),
                );
            }

            // Row selection: click, Shift+click (range), Ctrl+click (toggle)
            let click = ui.interact(
                row_rect,
                ui.id().with(("row_select", line)),
                egui::Sense::click(),
            );
            if click.clicked() {
                row_click = Some((line, ui.input(|i| i.modifiers)));
            }
            row_context_menu(
                &click,
                line,
                lang,
                time_delta.map(|_| eng.time_anchor()),
                external_tools,
                &mut tool_run,
                &mut anchor_pick,
            );
            // JSON toggle, registered after the row so it wins the click
            if r.is_json {
                let btn_rect = egui::Rect::from_min_size(
                    egui::pos2(text_x, text_top),
                    egui::vec2(json_w - char_w, font_row_h),
                );
                let tooltip = if r.expanded {
                    t(lang, "json_collapse")
                } else {
                    t(lang, "json_expand")
                };
                let btn = ui
                    .interact(
                        btn_rect,
                        ui.id().with(("wrap_json", line)),
                        egui::Sense::click(),
                    )
                    .on_hover_text(tooltip);
                let label = if r.expanded { "[-] JSON" } else { "[+] JSON" };
                painter.text(
                    btn_rect.min,
                    egui::Align2::LEFT_TOP,
                    label,
                    egui::FontId::monospace((font_size - 2.0).max(9.0)),
                    theme.secondary_accent(),
                );
                if btn.clicked() {
                    toggle_json = Some((line, r.expanded));
                }
            }
            y += r.height;
        }

        engine.wrap_anchor = anchor;
        engine.wrap_at_bottom = reached_end;
        engine.wrap_avg_row_height = new_avg;
        engine.wrap_virtual_offset = Some(virtual_offset);
        engine.wrap_request = None;
    });
    // User scrolling this frame: the scroll area started from `handed` (clamped to its
    // range like egui does) and ended on `state.offset`. Applied to the anchor next frame.
    let max_offset = (output.content_size.y - output.inner_rect.height()).max(0.0);
    let ended = output.state.offset.y;
    engine.wrap_scroll_delta = handed
        .map(|h| ended - h.clamp(0.0, max_offset))
        .unwrap_or(0.0);
    engine.wrap_scroll_abs = ended;
    engine.current_scroll_x = 0.0;
    engine.current_scroll_y = ended;
    if let Some(line) = anchor_pick {
        engine.toggle_time_anchor(line);
    }
    (row_click, toggle_json, tool_run)
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
                RichText::new(t(lang, "file_empty"))
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
    let scroll_content_width = engine.max_detected_width.max(HORIZONTAL_SCROLL_EXTENT);
    ScrollArea::horizontal()
        .id_salt("hex_header_scroll")
        .auto_shrink([false, true])
        .max_height(header_height)
        .horizontal_scroll_offset(engine.current_scroll_x)
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .show(ui, |ui| {
            ui.set_min_width(scroll_content_width);
            ui.horizontal(|ui| {
                if has_search {
                    // Keep the header aligned with the marker column of the rows
                    search_marker_label(ui, theme, font_size, false, false, false);
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
    let mut max_row_natural_width = 0.0_f32;
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
        ui.set_min_width(scroll_content_width);
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
                        search_marker_label(
                            ui,
                            theme,
                            font_size,
                            matches_search,
                            is_active_search,
                            false,
                        );
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
                    for &b in &chunk {
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
            max_row_natural_width = max_row_natural_width.max(row_resp.rect.width());
            paint_search_row_background(
                ui,
                row_bg,
                row_resp.rect,
                theme,
                matches_search,
                is_active_search,
                false,
                false,
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
    if max_row_natural_width > engine.max_detected_width {
        engine.max_detected_width = max_row_natural_width;
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
                    if rule.is_regex
                        && ui
                            .checkbox(&mut rule.captures_only, t(lang, "captures_only"))
                            .on_hover_text(t(lang, "tip_captures_only"))
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
/// PIN lock section of the settings: the master switch (lock when the screensaver ends),
/// the PIN editor and a "lock now" button. The typed PIN never leaves this function in
/// clear — only its scrambled form is stored (see `crate::config::scramble_pin`).
fn render_lock_settings(
    ui: &mut Ui,
    lang: Language,
    lock_enabled: &mut bool,
    lock_pin: &mut String,
    lock_now: &mut bool,
) {
    ui.label(
        RichText::new(format!("🔒 {}", t(lang, "lock_section")))
            .monospace()
            .strong(),
    );
    ui.checkbox(lock_enabled, t(lang, "lock_enable"));

    let draft_id = ui.id().with("lock_pin_draft");
    let mut draft: String = ui.data_mut(|d| d.get_temp(draft_id).unwrap_or_default());
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{}:", t(lang, "lock_pin"))).monospace());
        let field = ui.add(
            egui::TextEdit::singleline(&mut draft)
                .password(true)
                .desired_width(90.0)
                .hint_text(t(lang, "lock_pin_hint")),
        );
        let valid = crate::config::is_valid_pin(&draft);
        // Enter confirms the PIN, like the button next to it.
        let entered = valid && field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if entered
            || ui
                .add_enabled(valid, egui::Button::new(t(lang, "lock_save_pin")))
                .on_disabled_hover_text(t(lang, "lock_pin_hint"))
                .clicked()
        {
            *lock_pin = crate::config::scramble_pin(draft.trim());
            draft.clear();
        }
        if ui
            .add_enabled(
                !lock_pin.is_empty(),
                egui::Button::new(t(lang, "lock_clear_pin")),
            )
            .clicked()
        {
            lock_pin.clear();
            *lock_enabled = false;
            draft.clear();
        }
    });
    ui.data_mut(|d| d.insert_temp(draft_id, draft));

    if ui
        .add_enabled(!lock_pin.is_empty(), egui::Button::new(t(lang, "lock_now")))
        .on_hover_text(t(lang, "lock_now_tip"))
        .on_disabled_hover_text(t(lang, "lock_needs_pin"))
        .clicked()
    {
        *lock_now = true;
    }
    ui.label(RichText::new(t(lang, "lock_note")).small());
}

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
    level_colors: &mut bool,
    external_tools: &mut Vec<ExternalTool>,
    rules: &[HighlightRule],
    tool_runner: &mut ToolRunner,
    language_auto: &mut bool,
    lock_enabled: &mut bool,
    lock_pin: &mut String,
    lock_now: &mut bool,
    overview_strip: &mut bool,
    time_delta: &mut TimeDeltaPrefs,
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

    // Language selector: a combo box, so the row stays one line however many languages
    // ship with the build. The first entry follows the operating system, which is what a
    // fresh install does until a language is picked here.
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{}:", t(*lang, "language"))).monospace());
        let detected = Language::detect();
        let system_label = format!("{} ({})", t(*lang, "language_system"), detected.name());
        let selected = if *language_auto {
            system_label.clone()
        } else {
            lang.name().to_owned()
        };
        egui::ComboBox::from_id_salt("language_select")
            .selected_text(RichText::new(selected).monospace())
            .width(220.0)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(*language_auto, &system_label)
                    .on_hover_text(t(*lang, "language_system_tip"))
                    .clicked()
                {
                    *language_auto = true;
                    *lang = detected;
                }
                ui.separator();
                for choice in Language::ALL {
                    let picked = !*language_auto && *lang == *choice;
                    if ui.selectable_label(picked, choice.name()).clicked() {
                        *language_auto = false;
                        *lang = *choice;
                    }
                }
            });
    });

    ui.add_space(6.0);

    // Interface zoom: the same value `Ctrl +`, `Ctrl -`, `Ctrl 0` and `Ctrl + wheel`
    // move, so the buttons here and the shortcuts can never disagree.
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{}:", t(*lang, "zoom"))).monospace());
        let zoom = ui.ctx().zoom_factor();
        if ui
            .button(" - ")
            .on_hover_text(t(*lang, "zoom_tip"))
            .clicked()
        {
            ui.ctx()
                .set_zoom_factor(crate::config::stepped_zoom(zoom, -1));
        }
        ui.label(
            RichText::new(format!("🔍 {}%", crate::config::zoom_percent(zoom)))
                .monospace()
                .strong(),
        )
        .on_hover_text(t(*lang, "zoom_tip"));
        if ui
            .button(" + ")
            .on_hover_text(t(*lang, "zoom_tip"))
            .clicked()
        {
            ui.ctx()
                .set_zoom_factor(crate::config::stepped_zoom(zoom, 1));
        }
        if ui
            .button("100%")
            .on_hover_text(t(*lang, "zoom_tip"))
            .clicked()
        {
            ui.ctx().set_zoom_factor(1.0);
        }
    });

    ui.add_space(6.0);

    // Log font size in points: independent of the zoom, which scales the whole UI.
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{}:", t(*lang, "font_size"))).monospace());
        if ui
            .button(" - ")
            .on_hover_text(t(*lang, "font_dec_tip"))
            .clicked()
        {
            *font_size = (*font_size - 1.0).max(crate::config::MIN_FONT_SIZE);
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
            *font_size = (*font_size + 1.0).min(crate::config::MAX_FONT_SIZE);
        }
        if ui
            .button("100%")
            .on_hover_text(t(*lang, "font_reset_tip"))
            .clicked()
        {
            *font_size = crate::config::DEFAULT_FONT_SIZE;
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
            ui.add(egui::DragValue::new(screensaver_timeout_mins).range(0..=120))
                .on_hover_text(t(*lang, "screensaver_zero_off"));
            ui.add_space(8.0);
            if ui.button(t(*lang, "test_screensaver")).clicked() {
                *test_screensaver = true;
            }
        });
    }

    ui.add_space(6.0);
    render_lock_settings(ui, *lang, lock_enabled, lock_pin, lock_now);

    ui.add_space(6.0);
    ui.checkbox(telemetry_enabled, t(*lang, "telemetry"));
    ui.checkbox(sound_enabled, t(*lang, "sound_fx"));
    ui.checkbox(borderless, t(*lang, "borderless"));
    ui.checkbox(show_line_numbers, t(*lang, "show_lines"));
    ui.checkbox(level_colors, t(*lang, "level_colors"))
        .on_hover_text(t(*lang, "level_colors_tip"));
    ui.checkbox(overview_strip, t(*lang, "overview_strip"))
        .on_hover_text(t(*lang, "overview_strip_tip"));
    ui.checkbox(&mut time_delta.show, t(*lang, "show_time_delta"))
        .on_hover_text(t(*lang, "tip_time_delta"));
    ui.horizontal(|ui| {
        ui.label(t(*lang, "time_delta_gap"));
        ui.add(
            egui::DragValue::new(&mut time_delta.gap_ms)
                .range(0..=86_400_000)
                .speed(10.0)
                .suffix(" ms"),
        );
    });

    ui.add_space(6.0);
    ui.separator();
    ui.add_space(6.0);
    render_external_tools_editor(ui, *lang, external_tools, rules, tool_runner);
}

/// Settings section listing the external tools: one editable card per tool with name,
/// program, arguments, shortcut, bound rule, `{match}` regex, shell flag and the count of
/// dropped rule-bound runs.
fn render_external_tools_editor(
    ui: &mut Ui,
    lang: Language,
    external_tools: &mut Vec<ExternalTool>,
    rules: &[HighlightRule],
    tool_runner: &mut ToolRunner,
) {
    let (dim, warn) = (ui.visuals().weak_text_color(), ui.visuals().warn_fg_color);
    ui.label(
        RichText::new(format!("🛠 {}", t(lang, "ext_tools")))
            .monospace()
            .strong(),
    );
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!(
                "{}: {}",
                t(lang, "ext_tools_placeholders"),
                crate::external_tools::PLACEHOLDERS.join(" ")
            ))
            .monospace()
            .small()
            .color(dim),
        );
        // The fields alone do not say what a tool is *for*: point at the recipes.
        ui.hyperlink_to(
            RichText::new(t(lang, "ext_tools_cookbook")).small(),
            "https://github.com/matteobaccan/FastTail/blob/main/docs/external-tools-cookbook.md",
        )
        .on_hover_text(t(lang, "ext_tools_cookbook_tip"));
    });
    ui.add_space(4.0);

    let mut remove: Option<usize> = None;
    let mut rule_patterns: Vec<&str> = rules.iter().map(|r| r.pattern.as_str()).collect();
    rule_patterns.dedup();
    for (i, tool) in external_tools.iter_mut().enumerate() {
        ui.group(|ui| {
            egui::Grid::new(("ext_tool", i))
                .num_columns(2)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    ui.label(RichText::new(t(lang, "ext_tool_name")).monospace());
                    ui.add(egui::TextEdit::singleline(&mut tool.name).desired_width(260.0));
                    ui.end_row();

                    ui.label(RichText::new(t(lang, "ext_tool_program")).monospace());
                    ui.add(
                        egui::TextEdit::singleline(&mut tool.program)
                            .desired_width(260.0)
                            .hint_text("code"),
                    );
                    ui.end_row();

                    ui.label(RichText::new(t(lang, "ext_tool_args")).monospace());
                    ui.add(
                        egui::TextEdit::singleline(&mut tool.args)
                            .desired_width(260.0)
                            .hint_text("-g \"{file}:{lineno}\""),
                    );
                    ui.end_row();

                    ui.label(RichText::new(t(lang, "ext_tool_shortcut")).monospace());
                    ui.horizontal(|ui| {
                        let mut text = tool.shortcut.clone().unwrap_or_default();
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut text)
                                    .desired_width(140.0)
                                    .hint_text("Ctrl+Shift+F9"),
                            )
                            .changed()
                        {
                            let trimmed = text.trim().to_string();
                            tool.shortcut = (!trimmed.is_empty()).then_some(trimmed);
                        }
                        if tool.shortcut.is_some() && tool.parsed_shortcut().is_none() {
                            ui.label(
                                RichText::new(t(lang, "ext_tool_bad_shortcut"))
                                    .small()
                                    .color(warn),
                            );
                        }
                    });
                    ui.end_row();

                    ui.label(RichText::new(t(lang, "ext_tool_rule")).monospace());
                    ui.horizontal(|ui| {
                        let none = t(lang, "ext_tool_no_rule");
                        let selected = tool.bound_rule.clone().unwrap_or_else(|| none.to_string());
                        egui::ComboBox::from_id_salt(("ext_tool_rule", i))
                            .width(180.0)
                            .selected_text(RichText::new(&selected).monospace())
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_label(tool.bound_rule.is_none(), none)
                                    .clicked()
                                {
                                    tool.bound_rule = None;
                                }
                                for pat in &rule_patterns {
                                    let is_sel = tool.bound_rule.as_deref() == Some(*pat);
                                    if ui
                                        .selectable_label(is_sel, RichText::new(*pat).monospace())
                                        .clicked()
                                    {
                                        tool.bound_rule = Some((*pat).to_string());
                                    }
                                }
                            });
                        if let Some(bound) = tool.bound_rule.as_deref() {
                            if !rule_patterns.contains(&bound) {
                                ui.label(
                                    RichText::new(t(lang, "ext_tool_rule_missing"))
                                        .small()
                                        .color(warn),
                                );
                            }
                        }
                    });
                    ui.end_row();

                    ui.label(RichText::new(t(lang, "ext_tool_match")).monospace());
                    {
                        let mut text = tool.match_pattern.clone().unwrap_or_default();
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut text)
                                    .desired_width(260.0)
                                    .hint_text("req=([0-9]+)"),
                            )
                            .changed()
                        {
                            tool.match_pattern = (!text.is_empty()).then_some(text);
                        }
                    }
                    ui.end_row();
                });
            ui.horizontal(|ui| {
                ui.checkbox(&mut tool.use_shell, t(lang, "ext_tool_shell"));
                if tool.use_shell {
                    ui.label(
                        RichText::new(format!("⚠ {}", t(lang, "ext_tool_shell_warn")))
                            .small()
                            .color(warn),
                    );
                }
            });
            ui.horizontal(|ui| {
                if tool.bound_rule.is_some() {
                    ui.label(
                        RichText::new(format!(
                            "{}: {}",
                            t(lang, "ext_tool_dropped"),
                            tool_runner.dropped_for(&tool.name)
                        ))
                        .monospace()
                        .small()
                        .color(dim),
                    );
                }
                if ui
                    .button(RichText::new(format!("🗑 {}", t(lang, "ext_tool_remove"))).small())
                    .clicked()
                {
                    remove = Some(i);
                }
            });
        });
        ui.add_space(4.0);
    }
    if let Some(i) = remove {
        external_tools.remove(i);
    }
    if ui
        .button(RichText::new(format!("➕ {}", t(lang, "ext_tool_add"))).monospace())
        .clicked()
    {
        let n = external_tools.len() + 1;
        external_tools.push(ExternalTool::new(
            &format!("{} {n}", t(lang, "ext_tool_default_name")),
            "",
            "{line}",
        ));
    }
    if let Some(err) = tool_runner.last_error.as_deref() {
        ui.label(
            RichText::new(format!("{}: {err}", t(lang, "ext_tool_run_failed")))
                .small()
                .color(warn),
        );
    }
}

fn render_markdown_stream(
    ui: &mut Ui,
    engine: &mut TailEngine,
    theme: &CyberTheme,
    lang: Language,
) {
    if engine.total_lines() == 0 {
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new(t(lang, "file_empty"))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clear_button_shows_for_text_or_a_window() {
        // An unparsable text is not a window, yet there is something to clear.
        assert!(time_range_clear_shown(true, false));
        assert!(time_range_clear_shown(false, true));
        assert!(!time_range_clear_shown(false, false));
    }
}
