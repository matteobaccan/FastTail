// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The AI assistants settings window of the MCP server (`crate::mcp_serve`).

use crate::config::FastTailConfig;
use crate::i18n::{t, Language};
use crate::theme::CyberTheme;
use egui::RichText;
use std::path::PathBuf;
use std::time::Instant;

pub use crate::mcp_serve::*;

impl McpView {
    /// The AI assistants settings window; returns whether the settings changed.
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
            RichText::new(format!("✨ {}", t(lang, "mcp_title")))
                .monospace()
                .color(theme.accent_color()),
        )
        .id(egui::Id::new("fasttail_mcp"))
        .open(&mut open)
        .resizable(true)
        .default_width(520.0)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                changed |= self.settings_body(ui, cfg, theme, lang);
            });
        });
        self.open = open;
        if !self.open {
            self.confirm_enable = false;
            self.roots_text = None;
        }
        changed
    }

    fn settings_body(
        &mut self,
        ui: &mut egui::Ui,
        cfg: &mut FastTailConfig,
        theme: &CyberTheme,
        lang: Language,
    ) -> bool {
        let mut changed = false;
        ui.label(RichText::new(t(lang, "mcp_intro")).color(theme.text_dim()));
        ui.add_space(4.0);
        let mut enabled = cfg.mcp_enabled;
        if ui.checkbox(&mut enabled, t(lang, "mcp_enable")).changed() {
            if enabled {
                self.confirm_enable = true;
            } else {
                cfg.mcp_enabled = false;
                changed = true;
            }
        }
        if self.confirm_enable {
            ui.label(RichText::new(t(lang, "mcp_warning")).color(theme.warn_color()));
            ui.horizontal(|ui| {
                if ui.button(t(lang, "mcp_confirm")).clicked() {
                    cfg.mcp_enabled = true;
                    self.confirm_enable = false;
                    changed = true;
                }
                if ui.button(t(lang, "session_cancel")).clicked() {
                    self.confirm_enable = false;
                }
            });
        }
        ui.horizontal(|ui| {
            ui.label(t(lang, "mcp_port"));
            changed |= ui
                .add(egui::DragValue::new(&mut cfg.mcp_port).range(1024..=65535))
                .changed();
        });
        ui.horizontal(|ui| {
            ui.label(t(lang, "mcp_token"));
            let shown = if cfg.mcp_token.len() > 12 {
                format!("{}…", &cfg.mcp_token[..12])
            } else {
                cfg.mcp_token.clone()
            };
            ui.label(RichText::new(shown).monospace());
            if ui
                .add_enabled(
                    !cfg.mcp_token.is_empty(),
                    egui::Button::new(t(lang, "mcp_copy_token")),
                )
                .on_disabled_hover_text(t(lang, "mcp_token_later"))
                .clicked()
            {
                ui.ctx().copy_text(cfg.mcp_token.clone());
            }
            if ui
                .button(t(lang, "mcp_regenerate"))
                .on_hover_text(t(lang, "mcp_regenerate_tip"))
                .clicked()
            {
                cfg.mcp_token = crate::web::new_token();
                changed = true;
            }
        });
        changed |= ui
            .checkbox(&mut cfg.mcp_allow_write, t(lang, "mcp_allow_write"))
            .on_hover_text(t(lang, "mcp_allow_write_tip"))
            .changed();
        ui.label(t(lang, "mcp_roots"));
        let text = self.roots_text.get_or_insert_with(|| {
            cfg.mcp_open_roots
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("; ")
        });
        let edit = ui.add_enabled(
            cfg.mcp_allow_write,
            egui::TextEdit::singleline(text).desired_width(f32::INFINITY),
        );
        let edit = edit.on_disabled_hover_text(t(lang, "mcp_roots_off"));
        if edit.changed() {
            cfg.mcp_open_roots = text
                .split(';')
                .map(str::trim)
                .filter(|r| !r.is_empty())
                .map(PathBuf::from)
                .collect();
            changed = true;
        }
        match (&self.error, self.port()) {
            (Some(e), _) => {
                ui.label(
                    RichText::new(t(lang, "mcp_failed").replace("{0}", e))
                        .color(theme.warn_color()),
                );
            }
            (None, Some(port)) => {
                ui.label(
                    RichText::new(
                        t(lang, "mcp_listening").replace("{0}", &format!("127.0.0.1:{port}")),
                    )
                    .color(theme.text_dim()),
                );
            }
            (None, None) => {}
        }
        ui.separator();
        for (label, snippet) in [
            ("mcp_snippet_stdio", Self::stdio_snippet()),
            ("mcp_snippet_http", Self::http_snippet(cfg)),
        ] {
            ui.horizontal(|ui| {
                ui.label(t(lang, label));
                if ui.small_button(t(lang, "mcp_copy")).clicked() {
                    ui.ctx().copy_text(snippet.clone());
                }
            });
            let mut shown = snippet.as_str();
            ui.add(
                egui::TextEdit::multiline(&mut shown)
                    .code_editor()
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            );
        }
        ui.separator();
        ui.label(RichText::new(t(lang, "mcp_log")).strong());
        if self.log.is_empty() {
            ui.label(RichText::new(t(lang, "mcp_log_empty")).color(theme.text_dim()));
        }
        egui::ScrollArea::vertical()
            .id_salt("mcp_log")
            .max_height(180.0)
            .show(ui, |ui| {
                let now = Instant::now();
                for entry in self.log.iter().rev() {
                    let ago = now.duration_since(entry.at).as_secs();
                    ui.label(
                        RichText::new(format!(
                            "{:>6}  {}  {}  {} B",
                            t(lang, "mcp_ago").replace("{0}", &ago.to_string()),
                            entry.tool,
                            entry.args,
                            entry.bytes
                        ))
                        .monospace()
                        .size(11.0),
                    );
                }
            });
        changed
    }
}
