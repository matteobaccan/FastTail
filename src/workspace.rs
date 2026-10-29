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
use std::collections::BTreeMap;
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

/// Opens an Event Log channel or `.evtx` file.
fn open_event_log(
    parsed: Result<crate::eventlog::EventLogSpec, &'static str>,
    config: &FastTailConfig,
    wake: Option<WakeFn>,
) -> OpenOutcome {
    let error = match parsed {
        Err(key) => OpenError::System {
            key,
            detail: String::new(),
        },
        Ok(spec) => match crate::eventlog::open_engine(
            spec,
            config.event_log_initial_events,
            &crate::stdin_source::Settings::from_config(
                config.spool_dir.as_deref(),
                config.stdin_spool_max_mb,
            ),
            wake,
        ) {
            Ok(engine) => return OpenOutcome::Opened(Box::new(engine)),
            Err(e) => OpenError::System {
                key: e.key,
                detail: e.detail,
            },
        },
    };
    OpenOutcome::Failed {
        error,
        report: true,
    }
}

/// Opens `path`. `wake` is called from the watcher threads when new data arrives, so a
/// front end waiting on input can redraw.
pub fn open_target(path: &Path, config: &FastTailConfig, wake: Option<WakeFn>) -> OpenOutcome {
    // A file with its rotations: asked for (`rotated:<file>`), or always when the setting
    // says so and the file has some.
    let rotated = crate::rotation::parse(path).or_else(|| {
        (config.rotations == crate::rotation::Policy::Always
            && path.is_file()
            && !crate::rotation::siblings(path).is_empty())
        .then(|| path.to_path_buf())
    });
    if let Some(live) = rotated {
        let settings = crate::stdin_source::Settings::from_config(
            config.spool_dir.as_deref(),
            config.compressed_max_gb.saturating_mul(1024),
        );
        return match crate::rotation::open_engine(&live, &settings, wake) {
            Ok(engine) => OpenOutcome::Opened(Box::new(engine)),
            Err(e) => OpenOutcome::Failed {
                error: OpenError::Io(e),
                report: true,
            },
        };
    }
    if let Some(uri) = crate::plugins::PluginUri::parse(path) {
        let dir = crate::plugins::default_dir();
        let catalog = crate::plugins::discover(&dir);
        let config_dir = dir.parent().map(Path::to_path_buf).unwrap_or_default();
        return match crate::plugin_source::open_engine(
            uri,
            &catalog,
            &config.plugins,
            &config.stdin_settings(),
            &config_dir,
            wake,
        ) {
            Ok(engine) => OpenOutcome::Opened(Box::new(engine)),
            Err(e) => OpenOutcome::Failed {
                error: OpenError::System {
                    key: "plugin_failed",
                    detail: e.to_string(),
                },
                report: true,
            },
        };
    }
    if let Some(parsed) = crate::debug_output::DbwinSpec::parse(path) {
        let error = match parsed {
            Err(key) => OpenError::System {
                key,
                detail: String::new(),
            },
            Ok(spec) => match crate::debug_output::open_engine(
                spec,
                &crate::stdin_source::Settings::from_config(
                    config.spool_dir.as_deref(),
                    config.stdin_spool_max_mb,
                ),
                wake,
            ) {
                Ok(engine) => return OpenOutcome::Opened(Box::new(engine)),
                Err(e) => OpenError::System {
                    key: e.key,
                    detail: e.detail,
                },
            },
        };
        return OpenOutcome::Failed {
            error,
            report: true,
        };
    }
    if let Some(parsed) = crate::eventlog::EventLogSpec::parse(path) {
        return open_event_log(parsed, config, wake);
    }
    // An exported Event Log file: its events as text on Windows (elsewhere it opens as
    // bytes).
    if cfg!(windows) && crate::eventlog::is_evtx_file(path) {
        let spec = crate::eventlog::EventLogSpec {
            source: crate::eventlog::Source::File(path.to_path_buf()),
            filter: Default::default(),
        };
        return open_event_log(Ok(spec), config, wake);
    }
    if let Some(parsed) = crate::net_listener::ListenSpec::parse(path) {
        let error = match parsed {
            Err(key) => OpenError::System {
                key,
                detail: String::new(),
            },
            Ok(spec) => match crate::net_listener::open_engine(
                spec,
                config.listener_limits(),
                &config.listener_spool_settings(),
                wake,
            ) {
                Ok(engine) => return OpenOutcome::Opened(Box::new(engine)),
                Err(e) => OpenError::System {
                    key: "listen_bind_failed",
                    detail: format!("{}: {}", e.addr, e.error),
                },
            },
        };
        return OpenOutcome::Failed {
            error,
            report: true,
        };
    }
    if let Some(parsed) = crate::ssh_source::SshSpec::parse(path) {
        let settings = config.ssh_settings();
        let error = match parsed {
            Err(key) => OpenError::System {
                key,
                detail: String::new(),
            },
            Ok(_) if !crate::system_source::valid_program(&settings.program) => OpenError::System {
                key: "system_bad_program",
                detail: settings.program,
            },
            Ok(spec) => {
                match crate::ssh_source::open_engine(
                    spec,
                    &settings,
                    &config.remote_spool_settings(),
                    wake,
                ) {
                    Ok(engine) => return OpenOutcome::Opened(Box::new(engine)),
                    Err(e) => OpenError::System {
                        key: "system_start_failed",
                        detail: format!("{}: {e}", settings.program),
                    },
                }
            }
        };
        return OpenOutcome::Failed {
            error,
            report: true,
        };
    }
    if let Some(parsed) = crate::system_source::SystemUri::parse(path) {
        let error = match parsed {
            Err(key) => OpenError::System {
                key,
                detail: String::new(),
            },
            Ok(uri) => {
                let commands = config.system_commands();
                let program = uri.command_line(&commands, None).0;
                if !crate::system_source::valid_program(&program) {
                    return OpenOutcome::Failed {
                        error: OpenError::System {
                            key: "system_bad_program",
                            detail: program,
                        },
                        report: true,
                    };
                }
                match crate::system_source::open_engine(
                    uri,
                    &commands,
                    &config.stdin_settings(),
                    wake,
                ) {
                    Ok(engine) => return OpenOutcome::Opened(Box::new(engine)),
                    Err(e) => OpenError::System {
                        key: "system_start_failed",
                        detail: format!("{program}: {e}"),
                    },
                }
            }
        };
        return OpenOutcome::Failed {
            error,
            report: true,
        };
    }
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
    engine.set_level_default(config.level_detect_bytes);
    engine.set_out_of_order_tolerance(config.out_of_order_tolerance_ms as i64);
    engine.auto_bookmark_max = config.auto_bookmark_max;
    engine.set_highlight_rules(config.highlight_rules.clone());
    engine.size_unit = config.size_unit;
    engine.show_line_numbers = config.show_line_numbers;
    engine.show_time_delta = config.show_time_delta;
    engine.pin_time_default = config.pin_time_column;
    engine.set_auto_tokens(config.auto_tokens());
}

