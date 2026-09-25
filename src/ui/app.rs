use crate::ansi::AnsiMode;
use crate::baretail_bridge::{detect_baretail_config, BareTailConfig};
use crate::config::FastTailConfig;
use crate::external_tools::ToolRunner;
use crate::i18n::t;
use crate::paths::paths_equal;
use crate::screensaver::MatrixScreensaver;
use crate::session::{LoadedSession, Session, StreamEntry};
use crate::tail_engine::{FileEncoding, QuickLabel, TailEngine};
use crate::theme::CyberTheme;
use crate::ui::dock::{DockContext, FastTailTab, FastTailTabViewer};
use eframe::egui;
use egui::{Color32, CornerRadius, Key, Margin, RichText, Stroke, ViewportCommand};
use egui_dock::{DockArea, DockState};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
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
    /// PIN lock: set when the screensaver ends with the lock armed, or on Ctrl+L. While
    /// it is on, a modal covers the window and keyboard shortcuts are dropped — a
    /// deterrent against a passer-by, not a security boundary (see `config::scramble_pin`).
    pub locked: bool,
    pub lock_entry: String,
    pub lock_failed: bool,
    /// Wrong-PIN counter and the cooldown it triggers (see `LockAttempts`).
    pub lock_attempts: LockAttempts,
    /// Screensaver state of the previous frame, to catch the moment it ends.
    pub screensaver_was_active: bool,
    pub system: System,
    pub cpu_usage: f32,
    pub mem_used_mb: u64,
    pub last_sys_refresh: Instant,
    pub baretail_dialog_open: bool,
    pub baretail_config: Option<BareTailConfig>,
    pub last_dock_save: Instant,
    pub first_frame: bool,
    /// egui context used to wake the event loop from the filesystem watcher threads, so an
    /// idle software-rendered window picks up new lines without a periodic repaint.
    pub egui_ctx: egui::Context,
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
    /// INI text of the named session as last saved or loaded; compared with the live
    /// workspace at most once per second to show the `*` in the title bar.
    pub session_saved: String,
    pub session_dirty: bool,
    pub last_dirty_check: Instant,
    /// Session file waiting for the user to confirm discarding unsaved changes.
    pub pending_session_load: Option<PathBuf>,
    /// Streams of the last loaded session that could not be opened, shown once.
    pub session_missing: Option<Vec<PathBuf>>,
    /// Entry picker of a zip archive holding several files, while it is shown.
    pub zip_picker: Option<crate::ui::zip_picker::ZipPicker>,
    /// Why the last compressed file could not be opened (empty zip, no space...), shown
    /// once in a small window.
    pub open_notice: Option<String>,
    /// Window title last sent to the OS, to send it again only when it changes.
    pub title_applied: String,
    /// Timestamp of last live frame render for frame pacing.
    pub last_frame_render: Instant,
    /// Timestamp of last render for pure pointer movement throttling.
    pub last_mouse_render: Instant,
    /// Visuals last applied for the active renderer; reapplied when the software-UI flag
    /// changes so hardware and software rasterizers keep separate styling.
    pub applied_visuals: Option<(bool, CyberTheme)>,
    /// Search across every open stream (Ctrl+Shift+F), shown by the Find results tab.
    pub find_all: crate::find_all::FindAllSession,
}

/// Frame rate the mouse-move throttle targets on a software rasterizer: WARP rasterizes
/// every frame on the CPU, so above a few fps each pointer move costs a visible slice of CPU.
const SOFTWARE_MOUSE_FPS: u32 = 5;

/// Interval in microseconds the pure-pointer-move throttle should sleep for: on a software
/// rasterizer the user setting is tightened to `SOFTWARE_MOUSE_FPS`, but a user value below
/// it (including 0 = no throttling) still wins.
pub fn mouse_throttle_interval_us(is_software_renderer: bool, mouse_throttle_ms: u64) -> u64 {
    if is_software_renderer {
        (1_000_000 / SOFTWARE_MOUSE_FPS.max(1) as u64).min(mouse_throttle_ms.saturating_mul(1000))
    } else {
        mouse_throttle_ms.saturating_mul(1000)
    }
}

/// Applies the visuals for the active renderer: on a software rasterizer (WARP / llvmpipe /
/// VM / RDP) the costly per-frame effects are stripped to cut CPU per frame.
/// Label for the software renderer in the settings combo: the backend name plus a
/// short "not recommended" tag, since it rasterizes on the CPU.
fn software_renderer_label(lang: crate::i18n::Language) -> String {
    format!(
        "{} \u{2014} {}",
        t(lang, "renderer_software"),
        t(lang, "renderer_not_recommended")
    )
}

pub fn apply_renderer_visuals(ctx: &egui::Context, is_software_renderer: bool, theme: CyberTheme) {
    theme.apply(ctx);
    // Feathering (anti-aliasing) is the single most expensive epaint stage on a CPU
    // rasterizer; hard edges on the pixel grid are far cheaper. Set explicitly (not only
    // when software) so switching renderer classes restores the stock behaviour.
    ctx.tessellation_options_mut(|o| o.feathering = !is_software_renderer);
    if !is_software_renderer {
        return;
    }
    // Shadows and hover expansion grow the tessellated area of every window and hovered
    // widget, and rounded corners add feathered tessellation: all negligible on a GPU,
    // measurable on WARP.
    for theme_id in [egui::Theme::Dark, egui::Theme::Light] {
        ctx.style_mut_of(theme_id, |style| {
            let v = &mut style.visuals;
            v.window_shadow = egui::Shadow::NONE;
            v.popup_shadow = egui::Shadow::NONE;
            v.widgets.hovered.expansion = 0.0;
            v.widgets.noninteractive.corner_radius = egui::CornerRadius::same(0);
            v.widgets.inactive.corner_radius = egui::CornerRadius::same(0);
            v.widgets.hovered.corner_radius = egui::CornerRadius::same(0);
            v.widgets.active.corner_radius = egui::CornerRadius::same(0);
            v.widgets.open.corner_radius = egui::CornerRadius::same(0);
            v.window_corner_radius = egui::CornerRadius::same(0);
            v.menu_corner_radius = egui::CornerRadius::same(0);
        });
    }
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

/// Wrong PIN attempts on the lock screen. Three in a row close the prompt for a minute,
/// so guessing a 4-digit PIN costs hours instead of seconds; a correct PIN clears the
/// count. The state is deliberately in memory only: it is a deterrent, and a restart
/// clearing it changes nothing an attacker could not do by editing `fasttail.ini`.
#[derive(Debug, Default, Clone)]
pub struct LockAttempts {
    failures: u32,
    retry_at: Option<Instant>,
}

/// Wrong attempts allowed before the prompt pauses.
pub const LOCK_MAX_FAILURES: u32 = 3;
/// How long the prompt stays closed after those attempts.
pub const LOCK_COOLDOWN: Duration = Duration::from_secs(60);

impl LockAttempts {
    /// Registers a wrong PIN and returns whether it started a cooldown.
    pub fn register_failure(&mut self, now: Instant) -> bool {
        self.failures += 1;
        if self.failures.is_multiple_of(LOCK_MAX_FAILURES) {
            self.retry_at = Some(now + LOCK_COOLDOWN);
            true
        } else {
            false
        }
    }

    /// Time left before the next attempt is accepted, `None` when it is accepted now.
    pub fn cooldown_left(&self, now: Instant) -> Option<Duration> {
        let retry_at = self.retry_at?;
        (retry_at > now).then(|| retry_at - now)
    }

    /// Clears everything after a correct PIN.
    pub fn reset(&mut self) {
        self.failures = 0;
        self.retry_at = None;
    }
}

/// Font size of the lock prompt heading, and of the lines under it (the body text
/// keeps egui's monospace body size, which the measurement has to match).
const LOCK_TITLE_SIZE: f32 = 16.0;
const LOCK_BODY_SIZE: f32 = 14.0;
/// Width the lock prompt never goes below, and the share of the window it never exceeds.
const LOCK_MIN_WIDTH: f32 = 300.0;
const LOCK_MAX_WIDTH_RATIO: f32 = 0.9;

/// Width the lock prompt needs for the language it is drawn in: the widest line it can
/// show, measured with the font it is drawn with, clamped between a comfortable minimum
/// and most of the window. Without this the longest translations (the cooldown message
/// above all) wrapped inside a dialog sized for English.
fn lock_prompt_width(ctx: &egui::Context, lang: crate::i18n::Language) -> f32 {
    let cooldown = t(lang, "lock_cooldown").replace("{secs}", "60");
    let lines: [(String, f32); 5] = [
        (format!("🔒 {}", t(lang, "locked_title")), LOCK_TITLE_SIZE),
        (t(lang, "locked_prompt").to_owned(), LOCK_BODY_SIZE),
        (format!("⏳ {cooldown}"), LOCK_BODY_SIZE),
        (format!("⚠ {}", t(lang, "lock_wrong")), LOCK_BODY_SIZE),
        (t(lang, "lock_unlock").to_owned(), LOCK_BODY_SIZE),
    ];
    let widest = ctx.fonts_mut(|fonts| {
        lines
            .iter()
            .map(|(text, size)| {
                fonts
                    .layout_no_wrap(
                        text.clone(),
                        egui::FontId::monospace(*size),
                        egui::Color32::WHITE,
                    )
                    .size()
                    .x
            })
            .fold(0.0_f32, f32::max)
    });
    // Room for the window frame and a little air on both sides.
    let needed = widest + 48.0;
    let cap = (ctx.content_rect().width() * LOCK_MAX_WIDTH_RATIO).max(LOCK_MIN_WIDTH);
    needed.clamp(LOCK_MIN_WIDTH, cap)
}

/// Background of the locked window: an opaque wash of the theme background with a slow
/// drifting grid and a sweeping glow band, painted in the `Middle` layer so it covers the
/// workspace (drawn in `Background`) while staying under the prompt (`Foreground`).
/// Animated on purpose — a still frame reads as a crash, a moving one reads as locked.
fn paint_lock_backdrop(ctx: &egui::Context, theme: CyberTheme) {
    let rect = ctx.content_rect();
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Middle,
        egui::Id::new("fasttail_lock_backdrop"),
    ));
    painter.rect_filled(rect, 0.0, theme.bg_color());

    let time = ctx.input(|i| i.time) as f32;
    let accent = theme.accent_color();
    let step = 34.0;
    // The grid drifts by one cell over four seconds, so the motion is visible without
    // ever drawing attention away from the PIN box.
    let drift = (time * step / 4.0) % step;
    let line = Stroke::new(1.0, accent.gamma_multiply(0.07));
    let mut x = rect.left() - step + drift;
    while x < rect.right() {
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            line,
        );
        x += step;
    }
    let mut y = rect.top() - step + drift;
    while y < rect.bottom() {
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            line,
        );
        y += step;
    }

    // A soft band sweeping top to bottom every six seconds.
    let sweep_h = 120.0_f32.min(rect.height() * 0.4);
    let travel = rect.height() + sweep_h;
    let head = rect.top() - sweep_h + (time * travel / 6.0) % travel;
    let bands = 14;
    for band in 0..bands {
        let t = band as f32 / bands as f32;
        let top = head + t * sweep_h;
        let alpha = (1.0 - (t * 2.0 - 1.0).abs()) * 0.05;
        painter.rect_filled(
            egui::Rect::from_min_size(
                egui::pos2(rect.left(), top),
                egui::vec2(rect.width(), sweep_h / bands as f32 + 1.0),
            ),
            0.0,
            accent.gamma_multiply(alpha),
        );
    }

    // Keep the animation going at the screensaver's cadence, not at full speed.
    ctx.request_repaint_after(crate::screensaver::FRAME_INTERVAL);
}

