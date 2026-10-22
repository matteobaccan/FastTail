// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The "New merged view" dialog: the open streams with a check box each, those without
//! timestamps greyed out with the reason, and Create once two or more are ticked.

use crate::i18n::{t, Language};
use crate::tail_engine::TailEngine;
use crate::theme::CyberTheme;
use eframe::egui;
use egui::RichText;
use std::path::PathBuf;

/// The dialog's state while it is shown: the streams ticked, by path.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergePrompt {
    pub picked: Vec<PathBuf>,
}

/// What the dialog was closed with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeChoice {
    /// The streams to merge, in the order of the open streams.
    Create(Vec<PathBuf>),
    Cancel,
}

/// Whether `engine` can be a source of a merged view: not a merged view itself, and its
/// lines carry timestamps.
pub fn mergeable(engine: &TailEngine) -> bool {
    engine.merged.is_none() && crate::merged_stream::has_timestamps(engine)
}

impl MergePrompt {
    /// The dialog with `first` ticked (the stream whose menu opened it).
    pub fn starting_with(first: Option<PathBuf>) -> Self {
        Self {
            picked: first.into_iter().collect(),
        }
    }

    /// The streams ticked that can be merged, in the order of `engines`.
    pub fn sources(&self, engines: &[TailEngine]) -> Vec<PathBuf> {
        engines
            .iter()
            .filter(|e| self.picked.contains(&e.path) && mergeable(e))
            .map(|e| e.path.clone())
            .take(crate::merged_stream::MAX_SOURCES)
            .collect()
    }

    /// Draws the dialog; `Some` once it is closed.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        engines: &[TailEngine],
        lang: Language,
        theme: CyberTheme,
    ) -> Option<MergeChoice> {
        let mut is_open = true;
        let mut outcome = None;
        egui::Window::new(
            RichText::new(format!("⋈ {}", t(lang, "merge_title")))
                .monospace()
                .color(theme.accent_color()),
        )
        .id(egui::Id::new("fasttail_merge_prompt"))
        .open(&mut is_open)
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.label(
                RichText::new(t(lang, "merge_desc"))
                    .monospace()
                    .size(11.5)
                    .color(theme.text_dim()),
            );
            ui.add_space(6.0);
            egui::ScrollArea::vertical()
                .max_height(320.0)
                .show(ui, |ui| {
                    for e in engines.iter().filter(|e| e.merged.is_none()) {
                        let ok = mergeable(e);
                        let mut on = ok && self.picked.contains(&e.path);
                        let name = crate::find_all::stream_name(e);
                        let resp = ui
                            .add_enabled(ok, egui::Checkbox::new(&mut on, name))
                            .on_hover_text(e.path.display().to_string())
                            .on_disabled_hover_text(t(lang, "merge_no_timestamps"));
                        if resp.changed() {
                            self.picked.retain(|p| p != &e.path);
                            if on {
                                self.picked.push(e.path.clone());
                            }
                        }
                        if !ok {
                            ui.label(
                                RichText::new(format!("   ⓘ {}", t(lang, "merge_no_timestamps")))
                                    .monospace()
                                    .size(11.0)
                                    .color(theme.warn_color()),
                            );
                        }
                    }
                });
            let sources = self.sources(engines);
            ui.add_space(6.0);
            let esc = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        sources.len() >= 2,
                        egui::Button::new(t(lang, "merge_create")),
                    )
                    .on_disabled_hover_text(t(lang, "merge_need_two"))
                    .clicked()
                {
                    outcome = Some(MergeChoice::Create(sources.clone()));
                }
                if ui.button(t(lang, "open_pattern_cancel")).clicked() || esc {
                    outcome = Some(MergeChoice::Cancel);
                }
            });
        });
        if !is_open {
            outcome = Some(MergeChoice::Cancel);
        }
        outcome
    }
}
