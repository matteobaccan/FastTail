use crate::i18n::Language;
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FastTailConfig {
    pub theme: CyberTheme,
    pub language: Language,
    pub screensaver_enabled: bool,
    pub screensaver_timeout_mins: u32,
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
    #[serde(default)]
    pub borderless: bool,
    #[serde(default = "default_true")]
    pub show_line_numbers: bool,
    #[serde(default = "default_font_size")]
    pub font_size: f32,
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
    #[serde(default)]
    pub baretail_import: bool,
    pub baretail_prompt_shown: bool,
    #[serde(default)]
    pub dock_layout: Option<String>,
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

fn default_font_size() -> f32 {
    13.0
}

impl Default for FastTailConfig {
    fn default() -> Self {
        Self {
            theme: CyberTheme::Tron,
            language: Language::detect(),
            screensaver_enabled: true,
            screensaver_timeout_mins: 10,
            telemetry_enabled: true,
            sound_enabled: false,
            renderer: crate::renderer::RendererChoice::Auto,
            always_on_top: false,
            flash_on_alert: false,
            level_colors: true,
            borderless: false,
            show_line_numbers: true,
            font_size: 13.0,
            size_unit: SizeUnit::Bytes,
            open_files: Vec::new(),
            recent_files: Vec::new(),
            search_history: Vec::new(),
            bookmarks: Vec::new(),
            wrapped_files: Vec::new(),
            highlight_rules: Vec::new(),
            baretail_import: false,
            baretail_prompt_shown: false,
            dock_layout: None,
            window_x: None,
            window_y: None,
            window_width: None,
            window_height: None,
            window_maximized: false,
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
            .set("renderer", self.renderer.as_str())
            .set("always_on_top", self.always_on_top.to_string())
            .set("flash_on_alert", self.flash_on_alert.to_string())
            .set("level_colors", self.level_colors.to_string())
            .set("screensaver_enabled", self.screensaver_enabled.to_string())
            .set(
                "screensaver_timeout_mins",
                self.screensaver_timeout_mins.to_string(),
            )
            .set("telemetry_enabled", self.telemetry_enabled.to_string())
            .set("sound_enabled", self.sound_enabled.to_string())
            .set("borderless", self.borderless.to_string())
            .set("show_line_numbers", self.show_line_numbers.to_string())
            .set("font_size", self.font_size.to_string())
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
                if p.exists() && !cfg.open_files.contains(&p) {
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

        cfg
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            if let Ok(conf) = Ini::load_from_file(&path) {
                return Self::from_ini(&conf);
            }
        }

        if let Some(old_toml) = Self::old_toml_path() {
            if let Ok(content) = fs::read_to_string(&old_toml) {
                if let Ok(config) = toml::from_str::<FastTailConfig>(&content) {
                    let _ = config.save();
                    return config;
                }
            }
        }

        Self::default()
    }

    pub fn save(&self) -> Result<(), std::io::Error> {
        let path = Self::config_path();
        let conf = self.to_ini();
        match conf.write_to_file(&path) {
            Ok(()) => Ok(()),
            Err(primary_err) => {
                // Install directory may be read-only (e.g. Program Files): fall back to the user directory
                if let Some(user) = Self::user_config_path() {
                    if user != path {
                        if let Some(parent) = user.parent() {
                            let _ = fs::create_dir_all(parent);
                        }
                        if conf.write_to_file(&user).is_ok() {
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
