// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Colours: the theme's `Rgba` values mapped to ratatui colours, as 24-bit RGB when the
//! terminal renders it and as the nearest of the 16 basic colours otherwise.

use crate::color::Rgba;
use crate::log_level::LogLevel;
use crate::tail_engine::HighlightStyle;
use crate::theme::CyberTheme;
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::border;

/// How many colours the terminal can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorDepth {
    TrueColor,
    Ansi16,
}

/// Environment facts the colour depth is decided from, gathered by `from_env` so the
/// decision itself stays a pure function.
#[derive(Debug, Default, Clone)]
pub struct TermInfo {
    /// `FASTTAIL_TUI_COLORS`: `16` or `truecolor` forces the depth.
    pub forced: Option<String>,
    pub colorterm: Option<String>,
    pub term: Option<String>,
    pub term_program: Option<String>,
    /// `WT_SESSION` is set: running inside Windows Terminal.
    pub windows_terminal: bool,
    /// The Windows console accepted virtual-terminal sequences (conhost from Windows 10
    /// 1703 on renders 24-bit colour through them).
    pub windows_vt: bool,
    /// Running on Windows (the only platform where the console may lack box drawing).
    pub windows: bool,
    /// `FASTTAIL_TUI_ASCII` is set: plain `+-|` borders.
    pub forced_ascii: bool,
}

impl TermInfo {
    pub fn from_env() -> Self {
        let var = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
        #[cfg(windows)]
        let windows_vt = crossterm::ansi_support::supports_ansi();
        #[cfg(not(windows))]
        let windows_vt = false;
        Self {
            forced: var("FASTTAIL_TUI_COLORS"),
            colorterm: var("COLORTERM"),
            term: var("TERM"),
            term_program: var("TERM_PROGRAM"),
            windows_terminal: var("WT_SESSION").is_some(),
            windows_vt,
            windows: cfg!(windows),
            forced_ascii: var("FASTTAIL_TUI_ASCII").is_some(),
        }
    }

    /// Plain ASCII borders: asked for, or a Windows console without virtual-terminal
    /// support (the legacy conhost, whose raster fonts and OEM code page do not draw box
    /// characters reliably).
    pub fn ascii_borders(&self) -> bool {
        self.forced_ascii || (self.windows && !self.windows_vt && !self.windows_terminal)
    }

    pub fn depth(&self) -> ColorDepth {
        if let Some(forced) = &self.forced {
            return if forced.trim() == "16" {
                ColorDepth::Ansi16
            } else {
                ColorDepth::TrueColor
            };
        }
        let lower = |v: &Option<String>| v.as_deref().unwrap_or("").to_ascii_lowercase();
        let colorterm = lower(&self.colorterm);
        if colorterm.contains("truecolor") || colorterm.contains("24bit") {
            return ColorDepth::TrueColor;
        }
        if self.windows_terminal || self.windows_vt {
            return ColorDepth::TrueColor;
        }
        let program = lower(&self.term_program);
        if ["vscode", "wezterm", "iterm.app", "ghostty"]
            .iter()
            .any(|p| program == *p)
        {
            return ColorDepth::TrueColor;
        }
        let term = lower(&self.term);
        if term.contains("truecolor") || term.contains("24bit") || term.contains("direct") {
            return ColorDepth::TrueColor;
        }
        ColorDepth::Ansi16
    }
}

/// The 16 basic colours with the RGB values xterm uses for them, the reference the
/// nearest-colour fallback measures against.
const ANSI16: [(Color, [u8; 3]); 16] = [
    (Color::Black, [0, 0, 0]),
    (Color::Red, [205, 0, 0]),
    (Color::Green, [0, 205, 0]),
    (Color::Yellow, [205, 205, 0]),
    (Color::Blue, [0, 0, 238]),
    (Color::Magenta, [205, 0, 205]),
    (Color::Cyan, [0, 205, 205]),
    (Color::Gray, [229, 229, 229]),
    (Color::DarkGray, [127, 127, 127]),
    (Color::LightRed, [255, 0, 0]),
    (Color::LightGreen, [0, 255, 0]),
    (Color::LightYellow, [255, 255, 0]),
    (Color::LightBlue, [92, 92, 255]),
    (Color::LightMagenta, [255, 0, 255]),
    (Color::LightCyan, [0, 255, 255]),
    (Color::White, [255, 255, 255]),
];

