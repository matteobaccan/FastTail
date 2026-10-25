// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The "Export" dialog of a stream: what (the visible lines, the selected ones, the
//! search matches), the format (text, CSV, HTML) and the CSV options, then the file; the
//! export runs a few milliseconds per frame with a progress bar and Cancel.

use crate::config::FastTailConfig;
use crate::export::{CsvOptions, ExportJob, Format, Scope};
use crate::i18n::{t, Language};
use crate::tail_engine::TailEngine;
use crate::theme::CyberTheme;
use eframe::egui;
use egui::RichText;
use std::path::PathBuf;
use std::time::Duration;

/// Time a frame spends writing.
const FRAME_BUDGET: Duration = Duration::from_millis(8);

/// The dialog's state.
#[derive(Debug)]
pub struct ExportDialog {
    /// The stream it exports (`TailEngine::path`).
    pub source: PathBuf,
    pub scope: Scope,
    pub format: Format,
    pub csv: CsvOptions,
    pub job: Option<ExportJob>,
    /// What the last export did (an error in red).
    pub notice: Option<(String, bool)>,
}

impl ExportDialog {
    /// The dialog on stream `source` with `scope`, the format and CSV options as last used.
    pub fn new(source: PathBuf, scope: Scope, config: &FastTailConfig) -> Self {
        Self {
            source,
            scope,
            format: Format::from_name(&config.export_format).unwrap_or(Format::Text),
            csv: CsvOptions {
                sep: crate::export::separator_from_name(&config.export_csv_separator)
                    .unwrap_or(b','),
                bom: config.export_csv_bom,
                protect: config.export_csv_protect,
            },
            job: None,
            notice: None,
        }
    }

    /// Keeps the format and the CSV options in `config` for the next export.
    pub fn remember(&self, config: &mut FastTailConfig) {
        config.export_format = self.format.name().to_string();
        config.export_csv_separator = crate::export::separator_name(self.csv.sep).to_string();
        config.export_csv_bom = self.csv.bom;
        config.export_csv_protect = self.csv.protect;
    }

    /// Starts the export of `engine` to `path` (tests and the Save button).
    pub fn start(
        &mut self,
        engine: &TailEngine,
        path: PathBuf,
        theme: CyberTheme,
        level_colors: bool,
    ) -> std::io::Result<()> {
        let title = engine
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let (writer, max): (Box<dyn crate::export::RowWriter>, Option<usize>) = match self.format {
            Format::Text => (Box::new(crate::export::TextWriter), None),
            Format::Csv => (Box::new(crate::export::CsvWriter::new(self.csv)), None),
            Format::Html => (
                Box::new(crate::ui::export_html::HtmlWriter::new(
                    engine,
                    theme,
                    level_colors,
                    &title,
                )),
                Some(crate::export::HTML_MAX_LINES),
            ),
        };
        self.notice = None;
        self.job = Some(ExportJob::start(engine, self.scope, path, writer, max)?);
        Ok(())
    }

