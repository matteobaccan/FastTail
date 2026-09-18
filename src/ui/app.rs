use crate::baretail_bridge::{detect_baretail_config, BareTailConfig};
use crate::config::FastTailConfig;
use crate::external_tools::ToolRunner;
use crate::i18n::t;
use crate::paths::paths_equal;
use crate::screensaver::MatrixScreensaver;
use crate::tail_engine::{QuickLabel, TailEngine};
use crate::theme::CyberTheme;
use crate::ui::dock::{DockContext, FastTailTab, FastTailTabViewer};
use eframe::egui;
use egui::{Color32, CornerRadius, Key, Margin, RichText, Stroke, ViewportCommand};
use egui_dock::{DockArea, DockState};
use std::path::PathBuf;
use std::time::Instant;
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};

/// Drops the recorded rects whose surface no longer exists or is not a floating window.
///
/// `DockState::get_window_state` indexes the surface vector without a bounds check,
/// so a stale `SurfaceIndex` left behind by a closed floating window must never
/// reach it (v0.1.0 crash: "index out of bounds: the len is 3 but the index is 3").
pub fn prune_floating_window_rects<T>(
    dock_state: &DockState<T>,
    rects: &mut std::collections::HashMap<egui_dock::SurfaceIndex, egui::Rect>,
) {
    rects.retain(|idx, _| {
        matches!(
            dock_state.get_surface(*idx),
            Some(egui_dock::Surface::Window(..))
        )
    });
}

#[cfg(windows)]
mod win_util {
    #[link(name = "user32")]
    extern "system" {
        pub fn GetActiveWindow() -> *mut std::ffi::c_void;
        pub fn ShowWindow(hwnd: *mut std::ffi::c_void, cmd_show: i32) -> i32;
    }
    pub const SW_MINIMIZE: i32 = 6;
    pub const SW_MAXIMIZE: i32 = 3;
    pub const SW_RESTORE: i32 = 9;
}

pub struct FastTailApp {
    pub config: FastTailConfig,
    pub engines: Vec<TailEngine>,
    pub dock_state: DockState<FastTailTab>,
    pub screensaver: MatrixScreensaver,
    pub system: System,
    pub cpu_usage: f32,
    pub mem_used_mb: u64,
    pub last_sys_refresh: Instant,
    pub baretail_dialog_open: bool,
    pub baretail_config: Option<BareTailConfig>,
    pub last_dock_save: Instant,
    pub first_frame: bool,
    pub floating_window_rects: std::collections::HashMap<egui_dock::SurfaceIndex, egui::Rect>,
    /// Backend the window runs on, read once from the creation context.
    pub renderer: crate::renderer::ActiveRenderer,
    /// Window level currently applied to the viewport (see `config.always_on_top`).
    pub applied_on_top: bool,
    /// An OS attention request was sent and the window has not been focused since.
    pub attention_requested: bool,
    /// Quick colour labels (Ctrl+Shift+1..9), in memory only, pushed to every engine.
    pub quick_labels: Vec<QuickLabel>,
    /// Text of the "open pattern" prompt while it is shown (`None` when closed).
    pub pattern_prompt: Option<String>,
    /// Spawns external tools and enforces the rule-bound throttle and cap.
    pub tool_runner: ToolRunner,
}

/// Applies a dialog's persisted position and size to `win`; without a saved position the
/// dialog is centered, without a saved size `default_size` decides.
fn restore_dialog_geometry<'a>(
    win: egui::Window<'a>,
    ctx: &egui::Context,
    pos: Option<[f32; 2]>,
    size: Option<[f32; 2]>,
    default_size: impl FnOnce(egui::Window<'a>) -> egui::Window<'a>,
) -> egui::Window<'a> {
    let win = match pos {
        Some([x, y]) => win.default_pos(egui::pos2(x, y)),
        None => win
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center()),
    };
    match size {
        Some([w, h]) => win.default_size(egui::vec2(w, h)),
        None => default_size(win),
    }
}

/// Stores the rendered dialog rect back into the config so it reopens where it was.
fn capture_dialog_geometry<R>(
    resp: &Option<egui::InnerResponse<Option<R>>>,
    pos: &mut Option<[f32; 2]>,
    size: &mut Option<[f32; 2]>,
) {
    if let Some(inner) = resp {
        let rect = inner.response.rect;
        if rect.min.x > -1000.0 && rect.min.y > -1000.0 {
            *pos = Some([rect.min.x, rect.min.y]);
            *size = Some([rect.width(), rect.height()]);
        }
    }
}

fn setup_cjk_fonts(ctx: &egui::Context) {
    #[cfg(windows)]
    {
        let font_candidates = [
            "C:\\Windows\\Fonts\\msyh.ttc",
            "C:\\Windows\\Fonts\\msyhbd.ttc",
            "C:\\Windows\\Fonts\\simsun.ttc",
            "C:\\Windows\\Fonts\\malgun.ttf",
        ];
        for path in &font_candidates {
            if let Ok(bytes) = std::fs::read(path) {
                let mut fonts = egui::FontDefinitions::default();
                fonts.font_data.insert(
                    "cjk_fallback".to_owned(),
                    std::sync::Arc::new(egui::FontData::from_owned(bytes)),
                );
                fonts
                    .families
                    .entry(egui::FontFamily::Proportional)
                    .or_default()
                    .push("cjk_fallback".to_owned());
                fonts
                    .families
                    .entry(egui::FontFamily::Monospace)
                    .or_default()
                    .push("cjk_fallback".to_owned());

                ctx.set_fonts(fonts);
                break;
            }
        }
    }
}

impl FastTailApp {
    pub fn new(cc: &eframe::CreationContext<'_>, cli: crate::cli::CliArgs) -> Self {
        setup_cjk_fonts(&cc.egui_ctx);
        let mut config = FastTailConfig::load();
        if cli.fresh {
            // Empty workspace: no restored streams, no saved dock layout.
            config.open_files.clear();
            config.dock_layout = None;
        }
        config.theme.apply(&cc.egui_ctx);
        let mut app = Self::from_config(config);
        app.apply_cli(&cli);
        app.renderer = crate::renderer::ActiveRenderer::from_creation_context(cc);
        crate::renderer::mark_app_created();
        eprintln!(
            "renderer: running on {} ({})",
            app.renderer.chip(),
            app.renderer.details()
        );
        app
    }

    pub fn from_config(config: FastTailConfig) -> Self {
        let mut system = System::new_with_specifics(
            RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory(MemoryRefreshKind::everything()),
        );
        system.refresh_cpu_usage();
        system.refresh_memory();

        // Check for BareTail config on Windows if enabled (only done once, then disabled in fasttail.ini)
        let baretail_config = if config.baretail_import && !config.baretail_prompt_shown {
            detect_baretail_config()
        } else {
            None
        };
        let baretail_dialog_open = baretail_config.is_some();

        // Restore saved dock layout from config, or start clean
        let dock_state: DockState<FastTailTab> = config
            .dock_layout
            .as_ref()
            .and_then(|ron_str| ron::from_str(ron_str).ok())
            .unwrap_or_else(|| DockState::new(vec![]));

        let mut floating_window_rects = std::collections::HashMap::new();
        for (surf_index, surface) in dock_state.iter_surfaces_indexed() {
            if let egui_dock::Surface::Window(_tree, ws) = surface {
                let r = ws.rect();
                if r.is_positive() && r.min.x > -10000.0 && r.min.y > -10000.0 {
                    floating_window_rects.insert(surf_index, r);
                }
            }
        }

        let mut app = Self {
            config,
            engines: Vec::new(),
            dock_state,
            screensaver: MatrixScreensaver::default(),
            system,
            cpu_usage: 0.0,
            mem_used_mb: 0,
            last_sys_refresh: Instant::now(),
            baretail_dialog_open,
            baretail_config,
            last_dock_save: Instant::now(),
            first_frame: true,
            floating_window_rects,
            renderer: crate::renderer::ActiveRenderer::unknown(),
            applied_on_top: false,
            attention_requested: false,
            quick_labels: Vec::new(),
            pattern_prompt: None,
            tool_runner: ToolRunner::default(),
        };

        let has_restored_tabs = app.dock_state.iter_all_tabs().count() > 0;
        if has_restored_tabs {
            // Restore engines for tabs already positioned in the dock layout without altering dock tree
            let mut tabs_to_open: Vec<PathBuf> = Vec::new();
            for (_, tab) in app.dock_state.iter_all_tabs() {
                if let FastTailTab::LogStream(p) = tab {
                    if !tabs_to_open
                        .iter()
                        .any(|existing| paths_equal(existing.as_path(), p.as_path()))
                    {
                        tabs_to_open.push(p.clone());
                    }
                }
            }
            for path in tabs_to_open {
                let is_pattern = crate::wildcard::is_pattern_path(&path);
                if !is_pattern && !path.exists() {
                    continue;
                }
                let opened = if is_pattern {
                    // A pattern tab resolves to the newest match again at every start.
                    TailEngine::open_pattern(&path)
                } else {
                    TailEngine::open(&path)
                };
                if let Ok(mut engine) = opened {
                    engine.set_highlight_rules(app.config.highlight_rules.clone());
                    engine.size_unit = app.config.size_unit;
                    engine.wrap_lines = app.config.wrap_for(&path);
                    app.engines.push(engine);
                }
            }
        } else {
            // Clean dock layout: open previously saved files into dock
            for path in app.config.open_files.clone() {
                if path.exists() || crate::wildcard::is_pattern_path(&path) {
                    app.open_log_file(path);
                }
            }
        }

        // Open any files passed as CLI arguments
        for arg in std::env::args_os().skip(1) {
            let path = PathBuf::from(arg);
            if path.exists() {
                app.open_log_file(path);
            }
        }

        app
    }

