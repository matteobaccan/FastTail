// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The GUI side of the themes: conversions between the engine's `Rgba` and egui's
//! `Color32` (same premultiplied bytes, no loss either way) and `CyberTheme::apply`,
//! which turns a theme into egui visuals.

use crate::color::Rgba;
use crate::theme::CyberTheme;
use egui::{Color32, Stroke, Style, Visuals};

impl From<Rgba> for Color32 {
    fn from(c: Rgba) -> Self {
        Color32::from_rgba_premultiplied(c.r, c.g, c.b, c.a)
    }
}

impl From<Color32> for Rgba {
    fn from(c: Color32) -> Self {
        Rgba::from_rgba_premultiplied(c.r(), c.g(), c.b(), c.a())
    }
}

impl CyberTheme {
    pub fn apply(&self, ctx: &egui::Context) {
        let is_light = *self == CyberTheme::Light;
        let mut visuals = if is_light {
            Visuals::light()
        } else {
            Visuals::dark()
        };
        let bg = Color32::from(self.bg_color());
        let panel = Color32::from(self.panel_bg());
        let border = Color32::from(self.border_color());
        let text = Color32::from(self.text_primary());
        let accent = Color32::from(self.accent_color());
        let btn_bg = Color32::from(self.button_bg());

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba_and_color32_convert_without_loss() {
        for c in [
            Rgba::TRANSPARENT,
            Rgba::WHITE,
            Rgba::from_rgb(10, 20, 30),
            Rgba::from_black_alpha(180),
            CyberTheme::Tron.accent_color().gamma_multiply(0.35),
        ] {
            assert_eq!(Rgba::from(Color32::from(c)), c);
        }
        assert_eq!(
            Color32::from(Rgba::from_rgb(1, 2, 3)),
            Color32::from_rgb(1, 2, 3)
        );
        assert_eq!(Color32::from(Rgba::TRANSPARENT), Color32::TRANSPARENT);
        // Fading matches egui's own `gamma_multiply` byte for byte.
        let accent = CyberTheme::Blade.accent_color();
        assert_eq!(
            Color32::from(accent.gamma_multiply(0.4)),
            Color32::from(accent).gamma_multiply(0.4)
        );
    }
}