/// Restores what `fasttail.ini` saved for `path`: wrap, bookmarks and notes, then the
/// stream state (see `apply_stream_state`).
pub fn restore_stream(engine: &mut TailEngine, config: &FastTailConfig, path: &Path) {
    engine.wrap_lines = config.wrap_for(path);
    restore_bookmarks(engine, config, path);
    apply_stream_state(engine, config, path);
}

/// Applies the saved bookmarks of `path`. A compressed stream starts on an empty spool:
/// its bookmarks wait until the index covers them (see `TailEngine::poll_compressed`).
fn restore_bookmarks(engine: &mut TailEngine, cfg: &FastTailConfig, path: &Path) {
    if engine.system.is_some()
        || engine.ssh.is_some()
        || engine.listener.is_some()
        || engine.debug_output.is_some()
    {
        return;
    }
    // A derived stream saves them by source line: they wait for its fill to reach them.
    if let Some(feeder) = engine.derived.as_mut() {
        if let Some((_, lines, notes)) = cfg.saved_bookmarks(path) {
            feeder.pending_bookmarks = lines.clone();
            feeder.pending_notes = notes.clone();
        }
        return;
    }
    if let Some(c) = engine.compressed.as_mut() {
        if let Some((_, lines, notes)) = cfg.saved_bookmarks(path) {
            c.pending_bookmarks = lines.clone();
            c.pending_bookmark_notes = notes.clone();
        }
    } else if let Some((lines, notes)) = cfg.bookmarks_with_notes_for(path, engine.total_lines()) {
        engine.set_bookmarks_with_notes(lines, notes);
    }
}

