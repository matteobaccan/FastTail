// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The terminal app: one `TailEngine` per stream, the key handling and the drawing.
//! Every stream is a bordered window; the status bar, the prompts and the help are
//! bordered too, so the screen reads as a set of windows rather than a text dump.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use crate::ansi::{AnsiMode, StyleRun};
use crate::collapse::CollapseMode;
use crate::log_level::LogLevel;
use crate::scan_job::ScanKind;
use crate::tail_engine::{TailEngine, ViewMode};
use crate::time_range_text::side_readable;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};
use ratatui::Frame;

use crate::tui::clipboard::{Clipboard, Copied};
use crate::tui::colors::{Chrome, Palette};
use crate::tui::form::{FieldKey, TextField};
use crate::tui::hex;
use crate::tui::keys::{self, Action};
use crate::tui::mouse::{self, DialogHit, HitMap, Target, WindowHit};
use crate::tui::picker::{refusal_text, EntryPicker};
use crate::tui::view::{self, Paint};

/// Columns moved by one horizontal scroll step.
/// Characters of a bookmark note the status bar shows.
const NOTE_PREVIEW_CHARS: usize = 40;
/// Cells one `←` / `→` scrolls sideways.
const HSCROLL_STEP: usize = 1;

/// One open stream and the view state the terminal keeps for it.
pub struct Tab {
    pub engine: TailEngine,
    pub title: String,
    /// First row on screen (a row of the engine's filtered, collapsed view).
    pub top: usize,
    /// Terminal cells hidden on the left (horizontal scroll).
    pub hscroll: usize,
    /// The keyboard cursor: a row of the view, drawn reversed. Row actions act on it when
    /// nothing is selected. In follow mode it stays on the last row.
    pub cursor: usize,
    /// Text rows of its window at the last draw (the page size).
    pub height: usize,
    /// Bytes per row of the HEX view, from the window width at the last draw. In the HEX
    /// view `top` and `cursor` count these rows.
    pub hex_width: usize,
    /// Waiting for the first hit of a search that runs in the background.
    pending_first_hit: bool,
    /// Lines already seen while this stream was on screen, for the "new lines" mark.
    seen_lines: usize,
}

impl Tab {
    pub fn new(mut engine: TailEngine) -> Self {
        // Markdown is a GUI view: the terminal shows the lines (or the HEX a binary
        // file opened in).
        if !matches!(engine.view_mode, ViewMode::Text | ViewMode::Hex) {
            engine.set_view_mode(ViewMode::Text);
        }
        let title = stream_title(&engine);
        Self {
            engine,
            title,
            top: 0,
            hscroll: 0,
            cursor: 0,
            height: 20,
            hex_width: 16,
            pending_first_hit: false,
            seen_lines: 0,
        }
    }

    /// Moves the cursor to `row` (clamped to the view) and scrolls the window to keep it
    /// visible. Any move pauses follow; `Bottom` turns it back on.
    pub fn set_cursor(&mut self, row: usize) {
        let rows = self.row_count();
        self.engine.follow_tail = false;
        self.cursor = row.min(rows.saturating_sub(1));
        self.top = view::reveal(self.top, self.height.max(1), rows, self.cursor);
    }

    /// The line under the cursor (the first line of a collapsed group); none in HEX.
    pub fn cursor_line(&self) -> Option<usize> {
        if self.is_hex() {
            return None;
        }
        self.engine.get_actual_line_idx(self.cursor)
    }

    pub fn is_hex(&self) -> bool {
        self.engine.view_mode == ViewMode::Hex
    }

    /// Rows of the view: HEX rows, or the filtered and collapsed lines.
    pub fn row_count(&self) -> usize {
        if self.is_hex() {
            self.engine.total_hex_rows(self.hex_width)
        } else {
            self.engine.visible_line_count()
        }
    }

    /// Switches between the lines and the HEX view keeping the place: the HEX row
    /// holding the first byte of the cursor line, or the line holding the first byte of
    /// the cursor row (the next visible one when the filters hide it). Follow stays.
    fn toggle_hex(&mut self) {
        let follow = self.engine.follow_tail;
        let n = self.hex_width.max(1);
        if self.is_hex() {
            let offset = (self.cursor * n) as u64;
            self.engine.set_view_mode(ViewMode::Text);
            if !follow {
                let line = self.engine.line_of_offset(offset);
                self.set_cursor(self.engine.row_of_line_or_next(line));
            }
        } else {
            let offset = self
                .cursor_line()
                .and_then(|l| self.engine.line_offsets.get(l).copied())
                .unwrap_or(0);
            self.engine.set_view_mode(ViewMode::Hex);
            // The engine would carry the current text hit over; the place is the cursor.
            self.engine.scroll_to_byte = None;
            if !follow {
                self.set_cursor(offset as usize / n);
            }
        }
        self.engine.follow_tail = follow;
        self.pin_cursor();
    }

    /// Adopts the bytes per row a window `width` cells wide allows, keeping the cursor
    /// and the top on the same bytes.
    fn fit_hex_width(&mut self, width: usize) {
        let n = hex::bytes_per_row(width, hex::offset_digits(self.engine.file_size));
        if n != self.hex_width {
            self.cursor = self.cursor * self.hex_width / n;
            self.top = self.top * self.hex_width / n;
            self.hex_width = n;
        }
    }

    /// In follow mode the cursor sits on the last row, which moves as lines arrive.
    fn pin_cursor(&mut self) {
        let rows = self.row_count();
        if self.engine.follow_tail {
            self.cursor = rows.saturating_sub(1);
        } else if rows > 0 && self.cursor >= rows {
            self.cursor = rows - 1;
        }
    }
}

/// The title of a stream: its file name, `archive/entry` for an archive entry (the
/// compressed stream's own title with a plain separator), `stdin`.
fn stream_title(engine: &TailEngine) -> String {
    if engine.is_stdin() {
        return crate::stdin_source::STDIN_TITLE.to_string();
    }
    match &engine.compressed {
        Some(c) => c.title().replace(" \u{203a} ", "/"),
        None => engine
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| engine.path.display().to_string()),
    }
}

/// Which field the prompt dialog edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    Search,
    Include,
    Exclude,
    /// The note of the bookmark on this line.
    Note(usize),
    /// Go to a line or a time.
    Goto,
    /// A file, pattern or archive entry path to open.
    OpenFile,
    /// The file to save the open streams to as a session.
    SaveSession,
}

pub struct Prompt {
    pub kind: PromptKind,
    pub field: TextField,
}

/// The open-session dialog: the recent sessions and a field for a typed path.
pub struct SessionDialog {
    pub recent: Vec<PathBuf>,
    pub selected: usize,
    pub field: TextField,
}

/// The time range dialog of the focused stream: the two sides as typed, which one has
/// the keyboard, and whether `[ OK ]` found a side it cannot read.
pub struct TimeRangeDialog {
    pub from: TextField,
    pub to: TextField,
    pub on_to: bool,
    pub invalid: bool,
}

impl TimeRangeDialog {
    fn focused(&mut self) -> &mut TextField {
        if self.on_to {
            &mut self.to
        } else {
            &mut self.from
        }
    }
}

/// How the screen is divided between streams.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDir {
    SideBySide,
    Stacked,
}

/// Two windows on screen: the focused stream (`App::active`) and `other`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Split {
    pub dir: SplitDir,
    pub other: usize,
}

/// What one window shows: the loop redraws only when a signature changes.
#[derive(Debug, Clone, PartialEq, Default)]
struct PaneSignature {
    tab: usize,
    total: usize,
    rows: usize,
    file_size: u64,
    filter_generation: u64,
    search_generation: u64,
    current_hit: Option<usize>,
    scan: Option<(u8, u16)>,
    levels: usize,
    follow: bool,
    collapse_pending: bool,
    decompressed: Option<u16>,
    hex: Option<(usize, Option<(usize, usize)>)>,
    ansi: AnsiMode,
}

#[derive(Debug, Clone, PartialEq, Default)]
struct Signature {
    active: usize,
    split: Option<Split>,
    panes: Vec<PaneSignature>,
    unseen_tabs: usize,
}

pub struct App {
    pub tabs: Vec<Tab>,
    /// The stream with the keyboard focus.
    pub active: usize,
    pub split: Option<Split>,
    pub palette: Palette,
    pub prompt: Option<Prompt>,
    pub time_range: Option<TimeRangeDialog>,
    pub picker: Option<EntryPicker>,
    pub sessions: Option<SessionDialog>,
    /// A session file that exists, waiting for `[ OK ]` to be overwritten.
    pub confirm_overwrite: Option<PathBuf>,
    /// The configuration new streams are set up with (none in tests and benchmarks:
    /// the defaults).
    pub settings: Option<crate::tui::workspace::Settings>,
    /// Saves the configuration every `SAVE_EVERY` when it changed (the interactive run;
    /// not captures, benchmarks or tests).
    pub autosave: bool,
    last_save: Instant,
    /// The last save error shown, so a failing save is reported once, not every 2 s.
    save_error: Option<String>,
    pub message: Option<String>,
    pub show_help: bool,
    /// First line of the help shown (it scrolls on a short screen).
    pub help_top: usize,
    pub quit: bool,
    /// Mouse capture is on (the help says how to select text natively).
    pub mouse: bool,
    /// How often the files are read (the ini's `poll_interval_ms`).
    pub idle_poll: Duration,
    last_engine_poll: Option<Instant>,
    /// Clickable rectangles of the last frame.
    pub hits: HitMap,
    clipboard: Clipboard,
    /// Last left click on a row (when, stream, line), to tell a double click.
    last_click: Option<(Instant, usize, usize)>,
    /// Stream whose rows a left-button drag is selecting.
    drag: Option<usize>,
    last_signature: Signature,
}

/// How often the configuration is saved while it changes, as in the GUI.
const SAVE_EVERY: Duration = Duration::from_secs(2);

/// Two clicks on the same row within this time are a double click.
const DOUBLE_CLICK: Duration = Duration::from_millis(400);
/// Rows moved by one wheel step, as in the GUI.
const WHEEL_ROWS: usize = 3;

impl App {
    pub fn new(tabs: Vec<Tab>, palette: Palette) -> Self {
        Self {
            tabs,
            active: 0,
            split: None,
            palette,
            prompt: None,
            time_range: None,
            picker: None,
            sessions: None,
            confirm_overwrite: None,
            settings: None,
            autosave: false,
            last_save: Instant::now(),
            save_error: None,
            message: None,
            show_help: false,
            help_top: 0,
            quit: false,
            mouse: true,
            idle_poll: Duration::from_millis(250),
            last_engine_poll: None,
            hits: HitMap::default(),
            clipboard: Clipboard::default(),
            last_click: None,
            drag: None,
            last_signature: Signature::default(),
        }
    }

    /// Streams on screen, the focused one first.
    pub fn visible_tabs(&self) -> Vec<usize> {
        match self.split {
            Some(s) => vec![self.active, s.other],
            None => vec![self.active],
        }
    }

    /// Polls every engine (hidden streams keep tailing), applies what the engines ask the
    /// view to do, and says whether the screen needs a redraw.
    pub fn tick(&mut self) -> bool {
        // The files are read every `idle_poll` (the ini's `poll_interval_ms`), and at
        // every tick while background work runs, whose results arrive through the poll.
        let due = self.busy()
            || self
                .last_engine_poll
                .is_none_or(|at| at.elapsed() >= self.idle_poll);
        if due {
            self.last_engine_poll = Some(Instant::now());
            for tab in &mut self.tabs {
                tab.engine.poll_updates();
            }
        }
        let listed = self.picker.as_mut().is_some_and(|p| p.pull());
        for i in self.visible_tabs() {
            let tab = &mut self.tabs[i];
            if tab.pending_first_hit {
                if tab.engine.active_match_count() > 0 {
                    tab.pending_first_hit = false;
                    // The engine may already have made the first hit current: show it
                    // rather than stepping past it.
                    let current = if tab.is_hex() {
                        tab.engine
                            .current_search_byte()
                            .map(|(off, _)| tab.engine.scroll_to_byte = Some(off))
                    } else {
                        tab.engine
                            .current_search_line()
                            .map(|line| tab.engine.scroll_to_line = Some(line))
                    };
                    if current.is_none() {
                        tab.engine.search_next(false);
                    }
                } else if tab.engine.scan_progress().is_none() {
                    tab.pending_first_hit = false;
                    self.message = Some(format!("Not found: {}", tab.engine.search_query));
                }
            }
            if let Some(result) = tab.engine.take_goto_time_result() {
                self.message = goto_target(tab, result, "that time");
            }
            if let Some(line) = tab.engine.pending_jump.take() {
                // The context view shows the line it was entered on.
                if let Some(row) = tab.engine.get_visible_row_of_line(line) {
                    tab.set_cursor(row);
                }
            }
            if let Some(line) = tab.engine.scroll_to_line.take() {
                if let Some(row) = tab.engine.get_visible_row_of_line(line) {
                    // A jump (search hit, go-to) moves the cursor onto the line.
                    tab.set_cursor(row);
                }
            }
            if let Some(offset) = tab.engine.scroll_to_byte.take() {
                if tab.is_hex() {
                    tab.set_cursor(offset / tab.hex_width.max(1));
                }
            }
            tab.seen_lines = tab.engine.total_lines();
        }
        if self.autosave && self.last_save.elapsed() >= SAVE_EVERY {
            self.save_config();
        }
        let sig = self.signature();
        let changed = sig != self.last_signature;
        self.last_signature = sig;
        changed || listed
    }

    /// Background work in flight: the loop then polls a little faster.
    pub fn busy(&self) -> bool {
        self.tabs.iter().any(|t| {
            t.engine.scan_progress().is_some()
                || t.engine.index_pending
                || t.engine.compressed.as_ref().is_some_and(|c| c.is_running())
        }) || self.picker.as_ref().is_some_and(|p| p.scan.is_some())
    }

    fn signature(&self) -> Signature {
        let pane = |i: usize| {
            let e = &self.tabs[i].engine;
            PaneSignature {
                tab: i,
                total: e.total_lines(),
                rows: e.visible_line_count(),
                file_size: e.file_size,
                filter_generation: e.filter_generation,
                search_generation: e.search_generation,
                current_hit: e.current_search_line(),
                scan: e
                    .scan_progress()
                    .map(|(k, p, _)| (k as u8, (p * 1000.0) as u16)),
                levels: e.cached_levels().len(),
                follow: e.follow_tail,
                collapse_pending: e.collapse_pending(),
                decompressed: e
                    .compressed
                    .as_ref()
                    .filter(|c| c.is_running())
                    .map(|c| (c.progress() * 1000.0) as u16),
                ansi: e.ansi_effective(),
                hex: self.tabs[i]
                    .is_hex()
                    .then(|| (e.search_byte_matches.len(), e.current_search_byte())),
            }
        };
        Signature {
            active: self.active,
            split: self.split,
            panes: self.visible_tabs().into_iter().map(pane).collect(),
            unseen_tabs: self
                .tabs
                .iter()
                .filter(|t| t.engine.total_lines() > t.seen_lines)
                .count(),
        }
    }

