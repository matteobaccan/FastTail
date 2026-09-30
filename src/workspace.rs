// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! What a front end opens and how a stream starts, shared by the GUI and the terminal
//! interface: `open_target` turns a path (plain file, `*.log` pattern, compressed file,
//! archive or archive entry) into an engine or into the choice the user has to make, and
//! `apply_settings` / `restore_stream` give a new engine the `fasttail.ini` settings and
//! the state saved for its path.

use crate::ansi::AnsiMode;
use crate::compressed::{ArchiveEntryInfo, Codec, OpenError, Target};
use crate::config::FastTailConfig;
use crate::tail_engine::{FileEncoding, TailEngine, WakeFn};
use std::path::{Path, PathBuf};

/// What opening a path gave.
pub enum OpenOutcome {
    /// An engine, still without settings (see `apply_settings` and `restore_stream`).
    Opened(Box<TailEngine>),
    /// No such file: skipped without a message (a workspace file that was deleted).
    Missing,
    /// Opening failed. `report` is false for a plain file, which is skipped silently; a
    /// compressed file or archive entry says why.
    Failed { error: OpenError, report: bool },
    /// An archive holds a single file entry: open that entry path instead.
    OpenEntry(PathBuf),
    /// A zip or 7z archive with no file entry.
    EmptyArchive(PathBuf),
    /// A zip or 7z archive with several entries (or a `partial` list, which stopped early):
    /// the user picks the entries to open.
    ChooseEntries {
        archive: PathBuf,
        entries: Vec<ArchiveEntryInfo>,
        partial: bool,
    },
    /// A tar archive: its entries are found by a background scan (`compressed::TarScan`)
    /// while the user picks.
    ScanTar {
        archive: PathBuf,
        codec: Option<Codec>,
    },
    /// Listing a zip or 7z archive failed.
    ListFailed {
        archive: PathBuf,
        error: std::io::Error,
    },
}

/// Opens `path`. `wake` is called from the watcher threads when new data arrives, so a
/// front end waiting on input can redraw.
pub fn open_target(path: &Path, config: &FastTailConfig, wake: Option<WakeFn>) -> OpenOutcome {
    if crate::wildcard::is_pattern_path(path) {
        // A pattern resolves to its newest match at every open.
        let opened = match wake {
            Some(wake) => TailEngine::open_pattern_with_wake(path, wake),
            None => TailEngine::open_pattern(path),
        };
        return match opened {
            Ok(engine) => OpenOutcome::Opened(Box::new(engine)),
            Err(err) => OpenOutcome::Failed {
                error: OpenError::Io(err),
                report: false,
            },
        };
    }
    if !crate::compressed::source_exists(path) {
        return OpenOutcome::Missing;
    }
    let settings = config.compressed_settings();
    let (opened, report) = match crate::compressed::classify(path) {
        Target::Plain => {
            let opened = match wake {
                Some(wake) => TailEngine::open_with_wake(path, wake),
                None => TailEngine::open(path),
            };
            (opened.map_err(OpenError::Io), false)
        }
        Target::Compressed(_) => (
            crate::compressed::open_engine(path, None, &settings, wake),
            true,
        ),
        Target::Entry { archive, entry, .. } => (
            crate::compressed::open_engine(&archive, Some(&entry), &settings, wake),
            true,
        ),
        Target::EmptyZip => return OpenOutcome::EmptyArchive(path.to_path_buf()),
        Target::TarArchive(codec) => {
            return OpenOutcome::ScanTar {
                archive: path.to_path_buf(),
                codec,
            }
        }
        Target::SevenZArchive => {
            return match crate::compressed::list_7z_entries(path) {
                Ok(listing) => listed(path, listing.entries, listing.partial),
                Err(error) => OpenOutcome::ListFailed {
                    archive: path.to_path_buf(),
                    error,
                },
            }
        }
        Target::ZipArchive => {
            return match crate::compressed::list_zip_entries(path) {
                Ok(entries) => listed(path, entries, false),
                Err(error) => OpenOutcome::ListFailed {
                    archive: path.to_path_buf(),
                    error,
                },
            }
        }
    };
    match opened {
        Ok(engine) => OpenOutcome::Opened(Box::new(engine)),
        Err(error) => OpenOutcome::Failed { error, report },
    }
}

/// A listed zip or 7z: one openable entry opens directly, none is empty, anything else
/// (or a partial list) is a choice.
fn listed(archive: &Path, entries: Vec<ArchiveEntryInfo>, partial: bool) -> OpenOutcome {
    match entries.as_slice() {
        [] if !partial => OpenOutcome::EmptyArchive(archive.to_path_buf()),
        [only] if only.refusal.is_none() && !partial => {
            OpenOutcome::OpenEntry(crate::compressed::entry_path(archive, &only.name))
        }
        _ => OpenOutcome::ChooseEntries {
            archive: archive.to_path_buf(),
            entries,
            partial,
        },
    }
}

