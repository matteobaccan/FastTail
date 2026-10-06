// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Entry picker shown when a zip or 7z archive holds more than one file entry, or for
//! any tar archive: a filter box, a list sortable by name or size, multi-select, and the
//! entries that cannot be opened listed disabled with the reason. Each chosen entry opens
//! as its own stream.
//!
//! A zip arrives with its whole list (its central directory), a 7z with its header. A
//! tar has no directory: the picker opens at once on a `TarScan` and pulls the rows the
//! scan found at every frame, showing its progress with a stop button; entries can be
//! opened while it runs.

use crate::compressed::{ArchiveEntryInfo, EntryRefusal, ScanState, SevenZRefusal, TarScan};
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
pub struct ArchivePicker {
    pub archive: PathBuf,
    pub entries: Vec<ArchiveEntryInfo>,
    /// The header walk feeding `entries` (a tar); `None` for a zip or a 7z.
    pub scan: Option<TarScan>,
    /// The complete list was too long (a 7z past `MAX_7Z_ENTRIES`): only its start is
    /// listed, and the picker says so.
    pub partial: bool,
    /// The end of the scan has been seen (and acted on) by `sync`.
    scan_seen_over: bool,
    /// Entries were opened from the picker while the scan ran: the scan ending with a
    /// single entry then opens nothing by itself.
    pub opened_any: bool,
    pub filter: String,
    /// Indices into `entries` of the checked rows.
    pub selected: BTreeSet<usize>,
    pub sort_by: SortBy,
    pub descending: bool,
}

/// What the user did with the picker this frame.
#[derive(Debug, PartialEq, Eq)]
pub enum PickerOutcome {
    /// Open these entries; `close`: the picker is done (a scan still running keeps it
    /// open, so more entries can be picked as they are found).
    Open {
        names: Vec<String>,
        close: bool,
    },
    Cancel,
}

impl ArchivePicker {
    /// A picker on a list read at once (a zip, a 7z).
    pub fn new(archive: PathBuf, entries: Vec<ArchiveEntryInfo>) -> Self {
        Self {
            archive,
            entries,
            scan: None,
            partial: false,
            scan_seen_over: false,
            opened_any: false,
            filter: String::new(),
            selected: BTreeSet::new(),
            sort_by: SortBy::Name,
            descending: false,
        }
    }

    /// A picker filled by a tar scan.
    pub fn scanning(archive: PathBuf, scan: TarScan) -> Self {
        let mut picker = Self::new(archive, Vec::new());
        picker.scan = Some(scan);
        picker
    }

    /// State of the scan (`Done` for a zip).
    pub fn scan_state(&self) -> ScanState {
        self.scan
            .as_ref()
            .map_or(ScanState::Done, |scan| scan.state())
    }

    /// Pulls the rows the scan found since the last call. When the scan has just ended
    /// with exactly one entry that can be opened, and the user neither opened nor checked
    /// anything, returns the outcome that opens it and closes the picker.
    pub fn sync(&mut self) -> Option<PickerOutcome> {
        let scan = self.scan.as_ref()?;
        // The state is read before the rows, so rows listed just before the end are
        // pulled in this same call.
        let state = scan.state();
        let rows = scan.entries_from(self.entries.len());
        self.entries.extend(rows);
        if !state.is_over() || self.scan_seen_over {
            return None;
        }
        self.scan_seen_over = true;
        let mut openable = self.entries.iter().filter(|e| e.refusal.is_none());
        match (openable.next(), openable.next()) {
            (Some(only), None)
                if state == ScanState::Done && !self.opened_any && self.selected.is_empty() =>
            {
                Some(PickerOutcome::Open {
                    names: vec![only.name.clone()],
                    close: true,
                })
            }
            _ => None,
        }
    }

    fn is_scanning(&self) -> bool {
        !self.scan_state().is_over()
    }

