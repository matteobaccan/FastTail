use crate::i18n::Language;
use crate::tail_engine::{HighlightRule, SizeUnit};
use crate::theme::CyberTheme;
use ini::Ini;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const CONFIG_FILE_NAME: &str = "fasttail.ini";
const OLD_CONFIG_FILE_NAME: &str = "fasttail.toml";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FastTailConfig {
    pub theme: CyberTheme,
    pub language: Language,
    pub screensaver_enabled: bool,
    pub screensaver_timeout_mins: u32,
    pub telemetry_enabled: bool,
    pub sound_enabled: bool,
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
    pub highlight_rules: Vec<HighlightRule>,
    pub baretail_prompt_shown: bool,
    #[serde(default)]
    pub dock_layout: Option<String>,
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
            borderless: false,
            show_line_numbers: true,
            font_size: 13.0,
            size_unit: SizeUnit::Bytes,
            open_files: Vec::new(),
            recent_files: Vec::new(),
            highlight_rules: Vec::new(),
            baretail_prompt_shown: false,
            dock_layout: None,
        }
    }
}

impl FastTailConfig {
    pub fn config_path() -> PathBuf {
        let local = PathBuf::from(CONFIG_FILE_NAME);
        if local.exists() {
            return local;
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(parent) = exe.parent() {
                let candidate = parent.join(CONFIG_FILE_NAME);
                if candidate.exists() {
                    return candidate;
                }
            }
        }
        local
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

    pub fn to_ini(&self) -> Ini {
        let mut conf = Ini::new();

        let theme_str = match self.theme {
            CyberTheme::Tron => "Tron",
            CyberTheme::Matrix => "Matrix",
            CyberTheme::Blade => "Blade",
        };
        let unit_str = match self.size_unit {
            SizeUnit::Bytes => "Bytes",
            SizeUnit::MB => "MB",
            SizeUnit::GB => "GB",
        };

        conf.with_section(Some("general"))
            .set("theme", theme_str)
            .set("language", self.language.code())
            .set("screensaver_enabled", self.screensaver_enabled.to_string())
            .set("screensaver_timeout_mins", self.screensaver_timeout_mins.to_string())
            .set("telemetry_enabled", self.telemetry_enabled.to_string())
            .set("sound_enabled", self.sound_enabled.to_string())
            .set("borderless", self.borderless.to_string())
            .set("show_line_numbers", self.show_line_numbers.to_string())
            .set("font_size", self.font_size.to_string())
            .set("size_unit", unit_str)
            .set("baretail_prompt_shown", self.baretail_prompt_shown.to_string());

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

        if let Some(layout) = &self.dock_layout {
            conf.with_section(Some("dock")).set("layout", layout);
        }

        for (i, rule) in self.highlight_rules.iter().enumerate() {
            let mut sec = conf.with_section(Some(format!("highlight_{}", i)));
            sec.set("pattern", &rule.pattern);
            sec.set("is_regex", rule.is_regex.to_string());
            sec.set("case_sensitive", rule.case_sensitive.to_string());
            sec.set("fg", format!("{},{},{}", rule.fg_color[0], rule.fg_color[1], rule.fg_color[2]));
            sec.set("bg", format!("{},{},{}", rule.bg_color[0], rule.bg_color[1], rule.bg_color[2]));
            sec.set("bold", rule.bold.to_string());
            sec.set("italic", rule.italic.to_string());
            sec.set("enabled", rule.enabled.to_string());
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
                    _ => CyberTheme::Tron,
                };
            }
            if let Some(l) = general.get("language") {
                cfg.language = Language::from_code(l);
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
                    _ => SizeUnit::Bytes,
                };
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

        if let Some(sec) = conf.section(Some("dock")) {
            if let Some(layout) = sec.get("layout") {
                cfg.dock_layout = Some(layout.to_string());
            }
        }

        let mut rules = Vec::new();
        let mut idx = 0;
        while let Some(sec) = conf.section(Some(format!("highlight_{}", idx))) {
            let pattern = sec.get("pattern").unwrap_or("").to_string();
            if !pattern.is_empty() {
                let is_regex = sec.get("is_regex").and_then(|v| v.parse().ok()).unwrap_or(false);
                let case_sensitive = sec.get("case_sensitive").and_then(|v| v.parse().ok()).unwrap_or(false);
                let fg_color = sec.get("fg").and_then(parse_rgb).unwrap_or([255, 255, 255]);
                let bg_color = sec.get("bg").and_then(parse_rgb).unwrap_or([0, 0, 0]);
                let bold = sec.get("bold").and_then(|v| v.parse().ok()).unwrap_or(false);
                let italic = sec.get("italic").and_then(|v| v.parse().ok()).unwrap_or(false);
                let enabled = sec.get("enabled").and_then(|v| v.parse().ok()).unwrap_or(true);

                rules.push(HighlightRule {
                    pattern,
                    is_regex,
                    case_sensitive,
                    fg_color,
                    bg_color,
                    bold,
                    italic,
                    sound_alert: crate::audio::SoundAlertPreset::None,
                    enabled,
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
        conf.write_to_file(&path)
            .map_err(std::io::Error::other)
    }
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
