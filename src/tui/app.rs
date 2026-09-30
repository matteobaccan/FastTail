// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The terminal app: one `TailEngine` per stream, the key handling and the drawing.
//! Every stream is a bordered window; the status bar, the prompts and the help are
//! bordered too, so the screen reads as a set of windows rather than a text dump.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use crate::collapse::CollapseMode;
use crate::log_level::LogLevel;
use crate::scan_job::ScanKind;
use crate::tail_engine::{TailEngine, ViewMode};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};
use ratatui::Frame;

use crate::tui::clipboard::{Clipboard, Copied};
use crate::tui::colors::{Chrome, Palette};
use crate::tui::keys::{self, Action, PromptKey};
use crate::tui::mouse::{self, DialogHit, HitMap, Target, WindowHit};
use crate::tui::view;

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
    /// Waiting for the first hit of a search that runs in the background.
    pending_first_hit: bool,
    /// Lines already seen while this stream was on screen, for the "new lines" mark.
    seen_lines: usize,
}

impl Tab {
    pub fn new(mut engine: TailEngine) -> Self {
        // HEX and Markdown are GUI views: the terminal always shows the lines.
        if engine.view_mode != ViewMode::Text {
            engine.set_view_mode(ViewMode::Text);
        }
        let title = title_of(&engine.path);
        Self {
            engine,
            title,
            top: 0,
            hscroll: 0,
            cursor: 0,
            height: 20,
            pending_first_hit: false,
            seen_lines: 0,
        }
    }

    /// Moves the cursor to `row` (clamped to the view) and scrolls the window to keep it
    /// visible. Any move pauses follow; `Bottom` turns it back on.
    pub fn set_cursor(&mut self, row: usize) {
        let rows = self.engine.visible_line_count();
        self.engine.follow_tail = false;
        self.cursor = row.min(rows.saturating_sub(1));
        self.top = view::reveal(self.top, self.height.max(1), rows, self.cursor);
    }

    /// The line under the cursor (the first line of a collapsed group).
    pub fn cursor_line(&self) -> Option<usize> {
        self.engine.get_actual_line_idx(self.cursor)
    }

    /// In follow mode the cursor sits on the last row, which moves as lines arrive.
    fn pin_cursor(&mut self) {
        let rows = self.engine.visible_line_count();
        if self.engine.follow_tail {
            self.cursor = rows.saturating_sub(1);
        } else if rows > 0 && self.cursor >= rows {
            self.cursor = rows - 1;
        }
    }
}

fn title_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
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
}