/// Events the PIN prompt is allowed to see while the window is locked. Everything the
/// workspace behind could act on is dropped — including bare `Escape`, which otherwise
/// closed the dialog that happened to be open behind the lock, and the function keys.
/// Text, the editing keys of the PIN field and pointer events are kept, so the prompt
/// stays usable.
fn allowed_while_locked(event: &egui::Event) -> bool {
    use egui::{Event, Key};
    match event {
        Event::Key { key, modifiers, .. } => {
            if modifiers.ctrl || modifiers.command || modifiers.alt {
                return false;
            }
            matches!(
                key,
                Key::Enter
                    | Key::Backspace
                    | Key::Delete
                    | Key::ArrowLeft
                    | Key::ArrowRight
                    | Key::Home
                    | Key::End
                    | Key::Tab
            )
        }
        Event::Text(_) | Event::Paste(_) => true,
        Event::Copy | Event::Cut => false,
        _ => true,
    }
}

/// egui paints the dialog chrome with a plain arrow cursor, so the title bar reads as
/// inert even though it drags the dialog and carries the collapse/close buttons. Set the
/// cursor by hand: a move cursor over the drag strip, a pointing hand over the buttons at
/// its two ends (egui lays the title bar out as `[collapse] title [close]`, and the drag
/// widget — `area_id.with("__title_click")` — spans exactly the part between them).
fn apply_dialog_chrome_cursor<R>(
    ctx: &egui::Context,
    resp: &Option<egui::InnerResponse<Option<R>>>,
) {
    let Some(inner) = resp else {
        return;
    };
    let Some(title) = ctx.read_response(inner.response.layer_id.id.with("__title_click")) else {
        return;
    };
    if title.dragged() {
        ctx.set_cursor_icon(egui::CursorIcon::Move);
        return;
    }
    let Some(pos) = ctx.pointer_latest_pos() else {
        return;
    };
    // Ignore dialogs buried under another one.
    if ctx.layer_id_at(pos) != Some(inner.response.layer_id) {
        return;
    }
    let strip = egui::Rect::from_x_y_ranges(inner.response.rect.x_range(), title.rect.y_range());
    if !strip.contains(pos) {
        return;
    }
    ctx.set_cursor_icon(if title.rect.x_range().contains(pos.x) {
        egui::CursorIcon::Move
    } else {
        egui::CursorIcon::PointingHand
    });
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
        // One font per script, appended as fallbacks in this order: no single CJK face
        // covers the other scripts (YaHei carries no Hangul, Malgun no kana), so loading
        // only the first match left the other languages showing tofu boxes.
        let font_candidates = [
            // Simplified Chinese
            "C:\\Windows\\Fonts\\msyh.ttc",
            "C:\\Windows\\Fonts\\simsun.ttc",
            // Traditional Chinese
            "C:\\Windows\\Fonts\\msjh.ttc",
            "C:\\Windows\\Fonts\\mingliu.ttc",
            // Japanese
            "C:\\Windows\\Fonts\\YuGothR.ttc",
            "C:\\Windows\\Fonts\\meiryo.ttc",
            "C:\\Windows\\Fonts\\msgothic.ttc",
            // Korean
            "C:\\Windows\\Fonts\\malgun.ttf",
            "C:\\Windows\\Fonts\\gulim.ttc",
        ];
        let mut fonts = egui::FontDefinitions::default();
        let mut loaded = 0usize;
        for path in &font_candidates {
            let Ok(bytes) = std::fs::read(path) else {
                continue;
            };
            let name = format!("cjk_fallback_{loaded}");
            loaded += 1;
            fonts.font_data.insert(
                name.clone(),
                std::sync::Arc::new(egui::FontData::from_owned(bytes)),
            );
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts.families.entry(family).or_default().push(name.clone());
            }
        }
        if loaded > 0 {
            ctx.set_fonts(fonts);
        }
    }
}

impl FastTailApp {
    pub fn new(cc: &eframe::CreationContext<'_>, cli: crate::cli::CliArgs) -> Self {
        setup_cjk_fonts(&cc.egui_ctx);
        let mut config = FastTailConfig::load();
        // Spools of an instance that crashed or was killed: nobody else will delete them.
        crate::spool::sweep(&config.compressed_settings().spool_dir);
        if cli.fresh {
            // Empty workspace: no restored streams, no saved dock layout.
            config.open_files.clear();
            config.dock_layout = None;
        }
        config.theme.apply(&cc.egui_ctx);
        // egui owns the zoom factor (it scales the whole UI); restore the saved one
        // before the first frame so the window opens at the scale it was left at.
        cc.egui_ctx.set_zoom_factor(
            config
                .zoom_factor
                .clamp(crate::config::MIN_ZOOM, crate::config::MAX_ZOOM),
        );
        let mut app = Self::from_config_with_ctx(config, cc.egui_ctx.clone());
        app.apply_cli(&cli);
        if let Some(file) = &cli.session {
            app.load_session_file(file.clone(), true);
        }
        app.renderer = crate::renderer::ActiveRenderer::from_creation_context(cc);
        crate::renderer::mark_app_created();
        // On a software rasterizer the costly per-frame effects are stripped here, before
        // the first frame; `render_ui` keeps it applied when the theme changes.
        app.applied_visuals = Some((app.renderer.is_software(), app.config.theme));
        apply_renderer_visuals(&cc.egui_ctx, app.renderer.is_software(), app.config.theme);
        eprintln!(
            "renderer: running on {} ({})",
            app.renderer.chip(),
            app.renderer.details()
        );
        app
    }

    pub fn from_config(config: FastTailConfig) -> Self {
        Self::from_config_with_ctx(config, egui::Context::default())
    }

