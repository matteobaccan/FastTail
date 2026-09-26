//! Entry picker shown when a zip archive holds more than one file entry: a filter box,
//! a list sortable by name or size, multi-select, and the entries that cannot be opened
//! listed disabled with the reason. Each chosen entry opens as its own stream.

use crate::compressed::{EntryRefusal, ZipEntryInfo};
use crate::i18n::{t, Language};
use crate::theme::CyberTheme;
use egui::RichText;
use std::collections::BTreeSet;
use std::path::PathBuf;

/// Column the list is sorted by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortBy {
    Name,
    Size,
}

/// State of the picker for one archive.
pub struct ZipPicker {
    pub archive: PathBuf,
    pub entries: Vec<ZipEntryInfo>,
    pub filter: String,
    /// Indices into `entries` of the checked rows.
    pub selected: BTreeSet<usize>,
    pub sort_by: SortBy,
    pub descending: bool,
}

/// What the user did with the picker this frame.
pub enum PickerOutcome {
    Open(Vec<String>),
    Cancel,
}

impl ZipPicker {
    pub fn new(archive: PathBuf, entries: Vec<ZipEntryInfo>) -> Self {
        Self {
            archive,
            entries,
            filter: String::new(),
            selected: BTreeSet::new(),
            sort_by: SortBy::Name,
            descending: false,
        }
    }

    /// Indices of the entries matching the filter (case-insensitive), in display order.
    pub fn visible(&self) -> Vec<usize> {
        let needle = self.filter.trim().to_lowercase();
        let mut rows: Vec<usize> = (0..self.entries.len())
            .filter(|&i| needle.is_empty() || self.entries[i].name.to_lowercase().contains(&needle))
            .collect();
        match self.sort_by {
            SortBy::Name => rows.sort_by(|&a, &b| {
                self.entries[a]
                    .name
                    .to_lowercase()
                    .cmp(&self.entries[b].name.to_lowercase())
            }),
            SortBy::Size => rows.sort_by_key(|&i| self.entries[i].size),
        }
        if self.descending {
            rows.reverse();
        }
        rows
    }

    /// Names of the checked entries that can be opened, in archive order.
    pub fn chosen(&self) -> Vec<String> {
        self.selected
            .iter()
            .filter(|&&i| self.entries[i].refusal.is_none())
            .map(|&i| self.entries[i].name.clone())
            .collect()
    }

    fn sort_header(&mut self, ui: &mut egui::Ui, label: &str, column: SortBy) {
        let arrow = match (self.sort_by == column, self.descending) {
            (true, false) => " ▲",
            (true, true) => " ▼",
            _ => "",
        };
        if ui
            .button(
                RichText::new(format!("{label}{arrow}"))
                    .monospace()
                    .strong(),
            )
            .clicked()
        {
            if self.sort_by == column {
                self.descending = !self.descending;
            } else {
                self.sort_by = column;
                self.descending = false;
            }
        }
    }