    /// Indices of the entries matching the filter (case-insensitive), in display order.
    pub fn visible(&self) -> Vec<usize> {
        let needle = self.filter.trim().to_lowercase();
        let mut rows: Vec<usize> = (0..self.entries.len())
            .filter(|&i| needle.is_empty() || self.entries[i].name.to_lowercase().contains(&needle))
            .collect();
        match self.sort_by {
            // Cache lowercased name keys during sort to avoid O(N log N) string allocations
            SortBy::Name => rows.sort_by_cached_key(|&i| self.entries[i].name.to_lowercase()),
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

    /// The user opens the checked entries: the picker stays while the scan runs (the
    /// selection is cleared so the next pick starts fresh), and closes otherwise.
    pub fn open_chosen(&mut self) -> PickerOutcome {
        let names = self.chosen();
        let close = !self.is_scanning();
        if !close {
            self.selected.clear();
            self.opened_any = true;
        }
        PickerOutcome::Open { names, close }
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

    /// The scan line above the list: progress and stop while it runs, then why the list
    /// may be incomplete or empty.
    fn scan_status(&mut self, ui: &mut egui::Ui, lang: Language, theme: CyberTheme) {
        if self.partial {
            ui.label(
                RichText::new(format!("⚠ {}", t(lang, "sevenz_list_partial")))
                    .color(theme.warn_color()),
            );
        }
        let Some(scan) = self.scan.as_ref() else {
            return;
        };
        let state = scan.state();
        let notice = match &state {
            ScanState::Running => {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "🗜 {} {:.0}%",
                            t(lang, "archive_scanning"),
                            scan.progress() * 100.0
                        ))
                        .monospace()
                        .color(theme.warn_color()),
                    );
                    if ui
                        .button(RichText::new("✖").monospace())
                        .on_hover_text(t(lang, "archive_scan_stop"))
                        .clicked()
                    {
                        scan.cancel();
                    }
                });
                // The rows arrive from the scan thread: look again soon.
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(100));
                None
            }
            ScanState::Done => None,
            ScanState::LimitReached => Some(t(lang, "archive_scan_partial").to_string()),
            ScanState::Cancelled => Some(t(lang, "archive_scan_stopped").to_string()),
            ScanState::Damaged(err) => Some(format!("{}: {err}", t(lang, "archive_damaged"))),
            ScanState::Failed(err) => Some(format!("{}: {err}", t(lang, "compressed_failed"))),
        };
        if let Some(notice) = notice {
            ui.label(RichText::new(format!("⚠ {notice}")).color(theme.warn_color()));
        }
        if state.is_over() && self.entries.iter().all(|e| e.refusal.is_some()) {
            ui.label(RichText::new(t(lang, "archive_no_openable")).color(theme.warn_color()));
        }
    }

    /// Draws the picker; returns what the user chose, if anything.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        lang: Language,
        theme: CyberTheme,
    ) -> Option<PickerOutcome> {
        let mut outcome = self.sync();
        if outcome.is_some() {
            return outcome;
        }
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
            self.scan_status(ui, lang, theme);
            ui.horizontal(|ui| {
                ui.label(RichText::new("🔍").monospace());
                ui.add(
                    egui::TextEdit::singleline(&mut self.filter)
                        .hint_text(t(lang, "zip_picker_filter"))
                        .desired_width(260.0),
                );
                let visible = self.visible();
                let has_openable = visible.iter().any(|&i| self.entries[i].refusal.is_none());
                if ui
                    .add_enabled(has_openable, egui::Button::new(t(lang, "zip_picker_all")))
                    .on_disabled_hover_text(t(lang, "archive_no_openable"))
                    .clicked()
                {
                    for i in visible {
                        if self.entries[i].refusal.is_none() {
                            self.selected.insert(i);
                        }
                    }
                }
                if ui
                    .add_enabled(
                        !self.selected.is_empty(),
                        egui::Button::new(t(lang, "zip_picker_none")),
                    )
                    .on_disabled_hover_text(t(lang, "zip_picker_no_selection"))
                    .clicked()
                {
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
                                let mut row = ui
                                    .checkbox(&mut checked, RichText::new(&entry.name).monospace());
                                if let Some(block) = entry.block_size {
                                    // The entries before it in its solid block are decoded
                                    // too: say how much.
                                    row = row.on_hover_text(
                                        t(lang, "sevenz_block_size")
                                            .replace("{size}", &human_size(block)),
                                    );
                                }
                                if row.changed() {
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
                    outcome = Some(self.open_chosen());
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
        EntryRefusal::LinkOrSpecial => t(lang, "tar_entry_link").to_string(),
        EntryRefusal::Sparse => t(lang, "tar_entry_sparse").to_string(),
        EntryRefusal::DictionaryTooLarge => t(lang, "sevenz_entry_dictionary").to_string(),
    }
}

/// An error opening or listing an archive, in the UI language when FastTail itself
/// refused it (a 7z whose header is encrypted or too large, or declares too many entries).
pub fn io_error_text(lang: Language, err: &std::io::Error) -> String {
    match crate::compressed::sevenz_refusal(err) {
        Some(SevenZRefusal::EncryptedHeader) => t(lang, "sevenz_header_encrypted").to_string(),
        Some(SevenZRefusal::HeaderTooLarge) => t(lang, "sevenz_header_too_large").to_string(),
        Some(SevenZRefusal::TooManyEntries) => t(lang, "sevenz_too_many_entries").to_string(),
        None => err.to_string(),
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

    fn entry(name: &str, size: u64, refusal: Option<EntryRefusal>) -> ArchiveEntryInfo {
        ArchiveEntryInfo {
            name: name.to_string(),
            size,
            compressed_size: size / 2,
            block_size: None,
            offset: None,
            refusal,
        }
    }

    #[test]
    fn filter_sort_and_selection() {
        let mut picker = ArchivePicker::new(
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
        // A zip has no scan: opening closes the picker.
        assert_eq!(
            picker.open_chosen(),
            PickerOutcome::Open {
                names: vec!["worker.log".to_string()],
                close: true
            }
        );
        assert_eq!(picker.sync(), None);
    }

    #[test]
    fn sizes_read_like_a_file_manager() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(1536), "1.5 KB");
        assert_eq!(human_size(3 * 1024 * 1024 * 1024), "3.0 GB");
    }
}
