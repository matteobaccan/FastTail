// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use crate::color::Rgba;
use crate::log_level::LogLevel;
use serde::{Deserialize, Serialize};

/// Style of a row coloured by its detected log level (see `CyberTheme::level_style`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LevelStyle {
    pub fg: Rgba,
    /// `Rgba::TRANSPARENT` when the row keeps the normal background.
    pub bg: Rgba,
    pub bold: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CyberTheme {
    Tron,
    Matrix,
    Blade,
    Light,
    /// The classic blue and white of the DOS file managers.
    Commander,
}

impl CyberTheme {
    pub fn name(&self) -> &'static str {
        match self {
            CyberTheme::Tron => "Tron (Neon Cyan / Electric Blue)",
            CyberTheme::Matrix => "Matrix (Phosphor Green / Black)",
            CyberTheme::Blade => "Blade (Amber Noir / Neon Magenta)",
            CyberTheme::Light => "Light (Clean Solar / Crisp Slate)",
            CyberTheme::Commander => "Commander (Classic Blue / White)",
        }
    }

    pub fn bg_color(&self) -> Rgba {
        match self {
            CyberTheme::Tron => Rgba::from_rgb(10, 15, 24),
            CyberTheme::Matrix => Rgba::from_rgb(3, 6, 3),
            CyberTheme::Blade => Rgba::from_rgb(18, 16, 20),
            CyberTheme::Light => Rgba::from_rgb(243, 245, 249),
            CyberTheme::Commander => Rgba::from_rgb(0, 0, 128),
        }
    }

    pub fn panel_bg(&self) -> Rgba {
        match self {
            CyberTheme::Tron => Rgba::from_rgb(13, 20, 32),
            CyberTheme::Matrix => Rgba::from_rgb(6, 12, 6),
            CyberTheme::Blade => Rgba::from_rgb(26, 22, 28),
            CyberTheme::Light => Rgba::from_rgb(255, 255, 255),
            CyberTheme::Commander => Rgba::from_rgb(0, 0, 170),
        }
    }

    /// Background of the focused/active dock tab.
    pub fn tab_active_bg(&self) -> Rgba {
        match self {
            CyberTheme::Tron => Rgba::from_rgb(18, 32, 50),
            CyberTheme::Matrix => Rgba::from_rgb(10, 26, 12),
            CyberTheme::Blade => Rgba::from_rgb(38, 28, 42),
            CyberTheme::Light => Rgba::from_rgb(255, 255, 255),
            CyberTheme::Commander => Rgba::from_rgb(0, 110, 140),
        }
    }

    /// Background of inactive dock tabs.
    pub fn tab_inactive_bg(&self) -> Rgba {
        match self {
            CyberTheme::Tron => Rgba::from_rgb(8, 12, 18),
            CyberTheme::Matrix => Rgba::from_rgb(4, 8, 4),
            CyberTheme::Blade => Rgba::from_rgb(14, 12, 16),
            CyberTheme::Light => Rgba::from_rgb(234, 238, 244),
            CyberTheme::Commander => Rgba::from_rgb(0, 0, 110),
        }
    }

    pub fn button_bg(&self) -> Rgba {
        match self {
            CyberTheme::Tron => Rgba::from_rgb(12, 19, 30),
            CyberTheme::Matrix => Rgba::from_rgb(8, 18, 10),
            CyberTheme::Blade => Rgba::from_rgb(28, 20, 26),
            CyberTheme::Light => Rgba::from_rgb(240, 244, 250),
            CyberTheme::Commander => Rgba::from_rgb(0, 0, 150),
        }
    }

    pub fn code_block_bg(&self) -> Rgba {
        match self {
            CyberTheme::Tron => Rgba::from_rgb(18, 28, 44),
            CyberTheme::Matrix => Rgba::from_rgb(8, 16, 8),
            CyberTheme::Blade => Rgba::from_rgb(34, 28, 36),
            CyberTheme::Light => Rgba::from_rgb(238, 242, 248),
            CyberTheme::Commander => Rgba::from_rgb(0, 0, 150),
        }
    }

    pub fn border_color(&self) -> Rgba {
        match self {
            CyberTheme::Tron => Rgba::from_rgb(0, 229, 255),
            CyberTheme::Matrix => Rgba::from_rgb(0, 255, 65),
            CyberTheme::Blade => Rgba::from_rgb(255, 140, 0),
            CyberTheme::Light => Rgba::from_rgb(0, 120, 215),
            CyberTheme::Commander => Rgba::from_rgb(85, 255, 255),
        }
    }

    pub fn accent_color(&self) -> Rgba {
        match self {
            CyberTheme::Tron => Rgba::from_rgb(0, 229, 255),
            CyberTheme::Matrix => Rgba::from_rgb(0, 255, 65),
            CyberTheme::Blade => Rgba::from_rgb(255, 140, 0),
            CyberTheme::Light => Rgba::from_rgb(0, 114, 206),
            CyberTheme::Commander => Rgba::from_rgb(255, 255, 85),
        }
    }

    pub fn secondary_accent(&self) -> Rgba {
        match self {
            CyberTheme::Tron => Rgba::from_rgb(0, 180, 216),
            CyberTheme::Matrix => Rgba::from_rgb(50, 205, 50),
            CyberTheme::Blade => Rgba::from_rgb(255, 0, 85),
            CyberTheme::Light => Rgba::from_rgb(0, 150, 136),
            CyberTheme::Commander => Rgba::from_rgb(85, 255, 255),
        }
    }

    pub fn text_primary(&self) -> Rgba {
        match self {
            CyberTheme::Tron => Rgba::from_rgb(224, 240, 255),
            CyberTheme::Matrix => Rgba::from_rgb(220, 255, 220),
            CyberTheme::Blade => Rgba::from_rgb(255, 243, 224),
            CyberTheme::Light => Rgba::from_rgb(24, 28, 36),
            CyberTheme::Commander => Rgba::from_rgb(230, 240, 255),
        }
    }

    pub fn text_dim(&self) -> Rgba {
        match self {
            CyberTheme::Tron => Rgba::from_rgb(100, 140, 170),
            CyberTheme::Matrix => Rgba::from_rgb(130, 195, 130),
            CyberTheme::Blade => Rgba::from_rgb(175, 145, 135),
            CyberTheme::Light => Rgba::from_rgb(100, 116, 139),
            CyberTheme::Commander => Rgba::from_rgb(160, 180, 220),
        }
    }

    pub fn error_color(&self) -> Rgba {
        Rgba::from_rgb(255, 51, 68)
    }

    pub fn warn_color(&self) -> Rgba {
        match self {
            CyberTheme::Light => Rgba::from_rgb(195, 105, 0),
            _ => Rgba::from_rgb(255, 187, 0),
        }
    }

    pub fn info_color(&self) -> Rgba {
        self.accent_color()
    }

    /// Level palette: how a row whose level was detected is drawn when no user highlight
    /// rule matches it. INFO and unknown levels keep the plain text style (`None`).
    pub fn level_style(&self, level: LogLevel) -> Option<LevelStyle> {
        let is_light = *self == CyberTheme::Light;
        let plain = Rgba::TRANSPARENT;
        match level {
            LogLevel::Fatal => Some(LevelStyle {
                fg: if is_light {
                    Rgba::WHITE
                } else {
                    Rgba::from_rgb(255, 235, 238)
                },
                bg: if is_light {
                    Rgba::from_rgb(200, 30, 45)
                } else {
                    Rgba::from_rgb(120, 16, 28)
                },
                bold: true,
            }),
            LogLevel::Error => Some(LevelStyle {
                fg: self.level_color(level),
                bg: plain,
                bold: false,
            }),
            LogLevel::Warn => Some(LevelStyle {
                fg: self.warn_color(),
                bg: plain,
                bold: false,
            }),
            LogLevel::Debug => Some(LevelStyle {
                fg: self.text_dim(),
                bg: plain,
                bold: false,
            }),
            LogLevel::Trace => Some(LevelStyle {
                fg: self.level_color(level),
                bg: plain,
                bold: false,
            }),
            LogLevel::Info | LogLevel::Unknown => None,
        }
    }

    /// Colour of a level tag (selector entries, per-level counters).
    pub fn level_color(&self, level: LogLevel) -> Rgba {
        let is_light = *self == CyberTheme::Light;
        match level {
            LogLevel::Fatal | LogLevel::Error => {
                if is_light {
                    Rgba::from_rgb(200, 30, 45)
                } else {
                    self.error_color()
                }
            }
            LogLevel::Warn => self.warn_color(),
            LogLevel::Info => self.accent_color(),
            LogLevel::Debug => self.text_dim(),
            LogLevel::Trace => {
                if is_light {
                    Rgba::from_rgb(148, 160, 176)
                } else {
                    self.text_dim().gamma_multiply(0.7)
                }
            }
            LogLevel::Unknown => self.text_dim(),
        }
    }

    /// Foreground and background of quick label preset `n` (1..=9): red, orange, yellow,
    /// green, cyan, blue, violet, magenta, grey. Saturated on the dark themes, pastel on
    /// Light so the text stays readable.
    pub fn label_style(&self, n: u8) -> (Rgba, Rgba) {
        let i = (n.clamp(1, 9) - 1) as usize;
        if *self == CyberTheme::Light {
            const BG: [[u8; 3]; 9] = [
                [255, 205, 205],
                [255, 222, 180],
                [255, 245, 160],
                [200, 240, 200],
                [190, 240, 245],
                [200, 215, 255],
                [225, 205, 255],
                [255, 205, 235],
                [220, 225, 230],
            ];
            let [r, g, b] = BG[i];
            (Rgba::from_rgb(24, 28, 36), Rgba::from_rgb(r, g, b))
        } else {
            const BG: [[u8; 3]; 9] = [
                [220, 50, 50],
                [230, 120, 30],
                [230, 200, 30],
                [40, 190, 80],
                [0, 200, 220],
                [60, 120, 240],
                [150, 90, 240],
                [230, 60, 180],
                [150, 160, 170],
            ];
            let [r, g, b] = BG[i];
            let fg = if i == 5 || i == 6 {
                Rgba::WHITE
            } else {
                Rgba::from_rgb(10, 10, 10)
            };
            (fg, Rgba::from_rgb(r, g, b))
        }
    }

    /// Foreground of an automatically highlighted token of `kind` (see `auto_highlight`):
    /// cyans and violet on Tron, greens and amber on Matrix, amber and magenta on Blade,
    /// dark blues and teal on Light, each at least 4.5:1 against the theme background.
    pub fn token_color(&self, kind: crate::auto_highlight::TokenKind) -> Rgba {
        use crate::auto_highlight::TokenKind as K;
        let [r, g, b] = match (self, kind) {
            (CyberTheme::Tron, K::Ip) => [0, 210, 255],
            (CyberTheme::Tron, K::Uuid) => [190, 140, 255],
            (CyberTheme::Tron, K::Url) => [110, 170, 255],
            (CyberTheme::Tron, K::Duration) => [120, 255, 220],
            (CyberTheme::Tron, K::Path) => [170, 205, 235],
            (CyberTheme::Matrix, K::Ip) => [120, 255, 120],
            (CyberTheme::Matrix, K::Uuid) => [0, 200, 140],
            (CyberTheme::Matrix, K::Url) => [190, 255, 90],
            (CyberTheme::Matrix, K::Duration) => [255, 191, 0],
            (CyberTheme::Matrix, K::Path) => [160, 225, 175],
            (CyberTheme::Blade, K::Ip) => [255, 176, 0],
            (CyberTheme::Blade, K::Uuid) => [255, 95, 200],
            (CyberTheme::Blade, K::Url) => [255, 130, 170],
            (CyberTheme::Blade, K::Duration) => [255, 215, 120],
            (CyberTheme::Blade, K::Path) => [215, 165, 255],
            (CyberTheme::Light, K::Ip) => [0, 70, 160],
            (CyberTheme::Light, K::Uuid) => [95, 40, 160],
            (CyberTheme::Light, K::Url) => [0, 90, 200],
            (CyberTheme::Light, K::Duration) => [0, 110, 100],
            (CyberTheme::Light, K::Path) => [60, 70, 110],
            (CyberTheme::Commander, K::Ip) => [85, 255, 255],
            (CyberTheme::Commander, K::Uuid) => [255, 170, 255],
            (CyberTheme::Commander, K::Url) => [170, 205, 255],
            (CyberTheme::Commander, K::Duration) => [255, 255, 85],
            (CyberTheme::Commander, K::Path) => [205, 225, 255],
        };
        Rgba::from_rgb(r, g, b)
    }

    /// The 16 base ANSI colours (SGR 30-37 and 90-97, and the first 16 entries of the
    /// 256-colour table) for this theme. The dark themes lift black so it stays visible
    /// on their near-black backgrounds; Light darkens white, yellow and the bright colours
    /// so that every entry reads on its pale background (contrast checked in the tests).
    pub fn ansi_palette(&self) -> [Rgba; 16] {
        const DARK: [[u8; 3]; 16] = [
            [96, 100, 112],
            [240, 82, 82],
            [80, 200, 105],
            [230, 200, 80],
            [92, 142, 250],
            [210, 112, 230],
            [60, 205, 220],
            [205, 210, 218],
            [135, 140, 152],
            [255, 115, 115],
            [120, 240, 145],
            [255, 235, 125],
            [135, 175, 255],
            [240, 150, 255],
            [120, 240, 250],
            [255, 255, 255],
        ];
        const LIGHT: [[u8; 3]; 16] = [
            [24, 28, 36],
            [185, 28, 42],
            [22, 120, 48],
            [135, 90, 0],
            [25, 80, 200],
            [145, 40, 155],
            [0, 115, 130],
            [88, 94, 105],
            [85, 94, 110],
            [205, 40, 55],
            [26, 124, 54],
            [125, 95, 0],
            [45, 95, 215],
            [165, 55, 175],
            [0, 120, 134],
            [60, 65, 76],
        ];
        // On the blue of Commander every entry is lifted so it reads on it.
        const COMMANDER: [[u8; 3]; 16] = [
            [165, 170, 190],
            [255, 125, 125],
            [110, 240, 130],
            [255, 235, 100],
            [150, 185, 255],
            [245, 150, 255],
            [90, 235, 245],
            [230, 230, 230],
            [185, 190, 210],
            [255, 150, 150],
            [150, 255, 160],
            [255, 255, 140],
            [175, 205, 255],
            [255, 175, 255],
            [150, 250, 255],
            [255, 255, 255],
        ];
        let table = match self {
            CyberTheme::Light => &LIGHT,
            CyberTheme::Commander => &COMMANDER,
            _ => &DARK,
        };
        table.map(|[r, g, b]| Rgba::from_rgb(r, g, b))
    }

    /// An ANSI colour: the theme palette for 0..16, the xterm table for the rest of the
    /// 256 colours, a 24-bit colour as given.
    pub fn ansi_color(&self, color: crate::ansi::AnsiColor) -> Rgba {
        match color {
            crate::ansi::AnsiColor::Indexed(i) if i < 16 => self.ansi_palette()[i as usize],
            crate::ansi::AnsiColor::Indexed(i) => {
                let [r, g, b] = crate::ansi::xterm_color(i);
                Rgba::from_rgb(r, g, b)
            }
            crate::ansi::AnsiColor::Rgb(r, g, b) => Rgba::from_rgb(r, g, b),
        }
    }

    /// Foreground and background of an ANSI-styled run over a row whose own colours are
    /// `base_fg` / `base_bg`. Bold turns a base colour 0-7 into its bright variant, as
    /// terminals do (a monospace font has no bold weight here); dim fades the text;
    /// inverse swaps the two; a background without a foreground gets black or white text,
    /// whichever reads on it.
    pub fn ansi_colors(
        &self,
        style: &crate::ansi::AnsiStyle,
        base_fg: Rgba,
        base_bg: Rgba,
    ) -> (Rgba, Rgba) {
        use crate::ansi::AnsiColor;
        let fg_color = match style.fg {
            Some(AnsiColor::Indexed(i)) if style.bold && i < 8 => Some(AnsiColor::Indexed(i + 8)),
            other => other,
        };
        let bg = style.bg.map(|c| self.ansi_color(c));
        let mut fg = match (fg_color, bg) {
            (Some(c), _) => self.ansi_color(c),
            (None, Some(bg)) => readable_on(bg),
            (None, None) if style.bold => {
                if *self == CyberTheme::Light {
                    Rgba::BLACK
                } else {
                    Rgba::WHITE
                }
            }
            (None, None) => base_fg,
        };
        let mut bg = bg.unwrap_or(base_bg);
        if style.inverse {
            let behind = if bg == Rgba::TRANSPARENT {
                self.panel_bg()
            } else {
                bg
            };
            bg = fg;
            fg = behind;
        }
        if style.dim {
            fg = fg.gamma_multiply(0.6);
        }
        (fg, bg)
    }
}

