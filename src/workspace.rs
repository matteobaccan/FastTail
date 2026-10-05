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
use crate::paths::paths_equal;
use crate::session::StreamEntry;
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
    engine.set_auto_tokens(config.auto_tokens());
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

/// Whether a stream is kept in the workspace and in sessions: not standard input, not a
/// derived stream of "Open filter as new tab" (their spools go with the process).
pub fn is_persisted(engine: &TailEngine) -> bool {
    !engine.is_stdin() && engine.derived.is_none()
}

/// The session entry describing `engine` as it is now. The line-number and time delta
/// switches are always recorded, so a saved stream never depends on the defaults.
pub fn stream_entry(engine: &TailEngine) -> StreamEntry {
    let mut bookmarks: Vec<usize> = engine.bookmarks.iter().copied().collect();
    let mut bookmark_notes = engine.bookmark_notes.clone();
    if let Some(c) = engine.compressed.as_ref() {
        // A restored compressed stream still waiting for its index to reach them.
        if bookmarks.is_empty() {
            bookmarks = c.pending_bookmarks.clone();
            bookmark_notes = c.pending_bookmark_notes.clone();
        }
    }
    StreamEntry {
        path: engine.path.clone(),
        include_filter: engine.include_filter().to_string(),
        exclude_filter: engine.exclude_filter().to_string(),
        include_extra: extra_terms(engine.include_terms()),
        exclude_extra: extra_terms(engine.exclude_terms()),
        search_query: engine.search_query.trim().to_string(),
        wrap: engine.wrap_lines,
        encoding: Some(engine.encoding.name().to_string()),
        ansi: (engine.ansi_mode != AnsiMode::Auto).then(|| engine.ansi_mode.name().to_string()),
        timeline: engine.timeline_open,
        collapse: engine
            .collapse_mode()
            .is_on()
            .then(|| engine.collapse_mode().name().to_string()),
        context_lines: engine.context_lines(),
        line_numbers: Some(engine.show_line_numbers),
        time_delta: Some(engine.show_time_delta),
        time_display: (engine.time_display() != crate::timestamp::TimeDisplay::Written)
            .then(|| engine.time_display().to_config()),
        time_source_zone: (engine.time_source_zone() != crate::timestamp::SourceZone::Local)
            .then(|| engine.time_source_zone().to_config()),
        bookmarks,
        bookmark_notes,
        archive_entry: engine.compressed.as_ref().and_then(|c| c.entry.clone()),
        fields_parser: (*engine.field_choice() != crate::fields::ParserChoice::Auto)
            .then(|| engine.field_choice().name().to_string()),
        fields_regex: match engine.field_choice() {
            crate::fields::ParserChoice::Regex(p) if !p.is_empty() => Some(p.clone()),
            _ => None,
        },
        fields_view: engine.fields_view(),
        fields_columns: engine.chosen_field_columns().to_vec(),
        fields_widths: engine.field_widths().clone(),
    }
}

/// The non-empty terms after the first row, as a session stores them.
fn extra_terms(terms: &[String]) -> Vec<String> {
    terms
        .iter()
        .skip(1)
        .filter(|t| !t.is_empty())
        .cloned()
        .collect()
}

/// The workspace as `fasttail.ini` keeps it: the open files in tab order and the state of
/// each stream (filters, search, encoding, columns, bookmarks, wrap...).
#[derive(Debug, Clone, PartialEq)]
pub struct WorkspaceState {
    pub open_files: Vec<PathBuf>,
    pub streams: Vec<StreamEntry>,
}

/// The workspace of `engines`, with `order` the paths of the open tabs in the order the
/// front end shows them. Streams that are not persisted are left out.
pub fn snapshot<'a>(
    order: &[PathBuf],
    engines: impl IntoIterator<Item = &'a TailEngine>,
) -> WorkspaceState {
    let mut open_files: Vec<PathBuf> = Vec::new();
    for path in order {
        let skipped =
            crate::stdin_source::is_stdin_path(path) || crate::filter_tab::is_derived_path(path);
        if !skipped && !open_files.iter().any(|p| paths_equal(p, path)) {
            open_files.push(path.clone());
        }
    }
    let streams = engines
        .into_iter()
        .filter(|e| is_persisted(e))
        .map(stream_entry)
        .collect();
    WorkspaceState {
        open_files,
        streams,
    }
}

impl WorkspaceState {
    /// Writes the open files and the stream states into `config`, dropping the state of
    /// files no longer open. Wrap and bookmarks live in their own sections, written as
    /// they change (see `save_changes`), so their most-recent order is kept.
    pub fn write_into(self, config: &mut FastTailConfig) {
        config.open_files = self.open_files;
        for entry in self.streams {
            config.set_stream_state(stream_state_only(entry));
        }
        let open = config.open_files.clone();
        config.retain_stream_state_of(&open);
    }
}

