// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The "Event Log" dialog (Windows): the channels this user can list, with a search box,
//! and the pre-filter (levels and event IDs) Windows applies before the events reach the
//! stream.

use crate::eventlog::{EventLogSpec, PreFilter, Source};
use crate::i18n::{t, Language};
use crate::theme::CyberTheme;
use eframe::egui;
use egui::RichText;
use std::path::PathBuf;

/// The dialog's state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventLogDialog {
    /// The channels listed when the dialog opened.
    pub channels: Vec<String>,
    pub search: String,
    /// The channel picked, or typed.
    pub channel: String,
    /// Critical, Error, Warning, Information, Verbose.
    pub levels: [bool; 5],
    /// `7031, 7034-7040`.
    pub ids: String,
    /// Another computer whose channel is read (empty: this one).
    pub host: String,
}

impl Default for EventLogDialog {
    fn default() -> Self {
        Self {
            channels: Vec::new(),
            search: String::new(),
            channel: "Application".to_string(),
            levels: [false; 5],
            ids: String::new(),
            host: String::new(),
        }
    }
}

/// What the dialog was closed with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventLogChoice {
    Open(PathBuf),
    Cancel,
}

impl EventLogDialog {
    /// The dialog with the channels of this machine listed.
    pub fn with_channels() -> Self {
        Self {
            channels: crate::eventlog::channels(),
            ..Default::default()
        }
    }

    /// The stream the dialog would open, or the i18n key of what is wrong.
    pub fn spec(&self) -> Result<EventLogSpec, &'static str> {
        let channel = self.channel.trim();
        if channel.is_empty() {
            return Err("eventlog_no_channel");
        }
        let levels = (1..=5u8)
            .filter(|&l| self.levels[(l - 1) as usize])
            .collect();
        Ok(EventLogSpec {
            source: Source::Channel(channel.to_string()),
            filter: PreFilter {
                levels,
                ids: PreFilter::parse_ids(&self.ids)?,
            },
            host: Some(self.host.trim().to_string()).filter(|h| !h.is_empty()),
        })
    }

    /// Draws the dialog; `Some` once it is closed.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        lang: Language,
        theme: CyberTheme,
    ) -> Option<EventLogChoice> {
        let mut is_open = true;
        let mut outcome = None;
        egui::Window::new(
            RichText::new(format!("🗒 {}", t(lang, "eventlog_title")))
                .monospace()
                .color(theme.accent_color()),
        )
        .id(egui::Id::new("fasttail_eventlog_dialog"))
        .open(&mut is_open)
        .resizable(true)
        .collapsible(false)
        .default_width(460.0)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.label(
                RichText::new(t(lang, "eventlog_desc"))
                    .monospace()
                    .size(11.5)
                    .color(theme.text_dim()),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(t(lang, "eventlog_host"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.host)
                        .desired_width(200.0)
                        .hint_text(t(lang, "eventlog_host_hint")),
                )
                .on_hover_text(t(lang, "eventlog_host_tip"));
            });
            ui.horizontal(|ui| {
                ui.label(t(lang, "eventlog_channel"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.channel)
                        .font(egui::TextStyle::Monospace)
                        .desired_width(300.0),
                );
            });
            ui.horizontal(|ui| {
                ui.label("🔍");
                ui.add(
                    egui::TextEdit::singleline(&mut self.search)
                        .hint_text(t(lang, "eventlog_search"))
                        .desired_width(300.0),
                );
            });
            let mut open_now = false;
            let needle = self.search.trim().to_lowercase();
            egui::ScrollArea::vertical()
                .id_salt("fasttail_eventlog_channels")
                .max_height(220.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    if self.channels.is_empty() {
                        ui.label(
                            RichText::new(t(lang, "eventlog_no_list"))
                                .italics()
                                .color(theme.text_dim()),
                        );
                    }
                    for c in self
                        .channels
                        .iter()
                        .filter(|c| needle.is_empty() || c.to_lowercase().contains(&needle))
                    {
                        let resp =
                            ui.selectable_label(*c == self.channel, RichText::new(c).monospace());
                        if resp.clicked() {
                            self.channel = c.clone();
                        }
                        if resp.double_clicked() {
                            self.channel = c.clone();
                            open_now = true;
                        }
                    }
                });
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(t(lang, "eventlog_levels"));
                let names = [
                    "eventlog_level_critical",
                    "eventlog_level_error",
                    "eventlog_level_warning",
                    "eventlog_level_info",
                    "eventlog_level_verbose",
                ];
                for (on, key) in self.levels.iter_mut().zip(names) {
                    ui.checkbox(on, t(lang, key));
                }
            })
            .response
            .on_hover_text(t(lang, "eventlog_levels_tip"));
            ui.horizontal(|ui| {
                ui.label(t(lang, "eventlog_ids"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.ids)
                        .hint_text("7031, 7034-7040")
                        .desired_width(220.0),
                )
                .on_hover_text(t(lang, "eventlog_ids_tip"));
            });
            let spec = self.spec();
            match &spec {
                Ok(spec) => ui.label(
                    RichText::new(spec.to_path().display().to_string())
                        .monospace()
                        .small()
                        .color(theme.text_dim()),
                ),
                Err(key) => ui.label(
                    RichText::new(t(lang, key))
                        .small()
                        .color(theme.warn_color()),
                ),
            };
            let esc = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
            ui.horizontal(|ui| {
                let tip = match &spec {
                    Ok(_) => "",
                    Err(key) => t(lang, key),
                };
                let open = ui
                    .add_enabled(spec.is_ok(), egui::Button::new(t(lang, "system_open")))
                    .on_disabled_hover_text(tip)
                    .clicked();
                if (open || open_now) && spec.is_ok() {
                    if let Ok(spec) = &spec {
                        outcome = Some(EventLogChoice::Open(spec.to_path()));
                    }
                }
                if ui.button(t(lang, "open_pattern_cancel")).clicked() || esc {
                    outcome = Some(EventLogChoice::Cancel);
                }
            });
        });
        if !is_open {
            outcome = Some(EventLogChoice::Cancel);
        }
        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_uri_follows_the_choices() {
        let mut d = EventLogDialog::default();
        assert_eq!(
            d.spec().unwrap().to_path(),
            PathBuf::from("eventlog://Application")
        );
        d.channel = "Microsoft-Windows-PowerShell/Operational".into();
        d.levels = [true, true, false, false, false];
        d.ids = "4104, 4100-4103".into();
        assert_eq!(
            d.spec().unwrap().to_path(),
            PathBuf::from(
                "eventlog://Microsoft-Windows-PowerShell%2FOperational?levels=1,2&ids=4104,4100-4103"
            )
        );
        d.ids = "x".into();
        assert!(d.spec().is_err());
        d.ids.clear();
        d.channel = "  ".into();
        assert!(d.spec().is_err());
    }
}