/// Gives a new engine the global settings: refresh interval, Markdown cap, automatic
/// bookmark cap, highlight rules, size unit and the default line-number and time delta
/// columns. Standard input gets these and nothing from `restore_stream`.
pub fn apply_settings(engine: &mut TailEngine, config: &FastTailConfig) {
    engine.size_check_interval =
        std::time::Duration::from_millis(config.size_check_interval_ms as u64);
    engine.set_markdown_max_bytes((config.markdown_max_mb as u64) * 1024 * 1024);
    engine.auto_bookmark_max = config.auto_bookmark_max;
    engine.set_highlight_rules(config.highlight_rules.clone());
    engine.size_unit = config.size_unit;
    engine.show_line_numbers = config.show_line_numbers;
    engine.show_time_delta = config.show_time_delta;
}

/// Restores what `fasttail.ini` saved for `path`: wrap, bookmarks and notes, then the
/// stream state (see `apply_stream_state`).
pub fn restore_stream(engine: &mut TailEngine, config: &FastTailConfig, path: &Path) {
    engine.wrap_lines = config.wrap_for(path);
    restore_bookmarks(engine, config, path);
    apply_stream_state(engine, config);
}

/// Applies the saved bookmarks of `path`. A compressed stream starts on an empty spool:
/// its bookmarks wait until the index covers them (see `TailEngine::poll_compressed`).
fn restore_bookmarks(engine: &mut TailEngine, cfg: &FastTailConfig, path: &Path) {
    if let Some(c) = engine.compressed.as_mut() {
        if let Some((_, lines, notes)) = cfg.saved_bookmarks(path) {
            c.pending_bookmarks = lines.clone();
            c.pending_bookmark_notes = notes.clone();
        }
    } else if let Some((lines, notes)) = cfg.bookmarks_with_notes_for(path, engine.total_lines()) {
        engine.set_bookmarks_with_notes(lines, notes);
    }
}

/// Applies the persisted filters, search query, encoding and ANSI mode of the engine's
/// path (wrap and bookmarks are applied by the caller from their own sections).
fn apply_stream_state(engine: &mut TailEngine, cfg: &FastTailConfig) {
    let Some(entry) = cfg.stream_state_for(&engine.path).cloned() else {
        return;
    };
    engine.timeline_open = entry.timeline;
    engine.show_line_numbers = entry.line_numbers.unwrap_or(cfg.show_line_numbers);
    engine.show_time_delta = entry.time_delta.unwrap_or(cfg.show_time_delta);
    if let Some(zone) = entry
        .time_source_zone
        .as_deref()
        .and_then(crate::timestamp::SourceZone::from_config)
    {
        engine.set_time_source_zone(zone);
    }
    if let Some(display) = entry
        .time_display
        .as_deref()
        .and_then(crate::timestamp::TimeDisplay::from_config)
    {
        engine.set_time_display(display);
    }
    engine.view_columns_dirty = false;
    if let Some(choice) = entry.fields_parser.as_deref().and_then(|name| {
        crate::fields::ParserChoice::from_name(name, entry.fields_regex.as_deref().unwrap_or(""))
    }) {
        engine.set_field_choice(choice);
    }
    engine.set_fields_view(entry.fields_view);
    engine.set_field_columns(entry.fields_columns.clone());
    for (key, cells) in &entry.fields_widths {
        engine.set_field_width(key, *cells);
    }
    engine.fields_dirty = false;
    // First, so the filters and the search below run once, on the right text.
    if let Some(mode) = entry.ansi.as_deref().and_then(AnsiMode::from_name) {
        engine.set_ansi_mode(mode);
        engine.ansi_dirty = false;
    }
    if let Some(enc) = entry.encoding.as_deref().and_then(FileEncoding::from_name) {
        if enc != engine.encoding {
            // Re-decoding rebuilds the index and drops bookmarks: restore them after.
            let bookmarks: Vec<usize> = engine.bookmarks.iter().copied().collect();
            let notes = engine.bookmark_notes.clone();
            engine.set_encoding(enc);
            if !bookmarks.is_empty() && bookmarks.iter().all(|&l| l < engine.total_lines()) {
                engine.set_bookmarks_with_notes(bookmarks, notes);
            }
        }
    }
    let terms = |first: &String, extra: &[String]| -> Vec<String> {
        std::iter::once(first.clone())
            .chain(extra.iter().cloned())
            .collect()
    };
    let include = terms(&entry.include_filter, &entry.include_extra);
    let exclude = terms(&entry.exclude_filter, &entry.exclude_extra);
    if include.iter().chain(&exclude).any(|t| !t.is_empty()) {
        engine.set_filter_terms(include, exclude);
    }
    if !entry.search_query.is_empty() {
        engine.search_query = entry.search_query.clone();
        engine.update_search(&entry.search_query);
    }
    // Context lines come from the matches: after the filters, before the groups.
    if entry.context_lines > 0 {
        engine.set_context_lines(entry.context_lines);
        engine.context_lines_dirty = false;
    }
    // Last, so the groups are detected once, over the filtered lines.
    if let Some(mode) = entry
        .collapse
        .as_deref()
        .and_then(crate::collapse::CollapseMode::from_name)
    {
        engine.set_collapse_mode(mode);
        engine.collapse_mode_dirty = false;
    }
}