/// `entry` without wrap and bookmarks, as the `stream_N` sections store it.
fn stream_state_only(mut entry: StreamEntry) -> StreamEntry {
    entry.wrap = false;
    entry.bookmarks.clear();
    entry.bookmark_notes.clear();
    entry
}

/// Records in `config` what changed in `engine` since the last call (bookmarks and notes,
/// wrap, and the stream state: ANSI mode, timeline, collapse, context lines, columns,
/// fields) and clears the change flags. `defer_fields` keeps a field column change for a
/// later call (a width still being dragged). True when `config` changed and should be
/// saved. Nothing of a stream that is not persisted is recorded.
pub fn save_changes(
    engine: &mut TailEngine,
    config: &mut FastTailConfig,
    defer_fields: bool,
) -> bool {
    if !is_persisted(engine) {
        engine.bookmarks_dirty = false;
        engine.wrap_dirty = false;
        engine.ansi_dirty = false;
        engine.timeline_dirty = false;
        engine.collapse_mode_dirty = false;
        engine.context_lines_dirty = false;
        engine.view_columns_dirty = false;
        engine.fields_dirty = false;
        return false;
    }
    let mut changed = false;
    if engine.bookmarks_dirty {
        engine.bookmarks_dirty = false;
        let lines: Vec<usize> = engine.bookmarks.iter().copied().collect();
        config.set_bookmarks_with_notes(&engine.path, &lines, &engine.bookmark_notes);
        changed = true;
    }
    if engine.wrap_dirty {
        engine.wrap_dirty = false;
        config.set_wrap(&engine.path, engine.wrap_lines);
        changed = true;
    }
    if engine.ansi_dirty
        || engine.timeline_dirty
        || engine.collapse_mode_dirty
        || engine.context_lines_dirty
        || engine.view_columns_dirty
        || (engine.fields_dirty && !defer_fields)
    {
        engine.ansi_dirty = false;
        engine.timeline_dirty = false;
        engine.collapse_mode_dirty = false;
        engine.context_lines_dirty = false;
        engine.view_columns_dirty = false;
        engine.fields_dirty = false;
        config.set_stream_state(stream_state_only(stream_entry(engine)));
        changed = true;
    }
    changed
}

/// Streams reopened from a saved workspace.
pub struct Restored {
    /// The engines, with their settings and saved state, in the order of the paths.
    pub engines: Vec<TailEngine>,
    /// Paths that did not open: missing, unreadable, or an archive whose entries have to
    /// be chosen (a workspace keeps entry paths, not bare archives).
    pub skipped: Vec<PathBuf>,
}

/// Reopens `paths` (duplicates ignored) with the settings and the saved state of each: a
/// pattern resolves to its newest match again, a compressed file is decompressed again in
/// the background.
pub fn open_all(paths: &[PathBuf], config: &FastTailConfig, wake: Option<WakeFn>) -> Restored {
    let mut restored = Restored {
        engines: Vec::new(),
        skipped: Vec::new(),
    };
    let mut seen: Vec<&PathBuf> = Vec::new();
    for path in paths {
        if seen.iter().any(|p| paths_equal(p, path)) {
            continue;
        }
        seen.push(path);
        match open_target(path, config, wake.clone()) {
            OpenOutcome::Opened(engine) => {
                let mut engine = *engine;
                apply_settings(&mut engine, config);
                restore_stream(&mut engine, config, path);
                restored.engines.push(engine);
            }
            _ => restored.skipped.push(path.clone()),
        }
    }
    restored
}

/// Reopens the workspace `fasttail.ini` keeps: its open files, in tab order.
pub fn restore(config: &FastTailConfig, wake: Option<WakeFn>) -> Restored {
    open_all(&config.open_files, config, wake)
}

/// Row data handed to an external tool: the row text, the file being tailed (the resolved
/// file of a pattern stream, the archive of a compressed one, never its spool), the
/// 1-based line number and the selection text when the row is part of a selection.
pub fn tool_context_for_row(
    engine: &TailEngine,
    row: usize,
) -> Option<crate::external_tools::ToolContext> {
    let line = engine.get_line(row)?.into_owned();
    let file = engine.source_file();
    let selection = if engine.has_selection() && engine.is_selected(row) {
        engine.copy_selection_text()
    } else {
        None
    };
    Some(crate::external_tools::ToolContext::for_row(
        &file,
        row + 1,
        &line,
        selection.as_deref(),
    ))
}
