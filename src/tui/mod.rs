//! Terminal User Interface (TUI) frontend for FastTail using `ratatui` and `crossterm`.
//!
//! Provides ultra-low-overhead, zero-GPU log monitoring that runs inside any terminal,
//! over SSH sessions, on headless servers, or on legacy Windows environments.

use std::borrow::Cow;
use std::io::stdout;
use std::path::Path;
use std::time::{Duration, Instant};

use crossterm::{
    event::{
        self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
        MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Tabs},
    Frame, Terminal,
};

use crate::cli::CliArgs;
use crate::config::FastTailConfig;
use crate::log_level::LogLevel;
use crate::paths::paths_equal;
use crate::tail_engine::{HighlightStyle, SpanStyle, TailEngine, ViewMode};

/// RAII guard ensuring the terminal returns to cooked mode on exit or unwind.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            stdout(),
            crossterm::event::DisableMouseCapture,
            LeaveAlternateScreen,
            crossterm::cursor::Show
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    Search,
    IncludeFilter,
    ExcludeFilter,
    OpenFile,
    GotoLine,
}

pub struct TuiApp {
    pub engines: Vec<TailEngine>,
    pub active_tab: usize,
    pub scroll_offsets: Vec<usize>,
    pub show_line_numbers: bool,
    pub show_help: bool,
    pub prompt: Option<(PromptKind, String)>,
    pub status_notice: Option<(String, Instant)>,
    pub config: FastTailConfig,
    pub should_quit: bool,
}

impl TuiApp {
    pub fn new(cli: CliArgs, config: FastTailConfig) -> Self {
        let mut engines = Vec::new();
        let mut scroll_offsets = Vec::new();

        let mut paths_to_open = Vec::new();
        if !cli.fresh {
            for p in &config.open_files {
                if p.exists() || crate::wildcard::is_pattern_path(p) {
                    paths_to_open.push(p.clone());
                }
            }
        }
        for p in cli.paths {
            if !paths_to_open
                .iter()
                .any(|existing| paths_equal(existing, &p))
            {
                paths_to_open.push(p);
            }
        }

        for path in paths_to_open {
            let is_pattern = crate::wildcard::is_pattern_path(&path);
            let opened = if is_pattern {
                TailEngine::open_pattern(&path)
            } else {
                TailEngine::open(&path)
            };
            if let Ok(mut eng) = opened {
                eng.size_check_interval =
                    Duration::from_millis(config.size_check_interval_ms as u64);
                eng.set_markdown_max_bytes((config.markdown_max_mb as u64) * 1024 * 1024);
                eng.set_highlight_rules(config.highlight_rules.clone());
                eng.size_unit = config.size_unit;
                eng.wrap_lines = config.wrap_for(&path);
                if let Some(follow) = cli.follow {
                    eng.follow_tail = follow;
                }
                if let Some(ref inc) = cli.filter {
                    eng.set_include_filter(inc);
                }
                if let Some(ref exc) = cli.exclude {
                    eng.set_exclude_filter(exc);
                }
                engines.push(eng);
                scroll_offsets.push(0);
            }
        }

        Self {
            engines,
            active_tab: 0,
            scroll_offsets,
            show_line_numbers: config.show_line_numbers,
            show_help: false,
            prompt: None,
            status_notice: None,
            config,
            should_quit: false,
        }
    }

    pub fn current_engine(&mut self) -> Option<&mut TailEngine> {
        if self.engines.is_empty() {
            None
        } else {
            self.active_tab = self.active_tab.min(self.engines.len() - 1);
            Some(&mut self.engines[self.active_tab])
        }
    }

    pub fn current_scroll(&self) -> usize {
        self.scroll_offsets
            .get(self.active_tab)
            .copied()
            .unwrap_or(0)
    }

    pub fn set_current_scroll(&mut self, val: usize) {
        if self.active_tab < self.scroll_offsets.len() {
            self.scroll_offsets[self.active_tab] = val;
        }
    }

