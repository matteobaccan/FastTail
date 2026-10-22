// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The "Split file" dialog: by lines, by size or by time, all lines or the visible ones,
//! the folder, a preview of the pieces (and of the files it would replace), then the
//! split itself a few milliseconds per frame with a progress bar and Cancel.

use crate::i18n::{t, Language};
use crate::split::{preview, SplitBy, SplitJob, SplitPlan};
use crate::tail_engine::TailEngine;
use crate::theme::CyberTheme;
use eframe::egui;
use egui::RichText;
use std::path::PathBuf;
use std::time::Duration;

/// Time a frame spends writing pieces.
const FRAME_BUDGET: Duration = Duration::from_millis(8);

/// How the dialog cuts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Lines,
    Size,
    Time,
}

/// The time spans offered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Span {
    Hour,
    Day,
    Minutes,
}

/// The dialog's state.
#[derive(Debug)]
pub struct SplitDialog {
    /// The stream it splits (`TailEngine::path`).
    pub source: PathBuf,
    pub mode: Mode,
    pub lines: u64,
    pub megabytes: u64,
    pub span: Span,
    pub minutes: u64,
    pub visible_only: bool,
    pub dir: PathBuf,
    /// The user agreed to replace the files that exist.
    pub replace: bool,
    pub job: Option<SplitJob>,
    /// What the last split did (an error in red).
    pub notice: Option<(String, bool)>,
}

/// What the app has to do after a frame of the dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitAction {
    Close,
}

impl SplitDialog {
    /// The dialog on stream `source` (the file `file` on disk), the pieces next to it.
    pub fn new(source: PathBuf, file: &std::path::Path) -> Self {
        let dir = file
            .parent()
            .map_or_else(|| PathBuf::from("."), std::path::Path::to_path_buf);
        Self {
            source,
            mode: Mode::Lines,
            lines: 100_000,
            megabytes: 10,
            span: Span::Hour,
            minutes: 30,
            visible_only: false,
            dir,
            replace: false,
            job: None,
            notice: None,
        }
    }

    /// How to cut, as chosen.
    pub fn by(&self) -> SplitBy {
        match self.mode {
            Mode::Lines => SplitBy::Lines(self.lines.max(1)),
            Mode::Size => SplitBy::Bytes(self.megabytes.max(1) * 1024 * 1024),
            Mode::Time => SplitBy::Time(match self.span {
                Span::Hour => 3_600_000,
                Span::Day => 86_400_000,
                Span::Minutes => self.minutes.max(1) as i64 * 60_000,
            }),
        }
    }

    /// The plan for `engine`.
    pub fn plan(&self, engine: &TailEngine) -> SplitPlan {
        let file = engine
            .current_file
            .clone()
            .unwrap_or_else(|| engine.path.clone());
        let mut plan = SplitPlan::for_file(&file, self.by());
        plan.visible_only = self.visible_only;
        plan.dir = self.dir.clone();
        plan
    }