    /// Builds the app around a given egui context (used by the filesystem watchers to wake
    /// the event loop). Tests go through `from_config` with a fresh default context.
    pub fn from_config_with_ctx(config: FastTailConfig, egui_ctx: egui::Context) -> Self {
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
            locked: false,
            lock_entry: String::new(),
            lock_failed: false,
            lock_attempts: LockAttempts::default(),
            screensaver_was_active: false,
            system,
            cpu_usage: 0.0,
            mem_used_mb: 0,
            last_sys_refresh: Instant::now(),
            baretail_dialog_open,
            baretail_config,
            last_dock_save: Instant::now(),
            first_frame: true,
            egui_ctx,
            floating_window_rects,
            renderer: crate::renderer::ActiveRenderer::unknown(),
            applied_on_top: false,
            attention_requested: false,
            quick_labels: Vec::new(),
            pattern_prompt: None,
            tool_runner: ToolRunner::default(),
            session_saved: String::new(),
            session_dirty: false,
            last_dirty_check: Instant::now(),
            pending_session_load: None,
            session_missing: None,
            zip_picker: None,
            open_notice: None,
            title_applied: String::new(),
            last_frame_render: Instant::now(),
            last_mouse_render: Instant::now(),
            applied_visuals: None,
            find_all: crate::find_all::FindAllSession::default(),
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
                if !is_pattern && !crate::compressed::source_exists(&path) {
                    continue;
                }
                // A pattern tab resolves to the newest match again at every start; a
                // compressed one is decompressed again in the background.
                let opened = app.open_engine(&path);
                if let Some(Ok(mut engine)) = opened {
                    engine.size_check_interval =
                        std::time::Duration::from_millis(app.config.size_check_interval_ms as u64);
                    engine
                        .set_markdown_max_bytes((app.config.markdown_max_mb as u64) * 1024 * 1024);
                    engine.set_highlight_rules(app.config.highlight_rules.clone());
                    engine.size_unit = app.config.size_unit;
                    engine.wrap_lines = app.config.wrap_for(&path);
                    restore_bookmarks(&mut engine, &app.config, &path);
                    apply_stream_state(&mut engine, &app.config);
                    app.engines.push(engine);
                }
            }
        } else {
            // Clean dock layout: open previously saved files into dock
            for path in app.config.open_files.clone() {
                if crate::compressed::source_exists(&path)
                    || crate::wildcard::is_pattern_path(&path)
                {
                    app.open_log_file(path);
                }
            }
        }

        // A named session restored from the previous run: what is on screen now is what
        // fasttail.ini kept, which may already differ from the file; the snapshot is
        // taken from the file itself so the `*` is right from the first frame.
        if let Some(file) = app.config.current_session.clone() {
            match Session::load_from(&file) {
                Ok(loaded) => {
                    app.session_saved = loaded.session.serialized(file.parent());
                }
                Err(_) => app.config.current_session = None,
            }
        }

        app
    }

    // ----- Named sessions -----------------------------------------------------------

    /// Directory the current session file lives in (relative paths are computed to it).
    fn session_base_dir(&self) -> Option<PathBuf> {
        self.config
            .current_session
            .as_ref()
            .and_then(|f| f.parent().map(Path::to_path_buf))
    }

    /// The dock layout as RON, with the recorded floating window rectangles applied.
    fn dock_layout_ron(&mut self) -> Option<String> {
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
        // Results are not persisted: the Find results tab is left out (after the window
        // rects are applied, since dropping it may drop a floating window).
        let dock_to_save = crate::ui::find_results::without_find_results(&dock_to_save);
        ron::to_string(&dock_to_save).ok()
    }

    /// The live workspace as a session: streams in dock order with their filters,
    /// search query, wrap, encoding and bookmarks, plus the dock layout.
    pub fn capture_session(&mut self) -> Session {
        let mut paths: Vec<PathBuf> = Vec::new();
        for (_, tab) in self.dock_state.iter_all_tabs() {
            if let FastTailTab::LogStream(p) = tab {
                if !paths.iter().any(|e| paths_equal(e, p)) {
                    paths.push(p.clone());
                }
            }
        }
        let streams = paths
            .iter()
            .filter_map(|p| {
                self.engines
                    .iter()
                    .find(|e| paths_equal(&e.path, p))
                    .map(stream_entry_of)
            })
            .collect();
        Session {
            streams,
            dock_layout: self.dock_layout_ron(),
        }
    }

    /// The live workspace as text for the unsaved-changes check. The dock layout is
    /// represented by its structure (surfaces, nodes and tab order) rather than by the
    /// RON, whose node rectangles change at every resize without the user doing anything.
    fn session_fingerprint(&mut self) -> String {
        let base = self.session_base_dir();
        let mut session = self.capture_session();
        session.dock_layout = Some(dock_signature(
            &crate::ui::find_results::without_find_results(&self.dock_state),
        ));
        session.serialized(base.as_deref())
    }

    /// Records the live workspace as the saved state of the current session.
    fn refresh_session_snapshot(&mut self) {
        self.session_saved = self.session_fingerprint();
        self.session_dirty = false;
        self.last_dirty_check = Instant::now();
    }

    /// Compares the live workspace with the saved session, at most once per second.
    fn check_session_dirty(&mut self) {
        if self.config.current_session.is_none() {
            self.session_dirty = false;
            return;
        }
        if self.last_dirty_check.elapsed().as_secs_f32() < 1.0 {
            return;
        }
        self.last_dirty_check = Instant::now();
        let now = self.session_fingerprint();
        self.session_dirty = now != self.session_saved;
    }

    /// Text shown after the product name: ` · name` with a `*` when unsaved.
    pub fn session_title_suffix(&self) -> String {
        match &self.config.current_session {
            Some(file) => format!(
                " · {}{}",
                Session::name_of(file),
                if self.session_dirty { "*" } else { "" }
            ),
            None => String::new(),
        }
    }

    /// Saves the live workspace to `file` and makes it the current session.
    pub fn save_session_as(&mut self, file: PathBuf) -> std::io::Result<()> {
        let session = self.capture_session();
        session.save_to(&file)?;
        self.config.current_session = Some(file.clone());
        self.config.add_recent_session(&file);
        let _ = self.config.save();
        self.refresh_session_snapshot();
        Ok(())
    }

    /// Saves the live workspace to the current session file, if any.
    pub fn save_session(&mut self) -> std::io::Result<()> {
        match self.config.current_session.clone() {
            Some(file) => self.save_session_as(file),
            None => Ok(()),
        }
    }

    /// Makes the live workspace the default one (fasttail.ini) and leaves the named session.
    pub fn save_session_as_default(&mut self) {
        let session = self.capture_session();
        session.apply_to_config(&mut self.config);
        self.config.current_session = None;
        let _ = self.config.save();
        self.refresh_session_snapshot();
    }

    /// Loads `file`, replacing the workspace. Unless `force`, a named session with unsaved
    /// changes asks for confirmation first.
    pub fn load_session_file(&mut self, file: PathBuf, force: bool) {
        if !force && self.config.current_session.is_some() {
            self.last_dirty_check = Instant::now() - std::time::Duration::from_secs(2);
            self.check_session_dirty();
            if self.session_dirty {
                self.pending_session_load = Some(file);
                return;
            }
        }
        match Session::load_from(&file) {
            Ok(loaded) => self.replace_workspace(loaded, Some(file)),
            Err(err) => {
                eprintln!("fasttail: cannot load session {}: {err}", file.display());
                self.session_missing = Some(vec![file]);
            }
        }
    }

    /// Closes every stream and opens the ones of `loaded`, restoring their state and, when
    /// its tabs match, the saved dock layout.
    pub fn replace_workspace(&mut self, loaded: LoadedSession, file: Option<PathBuf>) {
        self.engines.clear();
        self.dock_state = DockState::new(vec![]);
        self.floating_window_rects.clear();
        self.config.open_files.clear();
        self.config.streams.clear();
        for entry in &loaded.session.streams {
            self.config.set_stream_state(entry.clone());
            self.config.set_wrap(&entry.path, entry.wrap);
            self.config.set_bookmarks(&entry.path, &entry.bookmarks);
            self.open_log_file(entry.path.clone());
        }
        if let Some(layout) = &loaded.session.dock_layout {
            if let Ok(ds) = ron::from_str::<DockState<FastTailTab>>(layout) {
                let tabs: Vec<PathBuf> = ds
                    .iter_all_tabs()
                    .filter_map(|(_, t)| match t {
                        FastTailTab::LogStream(p) => Some(p.clone()),
                        _ => None,
                    })
                    .collect();
                let matches = tabs.len() == self.engines.len()
                    && tabs
                        .iter()
                        .all(|t| self.engines.iter().any(|e| paths_equal(&e.path, t)));
                if matches {
                    for (surf_index, surface) in ds.iter_surfaces_indexed() {
                        if let egui_dock::Surface::Window(_tree, ws) = surface {
                            let r = ws.rect();
                            if r.is_positive() && r.min.x > -10000.0 && r.min.y > -10000.0 {
                                self.floating_window_rects.insert(surf_index, r);
                            }
                        }
                    }
                    self.dock_state = ds;
                }
            }
        }
        self.config.current_session = file.clone();
        if let Some(f) = &file {
            self.config.add_recent_session(f);
        }
        self.save_dock_layout();
        self.refresh_session_snapshot();
        if !loaded.missing.is_empty() {
            self.session_missing = Some(loaded.missing);
        }
    }

    /// Keeps every engine's set of tool-bound rule patterns in sync with the tools list
    /// and runs the tools whose rule matched appended lines, through the throttled runner.
    pub fn run_rule_bound_tools(&mut self) {
        if self.config.external_tools.is_empty() {
            for eng in &mut self.engines {
                if !eng.tool_bound_rules.is_empty() {
                    eng.tool_bound_rules.clear();
                }
                eng.pending_tool_hits.clear();
            }
            return;
        }
        let any_bound = self
            .config
            .external_tools
            .iter()
            .any(|t| t.bound_rule.is_some());
        if !any_bound {
            for eng in &mut self.engines {
                if !eng.tool_bound_rules.is_empty() {
                    eng.tool_bound_rules.clear();
                }
                eng.pending_tool_hits.clear();
            }
            return;
        }

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

    /// Puts the window behind the PIN. Does nothing when no PIN is set, so the user can
    /// never lock themselves out of a lock they cannot open.
    pub fn lock(&mut self) {
        if self.config.lock_pin.is_empty() {
            return;
        }
        self.locked = true;
        self.lock_entry.clear();
        self.lock_failed = false;
    }

    /// The PIN prompt shown while `locked`, as a modal that blocks the UI behind it.
    /// `crate::config::LOCK_BACKDOOR` opens it whatever the PIN is.
    /// The PIN prompt shown while `locked`, over an animated opaque backdrop that hides
    /// the workspace. `crate::config::LOCK_BACKDOOR` opens it whatever the PIN is, and
    /// three wrong PINs in a row close the prompt for `LOCK_COOLDOWN`.
    fn render_lock_overlay(&mut self, ctx: &egui::Context) {
        let theme = self.config.theme;
        let lang = self.config.language;
        paint_lock_backdrop(ctx, theme);
        let cooldown = self.lock_attempts.cooldown_left(Instant::now());
        if let Some(left) = cooldown {
            // Keep the countdown ticking even when nothing else asks for a frame.
            ctx.request_repaint_after(Duration::from_millis(250).min(left));
        }

        // The prompt is translated into sixteen languages, and "Too many attempts: try
        // again in 60 s" is far wider in some of them than in English: measure the text
        // that will actually be drawn and size the dialog from it, instead of a fixed
        // width the longest translation wraps out of.
        let width = lock_prompt_width(ctx, lang);

        egui::Modal::new(egui::Id::new("fasttail_lock_modal"))
            // The backdrop is painted by `paint_lock_backdrop` in a lower layer, so the
            // modal's own one must not dim it a second time.
            .backdrop_color(egui::Color32::TRANSPARENT)
            .frame(
                egui::Frame::window(&ctx.style_of(ctx.theme()))
                    .fill(theme.panel_bg())
                    .stroke(Stroke::new(2.0, theme.border_color())),
            )
            .show(ctx, |ui| {
                ui.set_min_width(width);
                ui.vertical_centered(|ui| {
                    ui.add_space(4.0);
                    ui.add(
                        egui::Label::new(
                            RichText::new(format!("🔒 {}", t(lang, "locked_title")))
                                .monospace()
                                .strong()
                                .size(LOCK_TITLE_SIZE)
                                .color(theme.accent_color()),
                        )
                        .wrap_mode(egui::TextWrapMode::Extend),
                    );
                    ui.add_space(6.0);
                    ui.add(
                        egui::Label::new(
                            RichText::new(t(lang, "locked_prompt"))
                                .monospace()
                                .color(theme.text_dim()),
                        )
                        .wrap_mode(egui::TextWrapMode::Extend),
                    );
                    ui.add_space(10.0);

                    if let Some(left) = cooldown {
                        // While the prompt is paused there is nothing to type into: show
                        // the countdown instead of a field that would refuse every entry.
                        self.lock_entry.clear();
                        ui.add(
                            egui::Label::new(
                                RichText::new(format!(
                                    "⏳ {}",
                                    t(lang, "lock_cooldown").replace(
                                        "{secs}",
                                        &left.as_secs().saturating_add(1).to_string()
                                    )
                                ))
                                .monospace()
                                .strong()
                                .color(theme.warn_color()),
                            )
                            .wrap_mode(egui::TextWrapMode::Extend),
                        );
                        ui.add_space(4.0);
                        return;
                    }

                    let entry = ui.add(
                        egui::TextEdit::singleline(&mut self.lock_entry)
                            .password(true)
                            .desired_width(180.0)
                            .horizontal_align(egui::Align::Center)
                            .font(egui::TextStyle::Monospace),
                    );
                    if !entry.has_focus() && ui.ctx().memory(|m| m.focused().is_none()) {
                        entry.request_focus();
                    }
                    // While locked every other consumer of the keyboard is muted, so a
                    // plain Enter can only mean "confirm this PIN" — checking it directly
                    // is more reliable than waiting for the field to lose focus.
                    let submitted = ui.input(|i| i.key_pressed(egui::Key::Enter));
                    ui.add_space(8.0);
                    let unlock_clicked = ui
                        .button(
                            RichText::new(t(lang, "lock_unlock"))
                                .monospace()
                                .strong()
                                .color(theme.accent_color()),
                        )
                        .on_hover_text(t(lang, "lock_unlock_tip"))
                        .clicked();

                    if submitted || unlock_clicked {
                        if crate::config::pin_matches(&self.config.lock_pin, &self.lock_entry) {
                            self.locked = false;
                            self.lock_failed = false;
                            self.lock_entry.clear();
                            self.lock_attempts.reset();
                            self.screensaver.on_user_input();
                        } else {
                            self.lock_failed = true;
                            self.lock_entry.clear();
                            self.lock_attempts.register_failure(Instant::now());
                            entry.request_focus();
                            crate::audio::play_sound(
                                crate::audio::CyberSound::BeepError,
                                self.config.sound_enabled,
                            );
                        }
                    }

                    if self.lock_failed {
                        ui.add_space(6.0);
                        ui.add(
                            egui::Label::new(
                                RichText::new(format!("⚠ {}", t(lang, "lock_wrong")))
                                    .monospace()
                                    .color(theme.warn_color()),
                            )
                            .wrap_mode(egui::TextWrapMode::Extend),
                        );
                    }
                    ui.add_space(4.0);
                });
            });
    }

    /// Opens or focuses the Find results tab; a non-empty `query` replaces the text of
    /// its query box, which takes the keyboard.
    pub fn open_find_results(&mut self, query: Option<String>) {
        if let Some(q) = query
            .map(|q| q.trim().to_string())
            .filter(|q| !q.is_empty())
        {
            self.find_all.input = q;
        }
        self.find_all.focus_input = true;
        crate::ui::find_results::open_find_results_tab(&mut self.dock_state);
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

        // Per-stream state of the default session (filters, search, encoding).
        for eng in &self.engines {
            let mut entry = stream_entry_of(eng);
            entry.wrap = false;
            entry.bookmarks.clear();
            self.config.set_stream_state(entry);
        }
        let open = self.config.open_files.clone();
        self.config.retain_stream_state_of(&open);

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
        let dock_to_save = crate::ui::find_results::without_find_results(&dock_to_save);

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
                    engine.follow_tail = follow && !engine.is_compressed();
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

    /// Waker handed to every stream: repaint as soon as its filesystem watcher fires, so
    /// an idle software-rendered window drops the periodic present loop (see `render_ui`).
    fn make_wake(ctx: &egui::Context) -> crate::tail_engine::WakeFn {
        let ctx = ctx.clone();
        std::sync::Arc::new(move || ctx.request_repaint()) as crate::tail_engine::WakeFn
    }

    /// Opens the engine for `path`: a pattern stream, a decompressed gzip file or zip
    /// entry, or a plain file. `None` for a zip archive, whose entries are chosen first.
    fn open_engine(&self, path: &Path) -> Option<Result<TailEngine, crate::compressed::OpenError>> {
        use crate::compressed::{OpenError, Target};
        let wake = Self::make_wake(&self.egui_ctx);
        if crate::wildcard::is_pattern_path(path) {
            return Some(TailEngine::open_pattern_with_wake(path, wake).map_err(OpenError::Io));
        }
        let settings = self.config.compressed_settings();
        match crate::compressed::classify(path) {
            Target::Plain => Some(TailEngine::open_with_wake(path, wake).map_err(OpenError::Io)),
            Target::Gzip => Some(crate::compressed::open_engine(
                path,
                None,
                &settings,
                Some(wake),
            )),
            Target::ZipEntry { archive, entry } => Some(crate::compressed::open_engine(
                &archive,
                Some(&entry),
                &settings,
                Some(wake),
            )),
            Target::ZipArchive | Target::EmptyZip => None,
        }
    }

    /// A zip archive was opened: one file entry opens directly, several open the entry
    /// picker, none is reported.
    fn open_zip_archive(&mut self, archive: PathBuf) {
        let lang = self.config.language;
        let entries = match crate::compressed::list_zip_entries(&archive) {
            Ok(entries) => entries,
            Err(err) => {
                self.open_notice = Some(format!("{}: {err}", archive.display()));
                return;
            }
        };
        match entries.as_slice() {
            [] => {
                self.open_notice = Some(format!("{}: {}", archive.display(), t(lang, "zip_empty")));
            }
            [only] if only.refusal.is_none() => {
                let path = crate::compressed::entry_path(&archive, &only.name);
                self.open_log_file(path);
            }
            _ => {
                self.zip_picker = Some(crate::ui::zip_picker::ZipPicker::new(archive, entries));
            }
        }
    }

    /// Text of a failed compressed open, in the UI language.
    fn open_error_text(&self, path: &Path, err: &crate::compressed::OpenError) -> String {
        use crate::compressed::OpenError;
        let lang = self.config.language;
        let reason = match err {
            OpenError::Io(e) => e.to_string(),
            OpenError::NotEnoughSpace { volume, needed } => t(lang, "compressed_no_space")
                .replace("{volume}", volume)
                .replace("{size}", &crate::ui::zip_picker::human_size(*needed)),
            OpenError::Refused(refusal) => crate::ui::zip_picker::refusal_text(lang, refusal),
            OpenError::NoSuchEntry => t(lang, "zip_no_entry").to_string(),
        };
        format!("{}: {reason}", path.display())
    }

    pub fn open_log_file(&mut self, path: PathBuf) {
        let is_pattern = crate::wildcard::is_pattern_path(&path);
        if !is_pattern && !crate::compressed::source_exists(&path) {
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

        let opened = match self.open_engine(&path) {
            Some(opened) => opened,
            None => {
                if crate::compressed::sniff(&path) == crate::compressed::Format::EmptyZip {
                    self.open_notice = Some(format!(
                        "{}: {}",
                        path.display(),
                        t(self.config.language, "zip_empty")
                    ));
                } else {
                    self.open_zip_archive(path);
                }
                return;
            }
        };
        if let Err(err) = &opened {
            // A plain file that fails to open is skipped silently, as it always was; a
            // compressed one says why.
            if !is_pattern && crate::compressed::classify(&path) != crate::compressed::Target::Plain
            {
                self.open_notice = Some(self.open_error_text(&path, err));
            }
        }
        if let Ok(mut engine) = opened {
            engine.size_check_interval =
                std::time::Duration::from_millis(self.config.size_check_interval_ms as u64);
            engine.set_markdown_max_bytes((self.config.markdown_max_mb as u64) * 1024 * 1024);
            engine.set_highlight_rules(self.config.highlight_rules.clone());
            engine.set_quick_labels(&self.quick_labels);
            engine.size_unit = self.config.size_unit;
            restore_bookmarks(&mut engine, &self.config, &path);
            engine.wrap_lines = self.config.wrap_for(&path);
            apply_stream_state(&mut engine, &self.config);
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

        // 0. Handle initial maximize / minimize on Windows / viewport
        if self.first_frame {
            self.first_frame = false;
            if self.config.window_minimized {
                // Reopen the way it was closed. The command is sent after the first frame
                // has been laid out, so the restored geometry is the saved one and not
                // whatever the window manager gives a window that starts iconified.
                ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
                #[cfg(windows)]
                unsafe {
                    let hwnd = win_util::GetActiveWindow();
                    if !hwnd.is_null() {
                        win_util::ShowWindow(hwnd, win_util::SW_MINIMIZE);
                    }
                }
            }
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

        // 0. While the window is locked, strip every event the workspace could act on
        // before anything reads the input. This has to happen first: `key_pressed` counts
        // matching events, so filtering them afterwards would leave the handlers above
        // having already acted — which is how a bare Escape kept closing the Settings
        // dialog sitting behind the lock.
        if self.locked {
            ctx.input_mut(|i| i.events.retain(allowed_while_locked));
        }

        // 1. Detect user activity to reset screensaver & handle window closing and viewport bounds
        let mut escape_pressed = false;
        let mut wheel_zoom = 1.0_f32;
        ctx.input(|i| {
            if i.viewport().close_requested() {
                self.save_dock_layout();
            }

            if let Some(maximized) = i.viewport().maximized {
                self.config.window_maximized = maximized;
            }
            if let Some(minimized) = i.viewport().minimized {
                self.config.window_minimized = minimized;
            }

            // A minimized window reports a placeholder geometry on Windows; keep the
            // last real one instead of saving that.
            if !self.config.window_maximized && !self.config.window_minimized {
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
                // A compressed stream is a static snapshot: follow stays off.
                for eng in self.engines.iter_mut().filter(|e| !e.is_compressed()) {
                    eng.follow_tail = !eng.follow_tail;
                }
            }

            // Zoom: `Ctrl +`, `Ctrl -` and `Ctrl 0` are applied by egui itself (it calls
            // `gui_zoom::zoom_with_keyboard` every frame), so handling them here as well
            // would zoom twice — once the whole UI, once the log font — which is exactly
            // why the Settings buttons looked like they did something else. `Ctrl + wheel`
            // is the one egui does not apply: it turns the wheel into `zoom_delta` and
            // leaves `smooth_scroll_delta` empty, so nothing happened at all. Only read it
            // here: the context must not be touched while its input lock is held.
            wheel_zoom = i.zoom_delta();

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
        self.check_session_dirty();
        let window_title = format!(
            "FastTail v{} by Matteo Baccan{}",
            env!("CARGO_PKG_VERSION"),
            self.session_title_suffix()
        );
        if window_title != self.title_applied {
            ctx.send_viewport_cmd(ViewportCommand::Title(window_title.clone()));
            self.title_applied = window_title;
        }

        // 2. Poll file updates, then run the tools bound to the rules that matched
        let size_interval =
            std::time::Duration::from_millis(self.config.size_check_interval_ms as u64);
        for eng in &mut self.engines {
            eng.size_check_interval = size_interval;
            eng.poll_updates();
        }
        self.run_rule_bound_tools();
        // The search across streams drains its jobs and notices closed or reloaded streams.
        self.find_all.poll(&self.engines);
        if self.find_all.is_active() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }

        // 3. Periodic telemetry refresh
        if self.last_sys_refresh.elapsed().as_secs_f32() >= 1.0 {
            self.system.refresh_cpu_usage();
            self.system.refresh_memory();
            self.cpu_usage = self.system.global_cpu_usage();
            self.mem_used_mb = self.system.used_memory() / (1024 * 1024);
            self.last_sys_refresh = Instant::now();
        }

        // Apply the wheel zoom collected above, outside `ctx.input`: `set_zoom_factor`
        // takes the context lock, and calling it from inside the input closure deadlocks
        // the frame (the window freezes on the first Ctrl + wheel).
        if wheel_zoom != 1.0 {
            let zoomed = (ctx.zoom_factor() * wheel_zoom)
                .clamp(crate::config::MIN_ZOOM, crate::config::MAX_ZOOM);
            ctx.set_zoom_factor(zoomed);
        }

        // 4a. Zoom: whoever moved it (egui's own Ctrl +/-/0, Ctrl + wheel above, or the
        // Settings row) leaves the new value on the context; store it so the window
        // reopens at the same scale.
        let zoom = ctx.zoom_factor();
        if (zoom - self.config.zoom_factor).abs() > 0.001 {
            self.config.zoom_factor = zoom.clamp(crate::config::MIN_ZOOM, crate::config::MAX_ZOOM);
            let _ = self.config.save();
        }

        // 4b. PIN lock: arm it when the screensaver ends (the user walked away) and on
        // Ctrl+L on demand. While locked, modifier shortcuts are dropped so the UI behind
        // the modal cannot be driven from the keyboard; plain typing feeds the PIN box.
        let screensaver_ended = self.screensaver_was_active && !self.screensaver.is_active;
        self.screensaver_was_active = self.screensaver.is_active;
        let lock_shortcut = ctx.input_mut(|i| {
            i.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::CTRL,
                egui::Key::L,
            ))
        });
        if !self.config.lock_pin.is_empty()
            && (lock_shortcut || (self.config.lock_enabled && screensaver_ended))
        {
            self.lock();
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

        // Keep streams updated even when idle or running in the background.
        // On software rasterizers (WARP) every present is expensive in CPU, so we do not
        // pump at the poll cadence: each stream's filesystem watcher wakes the event loop
        // (`make_wake`) when real data arrives, and this slow safety poll only catches
        // what notify misses (atomic saves, network shares). A background log therefore
        // idles without burning cores while staying up to date within ~2s.
        if self.engines.iter().any(|e| e.is_watching) {
            let cadence = if self.renderer.is_software() {
                std::time::Duration::from_secs(2)
            } else {
                std::time::Duration::from_millis(self.config.poll_interval_ms as u64)
            };
            ctx.request_repaint_after(cadence);
        }

        // 5. Apply theme visuals (only when theme changes, when the renderer's software flag
        // changes, or on first frame)
        let software_ui = self.renderer.is_software();
        if self.applied_visuals != Some((software_ui, self.config.theme)) {
            apply_renderer_visuals(&ctx, software_ui, self.config.theme);
            self.applied_visuals = Some((software_ui, self.config.theme));
        }

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
                    let title_text =
                        format!("FASTTAIL {}{}", version_str, self.session_title_suffix());
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

                        // Zoom level (Ctrl+, Ctrl-, Ctrl+0 and Ctrl+wheel all move the log
                        // font size): without this the zoom was invisible, so a stray
                        // Ctrl+wheel left the user with no clue why the text had changed.
                        // Clicking it goes back to 100%.
                        let zoom = crate::config::zoom_percent(ctx.zoom_factor());
                        let zoom_color = if zoom == 100 {
                            self.config.theme.text_dim()
                        } else {
                            self.config.theme.accent_color()
                        };
                        if ui
                            .button(
                                RichText::new(format!("🔍 {zoom}%"))
                                    .monospace()
                                    .size(10.5)
                                    .color(zoom_color),
                            )
                            .on_hover_text(t(self.config.language, "zoom_tip"))
                            .clicked()
                        {
                            ctx.set_zoom_factor(1.0);
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

                    // Sessions menu (🗂): save as, save, load, recent, save as default
                    let session_btn =
                        egui::Button::new(RichText::new("🗂").monospace().strong().color(text_pri))
                            .fill(self.config.theme.button_bg())
                            .stroke(Stroke::new(1.2, accent))
                            .corner_radius(CornerRadius::same(6))
                            .min_size(egui::vec2(30.0, 26.0));
                    let mut session_action: Option<SessionAction> = None;
                    let lang = self.config.language;
                    let has_session = self.config.current_session.is_some();
                    let recent_sessions = self.config.recent_sessions.clone();
                    let session_menu =
                        egui::menu::MenuButton::from_button(session_btn).ui(ui, |ui| {
                            if ui.button(t(lang, "session_save_as")).clicked() {
                                session_action = Some(SessionAction::SaveAs);
                                ui.close();
                            }
                            if ui
                                .add_enabled(
                                    has_session,
                                    egui::Button::new(t(lang, "session_save")),
                                )
                                .clicked()
                            {
                                session_action = Some(SessionAction::Save);
                                ui.close();
                            }
                            if ui.button(t(lang, "session_load")).clicked() {
                                session_action = Some(SessionAction::Load);
                                ui.close();
                            }
                            ui.menu_button(t(lang, "session_recent"), |ui| {
                                if recent_sessions.is_empty() {
                                    ui.label(
                                        RichText::new(t(lang, "session_no_recent"))
                                            .italics()
                                            .color(self.config.theme.text_dim()),
                                    );
                                }
                                for file in &recent_sessions {
                                    if ui
                                        .button(
                                            RichText::new(format!("🗂 {}", Session::name_of(file)))
                                                .monospace(),
                                        )
                                        .on_hover_text(file.display().to_string())
                                        .clicked()
                                    {
                                        session_action =
                                            Some(SessionAction::LoadFile(file.clone()));
                                        ui.close();
                                    }
                                }
                                if !recent_sessions.is_empty() {
                                    ui.separator();
                                    if ui.button(t(lang, "session_clear_recent")).clicked() {
                                        session_action = Some(SessionAction::ClearRecent);
                                        ui.close();
                                    }
                                }
                            });
                            ui.separator();
                            if ui.button(t(lang, "session_save_default")).clicked() {
                                session_action = Some(SessionAction::SaveDefault);
                                ui.close();
                            }
                        });
                    let _ = session_menu.0.on_hover_text(t(lang, "session_tip"));
                    if let Some(action) = session_action {
                        self.run_session_action(action);
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
                        .on_hover_text(t(self.config.language, "filter_tip"))
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
                        .on_hover_text(t(self.config.language, "play_tip"))
                        .clicked()
                    {
                        for eng in &mut self.engines {
                            eng.is_watching = true;
                            eng.follow_tail = !eng.is_compressed();
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
                        .on_hover_text(t(self.config.language, "pause_tip"))
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
                            .on_hover_text(t(self.config.language, "help_tip"))
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

                        if ui
                            .add(about_btn)
                            .on_hover_text(t(self.config.language, "about_tip"))
                            .clicked()
                        {
                            self.config.about_open = !self.config.about_open;
                            let _ = self.config.save();
                        }
                    });
                });
            });

        // 6b. Software-renderer warning banner: on WARP/llvmpipe the whole scene is
        // rasterized on the CPU, so tell the user a GPU is needed for smooth performance.
        if self.renderer.is_software() {
            let warn = self.config.theme.warn_color();
            egui::Panel::top("software_banner")
                .frame(
                    egui::Frame::new()
                        .fill(warn.gamma_multiply(0.18))
                        .stroke(Stroke::new(1.0, warn.gamma_multiply(0.8)))
                        .inner_margin(Margin {
                            left: 12,
                            right: 12,
                            top: 5,
                            bottom: 5,
                        }),
                )
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(format!("⚠ {}", t(self.config.language, "software_banner")))
                            .monospace()
                            .strong()
                            .color(warn),
                    );
                });
        }

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
                        t(self.config.language, "status_no_file").to_string()
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

        // Keyboard shortcut: Ctrl + Shift + F opens or focuses the Find results tab with the
        // focused stream's query. Consumed here, before the dock is drawn, so the stream's
        // own Ctrl + F does not also take it.
        if crate::ui::find_results::consume_find_all_shortcut(&ctx) {
            let query = focused_stream
                .as_ref()
                .and_then(|p| self.engines.iter().find(|e| paths_equal(&e.path, p)))
                .map(|e| e.search_query.clone());
            self.open_find_results(query);
        }

        let mut lock_now = false;
        let mut search_view = crate::ui::dock::SearchViewPrefs {
            search_pane: self.config.search_pane,
            search_pane_height: self.config.search_pane_height,
            overview_strip: self.config.overview_strip,
        };
        let search_view_before = search_view;
        let mut time_delta = crate::ui::dock::TimeDeltaPrefs {
            show: self.config.show_time_delta,
            gap_ms: self.config.time_delta_gap_ms,
        };
        let time_delta_before = time_delta;
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
            language_auto: &mut self.config.language_auto,
            lock_enabled: &mut self.config.lock_enabled,
            lock_pin: &mut self.config.lock_pin,
            lock_now: &mut lock_now,
            quick_labels: &mut self.quick_labels,
            labels_changed: &mut labels_changed,
            external_tools: &mut self.config.external_tools,
            tool_runner: &mut self.tool_runner,
            focused_stream,
            search_view: &mut search_view,
            time_delta: &mut time_delta,
            find_all: &mut self.find_all,
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

        // Requests of the Find results tab and the stream bars, applied now that the dock
        // is drawn: a result to show, or the tab to open with a stream's query.
        if crate::ui::find_results::apply_find_jump(
            &mut self.find_all,
            &mut self.engines,
            &mut self.dock_state,
            self.config.language,
        ) {
            ctx.request_repaint();
        }
        if let Some(eng) = self.engines.iter_mut().find(|e| e.find_all_request) {
            eng.find_all_request = false;
            let query = eng.search_query.clone();
            self.open_find_results(Some(query));
            ctx.request_repaint();
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
            if eng.ansi_dirty {
                // The ANSI mode lives in the stream entry, as in `save_dock_layout`.
                eng.ansi_dirty = false;
                let mut entry = stream_entry_of(eng);
                entry.wrap = false;
                entry.bookmarks.clear();
                self.config.set_stream_state(entry);
                bookmarks_changed = true;
            }
            if !eng.displayed && eng.unseen_severity >= 2 {
                critical_in_background = true;
            }
        }
        if bookmarks_changed {
            let _ = self.config.save();
        }
        // Results pane and overview strip preferences: a toggle is saved at once, the
        // pane height once the drag of its edge is over.
        if search_view != search_view_before {
            self.config.search_pane = search_view.search_pane;
            self.config.search_pane_height = search_view.search_pane_height;
            self.config.overview_strip = search_view.overview_strip;
            let toggled = search_view.search_pane != search_view_before.search_pane
                || search_view.overview_strip != search_view_before.overview_strip;
            if toggled || !ctx.input(|i| i.pointer.any_down()) {
                let _ = self.config.save();
            }
        }
        // Time delta column switch and gap threshold: saved as soon as they change.
        if time_delta != time_delta_before {
            self.config.show_time_delta = time_delta.show;
            self.config.time_delta_gap_ms = time_delta.gap_ms;
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

        if lock_now {
            self.lock();
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

                let bt_resp = egui::Window::new(
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
                apply_dialog_chrome_cursor(&ctx, &bt_resp);
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
            let mut popup_lock_now = false;
            let mut overview_strip = self.config.overview_strip;
            let mut time_delta = crate::ui::dock::TimeDeltaPrefs {
                show: self.config.show_time_delta,
                gap_ms: self.config.time_delta_gap_ms,
            };

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
                egui::ScrollArea::vertical()
                    // Claim the whole window: with the default auto-shrink the area
                    // collapses to its content, the window hugs it, and the size the user
                    // dragged (and the one restored from the config) is thrown away.
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
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
                            &mut self.config.language_auto,
                            &mut self.config.lock_enabled,
                            &mut self.config.lock_pin,
                            &mut popup_lock_now,
                            &mut overview_strip,
                            &mut time_delta,
                        );

                        // Rendering backend: applies at the next start.
                        ui.add_space(6.0);
                        let lang = self.config.language;
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("{}:", t(lang, "renderer"))).monospace(),
                            );
                            egui::ComboBox::from_id_salt("renderer_choice")
                                .selected_text(match self.config.renderer {
                                    crate::renderer::RendererChoice::Auto => {
                                        t(lang, "renderer_auto").to_string()
                                    }
                                    crate::renderer::RendererChoice::Glow => {
                                        t(lang, "renderer_glow").to_string()
                                    }
                                    crate::renderer::RendererChoice::Wgpu => {
                                        t(lang, "renderer_wgpu").to_string()
                                    }
                                    crate::renderer::RendererChoice::Software => {
                                        software_renderer_label(lang)
                                    }
                                })
                                .show_ui(ui, |ui| {
                                    for choice in crate::renderer::RendererChoice::ALL {
                                        let label = match choice {
                                            crate::renderer::RendererChoice::Auto => {
                                                t(lang, "renderer_auto").to_string()
                                            }
                                            crate::renderer::RendererChoice::Glow => {
                                                t(lang, "renderer_glow").to_string()
                                            }
                                            crate::renderer::RendererChoice::Wgpu => {
                                                t(lang, "renderer_wgpu").to_string()
                                            }
                                            crate::renderer::RendererChoice::Software => {
                                                software_renderer_label(lang)
                                            }
                                        };
                                        let resp = ui.selectable_value(
                                            &mut self.config.renderer,
                                            choice,
                                            label,
                                        );
                                        if choice == crate::renderer::RendererChoice::Software {
                                            resp.on_hover_text(t(lang, "renderer_software_warn"));
                                        }
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
                        // The software rasterizer is a fallback, not a real choice: warn about its
                        // CPU cost right where it can be selected.
                        if self.config.renderer == crate::renderer::RendererChoice::Software {
                            ui.label(
                                RichText::new(format!(
                                    "\u{26a0} {}",
                                    t(lang, "renderer_software_warn")
                                ))
                                .small()
                                .color(theme.warn_color()),
                            );
                        }
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

                        ui.add_space(6.0);
                        ui.separator();
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(format!("⚡ {}", t(lang, "perf_section")))
                                .monospace()
                                .strong(),
                        );
                        ui.add_space(4.0);

                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("{}:", t(lang, "poll_interval"))).monospace(),
                            );
                            if ui
                                .add(
                                    egui::DragValue::new(&mut self.config.poll_interval_ms)
                                        .range(50..=5000)
                                        .suffix(" ms"),
                                )
                                .on_hover_text(t(lang, "poll_interval_tip"))
                                .changed()
                            {
                                let _ = self.config.save();
                            }
                        });

                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("{}:", t(lang, "size_check_interval")))
                                    .monospace(),
                            );
                            if ui
                                .add(
                                    egui::DragValue::new(&mut self.config.size_check_interval_ms)
                                        .range(50..=10000)
                                        .suffix(" ms"),
                                )
                                .on_hover_text(t(lang, "size_check_interval_tip"))
                                .changed()
                            {
                                let _ = self.config.save();
                            }
                        });

                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("{}:", t(lang, "max_fps"))).monospace());
                            if ui
                                .add(
                                    egui::DragValue::new(&mut self.config.max_fps)
                                        .range(15..=240)
                                        .suffix(" FPS"),
                                )
                                .on_hover_text(t(lang, "max_fps_tip"))
                                .changed()
                            {
                                let _ = self.config.save();
                            }
                        });

                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("{}:", t(lang, "max_fps_software")))
                                    .monospace(),
                            );
                            if ui
                                .add(
                                    egui::DragValue::new(&mut self.config.max_fps_software)
                                        .range(10..=120)
                                        .suffix(" FPS"),
                                )
                                .on_hover_text(t(lang, "max_fps_software_tip"))
                                .changed()
                            {
                                let _ = self.config.save();
                            }
                        });

                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("{}:", t(lang, "mouse_throttle")))
                                    .monospace(),
                            );
                            if ui
                                .add(
                                    egui::DragValue::new(&mut self.config.mouse_throttle_ms)
                                        .range(0..=1000)
                                        .suffix(" ms"),
                                )
                                .on_hover_text(t(lang, "mouse_throttle_tip"))
                                .changed()
                            {
                                let _ = self.config.save();
                            }
                        });

                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("{}:", t(lang, "markdown_max_size")))
                                    .monospace(),
                            );
                            if ui
                                .add(
                                    egui::DragValue::new(&mut self.config.markdown_max_mb)
                                        .range(1..=100)
                                        .suffix(" MB"),
                                )
                                .on_hover_text(t(lang, "markdown_max_size_tip"))
                                .changed()
                            {
                                for engine in &mut self.engines {
                                    engine.set_markdown_max_bytes(
                                        (self.config.markdown_max_mb as u64) * 1024 * 1024,
                                    );
                                }
                                let _ = self.config.save();
                            }
                        });

                        // Compressed logs: where they are decompressed, and how far.
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("{}:", t(lang, "compressed_max_size")))
                                    .monospace(),
                            );
                            if ui
                                .add(
                                    egui::DragValue::new(&mut self.config.compressed_max_gb)
                                        .range(
                                            crate::compressed::MIN_MAX_GB
                                                ..=crate::compressed::MAX_MAX_GB,
                                        )
                                        .suffix(" GB"),
                                )
                                .on_hover_text(t(lang, "compressed_max_size_tip"))
                                .changed()
                            {
                                let _ = self.config.save();
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("{}:", t(lang, "spool_dir"))).monospace(),
                            );
                            let shown = self.config.compressed_settings().spool_dir;
                            ui.label(
                                RichText::new(shown.display().to_string())
                                    .monospace()
                                    .size(11.0)
                                    .color(self.config.theme.text_dim()),
                            )
                            .on_hover_text(t(lang, "spool_dir_tip"));
                            if ui
                                .button("📁")
                                .on_hover_text(t(lang, "spool_dir_tip"))
                                .clicked()
                            {
                                if let Some(dir) =
                                    rfd::FileDialog::new().set_directory(&shown).pick_folder()
                                {
                                    self.config.spool_dir = Some(dir);
                                    let _ = self.config.save();
                                }
                            }
                            if self.config.spool_dir.is_some()
                                && ui
                                    .button("↺")
                                    .on_hover_text(t(lang, "spool_dir_reset"))
                                    .clicked()
                            {
                                self.config.spool_dir = None;
                                let _ = self.config.save();
                            }
                        });
                    });
            });

            capture_dialog_geometry(
                &resp,
                &mut self.config.settings_pos,
                &mut self.config.settings_size,
            );
            apply_dialog_chrome_cursor(&ctx, &resp);

            if overview_strip != self.config.overview_strip {
                self.config.overview_strip = overview_strip;
                let _ = self.config.save();
            }
            if (time_delta.show, time_delta.gap_ms)
                != (self.config.show_time_delta, self.config.time_delta_gap_ms)
            {
                self.config.show_time_delta = time_delta.show;
                self.config.time_delta_gap_ms = time_delta.gap_ms;
                let _ = self.config.save();
            }
            if popup_lock_now {
                self.lock();
            }

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
                egui::ScrollArea::vertical()
                    // Claim the whole window: with the default auto-shrink the area
                    // collapses to its content, the window hugs it, and the size the user
                    // dragged (and the one restored from the config) is thrown away.
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
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
            apply_dialog_chrome_cursor(&ctx, &resp);

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
            // Resizable like the other dialogs: it was fixed-size, so the size stored for
            // it could never be restored and the window always reopened content-sized.
            .resizable(true)
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
                |win| win.default_width(440.0).default_height(420.0),
            );

            let resp = win.show(&ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new("⚡ FASTTAIL")
                                    .monospace()
                                    .strong()
                                    .size(18.0)
                                    .color(theme.accent_color()),
                            );
                        });

                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(8.0);

                        egui::Grid::new("about_info_grid")
                            .num_columns(2)
                            .spacing([16.0, 8.0])
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(t(lang, "about_version")).monospace().strong(),
                                );
                                ui.label(
                                    RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                                        .monospace()
                                        .color(theme.accent_color()),
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

                                ui.label(
                                    RichText::new(t(lang, "about_author")).monospace().strong(),
                                );
                                ui.label(
                                    RichText::new("Matteo Baccan")
                                        .monospace()
                                        .color(theme.text_primary()),
                                );
                                ui.end_row();

                                ui.label(
                                    RichText::new(t(lang, "about_website")).monospace().strong(),
                                );
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

                                ui.label(
                                    RichText::new(t(lang, "about_license")).monospace().strong(),
                                );
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
            });

            capture_dialog_geometry(
                &resp,
                &mut self.config.about_pos,
                &mut self.config.about_size,
            );
            apply_dialog_chrome_cursor(&ctx, &resp);

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
                egui::ScrollArea::vertical()
                    // Claim the whole window: with the default auto-shrink the area
                    // collapses to its content, the window hugs it, and the size the user
                    // dragged (and the one restored from the config) is thrown away.
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
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
                                    ui.label(
                                        RichText::new("CTRL +  /  CTRL =").monospace().strong(),
                                    );
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
                                    ui.label(
                                        RichText::new(t(lang, "help_desc_search")).monospace(),
                                    );
                                    ui.end_row();

                                    ui.label(
                                        RichText::new("Ctrl + Shift + F").monospace().strong(),
                                    );
                                    ui.label(
                                        RichText::new(t(lang, "help_desc_find_all")).monospace(),
                                    );
                                    ui.end_row();

                                    ui.label(
                                        RichText::new("F3  /  Shift + F3").monospace().strong(),
                                    );
                                    ui.label(
                                        RichText::new(t(lang, "help_desc_find_next")).monospace(),
                                    );
                                    ui.end_row();

                                    ui.label(
                                        RichText::new("Click / Shift + Click / Ctrl + Click")
                                            .monospace()
                                            .strong(),
                                    );
                                    ui.label(
                                        RichText::new(t(lang, "help_desc_select")).monospace(),
                                    );
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

                                    ui.label(
                                        RichText::new("☰ ↑ ↓ PgUp PgDn Enter Esc")
                                            .monospace()
                                            .strong(),
                                    );
                                    ui.label(
                                        RichText::new(t(lang, "help_desc_search_pane")).monospace(),
                                    );
                                    ui.end_row();

                                    ui.label(
                                        RichText::new("Ctrl + Shift + 1..9").monospace().strong(),
                                    );
                                    ui.label(
                                        RichText::new(t(lang, "help_desc_labels")).monospace(),
                                    );
                                    ui.end_row();

                                    ui.label(
                                        RichText::new(t(lang, "help_key_tools"))
                                            .monospace()
                                            .strong(),
                                    );
                                    ui.label(RichText::new(t(lang, "help_desc_tools")).monospace());
                                    ui.end_row();

                                    ui.label(
                                        RichText::new("Ctrl + Shift + T").monospace().strong(),
                                    );
                                    ui.label(RichText::new(t(lang, "pin_tip")).monospace());
                                    ui.end_row();

                                    ui.label(
                                        RichText::new("Ctrl + F2  /  F2  /  Shift + F2")
                                            .monospace()
                                            .strong(),
                                    );
                                    ui.label(
                                        RichText::new(t(lang, "help_desc_bookmark")).monospace(),
                                    );
                                    ui.end_row();

                                    ui.label(RichText::new("F1").monospace().strong());
                                    ui.label(RichText::new(t(lang, "help_desc_f1")).monospace());
                                    ui.end_row();

                                    ui.label(RichText::new("Esc").monospace().strong());
                                    ui.label(RichText::new(t(lang, "help_desc_esc")).monospace());
                                    ui.end_row();

                                    ui.label(RichText::new("Drag & Drop").monospace().strong());
                                    ui.label(
                                        RichText::new(t(lang, "help_desc_drag_drop")).monospace(),
                                    );
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
            apply_dialog_chrome_cursor(&ctx, &resp);

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

        // 11c. Session dialogs: discard unsaved changes? / streams that could not be opened
        if let Some(pending) = self.pending_session_load.clone() {
            let lang = self.config.language;
            let theme = self.config.theme;
            let mut is_open = true;
            let mut decision: Option<bool> = None;
            egui::Window::new(
                RichText::new(format!("🗂 {}", t(lang, "session_unsaved_title")))
                    .monospace()
                    .color(theme.warn_color()),
            )
            .id(egui::Id::new("fasttail_session_confirm"))
            .open(&mut is_open)
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(&ctx, |ui| {
                let current = self
                    .config
                    .current_session
                    .as_deref()
                    .map(Session::name_of)
                    .unwrap_or_default();
                ui.label(
                    RichText::new(t(lang, "session_unsaved_body").replace("{name}", &current))
                        .monospace()
                        .size(11.5)
                        .color(theme.text_primary()),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(t(lang, "session_load_anyway")).clicked() {
                        decision = Some(true);
                    }
                    if ui.button(t(lang, "session_cancel")).clicked()
                        || ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
                    {
                        decision = Some(false);
                    }
                });
            });
            match decision {
                Some(true) => {
                    self.pending_session_load = None;
                    self.load_session_file(pending, true);
                }
                Some(false) => self.pending_session_load = None,
                None if !is_open => self.pending_session_load = None,
                None => {}
            }
        }
        if let Some(missing) = self.session_missing.clone() {
            let lang = self.config.language;
            let theme = self.config.theme;
            let mut is_open = true;
            let mut close = false;
            egui::Window::new(
                RichText::new(format!("🗂 {}", t(lang, "session_missing_title")))
                    .monospace()
                    .color(theme.warn_color()),
            )
            .id(egui::Id::new("fasttail_session_missing"))
            .open(&mut is_open)
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(&ctx, |ui| {
                ui.label(
                    RichText::new(t(lang, "session_missing_body"))
                        .monospace()
                        .size(11.5)
                        .color(theme.text_primary()),
                );
                ui.add_space(4.0);
                for p in &missing {
                    ui.label(
                        RichText::new(format!("• {}", p.display()))
                            .monospace()
                            .size(11.0)
                            .color(theme.text_dim()),
                    );
                }
                ui.add_space(8.0);
                if ui.button(t(lang, "session_ok")).clicked()
                    || ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
                {
                    close = true;
                }
            });
            if close || !is_open {
                self.session_missing = None;
            }
        }

        // Zip entry picker: each chosen entry opens as its own stream.
        if let Some(picker) = self.zip_picker.as_mut() {
            let outcome = picker.show(&ctx, self.config.language, self.config.theme);
            if let Some(outcome) = outcome {
                let picker = self.zip_picker.take();
                if let (crate::ui::zip_picker::PickerOutcome::Open(names), Some(picker)) =
                    (outcome, picker)
                {
                    for name in names {
                        let path = crate::compressed::entry_path(&picker.archive, &name);
                        self.open_log_file(path);
                    }
                }
            }
        }
        if let Some(notice) = self.open_notice.clone() {
            let lang = self.config.language;
            let theme = self.config.theme;
            let mut is_open = true;
            let mut close = false;
            egui::Window::new(
                RichText::new(format!("🗜 {}", t(lang, "compressed_open_failed")))
                    .monospace()
                    .color(theme.warn_color()),
            )
            .id(egui::Id::new("fasttail_open_notice"))
            .open(&mut is_open)
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(&ctx, |ui| {
                ui.label(
                    RichText::new(notice)
                        .monospace()
                        .size(11.5)
                        .color(theme.text_primary()),
                );
                ui.add_space(8.0);
                if ui.button(t(lang, "session_ok")).clicked()
                    || ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
                {
                    close = true;
                }
            });
            if close || !is_open {
                self.open_notice = None;
            }
        }

        // 12. Render Matrix Screensaver if activated
        let viewport = ctx.content_rect();
        self.screensaver.render(&ctx, viewport);

        // The PIN prompt sits above everything, the screensaver included.
        if self.locked {
            self.render_lock_overlay(&ctx);
        }

        // 12. Borderless Window Resize Anchors & Visual Frames (Edges & Corners)
        // The resize handles read the pointer straight from the input rather than through
        // a widget, so the modal does not block them: skip them while locked, like every
        // other interaction with the window behind the prompt.
        let is_maximized =
            self.config.window_maximized || ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        if self.config.borderless && !is_maximized && !self.locked {
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
        // Detect whether this frame is driven purely by pointer movement (no clicks, no keys, no drag)
        let is_pure_mouse_move = ui.input(|i| {
            !i.pointer.any_down()
                && !i.raw.events.is_empty()
                && i.raw
                    .events
                    .iter()
                    .all(|e| matches!(e, egui::Event::PointerMoved(_)))
        });

        if is_pure_mouse_move && self.config.mouse_throttle_ms > 0 {
            let throttle = std::time::Duration::from_micros(mouse_throttle_interval_us(
                self.renderer.is_software(),
                self.config.mouse_throttle_ms,
            ));
            let elapsed = self.last_mouse_render.elapsed();
            if elapsed < throttle {
                std::thread::sleep(throttle - elapsed);
            }
            self.last_mouse_render = Instant::now();
        } else if self.renderer.is_software() {
            let target_fps = self.config.max_fps_software.max(1);
            let min_interval = std::time::Duration::from_micros(1_000_000 / target_fps as u64);
            let elapsed = self.last_frame_render.elapsed();
            if elapsed < min_interval {
                std::thread::sleep(min_interval - elapsed);
            }
        }
        self.last_frame_render = Instant::now();
        self.render_ui(ui);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_dock_layout();
        let _ = self.config.save();
        // Dropping the engines stops their decompression jobs and deletes their spools;
        // whatever is left of this process goes too.
        self.engines.clear();
        crate::spool::remove_own(&self.config.compressed_settings().spool_dir);
    }
}