    pub fn open_file<P: AsRef<Path>>(&mut self, path: P) {
        let path_buf = path.as_ref().to_path_buf();
        if let Some(pos) = self
            .engines
            .iter()
            .position(|e| paths_equal(&e.path, &path_buf))
        {
            self.active_tab = pos;
            return;
        }
        let is_pattern = crate::wildcard::is_pattern_path(&path_buf);
        let opened = if is_pattern {
            TailEngine::open_pattern(&path_buf)
        } else {
            TailEngine::open(&path_buf)
        };
        if let Ok(mut eng) = opened {
            eng.size_check_interval =
                Duration::from_millis(self.config.size_check_interval_ms as u64);
            eng.set_markdown_max_bytes((self.config.markdown_max_mb as u64) * 1024 * 1024);
            eng.set_highlight_rules(self.config.highlight_rules.clone());
            eng.size_unit = self.config.size_unit;
            eng.wrap_lines = self.config.wrap_for(&path_buf);
            self.engines.push(eng);
            self.scroll_offsets.push(0);
            self.active_tab = self.engines.len() - 1;
            if !self
                .config
                .open_files
                .iter()
                .any(|p| paths_equal(p, &path_buf))
            {
                self.config.open_files.push(path_buf.clone());
                let _ = self.config.save();
            }
        } else {
            self.status_notice = Some((
                format!("Failed to open {}", path_buf.display()),
                Instant::now(),
            ));
        }
    }

    pub fn close_current_tab(&mut self) {
        if self.engines.is_empty() {
            return;
        }
        let closed = self.engines.remove(self.active_tab);
        self.scroll_offsets.remove(self.active_tab);
        self.config
            .open_files
            .retain(|p| !paths_equal(p, &closed.path));
        let _ = self.config.save();
        if self.active_tab >= self.engines.len() && !self.engines.is_empty() {
            self.active_tab = self.engines.len() - 1;
        }
    }

    pub fn poll_updates(&mut self) {
        for (i, eng) in self.engines.iter_mut().enumerate() {
            let was_at_bottom = eng.follow_tail;
            eng.poll_updates();
            if was_at_bottom {
                let total = if eng.view_mode == ViewMode::Hex {
                    eng.total_hex_rows(16)
                } else {
                    eng.visible_line_count()
                };
                if i < self.scroll_offsets.len() {
                    self.scroll_offsets[i] = total;
                }
            }
        }
    }
}

pub fn run_tui(cli: CliArgs, config: FastTailConfig) -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        crossterm::event::EnableMouseCapture,
        crossterm::cursor::Hide
    )?;
    let _guard = TerminalGuard;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let mut app = TuiApp::new(cli, config);

    let poll_interval = Duration::from_millis(app.config.poll_interval_ms as u64);
    let mut last_poll = Instant::now();

    while !app.should_quit {
        terminal.draw(|f| draw_ui(f, &mut app))?;

        let timeout = poll_interval.saturating_sub(last_poll.elapsed());
        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(key) => {
                    if key.kind == KeyEventKind::Press {
                        handle_key_event(&mut app, key);
                    }
                }
                Event::Mouse(mouse) => {
                    handle_mouse_event(&mut app, mouse);
                }
                _ => {}
            }
        }

        if last_poll.elapsed() >= poll_interval {
            app.poll_updates();
            last_poll = Instant::now();
        }
    }

    Ok(())
}

fn find_next_match(eng: &TailEngine, cur_row: usize) -> Option<usize> {
    if eng.search_matches.is_empty() {
        return None;
    }
    for &line_idx in &eng.search_matches {
        if line_idx > cur_row {
            return Some(line_idx);
        }
    }
    eng.search_matches.first().copied()
}

fn find_prev_match(eng: &TailEngine, cur_row: usize) -> Option<usize> {
    if eng.search_matches.is_empty() {
        return None;
    }
    for &line_idx in eng.search_matches.iter().rev() {
        if line_idx < cur_row {
            return Some(line_idx);
        }
    }
    eng.search_matches.last().copied()
}