    /// Draws the dialog and runs the split a slice per frame; `Some` when closed.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        engine: &mut TailEngine,
        lang: Language,
        theme: CyberTheme,
    ) -> Option<SplitAction> {
        let mut is_open = true;
        let mut action = None;
        // The split writes while the dialog is shown.
        if let Some(job) = self.job.as_mut() {
            if job.step(engine, FRAME_BUDGET) {
                let job = self.job.take().expect("a job");
                self.notice = Some(match &job.error {
                    Some(e) => (t(lang, "split_failed").replace("{0}", e), true),
                    None if job.cancelled => (
                        t(lang, "split_cancelled").replace("{0}", &job.written.len().to_string()),
                        false,
                    ),
                    None => (
                        t(lang, "split_done")
                            .replace("{0}", &job.written.len().to_string())
                            .replace("{1}", &self.dir.display().to_string()),
                        false,
                    ),
                });
            } else {
                ctx.request_repaint();
            }
        }
        if self.mode == Mode::Time && !engine.timestamps_complete() {
            engine.ensure_timestamps();
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        egui::Window::new(
            RichText::new(format!("✂ {}", t(lang, "split_title")))
                .monospace()
                .color(theme.accent_color()),
        )
        .id(egui::Id::new("fasttail_split_dialog"))
        .open(&mut is_open)
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            let busy = self.job.is_some();
            ui.add_enabled_ui(!busy, |ui| self.options(ui, lang));
            ui.separator();
            let plan = self.plan(engine);
            let p = preview(engine, &plan);
            let free = crate::compressed::free_space(&self.dir).map(|(b, _)| b);
            let no_space = free.is_some_and(|f| f < p.bytes);
            let size = crate::ui::zip_picker::human_size(p.bytes);
            if p.waiting_for_timestamps {
                ui.label(
                    RichText::new(format!("⏳ {}", t(lang, "split_waiting")))
                        .small()
                        .color(theme.text_dim()),
                );
            } else if p.no_timestamps {
                ui.label(
                    RichText::new(t(lang, "split_no_timestamps"))
                        .small()
                        .color(theme.warn_color()),
                );
            } else {
                let key = if self.mode == Mode::Time {
                    "split_preview_time"
                } else {
                    "split_preview"
                };
                ui.label(
                    RichText::new(
                        t(lang, key)
                            .replace("{0}", &p.pieces.to_string())
                            .replace("{1}", &size),
                    )
                    .monospace(),
                );
                for name in &p.names {
                    ui.label(RichText::new(format!("  {name}")).monospace().small());
                }
                if p.pieces > p.names.len() {
                    ui.label(RichText::new("  …").monospace().small());
                }
            }
            if !p.existing.is_empty() {
                ui.label(
                    RichText::new(
                        t(lang, "split_existing").replace("{0}", &p.existing.len().to_string()),
                    )
                    .color(theme.warn_color()),
                );
                ui.add_enabled(
                    !busy,
                    egui::Checkbox::new(&mut self.replace, t(lang, "split_replace")),
                );
            }
            if no_space {
                ui.label(
                    RichText::new(
                        t(lang, "split_no_space")
                            .replace("{0}", &size)
                            .replace("{1}", &crate::ui::zip_picker::human_size(free.unwrap_or(0))),
                    )
                    .color(theme.error_color()),
                );
            }
            ui.add_space(4.0);
            if let Some(job) = &self.job {
                ui.add(egui::ProgressBar::new(job.progress()).show_percentage());
            }
            ui.horizontal(|ui| {
                let ready = !busy
                    && !p.waiting_for_timestamps
                    && !p.no_timestamps
                    && p.pieces > 0
                    && !no_space
                    && (p.existing.is_empty() || self.replace);
                // While the split runs the progress bar says why the button is off.
                let disabled_tip = if busy {
                    t(lang, "split_tip").to_string()
                } else if p.waiting_for_timestamps {
                    t(lang, "split_waiting").to_string()
                } else if p.no_timestamps {
                    t(lang, "split_no_timestamps").to_string()
                } else if no_space {
                    t(lang, "split_no_space")
                        .replace("{0}", &size)
                        .replace("{1}", &crate::ui::zip_picker::human_size(free.unwrap_or(0)))
                } else if !p.existing.is_empty() && !self.replace {
                    t(lang, "split_existing").replace("{0}", &p.existing.len().to_string())
                } else {
                    t(lang, "split_tip").to_string()
                };
                if ui
                    .add_enabled(ready, egui::Button::new(t(lang, "split_start")))
                    .on_hover_text(t(lang, "split_tip"))
                    .on_disabled_hover_text(disabled_tip)
                    .clicked()
                {
                    self.notice = None;
                    self.job = Some(SplitJob::start(engine, plan));
                }
                if busy {
                    if ui.button(t(lang, "open_pattern_cancel")).clicked() {
                        if let Some(job) = self.job.as_mut() {
                            job.cancel();
                        }
                    }
                } else if ui.button(t(lang, "split_close")).clicked() {
                    action = Some(SplitAction::Close);
                }
            });
            if let Some((text, error)) = &self.notice {
                let color = if *error {
                    theme.error_color()
                } else {
                    theme.text_dim()
                };
                ui.label(RichText::new(text).small().color(color));
            }
        });
        if !is_open {
            if let Some(job) = self.job.as_mut() {
                job.cancel();
            }
            action = Some(SplitAction::Close);
        }
        action
    }

    /// How to cut, which lines, and where.
    fn options(&mut self, ui: &mut egui::Ui, lang: Language) {
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.mode, Mode::Lines, t(lang, "split_by_lines"));
            ui.add(egui::DragValue::new(&mut self.lines).range(1..=u64::MAX / 2));
        });
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.mode, Mode::Size, t(lang, "split_by_size"));
            ui.add(
                egui::DragValue::new(&mut self.megabytes)
                    .range(1..=1_048_576)
                    .suffix(" MB"),
            );
        });
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.mode, Mode::Time, t(lang, "split_by_time"));
            ui.radio_value(&mut self.span, Span::Hour, t(lang, "split_hour"));
            ui.radio_value(&mut self.span, Span::Day, t(lang, "split_day"));
            ui.radio_value(&mut self.span, Span::Minutes, t(lang, "split_minutes"));
            ui.add_enabled(
                self.span == Span::Minutes,
                egui::DragValue::new(&mut self.minutes).range(1..=10_080),
            );
        });
        ui.checkbox(&mut self.visible_only, t(lang, "split_visible"));
        ui.horizontal(|ui| {
            ui.label(format!("{}:", t(lang, "split_folder")));
            ui.label(
                RichText::new(self.dir.display().to_string())
                    .monospace()
                    .small(),
            );
            if ui.button(t(lang, "split_choose")).clicked() {
                if let Some(dir) = rfd::FileDialog::new()
                    .set_directory(&self.dir)
                    .pick_folder()
                {
                    self.dir = dir;
                    self.replace = false;
                }
            }
        });
    }
}
