use crate::i18n::Language;
use crate::tail_engine::HighlightRule;
use crate::theme::CyberTheme;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const CONFIG_FILE_NAME: &str = "fasttail.toml";

#[derive(Debug, Clone, Serialize, Deserialize)]
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
            recent_files: Vec::new(),
            highlight_rules: Vec::new(),
            baretail_prompt_shown: false,
            dock_layout: None,
        }
    }
}

impl FastTailConfig {
    pub fn config_path() -> PathBuf {
        Path::new(CONFIG_FILE_NAME).to_path_buf()
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(config) = toml::from_str(&content) {
                    return config;
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) -> Result<(), std::io::Error> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        fs::write(Self::config_path(), content)
    }
}