    /// Keeps every engine's set of tool-bound rule patterns in sync with the tools list
    /// and runs the tools whose rule matched appended lines, through the throttled runner.
    pub fn run_rule_bound_tools(&mut self) {
        let bound: std::collections::HashSet<String> = self
            .config
            .external_tools
            .iter()
            .filter_map(|t| t.bound_rule.clone())
            .collect();
        for eng in &mut self.engines {
            if eng.tool_bound_rules != bound {
                eng.tool_bound_rules = bound.clone();
            }
            if eng.pending_tool_hits.is_empty() {
                continue;
            }
            let hits = std::mem::take(&mut eng.pending_tool_hits);
            for (pattern, row) in hits {
                let Some(ctx) = crate::ui::dock::tool_context_for_row(eng, row) else {
                    continue;
                };
                for tool in self
                    .config
                    .external_tools
                    .iter()
                    .filter(|t| t.bound_rule.as_deref() == Some(pattern.as_str()))
                {
                    self.tool_runner.run_bound(tool, &ctx);
                }
            }
        }
    }

    pub fn save_dock_layout(&mut self) {
        let mut current_open: Vec<PathBuf> = Vec::new();
        for (_, tab) in self.dock_state.iter_all_tabs() {
            if let FastTailTab::LogStream(p) = tab {
                if !current_open
                    .iter()
                    .any(|existing| paths_equal(existing.as_path(), p.as_path()))
                {
                    current_open.push(p.clone());
                }
            }
        }
        self.config.open_files = current_open;

        prune_floating_window_rects(&self.dock_state, &mut self.floating_window_rects);
        let mut dock_to_save = self.dock_state.clone();
        for (surf_index, rect) in &self.floating_window_rects {
            if let Some(ws) = dock_to_save.get_window_state_mut(*surf_index) {
                if rect.is_positive()
                    && rect.min.x.is_finite()
                    && rect.min.y.is_finite()
                    && rect.min.x > -10000.0
                    && rect.min.y > -10000.0
                {
                    ws.set_position(rect.min);
                    ws.set_size(rect.size());
                }
            }
        }

        if let Ok(ron_str) = ron::to_string(&dock_to_save) {
            if self.config.dock_layout.as_deref() != Some(&ron_str) {
                self.config.dock_layout = Some(ron_str);
            }
        }
        let _ = self.config.save();
    }

    /// Opens the files named on the command line (skipping ones already open) and applies
    /// the command line filters and follow flag to those streams only.
    pub fn apply_cli(&mut self, cli: &crate::cli::CliArgs) {
        for path in &cli.paths {
            let pattern_dir_exists = crate::wildcard::split_pattern(path)
                .map(|(dir, _)| dir.is_dir())
                .unwrap_or(false);
            if !path.exists() && !pattern_dir_exists {
                eprintln!("fasttail: {} not found, skipped", path.display());
                continue;
            }
            self.open_log_file(path.clone());
            if let Some(engine) = self
                .engines
                .iter_mut()
                .find(|e| e.path == *path || paths_equal(&e.path, path))
            {
                if let Some(f) = &cli.filter {
                    engine.set_include_filter(f);
                }
                if let Some(x) = &cli.exclude {
                    engine.set_exclude_filter(x);
                }
                if let Some(follow) = cli.follow {
                    engine.follow_tail = follow;
                }
            }
        }
    }

    /// Opens the "open pattern" prompt, prefilled with `text`.
    pub fn prompt_pattern(&mut self, text: String) {
        self.pattern_prompt = Some(text);
    }

    /// Default pattern offered for a directory: every `.log` file in it.
    pub fn default_pattern_for(dir: &std::path::Path) -> String {
        dir.join("*.log").to_string_lossy().to_string()
    }

    pub fn open_log_file(&mut self, path: PathBuf) {
        let is_pattern = crate::wildcard::is_pattern_path(&path);
        if !is_pattern && !path.exists() {
            return;
        }

        // Avoid duplicate tabs for same path
        for eng in &self.engines {
            if eng.path == path || paths_equal(&eng.path, &path) {
                // Already open, select tab
                let tab = FastTailTab::LogStream(eng.path.clone());
                if let Some(locator) = self.dock_state.find_tab(&tab) {
                    let _ = self.dock_state.set_active_tab(locator);
                }
                return;
            }
        }

        let opened = if is_pattern {
            TailEngine::open_pattern(&path)
        } else {
            TailEngine::open(&path)
        };
        if let Ok(mut engine) = opened {
            engine.set_highlight_rules(self.config.highlight_rules.clone());
            engine.set_quick_labels(&self.quick_labels);
            engine.size_unit = self.config.size_unit;
            if let Some(lines) = self.config.bookmarks_for(&path, engine.total_lines()) {
                engine.set_bookmarks(lines);
            }
            engine.wrap_lines = self.config.wrap_for(&path);
            self.engines.push(engine);

            crate::audio::play_sound(
                crate::audio::CyberSound::BlipAttach,
                self.config.sound_enabled,
            );

            // Add to open_files
            if !self.config.open_files.iter().any(|p| paths_equal(p, &path)) {
                self.config.open_files.push(path.clone());
            }

            // Keep recent files in MRU order (most recent at top, max 15)
            self.config.recent_files.retain(|p| !paths_equal(p, &path));
            self.config.recent_files.insert(0, path.clone());
            if self.config.recent_files.len() > 15 {
                self.config.recent_files.truncate(15);
            }
            let _ = self.config.save();

            let tab = FastTailTab::LogStream(path.clone());
            let already_in_dock = self.dock_state.find_tab(&tab).is_some()
                || self.dock_state.iter_all_tabs().any(|(_, t)| {
                    if let FastTailTab::LogStream(p) = t {
                        paths_equal(p, &path)
                    } else {
                        false
                    }
                });
            if !already_in_dock {
                if self.dock_state.iter_all_tabs().count() == 0 {
                    self.dock_state = egui_dock::DockState::new(vec![tab]);
                } else {
                    self.dock_state.main_surface_mut().push_to_first_leaf(tab);
                }
            }
            self.save_dock_layout();
        }
    }

    pub fn render_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();

        // Fill background of the window canvas with current theme bg color
        ui.painter()
            .rect_filled(ui.max_rect(), 0.0, self.config.theme.bg_color());

        // Always-on-top: apply whenever the setting and the viewport disagree (covers startup)
        if self.config.always_on_top != self.applied_on_top {
            self.applied_on_top = self.config.always_on_top;
            ctx.send_viewport_cmd(ViewportCommand::WindowLevel(if self.applied_on_top {
                egui::WindowLevel::AlwaysOnTop
            } else {
                egui::WindowLevel::Normal
            }));
        }

        // 0. Handle initial maximize on Windows / viewport
        if self.first_frame {
            self.first_frame = false;
            if self.config.window_maximized {
                ctx.send_viewport_cmd(ViewportCommand::Maximized(true));
                #[cfg(windows)]
                unsafe {
                    let hwnd = win_util::GetActiveWindow();
                    if !hwnd.is_null() {
                        win_util::ShowWindow(hwnd, win_util::SW_MAXIMIZE);
                    }
                }
            }
        }

        // 1. Detect user activity to reset screensaver & handle window closing and viewport bounds
        let mut escape_pressed = false;
        ctx.input(|i| {
            if i.viewport().close_requested() {
                self.save_dock_layout();
            }

            if let Some(maximized) = i.viewport().maximized {
                self.config.window_maximized = maximized;
            }

            if !self.config.window_maximized {
                if let Some(rect) = i.viewport().outer_rect {
                    if rect.min.x > -10000.0 && rect.min.y > -10000.0 {
                        self.config.window_x = Some(rect.min.x);
                        self.config.window_y = Some(rect.min.y);
                    }
                }
                if let Some(rect) = i.viewport().inner_rect {
                    if rect.width() >= 400.0 && rect.height() >= 300.0 {
                        self.config.window_width = Some(rect.width());
                        self.config.window_height = Some(rect.height());
                    }
                }
            }

            let has_user_input = if self.screensaver.is_active {
                !i.raw.events.is_empty()
            } else {
                i.raw.events.iter().any(|e| {
                    matches!(
                        e,
                        egui::Event::Key { pressed: true, .. }
                            | egui::Event::PointerMoved(_)
                            | egui::Event::PointerButton { pressed: true, .. }
                            | egui::Event::MouseWheel { .. }
                            | egui::Event::Touch { .. }
                            | egui::Event::Text(_)
                            | egui::Event::Paste(_)
                    )
                })
            };

            if has_user_input {
                self.screensaver.on_user_input();
            }

            // Keyboard shortcut: Space = toggle follow tail on active stream
            if i.key_pressed(Key::Space) {
                for eng in &mut self.engines {
                    eng.follow_tail = !eng.follow_tail;
                }
            }

            // Keyboard shortcut: Ctrl + / Ctrl = (Zoom in font)
            if i.modifiers.ctrl && (i.key_pressed(Key::Plus) || i.key_pressed(Key::Equals)) {
                self.config.font_size = (self.config.font_size + 1.0).min(32.0);
                let _ = self.config.save();
            }

            // Keyboard shortcut: Ctrl - (Zoom out font)
            if i.modifiers.ctrl && i.key_pressed(Key::Minus) {
                self.config.font_size = (self.config.font_size - 1.0).max(8.0);
                let _ = self.config.save();
            }

            // Keyboard shortcut: Ctrl 0 (Reset font size)
            if i.modifiers.ctrl && i.key_pressed(Key::Num0) {
                self.config.font_size = 13.0;
                let _ = self.config.save();
            }

            // Keyboard shortcut: Ctrl + Mouse Wheel (Zoom in/out font)
            if i.modifiers.ctrl && i.smooth_scroll_delta.y != 0.0 {
                if i.smooth_scroll_delta.y > 0.0 {
                    self.config.font_size = (self.config.font_size + 1.0).min(32.0);
                } else {
                    self.config.font_size = (self.config.font_size - 1.0).max(8.0);
                }
                let _ = self.config.save();
            }

            // Keyboard shortcut: F1 (Toggle Help)
            if i.key_pressed(Key::F1) {
                self.config.help_open = !self.config.help_open;
                let _ = self.config.save();
            }

            // Keyboard shortcut: Ctrl + Shift + T (toggle always-on-top)
            if i.modifiers.command && i.modifiers.shift && i.key_pressed(Key::T) {
                self.config.always_on_top = !self.config.always_on_top;
                let _ = self.config.save();
            }

            // Keyboard shortcut: Escape (Close any open popup)
            if i.key_pressed(Key::Escape) {
                escape_pressed = true;
                self.config.help_open = false;
                self.config.settings_open = false;
                self.config.filters_open = false;
                self.config.about_open = false;
                let _ = self.config.save();
            }

            // Drag & drop file support (single or multiple)
            if !i.raw.dropped_files.is_empty() {
                for file in &i.raw.dropped_files {
                    let path = file.path().to_path_buf();
                    if path.is_dir() {
                        // A folder: ask which files to follow (newest match is tailed).
                        self.pattern_prompt = Some(Self::default_pattern_for(&path));
                    } else {
                        self.open_log_file(path);
                    }
                }
            }
        });

