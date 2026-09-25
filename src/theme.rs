use crate::log_level::LogLevel;
use egui::{Color32, Stroke, Style, Visuals};
use serde::{Deserialize, Serialize};

/// Style of a row coloured by its detected log level (see `CyberTheme::level_style`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LevelStyle {
    pub fg: Color32,
    /// `Color32::TRANSPARENT` when the row keeps the normal background.
    pub bg: Color32,
    pub bold: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CyberTheme {
    Tron,
    Matrix,
    Blade,
    Light,
}

impl CyberTheme {
    pub fn name(&self) -> &'static str {
        match self {
            CyberTheme::Tron => "Tron (Neon Cyan / Electric Blue)",
            CyberTheme::Matrix => "Matrix (Phosphor Green / Black)",
            CyberTheme::Blade => "Blade (Amber Noir / Neon Magenta)",
            CyberTheme::Light => "Light (Clean Solar / Crisp Slate)",
        }
    }

    pub fn bg_color(&self) -> Color32 {
        match self {
            CyberTheme::Tron => Color32::from_rgb(10, 15, 24),
            CyberTheme::Matrix => Color32::from_rgb(3, 6, 3),
            CyberTheme::Blade => Color32::from_rgb(18, 16, 20),
            CyberTheme::Light => Color32::from_rgb(243, 245, 249),
        }
    }

    pub fn panel_bg(&self) -> Color32 {
        match self {
            CyberTheme::Tron => Color32::from_rgb(13, 20, 32),
            CyberTheme::Matrix => Color32::from_rgb(6, 12, 6),
            CyberTheme::Blade => Color32::from_rgb(26, 22, 28),
            CyberTheme::Light => Color32::from_rgb(255, 255, 255),
        }
    }

    /// Background of the focused/active dock tab.
    pub fn tab_active_bg(&self) -> Color32 {
        match self {
            CyberTheme::Tron => Color32::from_rgb(18, 32, 50),
            CyberTheme::Matrix => Color32::from_rgb(10, 26, 12),
            CyberTheme::Blade => Color32::from_rgb(38, 28, 42),
            CyberTheme::Light => Color32::from_rgb(255, 255, 255),
        }
    }

    /// Background of inactive dock tabs.
    pub fn tab_inactive_bg(&self) -> Color32 {
        match self {
            CyberTheme::Tron => Color32::from_rgb(8, 12, 18),
            CyberTheme::Matrix => Color32::from_rgb(4, 8, 4),
            CyberTheme::Blade => Color32::from_rgb(14, 12, 16),
            CyberTheme::Light => Color32::from_rgb(234, 238, 244),
        }
    }

    pub fn button_bg(&self) -> Color32 {
        match self {
            CyberTheme::Tron => Color32::from_rgb(12, 19, 30),
            CyberTheme::Matrix => Color32::from_rgb(8, 18, 10),
            CyberTheme::Blade => Color32::from_rgb(28, 20, 26),
            CyberTheme::Light => Color32::from_rgb(240, 244, 250),
        }
    }

    pub fn code_block_bg(&self) -> Color32 {
        match self {
            CyberTheme::Tron => Color32::from_rgb(18, 28, 44),
            CyberTheme::Matrix => Color32::from_rgb(8, 16, 8),
            CyberTheme::Blade => Color32::from_rgb(34, 28, 36),
            CyberTheme::Light => Color32::from_rgb(238, 242, 248),
        }
    }

    pub fn border_color(&self) -> Color32 {
        match self {
            CyberTheme::Tron => Color32::from_rgb(0, 229, 255),
            CyberTheme::Matrix => Color32::from_rgb(0, 255, 65),
            CyberTheme::Blade => Color32::from_rgb(255, 140, 0),
            CyberTheme::Light => Color32::from_rgb(0, 120, 215),
        }
    }

    pub fn accent_color(&self) -> Color32 {
        match self {
            CyberTheme::Tron => Color32::from_rgb(0, 229, 255),
            CyberTheme::Matrix => Color32::from_rgb(0, 255, 65),
            CyberTheme::Blade => Color32::from_rgb(255, 140, 0),
            CyberTheme::Light => Color32::from_rgb(0, 114, 206),
        }
    }

    pub fn secondary_accent(&self) -> Color32 {
        match self {
            CyberTheme::Tron => Color32::from_rgb(0, 180, 216),
            CyberTheme::Matrix => Color32::from_rgb(50, 205, 50),
            CyberTheme::Blade => Color32::from_rgb(255, 0, 85),
            CyberTheme::Light => Color32::from_rgb(0, 150, 136),
        }
    }

    pub fn text_primary(&self) -> Color32 {
        match self {
            CyberTheme::Tron => Color32::from_rgb(224, 240, 255),
            CyberTheme::Matrix => Color32::from_rgb(220, 255, 220),
            CyberTheme::Blade => Color32::from_rgb(255, 243, 224),
            CyberTheme::Light => Color32::from_rgb(24, 28, 36),
        }
    }

    pub fn text_dim(&self) -> Color32 {
        match self {
            CyberTheme::Tron => Color32::from_rgb(100, 140, 170),
            CyberTheme::Matrix => Color32::from_rgb(130, 195, 130),
            CyberTheme::Blade => Color32::from_rgb(175, 145, 135),
            CyberTheme::Light => Color32::from_rgb(100, 116, 139),
        }
    }

    pub fn error_color(&self) -> Color32 {
        Color32::from_rgb(255, 51, 68)
    }

    pub fn warn_color(&self) -> Color32 {
        match self {
            CyberTheme::Light => Color32::from_rgb(195, 105, 0),
            _ => Color32::from_rgb(255, 187, 0),
        }
    }

    pub fn info_color(&self) -> Color32 {
        self.accent_color()
    }

    /// Level palette: how a row whose level was detected is drawn when no user highlight
    /// rule matches it. INFO and unknown levels keep the plain text style (`None`).
    pub fn level_style(&self, level: LogLevel) -> Option<LevelStyle> {
        let is_light = *self == CyberTheme::Light;
        let plain = Color32::TRANSPARENT;
        match level {
            LogLevel::Fatal => Some(LevelStyle {
                fg: if is_light {
                    Color32::WHITE
                } else {
                    Color32::from_rgb(255, 235, 238)
                },
                bg: if is_light {
                    Color32::from_rgb(200, 30, 45)
                } else {
                    Color32::from_rgb(120, 16, 28)
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
    pub fn level_color(&self, level: LogLevel) -> Color32 {
        let is_light = *self == CyberTheme::Light;
        match level {
            LogLevel::Fatal | LogLevel::Error => {
                if is_light {
                    Color32::from_rgb(200, 30, 45)
                } else {
                    self.error_color()
                }
            }
            LogLevel::Warn => self.warn_color(),
            LogLevel::Info => self.accent_color(),
            LogLevel::Debug => self.text_dim(),
            LogLevel::Trace => {
                if is_light {
                    Color32::from_rgb(148, 160, 176)
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
    pub fn label_style(&self, n: u8) -> (Color32, Color32) {
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
            (Color32::from_rgb(24, 28, 36), Color32::from_rgb(r, g, b))
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
                Color32::WHITE
            } else {
                Color32::from_rgb(10, 10, 10)
            };
            (fg, Color32::from_rgb(r, g, b))
        }
    }

    /// The 16 base ANSI colours (SGR 30-37 and 90-97, and the first 16 entries of the
    /// 256-colour table) for this theme. The dark themes lift black so it stays visible
    /// on their near-black backgrounds; Light darkens white, yellow and the bright colours
    /// so that every entry reads on its pale background (contrast checked in the tests).
    pub fn ansi_palette(&self) -> [Color32; 16] {
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
        let table = if *self == CyberTheme::Light {
            &LIGHT
        } else {
            &DARK
        };
        table.map(|[r, g, b]| Color32::from_rgb(r, g, b))
    }

    /// An ANSI colour: the theme palette for 0..16, the xterm table for the rest of the
    /// 256 colours, a 24-bit colour as given.
    pub fn ansi_color(&self, color: crate::ansi::AnsiColor) -> Color32 {
        match color {
            crate::ansi::AnsiColor::Indexed(i) if i < 16 => self.ansi_palette()[i as usize],
            crate::ansi::AnsiColor::Indexed(i) => {
                let [r, g, b] = crate::ansi::xterm_color(i);
                Color32::from_rgb(r, g, b)
            }
            crate::ansi::AnsiColor::Rgb(r, g, b) => Color32::from_rgb(r, g, b),
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
        base_fg: Color32,
        base_bg: Color32,
    ) -> (Color32, Color32) {
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
                    Color32::BLACK
                } else {
                    Color32::WHITE
                }
            }
            (None, None) => base_fg,
        };
        let mut bg = bg.unwrap_or(base_bg);
        if style.inverse {
            let behind = if bg == Color32::TRANSPARENT {
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

    pub fn apply(&self, ctx: &egui::Context) {
        let is_light = *self == CyberTheme::Light;
        let mut visuals = if is_light {
            Visuals::light()
        } else {
            Visuals::dark()
        };
        let bg = self.bg_color();
        let panel = self.panel_bg();
        let border = self.border_color();
        let text = self.text_primary();
        let accent = self.accent_color();
        let btn_bg = self.button_bg();

        visuals.panel_fill = panel;
        visuals.window_fill = if is_light { panel } else { bg };
        visuals.extreme_bg_color = bg;
        visuals.faint_bg_color = if is_light {
            Color32::from_gray(240)
        } else {
            Color32::from_black_alpha(180)
        };

        visuals.widgets.noninteractive.bg_fill = panel;
        visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, text);
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, border.gamma_multiply(0.4));
        visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(6);

        visuals.widgets.inactive.bg_fill = btn_bg;
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, text);
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, border.gamma_multiply(0.35));
        visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(6);

        visuals.widgets.hovered.bg_fill = if is_light {
            Color32::from_rgb(228, 236, 248)
        } else {
            panel.linear_multiply(1.25)
        };
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, accent);
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.5_f32, accent);
        visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(6);
        visuals.widgets.hovered.expansion = 0.0;

        visuals.widgets.active.bg_fill = accent.gamma_multiply(0.25);
        visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, accent);
        visuals.widgets.active.bg_stroke = Stroke::new(2.0_f32, accent);
        visuals.widgets.active.corner_radius = egui::CornerRadius::same(6);
        visuals.widgets.active.expansion = 0.0;

        visuals.widgets.open.expansion = 0.0;

        visuals.selection.bg_fill = accent.gamma_multiply(0.35);
        visuals.selection.stroke = Stroke::new(1.0_f32, accent);

        visuals.window_stroke = Stroke::new(1.5_f32, border);
        visuals.window_corner_radius = egui::CornerRadius::same(8);
        visuals.menu_corner_radius = egui::CornerRadius::same(6);

        ctx.set_visuals(visuals.clone());
        ctx.set_theme(if is_light {
            egui::Theme::Light
        } else {
            egui::Theme::Dark
        });

        let style = Style {
            visuals: visuals.clone(),
            ..Default::default()
        };
        ctx.set_style_of(egui::Theme::Dark, style.clone());
        ctx.set_style_of(egui::Theme::Light, style);
    }
}

