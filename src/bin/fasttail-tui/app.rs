//! The terminal app: one `TailEngine` per stream, the key handling and the drawing.
//! Every stream is a bordered window; the status bar, the prompts and the help are
//! bordered too, so the screen reads as a set of windows rather than a text dump.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use fasttail::collapse::CollapseMode;
use fasttail::log_level::LogLevel;
use fasttail::scan_job::ScanKind;
use fasttail::tail_engine::{TailEngine, ViewMode};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};
use ratatui::Frame;

use crate::clipboard::{Clipboard, Copied};
use crate::colors::{Chrome, Palette};
use crate::keys::{self, Action, PromptKey};
use crate::mouse::{self, DialogHit, HitMap, Target, WindowHit};
use crate::view;

/// Columns moved by one horizontal scroll step.
const HSCROLL_STEP: usize = 8;

/// One open stream and the view state the terminal keeps for it.
pub struct Tab {
    pub engine: TailEngine,
    pub title: String,
    /// First row on screen (a row of the engine's filtered, collapsed view).
    pub top: usize,
    /// Characters hidden on the left (horizontal scroll).
    pub hscroll: usize,
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
            height: 20,
            pending_first_hit: false,
            seen_lines: 0,
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
            if let Some(line) = tab.engine.scroll_to_line.take() {
                if let Some(row) = tab.engine.get_visible_row_of_line(line) {
                    tab.engine.follow_tail = false;
                    let rows = tab.engine.visible_line_count();
                    tab.top = view::reveal(tab.top, tab.height, rows, row);
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
            Action::EditExclude => self.open_prompt(PromptKind::Exclude),
            Action::CycleSplit => self.cycle_split(),
            Action::CopyOrQuit if !self.tabs[self.active].engine.has_selection() => {
                self.quit = true
            }
            Action::Copy | Action::CopyOrQuit => self.copy_selection(),
            _ => self.apply_to_tab(action),
        }
    }