fn handle_key_event(app: &mut TuiApp, key: KeyEvent) {
    if let Some((kind, ref mut buf)) = app.prompt {
        match key.code {
            KeyCode::Esc => {
                app.prompt = None;
            }
            KeyCode::Enter => {
                let text = buf.clone();
                app.prompt = None;
                apply_prompt(app, kind, &text);
            }
            KeyCode::Backspace => {
                buf.pop();
            }
            KeyCode::Char(c) => {
                buf.push(c);
            }
            _ => {}
        }
        return;
    }

    if app.show_help {
        if matches!(
            key.code,
            KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') | KeyCode::Enter
        ) {
            app.show_help = false;
        }
        return;
    }

    match (key.modifiers, key.code) {
        (KeyModifiers::CONTROL, KeyCode::Char('c')) | (_, KeyCode::Char('q')) => {
            app.should_quit = true;
        }
        (_, KeyCode::Tab) => {
            if !app.engines.is_empty() {
                app.active_tab = (app.active_tab + 1) % app.engines.len();
            }
        }
        (_, KeyCode::BackTab) => {
            if !app.engines.is_empty() {
                app.active_tab = (app.active_tab + app.engines.len() - 1) % app.engines.len();
            }
        }
        (_, KeyCode::Char(c)) if c.is_ascii_digit() && c != '0' => {
            let idx = (c as usize) - ('1' as usize);
            if idx < app.engines.len() {
                app.active_tab = idx;
            }
        }
        (_, KeyCode::Char('c')) => {
            app.close_current_tab();
        }
        (_, KeyCode::Char('o')) => {
            app.prompt = Some((PromptKind::OpenFile, String::new()));
        }
        (_, KeyCode::Char('?')) | (_, KeyCode::F(1)) => {
            app.show_help = !app.show_help;
        }
        (_, KeyCode::Char('#')) => {
            app.show_line_numbers = !app.show_line_numbers;
        }
        (_, KeyCode::Char(' ')) => {
            if let Some(eng) = app.current_engine() {
                eng.follow_tail = !eng.follow_tail;
            }
        }
        (_, KeyCode::Char('m')) => {
            if let Some(eng) = app.current_engine() {
                let next = match eng.view_mode {
                    ViewMode::Hex => ViewMode::Text,
                    _ => ViewMode::Hex,
                };
                eng.set_view_mode(next);
            }
        }
        (_, KeyCode::Char('w')) => {
            if let Some(eng) = app.current_engine() {
                eng.wrap_lines = !eng.wrap_lines;
            }
        }
        (_, KeyCode::Char('/')) => {
            app.prompt = Some((PromptKind::Search, String::new()));
        }
        (_, KeyCode::Char('f')) => {
            app.prompt = Some((PromptKind::IncludeFilter, String::new()));
        }
        (_, KeyCode::Char('x')) => {
            app.prompt = Some((PromptKind::ExcludeFilter, String::new()));
        }
        (_, KeyCode::Char('g')) | (_, KeyCode::Home) => {
            if let Some(eng) = app.current_engine() {
                eng.follow_tail = false;
            }
            app.set_current_scroll(0);
        }
        (_, KeyCode::Char('G')) | (_, KeyCode::End) => {
            let total = if let Some(eng) = app.current_engine() {
                eng.follow_tail = true;
                if eng.view_mode == ViewMode::Hex {
                    eng.total_hex_rows(16)
                } else {
                    eng.visible_line_count()
                }
            } else {
                0
            };
            app.set_current_scroll(total);
        }
        (_, KeyCode::Up) | (_, KeyCode::Char('k')) => {
            if let Some(eng) = app.current_engine() {
                eng.follow_tail = false;
            }
            let cur = app.current_scroll();
            app.set_current_scroll(cur.saturating_sub(1));
        }
        (_, KeyCode::Down) | (_, KeyCode::Char('j')) => {
            let max = app
                .current_engine()
                .map(|e| e.visible_line_count())
                .unwrap_or(0);
            let cur = app.current_scroll();
            let next = (cur + 1).min(max);
            app.set_current_scroll(next);
            if next >= max {
                if let Some(eng) = app.current_engine() {
                    eng.follow_tail = true;
                }
            }
        }
        (_, KeyCode::PageUp) | (_, KeyCode::Char('u')) => {
            if let Some(eng) = app.current_engine() {
                eng.follow_tail = false;
            }
            let cur = app.current_scroll();
            app.set_current_scroll(cur.saturating_sub(20));
        }
        (_, KeyCode::PageDown) | (_, KeyCode::Char('d')) => {
            let max = app
                .current_engine()
                .map(|e| e.visible_line_count())
                .unwrap_or(0);
            let cur = app.current_scroll();
            let next = (cur + 20).min(max);
            app.set_current_scroll(next);
            if next >= max {
                if let Some(eng) = app.current_engine() {
                    eng.follow_tail = true;
                }
            }
        }
        (_, KeyCode::Char(':')) => {
            app.prompt = Some((PromptKind::GotoLine, String::new()));
        }
        (KeyModifiers::SHIFT, KeyCode::Char('N'))
        | (KeyModifiers::SHIFT, KeyCode::F(3))
        | (_, KeyCode::Char('N')) => {
            let cur = app.current_scroll();
            let prev_hit = app.current_engine().and_then(|eng| {
                eng.follow_tail = false;
                find_prev_match(eng, cur)
            });
            if let Some(hit) = prev_hit {
                app.set_current_scroll(hit);
            }
        }
        (_, KeyCode::Char('n')) | (_, KeyCode::F(3)) => {
            let cur = app.current_scroll();
            let next_hit = app.current_engine().and_then(|eng| {
                eng.follow_tail = false;
                find_next_match(eng, cur)
            });
            if let Some(hit) = next_hit {
                app.set_current_scroll(hit);
            }
        }
        _ => {}
    }
}