/// The basic colour closest to `rgb` (squared Euclidean distance).
pub fn nearest_ansi16(rgb: [u8; 3]) -> Color {
    let dist = |c: &[u8; 3]| -> u32 {
        (0..3)
            .map(|i| {
                let d = rgb[i] as i32 - c[i] as i32;
                (d * d) as u32
            })
            .sum()
    };
    ANSI16
        .iter()
        .min_by_key(|(_, c)| dist(c))
        .map(|(color, _)| *color)
        .unwrap_or(Color::Reset)
}

/// `color` for a terminal of `depth`; a fully transparent colour means "the terminal's
/// default".
pub fn map_color(color: Rgba, depth: ColorDepth) -> Color {
    if color.a == 0 {
        return Color::Reset;
    }
    let rgb = [color.r, color.g, color.b];
    match depth {
        ColorDepth::TrueColor => Color::Rgb(rgb[0], rgb[1], rgb[2]),
        ColorDepth::Ansi16 => nearest_ansi16(rgb),
    }
}

/// Borders for terminals that cannot draw box characters.
pub const ASCII_BORDER: border::Set<'static> = border::Set {
    top_left: "+",
    top_right: "+",
    bottom_left: "+",
    bottom_right: "+",
    vertical_left: "|",
    vertical_right: "|",
    horizontal_top: "-",
    horizontal_bottom: "-",
};

/// The focused window in ASCII: `=` edges tell it apart from the others.
pub const ASCII_FOCUSED_BORDER: border::Set<'static> = border::Set {
    horizontal_top: "=",
    horizontal_bottom: "=",
    ..ASCII_BORDER
};

/// Which window a border belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chrome {
    /// The stream window with the keyboard focus.
    Focused,
    /// Another stream window, or the status bar.
    Plain,
    /// A dialog (prompt, help) drawn over the windows.
    Dialog,
}

/// Palette of one run: the theme's level colours plus the few UI colours the TUI uses,
/// and the border characters the terminal can draw.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub theme: CyberTheme,
    pub depth: ColorDepth,
    pub ascii: bool,
    /// The ini's `level_colors`: rows no rule matched are coloured by their level.
    pub level_colors: bool,
}

impl Palette {
    pub fn new(theme: CyberTheme, depth: ColorDepth, ascii: bool) -> Self {
        Self {
            theme,
            depth,
            ascii,
            level_colors: true,
        }
    }

    /// Whole-row style of a highlight rule: its foreground and background (always
    /// solid in the ini), bold and italic.
    pub fn rule_style(&self, rule: &HighlightStyle) -> Style {
        let mut style = Style::default().fg(self.map(rule.fg)).bg(self.map(rule.bg));
        if rule.bold {
            style = style.add_modifier(Modifier::BOLD);
        }
        if rule.italic {
            style = style.add_modifier(Modifier::ITALIC);
        }
        style
    }