    fn copy_selection(&mut self) {
        let engine = &self.tabs[self.active].engine;
        let Some(text) = engine
            .has_selection()
            .then(|| engine.copy_selection_text())
            .flatten()
        else {
            self.message = Some("Nothing selected: click a row, SHIFT + click or drag".into());
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
        let scroll = |tab: &mut Tab, top: usize| {
            tab.engine.follow_tail = false;
            tab.top = view::clamp_top(top, page, rows);
        };
        match action {
            Action::ToggleFollow => tab.engine.follow_tail = !tab.engine.follow_tail,
            Action::Bottom => tab.engine.follow_tail = true,
            Action::Top => scroll(tab, 0),
            Action::LineUp => scroll(tab, tab.top.saturating_sub(1)),
            Action::LineDown => scroll(tab, tab.top + 1),
            Action::PageUp => scroll(tab, tab.top.saturating_sub(page)),
            Action::PageDown => scroll(tab, tab.top + page),
            Action::ScrollLeft => tab.hscroll = tab.hscroll.saturating_sub(HSCROLL_STEP),
            Action::ScrollRight => tab.hscroll += HSCROLL_STEP,
            Action::ScrollHome => tab.hscroll = 0,
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
        let inner = block.inner(area);
        frame.render_widget(block, area);
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
        let line = match &self.message {
            Some(m) => Line::styled(m.clone(), Style::default().fg(self.palette.accent())),
            None => Line::styled(
                "? help  q quit  Space follow  / search  n/N next  i/x filter  l level  c collapse  s split  y copy",
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
            "Up/Down j/k      scroll one line",
            "PgUp/PgDn        scroll one page (also Ctrl+B / Ctrl+F)",
            "Home g / End G   top / bottom (bottom follows)",
            "Left/Right 0     horizontal scroll / back to column 0",
            "Space            toggle follow",
            "/  n  N  Esc     search, next, previous, clear",
            "i  x             include / exclude filter",
            "l                cycle the minimum level",
            "c                cycle collapse: off, exact, numbers",
            "s                split: side by side, stacked, off",
            "Tab  Alt+1..9    next window or file / file N",
            "y  Ctrl+C        copy the selected rows (Ctrl+C quits if none)",
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

/// The rows of a window `height` rows tall: only these are read from the engine.
/// Also returns how many rows were drawn, for the mouse.
fn stream_rows(tab: &mut Tab, palette: &Palette, height: usize) -> (Vec<Line<'static>>, usize) {
    tab.height = height;
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
        lines.push(render_row(
            engine,
            palette,
            row,
            line_idx,
            gutter,
            &query,
            has_search,
            tab.hscroll,
        ));
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
    let mark = if bookmarked {
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
    let base = palette.level_style(engine.level_of(line_idx));
    let text = engine.get_row(line_idx).map(|r| r.line).unwrap_or_default();
    let text = text.trim_end_matches(['\r', '\n']);
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
    for (s, is_hit) in view::skip_chars(segs, hscroll) {
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

/// Opens `path` the way the GUI does: a single compressed file or an archive entry
/// (`archive.zip/entry.log`) is decompressed to a spool in the background; a zip with
/// exactly one entry opens that entry. Other archives need the GUI's entry picker.
pub fn open_path(path: &Path) -> Result<TailEngine, String> {
    use fasttail::compressed::{self, Target};
    let settings = compressed::Settings {
        spool_dir: fasttail::spool::spool_dir(None),
        limits: compressed::Limits::default(),
    };
    // `compressed::OpenError` has no `Display`: its debug form is enough for a prototype.
    let err = |e: &dyn std::fmt::Debug| format!("{}: {e:?}", path.display());
    if fasttail::wildcard::is_pattern_path(path) {
        return TailEngine::open_pattern(path).map_err(|e| err(&e));
    }
    match compressed::classify(path) {
        Target::Plain => TailEngine::open(path).map_err(|e| err(&e)),
        Target::Compressed(_) => {
            compressed::open_engine(path, None, &settings, None).map_err(|e| err(&e))
        }
        Target::Entry { archive, entry, .. } => {
            compressed::open_engine(&archive, Some(&entry), &settings, None).map_err(|e| err(&e))
        }
        Target::ZipArchive => {
            let entries = compressed::list_zip_entries(path).map_err(|e| err(&e))?;
            let usable: Vec<_> = entries.iter().filter(|e| e.refusal.is_none()).collect();
            if usable.len() == 1 {
                compressed::open_engine(path, Some(&usable[0].name), &settings, None)
                    .map_err(|e| err(&e))
            } else {
                let names: Vec<&str> = usable.iter().take(5).map(|e| e.name.as_str()).collect();
                Err(format!(
                    "{}: zip with {} entries; open one as {}/<entry> (e.g. {})",
                    path.display(),
                    usable.len(),
                    path.display(),
                    names.join(", ")
                ))
            }
        }
        Target::EmptyZip => Err(format!("{}: empty zip archive", path.display())),
        Target::TarArchive(_) => Err(format!(
            "{}: tar archive; open one entry as {}/<entry> (the TUI has no entry picker)",
            path.display(),
            path.display()
        )),
    }
}

/// Standard input as a stream, spooled to disk like the GUI does.
pub fn open_stdin() -> Result<TailEngine, String> {
    use fasttail::stdin_source as stdin;
    let input = stdin::take_stdin().ok_or("standard input is not available")?;
    let settings = stdin::Settings::from_config(None, stdin::DEFAULT_MAX_MB);
    let stream = stdin::StdinStream::start(input, &settings, None)
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

/// Poll timeout of the event loop: shorter while background work runs, so progress
/// shows smoothly, longer when idle.
pub fn poll_timeout(busy: bool) -> Duration {
    if busy {
        Duration::from_millis(50)
    } else {
        Duration::from_millis(100)
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
    use crate::colors::ColorDepth;
    use fasttail::theme::CyberTheme;
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