fn handle_mouse_event(app: &mut TuiApp, mouse: MouseEvent) {
    match mouse.kind {
        MouseEventKind::ScrollUp => {
            if let Some(eng) = app.current_engine() {
                eng.follow_tail = false;
            }
            let cur = app.current_scroll();
            app.set_current_scroll(cur.saturating_sub(3));
        }
        MouseEventKind::ScrollDown => {
            let max = app
                .current_engine()
                .map(|e| e.visible_line_count())
                .unwrap_or(0);
            let cur = app.current_scroll();
            let next = (cur + 3).min(max);
            app.set_current_scroll(next);
            if next >= max {
                if let Some(eng) = app.current_engine() {
                    eng.follow_tail = true;
                }
            }
        }
        MouseEventKind::Down(MouseButton::Left) => {
            if mouse.row == 0 && !app.engines.is_empty() {
                let mut col_accum = 0;
                for (i, eng) in app.engines.iter().enumerate() {
                    let title_len = eng
                        .path
                        .file_name()
                        .map(|n| n.to_string_lossy().len())
                        .unwrap_or(8)
                        + 4;
                    if (mouse.column as usize) >= col_accum
                        && (mouse.column as usize) < col_accum + title_len
                    {
                        app.active_tab = i;
                        break;
                    }
                    col_accum += title_len;
                }
            }
        }
        _ => {}
    }
}

fn apply_prompt(app: &mut TuiApp, kind: PromptKind, text: &str) {
    let trimmed = text.trim();
    match kind {
        PromptKind::Search => {
            let hit = if let Some(eng) = app.current_engine() {
                eng.update_search(trimmed);
                if !trimmed.is_empty() {
                    eng.follow_tail = false;
                    find_next_match(eng, 0)
                } else {
                    None
                }
            } else {
                None
            };
            if let Some(target) = hit {
                app.set_current_scroll(target);
            }
        }
        PromptKind::IncludeFilter => {
            if let Some(eng) = app.current_engine() {
                eng.set_include_filter(trimmed);
            }
        }
        PromptKind::ExcludeFilter => {
            if let Some(eng) = app.current_engine() {
                eng.set_exclude_filter(trimmed);
            }
        }
        PromptKind::OpenFile => {
            if !trimmed.is_empty() {
                app.open_file(trimmed);
            }
        }
        PromptKind::GotoLine => {
            if let Ok(line) = trimmed.parse::<usize>() {
                if let Some(eng) = app.current_engine() {
                    eng.follow_tail = false;
                }
                app.set_current_scroll(line.saturating_sub(1));
            }
        }
    }
}