    /// Handles a key; returns true when the screen changed.
    pub fn on_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        if self.confirm_overwrite.is_some() {
            return self.on_confirm_key(key);
        }
        if self.sessions.is_some() {
            return self.on_sessions_key(key);
        }
        if self.picker.is_some() {
            return self.on_picker_key(key);
        }
        if self.time_range.is_some() {
            return self.on_time_range_key(key);
        }
        if self.prompt.is_some() {
            return self.on_prompt_key(key);
        }
        let Some(action) = keys::map_key(key) else {
            return false;
        };
        // The help dialog scrolls with the arrows and the page keys and closes on any
        // other key; only `?` / F1 reopen it.
        if self.show_help {
            match action {
                Action::LineUp => self.help_top = self.help_top.saturating_sub(1),
                Action::LineDown => self.help_top += 1,
                Action::PageUp => self.help_top = self.help_top.saturating_sub(10),
                Action::PageDown => self.help_top += 10,
                _ => {
                    self.show_help = false;
                    self.help_top = 0;
                    if action == Action::ToggleHelp {
                        return true;
                    }
                }
            }
            if self.show_help {
                return true;
            }
        }
        self.message = None;
        self.apply(action);
        true
    }

    fn on_prompt_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        let Some(prompt) = self.prompt.as_mut() else {
            return false;
        };
        match prompt.field.on_key(key) {
            FieldKey::Submit => {
                if let Some(prompt) = self.prompt.take() {
                    self.submit_prompt(prompt);
                }
            }
            FieldKey::Cancel => self.prompt = None,
            FieldKey::Edited => {}
            FieldKey::Other => return false,
        }
        true
    }

    /// Pasted text (bracketed paste) goes into the field being edited.
    pub fn on_paste(&mut self, text: &str) -> bool {
        if let Some(d) = self.sessions.as_mut() {
            d.field.insert(text);
        } else if let Some(p) = self.picker.as_mut() {
            p.filter.insert(text);
            p.filter_changed();
        } else if let Some(d) = self.time_range.as_mut() {
            d.focused().insert(text);
        } else if let Some(p) = self.prompt.as_mut() {
            p.field.insert(text);
        } else {
            return false;
        }
        true
    }

    /// Records the workspace in the configuration and writes it when it differs from
    /// what this run last loaded or wrote, as the GUI does: the open files in window
    /// order, each stream's state, bookmarks and notes (standard input left out); every
    /// key the terminal does not edit is written back as it was read. Without a
    /// configuration file (tests, benchmarks) nothing is written.
    pub fn save_config(&mut self) {
        self.last_save = Instant::now();
        let Some(settings) = self.settings.as_mut() else {
            return;
        };
        if settings.path.as_os_str().is_empty() {
            return;
        }
        for tab in &mut self.tabs {
            crate::workspace::save_changes(&mut tab.engine, &mut settings.config, false);
        }
        let order: Vec<PathBuf> = self.tabs.iter().map(|t| t.engine.path.clone()).collect();
        crate::workspace::snapshot(&order, self.tabs.iter().map(|t| &t.engine))
            .write_into(&mut settings.config);
        match settings.save() {
            Ok(_) => self.save_error = None,
            Err(e) => {
                let text = format!("Cannot save {}: {e}", settings.path.display());
                if self.save_error.as_ref() != Some(&text) {
                    self.message = Some(text.clone());
                    self.save_error = Some(text);
                }
            }
        }
    }

    /// `T`: the next theme, in the order of the GUI's list, saved as the ini's `theme`.
    fn cycle_theme(&mut self) {
        use crate::theme::CyberTheme;
        let next = match self.palette.theme {
            CyberTheme::Tron => CyberTheme::Matrix,
            CyberTheme::Matrix => CyberTheme::Blade,
            CyberTheme::Blade => CyberTheme::Light,
            CyberTheme::Light => CyberTheme::Tron,
        };
        self.palette.theme = next;
        if let Some(s) = self.settings.as_mut() {
            s.config.theme = next;
        }
        self.message = Some(format!("Theme: {}", next.name()));
    }

    /// The configuration of the run (the defaults when there is none, as in tests).
    fn settings_mut(&mut self) -> &mut crate::tui::workspace::Settings {
        self.settings.get_or_insert_with(Default::default)
    }

    fn open_sessions(&mut self) {
        let recent = self.settings_mut().config.recent_sessions.clone();
        self.sessions = Some(SessionDialog {
            recent,
            selected: 0,
            field: TextField::default(),
        });
    }

    fn on_sessions_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        use crossterm::event::KeyCode;
        let Some(d) = self.sessions.as_mut() else {
            return false;
        };
        match key.code {
            KeyCode::Up => d.selected = d.selected.saturating_sub(1),
            KeyCode::Down => d.selected = (d.selected + 1).min(d.recent.len().saturating_sub(1)),
            _ => match d.field.on_key(key) {
                FieldKey::Submit => self.submit_sessions(),
                FieldKey::Cancel => self.sessions = None,
                FieldKey::Edited => {}
                FieldKey::Other => return false,
            },
        }
        true
    }

    /// Loads the typed path, else the selected recent session.
    fn submit_sessions(&mut self) {
        let Some(d) = self.sessions.take() else {
            return;
        };
        let typed = d.field.text().trim();
        let file = if typed.is_empty() {
            match d.recent.get(d.selected) {
                Some(f) => f.clone(),
                None => {
                    self.message = Some("No recent session: type a path".into());
                    return;
                }
            }
        } else {
            absolute(typed)
        };
        self.load_session(&file);
    }

    /// Replaces the open streams with the ones of the session `file`, as the GUI does:
    /// their saved state applies, standard input stays, the session becomes the current
    /// one and goes first in the recent sessions.
    pub fn load_session(&mut self, file: &Path) {
        let settings = self.settings_mut();
        let plan = match crate::tui::workspace::session_plan(settings, file) {
            Ok(plan) => plan,
            Err(e) => {
                self.message = Some(e);
                return;
            }
        };
        let (engines, errors) = crate::tui::workspace::open_plan(settings, &plan);
        settings.config.current_session = Some(file.to_path_buf());
        settings.config.add_recent_session(file);
        let stdin = self.tabs.iter().position(|t| t.engine.is_stdin());
        let stdin = stdin.map(|i| self.tabs.remove(i));
        self.tabs = engines.into_iter().map(Tab::new).collect();
        self.tabs.extend(stdin);
        self.active = 0;
        self.split = None;
        let mut notes: Vec<String> = errors;
        notes.extend(crate::tui::workspace::missing_notice(&plan.missing));
        self.message = Some(if notes.is_empty() {
            format!(
                "Session {} loaded: {} streams",
                crate::session::Session::name_of(file),
                plan.paths.len()
            )
        } else {
            notes.join("  |  ")
        });
        if self.tabs.is_empty() {
            self.message = Some(format!(
                "Session {} opened nothing",
                crate::session::Session::name_of(file)
            ));
        }
        self.save_config();
    }

    /// Checks the path of "Save session as": the session suffix is added when missing,
    /// the configuration file is refused, an existing file asks first.
    fn submit_save_session(&mut self, typed: &str) {
        if typed.is_empty() {
            return;
        }
        let file = crate::session::Session::with_suffix(&absolute(typed));
        let config_path = self.settings_mut().path.clone();
        if crate::paths::paths_equal(&file, &config_path) {
            self.message = Some(format!(
                "{} is the configuration file: it cannot be a session",
                file.display()
            ));
            return;
        }
        if file.exists() {
            self.confirm_overwrite = Some(file);
            return;
        }
        self.save_session(&file);
    }

    fn on_confirm_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        use crossterm::event::{KeyCode, KeyEventKind};
        if key.kind == KeyEventKind::Release {
            return false;
        }
        match key.code {
            KeyCode::Enter | KeyCode::Char('y') => {
                if let Some(file) = self.confirm_overwrite.take() {
                    self.save_session(&file);
                }
            }
            KeyCode::Esc | KeyCode::Char('n') => self.confirm_overwrite = None,
            _ => return false,
        }
        true
    }

    /// Writes the open streams and their state to `file` (no dock layout: the terminal
    /// has none, and the GUI then opens the streams as tabs), makes it the current
    /// session and puts it first in the recent sessions.
    pub fn save_session(&mut self, file: &Path) {
        let mut streams: Vec<crate::session::StreamEntry> = Vec::new();
        for t in self.tabs.iter().filter(|t| !t.engine.is_stdin()) {
            if !streams
                .iter()
                .any(|s| crate::paths::paths_equal(&s.path, &t.engine.path))
            {
                streams.push(crate::workspace::stream_entry(&t.engine));
            }
        }
        let count = streams.len();
        let session = crate::session::Session {
            streams,
            dock_layout: None,
        };
        if let Err(e) = session.save_to(file) {
            self.message = Some(format!("Cannot save {}: {e}", file.display()));
            return;
        }
        let config = &mut self.settings_mut().config;
        config.current_session = Some(file.to_path_buf());
        config.add_recent_session(file);
        let stdin = if self.tabs.iter().any(|t| t.engine.is_stdin()) {
            " (standard input is not saved)"
        } else {
            ""
        };
        self.message = Some(format!(
            "Session saved to {}: {count} streams{stdin}",
            file.display()
        ));
        self.save_config();
    }

    fn on_picker_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        use crossterm::event::KeyCode;
        let Some(p) = self.picker.as_mut() else {
            return false;
        };
        let page = 10;
        match key.code {
            KeyCode::Up => p.move_by(-1),
            KeyCode::Down => p.move_by(1),
            KeyCode::PageUp => p.move_by(-page),
            KeyCode::PageDown => p.move_by(page),
            _ => match p.filter.on_key(key) {
                FieldKey::Submit => self.open_picked(),
                FieldKey::Cancel => self.close_picker(),
                FieldKey::Edited => p.filter_changed(),
                FieldKey::Other => return false,
            },
        }
        true
    }

    fn close_picker(&mut self) {
        if let Some(mut p) = self.picker.take() {
            p.close();
        }
    }

    /// Opens the selected entry of the picker; a refused one says why and stays.
    fn open_picked(&mut self) {
        let Some(p) = &self.picker else {
            return;
        };
        let Some(entry) = p.chosen() else {
            self.message = Some("No entry matches".into());
            return;
        };
        if let Some(r) = &entry.refusal {
            self.message = Some(format!("{}: {}", entry.name, refusal_text(r)));
            return;
        }
        let path = crate::compressed::entry_path(&p.archive, &entry.name);
        self.close_picker();
        self.open_file(&path);
    }

    /// Opens `path` in a new stream, as the GUI's Open does: an already open stream is
    /// shown instead, an archive with several entries opens the entry picker.
    pub fn open_file(&mut self, path: &Path) {
        use crate::workspace::{open_target, OpenOutcome};
        if let Some(i) = self
            .tabs
            .iter()
            .position(|t| crate::paths::paths_equal(&t.engine.path, path))
        {
            self.focus_tab(i);
            return;
        }
        let default_config;
        let config = match &self.settings {
            Some(s) => &s.config,
            None => {
                default_config = crate::config::FastTailConfig::default();
                &default_config
            }
        };
        let shown = path.display();
        match open_target(path, config, None) {
            OpenOutcome::Opened(engine) => {
                let mut engine = *engine;
                if let Some(s) = self.settings.as_mut() {
                    s.prepare(&mut engine);
                    if !engine.is_stdin() {
                        s.config.add_recent_file(&engine.path);
                    }
                }
                self.tabs.push(Tab::new(engine));
                self.focus_tab(self.tabs.len() - 1);
            }
            OpenOutcome::OpenEntry(entry) => self.open_file(&entry),
            OpenOutcome::Missing => self.message = Some(format!("{shown}: not found")),
            OpenOutcome::Failed { error, .. } => self.message = Some(format!("{shown}: {error:?}")),
            OpenOutcome::EmptyArchive(_) => {
                self.message = Some(format!("{shown}: the archive holds no file"))
            }
            OpenOutcome::ChooseEntries {
                archive,
                entries,
                partial,
            } => self.picker = Some(EntryPicker::new(archive, entries, partial)),
            OpenOutcome::ScanTar { archive, codec } => {
                let scan = crate::compressed::TarScan::start(
                    &archive,
                    codec,
                    crate::compressed::ScanLimits::default(),
                );
                self.picker = Some(EntryPicker::scanning(archive, scan));
            }
            OpenOutcome::ListFailed { error, .. } => {
                self.message = Some(format!("{shown}: {error}"))
            }
        }
    }

    fn open_time_range(&mut self) {
        let e = &self.tabs[self.active].engine;
        self.time_range = Some(TimeRangeDialog {
            from: TextField::new(&e.time_from_text),
            to: TextField::new(&e.time_to_text),
            on_to: false,
            invalid: false,
        });
    }

    fn on_time_range_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        use crossterm::event::KeyCode;
        let Some(d) = self.time_range.as_mut() else {
            return false;
        };
        match d.focused().on_key(key) {
            FieldKey::Submit => self.submit_time_range(),
            // The previous range stays.
            FieldKey::Cancel => self.time_range = None,
            FieldKey::Edited => d.invalid = false,
            FieldKey::Other => match key.code {
                KeyCode::Tab | KeyCode::BackTab | KeyCode::Up | KeyCode::Down => d.on_to = !d.on_to,
                _ => return false,
            },
        }
        true
    }

    /// Applies both sides when both can be read (an empty side is an open end); otherwise
    /// marks the dialog and changes nothing.
    fn submit_time_range(&mut self) {
        let Some(d) = self.time_range.as_mut() else {
            return;
        };
        let engine = &mut self.tabs[self.active].engine;
        let reference = engine.time_reference();
        let (from, to) = (d.from.text().trim(), d.to.text().trim());
        if !(side_readable(from, reference) && side_readable(to, reference)) {
            d.invalid = true;
            return;
        }
        let (from_ok, to_ok) = engine.apply_time_range_text(from, to);
        engine.time_range_error = !from_ok || !to_ok;
        self.time_range = None;
        self.tabs[self.active].top = 0;
    }

    fn submit_prompt(&mut self, prompt: Prompt) {
        let tab = &mut self.tabs[self.active];
        let text = prompt.field.text().trim().to_string();
        match prompt.kind {
            PromptKind::Search => {
                tab.engine.search_query = text.clone();
                tab.engine.update_search(&text);
                tab.pending_first_hit = !text.is_empty();
            }
            PromptKind::Include => {
                tab.engine.set_include_filter(&text);
                tab.top = 0;
            }
            PromptKind::Exclude => {
                tab.engine.set_exclude_filter(&text);
                tab.top = 0;
            }
            // A note bookmarks the line; an empty one removes the note, not the bookmark.
            PromptKind::Note(line) => tab.engine.set_bookmark_note(line, &text),
            PromptKind::OpenFile => {
                if !text.is_empty() {
                    self.open_file(&absolute(&text));
                }
            }
            PromptKind::SaveSession => self.submit_save_session(&text),
            PromptKind::Goto if tab.is_hex() => match hex::parse_offset(&text) {
                Some(offset) if tab.engine.file_size > 0 => {
                    let last = tab.engine.file_size as usize - 1;
                    tab.set_cursor(offset.min(last) / tab.hex_width.max(1));
                }
                _ => self.message = Some(format!("Cannot go to \"{text}\"")),
            },
            PromptKind::Goto => {
                let current = tab.cursor_line().unwrap_or(0);
                let target = tab.engine.resolve_goto(&text, current);
                if target.is_some_and(|t| t.waiting) {
                    // The jump happens when the timing ends (see `tick`).
                    self.message = Some("Timing the file to find that time...".into());
                } else {
                    self.message = goto_target(tab, target, &text);
                }
            }
        }
    }

    /// Searches the focused stream for `text`, as the search dialog does.
    pub fn search(&mut self, text: &str) {
        self.submit_prompt(Prompt {
            kind: PromptKind::Search,
            field: TextField::new(text),
        });
    }

    fn open_prompt(&mut self, kind: PromptKind) {
        let e = &self.tabs[self.active].engine;
        let text = match kind {
            PromptKind::Search => e.search_query.clone(),
            PromptKind::Include => e.include_filter().to_string(),
            PromptKind::Exclude => e.exclude_filter().to_string(),
            PromptKind::Note(line) => e.bookmark_note(line).unwrap_or_default().to_string(),
            PromptKind::Goto | PromptKind::OpenFile => String::new(),
            PromptKind::SaveSession => {
                let current = self
                    .settings
                    .as_ref()
                    .and_then(|s| s.config.current_session.clone());
                current.map_or_else(
                    || format!("session{}", crate::session::SESSION_SUFFIX),
                    |f| f.display().to_string(),
                )
            }
        };
        self.prompt = Some(Prompt {
            kind,
            field: TextField::new(&text),
        });
    }

    /// Puts stream `i` in the focused window; in a split, choosing the stream of the
    /// other window swaps the two.
    fn focus_tab(&mut self, i: usize) {
        if i >= self.tabs.len() {
            return;
        }
        if let Some(s) = self.split.as_mut() {
            if s.other == i {
                s.other = self.active;
            }
        }
        self.active = i;
    }

    pub fn apply(&mut self, action: Action) {
        let n_tabs = self.tabs.len();
        let row_action = matches!(
            action,
            Action::ToggleBookmark
                | Action::NextBookmark
                | Action::PrevBookmark
                | Action::EditNote
                | Action::ToggleContext
                | Action::SelectUp
                | Action::SelectDown
        );
        if row_action && self.tabs[self.active].is_hex() {
            self.message = Some("The HEX view shows bytes, not lines: h returns to them".into());
            return;
        }
        match action {
            Action::Quit => self.quit = true,
            // In a split `Tab` moves the focus between the two windows; otherwise it
            // shows the next stream.
            Action::NextTab | Action::PrevTab if self.split.is_some() => {
                if let Some(s) = self.split.as_mut() {
                    std::mem::swap(&mut self.active, &mut s.other);
                }
            }
            Action::NextTab => self.focus_tab((self.active + 1) % n_tabs),
            Action::PrevTab => self.focus_tab((self.active + n_tabs - 1) % n_tabs),
            Action::GotoTab(i) => self.focus_tab(i),
            Action::ToggleHelp => self.show_help = !self.show_help,
            Action::StartSearch => self.open_prompt(PromptKind::Search),
            Action::EditInclude => self.open_prompt(PromptKind::Include),
            Action::GoTo => self.open_prompt(PromptKind::Goto),
            Action::TimeRange => self.open_time_range(),
            Action::CycleTheme => self.cycle_theme(),
            Action::OpenFile => self.open_prompt(PromptKind::OpenFile),
            Action::OpenSession => self.open_sessions(),
            Action::SaveSession => self.open_prompt(PromptKind::SaveSession),
            Action::EditNote => match self.tabs[self.active].cursor_line() {
                Some(line) => self.open_prompt(PromptKind::Note(line)),
                None => self.message = Some("No row for a note".into()),
            },
            Action::EditExclude => self.open_prompt(PromptKind::Exclude),
            Action::CycleSplit => self.cycle_split(),
            Action::CopyOrQuit if !self.tabs[self.active].engine.has_selection() => {
                self.quit = true
            }
            Action::Copy | Action::CopyOrQuit => self.copy_selection(),
            _ => self.apply_to_tab(action),
        }
    }

    /// Copies the selection, or the cursor row when nothing is selected.
    fn copy_selection(&mut self) {
        let tab = &mut self.tabs[self.active];
        let text = if tab.engine.has_selection() {
            tab.engine.copy_selection_text()
        } else {
            // The cursor row as a one-row selection, so a collapsed group copies whole.
            let line = tab.cursor_line();
            line.and_then(|line| {
                tab.engine.select_row(line);
                let text = tab.engine.copy_selection_text();
                tab.engine.clear_selection();
                text
            })
        };
        let Some(text) = text else {
            self.message = Some("Nothing to copy".into());
            return;
        };
        let lines = text.lines().count();
        self.message = Some(match self.clipboard.copy(&text) {
            Ok(Copied::System) => format!("Copied {lines} lines to the clipboard"),
            Ok(Copied::Osc52) => format!("Sent {lines} lines to the terminal clipboard (OSC 52)"),
            Err(e) => format!("Copy failed: {e}"),
        });
    }

    // ----- Mouse -----

    /// Handles a mouse event against the last frame; returns true when the screen
    /// changed.
    pub fn on_mouse(&mut self, ev: MouseEvent) -> bool {
        match ev.kind {
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                if self.prompt.is_some()
                    || self.time_range.is_some()
                    || self.picker.is_some()
                    || self.sessions.is_some()
                    || self.confirm_overwrite.is_some()
                    || self.show_help
                {
                    return false;
                }
                let Some(tab) = mouse::window_at(&self.hits, ev.column, ev.row) else {
                    return false;
                };
                let t = &mut self.tabs[tab];
                let rows = t.row_count();
                if t.engine.follow_tail {
                    t.top = view::follow_top(rows, t.height);
                }
                // The wheel pauses follow, as in the GUI.
                t.engine.follow_tail = false;
                let top = if ev.kind == MouseEventKind::ScrollUp {
                    t.top.saturating_sub(WHEEL_ROWS)
                } else {
                    t.top + WHEEL_ROWS
                };
                t.top = view::clamp_top(top, t.height, rows);
                true
            }
            MouseEventKind::Down(MouseButton::Left) => self.on_click(ev),
            MouseEventKind::Drag(MouseButton::Left) => {
                let Some(tab) = self.drag.filter(|&t| !self.tabs[t].is_hex()) else {
                    return false;
                };
                match mouse::hit_test(&self.hits, ev.column, ev.row) {
                    Target::Row { tab: t, row } if t == tab => {
                        let e = &mut self.tabs[tab].engine;
                        if let (Some(line), Some(anchor)) =
                            (e.get_actual_line_idx(row), e.selection_anchor)
                        {
                            // Re-derive the range from the anchor, so dragging back
                            // shrinks it.
                            e.select_row(anchor);
                            e.extend_selection_to(line);
                            return true;
                        }
                        false
                    }
                    _ => false,
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.drag = None;
                false
            }
            _ => false,
        }
    }

    fn on_click(&mut self, ev: MouseEvent) -> bool {
        let target = mouse::hit_test(&self.hits, ev.column, ev.row);
        match target {
            Target::ListItem(i) if self.sessions.is_some() => {
                if let Some(d) = self.sessions.as_mut() {
                    d.selected = i;
                    d.field = TextField::default();
                }
                self.submit_sessions();
            }
            Target::ListItem(i) => {
                if let Some(p) = self.picker.as_mut() {
                    p.selected = i;
                }
                self.open_picked();
            }
            Target::DialogOk => {
                if let Some(file) = self.confirm_overwrite.take() {
                    self.save_session(&file);
                } else if self.sessions.is_some() {
                    self.submit_sessions();
                } else if self.picker.is_some() {
                    self.open_picked();
                } else if self.time_range.is_some() {
                    self.submit_time_range();
                } else if let Some(prompt) = self.prompt.take() {
                    self.submit_prompt(prompt);
                }
                self.show_help = false;
            }
            Target::DialogCancel | Target::OutsideDialog => {
                self.prompt = None;
                self.time_range = None;
                self.sessions = None;
                self.confirm_overwrite = None;
                self.close_picker();
                self.show_help = false;
            }
            Target::DialogBody | Target::Nothing => return false,
            Target::TabTitle(i) | Target::WindowTitle(i) | Target::Window(i) => self.focus_tab(i),
            Target::Row { tab, row } if self.tabs[tab].is_hex() => {
                // A HEX row takes the cursor; there is nothing to select or bookmark.
                self.focus_tab(tab);
                let t = &mut self.tabs[tab];
                t.engine.follow_tail = false;
                t.cursor = row;
            }
            Target::Row { tab, row } => {
                self.focus_tab(tab);
                self.drag = Some(tab);
                let e = &mut self.tabs[tab].engine;
                let Some(line) = e.get_actual_line_idx(row) else {
                    return true;
                };
                let now = Instant::now();
                let double = self.last_click.is_some_and(|(at, t, l)| {
                    t == tab && l == line && now.duration_since(at) < DOUBLE_CLICK
                });
                if ev.modifiers.contains(KeyModifiers::SHIFT) && e.selection_anchor.is_some() {
                    e.extend_selection_to(line);
                } else {
                    e.select_row(line);
                }
                // The clicked row takes the cursor, without scrolling the window.
                let t = &mut self.tabs[tab];
                t.engine.follow_tail = false;
                t.cursor = row;
                let e = &mut t.engine;
                if double {
                    e.toggle_bookmark(line);
                    self.last_click = None;
                } else {
                    self.last_click = Some((now, tab, line));
                }
            }
        }
        true
    }

    fn cycle_split(&mut self) {
        if self.tabs.len() < 2 {
            self.message = Some("Split needs two files".into());
            return;
        }
        let other = (self.active + 1) % self.tabs.len();
        self.split = match self.split.map(|s| s.dir) {
            None => Some(Split {
                dir: SplitDir::SideBySide,
                other,
            }),
            Some(SplitDir::SideBySide) => self.split.map(|s| Split {
                dir: SplitDir::Stacked,
                ..s
            }),
            Some(SplitDir::Stacked) => None,
        };
    }

    fn apply_to_tab(&mut self, action: Action) {
        let tab = &mut self.tabs[self.active];
        let page = tab.height.max(1);
        let rows = tab.row_count();
        // In follow mode the window sits on the last page whatever `top` says.
        if tab.engine.follow_tail {
            tab.top = view::follow_top(rows, page);
        }
        tab.pin_cursor();
        let cursor = tab.cursor;
        match action {
            Action::ToggleFollow => {
                tab.engine.follow_tail = !tab.engine.follow_tail;
                tab.pin_cursor();
            }
            Action::Bottom => {
                tab.engine.follow_tail = true;
                tab.pin_cursor();
            }
            Action::Top => tab.set_cursor(0),
            Action::LineUp => tab.set_cursor(cursor.saturating_sub(1)),
            Action::LineDown => tab.set_cursor(cursor + 1),
            Action::PageUp => tab.set_cursor(cursor.saturating_sub(page)),
            Action::PageDown => tab.set_cursor(cursor + page),
            Action::SelectUp | Action::SelectDown => {
                if tab.engine.selection_anchor.is_none() {
                    if let Some(line) = tab.cursor_line() {
                        tab.engine.select_row(line);
                    }
                }
                let to = if action == Action::SelectUp {
                    cursor.saturating_sub(1)
                } else {
                    cursor + 1
                };
                tab.set_cursor(to);
                if let Some(line) = tab.cursor_line() {
                    tab.engine.extend_selection_to(line);
                }
            }
            Action::ScrollLeft => tab.hscroll = tab.hscroll.saturating_sub(HSCROLL_STEP),
            Action::ScrollRight => tab.hscroll += HSCROLL_STEP,
            Action::ScrollHome => tab.hscroll = 0,
            Action::ToggleBookmark => match tab.cursor_line() {
                Some(line) => tab.engine.toggle_bookmark(line),
                None => self.message = Some("No row to bookmark".into()),
            },
            Action::NextBookmark | Action::PrevBookmark => {
                let from = tab.cursor_line().unwrap_or(0);
                let forward = action == Action::NextBookmark;
                match tab.engine.bookmark_from(from, forward) {
                    Some((line, wrapped)) => {
                        if let Some(row) = tab.engine.get_visible_row_of_line(line) {
                            tab.set_cursor(row);
                        }
                        if wrapped {
                            self.message = Some(
                                if forward {
                                    "Bookmarks: back to the first"
                                } else {
                                    "Bookmarks: back to the last"
                                }
                                .into(),
                            );
                        }
                    }
                    None => self.message = Some("No bookmark visible".into()),
                }
            }
            Action::SearchNext => {
                if tab.engine.search_next(false).is_none() {
                    self.message = Some("No search hits".into());
                }
            }
            Action::SearchPrev => {
                if tab.engine.search_prev(false).is_none() {
                    self.message = Some("No search hits".into());
                }
            }
            Action::ToggleContext => toggle_context(tab, &mut self.message),
            Action::ToggleHex => tab.toggle_hex(),
            Action::CycleAnsi => {
                let next = match tab.engine.ansi_mode {
                    AnsiMode::Auto => AnsiMode::Render,
                    AnsiMode::Render => AnsiMode::Strip,
                    AnsiMode::Strip => AnsiMode::Raw,
                    AnsiMode::Raw => AnsiMode::Auto,
                };
                tab.engine.set_ansi_mode(next);
                self.message = Some(match next {
                    AnsiMode::Auto => {
                        format!("ANSI: auto (now {})", tab.engine.ansi_effective().name())
                    }
                    mode => format!("ANSI: {}", mode.name()),
                });
            }
            // In the context view, Esc returns to the filtered rows first.
            Action::ClearSearch if tab.engine.context_line().is_some() => {
                toggle_context(tab, &mut self.message)
            }
            Action::ClearSearch => {
                tab.engine.search_query.clear();
                tab.engine.update_search("");
                tab.engine.clear_selection();
                tab.pending_first_hit = false;
            }
            Action::CycleCollapse => {
                let next = match tab.engine.collapse_mode() {
                    CollapseMode::Off => CollapseMode::Exact,
                    CollapseMode::Exact => CollapseMode::Numbers,
                    CollapseMode::Numbers => CollapseMode::Off,
                };
                tab.engine.set_collapse_mode(next);
                self.message = Some(format!("Collapse: {}", next.name()));
            }
            Action::CycleLevel => {
                let next = match tab.engine.min_level {
                    LogLevel::Unknown => LogLevel::Debug,
                    LogLevel::Trace | LogLevel::Debug => LogLevel::Info,
                    LogLevel::Info => LogLevel::Warn,
                    LogLevel::Warn => LogLevel::Error,
                    LogLevel::Error | LogLevel::Fatal => LogLevel::Unknown,
                };
                tab.engine.set_min_level(next);
                tab.top = 0;
                self.message = Some(match next {
                    LogLevel::Unknown => "Level filter: off".into(),
                    l => format!("Level filter: {} and above", l.name()),
                });
            }
            _ => {}
        }
    }

    // ----- Drawing -----

    pub fn draw(&mut self, frame: &mut Frame) {
        // The hit map is rebuilt with every frame: a click is tested against what the
        // user saw.
        self.hits = HitMap::default();
        // The theme's background behind everything (the terminal's own at 16 colours).
        frame.render_widget(Block::default().style(self.palette.screen()), frame.area());
        let strip = if self.tabs.len() > 1 { 1 } else { 0 };
        let [tabs_area, main_area, status_area] = Layout::vertical([
            Constraint::Length(strip),
            Constraint::Min(3),
            Constraint::Length(3),
        ])
        .areas(frame.area());
        if strip > 0 {
            self.draw_tabs(frame, tabs_area);
        }
        match self.split {
            None => self.draw_stream(frame, main_area, self.active, true),
            Some(s) => {
                let layout = match s.dir {
                    SplitDir::SideBySide => Layout::horizontal([Constraint::Fill(1); 2]),
                    SplitDir::Stacked => Layout::vertical([Constraint::Fill(1); 2]),
                };
                let [first, second] = layout.areas(main_area);
                // The windows keep their places when the focus moves: the lower-numbered
                // stream is on the left (or on top).
                let (a, b) = if self.active < s.other {
                    (self.active, s.other)
                } else {
                    (s.other, self.active)
                };
                self.draw_stream(frame, first, a, a == self.active);
                self.draw_stream(frame, second, b, b == self.active);
            }
        }
        self.draw_status(frame, status_area);
        if self.show_help {
            self.draw_help(frame, main_area);
        }
        if self.prompt.is_some() {
            self.draw_prompt(frame, main_area);
        }
        if self.time_range.is_some() {
            self.draw_time_range(frame, main_area);
        }
        if self.picker.is_some() {
            self.draw_picker(frame, main_area);
        }
        if self.sessions.is_some() {
            self.draw_sessions(frame, main_area);
        }
        if self.confirm_overwrite.is_some() {
            self.draw_confirm(frame, main_area);
        }
    }

    /// The strip of stream titles, drawn span by span so each title's cells are known
    /// for the mouse.
    fn draw_tabs(&mut self, frame: &mut Frame, area: Rect) {
        let shown = self.visible_tabs();
        let normal = Style::default().fg(self.palette.dim());
        let selected = Style::default()
            .fg(self.palette.accent())
            .add_modifier(Modifier::BOLD | Modifier::REVERSED);
        let mut spans = Vec::new();
        let mut x = area.x;
        for (i, t) in self.tabs.iter().enumerate() {
            let unseen = !shown.contains(&i) && t.engine.total_lines() > t.seen_lines;
            let mark = if unseen { " +" } else { "" };
            let text = format!(" {}:{}{} ", i + 1, t.title, mark);
            let w = (text.chars().count() as u16).min(area.right().saturating_sub(x));
            if w > 0 {
                self.hits.tab_titles.push((Rect::new(x, area.y, w, 1), i));
            }
            x = x.saturating_add(w + 1);
            let style = if i == self.active { selected } else { normal };
            spans.push(Span::styled(text, style));
            spans.push(Span::styled("|", normal));
        }
        spans.pop();
        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }

    /// One stream window: title and follow state in the top border, counts and progress
    /// bottom left, filters and search bottom right, the rows inside.
    fn draw_stream(&mut self, frame: &mut Frame, area: Rect, idx: usize, focused: bool) {
        let palette = self.palette;
        let chrome = if focused {
            Chrome::Focused
        } else {
            Chrome::Plain
        };
        let tab = &mut self.tabs[idx];
        let e = &tab.engine;
        let follow = if e.follow_tail {
            Span::styled(
                " FOLLOW ",
                Style::default().fg(Color::Black).bg(palette.accent()),
            )
        } else {
            Span::styled(" PAUSED ", Style::default().fg(palette.dim()))
        };
        let title_style = if focused {
            Style::default()
                .fg(palette.accent())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(palette.dim())
        };
        let block = Block::bordered()
            .style(palette.window())
            .border_set(palette.border_set(chrome))
            .border_style(palette.border_style(chrome))
            .title_top(Line::from(vec![
                Span::styled(format!(" [#{}] {} ", idx + 1, tab.title), title_style),
                follow,
                Span::raw(" "),
            ]))
            .title_bottom(
                Line::from(format!(" {} ", counts_text(e, tab.is_hex(), tab.hex_width)))
                    .style(title_style),
            )
            .title_bottom(
                Line::from(format!(" {} ", view_state_text(e, tab.is_hex())))
                    .style(title_style)
                    .right_aligned(),
            );
        let mut inner = block.inner(area);
        frame.render_widget(block, area);
        if tab.engine.context_line().is_some() && inner.height > 1 {
            // The context view: every line, the filters suspended until Ctrl+K or Esc.
            let banner = Rect { height: 1, ..inner };
            frame.render_widget(
                Paragraph::new(Line::styled(
                    " In context: filters suspended. Ctrl+K or Esc returns",
                    Style::default().fg(Color::Black).bg(palette.accent()),
                )),
                banner,
            );
            inner.y += 1;
            inner.height -= 1;
        }
        let (lines, row_count) = if tab.is_hex() {
            hex_rows(tab, &palette, inner.width as usize, inner.height as usize)
        } else {
            stream_rows(tab, &palette, inner.height as usize)
        };
        self.hits.windows.push(WindowHit {
            tab: idx,
            outer: area,
            rows: inner,
            first_row: tab.top,
            row_count,
        });
        frame.render_widget(Paragraph::new(lines), inner);
    }

    /// The bordered bar at the bottom: a message, or the main keys.
    fn draw_status(&self, frame: &mut Frame, area: Rect) {
        let chrome = Chrome::Plain;
        let block = Block::bordered()
            .style(self.palette.window())
            .border_set(self.palette.border_set(chrome))
            .border_style(self.palette.border_style(chrome))
            .title_top(
                Line::from(" FastTail TUI ").style(
                    Style::default()
                        .fg(self.palette.accent())
                        .add_modifier(Modifier::BOLD),
                ),
            );
        let tab = &self.tabs[self.active];
        let note = tab
            .cursor_line()
            .and_then(|l| tab.engine.bookmark_note(l))
            .map(|n| n.chars().take(NOTE_PREVIEW_CHARS).collect::<String>());
        let line = match (&self.message, note) {
            (Some(m), _) => Line::styled(m.clone(), Style::default().fg(self.palette.accent())),
            // The note of the cursor row's bookmark, when there is one.
            (None, Some(n)) => Line::styled(format!("* {n}"), Style::default().fg(self.palette.accent())),
            (None, None) => Line::styled(
                "? help  q quit  Space follow  / search  n/N next  i/x filter  b mark  ]/[ marks  m note  h hex  y copy",
                Style::default().fg(self.palette.dim()),
            ),
        };
        frame.render_widget(Paragraph::new(line).block(block), area);
    }

    /// A centred bordered dialog over `area`, with what was under it cleared and its
    /// buttons (`[ OK ]`, and `[ Cancel ]` when `cancel`) on the last inner row. Returns
    /// the inner area above the buttons.
    fn dialog(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        size: (u16, u16),
        title: &str,
        cancel: bool,
    ) -> Rect {
        let (x, y, w, h) = view::centered(area.width, area.height, size.0, size.1);
        let rect = Rect::new(area.x + x, area.y + y, w, h);
        let block = Block::bordered()
            .style(self.palette.dialog())
            .border_set(self.palette.border_set(Chrome::Dialog))
            .border_style(self.palette.border_style(Chrome::Dialog))
            .title_top(
                Line::from(format!(" {title} ")).style(
                    Style::default()
                        .fg(self.palette.accent())
                        .add_modifier(Modifier::BOLD),
                ),
            );
        let inner = block.inner(rect);
        frame.render_widget(Clear, rect);
        frame.render_widget(block, rect);
        cast_shadow(frame.buffer_mut(), rect, self.palette.shadow());
        // Buttons, right-aligned on the last inner row.
        let labels: &[&str] = if cancel {
            &["[ OK ]", "[ Cancel ]"]
        } else {
            &["[ OK ]"]
        };
        let row = inner.bottom().saturating_sub(1);
        // Two columns between buttons, one after the last.
        let total: u16 = labels.iter().map(|l| l.len() as u16 + 2).sum::<u16>() - 1;
        let mut bx = inner.right().saturating_sub(total).max(inner.x);
        let mut rects = Vec::new();
        for label in labels {
            let r = Rect::new(bx, row, label.len() as u16, 1).intersection(inner);
            frame.render_widget(
                Paragraph::new(*label)
                    .style(Style::default().fg(Color::Black).bg(self.palette.accent())),
                r,
            );
            rects.push(r);
            bx += label.len() as u16 + 2;
        }
        self.hits.dialog = Some(DialogHit {
            outer: rect,
            ok: rects[0],
            cancel: rects.get(1).copied(),
        });
        Rect {
            height: inner.height.saturating_sub(1),
            ..inner
        }
    }

    fn draw_prompt(&mut self, frame: &mut Frame, area: Rect) {
        let Some(kind) = self.prompt.as_ref().map(|p| p.kind) else {
            return;
        };
        let title = match kind {
            PromptKind::Search => "Search",
            PromptKind::Include => "Include filter",
            PromptKind::Exclude => "Exclude filter",
            PromptKind::Note(_) => "Bookmark note",
            PromptKind::Goto if self.tabs[self.active].is_hex() => {
                "Go to byte offset (decimal or 0x hex)"
            }
            PromptKind::Goto => "Go to line (N, +N, -N) or time (14:02)",
            PromptKind::OpenFile => "Open file, pattern (*.log) or archive entry",
            PromptKind::SaveSession => "Save session as",
        };
        let inner = self.dialog(frame, area, (64, 5), title, true);
        let Some(p) = &self.prompt else {
            return;
        };
        let hint = Line::styled(
            "Enter confirm   Esc cancel   Ctrl+U clear",
            Style::default().fg(self.palette.dim()),
        );
        let (shown, x) = p.field.view(inner.width.saturating_sub(2) as usize);
        frame.render_widget(
            Paragraph::new(vec![
                Line::raw(format!("> {}", view::sanitize(&shown))),
                hint,
            ]),
            inner,
        );
        frame.set_cursor_position((inner.x + 2 + x as u16, inner.y));
    }

    fn draw_sessions(&mut self, frame: &mut Frame, area: Rect) {
        let rows = self.sessions.as_ref().map_or(0, |d| d.recent.len()) as u16;
        let inner = self.dialog(frame, area, (72, rows.max(1) + 7), "Open session", true);
        let palette = self.palette;
        let Some(d) = &self.sessions else {
            return;
        };
        let dim = Style::default().fg(palette.dim());
        let (shown, x) = d.field.view(inner.width.saturating_sub(6) as usize);
        let mut lines = vec![
            Line::from(vec![
                Span::styled("Path ", dim),
                Span::raw(view::sanitize(&shown)),
            ]),
            Line::styled("Recent sessions (Up/Down, Enter with an empty path):", dim),
        ];
        frame.set_cursor_position((inner.x + 5 + x as u16, inner.y));
        if d.recent.is_empty() {
            lines.push(Line::styled("  (none)", dim));
        }
        for (i, f) in d.recent.iter().enumerate() {
            let name = crate::session::Session::name_of(f);
            let text = view::sanitize(&format!("  {name}  {}", f.display()));
            let style = if i == d.selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            let y = inner.y + 2 + i as u16;
            if y < inner.bottom() {
                self.hits
                    .list_items
                    .push((Rect::new(inner.x, y, inner.width, 1), i));
            }
            lines.push(Line::styled(text, style));
        }
        frame.render_widget(Paragraph::new(lines), inner);
    }

    fn draw_confirm(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.dialog(frame, area, (64, 6), "Overwrite?", true);
        let Some(file) = &self.confirm_overwrite else {
            return;
        };
        let text = vec![
            Line::raw(view::sanitize(&format!("{} exists.", file.display()))),
            Line::raw("Replace it with the open streams? (Enter / Esc)"),
        ];
        frame.render_widget(Paragraph::new(text), inner);
    }

    fn draw_picker(&mut self, frame: &mut Frame, area: Rect) {
        let Some(title) = self.picker.as_ref().map(|p| {
            let name = p
                .archive
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            format!("Open an entry of {name}")
        }) else {
            return;
        };
        let height = area.height.saturating_sub(2).clamp(8, 24);
        let inner = self.dialog(frame, area, (72, height), &title, true);
        let palette = self.palette;
        let Some(p) = self.picker.as_mut() else {
            return;
        };
        let dim = Style::default().fg(palette.dim());
        // Filter line, the list, then a status line.
        let (shown_filter, x) = p.filter.view(inner.width.saturating_sub(8) as usize);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("Filter ", dim),
                Span::raw(view::sanitize(&shown_filter)),
            ])),
            Rect { height: 1, ..inner },
        );
        frame.set_cursor_position((inner.x + 7 + x as u16, inner.y));
        let list = Rect {
            y: inner.y + 1,
            height: inner.height.saturating_sub(2),
            ..inner
        };
        let shown = p.shown();
        let rows = list.height as usize;
        if p.selected < p.top {
            p.top = p.selected;
        } else if rows > 0 && p.selected >= p.top + rows {
            p.top = p.selected + 1 - rows;
        }
        let mut lines = Vec::with_capacity(rows);
        for (k, &i) in shown.iter().enumerate().skip(p.top).take(rows) {
            let e = &p.entries[i];
            let size = crate::tui::picker::human_size(e.size);
            let (text, style) = match &e.refusal {
                Some(r) => (format!("{}  ({})", e.name, refusal_text(r)), dim),
                None => (format!("{}  {size}", e.name), Style::default()),
            };
            let style = if k == p.selected {
                style.add_modifier(Modifier::REVERSED)
            } else {
                style
            };
            let y = list.y + (k - p.top) as u16;
            self.hits
                .list_items
                .push((Rect::new(list.x, y, list.width, 1), k));
            lines.push(Line::styled(view::sanitize(&text), style));
        }
        frame.render_widget(Paragraph::new(lines), list);
        let status = match (&p.scan, &p.partial) {
            (Some(scan), _) => format!(
                "{} entries, reading the archive {:.0}%",
                p.entries.len(),
                scan.progress() * 100.0
            ),
            (None, Some(note)) => format!("{} of {} entries. {note}", shown.len(), p.entries.len()),
            (None, None) => format!("{} of {} entries", shown.len(), p.entries.len()),
        };
        frame.render_widget(
            Paragraph::new(Line::styled(status, dim)),
            Rect {
                y: list.bottom(),
                height: 1,
                ..inner
            },
        );
    }

    fn draw_time_range(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.dialog(frame, area, (64, 9), "Time range", true);
        let palette = self.palette;
        let Some(d) = &self.time_range else {
            return;
        };
        let reference = self.tabs[self.active].engine.time_reference();
        const LABEL: u16 = 7;
        let width = inner.width.saturating_sub(LABEL) as usize;
        let mut lines = Vec::with_capacity(6);
        let mut cursor = (inner.x, inner.y);
        for (row, (label, field, focused)) in [("From", &d.from, !d.on_to), ("To", &d.to, d.on_to)]
            .into_iter()
            .enumerate()
        {
            let (shown, x) = field.view(width);
            let bad = !side_readable(field.text().trim(), reference);
            let label_style = if focused {
                Style::default()
                    .fg(palette.accent())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(palette.dim())
            };
            let field_style = if bad {
                Style::default().fg(palette.level_color(LogLevel::Error))
            } else {
                Style::default()
            }
            .add_modifier(Modifier::UNDERLINED);
            let pad = width.saturating_sub(unicode_width::UnicodeWidthStr::width(shown.as_str()));
            lines.push(Line::from(vec![
                Span::styled(format!("{label:<w$}", w = LABEL as usize), label_style),
                Span::styled(
                    format!("{}{}", view::sanitize(&shown), " ".repeat(pad)),
                    field_style,
                ),
            ]));
            if focused {
                cursor = (inner.x + LABEL + x as u16, inner.y + row as u16);
            }
        }
        let dim = Style::default().fg(palette.dim());
        lines.push(Line::raw(""));
        lines.push(Line::styled("2026-09-18 14:02:05, 14:02, -15m, now", dim));
        lines.push(Line::styled(
            "An empty side is open. Tab switches side.",
            dim,
        ));
        if d.invalid {
            lines.push(Line::styled(
                "A side cannot be read as a time",
                Style::default().fg(palette.level_color(LogLevel::Error)),
            ));
        }
        frame.render_widget(Paragraph::new(lines), inner);
        frame.set_cursor_position(cursor);
    }

    fn draw_help(&mut self, frame: &mut Frame, area: Rect) {
        let mouse_hint = if self.mouse {
            "Mouse on: SHIFT + drag selects text natively (--no-mouse)"
        } else {
            "Mouse off (--no-mouse): the terminal selects text"
        };
        let text = [
            "Up/Down j/k      move the cursor one row",
            "PgUp/PgDn        move it one page (also Ctrl+B / Ctrl+F)",
            "Shift+Up/Down    extend the selection from the cursor",
            "Home g / End G   first / last row (the last one follows)",
            "Left/Right 0     scroll sideways one cell / back to column 0",
            "Space            toggle follow",
            "/  n  N  Esc     search, next, previous, clear (also F3 / Shift+F3)",
            "i  x             include / exclude filter",
            "l                cycle the minimum level",
            "c                cycle collapse: off, exact, numbers",
            "s                split: side by side, stacked, off",
            "Tab  Alt+1..9    next window or file / file N (Shift+Tab back)",
            "b  Ctrl+F2       bookmark the cursor row, on or off",
            "] F2 / [ Shift+F2  next / previous bookmark (wraps)",
            "m                note of the cursor row's bookmark",
            "Ctrl+K           cursor row in context (filters off), again back",
            "Ctrl+G  :        go to a line (N, +N, -N) or a time (14:02)",
            "o                open a file, pattern or archive entry",
            "O  S             open a session / save the streams as a session",
            "t                time range: from / to (14:02, -15m, now)",
            "T                next theme: Tron, Matrix, Blade, Light",
            "a                ANSI colours: auto, render, strip, raw (^[)",
            "h                HEX view of the bytes (go to: 1024, 0x400), again back",
            "y  Ctrl+C        copy the selection or the cursor row",
            "                 (Ctrl+C quits when nothing is selected)",
            "?  F1            this help",
            "q                quit",
            "",
            "Wheel scrolls the window under the pointer; click focuses",
            "and selects a row, SHIFT + click or drag a range, double",
            "click toggles a bookmark.",
            mouse_hint,
        ];
        // Taller than the screen: Up / Down / PgUp / PgDn scroll it.
        let height = (text.len() as u16 + 3).min(area.height);
        let rows = height.saturating_sub(3) as usize;
        self.help_top = self.help_top.min(text.len().saturating_sub(rows));
        let title = if rows < text.len() {
            "Keys - Up/Down scroll, any other key closes"
        } else {
            "Keys - any key closes"
        };
        let inner = self.dialog(frame, area, (78, height), title, false);
        frame.render_widget(
            Paragraph::new(
                text.iter()
                    .skip(self.help_top)
                    .map(|l| Line::from(*l))
                    .collect::<Vec<_>>(),
            ),
            inner,
        );
    }
}

