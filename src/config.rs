use crate::i18n::Language;
use crate::session::{Session, StreamEntry, MAX_RECENT_SESSIONS};
use crate::tail_engine::{HighlightRule, SizeUnit};
use crate::theme::CyberTheme;
use ini::Ini;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const CONFIG_FILE_NAME: &str = "fasttail.ini";
const OLD_CONFIG_FILE_NAME: &str = "fasttail.toml";

/// Caps for persisted bookmarks: files remembered, and lines per file.
pub const MAX_BOOKMARK_FILES: usize = 50;
pub const MAX_BOOKMARKS_PER_FILE: usize = 1000;
/// Cap for the files remembered with line wrap on.
pub const MAX_WRAPPED_FILES: usize = 50;

/// Unlock phrase that always works, whatever the PIN is — a nod to WarGames.
pub const LOCK_BACKDOOR: &str = "joshua";

/// Scrambles a PIN before it is written to the ini file. FNV-1a over a fixed salt plus
/// the digits: it keeps the PIN from being read at a glance out of `fasttail.ini`, and
/// that is the whole of its ambition. The lock is a deterrent against someone walking
/// past the screen, not a security boundary — the log files stay readable on disk and
/// `LOCK_BACKDOOR` opens it anyway.
pub fn scramble_pin(pin: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in b"fasttail-lock-v1".iter().chain(pin.as_bytes()) {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// Whether `attempt` opens a lock whose scrambled PIN is `stored`.
pub fn pin_matches(stored: &str, attempt: &str) -> bool {
    let attempt = attempt.trim();
    if attempt.eq_ignore_ascii_case(LOCK_BACKDOOR) {
        return true;
    }
    !stored.is_empty() && scramble_pin(attempt) == stored
}

/// A PIN the lock will accept: 4 to 12 digits.
pub fn is_valid_pin(pin: &str) -> bool {
    let pin = pin.trim();
    (4..=12).contains(&pin.chars().count()) && pin.chars().all(|c| c.is_ascii_digit())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FastTailConfig {
    pub theme: CyberTheme,
    pub language: Language,
    /// Follow the operating system language instead of the stored one. On a fresh
    /// install this is how FastTail starts; picking a language in the settings turns it
    /// off, and picking "system language" turns it back on.
    #[serde(default = "default_language_auto")]
    pub language_auto: bool,
    pub screensaver_enabled: bool,
    pub screensaver_timeout_mins: u32,
    /// Lock the window behind a PIN when the screensaver ends or the user asks for it.
    #[serde(default)]
    pub lock_enabled: bool,
    /// Scrambled PIN (see `scramble_pin`), empty when no PIN is set. This is a deterrent
    /// against a passer-by, not protection: the log files themselves stay readable on disk.
    #[serde(default)]
    pub lock_pin: String,
    pub telemetry_enabled: bool,
    pub sound_enabled: bool,
    /// Rendering backend: auto (OpenGL, then wgpu on failure), glow or wgpu. Applies at start.
    #[serde(default)]
    pub renderer: crate::renderer::RendererChoice,
    /// Keep the main window above other windows.
    #[serde(default)]
    pub always_on_top: bool,
    /// Request OS attention (taskbar flash) when a sound-alert rule matches in a hidden tab
    /// while the window is unfocused.
    #[serde(default)]
    pub flash_on_alert: bool,
    /// Colour rows by their detected log level when no highlight rule matches them.
    #[serde(default = "default_true")]
    pub level_colors: bool,
    /// Search results pane under the rows of a stream with an active query, and its
    /// height; one preference for every stream.
    #[serde(default)]
    pub search_pane: bool,
    #[serde(default = "default_search_pane_height")]
    pub search_pane_height: f32,
    /// Overview strip (hits, bookmarks, errors) beside the main view's scroll bar.
    #[serde(default = "default_true")]
    pub overview_strip: bool,
    #[serde(default)]
    pub borderless: bool,
    #[serde(default = "default_true")]
    pub show_line_numbers: bool,
    #[serde(default = "default_font_size")]
    pub font_size: f32,
    /// Interface zoom (egui zoom factor): `Ctrl +`, `Ctrl -`, `Ctrl 0`, `Ctrl + wheel`
    /// and the Settings zoom row all move this one value, so they cannot disagree.
    #[serde(default = "default_zoom_factor")]
    pub zoom_factor: f32,
    /// Background stream polling cadence in milliseconds (50..=5000 ms, default 250).
    #[serde(default = "default_poll_interval_ms")]
    pub poll_interval_ms: u32,
    /// Fallback filesystem metadata size check cadence in milliseconds (50..=10000 ms, default 500).
    #[serde(default = "default_size_check_interval_ms")]
    pub size_check_interval_ms: u32,
    /// Maximum UI frame rate when running on a hardware GPU (15..=240, default 60).
    #[serde(default = "default_max_fps")]
    pub max_fps: u32,
    /// Maximum UI frame rate when running on a software rasterizer / VM (10..=60, default 30).
    #[serde(default = "default_max_fps_software")]
    pub max_fps_software: u32,
    /// Cadence in milliseconds for throttling pure pointer move events (0..=1000 ms, default 100).
    #[serde(default = "default_mouse_throttle_ms")]
    pub mouse_throttle_ms: u64,
    /// Maximum file size in megabytes for Markdown rendering (1..=100 MB, default 1).
    #[serde(default = "default_markdown_max_mb")]
    pub markdown_max_mb: u32,
    /// Folder the decompressed logs are spooled under (in its `fasttail-spool`
    /// subfolder); `None` (or empty in the ini) = the system temporary folder. See
    /// `spool`.
    #[serde(default)]
    pub spool_dir: Option<PathBuf>,
    /// Output cap of one decompression in GB (1..=1024, default 20).
    #[serde(default = "default_compressed_max_gb")]
    pub compressed_max_gb: u32,
    #[serde(default)]
    pub size_unit: SizeUnit,
    #[serde(default)]
    pub open_files: Vec<PathBuf>,
    pub recent_files: Vec<PathBuf>,
    #[serde(default)]
    pub search_history: Vec<String>,
    /// Bookmarked lines per file (most recently used first), see `set_bookmarks`.
    #[serde(default)]
    pub bookmarks: Vec<(PathBuf, Vec<usize>)>,
    /// Files whose stream has line wrap on (most recently toggled first), see `set_wrap`.
    #[serde(default)]
    pub wrapped_files: Vec<PathBuf>,
    pub highlight_rules: Vec<HighlightRule>,
    /// User-configured external tools (`[tool.N]` sections), see `external_tools`.
    #[serde(default)]
    pub external_tools: Vec<crate::external_tools::ExternalTool>,
    #[serde(default)]
    pub baretail_import: bool,
    pub baretail_prompt_shown: bool,
    #[serde(default)]
    pub dock_layout: Option<String>,
    /// Per-stream state of the default session (filters, search, encoding), keyed by
    /// path; wrap and bookmarks keep their own sections. See `set_stream_state`.
    #[serde(default, skip)]
    pub streams: Vec<StreamEntry>,
    /// Named session file the workspace was last saved to or loaded from.
    #[serde(default)]
    pub current_session: Option<PathBuf>,
    /// Recently used session files, most recent first.
    #[serde(default)]
    pub recent_sessions: Vec<PathBuf>,
    #[serde(default)]
    pub window_x: Option<f32>,
    #[serde(default)]
    pub window_y: Option<f32>,
    #[serde(default)]
    pub window_width: Option<f32>,
    #[serde(default)]
    pub window_height: Option<f32>,
    #[serde(default)]
    pub window_maximized: bool,
    /// Whether the window was minimized when the application was closed; it reopens that
    /// way, like it reopens maximized.
    #[serde(default)]
    pub window_minimized: bool,
    #[serde(default)]
    pub settings_open: bool,
    #[serde(default)]
    pub filters_open: bool,
    #[serde(default)]
    pub about_open: bool,
    #[serde(default)]
    pub help_open: bool,
    #[serde(default)]
    pub settings_pos: Option<[f32; 2]>,
    #[serde(default)]
    pub settings_size: Option<[f32; 2]>,
    #[serde(default)]
    pub filters_pos: Option<[f32; 2]>,
    #[serde(default)]
    pub filters_size: Option<[f32; 2]>,
    #[serde(default)]
    pub about_pos: Option<[f32; 2]>,
    #[serde(default)]
    pub about_size: Option<[f32; 2]>,
    #[serde(default)]
    pub help_pos: Option<[f32; 2]>,
    #[serde(default)]
    pub help_size: Option<[f32; 2]>,
}

fn default_true() -> bool {
    true
}

/// Font size the zoom is measured against: `font_size == DEFAULT_FONT_SIZE` is 100%.
pub const DEFAULT_FONT_SIZE: f32 = 13.0;
/// Range the zoom shortcuts and the settings clamp the font size to.
pub const MIN_FONT_SIZE: f32 = 8.0;
pub const MAX_FONT_SIZE: f32 = 32.0;

/// Bounds of the interface zoom (egui's zoom factor: it scales the whole UI, log text
/// included, because it multiplies the points-per-pixel of the frame).
pub const MIN_ZOOM: f32 = 0.5;
pub const MAX_ZOOM: f32 = 3.0;
/// One step of the zoom buttons and of `Ctrl +` / `Ctrl -`.
pub const ZOOM_STEP: f32 = 0.1;

/// The interface zoom as a percentage, the way the title bar and the settings show it.
pub fn zoom_percent(zoom_factor: f32) -> i32 {
    (zoom_factor * 100.0).round() as i32
}

/// `zoom` moved by `steps` notches and clamped to the supported range.
pub fn stepped_zoom(zoom: f32, steps: i32) -> f32 {
    (zoom + steps as f32 * ZOOM_STEP).clamp(MIN_ZOOM, MAX_ZOOM)
}

fn default_font_size() -> f32 {
    DEFAULT_FONT_SIZE
}

fn default_search_pane_height() -> f32 {
    crate::ui::dock::DEFAULT_SEARCH_PANE_HEIGHT
}

/// Height range accepted for the search results pane in `fasttail.ini`.
const SEARCH_PANE_HEIGHT_RANGE: std::ops::RangeInclusive<f32> = 40.0..=4000.0;

fn default_zoom_factor() -> f32 {
    1.0
}

fn default_language_auto() -> bool {
    true
}

fn default_poll_interval_ms() -> u32 {
    250
}

fn default_size_check_interval_ms() -> u32 {
    500
}

fn default_max_fps() -> u32 {
    60
}

fn default_max_fps_software() -> u32 {
    30
}

fn default_mouse_throttle_ms() -> u64 {
    100
}

fn default_markdown_max_mb() -> u32 {
    1
}

fn default_compressed_max_gb() -> u32 {
    crate::compressed::DEFAULT_MAX_GB
}

impl Default for FastTailConfig {
    fn default() -> Self {
        Self {
            theme: CyberTheme::Tron,
            language: Language::detect(),
            language_auto: default_language_auto(),
            screensaver_enabled: true,
            screensaver_timeout_mins: 10,
            lock_enabled: false,
            lock_pin: String::new(),
            telemetry_enabled: true,
            sound_enabled: false,
            renderer: crate::renderer::RendererChoice::Auto,
            always_on_top: false,
            flash_on_alert: false,
            level_colors: true,
            search_pane: false,
            search_pane_height: default_search_pane_height(),
            overview_strip: true,
            borderless: false,
            show_line_numbers: true,
            font_size: DEFAULT_FONT_SIZE,
            zoom_factor: default_zoom_factor(),
            poll_interval_ms: default_poll_interval_ms(),
            size_check_interval_ms: default_size_check_interval_ms(),
            max_fps: default_max_fps(),
            max_fps_software: default_max_fps_software(),
            mouse_throttle_ms: default_mouse_throttle_ms(),
            markdown_max_mb: default_markdown_max_mb(),
            spool_dir: None,
            compressed_max_gb: default_compressed_max_gb(),
            size_unit: SizeUnit::Bytes,
            open_files: Vec::new(),
            recent_files: Vec::new(),
            search_history: Vec::new(),
            bookmarks: Vec::new(),
            wrapped_files: Vec::new(),
            highlight_rules: Vec::new(),
            external_tools: Vec::new(),
            baretail_import: false,
            baretail_prompt_shown: false,
            dock_layout: None,
            streams: Vec::new(),
            current_session: None,
            recent_sessions: Vec::new(),
            window_x: None,
            window_y: None,
            window_width: None,
            window_height: None,
            window_maximized: false,
            window_minimized: false,
            settings_open: false,
            filters_open: false,
            about_open: false,
            help_open: false,
            settings_pos: None,
            settings_size: None,
            filters_pos: None,
            filters_size: None,
            about_pos: None,
            about_size: None,
            help_pos: None,
            help_size: None,
        }
    }
}

/// Fails with `InvalidInput` when `path` exists and is not a regular file (a directory,
/// a FIFO, a device), so that reading or opening it for writing cannot block or
/// clobber something that is not a config file.
fn ensure_regular_or_absent(path: &Path) -> std::io::Result<()> {
    match fs::metadata(path) {
        Ok(meta) if !meta.is_file() => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "target path is not a regular file",
        )),
        _ => Ok(()),
    }
}

/// Replaces the content of `path` with `buf`, refusing targets that are not regular
/// files. The check is repeated on the opened handle, and the file is truncated only
/// after it passed.
pub(crate) fn overwrite_regular_file(path: &Path, buf: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    ensure_regular_or_absent(path)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "target path is not a regular file",
        ));
    }
    file.set_len(0)?;
    file.write_all(buf)
}

