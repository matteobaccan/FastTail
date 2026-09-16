use crate::baretail_bridge::{detect_baretail_config, BareTailConfig};
use crate::config::FastTailConfig;
use crate::i18n::t;
use crate::screensaver::MatrixScreensaver;
use crate::tail_engine::TailEngine;
use crate::ui::dock::{DockContext, FastTailTab, FastTailTabViewer};
use eframe::egui;
use egui::{Key, RichText, Stroke, ViewportCommand};
use egui_dock::{DockArea, DockState};
use std::path::PathBuf;
use std::time::Instant;
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};

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
    pub search_query: String,
    pub settings_dialog_open: bool,
    pub filters_dialog_open: bool,
    pub about_dialog_open: bool,
    pub help_dialog_open: bool,
    pub is_maximized: bool,
    pub last_dock_save: Instant,
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
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_cjk_fonts(&cc.egui_ctx);
        let config = FastTailConfig::load();
        config.theme.apply(&cc.egui_ctx);

        let mut system = System::new_with_specifics(
            RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory(MemoryRefreshKind::everything()),
        );
        system.refresh_cpu_usage();
        system.refresh_memory();

        // Check for BareTail config on Windows if prompt wasn't shown yet
        let baretail_config = if !config.baretail_prompt_shown {
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
            .unwrap_or_else(|| {
                DockState::new(vec![])
            });

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
            search_query: String::new(),
            settings_dialog_open: false,
            filters_dialog_open: false,
            about_dialog_open: false,
            help_dialog_open: false,
            is_maximized: false,
            last_dock_save: Instant::now(),
        };

        // Open any files saved as open from the previous session
        for path in app.config.open_files.clone() {
            app.open_log_file(path);
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

    pub fn save_dock_layout(&mut self) {
        let mut current_open = Vec::new();
        for (_, tab) in self.dock_state.iter_all_tabs() {
            if let FastTailTab::LogStream(p) = tab {
                if !current_open.contains(p) {
                    current_open.push(p.clone());
                }
            }
        }
        self.config.open_files = current_open;

        if let Ok(ron_str) = ron::to_string(&self.dock_state) {
            if self.config.dock_layout.as_deref() != Some(&ron_str) {
                self.config.dock_layout = Some(ron_str);
            }
        }
        let _ = self.config.save();
    }

    pub fn open_log_file(&mut self, path: PathBuf) {
        if !path.exists() {
            return;
        }

        // Avoid duplicate tabs for same path
        for eng in &self.engines {
            if eng.path == path {
                // Already open, select tab
                let tab = FastTailTab::LogStream(path.clone());
                if let Some(locator) = self.dock_state.find_tab(&tab) {
                    self.dock_state.set_active_tab(locator);
                }
                return;
            }
        }

        if let Ok(mut engine) = TailEngine::open(&path) {
            engine.set_highlight_rules(self.config.highlight_rules.clone());
            engine.size_unit = self.config.size_unit;
            self.engines.push(engine);

            crate::audio::play_sound(crate::audio::CyberSound::BlipAttach, self.config.sound_enabled);

            // Add to open_files
            if !self.config.open_files.contains(&path) {
                self.config.open_files.push(path.clone());
            }

            // Keep recent files in MRU order (most recent at top, max 15)
            self.config.recent_files.retain(|p| p != &path);
            self.config.recent_files.insert(0, path.clone());
            if self.config.recent_files.len() > 15 {
                self.config.recent_files.truncate(15);
            }
            let _ = self.config.save();

            let tab = FastTailTab::LogStream(path);
            let already_in_dock = self.dock_state.find_tab(&tab).is_some();
            if !already_in_dock {
                if self.dock_state.iter_all_tabs().count() == 0 {
                    self.dock_state = egui_dock::DockState::new(vec![tab]);
                } else {
                    self.dock_state
                        .main_surface_mut()
                        .push_to_first_leaf(tab);
                }
            }
            self.save_dock_layout();
        }
    }
}