/// Darkens the cells a dialog at `rect` shades: two columns on its right and one row
/// below it, offset by one, as a light from the top left would. The characters stay, so
/// what is behind still shows through the shadow.
fn cast_shadow(buf: &mut ratatui::buffer::Buffer, rect: Rect, style: Style) {
    let screen = buf.area;
    let right = Rect::new(rect.right(), rect.y + 1, 2, rect.height);
    let below = Rect::new(rect.x + 2, rect.bottom(), rect.width, 1);
    for area in [right, below] {
        let area = area.intersection(screen);
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                buf[(x, y)].set_style(style);
            }
        }
    }
}

/// Bottom-left of a window: rows and lines, and the background work in progress.
fn counts_text(e: &TailEngine, hex: bool, hex_width: usize) -> String {
    let mut s = if hex {
        format!(
            "{} bytes, {} rows of {hex_width}",
            group_digits(e.file_size as usize),
            group_digits(e.total_hex_rows(hex_width))
        )
    } else {
        format!(
            "{}/{} lines",
            group_digits(e.visible_line_count()),
            group_digits(e.total_lines())
        )
    };
    if let Some((kind, p, hits)) = e.scan_progress() {
        let name = match kind {
            ScanKind::Index => "indexing",
            ScanKind::Filter => "filtering",
            ScanKind::Search => "searching",
            ScanKind::Levels => "levels",
            ScanKind::Timestamps => "timestamps",
            ScanKind::AutoBookmarks => "bookmarks",
            ScanKind::Collapse => "collapsing",
        };
        s.push_str(&format!(" - {name} {:.0}%", p * 100.0));
        if kind == ScanKind::Search {
            s.push_str(&format!(" ({hits} hits)"));
        }
    }
    if let Some(c) = e.compressed.as_ref().filter(|c| c.is_running()) {
        s.push_str(&format!(" - decompressing {:.0}%", c.progress() * 100.0));
    }
    s
}