        if escape_pressed {
            ctx.memory_mut(|m| m.stop_text_input());
        }

        // Save dock layout periodically every 2 seconds if changed
        if self.last_dock_save.elapsed().as_secs_f32() >= 2.0 {
            self.save_dock_layout();
            self.last_dock_save = Instant::now();
        }

        // 2. Poll file updates, then run the tools bound to the rules that matched
        for eng in &mut self.engines {
            eng.poll_updates();
        }
        self.run_rule_bound_tools();

        // 3. Periodic telemetry refresh
        if self.last_sys_refresh.elapsed().as_secs_f32() >= 1.0 {
            self.system.refresh_cpu_usage();
            self.system.refresh_memory();
            self.cpu_usage = self.system.global_cpu_usage();
            self.mem_used_mb = self.system.used_memory() / (1024 * 1024);
            self.last_sys_refresh = Instant::now();
        }

        // 4. Check screensaver idle timeout (only a focused window can start it)
        let window_focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        self.screensaver.check_inactivity(
            self.config.screensaver_timeout_mins,
            self.config.screensaver_enabled,
            window_focused,
        );
        if self.config.screensaver_enabled
            && self.config.screensaver_timeout_mins > 0
            && window_focused
            && !self.screensaver.is_active
        {
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
        }

        // 5. Apply theme visuals
        self.config.theme.apply(&ctx);

        // 6. Primary Title Bar (Title, window controls, telemetry, and safe draggable region)
        egui::Panel::top("title_panel")
            .frame(
                egui::Frame::new()
                    .fill(self.config.theme.bg_color())
                    .inner_margin(Margin {
                        left: 8,
                        right: 8,
                        top: 4,
                        bottom: 4,
                    }),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Glowing circular F icon badge
                    let (logo_rect, _) =
                        ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::hover());
                    let logo_bg = if self.config.theme == CyberTheme::Light {
                        Color32::from_rgb(220, 235, 252)
                    } else {
                        Color32::from_rgb(10, 26, 40)
                    };
                    ui.painter().circle(
                        logo_rect.center(),
                        10.0,
                        logo_bg,
                        Stroke::new(1.5, self.config.theme.accent_color()),
                    );
                    ui.painter().text(
                        logo_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "F",
                        egui::FontId::monospace(12.0),
                        self.config.theme.accent_color(),
                    );
                    ui.add_space(4.0);

                    // Title with integrated version
                    let version_str = format!("v{} by Matteo Baccan", env!("CARGO_PKG_VERSION"));
                    let title_text = format!("FASTTAIL {}", version_str);
                    let title_resp = ui.add(
                        egui::Label::new(
                            RichText::new(title_text)
                                .monospace()
                                .strong()
                                .size(13.0)
                                .color(self.config.theme.accent_color()),
                        )
                        .sense(egui::Sense::click_and_drag()),
                    );
                    if title_resp.hovered() || title_resp.dragged() {
                        ctx.set_cursor_icon(egui::CursorIcon::Move);
                    }
                    if title_resp.drag_started_by(egui::PointerButton::Primary) {
                        ctx.set_cursor_icon(egui::CursorIcon::Move);
                        ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                    }

                    // Right-aligned controls (Close, Maximize, Minimize, Telemetry, Drag Grip)
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.config.borderless {
                            // Close button [✕]
                            if ui
                                .button(
                                    RichText::new(" ✕ ")
                                        .color(self.config.theme.warn_color())
                                        .monospace()
                                        .strong(),
                                )
                                .on_hover_text(t(self.config.language, "close_tip"))
                                .clicked()
                            {
                                self.save_dock_layout();
                                let _ = self.config.save();
                                ctx.send_viewport_cmd(ViewportCommand::Close);
                                std::process::exit(0);
                            }

                            // Maximize / Restore button [🗖 / 🗗]
                            let max_icon = if self.config.window_maximized {
                                " 🗗 "
                            } else {
                                " 🗖 "
                            };
                            let max_tip = if self.config.window_maximized {
                                t(self.config.language, "restore_tip")
                            } else {
                                t(self.config.language, "maximize_tip")
                            };
                            if ui
                                .button(RichText::new(max_icon).monospace())
                                .on_hover_text(max_tip)
                                .clicked()
                            {
                                self.config.window_maximized = !self.config.window_maximized;
                                ctx.send_viewport_cmd(ViewportCommand::Maximized(
                                    self.config.window_maximized,
                                ));
                                #[cfg(windows)]
                                unsafe {
                                    let hwnd = win_util::GetActiveWindow();
                                    if !hwnd.is_null() {
                                        if self.config.window_maximized {
                                            win_util::ShowWindow(hwnd, win_util::SW_MAXIMIZE);
                                        } else {
                                            win_util::ShowWindow(hwnd, win_util::SW_RESTORE);
                                        }
                                    }
                                }
                            }

                            // Minimize button [—]
                            if ui
                                .button(RichText::new(" — ").monospace())
                                .on_hover_text(t(self.config.language, "minimize_tip"))
                                .clicked()
                            {
                                ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
                                #[cfg(windows)]
                                unsafe {
                                    let hwnd = win_util::GetActiveWindow();
                                    if !hwnd.is_null() {
                                        win_util::ShowWindow(hwnd, win_util::SW_MINIMIZE);
                                    }
                                }
                            }

                            ui.separator();
                        }

                        // Always-on-top pin (Ctrl+Shift+T)
                        let pin_text = if self.config.always_on_top {
                            RichText::new(" 📌 ")
                                .monospace()
                                .color(self.config.theme.accent_color())
                        } else {
                            RichText::new(" 📌 ")
                                .monospace()
                                .color(self.config.theme.text_dim())
                        };
                        if ui
                            .button(pin_text)
                            .on_hover_text(t(self.config.language, "pin_tip"))
                            .clicked()
                        {
                            self.config.always_on_top = !self.config.always_on_top;
                            let _ = self.config.save();
                        }
                        ui.separator();

                        if self.config.telemetry_enabled {
                            // RAM Meter & Progress Bar
                            let total_mem_gb = (self.system.total_memory() as f32
                                / (1024.0 * 1024.0 * 1024.0))
                                .max(1.0);
                            let used_mem_gb = self.mem_used_mb as f32 / 1024.0;
                            let ram_fraction = (used_mem_gb / total_mem_gb).clamp(0.0, 1.0);

                            let meter_bg = if self.config.theme == CyberTheme::Light {
                                Color32::from_rgb(220, 228, 238)
                            } else {
                                Color32::from_rgb(8, 22, 35)
                            };
                            let (ram_bar, _) =
                                ui.allocate_exact_size(egui::vec2(44.0, 6.0), egui::Sense::hover());
                            ui.painter()
                                .rect_filled(ram_bar, CornerRadius::same(3), meter_bg);
                            let ram_fill_w = (ram_bar.width() * ram_fraction).max(2.0);
                            ui.painter().rect_filled(
                                egui::Rect::from_min_size(
                                    ram_bar.min,
                                    egui::vec2(ram_fill_w, ram_bar.height()),
                                ),
                                CornerRadius::same(3),
                                self.config.theme.secondary_accent(),
                            );

                            ui.label(
                                RichText::new(format!(
                                    "RAM: {:.1} GB/{:.0} GB",
                                    used_mem_gb, total_mem_gb
                                ))
                                .monospace()
                                .size(10.5)
                                .color(self.config.theme.text_dim()),
                            );

                            ui.separator();

                            // CPU Meter & Progress Bar
                            let cpu_fraction = (self.cpu_usage / 100.0).clamp(0.0, 1.0);
                            let (cpu_bar, _) =
                                ui.allocate_exact_size(egui::vec2(44.0, 6.0), egui::Sense::hover());
                            ui.painter()
                                .rect_filled(cpu_bar, CornerRadius::same(3), meter_bg);
                            let cpu_fill_w = (cpu_bar.width() * cpu_fraction).max(2.0);
                            ui.painter().rect_filled(
                                egui::Rect::from_min_size(
                                    cpu_bar.min,
                                    egui::vec2(cpu_fill_w, cpu_bar.height()),
                                ),
                                CornerRadius::same(3),
                                self.config.theme.accent_color(),
                            );

                            ui.label(
                                RichText::new(format!("🖥 CPU: {:.0}%", self.cpu_usage))
                                    .monospace()
                                    .size(10.5)
                                    .color(self.config.theme.accent_color()),
                            );

                            ui.separator();
                        }