/// What the sessions menu asked for; run after the menu closed so `self` is free.
enum SessionAction {
    SaveAs,
    Save,
    Load,
    LoadFile(PathBuf),
    ClearRecent,
    SaveDefault,
}

impl FastTailApp {
    fn run_session_action(&mut self, action: SessionAction) {
        match action {
            SessionAction::SaveAs => {
                let mut dialog = rfd::FileDialog::new()
                    .add_filter("FastTail session", &["ini"])
                    .set_file_name(format!("session{}", crate::session::SESSION_SUFFIX));
                if let Some(dir) = self.session_base_dir() {
                    dialog = dialog.set_directory(dir);
                }
                if let Some(file) = dialog.save_file() {
                    let file = Session::with_suffix(&file);
                    if let Err(err) = self.save_session_as(file.clone()) {
                        eprintln!("fasttail: cannot save session {}: {err}", file.display());
                    }
                }
            }
            SessionAction::Save => {
                if let Err(err) = self.save_session() {
                    eprintln!("fasttail: cannot save session: {err}");
                }
            }
            SessionAction::Load => {
                let mut dialog = rfd::FileDialog::new().add_filter("FastTail session", &["ini"]);
                if let Some(dir) = self.session_base_dir() {
                    dialog = dialog.set_directory(dir);
                }
                if let Some(file) = dialog.pick_file() {
                    self.load_session_file(file, false);
                }
            }
            SessionAction::LoadFile(file) => self.load_session_file(file, false),
            SessionAction::ClearRecent => {
                self.config.recent_sessions.clear();
                let _ = self.config.save();
            }
            SessionAction::SaveDefault => self.save_session_as_default(),
        }
    }
}