/// Bottom-right of a window: filters, level, collapse and the search position.
fn view_state_text(e: &TailEngine, hex: bool) -> String {
    if hex {
        // Filters, level and collapse do not apply to the bytes.
        let query = e.search_query.trim();
        if query.is_empty() {
            return "HEX".into();
        }
        let current = e
            .current_match_idx
            .map_or("-".to_string(), |i| (i + 1).to_string());
        return format!("HEX  /{query} {current}/{}", e.search_byte_matches.len());
    }
    let mut parts = Vec::new();
    let inc = e.include_filter();
    let exc = e.exclude_filter();
    if !inc.is_empty() {
        parts.push(format!("+{inc}"));
    }
    if !exc.is_empty() {
        parts.push(format!("-{exc}"));
    }
    if e.min_level != LogLevel::Unknown {
        parts.push(format!(">={}", e.min_level.short()));
    }
    let (from, to) = (e.time_from_text.trim(), e.time_to_text.trim());
    if !from.is_empty() || !to.is_empty() {
        parts.push(format!("time:{from}..{to}"));
    }
    if e.ansi_mode != AnsiMode::Auto {
        parts.push(format!("ansi:{}", e.ansi_mode.name()));
    }
    if e.collapse_mode() != CollapseMode::Off {
        parts.push(format!("collapse:{}", e.collapse_mode().name()));
    }
    if !e.search_query.trim().is_empty() {
        let current = e
            .current_search_line()
            .and_then(|l| e.search_matches.binary_search(&l).ok())
            .map_or("-".to_string(), |i| (i + 1).to_string());
        parts.push(format!(
            "/{} {}/{}",
            e.search_query.trim(),
            current,
            e.search_total()
        ));
    }
    if parts.is_empty() {
        "no filter".into()
    } else {
        parts.join("  ")
    }
}

