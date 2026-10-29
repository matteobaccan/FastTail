// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The Web view settings window (`crate::web_view`).

use crate::config::FastTailConfig;
use crate::i18n::t;
use crate::theme::CyberTheme;
use egui::RichText;

pub use crate::web_view::*;

impl WebView {
    /// The Web view settings window; returns whether the settings changed.
    pub fn show_settings(
        &mut self,
        ctx: &egui::Context,
        cfg: &mut FastTailConfig,
        theme: &CyberTheme,
    ) -> bool {
        if !self.open {
            return false;
        }
        let lang = cfg.language;
        let mut changed = false;
        let mut open = self.open;
        egui::Window::new(
            RichText::new(format!("🌐 {}", t(lang, "web_title")))
                .monospace()
                .color(theme.accent_color()),
        )
        .id(egui::Id::new("fasttail_web_view"))
        .open(&mut open)
        .resizable(false)
        .default_width(480.0)
        .show(ctx, |ui| {
            ui.label(RichText::new(t(lang, "web_intro")).color(theme.text_dim()));
            ui.add_space(4.0);
            changed |= ui
                .checkbox(&mut cfg.web_enabled, t(lang, "web_enable"))
                .changed();
            ui.horizontal(|ui| {
                ui.label(t(lang, "web_port"));
                changed |= ui
                    .add(egui::DragValue::new(&mut cfg.web_port).range(1024..=65535))
                    .changed();
            });
            let mut remote = cfg.web_allow_remote;
            if ui
                .checkbox(&mut remote, t(lang, "web_allow_remote"))
                .on_hover_text(t(lang, "web_allow_remote_tip"))
                .changed()
            {
                if remote {
                    self.confirm_remote = true;
                } else {
                    cfg.web_allow_remote = false;
                    changed = true;
                }
            }
            if self.confirm_remote {
                ui.label(RichText::new(t(lang, "web_remote_warning")).color(theme.warn_color()));
                ui.horizontal(|ui| {
                    if ui.button(t(lang, "web_remote_confirm")).clicked() {
                        cfg.web_allow_remote = true;
                        self.confirm_remote = false;
                        changed = true;
                    }
                    if ui.button(t(lang, "session_cancel")).clicked() {
                        self.confirm_remote = false;
                    }
                });
            }
            if cfg.web_allow_remote {
                let lan = crate::web::lan_address().map_or_else(
                    || "?".to_string(),
                    |ip| format!("http://{ip}:{}/", cfg.web_port),
                );
                ui.label(
                    RichText::new(t(lang, "web_lan").replace("{0}", &lan))
                        .monospace()
                        .color(theme.warn_color()),
                );
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(t(lang, "web_token"));
                let shown = if cfg.web_token.len() > 12 {
                    format!("{}…", &cfg.web_token[..12])
                } else {
                    cfg.web_token.clone()
                };
                ui.label(RichText::new(shown).monospace());
                if ui
                    .button(t(lang, "web_regenerate"))
                    .on_hover_text(t(lang, "web_regenerate_tip"))
                    .clicked()
                {
                    cfg.web_token = crate::web::new_token();
                    changed = true;
                }
            });
            ui.horizontal(|ui| {
                let on = cfg.web_enabled && self.server.is_some();
                if ui
                    .add_enabled(on, egui::Button::new(t(lang, "web_open_browser")))
                    .on_disabled_hover_text(t(lang, "web_off"))
                    .clicked()
                {
                    ctx.open_url(egui::OpenUrl::new_tab(Self::link(cfg)));
                }
                if ui
                    .add_enabled(on, egui::Button::new(t(lang, "web_copy_link")))
                    .on_disabled_hover_text(t(lang, "web_off"))
                    .clicked()
                {
                    ctx.copy_text(Self::link(cfg));
                }
            });
            match (&self.error, self.connections()) {
                (Some(e), _) => {
                    ui.label(
                        RichText::new(t(lang, "web_failed").replace("{0}", e))
                            .color(theme.warn_color()),
                    );
                }
                (None, Some(n)) => {
                    ui.label(
                        RichText::new(t(lang, "web_connected").replace("{0}", &n.to_string()))
                            .color(theme.text_dim()),
                    );
                }
                (None, None) => {}
            }
        });
        self.open = open;
        if !self.open {
            self.confirm_remote = false;
        }
        changed
    }
}