/// Structure of the dock (surface, node and tab order) without geometry.
fn dock_signature(dock: &DockState<FastTailTab>) -> String {
    let mut out = String::new();
    for (path, tab) in dock.iter_all_tabs() {
        let name = match tab {
            FastTailTab::LogStream(p) => p.to_string_lossy().to_string(),
            other => format!("{other:?}"),
        };
        out.push_str(&format!(
            "{}:{}:{}:{name};",
            path.surface.0, path.node.0, path.tab.0
        ));
    }
    out
}

/// The session entry describing `engine` as it is now.
fn stream_entry_of(engine: &TailEngine) -> StreamEntry {
    let mut bookmarks: Vec<usize> = engine.bookmarks.iter().copied().collect();
    if let Some(c) = engine.compressed.as_ref() {
        // A restored compressed stream still waiting for its index to reach them.
        if bookmarks.is_empty() {
            bookmarks = c.pending_bookmarks.clone();
        }
    }
    StreamEntry {
        path: engine.path.clone(),
        include_filter: engine.include_filter.clone(),
        exclude_filter: engine.exclude_filter.clone(),
        search_query: engine.search_query.trim().to_string(),
        wrap: engine.wrap_lines,
        encoding: Some(engine.encoding.name().to_string()),
        ansi: (engine.ansi_mode != AnsiMode::Auto).then(|| engine.ansi_mode.name().to_string()),
        bookmarks,
        archive_entry: engine.compressed.as_ref().and_then(|c| c.entry.clone()),
    }
}