/// Moves the cursor to a go-to target; the status message when there is one to show.
fn goto_target(
    tab: &mut Tab,
    target: Option<crate::tail_engine::GotoTarget>,
    typed: &str,
) -> Option<String> {
    let Some(t) = target else {
        return Some(format!("Cannot go to \"{typed}\""));
    };
    // The exact line: a collapsed group hiding it opens.
    tab.engine.reveal_line(t.line);
    if let Some(row) = tab.engine.get_visible_row_of_line(t.line) {
        tab.set_cursor(row);
    }
    t.hidden.then(|| {
        format!(
            "Line {} is hidden by the filters: showing {}",
            t.requested + 1,
            t.line + 1
        )
    })
}

/// Enters the context view on the cursor row, or leaves it with the cursor back on the
/// line it was entered on.
fn toggle_context(tab: &mut Tab, message: &mut Option<String>) {
    if let Some(line) = tab.engine.context_line() {
        tab.engine.leave_context();
        if let Some(row) = tab.engine.get_visible_row_of_line(line) {
            tab.set_cursor(row);
        }
        tab.engine.follow_tail = false;
        return;
    }
    let Some(line) = tab.cursor_line() else {
        return;
    };
    if !tab.engine.enter_context(line) {
        *message = Some("Show in context needs an active filter".into());
    }
}

/// The rows of a window `height` rows tall: only these are read from the engine.
/// Also returns how many rows were drawn, for the mouse.
fn stream_rows(tab: &mut Tab, palette: &Palette, height: usize) -> (Vec<Line<'static>>, usize) {
    tab.height = height;
    tab.pin_cursor();
    let engine = &tab.engine;
    let rows = engine.visible_line_count();
    if engine.follow_tail {
        tab.top = view::follow_top(rows, height);
    }
    tab.top = view::clamp_top(tab.top, height, rows);
    let range = view::visible_range(tab.top, height, rows);
    let gutter = view::gutter_width(engine.total_lines());
    let query = engine.search_query.trim().to_string();
    let has_search = !query.is_empty() && engine.active_match_count() > 0;
    let mut lines = Vec::with_capacity(range.len());
    for row in range {
        let Some(line_idx) = engine.get_actual_line_idx(row) else {
            break;
        };
        let line = render_row(
            engine,
            palette,
            row,
            line_idx,
            gutter,
            &query,
            has_search,
            tab.hscroll,
        );
        lines.push(if row == tab.cursor {
            line.patch_style(Style::default().add_modifier(Modifier::REVERSED))
        } else {
            line
        });
    }
    let drawn = lines.len();
    if lines.is_empty() {
        let text = if engine.total_lines() == 0 {
            "(empty file, or still loading)"
        } else {
            "(no line matches the filters)"
        };
        lines.push(Line::styled(text, Style::default().fg(palette.dim())));
    }
    (lines, drawn)
}

/// The HEX rows of a window `width` x `height`: only the bytes of these rows are read,
/// in one go. Also returns how many rows were drawn, for the mouse.
fn hex_rows(
    tab: &mut Tab,
    palette: &Palette,
    width: usize,
    height: usize,
) -> (Vec<Line<'static>>, usize) {
    tab.height = height;
    tab.fit_hex_width(width);
    tab.pin_cursor();
    let n = tab.hex_width;
    let rows = tab.row_count();
    if tab.engine.follow_tail {
        tab.top = view::follow_top(rows, height);
    }
    tab.top = view::clamp_top(tab.top, height, rows);
    let range = view::visible_range(tab.top, height, rows);
    let engine = &tab.engine;
    if range.is_empty() {
        let text = "(empty file, or still loading)";
        return (
            vec![Line::styled(text, Style::default().fg(palette.dim()))],
            0,
        );
    }
    let first = range.start * n;
    let bytes = engine.get_bytes(first, range.len() * n).unwrap_or_default();
    let digits = hex::offset_digits(engine.file_size);
    let styles = hex::Styles {
        text: Style::default(),
        offset: Style::default().fg(palette.dim()),
        hit: palette.hit(),
        current: palette.active_hit(),
    };
    let current = engine.current_search_byte();
    let mut lines = Vec::with_capacity(range.len());
    for (i, chunk) in bytes.chunks(n).enumerate() {
        let row = range.start + i;
        let line = hex::render_row(
            first + i * n,
            chunk,
            n,
            digits,
            &engine.search_byte_matches,
            current,
            &styles,
        );
        lines.push(if row == tab.cursor {
            line.patch_style(Style::default().add_modifier(Modifier::REVERSED))
        } else {
            line
        });
    }
    let drawn = lines.len();
    (lines, drawn)
}

/// One row of the log view: gutter, collapse badge, then the text in its level colour
/// with the search hits painted over it.
#[allow(clippy::too_many_arguments)]
pub fn render_row(
    engine: &TailEngine,
    palette: &Palette,
    row: usize,
    line_idx: usize,
    gutter: usize,
    query: &str,
    has_search: bool,
    hscroll: usize,
) -> Line<'static> {
    let (hit, active, bookmarked) = engine.row_marks(row, line_idx, has_search);
    let mut spans = Vec::with_capacity(6);
    let gutter_style = if active {
        palette.active_hit()
    } else {
        Style::default().fg(palette.dim())
    };
    let mark = if bookmarked && engine.is_auto_bookmark(line_idx) {
        'o'
    } else if bookmarked {
        '*'
    } else if hit {
        '>'
    } else {
        ' '
    };
    spans.push(Span::styled(
        format!("{:>gutter$}{mark}", line_idx + 1),
        gutter_style,
    ));
    if let Some(c) = engine.collapsed_row(row).filter(|c| c.count > 1) {
        let badge = if c.open {
            format!("[-{}] ", c.count)
        } else {
            format!("[{}x] ", c.count)
        };
        spans.push(Span::styled(
            badge,
            Style::default()
                .fg(Color::Black)
                .bg(palette.level_color(LogLevel::Debug)),
        ));
    }
    // Render mode strips the sequences and hands their style runs; raw mode keeps the
    // ESC bytes, which `sanitize` shows as `^[`.
    let row = engine.get_row(line_idx);
    let text = row.as_ref().map_or("", |r| r.line.as_str());
    let text = text.trim_end_matches(['\r', '\n']);
    // Precedence as in the GUI: a search hit over a highlight rule over the ANSI
    // colours; the level palette only applies to rows no rule matched.
    let rule = engine.match_highlight(text);
    let base = match &rule {
        Some(rule) => palette.rule_style(rule),
        None => palette.level_style(engine.level_of(line_idx)),
    };
    let runs: &[StyleRun] = match (&rule, &row) {
        (None, Some(r)) => &r.ansi,
        _ => &[],
    };
    let run_ranges: Vec<std::ops::Range<usize>> = runs.iter().map(|r| r.start..r.end).collect();
    let hits = if hit && !query.is_empty() {
        view::hit_ranges(text, query)
    } else {
        Vec::new()
    };
    let segs: Vec<(String, Paint)> = view::segments(text, &hits, &run_ranges)
        .into_iter()
        .map(|(s, p)| (view::sanitize(s), p))
        .collect();
    let hit_style = if active {
        palette.active_hit()
    } else {
        palette.hit()
    };
    for (s, paint) in view::skip_cells(segs, hscroll) {
        let style = match paint {
            Paint::Plain => base,
            Paint::Run(i) => base.patch(palette.ansi_style(&runs[i].style)),
            Paint::Hit => hit_style,
        };
        spans.push(Span::styled(s, style));
    }
    let line = Line::from(spans);
    // A selected row gets a background of its own; hit spans keep theirs.
    if engine.is_selected(line_idx) {
        line.style(palette.selection())
    } else {
        line
    }
}

