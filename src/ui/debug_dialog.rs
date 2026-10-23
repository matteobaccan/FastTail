// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The "Debug output" dialog (Windows): this session or every session (administrators),
//! and the processes to include or leave out.

use crate::debug_output::{DbwinSpec, Filter};
use crate::i18n::{t, Language};
use crate::theme::CyberTheme;
use eframe::egui;
use egui::RichText;
use std::path::PathBuf;

/// The dialog's state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DebugDialog {
    pub global: bool,
    /// Comma-separated names (`*` / `?`) or process ids.
    pub include: String,
    pub exclude: String,
}

/// What the dialog was closed with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DebugChoice {
    Open(PathBuf),
    Cancel,
}

fn entries(text: &str) -> Vec<String> {
    text.split(',')
        .map(str::trim)
        // `&` and `?` would end the URI's value: the wildcard `?` stays, `&` cannot.
        .filter(|e| !e.is_empty() && !e.contains('&'))
        .map(str::to_string)
        .collect()
}

impl DebugDialog {
    /// The capture the dialog would open.
    pub fn spec(&self) -> DbwinSpec {
        DbwinSpec {
            global: self.global,
            filter: Filter {
                include: entries(&self.include),
                exclude: entries(&self.exclude),
            },
        }
    }

    /// Draws the dialog; `Some` once it is closed.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        lang: Language,
        theme: CyberTheme,
    ) -> Option<DebugChoice> {
        let mut is_open = true;
        let mut outcome = None;
        egui::Window::new(
            RichText::new(format!("🪲 {}", t(lang, "dbwin_title")))
                .monospace()
                .color(theme.accent_color()),
        )
        .id(egui::Id::new("fasttail_debug_dialog"))
        .open(&mut is_open)
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.label(
                RichText::new(t(lang, "dbwin_desc"))
                    .monospace()
                    .size(11.5)
                    .color(theme.text_dim()),
            );
            ui.add_space(6.0);
            egui::Grid::new("fasttail_debug_grid")
                .num_columns(2)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    ui.label("");
                    ui.checkbox(&mut self.global, t(lang, "dbwin_global"))
                        .on_hover_text(t(lang, "dbwin_global_tip"));
                    ui.end_row();
                    ui.label(t(lang, "dbwin_include"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.include)
                            .hint_text("app.exe, 1234")
                            .desired_width(220.0),
                    )
                    .on_hover_text(t(lang, "dbwin_filter_tip"));
                    ui.end_row();
                    ui.label(t(lang, "dbwin_exclude"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.exclude)
                            .hint_text("svchost.exe")
                            .desired_width(220.0),
                    )
                    .on_hover_text(t(lang, "dbwin_filter_tip"));
                    ui.end_row();
                });
            ui.add_space(4.0);
            let spec = self.spec();
            ui.label(
                RichText::new(spec.to_uri())
                    .monospace()
                    .small()
                    .color(theme.text_dim()),
            );
            let esc = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
            ui.horizontal(|ui| {
                if ui.button(t(lang, "dbwin_start")).clicked() {
                    outcome = Some(DebugChoice::Open(PathBuf::from(spec.to_uri())));
                }
                if ui.button(t(lang, "open_pattern_cancel")).clicked() || esc {
                    outcome = Some(DebugChoice::Cancel);
                }
            });
        });
        if !is_open {
            outcome = Some(DebugChoice::Cancel);
        }
        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_uri_follows_the_choices() {
        let mut d = DebugDialog::default();
        assert_eq!(d.spec().to_uri(), "dbwin://local");
        d.global = true;
        d.include = " app.exe, ,42 ".into();
        d.exclude = "bad&x, svc?.exe".into();
        assert_eq!(
            d.spec().to_uri(),
            "dbwin://global?include=app.exe,42&exclude=svc?.exe"
        );
    }
}