/// Applies the saved bookmarks of `path`. A compressed stream starts on an empty spool:
/// its bookmarks wait until the index covers them (see `TailEngine::poll_compressed`).
fn restore_bookmarks(engine: &mut TailEngine, cfg: &FastTailConfig, path: &Path) {
    if let Some(c) = engine.compressed.as_mut() {
        if let Some((_, lines)) = cfg.bookmarks.iter().find(|(p, _)| paths_equal(p, path)) {
            c.pending_bookmarks = lines.clone();
        }
    } else if let Some(lines) = cfg.bookmarks_for(path, engine.total_lines()) {
        engine.set_bookmarks(lines);
    }
}

/// Applies the persisted filters, search query, encoding and ANSI mode of the engine's
/// path (wrap and bookmarks are applied by the caller from their own sections).
fn apply_stream_state(engine: &mut TailEngine, cfg: &FastTailConfig) {
    let Some(entry) = cfg.stream_state_for(&engine.path).cloned() else {
        return;
    };
    // First, so the filters and the search below run once, on the right text.
    if let Some(mode) = entry.ansi.as_deref().and_then(AnsiMode::from_name) {
        engine.set_ansi_mode(mode);
        engine.ansi_dirty = false;
    }
    if let Some(enc) = entry.encoding.as_deref().and_then(FileEncoding::from_name) {
        if enc != engine.encoding {
            // Re-decoding rebuilds the index and drops bookmarks: restore them after.
            let bookmarks: Vec<usize> = engine.bookmarks.iter().copied().collect();
            engine.set_encoding(enc);
            if !bookmarks.is_empty() && bookmarks.iter().all(|&l| l < engine.total_lines()) {
                engine.set_bookmarks(bookmarks);
            }
        }
    }
    if !entry.include_filter.is_empty() {
        engine.set_include_filter(&entry.include_filter);
    }
    if !entry.exclude_filter.is_empty() {
        engine.set_exclude_filter(&entry.exclude_filter);
    }
    if !entry.search_query.is_empty() {
        engine.search_query = entry.search_query.clone();
        engine.update_search(&entry.search_query);
    }
}

