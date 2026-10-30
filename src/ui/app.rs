// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use crate::baretail_bridge::{detect_baretail_config, BareTailConfig};
use crate::config::FastTailConfig;
use crate::external_tools::ToolRunner;
use crate::i18n::t;
use crate::lock::LockAttempts;
use crate::paths::paths_equal;
use crate::screensaver::MatrixScreensaver;
use crate::session::{LoadedSession, Session, StreamEntry};
use crate::tail_engine::{QuickLabel, TailEngine};
use crate::theme::CyberTheme;
use crate::ui::dock::{DockContext, FastTailTab, FastTailTabViewer};
use crate::workspace::OpenOutcome;
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

/// What the key map is built from: `[shortcuts]` and the tools' shortcuts.
type KeymapInputs = (
    std::collections::BTreeMap<String, String>,
    Vec<Option<String>>,
);

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
    /// CPU and memory of the telemetry meters; `None` where `sysinfo` cannot read them.
    pub system: Option<System>,
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
    /// The Open folder dialog while it is shown (`None` when closed).
    pub folder_prompt: Option<crate::ui::folder_dialog::FolderPrompt>,
    /// The New merged view dialog while it is shown.
    pub merge_prompt: Option<crate::ui::merge_dialog::MergePrompt>,
    /// The Recent files window (the palette's "Recent files"), while it is shown.
    pub recent_files_window: bool,
    /// The stream whose information window is shown (the palette's "Stream information").
    pub stream_info: Option<PathBuf>,
    /// The Open system source dialog (`📦`, the palette).
    pub system_dialog: Option<crate::ui::system_dialog::SystemDialog>,
    /// The Remote file (SSH) dialog (`🌐`, the palette).
    pub remote_dialog: Option<crate::ui::remote_dialog::RemoteDialog>,
    /// A paste asked for (palette, menu, Append) and when: the clipboard text comes back
    /// as egui's paste event a frame later.
    paste_request: Option<(PasteTarget, std::time::Instant)>,
    /// The Listen dialog (`📡`, the palette).
    pub listen_dialog: Option<crate::ui::listen_dialog::ListenDialog>,
    /// The Debug output dialog (Windows).
    pub debug_dialog: Option<crate::ui::debug_dialog::DebugDialog>,
    /// The Event Log dialog (Windows), while open.
    pub eventlog_dialog: Option<crate::ui::eventlog_dialog::EventLogDialog>,
    /// The Export dialog, while it is shown.
    pub export_dialog: Option<crate::ui::export_dialog::ExportDialog>,
    /// The Split file dialog, while it is shown.
    pub split_dialog: Option<crate::ui::split_dialog::SplitDialog>,
    /// The Log formats dialog, while it is shown.
    pub log_formats: Option<crate::ui::log_formats::FormatsDialog>,
    /// The open-all folder groups: each opens the new files of its folder as tabs.
    pub folder_groups: Vec<crate::folder_source::FolderGroup>,
    /// Spawns external tools and enforces the rule-bound throttle and cap.
    pub tool_runner: ToolRunner,
    /// Stream whose term rows the Filters window opens and scrolls to (a `+` button or
    /// "Manage presets" of its stream bar), cleared once shown.
    pub filters_focus: Option<PathBuf>,
    /// INI text of the named session as last saved or loaded; compared with the live
    /// workspace at most once per second to show the `*` in the title bar.
    pub session_saved: String,
    pub session_dirty: bool,
    pub last_dirty_check: Instant,
    /// Session file waiting for the user to confirm discarding unsaved changes.
    pub pending_session_load: Option<PathBuf>,
    /// Streams of the last loaded session that could not be opened, shown once.
    pub session_missing: Option<Vec<PathBuf>>,
    /// The bookmark report dialog, while it is open.
    pub report_dialog: Option<crate::ui::report_dialog::ReportDialog>,
    /// The scratchpad of the current session (see `ui::scratchpad`).
    pub scratchpad: crate::ui::scratchpad::Scratchpad,
    /// A reference line to show once its stream has indexed that line.
    scratch_jump: Option<(PathBuf, usize)>,
    /// The notification-area icon while it is on, whether the window is hidden in it,
    /// a real quit (CTRL + Q, the tray's Quit) under close to tray, the alert count's
    /// baseline and the menu and badge last handed to the tray.
    tray: Option<crate::tray::Tray>,
    tray_failed: bool,
    /// Desktop notifications of the rules with "Notify".
    notifications: crate::notifications::Center,
    hidden_in_tray: bool,
    quitting: bool,
    alert_baseline: u64,
    /// Set while the window shows nothing: a thread then requests a repaint every
    /// `BACKGROUND_WAKE`, so `logic` keeps polling (see `background_waker`).
    background_wake: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    tray_menu_sent: crate::tray::TrayMenu,
    tray_badge_sent: Option<(bool, String)>,
    /// The side marked for compare, and the compare shown in the Compare tab.
    pub compare_mark: Option<crate::ui::compare_tab::CompareSide>,
    pub compare: Option<crate::ui::compare_tab::CompareView>,
    /// Markdown view caches of the open streams (see `dock::MarkdownCaches`).
    markdown_caches: crate::ui::dock::MarkdownCaches,
    /// Entry picker of a zip holding several files or of a tar archive, while it is shown.
    pub archive_picker: Option<crate::ui::zip_picker::ArchivePicker>,
    /// Why the last compressed file could not be opened (empty zip, no space...), shown
    /// once in a small window.
    pub open_notice: Option<String>,
    /// Why the terminal interface asked for at start (`--tui`, `interface=tui`) could not
    /// be started, so the window opened instead (Windows hand-off, `main.rs`).
    pub handoff_notice: Option<String>,
    /// The terminal was just picked as the interface in Settings: the dialog offering to
    /// switch now is open.
    interface_switch: bool,
    /// Standard input piped without `-`, waiting for its first byte before its stream
    /// (and tab) is created, and the command line options to apply to it then.
    pub pending_stdin: Option<crate::stdin_source::StdinStream>,
    pub stdin_options: StdinOptions,
    /// Streams a session save left out (standard input), or why the save failed, shown
    /// once after the save under the title `save_notice_title` (an i18n key).
    pub save_notice: Option<String>,
    pub save_notice_title: &'static str,
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
    /// The global filter as compiled for the streams (`None`: off or no term), shared by
    /// every engine; rebuilt from `config.global_filter` after an edit settles.
    pub global_spec: Option<std::sync::Arc<crate::scan_job::FilterSpec>>,
    /// When the terms of the global filter bar were last edited, until applied.
    global_edit_at: Option<Instant>,
    /// Key of `global_spec`: an apply that does not change it keeps the same set, so no
    /// stream refilters.
    global_key: Option<crate::global_filter::AppliedKey>,
    /// Command palette (CTRL + SHIFT + P).
    pub palette: crate::ui::palette::CommandPalette,
    /// Stream action picked in the palette, handed to its stream when the dock is drawn
    /// this frame (see `DockContext::palette_action`); kept for a few frames when that
    /// stream is not drawn, with the frames left.
    palette_action: Option<crate::actions::PendingAction>,
    /// The keyboard shortcuts (`keymap`), built from `[shortcuts]` and the tools'
    /// shortcuts; rebuilt when they change (`refresh_keymap`).
    pub keymap: crate::keymap::KeyMap,
    /// What `keymap` was built from (`None` before the first build).
    keymap_inputs: Option<KeymapInputs>,
    /// The Keyboard shortcuts window (`ui::shortcuts_dialog`).
    shortcuts_dialog: crate::ui::shortcuts_dialog::ShortcutsDialog,
    /// The web view server and its settings window (`ui::web_view`).
    web_view: crate::ui::web_view::WebView,
    /// The MCP server for AI assistants and its settings window (`ui::mcp_view`).
    pub mcp_view: crate::ui::mcp_view::McpView,
    /// The plugins menu, parameter window and settings (`ui::plugins_view`).
    pub plugins_view: crate::ui::plugins_view::PluginsView,
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

