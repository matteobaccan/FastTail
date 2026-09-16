use egui::{Color32, Stroke, Style, Visuals};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CyberTheme {
    Tron,
    Matrix,
    Blade,
}

impl CyberTheme {
    pub fn name(&self) -> &'static str {
        match self {
            CyberTheme::Tron => "Tron (Neon Cyan / Electric Blue)",
            CyberTheme::Matrix => "Matrix (Phosphor Green / Black)",
            CyberTheme::Blade => "Blade (Amber Noir / Neon Magenta)",
        }
    }

    pub fn bg_color(&self) -> Color32 {
        match self {
            CyberTheme::Tron => Color32::from_rgb(7, 11, 18),
            CyberTheme::Matrix => Color32::from_rgb(3, 6, 3),
            CyberTheme::Blade => Color32::from_rgb(18, 16, 20),
        }
    }

    pub fn panel_bg(&self) -> Color32 {
        match self {
            CyberTheme::Tron => Color32::from_rgb(11, 18, 28),
            CyberTheme::Matrix => Color32::from_rgb(6, 12, 6),
            CyberTheme::Blade => Color32::from_rgb(26, 22, 28),
        }
    }

    pub fn border_color(&self) -> Color32 {
        match self {
            CyberTheme::Tron => Color32::from_rgb(0, 229, 255),
            CyberTheme::Matrix => Color32::from_rgb(0, 255, 65),
            CyberTheme::Blade => Color32::from_rgb(255, 140, 0),
        }
    }

    pub fn accent_color(&self) -> Color32 {
        match self {
            CyberTheme::Tron => Color32::from_rgb(0, 229, 255),
            CyberTheme::Matrix => Color32::from_rgb(0, 255, 65),
            CyberTheme::Blade => Color32::from_rgb(255, 140, 0),
        }
    }

    pub fn secondary_accent(&self) -> Color32 {
        match self {
            CyberTheme::Tron => Color32::from_rgb(0, 140, 255),
            CyberTheme::Matrix => Color32::from_rgb(50, 205, 50),
            CyberTheme::Blade => Color32::from_rgb(255, 0, 85),
        }
    }

    pub fn text_primary(&self) -> Color32 {
        match self {
            CyberTheme::Tron => Color32::from_rgb(224, 247, 250),
            CyberTheme::Matrix => Color32::from_rgb(220, 255, 220),
            CyberTheme::Blade => Color32::from_rgb(255, 243, 224),
        }
    }

    pub fn text_dim(&self) -> Color32 {
        match self {
            CyberTheme::Tron => Color32::from_rgb(120, 160, 180),
            CyberTheme::Matrix => Color32::from_rgb(130, 195, 130),
            CyberTheme::Blade => Color32::from_rgb(175, 145, 135),
        }
    }

    pub fn error_color(&self) -> Color32 {
        Color32::from_rgb(255, 40, 70)
    }

    pub fn warn_color(&self) -> Color32 {
        Color32::from_rgb(255, 180, 0)
    }

    pub fn info_color(&self) -> Color32 {
        self.accent_color()
    }

    pub fn apply(&self, ctx: &egui::Context) {
        let mut visuals = Visuals::dark();
        let bg = self.bg_color();
        let panel = self.panel_bg();
        let border = self.border_color();
        let text = self.text_primary();
        let accent = self.accent_color();

        visuals.panel_fill = panel;
        visuals.window_fill = bg;
        visuals.extreme_bg_color = bg;
        visuals.faint_bg_color = Color32::from_black_alpha(180);

        visuals.widgets.noninteractive.bg_fill = panel;
        visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, text);
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, border.gamma_multiply(0.4));

        visuals.widgets.inactive.bg_fill = panel;
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, text);
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, border.gamma_multiply(0.3));

        visuals.widgets.hovered.bg_fill = panel.linear_multiply(1.2);
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, accent);
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.5_f32, accent);

        visuals.widgets.active.bg_fill = accent.gamma_multiply(0.2);
        visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, accent);
        visuals.widgets.active.bg_stroke = Stroke::new(2.0_f32, accent);

        visuals.selection.bg_fill = accent.gamma_multiply(0.3);
        visuals.selection.stroke = Stroke::new(1.0_f32, accent);

        visuals.window_stroke = Stroke::new(1.5_f32, border);
        visuals.window_corner_radius = egui::CornerRadius::same(2);

        let style = Style {
            visuals,
            ..Default::default()
        };
        ctx.set_style(style);
    }
}