#[cfg(test)]
mod tests {
    use super::allowed_while_locked;
    use egui::{Event, Key, Modifiers};

    fn key(key: Key, modifiers: Modifiers) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    #[test]
    fn lock_drops_the_keys_the_workspace_would_act_on() {
        // Bare Escape used to reach the dialog that sat behind the lock and close it.
        assert!(!allowed_while_locked(&key(Key::Escape, Modifiers::NONE)));
        assert!(!allowed_while_locked(&key(Key::F1, Modifiers::NONE)));
        assert!(!allowed_while_locked(&key(Key::Space, Modifiers::NONE)));
        assert!(!allowed_while_locked(&key(Key::O, Modifiers::CTRL)));
        assert!(!allowed_while_locked(&key(Key::W, Modifiers::ALT)));
        assert!(!allowed_while_locked(&Event::Copy));
    }

    fn measure_lock_width(lang: crate::i18n::Language, screen: egui::Vec2) -> f32 {
        let ctx = egui::Context::default();
        let mut width = 0.0;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, screen)),
            ..Default::default()
        };
        ctx.begin_pass(input);
        width = super::lock_prompt_width(&ctx, lang);
        // The font atlas built while measuring comes back as a texture delta that egui
        // insists is consumed; there is no painter here to consume it.
        let mut output = ctx.end_pass();
        output.textures_delta.clear();
        width
    }

    #[test]
    fn the_lock_prompt_is_sized_for_the_language_it_shows() {
        use crate::i18n::Language;
        let screen = egui::vec2(1200.0, 800.0);
        let english = measure_lock_width(Language::En, screen);
        let russian = measure_lock_width(Language::Ru, screen);

        // Every language gets at least the comfortable minimum...
        assert!(english >= super::LOCK_MIN_WIDTH);
        // ...and a language whose longest line is wider gets a wider dialog, which is the
        // whole point: "Too many attempts: try again in 60 s" does not wrap any more.
        assert!(
            russian > english,
            "russian {russian} should need more room than english {english}"
        );
        // Never wider than the window.
        assert!(russian <= screen.x * super::LOCK_MAX_WIDTH_RATIO);
    }

    #[test]
    fn the_lock_prompt_never_outgrows_a_small_window() {
        let width = measure_lock_width(crate::i18n::Language::Ru, egui::vec2(320.0, 240.0));
        assert!(width <= super::LOCK_MIN_WIDTH.max(320.0 * super::LOCK_MAX_WIDTH_RATIO));
    }

    #[test]
    fn three_wrong_pins_pause_the_prompt_for_a_minute() {
        use super::{LockAttempts, LOCK_COOLDOWN};
        use std::time::{Duration, Instant};

        let now = Instant::now();
        let mut attempts = LockAttempts::default();
        assert!(!attempts.register_failure(now));
        assert!(!attempts.register_failure(now));
        assert!(
            attempts.cooldown_left(now).is_none(),
            "two misses cost nothing"
        );

        assert!(
            attempts.register_failure(now),
            "the third one starts the pause"
        );
        let left = attempts.cooldown_left(now).expect("prompt is paused");
        assert!(left <= LOCK_COOLDOWN && left > LOCK_COOLDOWN - Duration::from_secs(1));
        assert!(attempts
            .cooldown_left(now + LOCK_COOLDOWN - Duration::from_secs(1))
            .is_some());
        assert!(attempts.cooldown_left(now + LOCK_COOLDOWN).is_none());

        // Three more misses pause it again, and a correct PIN forgets everything.
        for _ in 0..3 {
            attempts.register_failure(now + LOCK_COOLDOWN);
        }
        assert!(attempts.cooldown_left(now + LOCK_COOLDOWN).is_some());
        attempts.reset();
        assert!(attempts.cooldown_left(now + LOCK_COOLDOWN).is_none());
    }

    #[test]
    fn lock_keeps_what_the_pin_prompt_needs() {
        assert!(allowed_while_locked(&Event::Text("4".to_owned())));
        assert!(allowed_while_locked(&key(Key::Enter, Modifiers::NONE)));
        assert!(allowed_while_locked(&key(Key::Backspace, Modifiers::NONE)));
        assert!(allowed_while_locked(&key(Key::ArrowLeft, Modifiers::NONE)));
        assert!(allowed_while_locked(&Event::PointerGone));
    }
}