#[cfg_attr(not(windows), allow(unused_variables))]
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
            config.folder_groups.clear();
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
        // Standard input: with `-` the stream opens now, else piped input opens it on its
        // first byte. `-` without piped input was reported by `main` and cleared.
        if crate::stdin_source::classify() == crate::stdin_source::StdinKind::Piped {
            if let Some(input) = crate::stdin_source::take_stdin() {
                app.start_stdin(input, cli.stdin, StdinOptions::from_cli(&cli));
            }
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
        let wake_notify = Self::make_wake(&egui_ctx);
        let mut system = System::new_with_specifics(
            RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory(MemoryRefreshKind::everything()),
        )
        .ok();
        if let Some(system) = system.as_mut() {
            system.refresh_cpu_usage();
            system.refresh_memory();
        }

        // Check for BareTail config on Windows if enabled (only done once, then disabled in fasttail.ini)
        let baretail_config = if config.baretail_import && !config.baretail_prompt_shown {
            detect_baretail_config()
        } else {
            None
        };
        let baretail_dialog_open = baretail_config.is_some();

        // Restore saved dock layout from config, or start clean
        let mut dock_state: DockState<FastTailTab> = config
            .dock_layout
            .as_ref()
            .and_then(|ron_str| ron::from_str(ron_str).ok())
            .unwrap_or_else(|| DockState::new(vec![]));
        // A layout saved by 0.13.0 or earlier may hold an empty main surface.
        crate::ui::find_results::ensure_main_surface(&mut dock_state);
        // The terminal interface keeps the open files but not the dock layout: the tabs of
        // files it closed go here, the files it opened get a tab once the streams are
        // restored (below).
        let open = config.open_files.clone();
        crate::ui::find_results::retain_tabs(&mut dock_state, |tab| match tab {
            FastTailTab::LogStream(p) => open.iter().any(|o| paths_equal(o, p)),
            _ => true,
        });

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
            folder_prompt: None,
            merge_prompt: None,
            recent_files_window: false,
            stream_info: None,
            system_dialog: None,
            remote_dialog: None,
            paste_request: None,
            listen_dialog: None,
            debug_dialog: None,
            eventlog_dialog: None,
            log_formats: None,
            split_dialog: None,
            export_dialog: None,
            folder_groups: Vec::new(),
            tool_runner: ToolRunner::default(),
            filters_focus: None,
            session_saved: String::new(),
            session_dirty: false,
            last_dirty_check: Instant::now(),
            pending_session_load: None,
            session_missing: None,
            report_dialog: None,
            scratchpad: crate::ui::scratchpad::Scratchpad::default(),
            scratch_jump: None,
            compare_mark: None,
            compare: None,
            markdown_caches: Default::default(),
            tray: None,
            tray_failed: false,
            notifications: crate::notifications::Center::new(wake_notify),
            hidden_in_tray: false,
            quitting: false,
            alert_baseline: 0,
            background_wake: None,
            tray_menu_sent: crate::tray::TrayMenu::default(),
            tray_badge_sent: None,
            archive_picker: None,
            open_notice: None,
            handoff_notice: None,
            interface_switch: false,
            pending_stdin: None,
            stdin_options: StdinOptions::default(),
            save_notice: None,
            save_notice_title: "stdin_not_saved_title",
            title_applied: String::new(),
            last_frame_render: Instant::now(),
            last_mouse_render: Instant::now(),
            applied_visuals: None,
            find_all: crate::find_all::FindAllSession::default(),
            global_spec: None,
            global_edit_at: None,
            global_key: None,
            palette: Default::default(),
            palette_action: None,
            keymap: crate::keymap::KeyMap::default(),
            keymap_inputs: None,
            shortcuts_dialog: Default::default(),
            web_view: Default::default(),
            mcp_view: Default::default(),
            plugins_view: Default::default(),
        };
        app.global_key = app.config.global_filter.applied_key();
        app.global_spec = app.config.global_filter.compile();

        // Taken now: the openings below save the workspace, which keeps only the groups
        // already running (none yet).
        let saved_groups = app.config.folder_groups.clone();
        let has_restored_tabs = app.dock_state.iter_all_tabs().count() > 0;
        if has_restored_tabs {
            // Restore engines for tabs already positioned in the dock layout without altering dock tree
            let tabs: Vec<PathBuf> = app
                .dock_state
                .iter_all_tabs()
                .filter_map(|(_, tab)| match tab {
                    FastTailTab::LogStream(p) => Some(p.clone()),
                    _ => None,
                })
                .collect();
            // Missing and unreadable files are skipped, as are bare archives.
            let wake = Self::make_wake(&app.egui_ctx);
            let restored = crate::workspace::open_all(&tabs, &app.config, Some(wake));
            app.engines.extend(restored.engines);
            // Derived streams, saved under their `filter:` name: rebuilt from their
            // source, their tab renamed after the spool (or dropped with its source).
            for tab in tabs
                .iter()
                .filter(|t| crate::filter_tab::parse_identity(t).is_some())
            {
                let spool = app.restore_filter_tab(tab, false);
                rename_tab(&mut app.dock_state, tab, spool);
            }
            // Merged views, saved under their `merged:` name, after the derived streams
            // they may merge.
            for tab in tabs
                .iter()
                .filter(|t| crate::merged_stream::parse_identity(t).is_some())
            {
                let spool = app.restore_merged_view(tab, false);
                rename_tab(&mut app.dock_state, tab, spool);
            }
            // Open files the layout has no tab for (opened in the terminal interface) join
            // the main area.
            for path in app.config.open_files.clone() {
                if !tabs.iter().any(|t| paths_equal(t, &path)) {
                    app.open_log_file(path);
                }
            }
        } else {
            // Clean dock layout: open previously saved files into dock
            for path in app.config.open_files.clone() {
                if crate::compressed::source_exists(&path)
                    || crate::wildcard::is_pattern_path(&path)
                    || crate::system_source::is_system_path(&path)
                {
                    app.open_log_file(path);
                }
            }
        }

        // Folder groups: the files created since the last run join their tabs.
        for path in &saved_groups {
            app.restore_folder_group(path);
        }
        if !app.folder_groups.is_empty() {
            app.save_dock_layout();
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
        app.scratchpad = crate::ui::scratchpad::Scratchpad::load(app.scratchpad_file());

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
        let mut dock_to_save = crate::ui::find_results::without_find_results(&dock_to_save);
        rename_derived_tabs(&mut dock_to_save, &self.engines, true);
        crate::ui::find_results::retain_tabs(&mut dock_to_save, |tab| !is_stdin_tab(tab));
        ron::to_string(&dock_to_save).ok()
    }

    /// The live workspace as a session: streams in dock order with their filters,
    /// search query, wrap, encoding and bookmarks, plus the dock layout.
    pub fn capture_session(&mut self) -> Session {
        let mut paths: Vec<PathBuf> = Vec::new();
        for (_, tab) in self.dock_state.iter_all_tabs() {
            if let FastTailTab::LogStream(p) = tab {
                let stdin = crate::stdin_source::is_ephemeral_path(p);
                if !stdin && !paths.iter().any(|e| paths_equal(e, p)) {
                    paths.push(p.clone());
                }
            }
        }
        let all: Vec<&TailEngine> = self.engines.iter().collect();
        let streams = paths
            .iter()
            .filter_map(|p| {
                self.engines
                    .iter()
                    .find(|e| paths_equal(&e.path, p))
                    .filter(|e| crate::workspace::is_persisted(e))
                    .filter(|e| {
                        crate::workspace::sources_open(&crate::workspace::persisted_path(e), &all)
                    })
                    .map(crate::workspace::stream_entry)
            })
            .collect();
        let folders = self
            .folder_groups
            .iter()
            .filter(|g| self.engines.iter().any(|e| g.is_member(&e.path)))
            .map(|g| g.path())
            .collect();
        Session {
            streams,
            dock_layout: self.dock_layout_ron(),
            folders,
        }
    }

    /// The live workspace as text for the unsaved-changes check. The dock layout is
    /// represented by its structure (surfaces, nodes and tab order) rather than by the
    /// RON, whose node rectangles change at every resize without the user doing anything.
    fn session_fingerprint(&mut self) -> String {
        let base = self.session_base_dir();
        let mut session = self.capture_session();
        let mut dock = crate::ui::find_results::without_find_results(&self.dock_state);
        rename_derived_tabs(&mut dock, &self.engines, true);
        session.dock_layout = Some(dock_signature(&dock));
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

    /// `Esc`: closes the open dialog drawn on top (Settings, Filters, About, Help), not all
    /// of them at once.
    fn close_topmost_dialog(&mut self, ctx: &egui::Context) {
        let open = [
            ("fasttail_settings_popup", self.config.settings_open),
            ("fasttail_filters_popup", self.config.filters_open),
            ("fasttail_about_popup", self.config.about_open),
            ("fasttail_help_popup", self.config.help_open),
        ];
        let open_ids: Vec<(&str, egui::Id)> = open
            .iter()
            .filter(|(_, is_open)| *is_open)
            .map(|(name, _)| (*name, egui::Id::new(*name)))
            .collect();
        if open_ids.is_empty() {
            return;
        }
        // Layers come bottom to top; a dialog not drawn yet counts as the lowest.
        let top = ctx.memory(|m| {
            m.layer_ids()
                .filter_map(|layer| open_ids.iter().find(|(_, id)| *id == layer.id))
                .last()
                .map(|(name, _)| *name)
        });
        match top.unwrap_or(open_ids[open_ids.len() - 1].0) {
            "fasttail_settings_popup" => self.config.settings_open = false,
            "fasttail_filters_popup" => self.config.filters_open = false,
            "fasttail_about_popup" => self.config.about_open = false,
            _ => self.config.help_open = false,
        }
        let _ = self.config.save();
    }

    /// A failed session save is shown in the notice window: a GUI build has no console
    /// for stderr.
    fn session_save_failed(&mut self, detail: String) {
        eprintln!("fasttail: cannot save session: {detail}");
        self.save_notice_title = "session_save_failed";
        self.save_notice = Some(detail);
    }

    /// Saves the live workspace to `file` and makes it the current session.
    pub fn save_session_as(&mut self, file: PathBuf) -> std::io::Result<()> {
        let session = self.capture_session();
        session.save_to(&file)?;
        if self.engines.iter().any(|e| e.is_stdin()) {
            self.save_notice_title = "stdin_not_saved_title";
            self.save_notice = Some(format!(
                "{}\n  {}",
                t(self.config.language, "stdin_not_saved"),
                crate::stdin_source::STDIN_TITLE
            ));
        }
        self.config.current_session = Some(file.clone());
        self.config.add_recent_session(&file);
        let _ = self.config.save();
        self.refresh_session_snapshot();
        // The pad goes with the session it is saved as.
        self.scratchpad.move_to(self.scratchpad_file());
        Ok(())
    }

    /// The file the scratchpad of the current session is saved to: beside the named
    /// session, else `scratchpad.txt` beside `fasttail.ini`.
    pub fn scratchpad_file(&self) -> PathBuf {
        match &self.config.current_session {
            Some(session) => crate::ui::scratchpad::sidecar_of(session),
            None => crate::ui::scratchpad::default_pad_of(&FastTailConfig::config_path()),
        }
    }

    /// Applies what the streams, the Find results tab and the scratchpad asked for this
    /// frame: rows to send, a reference line to show. Returns whether anything changed.
    pub fn apply_scratchpad_requests(&mut self) -> bool {
        let lang = self.config.language;
        let mut changed = false;
        for engine in self.engines.iter_mut() {
            let Some(with_reference) = engine.scratch_request.take() else {
                continue;
            };
            changed = true;
            let first = engine
                .selection_bounds()
                .map(|(first, _)| first)
                .or_else(|| engine.current_search_line());
            let (Some(first), Some(text)) = (first, engine.copy_selection_text()) else {
                engine.view_notice = Some(t(lang, "scratch_nothing").to_string());
                continue;
            };
            let name = crate::find_all::stream_name(engine);
            let path = engine.path.clone();
            engine.view_notice = Some(
                match self
                    .scratchpad
                    .send(&name, &path, first, &text, with_reference)
                {
                    Ok(()) => {
                        t(lang, "scratch_sent").replace("{n}", &text.lines().count().to_string())
                    }
                    Err(crate::ui::scratchpad::SendRefused::TooLarge) => {
                        t(lang, "scratch_too_large").to_string()
                    }
                    Err(crate::ui::scratchpad::SendRefused::Empty) => {
                        t(lang, "scratch_nothing").to_string()
                    }
                },
            );
        }
        if let Some((path, line)) = self.find_all.scratch.take() {
            if let Some(engine) = self.engines.iter().find(|e| e.path == path) {
                if let Some(text) = engine.get_line(line) {
                    let name = crate::find_all::stream_name(engine);
                    if self
                        .scratchpad
                        .send(&name, &path, line, &text, true)
                        .is_err()
                    {
                        self.scratchpad.notice =
                            Some((t(lang, "scratch_too_large").to_string(), true));
                    }
                    changed = true;
                }
            }
        }
        // A reference line picked in the pad: shown like "Show in context", the file
        // opened first when it is not.
        let open: Vec<(String, PathBuf)> = self
            .engines
            .iter()
            .map(|e| (crate::find_all::stream_name(e), e.path.clone()))
            .collect();
        self.scratchpad.resolve_jump(&open, lang);
        if let Some((path, line)) = self.scratchpad.jump.take() {
            if !self.engines.iter().any(|e| e.path == path) {
                self.open_log_file(path.clone());
            }
            if self.engines.iter().any(|e| e.path == path) {
                self.scratch_jump = Some((path, line));
            } else {
                // An archive asks which entry to open: its lines are not reachable here.
                self.scratchpad.notice = Some((
                    t(lang, "scratch_file_not_found")
                        .replace("{name}", &path.display().to_string()),
                    true,
                ));
            }
            changed = true;
        }
        // Shown once the stream has indexed that line (or is fully indexed).
        if let Some((path, line)) = self.scratch_jump.clone() {
            match self.engines.iter().find(|e| e.path == path) {
                None => self.scratch_jump = None,
                Some(engine) => {
                    let complete = !engine.index_pending
                        && engine.compressed.as_ref().is_none_or(|c| c.is_finalized());
                    if complete || engine.total_lines() > line {
                        self.scratch_jump = None;
                        self.find_all.jump = Some((path, line));
                        self.find_all.jump_in_context = true;
                        changed = true;
                    }
                }
            }
        }
        changed
    }

    /// Opens the stream at `idx`'s filter as a derived stream in a new tab next to it.
    /// Returns the derived stream's path, `None` when the stream has no filter or the
    /// spool cannot be created (the stream bar says why).
    pub fn open_filter_tab(&mut self, idx: usize) -> Option<PathBuf> {
        let lang = self.config.language;
        let source = self.engines.get(idx)?;
        if source.time_range_pending() {
            // The window is not applied yet: there is nothing to freeze.
            self.engines[idx].view_notice = Some(t(lang, "time_range_pending").to_string());
            return None;
        }
        let filter = crate::filter_tab::FrozenFilter::of(source);
        if !filter.is_active() {
            self.engines[idx].view_notice = Some(t(lang, "filter_tab_no_filter").to_string());
            return None;
        }
        // The next free number among the derived streams of this source.
        let n = crate::filter_tab::next_number(self.engines.iter(), &source.path);
        let engine = match self.derived_engine(idx, n, filter) {
            Ok(engine) => engine,
            Err(e) => {
                self.engines[idx].view_notice = Some(e);
                return None;
            }
        };
        let source_path = self.engines[idx].path.clone();
        let spool = engine.path.clone();
        self.engines.push(engine);
        self.place_derived_tab(&source_path, spool.clone());
        self.save_dock_layout();
        Some(spool)
    }

    /// A derived engine number `n` of the stream at `idx` through `filter`, following.
    fn derived_engine(
        &self,
        idx: usize,
        n: usize,
        filter: crate::filter_tab::FrozenFilter,
    ) -> Result<TailEngine, String> {
        let mut engine = crate::filter_tab::open_tab(&self.engines[idx], n, filter, &self.config)?;
        engine.set_quick_labels(&self.quick_labels);
        engine.set_global_filter(self.global_spec.clone());
        Ok(engine)
    }

    /// Puts a derived stream's tab next to its source: in the source's dock leaf.
    fn place_derived_tab(&mut self, source: &Path, spool: PathBuf) {
        let tab = FastTailTab::LogStream(spool.clone());
        match self
            .dock_state
            .find_tab(&FastTailTab::LogStream(source.to_path_buf()))
        {
            Some(at) => {
                self.dock_state.set_focused_node_and_surface(at.node_path());
                self.dock_state.push_to_focused_leaf(tab);
            }
            None => self.add_stream_tab(spool),
        }
    }

    /// Rebuilds the derived stream saved as `identity` (`filter:<n>:<source>`) from its
    /// source, opened first when it is not, with the state saved for it. `place` puts
    /// its tab next to the source; without it the caller has a tab for it already.
    /// Returns the spool path, `None` when the source or the saved filter is missing.
    fn restore_filter_tab(&mut self, identity: &Path, place: bool) -> Option<PathBuf> {
        let (n, source) = crate::filter_tab::parse_identity(identity)?;
        if let Some(existing) = self.engines.iter().find(|e| {
            e.derived
                .as_ref()
                .is_some_and(|f| f.n == n && paths_equal(&f.source, &source))
        }) {
            return Some(existing.path.clone());
        }
        // Opening the source saves the workspace, which drops the states of streams not
        // open yet: this one's is put back after.
        let state = self.config.stream_state_for(identity)?.clone();
        let filter = state.frozen.clone()?;
        let position = |engines: &[TailEngine]| {
            engines
                .iter()
                .position(|e| e.derived.is_none() && paths_equal(&e.path, &source))
        };
        if position(&self.engines).is_none() {
            self.open_log_file(source.clone());
        }
        let idx = position(&self.engines)?;
        self.config.set_stream_state(state);
        let mut engine = self.derived_engine(idx, n, filter).ok()?;
        crate::workspace::restore_stream(&mut engine, &self.config, identity);
        let source_path = self.engines[idx].path.clone();
        let spool = engine.path.clone();
        self.engines.push(engine);
        if place {
            self.place_derived_tab(&source_path, spool.clone());
        }
        Some(spool)
    }

    /// Opens a merged view of the streams `sources` (two or more, by path) in a new tab and
    /// returns its path; `None` when its spool cannot be created (the notice says why).
    pub fn open_merged_view(&mut self, sources: &[PathBuf]) -> Option<PathBuf> {
        let n = crate::merged_stream::next_number(self.engines.iter());
        let engine = self.merged_engine(n, sources)?;
        let path = engine.path.clone();
        self.engines.push(engine);
        self.add_stream_tab(path.clone());
        self.save_dock_layout();
        Some(path)
    }

    /// The engine of merged view `n` of the open streams `sources` (by path), `None` with
    /// fewer than two of them open or when its spool cannot be created (the notice says
    /// why).
    fn merged_engine(&mut self, n: usize, sources: &[PathBuf]) -> Option<TailEngine> {
        let found: Vec<&TailEngine> = sources
            .iter()
            .filter_map(|p| self.engines.iter().find(|e| paths_equal(&e.path, p)))
            .collect();
        if found.len() < 2 {
            return None;
        }
        let mut engine = match crate::merged_stream::open_view(n, &found, &self.config) {
            Ok(engine) => engine,
            Err(e) => {
                self.open_notice = Some(e.to_string());
                return None;
            }
        };
        engine.set_quick_labels(&self.quick_labels);
        engine.set_global_filter(self.global_spec.clone());
        Some(engine)
    }

    /// Rebuilds the merged view saved as `identity` (`merged:<n>:<sources>`) from its
    /// sources, opening the ones that are not open, with the state saved for it. `place`
    /// gives it a tab; without it the caller has one for it already. Returns the spool
    /// path, `None` when fewer than two sources open.
    fn restore_merged_view(&mut self, identity: &Path, place: bool) -> Option<PathBuf> {
        let (n, saved) = crate::merged_stream::parse_identity(identity)?;
        if let Some(existing) = self.engines.iter().find(|e| {
            e.merged
                .as_ref()
                .and_then(|m| m.identity())
                .is_some_and(|i| i == identity)
        }) {
            return Some(existing.path.clone());
        }
        // Opening a source saves the workspace, which drops the states of streams not
        // open yet: this one's is put back after.
        let state = self.config.stream_state_for(identity).cloned();
        let find = |engines: &[TailEngine], s: &Path| {
            engines
                .iter()
                .find(|e| paths_equal(&crate::workspace::persisted_path(e), s))
                .map(|e| e.path.clone())
        };
        let mut sources = Vec::new();
        for source in &saved {
            if find(&self.engines, source).is_none() {
                if crate::filter_tab::parse_identity(source).is_some() {
                    self.restore_filter_tab(source, true);
                } else {
                    self.open_log_file(source.clone());
                }
            }
            sources.extend(find(&self.engines, source));
        }
        if let Some(state) = state {
            self.config.set_stream_state(state);
        }
        let mut engine = self.merged_engine(n, &sources)?;
        crate::workspace::restore_stream(&mut engine, &self.config, identity);
        let spool = engine.path.clone();
        self.engines.push(engine);
        if place {
            self.add_stream_tab(spool.clone());
        }
        Some(spool)
    }

    /// Opens the Log formats dialog on the formats folder next to `fasttail.ini`, its
    /// preview reading stream `source` (the focused one when `None`); with `lines`, a new
    /// format whose samples they are ("Create format from these lines").
    pub fn open_log_formats(&mut self, lines: Option<Vec<String>>, source: Option<usize>) {
        use crate::ui::log_formats::{FormatsDialog, PREVIEW_LINES};
        let source = match source {
            Some(idx) => Some(idx),
            None => self
                .focused_stream_path()
                .and_then(|p| self.engines.iter().position(|e| e.path == p)),
        };
        let stream_lines: Vec<String> = source
            .and_then(|idx| self.engines.get(idx))
            .map(|e| {
                (0..e.total_lines().min(PREVIEW_LINES))
                    .filter_map(|i| e.get_line(i).map(|l| l.into_owned()))
                    .collect()
            })
            .unwrap_or_default();
        let dir = crate::log_format::default_dir();
        self.log_formats = Some(match lines {
            Some(lines) => FormatsDialog::from_lines(dir, stream_lines, &lines),
            None => FormatsDialog::new(dir, stream_lines),
        });
    }

    /// Loads the formats folder `dir` again and gives every stream the new formats: a
    /// stream on a format reads with its new version, an automatic one is detected again.
    pub fn reload_log_formats(&mut self, dir: &Path) {
        crate::log_format::set_library(crate::log_format::Library::load(dir));
        for engine in &mut self.engines {
            engine.reload_field_parser();
        }
    }

    /// Opens the Split file dialog on stream `idx`.
    pub fn open_split_dialog(&mut self, idx: usize) {
        let Some(engine) = self.engines.get(idx) else {
            return;
        };
        let file = engine
            .current_file
            .clone()
            .unwrap_or_else(|| engine.path.clone());
        self.split_dialog = Some(crate::ui::split_dialog::SplitDialog::new(
            engine.path.clone(),
            &file,
        ));
    }

    /// Opens the Log formats dialog a row menu asked for, with the lines picked.
    /// The stream bar's Include rotations: the stream is opened again with its file's
    /// rotations, in the same tab, its filters and search kept.
    fn apply_rotation_requests(&mut self) {
        let Some(idx) = self.engines.iter().position(|e| e.rotation_request) else {
            return;
        };
        self.engines[idx].rotation_request = false;
        let old_path = self.engines[idx].path.clone();
        let id = crate::rotation::identity(&old_path);
        let lang = self.config.language;
        let wake = Self::make_wake(&self.egui_ctx);
        match crate::workspace::open_target(&id, &self.config, Some(wake)) {
            OpenOutcome::Opened(engine) => {
                let mut engine = *engine;
                crate::workspace::apply_settings(&mut engine, &self.config);
                engine.set_quick_labels(&self.quick_labels);
                let old = &self.engines[idx];
                let (include, exclude, search) = (
                    old.include_filter().to_string(),
                    old.exclude_filter().to_string(),
                    old.search_query.clone(),
                );
                if !include.is_empty() {
                    engine.set_include_filter(&include);
                }
                if !exclude.is_empty() {
                    engine.set_exclude_filter(&exclude);
                }
                engine.search_query = search;
                engine.search_edited_at = None;
                self.engines[idx] = engine;
                rename_tab(&mut self.dock_state, &old_path, Some(id.clone()));
                for p in &mut self.config.open_files {
                    if paths_equal(p, &old_path) {
                        *p = id.clone();
                    }
                }
                self.config.add_recent_file(&id);
                let _ = self.config.save();
                self.save_dock_layout();
            }
            _ => {
                self.engines[idx].view_notice = Some(t(lang, "rot_failed").to_string());
            }
        }
    }

    fn apply_format_requests(&mut self) {
        self.apply_plugin_requests();
        self.apply_rotation_requests();
        self.apply_source_requests();
        let commands = self.config.system_commands();
        let lang = self.config.language;
        for engine in &mut self.engines {
            if let Some(err) = engine
                .apply_reconnect_request(&commands)
                .or_else(|| engine.apply_ssh_request())
            {
                engine.view_notice = Some(t(lang, "system_reconnect_failed").replace("{0}", &err));
            }
        }
        if let Some(idx) = self.engines.iter().position(|e| e.export_request.is_some()) {
            let scope = self.engines[idx].export_request.take().expect("a request");
            self.export_dialog = Some(crate::ui::export_dialog::ExportDialog::new(
                self.engines[idx].path.clone(),
                scope,
                &self.config,
            ));
        }
        if let Some(idx) = self.engines.iter().position(|e| e.split_request) {
            self.engines[idx].split_request = false;
            self.open_split_dialog(idx);
        }
        let Some(idx) = self.engines.iter().position(|e| e.format_request.is_some()) else {
            return;
        };
        let Some(row) = self.engines[idx].format_request.take() else {
            return;
        };
        let engine = &self.engines[idx];
        let picked = if engine.is_selected(row) {
            engine
                .selected_lines_capped(crate::log_format::MAX_SAMPLES)
                .0
        } else {
            vec![row]
        };
        let lines: Vec<String> = picked
            .into_iter()
            .filter_map(|i| engine.get_line(i).map(|l| l.into_owned()))
            .collect();
        self.open_log_formats(Some(lines), Some(idx));
    }

    /// Feeds the merged views from their sources (a few milliseconds each per frame) and
    /// opens the dialog a stream menu asked for. Returns whether anything changed.
    pub fn apply_merged_views(&mut self) -> bool {
        self.apply_format_requests();
        let mut changed = false;
        if let Some(idx) = self.engines.iter().position(|e| e.merged_request) {
            self.engines[idx].merged_request = false;
            let first = self.engines[idx].path.clone();
            self.merge_prompt = Some(crate::ui::merge_dialog::MergePrompt::starting_with(Some(
                first,
            )));
            changed = true;
        }
        let mut engines: Vec<&mut TailEngine> = self.engines.iter_mut().collect();
        changed |=
            crate::merged_stream::feed_views(&mut engines, std::time::Duration::from_millis(4));
        changed
    }

    /// Opens the Explain tab for the span a stream asked for. Returns whether one opened.
    pub fn apply_explain_requests(&mut self) -> bool {
        let Some(idx) = self
            .engines
            .iter()
            .position(|e| e.explain_request.is_some())
        else {
            return false;
        };
        let engine = &mut self.engines[idx];
        let span = engine.explain_request.take().unwrap_or_default();
        let (baseline, only) = engine
            .explain
            .as_ref()
            .map_or((crate::spike::Baseline::Rest, false), |e| {
                (e.sides.baseline, e.only_filtered)
            });
        engine.explain_span(span, baseline, only);
        let path = engine.path.clone();
        crate::ui::explain_tab::open_tab(&mut self.dock_state, &path);
        true
    }

    /// Opens the Patterns tab a stream asked for. Returns whether one opened.
    pub fn apply_patterns_requests(&mut self) -> bool {
        let Some(idx) = self.engines.iter().position(|e| e.patterns_request) else {
            return false;
        };
        self.engines[idx].patterns_request = false;
        self.engines[idx].open_patterns();
        let path = self.engines[idx].path.clone();
        crate::ui::patterns_tab::open_tab(&mut self.dock_state, &path);
        true
    }

    /// Opens the Query tab a stream asked for, and shows a line a query result points to.
    /// Returns whether anything changed.
    pub fn apply_query_requests(&mut self) -> bool {
        let mut changed = false;
        if let Some(idx) = self.engines.iter().position(|e| e.query_request) {
            self.engines[idx].query_request = false;
            let path = self.engines[idx].path.clone();
            crate::ui::query_tab::open_tab(&mut self.dock_state, &path);
            changed = true;
        }
        let lang = self.config.language;
        for i in 0..self.engines.len() {
            let Some(line) = self.engines[i].query_jump.take() else {
                continue;
            };
            self.engines[i].request_jump(line, lang);
            let path = self.engines[i].path.clone();
            if let Some(tab) = self.dock_state.find_tab(&FastTailTab::LogStream(path)) {
                let _ = self.dock_state.set_active_tab(tab);
            }
            changed = true;
        }
        changed
    }

    /// Opens the Statistics tab a stream asked for: of a field, or of the stream (its
    /// first field, or the regex picker without a parser). Returns whether one opened.
    pub fn apply_stats_requests(&mut self) -> bool {
        let Some(idx) = self.engines.iter().position(|e| e.stats_request.is_some()) else {
            return false;
        };
        let engine = &mut self.engines[idx];
        let wanted = engine.stats_request.take().flatten();
        let key = wanted.or_else(|| {
            if engine.stats.is_some() {
                None
            } else {
                engine.field_catalogue().keys().first().cloned()
            }
        });
        if let Some(key) = key {
            let slices = engine
                .stats
                .as_ref()
                .map_or(crate::field_stats::SliceChoice::Auto, |s| s.slice_choice);
            engine.open_stats(crate::field_stats::StatsSource::Field(key), slices);
        }
        let path = engine.path.clone();
        crate::ui::stats_tab::open_tab(&mut self.dock_state, &path);
        true
    }

    /// Opens the Operations tab a stream asked for: by a field, or of the stream (its
    /// first field, or the regex picker without a parser). Returns whether one opened.
    pub fn apply_ops_requests(&mut self) -> bool {
        let Some(idx) = self.engines.iter().position(|e| e.ops_request.is_some()) else {
            return false;
        };
        let engine = &mut self.engines[idx];
        let wanted = engine.ops_request.take().flatten();
        let key = wanted.or_else(|| {
            if engine.operations.is_some() {
                None
            } else {
                engine.field_catalogue().keys().first().cloned()
            }
        });
        if let Some(key) = key {
            let name = engine
                .operations
                .as_ref()
                .map_or(crate::operations::NameSource::None, |o| o.name.clone());
            engine.open_operations(crate::field_stats::StatsSource::Field(key), name);
        }
        let path = engine.path.clone();
        crate::ui::operations_tab::open_tab(&mut self.dock_state, &path);
        true
    }

    /// Feeds the derived streams from their sources (a few milliseconds each per frame)
    /// and applies their requests: a filter to open as a tab, a line to show in the
    /// source. Returns whether anything changed.
    pub fn apply_filter_tab_requests(&mut self) -> bool {
        let mut changed = false;
        if let Some(idx) = self.engines.iter().position(|e| e.filter_tab_request) {
            self.engines[idx].filter_tab_request = false;
            changed |= self.open_filter_tab(idx).is_some();
        }
        for d in 0..self.engines.len() {
            let Some(source_path) = self.engines[d].derived.as_ref().map(|f| f.source.clone())
            else {
                continue;
            };
            if let Some(line) = self.engines[d].source_context_request.take() {
                // Shown like "Show in context" in the source (opened again if closed).
                if !self.engines.iter().any(|e| e.path == source_path) {
                    self.open_log_file(source_path.clone());
                }
                self.scratch_jump = Some((source_path.clone(), line));
                changed = true;
            }
        }
        let mut engines: Vec<&mut TailEngine> = self.engines.iter_mut().collect();
        changed |= crate::filter_tab::feed_tabs(&mut engines, std::time::Duration::from_millis(4));
        changed
    }

    /// Applies the compares the row menus asked for, and a line double-clicked in the
    /// Compare tab. Returns whether anything changed.
    pub fn apply_compare_requests(&mut self) -> bool {
        use crate::compare::CompareRequest as C;
        use crate::ui::compare_tab::{CompareSide, CompareView};
        let lang = self.config.language;
        let mut changed = false;
        for i in 0..self.engines.len() {
            let Some(request) = self.engines[i].compare_request.take() else {
                continue;
            };
            changed = true;
            let engine = &self.engines[i];
            let (lines, capped) = engine.selected_lines_capped(crate::compare::MAX_REGION_LINES);
            let side = |engine: &TailEngine, lines: Vec<usize>| {
                let mut side = CompareSide::of(engine, lines);
                side.capped |= capped;
                side
            };
            match request {
                C::SelectedPair if lines.len() == 2 => {
                    let left = CompareSide::of(engine, vec![lines[0]]);
                    let right = CompareSide::of(engine, vec![lines[1]]);
                    self.compare = Some(CompareView::new(left, right));
                    crate::ui::compare_tab::open_tab(&mut self.dock_state);
                }
                C::SelectedPair => {}
                C::Mark => {
                    let side = side(engine, lines);
                    self.engines[i].view_notice =
                        Some(t(lang, "compare_marked").replace("{mark}", &side.label()));
                    self.compare_mark = Some(side);
                }
                C::WithMark => {
                    if let Some(mark) = self.compare_mark.clone() {
                        let right = side(engine, lines);
                        self.compare = Some(CompareView::new(mark, right));
                        crate::ui::compare_tab::open_tab(&mut self.dock_state);
                    }
                }
            }
        }
        // A closed stream takes its mark with it.
        if let Some(mark) = &self.compare_mark {
            if !self.engines.iter().any(|e| e.path == mark.path) {
                self.compare_mark = None;
            }
        }
        if let Some((path, line)) = self.compare.as_mut().and_then(|v| v.jump.take()) {
            if self.engines.iter().any(|e| e.path == path) {
                self.find_all.jump = Some((path, line));
                self.find_all.jump_in_context = false;
                changed = true;
            }
        }
        changed
    }

    /// The app icon (RGBA, 128 × 128), also used for the tray.
    const ICON_RGBA: &'static [u8] = include_bytes!("../../assets/icon-128.rgba");

    /// The terminal was picked in Settings. On Windows: "Switch now" saves the workspace,
    /// hands off to `fasttail-tui.exe` and quits, or reports why it could not while the
    /// window keeps running; "At next start" closes the dialog. On Linux and macOS a
    /// window has no terminal to switch into: the dialog says when the choice applies.
    fn draw_interface_switch(&mut self, ctx: &egui::Context) {
        let lang = self.config.language;
        let theme = self.config.theme;
        let mut is_open = true;
        let mut now = false;
        let mut close = false;
        egui::Window::new(
            RichText::new(format!("⇄ {}", t(lang, "interface_switch_title")))
                .monospace()
                .color(theme.accent_color()),
        )
        .id(egui::Id::new("fasttail_interface_switch"))
        .open(&mut is_open)
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            let body = if cfg!(windows) {
                t(lang, "interface_switch_body")
            } else {
                t(lang, "interface_next_terminal_start")
            };
            ui.label(
                RichText::new(body)
                    .monospace()
                    .size(11.5)
                    .color(theme.text_primary()),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if cfg!(windows) {
                    now = ui.button(t(lang, "interface_switch_now")).clicked();
                    close = ui.button(t(lang, "interface_switch_later")).clicked();
                } else {
                    close = ui.button(t(lang, "session_ok")).clicked();
                }
            });
            close |= ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        });
        if now {
            self.interface_switch = false;
            self.switch_to_terminal(ctx);
        } else if close || !is_open {
            self.interface_switch = false;
        }
    }

    /// Settings "Switch now" (Windows): the workspace is saved, then `fasttail-tui.exe`
    /// starts in a new console and the window quits; when it cannot start, the window
    /// stays and says why.
    #[cfg(windows)]
    fn switch_to_terminal(&mut self, ctx: &egui::Context) {
        self.save_dock_layout();
        match crate::handoff::start_tui_in_new_console(&[]) {
            Ok(()) => self.quit(ctx),
            Err(e) => {
                self.handoff_notice =
                    Some(crate::handoff::tui_start_failure(self.config.language, &e));
            }
        }
    }

    #[cfg(not(windows))]
    fn switch_to_terminal(&mut self, _ctx: &egui::Context) {}

    /// Quits for real, even with close to tray on.
    pub fn quit(&mut self, ctx: &egui::Context) {
        self.quitting = true;
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }

    /// Hides the window in the tray; the tray keeps waking the app so the streams, rules
    /// and sounds go on.
    pub fn hide_to_tray(&mut self, ctx: &egui::Context) {
        if self.tray.is_none() {
            return;
        }
        self.save_dock_layout();
        self.hidden_in_tray = true;
        if let Some(tray) = &self.tray {
            tray.set_hidden(true);
        }
        ctx.send_viewport_cmd(ViewportCommand::Visible(false));
    }

    /// Shows the window hidden in the tray, focused.
    pub fn show_from_tray(&mut self, ctx: &egui::Context) {
        self.hidden_in_tray = false;
        if let Some(tray) = &self.tray {
            tray.set_hidden(false);
        }
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
    }

    /// The tray menu as it should read now.
    fn tray_menu(&self) -> crate::tray::TrayMenu {
        let lang = self.config.language;
        crate::tray::TrayMenu {
            show: t(lang, "tray_show").to_string(),
            hide: t(lang, "tray_hide").to_string(),
            follow_all: t(lang, "act_play_all").to_string(),
            pause_all: t(lang, "act_pause_all").to_string(),
            mute: t(lang, "tray_mute").to_string(),
            quit: t(lang, "tray_quit").to_string(),
            visible: !self.hidden_in_tray,
            muted: !self.config.sound_enabled,
            streams: self
                .engines
                .iter()
                .map(crate::find_all::stream_name)
                .collect(),
        }
    }

    /// Starts or stops the tray icon with the option, applies its clicks and menu picks,
    /// and keeps its menu and alert badge up to date.
    fn update_tray(&mut self, ctx: &egui::Context) {
        if !self.config.tray_icon || !crate::tray::Tray::available() {
            if self.tray.is_some() {
                if self.hidden_in_tray {
                    self.show_from_tray(ctx);
                }
                self.tray = None;
            }
            self.tray_failed = false;
            return;
        }
        if self.tray.is_none() && !self.tray_failed {
            let menu = self.tray_menu();
            self.tray = crate::tray::Tray::start(
                ctx.clone(),
                Self::ICON_RGBA,
                128,
                "FastTail",
                menu.clone(),
            );
            self.tray_failed = self.tray.is_none();
            self.tray_menu_sent = menu;
            self.tray_badge_sent = None;
        }
        let Some(events) = self.tray.as_ref().map(|t| t.events()) else {
            return;
        };
        for event in events {
            use crate::tray::TrayEvent as E;
            // While the workspace is locked the tray only shows, hides or quits.
            if self.locked && !matches!(event, E::Toggle | E::Show | E::Quit) {
                continue;
            }
            match event {
                E::Toggle if self.hidden_in_tray => self.show_from_tray(ctx),
                E::Toggle => self.hide_to_tray(ctx),
                E::Show => self.show_from_tray(ctx),
                E::FollowAll => {
                    for eng in &mut self.engines {
                        eng.is_watching = true;
                        eng.follow_tail = !eng.is_compressed();
                    }
                }
                E::PauseAll => {
                    for eng in &mut self.engines {
                        eng.is_watching = false;
                        eng.follow_tail = false;
                    }
                }
                E::ToggleMute => {
                    self.config.sound_enabled = !self.config.sound_enabled;
                    let _ = self.config.save();
                }
                E::Stream(i) => {
                    self.show_from_tray(ctx);
                    if let Some(path) = self.engines.get(i).map(|e| e.path.clone()) {
                        if let Some(tab) = self.dock_state.find_tab(&FastTailTab::LogStream(path)) {
                            let _ = self.dock_state.set_active_tab(tab);
                            self.dock_state
                                .set_focused_node_and_surface(tab.node_path());
                        }
                    }
                }
                E::Quit => self.quit(ctx),
            }
        }
        let menu = self.tray_menu();
        if menu != self.tray_menu_sent {
            if let Some(tray) = &self.tray {
                tray.set_menu(menu.clone());
            }
            self.tray_menu_sent = menu;
        }
        // The badge: alerts sounded while the window is hidden or not focused.
        let total: u64 = self.engines.iter().map(|e| e.sound_alerts).sum();
        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        // A stream closed meanwhile takes its alerts with it.
        self.alert_baseline = self.alert_baseline.min(total);
        if focused && !self.hidden_in_tray {
            self.alert_baseline = total;
        }
        let alerts = total.saturating_sub(self.alert_baseline);
        let lang = self.config.language;
        let mut tooltip = t(lang, "tray_tip").replace("{n}", &self.engines.len().to_string());
        if alerts > 0 {
            let count = if alerts > 99 {
                "99+".to_string()
            } else {
                alerts.to_string()
            };
            tooltip.push_str(&t(lang, "tray_tip_alerts").replace("{n}", &count));
        }
        let badge = (alerts > 0, tooltip);
        if self.tray_badge_sent.as_ref() != Some(&badge) {
            if let Some(tray) = &self.tray {
                tray.set_badge(badge.0.then_some([220, 40, 40]), &badge.1);
            }
            self.tray_badge_sent = Some(badge);
        }
    }

    /// Opens the Scratchpad tab (or brings it to the front).
    pub fn open_scratchpad(&mut self) {
        crate::ui::scratchpad::open_tab(&mut self.dock_state);
    }

    /// Opens the bookmark report dialog for one stream or for every open stream.
    pub fn open_bookmark_report(&mut self, scope: crate::ui::report_dialog::ReportScope) {
        self.report_dialog = Some(crate::ui::report_dialog::ReportDialog::new(
            scope,
            &self.config,
        ));
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
        self.scratchpad.move_to(self.scratchpad_file());
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
            Ok(loaded) => {
                self.replace_workspace(loaded, Some(file));
                // The pad of the session loaded (the current one is saved first).
                let pad = self.scratchpad_file();
                self.scratchpad.switch_to(pad);
            }
            Err(err) => {
                eprintln!("fasttail: cannot load session {}: {err}", file.display());
                self.session_missing = Some(vec![file]);
            }
        }
    }

    /// Closes every stream and opens the ones of `loaded`, restoring their state and, when
    /// its tabs match, the saved dock layout.
    pub fn replace_workspace(&mut self, loaded: LoadedSession, file: Option<PathBuf>) {
        // The Find results tab goes with the old dock: cancel its jobs and forget its
        // results, or reopened streams would show them as current.
        self.find_all.close();
        // Standard input cannot be reopened: its stream survives the switch.
        let stdin = self
            .engines
            .iter()
            .position(|e| e.is_stdin())
            .map(|i| self.engines.remove(i));
        self.engines.clear();
        self.folder_groups.clear();
        self.dock_state = DockState::new(vec![]);
        self.floating_window_rects.clear();
        self.config.open_files.clear();
        self.config.streams.clear();
        // Each state just before its stream opens (an opening saves the workspace, which
        // keeps only the states of open streams). A derived stream may open its source
        // before the source's own entry comes: the source's state goes in first.
        let adopt = |config: &mut FastTailConfig, entry: &StreamEntry| {
            config.set_stream_state(entry.clone());
            config.set_wrap(&entry.path, entry.wrap);
            config.set_bookmarks_with_notes(&entry.path, &entry.bookmarks, &entry.bookmark_notes);
        };
        for entry in &loaded.session.streams {
            // A derived stream's source, a merged view's sources, by their saved names.
            let sources = match crate::filter_tab::parse_identity(&entry.path) {
                Some((_, source)) => vec![source],
                None => crate::merged_stream::parse_identity(&entry.path)
                    .map(|(_, sources)| sources)
                    .unwrap_or_default(),
            };
            for source in &sources {
                let source_entry = loaded
                    .session
                    .streams
                    .iter()
                    .find(|s| paths_equal(&s.path, source));
                let source_open = self
                    .engines
                    .iter()
                    .any(|e| paths_equal(&crate::workspace::persisted_path(e), source));
                if let (Some(source_entry), false) = (source_entry, source_open) {
                    adopt(&mut self.config, source_entry);
                }
            }
            adopt(&mut self.config, entry);
            self.open_log_file(entry.path.clone());
        }
        if let Some(layout) = &loaded.session.dock_layout {
            if let Ok(mut ds) = ron::from_str::<DockState<FastTailTab>>(layout) {
                crate::ui::find_results::ensure_main_surface(&mut ds);
                let tabs: Vec<PathBuf> = ds
                    .iter_all_tabs()
                    .filter_map(|(_, t)| match t {
                        FastTailTab::LogStream(p) => Some(p.clone()),
                        _ => None,
                    })
                    .collect();
                let matches = !tabs
                    .iter()
                    .any(|p| crate::stdin_source::is_ephemeral_path(p))
                    && tabs.len() == self.engines.len()
                    && tabs.iter().all(|t| {
                        self.engines
                            .iter()
                            .any(|e| paths_equal(&crate::workspace::persisted_path(e), t))
                    });
                if matches {
                    rename_derived_tabs(&mut ds, &self.engines, false);
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
        // After the layout, which names only the files saved: new ones join their group.
        for path in &loaded.session.folders {
            self.restore_folder_group(path);
        }
        if let Some(engine) = stdin {
            self.add_stream_tab(engine.path.clone());
            self.engines.push(engine);
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
        if !crate::lock::can_lock(&self.config) {
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
    /// three wrong PINs in a row close the prompt for `lock::LOCK_COOLDOWN`.
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
                    .fill(theme.panel_bg().into())
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
        let order: Vec<PathBuf> = self
            .dock_state
            .iter_all_tabs()
            .filter_map(|(_, tab)| match tab {
                FastTailTab::LogStream(p) => Some(p.clone()),
                _ => None,
            })
            .collect();
        crate::workspace::snapshot(&order, &self.engines).write_into(&mut self.config);
        // The groups with a tab open; one still waiting for its first file is not kept.
        self.config.folder_groups = self
            .folder_groups
            .iter()
            .filter(|g| self.engines.iter().any(|e| g.is_member(&e.path)))
            .map(|g| g.path())
            .collect();

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
        let mut dock_to_save = crate::ui::find_results::without_find_results(&dock_to_save);

        // After the floating rectangles, whose surface indices this may shift.
        rename_derived_tabs(&mut dock_to_save, &self.engines, true);
        crate::ui::find_results::retain_tabs(&mut dock_to_save, |tab| !is_stdin_tab(tab));
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
            let pattern_dir_exists =
                crate::wildcard::FolderSpec::parse(path).is_some_and(|spec| spec.dir.is_dir());
            if !path.exists() && !pattern_dir_exists && !crate::system_source::is_system_path(path)
            {
                eprintln!("fasttail: {} not found, skipped", path.display());
                continue;
            }
            // `--all`: the pattern opens as a folder group, its streams get the options.
            if let Some(spec) = cli
                .all
                .then(|| crate::wildcard::FolderSpec::parse(path))
                .flatten()
            {
                let group = spec.path();
                self.open_folder_group(spec);
                let members: Vec<PathBuf> = self
                    .folder_groups
                    .iter()
                    .filter(|g| paths_equal(&g.path(), &group))
                    .flat_map(|g| {
                        self.engines
                            .iter()
                            .filter(|e| g.is_member(&e.path))
                            .map(|e| e.path.clone())
                    })
                    .collect();
                for member in members {
                    self.apply_cli_to(&member, cli);
                }
                continue;
            }
            // `--rotations`: the file with its rotations.
            let opened = cli.opened_path(path);
            self.open_log_file(opened.clone());
            // A plugin stream is kept under its stored identity (no secrets).
            self.apply_cli_to(&crate::plugins::stored_path(&opened), cli);
        }
    }

    /// The command-line filters, follow flag, time window and line of the stream `path`.
    fn apply_cli_to(&mut self, path: &Path, cli: &crate::cli::CliArgs) {
        let lang = self.config.language;
        let Some(engine) = self
            .engines
            .iter_mut()
            .find(|e| e.path == path || paths_equal(&e.path, path))
        else {
            return;
        };
        if let Some(f) = cli.window_filter() {
            engine.set_include_filter(f);
        }
        if let Some(expr) = &cli.filter_expr {
            engine.set_filter_expr(expr);
            engine.set_filter_expr_mode(true);
        }
        if let Some(x) = cli.window_exclude() {
            engine.set_exclude_filter(x);
        }
        if let Some(follow) = cli.follow {
            engine.follow_tail = follow && !engine.is_compressed();
        }
        crate::cli::apply_time_window(engine, cli.since.as_deref(), cli.until.as_deref());
        // Last: after the filters, so a line they hide lands as go to does.
        if let Some(line) = cli.line_of(path) {
            engine.open_at_line(line, lang);
        }
    }

    /// Starts copying `input` (standard input) into a spool. `explicit` (`-` given)
    /// opens the stream at once; otherwise it opens on the first byte (see
    /// `poll_pending_stdin`), so a launcher's silent pipe never shows an empty tab.
    pub fn start_stdin(
        &mut self,
        input: impl std::io::Read + Send + 'static,
        explicit: bool,
        options: StdinOptions,
    ) {
        let wake = Self::make_wake(&self.egui_ctx);
        let stream = match crate::stdin_source::StdinStream::start(
            input,
            &self.config.stdin_settings(),
            Some(wake),
        ) {
            Ok(stream) => stream,
            Err(err) => {
                eprintln!("fasttail: cannot spool standard input: {err}");
                return;
            }
        };
        self.stdin_options = options;
        if explicit {
            self.open_stdin_stream(stream);
        } else {
            self.pending_stdin = Some(stream);
        }
    }

    /// Opens the stream of piped standard input once its first byte arrived; drops it
    /// when the input ended empty.
    pub fn poll_pending_stdin(&mut self) {
        let Some(stream) = self.pending_stdin.as_ref() else {
            return;
        };
        if stream.received() > 0 {
            if let Some(stream) = self.pending_stdin.take() {
                self.open_stdin_stream(stream);
            }
        } else if !stream.is_reading() {
            self.pending_stdin = None;
        }
    }

    /// Adds the standard-input stream and its tab. Unlike `open_log_file` it is not
    /// recorded in the open files, the recent files or the stream state.
    fn open_stdin_stream(&mut self, stream: crate::stdin_source::StdinStream) {
        let wake = Self::make_wake(&self.egui_ctx);
        let mut engine = match crate::stdin_source::open_engine(stream, Some(wake)) {
            Ok(engine) => engine,
            Err(err) => {
                eprintln!("fasttail: cannot open standard input: {err}");
                return;
            }
        };
        crate::workspace::apply_settings(&mut engine, &self.config);
        engine.set_quick_labels(&self.quick_labels);
        let options = self.stdin_options.clone();
        if let Some(f) = &options.filter {
            engine.set_include_filter(f);
        }
        if let Some(expr) = &options.filter_expr {
            engine.set_filter_expr(expr);
            engine.set_filter_expr_mode(true);
        }
        if let Some(x) = &options.exclude {
            engine.set_exclude_filter(x);
        }
        if let Some(follow) = options.follow {
            engine.follow_tail = follow;
        }
        crate::cli::apply_time_window(
            &mut engine,
            options.since.as_deref(),
            options.until.as_deref(),
        );
        let path = engine.path.clone();
        self.engines.push(engine);
        crate::audio::play_sound(
            crate::audio::CyberSound::BlipAttach,
            self.config.sound_enabled,
        );
        self.add_stream_tab(path);
        self.save_dock_layout();
    }

    /// Asks the window for the clipboard text (egui answers with a paste event).
    fn request_paste(&mut self, ctx: &egui::Context, target: PasteTarget) {
        self.paste_request = Some((target, std::time::Instant::now()));
        ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaste);
        ctx.request_repaint();
    }

    /// Takes this frame's paste for a clipboard stream: the text a request asked for, or
    /// `Ctrl + Shift + V` outside a text field; a stream bar's Append asks for one.
    fn take_clipboard_paste(&mut self, ctx: &egui::Context) {
        if let Some(path) = self.engines.iter_mut().find_map(|e| {
            let c = e.clipboard.as_mut()?;
            std::mem::take(&mut c.append_requested).then(|| e.path.clone())
        }) {
            self.request_paste(ctx, PasteTarget::Append(path));
        }
        if self
            .paste_request
            .as_ref()
            .is_some_and(|(_, at)| at.elapsed() > std::time::Duration::from_secs(2))
        {
            // The clipboard held no text: no paste event came.
            self.paste_request = None;
        }
        let shortcut = crate::keymap::KeyBinding::parse(PASTE_STREAM_KEYS).is_some_and(|b| {
            self.keymap
                .bindings(crate::actions::ActionId::PasteAsStream)
                .contains(&b)
        });
        let typing = ctx.egui_wants_keyboard_input();
        let wanted = self.paste_request.is_some();
        let mut text = None;
        ctx.input_mut(|i| {
            let keys = i.modifiers.command && i.modifiers.shift;
            i.events.retain(|e| match e {
                egui::Event::Paste(t)
                    if text.is_none() && (wanted || (keys && shortcut && !typing)) =>
                {
                    text = Some(t.clone());
                    false
                }
                _ => true,
            });
        });
        let Some(text) = text else {
            return;
        };
        let target = self
            .paste_request
            .take()
            .map_or(PasteTarget::New, |(target, _)| target);
        self.paste_text(&text, target);
    }

    /// Opens `text` as a clipboard stream, or appends it to the one of `target`.
    pub fn paste_text(&mut self, text: &str, target: PasteTarget) {
        let lang = self.config.language;
        let settings = self.config.stdin_settings();
        let wake = Self::make_wake(&self.egui_ctx);
        let result = match &target {
            PasteTarget::Append(path) => match self.engines.iter_mut().find(|e| e.path == *path) {
                Some(engine) => engine.append_clipboard(text, &settings, Some(wake)),
                None => return,
            },
            PasteTarget::New => {
                let clock = crate::clipboard_source::clock_now();
                crate::clipboard_source::open_engine(text, &clock, &settings, Some(wake)).map(
                    |mut engine| {
                        crate::workspace::apply_settings(&mut engine, &self.config);
                        engine.set_quick_labels(&self.quick_labels);
                        let path = engine.path.clone();
                        self.engines.push(engine);
                        self.add_stream_tab(path);
                    },
                )
            }
        };
        if let Err(e) = result {
            let message = match e {
                crate::clipboard_source::PasteError::Empty => t(lang, "clip_empty").to_string(),
                crate::clipboard_source::PasteError::TooLarge(cap) => t(lang, "clip_too_large")
                    .replace("{0}", &crate::ui::zip_picker::human_size(cap)),
                crate::clipboard_source::PasteError::Busy => t(lang, "clip_busy").to_string(),
                crate::clipboard_source::PasteError::Io(e) => e.to_string(),
            };
            let at = match &target {
                PasteTarget::Append(path) => Some(path.clone()),
                PasteTarget::New => self.focused_stream_path(),
            };
            if let Some(engine) = at.and_then(|p| self.engines.iter_mut().find(|e| e.path == p)) {
                engine.view_notice = Some(message);
            }
        }
    }

    /// Puts the tab of `path` in the dock unless it is there already.
    fn add_stream_tab(&mut self, path: PathBuf) {
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
    }

    /// Opens the Open folder dialog, filled from `text` (a pattern path or a folder).
    pub fn prompt_pattern(&mut self, text: String) {
        self.folder_prompt = Some(crate::ui::folder_dialog::FolderPrompt::from_text(&text));
    }

    /// Default pattern offered for a directory: every `.log` file in it.
    pub fn default_pattern_for(dir: &std::path::Path) -> String {
        dir.join("*.log").to_string_lossy().to_string()
    }

    /// Waker handed to every stream: repaint as soon as its filesystem watcher fires, so
    /// an idle software-rendered window drops the periodic present loop (see `render_ui`).
    /// Shows the notifications the streams queued and acts on a clicked one: the window
    /// comes forward on the stream, at the line when it is still there.
    pub fn run_notifications(&mut self, ctx: &egui::Context) {
        let lang = self.config.language;
        let opts = crate::notifications::Options {
            enabled: self.config.notifications_enabled,
            always: self.config.notify_always,
            focused: !self.hidden_in_tray && ctx.input(|i| i.viewport().focused.unwrap_or(true)),
        };
        let texts = crate::notifications::Texts {
            more: t(lang, "notify_more"),
            summary: t(lang, "notify_summary"),
        };
        self.notifications
            .drain(self.engines.iter_mut(), &opts, &texts, Instant::now());
        if self.notifications.has_held() {
            // A summary is due once the rule is quiet, with or without new lines.
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
        }
        while let Some(target) = self.notifications.clicked() {
            self.show_from_tray(ctx);
            let Some(engine) = self.engines.iter_mut().find(|e| e.path == target.stream) else {
                continue;
            };
            if target.line < engine.line_offsets.len() {
                engine.request_jump(target.line, lang);
            }
            if let Some(tab) = self
                .dock_state
                .find_tab(&FastTailTab::LogStream(engine.path.clone()))
            {
                let _ = self.dock_state.set_active_tab(tab);
            }
        }
    }

    fn make_wake(ctx: &egui::Context) -> crate::tail_engine::WakeFn {
        let ctx = ctx.clone();
        std::sync::Arc::new(move || ctx.request_repaint()) as crate::tail_engine::WakeFn
    }

    /// Pulls the rows a tar scan found into the picker and acts on its end: a scan that
    /// ends with a single openable entry opens it and closes the picker. `ArchivePicker::show`
    /// syncs its own rows every frame; tests call this to drive a scan to its end.
    pub fn poll_archive_picker(&mut self) {
        let Some(picker) = self.archive_picker.as_mut() else {
            return;
        };
        if let Some(outcome) = picker.sync() {
            self.apply_picker_outcome(outcome);
        }
    }

    /// Opens what the picker chose, closing it when it is done.
    fn apply_picker_outcome(&mut self, outcome: crate::ui::zip_picker::PickerOutcome) {
        use crate::ui::zip_picker::PickerOutcome;
        let Some(archive) = self.archive_picker.as_ref().map(|p| p.archive.clone()) else {
            return;
        };
        match outcome {
            PickerOutcome::Open { names, close } => {
                if close {
                    self.archive_picker = None;
                }
                for name in names {
                    self.open_log_file(crate::compressed::entry_path(&archive, &name));
                }
            }
            PickerOutcome::Cancel => self.archive_picker = None,
        }
    }

    /// Text of a failed compressed open, in the UI language.
    fn open_error_text(&self, path: &Path, err: &crate::compressed::OpenError) -> String {
        use crate::compressed::OpenError;
        let lang = self.config.language;
        let reason = match err {
            OpenError::Io(e) => crate::ui::zip_picker::io_error_text(lang, e),
            OpenError::NotEnoughSpace { volume, needed } => t(lang, "compressed_no_space")
                .replace("{volume}", volume)
                .replace("{size}", &crate::ui::zip_picker::human_size(*needed)),
            OpenError::Refused(refusal) => crate::ui::zip_picker::refusal_text(lang, refusal),
            OpenError::NoSuchEntry => t(lang, "compressed_no_such_entry").to_string(),
            OpenError::System { .. } => err.system_text(lang).unwrap_or_default(),
        };
        format!("{}: {reason}", path.display())
    }

    /// Settings rows of the source links: the editor command (with presets) and the
    /// source roots; saved when a field loses the focus or a preset is picked.
    fn source_link_settings(&mut self, ui: &mut egui::Ui) {
        let lang = self.config.language;
        ui.add_space(6.0);
        ui.label(RichText::new(t(lang, "src_settings")).monospace().strong());
        let mut save = false;
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{}:", t(lang, "src_editor"))).monospace());
            let resp = ui
                .add(
                    egui::TextEdit::singleline(&mut self.config.editor_command)
                        .hint_text(t(lang, "src_editor_hint"))
                        .desired_width(260.0),
                )
                .on_hover_text(t(lang, "src_editor_tip"));
            save |= resp.lost_focus();
            egui::ComboBox::from_id_salt("src_editor_presets")
                .selected_text(t(lang, "src_presets"))
                .show_ui(ui, |ui| {
                    for (name, template) in crate::source_links::PRESETS {
                        if ui.selectable_label(false, name).clicked() {
                            self.config.editor_command = template.to_string();
                            save = true;
                        }
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{}:", t(lang, "src_roots"))).monospace());
            let id = egui::Id::new("src_roots_text");
            let mut text = ui.data_mut(|d| {
                d.get_temp::<String>(id)
                    .unwrap_or_else(|| crate::source_links::roots_text(&self.config.source_roots))
            });
            let resp = ui
                .add(egui::TextEdit::singleline(&mut text).desired_width(320.0))
                .on_hover_text(t(lang, "src_roots_tip"));
            if resp.changed() {
                self.config.source_roots = crate::source_links::parse_roots(&text);
            }
            if resp.lost_focus() {
                save = true;
                ui.data_mut(|d| d.remove::<String>(id));
            } else {
                ui.data_mut(|d| d.insert_temp(id, text));
            }
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{}:", t(lang, "rot_setting"))).monospace());
            for (policy, key) in [
                (crate::rotation::Policy::Ask, "rot_ask"),
                (crate::rotation::Policy::Always, "rot_always"),
                (crate::rotation::Policy::Never, "rot_never"),
            ] {
                if ui
                    .radio_value(&mut self.config.rotations, policy, t(lang, key))
                    .on_hover_text(t(lang, "rot_setting_tip"))
                    .changed()
                {
                    save = true;
                }
            }
        });
        if save {
            let _ = self.config.save();
        }
    }

    /// Opens a source location a row asked for: in the editor of Settings, or in FastTail
    /// as a stream at that line; a location whose file is not found says so.
    fn apply_source_requests(&mut self) {
        let Some(idx) = self.engines.iter().position(|e| e.source_request.is_some()) else {
            return;
        };
        let Some((loc, target)) = self.engines[idx].source_request.take() else {
            return;
        };
        let lang = self.config.language;
        let Some(file) = crate::source_links::resolve(&loc, &self.config.source_roots) else {
            self.engines[idx].view_notice =
                Some(t(lang, "src_not_found").replace("{0}", &loc.path));
            return;
        };
        match target {
            crate::source_links::Target::Editor => {
                if let Err(e) = crate::source_links::open_in_editor(
                    &self.config.editor_command,
                    &file,
                    loc.line,
                    loc.col,
                ) {
                    self.engines[idx].view_notice = Some(t(lang, "src_failed").replace("{0}", &e));
                }
            }
            crate::source_links::Target::Stream => {
                self.open_log_file(file.clone());
                if let Some(engine) = self
                    .engines
                    .iter_mut()
                    .find(|e| paths_equal(&e.path, &file))
                {
                    engine.open_at_line(loc.line as usize, lang);
                }
            }
        }
    }

    /// Opens the source plugin `name` with `values`: a new stream, or the stream of the
    /// same identity started again (the secrets, which the identity leaves out, given).
    pub fn open_plugin(&mut self, name: &str, values: std::collections::BTreeMap<String, String>) {
        // The manifests as they are now: one changed since the menu was built is refused.
        self.plugins_view.refresh();
        let catalog = self.plugins_view.catalog().clone();
        let plugin = catalog.find(name);
        let uri = crate::plugins::PluginUri {
            name: name.to_string(),
            values: values.clone(),
        };
        let path = PathBuf::from(uri.to_uri(plugin));
        self.open_log_file(path.clone());
        let config_dir = crate::config::FastTailConfig::config_path()
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let lang = self.config.language;
        if let Some(engine) = self.engines.iter_mut().find(|e| e.path == path) {
            if let Some(s) = engine.plugin.as_mut() {
                if s.status() != crate::plugin_source::Status::Running
                    && s.start(plugin, &self.config.plugins, Some(values), &config_dir)
                        .is_err()
                {
                    engine.view_notice = Some(t(lang, "plug_status_values").to_string());
                }
            }
        }
    }

    /// After a change of the approvals: the streams of a plugin no longer enabled stop
    /// and say why (a running program is ended, a restart does not come).
    fn block_unapproved_plugins(&mut self) {
        use crate::plugin_source::{Block, Status};
        let catalog = self.plugins_view.catalog().clone();
        for s in self.engines.iter_mut().filter_map(|e| e.plugin.as_mut()) {
            if !matches!(s.status(), Status::Running | Status::Restarting { .. }) {
                continue;
            }
            let block = match catalog
                .find(&s.uri.name)
                .map(|p| self.config.plugins.state(p))
            {
                Some(crate::plugins::State::Enabled) => continue,
                Some(crate::plugins::State::Changed) => Block::Changed,
                Some(crate::plugins::State::Disabled) => Block::NotEnabled,
                None => Block::Missing,
            };
            s.block(block);
        }
    }

    /// The stream bar's Start of plugin streams: the values asked when some are missing,
    /// the Plugins settings when the plugin is missing or not approved, else a new run.
    fn apply_plugin_requests(&mut self) {
        let asked: Vec<crate::plugins::PluginUri> = self
            .engines
            .iter_mut()
            .filter_map(|e| e.plugin.as_mut())
            .filter_map(|s| std::mem::take(&mut s.start_requested).then(|| s.uri.clone()))
            .collect();
        if asked.is_empty() {
            return;
        }
        self.plugins_view.reload(&self.config);
        let catalog = self.plugins_view.catalog().clone();
        let config_dir = crate::config::FastTailConfig::config_path()
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        for uri in asked {
            use crate::plugin_source::Block;
            let plugin = catalog.find(&uri.name);
            match crate::plugin_source::block_of(plugin, &self.config.plugins, &uri) {
                Some(Block::NeedsValues) => self.plugins_view.ask(&uri.name, &uri.values),
                Some(_) => self.plugins_view.open = true,
                None => {
                    if let Some(s) = self
                        .engines
                        .iter_mut()
                        .filter_map(|e| e.plugin.as_mut())
                        .find(|s| s.uri == uri)
                    {
                        let _ = s.start(plugin, &self.config.plugins, None, &config_dir);
                    }
                }
            }
        }
    }

    /// Answers the MCP requests waiting and opens the files `open_file` asked for.
    pub fn serve_mcp(&mut self) {
        let mut refs: Vec<&mut TailEngine> = self.engines.iter_mut().collect();
        let opens = self.mcp_view.serve(&mut refs, &self.config);
        let lang = self.config.language;
        for path in opens {
            self.open_log_file(path.clone());
            if let Some(engine) = self
                .engines
                .iter_mut()
                .find(|e| paths_equal(&e.path, &path))
            {
                engine.view_notice = Some(t(lang, "mcp_notice_open").to_string());
            }
        }
    }

    pub fn open_log_file(&mut self, path: PathBuf) {
        if let Some(opened) = self.attach_stream(path, false) {
            self.add_stream_tab(opened);
            self.save_dock_layout();
        }
    }

    /// "Open all": every matching file of `spec` as a tab of a new dock node, and the
    /// files created later as tabs of the same node (see `poll_folder_groups`). A group
    /// already open on the same folder and patterns is brought to the front instead.
    pub fn open_folder_group(&mut self, spec: crate::wildcard::FolderSpec) {
        let path = spec.path();
        if let Some(group) = self
            .folder_groups
            .iter()
            .find(|g| paths_equal(&g.path(), &path))
        {
            let first = self.engines.iter().find(|e| group.is_member(&e.path));
            if let Some(locator) = first.and_then(|e| {
                self.dock_state
                    .find_tab(&FastTailTab::LogStream(e.path.clone()))
            }) {
                let _ = self.dock_state.set_active_tab(locator);
            }
            return;
        }
        let wake = Self::make_wake(&self.egui_ctx);
        let (group, scan) = crate::folder_source::FolderGroup::start(
            spec,
            self.config.folder_max_streams,
            Some(wake),
        );
        let lang = self.config.language;
        if scan.open.is_empty() && scan.skipped == 0 {
            self.open_notice = Some(format!(
                "{}: {}",
                group.spec.dir.display(),
                t(lang, "folder_waiting")
            ));
        }
        self.folder_groups.push(group);
        self.open_group_files(self.folder_groups.len() - 1, scan, true);
        crate::audio::play_sound(
            crate::audio::CyberSound::BlipAttach,
            self.config.sound_enabled,
        );
        self.save_dock_layout();
    }

    /// Reopens a saved folder group: the files that are not open yet join the node of its
    /// open tabs (or a new one).
    pub fn restore_folder_group(&mut self, path: &Path) {
        let Some(spec) = crate::wildcard::FolderSpec::parse(path).filter(|s| s.dir.is_dir()) else {
            return;
        };
        if self
            .folder_groups
            .iter()
            .any(|g| paths_equal(&g.path(), &spec.path()))
        {
            return;
        }
        let wake = Self::make_wake(&self.egui_ctx);
        let (group, scan) = crate::folder_source::FolderGroup::start(
            spec,
            self.config.folder_max_streams,
            Some(wake),
        );
        self.folder_groups.push(group);
        self.open_group_files(self.folder_groups.len() - 1, scan, false);
    }

    /// Opens the files a scan of group `index` found and makes them (and those already
    /// open) the group's streams. New tabs join the node of the group's last tab, or a new
    /// node split to the right; the active tab and the focused node change only with
    /// `focus` (the user's own Open all). Says how many files the cap left out.
    fn open_group_files(
        &mut self,
        index: usize,
        scan: crate::folder_source::GroupScan,
        focus: bool,
    ) {
        let lang = self.config.language;
        let mut notes = Vec::new();
        if scan.skipped > 0 {
            notes.push(
                t(lang, "folder_skipped")
                    .replace("{n}", &scan.skipped.to_string())
                    .replace("{max}", &self.folder_groups[index].max_streams.to_string()),
            );
        }
        if scan.truncated {
            notes.push(
                t(lang, "folder_scan_limited")
                    .replace("{n}", &crate::wildcard::MAX_ENTRIES.to_string()),
            );
        }
        if !notes.is_empty() {
            self.open_notice = Some(format!(
                "{}: {}",
                self.folder_groups[index].spec.dir.display(),
                notes.join(" ")
            ));
        }
        let mut tabs = Vec::new();
        for path in scan.open {
            // Already open (under this or an equal path): it joins the group as it is.
            if let Some(open) = self.engines.iter().find(|e| paths_equal(&e.path, &path)) {
                let open = open.path.clone();
                self.folder_groups[index].adopt(open);
                continue;
            }
            if let Some(opened) = self.attach_stream(path, true) {
                self.folder_groups[index].adopt(opened.clone());
                tabs.push(FastTailTab::LogStream(opened));
            }
        }
        if tabs.is_empty() {
            return;
        }
        let group = &self.folder_groups[index];
        // The node of the group's last tab already in the dock.
        let anchor = self
            .dock_state
            .iter_all_tabs()
            .filter(|(_, t)| {
                matches!(t, FastTailTab::LogStream(p) if group.is_member(p) && !tabs.contains(t))
            })
            .map(|(at, _)| (at.surface, at.node))
            .last();
        let focused = self.dock_state.focused_leaf();
        match anchor {
            Some((surface, node)) => {
                if let Some(leaf) = self.dock_state[surface][node].get_leaf_mut() {
                    let active = leaf.active;
                    leaf.tabs.extend(tabs);
                    if !focus {
                        leaf.active = active;
                    }
                }
            }
            None if self.dock_state.iter_all_tabs().count() == 0 => {
                self.dock_state = DockState::new(tabs);
            }
            None if self.dock_state.main_surface().num_tabs() == 0 => {
                for tab in tabs {
                    self.dock_state.main_surface_mut().push_to_first_leaf(tab);
                }
            }
            None => {
                self.dock_state.main_surface_mut().split_right(
                    egui_dock::NodeIndex::root(),
                    0.5,
                    tabs,
                );
            }
        }
        // A split or a push focuses the new node: give the focus back.
        if let (false, Some(path)) = (focus, focused) {
            self.dock_state.set_focused_node_and_surface(path);
        }
    }

    /// Scans the folder groups that are due and opens their new files. A group whose
    /// tabs were all closed ends.
    pub fn poll_folder_groups(&mut self) {
        let mut changed = false;
        let mut i = 0;
        while i < self.folder_groups.len() {
            let group = &self.folder_groups[i];
            let open_now = self
                .engines
                .iter()
                .filter(|e| group.is_member(&e.path))
                .count();
            let scan = self.folder_groups[i].poll(open_now);
            if !scan.open.is_empty() || scan.skipped > 0 {
                self.open_group_files(i, scan, false);
                changed = true;
            } else if open_now == 0 && self.folder_groups[i].has_members() {
                self.folder_groups.remove(i);
                changed = true;
                continue;
            }
            i += 1;
        }
        if changed {
            self.save_dock_layout();
        }
    }

    /// Opens the stream of `path` without its tab and returns the path of the engine
    /// added, `None` when nothing was added (already open, missing, a picker or a derived
    /// stream, which places its own tab). `quiet` (files of a folder group) plays no
    /// sound, selects nothing, keeps the recent files and does not save.
    fn attach_stream(&mut self, path: PathBuf, quiet: bool) -> Option<PathBuf> {
        // A saved derived stream ("Open filter as new tab") is rebuilt from its source.
        if crate::filter_tab::parse_identity(&path).is_some() {
            if self.restore_filter_tab(&path, true).is_some() {
                self.save_dock_layout();
            }
            return None;
        }
        // A saved merged view is rebuilt from its sources.
        if crate::merged_stream::parse_identity(&path).is_some() {
            if self.restore_merged_view(&path, true).is_some() {
                self.save_dock_layout();
            }
            return None;
        }
        let is_pattern = crate::wildcard::is_pattern_path(&path);
        if !is_pattern
            && !crate::compressed::source_exists(&path)
            && !crate::system_source::is_system_path(&path)
        {
            return None;
        }

        // Avoid duplicate tabs for same path (a plugin URI in its stored form)
        let key = crate::plugins::stored_path(&path);
        for eng in &self.engines {
            if eng.path == key || paths_equal(&eng.path, &key) {
                // Already open, select tab
                let tab = FastTailTab::LogStream(eng.path.clone());
                if let (Some(locator), false) = (self.dock_state.find_tab(&tab), quiet) {
                    let _ = self.dock_state.set_active_tab(locator);
                }
                return None;
            }
        }

        let lang = self.config.language;
        let wake = Self::make_wake(&self.egui_ctx);
        let mut engine = match crate::workspace::open_target(&path, &self.config, Some(wake)) {
            OpenOutcome::Opened(engine) => *engine,
            OpenOutcome::Missing => return None,
            // A plain file that fails to open is skipped silently, as it always was; a
            // compressed one says why.
            OpenOutcome::Failed { error, report } => {
                if report {
                    self.open_notice = Some(self.open_error_text(&path, &error));
                }
                return None;
            }
            OpenOutcome::OpenEntry(entry) => return self.attach_stream(entry, quiet),
            OpenOutcome::EmptyArchive(archive) => {
                self.open_notice = Some(format!("{}: {}", archive.display(), t(lang, "zip_empty")));
                return None;
            }
            // A folder group opens no pickers: an archive of several entries is left for
            // the user to open by hand.
            OpenOutcome::ChooseEntries { .. }
            | OpenOutcome::ScanTar { .. }
            | OpenOutcome::ListFailed { .. }
                if quiet =>
            {
                return None;
            }
            OpenOutcome::ChooseEntries {
                archive,
                entries,
                partial,
            } => {
                let mut picker = crate::ui::zip_picker::ArchivePicker::new(archive, entries);
                picker.partial = partial;
                self.archive_picker = Some(picker);
                return None;
            }
            // A tar opens the entry picker at once while its headers are scanned in the
            // background.
            OpenOutcome::ScanTar { archive, codec } => {
                let scan = crate::compressed::TarScan::start(
                    &archive,
                    codec,
                    crate::compressed::ScanLimits::default(),
                );
                self.archive_picker = Some(crate::ui::zip_picker::ArchivePicker::scanning(
                    archive, scan,
                ));
                return None;
            }
            OpenOutcome::ListFailed { archive, error } => {
                let reason = crate::ui::zip_picker::io_error_text(lang, &error);
                self.open_notice = Some(format!("{}: {reason}", archive.display()));
                return None;
            }
        };
        // A plugin stream is kept, listed and shown under its stored identity, never the
        // path it was opened with (which may hold a secret); a file opened with its
        // rotations (`rotations=always`) under `rotated:<file>`.
        let path = if engine.plugin.is_some() || engine.rotation.is_some() {
            engine.path.clone()
        } else {
            path
        };
        crate::workspace::apply_settings(&mut engine, &self.config);
        engine.set_quick_labels(&self.quick_labels);
        crate::workspace::restore_stream(&mut engine, &self.config, &path);
        // A plain file with rotations next to it: the stream bar offers them.
        if self.config.rotations == crate::rotation::Policy::Ask
            && engine.rotation.is_none()
            && engine.compressed.is_none()
            && engine.piped().is_none()
            && path.is_file()
        {
            engine.rotation_offer = crate::rotation::offer(&path);
        }
        self.engines.push(engine);

        // Add to open_files
        if !self.config.open_files.iter().any(|p| paths_equal(p, &path)) {
            self.config.open_files.push(path.clone());
        }
        if !quiet {
            crate::audio::play_sound(
                crate::audio::CyberSound::BlipAttach,
                self.config.sound_enabled,
            );
            // Keep recent files in MRU order (most recent at top, max 15)
            self.config.add_recent_file(&path);
            let _ = self.config.save();
        }
        Some(path)
    }

    /// Compiles the global filter as it stands and saves it; the streams pick the new set
    /// up before they are drawn (see `render_ui`).
    pub fn apply_global_filter(&mut self) {
        self.global_edit_at = None;
        let key = self.config.global_filter.applied_key();
        if key != self.global_key {
            self.global_key = key;
            self.global_spec = self.config.global_filter.compile();
        }
        let _ = self.config.save();
    }

    /// The flag of the thread that wakes the app every `BACKGROUND_WAKE` while the window
    /// shows nothing (eframe calls `logic` only when a repaint is requested). Started on
    /// first use; the thread lives as long as the process and sleeps otherwise.
    fn background_waker(
        &mut self,
        ctx: &egui::Context,
    ) -> &std::sync::Arc<std::sync::atomic::AtomicBool> {
        self.background_wake.get_or_insert_with(|| {
            let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let (ctx, waiting) = (ctx.clone(), flag.clone());
            let _ = std::thread::Builder::new()
                .name("fasttail-background-wake".into())
                .spawn(move || loop {
                    std::thread::sleep(BACKGROUND_WAKE);
                    if waiting.load(std::sync::atomic::Ordering::Relaxed) {
                        ctx.request_repaint();
                    }
                });
            flag
        })
    }

    /// The work of a frame that draws nothing, for a minimised or hidden window: the
    /// streams are read (rules, sounds and automatic bookmarks run with the poll), the
    /// tools bound to rules run, the tray answers, and a rule with a sound alert asks for
    /// the user's attention when `flash_on_alert` is on. Nothing is on screen, so every
    /// stream counts its new lines as unseen.
    pub fn background_step(&mut self, ctx: &egui::Context) {
        self.update_tray(ctx);
        self.poll_pending_stdin();
        // Merged views follow their sources while the window shows nothing too.
        self.apply_merged_views();
        let size_interval =
            std::time::Duration::from_millis(self.config.size_check_interval_ms as u64);
        self.poll_folder_groups();
        for eng in &mut self.engines {
            eng.displayed = false;
            eng.size_check_interval = size_interval;
            eng.poll_updates();
        }
        self.run_rule_bound_tools();
        self.run_notifications(ctx);
        // The web view and the MCP server answer while the window shows nothing too.
        self.web_view
            .sync(&mut self.config, || Self::make_wake(ctx));
        let refs: Vec<&TailEngine> = self.engines.iter().collect();
        self.web_view.serve(
            &refs,
            &self.config.theme,
            self.config.level_colors,
            self.config.language,
        );
        self.mcp_view
            .sync(&mut self.config, || Self::make_wake(ctx));
        self.serve_mcp();
        if self.mcp_view.busy() {
            ctx.request_repaint();
        }
        let alert = self.engines.iter().any(|e| e.unseen_severity >= 2);
        if self.config.flash_on_alert && alert && !self.attention_requested {
            self.attention_requested = true;
            ctx.send_viewport_cmd(ViewportCommand::RequestUserAttention(
                egui::UserAttentionType::Informational,
            ));
        }
    }

    pub fn render_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        // Every tab dragged out into floating windows can leave the main surface without
        // a tree; the next open of a file would panic on it.
        crate::ui::find_results::ensure_main_surface(&mut self.dock_state);
        self.update_tray(&ctx);

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

        // 0b. Command palette (CTRL + SHIFT + P): drawn before anything else reads the
        // input, so that while it is open the keyboard is its own; what it picks runs
        // now, a stream action when that stream is drawn below.
        self.render_palette(&ctx);

        // 0c. Remapped shortcuts (`[shortcuts]`): a key given to an action runs it as the
        // palette does; a default key whose action moved away fires nothing. Before every
        // handler, so the handlers never see those keys.
        self.refresh_keymap();
        // 0d. The web view: the server follows the settings, its requests are answered
        // from the streams as they are now.
        self.web_view
            .sync(&mut self.config, || Self::make_wake(&ctx));
        let refs: Vec<&TailEngine> = self.engines.iter().collect();
        self.web_view.serve(
            &refs,
            &self.config.theme,
            self.config.level_colors,
            self.config.language,
        );
        // 0e. The MCP server: as the web view, a long search in steps over the frames;
        // the files `open_file` asked for open now, marked as opened by the assistant.
        self.mcp_view
            .sync(&mut self.config, || Self::make_wake(&ctx));
        self.serve_mcp();
        if self.mcp_view.busy() {
            ctx.request_repaint();
        }
        self.shortcuts_dialog.capture(&ctx);
        if !self.locked && !self.shortcuts_dialog.recording() {
            self.take_clipboard_paste(&ctx);
        }
        if !self.locked && !self.palette.is_open() && !self.shortcuts_dialog.recording() {
            for id in crate::ui::keys::take_remapped(&ctx, &self.keymap) {
                let target = self.focused_stream_path();
                self.run_action(&ctx, crate::actions::get(id), target);
            }
        }

        // 1. Detect user activity to reset screensaver & handle window closing and viewport bounds
        let mut escape_pressed = false;
        let mut wheel_zoom = 1.0_f32;
        let mut close_requested = false;
        let mut os_minimized = false;
        ctx.input(|i| {
            if i.viewport().close_requested() {
                self.save_dock_layout();
                close_requested = true;
            }
            os_minimized = i.viewport().minimized == Some(true);

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

            // Keyboard shortcut: Escape closes the topmost dialog (below).
            if i.key_pressed(Key::Escape) {
                escape_pressed = true;
            }

            // Drag & drop file support (single or multiple)
            if !i.raw.dropped_files.is_empty() {
                for file in &i.raw.dropped_files {
                    let path = file.path().to_path_buf();
                    if path.is_dir() {
                        // A folder: ask which files to follow, and how.
                        self.folder_prompt =
                            Some(crate::ui::folder_dialog::FolderPrompt::for_folder(&path));
                    } else {
                        self.open_log_file(path);
                    }
                }
            }
        });

        // Close to tray: the close is cancelled and the window hidden (the workspace was
        // saved above, as on a real close). CTRL + Q and the tray's Quit really quit.
        if close_requested && self.tray.is_some() && self.config.close_to_tray && !self.quitting {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.hide_to_tray(&ctx);
        }
        // Minimise to tray through the window's own button: best effort, on the frame
        // that reports the window minimised.
        if os_minimized
            && self.tray.is_some()
            && self.config.minimize_to_tray
            && !self.hidden_in_tray
        {
            ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
            self.hide_to_tray(&ctx);
        }
        let ctrl_q = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Q);
        if ctx.input_mut(|i| i.consume_shortcut(&ctrl_q)) {
            self.quit(&ctx);
        }

        if escape_pressed {
            ctx.memory_mut(|m| m.stop_text_input());
            self.close_topmost_dialog(&ctx);
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
        self.poll_pending_stdin();
        let size_interval =
            std::time::Duration::from_millis(self.config.size_check_interval_ms as u64);
        self.poll_folder_groups();
        for eng in &mut self.engines {
            eng.size_check_interval = size_interval;
            eng.poll_updates();
        }
        self.run_rule_bound_tools();
        self.run_notifications(&ctx);
        // The search across streams drains its jobs and notices closed or reloaded streams.
        // Under a time scope the streams it waits for are timed first.
        let to_time: Vec<PathBuf> = self
            .find_all
            .waiting_for_timing(&self.engines)
            .map(Path::to_path_buf)
            .collect();
        for eng in self
            .engines
            .iter_mut()
            .filter(|e| to_time.contains(&e.path))
        {
            eng.request_timing();
        }
        self.find_all.poll(&self.engines);
        if self.find_all.is_active() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }

        // 3. Periodic telemetry refresh
        if self.last_sys_refresh.elapsed().as_secs_f32() >= 1.0 {
            if let Some(system) = self.system.as_mut() {
                system.refresh_cpu_usage();
                system.refresh_memory();
                self.cpu_usage = system.global_cpu_usage();
                self.mem_used_mb = system.used_memory() / (1024 * 1024);
            }
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
                    .fill(self.config.theme.bg_color().into())
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
                        self.config.theme.accent_color().into(),
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
                                // `exit` skips `on_exit`: stop the decompression jobs
                                // and delete this process's spools here.
                                self.engines.clear();
                                crate::spool::remove_own(
                                    &self.config.compressed_settings().spool_dir,
                                );
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
                            let minimize = ui
                                .button(RichText::new(" — ").monospace())
                                .on_hover_text(t(self.config.language, "minimize_tip"))
                                .clicked();
                            // With minimise to tray, the window goes to the tray instead.
                            if minimize && self.tray.is_some() && self.config.minimize_to_tray {
                                self.hide_to_tray(&ctx);
                            } else if minimize {
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

                        // Command palette (CTRL + SHIFT + P); lit while it is open.
                        let palette_color = if self.palette.is_open() {
                            self.config.theme.accent_color()
                        } else {
                            self.config.theme.text_dim()
                        };
                        if ui
                            .button(RichText::new(" ⌨ ").monospace().color(palette_color))
                            .on_hover_text(format!(
                                "{}  ({})",
                                t(self.config.language, "palette_title"),
                                crate::ui::palette::PALETTE_SHORTCUT_LABEL
                            ))
                            .clicked()
                        {
                            let target = self.focused_stream_path();
                            self.palette.toggle(&ctx, target);
                        }

                        // Web view: while it is on, the browsers connected (click: its
                        // settings).
                        if let Some(n) = self.web_view.connections() {
                            let color = if n > 0 {
                                self.config.theme.accent_color()
                            } else {
                                self.config.theme.text_dim()
                            };
                            if ui
                                .button(RichText::new(format!(" 🔗 {n} ")).monospace().color(color))
                                .on_hover_text(
                                    t(self.config.language, "web_indicator_tip")
                                        .replace("{0}", &n.to_string()),
                                )
                                .clicked()
                            {
                                self.web_view.open = true;
                            }
                        }

                        // MCP server: while it is on, the requests of the last minute
                        // (click: its settings).
                        if let Some(n) = self.mcp_view.requests_last_minute() {
                            let color = if n > 0 {
                                self.config.theme.accent_color()
                            } else {
                                self.config.theme.text_dim()
                            };
                            if ui
                                .button(
                                    RichText::new(format!(" MCP {n} ")).monospace().color(color),
                                )
                                .on_hover_text(
                                    t(self.config.language, "mcp_indicator_tip")
                                        .replace("{0}", &n.to_string()),
                                )
                                .clicked()
                            {
                                self.mcp_view.open = true;
                            }
                        }

                        // Global filter bar (CTRL + SHIFT + H); lit while the filter applies.
                        let globe_color = if self.config.global_filter.is_applied() {
                            self.config.theme.accent_color()
                        } else {
                            self.config.theme.text_dim()
                        };
                        if ui
                            .button(RichText::new(" 🌐 ").monospace().color(globe_color))
                            .on_hover_text(t(self.config.language, "global_filter_tip"))
                            .clicked()
                        {
                            self.config.global_filter.bar_open =
                                !self.config.global_filter.bar_open;
                            let _ = self.config.save();
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
                            let total_mem_gb =
                                (self.system.as_ref().map_or(0, System::total_memory) as f32
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
                    .fill(self.config.theme.panel_bg().into())
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
                        self.pick_and_open_files();
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
                        self.open_pattern_prompt();
                    }

                    // Open system source: a container or a systemd unit as a stream.
                    let system_btn =
                        egui::Button::new(RichText::new("📦").monospace().strong().color(text_pri))
                            .fill(self.config.theme.button_bg())
                            .stroke(Stroke::new(1.2, accent))
                            .corner_radius(CornerRadius::same(6))
                            .min_size(egui::vec2(30.0, 26.0));
                    if ui
                        .add(system_btn)
                        .on_hover_text(t(self.config.language, "system_open_tip"))
                        .clicked()
                    {
                        self.system_dialog = Some(crate::ui::system_dialog::SystemDialog::new());
                    }

                    // Remote file over SSH: one stream per host.
                    let remote_btn =
                        egui::Button::new(RichText::new("🌐").monospace().strong().color(text_pri))
                            .fill(self.config.theme.button_bg())
                            .stroke(Stroke::new(1.2, accent))
                            .corner_radius(CornerRadius::same(6))
                            .min_size(egui::vec2(30.0, 26.0));
                    if ui
                        .add(remote_btn)
                        .on_hover_text(t(self.config.language, "remote_open_tip"))
                        .clicked()
                    {
                        self.remote_dialog = Some(Default::default());
                    }

                    // Plugins: the enabled source plugins, and their settings.
                    let plugin_btn =
                        egui::Button::new(RichText::new("🔌").monospace().strong().color(text_pri))
                            .fill(self.config.theme.button_bg())
                            .stroke(Stroke::new(1.2, accent))
                            .corner_radius(CornerRadius::same(6))
                            .min_size(egui::vec2(30.0, 26.0));
                    let mut plugin_to_open = None;
                    let theme_now = self.config.theme;
                    let plugin_menu =
                        egui::menu::MenuButton::from_button(plugin_btn).ui(ui, |ui| {
                            plugin_to_open = self.plugins_view.menu(ui, &self.config, &theme_now);
                        });
                    let _ = plugin_menu
                        .0
                        .on_hover_text(t(self.config.language, "plug_menu_tip"));
                    if let Some((name, values)) = plugin_to_open {
                        self.open_plugin(&name, values);
                    }

                    // Listen: lines received on a local port.
                    let listen_btn =
                        egui::Button::new(RichText::new("📡").monospace().strong().color(text_pri))
                            .fill(self.config.theme.button_bg())
                            .stroke(Stroke::new(1.2, accent))
                            .corner_radius(CornerRadius::same(6))
                            .min_size(egui::vec2(30.0, 26.0));
                    if ui
                        .add(listen_btn)
                        .on_hover_text(t(self.config.language, "listen_open_tip"))
                        .clicked()
                    {
                        self.listen_dialog = Some(Default::default());
                    }

                    // Debug output (Windows): OutputDebugString messages as a stream.
                    if cfg!(windows) {
                        let debug_btn = egui::Button::new(
                            RichText::new("🪲").monospace().strong().color(text_pri),
                        )
                        .fill(self.config.theme.button_bg())
                        .stroke(Stroke::new(1.2, accent))
                        .corner_radius(CornerRadius::same(6))
                        .min_size(egui::vec2(30.0, 26.0));
                        if ui
                            .add(debug_btn)
                            .on_hover_text(t(self.config.language, "dbwin_open_tip"))
                            .clicked()
                        {
                            self.debug_dialog = Some(Default::default());
                        }
                        // Event Log channels as streams.
                        let eventlog_btn = egui::Button::new(
                            RichText::new("🗒").monospace().strong().color(text_pri),
                        )
                        .fill(self.config.theme.button_bg())
                        .stroke(Stroke::new(1.2, accent))
                        .corner_radius(CornerRadius::same(6))
                        .min_size(egui::vec2(30.0, 26.0));
                        if ui
                            .add(eventlog_btn)
                            .on_hover_text(t(self.config.language, "eventlog_open_tip"))
                            .clicked()
                        {
                            self.eventlog_dialog =
                                Some(crate::ui::eventlog_dialog::EventLogDialog::with_channels());
                        }
                    }

                    // Sessions menu (🗂): save as, save, load, recent, save as default
                    let session_btn =
                        egui::Button::new(RichText::new("🗂").monospace().strong().color(text_pri))
                            .fill(self.config.theme.button_bg())
                            .stroke(Stroke::new(1.2, accent))
                            .corner_radius(CornerRadius::same(6))
                            .min_size(egui::vec2(30.0, 26.0));
                    let mut session_action: Option<SessionAction> = None;
                    let mut report_all = false;
                    let mut open_pad = false;
                    let mut paste_stream = false;
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
                                .on_disabled_hover_text(t(lang, "session_no_named_session"))
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
                            // The session's scratchpad.
                            ui.separator();
                            if ui
                                .button(format!("🗒 {}", t(lang, "scratch_open")))
                                .clicked()
                            {
                                open_pad = true;
                                ui.close();
                            }
                            if ui
                                .button(crate::ui::keys::with_key(
                                    &format!("📋 {}", t(lang, "clip_paste")),
                                    crate::actions::ActionId::PasteAsStream,
                                ))
                                .on_hover_text(t(lang, "clip_paste_tip"))
                                .clicked()
                            {
                                paste_stream = true;
                                ui.close();
                            }
                            // Every open stream's bookmarks as one Markdown report.
                            if ui
                                .add_enabled(
                                    !self.engines.is_empty(),
                                    egui::Button::new(format!(
                                        "📝 {}",
                                        t(lang, "bookmark_report_all")
                                    )),
                                )
                                .on_disabled_hover_text(t(lang, "status_no_file"))
                                .clicked()
                            {
                                report_all = true;
                                ui.close();
                            }
                        });
                    let _ = session_menu.0.on_hover_text(t(lang, "session_tip"));
                    if let Some(action) = session_action {
                        self.run_session_action(action);
                    }
                    if report_all {
                        self.open_bookmark_report(crate::ui::report_dialog::ReportScope::All);
                    }
                    if open_pad {
                        self.open_scratchpad();
                    }
                    if paste_stream {
                        let ctx = ui.ctx().clone();
                        self.request_paste(&ctx, PasteTarget::New);
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
                        RichText::new(format!("▶ {}", t(self.config.language, "toolbar_play")))
                            .monospace()
                            .strong()
                            .color(play_color),
                    )
                    .fill(play_bg)
                    .stroke(Stroke::new(1.2, play_color))
                    .corner_radius(CornerRadius::same(6))
                    .min_size(egui::vec2(0.0, 26.0));

                    if ui
                        .add_enabled(!self.engines.is_empty(), play_btn)
                        .on_hover_text(t(self.config.language, "play_tip"))
                        .on_disabled_hover_text(t(self.config.language, "status_no_file"))
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
                        RichText::new(format!("⏸ {}", t(self.config.language, "toolbar_pause")))
                            .monospace()
                            .strong()
                            .color(pause_color),
                    )
                    .fill(pause_bg)
                    .stroke(Stroke::new(1.2, pause_color))
                    .corner_radius(CornerRadius::same(6))
                    .min_size(egui::vec2(0.0, 26.0));

                    if ui
                        .add_enabled(!self.engines.is_empty(), pause_btn)
                        .on_hover_text(t(self.config.language, "pause_tip"))
                        .on_disabled_hover_text(t(self.config.language, "status_no_file"))
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
                        let help_btn = egui::Button::new(
                            RichText::new(format!(
                                "❓ {}",
                                t(self.config.language, "toolbar_help")
                            ))
                            .monospace()
                            .color(text_dim),
                        )
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
                        let about_btn = egui::Button::new(
                            RichText::new(format!(
                                "ℹ {}",
                                t(self.config.language, "toolbar_about")
                            ))
                            .monospace()
                            .color(text_dim),
                        )
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
                        .fill(warn.gamma_multiply(0.18).into())
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
        // Automatic highlighting follows the settings (Settings page or palette toggle).
        let auto_tokens = self.config.auto_tokens();
        for eng in &mut self.engines {
            eng.set_auto_tokens(auto_tokens);
        }
        let prev_borderless = self.config.borderless;
        let prev_theme = self.config.theme;
        let prev_lang = self.config.language;
        let mut tab_closed = false;
        let mut labels_changed = false;
        let mut query_ran: Option<String> = None;
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
                    .fill(current_theme.panel_bg().into())
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

                    let stdin_spool = self
                        .engines
                        .iter()
                        .find_map(|e| e.stdin.as_ref())
                        .map(|s| s.spool_path().display().to_string());
                    let clipboard_spool = active_path.as_ref().and_then(|p| {
                        self.engines
                            .iter()
                            .find(|e| e.path == *p)
                            .and_then(|e| e.clipboard.as_ref())
                            .map(|c| c.copier.spool_path().display().to_string())
                    });
                    let path_str = if let Some(path) = active_path {
                        if crate::stdin_source::is_stdin_path(&path) {
                            t(self.config.language, "stdin_footer")
                                .replace("{path}", stdin_spool.as_deref().unwrap_or("?"))
                        } else if let Some(spool) = &clipboard_spool {
                            t(self.config.language, "clip_footer").replace("{path}", spool)
                        } else if path.is_absolute() {
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
        dock_style.tab_bar.bg_fill = current_theme.bg_color().into();
        dock_style.tab_bar.hline_color = current_theme.accent_color().gamma_multiply(0.35).into();
        dock_style.tab_bar.height = 26.0;

        let active_tab_bg = current_theme.tab_active_bg();
        let inactive_tab_bg = current_theme.tab_inactive_bg();

        dock_style.tab.active.bg_fill = active_tab_bg.into();
        dock_style.tab.active.outline_color = current_theme.accent_color().into();
        dock_style.tab.active.corner_radius = CornerRadius {
            nw: 6,
            ne: 6,
            sw: 0,
            se: 0,
        };
        dock_style.tab.active.text_color = current_theme.accent_color().into();

        dock_style.tab.focused = dock_style.tab.active.clone();

        dock_style.tab.inactive.bg_fill = inactive_tab_bg.into();
        dock_style.tab.inactive.outline_color =
            current_theme.border_color().gamma_multiply(0.2).into();
        dock_style.tab.inactive.corner_radius = CornerRadius {
            nw: 6,
            ne: 6,
            sw: 0,
            se: 0,
        };
        dock_style.tab.inactive.text_color = current_theme.text_dim().into();

        dock_style.tab.tab_body.stroke =
            Stroke::new(1.5, current_theme.accent_color().gamma_multiply(0.7));
        dock_style.tab.tab_body.corner_radius = CornerRadius::same(6);
        dock_style.tab.tab_body.bg_fill = current_theme.panel_bg().into();

        dock_style.separator.width = 3.0;
        dock_style.separator.color_idle = current_theme.accent_color().gamma_multiply(0.25).into();
        dock_style.separator.color_hovered = current_theme.accent_color().into();
        dock_style.separator.color_dragged = current_theme.accent_color().into();

        dock_style.buttons.close_tab_color = current_theme.text_dim().into();
        dock_style.buttons.close_tab_active_color = current_theme.warn_color().into();

        let mut test_screensaver = false;
        // The stream in the focused dock leaf is the "current window": it alone receives
        // F3 / Shift+F3, Ctrl+F and the keyboard navigation shortcuts.
        let focused_stream = self.focused_stream_path();
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
                    // A tool on a built-in key does not fire: the built-in wins.
                    if self.keymap.tool_conflicts.contains(&n) {
                        return None;
                    }
                    let sc = tool.parsed_shortcut()?;
                    let (mods, key) = crate::ui::keys::egui_shortcut(sc);
                    i.consume_key(mods, key).then_some(n)
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

        // Global filter: CTRL + SHIFT + H shows or hides its bar (consumed before the dock,
        // like CTRL + SHIFT + F); an edit of the terms reaches the streams once typing
        // pauses, a switch or toggle at once; every stream, new ones included, gets the
        // same compiled set before it is drawn.
        if crate::ui::global_filter_bar::consume_shortcut(&ctx) {
            self.config.global_filter.bar_open = !self.config.global_filter.bar_open;
            let _ = self.config.save();
        }
        if self.config.global_filter.bar_open {
            let out = crate::ui::global_filter_bar::render(
                ui,
                &mut self.config.global_filter,
                &self.config.theme,
                self.config.language,
            );
            if out.terms_edited {
                self.global_edit_at = Some(Instant::now());
            }
            if out.switched || out.closed {
                self.apply_global_filter();
            }
        }
        if let Some(at) = self.global_edit_at {
            let delay = std::time::Duration::from_millis(crate::global_filter::APPLY_DELAY_MS);
            match delay.checked_sub(at.elapsed()) {
                Some(wait) if !wait.is_zero() => ctx.request_repaint_after(wait),
                _ => self.apply_global_filter(),
            }
        }
        // At most `SYNC_BUDGET_BYTES` refiltered on the UI thread per frame: with many
        // streams the rest follow on the next frames (the first always goes).
        let mut budget = crate::global_filter::SYNC_BUDGET_BYTES;
        for engine in &mut self.engines {
            if engine.holds_global_filter(&self.global_spec) {
                continue;
            }
            let synchronous = engine.file_size <= engine.job_threshold_bytes;
            if synchronous && budget == 0 {
                ctx.request_repaint();
                continue;
            }
            engine.set_global_filter(self.global_spec.clone());
            if synchronous {
                budget = budget.saturating_sub(engine.file_size.max(1));
            }
        }

        let mut lock_now = false;
        let mut preset_events = crate::ui::dock::PresetEvents::default();
        let mut search_view = crate::ui::dock::SearchViewPrefs {
            search_pane: self.config.search_pane,
            search_pane_height: self.config.search_pane_height,
            overview_strip: self.config.overview_strip,
            timeline_search_lane: self.config.timeline_search_lane,
        };
        let search_view_before = search_view;
        let mut time_delta = crate::ui::dock::TimeDeltaPrefs {
            show: self.config.show_time_delta,
            gap_ms: self.config.time_delta_gap_ms,
        };
        let time_delta_before = time_delta;
        let line_numbers_before = self.config.show_line_numbers;
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
            auto_highlight: &mut self.config.auto_highlight,
            auto_highlight_kinds: &mut self.config.auto_highlight_kinds,
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
            scratchpad: &mut self.scratchpad,
            compare: &mut self.compare,
            query_history: &self.config.query_history,
            query_ran: &mut query_ran,
            filter_presets: &mut self.config.filter_presets,
            preset_events: &mut preset_events,
            palette_action: self.palette_action.as_ref().map(|p| (p.path.clone(), p.id)),
            markdown_caches: &mut self.markdown_caches,
        };

        let mut palette_taken = false;
        if self.dock_state.iter_all_tabs().count() == 0 {
            egui::Frame::new()
                .fill(self.config.theme.panel_bg().into())
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
                                self.pick_and_open_files();
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
            palette_taken = tab_viewer.ctx.palette_action.is_none();
        }
        // A palette action its stream did not take (the tab was not drawn this frame)
        // waits a few frames, then is dropped with its stream gone or still hidden.
        self.palette_action = self.palette_action.take().and_then(|pending| {
            let open = self
                .engines
                .iter()
                .any(|e| paths_equal(&e.path, &pending.path));
            pending.after_frame(palette_taken, open)
        });
        if self.palette_action.is_some() {
            ctx.request_repaint();
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
        if self.apply_scratchpad_requests() {
            ctx.request_repaint();
        }
        if self.apply_filter_tab_requests() {
            ctx.request_repaint();
        }
        if self.apply_stats_requests() {
            ctx.request_repaint();
        }
        if self.apply_ops_requests() {
            ctx.request_repaint();
        }
        if self.apply_query_requests() {
            ctx.request_repaint();
        }
        if self.apply_patterns_requests() {
            ctx.request_repaint();
        }
        if self.apply_explain_requests() {
            ctx.request_repaint();
        }
        if self.apply_merged_views() {
            ctx.request_repaint();
        }
        if self.apply_compare_requests() {
            ctx.request_repaint();
        }
        let mark = self.compare_mark.as_ref().map(|m| m.label());
        ctx.data_mut(|d| match mark {
            Some(label) => {
                d.insert_temp(crate::ui::compare_tab::mark_id(), label);
            }
            None => d.remove::<String>(crate::ui::compare_tab::mark_id()),
        });
        self.scratchpad.save_if_due();
        if self.scratchpad.is_dirty() {
            ctx.request_repaint_after(crate::ui::scratchpad::SAVE_DELAY);
        }
        if let Some(eng) = self.engines.iter_mut().find(|e| e.report_request) {
            eng.report_request = false;
            let scope = crate::ui::report_dialog::ReportScope::Stream(eng.path.clone());
            self.open_bookmark_report(scope);
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
        // A column width being dragged is saved once, when the button is released.
        let pointer_down = ctx.input(|i| i.pointer.any_down());
        let mut critical_in_background = false;
        for eng in &mut self.engines {
            if crate::workspace::save_changes(eng, &mut self.config, pointer_down) {
                bookmarks_changed = true;
            }
            // The engine only queues its sounds (search wrap-around, rule alerts).
            for sound in eng.take_sounds() {
                sound.play();
            }
            if !eng.displayed && eng.unseen_severity >= 2 {
                critical_in_background = true;
            }
        }
        if bookmarks_changed {
            let _ = self.config.save();
        }
        // Results pane, overview strip and timeline preferences: a toggle is saved at
        // once, the pane height once the drag of its edge is over.
        if search_view != search_view_before {
            self.config.search_pane = search_view.search_pane;
            self.config.search_pane_height = search_view.search_pane_height;
            self.config.overview_strip = search_view.overview_strip;
            self.config.timeline_search_lane = search_view.timeline_search_lane;
            let toggled = search_view.search_pane != search_view_before.search_pane
                || search_view.overview_strip != search_view_before.overview_strip
                || search_view.timeline_search_lane != search_view_before.timeline_search_lane;
            if toggled || !ctx.input(|i| i.pointer.any_down()) {
                let _ = self.config.save();
            }
        }
        // Defaults for new streams and the gap threshold (Settings tab): saved as soon as
        // they change.
        if time_delta != time_delta_before || self.config.show_line_numbers != line_numbers_before {
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

        // A closed stream's Markdown cache goes with it (a few entries: cheap per frame).
        if !self.markdown_caches.is_empty() {
            let engines = &self.engines;
            self.markdown_caches
                .retain(|path, _| engines.iter().any(|e| &e.path == path));
        }

        if let Some(q) = query_ran {
            self.config.push_query_history(&q);
            let _ = self.config.save();
        }
        if labels_changed {
            for eng in &mut self.engines {
                eng.set_quick_labels(&self.quick_labels);
            }
            ctx.request_repaint();
        }

        if let Some(path) = preset_events.open_filters.take() {
            self.filters_focus = Some(path);
            self.config.filters_open = true;
            preset_events.changed = true;
        }
        if preset_events.changed {
            let _ = self.config.save();
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
                        .fill(theme.bg_color().into())
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

        // 8b. Keyboard shortcuts window: a change rebuilds the key map next frame.
        let theme_now = self.config.theme;
        if self
            .shortcuts_dialog
            .show(&ctx, &mut self.config, &self.keymap, &theme_now)
        {
            let _ = self.config.save();
        }
        if self
            .web_view
            .show_settings(&ctx, &mut self.config, &theme_now)
        {
            let _ = self.config.save();
        }
        if self
            .mcp_view
            .show_settings(&ctx, &mut self.config, &theme_now)
        {
            let _ = self.config.save();
        }
        if self
            .plugins_view
            .show_settings(&ctx, &mut self.config, &theme_now)
        {
            let _ = self.config.save();
            self.block_unapproved_plugins();
        }
        if let Some((name, values)) = self
            .plugins_view
            .show_dialog(&ctx, &self.config, &theme_now)
        {
            self.open_plugin(&name, values);
        }

        // 9. Render Settings Dialog if open (Popup modal)
        if self.config.settings_open {
            let mut is_open = true;
            let line_numbers_before = self.config.show_line_numbers;
            let theme = self.config.theme;
            let prev_borderless = self.config.borderless;
            let prev_theme = self.config.theme;
            let prev_lang = self.config.language;
            let mut test_screensaver = false;
            let mut popup_lock_now = false;
            let mut open_formats = false;
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
                    .fill(theme.panel_bg().into())
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
                            &mut self.config.auto_highlight,
                            &mut self.config.auto_highlight_kinds,
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

                        // Log formats: their own dialog.
                        ui.add_space(6.0);
                        if ui
                            .button(format!("◇ {}", t(self.config.language, "fmt_open")))
                            .clicked()
                        {
                            open_formats = true;
                        }
                        // Keyboard shortcuts and the web view: their own windows.
                        if ui
                            .button(format!("⌨ {}", t(self.config.language, "keys_open")))
                            .clicked()
                        {
                            self.shortcuts_dialog.open = true;
                        }
                        if ui
                            .button(format!("🌐 {}", t(self.config.language, "web_open")))
                            .clicked()
                        {
                            self.web_view.open = true;
                        }
                        if ui
                            .button(format!("✨ {}", t(self.config.language, "mcp_open")))
                            .clicked()
                        {
                            self.mcp_view.open = true;
                        }
                        if ui
                            .button(format!("🔌 {}", t(self.config.language, "plug_open")))
                            .clicked()
                        {
                            self.plugins_view.open = true;
                        }

                        // Source links: the editor a location opens in, and the folders a
                        // relative one is looked for in.
                        self.source_link_settings(ui);

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

                        // Interface at start: saved at once; picking the terminal offers to
                        // switch now (Windows) or says when it applies (Linux, macOS).
                        ui.add_space(6.0);
                        let before = self.config.interface;
                        ui.horizontal(|ui| {
                            use crate::config::Interface;
                            ui.label(
                                RichText::new(format!("{}:", t(lang, "interface_label")))
                                    .monospace(),
                            );
                            ui.selectable_value(
                                &mut self.config.interface,
                                Interface::Gui,
                                t(lang, "interface_gui"),
                            );
                            ui.selectable_value(
                                &mut self.config.interface,
                                Interface::Tui,
                                t(lang, "interface_tui"),
                            );
                        });
                        if cfg!(windows)
                            && !crate::handoff::sibling_present(&crate::handoff::tui_exe_name())
                        {
                            ui.label(
                                RichText::new(format!(
                                    "\u{26a0} {}",
                                    t(lang, "interface_tui_missing")
                                ))
                                .small()
                                .color(theme.warn_color()),
                            );
                        }
                        if self.config.interface != before {
                            let _ = self.config.save();
                            self.interface_switch =
                                self.config.interface == crate::config::Interface::Tui;
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
                        // Desktop notifications of the rules with "Notify".
                        let notify_off = crate::notifications::unavailable();
                        ui.add_enabled_ui(notify_off.is_none(), |ui| {
                            let mut changed = ui
                                .checkbox(
                                    &mut self.config.notifications_enabled,
                                    t(lang, "notifications_enabled"),
                                )
                                .on_hover_text(t(lang, "notifications_enabled_tip"))
                                .on_disabled_hover_text(t(lang, notify_off.unwrap_or_default()))
                                .changed();
                            ui.add_enabled_ui(self.config.notifications_enabled, |ui| {
                                ui.horizontal(|ui| {
                                    changed |= ui
                                        .checkbox(
                                            &mut self.config.notify_always,
                                            t(lang, "notify_always"),
                                        )
                                        .on_hover_text(t(lang, "notify_always_tip"))
                                        .changed();
                                    if ui.button(t(lang, "notify_test")).clicked() {
                                        self.notifications
                                            .test("FastTail", t(lang, "notify_test_body"));
                                    }
                                });
                            });
                            if changed {
                                let _ = self.config.save();
                            }
                        });
                        // Notification-area icon (Windows, and Linux with a StatusNotifierItem host).
                        let tray_ok = crate::tray::Tray::available();
                        ui.add_enabled_ui(tray_ok, |ui| {
                            let mut changed = ui
                                .checkbox(&mut self.config.tray_icon, t(lang, "tray_icon"))
                                .on_hover_text(t(lang, "tray_icon_tip"))
                                .on_disabled_hover_text(t(lang, "tray_unavailable"))
                                .changed();
                            ui.add_enabled_ui(self.config.tray_icon, |ui| {
                                changed |= ui
                                    .checkbox(
                                        &mut self.config.minimize_to_tray,
                                        t(lang, "minimize_to_tray"),
                                    )
                                    .changed();
                                changed |= ui
                                    .checkbox(
                                        &mut self.config.close_to_tray,
                                        t(lang, "close_to_tray"),
                                    )
                                    .on_hover_text(t(lang, "close_to_tray_tip"))
                                    .changed();
                            });
                            if changed {
                                let _ = self.config.save();
                            }
                        });

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
                                        .range(crate::settings_model::POLL_INTERVAL_MS)
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
                                        .range(crate::settings_model::SIZE_CHECK_INTERVAL_MS)
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
                                        .range(crate::settings_model::MAX_FPS)
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
                                        .range(crate::settings_model::MAX_FPS_SOFTWARE)
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
                                        .range(crate::settings_model::MOUSE_THROTTLE_MS)
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
                                        .range(crate::settings_model::MARKDOWN_MAX_MB)
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

                        // Where the level is looked for: the default of every stream
                        // without its own value (the stream's level menu sets that).
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("{}:", t(lang, "level_detect_bytes")))
                                    .monospace(),
                            );
                            let r = ui
                                .add(
                                    egui::DragValue::new(&mut self.config.level_detect_bytes)
                                        .range(crate::settings_model::LEVEL_DETECT_BYTES)
                                        .suffix(" B")
                                        // Typed text applies on Enter, not per key.
                                        .update_while_editing(false),
                                )
                                .on_hover_text(t(lang, "level_detect_bytes_tip"));
                            // Applied once the drag is over: each value detects every line again.
                            if r.drag_stopped() || (r.changed() && !r.dragged()) {
                                for engine in &mut self.engines {
                                    engine.set_level_default(self.config.level_detect_bytes);
                                }
                                let _ = self.config.save();
                            }
                        });

                        // The time column pinned in the column view of streams whose
                        // pins were not chosen.
                        if ui
                            .checkbox(&mut self.config.pin_time_column, t(lang, "pin_time_column"))
                            .on_hover_text(t(lang, "pin_time_column_tip"))
                            .changed()
                        {
                            for engine in &mut self.engines {
                                engine.pin_time_default = self.config.pin_time_column;
                            }
                            let _ = self.config.save();
                        }

                        // Copy as excerpt: the flavour of the block.
                        ui.horizontal(|ui| {
                            ui.label(t(lang, "excerpt_format"))
                                .on_hover_text(t(lang, "excerpt_format_tip"));
                            let mut changed = false;
                            for f in crate::export::ExcerptFlavour::ALL {
                                let name = match f {
                                    crate::export::ExcerptFlavour::Markdown => "Markdown",
                                    crate::export::ExcerptFlavour::Jira => "Jira",
                                    crate::export::ExcerptFlavour::Slack => "Slack",
                                };
                                changed |= ui
                                    .radio_value(&mut self.config.excerpt_format, f, name)
                                    .changed();
                            }
                            if changed {
                                for engine in &mut self.engines {
                                    engine.excerpt_flavour = self.config.excerpt_format;
                                }
                                let _ = self.config.save();
                            }
                        });

                        // How far back a timestamp may go before its line is marked.
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("{}:", t(lang, "ooo_tolerance"))).monospace(),
                            );
                            let r = ui
                                .add(
                                    egui::DragValue::new(
                                        &mut self.config.out_of_order_tolerance_ms,
                                    )
                                    .range(crate::settings_model::OUT_OF_ORDER_TOLERANCE_MS)
                                    .suffix(" ms")
                                    .update_while_editing(false),
                                )
                                .on_hover_text(t(lang, "ooo_tolerance_tip"));
                            if r.drag_stopped() || (r.changed() && !r.dragged()) {
                                for engine in &mut self.engines {
                                    engine.set_out_of_order_tolerance(
                                        self.config.out_of_order_tolerance_ms as i64,
                                    );
                                }
                                let _ = self.config.save();
                            }
                        });

                        // Streams an Open all folder group opens at most.
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("{}:", t(lang, "folder_max_streams")))
                                    .monospace(),
                            );
                            let r = ui
                                .add(
                                    egui::DragValue::new(&mut self.config.folder_max_streams)
                                        .range(crate::settings_model::FOLDER_MAX_STREAMS)
                                        .update_while_editing(false),
                                )
                                .on_hover_text(t(lang, "folder_max_streams_tip"));
                            if r.changed() {
                                let _ = self.config.save();
                            }
                        });

                        // Automatic bookmarks kept per stream; the open streams recompute
                        // theirs under the new cap once the drag is over.
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("{}:", t(lang, "auto_bookmark_max")))
                                    .monospace(),
                            );
                            let resp = ui
                                .add(
                                    egui::DragValue::new(&mut self.config.auto_bookmark_max)
                                        .range(crate::settings_model::AUTO_BOOKMARK_MAX)
                                        .speed(100.0),
                                )
                                .on_hover_text(t(lang, "auto_bookmark_max_tip"));
                            if (resp.changed() && !resp.dragged()) || resp.drag_stopped() {
                                for engine in &mut self.engines {
                                    engine.set_auto_bookmark_max(self.config.auto_bookmark_max);
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
                                        .range(crate::settings_model::COMPRESSED_MAX_GB)
                                        .suffix(" GB"),
                                )
                                .on_hover_text(t(lang, "compressed_max_size_tip"))
                                .changed()
                            {
                                let _ = self.config.save();
                            }
                        });
                        // Standard input: the size its spool restarts from empty at.
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("{}:", t(lang, "stdin_spool_max")))
                                    .monospace(),
                            );
                            if ui
                                .add(
                                    egui::DragValue::new(&mut self.config.stdin_spool_max_mb)
                                        .range(crate::settings_model::STDIN_SPOOL_MAX_MB)
                                        .suffix(" MB"),
                                )
                                .on_hover_text(t(lang, "stdin_spool_max_tip"))
                                .changed()
                            {
                                let _ = self.config.save();
                            }
                        });
                        // Windows Event Log: the events of history read before following.
                        if cfg!(windows) {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format!(
                                        "{}:",
                                        t(lang, "eventlog_initial_events")
                                    ))
                                    .monospace(),
                                );
                                if ui
                                    .add(
                                        egui::DragValue::new(
                                            &mut self.config.event_log_initial_events,
                                        )
                                        .range(crate::settings_model::EVENT_LOG_INITIAL_EVENTS),
                                    )
                                    .on_hover_text(t(lang, "eventlog_initial_events_tip"))
                                    .changed()
                                {
                                    let _ = self.config.save();
                                }
                            });
                        }
                        // System sources: the backlog read at open and the programs run.
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("{}:", t(lang, "system_backlog")))
                                    .monospace(),
                            );
                            if ui
                                .add(
                                    egui::DragValue::new(
                                        &mut self.config.system_source_backlog_lines,
                                    )
                                    .range(crate::settings_model::SYSTEM_SOURCE_BACKLOG_LINES),
                                )
                                .on_hover_text(t(lang, "system_backlog_tip"))
                                .changed()
                            {
                                let _ = self.config.save();
                            }
                        });
                        let mut save_commands = false;
                        for (key, value, default) in [
                            (
                                "system_docker_command",
                                &mut self.config.docker_command,
                                crate::system_source::DEFAULT_DOCKER_COMMAND,
                            ),
                            (
                                "system_journalctl_command",
                                &mut self.config.journalctl_command,
                                crate::system_source::DEFAULT_JOURNALCTL_COMMAND,
                            ),
                            (
                                "system_kubectl_command",
                                &mut self.config.kubectl_command,
                                crate::system_source::DEFAULT_KUBECTL_COMMAND,
                            ),
                        ] {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(format!("{}:", t(lang, key))).monospace());
                                let resp = ui
                                    .add(egui::TextEdit::singleline(value).desired_width(200.0))
                                    .on_hover_text(t(lang, "system_command_tip"));
                                if resp.lost_focus() {
                                    if value.trim().is_empty() {
                                        *value = default.to_string();
                                    }
                                    save_commands = true;
                                }
                            });
                        }
                        if save_commands {
                            let _ = self.config.save();
                        }
                        // Remote files over SSH: the client, the first fetch, the spool.
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("{}:", t(lang, "ssh_program"))).monospace(),
                            );
                            let resp = ui
                                .add(
                                    egui::TextEdit::singleline(&mut self.config.ssh_program)
                                        .desired_width(200.0),
                                )
                                .on_hover_text(t(lang, "system_command_tip"));
                            if resp.lost_focus() {
                                if self.config.ssh_program.trim().is_empty() {
                                    self.config.ssh_program =
                                        crate::ssh_source::DEFAULT_SSH_PROGRAM.to_string();
                                }
                                let _ = self.config.save();
                            }
                        });
                        for (key, tip, value, range) in [
                            (
                                "listener_spool_setting",
                                "stdin_spool_max_tip",
                                &mut self.config.listener_spool_max_mb,
                                crate::settings_model::STDIN_SPOOL_MAX_MB,
                            ),
                            (
                                "listener_mb_setting",
                                "listener_rate_tip",
                                &mut self.config.listener_max_mb_per_sec,
                                crate::settings_model::LISTENER_MAX_MB_PER_SEC,
                            ),
                            (
                                "remote_initial_setting",
                                "remote_initial_tip",
                                &mut self.config.remote_initial_mb,
                                crate::settings_model::REMOTE_INITIAL_MB,
                            ),
                            (
                                "remote_spool_setting",
                                "stdin_spool_max_tip",
                                &mut self.config.remote_spool_max_mb,
                                crate::settings_model::STDIN_SPOOL_MAX_MB,
                            ),
                        ] {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(format!("{}:", t(lang, key))).monospace());
                                if ui
                                    .add(egui::DragValue::new(value).range(range).suffix(" MB"))
                                    .on_hover_text(t(lang, tip))
                                    .changed()
                                {
                                    save_commands = true;
                                }
                            });
                        }
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("{}:", t(lang, "listener_lines_setting")))
                                    .monospace(),
                            );
                            if ui
                                .add(
                                    egui::DragValue::new(
                                        &mut self.config.listener_max_lines_per_sec,
                                    )
                                    .range(crate::settings_model::LISTENER_MAX_LINES_PER_SEC),
                                )
                                .on_hover_text(t(lang, "listener_rate_tip"))
                                .changed()
                            {
                                save_commands = true;
                            }
                        });
                        if save_commands {
                            let _ = self.config.save();
                        }
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
                || self.config.show_line_numbers != line_numbers_before
            {
                self.config.show_time_delta = time_delta.show;
                self.config.time_delta_gap_ms = time_delta.gap_ms;
                let _ = self.config.save();
            }
            if popup_lock_now {
                self.lock();
            }
            if open_formats {
                self.open_log_formats(None, None);
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
                    .fill(theme.panel_bg().into())
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
                        // Filter terms and presets first, then the colour rules.
                        let mut events = crate::ui::dock::PresetEvents::default();
                        crate::ui::dock::render_filters_content(
                            ui,
                            &mut self.engines,
                            &mut self.config.filter_presets,
                            &mut events,
                            Some(&mut self.filters_focus),
                            &theme,
                            lang,
                        );
                        if events.changed {
                            let _ = self.config.save();
                        }
                        ui.separator();
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
                    .fill(theme.panel_bg().into())
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
            // The keys as the key map has them: a rebound action shows its new key.
            use crate::actions::ActionId as A;
            let km = self.keymap.clone();
            let keys = |ids: &[crate::actions::ActionId]| -> String {
                ids.iter()
                    .map(|id| km.label(*id).unwrap_or_else(|| "—".to_string()))
                    .collect::<Vec<_>>()
                    .join("  /  ")
            };

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
                    .fill(theme.panel_bg().into())
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
                        // Every text wraps at the window's width, the descriptions in the
                        // key tables too, so the window can be made narrow.
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                        let desc_width = (ui.available_width() - HELP_KEY_WIDTH).max(120.0);
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
                                .max_col_width(desc_width)
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
                                .max_col_width(desc_width)
                                .show(ui, |ui| {
                                    ui.label(
                                        RichText::new(t(lang, "help_key_space"))
                                            .monospace()
                                            .strong(),
                                    );
                                    ui.label(RichText::new(t(lang, "help_desc_space")).monospace());
                                    ui.end_row();

                                    ui.label(
                                        RichText::new(keys(&[A::SearchFocus])).monospace().strong(),
                                    );
                                    ui.label(
                                        RichText::new(t(lang, "help_desc_search")).monospace(),
                                    );
                                    ui.end_row();

                                    ui.label(
                                        RichText::new(keys(&[A::FindAll])).monospace().strong(),
                                    );
                                    ui.label(
                                        RichText::new(t(lang, "help_desc_find_all")).monospace(),
                                    );
                                    ui.end_row();

                                    ui.label(
                                        RichText::new(keys(&[A::ShowContext])).monospace().strong(),
                                    );
                                    ui.label(
                                        RichText::new(t(lang, "help_desc_context")).monospace(),
                                    );
                                    ui.end_row();

                                    ui.label(
                                        RichText::new(keys(&[A::Patterns])).monospace().strong(),
                                    );
                                    ui.label(RichText::new(t(lang, "act_patterns")).monospace());
                                    ui.end_row();

                                    ui.label(
                                        RichText::new(crate::ui::palette::PALETTE_SHORTCUT_LABEL)
                                            .monospace()
                                            .strong(),
                                    );
                                    ui.label(
                                        RichText::new(t(lang, "help_desc_palette")).monospace(),
                                    );
                                    ui.end_row();

                                    ui.label(
                                        RichText::new(keys(&[A::GlobalFilterBar]))
                                            .monospace()
                                            .strong(),
                                    );
                                    ui.label(
                                        RichText::new(t(lang, "help_desc_global_filter"))
                                            .monospace(),
                                    );
                                    ui.end_row();

                                    ui.label(
                                        RichText::new(keys(&[A::Collapse])).monospace().strong(),
                                    );
                                    ui.label(
                                        RichText::new(t(lang, "help_desc_collapse")).monospace(),
                                    );
                                    ui.end_row();

                                    ui.label(RichText::new("± N").monospace().strong());
                                    ui.label(
                                        RichText::new(t(lang, "help_desc_context_lines"))
                                            .monospace(),
                                    );
                                    ui.end_row();

                                    ui.label(
                                        RichText::new(keys(&[A::SearchNext, A::SearchPrev]))
                                            .monospace()
                                            .strong(),
                                    );
                                    ui.label(
                                        RichText::new(t(lang, "help_desc_find_next")).monospace(),
                                    );
                                    ui.end_row();

                                    ui.label(
                                        RichText::new("Click / SHIFT + Click / CTRL + Click")
                                            .monospace()
                                            .strong(),
                                    );
                                    ui.label(
                                        RichText::new(t(lang, "help_desc_select")).monospace(),
                                    );
                                    ui.end_row();

                                    ui.label(
                                        RichText::new(keys(&[A::SelectAll, A::Copy]))
                                            .monospace()
                                            .strong(),
                                    );
                                    ui.label(RichText::new(t(lang, "help_desc_copy")).monospace());
                                    ui.end_row();

                                    ui.label(
                                        RichText::new(keys(&[A::GoToLine])).monospace().strong(),
                                    );
                                    ui.label(RichText::new(t(lang, "help_desc_goto")).monospace());
                                    ui.end_row();

                                    ui.label(RichText::new(keys(&[A::Wrap])).monospace().strong());
                                    ui.label(RichText::new(t(lang, "help_desc_wrap")).monospace());
                                    ui.end_row();

                                    ui.label(RichText::new("ALT + 1..9").monospace().strong());
                                    ui.label(RichText::new(t(lang, "help_desc_tabs")).monospace());
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
                                        RichText::new("CTRL + SHIFT + 1..9").monospace().strong(),
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
                                        RichText::new(keys(&[A::AlwaysOnTop])).monospace().strong(),
                                    );
                                    ui.label(RichText::new(t(lang, "pin_tip")).monospace());
                                    ui.end_row();

                                    ui.label(
                                        RichText::new(keys(&[A::LockNow])).monospace().strong(),
                                    );
                                    ui.label(RichText::new(t(lang, "help_desc_lock")).monospace());
                                    ui.end_row();

                                    ui.label(
                                        RichText::new(keys(&[
                                            A::BookmarkToggle,
                                            A::BookmarkNext,
                                            A::BookmarkPrev,
                                        ]))
                                        .monospace()
                                        .strong(),
                                    );
                                    ui.label(
                                        RichText::new(t(lang, "help_desc_bookmark")).monospace(),
                                    );
                                    ui.end_row();

                                    ui.label(
                                        RichText::new(keys(&[A::RuleNext, A::RulePrev]))
                                            .monospace()
                                            .strong(),
                                    );
                                    ui.label(
                                        RichText::new(t(lang, "help_desc_rule_nav")).monospace(),
                                    );
                                    ui.end_row();

                                    ui.label(
                                        RichText::new(t(lang, "help_key_double_click"))
                                            .monospace()
                                            .strong(),
                                    );
                                    ui.label(
                                        RichText::new(t(lang, "help_desc_token_hl")).monospace(),
                                    );
                                    ui.end_row();

                                    ui.label(RichText::new("CTRL + Q").monospace().strong());
                                    ui.label(RichText::new(t(lang, "help_desc_quit")).monospace());
                                    ui.end_row();

                                    ui.label(
                                        RichText::new("F7  /  SHIFT + F7").monospace().strong(),
                                    );
                                    ui.label(RichText::new(t(lang, "compare_next")).monospace());
                                    ui.end_row();

                                    ui.label(
                                        RichText::new("CTRL + SHIFT + N").monospace().strong(),
                                    );
                                    ui.label(RichText::new(t(lang, "scratch_send")).monospace());
                                    ui.end_row();

                                    ui.label(
                                        RichText::new(t(lang, "help_key_charsel"))
                                            .monospace()
                                            .strong(),
                                    );
                                    ui.label(
                                        RichText::new(t(lang, "help_desc_charsel")).monospace(),
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
                                RichText::new(t(lang, "help_filter_expr"))
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

        // 11a. Open system source dialog
        if let Some(dialog) = self.system_dialog.as_mut() {
            use crate::ui::system_dialog::SystemChoice;
            let commands = self.config.system_commands();
            match dialog.show(&ctx, &commands, self.config.language, self.config.theme) {
                Some(SystemChoice::Open(path)) => {
                    self.system_dialog = None;
                    self.open_log_file(path);
                }
                Some(SystemChoice::Cancel) => self.system_dialog = None,
                None => {}
            }
        }

        // 11a. Listen dialog
        if let Some(dialog) = self.listen_dialog.as_mut() {
            use crate::ui::listen_dialog::ListenChoice;
            match dialog.show(&ctx, self.config.language, self.config.theme) {
                Some(ListenChoice::Open(url)) => {
                    self.listen_dialog = None;
                    self.open_log_file(url);
                }
                Some(ListenChoice::Cancel) => self.listen_dialog = None,
                None => {}
            }
        }
        if let Some(dialog) = self.debug_dialog.as_mut() {
            use crate::ui::debug_dialog::DebugChoice;
            match dialog.show(&ctx, self.config.language, self.config.theme) {
                Some(DebugChoice::Open(uri)) => {
                    self.debug_dialog = None;
                    self.open_log_file(uri);
                }
                Some(DebugChoice::Cancel) => self.debug_dialog = None,
                None => {}
            }
        }

        if let Some(dialog) = self.eventlog_dialog.as_mut() {
            use crate::ui::eventlog_dialog::EventLogChoice;
            match dialog.show(&ctx, self.config.language, self.config.theme) {
                Some(EventLogChoice::Open(uri)) => {
                    self.eventlog_dialog = None;
                    self.open_log_file(uri);
                }
                Some(EventLogChoice::Cancel) => self.eventlog_dialog = None,
                None => {}
            }
        }

        // 11a. Remote file (SSH) dialog
        if let Some(dialog) = self.remote_dialog.as_mut() {
            use crate::ui::remote_dialog::RemoteChoice;
            let initial_mb = self.config.remote_initial_mb;
            match dialog.show(&ctx, initial_mb, self.config.language, self.config.theme) {
                Some(RemoteChoice::Open(urls)) => {
                    self.remote_dialog = None;
                    for url in urls {
                        self.open_log_file(url);
                    }
                }
                Some(RemoteChoice::Cancel) => self.remote_dialog = None,
                None => {}
            }
        }

        // 11a. New merged view dialog
        if let Some(prompt) = self.merge_prompt.as_mut() {
            use crate::ui::merge_dialog::MergeChoice;
            match prompt.show(&ctx, &self.engines, self.config.language, self.config.theme) {
                Some(MergeChoice::Create(sources)) => {
                    self.merge_prompt = None;
                    self.open_merged_view(&sources);
                }
                Some(MergeChoice::Cancel) => self.merge_prompt = None,
                None => {}
            }
        }

        // 11a''. Recent files (palette): a click opens one, as the toolbar's 🕒 menu does.
        if self.recent_files_window {
            let lang = self.config.language;
            let mut open = true;
            let mut pick = None;
            let mut clear = false;
            egui::Window::new(t(lang, "recent_files"))
                .id(egui::Id::new("fasttail_recent_files"))
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(&ctx, |ui| {
                    if self.config.recent_files.is_empty() {
                        ui.label(RichText::new(t(lang, "no_recent_files")).italics());
                        return;
                    }
                    for path in &self.config.recent_files {
                        if ui
                            .button(RichText::new(format!("📄 {}", path.display())).monospace())
                            .clicked()
                        {
                            pick = Some(path.clone());
                        }
                    }
                    ui.separator();
                    clear = ui
                        .button(format!("🗑 {}", t(lang, "clear_recent")))
                        .clicked();
                });
            if clear {
                self.config.recent_files.clear();
                let _ = self.config.save();
            }
            let escape = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
            if !open || pick.is_some() || escape {
                self.recent_files_window = false;
            }
            if let Some(path) = pick {
                self.open_log_file(path);
            }
        }

        // 11a''''. Stream information (palette): closed when its stream goes.
        if let Some(path) = self.stream_info.clone() {
            let lang = self.config.language;
            let mut open = true;
            match self.engines.iter().find(|e| e.path == path) {
                Some(engine) => {
                    let rows = crate::stream_info::rows(engine);
                    egui::Window::new(format!(
                        "{} · {}",
                        t(lang, "info_title"),
                        crate::find_all::stream_name(engine)
                    ))
                    .id(egui::Id::new("fasttail_stream_info"))
                    .open(&mut open)
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(&ctx, |ui| {
                        egui::Grid::new("fasttail_stream_info_grid")
                            .num_columns(2)
                            .striped(true)
                            .show(ui, |ui| {
                                for (item, value) in rows {
                                    ui.label(t(lang, item.key()));
                                    let text = match value {
                                        crate::stream_info::Value::Text(s) => s,
                                        crate::stream_info::Value::Key(k) => t(lang, k).to_string(),
                                    };
                                    ui.label(RichText::new(text).monospace());
                                    ui.end_row();
                                }
                            });
                    });
                }
                None => open = false,
            }
            let escape = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
            if !open || escape {
                self.stream_info = None;
            }
        }

        // 11a'''. Export dialog: closed when its stream goes.
        if let Some(dialog) = self.export_dialog.as_mut() {
            match self.engines.iter().find(|e| e.path == dialog.source) {
                Some(engine) => {
                    let lang = self.config.language;
                    if dialog.show(&ctx, engine, &mut self.config, lang) {
                        self.export_dialog = None;
                    }
                }
                None => self.export_dialog = None,
            }
        }

        // 11a''. Split file dialog: closed when its stream goes.
        if let Some(dialog) = self.split_dialog.as_mut() {
            match self.engines.iter_mut().find(|e| e.path == dialog.source) {
                Some(engine) => {
                    let lang = self.config.language;
                    if dialog.show(&ctx, engine, lang, self.config.theme).is_some() {
                        self.split_dialog = None;
                    }
                }
                None => self.split_dialog = None,
            }
        }

        // 11a'. Log formats dialog
        if let Some(dialog) = self.log_formats.as_mut() {
            use crate::ui::log_formats::FormatsAction;
            let lib = crate::log_format::library();
            match dialog.show(&ctx, &lib, self.config.language, self.config.theme) {
                Some(FormatsAction::Changed) => {
                    let dir = dialog.dir.clone();
                    self.reload_log_formats(&dir);
                }
                Some(FormatsAction::Close) => self.log_formats = None,
                None => {}
            }
        }

        // 11b. Open folder dialog (folder drop, 📂* button)
        if let Some(prompt) = self.folder_prompt.as_mut() {
            use crate::ui::folder_dialog::FolderChoice;
            match prompt.show(&ctx, self.config.language, self.config.theme) {
                Some(FolderChoice::FollowNewest(spec)) => {
                    self.folder_prompt = None;
                    self.open_log_file(spec.path());
                }
                Some(FolderChoice::OpenAll(spec)) => {
                    self.folder_prompt = None;
                    self.open_folder_group(spec);
                }
                Some(FolderChoice::Cancel) => self.folder_prompt = None,
                None => {}
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
        // Bookmark report dialog: built a few milliseconds per frame, then copied or saved.
        if let Some(mut dialog) = self.report_dialog.take() {
            if dialog.show(&ctx, &self.engines, &mut self.config) {
                self.report_dialog = Some(dialog);
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

        // Archive entry picker: each chosen entry opens as its own stream.
        if let Some(picker) = self.archive_picker.as_mut() {
            let outcome = picker.show(&ctx, self.config.language, self.config.theme);
            if let Some(outcome) = outcome {
                self.apply_picker_outcome(outcome);
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
        if let Some(notice) = self.handoff_notice.clone() {
            let lang = self.config.language;
            let theme = self.config.theme;
            let mut is_open = true;
            let mut close = false;
            egui::Window::new(
                RichText::new(format!("⚠ {}", t(lang, "handoff_failed")))
                    .monospace()
                    .color(theme.warn_color()),
            )
            .id(egui::Id::new("fasttail_handoff_notice"))
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
                self.handoff_notice = None;
            }
        }
        if self.interface_switch {
            self.draw_interface_switch(&ctx);
        }
        if let Some(notice) = self.save_notice.clone() {
            let lang = self.config.language;
            let theme = self.config.theme;
            let mut is_open = true;
            let mut close = false;
            egui::Window::new(
                RichText::new(format!("💾 {}", t(lang, self.save_notice_title)))
                    .monospace()
                    .color(theme.warn_color()),
            )
            .id(egui::Id::new("fasttail_save_notice"))
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
                self.save_notice = None;
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
        Color32::from(self.config.theme.bg_color()).to_normalized_gamma_f32()
    }

    /// Called before every `ui`, and alone while the window is minimised or hidden (no
    /// egui pass runs then: on Windows a minimised window gets no `ui` at all, issue #161).
    /// While the window shows nothing, the streams are polled here, with their rules,
    /// sounds, tools, alerts and the tray; a waker keeps these calls coming.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let hidden = self.hidden_in_tray || ctx.input(|i| i.viewport().visible() == Some(false));
        self.background_waker(ctx)
            .store(hidden, std::sync::atomic::Ordering::Relaxed);
        if hidden {
            self.background_step(ctx);
        }
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
        } else {
            // Frame cap: vsync is off on glow, so without it a stream of input events
            // (scrolling, a growing file) could render far above the display rate.
            let target_fps = if self.renderer.is_software() {
                self.config.max_fps_software
            } else {
                self.config.max_fps
            }
            .max(1);
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
        self.scratchpad.save_on_exit();
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
                        self.session_save_failed(format!("{}\n{err}", file.display()));
                    }
                }
            }
            SessionAction::Save => {
                if let Err(err) = self.save_session() {
                    self.session_save_failed(err.to_string());
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

    /// The stream of the focused dock leaf (or, without a focused leaf, the active tab of
    /// the main surface), if it is a stream.
    fn focused_stream_path(&mut self) -> Option<PathBuf> {
        match self.dock_state.find_active_focused() {
            Some((_, FastTailTab::LogStream(p))) => Some(p.clone()),
            Some(_) => None,
            None => match self.dock_state.main_surface_mut().find_active() {
                Some((_, FastTailTab::LogStream(p))) => Some(p.clone()),
                _ => None,
            },
        }
    }

    /// "Open File": the native picker, then every picked file is opened.
    fn pick_and_open_files(&mut self) {
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

    /// Rebuilds the key map when `[shortcuts]` or a tool's shortcut changed.
    pub fn refresh_keymap(&mut self) {
        let tools: Vec<Option<String>> = self
            .config
            .external_tools
            .iter()
            .map(|t| t.shortcut.clone())
            .collect();
        if self
            .keymap_inputs
            .as_ref()
            .is_some_and(|(s, t)| *s == self.config.shortcuts && *t == tools)
        {
            return;
        }
        self.keymap =
            crate::keymap::KeyMap::build(&self.config.shortcuts, &self.config.external_tools);
        crate::ui::keys::set_current(&self.keymap);
        self.keymap_inputs = Some((self.config.shortcuts.clone(), tools));
    }

    /// "Open pattern": the prompt, seeded with the folder of the most recent file.
    fn open_pattern_prompt(&mut self) {
        let seed = self
            .config
            .recent_files
            .first()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .map(|d| Self::default_pattern_for(&d))
            .unwrap_or_default();
        self.prompt_pattern(seed);
    }

    /// CTRL + SHIFT + P opens or closes the palette (never while the window is locked);
    /// while it is open it takes every key event of the frame, and what it picks runs.
    fn render_palette(&mut self, ctx: &egui::Context) {
        use crate::ui::palette::{PaletteOutcome, PALETTE_SHORTCUT};
        if self.locked {
            self.palette.close();
            return;
        }
        if ctx.input_mut(|i| i.consume_shortcut(&PALETTE_SHORTCUT)) {
            if self.palette.is_open() {
                self.palette.close();
                ctx.memory_mut(|m| m.stop_text_input());
            } else {
                let target = self.focused_stream_path();
                self.palette.open(target);
            }
            ctx.request_repaint();
        }
        if !self.palette.is_open() {
            return;
        }
        let target = self.palette.target().cloned();
        let state = crate::actions::ActionState {
            stream: target
                .as_ref()
                .and_then(|p| self.engines.iter().find(|e| paths_equal(&e.path, p)))
                .map(crate::actions::StreamState::of),
            has_pin: !self.config.lock_pin.is_empty(),
            has_session: self.config.current_session.is_some(),
        };
        let outcome = self.palette.show(ctx, &self.config, &state);
        // Keys typed while the palette is open were its own: no stream or window
        // shortcut may see them (Space must not toggle follow, F3 not search).
        ctx.input_mut(|i| {
            i.events.retain(|e| {
                !matches!(
                    e,
                    egui::Event::Key { .. }
                        | egui::Event::Text(_)
                        | egui::Event::Paste(_)
                        | egui::Event::Copy
                        | egui::Event::Cut
                )
            })
        });
        if !self.palette.is_open() {
            ctx.memory_mut(|m| m.stop_text_input());
        }
        match outcome {
            Some(PaletteOutcome::Run(action)) => {
                crate::actions::push_recent(&mut self.config.palette_recent, action.key);
                self.run_action(ctx, action, target);
                let _ = self.config.save();
            }
            Some(PaletteOutcome::SetValue(setting, index)) => {
                if let Some(meta) = crate::actions::ENUM_SETTINGS
                    .iter()
                    .find(|m| m.setting == setting)
                {
                    crate::actions::push_recent(&mut self.config.palette_recent, meta.key);
                }
                crate::actions::set_enum_value(setting, &mut self.config, index);
                if setting == crate::actions::EnumSetting::SizeUnit {
                    for eng in &mut self.engines {
                        eng.size_unit = self.config.size_unit;
                    }
                }
                let _ = self.config.save();
            }
            None => {}
        }
        ctx.request_repaint();
    }

    /// Runs an action picked in the command palette with the same effect as its button,
    /// menu item or key. A stream action goes to `target`, the stream focused when the
    /// palette opened, and runs when that stream is drawn this frame.
    fn run_action(
        &mut self,
        ctx: &egui::Context,
        action: crate::actions::Action,
        target: Option<PathBuf>,
    ) {
        use crate::actions::{ActionId as A, Scope};
        if action.scope == Scope::Stream {
            if let Some(path) = target {
                // Bring the stream to the front: another tab may have become active
                // meanwhile (a file dropped while the palette was open).
                if let Some(locator) = self
                    .dock_state
                    .find_tab(&FastTailTab::LogStream(path.clone()))
                {
                    let _ = self.dock_state.set_active_tab(locator);
                }
                self.palette_action = Some(crate::actions::PendingAction::new(path, action.id));
            }
            return;
        }
        let zoom = ctx.zoom_factor();
        match action.id {
            A::Help => self.config.help_open = !self.config.help_open,
            A::About => self.config.about_open = !self.config.about_open,
            A::Settings => self.config.settings_open = !self.config.settings_open,
            A::ColorFilters => self.config.filters_open = !self.config.filters_open,
            A::AlwaysOnTop => self.config.always_on_top = !self.config.always_on_top,
            A::LockNow if !self.config.lock_pin.is_empty() => {
                self.lock();
            }
            A::LockNow => {}
            A::GlobalFilterBar => {
                self.config.global_filter.bar_open = !self.config.global_filter.bar_open
            }
            A::FindAll => {
                let query = target
                    .as_ref()
                    .and_then(|p| self.engines.iter().find(|e| paths_equal(&e.path, p)))
                    .map(|e| e.search_query.clone());
                self.open_find_results(query);
            }
            A::ZoomIn => ctx.set_zoom_factor(crate::config::stepped_zoom(zoom, 1)),
            A::ZoomOut => ctx.set_zoom_factor(crate::config::stepped_zoom(zoom, -1)),
            A::ZoomReset => ctx.set_zoom_factor(1.0),
            A::PlayAll => {
                for eng in &mut self.engines {
                    eng.is_watching = true;
                    eng.follow_tail = !eng.is_compressed();
                }
            }
            A::PauseAll => {
                for eng in &mut self.engines {
                    eng.is_watching = false;
                    eng.follow_tail = false;
                }
            }
            A::OpenFile => self.pick_and_open_files(),
            A::OpenPattern => self.open_pattern_prompt(),
            A::PasteAsStream => self.request_paste(ctx, PasteTarget::New),
            A::PasteAppend => {
                if let Some(path) = target.filter(|p| {
                    self.engines
                        .iter()
                        .any(|e| e.path == *p && e.is_clipboard())
                }) {
                    self.request_paste(ctx, PasteTarget::Append(path));
                }
            }
            A::OpenRemote => self.remote_dialog = Some(Default::default()),
            A::OpenListener => self.listen_dialog = Some(Default::default()),
            A::OpenDebugOutput => self.debug_dialog = Some(Default::default()),
            A::OpenEventLog => {
                self.eventlog_dialog =
                    Some(crate::ui::eventlog_dialog::EventLogDialog::with_channels())
            }
            A::OpenSystemSource => {
                self.system_dialog = Some(crate::ui::system_dialog::SystemDialog::new());
            }
            A::LogFormats => self.open_log_formats(None, None),
            A::KeyboardShortcuts => self.shortcuts_dialog.open = true,
            A::SystemReconnect => {
                let focused = self.focused_stream_path();
                if let Some(e) = focused.and_then(|p| self.engines.iter_mut().find(|e| e.path == p))
                {
                    if let Some(s) = e.system.as_mut() {
                        s.reconnect_requested = true;
                    }
                    if let Some(s) = e.ssh.as_mut() {
                        s.reconnect_requested = true;
                    }
                }
            }
            A::SplitFile => {
                let focused = self.focused_stream_path();
                if let Some(idx) =
                    focused.and_then(|p| self.engines.iter().position(|e| e.path == p))
                {
                    self.open_split_dialog(idx);
                }
            }
            A::MergedView => {
                let first = self.focused_stream_path();
                self.merge_prompt =
                    Some(crate::ui::merge_dialog::MergePrompt::starting_with(first));
            }
            A::ClearRecentFiles => self.config.recent_files.clear(),
            A::RecentFiles => self.recent_files_window = true,
            A::StreamInfo => self.stream_info = self.focused_stream_path(),
            A::SessionSaveAs => self.run_session_action(SessionAction::SaveAs),
            A::SessionSave => self.run_session_action(SessionAction::Save),
            A::BookmarkReportAll => {
                self.open_bookmark_report(crate::ui::report_dialog::ReportScope::All)
            }
            A::ScratchpadOpen => self.open_scratchpad(),
            A::SessionLoad => self.run_session_action(SessionAction::Load),
            A::SessionClearRecent => self.run_session_action(SessionAction::ClearRecent),
            A::SessionSaveDefault => self.run_session_action(SessionAction::SaveDefault),
            A::SearchPane => self.config.search_pane = !self.config.search_pane,
            // The scope is chosen in the results pane's header.
            A::SearchScope => self.config.search_pane = true,
            A::Toggle(setting) => {
                let meta = crate::actions::bool_setting(setting);
                let value = !(meta.get)(&self.config);
                (meta.set)(&mut self.config, value);
                if setting == crate::actions::BoolSetting::Borderless {
                    ctx.send_viewport_cmd(ViewportCommand::Decorations(!self.config.borderless));
                }
            }
            // The value step is the palette's; stream actions were routed above.
            _ => {}
        }
    }
}

/// How often a minimised or hidden window wakes to read the streams (issue #161), as the
/// tray's own timer does.
const BACKGROUND_WAKE: Duration = Duration::from_millis(250);

/// Structure of the dock (surface, node and tab order) without geometry.
fn dock_signature(dock: &DockState<FastTailTab>) -> String {
    let mut out = String::new();
    for (path, tab) in dock.iter_all_tabs().filter(|(_, tab)| !is_stdin_tab(tab)) {
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

/// Room the key column of the help tables takes, plus the gap and the group frame: the
/// descriptions wrap in what is left of the window.
const HELP_KEY_WIDTH: f32 = 220.0;

/// Renames the tabs of the derived streams and saved merged views of `engines` in
/// `dock`: to their saved `filter:` / `merged:` name (`to_saved`), or from it back to
/// their spool.
fn rename_derived_tabs(dock: &mut DockState<FastTailTab>, engines: &[TailEngine], to_saved: bool) {
    for (_, tab) in dock.iter_all_tabs_mut() {
        let FastTailTab::LogStream(p) = tab else {
            continue;
        };
        let found = engines.iter().find_map(|e| {
            let identity = crate::workspace::saved_identity(e)?;
            if to_saved && e.path == *p {
                Some(identity)
            } else if !to_saved && identity == *p {
                Some(e.path.clone())
            } else {
                None
            }
        });
        if let Some(path) = found {
            *p = path;
        }
    }
}

/// The tab of `from` shows `to` instead, or goes when `to` is `None`.
fn rename_tab(dock: &mut DockState<FastTailTab>, from: &Path, to: Option<PathBuf>) {
    match to {
        Some(to) => {
            for (_, tab) in dock.iter_all_tabs_mut() {
                if matches!(tab, FastTailTab::LogStream(p) if p == from) {
                    *tab = FastTailTab::LogStream(to.clone());
                }
            }
        }
        None => crate::ui::find_results::retain_tabs(
            dock,
            |tab| !matches!(tab, FastTailTab::LogStream(p) if p == from),
        ),
    }
}

/// Tabs no workspace or session keeps as they are: standard input, and a derived stream
/// still named after its spool (`rename_derived_tabs` gives it its saved name first).
/// Where a paste goes: a new clipboard stream, or the end of that clipboard stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasteTarget {
    New,
    Append(PathBuf),
}

/// The default keys of Paste as stream (egui turns them into a paste event, not a key).
const PASTE_STREAM_KEYS: &str = "CTRL + SHIFT + V";

fn is_stdin_tab(tab: &FastTailTab) -> bool {
    matches!(tab, FastTailTab::LogStream(p)
        if crate::stdin_source::is_ephemeral_path(p)
            || crate::filter_tab::is_derived_path(p)
            || crate::merged_stream::is_merged_path(p))
}

/// Command line options applied to the standard-input stream when it opens.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StdinOptions {
    pub filter: Option<String>,
    pub filter_expr: Option<String>,
    pub exclude: Option<String>,
    pub follow: Option<bool>,
    pub since: Option<String>,
    pub until: Option<String>,
}

impl StdinOptions {
    pub fn from_cli(cli: &crate::cli::CliArgs) -> Self {
        Self {
            filter: cli.window_filter().cloned(),
            filter_expr: cli.filter_expr.clone(),
            exclude: cli.window_exclude().cloned(),
            follow: cli.follow,
            since: cli.since.clone(),
            until: cli.until.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::allowed_while_locked;
    use egui::{Event, Key, Modifiers};

    /// A relative `--since` is fixed on the clock the stream's time display reads the
    /// window on: with the display in UTC, `-1h` keeps the lines of the last UTC hour
    /// whatever the local zone.
    #[test]
    fn cli_relative_time_follows_the_time_display() {
        use crate::timestamp::{format_millis, TimeDisplay};
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let stamp = |ms: i64| format_millis(ms).replace(' ', "T") + "Z";
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("utc.log");
        std::fs::write(
            &path,
            format!(
                "{} old\n{} recent\n",
                stamp(now - 3 * 3_600_000),
                stamp(now - 30 * 60_000)
            ),
        )
        .unwrap();
        let mut engine = crate::tail_engine::TailEngine::open(&path).unwrap();
        engine.set_time_display(TimeDisplay::Utc);
        engine.ensure_timestamps();
        crate::cli::apply_time_window(&mut engine, Some("-1h"), None);
        assert!(!engine.time_range_error);
        let visible: Vec<usize> = (0..engine.visible_line_count())
            .filter_map(|r| engine.get_actual_line_idx(r))
            .collect();
        assert_eq!(visible, vec![1]);
        // Kept as typed: the window opened from the command line slides with the clock.
        assert_eq!(engine.time_from_text, "-1h");
        assert!(engine.time_window_live());
        crate::cli::apply_time_window(&mut engine, Some("nonsense"), None);
        assert!(engine.time_range_error);
    }

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
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, screen)),
            ..Default::default()
        };
        ctx.begin_pass(input);
        let width = super::lock_prompt_width(&ctx, lang);
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
    fn lock_keeps_what_the_pin_prompt_needs() {
        assert!(allowed_while_locked(&Event::Text("4".to_owned())));
        assert!(allowed_while_locked(&key(Key::Enter, Modifiers::NONE)));
        assert!(allowed_while_locked(&key(Key::Backspace, Modifiers::NONE)));
        assert!(allowed_while_locked(&key(Key::ArrowLeft, Modifiers::NONE)));
        assert!(allowed_while_locked(&Event::PointerGone));
    }
}