    /// Draws the dialog and runs the export a slice per frame; true when closed.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        engine: &TailEngine,
        config: &mut FastTailConfig,
        lang: Language,
    ) -> bool {
        let theme = config.theme;
        let mut is_open = true;
        let mut close = false;
        if let Some(job) = self.job.as_mut() {
            if job.step(engine, FRAME_BUDGET) {
                let job = self.job.take().expect("a job");
                self.notice = Some(match (&job.error, job.cancelled) {
                    (Some(e), _) => (format!("{}: {e}", t(lang, "export_failed")), true),
                    (None, true) => (t(lang, "export_cancelled").to_string(), false),
                    (None, false) => {
                        let mut text = t(lang, "export_done")
                            .replace("{0}", &job.lines.to_string())
                            .replace("{1}", &job.path.display().to_string());
                        if job.capped {
                            text.push(' ');
                            text.push_str(&t(lang, "export_html_cap").replace(
                                "{0}",
                                &crate::ui::dock::group_thousands(crate::export::HTML_MAX_LINES),
                            ));
                        }
                        (text, false)
                    }
                });
            } else {
                ctx.request_repaint();
            }
        }
        egui::Window::new(
            RichText::new(format!("💾 {}", t(lang, "export_title")))
                .monospace()
                .color(theme.accent_color()),
        )
        .id(egui::Id::new("fasttail_export_dialog"))
        .open(&mut is_open)
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            let busy = self.job.is_some();
            ui.add_enabled_ui(!busy, |ui| self.options(ui, engine, lang));
            let count = crate::export::scope_len(engine, self.scope);
            ui.separator();
            ui.label(
                RichText::new(
                    t(lang, "export_count")
                        .replace("{0}", &crate::ui::dock::group_thousands(count)),
                )
                .monospace(),
            );
            if self.format == Format::Html && count > crate::export::HTML_MAX_LINES {
                ui.label(
                    RichText::new(t(lang, "export_html_cap").replace(
                        "{0}",
                        &crate::ui::dock::group_thousands(crate::export::HTML_MAX_LINES),
                    ))
                    .small()
                    .color(theme.warn_color()),
                );
            }
            if let Some(job) = &self.job {
                ui.add(egui::ProgressBar::new(job.progress()).show_percentage());
            }
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !busy && count > 0,
                        egui::Button::new(t(lang, "export_save")),
                    )
                    .on_disabled_hover_text(t(lang, "status_no_file"))
                    .clicked()
                {
                    let stem = engine
                        .path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("fasttail");
                    let name = format!("{stem}-export.{}", self.format.extension());
                    if let Some(path) = rfd::FileDialog::new()
                        .set_file_name(name)
                        .add_filter(self.format.name(), &[self.format.extension()])
                        .save_file()
                    {
                        self.remember(config);
                        let _ = config.save();
                        if let Err(e) = self.start(engine, path, theme, config.level_colors) {
                            self.notice =
                                Some((format!("{}: {e}", t(lang, "export_failed")), true));
                        }
                    }
                }
                if busy {
                    if ui.button(t(lang, "open_pattern_cancel")).clicked() {
                        if let Some(job) = self.job.as_mut() {
                            job.cancel();
                        }
                    }
                } else if ui.button(t(lang, "split_close")).clicked() {
                    close = true;
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
            close = true;
        }
        close
    }

    fn options(&mut self, ui: &mut egui::Ui, engine: &TailEngine, lang: Language) {
        ui.horizontal(|ui| {
            ui.label(format!("{}:", t(lang, "export_what")));
            ui.radio_value(
                &mut self.scope,
                Scope::Visible,
                t(lang, "export_scope_visible"),
            );
            let has_selection = !engine.selection.is_empty();
            ui.add_enabled_ui(has_selection, |ui| {
                ui.radio_value(
                    &mut self.scope,
                    Scope::Selected,
                    t(lang, "export_scope_selected"),
                )
            });
            let has_matches = !engine.search_matches.is_empty();
            ui.add_enabled_ui(has_matches, |ui| {
                ui.radio_value(
                    &mut self.scope,
                    Scope::Matches,
                    t(lang, "export_scope_matches"),
                )
            });
        });
        ui.horizontal(|ui| {
            ui.label(format!("{}:", t(lang, "export_format")));
            ui.radio_value(
                &mut self.format,
                Format::Text,
                t(lang, "export_format_text"),
            );
            ui.radio_value(&mut self.format, Format::Csv, "CSV");
            ui.radio_value(&mut self.format, Format::Html, "HTML");
        });
        if self.format == Format::Csv {
            ui.horizontal(|ui| {
                ui.label(format!("{}:", t(lang, "export_csv_separator")));
                ui.radio_value(&mut self.csv.sep, b',', ",");
                ui.radio_value(&mut self.csv.sep, b';', ";");
                ui.radio_value(&mut self.csv.sep, b'\t', t(lang, "fields_csv_tab"));
            });
            ui.checkbox(&mut self.csv.bom, t(lang, "export_csv_bom"));
            ui.checkbox(&mut self.csv.protect, t(lang, "export_csv_protect"))
                .on_hover_text(t(lang, "export_csv_protect_tip"));
        }
    }
}