/// Relative luminance of a colour (WCAG 2), for the contrast choices above.
fn luminance(c: Rgba) -> f32 {
    let lin = |v: u8| {
        let v = v as f32 / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
}

/// Contrast ratio between two colours (1 to 21).
fn contrast(a: Rgba, b: Rgba) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// Black or white, whichever contrasts more with `bg`.
fn readable_on(bg: Rgba) -> Rgba {
    if contrast(Rgba::BLACK, bg) >= contrast(Rgba::WHITE, bg) {
        Rgba::BLACK
    } else {
        Rgba::WHITE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ansi::{AnsiColor, AnsiStyle};

    #[test]
    fn ansi_palette_reads_on_every_theme() {
        for theme in [
            CyberTheme::Tron,
            CyberTheme::Matrix,
            CyberTheme::Blade,
            CyberTheme::Light,
            CyberTheme::Commander,
        ] {
            for (i, c) in theme.ansi_palette().iter().enumerate() {
                let ratio = contrast(*c, theme.bg_color());
                // WCAG AA for normal text on Light; on the dark themes the dim entries
                // (black, bright black) only need to stay visible.
                let min = if theme == CyberTheme::Light || !matches!(i, 0 | 8) {
                    4.5
                } else {
                    3.0
                };
                assert!(ratio >= min, "{theme:?} colour {i}: contrast {ratio:.2}");
            }
        }
    }

    #[test]
    fn token_colours_read_on_every_theme_and_differ() {
        use crate::auto_highlight::TokenKind;
        for theme in [
            CyberTheme::Tron,
            CyberTheme::Matrix,
            CyberTheme::Blade,
            CyberTheme::Light,
            CyberTheme::Commander,
        ] {
            let colours: Vec<Rgba> = TokenKind::ALL
                .iter()
                .map(|k| theme.token_color(*k))
                .collect();
            for (kind, c) in TokenKind::ALL.iter().zip(&colours) {
                for bg in [theme.bg_color(), theme.panel_bg()] {
                    let ratio = contrast(*c, bg);
                    assert!(ratio >= 4.5, "{theme:?} {kind:?}: contrast {ratio:.2}");
                }
            }
            for (i, a) in colours.iter().enumerate() {
                assert!(
                    !colours[i + 1..].contains(a),
                    "{theme:?}: two kinds share a colour"
                );
            }
        }
        assert_ne!(
            CyberTheme::Tron.token_color(TokenKind::Ip),
            CyberTheme::Light.token_color(TokenKind::Ip)
        );
    }

    #[test]
    fn ansi_colors_resolve_bold_inverse_dim_and_backgrounds() {
        let theme = CyberTheme::Tron;
        let pal = theme.ansi_palette();
        let base_fg = theme.text_primary();
        let clear = Rgba::TRANSPARENT;
        let red = AnsiStyle {
            fg: Some(AnsiColor::Indexed(1)),
            ..Default::default()
        };
        assert_eq!(theme.ansi_colors(&red, base_fg, clear), (pal[1], clear));
        let bold_red = AnsiStyle { bold: true, ..red };
        assert_eq!(theme.ansi_colors(&bold_red, base_fg, clear).0, pal[9]);
        let inverse = AnsiStyle {
            inverse: true,
            ..red
        };
        assert_eq!(
            theme.ansi_colors(&inverse, base_fg, clear),
            (theme.panel_bg(), pal[1])
        );
        let dim = AnsiStyle { dim: true, ..red };
        assert_eq!(
            theme.ansi_colors(&dim, base_fg, clear).0,
            pal[1].gamma_multiply(0.6)
        );
        let on_white = AnsiStyle {
            bg: Some(AnsiColor::Indexed(15)),
            ..Default::default()
        };
        assert_eq!(
            theme.ansi_colors(&on_white, base_fg, clear),
            (Rgba::BLACK, pal[15])
        );
        let rgb = AnsiStyle {
            fg: Some(AnsiColor::Rgb(1, 2, 3)),
            bg: Some(AnsiColor::Indexed(196)),
            ..Default::default()
        };
        assert_eq!(
            theme.ansi_colors(&rgb, base_fg, clear),
            (Rgba::from_rgb(1, 2, 3), Rgba::from_rgb(255, 0, 0))
        );
        // Underline or italic alone keep the row's colours.
        let underline = AnsiStyle {
            underline: true,
            ..Default::default()
        };
        assert_eq!(
            theme.ansi_colors(&underline, base_fg, clear),
            (base_fg, clear)
        );
    }
}