    /// Draws the picker; returns what the user chose, if anything.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        lang: Language,
        theme: CyberTheme,
    ) -> Option<PickerOutcome> {
        let mut outcome = None;
        let archive_name = self
            .archive
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let mut open = true;
        egui::Window::new(
            RichText::new(format!(
                "🗜 {} — {archive_name}",
                t(lang, "zip_picker_title")
            ))
            .strong(),
        )
        .id(egui::Id::new("fasttail_zip_picker"))
        .collapsible(false)
        .resizable(true)
        .default_width(520.0)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("🔍").monospace());
                ui.add(
                    egui::TextEdit::singleline(&mut self.filter)
                        .hint_text(t(lang, "zip_picker_filter"))
                        .desired_width(260.0),
                );
                let visible = self.visible();
                if ui.button(t(lang, "zip_picker_all")).clicked() {
                    for i in visible {
                        if self.entries[i].refusal.is_none() {
                            self.selected.insert(i);
                        }
                    }
                }
                if ui.button(t(lang, "zip_picker_none")).clicked() {
                    self.selected.clear();
                }
            });
            ui.separator();
            ui.horizontal(|ui| {
                self.sort_header(ui, t(lang, "zip_picker_name"), SortBy::Name);
                self.sort_header(ui, t(lang, "zip_picker_size"), SortBy::Size);
            });
            egui::ScrollArea::vertical()
                .max_height(320.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for i in self.visible() {
                        let entry = &self.entries[i];
                        let size = human_size(entry.size);
                        ui.horizontal(|ui| match &entry.refusal {
                            None => {
                                let mut checked = self.selected.contains(&i);
                                if ui
                                    .checkbox(&mut checked, RichText::new(&entry.name).monospace())
                                    .changed()
                                {
                                    if checked {
                                        self.selected.insert(i);
                                    } else {
                                        self.selected.remove(&i);
                                    }
                                }
                                ui.label(RichText::new(size).monospace().color(theme.text_dim()));
                            }
                            Some(refusal) => {
                                let reason = refusal_text(lang, refusal);
                                ui.add_enabled(
                                    false,
                                    egui::Checkbox::new(
                                        &mut false,
                                        RichText::new(&entry.name).monospace(),
                                    ),
                                )
                                .on_disabled_hover_text(&reason);
                                ui.label(
                                    RichText::new(format!("{size} · {reason}"))
                                        .monospace()
                                        .color(theme.warn_color()),
                                );
                            }
                        });
                    }
                });
            ui.separator();
            ui.horizontal(|ui| {
                let chosen = self.chosen();
                let label = format!("📂 {} ({})", t(lang, "zip_picker_open"), chosen.len());
                if ui
                    .add_enabled(!chosen.is_empty(), egui::Button::new(label))
                    .on_disabled_hover_text(t(lang, "zip_picker_no_selection"))
                    .clicked()
                {
                    outcome = Some(PickerOutcome::Open(chosen));
                }
                if ui.button(t(lang, "session_cancel")).clicked() {
                    outcome = Some(PickerOutcome::Cancel);
                }
            });
        });
        if !open {
            outcome = Some(PickerOutcome::Cancel);
        }
        outcome
    }
}

/// Why an entry cannot be opened, in the UI language.
pub fn refusal_text(lang: Language, refusal: &EntryRefusal) -> String {
    match refusal {
        EntryRefusal::Encrypted => t(lang, "zip_entry_encrypted").to_string(),
        EntryRefusal::Method(method) => t(lang, "zip_entry_method").replace("{method}", method),
        EntryRefusal::UnsafeName => t(lang, "zip_entry_unsafe").to_string(),
        EntryRefusal::DuplicateName => t(lang, "zip_entry_duplicate").to_string(),
    }
}

/// `1.5 MB`-style size.
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, size: u64, refusal: Option<EntryRefusal>) -> ZipEntryInfo {
        ZipEntryInfo {
            name: name.to_string(),
            size,
            compressed_size: size / 2,
            refusal,
        }
    }

    #[test]
    fn filter_sort_and_selection() {
        let mut picker = ZipPicker::new(
            PathBuf::from("bundle.zip"),
            vec![
                entry("worker.log", 300, None),
                entry("server.log", 100, None),
                entry("secret.log", 50, Some(EntryRefusal::Encrypted)),
            ],
        );
        assert_eq!(picker.visible(), [2, 1, 0]);
        picker.sort_by = SortBy::Size;
        picker.descending = true;
        assert_eq!(picker.visible(), [0, 1, 2]);
        picker.filter = "SER".to_string();
        assert_eq!(picker.visible(), [1]);
        // A refused entry is never opened, even if it ended up selected.
        picker.selected.extend([0, 2]);
        assert_eq!(picker.chosen(), ["worker.log"]);
    }

    #[test]
    fn sizes_read_like_a_file_manager() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(1536), "1.5 KB");
        assert_eq!(human_size(3 * 1024 * 1024 * 1024), "3.0 GB");
    }
}