fn group_digits(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(c);
    }
    out
}

/// Opens `path` through `workspace::open_target`, as the GUI does: a pattern, a plain
/// file, a compressed file or an archive entry (`archive.zip/entry.log`), decompressed
/// to a spool in the background; an archive with one file entry opens that entry. At
/// start an archive with several entries is reported: `o` opens it in the entry picker
/// (see `App::open_file`).
pub fn open_path(
    path: &Path,
    config: &crate::config::FastTailConfig,
) -> Result<TailEngine, String> {
    use crate::workspace::{open_target, OpenOutcome};
    // `compressed::OpenError` has no `Display`: its debug form is enough for now.
    let shown = path.display();
    match open_target(path, config, None) {
        OpenOutcome::Opened(engine) => Ok(*engine),
        OpenOutcome::OpenEntry(entry) => open_path(&entry, config),
        OpenOutcome::Missing => Err(format!("{shown}: not found")),
        OpenOutcome::Failed { error, .. } => Err(format!("{shown}: {error:?}")),
        OpenOutcome::EmptyArchive(_) => Err(format!("{shown}: empty archive")),
        OpenOutcome::ChooseEntries { entries, .. } => {
            let names: Vec<&str> = entries
                .iter()
                .filter(|e| e.refusal.is_none())
                .take(5)
                .map(|e| e.name.as_str())
                .collect();
            Err(format!(
                "{shown}: archive with {} entries; open one as {shown}/<entry> (e.g. {})",
                entries.len(),
                names.join(", ")
            ))
        }
        OpenOutcome::ScanTar { .. } => Err(format!(
            "{shown}: tar archive; open one entry as {shown}/<entry>"
        )),
        OpenOutcome::ListFailed { error, .. } => Err(format!("{shown}: {error}")),
    }
}

/// Standard input as a stream, spooled to disk like the GUI does.
pub fn open_stdin(settings: &crate::stdin_source::Settings) -> Result<TailEngine, String> {
    use crate::stdin_source as stdin;
    let input = stdin::take_stdin().ok_or("standard input is not available")?;
    let stream = stdin::StdinStream::start(input, settings, None)
        .map_err(|e| format!("cannot spool standard input: {e}"))?;
    stdin::open_engine(stream, None).map_err(|e| format!("cannot open standard input: {e}"))
}

/// A path the user typed, made absolute the way the GUI's command line does.
pub fn absolute(p: &str) -> PathBuf {
    let path = PathBuf::from(p);
    if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .map(|d| d.join(&path))
            .unwrap_or(path)
    }
}

/// Longest wait of the event loop for input when nothing runs.
pub const IDLE_TICK: Duration = Duration::from_millis(100);
/// Longest wait while background work runs, so its progress moves smoothly.
pub const BUSY_TICK: Duration = Duration::from_millis(50);

/// Poll timeout of the event loop: 100 ms, 50 ms while background work runs. Input ends
/// the wait at once either way; the files are read at their own cadence (see `tick`).
pub fn poll_timeout(busy: bool) -> Duration {
    if busy {
        BUSY_TICK
    } else {
        IDLE_TICK
    }
}