pub struct Prompt {
    pub kind: PromptKind,
    pub text: String,
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
    pub message: Option<String>,
    pub show_help: bool,
    pub quit: bool,
    /// Mouse capture is on (the help says how to select text natively).
    pub mouse: bool,
    /// Event-loop wait when nothing runs (the ini's `poll_interval_ms`).
    pub idle_poll: Duration,
    /// Clickable rectangles of the last frame.
    pub hits: HitMap,
    clipboard: Clipboard,
    /// Last left click on a row (when, stream, line), to tell a double click.
    last_click: Option<(Instant, usize, usize)>,
    /// Stream whose rows a left-button drag is selecting.
    drag: Option<usize>,
    last_signature: Signature,
}

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
            message: None,
            show_help: false,
            quit: false,
            mouse: true,
            idle_poll: Duration::from_millis(250),
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
        for tab in &mut self.tabs {
            tab.engine.poll_updates();
        }
        for i in self.visible_tabs() {
            let tab = &mut self.tabs[i];
            if tab.pending_first_hit {
                if tab.engine.active_match_count() > 0 {
                    tab.pending_first_hit = false;
                    // The engine may already have made the first hit current: show it
                    // rather than stepping past it.
                    match tab.engine.current_search_line() {
                        Some(line) => tab.engine.scroll_to_line = Some(line),
                        None => {
                            tab.engine.search_next(false);
                        }
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
            tab.seen_lines = tab.engine.total_lines();
        }
        let sig = self.signature();
        let changed = sig != self.last_signature;
        self.last_signature = sig;
        changed
    }

    /// Background work in flight: the loop then polls a little faster.
    pub fn busy(&self) -> bool {
        self.tabs.iter().any(|t| {
            t.engine.scan_progress().is_some()
                || t.engine.compressed.as_ref().is_some_and(|c| c.is_running())
        })
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
        if self.prompt.is_some() {
            return self.on_prompt_key(key);
        }
        let Some(action) = keys::map_key(key) else {
            return false;
        };
        // The help dialog closes on any key, and only `?` / F1 reopen it.
        if self.show_help {
            self.show_help = false;
            if action == Action::ToggleHelp {
                return true;
            }
        }
        self.message = None;
        self.apply(action);
        true
    }

    fn on_prompt_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        let Some(pk) = keys::map_prompt_key(key) else {
            return false;
        };
        let Some(prompt) = self.prompt.as_mut() else {
            return false;
        };
        match keys::edit_prompt(&mut prompt.text, pk) {
            Some(PromptKey::Submit) => {
                if let Some(prompt) = self.prompt.take() {
                    self.submit_prompt(prompt);
                }
            }
            Some(PromptKey::Cancel) => self.prompt = None,
            _ => {}
        }
        true
    }

    fn submit_prompt(&mut self, prompt: Prompt) {
        let tab = &mut self.tabs[self.active];
        let text = prompt.text.trim().to_string();
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
            text: text.to_string(),
        });
    }

    fn open_prompt(&mut self, kind: PromptKind) {
        let e = &self.tabs[self.active].engine;
        let text = match kind {
            PromptKind::Search => e.search_query.clone(),
            PromptKind::Include => e.include_filter().to_string(),
            PromptKind::Exclude => e.exclude_filter().to_string(),
            PromptKind::Note(line) => e.bookmark_note(line).unwrap_or_default().to_string(),
            PromptKind::Goto => String::new(),
        };
        self.prompt = Some(Prompt { kind, text });
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
                if self.prompt.is_some() || self.show_help {
                    return false;
                }
                let Some(tab) = mouse::window_at(&self.hits, ev.column, ev.row) else {
                    return false;
                };
                let t = &mut self.tabs[tab];
                let rows = t.engine.visible_line_count();
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
                let Some(tab) = self.drag else {
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
            Target::DialogOk => {
                if let Some(prompt) = self.prompt.take() {
                    self.submit_prompt(prompt);
                }
                self.show_help = false;
            }
            Target::DialogCancel | Target::OutsideDialog => {
                self.prompt = None;
                self.show_help = false;
            }
            Target::DialogBody | Target::Nothing => return false,
            Target::TabTitle(i) | Target::WindowTitle(i) | Target::Window(i) => self.focus_tab(i),
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
        let rows = tab.engine.visible_line_count();
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
            .border_set(palette.border_set(chrome))
            .border_style(palette.border_style(chrome))
            .title_top(Line::from(vec![
                Span::styled(format!(" [#{}] {} ", idx + 1, tab.title), title_style),
                follow,
                Span::raw(" "),
            ]))
            .title_bottom(Line::from(format!(" {} ", counts_text(e))).style(title_style))
            .title_bottom(
                Line::from(format!(" {} ", view_state_text(e)))
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
        let (lines, row_count) = stream_rows(tab, &palette, inner.height as usize);
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
                "? help  q quit  Space follow  / search  n/N next  i/x filter  b mark  ]/[ marks  m note  y copy",
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
            PromptKind::Goto => "Go to line (N, +N, -N) or time (14:02)",
        };
        let inner = self.dialog(frame, area, (64, 5), title, true);
        let Some(p) = &self.prompt else {
            return;
        };
        let hint = Line::styled(
            "Enter confirm   Esc cancel   Ctrl+U clear",
            Style::default().fg(self.palette.dim()),
        );
        frame.render_widget(
            Paragraph::new(vec![Line::raw(format!("> {}", p.text)), hint]),
            inner,
        );
        let x = inner.x + 2 + p.text.chars().count() as u16;
        frame.set_cursor_position((x.min(inner.right().saturating_sub(1)), inner.y));
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
            "/  n  N  Esc     search, next, previous, clear",
            "i  x             include / exclude filter",
            "l                cycle the minimum level",
            "c                cycle collapse: off, exact, numbers",
            "s                split: side by side, stacked, off",
            "Tab  Alt+1..9    next window or file / file N",
            "b  Ctrl+F2       bookmark the cursor row, on or off",
            "] F2 / [ Shift+F2  next / previous bookmark (wraps)",
            "m                note of the cursor row's bookmark",
            "Ctrl+K           cursor row in context (filters off), again back",
            "Ctrl+G  :        go to a line (N, +N, -N) or a time (14:02)",
            "y  Ctrl+C        copy the selection or the cursor row",
            "                 (Ctrl+C quits when nothing is selected)",
            "q                quit",
            "",
            "Wheel scrolls the window under the pointer; click focuses",
            "and selects a row, SHIFT + click or drag a range, double",
            "click toggles a bookmark.",
            mouse_hint,
        ];
        let inner = self.dialog(
            frame,
            area,
            (66, text.len() as u16 + 3),
            "Keys - any key closes",
            false,
        );
        frame.render_widget(
            Paragraph::new(text.iter().map(|l| Line::from(*l)).collect::<Vec<_>>()),
            inner,
        );
    }
}

/// Bottom-left of a window: rows and lines, and the background work in progress.
fn counts_text(e: &TailEngine) -> String {
    let mut s = format!(
        "{}/{} lines",
        group_digits(e.visible_line_count()),
        group_digits(e.total_lines())
    );
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
fn view_state_text(e: &TailEngine) -> String {
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
    let text = engine.get_row(line_idx).map(|r| r.line).unwrap_or_default();
    let text = text.trim_end_matches(['\r', '\n']);
    // The first highlight rule that matches colours the row, as in the GUI; the level
    // palette only applies to rows no rule matched.
    let base = match engine.match_highlight(text) {
        Some(rule) => palette.rule_style(&rule),
        None => palette.level_style(engine.level_of(line_idx)),
    };
    let hits = if hit && !query.is_empty() {
        view::hit_ranges(text, query)
    } else {
        Vec::new()
    };
    let segs: Vec<(String, bool)> = view::segments(text, &hits)
        .into_iter()
        .map(|(s, h)| (view::sanitize(s), h))
        .collect();
    let hit_style = if active {
        palette.active_hit()
    } else {
        palette.hit()
    };
    for (s, is_hit) in view::skip_cells(segs, hscroll) {
        spans.push(Span::styled(s, if is_hit { hit_style } else { base }));
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
/// to a spool in the background; an archive with one file entry opens that entry. An
/// archive with several entries has to be opened by entry path until the terminal's
/// entry picker (task 3.7).
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

/// Poll timeout of the event loop: `idle` (the ini's `poll_interval_ms`) when nothing
/// runs, at most 50 ms while background work runs so progress shows smoothly. Input
/// ends the wait at once either way.
pub fn poll_timeout(busy: bool, idle: Duration) -> Duration {
    if busy {
        idle.min(Duration::from_millis(50))
    } else {
        idle
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
            text: "payment retried".into(),
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
                text: text.into(),
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
            p.text = "warn".into();
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