fn draw_ui(frame: &mut Frame, app: &mut TuiApp) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(if app.prompt.is_some() { 1 } else { 0 }),
        ])
        .split(frame.area());

    draw_header(frame, chunks[0], app);

    let active_tab = app.active_tab;
    let show_line_nums = app.show_line_numbers;
    if !app.engines.is_empty() && active_tab < app.engines.len() {
        let mut scroll = app.scroll_offsets[active_tab];
        draw_body(
            frame,
            chunks[1],
            &mut app.engines[active_tab],
            &mut scroll,
            show_line_nums,
        );
        app.scroll_offsets[active_tab] = scroll;
    } else {
        let empty = Paragraph::new("\n  No log files are open.\n\n  • Press 'o' to open a file or pattern (e.g. C:/logs/app-*.log)\n  • Press '?' for keyboard shortcuts\n  • Press 'q' to quit\n")
            .style(Style::default().fg(Color::DarkGray))
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Blue)));
        frame.render_widget(empty, chunks[1]);
    }

    draw_status(frame, chunks[2], app);

    if let Some((kind, ref input)) = app.prompt {
        draw_prompt(frame, chunks[3], kind, input);
    }

    if app.show_help {
        draw_help_modal(frame, frame.area());
    }
}

fn draw_header(frame: &mut Frame, area: Rect, app: &TuiApp) {
    let titles: Vec<Line> = if app.engines.is_empty() {
        vec![Line::from(" No files open (press 'o' to open) ")]
    } else {
        app.engines
            .iter()
            .enumerate()
            .map(|(i, eng)| {
                let name = eng
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "unnamed".to_string());
                let marker = if eng.has_new_data { "*" } else { "" };
                let label = format!(" [{}] {}{} ", i + 1, name, marker);
                Line::from(label)
            })
            .collect()
    };

    let tabs = Tabs::new(titles)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" ⚡ FASTTAIL ⚡ ")
                .title_alignment(Alignment::Center)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .select(app.active_tab)
        .style(Style::default().fg(Color::Gray))
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_widget(tabs, area);
}