/// Relative luminance of a colour (WCAG 2), for the contrast choices above.
fn luminance(c: Color32) -> f32 {
    let lin = |v: u8| {
        let v = v as f32 / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.r()) + 0.7152 * lin(c.g()) + 0.0722 * lin(c.b())
}

/// Contrast ratio between two colours (1 to 21).
fn contrast(a: Color32, b: Color32) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// Black or white, whichever contrasts more with `bg`.
fn readable_on(bg: Color32) -> Color32 {
    if contrast(Color32::BLACK, bg) >= contrast(Color32::WHITE, bg) {
        Color32::BLACK
    } else {
        Color32::WHITE
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
    fn ansi_colors_resolve_bold_inverse_dim_and_backgrounds() {
        let theme = CyberTheme::Tron;
        let pal = theme.ansi_palette();
        let base_fg = theme.text_primary();
        let clear = Color32::TRANSPARENT;
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
            (Color32::BLACK, pal[15])
        );
        let rgb = AnsiStyle {
            fg: Some(AnsiColor::Rgb(1, 2, 3)),
            bg: Some(AnsiColor::Indexed(196)),
            ..Default::default()
        };
        assert_eq!(
            theme.ansi_colors(&rgb, base_fg, clear),
            (Color32::from_rgb(1, 2, 3), Color32::from_rgb(255, 0, 0))
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