                        // Allocate remaining middle space of titlebar as draggable region (never overlaps buttons!)
                        let available_w = ui.available_width().max(20.0);
                        let (_drag_rect, drag_resp) = ui.allocate_exact_size(
                            egui::vec2(available_w, ui.available_height().max(18.0)),
                            egui::Sense::click_and_drag(),
                        );
                        if drag_resp.hovered() || drag_resp.dragged() {
                            ctx.set_cursor_icon(egui::CursorIcon::Move);
                        }
                        if drag_resp.drag_started_by(egui::PointerButton::Primary) {
                            ctx.set_cursor_icon(egui::CursorIcon::Move);
                            ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                        }
                        if drag_resp.double_clicked() {
                            self.config.window_maximized = !self.config.window_maximized;
                            ctx.send_viewport_cmd(ViewportCommand::Maximized(
                                self.config.window_maximized,
                            ));
                            #[cfg(windows)]
                            unsafe {
                                let hwnd = win_util::GetActiveWindow();
                                if !hwnd.is_null() {
                                    if self.config.window_maximized {
                                        win_util::ShowWindow(hwnd, win_util::SW_MAXIMIZE);
                                    } else {
                                        win_util::ShowWindow(hwnd, win_util::SW_RESTORE);
                                    }
                                }
                            }
                        }
                    });
                });
            });

        // 7. Secondary Action Toolbar (Dedicated clickable buttons below titlebar)
        egui::Panel::top("toolbar_panel")
            .frame(
                egui::Frame::new()
                    .fill(self.config.theme.panel_bg())
                    .stroke(Stroke::new(
                        1.0,
                        self.config.theme.accent_color().gamma_multiply(0.25),
                    ))
                    .inner_margin(Margin {
                        left: 10,
                        right: 10,
                        top: 6,
                        bottom: 6,
                    }),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let accent = self.config.theme.accent_color();
                    let text_pri = self.config.theme.text_primary();
                    let text_dim = self.config.theme.text_dim();
                    let warn = self.config.theme.warn_color();
                    let is_light = self.config.theme == CyberTheme::Light;

                    // Open File button
                    let open_btn = egui::Button::new(
                        RichText::new(format!("📁 {}", t(self.config.language, "open_file")))
                            .monospace()
                            .strong()
                            .color(text_pri),
                    )
                    .fill(self.config.theme.button_bg())
                    .stroke(Stroke::new(1.2, accent))
                    .corner_radius(CornerRadius::same(6))
                    .min_size(egui::vec2(0.0, 26.0));

                    if ui
                        .add(open_btn)
                        .on_hover_text(t(self.config.language, "open_file_tip"))
                        .clicked()
                    {
                        if let Some(paths) = rfd::FileDialog::new()
                            .add_filter("Log Files (*.log, *.txt, *.*)", &["log", "txt", "*"])
                            .set_title("Open Log Files")
                            .pick_files()
                        {
                            for path in paths {
                                self.open_log_file(path);
                            }
                        }
                    }

                    // Recent Files dropdown (🕒), right after Open File
                    let mut file_to_open = None;
                    // Icon-only button next to Open File; the localized name is its tooltip.
                    let recent_btn =
                        egui::Button::new(RichText::new("🕒").monospace().color(text_pri))
                            .fill(self.config.theme.button_bg())
                            .stroke(Stroke::new(1.2, accent))
                            .corner_radius(CornerRadius::same(6))
                            .min_size(egui::vec2(30.0, 26.0));

                    let recent_menu =
                        egui::menu::MenuButton::from_button(recent_btn).ui(ui, |ui| {
                            if self.config.recent_files.is_empty() {
                                ui.label(
                                    RichText::new(t(self.config.language, "no_recent_files"))
                                        .italics()
                                        .color(self.config.theme.text_dim()),
                                );
                            } else {
                                for path in &self.config.recent_files {
                                    let file_name =
                                        path.file_name().and_then(|n| n.to_str()).unwrap_or("log");
                                    let full_path = path.display().to_string();
                                    let btn_text = format!("📄 {} ({})", file_name, full_path);
                                    if ui.button(RichText::new(btn_text).monospace()).clicked() {
                                        file_to_open = Some(path.clone());
                                        ui.close();
                                    }
                                }
                                ui.separator();
                                if ui
                                    .button(
                                        RichText::new(format!(
                                            "🗑 {}",
                                            t(self.config.language, "clear_recent")
                                        ))
                                        .monospace()
                                        .color(self.config.theme.warn_color()),
                                    )
                                    .clicked()
                                {
                                    self.config.recent_files.clear();
                                    let _ = self.config.save();
                                    ui.close();
                                }
                            }
                        });
                    let _ = recent_menu
                        .0
                        .on_hover_text(t(self.config.language, "recent_files"));
                    if let Some(path) = file_to_open {
                        self.open_log_file(path);
                    }

                    // Open pattern button: tail the newest file matching `dir/app-*.log`
                    let pattern_btn = egui::Button::new(
                        RichText::new("📂*").monospace().strong().color(text_pri),
                    )
                    .fill(self.config.theme.button_bg())
                    .stroke(Stroke::new(1.2, accent))
                    .corner_radius(CornerRadius::same(6))
                    .min_size(egui::vec2(30.0, 26.0));
                    if ui
                        .add(pattern_btn)
                        .on_hover_text(t(self.config.language, "open_pattern_tip"))
                        .clicked()
                    {
                        let seed = self
                            .config
                            .recent_files
                            .first()
                            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
                            .map(|d| Self::default_pattern_for(&d))
                            .unwrap_or_default();
                        self.pattern_prompt = Some(seed);
                    }

                    // Filter button with amber border
                    let active_color_rules = self
                        .config
                        .highlight_rules
                        .iter()
                        .filter(|r| r.enabled && !r.pattern.is_empty())
                        .count();
                    let filt_label = if active_color_rules > 0 {
                        format!(
                            "⚡ {} ({})",
                            t(self.config.language, "highlight_rules"),
                            active_color_rules
                        )
                    } else {
                        format!("⚡ {}", t(self.config.language, "highlight_rules"))
                    };
                    let filt_bg = if is_light {
                        Color32::from_rgb(254, 249, 235)
                    } else {
                        Color32::from_rgb(20, 18, 12)
                    };
                    let filter_btn = egui::Button::new(
                        RichText::new(filt_label).monospace().strong().color(warn),
                    )
                    .fill(filt_bg)
                    .stroke(Stroke::new(1.2, warn))
                    .corner_radius(CornerRadius::same(6))
                    .min_size(egui::vec2(0.0, 26.0));

                    if ui
                        .add(filter_btn)
                        .on_hover_text("Open highlight color rules")
                        .clicked()
                    {
                        self.config.filters_open = !self.config.filters_open;
                        let _ = self.config.save();
                    }

                    // Play button (green border)
                    let play_color = Color32::from_rgb(0, 200, 100);
                    let play_bg = if is_light {
                        Color32::from_rgb(235, 252, 242)
                    } else {
                        Color32::from_rgb(10, 24, 18)
                    };
                    let play_btn = egui::Button::new(
                        RichText::new("▶ Play")
                            .monospace()
                            .strong()
                            .color(play_color),
                    )
                    .fill(play_bg)
                    .stroke(Stroke::new(1.2, play_color))
                    .corner_radius(CornerRadius::same(6))
                    .min_size(egui::vec2(0.0, 26.0));

                    if ui
                        .add(play_btn)
                        .on_hover_text("Resume monitoring and following tail on all streams")
                        .clicked()
                    {
                        for eng in &mut self.engines {
                            eng.is_watching = true;
                            eng.follow_tail = true;
                        }
                        ctx.request_repaint();
                    }

                    // Pause button (red border)
                    let pause_color = Color32::from_rgb(235, 45, 75);
                    let pause_bg = if is_light {
                        Color32::from_rgb(254, 240, 242)
                    } else {
                        Color32::from_rgb(26, 12, 16)
                    };
                    let pause_btn = egui::Button::new(
                        RichText::new("⏸ Pause")
                            .monospace()
                            .strong()
                            .color(pause_color),
                    )
                    .fill(pause_bg)
                    .stroke(Stroke::new(1.2, pause_color))
                    .corner_radius(CornerRadius::same(6))
                    .min_size(egui::vec2(0.0, 26.0));

                    if ui
                        .add(pause_btn)
                        .on_hover_text("Pause monitoring and tail following on all streams")
                        .clicked()
                    {
                        for eng in &mut self.engines {
                            eng.is_watching = false;
                            eng.follow_tail = false;
                        }
                        ctx.request_repaint();
                    }

                    // Settings button
                    let settings_btn = egui::Button::new(
                        RichText::new(format!("⚙ {}", t(self.config.language, "settings")))
                            .monospace()
                            .color(text_dim),
                    )
                    .fill(self.config.theme.button_bg())
                    .stroke(Stroke::new(1.0, text_dim.gamma_multiply(0.6)))
                    .corner_radius(CornerRadius::same(6))
                    .min_size(egui::vec2(0.0, 26.0));

                    if ui
                        .add(settings_btn)
                        .on_hover_text(t(self.config.language, "settings_tip"))
                        .clicked()
                    {
                        self.config.settings_open = !self.config.settings_open;
                        let _ = self.config.save();
                    }

                    // Right-aligned toolbar badges: Help & About with uniform height
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Help (F1)
                        let help_btn =
                            egui::Button::new(RichText::new("❓ Help").monospace().color(text_dim))
                                .fill(self.config.theme.button_bg())
                                .stroke(Stroke::new(1.0, text_dim.gamma_multiply(0.6)))
                                .corner_radius(CornerRadius::same(6))
                                .min_size(egui::vec2(0.0, 26.0));

                        if ui
                            .add(help_btn)
                            .on_hover_text("Shortcuts and Help (F1)")
                            .clicked()
                        {
                            self.config.help_open = !self.config.help_open;
                            let _ = self.config.save();
                        }

                        // About
                        let about_btn =
                            egui::Button::new(RichText::new("ℹ About").monospace().color(text_dim))
                                .fill(self.config.theme.button_bg())
                                .stroke(Stroke::new(1.0, text_dim.gamma_multiply(0.6)))
                                .corner_radius(CornerRadius::same(6))
                                .min_size(egui::vec2(0.0, 26.0));

                        if ui.add(about_btn).on_hover_text("About FastTail").clicked() {
                            self.config.about_open = !self.config.about_open;
                            let _ = self.config.save();
                        }
                    });
                });
            });

        // 7. Render Central Modular Docking Area
        let prev_borderless = self.config.borderless;
        let prev_theme = self.config.theme;
        let prev_lang = self.config.language;
        let mut tab_closed = false;
        let mut labels_changed = false;
        // Keyboard shortcut: Alt + 1..9 to switch focus to tab #1..9
        let mut switch_to_tab = None;
        ctx.input(|i| {
            if i.modifiers.alt && !i.modifiers.ctrl {
                let num_keys = [
                    (egui::Key::Num1, 0),
                    (egui::Key::Num2, 1),
                    (egui::Key::Num3, 2),
                    (egui::Key::Num4, 3),
                    (egui::Key::Num5, 4),
                    (egui::Key::Num6, 5),
                    (egui::Key::Num7, 6),
                    (egui::Key::Num8, 7),
                    (egui::Key::Num9, 8),
                ];
                for (key, idx) in num_keys {
                    if i.key_pressed(key) {
                        switch_to_tab = Some(idx);
                        break;
                    }
                }
            }
        });

        if let Some(idx) = switch_to_tab {
            if idx < self.engines.len() {
                let path = self.engines[idx].path.clone();
                let tab = FastTailTab::LogStream(path);
                if let Some(locator) = self.dock_state.find_tab(&tab) {
                    let _ = self.dock_state.set_active_tab(locator);
                    ctx.request_repaint();
                }
            }
        }

        let current_theme = self.config.theme;

        // 8. Bottom Status Bar Panel (matches screenshot)
        egui::Panel::bottom("status_bar")
            .frame(
                egui::Frame::new()
                    .fill(current_theme.panel_bg())
                    .stroke(Stroke::new(
                        1.0,
                        current_theme.accent_color().gamma_multiply(0.35),
                    ))
                    .inner_margin(Margin {
                        left: 12,
                        right: 12,
                        top: 5,
                        bottom: 5,
                    }),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Small glowing cyan indicator line on the left
                    let (ind_rect, _) =
                        ui.allocate_exact_size(egui::vec2(28.0, 3.0), egui::Sense::hover());
                    ui.painter().rect_filled(
                        ind_rect,
                        CornerRadius::same(2),
                        current_theme.accent_color(),
                    );
                    ui.add_space(4.0);

                    let active_path = if let Some((_, FastTailTab::LogStream(p))) =
                        self.dock_state.find_active_focused()
                    {
                        Some(p.clone())
                    } else {
                        self.dock_state.iter_all_tabs().find_map(|(_, tab)| {
                            if let FastTailTab::LogStream(p) = tab {
                                Some(p.clone())
                            } else {
                                None
                            }
                        })
                    };

                    let path_str = if let Some(path) = active_path {
                        if path.is_absolute() {
                            path.display().to_string()
                        } else if let Ok(cwd) = std::env::current_dir() {
                            cwd.join(&path).display().to_string()
                        } else {
                            path.display().to_string()
                        }
                    } else {
                        "No file open".to_string()
                    };

                    ui.label(
                        RichText::new(path_str)
                            .monospace()
                            .size(11.0)
                            .color(current_theme.text_primary()),
                    );

                    // Renderer chip at the far right: GL / WGPU (+ fallback), details on hover.
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let chip_color = if self.renderer.fallback {
                            current_theme.warn_color()
                        } else {
                            current_theme.secondary_accent()
                        };
                        ui.label(
                            RichText::new(self.renderer.chip())
                                .monospace()
                                .size(10.0)
                                .color(chip_color),
                        )
                        .on_hover_text(format!(
                            "{}\n{}",
                            t(self.config.language, "renderer_tip"),
                            self.renderer.details()
                        ));
                    });
                });
            });

        // 9. Styled Dock Area with Cyber Neon borders
        let mut dock_style = egui_dock::Style::from_egui(ui.style().as_ref());
        dock_style.tab_bar.bg_fill = current_theme.bg_color();
        dock_style.tab_bar.hline_color = current_theme.accent_color().gamma_multiply(0.35);
        dock_style.tab_bar.height = 26.0;

        let active_tab_bg = current_theme.tab_active_bg();
        let inactive_tab_bg = current_theme.tab_inactive_bg();

        dock_style.tab.active.bg_fill = active_tab_bg;
        dock_style.tab.active.outline_color = current_theme.accent_color();
        dock_style.tab.active.corner_radius = CornerRadius {
            nw: 6,
            ne: 6,
            sw: 0,
            se: 0,
        };
        dock_style.tab.active.text_color = current_theme.accent_color();

        dock_style.tab.focused = dock_style.tab.active.clone();

        dock_style.tab.inactive.bg_fill = inactive_tab_bg;
        dock_style.tab.inactive.outline_color = current_theme.border_color().gamma_multiply(0.2);
        dock_style.tab.inactive.corner_radius = CornerRadius {
            nw: 6,
            ne: 6,
            sw: 0,
            se: 0,
        };
        dock_style.tab.inactive.text_color = current_theme.text_dim();

        dock_style.tab.tab_body.stroke =
            Stroke::new(1.5, current_theme.accent_color().gamma_multiply(0.7));
        dock_style.tab.tab_body.corner_radius = CornerRadius::same(6);
        dock_style.tab.tab_body.bg_fill = current_theme.panel_bg();

        dock_style.separator.width = 3.0;
        dock_style.separator.color_idle = current_theme.accent_color().gamma_multiply(0.25);
        dock_style.separator.color_hovered = current_theme.accent_color();
        dock_style.separator.color_dragged = current_theme.accent_color();

        dock_style.buttons.close_tab_color = current_theme.text_dim();
        dock_style.buttons.close_tab_active_color = current_theme.warn_color();

        let mut test_screensaver = false;
        // The stream in the focused dock leaf is the "current window": it alone receives
        // F3 / Shift+F3, Ctrl+F and the keyboard navigation shortcuts.
        let focused_stream = match self.dock_state.find_active_focused() {
            Some((_, FastTailTab::LogStream(p))) => Some(p.clone()),
            Some(_) => None,
            None => match self.dock_state.main_surface_mut().find_active() {
                Some((_, FastTailTab::LogStream(p))) => Some(p.clone()),
                _ => None,
            },
        };
        // Streams drawn this frame set `displayed` again in the tab viewer; the others keep
        // counting unseen lines for the tab badge.
        for eng in &mut self.engines {
            eng.displayed = false;
        }

        // Keyboard shortcuts of the external tools run them on the current row of the
        // focused stream (a modifier is always required, see `Shortcut::parse`).
        let tool_shortcut = ctx.input_mut(|i| {
            self.config
                .external_tools
                .iter()
                .enumerate()
                .find_map(|(n, tool)| {
                    let sc = tool.parsed_shortcut()?;
                    i.consume_key(sc.modifiers(), sc.key).then_some(n)
                })
        });
        if let Some(n) = tool_shortcut {
            let lang = self.config.language;
            let focused = focused_stream
                .as_ref()
                .and_then(|p| self.engines.iter().position(|e| paths_equal(&e.path, p)));
            if let (Some(idx), Some(tool)) = (focused, self.config.external_tools.get(n)) {
                let engine = &mut self.engines[idx];
                if let Some(row) = engine.current_row() {
                    crate::ui::dock::run_tool_on_row(
                        engine,
                        tool,
                        &mut self.tool_runner,
                        row,
                        lang,
                    );
                }
            }
        }

        // Keyboard shortcut: Ctrl + Shift + 1..9 creates or toggles quick colour label N
        // for the current search text of the focused stream.
        const LABEL_KEYS: [Key; 9] = [
            Key::Num1,
            Key::Num2,
            Key::Num3,
            Key::Num4,
            Key::Num5,
            Key::Num6,
            Key::Num7,
            Key::Num8,
            Key::Num9,
        ];
        let label_key = ctx.input_mut(|i| {
            LABEL_KEYS.iter().enumerate().find_map(|(n, key)| {
                i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, *key)
                    .then_some(n as u8 + 1)
            })
        });
        if let Some(color) = label_key {
            let lang = self.config.language;
            let focused = focused_stream
                .as_ref()
                .and_then(|p| self.engines.iter().position(|e| paths_equal(&e.path, p)));
            if let Some(idx) = focused {
                let engine = &mut self.engines[idx];
                let text = engine
                    .current_search_line()
                    .map(|_| engine.last_searched_query.trim().to_string())
                    .filter(|t| !t.is_empty());
                match text {
                    Some(text) => {
                        if QuickLabel::toggle(&mut self.quick_labels, &text, color) {
                            labels_changed = true;
                        }
                    }
                    None => engine.view_notice = Some(t(lang, "label_no_text").to_string()),
                }
            }
        }

        let dock_ctx = DockContext {
            engines: &mut self.engines,
            open_files: &mut self.config.open_files,
            theme: &mut self.config.theme,
            language: &mut self.config.language,
            global_rules: &mut self.config.highlight_rules,
            screensaver_enabled: &mut self.config.screensaver_enabled,
            screensaver_timeout_mins: &mut self.config.screensaver_timeout_mins,
            telemetry_enabled: &mut self.config.telemetry_enabled,
            sound_enabled: &mut self.config.sound_enabled,
            borderless: &mut self.config.borderless,
            show_line_numbers: &mut self.config.show_line_numbers,
            font_size: &mut self.config.font_size,
            level_colors: &mut self.config.level_colors,
            size_unit: &mut self.config.size_unit,
            search_history: &mut self.config.search_history,
            tab_closed: &mut tab_closed,
            test_screensaver: &mut test_screensaver,
            quick_labels: &mut self.quick_labels,
            labels_changed: &mut labels_changed,
            external_tools: &mut self.config.external_tools,
            tool_runner: &mut self.tool_runner,
            focused_stream,
        };

        if self.dock_state.iter_all_tabs().count() == 0 {
            egui::Frame::new()
                .fill(self.config.theme.panel_bg())
                .show(ui, |ui| {
                    ui.centered_and_justified(|ui| {
                        ui.vertical_centered(|ui| {
                            ui.add_space(20.0);
                            ui.label(RichText::new("📂").size(48.0));
                            ui.add_space(10.0);
                            ui.label(
                                RichText::new(t(self.config.language, "no_file_open"))
                                    .monospace()
                                    .size(13.5)
                                    .color(self.config.theme.text_primary()),
                            );
                            ui.add_space(16.0);
                            let open_btn = egui::Button::new(
                                RichText::new(format!(
                                    "📁 {}",
                                    t(self.config.language, "open_file")
                                ))
                                .monospace()
                                .strong()
                                .size(13.0)
                                .color(self.config.theme.accent_color()),
                            )
                            .fill(self.config.theme.button_bg())
                            .stroke(Stroke::new(1.5, self.config.theme.accent_color()))
                            .corner_radius(CornerRadius::same(6))
                            .min_size(egui::vec2(160.0, 32.0));

                            if ui
                                .add(open_btn)
                                .on_hover_text(t(self.config.language, "open_file_tip"))
                                .clicked()
                            {
                                if let Some(paths) = rfd::FileDialog::new()
                                    .add_filter(
                                        "Log Files (*.log, *.txt, *.*)",
                                        &["log", "txt", "*"],
                                    )
                                    .set_title("Open Log Files")
                                    .pick_files()
                                {
                                    for path in paths {
                                        self.open_log_file(path);
                                    }
                                }
                            }

                            if !self.config.recent_files.is_empty() {
                                ui.add_space(16.0);
                                ui.label(
                                    RichText::new(t(self.config.language, "recent_files"))
                                        .monospace()
                                        .size(11.5)
                                        .color(self.config.theme.text_dim()),
                                );
                                ui.add_space(6.0);
                                let mut recent_to_open = None;
                                ui.horizontal_wrapped(|ui| {
                                    for path in self.config.recent_files.iter().take(5) {
                                        let file_name = path
                                            .file_name()
                                            .and_then(|n| n.to_str())
                                            .unwrap_or("log");
                                        let btn = egui::Button::new(
                                            RichText::new(format!("📄 {}", file_name))
                                                .monospace()
                                                .size(11.0)
                                                .color(self.config.theme.secondary_accent()),
                                        )
                                        .fill(self.config.theme.button_bg())
                                        .corner_radius(CornerRadius::same(4));
                                        if ui
                                            .add(btn)
                                            .on_hover_text(path.display().to_string())
                                            .clicked()
                                        {
                                            recent_to_open = Some(path.clone());
                                        }
                                    }
                                });
                                if let Some(path) = recent_to_open {
                                    self.open_log_file(path);
                                }
                            }
                        });
                    });
                });
        } else {
            let mut tab_viewer = FastTailTabViewer { ctx: dock_ctx };
            DockArea::new(&mut self.dock_state)
                .style(dock_style)
                .show_inside(ui, &mut tab_viewer);
        }

        // Record positions and sizes of floating dock windows from egui memory
        for (surf_index, surface) in self.dock_state.iter_surfaces_indexed() {
            if let egui_dock::Surface::Window(..) = surface {
                let id = egui::Id::new(format!("window {surf_index:?}"));
                if let Some(rect) = ctx.memory(|mem| mem.area_rect(id)) {
                    if rect.is_positive()
                        && rect.min.x.is_finite()
                        && rect.min.y.is_finite()
                        && rect.min.x > -10000.0
                        && rect.min.y > -10000.0
                    {
                        self.floating_window_rects.insert(surf_index, rect);
                    }
                }
            }
        }
        prune_floating_window_rects(&self.dock_state, &mut self.floating_window_rects);

        // Persist bookmarks and wrap toggles that changed this frame, and flash the window
        // on a background sound-alert match when the option is on and the window is not
        // focused.
        let mut bookmarks_changed = false;
        let mut critical_in_background = false;
        for eng in &mut self.engines {
            if eng.bookmarks_dirty {
                eng.bookmarks_dirty = false;
                let lines: Vec<usize> = eng.bookmarks.iter().copied().collect();
                self.config.set_bookmarks(&eng.path, &lines);
                bookmarks_changed = true;
            }
            if eng.wrap_dirty {
                eng.wrap_dirty = false;
                self.config.set_wrap(&eng.path, eng.wrap_lines);
                bookmarks_changed = true;
            }
            if !eng.displayed && eng.unseen_severity >= 2 {
                critical_in_background = true;
            }
        }
        if bookmarks_changed {
            let _ = self.config.save();
        }
        let window_focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        if window_focused {
            self.attention_requested = false;
        } else if self.config.flash_on_alert && critical_in_background && !self.attention_requested
        {
            self.attention_requested = true;
            ctx.send_viewport_cmd(ViewportCommand::RequestUserAttention(
                egui::UserAttentionType::Informational,
            ));
        }

        if test_screensaver {
            self.screensaver.is_active = true;
        }

        if tab_closed {
            self.save_dock_layout();
        }

        if labels_changed {
            for eng in &mut self.engines {
                eng.set_quick_labels(&self.quick_labels);
            }
            ctx.request_repaint();
        }

        if self.config.borderless != prev_borderless {
            ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(!self.config.borderless));
            let _ = self.config.save();
        }
        if self.config.theme != prev_theme || self.config.language != prev_lang {
            let _ = self.config.save();
        }

        // 8. Render BareTail Migration Dialog if discovered
        if self.baretail_dialog_open {
            let bt_cfg_opt = self.baretail_config.clone();
            if let Some(bt_cfg) = bt_cfg_opt {
                let theme = self.config.theme;
                let lang = self.config.language;
                let recent_count = bt_cfg.recent_files.len();
                let rules_count = bt_cfg.highlight_rules.len();

                egui::Window::new(
                    RichText::new(format!("⚡ {}", t(lang, "baretail_title")))
                        .monospace()
                        .color(theme.warn_color()),
                )
                .collapsible(false)
                .resizable(false)
                .pivot(egui::Align2::CENTER_CENTER)
                .default_pos(ctx.content_rect().center())
                .frame(
                    egui::Frame::window(&ctx.style_of(ctx.theme()))
                        .fill(theme.bg_color())
                        .stroke(Stroke::new(2.0_f32, theme.border_color())),
                )
                .show(&ctx, |ui| {
                    ui.label(
                        RichText::new(t(lang, "baretail_desc"))
                            .monospace()
                            .color(theme.text_primary()),
                    );
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(format!("• Discovered recent files: {}", recent_count))
                            .monospace()
                            .color(theme.secondary_accent()),
                    );
                    ui.label(
                        RichText::new(format!("• Highlight rules: {}", rules_count))
                            .monospace()
                            .color(theme.secondary_accent()),
                    );
                    ui.add_space(12.0);

                    ui.horizontal(|ui| {
                        if ui
                            .button(
                                RichText::new(t(lang, "baretail_import"))
                                    .monospace()
                                    .strong()
                                    .color(theme.accent_color()),
                            )
                            .clicked()
                        {
                            // Import highlight rules
                            for rule in &bt_cfg.highlight_rules {
                                if !self
                                    .config
                                    .highlight_rules
                                    .iter()
                                    .any(|r| r.pattern == rule.pattern)
                                {
                                    self.config.highlight_rules.push(rule.clone());
                                }
                            }
                            // Open files
                            for p in &bt_cfg.recent_files {
                                self.open_log_file(p.clone());
                            }

                            self.config.baretail_import = false;
                            self.config.baretail_prompt_shown = true;
                            let _ = self.config.save();
                            self.baretail_dialog_open = false;
                        }

                        if ui
                            .button(
                                RichText::new(t(lang, "baretail_skip"))
                                    .monospace()
                                    .color(theme.text_dim()),
                            )
                            .clicked()
                        {
                            self.config.baretail_import = false;
                            self.config.baretail_prompt_shown = true;
                            let _ = self.config.save();
                            self.baretail_dialog_open = false;
                        }
                    });
                });
            }
        }

        // 9. Render Settings Dialog if open (Popup modal)
        if self.config.settings_open {
            let mut is_open = true;
            let theme = self.config.theme;
            let prev_borderless = self.config.borderless;
            let prev_theme = self.config.theme;
            let prev_lang = self.config.language;
            let mut test_screensaver = false;

            let win = egui::Window::new(
                RichText::new(format!("⚙ {}", t(self.config.language, "settings")))
                    .monospace()
                    .color(theme.accent_color()),
            )
            .id(egui::Id::new("fasttail_settings_popup"))
            .open(&mut is_open)
            .resizable(true)
            .frame(
                egui::Frame::window(&ctx.style_of(ctx.theme()))
                    .fill(theme.panel_bg())
                    .stroke(Stroke::new(1.5_f32, theme.border_color())),
            );

            let win = restore_dialog_geometry(
                win,
                &ctx,
                self.config.settings_pos,
                self.config.settings_size,
                |win| win.default_width(460.0).default_height(400.0),
            );

            let resp = win.show(&ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    crate::ui::dock::render_settings_content(
                        ui,
                        &mut self.config.theme,
                        &mut self.config.language,
                        &mut self.config.screensaver_enabled,
                        &mut self.config.screensaver_timeout_mins,
                        &mut test_screensaver,
                        &mut self.config.telemetry_enabled,
                        &mut self.config.sound_enabled,
                        &mut self.config.borderless,
                        &mut self.config.show_line_numbers,
                        &mut self.config.font_size,
                        &mut self.config.level_colors,
                        &mut self.config.external_tools,
                        &self.config.highlight_rules,
                        &mut self.tool_runner,
                    );

                    // Rendering backend: applies at the next start.
                    ui.add_space(6.0);
                    let lang = self.config.language;
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{}:", t(lang, "renderer"))).monospace());
                        egui::ComboBox::from_id_salt("renderer_choice")
                            .selected_text(match self.config.renderer {
                                crate::renderer::RendererChoice::Auto => t(lang, "renderer_auto"),
                                crate::renderer::RendererChoice::Glow => t(lang, "renderer_glow"),
                                crate::renderer::RendererChoice::Wgpu => t(lang, "renderer_wgpu"),
                            })
                            .show_ui(ui, |ui| {
                                for choice in crate::renderer::RendererChoice::ALL {
                                    let label = match choice {
                                        crate::renderer::RendererChoice::Auto => {
                                            t(lang, "renderer_auto")
                                        }
                                        crate::renderer::RendererChoice::Glow => {
                                            t(lang, "renderer_glow")
                                        }
                                        crate::renderer::RendererChoice::Wgpu => {
                                            t(lang, "renderer_wgpu")
                                        }
                                    };
                                    ui.selectable_value(&mut self.config.renderer, choice, label);
                                }
                            });
                    });
                    ui.label(
                        RichText::new(format!(
                            "{} · {} {}",
                            t(lang, "renderer_note"),
                            self.renderer.chip(),
                            self.renderer.details()
                        ))
                        .small()
                        .color(theme.text_primary()),
                    );
                    ui.add_space(6.0);
                    if ui
                        .checkbox(&mut self.config.always_on_top, t(lang, "always_on_top"))
                        .on_hover_text(t(lang, "pin_tip"))
                        .changed()
                    {
                        let _ = self.config.save();
                    }
                    if ui
                        .checkbox(&mut self.config.flash_on_alert, t(lang, "flash_on_alert"))
                        .on_hover_text(t(lang, "flash_on_alert_tip"))
                        .changed()
                    {
                        let _ = self.config.save();
                    }
                });
            });

            capture_dialog_geometry(
                &resp,
                &mut self.config.settings_pos,
                &mut self.config.settings_size,
            );

            if test_screensaver {
                self.screensaver.is_active = true;
            }

            if self.config.settings_open != is_open {
                self.config.settings_open = is_open;
                let _ = self.config.save();
            }

            if self.config.borderless != prev_borderless {
                ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(!self.config.borderless));
                let _ = self.config.save();
            }
            if self.config.theme != prev_theme || self.config.language != prev_lang {
                let _ = self.config.save();
            }
        }

        // 10. Render Filters Dialog if open (Color Filters popup)
        if self.config.filters_open {
            let mut is_open = true;
            let theme = self.config.theme;
            let lang = self.config.language;

            let total_color_rules = self.config.highlight_rules.len();
            let active_color_rules = self
                .config
                .highlight_rules
                .iter()
                .filter(|r| r.enabled && !r.pattern.is_empty())
                .count();

            let popup_title = if active_color_rules > 0 {
                format!(
                    "⚡ {} ({} {})",
                    t(lang, "highlight_rules"),
                    active_color_rules,
                    t(lang, "active_count")
                )
            } else {
                format!("⚡ {} ({})", t(lang, "highlight_rules"), total_color_rules)
            };

            let win = egui::Window::new(
                RichText::new(popup_title)
                    .monospace()
                    .color(theme.warn_color()),
            )
            .id(egui::Id::new("fasttail_filters_popup"))
            .open(&mut is_open)
            .resizable(true)
            .frame(
                egui::Frame::window(&ctx.style_of(ctx.theme()))
                    .fill(theme.panel_bg())
                    .stroke(Stroke::new(1.5_f32, theme.border_color())),
            );

            let win = restore_dialog_geometry(
                win,
                &ctx,
                self.config.filters_pos,
                self.config.filters_size,
                |win| win.default_width(540.0).default_height(460.0),
            );

            let resp = win.show(&ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    crate::ui::dock::render_highlights_content(
                        ui,
                        &mut self.config.highlight_rules,
                        &mut self.engines,
                        &theme,
                        lang,
                    );
                });
            });

            capture_dialog_geometry(
                &resp,
                &mut self.config.filters_pos,
                &mut self.config.filters_size,
            );

            if self.config.filters_open != is_open {
                self.config.filters_open = is_open;
                let _ = self.config.save();
            }
        }

        // 11. Render About Dialog if open
        if self.config.about_open {
            let mut is_open = true;
            let theme = self.config.theme;
            let lang = self.config.language;

            let win = egui::Window::new(
                RichText::new(format!("ℹ {} FastTail", t(lang, "about")))
                    .monospace()
                    .color(theme.accent_color()),
            )
            .id(egui::Id::new("fasttail_about_popup"))
            .open(&mut is_open)
            .resizable(false)
            .frame(
                egui::Frame::window(&ctx.style_of(ctx.theme()))
                    .fill(theme.panel_bg())
                    .stroke(Stroke::new(1.5_f32, theme.border_color())),
            );

            let win = restore_dialog_geometry(
                win,
                &ctx,
                self.config.about_pos,
                self.config.about_size,
                |win| win.default_width(440.0),
            );

            let resp = win.show(&ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("⚡ FASTTAIL")
                            .monospace()
                            .strong()
                            .size(18.0)
                            .color(theme.accent_color()),
                    );
                    ui.label(
                        RichText::new("by Matteo Baccan")
                            .monospace()
                            .size(13.0)
                            .color(theme.text_primary()),
                    );
                    ui.hyperlink_to(
                        RichText::new("www.baccan.it")
                            .monospace()
                            .size(11.5)
                            .color(theme.secondary_accent()),
                        "https://www.baccan.it",
                    )
                    .on_hover_text("https://www.baccan.it");
                });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                egui::Grid::new("about_info_grid")
                    .num_columns(2)
                    .spacing([16.0, 8.0])
                    .show(ui, |ui| {
                        ui.label(RichText::new(t(lang, "about_version")).monospace().strong());
                        ui.label(
                            RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                                .monospace()
                                .color(theme.accent_color()),
                        );
                        ui.end_row();

                        ui.label(RichText::new(t(lang, "about_git_tag")).monospace().strong());
                        ui.label(
                            RichText::new(env!("GIT_TAG"))
                                .monospace()
                                .color(theme.secondary_accent()),
                        );
                        ui.end_row();

                        ui.label(
                            RichText::new(t(lang, "about_build_date"))
                                .monospace()
                                .strong(),
                        );
                        ui.label(
                            RichText::new(env!("BUILD_TIMESTAMP"))
                                .monospace()
                                .color(theme.text_primary()),
                        );
                        ui.end_row();

                        ui.label(
                            RichText::new(t(lang, "about_renderer"))
                                .monospace()
                                .strong(),
                        );
                        ui.label(
                            RichText::new(format!(
                                "{} · {}",
                                self.renderer.chip(),
                                self.renderer.details()
                            ))
                            .monospace()
                            .color(theme.text_primary()),
                        );
                        ui.end_row();

                        ui.label(RichText::new(t(lang, "about_author")).monospace().strong());
                        ui.label(
                            RichText::new("Matteo Baccan")
                                .monospace()
                                .color(theme.text_primary()),
                        );
                        ui.end_row();

                        ui.label(RichText::new("Website").monospace().strong());
                        ui.hyperlink_to(
                            RichText::new("www.baccan.it")
                                .monospace()
                                .color(theme.secondary_accent()),
                            "https://www.baccan.it",
                        )
                        .on_hover_text("https://www.baccan.it");
                        ui.end_row();

                        ui.label(RichText::new(t(lang, "about_repo")).monospace().strong());
                        ui.hyperlink_to(
                            RichText::new("github.com/matteobaccan/FastTail")
                                .monospace()
                                .color(theme.accent_color()),
                            "https://github.com/matteobaccan/FastTail",
                        )
                        .on_hover_text("https://github.com/matteobaccan/FastTail");
                        ui.end_row();

                        ui.label(RichText::new(t(lang, "about_license")).monospace().strong());
                        ui.label(RichText::new("MIT").monospace().color(theme.text_dim()));
                        ui.end_row();
                    });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);
                ui.label(
                    RichText::new(t(lang, "about_tagline"))
                        .monospace()
                        .size(10.5)
                        .color(theme.text_dim()),
                );
            });

            capture_dialog_geometry(
                &resp,
                &mut self.config.about_pos,
                &mut self.config.about_size,
            );

            if self.config.about_open != is_open {
                self.config.about_open = is_open;
                let _ = self.config.save();
            }
        }

        // 12. Render Help Dialog if open
        if self.config.help_open {
            let mut is_open = true;
            let theme = self.config.theme;
            let lang = self.config.language;

            let win = egui::Window::new(
                RichText::new(format!("❓ {}", t(lang, "shortcuts_title")))
                    .monospace()
                    .color(theme.accent_color()),
            )
            .id(egui::Id::new("fasttail_help_popup"))
            .open(&mut is_open)
            .resizable(true)
            .frame(
                egui::Frame::window(&ctx.style_of(ctx.theme()))
                    .fill(theme.panel_bg())
                    .stroke(Stroke::new(1.5_f32, theme.border_color())),
            );

            let win = restore_dialog_geometry(
                win,
                &ctx,
                self.config.help_pos,
                self.config.help_size,
                |win| win.default_width(580.0).default_height(500.0),
            );

            let resp = win.show(&ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(
                            RichText::new("⚡ FASTTAIL")
                                .monospace()
                                .strong()
                                .size(16.0)
                                .color(theme.accent_color()),
                        );
                        ui.label(
                            RichText::new(t(lang, "shortcuts_title"))
                                .monospace()
                                .size(12.0)
                                .color(theme.text_dim()),
                        );
                    });

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);

                    // Category 1: Zoom & Font Size
                    ui.group(|ui| {
                        ui.label(
                            RichText::new(t(lang, "help_cat_zoom"))
                                .monospace()
                                .strong()
                                .color(theme.warn_color()),
                        );
                        ui.separator();
                        egui::Grid::new("help_zoom_grid")
                            .num_columns(2)
                            .spacing([18.0, 6.0])
                            .show(ui, |ui| {
                                ui.label(RichText::new("CTRL +  /  CTRL =").monospace().strong());
                                ui.label(RichText::new(t(lang, "help_zoom_in")).monospace());
                                ui.end_row();

                                ui.label(RichText::new("CTRL -").monospace().strong());
                                ui.label(RichText::new(t(lang, "help_zoom_out")).monospace());
                                ui.end_row();

                                ui.label(RichText::new("CTRL 0").monospace().strong());
                                ui.label(RichText::new(t(lang, "help_zoom_reset")).monospace());
                                ui.end_row();

                                ui.label(RichText::new("CTRL + Wheel").monospace().strong());
                                ui.label(RichText::new(t(lang, "help_zoom_wheel")).monospace());
                                ui.end_row();
                            });
                    });

                    ui.add_space(8.0);

                    // Category 2: Navigazione & Streaming
                    ui.group(|ui| {
                        ui.label(
                            RichText::new(t(lang, "help_cat_nav"))
                                .monospace()
                                .strong()
                                .color(theme.warn_color()),
                        );
                        ui.separator();
                        egui::Grid::new("help_nav_grid")
                            .num_columns(2)
                            .spacing([18.0, 6.0])
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(t(lang, "help_key_space"))
                                        .monospace()
                                        .strong(),
                                );
                                ui.label(RichText::new(t(lang, "help_desc_space")).monospace());
                                ui.end_row();

                                ui.label(RichText::new("CTRL F").monospace().strong());
                                ui.label(RichText::new(t(lang, "help_desc_search")).monospace());
                                ui.end_row();

                                ui.label(RichText::new("F3  /  Shift + F3").monospace().strong());
                                ui.label(RichText::new(t(lang, "help_desc_find_next")).monospace());
                                ui.end_row();

                                ui.label(
                                    RichText::new("Click / Shift + Click / Ctrl + Click")
                                        .monospace()
                                        .strong(),
                                );
                                ui.label(RichText::new(t(lang, "help_desc_select")).monospace());
                                ui.end_row();

                                ui.label(
                                    RichText::new("Ctrl + A  /  Ctrl + C").monospace().strong(),
                                );
                                ui.label(RichText::new(t(lang, "help_desc_copy")).monospace());
                                ui.end_row();

                                ui.label(RichText::new("Ctrl + G").monospace().strong());
                                ui.label(RichText::new(t(lang, "help_desc_goto")).monospace());
                                ui.end_row();

                                ui.label(RichText::new("Alt + W").monospace().strong());
                                ui.label(RichText::new(t(lang, "help_desc_wrap")).monospace());
                                ui.end_row();

                                ui.label(RichText::new("Ctrl + Shift + 1..9").monospace().strong());
                                ui.label(RichText::new(t(lang, "help_desc_labels")).monospace());
                                ui.end_row();

                                ui.label(
                                    RichText::new(t(lang, "help_key_tools"))
                                        .monospace()
                                        .strong(),
                                );
                                ui.label(RichText::new(t(lang, "help_desc_tools")).monospace());
                                ui.end_row();

                                ui.label(RichText::new("Ctrl + Shift + T").monospace().strong());
                                ui.label(RichText::new(t(lang, "pin_tip")).monospace());
                                ui.end_row();

                                ui.label(
                                    RichText::new("Ctrl + F2  /  F2  /  Shift + F2")
                                        .monospace()
                                        .strong(),
                                );
                                ui.label(RichText::new(t(lang, "help_desc_bookmark")).monospace());
                                ui.end_row();

                                ui.label(RichText::new("F1").monospace().strong());
                                ui.label(RichText::new(t(lang, "help_desc_f1")).monospace());
                                ui.end_row();

                                ui.label(RichText::new("Esc").monospace().strong());
                                ui.label(RichText::new(t(lang, "help_desc_esc")).monospace());
                                ui.end_row();

                                ui.label(RichText::new("Drag & Drop").monospace().strong());
                                ui.label(RichText::new(t(lang, "help_desc_drag_drop")).monospace());
                                ui.end_row();
                            });
                    });

                    ui.add_space(8.0);

                    // Category 3: Filtri & Priorità
                    ui.group(|ui| {
                        ui.label(
                            RichText::new(t(lang, "help_cat_filters"))
                                .monospace()
                                .strong()
                                .color(theme.warn_color()),
                        );
                        ui.separator();
                        ui.label(
                            RichText::new(t(lang, "help_filter_order"))
                                .monospace()
                                .color(theme.text_primary()),
                        );
                        ui.label(
                            RichText::new(t(lang, "help_filter_reorder"))
                                .monospace()
                                .color(theme.secondary_accent()),
                        );
                        ui.label(
                            RichText::new(t(lang, "help_filter_styles"))
                                .monospace()
                                .color(theme.accent_color()),
                        );
                        ui.label(
                            RichText::new(t(lang, "help_filter_visibility"))
                                .monospace()
                                .color(theme.text_primary()),
                        );
                        ui.label(
                            RichText::new(t(lang, "help_filter_levels"))
                                .monospace()
                                .color(theme.warn_color()),
                        );
                        ui.label(
                            RichText::new(t(lang, "help_filter_recent"))
                                .monospace()
                                .color(theme.secondary_accent()),
                        );
                    });
                });
            });

            capture_dialog_geometry(&resp, &mut self.config.help_pos, &mut self.config.help_size);

            if self.config.help_open != is_open {
                self.config.help_open = is_open;
                let _ = self.config.save();
            }
        }

        // 11b. "Open pattern" prompt (folder drop, 📂* button)
        if self.pattern_prompt.is_some() {
            let lang = self.config.language;
            let theme = self.config.theme;
            let mut is_open = true;
            let mut submit = false;
            let mut cancel = false;
            let mut error: Option<String> = None;
            let text_id = egui::Id::new("fasttail_pattern_prompt_text");
            egui::Window::new(
                RichText::new(format!("📂* {}", t(lang, "open_pattern")))
                    .monospace()
                    .color(theme.accent_color()),
            )
            .id(egui::Id::new("fasttail_pattern_prompt"))
            .open(&mut is_open)
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(&ctx, |ui| {
                ui.label(
                    RichText::new(t(lang, "open_pattern_desc"))
                        .monospace()
                        .size(11.5)
                        .color(theme.text_dim()),
                );
                ui.add_space(6.0);
                let text = self.pattern_prompt.get_or_insert_with(String::new);
                let resp = ui.add(
                    egui::TextEdit::singleline(text)
                        .hint_text(t(lang, "open_pattern_hint"))
                        .desired_width(420.0)
                        .id(text_id),
                );
                // Take the keyboard focus when the prompt opens, without stealing it later.
                if !resp.has_focus() && ui.ctx().memory(|m| m.focused().is_none()) {
                    resp.request_focus();
                }
                let enter = resp.has_focus()
                    && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
                let esc = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
                let valid = crate::wildcard::split_pattern(std::path::Path::new(text.trim()))
                    .map(|(dir, _)| dir.is_dir())
                    .unwrap_or(false);
                if !text.trim().is_empty() && !valid {
                    error = Some(t(lang, "open_pattern_invalid").to_string());
                }
                if let Some(err) = &error {
                    ui.label(
                        RichText::new(format!("ⓘ {err}"))
                            .monospace()
                            .size(11.0)
                            .color(theme.warn_color()),
                    );
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(valid, egui::Button::new(t(lang, "open_pattern_go")))
                        .clicked()
                        || (enter && valid)
                    {
                        submit = true;
                    }
                    if ui.button(t(lang, "open_pattern_cancel")).clicked() || esc {
                        cancel = true;
                    }
                });
            });
            if submit {
                if let Some(text) = self.pattern_prompt.take() {
                    self.open_log_file(PathBuf::from(text.trim()));
                }
            } else if cancel || !is_open {
                self.pattern_prompt = None;
            }
        }

        // 12. Render Matrix Screensaver if activated
        let viewport = ctx.content_rect();
        self.screensaver.render(&ctx, viewport);

        // 12. Borderless Window Resize Anchors & Visual Frames (Edges & Corners)
        let is_maximized =
            self.config.window_maximized || ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        if self.config.borderless && !is_maximized {
            let screen = ctx.content_rect();
            let border: f32 = 8.0;

            let resize_zones = [
                // 4 Corners (larger hit targets)
                (
                    egui::Rect::from_min_max(
                        screen.min,
                        egui::pos2(screen.min.x + border * 1.5, screen.min.y + border * 1.5),
                    ),
                    egui::ResizeDirection::NorthWest,
                    egui::CursorIcon::ResizeNorthWest,
                ),
                (
                    egui::Rect::from_min_max(
                        egui::pos2(screen.max.x - border * 1.5, screen.min.y),
                        egui::pos2(screen.max.x, screen.min.y + border * 1.5),
                    ),
                    egui::ResizeDirection::NorthEast,
                    egui::CursorIcon::ResizeNorthEast,
                ),
                (
                    egui::Rect::from_min_max(
                        egui::pos2(screen.min.x, screen.max.y - border * 1.5),
                        egui::pos2(screen.min.x + border * 1.5, screen.max.y),
                    ),
                    egui::ResizeDirection::SouthWest,
                    egui::CursorIcon::ResizeSouthWest,
                ),
                (
                    egui::Rect::from_min_max(
                        egui::pos2(screen.max.x - border * 2.0, screen.max.y - border * 2.0),
                        screen.max,
                    ),
                    egui::ResizeDirection::SouthEast,
                    egui::CursorIcon::ResizeSouthEast,
                ),
                // 4 Edges
                (
                    egui::Rect::from_min_max(
                        egui::pos2(screen.min.x + border * 1.5, screen.min.y),
                        egui::pos2(screen.max.x - border * 1.5, screen.min.y + border),
                    ),
                    egui::ResizeDirection::North,
                    egui::CursorIcon::ResizeNorth,
                ),
                (
                    egui::Rect::from_min_max(
                        egui::pos2(screen.min.x + border * 1.5, screen.max.y - border),
                        egui::pos2(screen.max.x - border * 1.5, screen.max.y),
                    ),
                    egui::ResizeDirection::South,
                    egui::CursorIcon::ResizeSouth,
                ),
                (
                    egui::Rect::from_min_max(
                        egui::pos2(screen.min.x, screen.min.y + border * 1.5),
                        egui::pos2(screen.min.x + border, screen.max.y - border * 1.5),
                    ),
                    egui::ResizeDirection::West,
                    egui::CursorIcon::ResizeWest,
                ),
                (
                    egui::Rect::from_min_max(
                        egui::pos2(screen.max.x - border, screen.min.y + border * 1.5),
                        egui::pos2(screen.max.x, screen.max.y - border * 1.5),
                    ),
                    egui::ResizeDirection::East,
                    egui::CursorIcon::ResizeEast,
                ),
            ];

            if let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) {
                // Protect top-right window buttons from resize interception
                let in_window_buttons_zone =
                    pos.x > screen.max.x - 140.0 && pos.y < screen.min.y + 40.0;
                if !in_window_buttons_zone {
                    for (rect, direction, cursor) in resize_zones {
                        if rect.contains(pos) {
                            ctx.set_cursor_icon(cursor);
                            if ctx.input(|i| i.pointer.button_down(egui::PointerButton::Primary)) {
                                ctx.send_viewport_cmd(ViewportCommand::BeginResize(direction));
                            }
                            break;
                        }
                    }
                }
            }

            // Draw 1px subtle cyber border frame & tactile corner grip
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("borderless_overlays"),
            ));
            painter.rect_stroke(
                screen,
                egui::CornerRadius::same(0),
                Stroke::new(1.0_f32, self.config.theme.border_color()),
                egui::StrokeKind::Inside,
            );

            // Tactile diagonal grip in bottom-right corner
            let br = screen.max;
            let grip_color = self.config.theme.accent_color().gamma_multiply(0.8);
            for offset in &[4.0_f32, 8.0_f32, 12.0_f32, 16.0_f32] {
                painter.line_segment(
                    [
                        egui::pos2(br.x - offset, br.y),
                        egui::pos2(br.x, br.y - offset),
                    ],
                    Stroke::new(1.5_f32, grip_color),
                );
            }
        }
    }
}

impl eframe::App for FastTailApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        self.config.theme.bg_color().to_normalized_gamma_f32()
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.render_ui(ui);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_dock_layout();
        let _ = self.config.save();
    }
}