    /// Border characters of a window: double for the focused stream, rounded for a
    /// dialog, single for the rest; `+-|` in ASCII mode.
    pub fn border_set(&self, chrome: Chrome) -> border::Set<'static> {
        match (self.ascii, chrome) {
            (true, Chrome::Focused) => ASCII_FOCUSED_BORDER,
            (true, _) => ASCII_BORDER,
            (false, Chrome::Focused) => border::DOUBLE,
            (false, Chrome::Dialog) => border::ROUNDED,
            (false, Chrome::Plain) => border::PLAIN,
        }
    }

    /// Border colour: the theme accent for the focused window and dialogs.
    pub fn border_style(&self, chrome: Chrome) -> Style {
        match chrome {
            Chrome::Focused | Chrome::Dialog => Style::default().fg(self.accent()),
            Chrome::Plain => Style::default().fg(self.dim()),
        }
    }

    fn map(&self, c: Rgba) -> Color {
        map_color(c, self.depth)
    }

    /// Style of a row of level `level`: the theme's level palette, or the terminal's
    /// default text for INFO and unknown levels.
    pub fn level_style(&self, level: LogLevel) -> Style {
        if !self.level_colors {
            return Style::default();
        }
        match self.theme.level_style(level) {
            Some(ls) => {
                let mut style = Style::default().fg(self.map(ls.fg));
                // A transparent background is left unset, so the row's own background
                // (the selection band) shows through.
                if ls.bg.a != 0 {
                    style = style.bg(self.map(ls.bg));
                }
                if ls.bold {
                    style = style.add_modifier(Modifier::BOLD);
                }
                style
            }
            None => Style::default(),
        }
    }

    pub fn level_color(&self, level: LogLevel) -> Color {
        self.map(self.theme.level_color(level))
    }

    pub fn accent(&self) -> Color {
        self.map(self.theme.accent_color())
    }

    pub fn dim(&self) -> Color {
        self.map(self.theme.text_dim())
    }

    /// Search hits inside the text: dark text on the warning yellow, readable on every
    /// level colour.
    pub fn hit(&self) -> Style {
        Style::default()
            .fg(Color::Black)
            .bg(self.map(self.theme.warn_color()))
    }

    /// Selected rows: a dark blue band under the text (plain blue in 16 colours).
    pub fn selection(&self) -> Style {
        let bg = match self.depth {
            ColorDepth::TrueColor => Color::Rgb(38, 62, 102),
            ColorDepth::Ansi16 => Color::Blue,
        };
        Style::default().bg(bg)
    }

    /// The current hit: reversed accent, so it stands out from the other hits.
    pub fn active_hit(&self) -> Style {
        Style::default()
            .fg(Color::Black)
            .bg(self.accent())
            .add_modifier(Modifier::BOLD)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truecolor_keeps_the_exact_rgb() {
        let c = Rgba::from_rgb(255, 51, 68);
        assert_eq!(map_color(c, ColorDepth::TrueColor), Color::Rgb(255, 51, 68));
    }

    #[test]
    fn transparent_maps_to_the_terminal_default() {
        assert_eq!(
            map_color(Rgba::TRANSPARENT, ColorDepth::TrueColor),
            Color::Reset
        );
        assert_eq!(
            map_color(Rgba::TRANSPARENT, ColorDepth::Ansi16),
            Color::Reset
        );
    }

    #[test]
    fn ansi16_fallback_picks_sensible_neighbours() {
        // The theme's error red, warning amber and fatal background.
        assert_eq!(nearest_ansi16([255, 51, 68]), Color::LightRed);
        assert_eq!(nearest_ansi16([255, 187, 0]), Color::Yellow);
        assert_eq!(nearest_ansi16([120, 16, 28]), Color::Red);
        assert_eq!(nearest_ansi16([0, 0, 0]), Color::Black);
        assert_eq!(nearest_ansi16([250, 250, 250]), Color::White);
    }

    #[test]
    fn every_level_of_every_theme_maps_without_panicking() {
        for theme in [
            CyberTheme::Tron,
            CyberTheme::Matrix,
            CyberTheme::Blade,
            CyberTheme::Light,
        ] {
            for depth in [ColorDepth::TrueColor, ColorDepth::Ansi16] {
                let p = Palette::new(theme, depth, false);
                for level in LogLevel::ALL {
                    let _ = p.level_style(level);
                }
                // INFO keeps the terminal's own colours.
                assert_eq!(p.level_style(LogLevel::Info), Style::default());
            }
        }
    }

    #[test]
    fn depth_detection() {
        let mut info = TermInfo::default();
        assert_eq!(info.depth(), ColorDepth::Ansi16);
        info.colorterm = Some("truecolor".into());
        assert_eq!(info.depth(), ColorDepth::TrueColor);
        info.forced = Some("16".into());
        assert_eq!(info.depth(), ColorDepth::Ansi16);
        let wt = TermInfo {
            windows_terminal: true,
            ..TermInfo::default()
        };
        assert_eq!(wt.depth(), ColorDepth::TrueColor);
        let xterm = TermInfo {
            term: Some("xterm-256color".into()),
            ..TermInfo::default()
        };
        assert_eq!(xterm.depth(), ColorDepth::Ansi16);
    }

    #[test]
    fn ascii_borders_on_a_legacy_console_or_on_request() {
        let legacy = TermInfo {
            windows: true,
            ..TermInfo::default()
        };
        assert!(legacy.ascii_borders());
        let vt = TermInfo {
            windows: true,
            windows_vt: true,
            ..TermInfo::default()
        };
        assert!(!vt.ascii_borders());
        let unix = TermInfo::default();
        assert!(!unix.ascii_borders());
        let forced = TermInfo {
            forced_ascii: true,
            ..TermInfo::default()
        };
        assert!(forced.ascii_borders());
        let p = Palette::new(CyberTheme::Tron, ColorDepth::Ansi16, true);
        assert_eq!(p.border_set(Chrome::Plain).top_left, "+");
        assert_eq!(p.border_set(Chrome::Focused).horizontal_top, "=");
        let p = Palette::new(CyberTheme::Tron, ColorDepth::Ansi16, false);
        assert_eq!(p.border_set(Chrome::Focused).top_left, "╔");
    }
}
