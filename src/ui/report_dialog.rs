// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The bookmark report dialog: options, then the Markdown report built in bounded steps
//! (a few milliseconds per frame, with progress and Cancel) and copied to the clipboard
//! or saved as a `.md` file. The report itself is `crate::bookmark_report`.

use crate::bookmark_report::{
    normalize_tag, report_file_name, ReportJob, ReportOptions, ReportOrder, MAX_CLIPBOARD_BYTES,
    MAX_REPORT_CONTEXT,
};
use crate::config::FastTailConfig;
use crate::find_all::stream_name;
use crate::i18n::{t, Language};
use crate::tail_engine::TailEngine;
use crate::theme::CyberTheme;
use egui::RichText;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

/// Time spent reading context lines per frame.
const STEP_BUDGET: Duration = Duration::from_millis(6);

/// Which streams the report covers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReportScope {
    /// One stream (its `TailEngine::path`).
    Stream(PathBuf),
    /// Every open stream with bookmarks, in dock order.
    All,
}

/// Where the finished report goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Output {
    Copy,
    Save,
}

pub struct ReportDialog {
    pub scope: ReportScope,
    context: u8,
    include_auto: bool,
    order_time: bool,
    /// Tag filter as typed: `#deploy #oom`.
    tags_text: String,
    /// The report being built, the paths of its streams (by report index) and where it
    /// goes once built.
    job: Option<(ReportJob, Vec<PathBuf>, Output, CountsKey)>,
    /// The last report built, kept so a report too large for the clipboard can be saved
    /// without building it again (with the options it was built with).
    built: Option<(CountsKey, String)>,
    /// Last outcome: text and whether it is a warning.
    status: Option<(String, bool)>,
    /// What the dialog shows while it is open, recomputed only when the options or a
    /// stream's bookmarks change: `(key, bookmarks, streams, skipped, tag counts)`.
    counts: Option<(CountsKey, ReportCounts)>,
}

/// Bookmarks, streams with bookmarks, streams without, and tag counts.
type ReportCounts = (usize, usize, usize, BTreeMap<String, usize>);

/// The options and, per stream in scope, its path, bookmark generation and line count.
type CountsKey = (ReportOptions, Vec<(PathBuf, u64, usize)>);

/// Local time now as `YYYY-MM-DD HH:MM:SS`.
fn local_now() -> String {
    let utc = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    crate::timestamp::format_millis(utc + crate::timestamp::local_offset_millis(utc))
}

impl ReportDialog {
    /// A dialog with the choices last used (from `fasttail.ini`).
    pub fn new(scope: ReportScope, config: &FastTailConfig) -> Self {
        Self {
            scope,
            context: config.report_context.min(MAX_REPORT_CONTEXT as u8),
            include_auto: config.report_auto,
            order_time: config.report_order_time,
            tags_text: String::new(),
            job: None,
            built: None,
            status: None,
            counts: None,
        }
    }

    /// The tags of the filter field, normalized, and whether every word was a tag.
    fn tags(&self) -> (Vec<String>, bool) {
        let mut tags = Vec::new();
        let mut valid = true;
        for word in self.tags_text.split([' ', ',']).filter(|w| !w.is_empty()) {
            match normalize_tag(word) {
                Some(tag) if !tags.contains(&tag) => tags.push(tag),
                Some(_) => {}
                None => valid = false,
            }
        }
        (tags, valid)
    }

    fn options(&self) -> ReportOptions {
        ReportOptions {
            context: self.context as usize,
            include_auto: self.include_auto,
            tags: self.tags().0,
            order: if self.order_time {
                ReportOrder::Time
            } else {
                ReportOrder::Stream
            },
        }
    }

    /// What the report depends on: the options and each stream's bookmarks and length.
    fn counts_key(&self, engines: &[TailEngine]) -> CountsKey {
        (
            self.options(),
            self.in_scope(engines)
                .iter()
                .map(|e| (e.path.clone(), e.bookmarks_generation, e.total_lines()))
                .collect(),
        )
    }

    /// Bookmarks, streams with and without bookmarks, and tag counts of the streams in
    /// scope, recomputed only when `counts_key` changes.
    fn counts(&mut self, engines: &[TailEngine]) -> ReportCounts {
        let key = self.counts_key(engines);
        if self.counts.as_ref().is_none_or(|(k, _)| *k != key) {
            let options = &key.0;
            let (mut bookmarks, mut streams, mut skipped) = (0, 0, 0);
            let mut tags: BTreeMap<String, usize> = BTreeMap::new();
            for engine in self.in_scope(engines) {
                let n = engine.bookmark_report_count(options);
                if n > 0 {
                    streams += 1;
                    bookmarks += n;
                } else {
                    skipped += 1;
                }
                for (tag, count) in engine.bookmark_tag_counts() {
                    *tags.entry(tag).or_default() += count;
                }
            }
            self.counts = Some((key, (bookmarks, streams, skipped, tags)));
        }
        self.counts.as_ref().expect("counts").1.clone()
    }

