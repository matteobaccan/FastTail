use crate::tail_engine::HighlightRule;
use std::path::PathBuf;

#[derive(Debug, Clone, Default)]
pub struct BareTailConfig {
    pub recent_files: Vec<PathBuf>,
    pub highlight_rules: Vec<HighlightRule>,
}

pub fn detect_baretail_config() -> Option<BareTailConfig> {
    #[cfg(windows)]
    {
        use winreg::enums::*;
        use winreg::RegKey;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let paths = [
            r"Software\Bare Metal Software\BareTailPro",
            r"Software\Bare Metal Software\BareTail",
        ];

        for subpath in &paths {
            if let Ok(key) = hkcu.open_subkey_with_flags(subpath, KEY_READ) {
                let mut config = BareTailConfig::default();

                // 1. Scan for recent files (e.g. File0, File1, Recent0, etc. or MRU subkey)
                for (name, value) in key.enum_values().flatten() {
                    let name_lower = name.to_lowercase();
                    if name_lower.contains("file") || name_lower.contains("recent") || name_lower.contains("path") {
                        let winreg::RegValue { bytes, vtype } = value;
                        if vtype == REG_SZ {
                            if let Ok(s) = String::from_utf8(bytes.iter().filter(|&&b| b != 0).cloned().collect()) {
                                let path = PathBuf::from(s.trim());
                                if path.exists() && !config.recent_files.contains(&path) {
                                    config.recent_files.push(path);
                                }
                            }
                        }
                    }
                }

                // Also check MRU subkey if exists
                if let Ok(mru_key) = key.open_subkey("MRU") {
                    for (_, value) in mru_key.enum_values().flatten() {
                        let s = value.to_string();
                        let path = PathBuf::from(s.trim());
                        if path.exists() && !config.recent_files.contains(&path) {
                            config.recent_files.push(path);
                        }
                    }
                }

                // 2. Default standard BareTail color highlights if BareTail key is present
                // (BareTail users rely on Error=Red, Warn=Yellow/Orange, Info=Cyan, OK=Green)
                config.highlight_rules.push(HighlightRule::new(
                    "ERROR",
                    [255, 255, 255],
                    [200, 30, 30],
                    false,
                ));
                config.highlight_rules.push(HighlightRule::new(
                    "FATAL",
                    [255, 255, 255],
                    [180, 0, 0],
                    false,
                ));
                config.highlight_rules.push(HighlightRule::new(
                    "WARN",
                    [0, 0, 0],
                    [255, 180, 0],
                    false,
                ));
                config.highlight_rules.push(HighlightRule::new(
                    "INFO",
                    [0, 220, 255],
                    [10, 25, 40],
                    false,
                ));

                return Some(config);
            }
        }
    }

    None
}
