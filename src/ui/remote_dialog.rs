// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The "Remote file (SSH)" dialog: one or more hosts, the remote path, and how much of
//! the file to fetch first; Open opens one stream per host.

use crate::i18n::{t, Language};
use crate::theme::CyberTheme;
use eframe::egui;
use egui::RichText;
use std::path::PathBuf;

/// The dialog's state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RemoteDialog {
    pub hosts: String,
    pub path: String,
    /// Fetch the whole file instead of its last `remote_initial_mb`.
    pub whole: bool,
}

/// What the dialog was closed with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteChoice {
    /// One URL per host.
    Open(Vec<PathBuf>),
    Cancel,
}

impl RemoteDialog {
    /// The URLs to open, or the first refusal (the host and an i18n key).
    pub fn urls(&self) -> Result<Vec<PathBuf>, (String, &'static str)> {
        let urls: Result<Vec<String>, _> =
            crate::ssh_source::urls_for(&self.hosts, &self.path, self.whole)
                .into_iter()
                .collect();
        urls.map(|u| u.into_iter().map(PathBuf::from).collect())
    }

    /// Draws the dialog; `Some` once it is closed. `initial_mb` is shown on the "last N MB"
    /// choice.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        initial_mb: u32,
        lang: Language,
        theme: CyberTheme,
    ) -> Option<RemoteChoice> {
        let mut is_open = true;
        let mut outcome = None;
        egui::Window::new(
            RichText::new(format!("🌐 {}", t(lang, "remote_title")))
                .monospace()
                .color(theme.accent_color()),
        )
        .id(egui::Id::new("fasttail_remote_dialog"))
        .open(&mut is_open)
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.label(
                RichText::new(t(lang, "remote_desc"))
                    .monospace()
                    .size(11.5)
                    .color(theme.text_dim()),
            );
            ui.add_space(6.0);
            egui::Grid::new("fasttail_remote_grid")
                .num_columns(2)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    ui.label(t(lang, "remote_hosts"));
                    ui.add(
                        egui::TextEdit::multiline(&mut self.hosts)
                            .hint_text("deploy@web1, web2, web3:2222")
                            .desired_rows(2)
                            .desired_width(380.0),
                    )
                    .on_hover_text(t(lang, "remote_hosts_tip"));
                    ui.end_row();
                    ui.label(t(lang, "remote_path"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.path)
                            .hint_text("/var/log/app.log")
                            .desired_width(380.0),
                    );
                    ui.end_row();
                    ui.label(t(lang, "remote_initial"));
                    ui.horizontal(|ui| {
                        ui.radio_value(
                            &mut self.whole,
                            false,
                            t(lang, "remote_last_mb").replace("{0}", &initial_mb.to_string()),
                        );
                        ui.radio_value(&mut self.whole, true, t(lang, "remote_whole"));
                    });
                    ui.end_row();
                });
            ui.add_space(4.0);
            let urls = self.urls();
            match &urls {
                Ok(list) if list.is_empty() => {
                    ui.label("");
                }
                Ok(list) => {
                    for url in list {
                        ui.label(
                            RichText::new(url.display().to_string())
                                .monospace()
                                .small()
                                .color(theme.text_dim()),
                        );
                    }
                }
                Err((host, key)) => {
                    ui.label(
                        RichText::new(format!("{host}: {}", t(lang, key)))
                            .small()
                            .color(theme.warn_color()),
                    );
                }
            }
            ui.label(
                RichText::new(t(lang, "remote_auth_note"))
                    .small()
                    .color(theme.text_dim()),
            );
            let esc = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
            ui.horizontal(|ui| {
                let ready = urls.as_ref().is_ok_and(|l| !l.is_empty());
                if ui
                    .add_enabled(ready, egui::Button::new(t(lang, "system_open")))
                    .clicked()
                {
                    if let Ok(list) = &urls {
                        outcome = Some(RemoteChoice::Open(list.clone()));
                    }
                }
                if ui.button(t(lang, "open_pattern_cancel")).clicked() || esc {
                    outcome = Some(RemoteChoice::Cancel);
                }
            });
        });
        if !is_open {
            outcome = Some(RemoteChoice::Cancel);
        }
        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_and_path_give_one_url_each() {
        let d = RemoteDialog {
            hosts: "web1, web2".into(),
            path: "/var/log/app.log".into(),
            whole: false,
        };
        assert_eq!(
            d.urls().unwrap(),
            [
                PathBuf::from("ssh://web1/var/log/app.log"),
                PathBuf::from("ssh://web2/var/log/app.log")
            ]
        );
        let bad = RemoteDialog {
            hosts: "web1".into(),
            path: "app.log".into(),
            whole: false,
        };
        assert!(bad.urls().is_err());
    }
}
