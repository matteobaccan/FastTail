// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The "Open system source" dialog: the kind (a Docker container, the systemd journal or
//! the user's journal, the journal on Linux only), the containers or units listed in the
//! background, a name field, and Open.

use crate::i18n::{t, Language};
use crate::system_source::{Commands, ListKind, Listed, Listing, SystemUri};
use crate::theme::CyberTheme;
use eframe::egui;
use egui::RichText;
use std::path::PathBuf;
use std::sync::Arc;

/// The kinds the dialog offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Docker,
    Journal,
    UserJournal,
}

impl Kind {
    /// The kinds this platform can open.
    pub fn offered() -> &'static [Kind] {
        if cfg!(target_os = "linux") {
            &[Kind::Docker, Kind::Journal, Kind::UserJournal]
        } else {
            &[Kind::Docker]
        }
    }

    fn label(self, lang: Language) -> &'static str {
        match self {
            Kind::Docker => t(lang, "system_kind_docker"),
            Kind::Journal => t(lang, "system_kind_journal"),
            Kind::UserJournal => t(lang, "system_kind_user"),
        }
    }

    fn list_kind(self) -> ListKind {
        match self {
            Kind::Docker => ListKind::Docker,
            Kind::Journal => ListKind::Units { user: false },
            Kind::UserJournal => ListKind::Units { user: true },
        }
    }
}

/// The dialog's state.
pub struct SystemDialog {
    pub kind: Kind,
    pub name: String,
    listing: Option<Listing>,
    listed: Result<Vec<Listed>, String>,
    /// The listing of `kind` was asked for.
    listed_kind: Option<Kind>,
}

/// What the dialog was closed with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SystemChoice {
    Open(PathBuf),
    Cancel,
}

impl Default for SystemDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemDialog {
    pub fn new() -> Self {
        Self {
            kind: Kind::Docker,
            name: String::new(),
            listing: None,
            listed: Ok(Vec::new()),
            listed_kind: None,
        }
    }

    /// The URI the dialog would open, or why it cannot.
    pub fn uri(&self) -> Result<SystemUri, &'static str> {
        let name = self.name.trim();
        let text = match self.kind {
            Kind::Docker => format!("docker://{name}"),
            Kind::Journal => format!("journald://{name}"),
            Kind::UserJournal if name.is_empty() => "journald://user".to_string(),
            Kind::UserJournal => format!("journald://user/{name}"),
        };
        SystemUri::parse(std::path::Path::new(&text)).unwrap_or(Err("system_bad_container"))
    }

    fn refresh(&mut self, commands: &Commands, ctx: &egui::Context) {
        let repaint = ctx.clone();
        self.listing = Some(Listing::start(
            self.kind.list_kind(),
            commands,
            Some(Arc::new(move || repaint.request_repaint())),
        ));
        self.listed = Ok(Vec::new());
        self.listed_kind = Some(self.kind);
    }

    /// Draws the dialog; `Some` once it is closed.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        commands: &Commands,
        lang: Language,
        theme: CyberTheme,
    ) -> Option<SystemChoice> {
        if self.listed_kind != Some(self.kind) {
            self.refresh(commands, ctx);
        }
        if let Some(result) = self.listing.as_ref().and_then(Listing::poll) {
            self.listed = result;
            self.listing = None;
        }
        let mut is_open = true;
        let mut outcome = None;
        egui::Window::new(
            RichText::new(format!("📦 {}", t(lang, "system_title")))
                .monospace()
                .color(theme.accent_color()),
        )
        .id(egui::Id::new("fasttail_system_dialog"))
        .open(&mut is_open)
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.label(
                RichText::new(t(lang, "system_desc"))
                    .monospace()
                    .size(11.5)
                    .color(theme.text_dim()),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                for &kind in Kind::offered() {
                    ui.radio_value(&mut self.kind, kind, kind.label(lang));
                }
                if ui
                    .button("⟳")
                    .on_hover_text(t(lang, "system_refresh"))
                    .clicked()
                {
                    self.refresh(commands, ctx);
                }
            });
            ui.add_space(4.0);
            let mut open_now = false;
            egui::ScrollArea::vertical()
                .max_height(260.0)
                .min_scrolled_height(120.0)
                .show(ui, |ui| {
                    ui.set_min_width(460.0);
                    match (&self.listed, self.listing.is_some()) {
                        (_, true) => {
                            ui.label(
                                RichText::new(format!("⏳ {}", t(lang, "system_listing")))
                                    .color(theme.text_dim()),
                            );
                        }
                        (Err(err), false) => {
                            ui.label(RichText::new(format!("⚠ {err}")).color(theme.warn_color()));
                        }
                        (Ok(list), false) if list.is_empty() => {
                            ui.label(
                                RichText::new(t(lang, "system_list_empty"))
                                    .italics()
                                    .color(theme.text_dim()),
                            );
                        }
                        (Ok(list), false) => {
                            for entry in list {
                                let picked = self.name.trim() == entry.name;
                                let resp = ui.selectable_label(
                                    picked,
                                    RichText::new(format!("{}  {}", entry.name, entry.detail))
                                        .monospace(),
                                );
                                if resp.clicked() {
                                    self.name = entry.name.clone();
                                }
                                if resp.double_clicked() {
                                    self.name = entry.name.clone();
                                    open_now = true;
                                }
                            }
                        }
                    }
                });
            ui.add_space(4.0);
            let hint = match self.kind {
                Kind::Docker => t(lang, "system_hint_container"),
                _ => t(lang, "system_hint_unit"),
            };
            let field = ui.add(
                egui::TextEdit::singleline(&mut self.name)
                    .hint_text(hint)
                    .desired_width(460.0),
            );
            let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let uri = self.uri();
            match &uri {
                Ok(u) => ui.label(
                    RichText::new(u.to_uri())
                        .monospace()
                        .small()
                        .color(theme.text_dim()),
                ),
                Err(key) if !self.name.trim().is_empty() => ui.label(
                    RichText::new(t(lang, key))
                        .small()
                        .color(theme.warn_color()),
                ),
                Err(_) => ui.label(""),
            };
            let esc = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
            ui.horizontal(|ui| {
                let open = ui
                    .add_enabled(uri.is_ok(), egui::Button::new(t(lang, "system_open")))
                    .clicked();
                if (open || open_now || enter) && uri.is_ok() {
                    if let Ok(u) = &uri {
                        outcome = Some(SystemChoice::Open(PathBuf::from(u.to_uri())));
                    }
                }
                if ui.button(t(lang, "open_pattern_cancel")).clicked() || esc {
                    outcome = Some(SystemChoice::Cancel);
                }
            });
        });
        if !is_open {
            outcome = Some(SystemChoice::Cancel);
        }
        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_uri_follows_the_kind_and_the_name() {
        let mut d = SystemDialog::new();
        d.name = " api ".into();
        assert_eq!(d.uri().unwrap().to_uri(), "docker://api");
        d.name.clear();
        assert!(d.uri().is_err());
        if cfg!(target_os = "linux") {
            d.kind = Kind::Journal;
            assert_eq!(d.uri().unwrap().to_uri(), "journald://");
            d.kind = Kind::UserJournal;
            d.name = "a.service".into();
            assert_eq!(d.uri().unwrap().to_uri(), "journald://user/a.service");
        }
    }
}