fn draw_body(
    frame: &mut Frame,
    area: Rect,
    eng: &mut TailEngine,
    scroll: &mut usize,
    show_line_numbers: bool,
) {
    let is_hex = eng.view_mode == ViewMode::Hex;
    let height = area.height.saturating_sub(2) as usize;
    let total_items = if is_hex {
        eng.total_hex_rows(16)
    } else {
        eng.visible_line_count()
    };

    let mut top_idx = *scroll;
    if eng.follow_tail || top_idx + height > total_items {
        top_idx = total_items.saturating_sub(height);
        *scroll = top_idx;
    }

    let mut lines: Vec<Line> = Vec::with_capacity(height);

    if is_hex {
        for row in top_idx..top_idx.saturating_add(height).min(total_items) {
            let offset = row * 16;
            let bytes = eng.get_bytes(offset, 16).unwrap_or_default();
            let mut hex_part = String::with_capacity(48);
            let mut ascii_part = String::with_capacity(16);
            for i in 0..16 {
                if i < bytes.len() {
                    hex_part.push_str(&format!("{:02X} ", bytes[i]));
                    let b = bytes[i];
                    if (0x20..=0x7E).contains(&b) {
                        ascii_part.push(b as char);
                    } else {
                        ascii_part.push('.');
                    }
                } else {
                    hex_part.push_str("   ");
                }
                if i == 7 {
                    hex_part.push(' ');
                }
            }

            lines.push(Line::from(vec![
                Span::styled(
                    format!("{:08X}: ", offset),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(hex_part, Style::default().fg(Color::LightCyan)),
                Span::styled(
                    format!(" |{}|", ascii_part),
                    Style::default().fg(Color::Yellow),
                ),
            ]));
        }
    } else {
        for row in top_idx..top_idx.saturating_add(height).min(total_items) {
            let actual_line_idx = match eng.get_actual_line_idx(row) {
                Some(idx) => idx,
                None => continue,
            };
            let raw_text = eng.get_line(actual_line_idx).unwrap_or(Cow::Borrowed(""));
            let level = eng.level_of(actual_line_idx);

            let mut spans = Vec::new();
            if show_line_numbers {
                spans.push(Span::styled(
                    format!("{:6} ", actual_line_idx + 1),
                    Style::default().fg(Color::DarkGray),
                ));
            }

            let span_highlight = eng.match_highlight_spans(&raw_text);
            let is_search_hit = !eng.search_query.is_empty()
                && eng.search_matches.binary_search(&actual_line_idx).is_ok();

            if is_search_hit {
                spans.push(Span::styled(
                    raw_text.to_string(),
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ));
            } else if !span_highlight.spans.is_empty() {
                let mut last = 0;
                for h in &span_highlight.spans {
                    if h.start > last && h.start <= raw_text.len() {
                        spans.push(Span::styled(
                            raw_text[last..h.start].to_string(),
                            level_style(level),
                        ));
                    }
                    if h.start < raw_text.len() {
                        let end = h.end.min(raw_text.len());
                        if h.start < end {
                            let slice = raw_text[h.start..end].to_string();
                            match h.style {
                                SpanStyle::Rule(r) => {
                                    spans.push(Span::styled(slice, convert_highlight_style(&r)))
                                }
                                SpanStyle::Label(_) => spans.push(Span::styled(
                                    slice,
                                    Style::default().fg(Color::Black).bg(Color::Magenta),
                                )),
                            }
                        }
                    }
                    last = h.end;
                }
                if last < raw_text.len() {
                    spans.push(Span::styled(
                        raw_text[last..].to_string(),
                        level_style(level),
                    ));
                }
            } else if let Some(rest) = span_highlight.rest {
                spans.push(Span::styled(
                    raw_text.to_string(),
                    convert_highlight_style(&rest),
                ));
            } else {
                spans.push(Span::styled(raw_text.to_string(), level_style(level)));
            }

            lines.push(Line::from(spans));
        }
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue));
    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

fn draw_status(frame: &mut Frame, area: Rect, app: &TuiApp) {
    if app.engines.is_empty() {
        let text = Line::from(vec![
            Span::styled(
                " FASTTAIL TUI ",
                Style::default().fg(Color::Black).bg(Color::Cyan),
            ),
            Span::raw(" | [o] Open file | [?] Help | [q] Quit"),
        ]);
        frame.render_widget(Paragraph::new(text), area);
        return;
    }

    let eng = &app.engines[app.active_tab];
    let follow_badge = if eng.follow_tail {
        Span::styled(
            " [FOLLOW] ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(
            " [PAUSED] ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
    };

    let mode_badge = match eng.view_mode {
        ViewMode::Hex => Span::styled(
            " HEX ",
            Style::default().fg(Color::Black).bg(Color::LightMagenta),
        ),
        ViewMode::Markdown => {
            Span::styled(" MD ", Style::default().fg(Color::Black).bg(Color::Blue))
        }
        _ => Span::styled(" TXT ", Style::default().fg(Color::Black).bg(Color::White)),
    };

    let size_str = format!("{:.2} MB", eng.file_size as f64 / (1024.0 * 1024.0));
    let lines_str = format!("{} lines", eng.total_lines());
    let enc_str = eng.encoding.name();

    let mut info_spans = vec![
        follow_badge,
        Span::raw(" "),
        mode_badge,
        Span::raw(" "),
        Span::styled(size_str, Style::default().fg(Color::Cyan)),
        Span::raw(" | "),
        Span::styled(lines_str, Style::default().fg(Color::LightGreen)),
        Span::raw(" | "),
        Span::styled(enc_str, Style::default().fg(Color::DarkGray)),
    ];

    if !eng.include_filter.is_empty() {
        info_spans.push(Span::raw(" | Inc: "));
        info_spans.push(Span::styled(
            &eng.include_filter,
            Style::default().fg(Color::Green),
        ));
    }
    if !eng.exclude_filter.is_empty() {
        info_spans.push(Span::raw(" | Exc: "));
        info_spans.push(Span::styled(
            &eng.exclude_filter,
            Style::default().fg(Color::Red),
        ));
    }
    if !eng.search_query.is_empty() {
        let matches_count = eng.search_matches.len();
        info_spans.push(Span::raw(" | Find: "));
        info_spans.push(Span::styled(
            format!("{} ({} matches)", eng.search_query, matches_count),
            Style::default().fg(Color::Yellow),
        ));
    }

    let bar = Paragraph::new(Line::from(info_spans));
    frame.render_widget(bar, area);
}

fn draw_prompt(frame: &mut Frame, area: Rect, kind: PromptKind, input: &str) {
    let (prefix, color) = match kind {
        PromptKind::Search => (" Search (regex): ", Color::Yellow),
        PromptKind::IncludeFilter => (" Include filter (regex): ", Color::Green),
        PromptKind::ExcludeFilter => (" Exclude filter (regex): ", Color::Red),
        PromptKind::OpenFile => (" Open file / pattern: ", Color::Cyan),
        PromptKind::GotoLine => (" Go to line number: ", Color::Magenta),
    };

    let text = Line::from(vec![
        Span::styled(
            prefix,
            Style::default()
                .fg(Color::Black)
                .bg(color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(" {}_", input)),
    ]);
    frame.render_widget(Paragraph::new(text), area);
}

fn draw_help_modal(frame: &mut Frame, area: Rect) {
    let popup_area = centered_rect(65, 75, area);
    frame.render_widget(Clear, popup_area);

    let help_text = vec![
        Line::from(Span::styled(
            "⚡ FASTTAIL TUI - KEYBOARD SHORTCUTS ⚡",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  q / Ctrl+C   ", Style::default().fg(Color::Yellow)),
            Span::raw("Quit FastTail"),
        ]),
        Line::from(vec![
            Span::styled("  Tab / S-Tab  ", Style::default().fg(Color::Yellow)),
            Span::raw("Next / Previous file tab"),
        ]),
        Line::from(vec![
            Span::styled("  1..=9        ", Style::default().fg(Color::Yellow)),
            Span::raw("Switch directly to tab N"),
        ]),
        Line::from(vec![
            Span::styled("  o            ", Style::default().fg(Color::Yellow)),
            Span::raw("Open a new file or wildcard pattern"),
        ]),
        Line::from(vec![
            Span::styled("  c            ", Style::default().fg(Color::Yellow)),
            Span::raw("Close current tab"),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Space        ", Style::default().fg(Color::LightGreen)),
            Span::raw("Toggle Follow Tail (auto-scroll)"),
        ]),
        Line::from(vec![
            Span::styled("  j / Down     ", Style::default().fg(Color::LightGreen)),
            Span::raw("Scroll 1 line down"),
        ]),
        Line::from(vec![
            Span::styled("  k / Up       ", Style::default().fg(Color::LightGreen)),
            Span::raw("Scroll 1 line up"),
        ]),
        Line::from(vec![
            Span::styled("  d / PgDown   ", Style::default().fg(Color::LightGreen)),
            Span::raw("Scroll page down"),
        ]),
        Line::from(vec![
            Span::styled("  u / PgUp     ", Style::default().fg(Color::LightGreen)),
            Span::raw("Scroll page up"),
        ]),
        Line::from(vec![
            Span::styled("  g / Home     ", Style::default().fg(Color::LightGreen)),
            Span::raw("Jump to beginning of log"),
        ]),
        Line::from(vec![
            Span::styled("  G / End      ", Style::default().fg(Color::LightGreen)),
            Span::raw("Jump to end of log & resume follow"),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  /            ", Style::default().fg(Color::Magenta)),
            Span::raw("Search in buffer (Regex)"),
        ]),
        Line::from(vec![
            Span::styled("  n / F3       ", Style::default().fg(Color::Magenta)),
            Span::raw("Jump to next search match"),
        ]),
        Line::from(vec![
            Span::styled("  N / S-F3     ", Style::default().fg(Color::Magenta)),
            Span::raw("Jump to previous search match"),
        ]),
        Line::from(vec![
            Span::styled("  f            ", Style::default().fg(Color::Cyan)),
            Span::raw("Set Include filter (Regex)"),
        ]),
        Line::from(vec![
            Span::styled("  x            ", Style::default().fg(Color::Red)),
            Span::raw("Set Exclude filter (Regex)"),
        ]),
        Line::from(vec![
            Span::styled("  m            ", Style::default().fg(Color::Blue)),
            Span::raw("Toggle View Mode (TXT / HEX)"),
        ]),
        Line::from(vec![
            Span::styled("  #            ", Style::default().fg(Color::DarkGray)),
            Span::raw("Toggle Line numbers"),
        ]),
        Line::from(vec![
            Span::styled("  ? / F1       ", Style::default().fg(Color::White)),
            Span::raw("Close / Open this help screen"),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Press Esc or Enter to close",
            Style::default().fg(Color::Gray),
        )),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Help ")
        .title_alignment(Alignment::Center)
        .border_style(Style::default().fg(Color::Yellow));

    frame.render_widget(Paragraph::new(help_text).block(block), popup_area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn level_style(level: LogLevel) -> Style {
    match level {
        LogLevel::Fatal | LogLevel::Error => Style::default().fg(Color::LightRed),
        LogLevel::Warn => Style::default().fg(Color::LightYellow),
        LogLevel::Info => Style::default().fg(Color::LightCyan),
        LogLevel::Debug => Style::default().fg(Color::LightBlue),
        LogLevel::Trace => Style::default().fg(Color::DarkGray),
        LogLevel::Unknown => Style::default().fg(Color::White),
    }
}

fn convert_highlight_style(hs: &HighlightStyle) -> Style {
    let mut s = Style::default()
        .fg(Color::Rgb(hs.fg.r(), hs.fg.g(), hs.fg.b()))
        .bg(Color::Rgb(hs.bg.r(), hs.bg.g(), hs.bg.b()));
    if hs.bold {
        s = s.add_modifier(Modifier::BOLD);
    }
    if hs.italic {
        s = s.add_modifier(Modifier::ITALIC);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tui_app_init_empty() {
        let cli = CliArgs::default();
        let config = FastTailConfig::default();
        let app = TuiApp::new(cli, config);
        assert_eq!(app.active_tab, 0);
        assert!(app.engines.is_empty());
        assert!(!app.should_quit);
        assert!(!app.show_help);
    }

    #[test]
    fn test_tui_app_prompts() {
        let cli = CliArgs::default();
        let config = FastTailConfig::default();
        let mut app = TuiApp::new(cli, config);

        apply_prompt(&mut app, PromptKind::Search, "test_query");
        apply_prompt(&mut app, PromptKind::IncludeFilter, "filter_inc");
        apply_prompt(&mut app, PromptKind::ExcludeFilter, "filter_exc");
        apply_prompt(&mut app, PromptKind::GotoLine, "42");
    }

    #[test]
    fn test_tui_app_with_file() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("fasttail_tui_test.log");
        std::fs::write(&file_path, "Line 1\nLine 2 [ERROR] something bad\nLine 3\n").unwrap();

        let mut cli = CliArgs::default();
        cli.paths.push(file_path.clone());
        let config = FastTailConfig::default();
        let mut app = TuiApp::new(cli, config);

        assert_eq!(app.engines.len(), 1);
        assert_eq!(app.active_tab, 0);
        assert_eq!(app.engines[0].total_lines(), 3);

        // Test search
        apply_prompt(&mut app, PromptKind::Search, "ERROR");
        assert!(!app.engines[0].search_matches.is_empty());

        // Test mode switch
        assert_eq!(app.engines[0].view_mode, ViewMode::Text);
        app.engines[0].set_view_mode(ViewMode::Hex);
        assert_eq!(app.engines[0].view_mode, ViewMode::Hex);

        // Test tab close
        app.close_current_tab();
        assert_eq!(app.engines.len(), 0);

        let _ = std::fs::remove_file(file_path);
    }
}