/// Applies the persisted filters, search query, encoding and ANSI mode saved for `path`
/// (wrap and bookmarks are applied by the caller from their own sections).
fn apply_stream_state(engine: &mut TailEngine, cfg: &FastTailConfig, path: &Path) {
    let Some(entry) = cfg.stream_state_for(path).cloned() else {
        return;
    };
    engine.timeline_open = entry.timeline;
    engine.set_plots(
        entry
            .plot
            .as_deref()
            .map(crate::plot::PlotSpec::parse_list)
            .unwrap_or_default(),
    );
    engine.timeline_dirty = false;
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
    if let Some(mut choice) = entry.fields_parser.as_deref().and_then(|name| {
        crate::fields::ParserChoice::from_name(name, entry.fields_regex.as_deref().unwrap_or(""))
    }) {
        if let crate::fields::ParserChoice::Delimited { sep, header } = &mut choice {
            if let Some(s) = entry
                .csv_separator
                .as_deref()
                .and_then(crate::fields::separator_from_name)
            {
                *sep = s;
            }
            *header = entry.csv_header.unwrap_or(true);
        }
        engine.set_field_choice(choice);
    }
    // Only a saved choice: without one a CSV / TSV stream opens in the column view.
    if entry.fields_view || entry.fields_view_set {
        engine.set_fields_view(entry.fields_view);
    }
    engine.set_field_columns(entry.fields_columns.clone());
    engine.set_pinned_columns(entry.pinned.clone());
    for (key, cells) in &entry.fields_widths {
        engine.set_field_width(key, *cells);
    }
    engine.fields_dirty = false;
    // Before the filters, whose level stage uses it.
    engine.set_level_override(entry.level_bytes);
    engine.level_bytes_dirty = false;
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
    engine.restore_filter_expr(entry.filter_expr_mode, &entry.filter_expr);
    engine.search_mode = entry.search_mode;
    engine.search_case_sensitive = entry.search_case;
    if !entry.search_query.is_empty() {
        engine.search_query = entry.search_query.clone();
        engine.update_search(&entry.search_query);
    }
    if let Some(arch) = entry
        .disasm_arch
        .as_deref()
        .and_then(crate::disasm::Arch::from_name)
    {
        engine.asm.arch = arch;
    }
    // After the encoding (a new one rebuilds the index and drops them) and with the
    // filters, whose visible lines they take out.
    if !entry.hidden.is_empty() {
        engine.set_hidden_ranges(entry.hidden.clone());
        engine.hidden_dirty = false;
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

/// Whether a stream is kept in the workspace and in sessions: not standard input (its
/// spool goes with the process). A derived stream of "Open filter as new tab" is kept
/// under its `filter:` name and rebuilt from its source, a merged view under its
/// `merged:` name and rebuilt from its sources (see `persisted_path`).
pub fn is_persisted(engine: &TailEngine) -> bool {
    !engine.is_stdin()
        && engine
            .merged
            .as_ref()
            .is_none_or(|m| m.identity().is_some())
}

/// The saved name of a stream whose spool goes with the process: a derived stream's
/// `filter:<n>:<source>`, a saved merged view's `merged:<n>:<sources>`.
pub fn saved_identity(engine: &TailEngine) -> Option<PathBuf> {
    match (&engine.derived, &engine.merged) {
        (Some(feeder), _) => Some(feeder.identity()),
        (None, Some(feeder)) => feeder.identity(),
        (None, None) => None,
    }
}

/// The path a stream is saved under: its own, or its saved name (`saved_identity`).
pub fn persisted_path(engine: &TailEngine) -> PathBuf {
    saved_identity(engine).unwrap_or_else(|| engine.path.clone())
}

/// The bookmarks and notes of `engine` as they are saved. A derived stream's are
/// stored by source line, so they find their lines again when it is rebuilt; the ones
/// still waiting for the fill are kept.
fn saved_bookmarks(engine: &TailEngine) -> (Vec<usize>, BTreeMap<usize, String>) {
    // A merged view's lines are numbered again at each rebuild, and a system source
    // starts from another backlog when it reopens: their bookmarks stay with the process.
    if engine.merged.is_some()
        || engine.system.is_some()
        || engine.ssh.is_some()
        || engine.listener.is_some()
        || engine.debug_output.is_some()
    {
        return (Vec::new(), BTreeMap::new());
    }
    let Some(feeder) = &engine.derived else {
        return (
            engine.bookmarks.iter().copied().collect(),
            engine.bookmark_notes.clone(),
        );
    };
    let mut lines = feeder.pending_bookmarks.clone();
    let mut notes = feeder.pending_notes.clone();
    for &line in &engine.bookmarks {
        if let Some(source) = feeder.source_line(line) {
            lines.push(source);
            if let Some(note) = engine.bookmark_notes.get(&line) {
                notes.insert(source, note.clone());
            }
        }
    }
    lines.sort_unstable();
    lines.dedup();
    (lines, notes)
}

/// The session entry describing `engine` as it is now. The line-number and time delta
/// switches are always recorded, so a saved stream never depends on the defaults.
pub fn stream_entry(engine: &TailEngine) -> StreamEntry {
    let (mut bookmarks, mut bookmark_notes) = saved_bookmarks(engine);
    if let Some(c) = engine.compressed.as_ref() {
        // A restored compressed stream still waiting for its index to reach them.
        if bookmarks.is_empty() {
            bookmarks = c.pending_bookmarks.clone();
            bookmark_notes = c.pending_bookmark_notes.clone();
        }
    }
    StreamEntry {
        path: persisted_path(engine),
        include_filter: engine.include_filter().to_string(),
        exclude_filter: engine.exclude_filter().to_string(),
        include_extra: extra_terms(engine.include_terms()),
        exclude_extra: extra_terms(engine.exclude_terms()),
        filter_expr_mode: engine.filter_expr_mode(),
        filter_expr: engine.filter_expr().to_string(),
        search_query: engine.search_query.trim().to_string(),
        search_mode: engine.search_mode,
        search_case: engine.search_case_sensitive,
        wrap: engine.wrap_lines,
        encoding: Some(engine.encoding.name().to_string()),
        ansi: (engine.ansi_mode != AnsiMode::Auto).then(|| engine.ansi_mode.name().to_string()),
        timeline: engine.timeline_open,
        plot: (!engine.plots().is_empty())
            .then(|| crate::plot::PlotSpec::list_text(engine.plots())),
        collapse: engine
            .collapse_mode()
            .is_on()
            .then(|| engine.collapse_mode().name().to_string()),
        context_lines: engine.context_lines(),
        disasm_arch: (engine.asm.arch != crate::disasm::Arch::default())
            .then(|| engine.asm.arch.name().to_string()),
        level_bytes: engine.level_override(),
        hidden: engine.hidden_ranges().to_vec(),
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
            .then(|| engine.field_choice().saved_name()),
        fields_regex: match engine.field_choice() {
            crate::fields::ParserChoice::Regex(p) if !p.is_empty() => Some(p.clone()),
            _ => None,
        },
        csv_separator: match engine.field_choice() {
            crate::fields::ParserChoice::Delimited { sep, .. } => {
                Some(crate::fields::separator_name(*sep).to_string())
            }
            _ => None,
        },
        csv_header: match engine.field_choice() {
            crate::fields::ParserChoice::Delimited { header, .. } => Some(*header),
            _ => None,
        },
        fields_view: engine.fields_view(),
        fields_view_set: engine.field_view_chosen,
        fields_columns: engine.chosen_field_columns().to_vec(),
        fields_widths: engine.field_widths().clone(),
        pinned: engine.chosen_pins().map(<[String]>::to_vec),
        frozen: engine.derived.as_ref().map(|f| f.filter.clone()),
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
    let engines: Vec<&TailEngine> = engines.into_iter().collect();
    let mut open_files: Vec<PathBuf> = Vec::new();
    for path in order {
        if crate::stdin_source::is_stdin_path(path) {
            continue;
        }
        // A derived stream's or a merged view's tab names its spool: it is saved under
        // its `filter:` or `merged:` name (a merged view of standard input is not).
        let path = match engines
            .iter()
            .find(|e| (e.derived.is_some() || e.merged.is_some()) && e.path == *path)
        {
            Some(engine) => match saved_identity(engine) {
                // A merged view whose sources were closed would open them again.
                Some(identity) if !sources_open(&identity, &engines) => continue,
                Some(identity) => identity,
                None => continue,
            },
            None if crate::filter_tab::is_derived_path(path)
                || crate::merged_stream::is_merged_path(path) =>
            {
                continue
            }
            None => path.clone(),
        };
        if !open_files.iter().any(|p| paths_equal(p, &path)) {
            open_files.push(path);
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

/// Whether every source of the merged view saved as `identity` is among `engines` (any
/// other saved name: yes).
pub fn sources_open(identity: &Path, engines: &[&TailEngine]) -> bool {
    crate::merged_stream::parse_identity(identity).is_none_or(|(_, sources)| {
        sources
            .iter()
            .all(|s| engines.iter().any(|e| paths_equal(&persisted_path(e), s)))
    })
}

impl WorkspaceState {
    /// Keeps the derived streams and merged views of `previous` (the open files as saved
    /// before) whose sources are still open: a front end without them (the terminal
    /// interface) leaves them for the next one that has them.
    pub fn keep_derived(&mut self, previous: &[PathBuf]) {
        for path in previous {
            let sources = match crate::filter_tab::parse_identity(path) {
                Some((_, source)) => vec![source],
                None => match crate::merged_stream::parse_identity(path) {
                    Some((_, sources)) => sources,
                    None => continue,
                },
            };
            let open = |s: &PathBuf| self.open_files.iter().any(|p| paths_equal(p, s));
            if sources.iter().all(open) && !self.open_files.iter().any(|p| p == path) {
                self.open_files.push(path.clone());
            }
        }
    }

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
        // A folder group none of whose files is open any more ends (the terminal, which
        // runs no groups, closed them).
        config.folder_groups.retain(|g| {
            crate::wildcard::FolderSpec::parse(g)
                .is_some_and(|spec| open.iter().any(|f| spec.covers(f)))
        });
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
        engine.asm_dirty = false;
        engine.level_bytes_dirty = false;
        engine.hidden_dirty = false;
        return false;
    }
    let mut changed = false;
    let path = persisted_path(engine);
    if engine.bookmarks_dirty {
        engine.bookmarks_dirty = false;
        let (lines, notes) = saved_bookmarks(engine);
        config.set_bookmarks_with_notes(&path, &lines, &notes);
        changed = true;
    }
    if engine.wrap_dirty {
        engine.wrap_dirty = false;
        config.set_wrap(&path, engine.wrap_lines);
        changed = true;
    }
    if engine.ansi_dirty
        || engine.timeline_dirty
        || engine.collapse_mode_dirty
        || engine.context_lines_dirty
        || engine.view_columns_dirty
        || engine.asm_dirty
        || engine.level_bytes_dirty
        || engine.hidden_dirty
        || (engine.fields_dirty && !defer_fields)
    {
        engine.asm_dirty = false;
        engine.level_bytes_dirty = false;
        engine.hidden_dirty = false;
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