    /// The engines the report covers, in dock order (`engines` is in that order).
    fn in_scope<'a>(&self, engines: &'a [TailEngine]) -> Vec<&'a TailEngine> {
        engines
            .iter()
            .filter(|e| match &self.scope {
                ReportScope::Stream(path) => &e.path == path,
                ReportScope::All => true,
            })
            .collect()
    }

    /// Starts building the report for `output`.
    fn start(&mut self, engines: &[TailEngine], output: Output) {
        let options = self.options();
        let mut streams = Vec::new();
        let mut paths = Vec::new();
        for engine in self.in_scope(engines) {
            let stream = engine.bookmark_report_stream(stream_name(engine), &options);
            if !stream.bookmarks.is_empty() {
                streams.push(stream);
                paths.push(engine.path.clone());
            }
        }
        self.status = None;
        self.built = None;
        let key = self.counts_key(engines);
        self.job = Some((ReportJob::new(streams, options), paths, output, key));
    }

    /// Hands a finished report to its output.
    fn deliver(
        &mut self,
        ctx: &egui::Context,
        lang: Language,
        markdown: String,
        output: Output,
        key: CountsKey,
    ) {
        match output {
            Output::Copy if markdown.len() > MAX_CLIPBOARD_BYTES => {
                self.status = Some((t(lang, "report_too_large").to_string(), true));
            }
            Output::Copy => {
                ctx.copy_text(markdown);
                self.status = Some((t(lang, "report_copied").to_string(), false));
                self.built = None;
                return;
            }
            Output::Save => {
                let name = report_file_name(&local_now()[..10]);
                let target = rfd::FileDialog::new()
                    .set_title(t(lang, "bookmark_report_title"))
                    .set_file_name(name)
                    .add_filter("Markdown (*.md)", &["md"])
                    .save_file();
                if let Some(path) = target {
                    self.status = Some(match std::fs::write(&path, markdown.as_bytes()) {
                        Ok(()) => (
                            t(lang, "report_saved").replace("{path}", &path.display().to_string()),
                            false,
                        ),
                        Err(e) => (format!("{}: {e}", t(lang, "report_save_failed")), true),
                    });
                }
                self.built = None;
                return;
            }
        }
        // Only a report too large for the clipboard is kept, for Save, and only while
        // nothing it was built from changes (see `counts_key`).
        self.built = Some((key, markdown));
    }

    /// Draws the dialog and advances the report being built. Returns `false` once the
    /// dialog is closed. The choices are written back to `config` as they change.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        engines: &[TailEngine],
        config: &mut FastTailConfig,
    ) -> bool {
        let lang = config.language;
        let theme: CyberTheme = config.theme;

        // Advance the report being built; a stream closed meanwhile ends it.
        if let Some((job, paths, output, _)) = self.job.as_mut() {
            let index: Vec<Option<&TailEngine>> = paths
                .iter()
                .map(|p| engines.iter().find(|e| &e.path == p))
                .collect();
            if index.iter().any(Option::is_none) {
                job.cancel();
                self.status = Some((t(lang, "report_stream_closed").to_string(), true));
            }
            let done = job.step(STEP_BUDGET, |s, line| {
                index[s].and_then(|e| e.get_line(line).map(|l| l.into_owned()))
            });
            let output = *output;
            if job.is_cancelled() {
                self.job = None;
            } else if done {
                let (job, _, _, key) = self.job.take().expect("a job");
                let markdown = job.markdown(&local_now()[..16], env!("CARGO_PKG_VERSION"));
                self.deliver(ctx, lang, markdown, output, key);
            } else {
                ctx.request_repaint();
            }
        }

        let (bookmarks, streams, skipped, tag_counts) = self.counts(engines);
        let tags_valid = self.tags().1;
        let mut open = true;
        let mut close = false;
        let title = match &self.scope {
            ReportScope::Stream(_) => t(lang, "bookmark_report_title").to_string(),
            ReportScope::All => format!(
                "{} — {}",
                t(lang, "bookmark_report_title"),
                t(lang, "report_all_streams")
            ),
        };
        egui::Window::new(
            RichText::new(format!("📝 {title}"))
                .monospace()
                .color(theme.accent_color()),
        )
        .id(egui::Id::new("fasttail_bookmark_report"))
        .open(&mut open)
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.set_min_width(380.0);
            let busy = self.job.is_some();
            ui.add_enabled_ui(!busy, |ui| {
                egui::Grid::new("report_options")
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        ui.label(t(lang, "report_context"));
                        ui.add(egui::Slider::new(
                            &mut self.context,
                            0..=MAX_REPORT_CONTEXT as u8,
                        ));
                        ui.end_row();

                        ui.label(t(lang, "report_order"));
                        ui.horizontal(|ui| {
                            ui.radio_value(
                                &mut self.order_time,
                                false,
                                t(lang, "report_order_stream"),
                            );
                            ui.radio_value(
                                &mut self.order_time,
                                true,
                                t(lang, "report_order_time"),
                            );
                        });
                        ui.end_row();

                        ui.label(t(lang, "report_tags"));
                        let (_, valid) = self.tags();
                        let mut field = egui::TextEdit::singleline(&mut self.tags_text)
                            .hint_text("#deploy #oom")
                            .desired_width(200.0);
                        if !valid {
                            field = field.text_color(theme.warn_color().into());
                        }
                        ui.add(field).on_hover_text(t(lang, "tip_report_tags"));
                        ui.end_row();
                    });
                ui.checkbox(&mut self.include_auto, t(lang, "report_include_auto"));

                // The tags of the streams in scope, a click adds one to the filter.
                if !tag_counts.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        for (tag, n) in tag_counts.iter().take(24) {
                            if ui
                                .small_button(RichText::new(format!("#{tag} ({n})")).monospace())
                                .clicked()
                            {
                                if !self.tags_text.trim().is_empty() {
                                    self.tags_text.push(' ');
                                }
                                self.tags_text.push_str(&format!("#{tag}"));
                            }
                        }
                    });
                }
            });

            // What the report will hold.
            ui.add_space(4.0);
            let mut summary = t(lang, "report_summary")
                .replace("{bookmarks}", &bookmarks.to_string())
                .replace("{streams}", &streams.to_string());
            if self.scope == ReportScope::All && skipped > 0 {
                summary.push_str(" · ");
                summary.push_str(&t(lang, "report_skipped").replace("{n}", &skipped.to_string()));
            }
            ui.label(
                RichText::new(summary)
                    .monospace()
                    .size(11.0)
                    .color(theme.text_dim()),
            );

            ui.add_space(6.0);
            if let Some((job, _, _, _)) = self.job.as_mut() {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::ProgressBar::new(job.progress())
                            .desired_width(240.0)
                            .show_percentage(),
                    );
                    if ui.button(t(lang, "report_cancel")).clicked() {
                        job.cancel();
                    }
                });
            } else {
                // A report too large for the clipboard is kept for Save while nothing it
                // was built from has changed.
                let key = self.counts_key(engines);
                let ready = self
                    .built
                    .as_ref()
                    .filter(|(built_from, _)| *built_from == key)
                    .map(|(_, md)| md.clone());
                let too_large = ready
                    .as_ref()
                    .is_some_and(|md| md.len() > MAX_CLIPBOARD_BYTES);
                // An invalid word in the tag filter would silently mean "all bookmarks".
                let can_build = bookmarks > 0 && tags_valid;
                ui.horizontal(|ui| {
                    let copy_disabled_tip = if too_large {
                        t(lang, "report_too_large")
                    } else if bookmarks == 0 {
                        t(lang, "report_no_bookmarks")
                    } else {
                        t(lang, "tip_report_tags")
                    };
                    let copy = ui
                        .add_enabled(
                            can_build && !too_large,
                            egui::Button::new(format!("📋 {}", t(lang, "report_copy"))),
                        )
                        .on_disabled_hover_text(copy_disabled_tip);
                    if copy.clicked() {
                        self.start(engines, Output::Copy);
                    }
                    let save_disabled_tip = if bookmarks == 0 {
                        t(lang, "report_no_bookmarks")
                    } else {
                        t(lang, "tip_report_tags")
                    };
                    if ui
                        .add_enabled(
                            can_build,
                            egui::Button::new(format!("💾 {}", t(lang, "report_save"))),
                        )
                        .on_disabled_hover_text(save_disabled_tip)
                        .clicked()
                    {
                        match ready {
                            Some(md) => self.deliver(ui.ctx(), lang, md, Output::Save, key.clone()),
                            None => self.start(engines, Output::Save),
                        }
                    }
                    if ui.button(t(lang, "session_cancel")).clicked()
                        || ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
                    {
                        close = true;
                    }
                });
            }
            if let Some((text, warn)) = &self.status {
                ui.label(RichText::new(text).monospace().size(11.0).color(if *warn {
                    theme.warn_color()
                } else {
                    theme.accent_color()
                }));
            }
        });

        // The choices are remembered for the next report.
        config.report_context = self.context;
        config.report_auto = self.include_auto;
        config.report_order_time = self.order_time;
        open && !close
    }
}
