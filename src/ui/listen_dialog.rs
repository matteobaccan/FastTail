// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The "Listen" dialog: the protocol (syslog, TCP, UDP), the address (loopback unless the
//! user picks another, with a warning), the port and the sender prefix.

use crate::i18n::{t, Language};
use crate::net_listener::{ListenSpec, Proto};
use crate::theme::CyberTheme;
use eframe::egui;
use egui::RichText;
use std::path::PathBuf;

/// The dialog's state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListenDialog {
    pub proto: Proto,
    /// Loopback (`127.0.0.1`) unless the user unticks it and types an address.
    pub loopback: bool,
    pub address: String,
    pub port: u16,
    pub prefix: bool,
}

impl Default for ListenDialog {
    fn default() -> Self {
        Self {
            proto: Proto::Syslog,
            loopback: true,
            address: "0.0.0.0".to_string(),
            port: 5514,
            prefix: true,
        }
    }
}

/// What the dialog was closed with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListenChoice {
    Open(PathBuf),
    Cancel,
}

impl ListenDialog {
    /// The listener the dialog would open, or why it cannot (an i18n key).
    pub fn spec(&self) -> Result<ListenSpec, &'static str> {
        let scheme = match self.proto {
            Proto::Tcp => "tcp",
            Proto::Udp => "udp",
            Proto::Syslog => "syslog",
            Proto::Otlp => "otlp",
        };
        let host = if self.loopback {
            "127.0.0.1".to_string()
        } else {
            let a = self.address.trim();
            if a.contains(':') && !a.starts_with('[') {
                format!("[{a}]")
            } else {
                a.to_string()
            }
        };
        let default_prefix = matches!(self.proto, Proto::Udp | Proto::Syslog);
        let query = if self.prefix == default_prefix {
            String::new()
        } else if self.prefix {
            "?prefix=on".to_string()
        } else {
            "?prefix=off".to_string()
        };
        let url = format!("{scheme}://{host}:{}{query}", self.port);
        ListenSpec::parse(std::path::Path::new(&url)).unwrap_or(Err("listen_bad_url"))
    }

    /// Draws the dialog; `Some` once it is closed.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        lang: Language,
        theme: CyberTheme,
    ) -> Option<ListenChoice> {
        let mut is_open = true;
        let mut outcome = None;
        egui::Window::new(
            RichText::new(format!("📡 {}", t(lang, "listen_title")))
                .monospace()
                .color(theme.accent_color()),
        )
        .id(egui::Id::new("fasttail_listen_dialog"))
        .open(&mut is_open)
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.label(
                RichText::new(t(lang, "listen_desc"))
                    .monospace()
                    .size(11.5)
                    .color(theme.text_dim()),
            );
            ui.add_space(6.0);
            egui::Grid::new("fasttail_listen_grid")
                .num_columns(2)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    ui.label(t(lang, "listen_protocol"));
                    ui.horizontal(|ui| {
                        let before = self.proto;
                        ui.radio_value(&mut self.proto, Proto::Syslog, "syslog")
                            .on_hover_text(t(lang, "listen_syslog_tip"));
                        ui.radio_value(&mut self.proto, Proto::Tcp, "TCP");
                        ui.radio_value(&mut self.proto, Proto::Udp, "UDP");
                        ui.radio_value(&mut self.proto, Proto::Otlp, "OTLP")
                            .on_hover_text(t(lang, "listen_otlp_tip"));
                        if self.proto != before {
                            self.prefix = matches!(self.proto, Proto::Udp | Proto::Syslog);
                            // The usual ports of each kind.
                            if self.proto == Proto::Otlp && self.port == 5514 {
                                self.port = 4318;
                            } else if before == Proto::Otlp && self.port == 4318 {
                                self.port = 5514;
                            }
                        }
                    });
                    ui.end_row();
                    ui.label(t(lang, "listen_address"));
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.loopback, t(lang, "listen_loopback"));
                        ui.add_enabled(
                            !self.loopback,
                            egui::TextEdit::singleline(&mut self.address).desired_width(160.0),
                        );
                    });
                    ui.end_row();
                    ui.label(t(lang, "listen_port"));
                    ui.add(egui::DragValue::new(&mut self.port).range(1..=65535));
                    ui.end_row();
                    ui.label("");
                    ui.checkbox(&mut self.prefix, t(lang, "listen_prefix"));
                    ui.end_row();
                });
            ui.add_space(4.0);
            let spec = self.spec();
            match &spec {
                Ok(s) => {
                    ui.label(
                        RichText::new(s.to_uri())
                            .monospace()
                            .small()
                            .color(theme.text_dim()),
                    );
                    if s.is_exposed() {
                        ui.label(
                            RichText::new(format!("⚠ {}", t(lang, "listen_exposed")))
                                .small()
                                .color(theme.warn_color()),
                        );
                    }
                    if s.port < 1024 && !cfg!(windows) {
                        ui.label(
                            RichText::new(t(lang, "listen_low_port"))
                                .small()
                                .color(theme.text_dim()),
                        );
                    }
                }
                Err(key) => {
                    ui.label(
                        RichText::new(t(lang, key))
                            .small()
                            .color(theme.warn_color()),
                    );
                }
            }
            let esc = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
            let start_tip = match &spec {
                Ok(_) => "",
                Err(key) => t(lang, key),
            };
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(spec.is_ok(), egui::Button::new(t(lang, "listen_start")))
                    .on_disabled_hover_text(start_tip)
                    .clicked()
                {
                    if let Ok(s) = &spec {
                        outcome = Some(ListenChoice::Open(PathBuf::from(s.to_uri())));
                    }
                }
                if ui.button(t(lang, "open_pattern_cancel")).clicked() || esc {
                    outcome = Some(ListenChoice::Cancel);
                }
            });
        });
        if !is_open {
            outcome = Some(ListenChoice::Cancel);
        }
        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_url_follows_the_choices() {
        let mut d = ListenDialog::default();
        assert_eq!(d.spec().unwrap().to_uri(), "syslog://127.0.0.1:5514");
        d.proto = Proto::Tcp;
        d.prefix = false;
        d.port = 5140;
        assert_eq!(d.spec().unwrap().to_uri(), "tcp://127.0.0.1:5140");
        d.loopback = false;
        d.address = "::".into();
        let s = d.spec().unwrap();
        assert_eq!(s.to_uri(), "tcp://[::]:5140");
        assert!(s.is_exposed());
        d.address = "not an address".into();
        assert!(d.spec().is_err());
        d.loopback = true;
        d.proto = Proto::Otlp;
        d.port = 4318;
        assert_eq!(d.spec().unwrap().to_uri(), "otlp://127.0.0.1:4318");
    }
}
