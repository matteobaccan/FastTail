// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The Open folder dialog (open menu, folder drop): a folder, file-name patterns
//! separated by `;`, the subfolder option and the mode, Follow newest (one pattern stream)
//! or Open all (a folder group).

use crate::i18n::{t, Language};
use crate::theme::CyberTheme;
use crate::wildcard::FolderSpec;
use eframe::egui;
use egui::RichText;
use std::path::{Path, PathBuf};

/// The patterns offered for a new folder.
pub const DEFAULT_PATTERNS: &str = "*.log";

/// The dialog's fields while it is shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderPrompt {
    pub folder: String,
    pub patterns: String,
    pub recursive: bool,
    pub open_all: bool,
}

/// What the dialog was closed with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FolderChoice {
    /// One stream on the newest match (the spec's pattern path).
    FollowNewest(FolderSpec),
    /// Every match as its own stream.
    OpenAll(FolderSpec),
    Cancel,
}

impl FolderPrompt {
    /// The dialog on `dir` with the default patterns.
    pub fn for_folder(dir: &Path) -> Self {
        Self {
            folder: dir.to_string_lossy().to_string(),
            patterns: DEFAULT_PATTERNS.to_string(),
            recursive: false,
            open_all: false,
        }
    }

    /// The dialog filled from a pattern path (`C:\logs\app-*.log`); a path that is not
    /// one is taken as the folder.
    pub fn from_text(text: &str) -> Self {
        match FolderSpec::parse(Path::new(text.trim())) {
            Some(spec) => Self {
                folder: spec.dir.to_string_lossy().to_string(),
                patterns: spec.patterns_text(),
                recursive: spec.recursive,
                open_all: false,
            },
            None => Self::for_folder(Path::new(text.trim())),
        }
    }

    /// The spec the fields describe, when the folder exists.
    pub fn spec(&self) -> Option<FolderSpec> {
        let dir = PathBuf::from(self.folder.trim());
        if self.folder.trim().is_empty() || !dir.is_dir() {
            return None;
        }
        Some(FolderSpec::new(dir, &self.patterns, self.recursive))
    }

    /// The choice the fields make on Open.
    pub fn choice(&self) -> Option<FolderChoice> {
        let spec = self.spec()?;
        Some(if self.open_all {
            FolderChoice::OpenAll(spec)
        } else {
            FolderChoice::FollowNewest(spec)
        })
    }

    /// Draws the dialog; `Some` once it is closed.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        lang: Language,
        theme: CyberTheme,
    ) -> Option<FolderChoice> {
        let mut is_open = true;
        let mut outcome = None;
        egui::Window::new(
            RichText::new(format!("📂 {}", t(lang, "open_folder")))
                .monospace()
                .color(theme.accent_color()),
        )
        .id(egui::Id::new("fasttail_folder_prompt"))
        .open(&mut is_open)
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.label(
                RichText::new(t(lang, "open_folder_desc"))
                    .monospace()
                    .size(11.5)
                    .color(theme.text_dim()),
            );
            ui.add_space(6.0);
            egui::Grid::new("fasttail_folder_prompt_grid")
                .num_columns(2)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    ui.label(t(lang, "open_folder_folder"));
                    ui.horizontal(|ui| {
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut self.folder)
                                .desired_width(340.0)
                                .id(egui::Id::new("fasttail_folder_prompt_dir")),
                        );
                        // Take the keyboard focus when the dialog opens, not later.
                        if !resp.has_focus() && ui.ctx().memory(|m| m.focused().is_none()) {
                            resp.request_focus();
                        }
                        if ui
                            .button("…")
                            .on_hover_text(t(lang, "open_folder_browse"))
                            .clicked()
                        {
                            let mut dialog = rfd::FileDialog::new();
                            let current = PathBuf::from(self.folder.trim());
                            if current.is_dir() {
                                dialog = dialog.set_directory(&current);
                            }
                            if let Some(dir) = dialog.pick_folder() {
                                self.folder = dir.to_string_lossy().to_string();
                            }
                        }
                    });
                    ui.end_row();
                    ui.label(t(lang, "open_folder_patterns"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.patterns)
                            .hint_text(DEFAULT_PATTERNS)
                            .desired_width(370.0),
                    )
                    .on_hover_text(t(lang, "open_folder_patterns_tip"));
                    ui.end_row();
                    ui.label("");
                    ui.checkbox(&mut self.recursive, t(lang, "open_folder_recursive"))
                        .on_hover_text(t(lang, "open_folder_recursive_tip"));
                    ui.end_row();
                    ui.label(t(lang, "open_folder_mode"));
                    ui.vertical(|ui| {
                        ui.radio_value(&mut self.open_all, false, t(lang, "open_folder_newest"))
                            .on_hover_text(t(lang, "open_folder_newest_tip"));
                        ui.radio_value(&mut self.open_all, true, t(lang, "open_folder_all"))
                            .on_hover_text(t(lang, "open_folder_all_tip"));
                    });
                    ui.end_row();
                });
            let choice = self.choice();
            if !self.folder.trim().is_empty() && choice.is_none() {
                ui.label(
                    RichText::new(format!("ⓘ {}", t(lang, "open_folder_invalid")))
                        .monospace()
                        .size(11.0)
                        .color(theme.warn_color()),
                );
            }
            let enter = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
            let esc = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let open = ui
                    .add_enabled(
                        choice.is_some(),
                        egui::Button::new(t(lang, "open_pattern_go")),
                    )
                    .on_disabled_hover_text(t(lang, "open_folder_invalid"))
                    .clicked();
                if open || enter {
                    if let Some(choice) = choice.clone() {
                        outcome = Some(choice);
                    }
                }
                if ui.button(t(lang, "open_pattern_cancel")).clicked() || esc {
                    outcome = Some(FolderChoice::Cancel);
                }
            });
        });
        if !is_open {
            outcome = Some(FolderChoice::Cancel);
        }
        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_make_the_choice() {
        let dir = tempfile::tempdir().unwrap();
        let mut prompt = FolderPrompt::for_folder(dir.path());
        assert_eq!(prompt.patterns, "*.log");
        match prompt.choice() {
            Some(FolderChoice::FollowNewest(spec)) => {
                assert_eq!(spec.path(), dir.path().join("*.log"));
            }
            other => panic!("{other:?}"),
        }
        prompt.open_all = true;
        prompt.recursive = true;
        prompt.patterns = "*.log; *.txt".into();
        match prompt.choice() {
            Some(FolderChoice::OpenAll(spec)) => {
                assert!(spec.recursive);
                assert_eq!(spec.patterns, vec!["*.log", "*.txt"]);
            }
            other => panic!("{other:?}"),
        }
        prompt.folder = dir.path().join("missing").to_string_lossy().to_string();
        assert!(prompt.choice().is_none());
    }

    #[test]
    fn a_pattern_path_fills_the_fields() {
        let p = FolderPrompt::from_text("logs/**/app-*.log;*.txt");
        assert_eq!(p.folder, "logs");
        assert_eq!(p.patterns, "app-*.log;*.txt");
        assert!(p.recursive);
        let p = FolderPrompt::from_text("logs");
        assert_eq!(p.folder, "logs");
        assert_eq!(p.patterns, DEFAULT_PATTERNS);
    }
}
