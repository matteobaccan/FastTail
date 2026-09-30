// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The archive entry picker: the entries of a zip, 7z or tar with more than one entry,
//! filtered by what is typed, one chosen to open. A tar is listed by a background scan
//! while the picker is open.

use std::path::PathBuf;

use crate::compressed::{ArchiveEntryInfo, EntryRefusal, ScanState, TarScan};
use crate::tui::form::TextField;

pub struct EntryPicker {
    pub archive: PathBuf,
    pub entries: Vec<ArchiveEntryInfo>,
    /// The tar scan still listing, if any.
    pub scan: Option<TarScan>,
    /// The list stopped early (a zip list cut short, a tar scan limit or damage).
    pub partial: Option<String>,
    pub filter: TextField,
    /// Position in `shown()`.
    pub selected: usize,
    /// First position of `shown()` on screen.
    pub top: usize,
}

impl EntryPicker {
    pub fn new(archive: PathBuf, entries: Vec<ArchiveEntryInfo>, partial: bool) -> Self {
        Self {
            archive,
            entries,
            scan: None,
            partial: partial.then(|| "The list is partial".to_string()),
            filter: TextField::default(),
            selected: 0,
            top: 0,
        }
    }

    pub fn scanning(archive: PathBuf, scan: TarScan) -> Self {
        let mut p = Self::new(archive, Vec::new(), false);
        p.scan = Some(scan);
        p
    }

    /// Takes the entries the tar scan found since the last call; returns whether any
    /// came or the scan ended.
    pub fn pull(&mut self) -> bool {
        let Some(scan) = &self.scan else {
            return false;
        };
        let fresh = scan.entries_from(self.entries.len());
        let got = !fresh.is_empty();
        self.entries.extend(fresh);
        let state = scan.state();
        if !state.is_over() {
            return got;
        }
        self.partial = match state {
            ScanState::LimitReached => Some("The list is partial: too many entries".into()),
            ScanState::Damaged(e) => Some(format!("The archive is damaged: {e}")),
            ScanState::Failed(e) => Some(format!("Cannot read the archive: {e}")),
            _ => None,
        };
        self.scan = None;
        true
    }

    /// Indices of the entries whose name holds the filter (case-insensitive).
    pub fn shown(&self) -> Vec<usize> {
        let needle = self.filter.text().trim().to_lowercase();
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| needle.is_empty() || e.name.to_lowercase().contains(&needle))
            .map(|(i, _)| i)
            .collect()
    }

    /// Moves the selection by `delta` rows within the shown entries.
    pub fn move_by(&mut self, delta: isize) {
        let n = self.shown().len();
        if n == 0 {
            self.selected = 0;
            return;
        }
        self.selected = self.selected.saturating_add_signed(delta).min(n - 1);
    }

    /// The selected entry, when there is one on the list.
    pub fn chosen(&self) -> Option<&ArchiveEntryInfo> {
        self.shown().get(self.selected).map(|&i| &self.entries[i])
    }

    /// The filter changed: back to the first match.
    pub fn filter_changed(&mut self) {
        self.selected = 0;
        self.top = 0;
    }

    /// Stops a scan still running (the entries found stay).
    pub fn close(&mut self) {
        if let Some(scan) = self.scan.take() {
            scan.cancel();
        }
    }
}

/// A size for the list: bytes, KB, MB or GB with one decimal.
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 3] = ["KB", "MB", "GB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut v = bytes as f64 / 1024.0;
    let mut unit = 0;
    while v >= 1024.0 && unit + 1 < UNITS.len() {
        v /= 1024.0;
        unit += 1;
    }
    format!("{v:.1} {}", UNITS[unit])
}

/// Why an entry cannot be opened, as the picker shows it.
pub fn refusal_text(r: &EntryRefusal) -> String {
    match r {
        EntryRefusal::Encrypted => "encrypted".into(),
        EntryRefusal::Method(m) => format!("unsupported compression: {m}"),
        EntryRefusal::UnsafeName => "unsafe name".into(),
        EntryRefusal::DuplicateName => "same path as an earlier entry".into(),
        EntryRefusal::LinkOrSpecial => "link or special file".into(),
        EntryRefusal::Sparse => "sparse entry".into(),
        EntryRefusal::DictionaryTooLarge => "dictionary too large".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str) -> ArchiveEntryInfo {
        ArchiveEntryInfo {
            name: name.into(),
            size: 10,
            compressed_size: 10,
            block_size: None,
            offset: None,
            refusal: None,
        }
    }

    #[test]
    fn typing_filters_and_the_selection_stays_on_the_list() {
        let mut p = EntryPicker::new(
            "logs.zip".into(),
            vec![entry("app.log"), entry("db.log"), entry("DB-old.log")],
            false,
        );
        assert_eq!(p.shown(), [0, 1, 2]);
        p.move_by(5);
        assert_eq!(
            p.chosen().unwrap().name,
            "DB-old.log",
            "clamped to the last"
        );
        p.filter.insert("db");
        p.filter_changed();
        assert_eq!(p.shown(), [1, 2], "case-insensitive");
        assert_eq!(p.chosen().unwrap().name, "db.log");
        p.move_by(-3);
        assert_eq!(p.selected, 0);
        p.filter.insert("zzz");
        p.filter_changed();
        assert!(p.chosen().is_none());
    }

    #[test]
    fn sizes_read_as_units() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(1536), "1.5 KB");
        assert_eq!(human_size(5 * 1024 * 1024), "5.0 MB");
        assert_eq!(human_size(3 << 40), "3072.0 GB");
    }
}
