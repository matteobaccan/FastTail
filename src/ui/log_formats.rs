// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The "Log formats" dialog: the built-in formats (locked), the user's format files and
//! the files that did not load with their error; new, duplicate, delete, import and
//! export; an editor of the format as a draft with a live preview of what it reads from
//! its samples and from the focused stream's first lines. Nothing changes for the streams
//! until the draft is saved.

use crate::i18n::{t, Language};
use crate::log_format::Library;
use crate::log_level::LogLevel;
use crate::theme::CyberTheme;
use eframe::egui;
use egui::RichText;
use std::time::Instant;

use crate::log_format_edit::PREVIEW_DELAY;
pub use crate::log_format_edit::{
    preview, Draft, Editing, FormatsAction, FormatsDialog, Preview, PreviewRow, PREVIEW_LINES,
};

/// The text of an error of `save` / `import`: a name or file that is taken, or why the
/// format does not load, after a translated prefix.
fn error_text(lang: Language, error: &str, name: &str) -> String {
    if error.ends_with(": exists") {
        t(lang, "fmt_exists").replace("{0}", name)
    } else {
        format!("{}: {error}", t(lang, "fmt_invalid"))
    }
}

impl FormatsDialog {
    /// Draws the dialog; `Some` when the formats changed or it was closed.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        lib: &Library,
        lang: Language,
        theme: CyberTheme,
    ) -> Option<FormatsAction> {
        let mut is_open = true;
        let mut action = None;
        if self
            .edited_at
            .is_some_and(|at| at.elapsed() >= PREVIEW_DELAY)
        {
            self.refresh_preview();
        } else if let Some(at) = self.edited_at {
            ctx.request_repaint_after(PREVIEW_DELAY.saturating_sub(at.elapsed()));
        }
        egui::Window::new(
            RichText::new(format!("◇ {}", t(lang, "fmt_title")))
                .monospace()
                .color(theme.accent_color()),
        )
        .id(egui::Id::new("fasttail_log_formats"))
        .open(&mut is_open)
        .default_size([920.0, 620.0])
        .resizable(true)
        .collapsible(false)
        .show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(220.0);
                    if let Some(a) = self.list(ui, lib, lang, theme) {
                        action = Some(a);
                    }
                });
                ui.separator();
                ui.vertical(|ui| {
                    if let Some(a) = self.editor(ui, lib, lang, theme) {
                        action = Some(a);
                    }
                });
            });
        });
        if !is_open {
            action = Some(FormatsAction::Close);
        }
        action
    }

    /// The formats, the files that did not load, and the list actions.
    fn list(
        &mut self,
        ui: &mut egui::Ui,
        lib: &Library,
        lang: Language,
        theme: CyberTheme,
    ) -> Option<FormatsAction> {
        let mut action = None;
        egui::ScrollArea::vertical()
            .id_salt("fmt_list")
            .max_height(360.0)
            .show(ui, |ui| {
                for format in &lib.formats {
                    let current = match (&self.editing, &format.file) {
                        (Editing::File(p), Some(f)) => p == f,
                        (Editing::Builtin, None) => self.draft.name == format.name(),
                        _ => false,
                    };
                    let label = if format.plugin.is_some() {
                        format!("🔌 {}", format.name())
                    } else if format.builtin {
                        format!("🔒 {}", format.name())
                    } else {
                        format!("◇ {}", format.name())
                    };
                    let mut resp = ui.selectable_label(current, RichText::new(label).monospace());
                    resp = match (&format.file, format.builtin) {
                        (_, true) if format.plugin.is_some() => resp.on_hover_text(
                            t(lang, "fmt_plugin_tip")
                                .replace("{0}", format.plugin.as_deref().unwrap_or("")),
                        ),
                        (_, true) => resp.on_hover_text(t(lang, "fmt_builtin_tip")),
                        (Some(f), _) => resp.on_hover_text(f.display().to_string()),
                        _ => resp,
                    };
                    if resp.clicked() {
                        self.select(format);
                    }
                }
                for (path, error) in &lib.errors {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    ui.label(
                        RichText::new(format!("⚠ {name}"))
                            .monospace()
                            .color(theme.error_color()),
                    )
                    .on_hover_text(format!("{}: {error}", t(lang, "fmt_invalid")));
                }
                if !lib.formats.iter().any(|f| !f.builtin) {
                    ui.label(
                        RichText::new(
                            t(lang, "fmt_none").replace("{0}", &self.dir.display().to_string()),
                        )
                        .small()
                        .color(theme.text_dim()),
                    );
                }
            });
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            if ui.button(t(lang, "fmt_new")).clicked() {
                self.new_format();
            }
            if ui.button(t(lang, "fmt_duplicate")).clicked() {
                self.duplicate();
            }
            let file = match &self.editing {
                Editing::File(p) => Some(p.clone()),
                _ => None,
            };
            if ui
                .add_enabled(file.is_some(), egui::Button::new(t(lang, "fmt_delete")))
                .on_disabled_hover_text(t(lang, "fmt_builtin_tip"))
                .clicked()
            {
                self.confirm_delete = file;
            }
            if ui.button(t(lang, "fmt_import")).clicked() {
                if let Some(from) = rfd::FileDialog::new()
                    .add_filter("FastTail format", &["ini"])
                    .pick_file()
                {
                    match self.import(lib, &from) {
                        Ok(path) => {
                            self.notice = Some((
                                t(lang, "fmt_saved").replace("{0}", &path.display().to_string()),
                                false,
                            ));
                            action = Some(FormatsAction::Changed);
                        }
                        Err(e) => {
                            let name = e.strip_suffix(": exists").unwrap_or("").to_string();
                            self.notice = Some((error_text(lang, &e, &name), true));
                        }
                    }
                }
            }
            if ui.button(t(lang, "fmt_export")).clicked() {
                let name = self.draft.to_spec().file_name();
                if let Some(to) = rfd::FileDialog::new().set_file_name(&name).save_file() {
                    if let Err(e) = self.export(&to) {
                        self.notice = Some((e.to_string(), true));
                    }
                }
            }
        });
        if let Some(path) = self.confirm_delete.clone() {
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    t(lang, "fmt_delete_confirm").replace("{0}", &path.display().to_string()),
                )
                .color(theme.warn_color()),
            );
            ui.horizontal(|ui| {
                if ui.button(t(lang, "fmt_delete")).clicked() {
                    match self.delete(&path) {
                        Ok(()) => action = Some(FormatsAction::Changed),
                        Err(e) => self.notice = Some((e.to_string(), true)),
                    }
                }
                if ui.button(t(lang, "open_pattern_cancel")).clicked() {
                    self.confirm_delete = None;
                }
            });
        }
        action
    }

    /// The draft's fields, Save, and the preview.
    fn editor(
        &mut self,
        ui: &mut egui::Ui,
        lib: &Library,
        lang: Language,
        theme: CyberTheme,
    ) -> Option<FormatsAction> {
        let mut action = None;
        let editable = self.editable();
        let mut changed = false;
        egui::Grid::new("fmt_fields")
            .num_columns(2)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                let mut row =
                    |ui: &mut egui::Ui, key: &str, text: &mut String, multi: bool, hint: &str| {
                        ui.label(RichText::new(t(lang, key)).monospace());
                        let edit = if multi {
                            egui::TextEdit::multiline(text).desired_rows(2)
                        } else {
                            egui::TextEdit::singleline(text)
                        };
                        let resp = ui.add_enabled(
                            editable,
                            edit.font(egui::TextStyle::Monospace)
                                .hint_text(hint)
                                .desired_width(560.0),
                        );
                        changed |= resp.changed();
                        ui.end_row();
                    };
                let d = &mut self.draft;
                row(ui, "fmt_name", &mut d.name, false, "");
                row(
                    ui,
                    "fmt_patterns",
                    &mut d.patterns,
                    true,
                    r"^(?P<ts>\S+) (?P<msg>.*)$",
                );
                row(ui, "fmt_ts_field", &mut d.timestamp_field, false, "ts");
                row(
                    ui,
                    "fmt_ts_format",
                    &mut d.timestamp_format,
                    false,
                    "%d.%m.%y %H:%M:%S,%3f",
                );
                row(ui, "fmt_ts_zone", &mut d.timestamp_zone, false, "local");
                row(ui, "fmt_level_field", &mut d.level_field, false, "level");
                row(
                    ui,
                    "fmt_level_map",
                    &mut d.level_map,
                    false,
                    "W=warn,E=error",
                );
                row(ui, "fmt_msg_field", &mut d.message_field, false, "msg");
                row(ui, "fmt_units", &mut d.units, false, "took=ms,size=KiB");
                row(
                    ui,
                    "fmt_entry_start",
                    &mut d.entry_start,
                    false,
                    r"^\d{4}-\d\d-\d\d ",
                );
                row(ui, "fmt_files", &mut d.files, false, "payments-*.log");
                row(ui, "fmt_columns", &mut d.columns, false, "ts,level");
                row(ui, "fmt_samples", &mut d.samples, true, "");
            });
        if changed {
            self.edited_at = Some(Instant::now());
            ui.ctx().request_repaint_after(PREVIEW_DELAY);
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let valid = self.preview.error.is_none();
            if ui
                .add_enabled(editable && valid, egui::Button::new(t(lang, "fmt_save")))
                .on_disabled_hover_text(if editable {
                    t(lang, "fmt_invalid")
                } else {
                    t(lang, "fmt_builtin_tip")
                })
                .clicked()
            {
                match self.save(lib) {
                    Ok(path) => {
                        self.notice = Some((
                            t(lang, "fmt_saved").replace("{0}", &path.display().to_string()),
                            false,
                        ));
                        action = Some(FormatsAction::Changed);
                    }
                    Err(e) => {
                        let name = self.draft.name.trim().to_string();
                        self.notice = Some((error_text(lang, &e, &name), true));
                    }
                }
            }
            if let Some((text, error)) = &self.notice {
                let color = if *error {
                    theme.error_color()
                } else {
                    theme.text_dim()
                };
                ui.label(RichText::new(text).small().color(color));
            }
        });
        ui.separator();
        self.preview_ui(ui, lang, theme);
        action
    }

    /// The compile error, or the match ratio, the cost and the table of lines read.
    fn preview_ui(&self, ui: &mut egui::Ui, lang: Language, theme: CyberTheme) {
        let p = &self.preview;
        if let Some(error) = &p.error {
            ui.label(
                RichText::new(format!("{}: {error}", t(lang, "fmt_invalid")))
                    .monospace()
                    .color(theme.error_color()),
            );
            return;
        }
        let summary = t(lang, "fmt_match_ratio")
            .replace("{0}", &p.matched.to_string())
            .replace("{1}", &p.entries.to_string())
            .replace("{2}", &p.ratio().to_string());
        let cost = t(lang, "fmt_cost").replace("{0}", &format!("{:.1}", p.micros_per_line));
        ui.label(
            RichText::new(format!("{summary} · {cost}"))
                .monospace()
                .small(),
        );
        if p.few_entries() {
            ui.label(
                RichText::new(t(lang, "fmt_entries_warn"))
                    .small()
                    .color(theme.warn_color()),
            );
        }
        egui::ScrollArea::both()
            .id_salt("fmt_preview")
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for row in &p.rows {
                    let color = if !row.starts_entry || row.matched {
                        theme.text_primary()
                    } else {
                        theme.text_dim()
                    };
                    let time = row
                        .time
                        .map(crate::timestamp::format_millis)
                        .unwrap_or_else(|| "-".to_string());
                    let level = if row.level == LogLevel::Unknown {
                        "-".to_string()
                    } else {
                        format!("{:?}", row.level).to_uppercase()
                    };
                    let mark = match (row.sample, row.starts_entry, row.matched) {
                        (_, false, _) => "  ",
                        (_, true, true) => "✔ ",
                        (_, true, false) => "✘ ",
                    };
                    let head = format!("{mark}{time:<19} {level:<5} ");
                    let resp = ui.label(
                        RichText::new(format!("{head}{}", row.text))
                            .monospace()
                            .size(11.0)
                            .color(if row.sample {
                                theme.accent_color()
                            } else {
                                color
                            }),
                    );
                    if !row.fields.is_empty() {
                        let fields: Vec<String> = row
                            .fields
                            .iter()
                            .map(|(k, v)| format!("{k} = {v}"))
                            .collect();
                        resp.on_hover_text(fields.join("\n"));
                    }
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_and_save_reject_non_regular_files() {
        let dir = tempfile::tempdir().unwrap();
        let sub_dir = dir.path().join("sub");
        std::fs::create_dir(&sub_dir).unwrap();

        let mut dialog = FormatsDialog::new(dir.path().to_path_buf(), Vec::new());
        dialog.draft.name = "TestFormat".to_string();
        dialog.draft.patterns = r"^(?P<msg>.*)$".to_string();

        // Exporting to a directory target must fail
        let res = dialog.export(&sub_dir);
        assert!(res.is_err(), "exporting to a directory must fail");

        // Saving when the target path is a directory must fail
        dialog.editing = Editing::File(sub_dir.clone());
        let save_res = dialog.save(&Library::default());
        assert!(save_res.is_err(), "saving to a directory must fail");
    }
}