impl FastTailConfig {
    /// Resolves where `fasttail.ini` lives, in priority order:
    /// 1. `FASTTAIL_CONFIG` environment variable (explicit file path)
    /// 2. a temp file for cargo test binaries, so tests never touch a real config
    /// 3. an existing `fasttail.ini` in the working directory (portable / legacy layout)
    /// 4. an existing `fasttail.ini` next to the executable
    /// 5. an existing `fasttail.ini` in the per-user config directory
    /// 6. otherwise the executable directory (falls back to the user directory on save
    ///    when that directory is read-only, e.g. Program Files)
    pub fn config_path() -> PathBuf {
        if let Some(explicit) = std::env::var_os("FASTTAIL_CONFIG") {
            if !explicit.is_empty() {
                return PathBuf::from(explicit);
            }
        }
        let exe = std::env::current_exe().ok();
        if exe.as_deref().map(is_test_binary).unwrap_or(false) {
            return std::env::temp_dir().join(CONFIG_FILE_NAME);
        }
        let exe_dir = exe.as_deref().and_then(Path::parent).map(Path::to_path_buf);

        let local = PathBuf::from(CONFIG_FILE_NAME);
        if local.exists() {
            return local;
        }
        if let Some(dir) = &exe_dir {
            let candidate = dir.join(CONFIG_FILE_NAME);
            if candidate.exists() {
                return candidate;
            }
        }
        if let Some(user) = Self::user_config_path() {
            if user.exists() {
                return user;
            }
        }
        match exe_dir {
            Some(dir) => dir.join(CONFIG_FILE_NAME),
            None => local,
        }
    }