impl eframe::App for FastTailApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 1. Detect user activity to reset screensaver & handle window closing
        ctx.input(|i| {
            if i.viewport().close_requested() {
                self.save_dock_layout();
            }

            if !i.raw.events.is_empty() {
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
            if i.modifiers.ctrl && i.raw_scroll_delta.y != 0.0 {
                if i.raw_scroll_delta.y > 0.0 {
                    self.config.font_size = (self.config.font_size + 1.0).min(32.0);
                } else {
                    self.config.font_size = (self.config.font_size - 1.0).max(8.0);
                }
                let _ = self.config.save();
            }

            // Keyboard shortcut: F1 (Toggle Help)
            if i.key_pressed(Key::F1) {
                self.help_dialog_open = !self.help_dialog_open;
            }

            // Keyboard shortcut: Escape (Close any open popup)
            if i.key_pressed(Key::Escape) {
                self.help_dialog_open = false;
                self.settings_dialog_open = false;
                self.filters_dialog_open = false;
                self.about_dialog_open = false;
            }

            // Drag & drop file support (single or multiple)
            if !i.raw.dropped_files.is_empty() {
                for file in &i.raw.dropped_files {
                    if let Some(ref path) = file.path {
                        self.open_log_file(path.clone());
                    }
                }
            }
        });

        // Save dock layout periodically every 2 seconds if changed
        if self.last_dock_save.elapsed().as_secs_f32() >= 2.0 {
            self.save_dock_layout();
            self.last_dock_save = Instant::now();
        }

        // 2. Poll file updates
        for eng in &mut self.engines {
            eng.poll_updates();
        }

        // 3. Periodic telemetry refresh
        if self.last_sys_refresh.elapsed().as_secs_f32() >= 1.0 {
            self.system.refresh_cpu_usage();
            self.system.refresh_memory();
            self.cpu_usage = self.system.global_cpu_usage();
            self.mem_used_mb = self.system.used_memory() / (1024 * 1024);
            self.last_sys_refresh = Instant::now();
        }

        // 4. Check screensaver idle timeout
        self.screensaver.check_inactivity(
            self.config.screensaver_timeout_mins,
            self.config.screensaver_enabled,
        );

        // 5. Apply theme visuals
        self.config.theme.apply(ctx);

        // 6. Primary Title Bar (Title, window controls, telemetry, and safe draggable region)
        egui::TopBottomPanel::top("title_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                // Title and Subtitle (no double slashes)
                ui.label(
                    RichText::new("⚡ FASTTAIL by Matteo Baccan")
                        .monospace()
                        .strong()
                        .size(15.0)
                        .color(self.config.theme.accent_color()),
                );

                ui.label(
                    RichText::new(format!("• {}", t(self.config.language, "app_subtitle")))
                        .monospace()
                        .size(11.0)
                        .color(self.config.theme.text_dim()),
                );

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
                        let max_icon = if self.is_maximized { " 🗗 " } else { " 🗖 " };
                        let max_tip = if self.is_maximized {
                            t(self.config.language, "restore_tip")
                        } else {
                            t(self.config.language, "maximize_tip")
                        };
                        if ui
                            .button(RichText::new(max_icon).monospace())
                            .on_hover_text(max_tip)
                            .clicked()
                        {
                            self.is_maximized = !self.is_maximized;
                            ctx.send_viewport_cmd(ViewportCommand::Maximized(self.is_maximized));
                            #[cfg(windows)]
                            unsafe {
                                let hwnd = win_util::GetActiveWindow();
                                if !hwnd.is_null() {
                                    if self.is_maximized {
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

                    if self.config.telemetry_enabled {
                        ui.label(
                            RichText::new(format!(
                                "CPU: {:.1}% | RAM: {}MB",
                                self.cpu_usage, self.mem_used_mb
                            ))
                            .monospace()
                            .size(11.0)
                            .color(self.config.theme.secondary_accent()),
                        );
                        ui.separator();
                    }

                    // Tactile drag handle indicator
                    let drag_handle = ui
                        .label(
                            RichText::new("⠿ DRAG")
                                .monospace()
                                .strong()
                                .color(self.config.theme.accent_color()),
                        )
                        .on_hover_text(t(self.config.language, "drag_tip"));
                    if drag_handle.hovered() {
                        ctx.set_cursor_icon(egui::CursorIcon::Grab);
                    }
                    if drag_handle.drag_started_by(egui::PointerButton::Primary) {
                        ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                    }

                    // Allocate remaining middle space of titlebar as draggable region (never overlaps buttons!)
                    let available_w = ui.available_width().max(20.0);
                    let (_drag_rect, drag_resp) = ui.allocate_exact_size(
                        egui::vec2(available_w, ui.available_height().max(18.0)),
                        egui::Sense::click_and_drag(),
                    );
                    if drag_resp.hovered() {
                        ctx.set_cursor_icon(egui::CursorIcon::Grab);
                    }
                    if drag_resp.drag_started_by(egui::PointerButton::Primary) {
                        ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                    }
                    if drag_resp.double_clicked() {
                        self.is_maximized = !self.is_maximized;
                        ctx.send_viewport_cmd(ViewportCommand::Maximized(self.is_maximized));
                        #[cfg(windows)]
                        unsafe {
                            let hwnd = win_util::GetActiveWindow();
                            if !hwnd.is_null() {
                                if self.is_maximized {
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
        egui::TopBottomPanel::top("toolbar_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                // Open File button (multi-select dialog)
                if ui
                    .button(RichText::new(format!("📂 {}", t(self.config.language, "open_file"))).monospace())
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

                // Recent Files dropdown menu
                let mut file_to_open = None;
                let recent_title = format!("🕒 {}", t(self.config.language, "recent_files"));
                ui.menu_button(RichText::new(recent_title).monospace(), |ui| {
                    if self.config.recent_files.is_empty() {
                        ui.label(
                            RichText::new(t(self.config.language, "no_recent_files"))
                                .italics()
                                .color(self.config.theme.text_dim()),
                        );
                    } else {
                        for path in &self.config.recent_files {
                            let file_name = path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("log");
                            let full_path = path.display().to_string();
                            let btn_text = format!("📄 {} ({})", file_name, full_path);
                            if ui.button(RichText::new(btn_text).monospace()).clicked() {
                                file_to_open = Some(path.clone());
                                ui.close_menu();
                            }
                        }
                        ui.separator();
                        if ui
                            .button(
                                RichText::new(format!("🗑 {}", t(self.config.language, "clear_recent")))
                                    .monospace()
                                    .color(self.config.theme.warn_color()),
                            )
                            .clicked()
                        {
                            self.config.recent_files.clear();
                            let _ = self.config.save();
                            ui.close_menu();
                        }
                    }
                });
                if let Some(path) = file_to_open {
                    self.open_log_file(path);
                }

                ui.separator();

                // Filters button with live active counter (Requirement 2)
                let total_color_rules = self.config.highlight_rules.len();
                let active_color_rules = self.config.highlight_rules.iter().filter(|r| r.enabled && !r.pattern.is_empty()).count();
                let active_stream_filters = self.engines.iter().filter(|e| !e.include_filter.is_empty() || !e.exclude_filter.is_empty()).count();
                let total_active_filters = active_color_rules + active_stream_filters;
                let total_configured_filters = total_color_rules + active_stream_filters;

                let filter_label = if total_active_filters > 0 {
                    format!("⚡ {} ({} {})", t(self.config.language, "filters"), total_active_filters, t(self.config.language, "active_count"))
                } else if total_configured_filters > 0 {
                    format!("⚡ {} (0/{})", t(self.config.language, "filters"), total_configured_filters)
                } else {
                    format!("⚡ {} (0)", t(self.config.language, "filters"))
                };

                let filter_btn = if self.filters_dialog_open {
                    RichText::new(filter_label)
                        .monospace()
                        .color(self.config.theme.warn_color())
                        .strong()
                } else if total_active_filters > 0 {
                    RichText::new(filter_label)
                        .monospace()
                        .color(self.config.theme.accent_color())
                        .strong()
                } else {
                    RichText::new(filter_label).monospace()
                };

                let filter_tip = format!(
                    "{}: {} {} ({} {}, {} {})",
                    t(self.config.language, "filters"),
                    total_active_filters,
                    t(self.config.language, "active_count"),
                    active_color_rules,
                    t(self.config.language, "active_rules_stat"),
                    active_stream_filters,
                    t(self.config.language, "active_stream_stat"),
                );

                if ui.button(filter_btn).on_hover_text(filter_tip).clicked() {
                    self.filters_dialog_open = !self.filters_dialog_open;
                }

                // Settings popup button
                let settings_btn = if self.settings_dialog_open {
                    RichText::new(format!("⚙ {}", t(self.config.language, "settings")))
                        .monospace()
                        .color(self.config.theme.accent_color())
                } else {
                    RichText::new(format!("⚙ {}", t(self.config.language, "settings"))).monospace()
                };
                if ui
                    .button(settings_btn)
                    .on_hover_text(t(self.config.language, "settings_tip"))
                    .clicked()
                {
                    self.settings_dialog_open = !self.settings_dialog_open;
                }

                // About popup button
                let about_title = t(self.config.language, "about");
                let about_btn = if self.about_dialog_open {
                    RichText::new(format!("ℹ {}", about_title)).monospace().color(self.config.theme.accent_color()).strong()
                } else {
                    RichText::new(format!("ℹ {}", about_title)).monospace()
                };
                if ui
                    .button(about_btn)
                    .on_hover_text(t(self.config.language, "about_tip"))
                    .clicked()
                {
                    self.about_dialog_open = !self.about_dialog_open;
                }

                // Help popup button (F1)
                let help_btn = if self.help_dialog_open {
                    RichText::new(format!("❓ {}", t(self.config.language, "help")))
                        .monospace()
                        .color(self.config.theme.accent_color())
                        .strong()
                } else {
                    RichText::new(format!("❓ {}", t(self.config.language, "help"))).monospace()
                };
                if ui
                    .button(help_btn)
                    .on_hover_text(t(self.config.language, "help_tip"))
                    .clicked()
                {
                    self.help_dialog_open = !self.help_dialog_open;
                }
            });
        });

        // 7. Render Central Modular Docking Area
        let prev_borderless = self.config.borderless;
        let prev_theme = self.config.theme;
        let prev_lang = self.config.language;
        let mut tab_closed = false;
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
            size_unit: &mut self.config.size_unit,
            search_query: &mut self.search_query,
            tab_closed: &mut tab_closed,
        };

        let mut tab_viewer = FastTailTabViewer { ctx: dock_ctx };
        DockArea::new(&mut self.dock_state)
            .style(egui_dock::Style::from_egui(ctx.style().as_ref()))
            .show(ctx, &mut tab_viewer);

        if tab_closed {
            self.save_dock_layout();
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
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .frame(
                    egui::Frame::window(&ctx.style())
                        .fill(theme.bg_color())
                        .stroke(Stroke::new(2.0_f32, theme.border_color())),
                )
                .show(ctx, |ui| {
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
                                if !self.config.highlight_rules.iter().any(|r| r.pattern == rule.pattern) {
                                    self.config.highlight_rules.push(rule.clone());
                                }
                            }
                            // Open files
                            for p in &bt_cfg.recent_files {
                                self.open_log_file(p.clone());
                            }

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
                            self.config.baretail_prompt_shown = true;
                            let _ = self.config.save();
                            self.baretail_dialog_open = false;
                        }
                    });
                });
            }
        }

        // 9. Render Settings Dialog if open (Popup modal)
        if self.settings_dialog_open {
            let mut is_open = true;
            let theme = self.config.theme;
            let prev_borderless = self.config.borderless;
            let prev_theme = self.config.theme;
            let prev_lang = self.config.language;

            egui::Window::new(
                RichText::new(format!("⚙ {}", t(self.config.language, "settings")))
                    .monospace()
                    .color(theme.accent_color()),
            )
            .id(egui::Id::new("fasttail_settings_popup"))
            .open(&mut is_open)
            .resizable(true)
            .default_width(460.0)
            .default_height(400.0)
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(theme.panel_bg())
                    .stroke(Stroke::new(1.5_f32, theme.border_color())),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    crate::ui::dock::render_settings_content(
                        ui,
                        &mut self.config.theme,
                        &mut self.config.language,
                        &mut self.config.screensaver_enabled,
                        &mut self.config.screensaver_timeout_mins,
                        &mut self.config.telemetry_enabled,
                        &mut self.config.sound_enabled,
                        &mut self.config.borderless,
                        &mut self.config.show_line_numbers,
                        &mut self.config.font_size,
                    );
                });
            });

            self.settings_dialog_open = is_open;

            if self.config.borderless != prev_borderless {
                ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(!self.config.borderless));
                let _ = self.config.save();
            }
            if self.config.theme != prev_theme || self.config.language != prev_lang {
                let _ = self.config.save();
            }
        }

        // 10. Render Filters Dialog if open (Color Filters & Stream Rules popup)
        if self.filters_dialog_open {
            let mut is_open = true;
            let theme = self.config.theme;
            let lang = self.config.language;

            let total_color_rules = self.config.highlight_rules.len();
            let active_color_rules = self.config.highlight_rules.iter().filter(|r| r.enabled && !r.pattern.is_empty()).count();
            let active_stream_filters = self.engines.iter().filter(|e| !e.include_filter.is_empty() || !e.exclude_filter.is_empty()).count();
            let total_active_filters = active_color_rules + active_stream_filters;

            let popup_title = if total_active_filters > 0 {
                format!("⚡ {} ({} {})", t(lang, "filters"), total_active_filters, t(lang, "active_count"))
            } else {
                format!("⚡ {} ({})", t(lang, "filters"), total_color_rules)
            };

            egui::Window::new(
                RichText::new(popup_title)
                    .monospace()
                    .color(theme.warn_color()),
            )
            .id(egui::Id::new("fasttail_filters_popup"))
            .open(&mut is_open)
            .resizable(true)
            .default_width(540.0)
            .default_height(460.0)
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(theme.panel_bg())
                    .stroke(Stroke::new(1.5_f32, theme.border_color())),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    crate::ui::dock::render_highlights_content(
                        ui,
                        &mut self.config.highlight_rules,
                        &mut self.engines,
                        &theme,
                        lang,
                    );

                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(12.0);

                    crate::ui::dock::render_filters_content(
                        ui,
                        &mut self.engines,
                        &theme,
                        lang,
                    );
                });
            });

            self.filters_dialog_open = is_open;
        }

        // 11. Render About Dialog if open
        if self.about_dialog_open {
            let mut is_open = true;
            let theme = self.config.theme;
            let lang = self.config.language;

            egui::Window::new(
                RichText::new(format!("ℹ {} FastTail", t(lang, "about")))
                    .monospace()
                    .color(theme.accent_color()),
            )
            .id(egui::Id::new("fasttail_about_popup"))
            .open(&mut is_open)
            .resizable(false)
            .default_width(440.0)
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(theme.panel_bg())
                    .stroke(Stroke::new(1.5_f32, theme.border_color())),
            )
            .show(ctx, |ui| {
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

                        ui.label(RichText::new(t(lang, "about_build_date")).monospace().strong());
                        ui.label(
                            RichText::new(env!("BUILD_TIMESTAMP"))
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

                        ui.label(RichText::new(t(lang, "about_repo")).monospace().strong());
                        ui.hyperlink_to(
                            RichText::new("github.com/matteobaccan/FastTail")
                                .monospace()
                                .color(theme.accent_color()),
                            "https://github.com/matteobaccan/FastTail",
                        );
                        ui.end_row();

                        ui.label(RichText::new(t(lang, "about_license")).monospace().strong());
                        ui.label(
                            RichText::new("MIT")
                                .monospace()
                                .color(theme.text_dim()),
                        );
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

            self.about_dialog_open = is_open;
        }

        // 12. Render Help Dialog if open
        if self.help_dialog_open {
            let mut is_open = true;
            let theme = self.config.theme;
            let lang = self.config.language;

            egui::Window::new(
                RichText::new(format!("❓ {}", t(lang, "shortcuts_title")))
                    .monospace()
                    .color(theme.accent_color()),
            )
            .id(egui::Id::new("fasttail_help_popup"))
            .open(&mut is_open)
            .resizable(true)
            .default_width(580.0)
            .default_height(500.0)
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(theme.panel_bg())
                    .stroke(Stroke::new(1.5_f32, theme.border_color())),
            )
            .show(ctx, |ui| {
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
                                ui.label(RichText::new(t(lang, "help_key_space")).monospace().strong());
                                ui.label(RichText::new(t(lang, "help_desc_space")).monospace());
                                ui.end_row();

                                ui.label(RichText::new("CTRL F").monospace().strong());
                                ui.label(RichText::new(t(lang, "help_desc_search")).monospace());
                                ui.end_row();

                                ui.label(RichText::new("F3  /  Shift + F3").monospace().strong());
                                ui.label(RichText::new(t(lang, "help_desc_find_next")).monospace());
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
                            RichText::new(t(lang, "help_filter_recent"))
                                .monospace()
                                .color(theme.secondary_accent()),
                        );
                    });
                });
            });

            self.help_dialog_open = is_open;
        }

        // 12. Render Matrix Screensaver if activated
        let viewport = ctx.screen_rect();
        self.screensaver.render(ctx, viewport);

        // 12. Borderless Window Resize Anchors & Visual Frames (Edges & Corners)
        let is_maximized = self.is_maximized || ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        if self.config.borderless && !is_maximized {
            let screen = ctx.screen_rect();
            let border: f32 = 8.0;

            let resize_zones = [
                // 4 Corners (larger hit targets)
                (
                    egui::Rect::from_min_max(screen.min, egui::pos2(screen.min.x + border * 1.5, screen.min.y + border * 1.5)),
                    egui::ResizeDirection::NorthWest,
                    egui::CursorIcon::ResizeNorthWest,
                ),
                (
                    egui::Rect::from_min_max(egui::pos2(screen.max.x - border * 1.5, screen.min.y), egui::pos2(screen.max.x, screen.min.y + border * 1.5)),
                    egui::ResizeDirection::NorthEast,
                    egui::CursorIcon::ResizeNorthEast,
                ),
                (
                    egui::Rect::from_min_max(egui::pos2(screen.min.x, screen.max.y - border * 1.5), egui::pos2(screen.min.x + border * 1.5, screen.max.y)),
                    egui::ResizeDirection::SouthWest,
                    egui::CursorIcon::ResizeSouthWest,
                ),
                (
                    egui::Rect::from_min_max(egui::pos2(screen.max.x - border * 2.0, screen.max.y - border * 2.0), screen.max),
                    egui::ResizeDirection::SouthEast,
                    egui::CursorIcon::ResizeSouthEast,
                ),
                // 4 Edges
                (
                    egui::Rect::from_min_max(egui::pos2(screen.min.x + border * 1.5, screen.min.y), egui::pos2(screen.max.x - border * 1.5, screen.min.y + border)),
                    egui::ResizeDirection::North,
                    egui::CursorIcon::ResizeNorth,
                ),
                (
                    egui::Rect::from_min_max(egui::pos2(screen.min.x + border * 1.5, screen.max.y - border), egui::pos2(screen.max.x - border * 1.5, screen.max.y)),
                    egui::ResizeDirection::South,
                    egui::CursorIcon::ResizeSouth,
                ),
                (
                    egui::Rect::from_min_max(egui::pos2(screen.min.x, screen.min.y + border * 1.5), egui::pos2(screen.min.x + border, screen.max.y - border * 1.5)),
                    egui::ResizeDirection::West,
                    egui::CursorIcon::ResizeWest,
                ),
                (
                    egui::Rect::from_min_max(egui::pos2(screen.max.x - border, screen.min.y + border * 1.5), egui::pos2(screen.max.x, screen.max.y - border * 1.5)),
                    egui::ResizeDirection::East,
                    egui::CursorIcon::ResizeEast,
                ),
            ];

            if let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) {
                // Protect top-right window buttons from resize interception
                let in_window_buttons_zone = pos.x > screen.max.x - 140.0 && pos.y < screen.min.y + 40.0;
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
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("borderless_overlays")));
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

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_dock_layout();
        let _ = self.config.save();
    }
}