/// The screen as text, one string per terminal row (for tests and the docs' captures).
pub fn buffer_text(buffer: &ratatui::buffer::Buffer) -> Vec<String> {
    let w = buffer.area.width as usize;
    buffer
        .content
        .chunks(w.max(1))
        .map(|row| row.iter().map(|c| c.symbol()).collect::<String>())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::CyberTheme;
    use crate::tui::colors::ColorDepth;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use std::io::Write;

    fn app_with(files: &[(&str, &str)], ascii: bool) -> (App, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let tabs = files
            .iter()
            .map(|(name, body)| {
                let path = dir.path().join(name);
                std::fs::File::create(&path)
                    .unwrap()
                    .write_all(body.as_bytes())
                    .unwrap();
                Tab::new(TailEngine::open(&path).unwrap())
            })
            .collect();
        let palette = Palette::new(CyberTheme::Tron, ColorDepth::TrueColor, ascii);
        (App::new(tabs, palette), dir)
    }

    fn render(app: &mut App, w: u16, h: u16) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        app.tick();
        terminal.draw(|f| app.draw(f)).unwrap();
        buffer_text(terminal.backend().buffer())
    }

    const LOG: &str = "2026-09-28 10:00:00 INFO start\n\
                       2026-09-28 10:00:01 WARN slow disk\n\
                       2026-09-28 10:00:02 ERROR failed\n";

    fn numbered(n: usize) -> String {
        (1..=n).map(|i| format!("line {i}\n")).collect()
    }

    #[test]
    fn the_cursor_pages_pauses_follow_and_g_follows_again() {
        let body = numbered(10_000);
        let (mut app, _dir) = app_with(&[("big.log", body.as_str())], false);
        // A 40-row window: 44 terminal rows minus the borders and the status bar.
        render(&mut app, 80, 46);
        let height = app.tabs[0].height;
        app.tabs[0].engine.follow_tail = false;
        app.tabs[0].top = 0;
        app.tabs[0].set_cursor(height - 1);
        app.apply(Action::PageDown);
        let tab = &app.tabs[0];
        assert_eq!(tab.cursor, 2 * height - 1, "one page further");
        assert!(
            tab.cursor >= tab.top && tab.cursor < tab.top + height,
            "visible"
        );
        assert!(!tab.engine.follow_tail);
        app.apply(Action::LineUp);
        assert_eq!(app.tabs[0].cursor, 2 * height - 2);

        app.apply(Action::Bottom);
        assert!(app.tabs[0].engine.follow_tail);
        assert_eq!(app.tabs[0].cursor, 9_999, "on the last row");
        app.apply(Action::LineUp);
        assert!(!app.tabs[0].engine.follow_tail, "moving up pauses follow");
        app.apply(Action::Top);
        assert_eq!((app.tabs[0].cursor, app.tabs[0].top), (0, 0));
    }

    #[test]
    fn shift_arrows_select_from_the_cursor_which_is_drawn_reversed() {
        let (mut app, _dir) = app_with(&[("test.log", LOG)], false);
        render(&mut app, 70, 12);
        app.tabs[0].set_cursor(0);
        app.apply(Action::SelectDown);
        app.apply(Action::SelectDown);
        let e = &app.tabs[0].engine;
        assert!(e.is_selected(0) && e.is_selected(1) && e.is_selected(2));
        assert_eq!(app.tabs[0].cursor, 2);
        app.apply(Action::ClearSearch);
        assert!(
            !app.tabs[0].engine.has_selection(),
            "Esc clears the selection"
        );

        // Nothing selected: the cursor row is the one a row action takes.
        app.tabs[0].set_cursor(1);
        assert_eq!(app.tabs[0].cursor_line(), Some(1));
        let mut terminal = Terminal::new(TestBackend::new(70, 12)).unwrap();
        app.tick();
        terminal.draw(|f| app.draw(f)).unwrap();
        let buf = terminal.backend().buffer();
        let reversed = |y: u16| buf[(5, y)].modifier.contains(Modifier::REVERSED);
        // Rows 1-3 of the screen hold lines 1-3; the cursor is on the second.
        assert!(!reversed(1) && reversed(2) && !reversed(3));
    }

    #[test]
    fn bookmarks_notes_and_their_keys_follow_the_cursor() {
        let body = numbered(1_000);
        let (mut app, _dir) = app_with(&[("big.log", body.as_str())], false);
        render(&mut app, 80, 30);
        // `b` on lines 120 and 900 (rows = lines here, no filter), then `[` from 950.
        for line in [119, 899] {
            app.tabs[0].set_cursor(line);
            app.apply(Action::ToggleBookmark);
        }
        app.tabs[0].set_cursor(949);
        app.apply(Action::PrevBookmark);
        assert_eq!(app.tabs[0].cursor_line(), Some(899));
        app.apply(Action::PrevBookmark);
        assert_eq!(app.tabs[0].cursor_line(), Some(119));
        app.apply(Action::PrevBookmark);
        assert_eq!(app.tabs[0].cursor_line(), Some(899), "wraps to the last");
        assert!(app.message.as_deref().unwrap().contains("last"));
        app.message = None;
        app.apply(Action::NextBookmark);
        assert_eq!(app.tabs[0].cursor_line(), Some(119), "wraps to the first");

        // `m` on line 88: the note bookmarks it and the status bar shows it there.
        app.message = None;
        app.tabs[0].set_cursor(87);
        app.apply(Action::EditNote);
        assert_eq!(
            app.prompt.as_ref().map(|p| p.kind),
            Some(PromptKind::Note(87))
        );
        let prompt = app.prompt.take().unwrap();
        app.submit_prompt(Prompt {
            field: TextField::new("payment retried"),
            ..prompt
        });
        assert!(app.tabs[0].engine.is_bookmarked(87));
        let screen = render(&mut app, 80, 30);
        assert!(
            screen.iter().any(|l| l.contains("* payment retried")),
            "{screen:#?}"
        );
        // The gutter marks the bookmarked rows with `*`.
        assert!(screen.iter().any(|l| l.contains("88*")), "{screen:#?}");
        app.apply(Action::ToggleBookmark);
        assert!(!app.tabs[0].engine.is_bookmarked(87));
    }

    fn errors_every_tenth(n: usize) -> String {
        (1..=n)
            .map(|i| {
                let level = if i % 10 == 0 { "ERROR" } else { "INFO" };
                format!("{level} line {i}\n")
            })
            .collect()
    }

    #[test]
    fn ctrl_k_shows_the_cursor_row_in_context_and_esc_returns() {
        let body = errors_every_tenth(200);
        let (mut app, _dir) = app_with(&[("app.log", body.as_str())], false);
        app.tabs[0].engine.set_include_filter("ERROR");
        render(&mut app, 80, 20);
        // ERROR lines are 10, 20, ...: row 4 is line 50 (index 49).
        app.tabs[0].set_cursor(4);
        assert_eq!(app.tabs[0].cursor_line(), Some(49));
        app.apply(Action::ToggleContext);
        let screen = render(&mut app, 80, 20);
        assert!(app.tabs[0].engine.context_line().is_some());
        assert_eq!(
            app.tabs[0].cursor_line(),
            Some(49),
            "same line, every row shown"
        );
        assert_eq!(app.tabs[0].cursor, 49);
        assert!(
            screen.iter().any(|l| l.contains("filters suspended")),
            "{screen:#?}"
        );
        app.apply(Action::ClearSearch);
        render(&mut app, 80, 20);
        assert!(app.tabs[0].engine.context_line().is_none());
        assert_eq!(app.tabs[0].cursor_line(), Some(49), "back on the same line");
        assert_eq!(app.tabs[0].cursor, 4, "among the ERROR rows");
    }

    #[test]
    fn ctrl_k_without_a_filter_says_why() {
        let (mut app, _dir) = app_with(&[("test.log", LOG)], false);
        render(&mut app, 70, 12);
        app.apply(Action::ToggleContext);
        assert!(app.message.as_deref().unwrap().contains("filter"));
    }

    #[test]
    fn go_to_moves_the_cursor_to_a_line_or_the_next_visible_one() {
        let body = errors_every_tenth(200);
        let (mut app, _dir) = app_with(&[("app.log", body.as_str())], false);
        render(&mut app, 80, 20);
        let go = |app: &mut App, text: &str| {
            app.apply(Action::GoTo);
            let prompt = app.prompt.take().expect("the go-to dialog");
            assert_eq!(prompt.kind, PromptKind::Goto);
            app.submit_prompt(Prompt {
                field: TextField::new(text),
                ..prompt
            });
            app.tick();
        };
        go(&mut app, "120");
        assert_eq!(app.tabs[0].cursor_line(), Some(119));
        go(&mut app, "-19");
        assert_eq!(app.tabs[0].cursor_line(), Some(100));
        // Filtered to ERROR: line 45 is hidden, the next visible one is 50.
        app.tabs[0].engine.set_include_filter("ERROR");
        render(&mut app, 80, 20);
        app.message = None;
        go(&mut app, "45");
        assert_eq!(app.tabs[0].cursor_line(), Some(49));
        assert!(app.message.as_deref().unwrap().contains("hidden"));
        go(&mut app, "nonsense");
        assert!(app.message.as_deref().unwrap().contains("Cannot go to"));
    }

    fn go_to(app: &mut App, text: &str) {
        app.apply(Action::GoTo);
        let prompt = app.prompt.take().expect("the go-to dialog");
        app.submit_prompt(Prompt {
            field: TextField::new(text),
            ..prompt
        });
        app.tick();
    }

    #[test]
    fn h_shows_the_bytes_and_keeps_the_place_both_ways() {
        let body = numbered(300);
        let (mut app, _dir) = app_with(&[("n.log", body.as_str())], false);
        render(&mut app, 80, 20);
        let row = app.tabs[0].engine.get_visible_row_of_line(149).unwrap();
        app.tabs[0].set_cursor(row);
        let offset = app.tabs[0].engine.line_offsets[149] as usize;
        app.apply(Action::ToggleHex);
        let screen = render(&mut app, 80, 20);
        let tab = &app.tabs[0];
        assert!(tab.is_hex());
        assert_eq!(tab.hex_width, 16, "80 columns fit 16 bytes a row");
        assert_eq!(
            tab.cursor,
            offset / 16,
            "the row of the cursor line's first byte"
        );
        let label = format!("{:08X}  ", tab.cursor * 16);
        assert!(
            screen.iter().any(|l| l.contains(&label) && l.contains('|')),
            "{screen:#?}"
        );
        assert!(screen
            .iter()
            .any(|l| l.contains("rows of 16") && l.contains("HEX")));

        // Three rows down and back: the line holding the cursor row's first byte.
        for _ in 0..3 {
            app.apply(Action::LineDown);
        }
        let back = app.tabs[0].cursor * 16;
        app.apply(Action::ToggleHex);
        let tab = &app.tabs[0];
        assert!(!tab.is_hex());
        assert_eq!(
            tab.cursor_line(),
            Some(tab.engine.line_of_offset(back as u64))
        );

        // A wider window takes more bytes a row, the cursor stays on the same bytes.
        app.apply(Action::ToggleHex);
        render(&mut app, 80, 20);
        let before = app.tabs[0].cursor * 16;
        render(&mut app, 120, 20);
        assert_eq!(app.tabs[0].hex_width, 24);
        assert_eq!(app.tabs[0].cursor, before / 24);
    }

    #[test]
    fn hex_search_walks_the_byte_hits_and_row_actions_say_why_not() {
        let body = numbered(300);
        let (mut app, _dir) = app_with(&[("n.log", body.as_str())], false);
        render(&mut app, 80, 20);
        app.apply(Action::ToggleHex);
        app.search("line 25");
        render(&mut app, 80, 20);
        let (off, _) = app.tabs[0].engine.current_search_byte().unwrap();
        assert_eq!(off, body.find("line 25").unwrap());
        assert_eq!(app.tabs[0].cursor, off / 16);
        app.apply(Action::SearchNext);
        let screen = render(&mut app, 80, 20);
        let (off, _) = app.tabs[0].engine.current_search_byte().unwrap();
        assert_eq!(off, body.find("line 250").unwrap());
        assert_eq!(app.tabs[0].cursor, off / 16);
        assert!(
            screen.iter().any(|l| l.contains("/line 25 2/11")),
            "{screen:#?}"
        );

        app.apply(Action::ToggleBookmark);
        assert!(app.message.as_deref().unwrap().contains("HEX view"));
        // The filters do not apply to the bytes.
        app.tabs[0].engine.set_include_filter("line 1");
        render(&mut app, 80, 20);
        assert_eq!(app.tabs[0].row_count(), body.len().div_ceil(16));
    }

    #[test]
    fn hex_go_to_takes_a_byte_offset_and_follow_keeps_the_last_row() {
        let body = numbered(300);
        let (mut app, _dir) = app_with(&[("n.log", body.as_str())], false);
        render(&mut app, 80, 20);
        app.apply(Action::Bottom);
        app.apply(Action::ToggleHex);
        let screen = render(&mut app, 80, 20);
        let rows = body.len().div_ceil(16);
        assert!(app.tabs[0].engine.follow_tail, "follow stays on");
        assert_eq!(app.tabs[0].cursor, rows - 1);
        let last = format!("{:08X}  ", (rows - 1) * 16);
        assert!(screen.iter().any(|l| l.contains(&last)), "{screen:#?}");

        go_to(&mut app, "0x100");
        assert_eq!(app.tabs[0].cursor, 16);
        assert!(!app.tabs[0].engine.follow_tail);
        go_to(&mut app, "1000");
        assert_eq!(app.tabs[0].cursor, 1000 / 16);
        go_to(&mut app, "99999999");
        assert_eq!(app.tabs[0].cursor, rows - 1, "clamped to the last byte");
        go_to(&mut app, "zz");
        assert!(app.message.as_deref().unwrap().contains("Cannot go to"));
    }

    #[test]
    fn a_binary_file_opens_in_the_hex_view() {
        let body = "\u{0}\u{1}\u{2}binary\u{0}\u{0}".repeat(64);
        let (mut app, _dir) = app_with(&[("blob.bin", body.as_str())], false);
        let screen = render(&mut app, 80, 20);
        assert!(app.tabs[0].is_hex());
        assert!(
            screen.iter().any(|l| l.contains("00 01 02 62 69 6E")),
            "{screen:#?}"
        );
    }

    /// Draws the app and returns the buffer, to look at the cells' styles.
    fn draw_buffer(app: &mut App, w: u16, h: u16) -> ratatui::buffer::Buffer {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        app.tick();
        terminal.draw(|f| app.draw(f)).unwrap();
        terminal.backend().buffer().clone()
    }

    /// The cell where `needle` first starts on screen.
    fn cell_of<'a>(buf: &'a ratatui::buffer::Buffer, needle: &str) -> &'a ratatui::buffer::Cell {
        let rows = buffer_text(buf);
        let (y, x) = rows
            .iter()
            .enumerate()
            .find_map(|(y, r)| r.find(needle).map(|b| (y, r[..b].chars().count())))
            .unwrap_or_else(|| panic!("{needle:?} not on screen: {rows:#?}"));
        &buf[(x as u16, y as u16)]
    }

    #[test]
    fn a_cycles_the_ansi_modes_and_no_escape_reaches_the_terminal() {
        use crate::ansi::{AnsiColor, AnsiStyle};
        let body = "plain\n\x1b[31mred\x1b[0m and \x1b[1;4mloud\x1b[0m\n";
        let (mut app, _dir) = app_with(&[("c.log", body)], false);
        let red = app
            .palette
            .ansi_style(&AnsiStyle {
                fg: Some(AnsiColor::Indexed(1)),
                ..Default::default()
            })
            .fg;
        let no_esc = |buf: &ratatui::buffer::Buffer| {
            assert!(buffer_text(buf).iter().all(|r| !r.contains('\x1b')));
        };

        // Auto found the sequences: the colours and attributes are drawn.
        let buf = draw_buffer(&mut app, 80, 10);
        no_esc(&buf);
        assert!(buffer_text(&buf).iter().any(|r| r.contains("red and loud")));
        assert_eq!(cell_of(&buf, "red").fg, red.unwrap());
        let loud = cell_of(&buf, "loud").modifier;
        assert!(loud.contains(Modifier::BOLD | Modifier::UNDERLINED));

        // A search hit wins over the ANSI colour under it.
        app.search("red");
        let buf = draw_buffer(&mut app, 80, 10);
        assert_eq!(
            cell_of(&buf, "red and").bg,
            app.palette.active_hit().bg.unwrap()
        );
        app.apply(Action::ClearSearch);

        app.apply(Action::CycleAnsi);
        assert_eq!(app.message.as_deref(), Some("ANSI: render"));
        app.apply(Action::CycleAnsi);
        assert_eq!(app.message.as_deref(), Some("ANSI: strip"));
        let buf = draw_buffer(&mut app, 80, 10);
        assert!(buffer_text(&buf).iter().any(|r| r.contains("red and loud")));
        assert_ne!(
            cell_of(&buf, "red").fg,
            red.unwrap(),
            "strip paints nothing"
        );

        app.apply(Action::CycleAnsi);
        let buf = draw_buffer(&mut app, 80, 10);
        no_esc(&buf);
        let rows = buffer_text(&buf);
        assert!(
            rows.iter()
                .any(|r| r.contains("^[[31mred^[[0m and ^[[1;4mloud")),
            "{rows:#?}"
        );
        assert!(rows.iter().any(|r| r.contains("ansi:raw")));
        app.apply(Action::CycleAnsi);
        assert_eq!(app.message.as_deref(), Some("ANSI: auto (now render)"));
    }

    fn keys(app: &mut App, text: &str) {
        use crossterm::event::{KeyCode, KeyEvent};
        for c in text.chars() {
            app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
    }

    fn press(app: &mut App, code: crossterm::event::KeyCode) {
        app.on_key(crossterm::event::KeyEvent::new(code, KeyModifiers::NONE));
    }

    #[test]
    fn the_time_range_dialog_checks_both_sides_and_esc_keeps_the_range() {
        use crossterm::event::KeyCode;
        let (mut app, _dir) = app_with(&[("t.log", LOG)], false);
        render(&mut app, 80, 20);
        app.apply(Action::TimeRange);
        keys(&mut app, "10:00:01");
        press(&mut app, KeyCode::Tab);
        keys(&mut app, "yesterday-ish");
        let screen = render(&mut app, 80, 20);
        assert!(
            screen.iter().any(|l| l.contains("Time range")),
            "{screen:#?}"
        );
        press(&mut app, KeyCode::Enter);
        let d = app.time_range.as_ref().expect("still open");
        assert!(d.invalid, "an unreadable side blocks OK");
        assert!(
            app.tabs[0].engine.time_from_text.is_empty(),
            "nothing applied"
        );

        // Emptied, the "to" side is an open end.
        app.on_key(crossterm::event::KeyEvent::new(
            KeyCode::Char('u'),
            KeyModifiers::CONTROL,
        ));
        press(&mut app, KeyCode::Enter);
        assert!(app.time_range.is_none());
        assert_eq!(app.tabs[0].engine.time_from_text, "10:00:01");
        for _ in 0..200 {
            app.tick();
            if app.tabs[0].engine.visible_line_count() == 2 {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(app.tabs[0].engine.visible_line_count(), 2);
        let screen = render(&mut app, 80, 20);
        assert!(
            screen.iter().any(|l| l.contains("time:10:00:01..")),
            "{screen:#?}"
        );

        // Reopened with the range as typed; Esc keeps it.
        app.apply(Action::TimeRange);
        assert_eq!(app.time_range.as_ref().unwrap().from.text(), "10:00:01");
        press(&mut app, KeyCode::Backspace);
        press(&mut app, KeyCode::Esc);
        assert!(app.time_range.is_none());
        assert_eq!(app.tabs[0].engine.time_from_text, "10:00:01");
    }

    #[test]
    fn a_paste_goes_into_the_open_field_and_the_cursor_edits_in_place() {
        use crossterm::event::KeyCode;
        let (mut app, _dir) = app_with(&[("a.log", LOG)], false);
        assert!(!app.on_paste("ignored"), "no field open");
        app.apply(Action::StartSearch);
        app.on_paste("disk\r\n");
        press(&mut app, KeyCode::Home);
        keys(&mut app, "slow ");
        assert_eq!(app.prompt.as_ref().unwrap().field.text(), "slow disk  ");
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.tabs[0].engine.search_query, "slow disk");
    }

    /// `logs.tar.gz` holding `app.log`, `db.log` and `web.log`.
    fn tar_gz(dir: &Path) -> PathBuf {
        use std::io::Write as _;
        let mut builder = tar::Builder::new(Vec::new());
        for name in ["app.log", "db.log", "web.log"] {
            let data = format!("{name} line 1\n{name} line 2\n");
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            builder
                .append_data(&mut header, name, data.as_bytes())
                .unwrap();
        }
        let tar = builder.into_inner().unwrap();
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(&tar).unwrap();
        let path = dir.join("logs.tar.gz");
        std::fs::write(&path, gz.finish().unwrap()).unwrap();
        path
    }

    #[test]
    fn the_entry_picker_filters_by_typing_and_opens_the_entry() {
        use crossterm::event::KeyCode;
        let (mut app, dir) = app_with(&[("a.log", LOG)], false);
        let archive = tar_gz(dir.path());
        app.open_file(&archive);
        for _ in 0..200 {
            app.tick();
            if app.picker.as_ref().is_some_and(|p| p.scan.is_none()) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let names = |app: &App| -> Vec<String> {
            let p = app.picker.as_ref().expect("the picker");
            p.shown()
                .iter()
                .map(|&i| p.entries[i].name.clone())
                .collect()
        };
        assert_eq!(names(&app), ["app.log", "db.log", "web.log"]);
        let screen = render(&mut app, 80, 24);
        assert!(
            screen
                .iter()
                .any(|l| l.contains("Open an entry of logs.tar.gz")),
            "{screen:#?}"
        );
        keys(&mut app, "db");
        assert_eq!(names(&app), ["db.log"]);
        press(&mut app, KeyCode::Enter);
        assert!(app.picker.is_none());
        assert_eq!(app.tabs.len(), 2);
        assert_eq!(app.active, 1);
        assert_eq!(app.tabs[1].title, "logs.tar.gz/db.log");

        // Opening it again shows the stream already open.
        app.focus_tab(0);
        app.open_file(&crate::compressed::entry_path(&archive, "db.log"));
        assert_eq!((app.tabs.len(), app.active), (2, 1));

        // A click on a list row opens that entry; Esc closes the picker.
        app.open_file(&archive);
        for _ in 0..200 {
            app.tick();
            if app.picker.as_ref().is_some_and(|p| p.scan.is_none()) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        render(&mut app, 80, 24);
        let (rect, _) = app.hits.list_items[2];
        app.on_mouse(click(rect.x + 1, rect.y));
        assert_eq!(app.tabs.last().unwrap().title, "logs.tar.gz/web.log");
        app.open_file(&archive);
        press(&mut app, KeyCode::Esc);
        assert!(app.picker.is_none());
    }

    #[test]
    fn o_opens_a_typed_path_and_says_when_it_is_missing() {
        use crossterm::event::KeyCode;
        let (mut app, dir) = app_with(&[("a.log", LOG)], false);
        std::fs::write(dir.path().join("b.log"), "b\n").unwrap();
        app.apply(Action::OpenFile);
        let b = dir.path().join("b.log");
        app.on_paste(&b.display().to_string());
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.tabs.len(), 2);
        assert_eq!((app.active, app.tabs[1].title.as_str()), (1, "b.log"));
        app.apply(Action::OpenFile);
        app.on_paste(&dir.path().join("nope.log").display().to_string());
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.tabs.len(), 2);
        assert!(app.message.as_deref().unwrap().contains("not found"));
    }

    fn prompt_submit(app: &mut App, text: &str) {
        let prompt = app.prompt.take().expect("a prompt");
        app.submit_prompt(Prompt {
            field: TextField::new(text),
            ..prompt
        });
    }

    #[test]
    fn s_saves_a_session_asks_before_overwriting_and_refuses_the_ini() {
        use crossterm::event::KeyCode;
        let (mut app, dir) = app_with(&[("a.log", LOG)], false);
        let ini = dir.path().join("fasttail.ini");
        app.settings = Some(crate::tui::workspace::Settings {
            path: ini.clone(),
            ..Default::default()
        });
        app.tabs[0].engine.toggle_bookmark(1);
        app.apply(Action::SaveSession);
        assert_eq!(
            app.prompt.as_ref().unwrap().field.text(),
            "session.fasttail-session.ini"
        );
        let typed = dir.path().join("incident");
        prompt_submit(&mut app, &typed.display().to_string());
        let file = dir.path().join("incident.fasttail-session.ini");
        assert!(file.is_file(), "{:?}", app.message);
        let config = &app.settings.as_ref().unwrap().config;
        assert_eq!(config.recent_sessions.first(), Some(&file));
        assert_eq!(config.current_session.as_ref(), Some(&file));
        let written = std::fs::read_to_string(&file).unwrap();
        assert!(!written.contains("dock_layout"), "{written}");

        // The same path again: asked first; Esc keeps the file, Enter replaces it.
        std::fs::write(&file, "old").unwrap();
        app.apply(Action::SaveSession);
        prompt_submit(&mut app, &file.display().to_string());
        assert_eq!(app.confirm_overwrite.as_ref(), Some(&file));
        press(&mut app, KeyCode::Esc);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "old");
        app.apply(Action::SaveSession);
        prompt_submit(&mut app, &file.display().to_string());
        press(&mut app, KeyCode::Enter);
        assert!(app.confirm_overwrite.is_none());
        assert_ne!(std::fs::read_to_string(&file).unwrap(), "old");

        // The active fasttail.ini is never a session (the session saves above also
        // saved it, as a configuration).
        let config_bytes = std::fs::read(&ini).unwrap();
        app.apply(Action::SaveSession);
        prompt_submit(&mut app, &ini.display().to_string());
        assert_eq!(std::fs::read(&ini).unwrap(), config_bytes);
        assert!(app
            .message
            .as_deref()
            .unwrap()
            .contains("configuration file"));
    }

    #[test]
    fn o_loads_a_session_from_a_path_or_the_recent_list() {
        use crossterm::event::KeyCode;
        let (mut app, dir) = app_with(&[("a.log", LOG)], false);
        app.tabs[0].engine.toggle_bookmark(2);
        let file = dir.path().join("x.fasttail-session.ini");
        app.save_session(&file);

        let (mut other, _dir2) = app_with(&[("b.log", "b\n")], false);
        other.apply(Action::OpenSession);
        other.on_paste(&file.display().to_string());
        press(&mut other, KeyCode::Enter);
        assert!(other.sessions.is_none());
        assert_eq!(other.tabs.len(), 1, "{:?}", other.message);
        assert_eq!(other.tabs[0].title, "a.log");
        assert!(
            other.tabs[0].engine.is_bookmarked(2),
            "its saved state applies"
        );
        let config = &other.settings.as_ref().unwrap().config;
        assert_eq!(config.recent_sessions.first(), Some(&file));

        // Empty path: Enter loads the selected recent session; a missing file says why.
        other.apply(Action::OpenSession);
        let screen = render(&mut other, 80, 20);
        assert!(
            screen.iter().any(|l| l.contains("Open session")),
            "{screen:#?}"
        );
        press(&mut other, KeyCode::Enter);
        assert_eq!(other.tabs.len(), 1);
        other.apply(Action::OpenSession);
        keys(&mut other, "missing.fasttail-session.ini");
        press(&mut other, KeyCode::Enter);
        assert!(other.message.as_deref().unwrap().contains("cannot load"));
        assert_eq!(other.tabs.len(), 1, "the open streams stay");
    }

    #[test]
    fn the_help_scrolls_on_a_short_screen_and_closes_on_another_key() {
        use crossterm::event::KeyCode;
        let (mut app, _dir) = app_with(&[("a.log", LOG)], false);
        app.apply(Action::ToggleHelp);
        let tall = render(&mut app, 100, 60);
        for key in ["F3", "Ctrl+G", "O  S", "?  F1", "HEX", "ANSI"] {
            assert!(tall.iter().any(|l| l.contains(key)), "{key}: {tall:#?}");
        }
        let short = render(&mut app, 80, 24);
        assert!(
            short.iter().any(|l| l.contains("Up/Down scroll")),
            "{short:#?}"
        );
        assert!(short.iter().any(|l| l.contains("move the cursor one row")));
        press(&mut app, KeyCode::Down);
        assert!(app.show_help, "the arrows scroll");
        let scrolled = render(&mut app, 80, 24);
        assert!(!scrolled
            .iter()
            .any(|l| l.contains("move the cursor one row")));
        press(&mut app, KeyCode::Char('x'));
        assert!(!app.show_help);
    }

    #[test]
    fn the_loop_ticks_at_100_ms_and_reads_the_files_at_the_ini_cadence() {
        assert_eq!(poll_timeout(false), Duration::from_millis(100));
        assert_eq!(poll_timeout(true), Duration::from_millis(50));
        let (mut app, dir) = app_with(&[("a.log", LOG)], false);
        app.idle_poll = Duration::from_secs(3600);
        app.tick();
        let before = app.tabs[0].engine.total_lines();
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(dir.path().join("a.log"))
            .unwrap();
        f.write_all(b"2026-09-28 10:00:03 INFO more\n").unwrap();
        drop(f);
        app.tick();
        assert_eq!(app.tabs[0].engine.total_lines(), before, "not due yet");
        app.idle_poll = Duration::ZERO;
        app.tick();
        assert_eq!(app.tabs[0].engine.total_lines(), before + 1);
    }

    /// A terminal run over `ini`, as `tui::run` builds it.
    fn app_over(ini: &Path) -> App {
        let settings = crate::tui::workspace::Settings::read(ini);
        let plan = crate::tui::workspace::workspace_plan(&settings);
        let (engines, errors) = crate::tui::workspace::open_plan(&settings, &plan);
        assert!(errors.is_empty(), "{errors:?}");
        let palette = Palette::new(CyberTheme::Tron, ColorDepth::TrueColor, false);
        let mut app = App::new(engines.into_iter().map(Tab::new).collect(), palette);
        app.settings = Some(settings);
        app
    }

    #[test]
    fn an_idle_terminal_never_rewrites_what_a_gui_saved_meanwhile() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.log");
        std::fs::write(&a, LOG).unwrap();
        let ini = dir.path().join("fasttail.ini");
        let gui = crate::config::FastTailConfig {
            open_files: vec![a.clone()],
            ..Default::default()
        };
        gui.save_to(&ini).unwrap();
        let mut app = app_over(&ini);
        app.save_config();
        app.save_config();

        // The GUI saves other state; the terminal, unchanged, leaves the file alone.
        let gui_bytes = b"[general]\ntheme=Matrix\n".to_vec();
        std::fs::write(&ini, &gui_bytes).unwrap();
        app.save_config();
        assert_eq!(std::fs::read(&ini).unwrap(), gui_bytes);

        // Its own change is written: the last instance that writes defines the file.
        let b = dir.path().join("b.log");
        std::fs::write(&b, "b\n").unwrap();
        app.open_file(&b);
        app.save_config();
        let text = std::fs::read_to_string(&ini).unwrap();
        assert!(text.contains("b.log"), "{text}");
        let saved = crate::tui::workspace::Settings::read(&ini).config;
        assert_eq!(saved.open_files, vec![a, b.clone()]);
        assert_eq!(saved.recent_files.first(), Some(&b));
    }

    #[test]
    fn a_terminal_save_carries_the_gui_only_keys_through_byte_for_byte() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.log");
        std::fs::write(&a, LOG).unwrap();
        let ini = dir.path().join("fasttail.ini");
        let gui = crate::config::FastTailConfig {
            open_files: vec![a.clone()],
            font_size: 17.0,
            max_fps: 90,
            renderer: crate::renderer::RendererChoice::Glow,
            zoom_factor: 1.25,
            dock_layout: Some("(gui dock layout)".into()),
            ..Default::default()
        };
        gui.save_to(&ini).unwrap();
        let before = std::fs::read_to_string(&ini).unwrap();
        let gui_lines: Vec<&str> = before
            .lines()
            .filter(|l| {
                [
                    "font_size=",
                    "max_fps=",
                    "renderer=",
                    "zoom_factor=",
                    "layout=",
                ]
                .iter()
                .any(|k| l.starts_with(k))
            })
            .collect();
        assert_eq!(gui_lines.len(), 5, "{before}");

        let mut app = app_over(&ini);
        app.tabs[0].engine.toggle_bookmark(1);
        app.save_config();
        let after = std::fs::read_to_string(&ini).unwrap();
        assert_ne!(after, before, "the bookmark is saved");
        for line in gui_lines {
            assert!(after.lines().any(|l| l == line), "{line} lost:\n{after}");
        }
        let saved = crate::tui::workspace::Settings::read(&ini).config;
        assert_eq!(saved.bookmarks_for(&a, 3), Some(vec![1]));
    }

    #[test]
    fn the_theme_paints_the_windows_and_dialogs_cast_a_shadow() {
        let (mut app, _dir) = app_with(&[("a.log", LOG)], false);
        let panel = app.palette.window().bg.unwrap();
        let screen_bg = app.palette.screen().bg.unwrap();
        let buf = draw_buffer(&mut app, 80, 20);
        // Inside the window (right of the text), and the theme's text colour.
        assert_eq!(buf[(70, 3)].bg, panel);
        assert_eq!(buf[(70, 3)].fg, app.palette.window().fg.unwrap());

        app.apply(Action::StartSearch);
        let buf = draw_buffer(&mut app, 80, 20);
        let d = app.hits.dialog.unwrap().outer;
        assert_eq!(buf[(d.x + 1, d.y + 1)].bg, app.palette.dialog().bg.unwrap());
        let shadow = app.palette.shadow().bg.unwrap();
        assert_eq!(buf[(d.right(), d.y + 1)].bg, shadow, "right of the dialog");
        assert_eq!(buf[(d.x + 3, d.bottom())].bg, shadow, "below it");
        assert_ne!(buf[(d.x, d.bottom())].bg, shadow, "offset from the corner");
        assert_ne!(shadow, screen_bg);
        assert_eq!(buf[(d.x, d.y)].symbol(), "┌", "square corners");

        // 16 colours: the terminal's own background and text.
        let p16 = Palette::new(CyberTheme::Tron, ColorDepth::Ansi16, false);
        assert_eq!(p16.window(), Style::default());
        assert_eq!(p16.screen(), Style::default());
    }

    #[test]
    fn t_cycles_the_themes_and_records_the_choice_for_the_ini() {
        let (mut app, _dir) = app_with(&[("a.log", LOG)], false);
        app.settings = Some(crate::tui::workspace::Settings::default());
        let before = app.palette.window();
        app.apply(Action::CycleTheme);
        assert_eq!(app.palette.theme, CyberTheme::Matrix);
        assert_ne!(app.palette.window(), before);
        assert_eq!(
            app.settings.as_ref().unwrap().config.theme,
            CyberTheme::Matrix
        );
        assert!(app.message.as_deref().unwrap().starts_with("Theme: Matrix"));
        for _ in 0..3 {
            app.apply(Action::CycleTheme);
        }
        assert_eq!(app.palette.theme, CyberTheme::Tron, "back to the first");
    }

    #[test]
    fn stream_window_has_borders_title_and_counts() {
        let (mut app, _dir) = app_with(&[("test.log", LOG)], false);
        let screen = render(&mut app, 70, 12);
        // One file: no tab strip, the window starts on the first row, double-bordered.
        assert!(screen[0].starts_with('╔'), "{screen:#?}");
        assert!(screen[0].contains("[#1] test.log"), "{screen:#?}");
        assert!(screen[0].contains("FOLLOW"), "{screen:#?}");
        assert!(screen[1].starts_with('║') && screen[1].contains("INFO start"));
        assert!(screen.iter().any(|l| l.contains("WARN slow disk")));
        // Bottom border of the window: the counts on the left, the filter state right.
        let bottom = &screen[8];
        assert!(bottom.starts_with('╚'), "{screen:#?}");
        assert!(bottom.contains("3/3 lines") && bottom.contains("no filter"));
        // The status bar is its own single-bordered window.
        assert!(screen[9].starts_with('┌') && screen[9].contains("FastTail TUI"));
        assert!(screen[11].starts_with('└'));
    }

    #[test]
    fn ascii_mode_split_and_prompt_dialog() {
        let (mut app, _dir) = app_with(&[("a.log", LOG), ("b.log", LOG)], true);
        app.apply(Action::CycleSplit);
        let screen = render(&mut app, 80, 14);
        // Tab strip on row 0, then two windows side by side, the focused one with `=`.
        let (left, right) = screen[1].split_at(40);
        assert!(
            left.starts_with("+ [#1] a.log") && left.contains("=="),
            "{screen:#?}"
        );
        assert!(
            right.starts_with("+ [#2] b.log") && right.contains("--"),
            "{screen:#?}"
        );
        assert!(!screen.concat().contains('╔'));
        // Tab moves the focus: now the right window has the `=` border.
        app.apply(Action::NextTab);
        let screen = render(&mut app, 80, 14);
        let (left, right) = screen[1].split_at(40);
        assert!(left.contains("--") && right.contains("=="), "{screen:#?}");
        // The search prompt opens as a centred dialog over the windows.
        app.apply(Action::StartSearch);
        let screen = render(&mut app, 80, 14);
        assert!(
            screen.iter().any(|l| l.contains("+ Search ")),
            "{screen:#?}"
        );
        assert!(screen.iter().any(|l| l.contains("Enter confirm")));
        assert!(screen.iter().any(|l| l.contains("[ OK ]  [ Cancel ]")));
    }

    fn mouse(kind: MouseEventKind, column: u16, row: u16, shift: bool) -> MouseEvent {
        MouseEvent {
            kind,
            column,
            row,
            modifiers: if shift {
                KeyModifiers::SHIFT
            } else {
                KeyModifiers::NONE
            },
        }
    }

    fn click(column: u16, row: u16) -> MouseEvent {
        mouse(MouseEventKind::Down(MouseButton::Left), column, row, false)
    }

    #[test]
    fn clicking_the_second_window_of_a_split_focuses_it() {
        let (mut app, _dir) = app_with(&[("a.log", LOG), ("b.log", LOG)], false);
        app.apply(Action::CycleSplit);
        render(&mut app, 80, 14);
        assert_eq!(app.active, 0);
        // Somewhere on the right window's rows.
        assert!(app.on_mouse(click(60, 3)));
        assert_eq!(app.active, 1);
        let screen = render(&mut app, 80, 14);
        let (left, right) = screen[1].split_at(screen[1].char_indices().nth(40).unwrap().0);
        assert!(
            left.starts_with('┌') && right.starts_with('╔'),
            "{screen:#?}"
        );
        // A click on the strip title of stream 1 brings the focus back.
        assert!(app.on_mouse(click(2, 0)));
        assert_eq!(app.active, 0);
    }

    #[test]
    fn clicks_select_rows_and_the_wheel_scrolls_the_window_under_the_pointer() {
        let body: String = (1..=100)
            .map(|i| format!("2026-09-28 10:00:00 INFO line {i}\n"))
            .collect();
        let (mut app, _dir) = app_with(&[("a.log", &body), ("b.log", &body)], false);
        app.apply(Action::CycleSplit);
        render(&mut app, 80, 14);
        // Both windows follow the end: the wheel over the right one pauses only it.
        assert!(app.on_mouse(mouse(MouseEventKind::ScrollUp, 60, 5, false)));
        assert!(!app.tabs[1].engine.follow_tail);
        assert!(app.tabs[0].engine.follow_tail);
        assert_eq!(app.active, 0);
        render(&mut app, 80, 14);
        // The left window shows lines 93..=100 on rows 2..=9: click line 95, then
        // SHIFT + click line 97.
        assert!(app.on_mouse(click(10, 4)));
        assert_eq!(app.tabs[0].engine.selected_lines(), vec![94]);
        assert!(app.on_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 10, 6, true)));
        assert_eq!(app.tabs[0].engine.selected_lines(), vec![94, 95, 96]);
        // Clicking on a row a second time at once is a double click: a bookmark.
        app.last_click = None;
        app.on_mouse(click(10, 4));
        app.on_mouse(click(10, 4));
        assert!(app.tabs[0].engine.is_bookmarked(94));
    }

    #[test]
    fn dialog_buttons_and_clicks_outside() {
        let (mut app, _dir) = app_with(&[("a.log", LOG)], false);
        app.apply(Action::StartSearch);
        if let Some(p) = app.prompt.as_mut() {
            p.field = TextField::new("warn");
        }
        render(&mut app, 80, 14);
        let ok = app.hits.dialog.unwrap().ok;
        assert!(app.on_mouse(click(ok.x + 1, ok.y)));
        assert!(app.prompt.is_none());
        assert_eq!(app.tabs[0].engine.search_query, "warn");
        // A click outside an open dialog closes it without applying it.
        app.apply(Action::EditInclude);
        render(&mut app, 80, 14);
        assert!(app.on_mouse(click(0, 0)));
        assert!(app.prompt.is_none());
        assert_eq!(app.tabs[0].engine.include_filter(), "");
    }
}