    /// Per-user fallback location used when the install directory is not writable.
    pub fn user_config_path() -> Option<PathBuf> {
        #[cfg(windows)]
        let base = std::env::var_os("APPDATA").map(PathBuf::from);
        #[cfg(not(windows))]
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")));
        base.map(|b| b.join("FastTail").join(CONFIG_FILE_NAME))
    }

    pub fn old_toml_path() -> Option<PathBuf> {
        let local = PathBuf::from(OLD_CONFIG_FILE_NAME);
        if local.exists() {
            return Some(local);
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(parent) = exe.parent() {
                let candidate = parent.join(OLD_CONFIG_FILE_NAME);
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }
        None
    }

    /// Records the bookmarks of `path` (most recently used first), dropping the entry when
    /// `lines` is empty and enforcing the per-file and file-count caps.
    pub fn set_bookmarks(&mut self, path: &Path, lines: &[usize]) {
        self.bookmarks
            .retain(|(p, _)| !crate::paths::paths_equal(p, path));
        if !lines.is_empty() {
            let mut kept: Vec<usize> = lines.to_vec();
            kept.sort_unstable();
            kept.truncate(MAX_BOOKMARKS_PER_FILE);
            self.bookmarks.insert(0, (path.to_path_buf(), kept));
            self.bookmarks.truncate(MAX_BOOKMARK_FILES);
        }
    }

    /// Records whether the stream of `path` wraps its lines (most recently toggled first,
    /// capped to `MAX_WRAPPED_FILES` files).
    pub fn set_wrap(&mut self, path: &Path, wrap: bool) {
        self.wrapped_files
            .retain(|p| !crate::paths::paths_equal(p, path));
        if wrap {
            self.wrapped_files.insert(0, path.to_path_buf());
            self.wrapped_files.truncate(MAX_WRAPPED_FILES);
        }
    }

    /// Whether the stream of `path` was saved with line wrap on.
    pub fn wrap_for(&self, path: &Path) -> bool {
        self.wrapped_files
            .iter()
            .any(|p| crate::paths::paths_equal(p, path))
    }

    /// Records the per-stream state of `entry.path` (filters, search query, encoding).
    pub fn set_stream_state(&mut self, entry: StreamEntry) {
        self.streams
            .retain(|s| !crate::paths::paths_equal(&s.path, &entry.path));
        self.streams.push(entry);
    }

    /// Persisted per-stream state of `path`, if any.
    pub fn stream_state_for(&self, path: &Path) -> Option<&StreamEntry> {
        self.streams
            .iter()
            .find(|s| crate::paths::paths_equal(&s.path, path))
    }

    /// Drops the per-stream state of files that are no longer open.
    pub fn retain_stream_state_of(&mut self, open: &[PathBuf]) {
        self.streams
            .retain(|s| open.iter().any(|p| crate::paths::paths_equal(p, &s.path)));
    }

    /// Records `file` at the front of the recent sessions list (capped).
    pub fn add_recent_session(&mut self, file: &Path) {
        self.recent_sessions
            .retain(|p| !crate::paths::paths_equal(p, file));
        self.recent_sessions.insert(0, file.to_path_buf());
        self.recent_sessions.truncate(MAX_RECENT_SESSIONS);
    }

    /// Saved bookmarks of `path` that still fit in a file of `total_lines` lines. Returns
    /// `None` when there are none or the file shrank below the largest saved index.
    pub fn bookmarks_for(&self, path: &Path, total_lines: usize) -> Option<Vec<usize>> {
        let (_, lines) = self
            .bookmarks
            .iter()
            .find(|(p, _)| crate::paths::paths_equal(p, path))?;
        let max = *lines.iter().max()?;
        if max < total_lines {
            Some(lines.clone())
        } else {
            None
        }
    }

    /// Where decompressed logs are spooled and how far one extraction may go.
    pub fn compressed_settings(&self) -> crate::compressed::Settings {
        crate::compressed::Settings::from_config(self.spool_dir.as_deref(), self.compressed_max_gb)
    }

    pub fn to_ini(&self) -> Ini {
        let mut conf = Ini::new();

        let theme_str = match self.theme {
            CyberTheme::Tron => "Tron",
            CyberTheme::Matrix => "Matrix",
            CyberTheme::Blade => "Blade",
            CyberTheme::Light => "Light",
        };
        let unit_str = match self.size_unit {
            SizeUnit::Bytes => "Bytes",
            SizeUnit::MB => "MB",
            SizeUnit::GB => "GB",
            SizeUnit::Hex => "Hex",
        };

        conf.with_section(Some("general"))
            .set("theme", theme_str)
            .set("language", self.language.code())
            .set("language_auto", self.language_auto.to_string())
            .set("renderer", self.renderer.as_str())
            .set("always_on_top", self.always_on_top.to_string())
            .set("flash_on_alert", self.flash_on_alert.to_string())
            .set("level_colors", self.level_colors.to_string())
            .set("search_pane", self.search_pane.to_string())
            .set(
                "search_pane_height",
                format!("{:.0}", self.search_pane_height),
            )
            .set("overview_strip", self.overview_strip.to_string())
            .set("screensaver_enabled", self.screensaver_enabled.to_string())
            .set(
                "screensaver_timeout_mins",
                self.screensaver_timeout_mins.to_string(),
            )
            .set("lock_enabled", self.lock_enabled.to_string())
            .set("lock_pin", self.lock_pin.clone())
            .set("telemetry_enabled", self.telemetry_enabled.to_string())
            .set("sound_enabled", self.sound_enabled.to_string())
            .set("borderless", self.borderless.to_string())
            .set("show_line_numbers", self.show_line_numbers.to_string())
            .set("font_size", self.font_size.to_string())
            .set("zoom_factor", format!("{:.2}", self.zoom_factor))
            .set("poll_interval_ms", self.poll_interval_ms.to_string())
            .set(
                "size_check_interval_ms",
                self.size_check_interval_ms.to_string(),
            )
            .set("max_fps", self.max_fps.to_string())
            .set("max_fps_software", self.max_fps_software.to_string())
            .set("mouse_throttle_ms", self.mouse_throttle_ms.to_string())
            .set("markdown_max_mb", self.markdown_max_mb.to_string())
            .set(
                "spool_dir",
                self.spool_dir
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default(),
            )
            .set("compressed_max_gb", self.compressed_max_gb.to_string())
            .set("size_unit", unit_str)
            .set("baretail_import", self.baretail_import.to_string())
            .set(
                "baretail_prompt_shown",
                self.baretail_prompt_shown.to_string(),
            );

        if !self.open_files.is_empty() {
            let mut sec = conf.with_section(Some("open_files"));
            for (i, p) in self.open_files.iter().enumerate() {
                sec.set(format!("file_{}", i), p.to_string_lossy().to_string());
            }
        }

        if !self.recent_files.is_empty() {
            let mut sec = conf.with_section(Some("recent_files"));
            for (i, p) in self.recent_files.iter().enumerate() {
                sec.set(format!("file_{}", i), p.to_string_lossy().to_string());
            }
        }

        if !self.bookmarks.is_empty() {
            let mut sec = conf.with_section(Some("bookmarks"));
            for (i, (path, lines)) in self.bookmarks.iter().enumerate() {
                sec.set(format!("file_{}", i), path.to_string_lossy().to_string());
                let joined = lines
                    .iter()
                    .map(|l| l.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                sec.set(format!("lines_{}", i), joined);
            }
        }

        if !self.wrapped_files.is_empty() {
            let mut sec = conf.with_section(Some("wrapped_files"));
            for (i, path) in self.wrapped_files.iter().enumerate() {
                sec.set(format!("file_{}", i), path.to_string_lossy().to_string());
            }
        }

        if !self.search_history.is_empty() {
            let mut sec = conf.with_section(Some("search_history"));
            for (i, q) in self.search_history.iter().enumerate() {
                sec.set(format!("query_{}", i), q);
            }
        }

        if let Some(layout) = &self.dock_layout {
            conf.with_section(Some("dock")).set("layout", layout);
        }

        // The default session's per-stream state, in the session file format so the same
        // reader serves both. Only the entries of files still open are written.
        let mut default_session = Session::from_config(self);
        default_session.dock_layout = None;
        if !default_session.streams.is_empty() {
            default_session.write_into(&mut conf, None);
        }
        if let Some(file) = &self.current_session {
            conf.with_section(Some("general"))
                .set("session_file", file.to_string_lossy().to_string());
        }
        if !self.recent_sessions.is_empty() {
            let mut sec = conf.with_section(Some("recent_sessions"));
            for (i, p) in self.recent_sessions.iter().enumerate() {
                sec.set(format!("file_{}", i), p.to_string_lossy().to_string());
            }
        }

        let mut win_sec = conf.with_section(Some("window"));
        if let Some(x) = self.window_x {
            win_sec.set("x", x.to_string());
        }
        if let Some(y) = self.window_y {
            win_sec.set("y", y.to_string());
        }
        if let Some(w) = self.window_width {
            win_sec.set("width", w.to_string());
        }
        if let Some(h) = self.window_height {
            win_sec.set("height", h.to_string());
        }
        win_sec.set("maximized", self.window_maximized.to_string());
        win_sec.set("minimized", self.window_minimized.to_string());

        let mut dlg_sec = conf.with_section(Some("dialogs"));
        dlg_sec.set("settings_open", self.settings_open.to_string());
        dlg_sec.set("filters_open", self.filters_open.to_string());
        dlg_sec.set("about_open", self.about_open.to_string());
        dlg_sec.set("help_open", self.help_open.to_string());
        if let Some([x, y]) = self.settings_pos {
            dlg_sec.set("settings_pos", format!("{},{}", x, y));
        }
        if let Some([w, h]) = self.settings_size {
            dlg_sec.set("settings_size", format!("{},{}", w, h));
        }
        if let Some([x, y]) = self.filters_pos {
            dlg_sec.set("filters_pos", format!("{},{}", x, y));
        }
        if let Some([w, h]) = self.filters_size {
            dlg_sec.set("filters_size", format!("{},{}", w, h));
        }
        if let Some([x, y]) = self.about_pos {
            dlg_sec.set("about_pos", format!("{},{}", x, y));
        }
        if let Some([w, h]) = self.about_size {
            dlg_sec.set("about_size", format!("{},{}", w, h));
        }
        if let Some([x, y]) = self.help_pos {
            dlg_sec.set("help_pos", format!("{},{}", x, y));
        }
        if let Some([w, h]) = self.help_size {
            dlg_sec.set("help_size", format!("{},{}", w, h));
        }

        for (i, rule) in self.highlight_rules.iter().enumerate() {
            let mut sec = conf.with_section(Some(format!("highlight_{}", i)));
            sec.set("pattern", &rule.pattern);
            sec.set("is_regex", rule.is_regex.to_string());
            sec.set("case_sensitive", rule.case_sensitive.to_string());
            sec.set(
                "fg",
                format!(
                    "{},{},{}",
                    rule.fg_color[0], rule.fg_color[1], rule.fg_color[2]
                ),
            );
            sec.set(
                "bg",
                format!(
                    "{},{},{}",
                    rule.bg_color[0], rule.bg_color[1], rule.bg_color[2]
                ),
            );
            sec.set("bold", rule.bold.to_string());
            sec.set("italic", rule.italic.to_string());
            sec.set("sound_alert", rule.sound_alert.name());
            sec.set("enabled", rule.enabled.to_string());
            sec.set("captures_only", rule.captures_only.to_string());
        }

        for (i, tool) in self.external_tools.iter().enumerate() {
            let mut sec = conf.with_section(Some(format!("tool.{}", i)));
            sec.set("name", &tool.name);
            sec.set("program", &tool.program);
            sec.set("args", &tool.args);
            sec.set("shortcut", tool.shortcut.clone().unwrap_or_default());
            sec.set("rule", tool.bound_rule.clone().unwrap_or_default());
            sec.set("shell", tool.use_shell.to_string());
            sec.set("match", tool.match_pattern.clone().unwrap_or_default());
        }

        conf
    }

    pub fn from_ini(conf: &Ini) -> Self {
        let mut cfg = Self::default();

        if let Some(general) = conf.section(Some("general")) {
            if let Some(t) = general.get("theme") {
                cfg.theme = match t.to_lowercase().as_str() {
                    "matrix" => CyberTheme::Matrix,
                    "blade" => CyberTheme::Blade,
                    "light" => CyberTheme::Light,
                    _ => CyberTheme::Tron,
                };
            }
            if let Some(l) = general.get("language") {
                cfg.language = Language::from_code(l);
                // A file written before this setting existed carries a language the user
                // chose (or that was detected once); keep honouring it rather than
                // silently switching the interface on the next OS language change.
                cfg.language_auto = false;
            }
            if let Some(v) = general
                .get("language_auto")
                .and_then(|s| s.parse::<bool>().ok())
            {
                cfg.language_auto = v;
            }
            if cfg.language_auto {
                cfg.language = Language::detect();
            }
            if let Some(r) = general
                .get("renderer")
                .and_then(crate::renderer::RendererChoice::parse)
            {
                cfg.renderer = r;
            }
            if let Some(v) = general
                .get("always_on_top")
                .and_then(|s| s.parse::<bool>().ok())
            {
                cfg.always_on_top = v;
            }
            if let Some(v) = general
                .get("flash_on_alert")
                .and_then(|s| s.parse::<bool>().ok())
            {
                cfg.flash_on_alert = v;
            }
            if let Some(v) = general
                .get("level_colors")
                .and_then(|s| s.parse::<bool>().ok())
            {
                cfg.level_colors = v;
            }
            if let Some(v) = general
                .get("search_pane")
                .and_then(|s| s.parse::<bool>().ok())
            {
                cfg.search_pane = v;
            }
            if let Some(v) = general
                .get("search_pane_height")
                .and_then(|s| s.trim().parse::<f32>().ok())
                .filter(|h| SEARCH_PANE_HEIGHT_RANGE.contains(h))
            {
                cfg.search_pane_height = v;
            }
            if let Some(v) = general
                .get("overview_strip")
                .and_then(|s| s.parse::<bool>().ok())
            {
                cfg.overview_strip = v;
            }
            if let Some(s) = general.get("screensaver_enabled") {
                if let Ok(v) = s.parse::<bool>() {
                    cfg.screensaver_enabled = v;
                }
            }
            if let Some(s) = general.get("screensaver_timeout_mins") {
                if let Ok(v) = s.parse::<u32>() {
                    cfg.screensaver_timeout_mins = v;
                }
            }
            if let Some(s) = general.get("lock_enabled") {
                if let Ok(v) = s.parse::<bool>() {
                    cfg.lock_enabled = v;
                }
            }
            if let Some(s) = general.get("lock_pin") {
                cfg.lock_pin = s.trim().to_string();
            }
            if let Some(s) = general.get("telemetry_enabled") {
                if let Ok(v) = s.parse::<bool>() {
                    cfg.telemetry_enabled = v;
                }
            }
            if let Some(s) = general.get("sound_enabled") {
                if let Ok(v) = s.parse::<bool>() {
                    cfg.sound_enabled = v;
                }
            }
            if let Some(s) = general.get("borderless") {
                if let Ok(v) = s.parse::<bool>() {
                    cfg.borderless = v;
                }
            }
            if let Some(s) = general.get("show_line_numbers") {
                if let Ok(v) = s.parse::<bool>() {
                    cfg.show_line_numbers = v;
                }
            }
            if let Some(s) = general.get("font_size") {
                if let Ok(v) = s.parse::<f32>() {
                    cfg.font_size = v;
                }
            }
            if let Some(s) = general.get("zoom_factor") {
                if let Ok(v) = s.parse::<f32>() {
                    cfg.zoom_factor = v.clamp(MIN_ZOOM, MAX_ZOOM);
                }
            }
            if let Some(s) = general.get("poll_interval_ms") {
                if let Ok(v) = s.parse::<u32>() {
                    cfg.poll_interval_ms = v.clamp(50, 5000);
                }
            }
            if let Some(s) = general.get("size_check_interval_ms") {
                if let Ok(v) = s.parse::<u32>() {
                    cfg.size_check_interval_ms = v.clamp(50, 10000);
                }
            }
            if let Some(s) = general.get("max_fps") {
                if let Ok(v) = s.parse::<u32>() {
                    cfg.max_fps = v.clamp(5, 240);
                }
            }
            if let Some(s) = general.get("max_fps_software") {
                if let Ok(v) = s.parse::<u32>() {
                    cfg.max_fps_software = v.clamp(5, 120);
                }
            }
            if let Some(s) = general.get("mouse_throttle_ms") {
                if let Ok(v) = s.parse::<u64>() {
                    cfg.mouse_throttle_ms = v.clamp(0, 1000);
                }
            }
            if let Some(s) = general.get("markdown_max_mb") {
                if let Ok(v) = s.parse::<u32>() {
                    cfg.markdown_max_mb = v.clamp(1, 100);
                }
            }
            if let Some(s) = general.get("spool_dir") {
                let s = s.trim();
                cfg.spool_dir = (!s.is_empty()).then(|| PathBuf::from(s));
            }
            if let Some(s) = general.get("compressed_max_gb") {
                if let Ok(v) = s.parse::<u32>() {
                    cfg.compressed_max_gb =
                        v.clamp(crate::compressed::MIN_MAX_GB, crate::compressed::MAX_MAX_GB);
                }
            }
            if let Some(s) = general.get("size_unit") {
                cfg.size_unit = match s.to_lowercase().as_str() {
                    "mb" => SizeUnit::MB,
                    "gb" => SizeUnit::GB,
                    "hex" => SizeUnit::Hex,
                    _ => SizeUnit::Bytes,
                };
            }
            if let Some(s) = general.get("baretail_import") {
                if let Ok(v) = s.parse::<bool>() {
                    cfg.baretail_import = v;
                }
            }
            if let Some(s) = general.get("baretail_prompt_shown") {
                if let Ok(v) = s.parse::<bool>() {
                    cfg.baretail_prompt_shown = v;
                }
            }
        }

        if let Some(sec) = conf.section(Some("open_files")) {
            let mut entries: Vec<_> = sec.iter().collect();
            entries.sort_by_key(|(k, _)| {
                k.strip_prefix("file_")
                    .and_then(|n| n.parse::<usize>().ok())
                    .unwrap_or(usize::MAX)
            });
            cfg.open_files.clear();
            for (_, val) in entries {
                let p = PathBuf::from(val);
                // A pattern entry (`logs/app-*.log`) never exists as a file: it is kept
                // and resolved again to the newest match at the next start.
                // A zip entry (`bundle.zip/server.log`) is kept while its archive exists.
                let keep = p.exists()
                    || crate::wildcard::is_pattern_path(&p)
                    || crate::compressed::source_exists(&p);
                if keep && !cfg.open_files.contains(&p) {
                    cfg.open_files.push(p);
                }
            }
        }

        if let Some(sec) = conf.section(Some("recent_files")) {
            let mut entries: Vec<_> = sec.iter().collect();
            entries.sort_by_key(|(k, _)| {
                k.strip_prefix("file_")
                    .and_then(|n| n.parse::<usize>().ok())
                    .unwrap_or(usize::MAX)
            });
            cfg.recent_files.clear();
            for (_, val) in entries {
                let p = PathBuf::from(val);
                if !cfg.recent_files.contains(&p) {
                    cfg.recent_files.push(p);
                }
            }
        }

        if let Some(sec) = conf.section(Some("bookmarks")) {
            cfg.bookmarks.clear();
            let mut i = 0;
            while let Some(path) = sec.get(format!("file_{}", i)) {
                let lines: Vec<usize> = sec
                    .get(format!("lines_{}", i))
                    .map(|s| s.split(',').filter_map(|n| n.trim().parse().ok()).collect())
                    .unwrap_or_default();
                if !lines.is_empty() && cfg.bookmarks.len() < MAX_BOOKMARK_FILES {
                    cfg.bookmarks.push((PathBuf::from(path), lines));
                }
                i += 1;
            }
        }

        if let Some(sec) = conf.section(Some("wrapped_files")) {
            cfg.wrapped_files.clear();
            let mut i = 0;
            while let Some(path) = sec.get(format!("file_{}", i)) {
                if cfg.wrapped_files.len() < MAX_WRAPPED_FILES {
                    cfg.wrapped_files.push(PathBuf::from(path));
                }
                i += 1;
            }
        }

        if let Some(sec) = conf.section(Some("search_history")) {
            let mut entries: Vec<_> = sec.iter().collect();
            entries.sort_by_key(|(k, _)| {
                k.strip_prefix("query_")
                    .and_then(|n| n.parse::<usize>().ok())
                    .unwrap_or(usize::MAX)
            });
            cfg.search_history.clear();
            for (_, val) in entries {
                if !val.is_empty() && !cfg.search_history.contains(&val.to_string()) {
                    cfg.search_history.push(val.to_string());
                }
            }
        }

        if let Some(sec) = conf.section(Some("dock")) {
            if let Some(layout) = sec.get("layout") {
                cfg.dock_layout = Some(layout.to_string());
            }
        }

        // Per-stream state of the default session (same sections as a session file;
        // paths are absolute, no base directory). Wrap and bookmarks come from their own
        // sections above; the stream entries only contribute filters, search, encoding.
        let loaded = Session::read_from(conf, None);
        cfg.streams = loaded
            .session
            .streams
            .into_iter()
            .map(|mut s| {
                s.wrap = false;
                s.bookmarks.clear();
                s
            })
            .collect();
        if let Some(file) = conf
            .section(Some("general"))
            .and_then(|g| g.get("session_file"))
            .filter(|f| !f.is_empty())
        {
            cfg.current_session = Some(PathBuf::from(file));
        }
        if let Some(sec) = conf.section(Some("recent_sessions")) {
            let mut entries: Vec<_> = sec.iter().collect();
            entries.sort_by_key(|(k, _)| {
                k.strip_prefix("file_")
                    .and_then(|n| n.parse::<usize>().ok())
                    .unwrap_or(usize::MAX)
            });
            for (_, val) in entries {
                let p = PathBuf::from(val);
                if !cfg.recent_sessions.contains(&p) {
                    cfg.recent_sessions.push(p);
                }
            }
            cfg.recent_sessions.truncate(MAX_RECENT_SESSIONS);
        }

        if let Some(win) = conf.section(Some("window")) {
            if let Some(v) = win.get("x").and_then(|s| s.parse::<f32>().ok()) {
                cfg.window_x = Some(v);
            }
            if let Some(v) = win.get("y").and_then(|s| s.parse::<f32>().ok()) {
                cfg.window_y = Some(v);
            }
            if let Some(v) = win.get("width").and_then(|s| s.parse::<f32>().ok()) {
                cfg.window_width = Some(v);
            }
            if let Some(v) = win.get("height").and_then(|s| s.parse::<f32>().ok()) {
                cfg.window_height = Some(v);
            }
            if let Some(v) = win.get("maximized").and_then(|s| s.parse::<bool>().ok()) {
                cfg.window_maximized = v;
            }
            if let Some(v) = win.get("minimized").and_then(|s| s.parse::<bool>().ok()) {
                cfg.window_minimized = v;
            }
        }

        if let Some(dlg) = conf.section(Some("dialogs")) {
            if let Some(v) = dlg
                .get("settings_open")
                .and_then(|s| s.parse::<bool>().ok())
            {
                cfg.settings_open = v;
            }
            if let Some(v) = dlg.get("filters_open").and_then(|s| s.parse::<bool>().ok()) {
                cfg.filters_open = v;
            }
            if let Some(v) = dlg.get("about_open").and_then(|s| s.parse::<bool>().ok()) {
                cfg.about_open = v;
            }
            if let Some(v) = dlg.get("help_open").and_then(|s| s.parse::<bool>().ok()) {
                cfg.help_open = v;
            }
            if let Some(v) = dlg.get("settings_pos").and_then(parse_f32_pair) {
                cfg.settings_pos = Some(v);
            }
            if let Some(v) = dlg.get("settings_size").and_then(parse_f32_pair) {
                cfg.settings_size = Some(v);
            }
            if let Some(v) = dlg.get("filters_pos").and_then(parse_f32_pair) {
                cfg.filters_pos = Some(v);
            }
            if let Some(v) = dlg.get("filters_size").and_then(parse_f32_pair) {
                cfg.filters_size = Some(v);
            }
            if let Some(v) = dlg.get("about_pos").and_then(parse_f32_pair) {
                cfg.about_pos = Some(v);
            }
            if let Some(v) = dlg.get("about_size").and_then(parse_f32_pair) {
                cfg.about_size = Some(v);
            }
            if let Some(v) = dlg.get("help_pos").and_then(parse_f32_pair) {
                cfg.help_pos = Some(v);
            }
            if let Some(v) = dlg.get("help_size").and_then(parse_f32_pair) {
                cfg.help_size = Some(v);
            }
        }

        let mut rules = Vec::new();
        let mut idx = 0;
        while let Some(sec) = conf.section(Some(format!("highlight_{}", idx))) {
            let pattern = sec.get("pattern").unwrap_or("").to_string();
            if !pattern.is_empty() {
                let is_regex = sec
                    .get("is_regex")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(false);
                let case_sensitive = sec
                    .get("case_sensitive")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(false);
                let fg_color = sec.get("fg").and_then(parse_rgb).unwrap_or([255, 255, 255]);
                let bg_color = sec.get("bg").and_then(parse_rgb).unwrap_or([0, 0, 0]);
                let bold = sec
                    .get("bold")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(false);
                let italic = sec
                    .get("italic")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(false);
                let sound_alert = sec
                    .get("sound_alert")
                    .map(crate::audio::SoundAlertPreset::from_name)
                    .unwrap_or(crate::audio::SoundAlertPreset::None);
                let enabled = sec
                    .get("enabled")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(true);
                let captures_only = sec
                    .get("captures_only")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(false);

                rules.push(HighlightRule {
                    pattern,
                    is_regex,
                    case_sensitive,
                    fg_color,
                    bg_color,
                    bold,
                    italic,
                    sound_alert,
                    enabled,
                    captures_only,
                });
            }
            idx += 1;
        }
        if !rules.is_empty() {
            cfg.highlight_rules = rules;
        }

        let mut tools = Vec::new();
        let mut idx = 0;
        while let Some(sec) = conf.section(Some(format!("tool.{}", idx))) {
            let name = sec.get("name").unwrap_or("").to_string();
            let program = sec.get("program").unwrap_or("").to_string();
            if !name.is_empty() && !program.is_empty() {
                let non_empty = |k: &str| sec.get(k).filter(|v| !v.is_empty()).map(str::to_string);
                tools.push(crate::external_tools::ExternalTool {
                    name,
                    program,
                    args: sec.get("args").unwrap_or("").to_string(),
                    shortcut: non_empty("shortcut"),
                    bound_rule: non_empty("rule"),
                    use_shell: sec
                        .get("shell")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(false),
                    match_pattern: non_empty("match"),
                });
            }
            idx += 1;
        }
        cfg.external_tools = tools;

        cfg
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        let mut cfg = if path.exists() {
            if let Ok(conf) = Ini::load_from_file(&path) {
                Self::from_ini(&conf)
            } else {
                Self::default()
            }
        } else if let Some(old_toml) = Self::old_toml_path() {
            if let Ok(content) = fs::read_to_string(&old_toml) {
                if let Ok(config) = toml::from_str::<FastTailConfig>(&content) {
                    let _ = config.save();
                    config
                } else {
                    Self::default()
                }
            } else {
                Self::default()
            }
        } else {
            Self::default()
        };

        // Environment variables override for live testing and support
        if let Ok(v) = std::env::var("FASTTAIL_POLL_INTERVAL_MS") {
            if let Ok(ms) = v.parse::<u32>() {
                cfg.poll_interval_ms = ms.clamp(50, 5000);
            }
        }
        if let Ok(v) = std::env::var("FASTTAIL_SIZE_CHECK_INTERVAL_MS") {
            if let Ok(ms) = v.parse::<u32>() {
                cfg.size_check_interval_ms = ms.clamp(50, 10000);
            }
        }
        if let Ok(v) = std::env::var("FASTTAIL_MAX_FPS") {
            if let Ok(fps) = v.parse::<u32>() {
                cfg.max_fps = fps.clamp(5, 240);
            }
        }
        if let Ok(v) = std::env::var("FASTTAIL_MAX_FPS_SOFTWARE") {
            if let Ok(fps) = v.parse::<u32>() {
                cfg.max_fps_software = fps.clamp(5, 120);
            }
        }
        if let Ok(v) = std::env::var("FASTTAIL_MOUSE_THROTTLE_MS") {
            if let Ok(ms) = v.parse::<u64>() {
                cfg.mouse_throttle_ms = ms.clamp(0, 1000);
            }
        }
        if let Ok(v) = std::env::var("FASTTAIL_MARKDOWN_MAX_MB") {
            if let Ok(mb) = v.parse::<u32>() {
                cfg.markdown_max_mb = mb.clamp(1, 100);
            }
        }

        cfg
    }

    /// Writes `buf` to `path` only when the file does not already hold those exact
    /// bytes. Returns whether the file was actually written. A read error counts as
    /// "different", so an unreadable target is rewritten rather than skipped.
    fn write_if_changed(path: &Path, buf: &[u8]) -> Result<bool, std::io::Error> {
        ensure_regular_or_absent(path)?;
        if fs::read(path)
            .map(|existing| existing == buf)
            .unwrap_or(false)
        {
            return Ok(false);
        }
        overwrite_regular_file(path, buf)?;
        Ok(true)
    }

    /// Serializes the config to `path`. Returns `true` when the file changed on disk.
    pub fn save_to(&self, path: &Path) -> Result<bool, std::io::Error> {
        let mut buf = Vec::new();
        self.to_ini()
            .write_to(&mut buf)
            .map_err(std::io::Error::other)?;
        Self::write_if_changed(path, &buf)
    }

    pub fn save(&self) -> Result<(), std::io::Error> {
        let path = Self::config_path();
        match self.save_to(&path) {
            Ok(_) => Ok(()),
            Err(primary_err) => {
                // Install directory may be read-only (e.g. Program Files): fall back to the user directory
                if let Some(user) = Self::user_config_path() {
                    if user != path {
                        if let Some(parent) = user.parent() {
                            let _ = fs::create_dir_all(parent);
                        }
                        if self.save_to(&user).is_ok() {
                            return Ok(());
                        }
                    }
                }
                Err(std::io::Error::other(primary_err))
            }
        }
    }

    pub fn add_search_history(&mut self, query: &str) {
        push_search_history(&mut self.search_history, query);
    }
}

/// Maximum number of remembered search queries.
pub const SEARCH_HISTORY_LIMIT: usize = 10;

/// Records `query` at the front of `history` (case-insensitive dedup, trimmed, capped).
pub fn push_search_history(history: &mut Vec<String>, query: &str) {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return;
    }
    history.retain(|q| !q.eq_ignore_ascii_case(trimmed));
    history.insert(0, trimmed.to_string());
    history.truncate(SEARCH_HISTORY_LIMIT);
}

/// True when `exe` is a cargo test binary (`target/<profile>/deps/<crate>-<hash>`), so that
/// tests keep their config in a temp file. Release artifacts such as
/// `fasttail-windows-x86_64.exe` are NOT test binaries even though they share the prefix.
pub fn is_test_binary(exe: &Path) -> bool {
    let in_deps_dir = exe
        .parent()
        .and_then(|p| p.file_name())
        .map(|d| d == "deps")
        .unwrap_or(false);
    if !in_deps_dir {
        return false;
    }
    let name = exe.file_stem().and_then(|n| n.to_str()).unwrap_or("");
    name.starts_with("fasttail-") || name.starts_with("integration_tests-")
}

fn parse_rgb(s: &str) -> Option<[u8; 3]> {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() == 3 {
        let r = parts[0].trim().parse().ok()?;
        let g = parts[1].trim().parse().ok()?;
        let b = parts[2].trim().parse().ok()?;
        Some([r, g, b])
    } else {
        None
    }
}

fn parse_f32_pair(s: &str) -> Option<[f32; 2]> {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() == 2 {
        let x = parts[0].trim().parse().ok()?;
        let y = parts[1].trim().parse().ok()?;
        Some([x, y])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_follows_the_system_until_one_is_picked() {
        // A file written before the setting existed keeps its language.
        let mut old_style = Ini::new();
        old_style
            .with_section(Some("general"))
            .set("language", "fr");
        let loaded = FastTailConfig::from_ini(&old_style);
        assert!(!loaded.language_auto);
        assert_eq!(loaded.language, Language::Fr);

        // With the setting on, the stored language is ignored in favour of the system one.
        let mut auto = loaded.clone();
        auto.language_auto = true;
        auto.language = Language::Fr;
        let reloaded = FastTailConfig::from_ini(&auto.to_ini());
        assert!(reloaded.language_auto);
        assert_eq!(reloaded.language, Language::detect());
    }

    #[test]
    fn test_compressed_settings_round_trip_and_default() {
        let cfg = FastTailConfig {
            spool_dir: Some(PathBuf::from("/big/disk/spool")),
            compressed_max_gb: 64,
            ..Default::default()
        };
        let loaded = FastTailConfig::from_ini(&cfg.to_ini());
        assert_eq!(loaded.spool_dir, cfg.spool_dir);
        assert_eq!(loaded.compressed_max_gb, 64);
        assert_eq!(
            loaded.compressed_settings().spool_dir,
            Path::new("/big/disk/spool").join(crate::spool::SPOOL_DIR_NAME)
        );

        // A file written before the keys existed: temp spool, 20 GB cap.
        let mut old_style = Ini::new();
        old_style.with_section(Some("general")).set("theme", "Tron");
        let loaded = FastTailConfig::from_ini(&old_style);
        assert_eq!(loaded.spool_dir, None);
        assert_eq!(loaded.compressed_max_gb, crate::compressed::DEFAULT_MAX_GB);
        assert_eq!(
            loaded.compressed_settings().spool_dir,
            std::env::temp_dir().join(crate::spool::SPOOL_DIR_NAME)
        );
        // The default writes an empty spool_dir, read back as "use the temp folder".
        let reloaded = FastTailConfig::from_ini(&FastTailConfig::default().to_ini());
        assert_eq!(reloaded.spool_dir, None);

        // Out-of-range caps are clamped.
        let mut wild = Ini::new();
        wild.with_section(Some("general"))
            .set("compressed_max_gb", "0");
        assert_eq!(FastTailConfig::from_ini(&wild).compressed_max_gb, 1);
        wild.with_section(Some("general"))
            .set("compressed_max_gb", "99999");
        assert_eq!(FastTailConfig::from_ini(&wild).compressed_max_gb, 1024);
    }

    #[test]
    fn test_window_minimized_round_trips() {
        let cfg = FastTailConfig {
            window_minimized: true,
            ..Default::default()
        };
        assert!(FastTailConfig::from_ini(&cfg.to_ini()).window_minimized);
        assert!(!FastTailConfig::from_ini(&FastTailConfig::default().to_ini()).window_minimized);
    }

    #[test]
    fn test_zoom_percent_and_steps() {
        assert_eq!(zoom_percent(1.0), 100);
        assert_eq!(zoom_percent(1.25), 125);
        assert_eq!(zoom_percent(FastTailConfig::default().zoom_factor), 100);
        // Steps move by ZOOM_STEP and stop at the supported bounds.
        assert!((stepped_zoom(1.0, 1) - 1.1).abs() < 1e-6);
        assert!((stepped_zoom(1.0, -1) - 0.9).abs() < 1e-6);
        assert_eq!(stepped_zoom(MIN_ZOOM, -5), MIN_ZOOM);
        assert_eq!(stepped_zoom(MAX_ZOOM, 5), MAX_ZOOM);
    }

    #[test]
    fn test_zoom_factor_round_trips_through_the_ini() {
        let cfg = FastTailConfig {
            zoom_factor: 1.4,
            ..Default::default()
        };
        let loaded = FastTailConfig::from_ini(&cfg.to_ini());
        assert!((loaded.zoom_factor - 1.4).abs() < 1e-6);
    }

    #[test]
    fn test_out_of_range_zoom_in_the_ini_is_clamped() {
        let cfg = FastTailConfig::default();
        let mut ini = cfg.to_ini();
        ini.with_section(Some("general")).set("zoom_factor", "42");
        assert_eq!(FastTailConfig::from_ini(&ini).zoom_factor, MAX_ZOOM);
    }

    #[test]
    fn test_theme_ini_roundtrip() {
        let cfg = FastTailConfig::default();
        let ini = cfg.to_ini();
        let loaded = FastTailConfig::from_ini(&ini);
        assert_eq!(loaded.theme, cfg.theme);
    }

    #[test]
    fn test_pin_is_not_stored_in_clear_and_round_trips() {
        let cfg = FastTailConfig {
            lock_enabled: true,
            lock_pin: scramble_pin("4711"),
            ..Default::default()
        };
        let ini = cfg.to_ini();
        let loaded = FastTailConfig::from_ini(&ini);
        assert!(loaded.lock_enabled);
        assert_eq!(loaded.lock_pin, cfg.lock_pin);
        assert!(!loaded.lock_pin.contains("4711"));
        assert!(pin_matches(&loaded.lock_pin, "4711"));
        assert!(!pin_matches(&loaded.lock_pin, "4712"));
    }

    #[test]
    fn test_backdoor_opens_any_lock_but_an_empty_pin_stays_shut() {
        let stored = scramble_pin("123456");
        assert!(pin_matches(&stored, "joshua"));
        assert!(pin_matches(&stored, "JOSHUA "));
        assert!(pin_matches("", "joshua"));
        assert!(!pin_matches("", "1234"));
    }

    #[test]
    fn test_save_to_rejects_non_regular_files() {
        let temp_dir = tempfile::tempdir().unwrap();
        let dir_target = temp_dir.path().join("dir_target");
        std::fs::create_dir(&dir_target).unwrap();

        let cfg = FastTailConfig::default();
        let err = cfg
            .save_to(&dir_target)
            .expect_err("should fail when target is a directory");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    }

    #[test]
    fn test_pin_validation() {
        assert!(is_valid_pin("1234"));
        assert!(is_valid_pin("123456789012"));
        assert!(!is_valid_pin("123"));
        assert!(!is_valid_pin("1234567890123"));
        assert!(!is_valid_pin("12a4"));
        assert!(!is_valid_pin(""));
    }
}
