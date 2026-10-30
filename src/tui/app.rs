// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The terminal app: one `TailEngine` per stream, the key handling and the drawing.
//! Every stream is a bordered window; the status bar, the prompts and the help are
//! bordered too, so the screen reads as a set of windows rather than a text dump.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use crate::actions::ActionId;
use crate::ansi::{AnsiMode, StyleRun};
use crate::collapse::CollapseMode;
use crate::dock_layout::{Dir, Layout as DockLayout, Pane, Tab as DockTab};
use crate::log_level::LogLevel;
use crate::scan_job::ScanKind;
use crate::tail_engine::{QuickLabel, TailEngine, TimeDelta, ViewMode};
use crate::time_range_text::{self, side_readable, Side};
use crate::tui::browser::{self, FileBrowser, Kind};
use crate::tui::calendar::{self, Calendar};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};
use ratatui::Frame;

use crate::find_all::FindState;
use crate::i18n::Language;
use crate::i18n_tui::{en, tx, txf};
use crate::tui::clipboard::{Clipboard, Copied};
use crate::tui::colors::{Chrome, Palette};
use crate::tui::dock::{self, Place, Zone};
use crate::tui::find::{FindWindow, Row as FindRow};
use crate::tui::form::{FieldKey, TextField};
use crate::tui::formats::{FormatsKey, FormatsView};
use crate::tui::global::{self, GlobalDialog, GlobalKey};
use crate::tui::hex;
use crate::tui::json::{JsonDialog, JsonKey};
use crate::tui::keys::{self, Action};
use crate::tui::mouse::{self, DialogHit, Edges, HitMap, MovableDialog, Target, WindowHit};
use crate::tui::palette::{self, CommandPalette, PaletteKey};
use crate::tui::picker::{refusal_text, EntryPicker};
use crate::tui::presets::{self, PresetsDialog, PresetsKey};
use crate::tui::rules::{RuleWidget, RulesDialog, RulesKey};
use crate::tui::settings::SettingsForm;
use crate::tui::tools::{self, MenuKey, ToolMenu, ToolWidget, ToolsDialog, ToolsKey};
use crate::tui::view::{self, Paint};

/// Columns moved by one horizontal scroll step.
/// Characters of a bookmark note the status bar shows.
const NOTE_PREVIEW_CHARS: usize = 40;
/// Cells one `←` / `→` scrolls sideways.
const HSCROLL_STEP: usize = 1;
/// Columns a CTRL + arrow scrolls sideways.
const HSCROLL_FAST: usize = 10;

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
    /// Cells of its window's rows at the last draw.
    pub cells: usize,
    /// Waiting for the first hit of a search that runs in the background.
    pending_first_hit: bool,
    /// Lines already seen while this stream was on screen, for the "new lines" mark.
    seen_lines: usize,
}

impl Tab {
    pub fn new(mut engine: TailEngine) -> Self {
        // Markdown is a GUI view: the terminal shows the lines (or the HEX a binary
        // file opened in).
        if !matches!(
            engine.view_mode,
            ViewMode::Text | ViewMode::Hex | ViewMode::Asm
        ) {
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
            cells: 100,
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

    /// A view of bytes (HEX or ASM): no lines to select, bookmark or filter.
    pub fn is_hex(&self) -> bool {
        self.engine.view_mode.is_bytes()
    }

    pub fn is_asm(&self) -> bool {
        self.engine.view_mode == ViewMode::Asm
    }

    /// Rows of the view: HEX rows, the filtered and collapsed lines, or in ASM the rows
    /// of the screen (the cursor moves on them; the view moves by offset, see
    /// `asm_move`).
    pub fn row_count(&self) -> usize {
        if self.is_asm() {
            self.height.max(1)
        } else if self.is_hex() {
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
            let offset = if self.is_asm() {
                self.asm_cursor_offset()
            } else {
                (self.cursor * n) as u64
            };
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

    /// `d`: from HEX or the lines to the ASM view (at the HEX cursor row's bytes, or at
    /// the current search hit, else where it was or the entry point), and from ASM back
    /// to HEX at the cursor row's bytes. Follow stays.
    fn toggle_asm(&mut self) {
        let follow = self.engine.follow_tail;
        let n = self.hex_width.max(1);
        if self.is_asm() {
            let offset = self.asm_cursor_offset();
            self.engine.set_view_mode(ViewMode::Hex);
            self.engine.scroll_to_byte = None;
            if !follow {
                self.set_cursor(offset as usize / n);
            }
        } else {
            if self.engine.view_mode == ViewMode::Hex {
                self.engine.hex_top_offset = (self.cursor * n) as u64;
            }
            self.engine.set_view_mode(ViewMode::Asm);
            self.engine.scroll_to_byte = None;
            self.top = 0;
            self.cursor = 0;
        }
        self.engine.follow_tail = follow;
        self.pin_cursor();
    }

    /// The offset of the ASM row under the cursor.
    fn asm_cursor_offset(&mut self) -> u64 {
        let rows = self.engine.asm_rows(self.height.max(1));
        rows.get(self.cursor)
            .or(rows.last())
            .map_or(self.engine.asm.top, |r| r.offset)
    }

    /// Moves the ASM cursor `n` rows (negative: up), scrolling the view at its edges;
    /// `select` extends the selection from where it started (or the cursor row).
    fn asm_move(&mut self, n: isize, select: bool) {
        self.engine.follow_tail = false;
        let anchor = self.asm_cursor_offset();
        let height = self.height.max(1) as isize;
        let target = self.cursor as isize + n;
        if target < 0 {
            self.engine.asm_scroll(target);
            self.cursor = 0;
        } else if target >= height {
            self.engine.asm_scroll(target - height + 1);
            self.cursor = height as usize - 1;
        } else {
            self.cursor = target as usize;
        }
        // Never below the last row of a short file.
        let shown = self.engine.asm_rows(self.height.max(1)).len();
        self.cursor = self.cursor.min(shown.saturating_sub(1));
        if select {
            let from = self.engine.asm.selection.map_or(anchor, |(a, _)| a);
            let to = self.asm_cursor_offset();
            self.engine.asm.selection = Some((from, to));
        }
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
    if let Some(title) = engine.source_title() {
        return title;
    }
    if engine.merged.is_some() || engine.derived.is_some() {
        return crate::find_all::stream_name(engine);
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
    /// The file to save the open streams to as a session.
    SaveSession,
    /// The bytes of a line the stream's level is looked for in (empty: the setting).
    LevelBytes,
    /// How to split the stream's file (`100000`, `10mb`, `1h`, `1d`, `30m`).
    Split,
    /// Open system source (palette): a `docker://` or `journald://` URI.
    System,
    /// Open remote file (palette): an `ssh://` URL.
    Remote,
    /// Listen (palette): a `tcp://`, `udp://` or `syslog://` URL.
    Listen,
    DebugOutput,
    /// Open Event Log (palette, Windows): an `eventlog://` URI.
    EventLog,
    /// The part of the view the search covers: `1200-5000` or `14:00..15:00`.
    SearchScope,
    /// The file an export of these lines, in this format, is written to.
    Export(crate::export::Scope, crate::export::Format),
    /// The file the bookmark report is saved to (empty: the clipboard); `true` for every
    /// stream.
    Report(bool),
    /// A rule set file to add to the rules (a trailing `!` replaces them).
    RulesImport,
    /// The file the rules are written to (a trailing `!` replaces it).
    RulesExport,
    /// A format file to copy into the formats folder.
    FormatImport,
    /// The file the format being edited is written to.
    FormatExport,
    /// The field (or `/regex/`) the Statistics count.
    StatsField,
    /// The field (or `/regex/`) whose values group the Operations.
    OpsField,
    /// The span Explain compares with the rest: `14:00..14:05`.
    ExplainSpan,
    /// A source plugin to open, with its values: `plugin://name?key=value`.
    OpenPlugin,
}

impl PromptKind {
    /// A prompt opened from a dialog (the rules, the formats): drawn over it and given
    /// the keys before it.
    fn over_dialogs(&self) -> bool {
        matches!(
            self,
            PromptKind::RulesImport
                | PromptKind::RulesExport
                | PromptKind::FormatImport
                | PromptKind::FormatExport
                | PromptKind::OpenPlugin
        )
    }
}

pub struct Prompt {
    pub kind: PromptKind,
    pub field: TextField,
}

/// The Plugins dialog: the plugins found, the row selected (the plugins, then the folders
/// that did not load), and the source plugin whose approval waits for `y`.
pub struct PluginsDialog {
    pub catalog: crate::plugins::Catalog,
    pub selected: usize,
    pub confirm: Option<String>,
}

/// The Keys dialog: the row of `keys::REMAPPABLE` selected, and whether the next key
/// pressed is recorded for it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeysDialog {
    pub selected: usize,
    pub top: usize,
    pub capturing: bool,
    pub notice: Option<String>,
}

/// The dialog after the graphical interface was picked in Settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterfaceSwitch {
    /// "Switch now" (`Enter`, `[ OK ]`) or "At next start" (`Esc`, `[ Cancel ]`).
    Offer,
    /// The window did not start: why.
    Failed(String),
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
    /// The side being edited: its field, its calendar and its time.
    pub on_to: bool,
    pub zone: RangeZone,
    pub calendar: Calendar,
    /// In the time zone: the minutes are selected rather than the hours.
    pub on_minutes: bool,
    pub invalid: bool,
    /// What a bare `14:02` is read against: the log's first timestamp.
    pub reference: i64,
}

/// The part of the time range dialog with the keyboard. `Tab` walks From, its
/// calendar, its time, then To, its calendar, its time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeZone {
    Field,
    Calendar,
    Time,
}

/// List positions of the time range dialog's clickable parts (see `mouse::Target`).
const RANGE_FROM: usize = 0;
const RANGE_TO: usize = 1;
const RANGE_DAY: usize = 1_000_000;

impl TimeRangeDialog {
    fn focused(&mut self) -> &mut TextField {
        if self.on_to {
            &mut self.to
        } else {
            &mut self.from
        }
    }

    fn side(&self) -> Side {
        if self.on_to {
            Side::To
        } else {
            Side::From
        }
    }

    fn side_text(&self) -> &str {
        if self.on_to {
            self.to.text()
        } else {
            self.from.text()
        }
    }

    /// The calendar on the edited side's day, or on the log's first day.
    fn sync_calendar(&mut self) {
        let day = time_range_text::parse(self.side_text(), self.reference)
            .map(time_range_text::day_of)
            .unwrap_or_else(|| time_range_text::day_of(self.reference));
        self.calendar = Calendar::new(day);
    }

    /// The day under the calendar cursor into the edited side, keeping its time.
    fn pick_day(&mut self) {
        let text =
            time_range_text::pick_day(self.side_text(), self.reference, self.calendar.cursor);
        *self.focused() = TextField::new(&text);
        self.invalid = false;
    }

    /// Hours and minutes of the edited side moved by `hours` / `minutes` (wrapping
    /// within the day), on the calendar's day when the side names none.
    fn step_time(&mut self, hours: i64, minutes: i64) {
        let (h, m, s) = time_range_text::clock_of(self.side_text(), self.reference, self.side());
        let h = (h as i64 + hours).rem_euclid(24) as u32;
        let m = (m as i64 + minutes).rem_euclid(60) as u32;
        let text = time_range_text::with_clock(
            self.side_text(),
            self.reference,
            self.calendar.cursor,
            (h, m, s),
        );
        *self.focused() = TextField::new(&text);
        self.invalid = false;
    }

    /// `Tab` / `Shift+Tab` through the six stops.
    fn next_stop(&mut self, back: bool) {
        let zone = match self.zone {
            RangeZone::Field => 0,
            RangeZone::Calendar => 1,
            RangeZone::Time => 2,
        };
        let at = usize::from(self.on_to) * 3 + zone;
        let next = if back { (at + 5) % 6 } else { (at + 1) % 6 };
        self.on_to = next >= 3;
        self.zone = match next % 3 {
            0 => RangeZone::Field,
            1 => RangeZone::Calendar,
            _ => RangeZone::Time,
        };
        if self.zone == RangeZone::Calendar {
            self.sync_calendar();
        }
    }
}

/// A floating window: one leaf of tabs lying over the dock, at `rect` (screen cells;
/// kept inside the windows' area when drawn).
#[derive(Debug, Clone, PartialEq)]
pub struct Float {
    pub pane: Pane,
    pub rect: Rect,
}

/// Points per cell when a floating window's place goes to (or comes from) the GUI's
/// layout, which counts in points: about a monospace cell at the GUI's font size.
const POINTS_PER_CELL: (f32, f32) = (8.0, 16.0);

/// What the left button is dragging in the dock.
#[derive(Debug, Clone, PartialEq)]
enum DockDrag {
    /// The divider of the split at `path`, which covers `split`.
    Divider {
        path: Vec<bool>,
        dir: Dir,
        split: Rect,
    },
    /// A stream taken by its title at `from`; `drop` is the leaf, zone and preview
    /// under the pointer once it has moved.
    Move {
        stream: PathBuf,
        from: (u16, u16),
        drop: Option<(Vec<bool>, Zone, Rect)>,
    },
    /// The topmost floating window, taken by its title `grab` cells from its corner.
    FloatMove { grab: (u16, u16) },
    /// The topmost floating window, taken by a corner or an edge: the sides that move,
    /// and the window when it was taken (the other sides stay where they were).
    FloatResize { edges: Edges, from: Rect },
    /// Settings or Keys taken by its title `grab` cells from its corner.
    DialogMove {
        which: MovableDialog,
        grab: (u16, u16),
    },
    /// Settings or Keys taken by a corner or an edge, as `FloatResize`.
    DialogResize {
        which: MovableDialog,
        edges: Edges,
        from: Rect,
    },
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
    plot: Option<(u64, u16)>,
}

#[derive(Debug, Clone, PartialEq, Default)]
struct Signature {
    active: usize,
    dock: Option<Pane>,
    floats: Vec<Float>,
    panes: Vec<PaneSignature>,
    unseen_tabs: usize,
}

pub struct App {
    pub tabs: Vec<Tab>,
    /// The stream with the keyboard focus.
    pub active: usize,
    /// The windows on screen, as the GUI's dock: a tree whose leaves hold the streams'
    /// paths (and the GUI's own panels, drawn as placeholders).
    pub dock: Pane,
    /// The `[dock] layout` read at start or from a session: its floating windows are
    /// kept when the tree is saved back.
    dock_layout: Option<DockLayout>,
    /// The tree changed since it was last saved.
    dock_dirty: bool,
    dock_drag: Option<DockDrag>,
    /// Floating windows over the dock, bottom to top (the GUI's floating windows).
    pub floats: Vec<Float>,
    pub palette: Palette,
    pub prompt: Option<Prompt>,
    /// A split being written (the tab's stream path and the job), or waiting for the
    /// stream's timestamps before it starts.
    pub split: Option<(PathBuf, crate::split::SplitJob)>,
    pub split_waiting: Option<(PathBuf, crate::split::SplitBy)>,
    pub time_range: Option<TimeRangeDialog>,
    pub picker: Option<EntryPicker>,
    /// The Open dialog (`o`).
    pub browser: Option<FileBrowser>,
    pub sessions: Option<SessionDialog>,
    /// The Settings dialog (`,`).
    pub settings_form: Option<SettingsForm>,
    /// The configuration when Settings opened: Cancel goes back to it (every change
    /// applies at once, as in the GUI).
    settings_before: Option<crate::config::FastTailConfig>,
    /// The highlight-rule editor (`r`).
    pub rules: Option<RulesDialog>,
    /// The Log formats dialog (palette).
    pub formats: Option<FormatsView>,
    /// Two lines or regions side by side (`=`).
    pub compare: Option<crate::tui::compare::CompareView>,
    /// The lines marked to be compared with the next selection.
    pub compare_mark: Option<crate::tui::compare::Side>,
    /// The session's scratchpad, loaded when first used.
    pub scratchpad: Option<crate::scratchpad::Scratchpad>,
    /// The scratchpad's editor, while it is shown (`Alt+N`).
    pub pad_editor: Option<crate::tui::pad::PadEditor>,
    /// An analysis view over the windows (`Alt+A`).
    pub analysis: Option<crate::tui::analysis::AnalysisView>,
    /// The list of the analysis views (`Alt+A`): the row selected.
    pub analysis_menu: Option<usize>,
    /// The merged view dialog (`Alt+M`).
    pub merge: Option<crate::tui::merge::MergeDialog>,
    /// The recent files list (palette): the row selected.
    pub recent_menu: Option<usize>,
    /// The stream whose information is shown (`Alt+I`).
    pub info_view: Option<PathBuf>,
    /// PgUp / PgDn held down, for their acceleration.
    page_hold: keys::PageHold,
    /// A merged view has lines left to merge: the loop polls faster.
    views_busy: bool,
    /// The terminal keys in force: the defaults with `[shortcuts]`'s `tui.` entries.
    pub keymap: keys::KeyMap,
    /// The Keys dialog (palette): the remappable keys.
    pub keys_dialog: Option<KeysDialog>,
    /// The read-only web view of the open streams, when `[web]` is on.
    pub web_view: crate::web_view::WebView,
    /// The MCP server for AI assistants, when `[mcp]` is on.
    pub mcp_view: crate::mcp_serve::McpView,
    /// The Plugins dialog (palette).
    pub plugins: Option<PluginsDialog>,
    /// The filter presets (`p`).
    pub presets: Option<PresetsDialog>,
    /// The global filter editor (`F`), and when its pending edit is applied.
    pub global: Option<GlobalDialog>,
    global_due: Option<Instant>,
    /// The external tools menu (`!`).
    pub tool_menu: Option<ToolMenu>,
    /// The list of the lines hidden by hand in the active stream: the range selected.
    pub hidden_menu: Option<usize>,
    /// The pieces list of a stream with rotations (`R`): the selected row.
    pub piece_menu: Option<usize>,
    /// The list of the active stream's fields for the column view: the row selected.
    pub columns_menu: Option<usize>,
    /// The search results of the active stream as a list (`Alt+R`): the hit selected.
    pub results_menu: Option<usize>,
    /// What to export (`Alt+E`): the entry selected of `EXPORT_CHOICES`.
    pub export_menu: Option<usize>,
    /// An export being written: the stream's path and the job, stepped by `tick`.
    pub export: Option<(PathBuf, crate::export::ExportJob)>,
    /// The external tools editor (from Settings, or `e` in the menu).
    pub tools_editor: Option<ToolsDialog>,
    /// The command palette (`:`).
    pub palette_dialog: Option<CommandPalette>,
    /// Spawns the external tools and keeps the limits of the rule-bound runs.
    tool_runner: crate::external_tools::ToolRunner,
    /// Desktop notifications of the rules with "Notify" (a click focuses the stream).
    notifications: crate::notifications::Center,
    /// The PIN lock screen while locked: the PIN being typed and the last notice.
    pub locked: Option<LockScreen>,
    /// Wrong PINs and their cooldown, shared rules with the GUI (memory only).
    lock_attempts: crate::lock::LockAttempts,
    /// Time since the last key or mouse event, for the idle lock.
    idle: crate::lock::IdleClock,
    /// A session file that exists, waiting for `[ OK ]` to be overwritten.
    pub confirm_overwrite: Option<PathBuf>,
    /// The graphical interface was just picked in Settings: switch now or at next start,
    /// then why it could not start when that failed.
    pub interface_switch: Option<InterfaceSwitch>,
    /// Starts the window for "Switch now": `handoff::start_gui`, a stub in tests so that
    /// no test ever starts a real FastTail (it would read the ini of its directory).
    pub start_gui: fn() -> Result<(), crate::handoff::StartError>,
    /// The configuration new streams are set up with (none in tests and benchmarks:
    /// the defaults).
    pub settings: Option<crate::tui::workspace::Settings>,
    /// The row and the index of the source location `Ctrl+O` opened last.
    source_cycle: Option<(usize, usize)>,
    /// Saves the configuration every `SAVE_EVERY` when it changed (the interactive run;
    /// not captures, benchmarks or tests).
    pub autosave: bool,
    last_save: Instant,
    /// The last save error shown, so a failing save is reported once, not every 2 s.
    save_error: Option<String>,
    pub message: Option<String>,
    pub show_help: bool,
    /// The About window, as the GUI's.
    pub show_about: bool,
    /// First line of the help shown (it scrolls on a short screen).
    pub help_top: usize,
    /// The help entry under the cursor, and the rows of its first column when it is
    /// drawn in two (0 in one column).
    pub help_sel: usize,
    help_half: usize,
    /// Where Settings and Keys were moved or resized to, for this run (`None`: centred).
    settings_rect: Option<Rect>,
    help_rect: Option<Rect>,
    /// The last click on a movable dialog's title, to tell a double click.
    last_title_click: Option<Instant>,
    pub quit: bool,
    /// The JSON tree of a row (`J`).
    pub json_dialog: Option<JsonDialog>,
    /// Search all streams (`Ctrl+Shift+F`, `Alt+/`): the Find results window.
    pub find_window: Option<FindWindow>,
    /// Rows of its list as last drawn (the page of PgUp / PgDn).
    find_rows: usize,
    /// Windows that just closed, drawn as an outline shrinking to their centre for
    /// `CLOSE_EFFECT` (the place each had, and when it closed).
    pub closing: Vec<(Rect, Instant)>,
    /// A stream not on screen matched a rule with a sound alert (`flash_on_alert`): the
    /// loop rings the terminal bell once (`take_bell`).
    bell: bool,
    /// The bell rang for the alerts now waiting; it rings again after they were seen.
    bell_rung: bool,
    /// Mouse capture is on (the help says how to select text natively).
    pub mouse: bool,
    /// How often the files are read (the ini's `poll_interval_ms`).
    pub idle_poll: Duration,
    last_engine_poll: Option<Instant>,
    /// Clickable rectangles of the last frame.
    pub hits: HitMap,
    /// The language of the texts (`fasttail.ini`'s, English without a configuration).
    pub lang: Language,
    /// CPU and memory for the top bar (`telemetry_enabled`), read once a second.
    telemetry: Telemetry,
    clipboard: Clipboard,
    /// Last left click on a row (when, stream, line), to tell a double click.
    last_click: Option<(Instant, usize, usize)>,
    /// Stream whose rows a left-button drag is selecting.
    drag: Option<usize>,
    last_signature: Signature,
    /// A count typed before a move (`12j`, `3e`).
    count: Option<u32>,
    /// Quick labels (CTRL + SHIFT + 1..9, or `L` and a digit), in every stream; in
    /// memory only, as in the GUI.
    pub quick_labels: Vec<QuickLabel>,
    /// `L` was pressed: the next digit is a label colour.
    label_pending: bool,
}

/// What an empty workspace says.
const NO_FILE: &str = en("No file open: o opens one, Ctrl+O a session, q quits");

/// The lock screen: the PIN being typed (shown as `*`) and the last notice.
#[derive(Debug, Default)]
pub struct LockScreen {
    pub field: TextField,
    pub notice: String,
}

/// How often the configuration is saved while it changes, as in the GUI.
const SAVE_EVERY: Duration = Duration::from_secs(2);

/// Two clicks on the same row within this time are a double click.
const DOUBLE_CLICK: Duration = Duration::from_millis(400);
/// Rows moved by one wheel step, as in the GUI.
const WHEEL_ROWS: usize = 3;
/// The largest count before a move.
const MAX_COUNT: u32 = 99_999;
/// Share of a split one `Alt+arrow` moves its divider.
const RESIZE_STEP: f32 = 0.05;

impl App {
    pub fn new(tabs: Vec<Tab>, palette: Palette) -> Self {
        let paths: Vec<PathBuf> = tabs.iter().map(|t| t.engine.path.clone()).collect();
        Self {
            tabs,
            active: 0,
            dock: Pane::with_streams(&paths),
            dock_layout: None,
            dock_dirty: false,
            dock_drag: None,
            floats: Vec::new(),
            palette,
            prompt: None,
            split: None,
            split_waiting: None,
            time_range: None,
            picker: None,
            browser: None,
            sessions: None,
            settings_form: None,
            settings_before: None,
            rules: None,
            formats: None,
            compare: None,
            compare_mark: None,
            scratchpad: None,
            pad_editor: None,
            analysis: None,
            analysis_menu: None,
            merge: None,
            recent_menu: None,
            info_view: None,
            page_hold: keys::PageHold::default(),
            views_busy: false,
            keymap: keys::KeyMap::default(),
            keys_dialog: None,
            web_view: Default::default(),
            mcp_view: Default::default(),
            plugins: None,
            presets: None,
            global: None,
            global_due: None,
            tool_menu: None,
            hidden_menu: None,
            piece_menu: None,
            columns_menu: None,
            results_menu: None,
            export_menu: None,
            export: None,
            tools_editor: None,
            palette_dialog: None,
            locked: None,
            lock_attempts: Default::default(),
            idle: crate::lock::IdleClock::new(Instant::now()),
            tool_runner: Default::default(),
            notifications: crate::notifications::Center::new(std::sync::Arc::new(|| {})),
            confirm_overwrite: None,
            interface_switch: None,
            start_gui: if cfg!(test) {
                || Err(crate::handoff::StartError::NotInBuild)
            } else {
                crate::handoff::start_gui
            },
            settings: None,
            source_cycle: None,
            autosave: false,
            last_save: Instant::now(),
            save_error: None,
            message: None,
            show_help: false,
            help_top: 0,
            help_sel: 0,
            help_half: 0,
            settings_rect: None,
            help_rect: None,
            last_title_click: None,
            quit: false,
            bell: false,
            json_dialog: None,
            find_window: None,
            find_rows: 10,
            closing: Vec::new(),
            show_about: false,
            bell_rung: false,
            mouse: true,
            idle_poll: Duration::from_millis(250),
            last_engine_poll: None,
            hits: HitMap::default(),
            lang: Language::En,
            telemetry: Telemetry::default(),
            clipboard: Clipboard::default(),
            last_click: None,
            drag: None,
            last_signature: Signature::default(),
            count: None,
            quick_labels: Vec::new(),
            label_pending: false,
        }
    }

    /// Streams on screen, the focused one first.
    pub fn visible_tabs(&self) -> Vec<usize> {
        if self.tabs.is_empty() {
            return Vec::new();
        }
        let mut out = vec![self.active];
        for place in self.places() {
            if let Some(i) = self.shown_at(&place) {
                if !out.contains(&i) {
                    out.push(i);
                }
            }
        }
        out
    }

    /// Every window: the dock's leaves in order, then the floating ones bottom to top.
    fn places(&self) -> Vec<Place> {
        let mut out: Vec<Place> = self
            .dock
            .leaf_paths()
            .into_iter()
            .map(Place::Dock)
            .collect();
        out.extend((0..self.floats.len()).map(Place::Float));
        out
    }

    /// The leaf of tabs at `place`.
    fn leaf(&self, place: &Place) -> Option<&Pane> {
        match place {
            Place::Dock(path) => self.dock.get(path),
            Place::Float(i) => self.floats.get(*i).map(|f| &f.pane),
        }
    }

    /// The stream the leaf at `path` shows (none for a GUI panel).
    fn shown_in(&self, path: &[bool]) -> Option<usize> {
        self.shown_at(&Place::Dock(path.to_vec()))
    }

    /// The stream the window at `place` shows (none for a GUI panel).
    fn shown_at(&self, place: &Place) -> Option<usize> {
        match self.leaf(place) {
            Some(Pane::Leaf { tabs, active }) => tabs
                .get(*active)
                .and_then(DockTab::stream)
                .and_then(|p| self.tab_index(p)),
            _ => None,
        }
    }

    /// The window holding stream `path`.
    fn place_of(&self, path: &Path) -> Option<Place> {
        if let Some(i) = self
            .floats
            .iter()
            .position(|f| f.pane.find_stream(path).is_some())
        {
            return Some(Place::Float(i));
        }
        self.dock.find_stream(path).map(Place::Dock)
    }

    /// The window of the focused stream.
    fn focused_place(&self) -> Place {
        self.place_of(&self.active_path())
            .unwrap_or(Place::Dock(Vec::new()))
    }

    /// The streams of the floating windows.
    fn float_streams(&self) -> Vec<PathBuf> {
        self.floats.iter().flat_map(|f| f.pane.streams()).collect()
    }

    /// The windows' area of the last frame (a default before the first one).
    fn main_area(&self) -> Rect {
        if self.hits.main.area() > 0 {
            self.hits.main
        } else {
            Rect::new(0, 1, 100, 30)
        }
    }

    /// The stream open on `path`.
    fn tab_index(&self, path: &Path) -> Option<usize> {
        self.tabs
            .iter()
            .position(|t| crate::paths::paths_equal(&t.engine.path, path))
    }

    fn open_paths(&self) -> Vec<PathBuf> {
        self.tabs.iter().map(|t| t.engine.path.clone()).collect()
    }

    /// Takes the tree of `layout` (`[dock] layout`, the configuration's or a
    /// session's), in step with the open streams as the GUI does; without one every
    /// stream is a tab of one window.
    pub fn restore_dock(&mut self, layout: Option<&str>) {
        let open = self.open_paths();
        self.dock_layout = layout.and_then(DockLayout::parse);
        // The merged views are saved under their `merged:` name: their tabs show the spool.
        let tabs = &self.tabs;
        if let Some(l) = self.dock_layout.as_mut() {
            l.rename_streams(&|p| {
                tabs.iter()
                    .find(|t| crate::workspace::saved_identity(&t.engine).as_deref() == Some(p))
                    .map(|t| t.engine.path.clone())
            });
        }
        // The floating windows keep the open streams they hold (nothing is added).
        self.floats = Vec::new();
        let windows = self
            .dock_layout
            .as_ref()
            .map(DockLayout::windows)
            .unwrap_or_default();
        for (k, (pane, place)) in windows.into_iter().enumerate() {
            let Some(pane) = pane.reconcile(&open, &open) else {
                continue;
            };
            let rect = match place {
                Some((x, y, w, h)) => Rect::new(
                    (x / POINTS_PER_CELL.0).round().max(0.0) as u16,
                    (y / POINTS_PER_CELL.1).round().max(0.0) as u16,
                    (w / POINTS_PER_CELL.0).round() as u16,
                    (h / POINTS_PER_CELL.1).round() as u16,
                ),
                None => self.cascade(k),
            };
            self.floats.push(Float { pane, rect });
        }
        let floating = self.float_streams();
        let docked: Vec<PathBuf> = open
            .iter()
            .filter(|p| !floating.iter().any(|f| crate::paths::paths_equal(f, p)))
            .cloned()
            .collect();
        let main = self
            .dock_layout
            .as_ref()
            .and_then(DockLayout::main_pane)
            .and_then(|p| p.reconcile(&open, &floating));
        // Every stream floating (or none open) leaves the dock empty, as in the GUI.
        self.dock = main.unwrap_or_else(|| Pane::with_streams(&docked));
        self.focus_tab(self.active);
        self.dock_dirty = false;
    }

    /// The place of the `k`-th floating window with no place of its own: stepped down
    /// and right from the middle of the windows' area.
    fn cascade(&self, k: usize) -> Rect {
        let main = self.main_area();
        let step = (k as u16 % 8) * 2;
        dock::float_rect_at(main, main.x + main.width / 2 + step, main.y + 2 + step / 2)
    }

    /// `--split`: the first two streams side by side (nothing when the restored layout
    /// already has several windows).
    pub fn split_first_two(&mut self) {
        if self.tabs.len() > 1 && self.dock.leaf_paths().len() == 1 {
            let second = self.tabs[1].engine.path.clone();
            self.dock = self
                .dock
                .clone()
                .split_with(&[], Dir::Horizontal, true, &second);
        }
    }

    /// The tree as `[dock] layout`: standard input left out, the floating windows of
    /// the layout it came from kept (their streams leave the main surface); an empty
    /// dock is an empty main surface.
    fn dock_ron(&self) -> String {
        let stdin: Vec<PathBuf> = self
            .tabs
            .iter()
            .filter(|t| {
                t.engine.is_stdin()
                    || (t.engine.merged.is_some() && !crate::workspace::is_persisted(&t.engine))
            })
            .map(|t| t.engine.path.clone())
            .collect();
        let without_stdin =
            |pane: Pane| stdin.iter().try_fold(pane, |pane, s| pane.remove_stream(s));
        let pane = without_stdin(self.dock.clone()).unwrap_or_else(|| Pane::with_streams(&[]));
        let windows: Vec<(Pane, crate::dock_layout::Placement)> = self
            .floats
            .iter()
            .filter_map(|f| {
                let (px, py) = POINTS_PER_CELL;
                let r = f.rect;
                let place = (
                    r.x as f32 * px,
                    r.y as f32 * py,
                    r.width as f32 * px,
                    r.height as f32 * py,
                );
                Some((without_stdin(f.pane.clone())?, place))
            })
            .collect();
        let mut layout = match &self.dock_layout {
            Some(saved) => saved.clone(),
            None => DockLayout::from_pane(&pane),
        };
        layout.set_main_pane(&pane);
        layout.set_windows(&windows);
        // A merged view is saved under its `merged:` name, as in the GUI.
        layout.rename_streams(&|p| {
            self.tabs
                .iter()
                .find(|t| t.engine.path == p)
                .and_then(|t| crate::workspace::saved_identity(&t.engine))
        });
        layout.to_ron()
    }

    fn set_dock(&mut self, dock: Pane) {
        if dock != self.dock {
            self.dock = dock;
            self.dock_dirty = true;
        }
    }

    /// The focused stream's path (empty with no stream open).
    fn active_path(&self) -> PathBuf {
        self.tabs
            .get(self.active)
            .map(|t| t.engine.path.clone())
            .unwrap_or_default()
    }

    /// The path of the leaf of the focused stream.
    fn focused_leaf(&self) -> Vec<bool> {
        self.dock
            .find_stream(&self.active_path())
            .unwrap_or_default()
    }

    /// Polls every engine (hidden streams keep tailing), applies what the engines ask the
    /// view to do, and says whether the screen needs a redraw.
    /// Starts the split typed in the prompt, or waits for the timestamps it needs.
    fn submit_split(&mut self, text: &str) {
        let Some((by, replace)) = crate::split::parse_spec(text) else {
            self.message = Some(txf(self.lang, "Cannot split by \"{0}\"", &[&text]));
            return;
        };
        let tab = &mut self.tabs[self.active];
        let path = tab.engine.path.clone();
        if matches!(by, crate::split::SplitBy::Time(_)) && !tab.engine.timestamps_complete() {
            tab.engine.ensure_timestamps();
            self.split_waiting = Some((path, by));
            self.message = Some(tx(self.lang, "Split: reading the timestamps...").into());
            return;
        }
        self.start_split(path, by, replace);
    }

    fn start_split(&mut self, path: PathBuf, by: crate::split::SplitBy, replace: bool) {
        let Some(tab) = self.tabs.iter().find(|t| t.engine.path == path) else {
            return;
        };
        let file = tab
            .engine
            .current_file
            .clone()
            .unwrap_or_else(|| tab.engine.path.clone());
        let plan = crate::split::SplitPlan::for_file(&file, by);
        let preview = crate::split::preview(&tab.engine, &plan);
        if preview.no_timestamps {
            self.message = Some(tx(self.lang, "No timestamps: cannot split by time").into());
            return;
        }
        if !preview.existing.is_empty() && !replace {
            self.message = Some(txf(
                self.lang,
                "{0} files exist already: add ! to replace them",
                &[&preview.existing.len()],
            ));
            return;
        }
        let job = crate::split::SplitJob::start(&tab.engine, plan);
        self.split = Some((path, job));
    }

    /// Runs the split a slice per tick, and starts one that waited for the timestamps.
    fn step_split(&mut self) {
        if let Some((path, by)) = self.split_waiting.clone() {
            let ready = self
                .tabs
                .iter()
                .find(|t| t.engine.path == path)
                .map(|t| t.engine.timestamps_complete());
            match ready {
                Some(true) => {
                    self.split_waiting = None;
                    self.start_split(path, by, false);
                }
                Some(false) => {}
                None => self.split_waiting = None,
            }
        }
        let Some((path, mut job)) = self.split.take() else {
            return;
        };
        let Some(tab) = self.tabs.iter().find(|t| t.engine.path == path) else {
            job.cancel();
            return;
        };
        if job.step(&tab.engine, Duration::from_millis(20)) {
            self.message = Some(match &job.error {
                Some(e) => txf(self.lang, "Split failed: {0}", &[e]),
                None => txf(
                    self.lang,
                    "{0} files written in {1}",
                    &[&job.written.len(), &job.plan.dir.display()],
                ),
            });
        } else {
            self.message = Some(txf(
                self.lang,
                "Split: {0}%",
                &[&format!("{:.0}", job.progress() * 100.0)],
            ));
            self.split = Some((path, job));
        }
    }

    pub fn tick(&mut self) -> bool {
        self.step_split();
        self.step_export();
        self.step_rule_seeks();
        if let Some(pad) = self.scratchpad.as_mut() {
            pad.save_if_due();
        }
        self.serve_integrations();
        // The merged views take their sources' new lines in time order.
        let mut engines: Vec<&mut TailEngine> =
            self.tabs.iter_mut().map(|t| &mut t.engine).collect();
        self.views_busy = crate::filter_tab::feed_tabs(&mut engines, MERGE_BUDGET)
            | crate::merged_stream::feed_views(&mut engines, MERGE_BUDGET);
        self.apply_source_context();
        // The files are read every `idle_poll` (the ini's `poll_interval_ms`), and at
        // every tick while background work runs, whose results arrive through the poll.
        let due = self.busy()
            || self
                .last_engine_poll
                .is_none_or(|at| at.elapsed() >= self.idle_poll);
        if due {
            self.last_engine_poll = Some(Instant::now());
            // The engine counts what arrives while a stream is not on screen, and how
            // severe it is (2: a rule with a sound alert), as in the GUI's tab bar.
            let shown = self.visible_tabs();
            let commands = self
                .settings
                .as_ref()
                .map(|s| s.config.system_commands())
                .unwrap_or_default();
            for (i, tab) in self.tabs.iter_mut().enumerate() {
                if let Some(err) = tab
                    .engine
                    .apply_reconnect_request(&commands)
                    .or_else(|| tab.engine.apply_ssh_request())
                {
                    self.message = Some(txf(self.lang, "Reconnect failed: {0}", &[&err]));
                }
                tab.engine.displayed = shown.contains(&i);
                tab.engine.poll_updates();
                if tab.engine.displayed {
                    tab.engine.mark_seen();
                }
            }
            self.check_alerts(&shown);
        }
        let found = self.poll_find();
        // Without a configuration (tests, the benchmark) there are no meters.
        let telemetry_on = self
            .settings
            .as_ref()
            .is_some_and(|s| s.config.telemetry_enabled);
        let telemetry = telemetry_on && self.telemetry.refresh();
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
                    self.message = Some(txf(
                        self.lang,
                        "Not found: {0}",
                        &[&tab.engine.search_query],
                    ));
                }
            }
            if let Some(result) = tab.engine.take_goto_time_result() {
                self.message = goto_target(tab, result, tx(self.lang, "that time"), self.lang);
            }
            if tab.engine.hidden_cleared {
                tab.engine.hidden_cleared = false;
                self.message = Some(
                    tx(
                        self.lang,
                        "The file was rewritten: the hidden lines are shown again",
                    )
                    .into(),
                );
            }
            // The engine's notices (a line past the end, a go-to line the filters hide)
            // go to the status bar: the terminal has no notice row.
            if let Some(notice) = tab.engine.view_notice.take() {
                self.message = Some(notice);
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
                if tab.is_asm() {
                    tab.engine.asm.top = offset as u64;
                    tab.engine.follow_tail = false;
                    tab.cursor = 0;
                } else if tab.is_hex() {
                    tab.set_cursor(offset / tab.hex_width.max(1));
                }
            }
            tab.seen_lines = tab.engine.total_lines();
        }
        if due {
            self.run_bound_tools();
            self.run_notifications();
        }
        // The idle lock, armed by a PIN, `lock_enabled` and the idle minutes.
        let idle_locked = self.locked.is_none()
            && self
                .settings
                .as_ref()
                .is_some_and(|s| self.idle.idle_lock_due(Instant::now(), &s.config));
        if idle_locked {
            self.lock();
        }
        let global_applied = self.global_due.is_some_and(|due| Instant::now() >= due);
        if global_applied {
            self.apply_pending_global();
        }
        if self.autosave && self.last_save.elapsed() >= SAVE_EVERY {
            self.save_config();
        }
        let sig = self.signature();
        let changed = sig != self.last_signature;
        self.last_signature = sig;
        let animating = !self.closing.is_empty();
        self.closing.retain(|(_, at)| at.elapsed() < CLOSE_EFFECT);
        changed || listed || found || global_applied || idle_locked || telemetry || animating
    }

    /// Background work in flight: the loop then polls a little faster.
    pub fn busy(&self) -> bool {
        // A closing effect runs at the busy tick, so its frames are drawn.
        !self.closing.is_empty()
            || self.tabs.iter().any(|t| {
                t.engine.scan_progress().is_some()
                    || t.engine.index_pending
                    || t.engine.compressed.as_ref().is_some_and(|c| c.is_running())
            })
            || self.picker.as_ref().is_some_and(|p| p.scan.is_some())
            || self.split.is_some()
            || self.split_waiting.is_some()
            || self.export.is_some()
            || self.analysis.is_some()
            || self.views_busy
            || self.mcp_view.busy()
            || self
                .tabs
                .iter()
                .any(|t| t.engine.rule_seek_pattern().is_some())
            || self
                .find_window
                .as_ref()
                .is_some_and(|w| w.session.is_active())
            || self
                .tabs
                .iter()
                .any(|t| t.engine.plot.as_ref().is_some_and(|p| p.running()))
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
                plot: e
                    .plot
                    .as_ref()
                    .map(|p| (p.generation, (p.progress * 1000.0) as u16)),
            }
        };
        Signature {
            active: self.active,
            dock: Some(self.dock.clone()),
            floats: self.floats.clone(),
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
        let changed = self.on_key_inner(key);
        self.settings_live() || changed
    }

    fn on_key_inner(&mut self, key: crossterm::event::KeyEvent) -> bool {
        // The Windows console also reports releases: only presses (and repeats) act,
        // in every dialog alike, or a key would act twice.
        if key.kind == crossterm::event::KeyEventKind::Release {
            return false;
        }
        self.idle.touch(Instant::now());
        // Locked: only the PIN field's keys and Enter, nothing else (not even quit).
        if self.locked.is_some() {
            return self.on_lock_key(key);
        }
        // Ctrl+L locks over any dialog, which is there again after the unlock.
        if self.keymap.map(key) == Some(Action::Lock) {
            self.lock();
            return true;
        }
        if self.confirm_overwrite.is_some() {
            return self.on_confirm_key(key);
        }
        if self.interface_switch.is_some() {
            return self.on_interface_switch_key(key);
        }
        if self.prompt.as_ref().is_some_and(|p| p.kind.over_dialogs()) {
            return self.on_prompt_key(key);
        }
        if self.compare.is_some() {
            return self.on_compare_key(key);
        }
        if self.pad_editor.is_some() {
            return self.on_pad_key(key);
        }
        if self.analysis_menu.is_some() {
            return self.on_analysis_menu_key(key);
        }
        if self.merge.is_some() {
            return self.on_merge_key(key);
        }
        if self.recent_menu.is_some() {
            return self.on_recent_key(key);
        }
        if self.info_view.is_some() {
            use crossterm::event::{KeyCode, KeyEventKind};
            if key.kind != KeyEventKind::Release
                && matches!(key.code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q'))
            {
                self.info_view = None;
                return true;
            }
            return false;
        }
        if self.keys_dialog.is_some() {
            return self.on_keys_dialog_key(key);
        }
        if self.plugins.is_some() {
            return self.on_plugins_key(key);
        }
        if self.analysis.is_some() && self.prompt.is_none() {
            return self.on_analysis_key(key);
        }
        // The tools editor opens over Settings, so it has the keys first.
        if self.tools_editor.is_some() {
            return self.on_tools_editor_key(key);
        }
        if self.settings_form.is_some() {
            return self.on_settings_key(key);
        }
        if self.rules.is_some() {
            return self.on_rules_key(key);
        }
        if self.formats.is_some() {
            return self.on_formats_key(key);
        }
        if self.presets.is_some() {
            return self.on_presets_key(key);
        }
        if self.global.is_some() {
            return self.on_global_key(key);
        }
        if self.tool_menu.is_some() {
            return self.on_tool_menu_key(key);
        }
        if self.hidden_menu.is_some() {
            return self.on_hidden_menu_key(key);
        }
        if self.piece_menu.is_some() {
            return self.on_piece_menu_key(key);
        }
        if self.results_menu.is_some() {
            return self.on_results_menu_key(key);
        }
        if self.export_menu.is_some() {
            return self.on_export_menu_key(key);
        }
        if self.columns_menu.is_some() {
            return self.on_columns_menu_key(key);
        }
        if self.palette_dialog.is_some() {
            return self.on_palette_key(key);
        }
        if self.json_dialog.is_some() {
            return self.on_json_key(key);
        }
        if self.find_window.as_ref().is_some_and(|w| w.shown) {
            return self.on_find_key(key);
        }
        if self.show_about {
            use crossterm::event::{KeyCode, KeyEventKind};
            if key.kind != KeyEventKind::Release
                && matches!(key.code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q'))
            {
                self.show_about = false;
            }
            return true;
        }
        if self.sessions.is_some() {
            return self.on_sessions_key(key);
        }
        if self.picker.is_some() {
            return self.on_picker_key(key);
        }
        if self.browser.is_some() {
            return self.on_browser_key(key);
        }
        if self.time_range.is_some() {
            return self.on_time_range_key(key);
        }
        if self.prompt.is_some() {
            return self.on_prompt_key(key);
        }
        if self.show_help && self.on_help_key(key) {
            return true;
        }
        // A tool's shortcut runs it on the cursor row (shortcuts always have a modifier).
        if let Some(i) = self
            .settings
            .as_ref()
            .and_then(|s| tools::tool_for_key(&s.config.external_tools, &key))
        {
            self.count = None;
            self.run_tool(i);
            return true;
        }
        // `L` then `1`-`9`: a quick label of that colour; any other key drops the `L`.
        if self.label_pending && key.kind != crossterm::event::KeyEventKind::Release {
            self.label_pending = false;
            self.message = None;
            if let crossterm::event::KeyCode::Char(c @ '1'..='9') = key.code {
                self.toggle_label(c as u8 - b'0');
            }
            return true;
        }
        if let Some(n) = self.count_digit(key) {
            self.count = Some(n);
            self.message = Some(txf(self.lang, "Count {0}", &[&n]));
            return true;
        }
        let count = self.count.take();
        if count.is_some() && key.code == crossterm::event::KeyCode::Esc {
            self.message = None;
            return true;
        }
        let Some(action) = self.keymap.map(key) else {
            if count.is_some() {
                self.message = None;
                return true;
            }
            return false;
        };
        // Any other key closes the help (and acts); `?` / F1 only close it.
        if self.show_help {
            self.show_help = false;
            if action == Action::ToggleHelp {
                return true;
            }
        }
        self.message = None;
        self.apply_counted(action, count.unwrap_or(1));
        true
    }

    /// The count after digit `key`: `1`-`9` start one, `0` extends it (alone it is
    /// still the scroll to column 0). At most `MAX_COUNT`.
    fn count_digit(&self, key: crossterm::event::KeyEvent) -> Option<u32> {
        use crossterm::event::{KeyCode, KeyEventKind};
        if key.kind == KeyEventKind::Release
            || key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return None;
        }
        let KeyCode::Char(c @ '0'..='9') = key.code else {
            return None;
        };
        if c == '0' && self.count.is_none() {
            return None;
        }
        let digit = c as u32 - '0' as u32;
        Some(
            self.count
                .unwrap_or(0)
                .saturating_mul(10)
                .saturating_add(digit)
                .min(MAX_COUNT),
        )
    }

    /// `action` repeated `count` times for the moves that take a count (`j` `k` `n`
    /// `N` `e` `E` `w` `W`); any other action runs once.
    pub fn apply_counted(&mut self, action: Action, count: u32) {
        match action {
            _ if self.tabs.is_empty() => self.apply(action),
            Action::NextError | Action::PrevError | Action::NextWarn | Action::PrevWarn => {
                let errors = matches!(action, Action::NextError | Action::PrevError);
                let forward = matches!(action, Action::NextError | Action::NextWarn);
                self.level_jump(errors, forward, count.max(1));
            }
            Action::LineUp | Action::LineDown | Action::SearchNext | Action::SearchPrev => {
                for _ in 0..count.max(1) {
                    self.apply(action);
                }
            }
            _ => self.apply(action),
        }
    }

    /// `e` `E` `w` `W`: the cursor to the `count`-th next (or previous) ERROR or WARN
    /// line visible under the filters, wrapping around.
    fn level_jump(&mut self, errors: bool, forward: bool, count: u32) {
        let name = if errors { "ERROR" } else { "WARN" };
        let tab = &mut self.tabs[self.active];
        if tab.is_hex() {
            self.message = Some(
                tx(
                    self.lang,
                    "The HEX view shows bytes, not lines: h returns to them",
                )
                .into(),
            );
            return;
        }
        let warn = LogLevel::Warn as u8;
        let want = |v: u8| {
            if errors {
                crate::tail_engine::is_error_level(v)
            } else {
                v == warn
            }
        };
        let mut line = tab.cursor_line().unwrap_or(0);
        let (mut found, mut wrapped) = (false, false);
        for _ in 0..count {
            match tab.engine.level_line_from(line, forward, want) {
                Some((l, w)) => {
                    line = l;
                    found = true;
                    wrapped |= w;
                }
                None => break,
            }
        }
        if !found {
            let yet = if tab.engine.levels_complete() {
                ""
            } else {
                tx(self.lang, " yet (levels are still being read)")
            };
            self.message = Some(txf(self.lang, "No {0} line visible{1}", &[&name, &yet]));
            return;
        }
        if let Some(row) = tab.engine.get_visible_row_of_line(line) {
            tab.set_cursor(row);
        }
        if wrapped {
            let end = if forward { "first" } else { "last" };
            self.message = Some(txf(self.lang, "{0}: back to the {1}", &[&name, &end]));
        }
    }

    /// The help's own keys: the cursor over the entries and `Enter` to run one. Returns
    /// false for a key the help leaves to the view (it then closes the help).
    fn on_help_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        use crossterm::event::{KeyCode, KeyEventKind};
        if key.kind == KeyEventKind::Release {
            return false;
        }
        let sel = self.help_sel;
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.help_sel = help_step(sel, -1),
            KeyCode::Down | KeyCode::Char('j') => self.help_sel = help_step(sel, 1),
            KeyCode::PageUp => {
                self.help_sel = help_step(sel.saturating_sub(10).saturating_add(1), -1)
            }
            KeyCode::PageDown => self.help_sel = help_step((sel + 10).min(HELP.len()) - 1, 1),
            KeyCode::Home => self.help_sel = help_step(HELP.len() - 1, 1),
            KeyCode::End => self.help_sel = help_step(0, -1),
            // Two columns: Left / Right jump to the other one.
            // Several columns: Left / Right move to the entry beside.
            KeyCode::Left | KeyCode::Right if self.help_half > 0 => {
                let other = if key.code == KeyCode::Left {
                    sel.checked_sub(self.help_half).unwrap_or(sel)
                } else if sel + self.help_half < HELP.len() {
                    sel + self.help_half
                } else {
                    sel
                };
                self.help_sel = if HELP[other].2.is_some() {
                    other
                } else {
                    help_step(other, 1)
                };
            }
            KeyCode::Enter => self.run_help_entry(sel),
            KeyCode::Esc => self.show_help = false,
            _ => return false,
        }
        true
    }

    /// Closes the help and runs the command of entry `i`, if it has one.
    fn run_help_entry(&mut self, i: usize) {
        self.show_help = false;
        if let Some(Some(action)) = HELP.get(i).map(|e| e.2) {
            self.message = None;
            self.apply(action);
        }
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
        let changed = self.on_paste_inner(text);
        self.settings_live() || changed
    }

    fn on_paste_inner(&mut self, text: &str) -> bool {
        // Nothing reaches the screens under the lock, not even pasted text.
        if self.locked.is_some() {
            return false;
        }
        if self.pad_editor.is_some() && self.prompt.is_none() {
            if let Some(e) = self.pad_editor.as_mut() {
                e.insert(text);
            }
            self.pad_edited();
            return true;
        }
        if let Some(form) = self.tools_editor.as_mut().and_then(|d| d.form.as_mut()) {
            if let Some(ToolWidget::Text(t)) =
                form.fields.get_mut(form.focus).map(|f| &mut f.widget)
            {
                t.insert(text);
            }
            return true;
        }
        if let Some(p) = self.palette_dialog.as_mut() {
            p.field.insert(text);
            p.selected = 0;
            return true;
        }
        if self.global.is_some() {
            // Typed character by character, so the term rows follow as with keys.
            for c in text.chars().filter(|c| !c.is_control()) {
                let key = crossterm::event::KeyEvent::new(
                    crossterm::event::KeyCode::Char(c),
                    KeyModifiers::NONE,
                );
                self.on_global_key(key);
            }
            return true;
        }
        if let Some(presets::Mode::Name { field, .. }) = self.presets.as_mut().map(|d| &mut d.mode)
        {
            field.insert(text);
            return true;
        }
        if let Some(form) = self.rules.as_mut().and_then(|d| d.form.as_mut()) {
            if let Some(RuleWidget::Text(t)) =
                form.fields.get_mut(form.focus).map(|f| &mut f.widget)
            {
                t.insert(text);
            }
            return true;
        }
        if let Some(f) = self.settings_form.as_mut() {
            if let Some(crate::tui::settings::Widget::Text(t)) =
                f.fields.get_mut(f.focus).map(|x| &mut x.widget)
            {
                t.insert(text);
            }
        } else if let Some(d) = self.sessions.as_mut() {
            d.field.insert(text);
        } else if let Some(p) = self.picker.as_mut() {
            p.filter.insert(text);
            p.filter_changed();
        } else if let Some(b) = self.browser.as_mut() {
            b.field.insert(text);
            b.filter_changed();
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
        let layout = self.dock_dirty.then(|| self.dock_ron());
        let Some(settings) = self.settings.as_mut() else {
            return;
        };
        if settings.path.as_os_str().is_empty() {
            return;
        }
        if let Some(layout) = layout {
            settings.config.dock_layout = Some(layout);
            self.dock_dirty = false;
        }
        for tab in &mut self.tabs {
            crate::workspace::save_changes(&mut tab.engine, &mut settings.config, false);
        }
        let order: Vec<PathBuf> = self.tabs.iter().map(|t| t.engine.path.clone()).collect();
        let mut state = crate::workspace::snapshot(&order, self.tabs.iter().map(|t| &t.engine));
        // The derived streams of the window ("Open filter as new tab") stay for it.
        state.keep_derived(&settings.config.open_files);
        state.write_into(&mut settings.config);
        match settings.save() {
            Ok(_) => self.save_error = None,
            Err(e) => {
                let text = txf(
                    self.lang,
                    "Cannot save {0}: {1}",
                    &[&settings.path.display(), &e],
                );
                if self.save_error.as_ref() != Some(&text) {
                    self.message = Some(text.clone());
                    self.save_error = Some(text);
                }
            }
        }
    }

    fn open_settings(&mut self) {
        let config = self.settings_mut().config.clone();
        self.settings_form = Some(SettingsForm::from_config(&config));
        self.settings_before = Some(config);
    }

    /// While Settings is open, every valid change reaches the configuration and the
    /// running interface at once, as the GUI's do. Returns whether anything changed.
    fn settings_live(&mut self) -> bool {
        let Some(form) = self.settings_form.as_ref() else {
            return false;
        };
        let settings = self.settings.get_or_insert_with(Default::default);
        let mut next = settings.config.clone();
        form.apply_live(&mut next);
        if next == settings.config {
            return false;
        }
        settings.config = next;
        self.apply_running_config();
        // A new language shows in the dialog at once too.
        if let Some(form) = self.settings_form.as_mut() {
            form.lang = self.lang;
        }
        true
    }

    /// Cancel (or `Esc`) of Settings: back to the configuration it opened with. The
    /// external tools edited from it stay, as their editor saved them.
    fn cancel_settings(&mut self) {
        if self.settings_form.take().is_none() {
            return;
        }
        if let Some(mut before) = self.settings_before.take() {
            let settings = self.settings.get_or_insert_with(Default::default);
            before.external_tools = settings.config.external_tools.clone();
            if before != settings.config {
                settings.config = before;
                self.apply_running_config();
            }
        }
    }

    /// The configuration reaches the running interface: theme, level colours, polling,
    /// each stream's settings (the line-number and time delta columns are defaults for
    /// new streams: each open stream keeps its own, as with the GUI's per-stream
    /// switches).
    fn apply_running_config(&mut self) {
        let Some(settings) = self.settings.as_ref() else {
            return;
        };
        let config = &settings.config;
        self.lang = config.language;
        self.palette.theme = config.theme;
        self.palette.level_colors = config.level_colors;
        self.idle_poll = Duration::from_millis(config.poll_interval_ms as u64);
        self.keymap = keys::KeyMap::build(&config.shortcuts);
        for tab in &mut self.tabs {
            let columns = (tab.engine.show_line_numbers, tab.engine.show_time_delta);
            crate::workspace::apply_settings(&mut tab.engine, config);
            (tab.engine.show_line_numbers, tab.engine.show_time_delta) = columns;
        }
    }

    /// `Ctrl+L` (or the idle lock): the PIN screen, when a PIN is set. Without one
    /// nothing locks, so nobody can be locked out; the state under it stays as it is.
    fn lock(&mut self) {
        let can = self
            .settings
            .as_ref()
            .is_some_and(|s| crate::lock::can_lock(&s.config));
        if !can {
            self.message = Some(tx(self.lang, "Set a PIN in Settings (,) to lock").into());
            return;
        }
        self.locked = Some(LockScreen::default());
    }

    /// A key on the lock screen: the PIN field's editing keys and `Enter`; every other
    /// key, `Esc`, `q` and `Ctrl+C` included, is dropped.
    fn on_lock_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        use crossterm::event::KeyCode;
        let now = Instant::now();
        let Some(screen) = self.locked.as_mut() else {
            return false;
        };
        if self.lock_attempts.cooldown_left(now).is_some() {
            return true;
        }
        match key.code {
            KeyCode::Enter => {
                let stored = self
                    .settings
                    .as_ref()
                    .map(|s| s.config.lock_pin.clone())
                    .unwrap_or_default();
                if crate::lock::pin_matches(&stored, screen.field.text()) {
                    self.lock_attempts.reset();
                    self.locked = None;
                } else {
                    screen.field = TextField::default();
                    screen.notice = if self.lock_attempts.register_failure(now) {
                        tx(self.lang, "Too many wrong PINs").into()
                    } else {
                        tx(self.lang, "Wrong PIN").into()
                    };
                }
                true
            }
            KeyCode::Char(_)
                if key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                false
            }
            KeyCode::Char(_)
            | KeyCode::Backspace
            | KeyCode::Delete
            | KeyCode::Left
            | KeyCode::Right
            | KeyCode::Home
            | KeyCode::End => {
                screen.field.on_key(key);
                true
            }
            _ => false,
        }
    }

    /// The focused stream's search text as quick label `color`: added, recoloured, or
    /// removed when it already has that colour (`QuickLabel::toggle`, as in the GUI).
    fn toggle_label(&mut self, color: u8) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let text = tab.engine.search_query.trim().to_string();
        if text.is_empty() {
            self.message = Some(tx(self.lang, "Search for the text to label first (/)").into());
            return;
        }
        if QuickLabel::toggle(&mut self.quick_labels, &text, color) {
            let on = self
                .quick_labels
                .iter()
                .any(|l| l.text == text && l.color == color);
            self.message = Some(if on {
                txf(self.lang, "Label {0}: {1}", &[&color, &text])
            } else {
                txf(self.lang, "Label removed: {0}", &[&text])
            });
            self.sync_labels();
        }
    }

    /// Hands the quick labels to every stream.
    fn sync_labels(&mut self) {
        for tab in &mut self.tabs {
            tab.engine.set_quick_labels(&self.quick_labels);
        }
    }

    /// `:`: the command palette, named in the interface language.
    fn open_palette(&mut self) {
        let lang = self
            .settings
            .as_ref()
            .map_or(crate::i18n::Language::En, |s| s.config.language);
        self.palette_dialog = Some(CommandPalette::new(lang));
    }

    /// `J`: the JSON dialog on the cursor row's payload.
    fn open_json(&mut self) {
        let tab = &self.tabs[self.active];
        let Some(line) = tab.cursor_line() else {
            self.message = Some(tx(self.lang, "No row").into());
            return;
        };
        let text = tab.engine.get_line(line).unwrap_or_default();
        match crate::json_tree::Tree::of_line(&text) {
            Ok(tree) => {
                let number = tab.engine.shown_line_number(line);
                self.json_dialog = Some(JsonDialog::new(number, tree));
            }
            Err(crate::json_tree::NoTree::TooLarge) => {
                self.message = Some(tx(self.lang, "JSON over 4 MB: not shown as a tree").into())
            }
            Err(crate::json_tree::NoTree::NotJson) => {
                self.message = Some(tx(self.lang, "No JSON on this row").into())
            }
        }
    }

    /// `Ctrl+Shift+F` / `Alt+/`: the Find results window, its query box holding the
    /// focused stream's search; the results of the last search when it was hidden.
    fn open_find(&mut self) {
        if let Some(w) = self.find_window.as_mut() {
            w.shown = true;
            w.editing = w.row_count() == 0;
            return;
        }
        let query = self
            .tabs
            .get(self.active)
            .map(|t| t.engine.search_query.trim().to_string())
            .unwrap_or_default();
        self.find_window = Some(FindWindow::new(&query));
    }

    /// Closes the Find results window, cancelling its jobs.
    fn close_find(&mut self) {
        if let Some(mut w) = self.find_window.take() {
            w.session.close();
        }
    }

    /// Drains the Find results jobs; true when the window has something new to draw.
    fn poll_find(&mut self) -> bool {
        let Some(w) = self.find_window.as_mut() else {
            return false;
        };
        let before = (
            w.session.total_hits(),
            w.session.running_count(),
            w.session.queued_count(),
            w.session
                .groups
                .iter()
                .map(|g| (g.progress * 100.0) as usize)
                .sum::<usize>(),
        );
        let engines: Vec<&TailEngine> = self.tabs.iter().map(|t| &t.engine).collect();
        w.session.poll(&engines);
        let after = (
            w.session.total_hits(),
            w.session.running_count(),
            w.session.queued_count(),
            w.session
                .groups
                .iter()
                .map(|g| (g.progress * 100.0) as usize)
                .sum::<usize>(),
        );
        w.shown && before != after
    }

    /// Runs the query box's text on every open stream.
    fn run_find(&mut self) {
        let Some(w) = self.find_window.as_mut() else {
            return;
        };
        w.session.input = w.field.text().to_string();
        let engines: Vec<&TailEngine> = self.tabs.iter().map(|t| &t.engine).collect();
        w.session.start(&engines);
        w.selected = 0;
        w.top = 0;
        if !w.session.groups.is_empty() {
            w.editing = false;
        }
    }

    fn on_find_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        use crossterm::event::{KeyCode, KeyModifiers};
        let page = self.find_rows.max(1);
        let Some(w) = self.find_window.as_mut() else {
            return false;
        };
        // The key that opened the window also takes it back to the query box.
        if self.keymap.map(key) == Some(Action::FindAll) {
            w.editing = true;
            return true;
        }
        if w.editing {
            match w.field.on_key(key) {
                FieldKey::Submit => self.run_find(),
                FieldKey::Cancel => self.close_find(),
                FieldKey::Edited => {}
                FieldKey::Other => {
                    if matches!(key.code, KeyCode::Tab | KeyCode::Down) && w.row_count() > 0 {
                        w.editing = false;
                    }
                }
            }
            return true;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => self.close_find(),
            KeyCode::Char('k') if ctrl => self.activate_find(true),
            KeyCode::Up | KeyCode::Char('k') => w.move_by(-1, page),
            KeyCode::Down | KeyCode::Char('j') => w.move_by(1, page),
            KeyCode::PageUp => w.move_by(-(page as isize), page),
            KeyCode::PageDown => w.move_by(page as isize, page),
            KeyCode::Home => w.move_by(isize::MIN / 2, page),
            KeyCode::End => w.move_by(isize::MAX / 2, page),
            KeyCode::Tab | KeyCode::BackTab | KeyCode::Char('/') => w.editing = true,
            KeyCode::Char('r') => {
                let engines: Vec<&TailEngine> = self.tabs.iter().map(|t| &t.engine).collect();
                w.session.refresh(&engines);
                w.selected = 0;
                w.top = 0;
            }
            KeyCode::Char('s') => w.session.stop(),
            KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') => {
                let open = |w: &FindWindow, g: usize| !w.session.groups[g].collapsed;
                let toggle = match (w.row(w.selected), key.code) {
                    (Some(FindRow::Header(g)), KeyCode::Left) => open(w, g),
                    (Some(FindRow::Header(g)), KeyCode::Right) => !open(w, g),
                    (Some(FindRow::Hit(..)), KeyCode::Left) | (Some(_), KeyCode::Char(' ')) => true,
                    _ => false,
                };
                if toggle {
                    w.toggle_group();
                    w.reveal(page);
                }
            }
            KeyCode::Enter => self.activate_find(false),
            _ => {}
        }
        true
    }

    /// `Enter` on the selected row: a header opens or closes its group, a result focuses
    /// its stream on the line (`in_context`: "Show in context", filters suspended) and
    /// hides the window; a stale result does not jump.
    fn activate_find(&mut self, in_context: bool) {
        let Some(w) = self.find_window.as_mut() else {
            return;
        };
        let (g, h) = match w.row(w.selected) {
            Some(FindRow::Hit(g, h)) => (g, h),
            Some(FindRow::Header(_)) => {
                w.toggle_group();
                return;
            }
            None => return,
        };
        let committed = if in_context {
            w.session.commit_in_context(g, h)
        } else {
            w.session.commit(g, h)
        };
        if !committed {
            self.message = Some(
                tx(
                    self.lang,
                    "This stream was reloaded since the search: press r to search again",
                )
                .into(),
            );
            return;
        }
        let Some((path, line)) = w.session.jump.take() else {
            return;
        };
        let in_context = std::mem::take(&mut w.session.jump_in_context);
        w.shown = false;
        let Some(i) = self.tabs.iter().position(|t| t.engine.path == path) else {
            return;
        };
        self.focus_tab(i);
        let engine = &mut self.tabs[i].engine;
        if !(in_context && engine.enter_context(line)) {
            engine.request_jump(line, self.lang);
        }
    }

    fn on_json_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        let Some(d) = self.json_dialog.as_mut() else {
            return false;
        };
        match d.on_key(key) {
            JsonKey::Close => self.json_dialog = None,
            JsonKey::Cut => self.message = Some(tx(self.lang, "Stopped at 5,000 rows").into()),
            JsonKey::Copy { text, path } => {
                let what = if path { "path" } else { "value" };
                self.message = Some(match self.clipboard.copy(&text) {
                    Ok(Copied::System) => txf(self.lang, "Copied the {0}", &[&what]),
                    Ok(Copied::Osc52) => txf(
                        self.lang,
                        "Sent the {0} to the terminal clipboard (OSC 52)",
                        &[&what],
                    ),
                    Err(e) => txf(self.lang, "Copy failed: {0}", &[&e]),
                });
            }
            JsonKey::Handled => {}
        }
        true
    }

    fn on_palette_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        let Some(p) = self.palette_dialog.as_mut() else {
            return false;
        };
        match p.on_key(key) {
            PaletteKey::Run(id, action) => {
                self.palette_dialog = None;
                self.run_palette(id, action);
            }
            PaletteKey::GoTo(text) => {
                self.palette_dialog = None;
                self.submit_prompt(Prompt {
                    kind: PromptKind::Goto,
                    field: TextField::new(&text),
                });
            }
            PaletteKey::Close => self.palette_dialog = None,
            PaletteKey::Moved => {}
            PaletteKey::Other => return false,
        }
        true
    }

    /// Runs a palette entry; the text and HEX views only switch when not already there.
    fn run_palette(&mut self, id: ActionId, action: Action) {
        let hex = self.tabs.get(self.active).is_some_and(|t| t.is_hex());
        let asm = self.tabs.get(self.active).is_some_and(|t| t.is_asm());
        match id {
            ActionId::ViewText if !hex => {}
            ActionId::ViewHex if hex && !asm => {}
            ActionId::ViewAsm if asm => {}
            ActionId::AsmArch if !asm => {
                self.message = Some(tx(self.lang, "Only in the ASM view: d opens it").into())
            }
            _ => self.apply(action),
        }
    }

    /// Keys of the pieces list: arrows choose, Enter goes to the piece's first line.
    fn on_piece_menu_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        use crossterm::event::{KeyCode, KeyEventKind};
        if key.kind == KeyEventKind::Release {
            return false;
        }
        let count = self
            .tabs
            .get(self.active)
            .map_or(0, |t| t.engine.rotation_pieces().len());
        let Some(selected) = self.piece_menu.as_mut() else {
            return false;
        };
        match key.code {
            KeyCode::Esc => self.piece_menu = None,
            KeyCode::Up | KeyCode::Char('k') => *selected = selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                *selected = (*selected + 1).min(count.saturating_sub(1))
            }
            KeyCode::Enter => {
                let i = *selected;
                self.go_to_piece(i);
            }
            _ => return false,
        }
        true
    }

    /// Keys of the search results list: arrows, `PgUp` `PgDn`, `Home` `End` choose,
    /// Enter goes to the hit.
    fn on_results_menu_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        use crossterm::event::{KeyCode, KeyEventKind};
        if key.kind == KeyEventKind::Release {
            return false;
        }
        let count = self
            .tabs
            .get(self.active)
            .map_or(0, |t| t.engine.search_matches.len());
        let page = self
            .tabs
            .get(self.active)
            .map_or(10, |t| t.height.max(2) - 1);
        let Some(selected) = self.results_menu.as_mut() else {
            return false;
        };
        let last = count.saturating_sub(1);
        match key.code {
            KeyCode::Esc => self.results_menu = None,
            KeyCode::Up | KeyCode::Char('k') => *selected = selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => *selected = (*selected + 1).min(last),
            KeyCode::PageUp => *selected = selected.saturating_sub(page),
            KeyCode::PageDown => *selected = (*selected + page).min(last),
            KeyCode::Home => *selected = 0,
            KeyCode::End => *selected = last,
            KeyCode::Enter => {
                let i = *selected;
                self.go_to_result(i);
            }
            _ => return false,
        }
        true
    }

    /// Makes hit `i` of the search results current, moves the cursor to its line and
    /// closes the list.
    fn go_to_result(&mut self, i: usize) {
        self.results_menu = None;
        let lang = self.lang;
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let Some(&line) = tab.engine.search_matches.get(i) else {
            return;
        };
        tab.engine.follow_tail = false;
        tab.engine.current_match_idx = Some(i);
        let current = tab.cursor_line().unwrap_or(0);
        let target = tab.engine.resolve_goto(&(line + 1).to_string(), current);
        self.message = goto_target(tab, target, &(line + 1).to_string(), lang);
    }

    fn draw_results_menu(&mut self, frame: &mut Frame, area: Rect) {
        let dim = Style::default().fg(self.palette.dim());
        let Some(selected) = self.results_menu else {
            return;
        };
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let count = tab.engine.search_matches.len();
        let height = (count as u16 + 6).min(area.height.saturating_sub(2));
        let width = area.width.saturating_sub(4).min(120);
        let scoped = !tab.engine.search_scope().is_all();
        let title = if scoped {
            txf(
                self.lang,
                "{0} search results in range",
                &[&group_digits(count)],
            )
        } else {
            txf(self.lang, "{0} search results", &[&group_digits(count)])
        };
        let inner = self.dialog(frame, area, (width, height), &title, false);
        let tab = &self.tabs[self.active];
        let hits = &tab.engine.search_matches;
        let mut out = vec![Line::styled(
            tx(
                self.lang,
                "Enter goes to the line, Up/Down PgUp/PgDn choose, Esc closes",
            ),
            dim,
        )];
        let room = inner.height.saturating_sub(3) as usize;
        let top = selected.saturating_sub(room.saturating_sub(1));
        let cells = inner.width as usize;
        let digits = group_digits(tab.engine.total_lines()).len();
        let engine = &tab.engine;
        for (i, &line) in hits.iter().enumerate().skip(top).take(room) {
            let y = inner.y + 1 + (i - top) as u16;
            self.hits
                .list_items
                .push((Rect::new(inner.x, y, inner.width, 1), i));
            let style = if i == selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                palette_level(&self.palette, engine, line)
            };
            let text = engine.get_line(line).unwrap_or_default();
            let text = view::sanitize(text.trim_end_matches(['\r', '\n']));
            let row = format!(" {:>digits$}  {text}", group_digits(line + 1));
            out.push(Line::styled(head_chars(&row, cells), style));
        }
        frame.render_widget(Paragraph::new(out), inner);
    }

    /// Keys of the export menu: arrows choose, Enter asks for the file.
    fn on_export_menu_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        use crossterm::event::{KeyCode, KeyEventKind};
        if key.kind == KeyEventKind::Release {
            return false;
        }
        let Some(selected) = self.export_menu.as_mut() else {
            return false;
        };
        match key.code {
            KeyCode::Esc => self.export_menu = None,
            KeyCode::Up | KeyCode::Char('k') => *selected = selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                *selected = (*selected + 1).min(EXPORT_CHOICES.len() - 1)
            }
            KeyCode::Enter => {
                let i = *selected;
                self.choose_export(i);
            }
            _ => return false,
        }
        true
    }

    /// Export choice `i`: asks for the file when the scope holds lines.
    fn choose_export(&mut self, i: usize) {
        self.export_menu = None;
        let Some(&(scope, format)) = EXPORT_CHOICES.get(i) else {
            return;
        };
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        if crate::export::scope_len(&tab.engine, scope) == 0 {
            self.message = Some(tx(self.lang, "Nothing to export").into());
            return;
        }
        if self.export.is_some() {
            self.message = Some(tx(self.lang, "An export is still being written").into());
            return;
        }
        self.open_prompt(PromptKind::Export(scope, format));
    }

    fn draw_export_menu(&mut self, frame: &mut Frame, area: Rect) {
        let dim = Style::default().fg(self.palette.dim());
        let Some(selected) = self.export_menu else {
            return;
        };
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let counts: Vec<usize> = EXPORT_CHOICES
            .iter()
            .map(|&(scope, _)| crate::export::scope_len(&tab.engine, scope))
            .collect();
        let height = (EXPORT_CHOICES.len() as u16 + 4).min(area.height);
        let inner = self.dialog(frame, area, (60, height), tx(self.lang, "Export"), false);
        let mut out = vec![Line::styled(
            tx(
                self.lang,
                "Enter asks for the file, Up/Down choose, Esc closes",
            ),
            dim,
        )];
        let width = inner.width as usize;
        for (i, &(scope, format)) in EXPORT_CHOICES.iter().enumerate() {
            let y = inner.y + 1 + i as u16;
            self.hits
                .list_items
                .push((Rect::new(inner.x, y, inner.width, 1), i));
            let n = counts[i];
            let style = if i == selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else if n == 0 {
                dim
            } else {
                Style::default()
            };
            let what = match scope {
                crate::export::Scope::Visible => tx(self.lang, "Visible lines"),
                crate::export::Scope::Selected => tx(self.lang, "Selected lines"),
                crate::export::Scope::Matches => tx(self.lang, "Search matches"),
            };
            let text = format!(
                " {:<16} {:<5} {:>12}",
                what,
                export_format_name(format),
                group_digits(n)
            );
            out.push(Line::styled(head_chars(&text, width), style));
        }
        frame.render_widget(Paragraph::new(out), inner);
    }

    /// Starts writing `scope` of the active stream as `format` to `text` (a trailing `!`
    /// replaces an existing file).
    fn start_export(
        &mut self,
        scope: crate::export::Scope,
        format: crate::export::Format,
        text: &str,
    ) {
        let (text, replace) = match text.strip_suffix('!') {
            Some(rest) => (rest.trim(), true),
            None => (text, false),
        };
        if text.is_empty() {
            return;
        }
        let path = PathBuf::from(text);
        if path.exists() && !replace {
            self.message = Some(txf(
                self.lang,
                "{0} exists: add ! to replace it",
                &[&path.display()],
            ));
            return;
        }
        let config = self.settings.as_ref().map(|s| &s.config);
        let csv = crate::export::CsvOptions {
            sep: config
                .and_then(|c| crate::export::separator_from_name(&c.export_csv_separator))
                .unwrap_or(b','),
            bom: config.is_none_or(|c| c.export_csv_bom),
            protect: config.is_none_or(|c| c.export_csv_protect),
        };
        let theme = self.palette.theme;
        let level_colors = self.palette.level_colors;
        let tab = &self.tabs[self.active];
        let engine = &tab.engine;
        let (writer, max): (Box<dyn crate::export::RowWriter>, Option<usize>) = match format {
            crate::export::Format::Text => (Box::new(crate::export::TextWriter), None),
            crate::export::Format::Csv => (Box::new(crate::export::CsvWriter::new(csv)), None),
            crate::export::Format::Html => (
                Box::new(crate::export_html::HtmlWriter::new(
                    engine,
                    theme,
                    level_colors,
                    &stream_title(engine),
                )),
                Some(crate::export::HTML_MAX_LINES),
            ),
        };
        match crate::export::ExportJob::start(engine, scope, path, writer, max) {
            Ok(job) => {
                self.export = Some((engine.path.clone(), job));
                self.step_export();
            }
            Err(e) => self.message = Some(txf(self.lang, "Export failed: {0}", &[&e])),
        }
    }

    /// Writes the running export for a few milliseconds; says how far it got, and where
    /// it went once done. Closing its stream cancels it.
    fn step_export(&mut self) {
        let Some((path, mut job)) = self.export.take() else {
            return;
        };
        let Some(tab) = self.tabs.iter().find(|t| t.engine.path == path) else {
            job.cancel();
            return;
        };
        if job.step(&tab.engine, Duration::from_millis(20)) {
            self.message = Some(match &job.error {
                Some(e) => txf(self.lang, "Export failed: {0}", &[e]),
                None if job.capped => txf(
                    self.lang,
                    "Exported {0} lines to {1} (an HTML page holds at most 200,000)",
                    &[&group_digits(job.lines), &job.path.display()],
                ),
                None => txf(
                    self.lang,
                    "Exported {0} lines to {1}",
                    &[&group_digits(job.lines), &job.path.display()],
                ),
            });
        } else {
            self.message = Some(txf(
                self.lang,
                "Export: {0}%",
                &[&format!("{:.0}", job.progress() * 100.0)],
            ));
            self.export = Some((path, job));
        }
    }

    /// The search scope typed in the prompt: a line range (`1200-5000`, `1200-`, `-5000`,
    /// `1200`), a time range (`14:00..15:30`, an open side allowed), or empty for every
    /// line.
    fn submit_search_scope(&mut self, text: &str) {
        use crate::tail_engine::SearchScope;
        let scope = if text.is_empty() {
            Some(SearchScope::All)
        } else if let Some((from, to)) = text.split_once("..") {
            Some(SearchScope::Time {
                from: from.trim().to_string(),
                to: to.trim().to_string(),
            })
        } else {
            SearchScope::parse_lines(text)
        };
        let Some(scope) = scope else {
            self.message = Some(txf(self.lang, "Cannot read \"{0}\" as a range", &[&text]));
            return;
        };
        let tab = &mut self.tabs[self.active];
        let (from_ok, to_ok) = tab.engine.set_search_scope(scope);
        self.message = Some(if !(from_ok && to_ok) {
            txf(self.lang, "Cannot read \"{0}\" as a range", &[&text])
        } else if tab.engine.search_scope().is_all() {
            tx(self.lang, "Search everywhere").into()
        } else {
            txf(self.lang, "Search in {0}", &[&text])
        });
    }

    /// Copies the selected rows, or the cursor row, as the view shows them: the column
    /// view's cells separated by tabs, a closed group with its badge.
    fn copy_as_shown(&mut self) {
        let tab = &mut self.tabs[self.active];
        let text = if tab.engine.has_selection() {
            tab.engine.copy_selection_as_shown()
        } else {
            tab.cursor_line().and_then(|line| {
                tab.engine.select_row(line);
                let text = tab.engine.copy_selection_as_shown();
                tab.engine.clear_selection();
                text
            })
        };
        match text {
            Some(text) => self.copy_text(&text),
            None => self.message = Some(tx(self.lang, "Nothing to copy").into()),
        }
    }

    /// `Alt+Y`: the selected rows (or the cursor row) as an excerpt in the settings'
    /// flavour (Markdown, Jira, Slack).
    fn copy_excerpt(&mut self) {
        let flavour = self
            .settings
            .as_ref()
            .map(|s| s.config.excerpt_format)
            .unwrap_or_default();
        let tab = &mut self.tabs[self.active];
        let lines = if tab.engine.has_selection() {
            tab.engine.selected_lines()
        } else {
            tab.cursor_line().into_iter().collect()
        };
        match crate::export::excerpt(&tab.engine, &lines, flavour) {
            Some(text) => self.copy_text(&text),
            None => self.message = Some(tx(self.lang, "Nothing to copy").into()),
        }
    }

    /// Puts `text` on the clipboard and says how many lines went.
    fn copy_text(&mut self, text: &str) {
        let lines = text.lines().count();
        self.message = Some(match self.clipboard.copy(text) {
            Ok(Copied::System) => txf(self.lang, "Copied {0} lines to the clipboard", &[&lines]),
            Ok(Copied::Osc52) => txf(
                self.lang,
                "Sent {0} lines to the terminal clipboard (OSC 52)",
                &[&lines],
            ),
            Err(e) => txf(self.lang, "Copy failed: {0}", &[&e]),
        });
    }

    /// The bookmark report of the active stream (or of every stream) as Markdown, with
    /// the context and order of the window's last report: saved to `text`, or copied
    /// when it is empty.
    fn write_report(&mut self, all: bool, text: &str) {
        use crate::bookmark_report::{ReportJob, ReportOptions, ReportOrder};
        let options = match self.settings.as_ref().map(|s| &s.config) {
            Some(c) => ReportOptions {
                context: (c.report_context as usize)
                    .min(crate::bookmark_report::MAX_REPORT_CONTEXT),
                include_auto: c.report_auto,
                tags: Vec::new(),
                order: if c.report_order_time {
                    ReportOrder::Time
                } else {
                    ReportOrder::Stream
                },
            },
            None => ReportOptions::default(),
        };
        let engines: Vec<&TailEngine> = if all {
            self.tabs.iter().map(|t| &t.engine).collect()
        } else {
            self.tabs
                .get(self.active)
                .map(|t| &t.engine)
                .into_iter()
                .collect()
        };
        let streams: Vec<_> = engines
            .iter()
            .map(|e| e.bookmark_report_stream(stream_title(e), &options))
            .filter(|s| !s.bookmarks.is_empty())
            .collect();
        if streams.is_empty() {
            self.message = Some(tx(self.lang, "No bookmarks to report").into());
            return;
        }
        let with: Vec<&TailEngine> = engines
            .into_iter()
            .filter(|e| e.bookmark_report_count(&options) > 0)
            .collect();
        let mut job = ReportJob::new(streams, options);
        job.step(Duration::MAX, |s, line| {
            with.get(s)
                .and_then(|e| e.get_line(line))
                .map(|t| t.trim_end_matches(['\r', '\n']).to_string())
        });
        let now = crate::timestamp::format_millis(crate::timestamp::local_now_millis());
        let markdown = job.markdown(&now, env!("CARGO_PKG_VERSION"));
        if text.is_empty() {
            if markdown.len() > crate::bookmark_report::MAX_CLIPBOARD_BYTES {
                self.message = Some(
                    tx(
                        self.lang,
                        "The report is too large for the clipboard: save it",
                    )
                    .into(),
                );
            } else {
                self.copy_text(&markdown);
            }
            return;
        }
        use std::io::Write;
        let path = PathBuf::from(text);
        let result = crate::paths::create_export_file(&path)
            .and_then(|mut f| f.write_all(markdown.as_bytes()));
        self.message = Some(match result {
            Ok(()) => txf(self.lang, "Report saved to {0}", &[&path.display()]),
            Err(e) => txf(self.lang, "Could not save the report: {0}", &[&e]),
        });
    }

    /// Moves the cursor to the first line of piece `i` (the next visible one when the
    /// filters hide it) and closes the list.
    fn go_to_piece(&mut self, i: usize) {
        self.piece_menu = None;
        let lang = self.lang;
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let Some((line, _)) = tab.engine.rotation_pieces().get(i).cloned() else {
            return;
        };
        tab.engine.follow_tail = false;
        let current = tab.cursor_line().unwrap_or(0);
        let target = tab.engine.resolve_goto(&(line + 1).to_string(), current);
        self.message = goto_target(tab, target, &(line + 1).to_string(), lang);
    }

    fn draw_piece_menu(&mut self, frame: &mut Frame, area: Rect) {
        let dim = Style::default().fg(self.palette.dim());
        let Some(selected) = self.piece_menu else {
            return;
        };
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let rows = tab.engine.rotation_pieces();
        let height = (rows.len() as u16 + 6).min(area.height);
        let inner = self.dialog(
            frame,
            area,
            (64, height),
            tx(self.lang, "Go to piece"),
            false,
        );
        let mut out = vec![Line::styled(
            tx(
                self.lang,
                "Enter goes to the piece, Up/Down choose, Esc closes",
            ),
            dim,
        )];
        let room = inner.height.saturating_sub(3) as usize;
        let top = selected.saturating_sub(room.saturating_sub(1));
        let width = inner.width as usize;
        for (i, (line, name)) in rows.iter().enumerate().skip(top).take(room) {
            let y = inner.y + 1 + (i - top) as u16;
            self.hits
                .list_items
                .push((Rect::new(inner.x, y, inner.width, 1), i));
            let style = if i == selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            let text = format!(" {:>12}  {name}", group_digits(line + 1));
            out.push(Line::styled(head_chars(&text, width), style));
        }
        frame.render_widget(Paragraph::new(out), inner);
    }

    /// Keys of the hidden lines list: arrows choose, Enter shows the range, `a` shows all.
    fn on_hidden_menu_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        use crossterm::event::{KeyCode, KeyEventKind};
        if key.kind == KeyEventKind::Release {
            return false;
        }
        let count = self
            .tabs
            .get(self.active)
            .map_or(0, |t| t.engine.hidden_ranges().len());
        let Some(selected) = self.hidden_menu.as_mut() else {
            return false;
        };
        match key.code {
            KeyCode::Esc => self.hidden_menu = None,
            KeyCode::Up | KeyCode::Char('k') => *selected = selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                *selected = (*selected + 1).min(count.saturating_sub(1))
            }
            KeyCode::Enter => {
                let i = *selected;
                self.show_hidden(Some(i));
            }
            KeyCode::Char('a') => self.show_hidden(None),
            _ => return false,
        }
        true
    }

    /// The rows of the columns list: the columns shown, in order and ticked, then the
    /// other fields known for the stream (at most `fields::MAX_CATALOGUE`).
    fn column_menu_rows(engine: &TailEngine) -> Vec<(String, bool)> {
        // In the order drawn: the pinned columns first.
        let (shown, _) = engine.drawn_field_columns();
        let mut rows: Vec<(String, bool)> = shown.iter().map(|k| (k.clone(), true)).collect();
        for key in engine.field_catalogue().keys() {
            if !shown.contains(key) && rows.len() < crate::fields::MAX_CATALOGUE {
                rows.push((key.clone(), false));
            }
        }
        rows
    }

    fn on_columns_menu_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};
        if key.kind == KeyEventKind::Release {
            return false;
        }
        let Some(selected) = self.columns_menu else {
            return false;
        };
        let Some(tab) = self.tabs.get_mut(self.active) else {
            self.columns_menu = None;
            return true;
        };
        let rows = Self::column_menu_rows(&tab.engine);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        let mut shown: Vec<String> = rows.iter().filter(|r| r.1).map(|r| r.0.clone()).collect();
        let moved = |shown: &mut Vec<String>, up: bool| -> Option<usize> {
            if selected >= shown.len() {
                return None;
            }
            let to = if up {
                selected.checked_sub(1)?
            } else {
                (selected + 1 < shown.len()).then_some(selected + 1)?
            };
            shown.swap(selected, to);
            Some(to)
        };
        match key.code {
            KeyCode::Esc => self.columns_menu = None,
            KeyCode::Up | KeyCode::Char('K') if shift || key.code == KeyCode::Char('K') => {
                if let Some(to) = moved(&mut shown, true) {
                    tab.engine.set_field_columns(shown);
                    self.columns_menu = Some(to);
                }
            }
            KeyCode::Down | KeyCode::Char('J') if shift || key.code == KeyCode::Char('J') => {
                if let Some(to) = moved(&mut shown, false) {
                    tab.engine.set_field_columns(shown);
                    self.columns_menu = Some(to);
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.columns_menu = Some(selected.saturating_sub(1))
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.columns_menu = Some((selected + 1).min(rows.len().saturating_sub(1)))
            }
            KeyCode::Char(' ') | KeyCode::Enter => self.toggle_column(selected),
            KeyCode::Char('r') => {
                tab.engine.reset_field_columns();
                self.columns_menu = Some(0);
            }
            KeyCode::Char('p') | KeyCode::Char('P') => {
                if let Some((key, _)) = rows.get(selected) {
                    tab.engine.set_plots(plot_first(tab.engine.plots(), key));
                }
            }
            KeyCode::Char('S') | KeyCode::Char('O') => {
                let stats = key.code == KeyCode::Char('S');
                if let Some((field, _)) = rows.get(selected).cloned() {
                    self.columns_menu = None;
                    self.open_source_view(crate::field_stats::StatsSource::Field(field), stats);
                }
            }
            KeyCode::Char('f') | KeyCode::Char('F') => {
                // Pins or unpins a shown column; the row follows it.
                if let Some((key, true)) = rows.get(selected).cloned() {
                    let pinned = tab.engine.pinned_columns().contains(&key);
                    tab.engine.pin_column(&key, !pinned);
                    let rows = Self::column_menu_rows(&tab.engine);
                    self.columns_menu = rows.iter().position(|r| r.0 == key).or(Some(0));
                }
            }
            KeyCode::Char('a') | KeyCode::Char('A') => {
                let key = rows.get(selected).map(|r| r.0.as_str());
                if let Some(spec) = tab
                    .engine
                    .plots()
                    .iter()
                    .find(|p| Some(p.key.as_str()) == key)
                {
                    let (key, stat) = (spec.key.clone(), spec.stat.next());
                    tab.engine.set_plot_stat(&key, stat);
                }
            }
            _ => return false,
        }
        true
    }

    /// Shows or hides the field of row `i` of the columns list. The last shown column
    /// stays: an empty list would mean the default columns again.
    fn toggle_column(&mut self, i: usize) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let rows = Self::column_menu_rows(&tab.engine);
        let Some((key, on)) = rows.get(i).cloned() else {
            return;
        };
        let mut shown: Vec<String> = rows.iter().filter(|r| r.1).map(|r| r.0.clone()).collect();
        if on {
            if shown.len() <= 1 {
                return;
            }
            shown.retain(|k| *k != key);
        } else {
            shown.push(key.clone());
        }
        tab.engine.set_field_columns(shown);
        // The row follows its field to its new place in the list.
        let rows = Self::column_menu_rows(&tab.engine);
        self.columns_menu = rows.iter().position(|r| r.0 == key).or(Some(0));
    }

    fn draw_columns_menu(&mut self, frame: &mut Frame, area: Rect) {
        let dim = Style::default().fg(self.palette.dim());
        let Some(selected) = self.columns_menu else {
            return;
        };
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let rows = Self::column_menu_rows(&tab.engine);
        let plots = tab.engine.plots().to_vec();
        let pins = tab.engine.pinned_columns();
        let height = (rows.len() as u16 + 6).min(area.height);
        let inner = self.dialog(frame, area, (74, height), tx(self.lang, "Columns"), false);
        let mut out = vec![Line::styled(
            tx(
                self.lang,
                "Space show/hide, Shift+Up/Down move, r reset, p plot, a stat, f pin",
            ),
            dim,
        )];
        let room = inner.height.saturating_sub(3) as usize;
        let top = selected.saturating_sub(room.saturating_sub(1));
        let width = inner.width as usize;
        let ascii = self.palette.ascii;
        for (i, (key, on)) in rows.iter().enumerate().skip(top).take(room) {
            let y = inner.y + 1 + (i - top) as u16;
            self.hits
                .list_items
                .push((Rect::new(inner.x, y, inner.width, 1), i));
            let style = if i == selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else if *on {
                Style::default()
            } else {
                dim
            };
            let mark = match (on, ascii) {
                (true, true) => "[x]",
                (false, true) => "[ ]",
                (true, false) => "☑",
                (false, false) => "☐",
            };
            let plotted = plots
                .iter()
                .position(|p| p.key == *key)
                .map(|at| {
                    let sign = if ascii { "~" } else { "📈" };
                    let stat = plots[at].stat.name();
                    // Only the first plotted field has the terminal's sparkline row.
                    if at == 0 {
                        format!("  {sign} {stat}")
                    } else {
                        format!("  ({sign} {stat})")
                    }
                })
                .unwrap_or_default();
            let pin = match (pins.contains(key), ascii) {
                (true, true) => "  ^",
                (true, false) => "  📌",
                _ => "",
            };
            out.push(Line::styled(
                head_chars(&format!(" {mark} {key}{pin}{plotted}"), width),
                style,
            ));
        }
        frame.render_widget(Paragraph::new(out), inner);
    }

    /// Shows the hidden range `i` again, or all of them; the list closes with the last.
    fn show_hidden(&mut self, i: Option<usize>) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        match i {
            Some(i) => tab.engine.show_hidden_range(i),
            None => tab.engine.show_all_hidden(),
        }
        let left = tab.engine.hidden_ranges().len();
        self.hidden_menu = (left > 0).then(|| self.hidden_menu.unwrap_or(0).min(left - 1));
    }

    fn draw_hidden_menu(&mut self, frame: &mut Frame, area: Rect) {
        let dim = Style::default().fg(self.palette.dim());
        let Some(selected) = self.hidden_menu else {
            return;
        };
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let rows: Vec<(String, String)> = tab
            .engine
            .hidden_ranges()
            .iter()
            .map(|&(a, b)| {
                let lines = if a == b {
                    group_digits(a + 1)
                } else {
                    format!("{}-{}", group_digits(a + 1), group_digits(b + 1))
                };
                let first = tab.engine.get_line(a).unwrap_or_default();
                (lines, view::sanitize(&first))
            })
            .collect();
        let height = (rows.len() as u16 + 6).min(area.height);
        let inner = self.dialog(
            frame,
            area,
            (78, height),
            tx(self.lang, "Hidden lines"),
            false,
        );
        let mut out = vec![Line::styled(
            tx(
                self.lang,
                "Enter shows the range again, a shows all, Up/Down choose, Esc closes",
            ),
            dim,
        )];
        let room = inner.height.saturating_sub(3) as usize;
        let top = selected.saturating_sub(room.saturating_sub(1));
        let width = inner.width as usize;
        for (i, (lines, first)) in rows.iter().enumerate().skip(top).take(room) {
            let y = inner.y + 1 + (i - top) as u16;
            self.hits
                .list_items
                .push((Rect::new(inner.x, y, inner.width, 1), i));
            let style = if i == selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            let text = format!(" {lines:>15}  {first}");
            out.push(Line::styled(head_chars(&text, width), style));
        }
        frame.render_widget(Paragraph::new(out), inner);
    }

    /// `!`: the menu of the external tools.
    fn open_tool_menu(&mut self) {
        let n = self.settings_mut().config.external_tools.len();
        self.tool_menu = Some(ToolMenu::new(n));
    }

    fn on_tool_menu_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        let Some(m) = self.tool_menu.as_mut() else {
            return false;
        };
        match m.on_key(key) {
            MenuKey::Run(i) => {
                self.tool_menu = None;
                self.run_tool(i);
            }
            MenuKey::Close => self.tool_menu = None,
            MenuKey::Edit => {
                self.tool_menu = None;
                self.open_tools_editor();
            }
            MenuKey::Moved => {}
            MenuKey::Other => return false,
        }
        true
    }

    /// The external tools editor over the tools of `fasttail.ini`.
    fn open_tools_editor(&mut self) {
        let config = &self.settings_mut().config;
        self.tools_editor = Some(ToolsDialog::new(
            &config.external_tools,
            &config.highlight_rules,
        ));
    }

    fn on_tools_editor_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        let Some(d) = self.tools_editor.as_mut() else {
            return false;
        };
        match d.on_key(key) {
            ToolsKey::Changed => self.store_tools(),
            ToolsKey::Close => self.tools_editor = None,
            ToolsKey::Moved => {}
            ToolsKey::Other => return false,
        }
        true
    }

    /// The editor's tools become the configuration's, saved at once; the Settings row
    /// under the editor counts them again.
    fn store_tools(&mut self) {
        let Some(d) = &self.tools_editor else {
            return;
        };
        let tools = d.tools();
        let count = tools.len();
        self.settings
            .get_or_insert_with(Default::default)
            .config
            .external_tools = tools;
        if let Some(form) = self.settings_form.as_mut() {
            for f in &mut form.fields {
                if f.key == crate::tui::settings::Key::Tools {
                    f.widget = crate::tui::settings::Widget::Button(txf(
                        self.lang,
                        "{0} defined - Enter edits them",
                        &[&count],
                    ));
                }
            }
        }
        self.save_config();
    }

    /// Runs tool `i` on the cursor row of the focused stream (with the selection's
    /// text when the row is selected), as the GUI's row menu does.
    fn run_tool(&mut self, i: usize) {
        let Some(tool) = self
            .settings
            .as_ref()
            .and_then(|s| s.config.external_tools.get(i).cloned())
        else {
            return;
        };
        let Some(tab) = self.tabs.get(self.active) else {
            self.message = Some(tx(self.lang, NO_FILE).into());
            return;
        };
        let Some(line) = tab.cursor_line() else {
            self.message =
                Some(tx(self.lang, "No row for a tool (the HEX view shows bytes)").into());
            return;
        };
        let Some(ctx) = crate::workspace::tool_context_for_row(&tab.engine, line) else {
            return;
        };
        self.message = Some(match self.tool_runner.run_manual(&tool, &ctx) {
            Ok(()) => txf(self.lang, "Started {0}", &[&tool.name]),
            Err(e) => txf(self.lang, "Cannot start {0}: {1}", &[&tool.name, &e]),
        });
    }

    /// Shows the notifications the streams queued (a stream counts as seen when it is on
    /// screen: the terminal does not say whether it has the focus) and focuses the stream
    /// of a clicked one, at its line.
    fn run_notifications(&mut self) {
        let Some(settings) = self.settings.as_ref() else {
            return;
        };
        let opts = crate::notifications::Options {
            enabled: settings.config.notifications_enabled,
            always: settings.config.notify_always,
            focused: true,
        };
        let lang = settings.config.language;
        let texts = crate::notifications::Texts {
            more: crate::i18n::t(lang, "notify_more"),
            summary: crate::i18n::t(lang, "notify_summary"),
        };
        self.notifications.drain(
            self.tabs.iter_mut().map(|t| &mut t.engine),
            &opts,
            &texts,
            Instant::now(),
        );
        while let Some(target) = self.notifications.clicked() {
            let Some(i) = self
                .tabs
                .iter()
                .position(|t| t.engine.path == target.stream)
            else {
                continue;
            };
            self.focus_tab(i);
            if target.line < self.tabs[i].engine.total_lines() {
                self.tabs[i].engine.follow_tail = false;
                self.tabs[i].set_cursor(target.line);
            }
        }
    }

    /// Runs the tools bound to the rules that matched appended lines, within the limits
    /// of `ToolRunner` (one run per second per tool, 10 children), as the GUI does.
    fn run_bound_tools(&mut self) {
        let Some(settings) = self.settings.as_ref() else {
            return;
        };
        let tools = &settings.config.external_tools;
        let bound: std::collections::HashSet<String> =
            tools.iter().filter_map(|t| t.bound_rule.clone()).collect();
        for tab in &mut self.tabs {
            let e = &mut tab.engine;
            if e.tool_bound_rules != bound {
                e.tool_bound_rules = bound.clone();
            }
            if e.pending_tool_hits.is_empty() {
                continue;
            }
            let hits = std::mem::take(&mut e.pending_tool_hits);
            for (pattern, row) in hits {
                let Some(ctx) = crate::workspace::tool_context_for_row(e, row) else {
                    continue;
                };
                for tool in tools
                    .iter()
                    .filter(|t| t.bound_rule.as_deref() == Some(pattern.as_str()))
                {
                    self.tool_runner.run_bound(tool, &ctx);
                }
            }
        }
    }

    /// `F`: the global filter editor.
    fn open_global(&mut self) {
        let gf = &self.settings_mut().config.global_filter;
        self.global = Some(GlobalDialog::new(gf));
    }

    fn on_global_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        let Some(d) = self.global.as_mut() else {
            return false;
        };
        match d.on_key(key) {
            // Applied `APPLY_DELAY_MS` after the last edit (see `tick`).
            GlobalKey::Edited => {
                self.global_due = Some(
                    Instant::now() + Duration::from_millis(crate::global_filter::APPLY_DELAY_MS),
                );
            }
            GlobalKey::Close => self.close_global(),
            GlobalKey::Moved => {}
            GlobalKey::Other => return false,
        }
        true
    }

    /// Closes the editor, applying a pending edit at once.
    fn close_global(&mut self) {
        self.apply_pending_global();
        self.global = None;
        self.global_due = None;
    }

    /// The editor's pending edit, if any, reaches the streams and `fasttail.ini`.
    fn apply_pending_global(&mut self) {
        let Some(d) = self.global.as_mut().filter(|d| d.dirty) else {
            return;
        };
        d.dirty = false;
        self.global_due = None;
        let base = self.settings_mut().config.global_filter.clone();
        let gf = match &self.global {
            Some(d) => d.filter(&base),
            None => return,
        };
        self.set_global_filter(gf);
    }

    /// `f`: the global filter on or off, at once.
    fn toggle_global(&mut self) {
        let mut gf = self.settings_mut().config.global_filter.clone();
        gf.enabled = !gf.enabled;
        let state = match (gf.enabled, gf.has_terms()) {
            (true, true) => "on",
            (true, false) => tx(self.lang, "on, but it has no term: F edits it"),
            (false, _) => "off",
        };
        self.message = Some(txf(self.lang, "Global filter {0}", &[&state]));
        self.set_global_filter(gf);
    }

    /// `gf` becomes the global filter: compiled once and shared by every stream (and
    /// the streams opened later), then saved.
    fn set_global_filter(&mut self, gf: crate::global_filter::GlobalFilter) {
        let compiled = gf.compile();
        let settings = self.settings.get_or_insert_with(Default::default);
        settings.config.global_filter = gf;
        settings.global = compiled.clone();
        for tab in &mut self.tabs {
            tab.engine.set_global_filter(compiled.clone());
        }
        self.save_config();
    }

    /// `p`: the filter presets of `fasttail.ini`.
    /// `p`: the presets, pinned first, then the recently applied, then by name.
    fn open_presets(&mut self) {
        let all = self.settings_mut().config.filter_presets.clone();
        let list: Vec<_> = crate::filter_preset::display_order(&all)
            .into_iter()
            .map(|i| all[i].clone())
            .collect();
        self.presets = Some(PresetsDialog::new(&list));
    }

    fn on_presets_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        let Some(d) = self.presets.as_mut() else {
            return false;
        };
        match d.on_key(key) {
            PresetsKey::Changed => self.store_presets(),
            PresetsKey::Apply { index, all } => self.apply_preset(index, all),
            PresetsKey::Save { name, with_time } => {
                let Some(tab) = self.tabs.get(self.active) else {
                    self.message = Some(tx(self.lang, NO_FILE).into());
                    return true;
                };
                let preset = crate::filter_preset::FilterPreset {
                    state: crate::filter_preset::FilterState::of_engine(&tab.engine, with_time),
                    name,
                    ..Default::default()
                };
                self.message = Some(txf(self.lang, "Preset {0} saved", &[&preset.name]));
                if let Some(d) = self.presets.as_mut() {
                    d.saved(preset);
                }
                self.store_presets();
            }
            PresetsKey::Close => self.presets = None,
            PresetsKey::Moved => {}
            PresetsKey::Other => return false,
        }
        true
    }

    /// The dialog's presets become the configuration's, saved at once.
    fn store_presets(&mut self) {
        let Some(d) = &self.presets else {
            return;
        };
        self.settings
            .get_or_insert_with(Default::default)
            .config
            .filter_presets = d.presets();
        self.save_config();
    }

    /// Preset `index` replaces the filters of the focused stream, or of every stream,
    /// in one recomputation each; the dialog closes.
    fn apply_preset(&mut self, index: usize, all: bool) {
        let Some(p) = self
            .presets
            .as_ref()
            .and_then(|d| d.list.items.get(index).cloned())
        else {
            return;
        };
        if self.tabs.is_empty() {
            self.message = Some(tx(self.lang, NO_FILE).into());
            return;
        }
        // Applied now: among the recent ones next time.
        if let Some(d) = self.presets.as_mut() {
            crate::filter_preset::mark_used(&mut d.list.items, index);
        }
        if let Some(s) = self.settings.as_mut() {
            if let Some(i) = crate::filter_preset::find(&s.config.filter_presets, &p.name) {
                crate::filter_preset::mark_used(&mut s.config.filter_presets, i);
            }
        }
        if all {
            for tab in &mut self.tabs {
                p.apply_to(&mut tab.engine);
            }
        } else if let Some(tab) = self.tabs.get_mut(self.active) {
            p.apply_to(&mut tab.engine);
        }
        self.presets = None;
        let target = if all {
            tx(self.lang, "every stream")
        } else {
            tx(self.lang, "the stream")
        };
        self.message = Some(txf(
            self.lang,
            "Preset {0} applied to {1}",
            &[&p.name, &target],
        ));
        self.save_config();
    }

    /// `r`: the highlight-rule editor over the rules of `fasttail.ini`.
    fn open_rules(&mut self) {
        let config = &self.settings_mut().config;
        let mut d = RulesDialog::new(&config.highlight_rules, &config.external_tools);
        d.labels = self.quick_labels.clone();
        self.rules = Some(d);
    }

    fn on_rules_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        let Some(d) = self.rules.as_mut() else {
            return false;
        };
        match d.on_key(key) {
            RulesKey::Changed => self.apply_rules(),
            RulesKey::Labels => {
                self.quick_labels = d.labels.clone();
                self.sync_labels();
            }
            RulesKey::Close => self.rules = None,
            RulesKey::Import => self.open_prompt(PromptKind::RulesImport),
            RulesKey::Export => self.open_prompt(PromptKind::RulesExport),
            RulesKey::Moved => {}
            RulesKey::Other => return false,
        }
        true
    }

    /// Starts or stops the web view and the MCP server as `fasttail.ini` says, and
    /// answers their requests from the open streams; files the MCP server asked to open
    /// are opened.
    fn serve_integrations(&mut self) {
        let Some(settings) = self.settings.as_mut() else {
            return;
        };
        let config = &mut settings.config;
        let wake = || -> crate::tail_engine::WakeFn { std::sync::Arc::new(|| {}) };
        self.web_view.sync(config, wake);
        self.mcp_view.sync(config, wake);
        if let Some(e) = self.web_view.error.take() {
            self.message = Some(txf(self.lang, "Web view: {0}", &[&e]));
        }
        if let Some(e) = self.mcp_view.error.take() {
            self.message = Some(txf(self.lang, "MCP server: {0}", &[&e]));
        }
        let refs: Vec<&TailEngine> = self.tabs.iter().map(|t| &t.engine).collect();
        self.web_view.serve(
            &refs,
            &self.palette.theme,
            self.palette.level_colors,
            self.lang,
        );
        let config = config.clone();
        let mut refs: Vec<&mut TailEngine> = self.tabs.iter_mut().map(|t| &mut t.engine).collect();
        let opens = self.mcp_view.serve(&mut refs, &config);
        for path in opens {
            self.open_file(&path);
        }
    }

    fn open_plugins(&mut self) {
        self.plugins = Some(PluginsDialog {
            catalog: crate::plugins::discover(&crate::plugins::default_dir()),
            selected: 0,
            confirm: None,
        });
    }

    fn on_plugins_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        use crate::plugins::{Kind, State};
        use crossterm::event::{KeyCode, KeyEventKind};
        if key.kind == KeyEventKind::Release {
            return false;
        }
        let lang = self.lang;
        let Some(d) = self.plugins.as_mut() else {
            return false;
        };
        if let Some(name) = d.confirm.take() {
            if key.code == KeyCode::Char('y') {
                if let Some(p) = d.catalog.find(&name).cloned() {
                    self.settings_mut().config.plugins.set_enabled(&p, true);
                    self.plugins_changed();
                    self.message = Some(txf(lang, "{0} enabled", &[&name]));
                }
            }
            return true;
        }
        let rows = d.catalog.plugins.len() + d.catalog.broken.len();
        match key.code {
            KeyCode::Esc => self.plugins = None,
            KeyCode::Up | KeyCode::Char('k') => d.selected = d.selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                d.selected = (d.selected + 1).min(rows.saturating_sub(1))
            }
            KeyCode::Char('r') => {
                d.catalog = crate::plugins::discover(&crate::plugins::default_dir());
                d.selected = d.selected.min(rows.saturating_sub(1));
            }
            KeyCode::Char(' ') | KeyCode::Enter => {
                let Some(p) = d.catalog.plugins.get(d.selected).cloned() else {
                    return true;
                };
                let state = self.settings_mut().config.plugins.state(&p);
                if state == State::Enabled {
                    self.settings_mut().config.plugins.set_enabled(&p, false);
                    self.plugins_changed();
                    self.message = Some(txf(lang, "{0} disabled", &[&p.name]));
                } else if p.kind == Kind::Source {
                    // A program the plugin runs: approved only after its hash is seen.
                    if let Some(d) = self.plugins.as_mut() {
                        d.confirm = Some(p.name.clone());
                    }
                } else {
                    self.settings_mut().config.plugins.set_enabled(&p, true);
                    self.plugins_changed();
                    self.message = Some(txf(lang, "{0} enabled", &[&p.name]));
                }
            }
            KeyCode::Char('o') => {
                let source = d
                    .catalog
                    .plugins
                    .get(d.selected)
                    .filter(|p| p.kind == Kind::Source)
                    .cloned();
                match source {
                    Some(p) if self.settings_mut().config.plugins.state(&p) == State::Enabled => {
                        self.open_prompt(PromptKind::OpenPlugin)
                    }
                    Some(_) => {
                        self.message = Some(tx(lang, "Enable the plugin first (Space)").into())
                    }
                    None => {}
                }
            }
            _ => return false,
        }
        true
    }

    /// After a change of the approvals: saved, the formats of the plugins loaded again,
    /// and the plugin streams no longer enabled stopped.
    fn plugins_changed(&mut self) {
        use crate::plugin_source::{Block, Status};
        self.save_config();
        let approvals = self.settings_mut().config.plugins.clone();
        crate::log_format::load_with_plugins(&approvals);
        for tab in &mut self.tabs {
            tab.engine.reload_field_parser();
        }
        let Some(catalog) = self.plugins.as_ref().map(|d| d.catalog.clone()) else {
            return;
        };
        for s in self
            .tabs
            .iter_mut()
            .filter_map(|t| t.engine.plugin.as_mut())
        {
            if !matches!(s.status(), Status::Running | Status::Restarting { .. }) {
                continue;
            }
            let block = match catalog.find(&s.uri.name).map(|p| approvals.state(p)) {
                Some(crate::plugins::State::Enabled) => continue,
                Some(crate::plugins::State::Changed) => Block::Changed,
                Some(crate::plugins::State::Disabled) => Block::NotEnabled,
                None => Block::Missing,
            };
            s.block(block);
        }
    }

    /// Opens the source plugin of `text` (`plugin://name?key=value`) as a stream, its
    /// values given to the program (secrets included, which the stream's name leaves out).
    fn open_plugin(&mut self, text: &str) {
        let Some(uri) = crate::plugins::PluginUri::parse(Path::new(text.trim())) else {
            self.message = Some(txf(self.lang, "Not a plugin URI: {0}", &[&text]));
            return;
        };
        let catalog = crate::plugins::discover(&crate::plugins::default_dir());
        let plugin = catalog.find(&uri.name).cloned();
        let path = PathBuf::from(uri.to_uri(plugin.as_ref()));
        self.open_file(&path);
        let config_dir = self
            .settings
            .as_ref()
            .and_then(|s| s.path.parent().map(Path::to_path_buf))
            .unwrap_or_default();
        let approvals = self.settings_mut().config.plugins.clone();
        let lang = self.lang;
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.engine.path == path) {
            if let Some(s) = tab.engine.plugin.as_mut() {
                if s.status() != crate::plugin_source::Status::Running
                    && s.start(plugin.as_ref(), &approvals, Some(uri.values), &config_dir)
                        .is_err()
                {
                    self.message = Some(tx(lang, "The plugin needs other values").into());
                }
            }
        }
        self.plugins = None;
    }

    /// The plugins found: name, kind, version and whether it runs; the folders that did
    /// not load in the error colour; a source plugin's hash before it is approved.
    fn draw_plugins(&mut self, frame: &mut Frame, area: Rect) {
        use crate::plugins::{Kind, State};
        let palette = self.palette;
        let dim = Style::default().fg(palette.dim());
        let error = Style::default().fg(palette.level_color(LogLevel::Error));
        let approvals = self
            .settings
            .as_ref()
            .map(|s| s.config.plugins.clone())
            .unwrap_or_default();
        let Some(d) = self.plugins.as_ref() else {
            return;
        };
        let rows = d.catalog.plugins.len() + d.catalog.broken.len();
        let height = (rows as u16 + 7).min(area.height);
        let inner = self.dialog(frame, area, (84, height), tx(self.lang, "Plugins"), false);
        let Some(d) = self.plugins.as_ref() else {
            return;
        };
        let cells = inner.width as usize;
        let mut out = vec![Line::styled(
            tx(
                self.lang,
                "Space enables or disables, o opens a source plugin, r reads the folder again, Esc closes",
            ),
            dim,
        )];
        out.push(Line::styled(
            head_chars(&d.catalog.dir.display().to_string(), cells),
            dim,
        ));
        if let Some(name) = &d.confirm {
            let hash = d
                .catalog
                .find(name)
                .map(|p| p.hash.clone())
                .unwrap_or_default();
            out.push(Line::styled(
                head_chars(
                    &txf(
                        self.lang,
                        "y runs the programs of {0}, any other key does not (manifest {1})",
                        &[name, &hash],
                    ),
                    cells,
                ),
                error,
            ));
        }
        if rows == 0 {
            out.push(Line::styled(tx(self.lang, "(no plugin installed)"), dim));
        }
        for (i, p) in d.catalog.plugins.iter().enumerate() {
            let state = match approvals.state(p) {
                State::Enabled => tx(self.lang, "enabled"),
                State::Disabled => tx(self.lang, "disabled"),
                State::Changed => tx(self.lang, "changed since approved"),
            };
            let kind = match p.kind {
                Kind::Source => tx(self.lang, "source"),
                Kind::Formats => tx(self.lang, "formats"),
            };
            let style = if i == d.selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            let text = format!(
                " {:<24} {:<8} {:<10} {:<22} {}",
                head_chars(&p.name, 24),
                kind,
                head_chars(&p.version, 10),
                state,
                p.description
            );
            out.push(Line::styled(head_chars(&text, cells), style));
        }
        for (j, b) in d.catalog.broken.iter().enumerate() {
            let i = d.catalog.plugins.len() + j;
            let style = if i == d.selected {
                error.add_modifier(Modifier::REVERSED)
            } else {
                error
            };
            let name = b
                .dir
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            out.push(Line::styled(
                head_chars(&format!(" {name}: {}", b.error), cells),
                style,
            ));
        }
        frame.render_widget(Paragraph::new(out), inner);
    }

    /// A boolean setting of the palette flipped, applied and saved.
    fn toggle_setting(&mut self, setting: crate::actions::BoolSetting) {
        let meta = crate::actions::bool_setting(setting);
        let config = &mut self.settings_mut().config;
        let on = !(meta.get)(config);
        (meta.set)(config, on);
        self.apply_running_config();
        self.save_config();
        let name = crate::i18n::t(self.lang, meta.name);
        self.message = Some(if on {
            txf(self.lang, "{0}: on", &[&name])
        } else {
            txf(self.lang, "{0}: off", &[&name])
        });
    }

    /// Sets the `[shortcuts]` entry of the terminal action `name` (`None`: its default
    /// keys again), then rebuilds the keys and saves.
    fn set_shortcut(&mut self, name: &str, keys: Option<String>) {
        let key = format!("{}{name}", keys::TUI_PREFIX);
        let config = &mut self.settings_mut().config;
        match keys {
            Some(k) => {
                config.shortcuts.insert(key, k);
            }
            None => {
                config.shortcuts.remove(&key);
            }
        }
        self.keymap = keys::KeyMap::build(&self.settings_mut().config.shortcuts);
        self.save_config();
    }

    fn on_keys_dialog_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        use crossterm::event::{KeyCode, KeyEventKind};
        if key.kind == KeyEventKind::Release {
            return false;
        }
        let Some(d) = self.keys_dialog.as_mut() else {
            return false;
        };
        let last = keys::REMAPPABLE.len() - 1;
        let name = keys::REMAPPABLE[d.selected.min(last)].0;
        if d.capturing {
            d.capturing = false;
            if key.code == KeyCode::Esc {
                d.notice = None;
                return true;
            }
            let k = keys::TuiKey::from_event(key);
            let holder = self.keymap.holder(k).filter(|h| *h != name);
            // The action that had the key keeps its other keys.
            if let Some(other) = holder {
                let rest: Vec<String> = self
                    .keymap
                    .keys_text(other)
                    .0
                    .split_whitespace()
                    .filter(|t| keys::TuiKey::parse(t) != Some(k))
                    .map(str::to_string)
                    .collect();
                self.set_shortcut(other, Some(rest.join(", ")));
            }
            self.set_shortcut(name, Some(k.text()));
            let lang = self.lang;
            if let Some(d) = self.keys_dialog.as_mut() {
                d.notice = Some(match holder {
                    Some(other) => txf(
                        lang,
                        "{0}: {1} (taken from {2})",
                        &[&name, &k.text(), &other],
                    ),
                    None => format!("{name}: {}", k.text()),
                });
            }
            return true;
        }
        match key.code {
            KeyCode::Esc => self.keys_dialog = None,
            KeyCode::Up | KeyCode::Char('k') => d.selected = d.selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => d.selected = (d.selected + 1).min(last),
            KeyCode::PageUp => d.selected = d.selected.saturating_sub(10),
            KeyCode::PageDown => d.selected = (d.selected + 10).min(last),
            KeyCode::Home => d.selected = 0,
            KeyCode::End => d.selected = last,
            KeyCode::Enter => {
                d.capturing = true;
                d.notice = Some(txf(
                    self.lang,
                    "Press the new key for {0} (Esc cancels)",
                    &[&name],
                ));
            }
            KeyCode::Delete | KeyCode::Char('d') => {
                d.notice = Some(txf(self.lang, "{0}: no key", &[&name]));
                self.set_shortcut(name, Some(String::new()));
            }
            KeyCode::Char('r') => {
                d.notice = Some(txf(self.lang, "{0}: default keys", &[&name]));
                self.set_shortcut(name, None);
            }
            KeyCode::Char('R') => {
                self.settings_mut()
                    .config
                    .shortcuts
                    .retain(|k, _| !k.starts_with(keys::TUI_PREFIX));
                self.keymap = keys::KeyMap::build(&self.settings_mut().config.shortcuts);
                self.save_config();
                if let Some(d) = self.keys_dialog.as_mut() {
                    d.notice = Some(tx(self.lang, "Every key is back to its default").into());
                }
            }
            _ => return false,
        }
        true
    }

    /// The remappable keys: the action's name, its keys (`*` when set in `[shortcuts]`).
    fn draw_keys_dialog(&mut self, frame: &mut Frame, area: Rect) {
        let dim = Style::default().fg(self.palette.dim());
        let accent = Style::default().fg(self.palette.accent());
        let height = area.height.saturating_sub(2).clamp(8, 40);
        let inner = self.dialog(frame, area, (64, height), tx(self.lang, "Keys"), false);
        let lang = self.lang;
        let notices = self.keymap.notices.clone();
        let rows: Vec<(String, bool)> = keys::REMAPPABLE
            .iter()
            .map(|(n, _, _)| self.keymap.keys_text(n))
            .collect();
        let Some(d) = self.keys_dialog.as_mut() else {
            return;
        };
        let mut out = vec![Line::styled(
            tx(
                lang,
                "Enter records a key, d no key, r default, R all defaults, Esc closes",
            ),
            dim,
        )];
        let info = d.notice.clone().or_else(|| notices.first().cloned());
        out.push(Line::styled(
            head_chars(info.as_deref().unwrap_or(""), inner.width as usize),
            accent,
        ));
        let room = (inner.height as usize).saturating_sub(out.len());
        if d.selected < d.top {
            d.top = d.selected;
        } else if room > 0 && d.selected >= d.top + room {
            d.top = d.selected + 1 - room;
        }
        for (i, ((name, _, _), (text, set))) in keys::REMAPPABLE
            .iter()
            .zip(&rows)
            .enumerate()
            .skip(d.top)
            .take(room)
        {
            let style = if i == d.selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else if *set {
                accent
            } else {
                Style::default()
            };
            let mark = if *set { '*' } else { ' ' };
            let shown = if text.is_empty() { "-" } else { text.as_str() };
            out.push(Line::styled(format!(" {name:<22}{mark} {shown}"), style));
        }
        frame.render_widget(Paragraph::new(out), inner);
    }

    /// Opens the analysis view `kind` on the focused stream: Statistics and Operations ask
    /// for their field, Explain for its span, Patterns start learning at once.
    fn open_analysis(&mut self, kind: crate::tui::analysis::Kind) {
        use crate::tui::analysis::{AnalysisView, Kind};
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        match kind {
            Kind::Stats => self.open_prompt(PromptKind::StatsField),
            Kind::Operations => self.open_prompt(PromptKind::OpsField),
            Kind::Explain => self.open_prompt(PromptKind::ExplainSpan),
            Kind::Patterns => {
                if tab.engine.patterns.is_none() {
                    tab.engine.open_patterns();
                }
                self.analysis = Some(AnalysisView::new(kind, tab.engine.path.clone()));
            }
            Kind::Query => {
                let mut v = AnalysisView::new(kind, tab.engine.path.clone());
                if let Some(q) = &tab.engine.query {
                    v.query = TextField::new(&q.text);
                }
                self.analysis = Some(v);
            }
        }
    }

    /// The Statistics (`stats`) or the Operations of the focused stream over `source`.
    fn open_source_view(&mut self, source: crate::field_stats::StatsSource, stats: bool) {
        use crate::tui::analysis::{AnalysisView, Kind};
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let kind = if stats {
            tab.engine
                .open_stats(source, crate::field_stats::SliceChoice::Auto);
            Kind::Stats
        } else {
            tab.engine
                .open_operations(source, crate::operations::NameSource::Message);
            Kind::Operations
        };
        self.analysis = Some(AnalysisView::new(kind, tab.engine.path.clone()));
    }

    /// The palette's workspace commands, with or without a stream open: the recent files
    /// and sessions, saving the session or the default one. Returns whether `action` was
    /// one of them.
    fn apply_workspace_action(&mut self, action: Action) -> bool {
        match action {
            Action::RecentFiles => {
                let empty = self
                    .settings
                    .as_ref()
                    .is_none_or(|s| s.config.recent_files.is_empty());
                if empty {
                    self.message = Some(tx(self.lang, "No recent files").into());
                } else {
                    self.recent_menu = Some(0);
                }
            }
            Action::ClearRecentFiles => {
                if let Some(s) = self.settings.as_mut() {
                    s.config.recent_files.clear();
                }
                self.message = Some(tx(self.lang, "Recent files cleared").into());
                self.save_config();
            }
            Action::ClearRecentSessions => {
                if let Some(s) = self.settings.as_mut() {
                    s.config.recent_sessions.clear();
                }
                self.message = Some(tx(self.lang, "Recent sessions cleared").into());
                self.save_config();
            }
            Action::SaveCurrentSession => {
                match self
                    .settings
                    .as_ref()
                    .and_then(|s| s.config.current_session.clone())
                {
                    Some(file) => self.save_session(&file),
                    None => self.open_prompt(PromptKind::SaveSession),
                }
            }
            Action::SaveDefaultSession => {
                // The workspace becomes the default session: the named one is left.
                if let Some(s) = self.settings.as_mut() {
                    s.config.current_session = None;
                }
                self.dock_dirty = true;
                self.save_config();
                if self.save_error.is_none() {
                    self.message = Some(tx(self.lang, "Saved as the default session").into());
                }
            }
            _ => return false,
        }
        true
    }

    fn on_recent_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        use crossterm::event::{KeyCode, KeyEventKind};
        if key.kind == KeyEventKind::Release {
            return false;
        }
        let files: Vec<PathBuf> = self
            .settings
            .as_ref()
            .map(|s| s.config.recent_files.clone())
            .unwrap_or_default();
        let Some(selected) = self.recent_menu.as_mut() else {
            return false;
        };
        let last = files.len().saturating_sub(1);
        match key.code {
            KeyCode::Esc => self.recent_menu = None,
            KeyCode::Up | KeyCode::Char('k') => *selected = selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => *selected = (*selected + 1).min(last),
            KeyCode::Enter => {
                let at = *selected;
                self.recent_menu = None;
                if let Some(path) = files.get(at) {
                    self.open_file(path);
                }
            }
            KeyCode::Delete | KeyCode::Char('d') => {
                let at = *selected;
                if let Some(s) = self.settings.as_mut() {
                    if at < s.config.recent_files.len() {
                        s.config.recent_files.remove(at);
                    }
                    let left = s.config.recent_files.len();
                    if left == 0 {
                        self.recent_menu = None;
                    } else if let Some(sel) = self.recent_menu.as_mut() {
                        *sel = (*sel).min(left - 1);
                    }
                }
                self.save_config();
            }
            KeyCode::Char('c') => {
                self.recent_menu = None;
                self.apply_workspace_action(Action::ClearRecentFiles);
            }
            _ => return false,
        }
        true
    }

    /// The stream information (`Alt+I`), labels in the window's words.
    fn draw_info(&mut self, frame: &mut Frame, area: Rect) {
        use crate::stream_info::Value;
        let Some(path) = self.info_view.clone() else {
            return;
        };
        let Some(i) = self.tab_index(&path) else {
            self.info_view = None;
            return;
        };
        let lang = self.lang;
        let rows: Vec<(String, String)> = crate::stream_info::rows(&self.tabs[i].engine)
            .into_iter()
            .map(|(item, value)| {
                let text = match value {
                    Value::Text(s) => s,
                    Value::Key(k) => crate::i18n::t(lang, k).to_string(),
                };
                (crate::i18n::t(lang, item.key()).to_string(), text)
            })
            .collect();
        let label_w = rows.iter().map(|(l, _)| width(l)).max().unwrap_or(10);
        let value_w = rows.iter().map(|(_, v)| width(v)).max().unwrap_or(10);
        let w = (label_w + value_w + 6).clamp(40, area.width.saturating_sub(4) as usize) as u16;
        let title = format!(
            "{} · {}",
            crate::i18n::t(lang, "info_title"),
            self.tabs[i].title
        );
        let inner = self.dialog(frame, area, (w, rows.len() as u16 + 3), &title, false);
        let dim = Style::default().fg(self.palette.dim());
        let out: Vec<Line> = rows
            .into_iter()
            .map(|(l, v)| {
                Line::from(vec![
                    Span::styled(format!(" {l:<label_w$}  "), dim),
                    Span::raw(v),
                ])
            })
            .collect();
        frame.render_widget(Paragraph::new(out), inner);
    }

    fn draw_recent(&mut self, frame: &mut Frame, area: Rect) {
        let dim = Style::default().fg(self.palette.dim());
        let Some(selected) = self.recent_menu else {
            return;
        };
        let files: Vec<PathBuf> = self
            .settings
            .as_ref()
            .map(|s| s.config.recent_files.clone())
            .unwrap_or_default();
        let help = tx(
            self.lang,
            "Enter opens, Delete removes, c clears the list, Esc closes",
        );
        let width = files
            .iter()
            .map(|p| p.display().to_string().chars().count() as u16 + 4)
            .chain(std::iter::once(help.chars().count() as u16 + 4))
            .max()
            .unwrap_or(40)
            .clamp(40, area.width.saturating_sub(4).max(40));
        let room = area.height.saturating_sub(6).max(1) as usize;
        let shown = files.len().min(room);
        let inner = self.dialog(
            frame,
            area,
            (width, shown as u16 + 4),
            tx(self.lang, "Recent files"),
            false,
        );
        let top = selected.saturating_sub(shown.saturating_sub(1));
        let mut out = vec![Line::styled(help, dim)];
        for (i, path) in files.iter().enumerate().skip(top).take(shown) {
            let y = inner.y + 1 + (i - top) as u16;
            self.hits
                .list_items
                .push((Rect::new(inner.x, y, inner.width, 1), i));
            let style = if i == selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            out.push(Line::styled(format!(" {}", path.display()), style));
        }
        frame.render_widget(Paragraph::new(out), inner);
    }

    /// `Alt+M`: on a merged view, its sources on and off; elsewhere the open streams with
    /// timestamps to merge, the focused one checked.
    fn open_merge(&mut self) {
        use crate::tui::merge::MergeDialog;
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        if let Some(m) = &tab.engine.merged {
            let sources = m
                .sources
                .iter()
                .map(|s| (s.path.clone(), s.name.clone(), s.enabled))
                .collect();
            self.merge = Some(MergeDialog::sources(tab.engine.path.clone(), sources));
            return;
        }
        let streams: Vec<(PathBuf, String)> = self
            .tabs
            .iter()
            .filter(|t| {
                t.engine.merged.is_none() && crate::merged_stream::has_timestamps(&t.engine)
            })
            .take(crate::merged_stream::MAX_SOURCES)
            .map(|t| (t.engine.path.clone(), t.title.clone()))
            .collect();
        if streams.len() < 2 {
            self.message =
                Some(tx(self.lang, "Merging needs two open streams with timestamps").into());
            return;
        }
        let focused = tab.engine.path.clone();
        self.merge = Some(MergeDialog::new(streams, &[focused]));
    }

    fn on_merge_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        use crate::tui::merge::MergeKey;
        let Some(d) = self.merge.as_mut() else {
            return false;
        };
        match d.on_key(key) {
            MergeKey::Close => self.merge = None,
            MergeKey::Merge(paths) if paths.len() < 2 => {
                self.message = Some(tx(self.lang, "Check two streams or more (Space)").into());
            }
            MergeKey::Merge(paths) => {
                self.merge = None;
                self.open_merged_view(&paths);
            }
            MergeKey::Source(i, on) => {
                let view = d.view.clone();
                if let Some(t) = view.and_then(|v| self.tab_index(&v)) {
                    if let Some(m) = self.tabs[t].engine.merged.as_mut() {
                        m.set_enabled(i, on);
                    }
                }
            }
            MergeKey::Moved => {}
            MergeKey::Other => return false,
        }
        true
    }

    /// `Alt+T`: the focused stream's filter (include / exclude, level, time range, hidden
    /// lines) frozen into a new tab of the same file that follows it, as the window's
    /// "Open filter as new tab"; the source keeps its filter.
    fn open_filter_tab(&mut self) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        if tab.engine.time_range_pending() {
            self.message = Some(tx(self.lang, "The time range is not applied yet").into());
            return;
        }
        let filter = crate::filter_tab::FrozenFilter::of(&tab.engine);
        if !filter.is_active() {
            self.message = Some(
                tx(
                    self.lang,
                    "No filter to open as a tab: set an include, exclude, level or time filter first",
                )
                .into(),
            );
            return;
        }
        let n =
            crate::filter_tab::next_number(self.tabs.iter().map(|t| &t.engine), &tab.engine.path);
        match self.filter_tab_engine(self.active, n, filter) {
            Ok(engine) => self.add_engine_tab(engine),
            Err(e) => self.message = Some(e),
        }
    }

    /// The engine of filter tab `n` of the stream at `idx`, with the global filter and the
    /// quick labels.
    fn filter_tab_engine(
        &self,
        idx: usize,
        n: usize,
        filter: crate::filter_tab::FrozenFilter,
    ) -> Result<TailEngine, String> {
        let default_config;
        let config = match &self.settings {
            Some(s) => &s.config,
            None => {
                default_config = crate::config::FastTailConfig::default();
                &default_config
            }
        };
        let mut engine = crate::filter_tab::open_tab(&self.tabs[idx].engine, n, filter, config)?;
        if let Some(s) = &self.settings {
            if s.global.is_some() {
                engine.set_global_filter(s.global.clone());
            }
        }
        engine.set_quick_labels(&self.quick_labels);
        Ok(engine)
    }

    /// A filter tab asked to show a line in its source (`Ctrl+K`): the source, opened
    /// again when it was closed, shows it in context.
    fn apply_source_context(&mut self) {
        let Some(d) = self
            .tabs
            .iter()
            .position(|t| t.engine.source_context_request.is_some())
        else {
            return;
        };
        let line = self.tabs[d]
            .engine
            .source_context_request
            .take()
            .unwrap_or(0);
        let Some(source) = self.tabs[d]
            .engine
            .derived
            .as_ref()
            .map(|f| f.source.clone())
        else {
            return;
        };
        if self.tab_index(&source).is_none() {
            self.open_file(&source);
        }
        let Some(s) = self.tab_index(&source) else {
            return;
        };
        self.focus_tab(s);
        let tab = &mut self.tabs[s];
        if !tab.engine.enter_context(line) {
            tab.engine.scroll_to_line = Some(line);
        }
        tab.engine.follow_tail = false;
    }

    /// A merged view of the open streams `sources` in a new tab of the focused window.
    fn open_merged_view(&mut self, sources: &[PathBuf]) {
        let n = crate::merged_stream::next_number(self.tabs.iter().map(|t| &t.engine));
        match self.merged_engine(n, sources) {
            Ok(engine) => self.add_engine_tab(engine),
            Err(e) => self.message = Some(e),
        }
    }

    /// The engine of merged view `n` of the open streams `sources`, with the global
    /// filter and the quick labels.
    fn merged_engine(&self, n: usize, sources: &[PathBuf]) -> Result<TailEngine, String> {
        let found: Vec<&TailEngine> = sources
            .iter()
            .filter_map(|p| self.tab_index(p).map(|i| &self.tabs[i].engine))
            .collect();
        if found.len() < 2 {
            return Err(tx(self.lang, "Check two streams or more (Space)").into());
        }
        let default_config;
        let config = match &self.settings {
            Some(s) => &s.config,
            None => {
                default_config = crate::config::FastTailConfig::default();
                &default_config
            }
        };
        let mut engine = crate::merged_stream::open_view(n, &found, config)
            .map_err(|e| txf(self.lang, "Cannot merge: {0}", &[&e]))?;
        if let Some(s) = &self.settings {
            if s.global.is_some() {
                engine.set_global_filter(s.global.clone());
            }
        }
        engine.set_quick_labels(&self.quick_labels);
        Ok(engine)
    }

    /// Rebuilds the merged views saved as `views` (`merged:<n>:<sources>`, from the
    /// workspace or a session) from their open sources, with the state saved for them;
    /// a view with fewer than two of its sources open is skipped. The tabs are not placed:
    /// `restore_dock` follows.
    pub fn restore_views(&mut self, views: &[PathBuf]) {
        // The filter tabs first: a merged view may read one.
        for identity in views {
            let Some((n, source)) = crate::filter_tab::parse_identity(identity) else {
                continue;
            };
            let Some(filter) = self
                .settings
                .as_ref()
                .and_then(|s| s.config.stream_state_for(identity))
                .and_then(|state| state.frozen.clone())
            else {
                continue;
            };
            let Some(idx) = self.tabs.iter().position(|t| {
                t.engine.derived.is_none()
                    && crate::paths::paths_equal(
                        &crate::workspace::persisted_path(&t.engine),
                        &source,
                    )
            }) else {
                continue;
            };
            let Ok(mut engine) = self.filter_tab_engine(idx, n, filter) else {
                continue;
            };
            if let Some(s) = &self.settings {
                crate::workspace::restore_stream(&mut engine, &s.config, identity);
            }
            self.tabs.push(Tab::new(engine));
        }
        for identity in views {
            let Some((n, saved)) = crate::merged_stream::parse_identity(identity) else {
                continue;
            };
            let sources: Vec<PathBuf> = saved
                .iter()
                .filter_map(|s| {
                    self.tabs
                        .iter()
                        .find(|t| {
                            crate::paths::paths_equal(
                                &crate::workspace::persisted_path(&t.engine),
                                s,
                            )
                        })
                        .map(|t| t.engine.path.clone())
                })
                .collect();
            let Ok(mut engine) = self.merged_engine(n, &sources) else {
                continue;
            };
            if let Some(s) = &self.settings {
                crate::workspace::restore_stream(&mut engine, &s.config, identity);
            }
            self.tabs.push(Tab::new(engine));
        }
    }

    fn draw_merge(&mut self, frame: &mut Frame, area: Rect) {
        let dim = Style::default().fg(self.palette.dim());
        let Some(d) = self.merge.clone() else {
            return;
        };
        let (title, help) = if d.view.is_some() {
            (
                tx(self.lang, "Merged view sources"),
                tx(self.lang, "Space turns a source on or off, Esc closes"),
            )
        } else {
            (
                tx(self.lang, "Merged view of streams"),
                tx(self.lang, "Space checks, Enter merges by time, Esc closes"),
            )
        };
        let width = d
            .items
            .iter()
            .map(|i| unicode_width::UnicodeWidthStr::width(i.name.as_str()) as u16 + 8)
            .chain(std::iter::once(
                unicode_width::UnicodeWidthStr::width(help) as u16 + 4,
            ))
            .max()
            .unwrap_or(40)
            .clamp(40, 100);
        let inner = self.dialog(frame, area, (width, d.items.len() as u16 + 4), title, false);
        let mut out = vec![Line::styled(help, dim)];
        for (i, item) in d.items.iter().enumerate() {
            let y = inner.y + 1 + i as u16;
            self.hits
                .list_items
                .push((Rect::new(inner.x, y, inner.width, 1), i));
            let style = if i == d.selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default().fg(source_color(i))
            };
            let mark = if item.checked { "[x]" } else { "[ ]" };
            out.push(Line::styled(format!(" {mark} {}", item.name), style));
        }
        frame.render_widget(Paragraph::new(out), inner);
    }

    fn on_analysis_menu_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        use crate::tui::analysis::Kind;
        use crossterm::event::{KeyCode, KeyEventKind};
        if key.kind == KeyEventKind::Release {
            return false;
        }
        let Some(selected) = self.analysis_menu.as_mut() else {
            return false;
        };
        match key.code {
            KeyCode::Esc => self.analysis_menu = None,
            KeyCode::Up | KeyCode::Char('k') => *selected = selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                *selected = (*selected + 1).min(Kind::ALL.len() - 1)
            }
            KeyCode::Enter => {
                let kind = Kind::ALL[*selected];
                self.analysis_menu = None;
                self.open_analysis(kind);
            }
            _ => return false,
        }
        true
    }

    fn draw_analysis_menu(&mut self, frame: &mut Frame, area: Rect) {
        use crate::tui::analysis::Kind;
        let dim = Style::default().fg(self.palette.dim());
        let Some(selected) = self.analysis_menu else {
            return;
        };
        let inner = self.dialog(
            frame,
            area,
            (48, Kind::ALL.len() as u16 + 4),
            tx(self.lang, "Analysis"),
            false,
        );
        let mut out = vec![Line::styled(
            tx(self.lang, "Enter opens, Up/Down choose, Esc closes"),
            dim,
        )];
        for (i, kind) in Kind::ALL.iter().enumerate() {
            let y = inner.y + 1 + i as u16;
            self.hits
                .list_items
                .push((Rect::new(inner.x, y, inner.width, 1), i));
            let style = if i == selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            out.push(Line::styled(
                format!(" {}", tx(self.lang, kind.title())),
                style,
            ));
        }
        frame.render_widget(Paragraph::new(out), inner);
    }

    /// The tab the analysis view reads, when it is still open.
    fn analysis_tab(&self) -> Option<usize> {
        let path = &self.analysis.as_ref()?.path;
        self.tabs.iter().position(|t| &t.engine.path == path)
    }

    fn on_analysis_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        use crate::tui::analysis::{build, AnalysisKey, Kind, RowAction};
        let Some(i) = self.analysis_tab() else {
            self.analysis = None;
            return true;
        };
        let (lang, ascii) = (self.lang, self.palette.ascii);
        let Some(kind) = self.analysis.as_ref().map(|v| v.kind) else {
            return false;
        };
        let table = build(kind, &mut self.tabs[i].engine, lang, ascii);
        let Some(v) = self.analysis.as_mut() else {
            return false;
        };
        match v.on_key(key, &table) {
            AnalysisKey::Moved => {}
            AnalysisKey::Other => return false,
            AnalysisKey::Close => {
                // Patterns and Explain keep counting only while shown, as in the window.
                let e = &mut self.tabs[i].engine;
                match kind {
                    Kind::Stats => e.close_stats(),
                    Kind::Operations => e.close_operations(),
                    Kind::Explain => e.close_explain(),
                    Kind::Patterns if e.pattern_filter().is_none() => e.close_patterns(),
                    _ => {}
                }
                self.analysis = None;
            }
            AnalysisKey::RunQuery(text) => {
                if !text.is_empty() {
                    self.tabs[i].engine.run_query(&text);
                    if let Some(v) = self.analysis.as_mut() {
                        v.editing = false;
                        v.selected = 0;
                    }
                }
            }
            AnalysisKey::Activate(action, exclude) => {
                let e = &mut self.tabs[i].engine;
                match action {
                    RowAction::None => {}
                    RowAction::Value(value) => {
                        if let Some(source) = e.stats.as_ref().map(|s| s.source.clone()) {
                            let term =
                                crate::field_stats::filter_term(&source, &value, e.filter_is_regex);
                            e.add_filter_term(&term, !exclude);
                            self.message = Some(if exclude {
                                txf(lang, "Excluded {0}", &[&term])
                            } else {
                                txf(lang, "Included {0}", &[&term])
                            });
                        }
                    }
                    RowAction::Pair(key, value) => {
                        let source = crate::field_stats::StatsSource::Field(key);
                        let term =
                            crate::field_stats::filter_term(&source, &value, e.filter_is_regex);
                        e.add_filter_term(&term, !exclude);
                        self.message = Some(if exclude {
                            txf(lang, "Excluded {0}", &[&term])
                        } else {
                            txf(lang, "Included {0}", &[&term])
                        });
                    }
                    RowAction::Pattern(id) => {
                        let (mut inc, mut exc) = e
                            .pattern_filter()
                            .map(|f| (f.include.clone(), f.exclude.clone()))
                            .unwrap_or_default();
                        if exclude {
                            inc.retain(|&x| x != id);
                            if !exc.contains(&id) {
                                exc.push(id);
                            }
                        } else {
                            inc = vec![id];
                            exc.clear();
                        }
                        e.set_pattern_filter(inc, exc);
                        self.analysis = None;
                        self.tabs[i].top = 0;
                    }
                    RowAction::Line(line) => {
                        self.analysis = None;
                        self.focus_tab(i);
                        let tab = &mut self.tabs[i];
                        tab.engine.follow_tail = false;
                        let current = tab.cursor_line().unwrap_or(0);
                        let target = tab.engine.resolve_goto(&(line + 1).to_string(), current);
                        self.message = goto_target(tab, target, &(line + 1).to_string(), lang);
                    }
                }
            }
        }
        true
    }

    /// The analysis view over the windows: its summary, then the table with its header,
    /// the selected row reversed, numbers to the right.
    fn draw_analysis(&mut self, frame: &mut Frame, area: Rect) {
        use crate::tui::analysis::{build, Kind};
        let palette = self.palette;
        let dim = Style::default().fg(palette.dim());
        let error = Style::default().fg(palette.level_color(LogLevel::Error));
        let Some(i) = self.analysis_tab() else {
            self.analysis = None;
            return;
        };
        let Some(kind) = self.analysis.as_ref().map(|v| v.kind) else {
            return;
        };
        let (lang, ascii) = (self.lang, palette.ascii);
        let table = build(kind, &mut self.tabs[i].engine, lang, ascii);
        let title = format!(
            "{} - {}",
            tx(lang, kind.title()),
            stream_title(&self.tabs[i].engine)
        );
        let inner = self.dialog(frame, area, (area.width, area.height), &title, false);
        let Some(v) = self.analysis.as_mut() else {
            return;
        };
        let width = inner.width as usize;
        let mut out = Vec::new();
        let mut cursor = None;
        if kind == Kind::Query {
            let (shown, x) = v.query.view(width.saturating_sub(2));
            let style = if v.editing {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            if v.editing {
                cursor = Some((inner.x + 2 + x as u16, inner.y));
            }
            out.push(Line::from(vec![
                Span::styled("> ", dim),
                Span::styled(
                    format!(
                        "{:<w$}",
                        view::sanitize(&shown),
                        w = width.saturating_sub(2)
                    ),
                    style,
                ),
            ]));
        }
        let hint = if v.narrowing || !v.narrow.text().is_empty() {
            format!("/{}", v.narrow.text())
        } else if kind == Kind::Query && v.editing {
            tx(
                lang,
                "Enter runs the query, Tab goes to the rows, Esc closes",
            )
            .to_string()
        } else {
            tx(
                lang,
                "Enter filters or goes to the line, x excludes, s/S sort, / narrows, Esc closes",
            )
            .to_string()
        };
        out.push(Line::styled(head_chars(&hint, width), dim));
        for line in &table.summary {
            out.push(Line::styled(head_chars(line, width), dim));
        }
        if let Some(e) = &table.error {
            out.push(Line::styled(head_chars(e, width), error));
        }
        let shown = v.shown(&table);
        // Column widths: as wide as the widest cell on screen, the last takes the rest.
        let n = table.header.len();
        if n > 0 {
            let room = (inner.height as usize).saturating_sub(out.len() + 1);
            v.page = room.max(1);
            v.selected = v.selected.min(shown.len().saturating_sub(1));
            if v.selected < v.top {
                v.top = v.selected;
            } else if v.selected >= v.top + v.page {
                v.top = v.selected + 1 - v.page;
            }
            let rows: Vec<usize> = shown.iter().skip(v.top).take(room).copied().collect();
            let cell_w = |s: &str| unicode_width::UnicodeWidthStr::width(s);
            let mut widths: Vec<usize> = (0..n)
                .map(|c| {
                    rows.iter()
                        .filter_map(|&r| table.rows[r].cells.get(c))
                        .map(|s| cell_w(s))
                        .chain(std::iter::once(cell_w(&table.header[c])))
                        .max()
                        .unwrap_or(1)
                        .min(48)
                })
                .collect();
            let fixed: usize = widths[..n - 1].iter().map(|w| w + 2).sum();
            widths[n - 1] = width.saturating_sub(fixed).max(4);
            let line_of = |cells: &[String], style: Style| -> Line<'static> {
                let mut spans = Vec::new();
                for (c, w) in widths.iter().enumerate() {
                    let text = cells.get(c).map(String::as_str).unwrap_or("");
                    let text = view::sanitize(text);
                    let cut = head_chars(&text, *w);
                    let pad = w.saturating_sub(cell_w(&cut));
                    let cell = if table.right.get(c).copied().unwrap_or(false) {
                        format!("{}{cut}", " ".repeat(pad))
                    } else {
                        format!("{cut}{}", " ".repeat(pad))
                    };
                    spans.push(Span::styled(cell, style));
                    if c + 1 < n {
                        spans.push(Span::styled("  ", style));
                    }
                }
                Line::from(spans)
            };
            let mut head = table.header.clone();
            if let Some((c, reverse)) = v.sort {
                if let Some(h) = head.get_mut(c) {
                    h.push(if reverse { '^' } else { 'v' });
                }
            }
            out.push(line_of(
                &head,
                Style::default()
                    .fg(palette.accent())
                    .add_modifier(Modifier::BOLD),
            ));
            for (k, &r) in rows.iter().enumerate() {
                let row = &table.rows[r];
                let base = row
                    .level
                    .map_or(Style::default(), |l| palette.level_style(l));
                let style = if v.top + k == v.selected {
                    base.add_modifier(Modifier::REVERSED)
                } else {
                    base
                };
                out.push(line_of(&row.cells, style));
            }
            if rows.is_empty() && table.error.is_none() {
                out.push(Line::styled(tx(lang, "(nothing yet)"), dim));
            }
        }
        frame.render_widget(Paragraph::new(out), inner);
        if let Some(at) = cursor {
            frame.set_cursor_position(at);
        }
    }

    /// `=`: the two selected rows against each other; else the selection (or the cursor
    /// row) against the marked lines, or marked to be compared with the next one.
    fn compare_selection(&mut self) {
        use crate::tui::compare::{CompareView, Side};
        let tab = &self.tabs[self.active];
        let e = &tab.engine;
        let mut lines = e.selected_lines();
        if lines.is_empty() {
            lines.extend(tab.cursor_line());
        }
        lines.truncate(crate::compare::MAX_REGION_LINES);
        let Some(&first) = lines.first() else {
            return;
        };
        let text = |l: usize| {
            e.get_line(l)
                .map(|t| t.trim_end_matches(['\r', '\n']).to_string())
                .unwrap_or_default()
        };
        let name = stream_title(e);
        let side = |ls: &[usize]| Side {
            name: name.clone(),
            first: ls[0],
            lines: ls.iter().map(|&l| text(l)).collect(),
        };
        let pair = match self.compare_mark.take() {
            Some(mark) => Some((mark, side(&lines))),
            None if lines.len() == 2 => Some((side(&lines[..1]), side(&lines[1..]))),
            None => None,
        };
        match pair {
            Some((left, right)) => {
                self.compare = Some(CompareView::new(left, right, Default::default()));
            }
            None => {
                let n = lines.len();
                self.compare_mark = Some(side(&lines));
                self.message = Some(txf(
                    self.lang,
                    "Marked {0} lines from line {1}: select others and press = to compare",
                    &[&n, &(first + 1)],
                ));
            }
        }
    }

    fn on_compare_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        use crate::tui::compare::CompareKey;
        let Some(v) = self.compare.as_mut() else {
            return false;
        };
        match v.on_key(key) {
            CompareKey::Copy(text) => self.copy_text(&text),
            CompareKey::Close => self.compare = None,
            CompareKey::Moved => {}
            CompareKey::Other => return false,
        }
        true
    }

    /// The two sides side by side: a header with the toggles, then one row per aligned
    /// pair, the changed tokens marked, removed lines in the error colour and added ones
    /// in the info colour.
    fn draw_compare(&mut self, frame: &mut Frame, area: Rect) {
        let palette = self.palette;
        let dim = Style::default().fg(palette.dim());
        let removed = Style::default().fg(palette.level_color(LogLevel::Error));
        let added = Style::default().fg(palette.level_color(LogLevel::Info));
        let marked = Style::default()
            .fg(palette.level_color(LogLevel::Warn))
            .add_modifier(Modifier::REVERSED);
        let inner = self.dialog(
            frame,
            area,
            (area.width, area.height),
            tx(self.lang, "Compare"),
            false,
        );
        let lang = self.lang;
        let Some(v) = self.compare.as_mut() else {
            return;
        };
        let width = inner.width as usize;
        let half = width.saturating_sub(3) / 2;
        let mut out = Vec::new();
        let toggles: String = v
            .toggles()
            .iter()
            .map(|(k, name, on)| {
                format!(
                    "[{}] {} ({k})  ",
                    if *on { 'x' } else { ' ' },
                    tx(lang, name)
                )
            })
            .collect();
        out.push(Line::styled(head_chars(&toggles, width), dim));
        let state = if v.is_equal() {
            tx(lang, "Equal (with the options above)").to_string()
        } else {
            txf(
                lang,
                "{0} changes: ] / [ go to the next / previous, y copies the diff, Esc closes",
                &[&v.diff.changes.len()],
            )
        };
        out.push(Line::styled(head_chars(&state, width), dim));
        let head = |s: &crate::tui::compare::Side| format!("{}:{}", s.name, s.first + 1);
        out.push(Line::from(vec![
            Span::styled(
                pad_cell(&head(&v.left), half),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(" | ", dim),
            Span::styled(
                pad_cell(&head(&v.right), half),
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ]));
        let room = (inner.height as usize).saturating_sub(out.len());
        v.page = room.max(1);
        let cell =
            |text: &str, ranges: &[std::ops::Range<usize>], base: Style| -> Vec<Span<'static>> {
                let mut spans = Vec::new();
                let mut pos = 0;
                for r in ranges {
                    let (a, b) = (r.start.min(text.len()), r.end.min(text.len()));
                    if a < pos || !text.is_char_boundary(a) || !text.is_char_boundary(b) {
                        continue;
                    }
                    spans.push((text[pos..a].to_string(), base));
                    spans.push((text[a..b].to_string(), marked));
                    pos = b;
                }
                spans.push((text[pos..].to_string(), base));
                // Cut or pad to `half` cells.
                let mut used = 0;
                let mut out = Vec::new();
                for (s, style) in spans {
                    let s = view::sanitize(&s);
                    let mut piece = String::new();
                    for c in s.chars() {
                        let w = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
                        if used + w > half {
                            break;
                        }
                        piece.push(c);
                        used += w;
                    }
                    if !piece.is_empty() {
                        out.push(Span::styled(piece, style));
                    }
                }
                out.push(Span::raw(" ".repeat(half.saturating_sub(used))));
                out
            };
        for row in v.diff.rows.iter().skip(v.top).take(room) {
            use crate::compare::RowKind;
            let left = row
                .left
                .and_then(|i| v.shown.0.get(i))
                .map_or("", |s| s.as_str());
            let right = row
                .right
                .and_then(|i| v.shown.1.get(i))
                .map_or("", |s| s.as_str());
            let (lw, rw): (&[std::ops::Range<usize>], &[std::ops::Range<usize>]) = match &row.words
            {
                Some(w) => (&w.left, &w.right),
                None => (&[], &[]),
            };
            let (lb, rb) = match row.kind {
                RowKind::Equal => (Style::default(), Style::default()),
                RowKind::Changed => (Style::default(), Style::default()),
                RowKind::Removed => (removed, Style::default()),
                RowKind::Added => (Style::default(), added),
            };
            let mut spans = cell(left, lw, lb);
            spans.push(Span::styled(" | ", dim));
            spans.extend(cell(right, rw, rb));
            out.push(Line::from(spans));
        }
        frame.render_widget(Paragraph::new(out), inner);
    }

    /// The file the session's pad is kept in: beside the named session, else
    /// `scratchpad.txt` beside `fasttail.ini` (as the window keeps it).
    fn pad_file(&self) -> PathBuf {
        let settings = self.settings.as_ref();
        match settings.and_then(|s| s.config.current_session.as_deref()) {
            Some(session) => crate::scratchpad::sidecar_of(session),
            None => crate::scratchpad::default_pad_of(
                &settings.map_or_else(crate::config::FastTailConfig::config_path, |s| {
                    s.path.clone()
                }),
            ),
        }
    }

    /// The session's pad, loaded on first use.
    fn pad(&mut self) -> &mut crate::scratchpad::Scratchpad {
        if self.scratchpad.is_none() {
            self.scratchpad = Some(crate::scratchpad::Scratchpad::load(self.pad_file()));
        }
        self.scratchpad.as_mut().expect("loaded above")
    }

    fn open_pad(&mut self) {
        let text = self.pad().text.clone();
        self.pad_editor = Some(crate::tui::pad::PadEditor::new(&text));
    }

    /// The editor's text written back to the pad, saved a second later.
    fn pad_edited(&mut self) {
        let Some(text) = self.pad_editor.as_ref().map(|e| e.text()) else {
            return;
        };
        let pad = self.pad();
        pad.text = text;
        pad.touch();
    }

    /// Saves the pad now when it changed (on exit).
    pub fn save_scratchpad(&mut self) {
        if let Some(pad) = self.scratchpad.as_mut() {
            pad.save_on_exit();
        }
    }

    fn on_pad_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        use crate::tui::pad::PadKey;
        let Some(e) = self.pad_editor.as_mut() else {
            return false;
        };
        match e.on_key(key) {
            PadKey::Edited => self.pad_edited(),
            PadKey::Moved => {}
            PadKey::Close => {
                self.pad_editor = None;
                if let Some(pad) = self.scratchpad.as_mut() {
                    if pad.is_dirty() {
                        if let Err(err) = pad.save() {
                            self.message = Some(err.to_string());
                        }
                    }
                }
            }
            PadKey::Jump(name, line) => self.pad_jump(&name, line),
            PadKey::Other => return false,
        }
        true
    }

    /// A reference of the pad: its stream focused at that line (opened when needed).
    fn pad_jump(&mut self, name: &str, line: usize) {
        let open: Vec<(String, PathBuf)> = self
            .tabs
            .iter()
            .map(|t| (stream_title(&t.engine), t.engine.path.clone()))
            .collect();
        let Some(path) = self.pad().resolve(name, &open) else {
            self.message = Some(txf(self.lang, "{0}: not found", &[&name]));
            return;
        };
        self.open_file(&path);
        if let Some(i) = self
            .tabs
            .iter()
            .position(|t| crate::paths::paths_equal(&t.engine.path, &path))
        {
            self.pad_editor = None;
            self.focus_tab(i);
            self.tabs[i].engine.open_at_line(line + 1, self.lang);
        }
    }

    /// `Ctrl+N`: the selected rows (or the cursor row) at the end of the pad, under a
    /// reference line naming their stream and first line.
    fn send_to_pad(&mut self) {
        let tab = &mut self.tabs[self.active];
        let lines = tab.engine.selected_lines();
        let first = lines.first().copied().or_else(|| tab.cursor_line());
        let text = if tab.engine.has_selection() {
            tab.engine.copy_selection_text()
        } else {
            tab.cursor_line().and_then(|l| {
                tab.engine
                    .get_line(l)
                    .map(|t| t.trim_end_matches(['\r', '\n']).to_string())
            })
        };
        let (Some(first), Some(text)) = (first, text) else {
            self.message = Some(tx(self.lang, "Nothing to send").into());
            return;
        };
        let name = stream_title(&tab.engine);
        let path = tab.engine.path.clone();
        let n = text.lines().count();
        let lang = self.lang;
        let sent = self.pad().send(&name, &path, first, &text, true);
        self.message = Some(match sent {
            Ok(()) => txf(
                lang,
                "{0} lines sent to the scratchpad (Alt+N shows it)",
                &[&n],
            ),
            Err(crate::scratchpad::SendRefused::TooLarge) => {
                tx(lang, "The scratchpad is full (4 MB)").into()
            }
            Err(crate::scratchpad::SendRefused::Empty) => tx(lang, "Nothing to send").into(),
        });
        if let Some(e) = self.pad_editor.as_mut() {
            let text = self
                .scratchpad
                .as_ref()
                .map(|p| p.text.clone())
                .unwrap_or_default();
            *e = crate::tui::pad::PadEditor::new(&text);
        }
    }

    /// The scratchpad's editor over the windows: its lines, the cursor, the reference
    /// lines in the accent colour.
    fn draw_pad(&mut self, frame: &mut Frame, area: Rect) {
        let palette = self.palette;
        let dim = Style::default().fg(palette.dim());
        let accent = Style::default().fg(palette.accent());
        let width = area.width.saturating_sub(4).max(20);
        let height = area.height.saturating_sub(2).max(6);
        let inner = self.dialog(
            frame,
            area,
            (width, height),
            tx(self.lang, "Scratchpad"),
            false,
        );
        let hint = tx(
            self.lang,
            "Type to write, Ctrl+G goes to the reference line above, Ctrl+Z undoes, Esc closes",
        );
        let notice = self
            .scratchpad
            .as_ref()
            .and_then(|p| p.notice.as_ref().map(|(t, _)| t.clone()));
        let Some(e) = self.pad_editor.as_mut() else {
            return;
        };
        let rows = inner.height.saturating_sub(1) as usize;
        e.scroll(rows);
        let cells = inner.width as usize;
        let mut out = vec![Line::styled(
            head_chars(notice.as_deref().unwrap_or(hint), cells),
            dim,
        )];
        let mut cursor = None;
        for (i, line) in e.lines.iter().enumerate().skip(e.top).take(rows) {
            let shown = view::sanitize(line);
            let style = if crate::scratchpad::parse_reference(line).is_some() {
                accent
            } else {
                Style::default()
            };
            // The line scrolls sideways so the cursor stays inside.
            let (text, x) = if i == e.row {
                let field = TextField::new(line);
                let mut field = field;
                for _ in 0..line.chars().count().saturating_sub(e.col) {
                    field.on_key(crossterm::event::KeyEvent::new(
                        crossterm::event::KeyCode::Left,
                        crossterm::event::KeyModifiers::NONE,
                    ));
                }
                let (t, x) = field.view(cells);
                (view::sanitize(&t), Some(x))
            } else {
                (head_chars(&shown, cells), None)
            };
            if let Some(x) = x {
                cursor = Some((inner.x + x as u16, inner.y + 1 + (i - e.top) as u16));
            }
            out.push(Line::styled(text, style));
        }
        frame.render_widget(Paragraph::new(out), inner);
        if let Some(at) = cursor {
            frame.set_cursor_position(at);
        }
    }

    /// Adds the rules of a rule set file to the rules (skipping those already there), or
    /// with a trailing `!` replaces every rule with them.
    fn import_rules(&mut self, text: &str) {
        let (text, replace) = match text.strip_suffix('!') {
            Some(rest) => (rest.trim(), true),
            None => (text, false),
        };
        if text.is_empty() {
            return;
        }
        let path = PathBuf::from(text);
        let incoming = match crate::config::read_rule_set(&path) {
            Ok(rules) => rules,
            Err(e) => {
                use crate::config::RuleSetError;
                let why = match e {
                    RuleSetError::NotARuleSet => tx(self.lang, "not a rule set file").to_string(),
                    RuleSetError::NewerVersion(v) => txf(
                        self.lang,
                        "written by a newer FastTail (version {0})",
                        &[&v],
                    ),
                    RuleSetError::Io(e) => e,
                };
                self.message = Some(txf(self.lang, "Cannot import {0}: {1}", &[&text, &why]));
                return;
            }
        };
        let config = &mut self.settings_mut().config;
        let mut rules = if replace {
            Vec::new()
        } else {
            config.highlight_rules.clone()
        };
        let n = incoming.len();
        let added = crate::config::append_rules(&mut rules, incoming);
        let tools = config.external_tools.clone();
        let labels = self.quick_labels.clone();
        let mut d = RulesDialog::new(&rules, &tools);
        d.labels = labels;
        if let Some(old) = self.rules.as_ref() {
            d.list.selected = old.list.selected.min(rules.len().saturating_sub(1));
        }
        self.rules = Some(d);
        self.apply_rules();
        self.message = Some(if replace {
            txf(self.lang, "{0} rules imported, replacing the rules", &[&n])
        } else {
            txf(
                self.lang,
                "{0} of {1} rules added (the others were there already)",
                &[&added, &n],
            )
        });
    }

    /// Writes every rule to a rule set file (a trailing `!` replaces an existing one).
    fn export_rules(&mut self, text: &str) {
        let (text, replace) = match text.strip_suffix('!') {
            Some(rest) => (rest.trim(), true),
            None => (text, false),
        };
        if text.is_empty() {
            return;
        }
        let path = PathBuf::from(text);
        if path.exists() && !replace {
            self.message = Some(txf(
                self.lang,
                "{0} exists: add ! to replace it",
                &[&path.display()],
            ));
            return;
        }
        let rules = self.settings_mut().config.highlight_rules.clone();
        self.message = Some(match crate::config::write_rule_set(&path, &rules) {
            Ok(()) => txf(
                self.lang,
                "{0} rules exported to {1}",
                &[&rules.len(), &path.display()],
            ),
            Err(e) => txf(self.lang, "Export failed: {0}", &[&e]),
        });
    }

    /// Walks the rule seeks of the streams on screen for a few milliseconds; a line found
    /// is reached through the engine's jump (see `tick`), a walk that found nothing says
    /// so.
    fn step_rule_seeks(&mut self) {
        let lang = self.lang;
        for i in self.visible_tabs() {
            let tab = &mut self.tabs[i];
            let sought = tab.engine.rule_seek_pattern().map(str::to_owned);
            match tab
                .engine
                .step_rule_seek(crate::tail_engine::RULE_SEEK_BUDGET, lang)
            {
                Some(crate::tail_engine::RuleSeekStep::Found { line, wrapped }) => {
                    tab.engine.pending_jump = None;
                    tab.engine.reveal_line(line);
                    if let Some(row) = tab.engine.get_visible_row_of_line(line) {
                        tab.set_cursor(row);
                    }
                    tab.engine.follow_tail = false;
                    if wrapped {
                        self.message = Some(tx(lang, "Search wrapped around").into());
                    }
                }
                Some(crate::tail_engine::RuleSeekStep::NotFound) => {
                    self.message = Some(txf(
                        lang,
                        "No line matches the rule {0}",
                        &[&sought.unwrap_or_default()],
                    ));
                }
                Some(crate::tail_engine::RuleSeekStep::Pending) | None => {}
            }
        }
    }

    /// The Log formats dialog, previewing over the focused stream's first lines.
    fn open_formats(&mut self) {
        let lines = self
            .tabs
            .get(self.active)
            .map(|t| {
                let e = &t.engine;
                (0..e.total_lines().min(crate::log_format_edit::PREVIEW_LINES))
                    .filter_map(|i| e.get_line(i).map(|l| l.into_owned()))
                    .collect()
            })
            .unwrap_or_default();
        self.formats = Some(FormatsView::new(crate::log_format::default_dir(), lines));
    }

    fn on_formats_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        let lib = crate::log_format::library();
        let lang = self.lang;
        let Some(v) = self.formats.as_mut() else {
            return false;
        };
        match v.on_key(key, &lib, lang) {
            FormatsKey::Changed => self.reload_formats(),
            FormatsKey::Import => self.open_prompt(PromptKind::FormatImport),
            FormatsKey::Export => self.open_prompt(PromptKind::FormatExport),
            FormatsKey::Close => self.formats = None,
            FormatsKey::Moved => {}
            FormatsKey::Other => return false,
        }
        true
    }

    /// Loads the formats folder again and gives every stream the new formats.
    fn reload_formats(&mut self) {
        let dir = self
            .formats
            .as_ref()
            .map_or_else(crate::log_format::default_dir, |f| f.state.dir.clone());
        crate::log_format::set_library(crate::log_format::Library::load(&dir));
        for tab in &mut self.tabs {
            tab.engine.reload_field_parser();
        }
    }

    fn import_format(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let lib = crate::log_format::library();
        let lang = self.lang;
        let Some(v) = self.formats.as_mut() else {
            return;
        };
        match v.state.import(&lib, Path::new(text)) {
            Ok(path) => {
                v.notice = Some((txf(lang, "Imported {0}", &[&path.display()]), false));
                self.reload_formats();
            }
            Err(e) => {
                let name = Path::new(text).display().to_string();
                v.notice = Some((crate::tui::formats::save_error(lang, &e, &name), true));
            }
        }
    }

    fn export_format(&mut self, text: &str) {
        let (text, replace) = match text.strip_suffix('!') {
            Some(rest) => (rest.trim(), true),
            None => (text, false),
        };
        if text.is_empty() {
            return;
        }
        let lang = self.lang;
        let Some(v) = self.formats.as_mut() else {
            return;
        };
        let path = PathBuf::from(text);
        v.notice = Some(if path.exists() && !replace {
            (
                txf(lang, "{0} exists: add ! to replace it", &[&path.display()]),
                true,
            )
        } else {
            match v.state.export(&path) {
                Ok(()) => (txf(lang, "Exported to {0}", &[&path.display()]), false),
                Err(e) => (txf(lang, "Export failed: {0}", &[&e]), true),
            }
        });
    }

    /// The cursor row's source location opened in FastTail at its line (the next one
    /// when pressed again on the same row, as `Ctrl+O`).
    fn open_source_here(&mut self) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let Some(line) = tab.cursor_line() else {
            return;
        };
        let text = tab.engine.get_line(line).unwrap_or_default();
        let locations = crate::source_links::find(&text);
        if locations.is_empty() {
            self.message = Some(tx(self.lang, "No source location on this row").into());
            return;
        }
        let next = match self.source_cycle {
            Some((l, i)) if l == line => (i + 1) % locations.len(),
            _ => 0,
        };
        self.source_cycle = Some((line, next));
        let loc = locations[next].clone();
        let roots = self
            .settings
            .as_ref()
            .map(|s| s.config.source_roots.clone())
            .unwrap_or_default();
        let Some(file) = crate::source_links::resolve(&loc, &roots) else {
            self.message = Some(txf(
                self.lang,
                "{0} not found under the source roots",
                &[&loc.path],
            ));
            return;
        };
        let before = self.tabs.len();
        self.open_file(&file);
        let opened = self
            .tabs
            .iter()
            .position(|t| crate::paths::paths_equal(&t.engine.path, &file));
        if let Some(i) = opened {
            self.focus_tab(i);
            self.tabs[i]
                .engine
                .open_at_line(loc.line as usize, self.lang);
            if self.tabs.len() > before || self.message.is_none() {
                self.message = Some(txf(
                    self.lang,
                    "Opened {0}:{1} ({2} of {3})",
                    &[&loc.path, &loc.line, &(next + 1), &locations.len()],
                ));
            }
        }
    }

    /// The Log formats dialog: the list of formats, or the form of the one being edited
    /// with what it reads.
    fn draw_formats(&mut self, frame: &mut Frame, area: Rect) {
        let palette = self.palette;
        let dim = Style::default().fg(palette.dim());
        let error = Style::default().fg(palette.level_color(LogLevel::Error));
        let lib = crate::log_format::library();
        let Some(v) = self.formats.as_ref() else {
            return;
        };
        let rows = if v.editing {
            v.fields.len() + 10
        } else {
            lib.formats.len() + lib.errors.len() + 4
        };
        let height = (rows as u16 + 4).min(area.height);
        let width = area.width.saturating_sub(4).clamp(40, 100);
        let inner = self.dialog(
            frame,
            area,
            (width, height),
            tx(self.lang, "Log formats"),
            false,
        );
        let Some(v) = self.formats.as_ref() else {
            return;
        };
        let cells = inner.width as usize;
        let mut out = Vec::new();
        let mut cursor = None;
        if v.confirm_delete {
            out.push(Line::styled(
                txf(
                    self.lang,
                    "Delete the format {0}? y deletes, any other key keeps it",
                    &[&v.state.draft.name],
                ),
                error,
            ));
        } else if v.editing {
            let hint = if v.read_only() {
                tx(self.lang, "Built-in: c makes a copy to change. Esc back")
            } else {
                tx(
                    self.lang,
                    "Tab/Up/Down move, type to edit, Enter saves, Ctrl+D deletes, Esc back",
                )
            };
            out.push(Line::styled(hint, dim));
        } else {
            out.push(Line::styled(
                tx(
                    self.lang,
                    "Enter edit  n new  c copy  d delete  i import  x export  Esc close",
                ),
                dim,
            ));
        }
        if let Some((text, bad)) = &v.notice {
            out.push(Line::styled(
                head_chars(text, cells),
                if *bad { error } else { dim },
            ));
        }
        if v.editing {
            let label_w = 16usize;
            let field_w = cells.saturating_sub(label_w + 1).max(10);
            for (i, (part, field)) in v.fields.iter().enumerate() {
                let label = match part {
                    crate::tui::formats::Part::Pattern(n) => {
                        format!("{} {}", tx(self.lang, part.label()), n + 1)
                    }
                    _ => tx(self.lang, part.label()).to_string(),
                };
                let (shown, x) = field.view(field_w);
                let style = if i == v.focus {
                    let y = inner.y + out.len() as u16;
                    if y < inner.bottom() {
                        cursor = Some((inner.x + label_w as u16 + 1 + x as u16, y));
                    }
                    Style::default().add_modifier(Modifier::REVERSED)
                } else {
                    Style::default()
                };
                out.push(Line::from(vec![
                    Span::styled(format!("{:<label_w$} ", head_chars(&label, label_w)), dim),
                    Span::styled(format!("{:<field_w$}", view::sanitize(&shown)), style),
                ]));
            }
            let (summary, bad) = v.summary(self.lang);
            out.push(Line::raw(""));
            out.push(Line::styled(
                head_chars(&summary, cells),
                if bad { error } else { Style::default() },
            ));
            let room = (inner.height as usize).saturating_sub(out.len());
            for row in v.state.preview.rows.iter().take(room) {
                let style = if !row.starts_entry {
                    dim
                } else if row.matched {
                    palette.level_style(row.level)
                } else {
                    error
                };
                let mark = if !row.starts_entry {
                    ' '
                } else if row.matched {
                    '+'
                } else {
                    '-'
                };
                let text = format!("{mark} {}", view::sanitize(&row.text));
                out.push(Line::styled(head_chars(&text, cells), style));
            }
        } else {
            let start = out.len() as u16;
            for (i, f) in lib.formats.iter().enumerate() {
                let y = inner.y + start + i as u16;
                self.hits
                    .list_items
                    .push((Rect::new(inner.x, y, inner.width, 1), i));
                let kind = if f.builtin {
                    tx(self.lang, "built-in").to_string()
                } else if let Some(p) = &f.plugin {
                    p.clone()
                } else {
                    f.file
                        .as_ref()
                        .and_then(|p| p.file_name())
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default()
                };
                let style = if i == v.selected {
                    Style::default().add_modifier(Modifier::REVERSED)
                } else {
                    Style::default()
                };
                let text = format!(" {:<28} {kind}", head_chars(f.name(), 28));
                out.push(Line::styled(head_chars(&text, cells), style));
            }
            for (j, (path, why)) in lib.errors.iter().enumerate() {
                let i = lib.formats.len() + j;
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                let style = if i == v.selected {
                    error.add_modifier(Modifier::REVERSED)
                } else {
                    error
                };
                let text = format!(" {name}: {why}");
                out.push(Line::styled(head_chars(&text, cells), style));
            }
        }
        frame.render_widget(Paragraph::new(out), inner);
        if let Some(at) = cursor {
            frame.set_cursor_position(at);
        }
    }

    /// The editor's rules reach every stream and `fasttail.ini` at once, as in the GUI;
    /// the tools bound to a renamed or deleted rule follow it.
    fn apply_rules(&mut self) {
        let Some(d) = &self.rules else {
            return;
        };
        let rules = d.rules();
        let settings = self.settings.get_or_insert_with(Default::default);
        let before = std::mem::replace(&mut settings.config.highlight_rules, rules.clone());
        d.bind_tools(&before, &mut settings.config.external_tools);
        for tab in &mut self.tabs {
            tab.engine.set_highlight_rules(rules.clone());
        }
        self.save_config();
    }

    fn on_settings_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        let Some(form) = self.settings_form.as_mut() else {
            return false;
        };
        // `Enter` on the External tools row opens their editor over Settings.
        let on_tools = form
            .fields
            .get(form.focus)
            .is_some_and(|f| f.key == crate::tui::settings::Key::Tools);
        if on_tools
            && key.code == crossterm::event::KeyCode::Enter
            && key.kind != crossterm::event::KeyEventKind::Release
        {
            self.open_tools_editor();
            return true;
        }
        let on_plugins = form
            .fields
            .get(form.focus)
            .is_some_and(|f| f.key == crate::tui::settings::Key::Plugins);
        if on_plugins
            && key.code == crossterm::event::KeyCode::Enter
            && key.kind != crossterm::event::KeyEventKind::Release
        {
            self.open_plugins();
            return true;
        }
        match form.on_key(key) {
            FieldKey::Submit => self.submit_settings(),
            FieldKey::Cancel => self.cancel_settings(),
            FieldKey::Edited => {}
            FieldKey::Other => return false,
        }
        true
    }

    /// `[ OK ]` of Settings: every field valid, the values go into the configuration,
    /// reach the running interface (theme, level colours, polling, each stream's
    /// settings) and are saved. A field out of its range keeps the dialog open.
    fn submit_settings(&mut self) {
        let Some(form) = self.settings_form.as_mut() else {
            return;
        };
        let settings = self.settings.get_or_insert_with(Default::default);
        let interface_before = settings.config.interface;
        if let Err(problems) = form.apply(&mut settings.config) {
            form.rejected = true;
            let names: Vec<String> = problems
                .iter()
                .map(|(i, p)| format!("{}: {p}", form.fields[*i].label))
                .collect();
            self.message = Some(names.join("  |  "));
            return;
        }
        self.settings_form = None;
        self.settings_before = None;
        let config = &settings.config;
        if config.interface == crate::config::Interface::Gui && interface_before != config.interface
        {
            self.interface_switch = Some(InterfaceSwitch::Offer);
        }
        self.apply_running_config();
        self.message = Some(tx(self.lang, "Settings saved").into());
        self.save_config();
    }

    /// `Shift+T`: the next theme, in the order of the GUI's list, saved as the ini's
    /// `theme`.
    fn cycle_theme(&mut self) {
        use crate::theme::CyberTheme;
        let next = match self.palette.theme {
            CyberTheme::Tron => CyberTheme::Matrix,
            CyberTheme::Matrix => CyberTheme::Blade,
            CyberTheme::Blade => CyberTheme::Light,
            CyberTheme::Light => CyberTheme::Commander,
            CyberTheme::Commander => CyberTheme::Tron,
        };
        self.palette.theme = next;
        if let Some(s) = self.settings.as_mut() {
            s.config.theme = next;
        }
        self.message = Some(txf(self.lang, "Theme: {0}", &[&next.name()]));
    }

    /// The GUI's "Flash on background alert" in a terminal: while a stream that is not on
    /// screen has an alert waiting, the bell rings once (`flash_on_alert`); it rings again
    /// for alerts that come after the waiting ones were seen.
    fn check_alerts(&mut self, shown: &[usize]) {
        let waiting = self
            .tabs
            .iter()
            .enumerate()
            .any(|(i, t)| !shown.contains(&i) && t.engine.unseen_severity >= 2);
        if !waiting {
            self.bell_rung = false;
            return;
        }
        let on = self
            .settings
            .as_ref()
            .is_some_and(|s| s.config.flash_on_alert);
        if on && !self.bell_rung {
            self.bell = true;
            self.bell_rung = true;
        }
    }

    /// Whether the bell should ring now (once per call that returns true).
    pub fn take_bell(&mut self) -> bool {
        std::mem::take(&mut self.bell)
    }

    /// Play / Pause of the top bar: every stream is watched and follows (a compressed
    /// one is only watched), or none is, as the GUI's toolbar does.
    fn play_all(&mut self, on: bool) {
        for tab in &mut self.tabs {
            tab.engine.is_watching = on;
            if !on || tab.engine.compressed.is_none() {
                tab.engine.follow_tail = on;
            }
            if tab.engine.follow_tail {
                tab.pin_cursor();
            }
        }
        self.message = Some(
            if on {
                tx(self.lang, "Following every stream")
            } else {
                tx(self.lang, "Every stream paused")
            }
            .into(),
        );
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
                    self.message = Some(tx(self.lang, "No recent session: type a path").into());
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
        self.sync_labels();
        self.active = 0;
        self.restore_views(&plan.views);
        self.restore_dock(plan.dock_layout.as_deref());
        // The session's layout becomes the configuration's, as in the GUI.
        self.dock_dirty = true;
        let mut notes: Vec<String> = errors;
        notes.extend(crate::tui::workspace::missing_notice(
            &plan.missing,
            self.lang,
        ));
        self.message = Some(if notes.is_empty() {
            txf(
                self.lang,
                "Session {0} loaded: {1} streams",
                &[&crate::session::Session::name_of(file), &plan.paths.len()],
            )
        } else {
            notes.join("  |  ")
        });
        if self.tabs.is_empty() {
            self.message = Some(txf(
                self.lang,
                "Session {0} opened nothing",
                &[&crate::session::Session::name_of(file)],
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
            self.message = Some(txf(
                self.lang,
                "{0} is the configuration file: it cannot be a session",
                &[&file.display()],
            ));
            return;
        }
        if file.exists() {
            self.confirm_overwrite = Some(file);
            return;
        }
        self.save_session(&file);
    }

    /// The switch dialog: `Enter` switches now (or closes the failure note), `Esc`
    /// leaves the choice for the next start.
    fn on_interface_switch_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        use crossterm::event::{KeyCode, KeyEventKind};
        if key.kind == KeyEventKind::Release {
            return false;
        }
        match key.code {
            KeyCode::Enter => self.confirm_interface_switch(),
            KeyCode::Esc => self.interface_switch = None,
            _ => return false,
        }
        true
    }

    /// `[ OK ]` of the switch dialog.
    fn confirm_interface_switch(&mut self) {
        match self.interface_switch.take() {
            Some(InterfaceSwitch::Offer) => self.switch_to_gui(),
            Some(InterfaceSwitch::Failed(_)) | None => {}
        }
    }

    /// Settings "Switch now": the workspace is saved, then the window starts
    /// (`handoff::start_gui`) and the terminal interface quits, restoring the terminal.
    /// When the window cannot start, a dialog says why and this interface keeps running.
    fn switch_to_gui(&mut self) {
        self.save_config();
        match (self.start_gui)() {
            Ok(()) => self.quit = true,
            Err(e) => self.interface_switch = Some(InterfaceSwitch::Failed(e.text(self.lang))),
        }
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

    /// Writes the open streams, their state and the dock layout to `file`, makes it the
    /// current session and puts it first in the recent sessions.
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
            dock_layout: Some(self.dock_ron()),
            // The terminal opens no folder groups; the window keeps its own.
            folders: Vec::new(),
        };
        if let Err(e) = session.save_to(file) {
            self.message = Some(txf(
                self.lang,
                "Cannot save {0}: {1}",
                &[&file.display(), &e],
            ));
            return;
        }
        let config = &mut self.settings_mut().config;
        config.current_session = Some(file.to_path_buf());
        config.add_recent_session(file);
        let stdin = if self.tabs.iter().any(|t| t.engine.is_stdin()) {
            tx(self.lang, " (standard input is not saved)")
        } else {
            ""
        };
        self.message = Some(txf(
            self.lang,
            "Session saved to {0}: {1} streams{2}",
            &[&file.display(), &count, &stdin],
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

    /// `o`: the Open dialog on the folder of the focused stream (the current folder
    /// for standard input or an archive entry).
    fn open_browser(&mut self) {
        let start = self
            .tabs
            .get(self.active)
            .and_then(|t| t.engine.path.parent().map(Path::to_path_buf))
            .filter(|d| d.is_dir())
            .or_else(|| std::env::current_dir().ok());
        self.browser = Some(FileBrowser::at(start, self.lang));
    }

    fn on_browser_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        use crossterm::event::{KeyCode, KeyEventKind};
        let Some(b) = self.browser.as_mut() else {
            return false;
        };
        if key.kind == KeyEventKind::Release {
            return false;
        }
        let page = 10;
        match key.code {
            KeyCode::Up => b.move_by(-1),
            KeyCode::Down => b.move_by(1),
            KeyCode::PageUp => b.move_by(-page),
            KeyCode::PageDown => b.move_by(page),
            // Backspace on an empty name goes up, as in a file dialog.
            KeyCode::Backspace if b.field.text().is_empty() => b.up(),
            _ => match b.field.on_key(key) {
                FieldKey::Submit => self.submit_browser(),
                FieldKey::Cancel => self.browser = None,
                FieldKey::Edited => b.filter_changed(),
                FieldKey::Other => return false,
            },
        }
        true
    }

    /// `Enter` / `[ OK ]` in the Open dialog: a folder is listed, a file opens.
    fn submit_browser(&mut self) {
        let Some(b) = self.browser.as_mut() else {
            return;
        };
        if let Some(path) = b.activate() {
            self.browser = None;
            self.open_file(&path);
        }
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
            self.message = Some(tx(self.lang, "No entry matches").into());
            return;
        };
        if let Some(r) = &entry.refusal {
            self.message = Some(format!("{}: {}", entry.name, refusal_text(r, self.lang)));
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
            OpenOutcome::Opened(engine) => self.add_engine_tab(*engine),
            OpenOutcome::OpenEntry(entry) => self.open_file(&entry),
            OpenOutcome::Missing => {
                self.message = Some(txf(self.lang, "{0}: not found", &[&shown]))
            }
            OpenOutcome::Failed { error, .. } => {
                let reason = error
                    .system_text(self.lang)
                    .unwrap_or_else(|| format!("{error:?}"));
                self.message = Some(format!("{shown}: {reason}"))
            }
            OpenOutcome::EmptyArchive(_) => {
                self.message = Some(txf(self.lang, "{0}: the archive holds no file", &[&shown]))
            }
            OpenOutcome::ChooseEntries {
                archive,
                entries,
                partial,
            } => self.picker = Some(EntryPicker::new(archive, entries, partial, self.lang)),
            OpenOutcome::ScanTar { archive, codec } => {
                let scan = crate::compressed::TarScan::start(
                    &archive,
                    codec,
                    crate::compressed::ScanLimits::default(),
                );
                self.picker = Some(EntryPicker::scanning(archive, scan, self.lang));
            }
            OpenOutcome::ListFailed { error, .. } => {
                self.message = Some(format!("{shown}: {error}"))
            }
        }
    }

    /// Adds `engine` as a tab of the focused window and focuses it, with the settings'
    /// stream defaults (recorded in the recent files unless it is ephemeral).
    fn add_engine_tab(&mut self, mut engine: TailEngine) {
        if let Some(s) = self.settings.as_mut() {
            let spool_view = engine.merged.is_some() || engine.derived.is_some();
            if !spool_view {
                s.prepare(&mut engine);
            }
            if !engine.is_stdin() && !engine.is_clipboard() && !spool_view {
                s.config.add_recent_file(&engine.path);
            }
            // A plain file with rotations next to it: the stream bar offers them.
            if !spool_view
                && s.config.rotations == crate::rotation::Policy::Ask
                && engine.rotation.is_none()
                && engine.compressed.is_none()
                && engine.piped().is_none()
                && engine.path.is_file()
            {
                engine.rotation_offer = crate::rotation::offer(&engine.path);
            }
        }
        let place = self.focused_place();
        let path = engine.path.clone();
        engine.set_quick_labels(&self.quick_labels);
        self.tabs.push(Tab::new(engine));
        // The new stream is a tab of the focused window, as in the GUI.
        match place {
            Place::Float(f) => {
                if let Pane::Leaf { tabs, active } = &mut self.floats[f].pane {
                    tabs.push(DockTab::LogStream(path));
                    *active = tabs.len() - 1;
                }
                self.dock_dirty = true;
            }
            Place::Dock(leaf) => {
                let mut dock = self.dock.clone();
                dock.add_tab(&leaf, DockTab::LogStream(path));
                self.set_dock(dock);
            }
        }
        self.focus_tab(self.tabs.len() - 1);
    }

    /// Appends the clipboard text to the focused clipboard stream, on a line of its own.
    fn paste_append(&mut self) {
        let unreadable = "The clipboard holds no text (or cannot be read here)";
        if !self
            .tabs
            .get(self.active)
            .is_some_and(|t| t.engine.is_clipboard())
        {
            self.message = Some(tx(self.lang, "Not a clipboard stream").into());
            return;
        }
        let Some(text) = self.clipboard.paste() else {
            self.message = Some(tx(self.lang, unreadable).into());
            return;
        };
        let settings = match &self.settings {
            Some(s) => s.config.stdin_settings(),
            None => crate::config::FastTailConfig::default().stdin_settings(),
        };
        let lang = self.lang;
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        if let Err(e) = tab.engine.append_clipboard(&text, &settings, None) {
            self.message = Some(match e {
                crate::clipboard_source::PasteError::TooLarge(cap) => txf(
                    lang,
                    "The clipboard text is larger than {0}",
                    &[&crate::tui::picker::human_size(cap)],
                ),
                crate::clipboard_source::PasteError::Empty => tx(lang, unreadable).into(),
                crate::clipboard_source::PasteError::Busy => {
                    tx(lang, "The previous paste is still being read").into()
                }
                other => other.to_string(),
            });
        }
    }

    /// Opens the clipboard text as a new stream (`clipboard 14:02`).
    fn paste_stream(&mut self) {
        let unreadable = "The clipboard holds no text (or cannot be read here)";
        let Some(text) = self.clipboard.paste() else {
            self.message = Some(tx(self.lang, unreadable).into());
            return;
        };
        let settings = match &self.settings {
            Some(s) => s.config.stdin_settings(),
            None => crate::config::FastTailConfig::default().stdin_settings(),
        };
        let clock = crate::clipboard_source::clock_now();
        match crate::clipboard_source::open_engine(&text, &clock, &settings, None) {
            Ok(engine) => self.add_engine_tab(engine),
            Err(e) => {
                self.message = Some(match e {
                    crate::clipboard_source::PasteError::TooLarge(cap) => txf(
                        self.lang,
                        "The clipboard text is larger than {0}",
                        &[&crate::tui::picker::human_size(cap)],
                    ),
                    crate::clipboard_source::PasteError::Empty => tx(self.lang, unreadable).into(),
                    other => other.to_string(),
                })
            }
        }
    }

    fn open_time_range(&mut self) {
        let e = &self.tabs[self.active].engine;
        let reference = e.time_reference();
        let mut d = TimeRangeDialog {
            from: TextField::new(&e.time_from_text),
            to: TextField::new(&e.time_to_text),
            on_to: false,
            zone: RangeZone::Field,
            calendar: Calendar::new(time_range_text::day_of(reference)),
            on_minutes: false,
            invalid: false,
            reference,
        };
        d.sync_calendar();
        self.time_range = Some(d);
    }

    fn on_time_range_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        use crossterm::event::{KeyCode, KeyEventKind};
        let Some(d) = self.time_range.as_mut() else {
            return false;
        };
        if key.kind == KeyEventKind::Release {
            return false;
        }
        match key.code {
            // The previous range stays.
            KeyCode::Esc => {
                self.time_range = None;
                return true;
            }
            KeyCode::Tab => {
                d.next_stop(false);
                return true;
            }
            KeyCode::BackTab => {
                d.next_stop(true);
                return true;
            }
            _ => {}
        }
        match d.zone {
            RangeZone::Field => match d.focused().on_key(key) {
                FieldKey::Submit => self.submit_time_range(),
                FieldKey::Cancel => self.time_range = None,
                FieldKey::Edited => d.invalid = false,
                FieldKey::Other => match key.code {
                    KeyCode::Up | KeyCode::Down => {
                        d.on_to = !d.on_to;
                        d.sync_calendar();
                    }
                    _ => return false,
                },
            },
            RangeZone::Calendar => match key.code {
                KeyCode::Left => d.calendar.move_days(-1),
                KeyCode::Right => d.calendar.move_days(1),
                KeyCode::Up => d.calendar.move_days(-7),
                KeyCode::Down => d.calendar.move_days(7),
                KeyCode::PageUp => d.calendar.move_months(-1),
                KeyCode::PageDown => d.calendar.move_months(1),
                KeyCode::Char(' ') => d.pick_day(),
                // Enter picks the day and applies the range.
                KeyCode::Enter => {
                    d.pick_day();
                    self.submit_time_range();
                }
                _ => return false,
            },
            RangeZone::Time => match key.code {
                KeyCode::Left | KeyCode::Right => d.on_minutes = !d.on_minutes,
                KeyCode::Up if d.on_minutes => d.step_time(0, 1),
                KeyCode::Down if d.on_minutes => d.step_time(0, -1),
                KeyCode::PageUp if d.on_minutes => d.step_time(0, 10),
                KeyCode::PageDown if d.on_minutes => d.step_time(0, -10),
                KeyCode::Up => d.step_time(1, 0),
                KeyCode::Down => d.step_time(-1, 0),
                KeyCode::PageUp => d.step_time(6, 0),
                KeyCode::PageDown => d.step_time(-6, 0),
                KeyCode::Enter => self.submit_time_range(),
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
        if matches!(
            prompt.kind,
            PromptKind::System
                | PromptKind::Remote
                | PromptKind::Listen
                | PromptKind::DebugOutput
                | PromptKind::EventLog
        ) {
            let text = prompt.field.text().trim().to_string();
            self.open_file(Path::new(&text));
            return;
        }
        let text = prompt.field.text().trim().to_string();
        match prompt.kind {
            PromptKind::RulesImport => return self.import_rules(&text),
            PromptKind::RulesExport => return self.export_rules(&text),
            PromptKind::FormatImport => return self.import_format(&text),
            PromptKind::FormatExport => return self.export_format(&text),
            PromptKind::Report(true) => return self.write_report(true, &text),
            PromptKind::OpenPlugin => return self.open_plugin(&text),
            _ => {}
        }
        let tab = &mut self.tabs[self.active];
        match prompt.kind {
            PromptKind::Search => {
                // `re:` makes the query a regex (`re:(?-i)` for case), as the window's
                // `.*` toggle does.
                use crate::tail_engine::SearchMode;
                let (mode, query) = if let Some(rest) = text.strip_prefix("mre:") {
                    (SearchMode::Multiline, rest.to_string())
                } else if let Some(rest) = text.strip_prefix("re:") {
                    (SearchMode::Regex, rest.to_string())
                } else {
                    (SearchMode::Text, text.clone())
                };
                tab.engine.search_query = query.clone();
                let case = tab.engine.search_case_sensitive;
                if mode != tab.engine.search_mode {
                    tab.engine.set_search_mode(mode, case);
                } else {
                    tab.engine.update_search(&query);
                }
                if let Some(e) = &tab.engine.search_error {
                    self.message = Some(txf(self.lang, "Invalid regex: {0}", &[e]));
                }
                tab.pending_first_hit = !query.is_empty();
            }
            PromptKind::Include if tab.engine.filter_expr_mode() => {
                tab.engine.set_filter_expr(&text);
                tab.top = 0;
                if let Some(e) = tab.engine.expr_error() {
                    self.message = Some(crate::filter_expr::error_text(self.lang, &text, e));
                }
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
            PromptKind::SaveSession => self.submit_save_session(&text),
            PromptKind::Split => self.submit_split(&text),
            PromptKind::SearchScope => self.submit_search_scope(&text),
            PromptKind::Export(scope, format) => self.start_export(scope, format, &text),
            PromptKind::Report(all) => self.write_report(all, &text),
            PromptKind::RulesImport
            | PromptKind::RulesExport
            | PromptKind::FormatImport
            | PromptKind::FormatExport => {}
            PromptKind::OpenPlugin => {}
            PromptKind::StatsField | PromptKind::OpsField => {
                let stats = prompt.kind == PromptKind::StatsField;
                match crate::tui::analysis::parse_source(&text) {
                    Some(source) => self.open_source_view(source, stats),
                    None => {
                        self.message = Some(txf(
                            self.lang,
                            "Cannot read \"{0}\" as a field or /regex/",
                            &[&text],
                        ))
                    }
                }
            }
            PromptKind::ExplainSpan => {
                let (from, to) = text.split_once("..").unwrap_or((text.as_str(), ""));
                if tab.engine.explain_texts(from.trim(), to.trim()) {
                    if let Some(span) = tab.engine.explain_request.take() {
                        tab.engine
                            .explain_span(span, crate::spike::Baseline::Rest, false);
                    }
                    let path = tab.engine.path.clone();
                    self.analysis = Some(crate::tui::analysis::AnalysisView::new(
                        crate::tui::analysis::Kind::Explain,
                        path,
                    ));
                } else {
                    self.message = Some(txf(self.lang, "Cannot read \"{0}\" as a range", &[&text]));
                }
            }
            PromptKind::System
            | PromptKind::Remote
            | PromptKind::Listen
            | PromptKind::DebugOutput
            | PromptKind::EventLog => {}
            PromptKind::LevelBytes if text.is_empty() => tab.engine.set_level_override(None),
            PromptKind::LevelBytes => {
                match text.parse().ok().and_then(crate::log_level::valid_window) {
                    Some(bytes) => tab.engine.set_level_override(Some(bytes)),
                    None => {
                        self.message =
                            Some(txf(self.lang, "\"{0}\": from 16 to 4096 bytes", &[&text]))
                    }
                }
            }
            PromptKind::Goto if tab.is_asm() => {
                if tab.engine.asm_go_to(&text) {
                    tab.engine.follow_tail = false;
                    tab.cursor = 0;
                } else {
                    self.message = Some(txf(self.lang, "Cannot go to \"{0}\"", &[&text]));
                }
            }
            PromptKind::Goto if tab.is_hex() => match hex::parse_offset(&text) {
                Some(offset) if tab.engine.file_size > 0 => {
                    let last = tab.engine.file_size as usize - 1;
                    tab.set_cursor(offset.min(last) / tab.hex_width.max(1));
                }
                _ => self.message = Some(txf(self.lang, "Cannot go to \"{0}\"", &[&text])),
            },
            PromptKind::Goto => {
                let current = tab.cursor_line().unwrap_or(0);
                let target = tab.engine.resolve_goto(&text, current);
                if target.is_some_and(|t| t.waiting) {
                    // The jump happens when the timing ends (see `tick`).
                    self.message =
                        Some(tx(self.lang, "Timing the file to find that time...").into());
                } else {
                    self.message = goto_target(tab, target, &text, self.lang);
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
        // The prompts that work with no stream open.
        if matches!(
            kind,
            PromptKind::System
                | PromptKind::Remote
                | PromptKind::Listen
                | PromptKind::DebugOutput
                | PromptKind::EventLog
        ) {
            let start = match kind {
                PromptKind::System => "docker://",
                PromptKind::Remote => "ssh://",
                PromptKind::DebugOutput => "dbwin://local",
                PromptKind::EventLog => "eventlog://Application",
                _ => "syslog://127.0.0.1:5514",
            };
            self.prompt = Some(Prompt {
                kind,
                field: TextField::new(start),
            });
            return;
        }
        if kind == PromptKind::OpenPlugin {
            let text = self
                .plugins
                .as_ref()
                .and_then(|d| d.catalog.plugins.get(d.selected))
                .map(plugin_uri_with_defaults)
                .unwrap_or_default();
            self.prompt = Some(Prompt {
                kind,
                field: TextField::new(&text),
            });
            return;
        }
        if kind.over_dialogs() || kind == PromptKind::Report(true) {
            let text = match kind {
                PromptKind::RulesExport => format!("highlights{}", crate::config::RULE_SET_SUFFIX),
                PromptKind::FormatExport => self
                    .formats
                    .as_ref()
                    .map(|f| f.state.draft.to_spec().file_name())
                    .unwrap_or_default(),
                PromptKind::Report(_) => {
                    let date =
                        crate::timestamp::format_millis(crate::timestamp::local_now_millis());
                    crate::bookmark_report::report_file_name(&date[..10.min(date.len())])
                }
                _ => String::new(),
            };
            self.prompt = Some(Prompt {
                kind,
                field: TextField::new(&text),
            });
            return;
        }
        let e = &self.tabs[self.active].engine;
        let text = match kind {
            // A regex search opens with its `re:` prefix.
            PromptKind::Search if e.search_mode == crate::tail_engine::SearchMode::Regex => {
                format!("re:{}", e.search_query)
            }
            PromptKind::Search if e.search_mode == crate::tail_engine::SearchMode::Multiline => {
                format!("mre:{}", e.search_query)
            }
            PromptKind::Search => e.search_query.clone(),
            PromptKind::Include if e.filter_expr_mode() => e.filter_expr().to_string(),
            PromptKind::Include => e.include_filter().to_string(),
            PromptKind::Exclude => e.exclude_filter().to_string(),
            PromptKind::Note(line) => e.bookmark_note(line).unwrap_or_default().to_string(),
            PromptKind::Goto => String::new(),
            PromptKind::Split => "100000".to_string(),
            PromptKind::SearchScope => match e.search_scope() {
                crate::tail_engine::SearchScope::All => String::new(),
                crate::tail_engine::SearchScope::Lines { first, last } => format!(
                    "{}-{}",
                    first + 1,
                    last.map(|l| (l + 1).to_string()).unwrap_or_default()
                ),
                crate::tail_engine::SearchScope::Time { from, to } => format!("{from}..{to}"),
            },
            PromptKind::Export(scope, format) => export_path(e, scope, format),
            PromptKind::Report(_) => {
                let date = crate::timestamp::format_millis(crate::timestamp::local_now_millis());
                crate::bookmark_report::report_file_name(&date[..10.min(date.len())])
            }
            PromptKind::RulesImport
            | PromptKind::RulesExport
            | PromptKind::FormatImport
            | PromptKind::FormatExport => unreachable!("opened above"),
            PromptKind::StatsField | PromptKind::OpsField => {
                let cols = e.drawn_field_columns().0;
                cols.first().cloned().unwrap_or_default()
            }
            PromptKind::OpenPlugin => unreachable!("opened above"),
            PromptKind::ExplainSpan => {
                let (f, t) = (e.time_from_text.trim(), e.time_to_text.trim());
                if f.is_empty() && t.is_empty() {
                    String::new()
                } else {
                    format!("{f}..{t}")
                }
            }
            PromptKind::System
            | PromptKind::Remote
            | PromptKind::Listen
            | PromptKind::DebugOutput
            | PromptKind::EventLog => {
                unreachable!("opened above")
            }
            PromptKind::LevelBytes => e
                .level_override()
                .map(|b| b.to_string())
                .unwrap_or_default(),
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

    /// Focuses stream `i`, shown in its window (a stream the tree lacks joins the first
    /// window).
    fn focus_tab(&mut self, i: usize) {
        if i >= self.tabs.len() {
            return;
        }
        self.active = i;
        let path = self.active_path();
        // A stream of a floating window: shown there, the window raised to the top.
        if let Some(Place::Float(f)) = self.place_of(&path) {
            let before = self.floats.clone();
            let mut float = self.floats.remove(f);
            float.pane.show(&path);
            self.floats.push(float);
            if self.floats != before {
                self.dock_dirty = true;
            }
            return;
        }
        let mut dock = self.dock.clone();
        if !dock.show(&path) {
            let open = self.open_paths();
            let floating = self.float_streams();
            dock = dock
                .reconcile(&open, &floating)
                .unwrap_or_else(|| Pane::with_streams(&open));
            dock.show(&path);
        }
        self.set_dock(dock);
    }

    /// `Tab` with several windows: the stream of the next (or previous) window.
    fn cycle_pane(&mut self, forward: bool) {
        let leaves = self.places();
        let n = leaves.len();
        let here = self.focused_place();
        let cur = leaves.iter().position(|l| *l == here).unwrap_or(0);
        for k in 1..=n {
            let at = if forward {
                (cur + k) % n
            } else {
                (cur + n - k % n) % n
            };
            let shown = self
                .shown_at(&leaves[at])
                .or_else(|| match self.leaf(&leaves[at]) {
                    Some(Pane::Leaf { tabs, .. }) => tabs
                        .iter()
                        .filter_map(DockTab::stream)
                        .find_map(|p| self.tab_index(p)),
                    _ => None,
                });
            if let Some(i) = shown {
                self.focus_tab(i);
                return;
            }
        }
    }

    /// `s` / `_`: a new window beside (or below) the focused one with the next stream:
    /// the next tab of the focused window, else the next stream by number.
    fn split_focused(&mut self, dir: Dir) {
        let n = self.tabs.len();
        if n < 2 {
            self.message = Some(tx(self.lang, "A split needs two streams: o opens another").into());
            return;
        }
        if let Place::Float(_) = self.focused_place() {
            self.message = Some(
                tx(
                    self.lang,
                    "A floating window: Alt+F docks it, then it splits",
                )
                .into(),
            );
            return;
        }
        let leaf = self.focused_leaf();
        let me = self.active_path();
        let in_leaf: Vec<PathBuf> = match self.dock.get(&leaf) {
            Some(Pane::Leaf { tabs, .. }) => tabs
                .iter()
                .filter_map(|t| t.stream().map(Path::to_path_buf))
                .collect(),
            _ => Vec::new(),
        };
        let same = |p: &PathBuf| crate::paths::paths_equal(p, &me);
        let pos = in_leaf.iter().position(same).unwrap_or(0);
        let other = in_leaf
            .iter()
            .cycle()
            .skip(pos + 1)
            .take(in_leaf.len())
            .find(|p| !same(p))
            .cloned()
            .unwrap_or_else(|| self.tabs[(self.active + 1) % n].engine.path.clone());
        let dock = self.dock.clone().split_with(&leaf, dir, true, &other);
        self.set_dock(dock);
        self.focus_tab(self.active);
    }

    /// `Ctrl+W`: the focused stream closes, as the GUI's tab ✕ does: its state is kept
    /// in the configuration, its window goes when it was its last tab, and the focus
    /// moves to the stream shown in its place. After the last one the workspace is
    /// empty.
    fn close_stream(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        let i = self.active;
        let path = self.active_path();
        let place = self.focused_place();
        if let Some(s) = self.settings.as_mut() {
            crate::workspace::save_changes(&mut self.tabs[i].engine, &mut s.config, false);
        }
        let window = self.window_rect(&place);
        let tab = self.tabs.remove(i);
        // Indices of the last frame and of a gesture in progress no longer hold.
        self.drag = None;
        self.last_click = None;
        self.dock_drag = None;
        match place {
            Place::Float(f) => {
                match self.floats[f].pane.clone().remove_stream(&path) {
                    Some(pane) => self.floats[f].pane = pane,
                    None => {
                        self.floats.remove(f);
                        self.start_closing(window);
                    }
                }
                self.dock_dirty = true;
            }
            Place::Dock(_) => {
                let leaves = self.dock.leaf_paths().len();
                let dock = self
                    .dock
                    .clone()
                    .remove_stream(&path)
                    .unwrap_or_else(|| Pane::with_streams(&[]));
                // The window went with its last tab.
                if dock.leaf_paths().len() < leaves || dock.streams().is_empty() {
                    self.start_closing(window);
                }
                self.set_dock(dock);
            }
        }
        let leaf = match &place {
            Place::Float(f) if *f < self.floats.len() => place.clone(),
            Place::Float(_) => Place::Dock(Vec::new()),
            Place::Dock(_) => place.clone(),
        };
        self.active = self
            .shown_at(&leaf)
            .or_else(|| {
                let first = self.dock.leaf_paths().into_iter().next()?;
                self.shown_in(&first)
            })
            .or_else(|| {
                let last = self.floats.len().checked_sub(1)?;
                self.shown_at(&Place::Float(last))
            })
            .unwrap_or(0)
            .min(self.tabs.len().saturating_sub(1));
        self.focus_tab(self.active);
        self.message = Some(if self.tabs.is_empty() {
            txf(
                self.lang,
                "Closed {0}: no file open, o opens one",
                &[&tab.title],
            )
        } else {
            txf(self.lang, "Closed {0}", &[&tab.title])
        });
    }

    /// `Alt+X`: the focused window closes; its tabs join the window beside it.
    fn close_pane(&mut self) {
        if let Place::Float(f) = self.focused_place() {
            self.dock_float(f);
            return;
        }
        let leaf = self.focused_leaf();
        if leaf.is_empty() {
            self.message = Some(tx(self.lang, "One window: nothing to close").into());
            return;
        }
        let window = self.window_rect(&Place::Dock(leaf.clone()));
        let dock = self.dock.clone().close_leaf(&leaf);
        self.set_dock(dock);
        self.start_closing(window);
        self.focus_tab(self.active);
    }

    /// A chip of the focused stream's bars: the same as its key, or the switch the GUI's
    /// bar has (monitor, encoding, `Aa`, `.*`, context lines).
    fn run_chip(&mut self, chip: crate::tui::bars::BarChip) {
        use crate::tui::bars::BarChip as C;
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let e = &mut tab.engine;
        match chip {
            C::Follow => self.apply(Action::ToggleFollow),
            C::Monitor => {
                e.is_watching = !e.is_watching;
                self.message = Some(
                    if e.is_watching {
                        tx(self.lang, "Auto-update on")
                    } else {
                        tx(self.lang, "Auto-update off")
                    }
                    .into(),
                );
            }
            C::View => self.apply(Action::ToggleHex),
            C::LineNumbers => self.apply(Action::ToggleLineNumbers),
            C::Encoding => {
                let next = crate::tui::bars::next_encoding(e.encoding);
                e.set_encoding(next);
                self.message = Some(txf(self.lang, "Encoding: {0}", &[&next.name()]));
            }
            C::Ansi => self.apply(Action::CycleAnsi),
            C::Collapse => self.apply(Action::CycleCollapse),
            C::Context => {
                let next = crate::tui::bars::next_context(e.context_lines());
                e.set_context_lines(next);
                self.message = Some(txf(self.lang, "Context lines: {0}", &[&next]));
            }
            C::Time => self.apply(Action::TimeRange),
            C::Include => self.apply(Action::EditInclude),
            C::Exclude => self.apply(Action::EditExclude),
            C::Case => {
                e.filter_case_sensitive = !e.filter_case_sensitive;
                e.refresh_filters();
            }
            C::Regex => {
                e.filter_is_regex = !e.filter_is_regex;
                e.refresh_filters();
            }
            C::Level => self.apply(Action::CycleLevel),
            C::LevelBytes => self.open_prompt(PromptKind::LevelBytes),
            C::Hidden => self.hidden_menu = Some(0),
            C::OutOfOrder => self.apply(Action::NextOutOfOrder),
            C::SourceEnded => self.apply(Action::Reconnect),
            C::Columns => self.apply(Action::ToggleColumns),
            C::TimeDelta => self.apply(Action::ToggleTimeDelta),
            C::FilterExpr => self.apply(Action::ToggleFilterExpr),
            C::TimeDisplay => {
                let next = crate::tui::bars::next_time_display(e.time_display());
                self.apply(Action::SetTimeDisplay(next));
            }
            C::Fields => self.columns_menu = Some(0),
            C::Presets => self.apply(Action::Presets),
            C::Global => self.apply(Action::EditGlobal),
            C::Named(i) => e.named.toggle_focus(i),
        }
    }

    /// Where the window at `place` was drawn on the last frame.
    fn window_rect(&self, place: &Place) -> Option<Rect> {
        match place {
            Place::Float(f) => self
                .hits
                .floats
                .iter()
                .find(|(_, i)| i == f)
                .map(|(r, _)| *r),
            Place::Dock(path) => self
                .hits
                .leaves
                .iter()
                .find(|l| &l.path == path)
                .map(|l| l.area),
        }
    }

    /// The closing effect of a window that was drawn at `rect`.
    fn start_closing(&mut self, rect: Option<Rect>) {
        if let Some(rect) = rect.filter(|r| r.area() > 0) {
            self.closing.push((rect, Instant::now()));
        }
    }

    /// The closing windows: each outline shrinks to its centre, dimmer as it goes.
    fn draw_closing(&self, frame: &mut Frame) {
        for (rect, at) in &self.closing {
            let t = (at.elapsed().as_secs_f32() / CLOSE_EFFECT.as_secs_f32()).min(1.0);
            if t >= 1.0 {
                continue;
            }
            let scale = 1.0 - t;
            let w = ((rect.width as f32 * scale) as u16).max(2);
            let h = ((rect.height as f32 * scale) as u16).max(2);
            let r = Rect::new(
                rect.x + (rect.width - w.min(rect.width)) / 2,
                rect.y + (rect.height - h.min(rect.height)) / 2,
                w.min(rect.width),
                h.min(rect.height),
            )
            .intersection(frame.area());
            let style = if t < 0.5 {
                Style::default().fg(self.palette.accent())
            } else {
                Style::default().fg(self.palette.dim())
            };
            let block = Block::bordered()
                .border_set(self.palette.border_set(Chrome::Plain))
                .border_style(style);
            frame.render_widget(block, r);
        }
    }

    /// `Alt+arrows`: the nearest divider of the focused window moves by `delta`.
    fn resize_pane(&mut self, dir: Dir, delta: f32) {
        // A floating window moves instead: 4 columns or 1 row a step.
        if let Place::Float(f) = self.focused_place() {
            let r = self.floats[f].rect;
            let back = delta < 0.0;
            let moved = match dir {
                Dir::Horizontal if back => Rect {
                    x: r.x.saturating_sub(4),
                    ..r
                },
                Dir::Horizontal => Rect { x: r.x + 4, ..r },
                Dir::Vertical if back => Rect {
                    y: r.y.saturating_sub(1),
                    ..r
                },
                Dir::Vertical => Rect { y: r.y + 1, ..r },
            };
            self.floats[f].rect = dock::clamp_into(moved, self.main_area());
            self.dock_dirty = true;
            return;
        }
        let leaf = self.focused_leaf();
        let Some((split, _)) = self.dock.split_above(&leaf, dir) else {
            self.message = Some(tx(self.lang, "No divider that way").into());
            return;
        };
        if let Some(Pane::Split { fraction, .. }) = self.dock.get(&split) {
            let value = fraction + delta;
            let mut dock = self.dock.clone();
            dock.set_fraction(&split, value);
            self.set_dock(dock);
        }
    }

    /// `<` / `>`: the focused stream becomes a tab of the previous / next window.
    fn move_to_pane(&mut self, forward: bool) {
        if let Place::Float(_) = self.focused_place() {
            self.message = Some(tx(self.lang, "A floating window: Alt+F docks it").into());
            return;
        }
        let leaves = self.dock.leaf_paths();
        let n = leaves.len();
        if n < 2 {
            self.message = Some(tx(self.lang, "One window: s splits it").into());
            return;
        }
        let here = self.focused_leaf();
        let cur = leaves.iter().position(|l| *l == here).unwrap_or(0);
        let target = &leaves[if forward {
            (cur + 1) % n
        } else {
            (cur + n - 1) % n
        }];
        let dock = self.dock.clone().move_stream(target, &self.active_path());
        self.set_dock(dock);
        self.focus_tab(self.active);
    }

    /// `Ctrl+PgUp/PgDn`: the previous / next tab of the focused window.
    fn step_in_pane(&mut self, forward: bool) {
        let place = self.focused_place();
        self.show_leaf_tab(&place, None, forward);
    }

    /// Shows tab `pos` of the leaf at `leaf` (or the next / previous one when `None`),
    /// focusing its stream.
    fn show_leaf_tab(&mut self, place: &Place, pos: Option<usize>, forward: bool) {
        let Some(Pane::Leaf { tabs, active }) = self.leaf(place).cloned() else {
            return;
        };
        let n = tabs.len();
        let next = match pos {
            Some(p) => p.min(n - 1),
            None if n < 2 => {
                self.message = Some(tx(self.lang, "One tab in this window").into());
                return;
            }
            None if forward => (active + 1) % n,
            None => (active + n - 1) % n,
        };
        match place {
            Place::Dock(leaf) => {
                let mut dock = self.dock.clone();
                if let Some(Pane::Leaf { active, .. }) = dock.get_mut(leaf) {
                    *active = next;
                }
                self.set_dock(dock);
            }
            Place::Float(f) => {
                if let Pane::Leaf { active, .. } = &mut self.floats[*f].pane {
                    *active = next;
                }
                self.dock_dirty = true;
            }
        }
        if let Some(i) = tabs[next].stream().and_then(|p| self.tab_index(p)) {
            self.focus_tab(i);
        }
    }

    /// `Alt+F`: the focused window floats over the dock, or a floating one docks back.
    fn toggle_float(&mut self) {
        match self.focused_place() {
            Place::Float(f) => self.dock_float(f),
            Place::Dock(_) => {
                let rect = self.cascade(self.floats.len());
                let path = self.active_path();
                self.float_stream(&path, rect);
            }
        }
    }

    /// `stream` in a new floating window at `rect`, taken out of the window it was in
    /// (the dock may be left empty).
    fn float_stream(&mut self, stream: &Path, rect: Rect) {
        match self.place_of(stream) {
            Some(Place::Float(f)) => match self.floats[f].pane.clone().remove_stream(stream) {
                Some(pane) => self.floats[f].pane = pane,
                None => {
                    self.floats.remove(f);
                }
            },
            _ => {
                let dock = self
                    .dock
                    .clone()
                    .remove_stream(stream)
                    .unwrap_or_else(|| Pane::with_streams(&[]));
                self.set_dock(dock);
            }
        }
        self.floats.push(Float {
            pane: Pane::with_streams(&[stream.to_path_buf()]),
            rect: dock::clamp_into(rect, self.main_area()),
        });
        self.dock_dirty = true;
        if let Some(i) = self.tab_index(stream) {
            self.focus_tab(i);
        }
    }

    /// The floating window `f` docks back: its tabs join the first window of the dock
    /// (an empty dock takes the window whole).
    fn dock_float(&mut self, f: usize) {
        let float = self.floats.remove(f);
        if self.dock.streams().is_empty() {
            self.set_dock(float.pane);
            self.focus_tab(self.active);
            return;
        }
        let first = self
            .dock
            .leaf_paths()
            .into_iter()
            .next()
            .unwrap_or_default();
        let mut dock = self.dock.clone();
        if let Pane::Leaf { tabs, .. } = float.pane {
            for tab in tabs {
                dock.add_tab(&first, tab);
            }
        }
        self.set_dock(dock);
        self.dock_dirty = true;
        self.focus_tab(self.active);
    }

    /// A stream dropped on the leaf at `leaf`: an edge splits it, the centre adds a tab.
    fn drop_stream(&mut self, stream: &Path, leaf: &[bool], zone: Zone, preview: Rect) {
        if zone == Zone::Float {
            self.float_stream(stream, preview);
            return;
        }
        // A tab dragged out of a floating window leaves it first.
        if let Some(Place::Float(f)) = self.place_of(stream) {
            match self.floats[f].pane.clone().remove_stream(stream) {
                Some(pane) => self.floats[f].pane = pane,
                None => {
                    self.floats.remove(f);
                }
            }
            self.dock_dirty = true;
        }
        let dock = match zone.split() {
            // An empty dock takes the stream whole, wherever it is dropped.
            _ if self.dock.streams().is_empty() => Pane::with_streams(&[stream.to_path_buf()]),
            Some((dir, after)) => self.dock.clone().split_with(leaf, dir, after, stream),
            None => self.dock.clone().move_stream(leaf, stream),
        };
        self.set_dock(dock);
        if let Some(i) = self.tab_index(stream) {
            self.focus_tab(i);
        }
    }

    pub fn apply(&mut self, action: Action) {
        if self.apply_workspace_action(action) {
            return;
        }
        let n_tabs = self.tabs.len();
        // With no file open only what opens one (or leaves) acts.
        if n_tabs == 0 {
            match action {
                Action::Quit | Action::CopyOrQuit => self.quit = true,
                Action::ToggleHelp => {
                    self.show_help = !self.show_help;
                    self.help_sel = help_step(HELP.len() - 1, 1);
                    self.help_top = 0;
                }
                Action::OpenFile => self.open_browser(),
                Action::OpenSystem => self.open_prompt(PromptKind::System),
                Action::OpenRemote => self.open_prompt(PromptKind::Remote),
                Action::PasteStream => self.paste_stream(),
                Action::PasteAppend => self.paste_append(),
                Action::OpenListen => self.open_prompt(PromptKind::Listen),
                Action::OpenDebugOutput => self.open_prompt(PromptKind::DebugOutput),
                Action::OpenEventLog => self.open_prompt(PromptKind::EventLog),
                Action::OpenSession => self.open_sessions(),
                Action::Settings => self.open_settings(),
                Action::EditRules => self.open_rules(),
                Action::EditGlobal => self.open_global(),
                Action::ToggleGlobal => self.toggle_global(),
                Action::CycleTheme => self.cycle_theme(),
                Action::PlayAll | Action::PauseAll => {}
                Action::About => self.show_about = !self.show_about,
                Action::LogFormats => self.open_formats(),
                Action::Scratchpad => self.open_pad(),
                Action::KeysDialog => self.keys_dialog = Some(KeysDialog::default()),
                Action::ToggleSetting(s) => self.toggle_setting(s),
                Action::Plugins => self.open_plugins(),
                _ => self.message = Some(tx(self.lang, NO_FILE).into()),
            }
            return;
        }
        let row_action = matches!(
            action,
            Action::ToggleBookmark
                | Action::NextBookmark
                | Action::PrevBookmark
                | Action::NextOutOfOrder
                | Action::PrevOutOfOrder
                | Action::EditNote
                | Action::ToggleContext
                | Action::SelectUp
                | Action::SelectDown
        );
        let asm_select = self.tabs[self.active].is_asm()
            && matches!(action, Action::SelectUp | Action::SelectDown);
        if row_action && self.tabs[self.active].is_hex() && !asm_select {
            self.message = Some(
                tx(
                    self.lang,
                    "The HEX view shows bytes, not lines: h returns to them",
                )
                .into(),
            );
            return;
        }
        match action {
            Action::Quit => self.quit = true,
            // With several windows `Tab` moves the focus between them; otherwise it
            // shows the next stream.
            Action::NextTab | Action::PrevTab if self.places().len() > 1 => {
                self.cycle_pane(action == Action::NextTab)
            }
            Action::NextTab => self.focus_tab((self.active + 1) % n_tabs),
            Action::PrevTab => self.focus_tab((self.active + n_tabs - 1) % n_tabs),
            Action::GotoTab(i) => self.focus_tab(i),
            Action::ToggleHelp => {
                self.show_help = !self.show_help;
                self.help_sel = help_step(HELP.len() - 1, 1);
                self.help_top = 0;
            }
            Action::StartSearch => self.open_prompt(PromptKind::Search),
            Action::EditInclude => self.open_prompt(PromptKind::Include),
            Action::Split => self.open_prompt(PromptKind::Split),
            Action::OpenSystem => self.open_prompt(PromptKind::System),
            Action::OpenRemote => self.open_prompt(PromptKind::Remote),
            Action::PasteStream => self.paste_stream(),
            Action::PasteAppend => self.paste_append(),
            Action::OpenListen => self.open_prompt(PromptKind::Listen),
            Action::OpenDebugOutput => self.open_prompt(PromptKind::DebugOutput),
            Action::OpenEventLog => self.open_prompt(PromptKind::EventLog),
            Action::Reconnect => {
                let engine = self.tabs.get_mut(self.active).map(|t| &mut t.engine);
                match engine {
                    Some(e) if e.system.as_ref().is_some_and(|s| !s.is_running()) => {
                        if let Some(s) = e.system.as_mut() {
                            s.reconnect_requested = true;
                        }
                    }
                    Some(e) if e.ssh.as_ref().is_some_and(|s| s.is_stopped()) => {
                        if let Some(s) = e.ssh.as_mut() {
                            s.reconnect_requested = true;
                        }
                    }
                    _ => self.message = Some(tx(self.lang, "No ended system source").into()),
                }
            }
            Action::GoTo => self.open_prompt(PromptKind::Goto),
            Action::FindAll => self.open_find(),
            Action::Palette => self.open_palette(),
            Action::Label(n) => self.toggle_label(n),
            Action::LabelPrefix => {
                self.label_pending = true;
                self.message = Some(tx(self.lang, "Label colour: 1-9 (the search text)").into());
            }
            Action::TimeRange => self.open_time_range(),
            Action::CycleTheme => self.cycle_theme(),
            Action::PlayAll | Action::PauseAll => self.play_all(action == Action::PlayAll),
            Action::Settings => self.open_settings(),
            Action::EditRules => self.open_rules(),
            Action::Presets => self.open_presets(),
            Action::EditGlobal => self.open_global(),
            Action::ToggleGlobal => self.toggle_global(),
            Action::Tools => self.open_tool_menu(),
            Action::Lock => self.lock(),
            Action::OpenFile => self.open_browser(),
            Action::OpenSession => self.open_sessions(),
            Action::SaveSession => self.open_prompt(PromptKind::SaveSession),
            Action::JsonTree => self.open_json(),
            Action::About => self.show_about = !self.show_about,
            Action::EditNote => match self.tabs[self.active].cursor_line() {
                Some(line) => self.open_prompt(PromptKind::Note(line)),
                None => self.message = Some(tx(self.lang, "No row for a note").into()),
            },
            Action::EditExclude => self.open_prompt(PromptKind::Exclude),
            Action::SplitRight => self.split_focused(Dir::Horizontal),
            Action::SplitDown => self.split_focused(Dir::Vertical),
            Action::NextError | Action::PrevError | Action::NextWarn | Action::PrevWarn => {
                self.apply_counted(action, 1)
            }
            Action::ClosePane => self.close_pane(),
            Action::ToggleFloat => self.toggle_float(),
            Action::CloseStream => self.close_stream(),
            Action::ResizeLeft => self.resize_pane(Dir::Horizontal, -RESIZE_STEP),
            Action::ResizeRight => self.resize_pane(Dir::Horizontal, RESIZE_STEP),
            Action::ResizeUp => self.resize_pane(Dir::Vertical, -RESIZE_STEP),
            Action::ResizeDown => self.resize_pane(Dir::Vertical, RESIZE_STEP),
            Action::MovePrevPane => self.move_to_pane(false),
            Action::MoveNextPane => self.move_to_pane(true),
            Action::PrevInPane => self.step_in_pane(false),
            Action::NextInPane => self.step_in_pane(true),
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
        let text = if tab.is_asm() {
            if tab.engine.asm.selection.is_none() {
                let at = tab.asm_cursor_offset();
                tab.engine.asm.selection = Some((at, at));
                let text = tab.engine.asm_selection_text();
                tab.engine.asm.selection = None;
                text
            } else {
                tab.engine.asm_selection_text()
            }
        } else if tab.engine.has_selection() {
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
            self.message = Some(tx(self.lang, "Nothing to copy").into());
            return;
        };
        self.copy_text(&text);
    }

    // ----- Mouse -----

    /// Handles a mouse event against the last frame; returns true when the screen
    /// changed.
    pub fn on_mouse(&mut self, ev: MouseEvent) -> bool {
        let changed = self.on_mouse_inner(ev);
        self.settings_live() || changed
    }

    fn on_mouse_inner(&mut self, ev: MouseEvent) -> bool {
        self.idle.touch(Instant::now());
        // The mouse does nothing while locked.
        if self.locked.is_some() {
            return false;
        }
        match ev.kind {
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                if let Some(b) = self.browser.as_mut() {
                    let up = ev.kind == MouseEventKind::ScrollUp;
                    b.move_by(if up {
                        -(WHEEL_ROWS as isize)
                    } else {
                        WHEEL_ROWS as isize
                    });
                    return true;
                }
                if self.prompt.is_some()
                    || self.time_range.is_some()
                    || self.picker.is_some()
                    || self.sessions.is_some()
                    || self.settings_form.is_some()
                    || self.rules.is_some()
                    || self.formats.is_some()
                    || self.compare.is_some()
                    || self.pad_editor.is_some()
                    || self.analysis.is_some()
                    || self.analysis_menu.is_some()
                    || self.merge.is_some()
                    || self.recent_menu.is_some()
                    || self.info_view.is_some()
                    || self.keys_dialog.is_some()
                    || self.plugins.is_some()
                    || self.presets.is_some()
                    || self.global.is_some()
                    || self.tool_menu.is_some()
                    || self.hidden_menu.is_some()
                    || self.piece_menu.is_some()
                    || self.results_menu.is_some()
                    || self.export_menu.is_some()
                    || self.columns_menu.is_some()
                    || self.palette_dialog.is_some()
                    || self.json_dialog.is_some()
                    || self.find_window.as_ref().is_some_and(|w| w.shown)
                    || self.show_about
                    || self.tools_editor.is_some()
                    || self.confirm_overwrite.is_some()
                    || self.interface_switch.is_some()
                    || self.show_help
                {
                    return false;
                }
                let Some(tab) = mouse::window_at(&self.hits, ev.column, ev.row) else {
                    return false;
                };
                let t = &mut self.tabs[tab];
                if t.is_asm() {
                    t.engine.follow_tail = false;
                    let up = ev.kind == MouseEventKind::ScrollUp;
                    t.engine.asm_scroll(if up {
                        -(WHEEL_ROWS as isize)
                    } else {
                        WHEEL_ROWS as isize
                    });
                    return true;
                }
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
            MouseEventKind::Drag(MouseButton::Left) if self.dock_drag.is_some() => {
                self.on_dock_drag(ev.column, ev.row)
            }
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
                match self.dock_drag.take() {
                    Some(DockDrag::Move {
                        stream,
                        drop: Some((leaf, zone, preview)),
                        ..
                    }) => {
                        self.drop_stream(&stream, &leaf, zone, preview);
                        true
                    }
                    Some(_) => true,
                    None => false,
                }
            }
            _ => false,
        }
    }

    fn start_move(&mut self, tab: usize, ev: MouseEvent) {
        self.dock_drag = Some(DockDrag::Move {
            stream: self.tabs[tab].engine.path.clone(),
            from: (ev.column, ev.row),
            drop: None,
        });
    }

    /// A drag of a divider (the split follows the pointer) or of a window (the drop
    /// zone under the pointer is shown).
    fn on_dock_drag(&mut self, col: u16, row: u16) -> bool {
        match self.dock_drag.clone() {
            Some(DockDrag::Divider { path, dir, split }) => {
                let mut dock = self.dock.clone();
                dock.set_fraction(&path, dock::fraction_at(split, dir, col, row));
                let changed = dock != self.dock;
                self.set_dock(dock);
                changed
            }
            Some(DockDrag::Move { stream, from, .. }) => {
                let at = ratatui::layout::Position::new(col, row);
                let main = self.main_area();
                let drop = if (col, row) == from {
                    None
                } else {
                    // Over a window: its zones; anywhere else: a floating window.
                    let over = self.hits.leaves.iter().find(|l| l.area.contains(at));
                    let empty = self.dock.streams().is_empty();
                    Some(match over {
                        // The empty dock is one drop zone.
                        Some(l) if empty => (l.path.clone(), Zone::Center, l.area),
                        Some(l) => match dock::zone_at(l.area, col, row) {
                            Zone::Float => (
                                l.path.clone(),
                                Zone::Float,
                                dock::float_rect_at(main, col, row),
                            ),
                            zone => (l.path.clone(), zone, dock::zone_rect(l.area, zone)),
                        },
                        None => (Vec::new(), Zone::Float, dock::float_rect_at(main, col, row)),
                    })
                };
                self.dock_drag = Some(DockDrag::Move { stream, from, drop });
                true
            }
            // The floating window being dragged is the topmost (raised on the click).
            Some(DockDrag::FloatMove { grab }) => {
                let Some(f) = self.floats.last_mut() else {
                    return false;
                };
                let moved = Rect {
                    x: col.saturating_sub(grab.0),
                    y: row.saturating_sub(grab.1),
                    ..f.rect
                };
                let main = if self.hits.main.area() > 0 {
                    self.hits.main
                } else {
                    moved
                };
                f.rect = dock::clamp_into(moved, main);
                self.dock_dirty = true;
                true
            }
            Some(DockDrag::FloatResize { edges, from }) => {
                let main = self.main_area();
                let Some(f) = self.floats.last_mut() else {
                    return false;
                };
                f.rect = dock::clamp_into(dock::resized(from, edges, col, row), main);
                self.dock_dirty = true;
                true
            }
            Some(DockDrag::DialogMove { which, grab }) => {
                let Some(at) = *self.dialog_rect(which) else {
                    return false;
                };
                let moved = Rect {
                    x: col.saturating_sub(grab.0),
                    y: row.saturating_sub(grab.1),
                    ..at
                };
                let screen = self.hits.screen;
                *self.dialog_rect(which) = Some(fit_dialog(moved, which, screen));
                true
            }
            Some(DockDrag::DialogResize { which, edges, from }) => {
                let screen = self.hits.screen;
                let min = dialog_min(which);
                // At least the dialog's minimum: the moving sides stop there.
                let mut r = dock::resized(from, edges, col, row);
                if r.width < min.0 {
                    if edges.left {
                        r.x = from.right().saturating_sub(min.0);
                    }
                    r.width = min.0;
                }
                if r.height < min.1 {
                    if edges.top {
                        r.y = from.bottom().saturating_sub(min.1);
                    }
                    r.height = min.1;
                }
                *self.dialog_rect(which) = Some(fit_dialog(r, which, screen));
                true
            }
            None => false,
        }
    }

    fn on_click(&mut self, ev: MouseEvent) -> bool {
        let target = mouse::hit_test(&self.hits, ev.column, ev.row);
        match target {
            Target::ListItem(i) if self.show_help => self.run_help_entry(i),
            Target::Button(i) => {
                if let Some((_, _, action)) = STATUS.get(i) {
                    self.message = None;
                    self.apply(*action);
                }
            }
            Target::TopButton(action) => {
                self.message = None;
                self.apply(action);
            }
            Target::BarChip(tab, chip) => {
                self.message = None;
                self.focus_tab(tab);
                self.run_chip(chip);
            }
            Target::DialogClose => {
                self.cancel_settings();
                self.show_help = false;
            }
            Target::DialogTitle(which) | Target::DialogEdge(which, _) => {
                let Some(outer) = self.hits.dialog.map(|d| d.outer) else {
                    return false;
                };
                let now = Instant::now();
                let double = matches!(target, Target::DialogTitle(_))
                    && self
                        .last_title_click
                        .is_some_and(|at| now.duration_since(at) <= DOUBLE_CLICK);
                if double {
                    // Back to the centred default size.
                    self.last_title_click = None;
                    *self.dialog_rect(which) = None;
                    return true;
                }
                self.last_title_click = matches!(target, Target::DialogTitle(_)).then_some(now);
                *self.dialog_rect(which) = Some(outer);
                self.dock_drag = Some(match target {
                    Target::DialogEdge(_, edges) => DockDrag::DialogResize {
                        which,
                        edges,
                        from: outer,
                    },
                    _ => DockDrag::DialogMove {
                        which,
                        grab: (ev.column - outer.x, ev.row - outer.y),
                    },
                });
            }
            Target::GotoLine(tab) => {
                self.message = None;
                self.focus_tab(tab);
                self.apply(Action::GoTo);
            }
            Target::VolumeCell(tab, ms) => {
                self.focus_tab(tab);
                let t = &mut self.tabs[tab];
                if let Some(line) = t.engine.goto_time(ms) {
                    t.engine.follow_tail = false;
                    t.engine.reveal_line(line);
                    t.engine.scroll_to_line = Some(line);
                }
            }
            Target::ListItem(i) if self.time_range.is_some() => {
                if let Some(d) = self.time_range.as_mut() {
                    if i >= RANGE_DAY {
                        d.zone = RangeZone::Calendar;
                        d.calendar.cursor = (i - RANGE_DAY) as i64;
                        d.pick_day();
                    } else {
                        d.zone = RangeZone::Field;
                        d.on_to = i == RANGE_TO;
                        d.sync_calendar();
                    }
                }
            }
            // The rule list: a click selects a rule, a click on the selected one edits
            // it; in the form a click focuses a field.
            Target::ListItem(i) if self.rules.is_some() => {
                if let Some(d) = self.rules.as_mut() {
                    match d.form.as_mut() {
                        Some(f) => f.focus = i.min(f.fields.len().saturating_sub(1)),
                        // The rows after the rules are the quick labels.
                        None if i >= d.list.items.len() => {
                            d.on_labels = true;
                            d.label_sel =
                                (i - d.list.items.len()).min(d.labels.len().saturating_sub(1));
                        }
                        None if d.list.selected == i && !d.on_labels => d.edit_selected(),
                        None => {
                            d.on_labels = false;
                            d.list.selected = i.min(d.list.items.len().saturating_sub(1));
                        }
                    }
                }
            }
            // The tools editor: a click selects a tool, a second click edits it; in the
            // form a click focuses a field.
            Target::ListItem(i) if self.tools_editor.is_some() => {
                if let Some(d) = self.tools_editor.as_mut() {
                    match d.form.as_mut() {
                        Some(f) => f.focus = i.min(f.fields.len().saturating_sub(1)),
                        None if d.list.selected == i => d.edit_selected(),
                        None => d.list.selected = i.min(d.list.items.len().saturating_sub(1)),
                    }
                }
            }
            // A click selects a row, a click on the selected row runs it (`Enter`).
            Target::ListItem(i) if self.find_window.as_ref().is_some_and(|w| w.shown) => {
                if let Some(w) = self.find_window.as_mut() {
                    w.editing = false;
                    if w.selected == i {
                        self.activate_find(false);
                    } else {
                        w.selected = i.min(w.row_count().saturating_sub(1));
                    }
                }
            }
            // A click folds or unfolds the node.
            Target::ListItem(i) if self.json_dialog.is_some() => {
                if let Some(d) = self.json_dialog.as_mut() {
                    d.toggle_row(i);
                }
            }
            // A click runs the palette entry.
            Target::ListItem(i) if self.palette_dialog.is_some() => {
                if let Some(p) = self.palette_dialog.take() {
                    if let Some(e) = p.entries.get(i) {
                        self.run_palette(e.id, e.action);
                    }
                }
            }
            // A click shows that hidden range again.
            Target::ListItem(i) if self.hidden_menu.is_some() => {
                self.show_hidden(Some(i));
            }
            // A click goes to that piece.
            Target::ListItem(i) if self.piece_menu.is_some() => self.go_to_piece(i),
            Target::ListItem(i) if self.results_menu.is_some() => self.go_to_result(i),
            Target::ListItem(i) if self.export_menu.is_some() => self.choose_export(i),
            Target::ListItem(i) if self.recent_menu.is_some() => {
                self.recent_menu = Some(i);
                let enter = crossterm::event::KeyEvent::new(
                    crossterm::event::KeyCode::Enter,
                    crossterm::event::KeyModifiers::NONE,
                );
                self.on_recent_key(enter);
            }
            Target::ListItem(i) if self.merge.is_some() => {
                if let Some(d) = self.merge.as_mut() {
                    d.selected = i;
                }
                let space = crossterm::event::KeyEvent::new(
                    crossterm::event::KeyCode::Char(' '),
                    crossterm::event::KeyModifiers::NONE,
                );
                self.on_merge_key(space);
            }
            Target::ListItem(i) if self.analysis_menu.is_some() => {
                self.analysis_menu = None;
                if let Some(&kind) = crate::tui::analysis::Kind::ALL.get(i) {
                    self.open_analysis(kind);
                }
            }
            // A click shows or hides that column.
            Target::ListItem(i) if self.columns_menu.is_some() => {
                self.columns_menu = Some(i);
                self.toggle_column(i);
            }
            // A click runs the tool.
            Target::ListItem(i) if self.tool_menu.is_some() => {
                self.tool_menu = None;
                self.run_tool(i);
            }
            // A click focuses a row of the global filter editor.
            Target::ListItem(i) if self.global.is_some() => {
                if let Some(d) = self.global.as_mut() {
                    d.focus = i.min(d.rows().len().saturating_sub(1));
                }
            }
            // A click selects a preset, a click on the selected one applies it.
            Target::ListItem(i) if self.presets.is_some() => {
                let apply = self.presets.as_mut().and_then(|d| {
                    let hit = d.list.selected == i;
                    d.list.selected = i.min(d.list.items.len().saturating_sub(1));
                    hit.then_some(d.list.selected)
                });
                if let Some(index) = apply {
                    self.apply_preset(index, false);
                }
            }
            Target::ListItem(i) if self.settings_form.is_some() => {
                let tools = self.settings_form.as_mut().is_some_and(|f| {
                    f.focus = i;
                    f.fields
                        .get(i)
                        .is_some_and(|x| x.key == crate::tui::settings::Key::Tools)
                });
                // The External tools row opens their editor.
                if tools {
                    self.open_tools_editor();
                }
            }
            // One click selects, a click on the selected entry opens it (as a double
            // click does in a file dialog).
            Target::ListItem(i) if self.browser.is_some() => {
                if let Some(b) = self.browser.as_mut() {
                    if b.selected == i {
                        self.submit_browser();
                    } else {
                        b.selected = i;
                    }
                }
            }
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
            Target::DialogOk if self.interface_switch.is_some() => {
                self.confirm_interface_switch();
            }
            Target::DialogOk => {
                if let Some(d) = self.tools_editor.as_mut() {
                    if d.form.is_none() {
                        self.tools_editor = None;
                    } else if d.submit_form() {
                        self.store_tools();
                    }
                } else if self.json_dialog.is_some() {
                    self.json_dialog = None;
                } else if self.find_window.as_ref().is_some_and(|w| w.shown) {
                    self.close_find();
                } else if self.show_about {
                    self.show_about = false;
                } else if self.palette_dialog.is_some() {
                    // OK is Enter.
                    let enter = crossterm::event::KeyEvent::new(
                        crossterm::event::KeyCode::Enter,
                        KeyModifiers::NONE,
                    );
                    if !self.on_palette_key(enter) {
                        self.palette_dialog = None;
                    }
                } else if self.hidden_menu.take().is_some()
                    || self.piece_menu.take().is_some()
                    || self.results_menu.take().is_some()
                    || self.export_menu.take().is_some()
                    || self.analysis_menu.take().is_some()
                    || self.merge.take().is_some()
                    || self.recent_menu.take().is_some()
                    || self.info_view.take().is_some()
                    || self.columns_menu.take().is_some()
                {
                } else if let Some(m) = self.tool_menu.take() {
                    if m.count > 0 {
                        self.run_tool(m.selected);
                    }
                } else if self.global.is_some() {
                    self.close_global();
                } else if let Some(d) = self.presets.as_mut() {
                    match d.mode {
                        presets::Mode::List => self.presets = None,
                        _ => {
                            let enter = crossterm::event::KeyEvent::new(
                                crossterm::event::KeyCode::Enter,
                                KeyModifiers::NONE,
                            );
                            self.on_presets_key(enter);
                        }
                    }
                } else if let Some(d) = self.rules.as_mut() {
                    if d.form.is_none() {
                        self.rules = None;
                    } else if d.submit_form() {
                        self.apply_rules();
                    }
                } else if self.settings_form.is_some() {
                    self.submit_settings();
                } else if let Some(file) = self.confirm_overwrite.take() {
                    self.save_session(&file);
                } else if self.sessions.is_some() {
                    self.submit_sessions();
                } else if self.picker.is_some() {
                    self.open_picked();
                } else if self.browser.is_some() {
                    self.submit_browser();
                } else if self.time_range.is_some() {
                    self.submit_time_range();
                } else if let Some(prompt) = self.prompt.take() {
                    self.submit_prompt(prompt);
                }
                self.show_help = false;
            }
            // Cancel in the rule form drops the form only: the list's changes are
            // already applied.
            // Cancel in the tool form drops the form only.
            Target::DialogCancel
                if self.tools_editor.as_ref().is_some_and(|d| d.form.is_some()) =>
            {
                if let Some(d) = self.tools_editor.as_mut() {
                    d.form = None;
                }
            }
            // A click outside the tools editor closes it, back to Settings if open.
            Target::OutsideDialog if self.tools_editor.is_some() => self.tools_editor = None,
            // Cancel while naming or confirming goes back to the preset list.
            Target::DialogCancel
                if self
                    .presets
                    .as_ref()
                    .is_some_and(|d| d.mode != presets::Mode::List) =>
            {
                if let Some(d) = self.presets.as_mut() {
                    d.mode = presets::Mode::List;
                    d.problem = None;
                }
            }
            Target::DialogCancel if self.rules.as_ref().is_some_and(|d| d.form.is_some()) => {
                if let Some(d) = self.rules.as_mut() {
                    d.form = None;
                }
            }
            Target::DialogCancel | Target::OutsideDialog => {
                // The global filter's edits are kept, as the GUI's bar keeps them.
                self.close_global();
                self.tool_menu = None;
                self.hidden_menu = None;
                self.columns_menu = None;
                self.palette_dialog = None;
                self.json_dialog = None;
                if let Some(w) = self.find_window.as_mut() {
                    w.shown = false;
                }
                self.show_about = false;
                self.prompt = None;
                self.rules = None;
                self.presets = None;
                self.time_range = None;
                self.sessions = None;
                self.cancel_settings();
                self.confirm_overwrite = None;
                self.interface_switch = None;
                self.browser = None;
                self.close_picker();
                self.show_help = false;
            }
            Target::DialogBody | Target::Nothing => return false,
            Target::Divider(i) => {
                if let Some(d) = self.hits.dividers.get(i) {
                    self.dock_drag = Some(DockDrag::Divider {
                        path: d.path.clone(),
                        dir: d.dir,
                        split: d.split,
                    });
                }
            }
            Target::LeafTab(i) => {
                let Some((_, place, pos)) = self.hits.leaf_tabs.get(i).cloned() else {
                    return false;
                };
                self.show_leaf_tab(&place, Some(pos), true);
                let place = self.focused_place();
                if let Some(i) = self.shown_at(&place) {
                    self.start_move(i, ev);
                }
            }
            // A floating window: raised and focused, then moved by its title or resized
            // by its bottom-right corner.
            // `[x]`: the stream the window shows closes, as with Ctrl+W.
            Target::Close(i) => {
                let Some((_, place)) = self.hits.close_buttons.get(i).cloned() else {
                    return false;
                };
                if let Some(idx) = self.shown_at(&place) {
                    self.focus_tab(idx);
                    self.close_stream();
                }
            }
            Target::FloatTitle(f) | Target::FloatEdge(f, _) => {
                let Some(&(rect, _)) = self.hits.floats.iter().find(|(_, i)| *i == f) else {
                    return false;
                };
                let shown = self.shown_at(&Place::Float(f));
                match shown {
                    Some(i) => self.focus_tab(i),
                    None => {
                        let float = self.floats.remove(f);
                        self.floats.push(float);
                    }
                }
                self.dock_drag = Some(match target {
                    Target::FloatEdge(_, edges) => DockDrag::FloatResize { edges, from: rect },
                    _ => DockDrag::FloatMove {
                        grab: (ev.column - rect.x, ev.row - rect.y),
                    },
                });
            }
            // A title is also where a window is taken to be moved.
            Target::TabTitle(i) | Target::WindowTitle(i) => {
                self.focus_tab(i);
                self.start_move(i, ev);
            }
            Target::Window(i) => self.focus_tab(i),
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

    fn apply_to_tab(&mut self, action: Action) {
        // A held PgUp / PgDn moves further (`page_acceleration`).
        let accelerate = self
            .settings
            .as_ref()
            .is_none_or(|s| s.config.page_acceleration);
        let pages = match action {
            Action::PageUp | Action::PageDown if accelerate => self
                .page_hold
                .press(action == Action::PageDown, Instant::now()),
            _ => 1,
        };
        let tab = &mut self.tabs[self.active];
        let page = tab.height.max(1);
        let rows = tab.row_count();
        // In follow mode the window sits on the last page whatever `top` says.
        if tab.engine.follow_tail {
            tab.top = view::follow_top(rows, page);
        }
        tab.pin_cursor();
        let cursor = tab.cursor;
        if tab.is_asm() {
            let step = match action {
                Action::LineUp | Action::SelectUp => Some(-1),
                Action::LineDown | Action::SelectDown => Some(1),
                Action::PageUp => Some(-((page * pages) as isize)),
                Action::PageDown => Some((page * pages) as isize),
                _ => None,
            };
            if let Some(n) = step {
                let select = matches!(action, Action::SelectUp | Action::SelectDown);
                if !select {
                    tab.engine.asm.selection = None;
                }
                tab.asm_move(n, select);
                return;
            }
            if action == Action::Top {
                tab.engine.follow_tail = false;
                tab.engine.asm_seek(0);
                tab.cursor = 0;
                return;
            }
        }
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
            Action::PageUp => tab.set_cursor(cursor.saturating_sub(page * pages)),
            Action::PageDown => tab.set_cursor(cursor + page * pages),
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
            Action::ScrollLeftFast => tab.hscroll = tab.hscroll.saturating_sub(HSCROLL_FAST),
            Action::ScrollRightFast => tab.hscroll += HSCROLL_FAST,
            Action::ScrollHome => tab.hscroll = 0,
            // The selection when there is one, else the cursor row (not in context: the
            // context view shows every line).
            Action::ToggleColumns => {
                if tab.engine.field_parser().is_some() {
                    let on = !tab.engine.fields_view();
                    tab.engine.set_fields_view(on);
                } else {
                    self.message =
                        Some(tx(self.lang, "No fields: the lines are plain text").into());
                }
            }
            Action::IncludeRotations => {
                // A stream read with its rotations: the list of its pieces.
                if tab.engine.rotation.is_some() {
                    self.piece_menu = Some(0);
                    return;
                }
                if tab.engine.rotation_offer.is_none() {
                    self.message = Some(tx(self.lang, "No rotations to include").into());
                    return;
                }
                let id = crate::rotation::identity(&tab.engine.path);
                self.close_stream();
                self.open_file(&id);
            }
            Action::OpenSource => {
                let Some(line) = tab.cursor_line() else {
                    return;
                };
                let text = tab.engine.get_line(line).unwrap_or_default();
                let locations = crate::source_links::find(&text);
                if locations.is_empty() {
                    self.message = Some(tx(self.lang, "No source location on this row").into());
                    return;
                }
                let next = match self.source_cycle {
                    Some((l, i)) if l == line => (i + 1) % locations.len(),
                    _ => 0,
                };
                self.source_cycle = Some((line, next));
                let loc = &locations[next];
                let (editor, roots) = self
                    .settings
                    .as_ref()
                    .map(|s| {
                        (
                            s.config.editor_command.clone(),
                            s.config.source_roots.clone(),
                        )
                    })
                    .unwrap_or_default();
                self.message = Some(match crate::source_links::resolve(loc, &roots) {
                    None => txf(
                        self.lang,
                        "{0} not found under the source roots",
                        &[&loc.path],
                    ),
                    Some(file) => {
                        match crate::source_links::open_in_editor(&editor, &file, loc.line, loc.col)
                        {
                            Ok(()) => txf(
                                self.lang,
                                "Opened {0}:{1} ({2} of {3})",
                                &[&loc.path, &loc.line, &(next + 1), &locations.len()],
                            ),
                            Err(e) => txf(self.lang, "The editor did not start: {0}", &[&e]),
                        }
                    }
                });
            }
            Action::HideLines if tab.engine.context_line().is_some() || tab.is_hex() => {}
            Action::HideLines => {
                // The selection, else the cursor row (its whole group when collapsed).
                let row = (!tab.engine.has_selection())
                    .then(|| tab.cursor_line())
                    .flatten();
                if row.is_none() && !tab.engine.has_selection() {
                    self.message = Some(tx(self.lang, "No row to hide").into());
                    return;
                }
                let hidden = tab.engine.hide_selection_or_row(row);
                // A background refilter shows no row yet: the cursor waits for it.
                let rows = tab.row_count();
                if rows > 0 {
                    tab.cursor = tab.cursor.min(rows - 1);
                }
                self.message = Some(txf(self.lang, "{0} lines hidden", &[&hidden]));
            }
            Action::ToggleBookmark => match tab.cursor_line() {
                Some(line) => tab.engine.toggle_bookmark(line),
                None => self.message = Some(tx(self.lang, "No row to bookmark").into()),
            },
            Action::NextOutOfOrder | Action::PrevOutOfOrder => {
                let forward = action == Action::NextOutOfOrder;
                match tab.engine.walk_out_of_order(forward) {
                    Some(line) => {
                        if let Some(row) = tab.engine.get_visible_row_of_line(line) {
                            tab.set_cursor(row);
                        }
                        if let Some(jump) = tab.engine.out_of_order_jump(line) {
                            self.message = Some(txf(
                                self.lang,
                                "Out of order: {0} back from the line above",
                                &[&crate::timestamp::format_jump(jump)],
                            ));
                        }
                    }
                    None => self.message = Some(tx(self.lang, "No line out of order").into()),
                }
            }
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
                                    tx(self.lang, "Bookmarks: back to the first")
                                } else {
                                    tx(self.lang, "Bookmarks: back to the last")
                                }
                                .into(),
                            );
                        }
                    }
                    None => self.message = Some(tx(self.lang, "No bookmark visible").into()),
                }
            }
            // With no search typed, `n` / `N` walk the kept searches.
            Action::SearchNext | Action::SearchPrev
                if tab.engine.search_query.trim().is_empty() && !tab.engine.named.is_empty() =>
            {
                let from = tab.cursor_line().unwrap_or(0);
                match tab.engine.next_named(from, action == Action::SearchNext) {
                    Some((_, i)) => {
                        self.message = Some(tab.engine.named.list[i].query.clone());
                    }
                    None => self.message = Some(tx(self.lang, "No search hits").into()),
                }
            }
            Action::SearchNext if tab.engine.search_next(false).is_none() => {
                self.message = Some(tx(self.lang, "No search hits").into());
            }
            Action::SearchNext => {}
            Action::SearchPrev if tab.engine.search_prev(false).is_none() => {
                self.message = Some(tx(self.lang, "No search hits").into());
            }
            Action::SearchPrev => {}
            Action::ToggleContext => toggle_context(tab, &mut self.message, self.lang),
            Action::ToggleHex => tab.toggle_hex(),
            Action::ToggleAsm => tab.toggle_asm(),
            Action::CycleArch if !tab.is_asm() => {
                self.message = Some(tx(self.lang, "Only in the ASM view: d opens it").into())
            }
            Action::CycleArch => {
                let next = tab.engine.asm.arch.next();
                tab.engine.set_disasm_arch(next);
                self.message = Some(match tab.engine.asm.known_executable() {
                    Some(_) => txf(
                        self.lang,
                        "Architecture {0} (the executable's {1} is used while it is open)",
                        &[&next.name(), &tab.engine.asm.label()],
                    ),
                    None => txf(self.lang, "Architecture: {0}", &[&next.name()]),
                });
            }
            Action::ToggleLineNumbers => {
                tab.engine.show_line_numbers = !tab.engine.show_line_numbers;
                self.message = Some(
                    if tab.engine.show_line_numbers {
                        tx(self.lang, "Line numbers on")
                    } else {
                        tx(self.lang, "Line numbers off")
                    }
                    .into(),
                );
            }
            Action::SearchResults => {
                if tab.engine.search_matches.is_empty() {
                    self.message = Some(tx(self.lang, "No search results: / searches").into());
                } else {
                    let at = tab.engine.current_match_idx.unwrap_or(0);
                    self.results_menu = Some(at.min(tab.engine.search_matches.len() - 1));
                }
            }
            Action::SearchScope => self.open_prompt(PromptKind::SearchScope),
            Action::LogFormats => self.open_formats(),
            Action::Scratchpad => self.open_pad(),
            Action::SendToScratchpad => self.send_to_pad(),
            Action::Analysis => self.analysis_menu = Some(0),
            Action::MergedView => self.open_merge(),
            Action::FilterTab => self.open_filter_tab(),
            Action::SelectAll => tab.engine.select_all_visible(),
            Action::StreamInfo => self.info_view = Some(tab.engine.path.clone()),
            Action::OperationNext | Action::OperationPrev => {
                use crate::operation_jump::OpJump;
                let Some(from) = tab.cursor_line() else {
                    return;
                };
                self.message = Some(
                    match tab.engine.next_same_operation(from, action == Action::OperationNext) {
                        OpJump::Found(_, id) => id,
                        OpJump::NoId => tx(
                            self.lang,
                            "This row has no operation id (trace_id, request_id, req_id, opid, correlation_id, thread)",
                        )
                        .into(),
                        OpJump::NotFound(id) => {
                            txf(self.lang, "No other line of {0} that way", &[&id])
                        }
                    },
                );
            }
            Action::KeepSearch => {
                let query = tab.engine.search_query.trim().to_string();
                if query.is_empty() {
                    let at = tab
                        .engine
                        .named
                        .focus
                        .or_else(|| tab.engine.named.list.len().checked_sub(1));
                    match at {
                        Some(i) => {
                            let q = tab.engine.named.list[i].query.clone();
                            tab.engine.remove_named(i);
                            self.message =
                                Some(txf(self.lang, "Forgot the kept search {0}", &[&q]));
                        }
                        None => {
                            self.message = Some(
                                tx(self.lang, "Type a search (/) first, then Alt+K keeps it")
                                    .into(),
                            )
                        }
                    }
                } else {
                    use crate::named_search::KeepError;
                    self.message = Some(match tab.engine.keep_search(&query) {
                        Ok(_) => {
                            tab.pending_first_hit = false;
                            tab.engine.search_query.clear();
                            tab.engine.update_search("");
                            txf(self.lang, "Kept {0}: n walks the kept searches", &[&query])
                        }
                        Err(KeepError::Full) => tx(
                            self.lang,
                            "Eight searches are kept already: Alt+K with no search forgets one",
                        )
                        .into(),
                        Err(_) => txf(self.lang, "{0} is kept already", &[&query]),
                    });
                }
            }
            Action::VolumeRow => {
                tab.engine.timeline_open = !tab.engine.timeline_open;
                tab.engine.timeline_dirty = true;
                if tab.engine.timeline_open {
                    tab.engine.request_timeline();
                }
            }
            Action::RemoveBookmark => match tab.cursor_line() {
                Some(line) if tab.engine.is_bookmarked(line) => tab.engine.remove_bookmark(line),
                _ => self.message = Some(tx(self.lang, "No bookmark on this row").into()),
            },
            Action::ToggleMonitor => self.run_chip(crate::tui::bars::BarChip::Monitor),
            Action::TimeAnchor => {
                let Some(line) = tab.cursor_line() else {
                    return;
                };
                tab.engine.toggle_time_anchor(line);
                // The anchor is read in the Δt column: it is shown with it.
                if tab.engine.time_anchor().is_some() {
                    tab.engine.show_time_delta = true;
                    self.message = Some(txf(self.lang, "Time anchor on line {0}", &[&(line + 1)]));
                } else {
                    self.message = Some(tx(self.lang, "Time anchor off").into());
                }
            }
            Action::TimeAnchorClear => {
                tab.engine.clear_time_anchor();
                self.message = Some(tx(self.lang, "Time anchor off").into());
            }
            Action::KeysDialog => self.keys_dialog = Some(KeysDialog::default()),
            Action::ToggleSetting(s) => self.toggle_setting(s),
            Action::Plugins => self.open_plugins(),
            Action::Patterns => self.open_analysis(crate::tui::analysis::Kind::Patterns),
            Action::Compare => self.compare_selection(),
            Action::ToggleFilterExpr => {
                let on = !tab.engine.filter_expr_mode();
                tab.engine.set_filter_expr_mode(on);
                tab.top = 0;
                self.message = Some(if on {
                    txf(
                        self.lang,
                        "Include as an expression: {0} (i edits it)",
                        &[&tab.engine.filter_expr()],
                    )
                } else {
                    tx(self.lang, "Include as terms").into()
                });
            }
            Action::RuleNext | Action::RulePrev => {
                let Some(from) = tab.cursor_line() else {
                    return;
                };
                let forward = action == Action::RuleNext;
                match tab.engine.start_rule_seek(None, forward, from) {
                    Ok(()) => self.step_rule_seeks(),
                    Err(crate::tail_engine::RuleNavError::NoRuleOnRow) => {
                        self.message =
                            Some(tx(self.lang, "No highlight rule matches this row").into())
                    }
                    Err(crate::tail_engine::RuleNavError::Empty) => {}
                }
            }
            Action::ClearBookmarks => {
                let n = tab.engine.bookmarks.len();
                tab.engine.clear_bookmarks();
                self.message = Some(txf(self.lang, "{0} bookmarks removed", &[&n]));
            }
            Action::OpenSourceHere => self.open_source_here(),
            Action::Export => self.export_menu = Some(0),
            Action::CopyAsShown => self.copy_as_shown(),
            Action::CopyExcerpt => self.copy_excerpt(),
            Action::BookmarkReport => self.open_prompt(PromptKind::Report(false)),
            Action::BookmarkReportAll => self.open_prompt(PromptKind::Report(true)),
            Action::ToggleTimeDelta => {
                let on = !tab.engine.show_time_delta;
                tab.engine.set_show_time_delta(on);
                self.message = Some(
                    if on {
                        tx(self.lang, "Time delta on")
                    } else {
                        tx(self.lang, "Time delta off")
                    }
                    .into(),
                );
            }
            Action::SetTimeDisplay(display) => {
                tab.engine.set_time_display(display);
                self.message = Some(txf(
                    self.lang,
                    "Time shown: {0}",
                    &[&crate::tui::bars::time_display_name(display, self.lang)],
                ));
            }
            Action::CycleAnsi => {
                let next = match tab.engine.ansi_mode {
                    AnsiMode::Auto => AnsiMode::Render,
                    AnsiMode::Render => AnsiMode::Strip,
                    AnsiMode::Strip => AnsiMode::Raw,
                    AnsiMode::Raw => AnsiMode::Auto,
                };
                tab.engine.set_ansi_mode(next);
                self.message = Some(match next {
                    AnsiMode::Auto => txf(
                        self.lang,
                        "ANSI: auto (now {0})",
                        &[&tab.engine.ansi_effective().name()],
                    ),
                    mode => txf(self.lang, "ANSI: {0}", &[&mode.name()]),
                });
            }
            // In the context view, Esc returns to the filtered rows first.
            Action::ClearSearch if tab.engine.context_line().is_some() => {
                toggle_context(tab, &mut self.message, self.lang)
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
                self.message = Some(txf(self.lang, "Collapse: {0}", &[&next.name()]));
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
                    LogLevel::Unknown => tx(self.lang, "Level filter: off").into(),
                    l => txf(self.lang, "Level filter: {0} and above", &[&l.name()]),
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
        self.hits.screen = frame.area();
        // Locked: the PIN dialog on a blank screen and nothing else.
        if self.locked.is_some() {
            self.draw_lock(frame);
            return;
        }
        // The theme's background behind everything (the terminal's own at 16 colours).
        frame.render_widget(Block::default().style(self.palette.screen()), frame.area());
        let strip = if self.tabs.len() > 1 { 1 } else { 0 };
        let [top_area, tabs_area, main_area, status_area] = Layout::vertical([
            Constraint::Length(top_bar_rows(frame.area().height)),
            Constraint::Length(strip),
            Constraint::Min(3),
            Constraint::Length(3),
        ])
        .areas(frame.area());
        self.draw_top_bar(frame, top_area);
        if strip > 0 {
            self.draw_tabs(frame, tabs_area);
        }
        self.hits.main = main_area;
        self.draw_dock(frame, main_area);
        self.draw_floats(frame, main_area);
        self.draw_closing(frame);
        self.draw_status(frame, status_area);
        if self.show_help {
            // The whole screen: the keys need the room more than the windows do.
            self.draw_help(frame, frame.area());
        }
        if self.prompt.as_ref().is_some_and(|p| !p.kind.over_dialogs()) {
            self.draw_prompt(frame, main_area);
        }
        if self.time_range.is_some() {
            self.draw_time_range(frame, main_area);
        }
        if self.browser.is_some() {
            self.draw_browser(frame, main_area);
        }
        if self.picker.is_some() {
            self.draw_picker(frame, main_area);
        }
        if self.sessions.is_some() {
            self.draw_sessions(frame, main_area);
        }
        if self.settings_form.is_some() {
            self.draw_settings(frame, main_area);
        }
        if self.rules.is_some() {
            self.draw_rules(frame, main_area);
        }
        if self.formats.is_some() {
            self.draw_formats(frame, main_area);
        }
        if self.presets.is_some() {
            self.draw_presets(frame, main_area);
        }
        if self.global.is_some() {
            self.draw_global(frame, main_area);
        }
        if self.tool_menu.is_some() {
            self.draw_tool_menu(frame, main_area);
        }
        if self.columns_menu.is_some() {
            self.draw_columns_menu(frame, main_area);
        }
        if self.hidden_menu.is_some() {
            self.draw_hidden_menu(frame, main_area);
        }
        if self.piece_menu.is_some() {
            self.draw_piece_menu(frame, main_area);
        }
        if self.results_menu.is_some() {
            self.draw_results_menu(frame, main_area);
        }
        if self.export_menu.is_some() {
            self.draw_export_menu(frame, main_area);
        }
        if self.pad_editor.is_some() {
            self.draw_pad(frame, main_area);
        }
        if self.analysis.is_some() {
            self.draw_analysis(frame, main_area);
        }
        if self.analysis_menu.is_some() {
            self.draw_analysis_menu(frame, main_area);
        }
        if self.merge.is_some() {
            self.draw_merge(frame, main_area);
        }
        if self.recent_menu.is_some() {
            self.draw_recent(frame, main_area);
        }
        if self.info_view.is_some() {
            self.draw_info(frame, main_area);
        }
        if self.keys_dialog.is_some() {
            self.draw_keys_dialog(frame, main_area);
        }
        if self.plugins.is_some() {
            self.draw_plugins(frame, main_area);
        }
        if self.compare.is_some() {
            self.draw_compare(frame, main_area);
        }
        if self.prompt.as_ref().is_some_and(|p| p.kind.over_dialogs()) {
            self.draw_prompt(frame, main_area);
        }
        if self.palette_dialog.is_some() {
            self.draw_palette(frame, main_area);
        }
        if self.json_dialog.is_some() {
            self.draw_json(frame, main_area);
        }
        if self.find_window.as_ref().is_some_and(|w| w.shown) {
            self.draw_find(frame, main_area);
        }
        if self.show_about {
            self.draw_about(frame, main_area);
        }
        if self.tools_editor.is_some() {
            self.draw_tools_editor(frame, main_area);
        }
        if self.confirm_overwrite.is_some() {
            self.draw_confirm(frame, main_area);
        }
        if self.interface_switch.is_some() {
            self.draw_interface_switch(frame, main_area);
        }
    }

    /// The top bar, as the GUI's title bar and toolbar. First row: the name, version and
    /// session on the left; the CPU and memory meters (`telemetry_enabled`), the global
    /// filter and the command palette on the right. Second row (when the terminal has
    /// room): Open, Sessions, the highlight rules with their count, Play, Pause and
    /// Settings, and Help on the right. Every button is clickable and names its key.
    fn draw_top_bar(&mut self, frame: &mut Frame, area: Rect) {
        if area.height == 0 {
            return;
        }
        let accent = Style::default()
            .fg(self.palette.accent())
            .add_modifier(Modifier::BOLD);
        let dim = Style::default().fg(self.palette.dim());
        let ascii = self.palette.ascii;
        let config = self.settings.as_ref().map(|s| &s.config);
        let mut title = txf(
            self.lang,
            " FastTail v{0} by {1}",
            &[&env!("CARGO_PKG_VERSION"), &AUTHOR],
        );
        if let Some(file) = config.and_then(|c| c.current_session.as_deref()) {
            let dot = if ascii { "-" } else { "·" };
            title.push_str(&format!(
                " {dot} {}",
                crate::session::Session::name_of(file)
            ));
        }
        let applied = config.is_some_and(|c| c.global_filter.is_applied());
        let rules = config.map_or(0, |c| {
            c.highlight_rules
                .iter()
                .filter(|r| r.enabled && !r.pattern.is_empty())
                .count()
        });
        let telemetry = config
            .is_some_and(|c| c.telemetry_enabled)
            .then(|| self.telemetry.text(ascii))
            .flatten();

        let row = Rect::new(area.x, area.y, area.width, 1);
        let mut right: Vec<TopItem> = Vec::new();
        if let Some(text) = telemetry {
            right.push(TopItem::Text(text, dim));
        }
        let global = if applied {
            tx(self.lang, "Global on")
        } else {
            tx(self.lang, "Global off")
        };
        right.push(TopItem::Button(
            "F",
            global.into(),
            Action::EditGlobal,
            applied,
        ));
        right.push(TopItem::Button(
            ":",
            tx(self.lang, "Palette").into(),
            Action::Palette,
            false,
        ));
        let left = vec![TopItem::Text(title, accent)];
        self.draw_top_row(frame, row, &left, &right);

        if area.height < 2 {
            return;
        }
        let (play, pause) = if ascii {
            (tx(self.lang, "> Play"), tx(self.lang, "|| Pause"))
        } else {
            (tx(self.lang, "▶ Play"), tx(self.lang, "⏸ Pause"))
        };
        let left = vec![
            TopItem::Button("o", tx(self.lang, "Open").into(), Action::OpenFile, false),
            TopItem::Button(
                "O",
                tx(self.lang, "Sessions").into(),
                Action::OpenSession,
                false,
            ),
            TopItem::Button(
                "r",
                txf(self.lang, "Rules ({0})", &[&rules]),
                Action::EditRules,
                false,
            ),
            TopItem::Button("", play.into(), Action::PlayAll, false),
            TopItem::Button("", pause.into(), Action::PauseAll, false),
            TopItem::Button(
                ",",
                tx(self.lang, "Settings").into(),
                Action::Settings,
                false,
            ),
        ];
        let about = if ascii {
            tx(self.lang, "About")
        } else {
            tx(self.lang, "ℹ About")
        };
        let right = vec![
            TopItem::Button("", about.into(), Action::About, false),
            TopItem::Button("?", tx(self.lang, "Help").into(), Action::ToggleHelp, false),
        ];
        let row = Rect::new(area.x, area.y + 1, area.width, 1);
        self.draw_top_row(frame, row, &left, &right);
    }

    /// One row of the top bar: `left` from the left edge, `right` against the right one,
    /// as many items as fit (the right ones first, they hold the state).
    fn draw_top_row(&mut self, frame: &mut Frame, row: Rect, left: &[TopItem], right: &[TopItem]) {
        let bracket = Style::default().fg(self.palette.dim());
        let key = Style::default()
            .fg(self.palette.accent())
            .add_modifier(Modifier::BOLD);
        let on = Style::default()
            .fg(self.palette.accent())
            .add_modifier(Modifier::BOLD | Modifier::REVERSED);
        // The meters go first when the left side would have less than 24 cells.
        let mut right = right;
        let width = |items: &[TopItem]| -> u16 { items.iter().map(|i| i.width() + 1).sum() };
        while right.len() > 1
            && matches!(right[0], TopItem::Text(..))
            && width(right) + 24 > row.width
        {
            right = &right[1..];
        }
        let right_x = row.right().saturating_sub(width(right)).max(row.x);
        let buf = frame.buffer_mut();
        let mut place = |items: &[TopItem], mut x: u16, end: u16, hits: &mut HitMap| {
            for item in items {
                // Text is cut to the room left; a button is drawn whole or not at all.
                let w = match item {
                    TopItem::Text(..) => item.width().min(end.saturating_sub(x)),
                    TopItem::Button(..) => item.width(),
                };
                if w == 0 || x + w > end {
                    break;
                }
                match item {
                    TopItem::Text(text, style) => {
                        buf.set_stringn(x, row.y, text, w as usize, *style);
                    }
                    TopItem::Button(k, label, action, active) => {
                        let mut cx = x;
                        buf.set_string(cx, row.y, "[", bracket);
                        cx += 1;
                        if !k.is_empty() {
                            buf.set_string(cx, row.y, *k, key);
                            cx += k.chars().count() as u16 + 1;
                        }
                        let style = if *active { on } else { Style::default() };
                        buf.set_string(cx, row.y, label, style);
                        cx += unicode_width::UnicodeWidthStr::width(label.as_str()) as u16;
                        buf.set_string(cx, row.y, "]", bracket);
                        hits.top_buttons.push((Rect::new(x, row.y, w, 1), *action));
                    }
                }
                x += w + 1;
            }
        };
        place(right, right_x + 1, row.right(), &mut self.hits);
        place(left, row.x, right_x, &mut self.hits);
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

    /// The dock's windows, and while a window is dragged the part of the window under
    /// the pointer it would fill.
    fn draw_dock(&mut self, frame: &mut Frame, area: Rect) {
        let (leaves, dividers) = dock::layout(&self.dock, area);
        if self.dock.streams().is_empty() {
            // An empty dock: the background, a hint and one place to drop a window.
            let hint = if self.tabs.is_empty() {
                tx(self.lang, NO_FILE)
            } else {
                tx(self.lang, "Alt+F on a floating window docks it here")
            };
            // On the bottom row, where a new floating window does not cover it.
            let y = area.bottom().saturating_sub(1).max(area.y);
            frame.render_widget(
                Paragraph::new(Line::styled(hint, Style::default().fg(self.palette.dim())))
                    .centered(),
                Rect::new(area.x, y, area.width, 1.min(area.height)),
            );
            self.hits.leaves = leaves;
            return;
        }
        for leaf in &leaves {
            let Some(Pane::Leaf { tabs, active }) = self.dock.get(&leaf.path).cloned() else {
                continue;
            };
            let place = Place::Dock(leaf.path.clone());
            match self.shown_at(&place) {
                Some(idx) => self.draw_stream(frame, leaf.area, idx, &place, &tabs),
                None => self.draw_panel(frame, leaf.area, &place, &tabs, active),
            }
        }
        self.hits.leaves = leaves;
        self.hits.dividers = dividers;
    }

    /// The floating windows over the dock, bottom to top, each with a shadow and a grip
    /// in its bottom-right corner; then, while a window is dragged, where it would go.
    fn draw_floats(&mut self, frame: &mut Frame, area: Rect) {
        for f in 0..self.floats.len() {
            let r = dock::clamp_into(self.floats[f].rect, area);
            let place = Place::Float(f);
            let Pane::Leaf { tabs, active } = self.floats[f].pane.clone() else {
                continue;
            };
            frame.render_widget(Clear, r);
            match self.shown_at(&place) {
                Some(idx) => self.draw_stream(frame, r, idx, &place, &tabs),
                None => self.draw_panel(frame, r, &place, &tabs, active),
            }
            cast_shadow(frame.buffer_mut(), r, self.palette.shadow());
            let grip = if self.palette.ascii { "#" } else { "\u{25e2}" };
            frame.render_widget(
                Paragraph::new(Span::styled(
                    grip,
                    Style::default().fg(self.palette.accent()),
                )),
                Rect::new(
                    r.right().saturating_sub(1),
                    r.bottom().saturating_sub(1),
                    1,
                    1,
                ),
            );
            self.hits.floats.push((r, f));
        }
        if let Some(DockDrag::Move {
            drop: Some((_, zone, preview)),
            ..
        }) = &self.dock_drag
        {
            let title = if *zone == Zone::Float {
                tx(self.lang, " float here ")
            } else {
                tx(self.lang, " drop here ")
            };
            let block = Block::bordered()
                .border_set(self.palette.border_set(Chrome::Focused))
                .border_style(
                    Style::default()
                        .fg(self.palette.accent())
                        .add_modifier(Modifier::BOLD),
                )
                .title_top(Line::from(title).centered());
            frame.render_widget(block, *preview);
        }
    }

    /// The top-border title of a window: `[#N] name` alone, or every tab of its leaf
    /// with the shown one marked, each clickable (and draggable).
    fn leaf_title(
        &mut self,
        area: Rect,
        place: &Place,
        tabs: &[DockTab],
        shown: usize,
        style: Style,
    ) -> Vec<Span<'static>> {
        let name = |me: &Self, k: usize| match tabs[k].stream().and_then(|p| me.tab_index(p)) {
            Some(i) if k == shown => format!(" [#{}] {} ", i + 1, me.tabs[i].title),
            Some(i) => format!(" {}:{} ", i + 1, me.tabs[i].title),
            None => format!(" {} ", panel_name(&tabs[k], self.lang)),
        };
        if tabs.len() < 2 {
            return vec![Span::styled(name(self, shown), style)];
        }
        let dim = Style::default().fg(self.palette.dim());
        let mut spans = Vec::with_capacity(tabs.len() * 2);
        let right = area.right().saturating_sub(1);
        let mut x = area.x + 1;
        for k in 0..tabs.len() {
            let span = Span::styled(name(self, k), if k == shown { style } else { dim });
            let w = (span.width() as u16).min(right.saturating_sub(x));
            if w > 0 {
                self.hits
                    .leaf_tabs
                    .push((Rect::new(x, area.y, w, 1), place.clone(), k));
            }
            x = x.saturating_add(span.width() as u16 + 1);
            spans.push(span);
            spans.push(Span::styled("|", dim));
        }
        spans.pop();
        spans
    }

    /// A leaf showing one of the GUI's panels (Filters, Settings...): its tabs, and a
    /// line saying the panel lives in the GUI.
    fn draw_panel(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        place: &Place,
        tabs: &[DockTab],
        shown: usize,
    ) {
        let style = Style::default().fg(self.palette.dim());
        let title = self.leaf_title(area, place, tabs, shown, style);
        let block = Block::bordered()
            .style(self.palette.window())
            .border_set(self.palette.border_set(Chrome::Plain))
            .border_style(self.palette.border_style(Chrome::Plain))
            .title_top(Line::from(title));
        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(
            Paragraph::new(Line::styled(
                tx(
                    self.lang,
                    " A panel of the GUI. Ctrl+PgUp/PgDn: the other tabs of this window",
                ),
                style,
            )),
            inner,
        );
    }

    /// One stream window: title and follow state in the top border, counts and progress
    /// bottom left, filters and search bottom right, the rows inside.
    fn draw_stream(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        idx: usize,
        place: &Place,
        tabs: &[DockTab],
    ) {
        let palette = self.palette;
        let focused = idx == self.active;
        let chrome = if focused {
            Chrome::Focused
        } else {
            Chrome::Plain
        };
        let title_style = if focused {
            Style::default()
                .fg(palette.accent())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(palette.dim())
        };
        let shown = match self.leaf(place) {
            Some(Pane::Leaf { active, .. }) => *active,
            _ => 0,
        };
        let mut title = self.leaf_title(area, place, tabs, shown, title_style);
        // The `[x]` sits right-aligned in the top border, before the corner.
        if area.width >= 8 {
            self.hits.close_buttons.push((
                Rect::new(area.right().saturating_sub(4), area.y, 3, 1),
                place.clone(),
            ));
        }
        let gap_ms = self
            .settings
            .as_ref()
            .map_or(crate::config::DEFAULT_TIME_DELTA_GAP_MS, |s| {
                s.config.time_delta_gap_ms
            });
        let tab = &mut self.tabs[idx];
        let e = &tab.engine;
        let follow = if e.follow_tail {
            Span::styled(
                tx(self.lang, " FOLLOW "),
                Style::default().fg(Color::Black).bg(palette.accent()),
            )
        } else {
            Span::styled(
                tx(self.lang, " PAUSED "),
                Style::default().fg(palette.dim()),
            )
        };
        title.push(follow);
        title.push(Span::raw(" "));
        let close = Span::styled(
            "[x]",
            Style::default()
                .fg(palette.accent())
                .add_modifier(Modifier::BOLD),
        );
        let right = view_state_text(e, tab.is_hex(), self.lang);
        let (left, ln) = bottom_left_text(
            tab,
            self.lang,
            (area.width as usize).saturating_sub(width(&right) + 6),
        );
        if let Some((at, w)) = ln {
            self.hits.goto_links.push((
                Rect::new(area.x + 2 + at as u16, area.bottom() - 1, w as u16, 1),
                idx,
            ));
        }
        let block = Block::bordered()
            .style(palette.window())
            .border_set(palette.border_set(chrome))
            .border_style(palette.border_style(chrome))
            .title_top(Line::from(title))
            .title_top(Line::from(close).right_aligned())
            .title_bottom(Line::from(format!(" {left} ")).style(title_style))
            .title_bottom(
                Line::from(format!(" {right} "))
                    .style(title_style)
                    .right_aligned(),
            );
        let mut inner = block.inner(area);
        frame.render_widget(block, area);
        // The GUI's stream bar and filter row (the filter row not in HEX), when the
        // window has room for them and a few rows of the log.
        let bars = crate::tui::bars::rows_for(inner.height, tab.is_hex());
        if bars > 0 {
            let global = self
                .settings
                .as_ref()
                .is_some_and(|s| s.config.global_filter.is_applied());
            let lines = crate::tui::bars::lines(tab, &palette, global, self.lang);
            for (n, chips) in lines.into_iter().take(bars).enumerate() {
                let row = Rect::new(inner.x, inner.y + n as u16, inner.width, 1);
                for (rect, chip) in
                    crate::tui::bars::draw(frame.buffer_mut(), row, &chips, &palette)
                {
                    self.hits.bar_chips.push((rect, idx, chip));
                }
            }
            inner.y += bars as u16;
            inner.height -= bars as u16;
        }
        let tab = &mut self.tabs[idx];
        if tab.engine.context_line().is_some() && inner.height > 1 {
            // The context view: every line, the filters suspended until Ctrl+K or Esc.
            let banner = Rect { height: 1, ..inner };
            frame.render_widget(
                Paragraph::new(Line::styled(
                    tx(
                        self.lang,
                        " In context: filters suspended. Ctrl+K or Esc returns",
                    ),
                    Style::default().fg(Color::Black).bg(palette.accent()),
                )),
                banner,
            );
            inner.y += 1;
            inner.height -= 1;
        }
        // The volume row: lines per time slice above the rows (the window's timeline strip).
        let mut volume_hits = Vec::new();
        if tab.engine.timeline_open && !tab.is_hex() && !tab.is_asm() && inner.height > 4 {
            let row = Rect::new(inner.x, inner.y, inner.width, 1);
            let (line, cells) = volume_row(&tab.engine, &palette, row.width as usize, self.lang);
            frame.render_widget(Paragraph::new(line), row);
            volume_hits = cells
                .into_iter()
                .map(|(x, ms)| (Rect::new(row.x + x as u16, row.y, 1, 1), idx, ms))
                .collect();
            inner.y += 1;
            inner.height -= 1;
        }
        // The first plotted field as a sparkline on the last row, under the log rows.
        if !tab.is_hex() && !tab.is_asm() && !tab.engine.plots().is_empty() && inner.height > 3 {
            let row = Rect::new(inner.x, inner.bottom() - 1, inner.width, 1);
            let line = sparkline_row(&tab.engine, &palette, row.width as usize, self.lang);
            frame.render_widget(Paragraph::new(line), row);
            inner.height -= 1;
        }
        tab.cells = inner.width as usize;
        let (lines, row_count) = if tab.is_asm() {
            asm_rows(
                tab,
                &palette,
                inner.width as usize,
                inner.height as usize,
                self.lang,
            )
        } else if tab.is_hex() {
            hex_rows(
                tab,
                &palette,
                inner.width as usize,
                inner.height as usize,
                self.lang,
            )
        } else if tab.engine.columns_shown() && inner.height > 1 {
            // The column view: a header row of the field names over the rows.
            let (header, lines, drawn) =
                column_rows(tab, &palette, inner.height as usize - 1, gap_ms, self.lang);
            frame.render_widget(Paragraph::new(header), Rect { height: 1, ..inner });
            inner.y += 1;
            inner.height -= 1;
            (lines, drawn)
        } else {
            stream_rows(tab, &palette, inner.height as usize, gap_ms, self.lang)
        };
        self.hits.volume_cells.extend(volume_hits);
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
    fn draw_status(&mut self, frame: &mut Frame, area: Rect) {
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
        // The web view and the MCP server, while they listen.
        let mut serving = Vec::new();
        if let (Some(c), true) = (
            self.settings.as_ref().map(|s| &s.config),
            self.web_view.connections().is_some(),
        ) {
            serving.push(format!("web :{}", c.web_port));
        }
        if let Some(port) = self.mcp_view.port() {
            serving.push(format!("MCP :{port}"));
        }
        // The elapsed time of a selection of timed rows, as the window's stream bar shows it.
        let elapsed = self
            .tabs
            .get(self.active)
            .and_then(|t| t.engine.selection_elapsed());
        let block = match elapsed {
            Some((millis, rows)) => block.title_bottom(
                Line::from(txf(
                    self.lang,
                    " Δ {0} · {1} rows ",
                    &[&crate::timestamp::format_delta(millis), &rows],
                ))
                .style(Style::default().fg(self.palette.accent())),
            ),
            None => block,
        };
        let block = if serving.is_empty() {
            block
        } else {
            block.title_bottom(Line::from(format!(" {} ", serving.join(" · "))).right_aligned())
        };
        let note = self.tabs.get(self.active).and_then(|tab| {
            tab.cursor_line()
                .and_then(|l| tab.engine.bookmark_note(l))
                .map(|n| n.chars().take(NOTE_PREVIEW_CHARS).collect::<String>())
        });
        let inner = block.inner(area);
        let line = match (&self.message, note) {
            (Some(m), _) => Line::styled(m.clone(), Style::default().fg(self.palette.accent())),
            // The note of the cursor row's bookmark, when there is one.
            (None, Some(n)) => {
                Line::styled(format!("* {n}"), Style::default().fg(self.palette.accent()))
            }
            // The main commands as clickable `[ ]` buttons, as many as fit.
            (None, None) => {
                let bracket = Style::default().fg(self.palette.dim());
                let key = Style::default()
                    .fg(self.palette.accent())
                    .add_modifier(Modifier::BOLD);
                let mut spans = Vec::with_capacity(STATUS.len() * 4);
                let mut x = inner.x;
                for (i, (k, label, _)) in STATUS.iter().enumerate() {
                    let k = tx(self.lang, k);
                    let w = (k.chars().count() + tx(self.lang, label).chars().count() + 3) as u16;
                    if x + w > inner.right() {
                        break;
                    }
                    self.hits.buttons.push((Rect::new(x, inner.y, w, 1), i));
                    spans.push(Span::styled("[", bracket));
                    spans.push(Span::styled(k, key));
                    spans.push(Span::raw(format!(" {}", tx(self.lang, label))));
                    spans.push(Span::styled("]", bracket));
                    spans.push(Span::raw(" "));
                    x += w + 1;
                }
                Line::from(spans)
            }
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
        self.dialog_at(frame, rect, title, cancel, None)
    }

    /// Settings or Keys: where the user moved it (kept inside `area`), else centred at
    /// `size`; `[x]` in the top border, and the title and borders take a drag.
    fn movable_dialog(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        which: MovableDialog,
        size: (u16, u16),
        title: &str,
        cancel: bool,
    ) -> Rect {
        let rect = match *self.dialog_rect(which) {
            Some(r) => fit_dialog(r, which, area),
            None => {
                let (x, y, w, h) = view::centered(area.width, area.height, size.0, size.1);
                Rect::new(area.x + x, area.y + y, w, h)
            }
        };
        if self.dialog_rect(which).is_some() {
            *self.dialog_rect(which) = Some(rect);
        }
        self.dialog_at(frame, rect, title, cancel, Some(which))
    }

    /// Where `which` was moved to (`None`: centred at its default size).
    fn dialog_rect(&mut self, which: MovableDialog) -> &mut Option<Rect> {
        match which {
            MovableDialog::Settings => &mut self.settings_rect,
            MovableDialog::Help => &mut self.help_rect,
        }
    }

    fn dialog_at(
        &mut self,
        frame: &mut Frame,
        rect: Rect,
        title: &str,
        cancel: bool,
        movable: Option<MovableDialog>,
    ) -> Rect {
        let mut block = Block::bordered()
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
        // The `[x]`, as the floating windows draw it: right-aligned, before the corner.
        let close = (movable.is_some() && rect.width >= 8).then(|| {
            block = block.clone().title_top(
                Line::from(Span::styled(
                    "[x]",
                    Style::default()
                        .fg(self.palette.accent())
                        .add_modifier(Modifier::BOLD),
                ))
                .right_aligned(),
            );
            Rect::new(rect.right().saturating_sub(4), rect.y, 3, 1)
        });
        let inner = block.inner(rect);
        frame.render_widget(Clear, rect);
        frame.render_widget(block, rect);
        cast_shadow(frame.buffer_mut(), rect, self.palette.shadow());
        // Buttons, right-aligned on the last inner row.
        let labels: &[&str] = if cancel {
            &[tx(self.lang, "[ OK ]"), tx(self.lang, "[ Cancel ]")]
        } else {
            &[tx(self.lang, "[ OK ]")]
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
            close,
            movable,
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
            PromptKind::Search => tx(self.lang, "Search (re: regex, mre: multi-line regex)"),
            PromptKind::Include => tx(self.lang, "Include filter"),
            PromptKind::Exclude => tx(self.lang, "Exclude filter"),
            PromptKind::Note(_) => tx(self.lang, "Bookmark note"),
            PromptKind::Goto if self.tabs[self.active].is_hex() => {
                tx(self.lang, "Go to byte offset (decimal or 0x hex)")
            }
            PromptKind::Goto => tx(self.lang, "Go to line (N, +N, -N) or time (14:02)"),
            PromptKind::SaveSession => tx(self.lang, "Save session as"),
            PromptKind::LevelBytes => tx(
                self.lang,
                "Level searched in the first N bytes (16 to 4096, empty: the setting)",
            ),
            PromptKind::Split => tx(
                self.lang,
                "Split every N lines, 10mb, 1h, 1d or 30m (pieces next to the file)",
            ),
            PromptKind::System => tx(
                self.lang,
                "Open docker://<container>, k8s://<namespace>/<pod>, journald://<unit> or journald://user/<unit>",
            ),
            PromptKind::Remote => tx(
                self.lang,
                "Open ssh://[user@]host[:port]/path (#whole: the whole file)",
            ),
            PromptKind::Listen => tx(
                self.lang,
                "Listen on syslog://, tcp://, udp:// or otlp://ADDRESS:PORT (127.0.0.1: this machine only)",
            ),
            PromptKind::DebugOutput => tx(
                self.lang,
                "Capture debug output: dbwin://local or dbwin://global (?include=app.exe&exclude=...)",
            ),
            PromptKind::EventLog => tx(
                self.lang,
                "Follow an Event Log channel: eventlog://Application (?levels=1,2&ids=7031,7034-7040)",
            ),
            PromptKind::SearchScope => tx(
                self.lang,
                "Search in lines 1200-5000 or times 14:00..15:30 (empty: everywhere)",
            ),
            PromptKind::Export(..) => tx(
                self.lang,
                "Export to this file (a trailing ! replaces it)",
            ),
            PromptKind::Report(_) => tx(
                self.lang,
                "Save the bookmark report to (empty: copy it to the clipboard)",
            ),
            PromptKind::RulesImport => tx(
                self.lang,
                "Import a rule set file (a trailing ! replaces every rule)",
            ),
            PromptKind::RulesExport => tx(
                self.lang,
                "Export the rules to this file (a trailing ! replaces it)",
            ),
            PromptKind::FormatImport => tx(self.lang, "Import a format file"),
            PromptKind::StatsField => tx(
                self.lang,
                "Statistics of the field (or /regex/ for its first group)",
            ),
            PromptKind::OpsField => tx(
                self.lang,
                "Operations grouped by the field (or /regex/ for its first group)",
            ),
            PromptKind::ExplainSpan => tx(
                self.lang,
                "Explain the span 14:00..14:05 against the rest of the log",
            ),
            PromptKind::OpenPlugin => tx(
                self.lang,
                "Open the plugin source: plugin://name?key=value&other=value",
            ),
            PromptKind::FormatExport => tx(
                self.lang,
                "Export the format to this file (a trailing ! replaces it)",
            ),
        };
        let inner = self.dialog(frame, area, (64, 5), title, true);
        let Some(p) = &self.prompt else {
            return;
        };
        let hint = Line::styled(
            tx(self.lang, "Enter confirm   Esc cancel   Ctrl+U clear"),
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

    /// The lock screen: a blank screen and a bordered PIN dialog, without any line,
    /// name, count or status of the streams; during the cooldown, a countdown.
    fn draw_lock(&mut self, frame: &mut Frame) {
        let area = frame.area();
        frame.render_widget(Clear, area);
        frame.render_widget(Block::default().style(self.palette.screen()), area);
        let palette = self.palette;
        let block = Block::bordered()
            .style(palette.dialog())
            .border_set(palette.border_set(Chrome::Dialog))
            .border_style(palette.border_style(Chrome::Dialog))
            .title_top(
                Line::from(tx(self.lang, " FastTail is locked ")).style(
                    Style::default()
                        .fg(palette.accent())
                        .add_modifier(Modifier::BOLD),
                ),
            );
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let Some(screen) = self.locked.as_ref() else {
            return;
        };
        let wait = self.lock_attempts.cooldown_left(Instant::now());
        let mut lines = vec![Line::raw("")];
        match wait {
            Some(left) => lines.push(
                Line::raw(txf(
                    self.lang,
                    "Too many wrong PINs: try again in {0} s",
                    &[&(left.as_secs() + 1)],
                ))
                .centered(),
            ),
            None => {
                let stars = "*".repeat(screen.field.text().chars().count());
                lines.push(Line::raw(txf(self.lang, "PIN: {0}_", &[&stars])).centered());
                if !screen.notice.is_empty() {
                    lines.push(
                        Line::styled(
                            screen.notice.clone(),
                            Style::default().fg(palette.level_color(LogLevel::Error)),
                        )
                        .centered(),
                    );
                }
            }
        }
        lines.push(Line::raw(""));
        lines.push(
            Line::styled(
                tx(
                    self.lang,
                    "Type the PIN and press Enter. The lock deters onlookers; it is not security.",
                ),
                Style::default().fg(palette.dim()),
            )
            .centered(),
        );
        let y = inner.y + inner.height.saturating_sub(lines.len() as u16) / 2;
        let rect = Rect::new(
            inner.x,
            y,
            inner.width,
            (lines.len() as u16).min(inner.height),
        );
        frame.render_widget(Paragraph::new(lines), rect);
    }

    /// The external tools editor: the list (name, command, shortcut, rule and dropped
    /// runs), or the form of one tool with the placeholders the arguments take.
    fn draw_tools_editor(&mut self, frame: &mut Frame, area: Rect) {
        let palette = self.palette;
        let dim = Style::default().fg(palette.dim());
        let Some(d) = self.tools_editor.as_ref() else {
            return;
        };
        if let Some(form) = &d.form {
            let lines = form.lines(self.lang);
            let problems = form.problems(&d.rules, self.lang);
            let (focus, rejected) = (form.focus, form.rejected);
            let title = if form.index.is_some() {
                tx(
                    self.lang,
                    "Tool - Tab/Up/Down move, Space ticks, Left/Right choose",
                )
            } else {
                tx(
                    self.lang,
                    "New tool - Tab/Up/Down move, Space ticks, Left/Right choose",
                )
            };
            let height = (lines.len() as u16 + 5).min(area.height);
            let inner = self.dialog(frame, area, (78, height), title, true);
            let error = Style::default().fg(palette.level_color(LogLevel::Error));
            let mut out = Vec::with_capacity(lines.len());
            let fields = lines.len() - 1;
            for (i, text) in lines.iter().enumerate() {
                let y = inner.y + i as u16;
                if y + 1 >= inner.bottom() {
                    break;
                }
                if i == fields {
                    out.push(Line::styled(view::sanitize(text), dim));
                    continue;
                }
                self.hits
                    .list_items
                    .push((Rect::new(inner.x, y, inner.width, 1), i));
                let problem = problems.iter().find(|(p, _)| *p == i).map(|(_, p)| p);
                let mut style = Style::default();
                if problem.is_some() && rejected {
                    style = error;
                }
                if i == focus {
                    style = style.add_modifier(Modifier::REVERSED);
                }
                let shown = match problem {
                    Some(p) if rejected => format!("{text}  ({p})"),
                    _ => text.clone(),
                };
                out.push(Line::styled(view::sanitize(&shown), style));
            }
            frame.render_widget(Paragraph::new(out), inner);
            return;
        }
        let rows = d.list.items.len().max(1) as u16;
        let height = (rows + 6).min(area.height);
        let inner = self.dialog(
            frame,
            area,
            (78, height),
            tx(self.lang, "External tools"),
            false,
        );
        let Some(d) = self.tools_editor.as_mut() else {
            return;
        };
        let list_rows = inner.height.saturating_sub(3) as usize;
        if d.list.selected < d.top {
            d.top = d.list.selected;
        } else if list_rows > 0 && d.list.selected >= d.top + list_rows {
            d.top = d.list.selected + 1 - list_rows;
        }
        let mut out = vec![Line::styled(
            tx(
                self.lang,
                "Enter edit  a add  d delete  Alt+Up/Down K/J move  Esc close",
            ),
            dim,
        )];
        if d.list.items.is_empty() {
            out.push(Line::styled(tx(self.lang, "  (no tools: a adds one)"), dim));
        }
        for (i, t) in d.list.items.iter().enumerate().skip(d.top).take(list_rows) {
            let y = inner.y + 1 + (i - d.top) as u16;
            self.hits
                .list_items
                .push((Rect::new(inner.x, y, inner.width, 1), i));
            let row = if i == d.list.selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            let mut extra = Vec::new();
            if let Some(sc) = t.shortcut.as_deref() {
                extra.push(sc.to_string());
            }
            if let Some(rule) = t.bound_rule.as_deref() {
                extra.push(txf(self.lang, "rule {0}", &[&rule]));
                let dropped = self.tool_runner.dropped_for(&t.name);
                if dropped > 0 {
                    extra.push(format!("{dropped} dropped"));
                }
            }
            if t.use_shell {
                extra.push("shell".into());
            }
            let name: String = view::sanitize(&t.name).chars().take(20).collect();
            let command: String = view::sanitize(&format!("{} {}", t.program, t.args))
                .chars()
                .take(30)
                .collect();
            out.push(Line::from(vec![
                Span::styled(format!("{:>2}. {name:<20} ", i + 1), row),
                Span::styled(format!("{command:<31}"), dim),
                Span::styled(extra.join("  "), dim),
            ]));
        }
        frame.render_widget(Paragraph::new(out), inner);
    }

    /// The command palette: the query, then the matching actions with their keys; a
    /// number goes to that line.
    fn draw_palette(&mut self, frame: &mut Frame, area: Rect) {
        let dim = Style::default().fg(self.palette.dim());
        let Some(p) = self.palette_dialog.as_ref() else {
            return;
        };
        let shown = p.shown();
        let goto = palette::is_goto(p.field.text());
        let height = (shown.len().max(1) as u16 + 5).min(area.height).min(22);
        let inner = self.dialog(frame, area, (64, height), tx(self.lang, "Commands"), true);
        let Some(p) = self.palette_dialog.as_ref() else {
            return;
        };
        let (text, x) = p.field.view(inner.width.saturating_sub(2) as usize);
        let mut out = vec![Line::raw(format!(": {}", view::sanitize(&text)))];
        out.push(Line::styled(
            if goto {
                tx(
                    self.lang,
                    "Enter goes to this line (N +N -N) or time (14:02)",
                )
            } else {
                tx(
                    self.lang,
                    "Type to filter, Enter runs, a number goes to that line",
                )
            },
            dim,
        ));
        if !goto && shown.is_empty() {
            out.push(Line::styled(tx(self.lang, "  (no command matches)"), dim));
        }
        let room = inner.height.saturating_sub(2) as usize;
        let top = p.selected.saturating_sub(room.saturating_sub(1));
        let width = inner.width as usize;
        let mut items = Vec::new();
        for (row, &i) in shown.iter().enumerate().skip(top).take(room) {
            let e = &p.entries[i];
            let style = if row == p.selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            let name_room = width.saturating_sub(e.key.len() + 3);
            let name: String = e.name.chars().take(name_room).collect();
            let gap = width.saturating_sub(name.chars().count() + e.key.len() + 2);
            out.push(Line::from(vec![
                Span::styled(format!(" {name}{}", " ".repeat(gap)), style),
                Span::styled(format!("{} ", e.key), style.patch(dim)),
            ]));
            let y = inner.y + 2 + (row - top) as u16;
            items.push((Rect::new(inner.x, y, inner.width, 1), i));
        }
        self.hits.list_items.extend(items);
        frame.render_widget(Paragraph::new(out), inner);
        frame.set_cursor_position((inner.x + 2 + x as u16, inner.y));
    }

    /// The About window, as the GUI's: name, tagline, version, build date, the terminal's
    /// colours, author, website, repository and licence.
    fn draw_about(&mut self, frame: &mut Frame, area: Rect) {
        use crate::i18n::t;
        let lang = self
            .settings
            .as_ref()
            .map(|s| s.config.language)
            .unwrap_or(crate::i18n::Language::En);
        let accent = Style::default()
            .fg(self.palette.accent())
            .add_modifier(Modifier::BOLD);
        let dim = Style::default().fg(self.palette.dim());
        let colours = match self.palette.depth {
            crate::tui::colors::ColorDepth::TrueColor => "truecolor",
            crate::tui::colors::ColorDepth::Ansi256 => tx(self.lang, "256 colours"),
            crate::tui::colors::ColorDepth::Ansi16 => tx(self.lang, "16 colours"),
        };
        let terminal = if self.palette.ascii {
            txf(self.lang, "terminal, {0}, ASCII", &[&colours])
        } else {
            txf(self.lang, "terminal, {0}", &[&colours])
        };
        let rows: Vec<(&str, String)> = vec![
            (
                t(lang, "about_version"),
                format!("v{}", env!("CARGO_PKG_VERSION")),
            ),
            (
                t(lang, "about_build_date"),
                env!("BUILD_TIMESTAMP").to_string(),
            ),
            (t(lang, "about_renderer"), terminal),
            (t(lang, "about_author"), AUTHOR.to_string()),
            (t(lang, "about_website"), "www.baccan.it".to_string()),
            (
                t(lang, "about_repo"),
                "github.com/matteobaccan/FastTail".to_string(),
            ),
            (t(lang, "about_license"), "MIT".to_string()),
        ];
        let label_w = rows
            .iter()
            .map(|(l, _)| unicode_width::UnicodeWidthStr::width(*l))
            .max()
            .unwrap_or(0);
        let tagline = t(lang, "about_tagline");
        let height = rows.len() as u16 + 7;
        let title = txf(self.lang, "{0} FastTail", &[&t(lang, "about")]);
        let inner = self.dialog(frame, area, (64, height), &title, false);
        let mut out = vec![
            Line::styled(" FASTTAIL", accent),
            Line::styled(format!(" {tagline}"), dim),
            Line::raw(""),
        ];
        for (label, value) in rows {
            let pad = label_w.saturating_sub(unicode_width::UnicodeWidthStr::width(label));
            out.push(Line::from(vec![
                Span::styled(format!(" {label}{} ", " ".repeat(pad)), dim),
                Span::raw(value),
            ]));
        }
        frame.render_widget(Paragraph::new(out), inner);
    }

    /// The Find results window: the query box, a summary, then one header per stream
    /// (name, matches, state) and its results (line number, text). Below 60 columns the
    /// line numbers are left out.
    fn draw_find(&mut self, frame: &mut Frame, area: Rect) {
        let palette = self.palette;
        let lang = self.lang;
        let dim = Style::default().fg(palette.dim());
        let width = area.width.saturating_sub(4).clamp(30, 160);
        let height = area.height.saturating_sub(2).max(9);
        let title = tx(lang, "Find results");
        let inner = self.dialog(frame, area, (width, height), title, false);
        let room = inner.height.saturating_sub(3) as usize;
        self.find_rows = room.max(1);
        let Some(w) = self.find_window.as_mut() else {
            return;
        };
        w.reveal(room.max(1));
        let w = &*w;
        let (shown, x) = w.field.view(inner.width.saturating_sub(2) as usize);
        let query_style = if w.editing { Style::default() } else { dim };
        let mut out = vec![Line::styled(
            format!("> {}", view::sanitize(&shown)),
            query_style,
        )];
        let session = &w.session;
        out.push(if session.started_at.is_none() {
            Line::styled(tx(lang, "Enter search all streams   Esc close"), dim)
        } else {
            let mut text = txf(
                lang,
                "{0} matches in {1} of {2} streams",
                &[
                    &session.total_hits(),
                    &session.streams_with_hits(),
                    &session.groups.len(),
                ],
            );
            if session.is_active() {
                text.push_str("   ");
                text.push_str(&txf(
                    lang,
                    "searching {0}, queued {1}",
                    &[&session.running_count(), &session.queued_count()],
                ));
            }
            Line::styled(text, Style::default().fg(palette.accent()))
        });
        out.push(Line::styled(
            tx(
                lang,
                "Enter go to line   Space fold   r refresh   s stop   Tab query   Esc close",
            ),
            dim,
        ));
        let numbers = inner.width >= 60;
        let mut items = Vec::new();
        for i in w.top..(w.top + room).min(w.row_count()) {
            let Some(row) = w.row(i) else {
                break;
            };
            let mut line = match row {
                FindRow::Header(g) => {
                    let group = &session.groups[g];
                    let mark = if group.collapsed { "▶" } else { "▼" };
                    let mut text = format!(
                        "{mark} {}  {}",
                        view::sanitize(&group.name),
                        txf(lang, "{0} matches", &[&group.total])
                    );
                    let state = match group.state {
                        FindState::Queued => Some(tx(lang, "queued").to_string()),
                        FindState::Running => Some(format!("{:.0}%", group.progress * 100.0)),
                        FindState::Done => None,
                        FindState::Stopped => Some(tx(lang, "stopped").to_string()),
                        FindState::Failed => Some(tx(lang, "failed").to_string()),
                        FindState::SkippedHex => Some(tx(lang, "HEX: not searched").to_string()),
                        FindState::Stale => Some(tx(lang, "stale: press r").to_string()),
                        FindState::SkippedUntimed => Some(tx(lang, "no timestamps").to_string()),
                    };
                    if let Some(state) = state {
                        text.push_str(&format!("  ({state})"));
                    }
                    if group.capped() {
                        text.push_str("  ");
                        text.push_str(&txf(lang, "first {0} listed", &[&group.hits.len()]));
                    }
                    Line::styled(
                        text,
                        Style::default()
                            .fg(palette.accent())
                            .add_modifier(Modifier::BOLD),
                    )
                }
                FindRow::Hit(g, h) => {
                    let group = &session.groups[g];
                    let idx = group.hits[h];
                    let engine = self
                        .tabs
                        .iter()
                        .map(|t| &t.engine)
                        .find(|e| e.path == group.path);
                    let text = engine.and_then(|e| e.get_line(idx)).unwrap_or_default();
                    let text = view::sanitize(&text);
                    let style = if group.jumpable() {
                        Style::default()
                    } else {
                        dim
                    };
                    let mut spans = vec![if numbers {
                        let number = engine.map_or(idx + 1, |e| e.shown_line_number(idx));
                        Span::styled(format!("  {number:>8}  "), dim)
                    } else {
                        Span::raw("  ")
                    }];
                    spans.extend(find_hit_spans(&text, &session.query, style, palette.hit()));
                    Line::from(spans)
                }
            };
            if i == w.selected && !w.editing {
                line = line.patch_style(Style::default().add_modifier(Modifier::REVERSED));
            }
            let y = inner.y + 3 + (i - w.top) as u16;
            items.push((Rect::new(inner.x, y, inner.width, 1), i));
            out.push(line);
        }
        let editing = w.editing;
        self.hits.list_items.extend(items);
        frame.render_widget(Paragraph::new(out), inner);
        if editing {
            frame.set_cursor_position((inner.x + 2 + x as u16, inner.y));
        }
    }

    /// The JSON dialog: the tree rows around the selected one, keys coloured as the GUI's
    /// (keys plain, strings in the second accent, numbers in the accent, the rest dim).
    fn draw_json(&mut self, frame: &mut Frame, area: Rect) {
        let palette = self.palette;
        let dim = Style::default().fg(palette.dim());
        let Some(d) = self.json_dialog.as_ref() else {
            return;
        };
        let rows = d.rows();
        let title = txf(self.lang, "JSON - line {0}", &[&d.line_number]);
        let width = area.width.saturating_sub(4).clamp(20, 110);
        let height = (rows.len() as u16 + 5).min(area.height).max(7);
        let inner = self.dialog(frame, area, (width, height), &title, false);
        let Some(d) = self.json_dialog.as_ref() else {
            return;
        };
        let mut out = vec![Line::styled(
            tx(
                self.lang,
                "Arrows move and fold, * expand all, - collapse, y value, Y path",
            ),
            dim,
        )];
        let room = inner.height.saturating_sub(1) as usize;
        let top = d.selected.saturating_sub(room.saturating_sub(1));
        let mut items = Vec::new();
        for (i, row) in rows.iter().enumerate().skip(top).take(room) {
            let text = d.row_text(*row, self.lang);
            let value_style = match text.kind {
                Some(crate::json_tree::Kind::String) => Style::default().fg(palette.secondary()),
                Some(crate::json_tree::Kind::Number) => Style::default().fg(palette.accent()),
                _ => dim,
            };
            let mut spans = vec![Span::styled(text.prefix, dim)];
            if let Some(key) = text.key {
                spans.push(Span::raw(view::sanitize(&key)));
                spans.push(Span::styled(": ", dim));
            }
            spans.push(Span::styled(view::sanitize(&text.value), value_style));
            let mut line = Line::from(spans);
            if i == d.selected {
                line = line.patch_style(Style::default().add_modifier(Modifier::REVERSED));
            }
            out.push(line);
            let y = inner.y + 1 + (i - top) as u16;
            items.push((Rect::new(inner.x, y, inner.width, 1), i));
        }
        if let Some(at) = d.tree.error() {
            out.push(Line::styled(
                txf(self.lang, "! invalid JSON from byte {0}", &[&at]),
                dim,
            ));
        }
        self.hits.list_items.extend(items);
        frame.render_widget(Paragraph::new(out), inner);
    }

    /// The external tools menu: number, name and command of each tool, its shortcut
    /// and the rule it runs for.
    fn draw_tool_menu(&mut self, frame: &mut Frame, area: Rect) {
        let palette = self.palette;
        let dim = Style::default().fg(palette.dim());
        let Some(selected) = self.tool_menu.as_ref().map(|m| m.selected) else {
            return;
        };
        let tools = self
            .settings
            .as_ref()
            .map(|s| s.config.external_tools.clone())
            .unwrap_or_default();
        let rows = tools.len().max(1) as u16;
        let height = (rows + 6).min(area.height);
        let inner = self.dialog(
            frame,
            area,
            (78, height),
            tx(
                self.lang,
                "External tools - on the cursor row or the selection",
            ),
            false,
        );
        let mut out = vec![Line::styled(
            tx(
                self.lang,
                "Enter or 1-9 runs, Up/Down choose, e edits the tools, Esc closes",
            ),
            dim,
        )];
        if tools.is_empty() {
            out.push(Line::styled(
                tx(
                    self.lang,
                    "  (no tools: e adds one, as Settings > External tools does)",
                ),
                dim,
            ));
        }
        let room = inner.height.saturating_sub(3) as usize;
        let top = selected.saturating_sub(room.saturating_sub(1));
        for (i, t) in tools.iter().enumerate().skip(top).take(room) {
            let y = inner.y + 1 + (i - top) as u16;
            self.hits
                .list_items
                .push((Rect::new(inner.x, y, inner.width, 1), i));
            let row = if i == selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            let mut extra = Vec::new();
            if let Some(sc) = t.shortcut.as_deref().filter(|s| !s.is_empty()) {
                extra.push(sc.to_string());
            }
            if let Some(rule) = t.bound_rule.as_deref() {
                let dropped = self.tool_runner.dropped_for(&t.name);
                extra.push(if dropped > 0 {
                    txf(self.lang, "rule {0}, {1} dropped", &[&rule, &dropped])
                } else {
                    txf(self.lang, "rule {0}", &[&rule])
                });
            }
            let name: String = view::sanitize(&t.name).chars().take(20).collect();
            let command: String = view::sanitize(&format!("{} {}", t.program, t.args))
                .chars()
                .take(30)
                .collect();
            out.push(Line::from(vec![
                Span::styled(format!("{:>2}. {name:<20} ", i + 1), row),
                Span::styled(format!("{command:<31}"), dim),
                Span::styled(extra.join("  "), dim),
            ]));
        }
        frame.render_widget(Paragraph::new(out), inner);
    }

    /// The global filter editor: the switches, then the include and exclude rows
    /// (filled ones and a spare one each), a regex that does not compile marked.
    fn draw_global(&mut self, frame: &mut Frame, area: Rect) {
        let palette = self.palette;
        let dim = Style::default().fg(palette.dim());
        let error = Style::default().fg(palette.level_color(LogLevel::Error));
        let Some(d) = self.global.as_ref() else {
            return;
        };
        let rows = d.rows();
        let height = (rows.len() as u16 + 6).min(area.height);
        let inner = self.dialog(
            frame,
            area,
            (72, height),
            tx(
                self.lang,
                "Global filter - every stream combines it with its own filters",
            ),
            false,
        );
        // An expression that does not compile says why (the last valid one filters).
        let base = self.settings_mut().config.global_filter.clone();
        let broken = self
            .global
            .as_ref()
            .and_then(|d| d.expr_error(&base, self.lang));
        let hint = match &broken {
            Some(why) => why.clone(),
            None => tx(
                self.lang,
                "Tab/Up/Down move, Space ticks, type a term; applied as you type. Enter/Esc close",
            )
            .to_string(),
        };
        let Some(d) = self.global.as_ref() else {
            return;
        };
        let mut out = vec![Line::styled(
            hint,
            if broken.is_some() { error } else { dim },
        )];
        let mut cursor = None;
        for (k, row) in rows.iter().enumerate() {
            let y = inner.y + 1 + k as u16;
            if y + 1 >= inner.bottom() {
                break;
            }
            self.hits
                .list_items
                .push((Rect::new(inner.x, y, inner.width, 1), k));
            let (label, value) = match row {
                global::Row::Enabled => (tx(self.lang, "On (f)"), d.enabled.text("")),
                global::Row::MatchCase => (tx(self.lang, "Match case"), d.case_sensitive.text("")),
                global::Row::Regex => (tx(self.lang, "Regular expressions"), d.is_regex.text("")),
                global::Row::ExprMode => (tx(self.lang, "Expression"), d.expr_mode.text("")),
                global::Row::Expression => (
                    tx(self.lang, "Include (expression)"),
                    d.expression.text().to_string(),
                ),
                global::Row::Include(i) => (
                    if *i == 0 {
                        tx(self.lang, "Include (all match)")
                    } else {
                        ""
                    },
                    d.term(*row).to_string(),
                ),
                global::Row::Exclude(i) => (
                    if *i == 0 {
                        tx(self.lang, "Exclude (none match)")
                    } else {
                        ""
                    },
                    d.term(*row).to_string(),
                ),
            };
            let term_row = matches!(
                row,
                global::Row::Include(_) | global::Row::Exclude(_) | global::Row::Expression
            );
            let mut style = Style::default();
            if k == d.focus {
                style = style.add_modifier(Modifier::REVERSED);
                if term_row {
                    let field = match row {
                        global::Row::Include(i) => d.include.get(*i),
                        global::Row::Exclude(i) => d.exclude.get(*i),
                        global::Row::Expression => Some(&d.expression),
                        _ => None,
                    };
                    let x = field.map_or(0, |f| f.view(40).1);
                    cursor = Some((inner.x + 22 + x as u16, y));
                }
            }
            let mut spans = vec![
                Span::styled(format!("{label:<22}"), dim),
                Span::styled(format!("{:<40}", view::sanitize(&value)), style),
            ];
            if d.invalid.contains(row) {
                spans.push(Span::styled(tx(self.lang, " does not compile"), error));
            }
            out.push(Line::from(spans));
        }
        frame.render_widget(Paragraph::new(out), inner);
        if let Some(at) = cursor {
            frame.set_cursor_position(at);
        }
    }

    /// The filter presets: the list (name and filters, the one equal to the focused
    /// stream marked `=`, the one applied and edited since `~`), or the name being
    /// typed, or the delete confirmation.
    fn draw_presets(&mut self, frame: &mut Frame, area: Rect) {
        let palette = self.palette;
        let dim = Style::default().fg(palette.dim());
        let accent = Style::default().fg(palette.accent());
        let Some(d) = self.presets.as_ref() else {
            return;
        };
        match &d.mode {
            presets::Mode::Name {
                field,
                rename,
                with_time,
                on_time,
            } => {
                let title = if rename.is_some() {
                    tx(self.lang, "Rename preset")
                } else {
                    tx(self.lang, "Save the stream's filters as a preset")
                };
                let (shown, x) = field.view(60);
                let problem = d.problem;
                let mut lines = vec![Line::from(vec![
                    Span::styled(tx(self.lang, "Name "), dim),
                    Span::raw(view::sanitize(&shown)),
                ])];
                if rename.is_none() {
                    let style = if *on_time {
                        Style::default().add_modifier(Modifier::REVERSED)
                    } else {
                        Style::default()
                    };
                    lines.push(Line::styled(
                        with_time.text(tx(self.lang, "include the time range (Tab, Space)")),
                        style,
                    ));
                    lines.push(Line::styled(
                        tx(self.lang, "A taken name replaces that preset."),
                        dim,
                    ));
                }
                if let Some(p) = problem {
                    lines.push(Line::styled(
                        tx(self.lang, p),
                        Style::default().fg(palette.level_color(LogLevel::Error)),
                    ));
                }
                let on_time = *on_time;
                let height = lines.len() as u16 + 4;
                let inner = self.dialog(frame, area, (70, height), title, true);
                if !on_time {
                    frame.set_cursor_position((inner.x + 5 + x as u16, inner.y));
                }
                frame.render_widget(Paragraph::new(lines), inner);
            }
            presets::Mode::ConfirmDelete(i) => {
                let name = d
                    .list
                    .items
                    .get(*i)
                    .map(|p| view::sanitize(&p.name))
                    .unwrap_or_default();
                let inner =
                    self.dialog(frame, area, (64, 6), tx(self.lang, "Delete preset?"), true);
                frame.render_widget(
                    Paragraph::new(vec![
                        Line::raw(txf(self.lang, "Delete the preset {0}?", &[&name])),
                        Line::styled(tx(self.lang, "Enter deletes, Esc keeps it."), dim),
                    ]),
                    inner,
                );
            }
            presets::Mode::List => {
                let label = self
                    .tabs
                    .get(self.active)
                    .map(|t| crate::filter_preset::preset_label(&d.list.items, &t.engine));
                let marks: Vec<&str> = d
                    .list
                    .items
                    .iter()
                    .map(|p| match label {
                        Some(crate::filter_preset::PresetLabel::Matches(n)) if n == p.name => "=",
                        Some(crate::filter_preset::PresetLabel::Modified(n)) if n == p.name => "~",
                        _ => " ",
                    })
                    .collect();
                let rows = d.list.items.len().max(1) as u16;
                let height = (rows + 6).min(area.height);
                let inner = self.dialog(
                    frame,
                    area,
                    (78, height),
                    tx(
                        self.lang,
                        "Filter presets - = the stream's filters, ~ edited since",
                    ),
                    false,
                );
                let Some(d) = self.presets.as_mut() else {
                    return;
                };
                let list_rows = inner.height.saturating_sub(3) as usize;
                if d.list.selected < d.top {
                    d.top = d.list.selected;
                } else if list_rows > 0 && d.list.selected >= d.top + list_rows {
                    d.top = d.list.selected + 1 - list_rows;
                }
                let mut out = vec![Line::styled(
                    tx(
                        self.lang,
                        "Enter apply  A to all  s save  r rename  d delete  * pin  Alt+Up/Down move  Esc",
                    ),
                    dim,
                )];
                if d.list.items.is_empty() {
                    out.push(Line::styled(
                        tx(
                            self.lang,
                            "  (no presets: s saves the stream's filters as one)",
                        ),
                        dim,
                    ));
                }
                for (i, p) in d.list.items.iter().enumerate().skip(d.top).take(list_rows) {
                    let y = inner.y + 1 + (i - d.top) as u16;
                    self.hits
                        .list_items
                        .push((Rect::new(inner.x, y, inner.width, 1), i));
                    let row = if i == d.list.selected {
                        Style::default().add_modifier(Modifier::REVERSED)
                    } else {
                        Style::default()
                    };
                    let name: String = view::sanitize(&p.name).chars().take(24).collect();
                    let pin = if p.pinned { "*" } else { " " };
                    out.push(Line::from(vec![
                        Span::styled(format!("{}{pin}", marks[i]), accent),
                        Span::styled(format!("{name:<24} "), row),
                        Span::styled(view::sanitize(&presets::summary(&p.state, self.lang)), dim),
                    ]));
                }
                frame.render_widget(Paragraph::new(out), inner);
            }
        }
    }

    /// The rule editor: the list (each pattern in its own colours, then its options),
    /// or the form of one rule with a preview at the terminal's colour depth.
    fn draw_rules(&mut self, frame: &mut Frame, area: Rect) {
        let palette = self.palette;
        let dim = Style::default().fg(palette.dim());
        let Some(d) = self.rules.as_ref() else {
            return;
        };
        if let Some(form) = &d.form {
            let lines = form.lines(self.lang);
            let problems = form.problems(self.lang);
            let fallback = form
                .index
                .and_then(|i| d.list.items.get(i))
                .map(|e| e.rule.clone())
                .unwrap_or_else(|| {
                    crate::tail_engine::HighlightRule::new("", [255; 3], [0; 3], false)
                });
            let rule = form.rule(&fallback);
            let (focus, rejected) = (form.focus, form.rejected);
            let title = if form.index.is_some() {
                tx(
                    self.lang,
                    "Rule - Tab/Up/Down move, Space ticks, Left/Right choose, [ ] colour",
                )
            } else {
                tx(
                    self.lang,
                    "New rule - Tab/Up/Down move, Space ticks, Left/Right choose, [ ] colour",
                )
            };
            let height = (lines.len() as u16 + 6).min(area.height);
            let inner = self.dialog(frame, area, (78, height), title, true);
            let error = Style::default().fg(palette.level_color(LogLevel::Error));
            let mut out = Vec::with_capacity(lines.len() + 2);
            for (i, text) in lines.iter().enumerate() {
                let y = inner.y + i as u16;
                if y + 2 >= inner.bottom() {
                    break;
                }
                self.hits
                    .list_items
                    .push((Rect::new(inner.x, y, inner.width, 1), i));
                let problem = problems.iter().find(|(p, _)| *p == i).map(|(_, p)| p);
                let mut style = Style::default();
                if problem.is_some() && rejected {
                    style = error;
                }
                if i == focus {
                    style = style.add_modifier(Modifier::REVERSED);
                }
                let shown = match problem {
                    Some(p) if rejected => format!("{text}  ({p})"),
                    _ => text.clone(),
                };
                out.push(Line::styled(view::sanitize(&shown), style));
            }
            let sample = if rule.pattern.is_empty() {
                tx(self.lang, "sample text").to_string()
            } else {
                view::sanitize(&rule.pattern)
            };
            out.push(Line::raw(""));
            out.push(Line::from(vec![
                Span::styled(format!("{:<30}", tx(self.lang, "Preview")), dim),
                Span::styled(format!(" {sample} "), palette.rule_style(&rule.style())),
            ]));
            frame.render_widget(Paragraph::new(out), inner);
            return;
        }
        // The quick labels take a title row and one row each below the rules.
        let label_rows = if d.labels.is_empty() {
            0
        } else {
            d.labels.len() + 1
        };
        let rows = (d.list.items.len().max(1) + label_rows) as u16;
        let height = (rows + 6).min(area.height);
        let inner = self.dialog(
            frame,
            area,
            (78, height),
            tx(
                self.lang,
                "Highlight rules - the first rule that matches paints the line",
            ),
            false,
        );
        let Some(d) = self.rules.as_mut() else {
            return;
        };
        let label_rows = label_rows.min(inner.height.saturating_sub(4) as usize);
        let list_rows = (inner.height.saturating_sub(3) as usize).saturating_sub(label_rows);
        if d.list.selected < d.top {
            d.top = d.list.selected;
        } else if list_rows > 0 && d.list.selected >= d.top + list_rows {
            d.top = d.list.selected + 1 - list_rows;
        }
        let mut out = Vec::with_capacity(list_rows + 2);
        out.push(Line::styled(
            tx(
                self.lang,
                "Enter edit  a add  d delete  Space on/off  K/J move  i import  x export  Esc",
            ),
            dim,
        ));
        if d.list.items.is_empty() {
            out.push(Line::styled(tx(self.lang, "  (no rules: a adds one)"), dim));
        }
        for (i, e) in d.list.items.iter().enumerate().skip(d.top).take(list_rows) {
            let y = inner.y + 1 + (i - d.top) as u16;
            self.hits
                .list_items
                .push((Rect::new(inner.x, y, inner.width, 1), i));
            let r = &e.rule;
            let mark = if r.enabled { "[x]" } else { "[ ]" };
            let mut flags = Vec::new();
            if r.is_regex {
                flags.push(if r.captures_only {
                    tx(self.lang, "regex groups")
                } else {
                    "regex"
                });
            }
            if r.case_sensitive {
                flags.push("Aa");
            }
            if r.auto_bookmark {
                flags.push("bookmarks");
            }
            if r.sound_alert != crate::audio::SoundAlertPreset::None {
                flags.push(r.sound_alert.name());
            }
            let tools: Vec<&str> = e
                .tools
                .iter()
                .filter_map(|t| d.tool_names.get(*t).map(String::as_str))
                .collect();
            let tools = if tools.is_empty() {
                String::new()
            } else {
                txf(self.lang, "  tool: {0}", &[&tools.join(", ")])
            };
            let selected = i == d.list.selected && !d.on_labels;
            let row = if selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            let name: String = view::sanitize(&r.pattern).chars().take(36).collect();
            out.push(Line::from(vec![
                Span::styled(format!("{mark} {:>2}. ", i + 1), row),
                Span::styled(format!(" {name} "), palette.rule_style(&r.style())),
                Span::styled(
                    format!("  {}{tools}", flags.join(" ")),
                    row.fg(palette.dim()),
                ),
            ]));
        }
        if label_rows > 0 {
            let shown_rules = out.len();
            out.push(Line::styled(
                tx(
                    self.lang,
                    "Quick labels (memory only) - Tab here and back, d removes",
                ),
                dim,
            ));
            let first = inner.y + shown_rules as u16 + 1;
            let n = d.list.items.len();
            for (j, l) in d.labels.iter().enumerate().take(label_rows - 1) {
                self.hits
                    .list_items
                    .push((Rect::new(inner.x, first + j as u16, inner.width, 1), n + j));
                let row = if d.on_labels && j == d.label_sel {
                    Style::default().add_modifier(Modifier::REVERSED)
                } else {
                    Style::default()
                };
                let text: String = view::sanitize(&l.text).chars().take(50).collect();
                out.push(Line::from(vec![
                    Span::styled(format!("    {} ", l.color), row),
                    Span::styled(format!(" {text} "), palette.label_style(l.color)),
                ]));
            }
        }
        frame.render_widget(Paragraph::new(out), inner);
    }

    fn draw_settings(&mut self, frame: &mut Frame, area: Rect) {
        let Some(lines) = self.settings_form.as_ref().map(|f| f.lines()) else {
            return;
        };
        let height = (lines.len() as u16 + 5).min(area.height);
        let inner = self.movable_dialog(
            frame,
            area,
            MovableDialog::Settings,
            (74, height),
            tx(
                self.lang,
                "Settings - Tab/Up/Down move, Left/Right choose, Space ticks",
            ),
            true,
        );
        let palette = self.palette;
        let Some(form) = self.settings_form.as_mut() else {
            return;
        };
        let rows = inner.height.saturating_sub(1) as usize;
        // Keep the focused field on screen.
        if let Some(at) = lines.iter().position(|(_, f)| *f == Some(form.focus)) {
            if at < form.top {
                form.top = at.saturating_sub(1);
            } else if rows > 0 && at >= form.top + rows {
                form.top = at + 1 - rows;
            }
        }
        let problems = form.problems();
        let error = Style::default().fg(palette.level_color(LogLevel::Error));
        let mut out = Vec::with_capacity(rows);
        for (k, (text, field)) in lines.iter().enumerate().skip(form.top).take(rows) {
            let y = inner.y + (k - form.top) as u16;
            let line = match field {
                None => Line::styled(
                    text.clone(),
                    Style::default()
                        .fg(palette.accent())
                        .add_modifier(Modifier::BOLD),
                ),
                Some(i) => {
                    self.hits
                        .list_items
                        .push((Rect::new(inner.x, y, inner.width, 1), *i));
                    let problem = problems.iter().find(|(p, _)| p == i).map(|(_, p)| p);
                    let mut style = Style::default();
                    if problem.is_some() && form.rejected {
                        style = error;
                    }
                    if *i == form.focus {
                        style = style.add_modifier(Modifier::REVERSED);
                    }
                    let shown = match problem {
                        Some(p) => format!("{text}  ({p})"),
                        None => text.clone(),
                    };
                    Line::styled(view::sanitize(&shown), style)
                }
            };
            out.push(line);
        }
        frame.render_widget(Paragraph::new(out), inner);
        frame.render_widget(
            Paragraph::new(Line::styled(
                tx(self.lang, "Enter or [ OK ] applies and saves, Esc cancels"),
                Style::default().fg(palette.dim()),
            )),
            Rect {
                y: inner.bottom().saturating_sub(1),
                height: 1,
                ..inner
            },
        );
    }

    fn draw_sessions(&mut self, frame: &mut Frame, area: Rect) {
        let rows = self.sessions.as_ref().map_or(0, |d| d.recent.len()) as u16;
        let inner = self.dialog(
            frame,
            area,
            (72, rows.max(1) + 7),
            tx(self.lang, "Open session"),
            true,
        );
        let palette = self.palette;
        let Some(d) = &self.sessions else {
            return;
        };
        let dim = Style::default().fg(palette.dim());
        let (shown, x) = d.field.view(inner.width.saturating_sub(6) as usize);
        let mut lines = vec![
            Line::from(vec![
                Span::styled(tx(self.lang, "Path "), dim),
                Span::raw(view::sanitize(&shown)),
            ]),
            Line::styled(
                tx(
                    self.lang,
                    "Recent sessions (Up/Down, Enter with an empty path):",
                ),
                dim,
            ),
        ];
        frame.set_cursor_position((inner.x + 5 + x as u16, inner.y));
        if d.recent.is_empty() {
            lines.push(Line::styled(tx(self.lang, "  (none)"), dim));
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

    fn draw_interface_switch(&mut self, frame: &mut Frame, area: Rect) {
        let Some(state) = self.interface_switch.clone() else {
            return;
        };
        let (title, text, cancel) = match &state {
            InterfaceSwitch::Offer => (
                tx(self.lang, "Switch interface"),
                vec![
                    Line::raw(tx(
                        self.lang,
                        "The graphical interface opens at the next start.",
                    )),
                    Line::raw(tx(
                        self.lang,
                        "Switch now? The workspace is saved and the terminal",
                    )),
                    Line::raw(tx(
                        self.lang,
                        "interface closes. (Enter: now / Esc: at next start)",
                    )),
                ],
                true,
            ),
            InterfaceSwitch::Failed(why) => (
                tx(self.lang, "Graphical interface not started"),
                vec![
                    Line::raw(view::sanitize(why)),
                    Line::raw(tx(
                        self.lang,
                        "It opens at the next start; this interface keeps running.",
                    )),
                ],
                false,
            ),
        };
        let width = text
            .iter()
            .map(|l| l.width() as u16 + 4)
            .max()
            .unwrap_or(40)
            .clamp(40, area.width.saturating_sub(2).max(40));
        let inner = self.dialog(frame, area, (width, text.len() as u16 + 3), title, cancel);
        frame.render_widget(
            Paragraph::new(text).wrap(ratatui::widgets::Wrap { trim: false }),
            inner,
        );
    }

    fn draw_confirm(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.dialog(frame, area, (64, 6), tx(self.lang, "Overwrite?"), true);
        let Some(file) = &self.confirm_overwrite else {
            return;
        };
        let text = vec![
            Line::raw(view::sanitize(&txf(
                self.lang,
                "{0} exists.",
                &[&file.display()],
            ))),
            Line::raw(tx(
                self.lang,
                "Replace it with the open streams? (Enter / Esc)",
            )),
        ];
        frame.render_widget(Paragraph::new(text), inner);
    }

    /// The Open dialog: the folder, the name field, the list (name, size or kind,
    /// date) and a status line.
    fn draw_browser(&mut self, frame: &mut Frame, area: Rect) {
        let height = area.height.saturating_sub(2).clamp(10, 30);
        let inner = self.dialog(frame, area, (78, height), tx(self.lang, "Open"), true);
        let palette = self.palette;
        let Some(b) = self.browser.as_mut() else {
            return;
        };
        let dim = Style::default().fg(palette.dim());
        let accent = Style::default().fg(palette.accent());
        let row = |y: u16| Rect {
            y,
            height: 1,
            ..inner
        };
        let width = inner.width as usize;
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(tx(self.lang, "Look in "), dim),
                Span::styled(
                    view::sanitize(&tail_chars(&b.location(), width.saturating_sub(8))),
                    accent,
                ),
            ])),
            row(inner.y),
        );
        let (shown_field, x) = b.field.view(width.saturating_sub(6));
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(tx(self.lang, "Name "), dim),
                Span::raw(view::sanitize(&shown_field)),
            ])),
            row(inner.y + 1),
        );
        frame.set_cursor_position((inner.x + 5 + x as u16, inner.y + 1));
        let list = Rect {
            y: inner.y + 2,
            height: inner.height.saturating_sub(4),
            ..inner
        };
        let shown = b.shown();
        let rows = list.height as usize;
        if b.selected < b.top {
            b.top = b.selected;
        } else if rows > 0 && b.selected >= b.top + rows {
            b.top = b.selected + 1 - rows;
        }
        // Name, then a 10-cell size or kind and a 16-cell date.
        let name_w = width.saturating_sub(29).max(8);
        let mut lines = Vec::with_capacity(rows);
        for (k, &i) in shown.iter().enumerate().skip(b.top).take(rows) {
            let e = &b.entries[i];
            let (kind, style) = match e.kind {
                Kind::Parent => ("<UP>".to_string(), accent),
                Kind::Drive => ("<DRIVE>".to_string(), accent),
                Kind::Dir => ("<DIR>".to_string(), accent),
                Kind::File => (crate::tui::picker::human_size(e.size), Style::default()),
            };
            let date = e.modified.map(browser::date_text).unwrap_or_default();
            let name = head_chars(&view::sanitize(&e.name), name_w);
            let text = format!("{name:<name_w$} {kind:>10} {date:>16}");
            let style = if k == b.selected {
                style.add_modifier(Modifier::REVERSED)
            } else {
                style
            };
            let y = list.y + (k - b.top) as u16;
            self.hits
                .list_items
                .push((Rect::new(list.x, y, list.width, 1), k));
            lines.push(Line::styled(text, style));
        }
        frame.render_widget(Paragraph::new(lines), list);
        let (dirs, files) = b.entries.iter().fold((0, 0), |(d, f), e| match e.kind {
            Kind::File => (d, f + 1),
            Kind::Dir | Kind::Drive => (d + 1, f),
            Kind::Parent => (d, f),
        });
        let status = match &b.note {
            Some(note) => note.clone(),
            None => txf(
                self.lang,
                "{0} folders, {1} files. Enter opens, Backspace goes up, type to filter or a path",
                &[&dirs, &files],
            ),
        };
        frame.render_widget(
            Paragraph::new(Line::styled(head_chars(&status, width), dim)),
            row(list.bottom()),
        );
    }

    fn draw_picker(&mut self, frame: &mut Frame, area: Rect) {
        let Some(title) = self.picker.as_ref().map(|p| {
            let name = p
                .archive
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            txf(self.lang, "Open an entry of {0}", &[&name])
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
                Span::styled(tx(self.lang, "Filter "), dim),
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
                Some(r) => (format!("{}  ({})", e.name, refusal_text(r, self.lang)), dim),
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
            (Some(scan), _) => txf(
                self.lang,
                "{0} entries, reading the archive {1}%",
                &[&p.entries.len(), &format!("{:.0}", scan.progress() * 100.0)],
            ),
            (None, Some(note)) => txf(
                self.lang,
                "{0} of {1} entries. {2}",
                &[&shown.len(), &p.entries.len(), &note],
            ),
            (None, None) => txf(
                self.lang,
                "{0} of {1} entries",
                &[&shown.len(), &p.entries.len()],
            ),
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
        let inner = self.dialog(frame, area, (64, 21), tx(self.lang, "Time range"), true);
        let palette = self.palette;
        let Some(d) = &self.time_range else {
            return;
        };
        let reference = d.reference;
        const LABEL: u16 = 7;
        let width = inner.width.saturating_sub(LABEL) as usize;
        let dim = Style::default().fg(palette.dim());
        let accent = Style::default()
            .fg(palette.accent())
            .add_modifier(Modifier::BOLD);
        let mut lines = Vec::with_capacity(18);
        let mut cursor = None;
        for (row, (label, field, on)) in [
            (tx(self.lang, "From"), &d.from, !d.on_to),
            (tx(self.lang, "To"), &d.to, d.on_to),
        ]
        .into_iter()
        .enumerate()
        {
            let (shown, x) = field.view(width);
            let bad = !side_readable(field.text().trim(), reference);
            let label_style = if on { accent } else { dim };
            let mut field_style = if bad {
                Style::default().fg(palette.level_color(LogLevel::Error))
            } else {
                Style::default()
            }
            .add_modifier(Modifier::UNDERLINED);
            if on && d.zone == RangeZone::Field {
                field_style = field_style.add_modifier(Modifier::BOLD);
                cursor = Some((inner.x + LABEL + x as u16, inner.y + row as u16));
            }
            let pad = width.saturating_sub(unicode_width::UnicodeWidthStr::width(shown.as_str()));
            lines.push(Line::from(vec![
                Span::styled(format!("{label:<w$}", w = LABEL as usize), label_style),
                Span::styled(
                    format!("{}{}", view::sanitize(&shown), " ".repeat(pad)),
                    field_style,
                ),
            ]));
            self.hits.list_items.push((
                Rect::new(inner.x, inner.y + row as u16, inner.width, 1),
                if row == 0 { RANGE_FROM } else { RANGE_TO },
            ));
        }
        // The calendar and the time of the edited side.
        let side = if d.on_to {
            tx(self.lang, "To")
        } else {
            tx(self.lang, "From")
        };
        let picked = time_range_text::parse(d.side_text(), reference).map(time_range_text::day_of);
        let cal_on = d.zone == RangeZone::Calendar;
        lines.push(Line::raw(""));
        lines.push(Line::from(vec![
            Span::styled(format!("{side}: "), accent),
            Span::styled(
                format!("\u{25c0} {} \u{25b6}", d.calendar.title(self.lang)),
                if cal_on { accent } else { Style::default() },
            ),
            Span::styled(tx(self.lang, "  PgUp/PgDn month"), dim),
        ]));
        lines.push(Line::styled(tx(self.lang, "Mo Tu We Th Fr Sa Su"), dim));
        let first_week_row = inner.y + lines.len() as u16;
        for (w, week) in d.calendar.weeks().iter().enumerate() {
            let mut spans = Vec::with_capacity(7);
            for (col, cell) in week.iter().enumerate() {
                let text = match cell {
                    Some(day) => format!("{:>2} ", calendar::day_of_month(*day)),
                    None => "   ".into(),
                };
                let mut style = Style::default();
                if let Some(day) = cell {
                    if Some(*day) == picked {
                        style = accent;
                    }
                    if *day == d.calendar.cursor {
                        style = style.add_modifier(if cal_on {
                            Modifier::REVERSED
                        } else {
                            Modifier::UNDERLINED
                        });
                    }
                    self.hits.list_items.push((
                        Rect::new(inner.x + col as u16 * 3, first_week_row + w as u16, 2, 1),
                        RANGE_DAY + *day as usize,
                    ));
                }
                spans.push(Span::styled(text, style));
            }
            lines.push(Line::from(spans));
        }
        while lines.len() < 12 {
            lines.push(Line::raw(""));
        }
        let (h, m, _) = time_range_text::clock_of(d.side_text(), reference, d.side());
        let time_on = d.zone == RangeZone::Time;
        let part = |selected: bool| {
            if time_on && selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else if time_on {
                accent
            } else {
                Style::default()
            }
        };
        lines.push(Line::from(vec![
            Span::styled(tx(self.lang, "Time   "), if time_on { accent } else { dim }),
            Span::styled(format!("{h:02}"), part(!d.on_minutes)),
            Span::raw(":"),
            Span::styled(format!("{m:02}"), part(d.on_minutes)),
            Span::styled(
                tx(self.lang, "   Up/Down change, Left/Right hours or minutes"),
                dim,
            ),
        ]));
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            tx(
                self.lang,
                "Type 2026-09-18 14:02, 14:02, -15m or now; empty side = open.",
            ),
            dim,
        ));
        lines.push(Line::styled(
            tx(
                self.lang,
                "Tab: field, calendar, time. Space picks a day, Enter applies.",
            ),
            dim,
        ));
        if d.invalid {
            lines.push(Line::styled(
                tx(self.lang, "A side cannot be read as a time"),
                Style::default().fg(palette.level_color(LogLevel::Error)),
            ));
        }
        frame.render_widget(Paragraph::new(lines), inner);
        if let Some(c) = cursor {
            frame.set_cursor_position(c);
        }
    }

    fn draw_help(&mut self, frame: &mut Frame, area: Rect) {
        let note = if self.mouse {
            tx(
                self.lang,
                "Mouse on: SHIFT + drag selects text natively (--no-mouse turns the mouse off)",
            )
        } else {
            tx(
                self.lang,
                "Mouse off (--no-mouse): the terminal selects text",
            )
        };
        // As many columns (up to 3, each at least 44 cells) as it takes to show every
        // entry at once; a screen too small for that scrolls, the cursor kept in view.
        let lang = self.lang;
        let kw = HELP
            .iter()
            .map(|(k, _, _)| tx(lang, k).chars().count())
            .max()
            .unwrap_or(0)
            + 2;
        let entries: Vec<String> = HELP
            .iter()
            .map(|(k, d, _)| format!("{:<kw$}{}", tx(lang, k), tx(lang, d)))
            .collect();
        let longest = entries.iter().map(|e| e.chars().count()).max().unwrap_or(0);
        // Moved or resized: the columns fit the dialog's inner area, not the screen.
        let stored = self
            .help_rect
            .map(|r| fit_dialog(r, MovableDialog::Help, area));
        let (room_w, room_h) = match stored {
            Some(r) => (
                (r.width as usize).saturating_sub(2),
                (r.height as usize).saturating_sub(3),
            ),
            None => (
                (area.width as usize).saturating_sub(4),
                (area.height as usize).saturating_sub(4),
            ),
        };
        let fits = |n: usize| {
            let col = longest.min(room_w.saturating_sub(2 * (n - 1)) / n);
            (col >= 44 || n == 1).then_some(col)
        };
        let mut cols = 1;
        let mut col = fits(1).unwrap_or(longest).max(1);
        for n in 1..=3 {
            let Some(c) = fits(n) else {
                break;
            };
            cols = n;
            col = c;
            if HELP.len().div_ceil(n) < room_h {
                break;
            }
        }
        let per_col = HELP.len().div_ceil(cols);
        self.help_half = if cols > 1 { per_col } else { 0 };
        let total = per_col + 1;
        let width = (cols * col + 2 * (cols - 1) + 2) as u16;
        let height = stored.map_or((total as u16 + 3).min(area.height), |r| r.height);
        let rows = height.saturating_sub(3) as usize;
        // Keep the selected entry's row on screen.
        let sel_row = self.help_sel % per_col.max(1);
        if sel_row < self.help_top {
            self.help_top = sel_row;
        } else if rows > 0 && sel_row >= self.help_top + rows {
            self.help_top = sel_row + 1 - rows;
        }
        self.help_top = self.help_top.min(total.saturating_sub(rows));
        let hidden = total.saturating_sub(self.help_top + rows);
        let inner = self.movable_dialog(
            frame,
            area,
            MovableDialog::Help,
            (width, height),
            tx(self.lang, "Keys - arrows choose, Enter runs, Esc closes"),
            false,
        );
        let selected = Style::default().add_modifier(Modifier::REVERSED);
        let info = Style::default().fg(self.palette.dim());
        let mut lines = Vec::with_capacity(rows);
        for row in self.help_top..(self.help_top + rows).min(total) {
            let y = inner.y + (row - self.help_top) as u16;
            if row >= per_col {
                lines.push(Line::styled(head_chars(note, inner.width as usize), info));
                continue;
            }
            let mut spans = Vec::with_capacity(2 * cols);
            for c in 0..cols {
                let i = row + c * per_col;
                if i >= HELP.len() {
                    continue;
                }
                let style = if i == self.help_sel {
                    selected
                } else if HELP[i].2.is_none() {
                    info
                } else {
                    Style::default()
                };
                let x = inner.x + (c * (col + 2)) as u16;
                self.hits
                    .list_items
                    .push((Rect::new(x, y, col as u16, 1), i));
                if c > 0 {
                    spans.push(Span::raw("  "));
                }
                let text = head_chars(&entries[i], col);
                spans.push(Span::styled(format!("{text:<col$}"), style));
            }
            lines.push(Line::from(spans));
        }
        frame.render_widget(Paragraph::new(lines), inner);
        if hidden > 0 {
            // On the button row, left of OK, so no entry is covered.
            let hint = txf(self.lang, " ↓ {0} more (Down) ", &[&hidden]);
            let at = Rect::new(
                inner.x,
                inner.bottom(),
                (hint.chars().count() as u16).min(inner.width),
                1,
            );
            frame.render_widget(
                Paragraph::new(Line::styled(
                    hint,
                    Style::default()
                        .fg(self.palette.accent())
                        .add_modifier(Modifier::BOLD),
                )),
                at,
            );
        }
    }
}

/// The help: keys, what they do and the command `Enter` runs from the help (`None` for
/// entries that only describe, which the cursor steps over).
/// `p` in the columns list: `key` becomes the first plotted field (the one the sparkline
/// row draws), keeping its statistic; `p` on the first plotted field removes it.
fn plot_first(plots: &[crate::plot::PlotSpec], key: &str) -> Vec<crate::plot::PlotSpec> {
    if plots.first().is_some_and(|p| p.key == key) {
        return plots[1..].to_vec();
    }
    let stat = plots
        .iter()
        .find(|p| p.key == key)
        .map_or(crate::plot::PlotStat::Avg, |p| p.stat);
    let mut out = vec![crate::plot::PlotSpec {
        key: key.to_string(),
        stat,
    }];
    out.extend(plots.iter().filter(|p| p.key != key).cloned());
    out.truncate(crate::plot::MAX_PLOTS);
    out
}

/// The sparkline row of a stream: `📈 key avg`, the line over the time the values cover,
/// then the lowest and highest value; the label goes first when the row is narrow.
/// The volume row of `engine` `columns` cells wide: a label, then one block per column of
/// the timeline histogram (its height the column's lines against the busiest, in the
/// ERROR or WARN colour when it holds such lines), and the cell offsets with the time each
/// starts at, for the clicks.
fn volume_row(
    engine: &TailEngine,
    palette: &Palette,
    columns: usize,
    lang: crate::i18n::Language,
) -> (Line<'static>, Vec<(usize, i64)>) {
    let dim = Style::default().fg(palette.dim());
    let label = if palette.ascii { " # " } else { " ▤ " };
    let hist = engine.time_histogram();
    let room = columns.saturating_sub(width(label) + 1);
    let cols = hist.columns(room);
    if cols.is_empty() || room == 0 {
        let text = if engine.histogram_lines() == 0 && engine.total_lines() > 0 {
            tx(lang, "volume: no timestamps yet")
        } else {
            tx(lang, "volume: no lines")
        };
        return (
            Line::from(vec![Span::styled(label, dim), Span::styled(text, dim)]),
            Vec::new(),
        );
    }
    let totals: Vec<u64> = cols.iter().map(|c| c.total()).collect();
    // Few buckets are spread over the room, as the window's strip stretches them.
    let per = (room / cols.len()).max(1);
    let blocks = crate::tui::spark::counts(&totals, cols.len(), palette.ascii);
    let mut spans = vec![Span::styled(label, dim)];
    let mut cells = Vec::with_capacity(room);
    let mut x = width(label);
    for (c, ch) in cols.iter().zip(blocks.chars()) {
        let worst = |l: LogLevel| c.counts[l as usize] > 0;
        let style = if worst(LogLevel::Error) || worst(LogLevel::Fatal) {
            Style::default().fg(palette.level_color(LogLevel::Error))
        } else if worst(LogLevel::Warn) {
            Style::default().fg(palette.level_color(LogLevel::Warn))
        } else {
            Style::default().fg(palette.accent())
        };
        spans.push(Span::styled(ch.to_string().repeat(per), style));
        for k in 0..per {
            cells.push((x + k, c.start_ms));
        }
        x += per;
    }
    (Line::from(spans), cells)
}

fn sparkline_row(
    engine: &TailEngine,
    palette: &Palette,
    columns: usize,
    lang: crate::i18n::Language,
) -> Line<'static> {
    let dim = Style::default().fg(palette.dim());
    let Some(spec) = engine.plots().first() else {
        return Line::default();
    };
    let mark = if palette.ascii { "~" } else { "📈" };
    let label = format!(" {mark} {} {} ", spec.key, spec.stat.name());
    let plot = engine.plot.as_ref();
    let series = plot.and_then(|p| p.series.iter().find(|s| s.spec.key == spec.key));
    let note = |text: String| {
        Line::from(vec![
            Span::styled(label.clone(), dim),
            Span::styled(text, dim),
        ])
    };
    let Some(series) = series.filter(|s| !s.is_empty()) else {
        return match plot.filter(|p| p.running()) {
            Some(p) => note(txf(
                lang,
                "counting {0}%",
                &[&format!("{:.0}", p.progress * 100.0)],
            )),
            None => note(tx(lang, "no numeric values").to_string()),
        };
    };
    let range = |lo: f64, hi: f64| {
        let unit = series.unit();
        format!(
            " {} - {}",
            crate::plot::format_value(lo, unit),
            crate::plot::format_value(hi, unit)
        )
    };
    // A first pass measures the scale's ends, the second fits the line beside them.
    let mut label_w = width(&label);
    if columns < label_w + 24 {
        label_w = 0;
    }
    let ends = crate::tui::spark::spark(
        series,
        columns.saturating_sub(label_w + 16).max(1),
        palette.ascii,
    )
    .map_or(0, |s| width(&range(s.min, s.max)));
    let room = columns.saturating_sub(label_w + ends);
    let Some(sp) = crate::tui::spark::spark(series, room.max(1), palette.ascii) else {
        return note(tx(lang, "no numeric values").to_string());
    };
    let mut spans = Vec::new();
    if label_w > 0 {
        spans.push(Span::styled(label, dim));
    }
    spans.push(Span::styled(sp.text, Style::default().fg(palette.accent())));
    spans.push(Span::styled(range(sp.min, sp.max), dim));
    Line::from(spans)
}

/// A Find results row: the text with the query's occurrences (case folded, as the search
/// matches) in the hit style. Only ASCII is folded here, so a non-ASCII query whose case
/// differs is drawn plain.
fn find_hit_spans(text: &str, query: &str, style: Style, hit: Style) -> Vec<Span<'static>> {
    if query.is_empty() {
        return vec![Span::styled(text.to_string(), style)];
    }
    let folded = text.to_ascii_lowercase();
    let needle = query.to_ascii_lowercase();
    let mut spans = Vec::new();
    let mut at = 0;
    for (start, m) in folded.match_indices(needle.as_str()) {
        if start > at {
            spans.push(Span::styled(text[at..start].to_string(), style));
        }
        spans.push(Span::styled(text[start..start + m.len()].to_string(), hit));
        at = start + m.len();
    }
    if at < text.len() {
        spans.push(Span::styled(text[at..].to_string(), style));
    }
    spans
}

const HELP: &[(&str, &str, Option<Action>)] = &[
    ("Up/Down j/k", en("move the cursor one row"), None),
    (
        "PgUp/PgDn",
        en("move it one page (also Ctrl+B / Ctrl+F)"),
        None,
    ),
    (
        "Shift+Up/Down",
        en("extend the selection from the cursor"),
        None,
    ),
    (
        "Home g  End G",
        en("first row / last row, and follow it"),
        Some(Action::Bottom),
    ),
    (
        "Left/Right 0",
        en("sideways (Ctrl: by 10) / column 0"),
        None,
    ),
    (en("Space"), en("toggle follow"), Some(Action::ToggleFollow)),
    (
        "/  Alt+/",
        en("search; Alt+/ all streams"),
        Some(Action::StartSearch),
    ),
    (
        "n Shift+N  F3",
        en("next / previous hit (Shift+F3 previous)"),
        Some(Action::SearchNext),
    ),
    (
        "Esc",
        en("clear the search and the selection"),
        Some(Action::ClearSearch),
    ),
    (
        "i / x",
        en("include / exclude filter"),
        Some(Action::EditInclude),
    ),
    ("l", en("cycle the minimum level"), Some(Action::CycleLevel)),
    (
        "H  R",
        en("hide rows; R: rotations, pieces"),
        Some(Action::HideLines),
    ),
    (
        "C",
        en("column view of the fields on or off"),
        Some(Action::ToggleColumns),
    ),
    (
        "c",
        en("cycle collapse: off, exact, numbers"),
        Some(Action::CycleCollapse),
    ),
    (
        "s |  _",
        en("new window beside / below (next stream)"),
        Some(Action::SplitRight),
    ),
    (
        "Ctrl+W",
        en("close the stream, and its empty window"),
        Some(Action::CloseStream),
    ),
    (
        "Alt+X",
        en("close the window (its tabs join the next)"),
        Some(Action::ClosePane),
    ),
    (
        "< >",
        en("move the stream to the prev / next window"),
        Some(Action::MoveNextPane),
    ),
    (
        "Ctrl+PgUp/PgDn",
        en("previous / next tab of the window"),
        Some(Action::NextInPane),
    ),
    (
        "Alt+F",
        en("float the window, or dock it back"),
        Some(Action::ToggleFloat),
    ),
    (
        "Alt+arrows",
        en("move the divider or a floating window"),
        None,
    ),
    (
        en("click"),
        en("focus a window, select a row, show a tab"),
        None,
    ),
    (
        en("Shift+click"),
        en("extend the selection (or drag over rows)"),
        None,
    ),
    (en("double click"), en("toggle the row's bookmark"), None),
    (en("wheel"), en("scroll the window under the pointer"), None),
    ("[x]", en("top right: close the window's stream"), None),
    (
        en("drag edge"),
        en("resize a floating window (corners, sides)"),
        None,
    ),
    (en("drag divider"), en("resize the windows beside it"), None),
    (
        en("drag title"),
        en("edge splits, centre = tab, else floats"),
        None,
    ),
    (
        "Tab  Alt+1..9",
        en("next window or file / file N (Shift+Tab)"),
        Some(Action::NextTab),
    ),
    (
        "b  Ctrl+F2",
        en("bookmark the cursor row, on or off"),
        Some(Action::ToggleBookmark),
    ),
    (
        "e E  w W",
        en("next / previous ERROR, WARN line (wraps)"),
        Some(Action::NextError),
    ),
    ("10j  3e", en("a count repeats j k n N e E w W"), None),
    (
        "] [  F2",
        en("next / previous bookmark (Shift+F2 back)"),
        Some(Action::NextBookmark),
    ),
    (
        "m",
        en("note of the cursor row's bookmark"),
        Some(Action::EditNote),
    ),
    (
        "Ctrl+K",
        en("the row in context (filters off) and back"),
        Some(Action::ToggleContext),
    ),
    (
        "J",
        en("the cursor row's JSON as a tree"),
        Some(Action::JsonTree),
    ),
    (
        "Ctrl+G  :",
        en("go to line or time; : all commands"),
        Some(Action::GoTo),
    ),
    (
        "o  Ctrl+O",
        en("open a file; Ctrl+O: the row's source"),
        Some(Action::OpenFile),
    ),
    (
        "Shift+O/S",
        en("open / save a session"),
        Some(Action::OpenSession),
    ),
    (
        "t",
        en("time range: from / to, calendar and time"),
        Some(Action::TimeRange),
    ),
    (
        ",",
        en("Settings: theme, language, view, sound"),
        Some(Action::Settings),
    ),
    (
        "r  L",
        en("highlight rules; L 1-9 labels the search"),
        Some(Action::EditRules),
    ),
    (
        "p",
        en("filter presets: apply, save, rename"),
        Some(Action::Presets),
    ),
    (
        "F  f",
        en("global filter: edit / on or off"),
        Some(Action::EditGlobal),
    ),
    (
        "!",
        en("external tools on the row or selection"),
        Some(Action::Tools),
    ),
    ("Ctrl+L", en("lock with the PIN"), Some(Action::Lock)),
    (
        "Shift+T",
        en("next theme (Tron ... Commander)"),
        Some(Action::CycleTheme),
    ),
    (
        "a  #",
        en("ANSI: auto/render/strip/raw; # numbers"),
        Some(Action::CycleAnsi),
    ),
    (
        "h  d  D",
        en("HEX / ASM view and back; D: 16/32/64"),
        Some(Action::ToggleHex),
    ),
    (
        "y Ctrl+C",
        en("copy selection or row (Ctrl+C: else quit)"),
        Some(Action::Copy),
    ),
    (
        "q  ?  F1",
        en("quit; ? or F1: this help"),
        Some(Action::Quit),
    ),
];

/// The first `n` characters of `s`, the last one an ellipsis when it is cut.
fn head_chars(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    let mut out: String = s.chars().take(n.saturating_sub(1)).collect();
    out.push('~');
    out
}

/// The last `n` characters of `s` (the end of a long path), marked when cut.
fn tail_chars(s: &str, n: usize) -> String {
    let count = s.chars().count();
    if count <= n {
        return s.to_string();
    }
    let mut out = String::from("~");
    out.extend(s.chars().skip(count + 1 - n.max(1)));
    out
}

/// The name of a tab that is not an open stream: a GUI panel, or a file this run does
/// not have open.
fn panel_name(tab: &DockTab, lang: Language) -> String {
    match tab {
        DockTab::LogStream(p) => p
            .file_name()
            .map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into()),
        DockTab::Filters => tx(lang, "Filters").into(),
        DockTab::Highlights => tx(lang, "Highlights").into(),
        DockTab::Settings => tx(lang, "Settings").into(),
        DockTab::FindResults => tx(lang, "Find results").into(),
        DockTab::Scratchpad => tx(lang, "Scratchpad").into(),
        DockTab::Compare => tx(lang, "Compare").into(),
        DockTab::Statistics(_) => tx(lang, "Statistics").into(),
        DockTab::Query(_) => tx(lang, "Query").into(),
        DockTab::Patterns(_) => tx(lang, "Patterns").into(),
        DockTab::Explain(_) => tx(lang, "Explain").into(),
    }
}

/// The buttons of the status bar: key, label, command; drawn as `[? help]`, as many as
/// fit the width.
const STATUS: &[(&str, &str, Action)] = &[
    ("/", en("search"), Action::StartSearch),
    ("n", en("next"), Action::SearchNext),
    ("i", en("include"), Action::EditInclude),
    ("x", en("exclude"), Action::EditExclude),
    ("t", en("time"), Action::TimeRange),
    (en("Space"), en("follow"), Action::ToggleFollow),
    ("b", en("mark"), Action::ToggleBookmark),
    ("Shift+T", en("theme"), Action::CycleTheme),
    ("q", en("quit"), Action::Quit),
];

/// How long a closing window's outline takes to shrink away.
const CLOSE_EFFECT: Duration = Duration::from_millis(180);

/// The author, as the GUI's title bar and About window name him.
const AUTHOR: &str = "Matteo Baccan";

/// Rows of the top bar: the title row always, the command row from 16 rows up (a
/// smaller terminal keeps them for the log; the keys still work).
fn top_bar_rows(height: u16) -> u16 {
    if height >= 16 {
        2
    } else {
        1
    }
}

/// An item of the top bar: text, or a `[key label]` button (shown reversed when `on`).
enum TopItem {
    Text(String, Style),
    Button(&'static str, String, Action, bool),
}

impl TopItem {
    /// Its width in terminal cells.
    fn width(&self) -> u16 {
        let w = unicode_width::UnicodeWidthStr::width;
        match self {
            TopItem::Text(text, _) => w(text.as_str()) as u16,
            TopItem::Button(k, label, _, _) => {
                let key = if k.is_empty() { 0 } else { w(k) + 1 };
                (2 + key + w(label.as_str())) as u16
            }
        }
    }
}

/// The CPU and memory meters of the top bar, as the GUI's title bar shows them, read at
/// most once a second.
#[derive(Default)]
struct Telemetry {
    system: Option<sysinfo::System>,
    read_at: Option<Instant>,
    cpu: f32,
    used_mb: u64,
    total_mb: u64,
}

impl Telemetry {
    /// Reads the meters again when a second has passed. Returns whether what the bar
    /// shows changed.
    fn refresh(&mut self) -> bool {
        if self
            .read_at
            .is_some_and(|at| at.elapsed() < Duration::from_secs(1))
        {
            return false;
        }
        self.read_at = Some(Instant::now());
        if self.system.is_none() {
            let Ok(mut s) = sysinfo::System::new_with_specifics(
                sysinfo::RefreshKind::nothing()
                    .with_cpu(sysinfo::CpuRefreshKind::everything())
                    .with_memory(sysinfo::MemoryRefreshKind::everything()),
            ) else {
                // No meters where `sysinfo` cannot read them.
                return false;
            };
            s.refresh_cpu_usage();
            self.system = Some(s);
        }
        let Some(system) = self.system.as_mut() else {
            return false;
        };
        system.refresh_cpu_usage();
        system.refresh_memory();
        let before = (self.cpu.round() as u32, self.used_mb / 100, self.total_mb);
        self.cpu = system.global_cpu_usage();
        self.used_mb = system.used_memory() / (1024 * 1024);
        self.total_mb = system.total_memory() / (1024 * 1024);
        before != (self.cpu.round() as u32, self.used_mb / 100, self.total_mb)
    }

    /// `CPU ██░░░░  12%  RAM ███░░░ 3.1/16 GB`, or nothing before the first reading.
    fn text(&self, ascii: bool) -> Option<String> {
        if self.total_mb == 0 {
            return None;
        }
        let (full, empty) = if ascii { ('#', '-') } else { ('█', '░') };
        let bar = |fraction: f32| -> String {
            let n = ((fraction.clamp(0.0, 1.0) * 6.0).round() as usize).min(6);
            std::iter::repeat_n(full, n)
                .chain(std::iter::repeat_n(empty, 6 - n))
                .collect()
        };
        let used = self.used_mb as f32 / 1024.0;
        let total = (self.total_mb as f32 / 1024.0).max(1.0);
        Some(format!(
            "CPU {} {:>3.0}%  RAM {} {used:.1}/{total:.0} GB",
            bar(self.cpu / 100.0),
            self.cpu,
            bar(used / total),
        ))
    }
}

/// The next entry of the help that runs a command, from `from` in `step` direction
/// (wrapping); `from` itself when none.
fn help_step(from: usize, step: isize) -> usize {
    let n = HELP.len() as isize;
    let mut i = from as isize;
    for _ in 0..n {
        i = (i + step).rem_euclid(n);
        if HELP[i as usize].2.is_some() {
            return i as usize;
        }
    }
    from
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
/// The smallest a movable dialog is resized to (the screen when smaller).
fn dialog_min(which: MovableDialog) -> (u16, u16) {
    match which {
        MovableDialog::Settings => (40, 8),
        MovableDialog::Help => (30, 6),
    }
}

/// `r` at least `which`'s minimum size and at most `bounds`, moved inside `bounds`.
fn fit_dialog(r: Rect, which: MovableDialog, bounds: Rect) -> Rect {
    let min = dialog_min(which);
    let w = r.width.clamp(min.0.min(bounds.width), bounds.width);
    let h = r.height.clamp(min.1.min(bounds.height), bounds.height);
    let x = r.x.clamp(bounds.x, bounds.right().saturating_sub(w));
    let y = r.y.clamp(bounds.y, bounds.bottom().saturating_sub(h));
    Rect::new(x, y, w, h)
}

/// Bottom-left of a window: the counts, the cursor's `Ln` / `Col` (text views) and the
/// progress, cut to `room` cells by leaving out `Col`, then `Ln`. Also where `Ln` is
/// (its offset in the text and its width), for the click that opens Go to.
fn bottom_left_text(tab: &Tab, lang: Language, room: usize) -> (String, Option<(usize, usize)>) {
    let e = &tab.engine;
    let counts = counts_text(e, tab.is_hex(), tab.hex_width, lang);
    let progress = progress_text(e, lang);
    let ln = tab
        .cursor_line()
        .filter(|_| e.visible_line_count() > 0)
        .map(|l| txf(lang, "Ln {0}", &[&group_digits(l + 1)]));
    let col = (tab.hscroll > 0 && !tab.is_hex())
        .then(|| txf(lang, "Col {0}", &[&group_digits(tab.hscroll + 1)]));
    let compose = |ln: Option<&String>, col: Option<&String>| {
        let mut s = counts.clone();
        let mut at = None;
        if let Some(ln) = ln {
            s.push_str(" - ");
            at = Some((width(&s), width(ln)));
            s.push_str(ln);
            if let Some(col) = col {
                s.push(' ');
                s.push_str(col);
            }
        }
        s.push_str(&progress);
        (s, at)
    };
    let candidates = [
        compose(ln.as_ref(), col.as_ref()),
        compose(ln.as_ref(), None),
        compose(None, None),
    ];
    let fits = candidates.iter().position(|(s, _)| width(s) <= room);
    let [full, no_col, bare] = candidates;
    match fits {
        Some(0) => full,
        Some(1) => no_col,
        _ => bare,
    }
}

fn width(s: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(s)
}

fn counts_text(e: &TailEngine, hex: bool, hex_width: usize, lang: Language) -> String {
    let s = if e.view_mode == ViewMode::Asm {
        // Where the screen starts, out of the size (instructions are not counted).
        txf(
            lang,
            "{0} bytes, at 0x{1}",
            &[
                &group_digits(e.file_size as usize),
                &format!("{:X}", e.asm.top),
            ],
        )
    } else if hex {
        txf(
            lang,
            "{0} bytes, {1} rows of {2}",
            &[
                &group_digits(e.file_size as usize),
                &group_digits(e.total_hex_rows(hex_width)),
                &hex_width,
            ],
        )
    } else {
        txf(
            lang,
            "{0}/{1} lines",
            &[
                &group_digits(e.visible_line_count()),
                &group_digits(e.total_lines()),
            ],
        )
    };
    s
}

/// The running scan or decompression of a stream, ` - filtering 40%`, or nothing.
fn progress_text(e: &TailEngine, lang: Language) -> String {
    let mut s = String::new();
    if let Some((kind, p, hits)) = e.scan_progress() {
        let name = match kind {
            ScanKind::Index => "indexing",
            ScanKind::Filter => "filtering",
            ScanKind::Search => "searching",
            ScanKind::Levels => "levels",
            ScanKind::Timestamps => "timestamps",
            ScanKind::AutoBookmarks => "bookmarks",
            ScanKind::Collapse => "collapsing",
            ScanKind::FieldStats => "statistics",
            ScanKind::Query => "query",
            ScanKind::Patterns => "patterns",
            ScanKind::Explain => "explain",
            ScanKind::Plot => "plot",
            ScanKind::Operations => "operations",
        };
        s.push_str(&format!(" - {name} {:.0}%", p * 100.0));
        if kind == ScanKind::Search {
            s.push_str(&txf(lang, " ({0} hits)", &[&hits]));
        }
    }
    if let Some(c) = e.compressed.as_ref().filter(|c| c.is_running()) {
        s.push_str(&txf(
            lang,
            " - decompressing {0}%",
            &[&format!("{:.0}", c.progress() * 100.0)],
        ));
    }
    s
}

/// Bottom-right of a window: filters, level, collapse and the search position.
fn view_state_text(e: &TailEngine, hex: bool, lang: Language) -> String {
    if hex {
        // Filters, level and collapse do not apply to the bytes. ASM names its
        // architecture (the executable's when one is recognised).
        let view = if e.view_mode == ViewMode::Asm {
            format!("ASM {}", e.asm.label())
        } else {
            "HEX".into()
        };
        let query = e.search_query.trim();
        if query.is_empty() {
            return view;
        }
        let current = e
            .current_match_idx
            .map_or("-".to_string(), |i| (i + 1).to_string());
        return format!("{view}  /{query} {current}/{}", e.search_byte_matches.len());
    }
    let mut parts = Vec::new();
    let inc = e.include_display();
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
        let scope = if e.search_scope().is_all() {
            String::new()
        } else {
            format!(" {}", tx(lang, "in range"))
        };
        parts.push(format!(
            "/{} {}/{}{scope}",
            e.search_query.trim(),
            current,
            e.search_total()
        ));
    }
    if parts.is_empty() {
        tx(lang, "no filter").into()
    } else {
        parts.join("  ")
    }
}

/// `plugin://name?key=default&...` of a source plugin, for the prompt that opens it.
fn plugin_uri_with_defaults(p: &crate::plugins::Plugin) -> String {
    let values: std::collections::BTreeMap<String, String> = p
        .source
        .as_ref()
        .map(|s| {
            s.params
                .iter()
                .map(|q| (q.id.clone(), q.default.clone()))
                .collect()
        })
        .unwrap_or_default();
    let uri = crate::plugins::PluginUri {
        name: p.name.clone(),
        values: values.clone(),
    };
    // Every value written, secrets too (the user types them here).
    let mut text = uri.to_uri(None);
    if !values.is_empty() {
        let query: Vec<String> = values.iter().map(|(k, v)| format!("{k}={v}")).collect();
        text = format!("{text}?{}", query.join("&"));
    }
    text
}

/// The export menu's entries: each scope in each format.
const EXPORT_CHOICES: [(crate::export::Scope, crate::export::Format); 9] = {
    use crate::export::{Format as F, Scope as S};
    [
        (S::Visible, F::Text),
        (S::Visible, F::Csv),
        (S::Visible, F::Html),
        (S::Selected, F::Text),
        (S::Selected, F::Csv),
        (S::Selected, F::Html),
        (S::Matches, F::Text),
        (S::Matches, F::Csv),
        (S::Matches, F::Html),
    ]
};

fn export_format_name(format: crate::export::Format) -> &'static str {
    match format {
        crate::export::Format::Text => "TXT",
        crate::export::Format::Csv => "CSV",
        crate::export::Format::Html => "HTML",
    }
}

/// Where an export goes by default: next to the stream's file (the working folder for a
/// stream without one), named after it and the scope (`app-matches.csv`).
fn export_path(
    engine: &TailEngine,
    scope: crate::export::Scope,
    format: crate::export::Format,
) -> String {
    let what = match scope {
        crate::export::Scope::Visible => "visible",
        crate::export::Scope::Selected => "selected",
        crate::export::Scope::Matches => "matches",
    };
    let file = engine.current_file.as_deref().unwrap_or(&engine.path);
    let stem = file
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .filter(|s| !s.is_empty() && file.is_file())
        .unwrap_or_else(|| "fasttail".to_string());
    let name = format!("{stem}-{what}.{}", format.extension());
    match file
        .parent()
        .filter(|p| file.is_file() && !p.as_os_str().is_empty())
    {
        Some(dir) => dir.join(name).display().to_string(),
        None => name,
    }
}

/// The level colour of a line for a list, as the rows draw it.
fn palette_level(palette: &Palette, engine: &TailEngine, line: usize) -> Style {
    palette.level_style(engine.level_of(line))
}

/// Moves the cursor to a go-to target; the status message when there is one to show.
fn goto_target(
    tab: &mut Tab,
    target: Option<crate::tail_engine::GotoTarget>,
    typed: &str,
    lang: Language,
) -> Option<String> {
    let Some(t) = target else {
        return Some(txf(lang, "Cannot go to \"{0}\"", &[&typed]));
    };
    // The exact line: a collapsed group hiding it opens.
    tab.engine.reveal_line(t.line);
    if let Some(row) = tab.engine.get_visible_row_of_line(t.line) {
        tab.set_cursor(row);
    }
    t.hidden.then(|| {
        txf(
            lang,
            "Line {0} is hidden by the filters: showing {1}",
            &[&(t.requested + 1), &(t.line + 1)],
        )
    })
}

/// Enters the context view on the cursor row, or leaves it with the cursor back on the
/// line it was entered on.
fn toggle_context(tab: &mut Tab, message: &mut Option<String>, lang: Language) {
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
    // A filter tab shows the line in its source.
    if let Some(source_line) = tab
        .engine
        .derived
        .as_ref()
        .and_then(|d| d.source_line(line))
    {
        tab.engine.source_context_request = Some(source_line);
        return;
    }
    if !tab.engine.enter_context(line) {
        *message = Some(tx(lang, "Show in context needs an active filter").into());
    }
}

/// The rows of a window `height` rows tall: only these are read from the engine.
/// Also returns how many rows were drawn, for the mouse.
fn stream_rows(
    tab: &mut Tab,
    palette: &Palette,
    height: usize,
    gap_ms: u64,
    lang: Language,
) -> (Vec<Line<'static>>, usize) {
    tab.height = height;
    tab.pin_cursor();
    let delta = time_delta_column(&mut tab.engine, gap_ms);
    tab.engine.refresh_time_zone_sample();
    let engine = &tab.engine;
    let rows = engine.visible_line_count();
    if engine.follow_tail {
        tab.top = view::follow_top(rows, height);
    }
    tab.top = view::clamp_top(tab.top, height, rows);
    let range = view::visible_range(tab.top, height, rows);
    // Settings > Line numbers off keeps only the mark column.
    let gutter = if engine.show_line_numbers {
        view::gutter_width(engine.total_lines())
    } else {
        0
    };
    let tag = source_tag_width(engine, tab.cells);
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
            (gutter, delta, tag),
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
            tx(lang, "(empty file, or still loading)")
        } else {
            tx(lang, "(no line matches the filters)")
        };
        lines.push(Line::styled(text, Style::default().fg(palette.dim())));
    }
    (lines, drawn)
}

/// Widest a terminal column grows to fit its cells.
const COLUMN_MAX_CELLS: usize = 40;

/// The column view's rows of a window `height` rows tall, and its header row: each shown
/// field in a column as wide as its widest cell among the rows drawn (at most
/// `COLUMN_MAX_CELLS`, or the width set for it), then the message. Only these rows are
/// parsed. Also returns how many rows were drawn, for the mouse.
fn column_rows(
    tab: &mut Tab,
    palette: &Palette,
    height: usize,
    gap_ms: u64,
    lang: Language,
) -> (Line<'static>, Vec<Line<'static>>, usize) {
    tab.height = height;
    tab.pin_cursor();
    let delta = time_delta_column(&mut tab.engine, gap_ms);
    let rows = tab.engine.visible_line_count();
    if tab.engine.follow_tail {
        tab.top = view::follow_top(rows, height);
    }
    tab.top = view::clamp_top(tab.top, height, rows);
    let range = view::visible_range(tab.top, height, rows);
    let engine = &mut tab.engine;
    // The pinned columns first: they stay at the left while the others scroll sideways.
    let (keys, pinned) = engine.drawn_field_columns();
    // The rows drawn, with their fields.
    let mut drawn: Vec<(
        usize,
        usize,
        Option<std::sync::Arc<crate::fields::RowFields>>,
    )> = Vec::with_capacity(range.len());
    for row in range {
        let Some(line_idx) = engine.get_actual_line_idx(row) else {
            break;
        };
        let text = engine
            .get_line(line_idx)
            .map(|t| t.into_owned())
            .unwrap_or_default();
        let fields = engine.row_fields(line_idx, &text).filter(|f| f.shaped);
        drawn.push((row, line_idx, fields));
    }
    let set = engine.field_widths().clone();
    let widths: Vec<usize> = keys
        .iter()
        .map(|k| match set.get(k) {
            Some(w) => *w as usize,
            None => drawn
                .iter()
                .filter_map(|(_, _, f)| f.as_ref()?.get(k).map(width))
                .chain(std::iter::once(width(k)))
                .max()
                .unwrap_or(1)
                .min(COLUMN_MAX_CELLS),
        })
        .collect();
    let engine = &tab.engine;
    let gutter = if engine.show_line_numbers {
        view::gutter_width(engine.total_lines())
    } else {
        0
    };
    let tag = source_tag_width(engine, tab.cells);
    let message = tx(lang, "message");
    let mut fixed_head = String::new();
    let mut head = String::new();
    for (i, (k, w)) in keys.iter().zip(&widths).enumerate() {
        let out = if i < pinned {
            &mut fixed_head
        } else {
            &mut head
        };
        out.push_str(&pad_cell(k, *w));
        out.push(' ');
    }
    head.push_str(message);
    let head: String = view::skip_cells(vec![(head, Paint::Plain)], tab.hscroll)
        .into_iter()
        .map(|(s, _)| s)
        .collect();
    let head_style = Style::default()
        .fg(palette.accent())
        .add_modifier(Modifier::BOLD);
    let header = Line::from(vec![
        Span::raw(" ".repeat(
            gutter + 1 + delta.map_or(0, |_| DELTA_CELLS + 1) + if tag > 0 { tag + 1 } else { 0 },
        )),
        Span::styled(fixed_head, head_style),
        Span::styled(head, head_style),
    ]);
    let query = engine.search_query.trim().to_string();
    let has_search = !query.is_empty() && engine.active_match_count() > 0;
    let mut lines = Vec::with_capacity(drawn.len());
    for (row, line_idx, fields) in &drawn {
        let line = match fields {
            Some(fields) => {
                let (mut fixed, mut text) = (String::new(), String::new());
                for (i, (k, w)) in keys.iter().zip(&widths).enumerate() {
                    let out = if i < pinned { &mut fixed } else { &mut text };
                    out.push_str(&pad_cell(fields.get(k).unwrap_or(""), *w));
                    out.push(' ');
                }
                text.push_str(&fields.message(&keys));
                render_cells_row(
                    engine,
                    palette,
                    *row,
                    *line_idx,
                    (gutter, delta, tag),
                    &query,
                    has_search,
                    tab.hscroll,
                    (&fixed, &text),
                )
            }
            // A line without the parser's shape runs across the columns, dimmed.
            None => render_row(
                engine,
                palette,
                *row,
                *line_idx,
                (gutter, delta, tag),
                &query,
                has_search,
                tab.hscroll,
            )
            .patch_style(Style::default().add_modifier(Modifier::DIM)),
        };
        lines.push(if *row == tab.cursor {
            line.patch_style(Style::default().add_modifier(Modifier::REVERSED))
        } else {
            line
        });
    }
    let count = lines.len();
    if lines.is_empty() {
        let text = if engine.total_lines() == 0 {
            tx(lang, "(empty file, or still loading)")
        } else {
            tx(lang, "(no line matches the filters)")
        };
        lines.push(Line::styled(text, Style::default().fg(palette.dim())));
    }
    (header, lines, count)
}

/// `text` cut or padded to `cells` terminal cells.
fn pad_cell(text: &str, cells: usize) -> String {
    let text: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let mut out = String::new();
    let mut used = 0;
    for c in text.chars() {
        let w = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if used + w > cells {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push_str(&" ".repeat(cells - used));
    out
}

/// A row of the column view: the gutter as `render_row` draws it, then the pinned cells
/// as they are and the other cells and the message scrolled by `hscroll`, in the level or
/// rule colour of the line, search hits tinted.
#[allow(clippy::too_many_arguments)]
fn render_cells_row(
    engine: &TailEngine,
    palette: &Palette,
    row: usize,
    line_idx: usize,
    (gutter, delta, tag): (usize, Option<u64>, usize),
    query: &str,
    has_search: bool,
    hscroll: usize,
    (fixed, text): (&str, &str),
) -> Line<'static> {
    let (hit, active, bookmarked) = engine.row_marks(row, line_idx, has_search);
    let gutter_style = if active {
        palette.active_hit()
    } else {
        Style::default().fg(palette.dim())
    };
    let mark = row_mark(engine, line_idx, hit, bookmarked);
    let mut spans = vec![Span::styled(
        if gutter == 0 {
            mark.to_string()
        } else {
            format!("{:>gutter$}{mark}", row_number(engine, line_idx))
        },
        gutter_style,
    )];
    if let Some(gap_ms) = delta {
        spans.push(time_delta_cell(engine, palette, row, gap_ms));
    }
    spans.extend(source_tag_cell(engine, line_idx, tag, palette.ascii));
    let raw = engine.get_line(line_idx).unwrap_or_default();
    let base = match engine.match_highlight(&raw) {
        Some(rule) => palette.rule_style(&rule),
        None => palette.level_style(engine.level_of(line_idx)),
    };
    let segs_of = |text: &str| -> Vec<(String, Paint)> {
        let hits = if hit && !query.is_empty() {
            view::hit_ranges(text, query)
        } else {
            Vec::new()
        };
        view::segments(text, &hits, &[])
            .into_iter()
            .map(|(s, p)| (view::sanitize(s), p))
            .collect()
    };
    let hit_style = if active {
        palette.active_hit()
    } else {
        palette.hit()
    };
    let fixed_segs = segs_of(fixed);
    for (s, paint) in fixed_segs
        .into_iter()
        .chain(view::skip_cells(segs_of(text), hscroll))
    {
        let style = match paint {
            Paint::Hit => hit_style,
            _ => base,
        };
        spans.push(Span::styled(s, style));
    }
    let line = Line::from(spans);
    if engine.is_selected(line_idx) {
        line.style(palette.selection())
    } else {
        line
    }
}

/// The HEX rows of a window `width` x `height`: only the bytes of these rows are read,
/// in one go. Also returns how many rows were drawn, for the mouse.
fn hex_rows(
    tab: &mut Tab,
    palette: &Palette,
    width: usize,
    height: usize,
    lang: Language,
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
        let text = tx(lang, "(empty file, or still loading)");
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

/// The ASM view of `tab`: rows decoded from `engine.asm.top`, one per row of the window
/// (address, section, bytes, instruction; `db` data in the secondary colour), the search
/// hits and the selection painted, the cursor row reversed. Follow keeps the end of the
/// file on the last row.
fn asm_rows(
    tab: &mut Tab,
    palette: &Palette,
    width: usize,
    height: usize,
    lang: Language,
) -> (Vec<Line<'static>>, usize) {
    tab.height = height;
    tab.top = 0;
    let follow = tab.engine.follow_tail;
    if follow {
        tab.engine.asm_bottom(height);
    }
    let rows = tab.engine.asm_rows(height);
    if rows.is_empty() {
        let text = tx(lang, "(empty file, or still loading)");
        return (
            vec![Line::styled(text, Style::default().fg(palette.dim()))],
            0,
        );
    }
    tab.cursor = if follow {
        rows.len() - 1
    } else {
        tab.cursor.min(rows.len() - 1)
    };
    let engine = &tab.engine;
    let wide = rows.iter().any(|r| r.address > u32::MAX as u64);
    let digits = if wide { 16 } else { 8 };
    // Bytes beyond what the window holds are cut with `…`; narrow windows keep the
    // instruction and lose the bytes first.
    let bytes_width = if width >= digits + 60 { 30 } else { 12 };
    let has_search = !engine.last_searched_query.is_empty();
    let current = engine.current_search_byte();
    let selection = engine.asm.selection.map(|(a, b)| (a.min(b), a.max(b)));
    let mut lines = Vec::with_capacity(rows.len());
    for (i, row) in rows.iter().enumerate() {
        let end = row.offset + row.bytes.len() as u64;
        let hit = has_search && engine.hex_row_matches(row.offset as usize, end as usize);
        let active =
            hit && current.is_some_and(|(o, l)| (o as u64) < end && (o + l) as u64 > row.offset);
        let selected = selection.is_some_and(|(a, b)| row.offset >= a && row.offset <= b);
        let mut bytes = crate::disasm::hex_bytes(&row.bytes);
        if bytes.chars().count() > bytes_width {
            bytes = bytes.chars().take(bytes_width - 1).collect::<String>() + "…";
        }
        // The mnemonic in the text colour, its operands in the accent; `db` data in the
        // secondary colour.
        let (mnemonic, operands) = row.text.split_once(' ').unwrap_or((&row.text, ""));
        let (code, args) = if row.data {
            (row.text.clone(), String::new())
        } else {
            (format!("{mnemonic:<7}"), operands.to_string())
        };
        let text_style = if row.data {
            Style::default().fg(palette.secondary())
        } else {
            Style::default()
        };
        let mut line = Line::from(vec![
            Span::styled(
                format!("{:0digits$X} ", row.address),
                Style::default().fg(palette.dim()),
            ),
            Span::styled(
                format!("{:<9}", row.section.as_deref().unwrap_or("")),
                Style::default().fg(palette.accent()),
            ),
            Span::styled(
                format!("{bytes:<bytes_width$} "),
                Style::default().fg(palette.dim()),
            ),
            Span::styled(code, text_style),
            Span::styled(args, Style::default().fg(palette.accent())),
        ]);
        if active {
            line = line.patch_style(palette.active_hit());
        } else if hit {
            line = line.patch_style(palette.hit());
        } else if selected {
            line = line.patch_style(palette.selection());
        }
        if i == tab.cursor {
            line = line.patch_style(Style::default().add_modifier(Modifier::REVERSED));
        }
        // Sideways scrolling as in the other views.
        if tab.hscroll > 0 {
            let mut left = tab.hscroll;
            let spans: Vec<Span<'static>> = line
                .spans
                .into_iter()
                .filter_map(|s| {
                    let n = s.content.chars().count();
                    if left >= n {
                        left -= n;
                        return None;
                    }
                    let kept: String = s.content.chars().skip(left).collect();
                    left = 0;
                    Some(Span::styled(kept, s.style))
                })
                .collect();
            line = Line::from(spans).style(line.style);
        }
        lines.push(line);
    }
    let drawn = lines.len();
    (lines, drawn)
}

/// Cells of the time delta column, the widest value (`+12:34.567`, `+1d 02:03`) included.
const DELTA_CELLS: usize = 10;

/// The time delta column for this frame: `Some(gap_ms)` when the stream shows it and its
/// lines have times. Asks for the stream to be timed meanwhile (a background scan on a
/// large file), as the window does.
fn time_delta_column(engine: &mut TailEngine, gap_ms: u64) -> Option<u64> {
    if !engine.show_time_delta {
        return None;
    }
    engine.want_timestamps();
    engine.timestamps_usable().then_some(gap_ms)
}

/// A row's time delta cell, right-aligned: blank on continuation lines and lines without
/// a time, `...` until the row is timed, the anchor mark on the anchor, and the accent
/// colour on a gap of at least `gap_ms` (in previous-row mode only, as in the window).
fn time_delta_cell(
    engine: &TailEngine,
    palette: &Palette,
    row: usize,
    gap_ms: u64,
) -> Span<'static> {
    let dim = Style::default().fg(palette.dim());
    let accent = Style::default().fg(palette.accent());
    let (text, style) = match engine.row_time_delta(row) {
        TimeDelta::Blank => (String::new(), dim),
        TimeDelta::Pending => ("...".to_string(), dim),
        TimeDelta::Anchor => ((if palette.ascii { "@" } else { "⚓" }).to_string(), accent),
        TimeDelta::Millis(millis) => {
            let gap = gap_ms > 0
                && engine.time_anchor().is_none()
                && millis >= i64::try_from(gap_ms).unwrap_or(i64::MAX);
            (
                crate::timestamp::format_delta(millis),
                if gap { accent } else { dim },
            )
        }
    };
    let used = unicode_width::UnicodeWidthStr::width(text.as_str());
    let pad = DELTA_CELLS.saturating_sub(used);
    Span::styled(format!("{}{text} ", " ".repeat(pad)), style)
}

/// A byte range of a row after the bytes `start..end` were replaced by `with_len` bytes:
/// before them it stays, after them it moves, across them it covers the new text.
fn shift_range(
    r: std::ops::Range<usize>,
    (start, end): (usize, usize),
    with_len: usize,
) -> std::ops::Range<usize> {
    let moved = |at: usize, inside: usize| {
        if at <= start {
            at
        } else if at >= end {
            at - end + start + with_len
        } else {
            inside
        }
    };
    moved(r.start, start)..moved(r.end, start + with_len)
}

/// The mark after a row's number: `o` an automatic bookmark, `*` a bookmark, `>` a
/// search hit, `<` a time earlier than the line above's (a clock jumping back).
fn row_mark(engine: &TailEngine, line_idx: usize, hit: bool, bookmarked: bool) -> char {
    if bookmarked && engine.is_auto_bookmark(line_idx) {
        'o'
    } else if bookmarked {
        '*'
    } else if hit {
        '>'
    } else if engine.out_of_order_jump(line_idx).is_some() && !engine.timestamps_go_backwards() {
        '<'
    } else if engine.is_piece_start(line_idx) {
        // The first line of a rotation piece (the window draws a dashed line).
        '~'
    } else {
        ' '
    }
}

/// One row of the log view: gutter, collapse badge, then the text in its level colour
/// with the search hits painted over it.
#[allow(clippy::too_many_arguments)]
pub fn render_row(
    engine: &TailEngine,
    palette: &Palette,
    row: usize,
    line_idx: usize,
    (gutter, delta, tag): (usize, Option<u64>, usize),
    query: &str,
    has_search: bool,
    hscroll: usize,
) -> Line<'static> {
    let (hit, active, bookmarked) = engine.row_marks(row, line_idx, has_search);
    let mut spans = Vec::with_capacity(7);
    let gutter_style = if active {
        palette.active_hit()
    } else {
        Style::default().fg(palette.dim())
    };
    let mark = row_mark(engine, line_idx, hit, bookmarked);
    spans.push(Span::styled(
        if gutter == 0 {
            mark.to_string()
        } else {
            format!("{:>gutter$}{mark}", row_number(engine, line_idx))
        },
        gutter_style,
    ));
    if let Some(gap_ms) = delta {
        spans.push(time_delta_cell(engine, palette, row, gap_ms));
    }
    spans.extend(source_tag_cell(engine, line_idx, tag, palette.ascii));
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
    // The leading timestamp in the stream's time display (UTC, local); the colour runs
    // move with the text after it.
    let timed = engine.display_time(text);
    let shown;
    let text = match &timed {
        Some((start, end, with)) => {
            shown = format!("{}{with}{}", &text[..*start], &text[*end..]);
            shown.as_str()
        }
        None => text,
    };
    let moved = |r: std::ops::Range<usize>| match &timed {
        Some((start, end, with)) => shift_range(r, (*start, *end), with.len()),
        None => r,
    };
    // With captures-only rules or quick labels the row is painted span by span, first
    // rule winning per byte, then the labels, then the ANSI colours (see
    // `TailEngine::match_row_spans`); a search hit still wins over all of them.
    if engine.has_span_rules() {
        let painted = match &row {
            Some(r) => engine.match_row_spans(r),
            None => Default::default(),
        };
        let base = match &painted.rest {
            Some(rule) => palette.rule_style(rule),
            None => palette.level_style(engine.level_of(line_idx)),
        };
        let spans_in: Vec<std::ops::Range<usize>> = painted
            .spans
            .iter()
            .map(|s| moved(s.start..s.end))
            .collect();
        let hits = if hit && !query.is_empty() {
            view::hit_ranges(text, query)
        } else {
            Vec::new()
        };
        let segs: Vec<(String, Paint)> = view::segments(text, &hits, &spans_in)
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
                Paint::Run(i) => base.patch(palette.span_style(&painted.spans[i].style)),
                Paint::Hit => hit_style,
            };
            spans.push(Span::styled(s, style));
        }
        let line = Line::from(spans);
        return if engine.is_selected(line_idx) {
            line.style(palette.selection())
        } else {
            line
        };
    }
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
    let run_ranges: Vec<std::ops::Range<usize>> =
        runs.iter().map(|r| moved(r.start..r.end)).collect();
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
    let lang = config.language;
    // `compressed::OpenError` has no `Display`: its debug form is enough for now.
    let shown = path.display();
    match open_target(path, config, None) {
        OpenOutcome::Opened(engine) => Ok(*engine),
        OpenOutcome::OpenEntry(entry) => open_path(&entry, config),
        OpenOutcome::Missing => Err(txf(lang, "{0}: not found", &[&shown])),
        OpenOutcome::Failed { error, .. } => {
            let reason = error
                .system_text(lang)
                .unwrap_or_else(|| format!("{error:?}"));
            Err(format!("{shown}: {reason}"))
        }
        OpenOutcome::EmptyArchive(_) => Err(txf(lang, "{0}: empty archive", &[&shown])),
        OpenOutcome::ChooseEntries { entries, .. } => {
            let names: Vec<&str> = entries
                .iter()
                .filter(|e| e.refusal.is_none())
                .take(5)
                .map(|e| e.name.as_str())
                .collect();
            Err(txf(
                lang,
                "{0}: archive with {1} entries; open one as {2}/<entry> (e.g. {3})",
                &[&shown, &entries.len(), &shown, &names.join(", ")],
            ))
        }
        OpenOutcome::ScanTar { .. } => Err(txf(
            lang,
            "{0}: tar archive; open one entry as {1}/<entry>",
            &[&shown, &shown],
        )),
        OpenOutcome::ListFailed { error, .. } => Err(format!("{shown}: {error}")),
    }
}

/// Standard input as a stream, spooled to disk like the GUI does.
pub fn open_stdin(
    settings: &crate::stdin_source::Settings,
    lang: Language,
) -> Result<TailEngine, String> {
    use crate::stdin_source as stdin;
    let input = stdin::take_stdin().ok_or(tx(lang, "standard input is not available"))?;
    let stream = stdin::StdinStream::start(input, settings, None)
        .map_err(|e| txf(lang, "cannot spool standard input: {0}", &[&e]))?;
    stdin::open_engine(stream, None)
        .map_err(|e| txf(lang, "cannot open standard input: {0}", &[&e]))
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

/// The number a row shows: its line, in a merged view the line in its source, as in the
/// GUI.
fn row_number(engine: &TailEngine, line_idx: usize) -> usize {
    let source_line = match (&engine.merged, &engine.derived) {
        (Some(m), _) => m.source_of(line_idx).map(|(_, l)| l),
        (None, Some(d)) => d.source_line(line_idx),
        (None, None) => None,
    };
    source_line.unwrap_or(line_idx) + 1
}

/// Time a tick gives the merged views to take their sources' lines.
const MERGE_BUDGET: Duration = Duration::from_millis(8);

/// Most cells the source tag of a merged view's rows takes (`[app.log]`).
const SOURCE_TAG_CELLS: usize = 10;

/// The colour of source `i` of a merged view (its tag, its row in the dialog).
fn source_color(i: usize) -> Color {
    const COLORS: [Color; 8] = [
        Color::Cyan,
        Color::Magenta,
        Color::Yellow,
        Color::Green,
        Color::Blue,
        Color::LightRed,
        Color::LightCyan,
        Color::LightMagenta,
    ];
    COLORS[i % COLORS.len()]
}

/// The source tag of merged line `line_idx`, `width` cells plus a space (`[app]`), in its
/// source's colour; `width` 1 draws only a coloured mark. `None` outside a merged view.
fn source_tag_cell(
    engine: &TailEngine,
    line_idx: usize,
    width: usize,
    ascii: bool,
) -> Option<Span<'static>> {
    let m = engine.merged.as_ref()?;
    let (source, _) = m.source_of(line_idx).unwrap_or((usize::MAX, 0));
    let Some(src) = m.sources.get(source) else {
        return Some(Span::raw(" ".repeat(width + 1)));
    };
    let style = Style::default().fg(source_color(source));
    if width <= 1 {
        return Some(Span::styled(if ascii { "| " } else { "▌ " }, style));
    }
    let name: String = {
        let mut out = String::new();
        let mut used = 0;
        for c in src.name.chars() {
            let w = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
            if used + w + 2 > width {
                break;
            }
            out.push(c);
            used += w;
        }
        out
    };
    let text = format!("[{name}]");
    let pad = width.saturating_sub(unicode_width::UnicodeWidthStr::width(text.as_str()));
    Some(Span::styled(format!("{text}{} ", " ".repeat(pad)), style))
}

/// Cells of the source tags of a merged view drawn `cells` wide: the longest name, at
/// most `SOURCE_TAG_CELLS`; a narrow window (under 60 cells) keeps a coloured mark only.
fn source_tag_width(engine: &TailEngine, cells: usize) -> usize {
    let Some(m) = &engine.merged else {
        return 0;
    };
    if cells < 60 {
        return 1;
    }
    m.sources
        .iter()
        .map(|s| unicode_width::UnicodeWidthStr::width(s.name.as_str()) + 2)
        .max()
        .unwrap_or(3)
        .min(SOURCE_TAG_CELLS)
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
        // Rows 3-5 of the screen (under the top bar, the border and the stream bar)
        // hold lines 0-2; the cursor is on line 1.
        assert!(!reversed(3) && reversed(4) && !reversed(5));
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
    fn the_bottom_border_shows_the_cursor_line_and_the_first_column() {
        let body = errors_every_tenth(200);
        let (mut app, _dir) = app_with(&[("app.log", body.as_str())], false);
        app.tabs[0].engine.set_include_filter("ERROR");
        render(&mut app, 80, 20);
        // ERROR lines are 10, 20, ...: row 4 is line 50, numbered as the gutter does.
        app.tabs[0].set_cursor(4);
        let bottom = |app: &mut App, w: u16| {
            let screen = render(app, w, 20);
            screen
                .iter()
                .rev()
                .find(|l| l.contains(" lines"))
                .cloned()
                .unwrap_or_else(|| panic!("{screen:#?}"))
        };
        let line = bottom(&mut app, 80);
        assert!(line.contains("20/200 lines - Ln 50 "), "{line}");
        assert!(!line.contains("Col"), "{line}");
        // Sideways: the first drawn column, gone again with `0`.
        app.apply(Action::ScrollRight);
        app.apply(Action::ScrollRightFast);
        let line = bottom(&mut app, 80);
        assert!(line.contains("- Ln 50 Col 12 "), "{line}");
        // A narrow window leaves out Col first, then Ln, never the counts.
        let line = bottom(&mut app, 36);
        assert!(
            line.contains("20/200 lines - Ln 50") && !line.contains("Col"),
            "{line}"
        );
        let line = bottom(&mut app, 30);
        assert!(
            line.contains("20/200 lines") && !line.contains("Ln"),
            "{line}"
        );
        app.apply(Action::ScrollHome);
        let line = bottom(&mut app, 80);
        assert!(!line.contains("Col"), "{line}");
        // A click on `Ln` opens Go to for the stream.
        let (r, tab) = app.hits.goto_links[0];
        assert_eq!(tab, 0);
        let screen = render(&mut app, 80, 20);
        let cells: String = screen[r.y as usize]
            .chars()
            .skip(r.x as usize)
            .take(r.width as usize)
            .collect();
        assert_eq!(cells, "Ln 50");
        app.on_mouse(click(r.x + 1, r.y));
        assert_eq!(app.prompt.as_ref().map(|p| p.kind), Some(PromptKind::Goto));
        // HEX keeps its byte and row counts.
        app.prompt = None;
        app.apply(Action::ToggleHex);
        let screen = render(&mut app, 80, 20);
        let line = screen.iter().rev().find(|l| l.contains(" bytes,")).unwrap();
        assert!(!line.contains("Ln ") && !line.contains("Col "), "{line}");
        assert!(app.hits.goto_links.is_empty());
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

    #[test]
    fn the_palette_goes_to_a_line_and_runs_commands() {
        use crossterm::event::KeyCode;
        let body = numbered(1500);
        let (mut app, _dir) = app_with(&[("n.log", body.as_str())], false);
        render(&mut app, 80, 20);
        // `:`, a number, Enter: line 1200.
        press(&mut app, KeyCode::Char(':'));
        assert!(app.palette_dialog.is_some());
        for c in "1200".chars() {
            press(&mut app, KeyCode::Char(c));
        }
        let screen = render(&mut app, 80, 20);
        assert!(screen.iter().any(|l| l.contains(": 1200")), "{screen:#?}");
        press(&mut app, KeyCode::Enter);
        app.tick();
        assert!(app.palette_dialog.is_none());
        assert_eq!(app.tabs[0].cursor_line(), Some(1199));
        // A command by name: the HEX view, which the text view entry leaves alone.
        press(&mut app, KeyCode::Char(':'));
        app.on_paste("hex");
        let screen = render(&mut app, 80, 20);
        assert!(screen.iter().any(|l| l.contains("HEX")), "{screen:#?}");
        press(&mut app, KeyCode::Enter);
        assert!(app.tabs[0].is_hex());
        app.run_palette(ActionId::ViewHex, Action::ToggleHex);
        assert!(app.tabs[0].is_hex(), "already in HEX");
        app.run_palette(ActionId::ViewText, Action::ToggleHex);
        assert!(!app.tabs[0].is_hex());
        // Esc closes without running anything.
        press(&mut app, KeyCode::Char(':'));
        press(&mut app, KeyCode::Esc);
        assert!(app.palette_dialog.is_none());
    }

    #[test]
    fn a_quick_label_paints_the_search_text_and_the_rule_editor_removes_it() {
        use crossterm::event::KeyCode;
        let (mut app, _dir) = app_with(&[("a.log", LOG), ("b.log", LOG)], false);
        render(&mut app, 80, 20);
        // No search yet: nothing to label.
        press(&mut app, KeyCode::Char('L'));
        press(&mut app, KeyCode::Char('3'));
        assert!(app.quick_labels.is_empty());
        assert!(app.message.as_deref().unwrap().contains("Search"));
        let prompt = Prompt {
            kind: PromptKind::Search,
            field: TextField::new("slow"),
        };
        app.submit_prompt(prompt);
        press(&mut app, KeyCode::Char('L'));
        press(&mut app, KeyCode::Char('3'));
        assert_eq!(app.message.as_deref(), Some("Label 3: slow"));
        // Every stream has it; CTRL + SHIFT + 3 again removes it, and once more adds it.
        assert_eq!(app.tabs[1].engine.quick_labels().len(), 1);
        let ctrl_shift = KeyModifiers::CONTROL | KeyModifiers::SHIFT;
        app.on_key(crossterm::event::KeyEvent::new(
            KeyCode::Char('3'),
            ctrl_shift,
        ));
        assert!(app.tabs[0].engine.quick_labels().is_empty());
        app.on_key(crossterm::event::KeyEvent::new(
            KeyCode::Char('3'),
            ctrl_shift,
        ));
        // Without the search the label paints the text in its colours.
        press(&mut app, KeyCode::Esc);
        let mut terminal = Terminal::new(TestBackend::new(80, 20)).unwrap();
        app.tick();
        terminal.draw(|f| app.draw(f)).unwrap();
        let buf = terminal.backend().buffer();
        let lines = buffer_text(buf);
        let (y, row) = lines
            .iter()
            .enumerate()
            .find(|(_, l)| l.contains("slow disk"))
            .unwrap();
        let x = row[..row.find("slow").unwrap()].chars().count();
        assert_eq!(
            buf[(x as u16, y as u16)].bg,
            app.palette.label_style(3).bg.unwrap()
        );
        // The rule editor lists it; Tab then d removes it from every stream.
        app.apply(Action::EditRules);
        let screen = render(&mut app, 80, 20);
        assert!(
            screen.iter().any(|l| l.contains("Quick labels")),
            "{screen:#?}"
        );
        press(&mut app, KeyCode::Tab);
        press(&mut app, KeyCode::Char('d'));
        assert!(app.quick_labels.is_empty());
        assert!(app.tabs[1].engine.quick_labels().is_empty());
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

    /// push rbp; mov rbp,rsp; sub rsp,10h; mov eax,0; leave; ret, `n` times.
    fn asm_app(n: usize) -> (App, tempfile::TempDir) {
        let code: [u8; 15] = [
            0x55, 0x48, 0x89, 0xE5, 0x48, 0x83, 0xEC, 0x10, 0xB8, 0x00, 0x00, 0x00, 0x00, 0xC9,
            0xC3,
        ];
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("code.bin");
        std::fs::write(&path, code.repeat(n)).unwrap();
        let tabs = vec![Tab::new(TailEngine::open(&path).unwrap())];
        let palette = Palette::new(CyberTheme::Tron, ColorDepth::TrueColor, false);
        (App::new(tabs, palette), dir)
    }

    #[test]
    fn d_opens_the_asm_view_which_moves_by_instructions() {
        let (mut app, _dir) = asm_app(40);
        render(&mut app, 80, 20);
        app.apply(Action::Top);
        assert!(app.tabs[0].is_hex(), "a binary file opens in HEX");
        // From the HEX cursor row (the first: offset 0, so the start of the file).
        app.apply(Action::ToggleAsm);
        let screen = render(&mut app, 80, 20);
        assert!(app.tabs[0].is_asm());
        for text in [
            "push   rbp",
            "48 89 E5",
            "mov    rbp,rsp",
            "ASM x86-64",
            "[ASM]",
        ] {
            assert!(
                screen.iter().any(|l| l.contains(text)),
                "{text}: {screen:#?}"
            );
        }
        app.apply(Action::LineDown);
        app.apply(Action::LineDown);
        assert_eq!(app.tabs[0].cursor, 2);
        app.apply(Action::PageDown);
        assert!(app.tabs[0].engine.asm.top > 0, "a page moves the view");
        app.apply(Action::Top);
        assert_eq!(app.tabs[0].engine.asm.top, 0);
        // Shift+Down selects; y copies the rows as address, bytes, instruction.
        app.apply(Action::SelectDown);
        let text = app.tabs[0].engine.asm_selection_text().unwrap();
        assert_eq!(text.lines().count(), 2, "{text}");
        assert!(text.lines().nth(1).unwrap().ends_with("mov rbp,rsp"));
        // Go to an offset, then D changes the architecture shown in the border.
        go_to(&mut app, "0x10");
        assert_eq!(app.tabs[0].engine.asm.top, 16);
        app.apply(Action::CycleArch);
        let screen = render(&mut app, 80, 20);
        assert!(
            screen.iter().any(|l| l.contains("ASM x86-16")),
            "{screen:#?}"
        );
        assert!(app.tabs[0].engine.asm_dirty, "saved as disasm_arch=");
        // d returns to HEX on the same bytes, h to the lines.
        app.apply(Action::ToggleAsm);
        assert!(app.tabs[0].is_hex() && !app.tabs[0].is_asm());
        app.apply(Action::ToggleAsm);
        app.apply(Action::ToggleHex);
        assert!(!app.tabs[0].is_hex());
    }

    #[test]
    fn the_asm_view_fits_narrow_and_wide_windows_and_follows() {
        let (mut app, _dir) = asm_app(400);
        render(&mut app, 80, 20);
        app.apply(Action::ToggleAsm);
        app.apply(Action::Bottom);
        for width in [80, 200] {
            let screen = render(&mut app, width, 20);
            assert!(screen.iter().all(|l| l.chars().count() == width as usize));
            assert!(
                screen.iter().any(|l| l.contains("C3") && l.contains("ret")),
                "{width}: {screen:#?}"
            );
        }
        // Follow keeps the last instruction on the last row of the window.
        let height = app.tabs[0].height;
        let rows = app.tabs[0].engine.asm_rows(height);
        let last = rows.last().unwrap();
        assert_eq!(last.offset + 1, app.tabs[0].engine.file_size);
        app.apply(Action::LineUp);
        assert!(!app.tabs[0].engine.follow_tail);
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
    fn r_lists_the_rotation_pieces_and_goes_to_one() {
        use crossterm::event::KeyCode;
        let dir = tempfile::tempdir().unwrap();
        let spool = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("app.log.1"), "one-a\none-b\n").unwrap();
        std::fs::write(dir.path().join("app.log"), "live-a\n").unwrap();
        let settings = crate::stdin_source::Settings::from_config(Some(spool.path()), 64);
        let engine =
            crate::rotation::open_engine(&dir.path().join("app.log"), &settings, None).unwrap();
        let palette = Palette::new(CyberTheme::Tron, ColorDepth::TrueColor, true);
        let mut app = App::new(vec![Tab::new(engine)], palette);
        let started = std::time::Instant::now();
        while app.tabs[0].engine.total_lines() < 3 || app.tabs[0].engine.rotation_pieces().len() < 2
        {
            render(&mut app, 100, 20);
            assert!(started.elapsed() < std::time::Duration::from_secs(10));
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        // The first line of the live file carries the piece mark.
        let screen = render(&mut app, 100, 20).join("\n");
        assert!(screen.lines().any(|l| l.contains("3~live-a")), "{screen}");
        keys(&mut app, "R");
        assert_eq!(app.piece_menu, Some(0));
        let screen = render(&mut app, 100, 20).join("\n");
        assert!(
            screen.contains("app.log.1") && screen.contains("Go to piece"),
            "{screen}"
        );
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.piece_menu, None);
        assert_eq!(app.tabs[0].cursor_line(), Some(2));
        assert!(!app.tabs[0].engine.follow_tail);
    }

    #[test]
    fn append_needs_a_clipboard_stream() {
        let (mut app, _dir) = app_with(&[("a.log", LOG)], true);
        app.apply(Action::PasteAppend);
        assert_eq!(app.message.as_deref(), Some("Not a clipboard stream"));
    }

    #[test]
    fn the_time_column_is_pinned_and_stays_while_scrolling_sideways() {
        use crossterm::event::KeyCode;
        let csv = "level,proto,timestamp,message\nerror,tcp,10:00,boom\ninfo,udp,10:01,fine\n";
        let (mut app, _dir) = app_with(&[("export.csv", csv)], true);
        render(&mut app, 120, 20);
        assert_eq!(app.tabs[0].engine.pinned_columns(), ["timestamp"]);
        let screen = render(&mut app, 120, 20).join("\n");
        assert!(
            screen.lines().any(|l| l.contains("10:00     error tcp")),
            "the pinned column first: {screen}"
        );
        // Scrolled sideways past level: the time stays, level is gone.
        app.tabs[0].hscroll = 6;
        let screen = render(&mut app, 120, 20).join("\n");
        let row = screen
            .lines()
            .find(|l| l.contains("10:00"))
            .unwrap_or_else(|| panic!("{screen}"));
        assert!(
            row.contains("10:00     tcp") && !row.contains("error"),
            "{row}"
        );
        // f in the columns list unpins it.
        app.tabs[0].hscroll = 0;
        app.columns_menu = Some(0);
        render(&mut app, 120, 20);
        keys(&mut app, "f");
        assert_eq!(app.tabs[0].engine.chosen_pins(), Some(&[][..]));
        assert!(app.tabs[0].engine.pinned_columns().is_empty());
        press(&mut app, KeyCode::Esc);
    }

    #[test]
    fn c_shows_the_fields_as_columns_and_the_list_hides_them() {
        use crossterm::event::KeyCode;
        let csv = "level,proto,timestamp,message\nerror,tcp,10:00,boom\ninfo,udp,10:01,fine\n";
        let (mut app, _dir) = app_with(&[("export.csv", csv)], true);
        // The columns in their own order here (pinning has its own test).
        app.tabs[0].engine.pin_time_default = false;
        render(&mut app, 120, 20);
        assert!(app.tabs[0].engine.field_parser().is_some());
        // A CSV opens in the column view; C turns it off and on.
        assert!(app.tabs[0].engine.columns_shown());
        keys(&mut app, "C");
        assert!(!app.tabs[0].engine.columns_shown());
        keys(&mut app, "C");
        assert!(app.tabs[0].engine.columns_shown());
        let screen = render(&mut app, 120, 20).join("\n");
        let header = screen
            .lines()
            .find(|l| l.contains("level") && l.contains("proto") && l.contains("timestamp"))
            .unwrap_or_else(|| panic!("no header in {screen}"));
        assert!(header.contains("message"), "{header}");
        assert!(
            screen.lines().any(|l| l.contains("error tcp   10:00")),
            "{screen}"
        );

        // The list: untick proto and timestamp; the window keeps level only.
        let (r, _, _) = *app
            .hits
            .bar_chips
            .iter()
            .find(|(_, _, c)| *c == crate::tui::bars::BarChip::Fields)
            .expect("the Fields chip");
        app.on_mouse(click(r.x + 1, r.y));
        assert_eq!(app.columns_menu, Some(0));
        render(&mut app, 120, 20);
        press(&mut app, KeyCode::Down);
        keys(&mut app, " ");
        assert_eq!(
            app.tabs[0].engine.chosen_field_columns(),
            ["level", "timestamp"]
        );
        // The row followed proto to the unticked part of the list; timestamp is row 1.
        press(&mut app, KeyCode::Up);
        keys(&mut app, " ");
        assert_eq!(app.tabs[0].engine.chosen_field_columns(), ["level"]);
        app.columns_menu = Some(0);
        keys(&mut app, " ");
        assert_eq!(
            app.tabs[0].engine.chosen_field_columns(),
            ["level"],
            "the last column stays"
        );
        // The header names level only (proto joins the message as `proto=tcp`); wide
        // enough for the list not to cover the rows.
        let screen = render(&mut app, 160, 20).join("\n");
        assert!(
            screen.lines().any(|l| l.contains("level message")),
            "{screen}"
        );
        assert!(
            screen.lines().any(|l| l.contains("boom proto=tcp")),
            "{screen}"
        );
        // Tick proto again, then Shift+Up moves it before level.
        let rows = App::column_menu_rows(&app.tabs[0].engine);
        let proto = rows.iter().position(|r| r.0 == "proto").unwrap();
        app.columns_menu = Some(proto);
        keys(&mut app, " ");
        assert_eq!(
            app.tabs[0].engine.chosen_field_columns(),
            ["level", "proto"]
        );
        let shift_up = crossterm::event::KeyEvent::new(KeyCode::Up, KeyModifiers::SHIFT);
        app.on_key(shift_up);
        assert_eq!(
            app.tabs[0].engine.chosen_field_columns(),
            ["proto", "level"]
        );
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.columns_menu, None);
        // The same keys the window saves.
        let entry = crate::workspace::stream_entry(&app.tabs[0].engine);
        assert!(entry.fields_view);
        assert_eq!(entry.fields_columns, ["proto", "level"]);
    }

    #[test]
    fn a_line_without_fields_runs_across_the_columns() {
        let log = "{\"level\":\"info\",\"msg\":\"a\"}\nplain text here\n{\"level\":\"warn\",\"msg\":\"b\"}\n";
        let (mut app, _dir) = app_with(&[("app.json", log)], true);
        render(&mut app, 100, 16);
        app.tabs[0]
            .engine
            .set_field_choice(crate::fields::ParserChoice::Json);
        app.tabs[0].engine.set_fields_view(true);
        let screen = render(&mut app, 100, 16).join("\n");
        assert!(screen.contains("plain text here"), "{screen}");
        assert!(screen
            .lines()
            .any(|l| l.contains("warn") && l.contains("b")));
        // Without fields, C says why and changes nothing.
        let (mut plain, _d) = app_with(&[("p.log", "just text\n")], true);
        render(&mut plain, 80, 10);
        keys(&mut plain, "C");
        assert!(!plain.tabs[0].engine.fields_view());
        assert!(plain.message.is_some());
    }

    #[test]
    fn the_time_range_dialog_checks_both_sides_and_esc_keeps_the_range() {
        use crossterm::event::KeyCode;
        let (mut app, _dir) = app_with(&[("t.log", LOG)], false);
        render(&mut app, 80, 20);
        app.apply(Action::TimeRange);
        keys(&mut app, "10:00:01");
        press(&mut app, KeyCode::Down);
        keys(&mut app, "yesterday-ish");
        let screen = render(&mut app, 80, 30);
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

    #[test]
    fn o_browses_the_folder_of_the_stream_with_keys_and_mouse() {
        use crossterm::event::KeyCode;
        let (mut app, dir) = app_with(&[("a.log", LOG)], false);
        std::fs::create_dir(dir.path().join("old")).unwrap();
        std::fs::write(dir.path().join("old").join("c.log"), "c\n").unwrap();
        std::fs::write(dir.path().join("b.log"), "b\n").unwrap();
        app.apply(Action::OpenFile);
        let screen = render(&mut app, 100, 30);
        assert!(screen.iter().any(|l| l.contains(" Open ")), "{screen:#?}");
        assert!(screen
            .iter()
            .any(|l| l.contains("<DIR>") && l.contains("old")));
        // .., old, a.log, b.log: Down twice then Enter enters old; Enter on c.log.
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Enter);
        assert_eq!(
            app.browser.as_ref().unwrap().dir.as_deref(),
            Some(dir.path().join("old").as_path())
        );
        // Backspace on the empty name goes back up, onto the folder it left.
        press(&mut app, KeyCode::Backspace);
        let b = app.browser.as_ref().unwrap();
        assert_eq!(b.chosen().unwrap().name, "old");
        // Typing filters (a.log is then selected); a click selects b.log, a second
        // click opens it.
        app.on_paste(".log");
        assert_eq!(
            app.browser.as_ref().unwrap().chosen().unwrap().name,
            "a.log"
        );
        let screen = render(&mut app, 100, 30);
        let y = screen
            .iter()
            .position(|l| l.contains("b.log") && l.contains(" B "))
            .unwrap();
        assert!(app.on_mouse(click(30, y as u16)));
        assert!(app.browser.is_some());
        assert!(app.on_mouse(click(30, y as u16)));
        assert!(app.browser.is_none());
        assert_eq!(app.tabs.len(), 2);
        assert_eq!(app.tabs[1].title, "b.log");
        // Esc closes the dialog.
        app.apply(Action::OpenFile);
        press(&mut app, KeyCode::Esc);
        assert!(app.browser.is_none());
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
    fn the_help_is_a_menu_the_cursor_walks_and_enter_runs() {
        use crossterm::event::KeyCode;
        let (mut app, _dir) = app_with(&[("a.log", LOG)], false);
        app.apply(Action::ToggleHelp);
        let tall = render(&mut app, 100, 60);
        for key in ["F3", "Ctrl+G", "Shift+O", "Shift+T", "?  F1", "HEX", "ANSI"] {
            assert!(tall.iter().any(|l| l.contains(key)), "{key}: {tall:#?}");
        }
        // The cursor starts on the first entry with a command and skips the others.
        assert_eq!(HELP[app.help_sel].2, Some(Action::Bottom));
        press(&mut app, KeyCode::Up);
        assert_eq!(
            HELP[app.help_sel].2,
            Some(Action::Quit),
            "wraps to the last"
        );
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Down);
        assert_eq!(HELP[app.help_sel].2, Some(Action::ToggleFollow));

        // Two columns on a wide screen; Right jumps to the other column.
        let wide = render(&mut app, 170, 30);
        assert!(
            wide.iter().any(|l| l.contains("move the cursor one row"))
                && wide.iter().any(|l| l.contains("next theme")),
            "two columns show every key: {wide:#?}"
        );
        press(&mut app, KeyCode::Right);
        assert!(app.help_sel >= app.help_half);

        // A short screen scrolls to keep the cursor visible.
        press(&mut app, KeyCode::End);
        let short = render(&mut app, 80, 24);
        assert!(short.iter().any(|l| l.contains("quit")), "{short:#?}");
        assert!(!short.iter().any(|l| l.contains("move the cursor one row")));

        // Enter runs the chosen command: the search dialog opens.
        let search = HELP
            .iter()
            .position(|e| e.2 == Some(Action::StartSearch))
            .unwrap();
        app.help_sel = search;
        press(&mut app, KeyCode::Enter);
        assert!(!app.show_help);
        assert_eq!(
            app.prompt.as_ref().map(|p| p.kind),
            Some(PromptKind::Search)
        );
        app.prompt = None;

        // A click on an entry runs it; another key closes the help and acts.
        app.apply(Action::ToggleHelp);
        render(&mut app, 100, 60);
        let (rect, _) = *app
            .hits
            .list_items
            .iter()
            .find(|(_, i)| HELP[*i].2 == Some(Action::TimeRange))
            .unwrap();
        app.on_mouse(click(rect.x + 1, rect.y));
        assert!(!app.show_help && app.time_range.is_some());
        app.time_range = None;
        app.apply(Action::ToggleHelp);
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

    #[cfg(unix)]
    #[test]
    fn a_system_source_opens_from_the_palette_prompt_with_no_stream_open() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("fake-docker");
        std::fs::write(&fake, "#!/bin/sh\necho '2026-10-23T06:00:00.000Z hello'\n").unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let ini = dir.path().join("fasttail.ini");
        crate::config::FastTailConfig {
            docker_command: fake.to_string_lossy().into_owned(),
            spool_dir: Some(dir.path().join("spool")),
            ..Default::default()
        }
        .save_to(&ini)
        .unwrap();
        let mut app = app_over(&ini);
        assert!(app.tabs.is_empty());
        app.apply(Action::OpenSystem);
        assert_eq!(
            app.prompt.as_ref().map(|p| p.kind),
            Some(PromptKind::System)
        );
        let mut prompt = app.prompt.take().unwrap();
        prompt.field = TextField::new("docker://api");
        app.submit_prompt(prompt);
        assert_eq!(app.tabs.len(), 1, "{:?}", app.message);
        assert_eq!(stream_title(&app.tabs[0].engine), "docker: api");
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
        // Inside the window (right of the text, under its two bars), and the theme's
        // text colour.
        assert_eq!(buf[(70, 6)].bg, panel);
        assert_eq!(buf[(70, 6)].fg, app.palette.window().fg.unwrap());

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
        assert_eq!(app.palette.theme, CyberTheme::Commander, "the blue classic");
        app.apply(Action::CycleTheme);
        assert_eq!(app.palette.theme, CyberTheme::Tron, "back to the first");
    }

    #[test]
    fn settings_moves_resizes_and_closes_with_its_x() {
        use crossterm::event::KeyCode;
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.log");
        std::fs::write(&a, LOG).unwrap();
        let ini = dir.path().join("fasttail.ini");
        crate::config::FastTailConfig {
            open_files: vec![a],
            ..Default::default()
        }
        .save_to(&ini)
        .unwrap();
        let saved = std::fs::read(&ini).unwrap();
        let mut app = app_over(&ini);
        app.apply(Action::Settings);
        let screen = render(&mut app, 200, 110);
        let d = app.hits.dialog.unwrap();
        let start = d.outer;
        let x = d.close.expect("[x]");
        assert_eq!(x.y, start.y);
        assert!(screen[x.y as usize].contains("[x]"), "{screen:#?}");
        // The title row moves it; released outside, it stays open where it was left.
        app.on_mouse(click(start.x + 5, start.y));
        app.on_mouse(drag(start.x + 35, start.y + 5));
        app.on_mouse(release(199, 109));
        assert!(app.settings_form.is_some(), "a drag never closes it");
        render(&mut app, 200, 110);
        let moved = app.hits.dialog.unwrap().outer;
        assert_eq!(
            (moved.x, moved.y, moved.width, moved.height),
            (start.x + 30, start.y + 5, start.width, start.height)
        );
        // It keeps the keyboard.
        let focus = app.settings_form.as_ref().unwrap().focus;
        press(&mut app, KeyCode::Down);
        assert_ne!(app.settings_form.as_ref().unwrap().focus, focus);
        // The bottom-right corner resizes it, not below its minimum.
        app.on_mouse(click(moved.right() - 1, moved.bottom() - 1));
        app.on_mouse(drag(moved.x + 2, moved.y + 2));
        app.on_mouse(release(moved.x + 2, moved.y + 2));
        render(&mut app, 200, 110);
        let small = app.hits.dialog.unwrap().outer;
        assert_eq!(
            (small.x, small.y, small.width, small.height),
            (moved.x, moved.y, 40, 8)
        );
        // [x] closes as Esc: the theme it opened with is back, nothing written.
        let theme = app.palette.theme;
        let f = app.settings_form.as_mut().unwrap();
        f.focus = f.fields.iter().position(|x| x.label == "Theme").unwrap();
        press(&mut app, KeyCode::Right);
        assert_ne!(app.palette.theme, theme);
        render(&mut app, 200, 110);
        let x = app.hits.dialog.unwrap().close.unwrap();
        app.on_mouse(click(x.x + 1, x.y));
        assert!(app.settings_form.is_none());
        assert_eq!(app.palette.theme, theme);
        assert_eq!(std::fs::read(&ini).unwrap(), saved);
        // Reopened where it was left; kept inside a smaller terminal.
        app.apply(Action::Settings);
        render(&mut app, 200, 110);
        assert_eq!(app.hits.dialog.unwrap().outer, small);
        render(&mut app, 80, 24);
        let fit = app.hits.dialog.unwrap().outer;
        assert!(fit.right() <= 80 && fit.bottom() <= 24, "{fit:?}");
        // A double click on the title: centred at the default size again.
        render(&mut app, 200, 110);
        let t = app.hits.dialog.unwrap().outer;
        app.on_mouse(click(t.x + 5, t.y));
        app.on_mouse(release(t.x + 5, t.y));
        app.on_mouse(click(t.x + 5, t.y));
        app.on_mouse(release(t.x + 5, t.y));
        render(&mut app, 200, 110);
        assert_eq!(app.hits.dialog.unwrap().outer, start);
    }

    #[test]
    fn the_keys_dialog_lays_out_its_columns_for_its_own_width() {
        let (mut app, _dir) = app_with(&[("a.log", LOG)], false);
        let columns = |app: &App| {
            let mut xs: Vec<u16> = app.hits.list_items.iter().map(|(r, _)| r.x).collect();
            xs.sort_unstable();
            xs.dedup();
            xs.len()
        };
        app.apply(Action::ToggleHelp);
        render(&mut app, 200, 30);
        let start = app.hits.dialog.unwrap().outer;
        assert_eq!(columns(&app), 3, "{start:?}");
        // The right border, dragged left to 92 columns: two columns of 44.
        app.on_mouse(click(start.right() - 1, start.y + 2));
        app.on_mouse(drag(start.x + 91, start.y + 2));
        app.on_mouse(release(start.x + 91, start.y + 2));
        render(&mut app, 200, 30);
        let d = app.hits.dialog.unwrap().outer;
        assert_eq!(d.width, 92);
        assert_eq!(columns(&app), 2);
        for (r, _) in &app.hits.list_items {
            assert!(r.right() < d.right() && r.width >= 44, "{r:?} in {d:?}");
        }
        assert!(app.hits.list_items.iter().any(|(_, i)| *i == app.help_sel));
        // Moved to the top-left corner, closed and reopened: where it was left.
        app.on_mouse(click(d.x + 5, d.y));
        app.on_mouse(drag(0, 0));
        app.on_mouse(release(0, 0));
        press(&mut app, crossterm::event::KeyCode::Esc);
        assert!(!app.show_help);
        app.apply(Action::ToggleHelp);
        render(&mut app, 200, 30);
        let back = app.hits.dialog.unwrap().outer;
        assert_eq!(
            (back.x, back.y, back.width, back.height),
            (0, 0, 92, d.height)
        );
        // Its [x] closes it.
        let x = app.hits.dialog.unwrap().close.unwrap();
        app.on_mouse(click(x.x, x.y));
        assert!(!app.show_help);
    }

    #[test]
    fn a_line_from_the_command_line_takes_the_cursor_and_its_notice_shows() {
        let body = numbered(300);
        let (mut app, _dir) = app_with(&[("a.log", body.as_str())], false);
        app.tabs[0].engine.open_at_line(120, Language::En);
        render(&mut app, 80, 20);
        assert_eq!(app.tabs[0].cursor_line(), Some(119));
        assert!(!app.tabs[0].engine.follow_tail);
        app.tabs[0].engine.open_at_line(900, Language::En);
        render(&mut app, 80, 20);
        assert_eq!(app.tabs[0].cursor_line(), Some(299));
        assert_eq!(
            app.message.as_deref(),
            Some("Line 900 is past the end (300 lines)")
        );
    }

    #[test]
    fn h_hides_the_selection_and_the_chip_lists_it_back() {
        use crossterm::event::KeyCode;
        let body = numbered(300);
        let (mut app, _dir) = app_with(&[("a.log", body.as_str())], false);
        render(&mut app, 120, 30);
        // The cursor row alone.
        app.tabs[0].set_cursor(4);
        press(&mut app, KeyCode::Char('H'));
        assert_eq!(app.tabs[0].engine.hidden_ranges(), &[(4, 4)]);
        assert_eq!(app.message.as_deref(), Some("1 lines hidden"));
        // A selection of lines 10..=19 (0-based).
        app.tabs[0].engine.select_row(10);
        app.tabs[0].engine.extend_selection_to(19);
        press(&mut app, KeyCode::Char('H'));
        assert_eq!(app.tabs[0].engine.hidden_count(), 11);
        let screen = render(&mut app, 120, 30);
        assert!(
            // The emoji takes two cells.
            screen.iter().any(|l| l.contains(" 11 hidden]")),
            "{screen:#?}"
        );
        // The chip opens the list; Enter shows the first range, `a` the rest.
        let (r, _, _) = *app
            .hits
            .bar_chips
            .iter()
            .find(|(_, _, c)| *c == crate::tui::bars::BarChip::Hidden)
            .unwrap();
        app.on_mouse(click(r.x + 1, r.y));
        assert_eq!(app.hidden_menu, Some(0));
        let screen = render(&mut app, 120, 30);
        assert!(
            screen.iter().any(|l| l.contains("Hidden lines")),
            "{screen:#?}"
        );
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.tabs[0].engine.hidden_ranges(), &[(10, 19)]);
        press(&mut app, KeyCode::Char('a'));
        assert!(app.tabs[0].engine.hidden_ranges().is_empty());
        assert_eq!(app.hidden_menu, None, "closed with the last range");
    }

    #[test]
    fn the_level_bytes_chip_sets_the_streams_window() {
        let late = format!(
            "{:<150}ERROR boom\n{:<150}INFO fine\n",
            "host=a pid=1", "host=a pid=1"
        );
        let (mut app, _dir) = app_with(&[("a.log", late.as_str())], false);
        let screen = render(&mut app, 120, 30);
        assert!(screen.iter().any(|l| l.contains("[≤96 B]")), "{screen:#?}");
        assert_eq!(app.tabs[0].engine.level_of(0), LogLevel::Unknown);
        let (r, _, _) = *app
            .hits
            .bar_chips
            .iter()
            .find(|(_, _, c)| *c == crate::tui::bars::BarChip::LevelBytes)
            .unwrap();
        app.on_mouse(click(r.x + 1, r.y));
        assert_eq!(
            app.prompt.as_ref().map(|p| p.kind),
            Some(PromptKind::LevelBytes)
        );
        // Out of range: refused, nothing changes.
        app.prompt.as_mut().unwrap().field = TextField::new("8");
        let prompt = app.prompt.take().unwrap();
        app.submit_prompt(prompt);
        assert!(app.message.as_deref().unwrap().contains("16 to 4096"));
        assert_eq!(app.tabs[0].engine.level_override(), None);
        // 256: the level at byte 150 is found, the chip shows the stream's value.
        app.open_prompt(PromptKind::LevelBytes);
        app.prompt.as_mut().unwrap().field = TextField::new("256");
        let prompt = app.prompt.take().unwrap();
        app.submit_prompt(prompt);
        assert_eq!(app.tabs[0].engine.level_override(), Some(256));
        assert_eq!(app.tabs[0].engine.level_of(0), LogLevel::Error);
        let screen = render(&mut app, 120, 30);
        assert!(screen.iter().any(|l| l.contains("[≤256 B]")), "{screen:#?}");
        // Empty: back to the setting.
        app.open_prompt(PromptKind::LevelBytes);
        assert_eq!(app.prompt.as_ref().unwrap().field.text(), "256");
        app.prompt.as_mut().unwrap().field = TextField::new("");
        let prompt = app.prompt.take().unwrap();
        app.submit_prompt(prompt);
        assert_eq!(app.tabs[0].engine.level_window(), 96);
    }

    #[test]
    fn the_settings_dialog_applies_saves_and_refuses_a_value_out_of_range() {
        use crossterm::event::KeyCode;
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.log");
        std::fs::write(&a, LOG).unwrap();
        let ini = dir.path().join("fasttail.ini");
        crate::config::FastTailConfig {
            open_files: vec![a],
            ..Default::default()
        }
        .save_to(&ini)
        .unwrap();
        let mut app = app_over(&ini);
        app.apply(Action::Settings);
        let screen = render(&mut app, 90, 40);
        assert!(
            screen.iter().any(|l| l.contains("Performance and refresh")),
            "{screen:#?}"
        );

        // Theme: one to the right; line numbers in new streams: off; Poll interval: out
        // of range.
        let focus = |app: &mut App, label: &str| {
            let f = app.settings_form.as_mut().unwrap();
            f.focus = f.fields.iter().position(|x| x.label == label).unwrap();
        };
        focus(&mut app, "Theme");
        press(&mut app, KeyCode::Right);
        assert_eq!(
            app.palette.theme,
            CyberTheme::Matrix,
            "at once, as in the GUI"
        );
        focus(&mut app, "Line numbers in new streams");
        press(&mut app, KeyCode::Char(' '));
        focus(&mut app, "Poll interval (ms)");
        app.on_key(crossterm::event::KeyEvent::new(
            KeyCode::Char('u'),
            KeyModifiers::CONTROL,
        ));
        keys(&mut app, "7");
        press(&mut app, KeyCode::Enter);
        assert!(app.settings_form.is_some(), "kept open");
        assert!(app.message.as_deref().unwrap().contains("50 to 5000"));
        assert_eq!(
            app.idle_poll,
            Duration::from_millis(250),
            "the invalid field waits"
        );

        keys(&mut app, "00");
        press(&mut app, KeyCode::Enter);
        assert!(app.settings_form.is_none());
        assert_eq!(app.palette.theme, CyberTheme::Matrix);
        assert_eq!(app.idle_poll, Duration::from_millis(700));
        assert!(
            app.tabs[0].engine.show_line_numbers,
            "the open stream keeps its own"
        );
        let saved = crate::tui::workspace::Settings::read(&ini).config;
        assert_eq!(saved.theme, CyberTheme::Matrix);
        assert_eq!(saved.poll_interval_ms, 700);
        assert!(!saved.show_line_numbers);
        // A stream opened now takes the default.
        let b = dir.path().join("b.log");
        std::fs::write(&b, LOG).unwrap();
        app.open_file(&b);
        assert!(!app.tabs[1].engine.show_line_numbers);
        // `#` switches the focused stream's own.
        app.focus_tab(0);
        press(&mut app, KeyCode::Char('#'));
        assert_eq!(app.message.as_deref(), Some("Line numbers off"));
        let screen = render(&mut app, 90, 20);
        assert!(
            screen.iter().any(|l| l.starts_with("\u{2551} 2026-09-28")),
            "no line numbers, only the mark column: {screen:#?}"
        );

        // Esc goes back to the values Settings opened with.
        app.apply(Action::Settings);
        focus(&mut app, "Theme");
        press(&mut app, KeyCode::Right);
        assert_eq!(app.palette.theme, CyberTheme::Blade);
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.palette.theme, CyberTheme::Matrix);
        assert_eq!(
            app.settings.as_ref().unwrap().config.theme,
            CyberTheme::Matrix
        );
    }

    #[test]
    fn the_time_range_calendar_picks_a_day_and_the_time_steps() {
        use crossterm::event::KeyCode;
        let (mut app, _dir) = app_with(&[("t.log", LOG)], false);
        app.tick();
        app.apply(Action::TimeRange);
        // From: a typed day, Tab to its calendar (on that day), one day on, Space picks.
        keys(&mut app, "2026-09-28");
        press(&mut app, KeyCode::Tab);
        let d = app.time_range.as_ref().unwrap();
        assert_eq!(d.zone, RangeZone::Calendar);
        assert_eq!(d.calendar.title(Language::En), "September 2026");
        let first = d.calendar.cursor;
        press(&mut app, KeyCode::Right);
        press(&mut app, KeyCode::Char(' '));
        assert_eq!(app.time_range.as_ref().unwrap().from.text(), "2026-09-29");
        // Its time: Tab, hours up twice, then the minutes up once.
        press(&mut app, KeyCode::Tab);
        press(&mut app, KeyCode::Up);
        press(&mut app, KeyCode::Up);
        press(&mut app, KeyCode::Right);
        press(&mut app, KeyCode::Up);
        assert_eq!(
            app.time_range.as_ref().unwrap().from.text(),
            "2026-09-29 02:01:00"
        );
        let screen = render(&mut app, 80, 30);
        assert!(
            screen.iter().any(|l| l.contains("Mo Tu We Th Fr Sa Su")),
            "{screen:#?}"
        );
        assert!(screen
            .iter()
            .any(|l| l.contains("Time") && l.contains("02:01")));

        // Tab on reaches To; a click on the From field goes back to it, and a click on
        // a day picks it for that side.
        press(&mut app, KeyCode::Tab);
        assert!(app.time_range.as_ref().unwrap().on_to);
        let (field, _) = *app
            .hits
            .list_items
            .iter()
            .find(|(_, i)| *i == RANGE_FROM)
            .unwrap();
        app.on_mouse(click(field.x + 8, field.y));
        let d = app.time_range.as_ref().unwrap();
        assert!(!d.on_to && d.zone == RangeZone::Field, "back on From");
        render(&mut app, 80, 30);
        let (day, _) = *app
            .hits
            .list_items
            .iter()
            .find(|(_, i)| *i == RANGE_DAY + first as usize)
            .unwrap();
        app.on_mouse(click(day.x, day.y));
        assert_eq!(
            app.time_range.as_ref().unwrap().from.text(),
            "2026-09-28 02:01:00",
            "the day changes, the time stays"
        );
    }

    #[test]
    fn the_windows_have_the_guis_stream_bar_and_filter_row_which_click() {
        let (mut app, _dir) = app_with(&[("a.log", LOG)], false);
        let screen = render(&mut app, 120, 30);
        let bar = screen
            .iter()
            .find(|l| l.contains("[x] Follow"))
            .expect("stream bar");
        for chip in [
            "[x] Auto-update",
            "[TXT]",
            "[# 123]",
            "[UTF-8]",
            "[ANSI auto]",
            "[× off]",
            "[± 0]",
        ] {
            assert!(bar.contains(chip), "{chip}: {bar}");
        }
        let filters = screen
            .iter()
            .find(|l| l.contains("Include"))
            .expect("filter row");
        for chip in ["Exclude", "[Aa]", "[.*]", "[All levels]", "[Presets]"] {
            assert!(filters.contains(chip), "{chip}: {filters}");
        }
        let click_chip = |app: &mut App, chip: crate::tui::bars::BarChip| {
            let (r, _, _) = *app
                .hits
                .bar_chips
                .iter()
                .find(|(_, _, c)| *c == chip)
                .unwrap();
            app.on_mouse(click(r.x + 1, r.y));
        };
        use crate::tui::bars::BarChip as C;
        click_chip(&mut app, C::Follow);
        assert!(!app.tabs[0].engine.follow_tail);
        render(&mut app, 120, 30);
        click_chip(&mut app, C::Monitor);
        assert!(!app.tabs[0].engine.is_watching);
        render(&mut app, 120, 30);
        click_chip(&mut app, C::Case);
        assert!(app.tabs[0].engine.filter_case_sensitive);
        render(&mut app, 120, 30);
        click_chip(&mut app, C::Context);
        assert_eq!(app.tabs[0].engine.context_lines(), 1);
        render(&mut app, 120, 30);
        click_chip(&mut app, C::Include);
        assert_eq!(
            app.prompt.as_ref().map(|p| p.kind),
            Some(PromptKind::Include)
        );
    }

    #[test]
    fn arrows_scroll_sideways_by_one_and_ctrl_arrows_by_ten() {
        let (mut app, _dir) = app_with(&[("a.log", LOG)], false);
        app.apply(Action::ScrollRight);
        assert_eq!(app.tabs[0].hscroll, 1);
        app.apply(Action::ScrollRightFast);
        assert_eq!(app.tabs[0].hscroll, 11);
        app.apply(Action::ScrollLeftFast);
        app.apply(Action::ScrollLeftFast);
        assert_eq!(app.tabs[0].hscroll, 0);
    }

    /// Every dialog and editor opens over the windows, inside the screen, with its frame
    /// and buttons registered for the mouse (task 7.2 of tui-interface).
    #[test]
    fn every_dialog_draws_inside_the_screen() {
        let log = "2026-09-28 10:00:00 INFO start\n{\"user\":{\"id\":7}}\n";
        let (mut app, _dir) = app_with(&[("a.log", log)], false);
        app.settings = Some(crate::tui::workspace::Settings::default());
        let actions = [
            Action::StartSearch,
            Action::EditInclude,
            Action::EditExclude,
            Action::GoTo,
            Action::EditNote,
            Action::TimeRange,
            Action::OpenFile,
            Action::OpenSession,
            Action::SaveSession,
            Action::Settings,
            Action::EditRules,
            Action::Presets,
            Action::EditGlobal,
            Action::Tools,
            Action::Palette,
            Action::About,
            Action::JsonTree,
        ];
        for action in actions {
            render(&mut app, 100, 30);
            app.tabs[0].set_cursor(1);
            app.apply(action);
            let screen = render(&mut app, 100, 30);
            let d = app
                .hits
                .dialog
                .unwrap_or_else(|| panic!("{action:?} drew no dialog: {screen:#?}"));
            assert!(
                d.outer.right() <= 100 && d.outer.bottom() <= 30,
                "{action:?}"
            );
            assert!(d
                .outer
                .contains(ratatui::layout::Position::new(d.ok.x, d.ok.y)));
            // Esc closes it, whichever it is.
            let esc =
                crossterm::event::KeyEvent::new(crossterm::event::KeyCode::Esc, KeyModifiers::NONE);
            app.on_key(esc);
            if app.settings_form.is_some() || app.rules.is_some() || app.tools_editor.is_some() {
                app.on_key(esc);
            }
            render(&mut app, 100, 30);
            assert!(app.hits.dialog.is_none(), "{action:?} still open");
        }
    }

    /// The HEX view fits as many bytes per row as the window has room for.
    #[test]
    fn the_hex_view_uses_the_width_it_has() {
        let (mut app, _dir) = app_with(&[("a.log", LOG)], false);
        app.apply(Action::ToggleHex);
        render(&mut app, 80, 20);
        let narrow = app.tabs[0].hex_width;
        let screen = render(&mut app, 240, 20);
        let wide = app.tabs[0].hex_width;
        assert!(narrow >= 8 && wide > narrow, "{narrow} {wide}");
        assert!(screen.iter().any(|l| l.contains("00000000")), "{screen:#?}");
    }

    /// Chinese and Japanese texts are measured in cells: the bars and the help fit.
    #[test]
    fn a_cjk_language_fits_the_cells() {
        for lang in [Language::Zh, Language::Ja] {
            let (mut app, _dir) = app_with(&[("a.log", LOG)], false);
            app.settings = Some(crate::tui::workspace::Settings::default());
            app.settings.as_mut().unwrap().config.language = lang;
            app.apply_running_config();
            // A wide character takes two cells: the text capture has a blank after it.
            let has = |screen: &[String], en| {
                let text = crate::i18n_tui::tx(lang, en).replace(' ', "");
                screen.iter().any(|l| l.replace(' ', "").contains(&text))
            };
            let screen = render(&mut app, 100, 30);
            assert!(has(&screen, "Open"), "{screen:#?}");
            // Every row still ends with the window's right border at column 100.
            assert!(screen[3..8].iter().all(|l| l.ends_with('║')), "{screen:#?}");
            app.apply(Action::ToggleHelp);
            let help = render(&mut app, 120, 30);
            assert!(has(&help, "quit; ? or F1: this help"), "{help:#?}");
        }
    }

    /// Frame time on a generated file at 200 x 60 (p95 at most 5 ms), ignored by default
    /// like the benchmarks: `cargo test --release ... -- --ignored frame_time`.
    #[test]
    #[ignore]
    fn frame_time_at_200_by_60() {
        let mut text = String::new();
        for i in 0..200_000 {
            let level = ["INFO", "WARN", "ERROR"][i % 3];
            text.push_str(&format!(
                "2026-09-28 10:{:02}:{:02} {level} request {i} took {}ms\n",
                (i / 60) % 60,
                i % 60,
                i % 997
            ));
        }
        let (mut app, _dir) = app_with(&[("big.log", text.as_str())], false);
        for _ in 0..500 {
            app.tick();
            if !app.busy() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let mut terminal = Terminal::new(TestBackend::new(200, 60)).unwrap();
        app.tabs[0].engine.follow_tail = false;
        let rows = app.tabs[0].engine.visible_line_count();
        let mut samples = Vec::new();
        for i in 0..300 {
            app.tabs[0].top = rows / 300 * i;
            let t = Instant::now();
            terminal.draw(|f| app.draw(f)).unwrap();
            samples.push(t.elapsed());
        }
        samples.sort();
        let p95 = samples[samples.len() * 95 / 100];
        assert!(p95 <= Duration::from_millis(5), "p95 {p95:?}");
    }

    #[test]
    fn a_closing_window_shrinks_away() {
        let (mut app, _dir) = app_with(&[("a.log", LOG), ("b.log", LOG)], false);
        app.apply(Action::SplitRight);
        render(&mut app, 80, 20);
        // b's window goes with its only tab: its outline is drawn shrinking, then gone.
        app.focus_tab(1);
        app.apply(Action::CloseStream);
        assert_eq!(app.closing.len(), 1);
        assert!(app.busy(), "drawn at the busy tick");
        render(&mut app, 80, 20);
        std::thread::sleep(CLOSE_EFFECT + Duration::from_millis(20));
        app.tick();
        assert!(app.closing.is_empty());
        // Closing a tab of a window that keeps others has no effect.
        let (mut app, _dir) = app_with(&[("a.log", LOG), ("b.log", LOG)], false);
        render(&mut app, 80, 20);
        app.apply(Action::CloseStream);
        assert!(app.closing.is_empty());
    }

    #[test]
    fn find_all_lists_every_stream_and_enter_jumps_to_the_line() {
        use crossterm::event::{KeyCode, KeyEvent};
        let api = "ok\nreq-8812 start\nok\nreq-8812 end\n";
        let db = "fine\nreq-8812 seen\n";
        let (mut app, _dir) = app_with(&[("api.log", api), ("db.log", db)], false);
        let deadline = Instant::now() + Duration::from_secs(10);
        while app.tabs.iter().any(|t| t.engine.index_pending) {
            assert!(Instant::now() < deadline, "indexing did not finish");
            render(&mut app, 200, 40);
            std::thread::sleep(Duration::from_millis(2));
        }
        app.focus_tab(0);
        app.tabs[0].engine.search_query = "start".into();
        let shift_ctrl = KeyModifiers::CONTROL | KeyModifiers::SHIFT;
        app.on_key(KeyEvent::new(KeyCode::Char('F'), shift_ctrl));
        // The query box holds the focused stream's search.
        assert_eq!(app.find_window.as_ref().unwrap().field.text(), "start");
        app.on_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        keys(&mut app, "req-8812");
        press(&mut app, KeyCode::Enter);
        while app.find_window.as_ref().unwrap().session.is_active() {
            assert!(Instant::now() < deadline, "the search did not finish");
            app.tick();
            std::thread::sleep(Duration::from_millis(2));
        }
        let screen = render(&mut app, 200, 40);
        let has = |screen: &[String], text: &str| screen.iter().any(|l| l.contains(text));
        assert!(has(&screen, "3 matches in 2 of 2 streams"), "{screen:#?}");
        assert!(has(&screen, "api.log  2 matches"), "{screen:#?}");
        assert!(has(&screen, "db.log  1 matches"), "{screen:#?}");
        assert!(has(&screen, "       4  req-8812 end"), "{screen:#?}");
        // At 80 columns the line numbers stay, below 60 they go first.
        let screen = render(&mut app, 80, 24);
        assert!(has(&screen, "       2  req-8812 start"), "{screen:#?}");
        let screen = render(&mut app, 56, 24);
        assert!(has(&screen, "  req-8812 start"), "{screen:#?}");
        assert!(!has(&screen, "2  req-8812 start"), "{screen:#?}");
        // The last result is db.log's: Enter focuses it on the line, follow paused,
        // its search untouched; the window hides and the key shows it again.
        press(&mut app, KeyCode::End);
        press(&mut app, KeyCode::Enter);
        assert!(!app.find_window.as_ref().unwrap().shown);
        assert_eq!(app.active, 1);
        render(&mut app, 200, 40);
        let tab = &app.tabs[1];
        assert_eq!(tab.cursor_line(), Some(1));
        assert!(!tab.engine.follow_tail);
        assert!(tab.engine.search_query.is_empty());
        assert_eq!(app.tabs[0].engine.search_query, "start");
        app.apply(Action::FindAll);
        let w = app.find_window.as_ref().unwrap();
        assert!(w.shown && !w.editing);
        assert_eq!(w.session.total_hits(), 3);
        // Esc closes the window and drops the results.
        press(&mut app, KeyCode::Esc);
        assert!(app.find_window.is_none());
    }

    #[test]
    fn find_results_tint_the_query_in_any_case() {
        let hit = Style::default().add_modifier(Modifier::BOLD);
        let spans = find_hit_spans("GET /a timeout, Timeout", "timeout", Style::default(), hit);
        let texts: Vec<(&str, bool)> = spans
            .iter()
            .map(|s| (s.content.as_ref(), s.style == hit))
            .collect();
        assert_eq!(
            texts,
            vec![
                ("GET /a ", false),
                ("timeout", true),
                (", ", false),
                ("Timeout", true)
            ]
        );
    }

    #[test]
    fn p_in_the_fields_list_draws_the_field_as_a_sparkline_row() {
        use crossterm::event::KeyCode;
        let log: String = (0..40)
            .map(|i| {
                format!(
                    "{{\"ts\":\"2026-10-24T10:00:{:02}Z\",\"level\":\"info\",\"ms\":{}}}\n",
                    i,
                    if i < 20 { 10 } else { 500 }
                )
            })
            .collect();
        let (mut app, _dir) = app_with(&[("api.log", log.as_str())], false);
        let deadline = Instant::now() + Duration::from_secs(10);
        let settle = |app: &mut App| {
            while app.busy() {
                assert!(Instant::now() < deadline, "the scans did not finish");
                render(app, 100, 20);
                std::thread::sleep(Duration::from_millis(2));
            }
        };
        settle(&mut app);
        render(&mut app, 100, 20);
        let row = App::column_menu_rows(&app.tabs[0].engine)
            .iter()
            .position(|r| r.0 == "ms")
            .expect("the ms field is listed");
        app.columns_menu = Some(row);
        press(&mut app, KeyCode::Char('p'));
        assert_eq!(app.tabs[0].engine.plots()[0].key, "ms");
        let menu = render(&mut app, 100, 20);
        // (A wide character takes two cells: the buffer shows a blank after it.)
        assert!(
            menu.iter()
                .any(|l| l.contains("☑ ms  📈") && l.contains("avg")),
            "{menu:#?}"
        );
        press(&mut app, KeyCode::Char('a'));
        press(&mut app, KeyCode::Esc);
        settle(&mut app);
        let screen = render(&mut app, 100, 20);
        let spark = screen
            .iter()
            .find(|l| l.contains("📈") && l.contains(" ms max "))
            .unwrap_or_else(|| panic!("{screen:#?}"));
        assert!(spark.contains('▁') && spark.contains('█'), "{spark}");
        assert!(spark.contains("10 - 500"), "{spark}");
        // Narrow: the label goes, the line and the scale stay.
        let narrow = render(&mut app, 30, 20);
        assert!(
            narrow
                .iter()
                .any(|l| l.contains("10 - 500") && !l.contains("📈")),
            "{narrow:#?}"
        );
        // `p` on the drawn field removes it, and the row with it.
        app.columns_menu = Some(row);
        press(&mut app, KeyCode::Char('p'));
        press(&mut app, KeyCode::Esc);
        assert!(app.tabs[0].engine.plots().is_empty());
        let screen = render(&mut app, 100, 20);
        assert!(
            !screen.iter().any(|l| l.contains("10 - 500")),
            "{screen:#?}"
        );
    }

    #[test]
    fn plot_first_puts_the_field_first_and_keeps_its_statistic() {
        use crate::plot::{PlotSpec, PlotStat};
        let spec = |k: &str, stat| PlotSpec {
            key: k.into(),
            stat,
        };
        let plots = vec![spec("a", PlotStat::Avg), spec("b", PlotStat::P95)];
        assert_eq!(
            plot_first(&plots, "b"),
            vec![spec("b", PlotStat::P95), spec("a", PlotStat::Avg)]
        );
        assert_eq!(plot_first(&plots, "a"), vec![spec("b", PlotStat::P95)]);
        let full = vec![
            spec("a", PlotStat::Avg),
            spec("b", PlotStat::Avg),
            spec("c", PlotStat::Avg),
        ];
        assert_eq!(plot_first(&full, "d").len(), crate::plot::MAX_PLOTS);
        assert_eq!(plot_first(&full, "d")[0].key, "d");
    }

    #[test]
    fn find_all_marks_a_reloaded_stream_stale_and_does_not_jump() {
        use crossterm::event::KeyCode;
        let (mut app, dir) = app_with(&[("a.log", "boom 1\nboom 2\n")], false);
        let deadline = Instant::now() + Duration::from_secs(10);
        while app.tabs[0].engine.index_pending {
            assert!(Instant::now() < deadline, "indexing did not finish");
            render(&mut app, 120, 30);
            std::thread::sleep(Duration::from_millis(2));
        }
        app.apply(Action::FindAll);
        keys(&mut app, "boom");
        press(&mut app, KeyCode::Enter);
        while app.find_window.as_ref().unwrap().session.is_active() {
            assert!(Instant::now() < deadline, "the search did not finish");
            app.tick();
            std::thread::sleep(Duration::from_millis(2));
        }
        // The file is truncated and rewritten: its group goes stale.
        std::fs::write(dir.path().join("a.log"), "x\n").unwrap();
        while app.find_window.as_ref().unwrap().session.groups[0].state != FindState::Stale {
            assert!(Instant::now() < deadline, "the group did not go stale");
            app.tabs[0].engine.poll_updates();
            app.tick();
            std::thread::sleep(Duration::from_millis(5));
        }
        let screen = render(&mut app, 120, 30);
        assert!(
            screen.iter().any(|l| l.contains("stale: press r")),
            "{screen:#?}"
        );
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Enter);
        assert!(app.find_window.as_ref().unwrap().shown, "no jump");
        assert_eq!(
            app.message.as_deref(),
            Some("This stream was reloaded since the search: press r to search again")
        );
    }

    #[test]
    fn j_opens_the_json_of_the_cursor_row_as_a_tree() {
        let log = "plain line
2026-10-05T10:00:00Z {\"user\":{\"id\":7},\"ok\":true}
";
        let (mut app, _dir) = app_with(&[("j.log", log)], false);
        render(&mut app, 100, 30);
        app.tabs[0].set_cursor(0);
        app.apply(Action::JsonTree);
        assert!(app.json_dialog.is_none());
        assert_eq!(app.message.as_deref(), Some("No JSON on this row"));
        app.tabs[0].set_cursor(1);
        app.apply(Action::JsonTree);
        let screen = render(&mut app, 100, 30);
        assert!(
            screen.iter().any(|l| l.contains("JSON - line 2")),
            "{screen:#?}"
        );
        assert!(
            screen.iter().any(|l| l.contains("> user: {1 keys}")),
            "{screen:#?}"
        );
        assert!(screen.iter().any(|l| l.contains("ok: true")));
        // A click on `user` unfolds it.
        let (rect, _) = app.hits.list_items[0];
        app.on_mouse(click(rect.x + 2, rect.y));
        let screen = render(&mut app, 100, 30);
        assert!(screen.iter().any(|l| l.contains("id: 7")), "{screen:#?}");
        let key = |c| crossterm::event::KeyEvent::new(c, KeyModifiers::NONE);
        app.on_key(key(crossterm::event::KeyCode::Esc));
        assert!(app.json_dialog.is_none());
    }

    #[test]
    fn an_alert_in_a_hidden_stream_rings_the_bell_once_until_it_is_seen() {
        let (mut app, dir) = app_with(&[("a.log", LOG), ("b.log", LOG)], false);
        let mut settings = crate::tui::workspace::Settings::default();
        settings.config.flash_on_alert = true;
        app.settings = Some(settings);
        let mut rule =
            crate::tail_engine::HighlightRule::new("boom", [255, 0, 0], [0, 0, 0], false);
        rule.sound_alert = crate::audio::SoundAlertPreset::Critical;
        app.idle_poll = Duration::ZERO;
        for tab in &mut app.tabs {
            tab.engine.set_highlight_rules(vec![rule.clone()]);
            tab.engine.size_check_interval = Duration::ZERO;
        }
        app.tick();
        let append = |text: &str| {
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(dir.path().join("b.log"))
                .unwrap();
            f.write_all(text.as_bytes()).unwrap();
        };
        let ring = |app: &mut App| {
            for _ in 0..200 {
                app.tick();
                if app.take_bell() {
                    return true;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            false
        };
        // b is not on screen: a plain line stays quiet, an alert line rings once.
        append(
            "2026-09-28 10:00:09 INFO fine
",
        );
        assert!(!ring(&mut app));
        append(
            "2026-09-28 10:00:10 ERROR boom
",
        );
        assert!(ring(&mut app));
        append(
            "2026-09-28 10:00:11 ERROR boom again
",
        );
        assert!(!ring(&mut app), "once until the alerts are seen");
        // Seen (b shown), then hidden again: the next alert rings.
        app.focus_tab(1);
        app.tick();
        app.focus_tab(0);
        app.tick();
        append(
            "2026-09-28 10:00:12 ERROR boom
",
        );
        assert!(ring(&mut app));
        // Off in Settings: silent.
        app.focus_tab(1);
        app.tick();
        app.focus_tab(0);
        app.settings.as_mut().unwrap().config.flash_on_alert = false;
        append(
            "2026-09-28 10:00:13 ERROR boom
",
        );
        assert!(!ring(&mut app));
    }

    #[test]
    fn follow_and_auto_update_are_two_independent_check_boxes() {
        use crate::tui::bars::BarChip as C;
        let bar = |app: &mut App, w: u16| {
            let screen = render(app, w, 30);
            screen
                .iter()
                .find(|l| l.contains("[TXT]"))
                // A wide character is drawn as two cells, the second one blank.
                .map(|l| l.replace("  ", " "))
                .unwrap_or_else(|| panic!("stream bar: {screen:#?}"))
        };
        let click_chip = |app: &mut App, chip: C| {
            let (r, _, _) = *app
                .hits
                .bar_chips
                .iter()
                .find(|(_, _, c)| *c == chip)
                .unwrap();
            app.on_mouse(click(r.x + 1, r.y));
        };
        for ascii in [false, true] {
            let (mut app, _dir) = app_with(&[("a.log", LOG)], ascii);
            let line = bar(&mut app, 120);
            assert!(
                line.contains("[x] Follow") && line.contains("[x] Auto-update"),
                "{line}"
            );
            assert!(!line.contains("Monitor") && !line.contains('▶'), "{line}");
            // Follow off, auto-update untouched.
            click_chip(&mut app, C::Follow);
            assert!(!app.tabs[0].engine.follow_tail && app.tabs[0].engine.is_watching);
            let line = bar(&mut app, 120);
            assert!(
                line.contains("[ ] Follow") && line.contains("[x] Auto-update"),
                "{line}"
            );
            // Auto-update off, follow untouched.
            click_chip(&mut app, C::Monitor);
            assert!(!app.tabs[0].engine.follow_tail && !app.tabs[0].engine.is_watching);
            assert_eq!(app.message.as_deref(), Some("Auto-update off"));
            let line = bar(&mut app, 120);
            assert!(line.contains("[ ] Auto-update"), "{line}");
            click_chip(&mut app, C::Monitor);
            assert_eq!(app.message.as_deref(), Some("Auto-update on"));
        }
        // Long translations still fit an 80-column bar.
        for (lang, follow, auto) in [
            (Language::De, "[x] Folgen", "[x] Auto-Aktualisierung"),
            (Language::Ja, "[x] 追 従", "[x] 自 動 更 新"),
        ] {
            let (mut app, _dir) = app_with(&[("a.log", LOG)], false);
            app.settings = Some(crate::tui::workspace::Settings::default());
            app.settings.as_mut().unwrap().config.language = lang;
            app.apply_running_config();
            let line = bar(&mut app, 80);
            assert!(line.contains(follow) && line.contains(auto), "{line}");
        }
    }

    #[test]
    fn the_terminal_speaks_the_language_of_the_ini_and_switches_at_once() {
        let (mut app, _dir) = app_with(&[("a.log", LOG)], false);
        app.settings = Some(crate::tui::workspace::Settings::default());
        app.settings.as_mut().unwrap().config.language = Language::It;
        app.apply_running_config();
        let screen = render(&mut app, 120, 30);
        assert!(screen[1].contains("[o Apri]"), "{screen:#?}");
        assert!(
            screen.iter().any(|l| l.contains("[x] Segui")),
            "{screen:#?}"
        );
        app.apply(Action::ToggleHelp);
        let screen = render(&mut app, 120, 30);
        assert!(screen
            .iter()
            .any(|l| l.contains("sposta il cursore di una riga")));
        app.apply(Action::ToggleHelp);
        // Settings: the language field changes the texts while the dialog is open.
        app.apply(Action::Settings);
        let f = app.settings_form.as_mut().unwrap();
        f.focus = f.fields.iter().position(|x| x.label == "Language").unwrap();
        let fr = Language::ALL
            .iter()
            .position(|l| *l == Language::Fr)
            .unwrap();
        if let crate::tui::settings::Widget::Radio(r) = &mut f.fields[f.focus].widget {
            r.selected = fr - 1;
        }
        press(&mut app, crossterm::event::KeyCode::Right);
        assert_eq!(app.lang, Language::Fr);
        let screen = render(&mut app, 120, 30);
        assert!(screen.iter().any(|l| l.contains("Réglages")), "{screen:#?}");
    }

    #[test]
    fn the_top_bar_has_the_title_and_the_guis_commands_which_click() {
        let (mut app, _dir) = app_with(&[("a.log", LOG), ("b.log", LOG)], false);
        let screen = render(&mut app, 120, 20);
        assert!(screen[0].starts_with(" FastTail v"), "{screen:#?}");
        assert!(
            screen[0].contains("[F Global off]") && screen[0].ends_with("[: Palette]"),
            "{screen:#?}"
        );
        let tools = &screen[1];
        for label in [
            "[o Open]",
            "[O Sessions]",
            "[r Rules (0)]",
            "[▶ Play]",
            "[⏸ Pause]",
            "[, Settings]",
        ] {
            assert!(tools.contains(label), "{label}: {tools}");
        }
        assert!(tools.trim_end().ends_with("[ℹ About] [? Help]"), "{tools}");
        assert!(screen[0].contains("by Matteo Baccan"), "{screen:#?}");
        let click_on = |app: &mut App, action: Action| {
            let (r, _) = *app
                .hits
                .top_buttons
                .iter()
                .find(|(_, a)| *a == action)
                .unwrap();
            app.on_mouse(click(r.x + 1, r.y));
        };
        // Pause stops every stream; Play makes them follow again.
        click_on(&mut app, Action::PauseAll);
        assert!(app
            .tabs
            .iter()
            .all(|t| !t.engine.follow_tail && !t.engine.is_watching));
        render(&mut app, 120, 20);
        click_on(&mut app, Action::PlayAll);
        assert!(app
            .tabs
            .iter()
            .all(|t| t.engine.follow_tail && t.engine.is_watching));
        render(&mut app, 120, 20);
        click_on(&mut app, Action::OpenFile);
        assert!(app.browser.is_some());
        app.browser = None;
        render(&mut app, 120, 20);
        click_on(&mut app, Action::About);
        let screen = render(&mut app, 120, 20);
        assert!(app.show_about);
        assert!(screen
            .iter()
            .any(|l| l.contains("Matteo Baccan") && l.contains("Author")));
        assert!(screen
            .iter()
            .any(|l| l.contains("github.com/matteobaccan/FastTail")));
        let esc =
            crossterm::event::KeyEvent::new(crossterm::event::KeyCode::Esc, KeyModifiers::NONE);
        app.on_key(esc);
        assert!(!app.show_about);
        // A short terminal keeps only the title row.
        app.browser = None;
        let screen = render(&mut app, 120, 12);
        assert!(screen[0].contains("FastTail v") && !screen[1].contains("[o Open]"));
    }

    #[test]
    fn the_status_bar_buttons_are_bracketed_and_clickable() {
        let (mut app, _dir) = app_with(&[("a.log", LOG)], false);
        let screen = render(&mut app, 120, 20);
        let bar = &screen[screen.len() - 2];
        assert!(
            bar.contains("[/ search]") && bar.contains("[q quit]"),
            "{bar}"
        );
        let (rect, _) = *app
            .hits
            .buttons
            .iter()
            .find(|(_, i)| STATUS[*i].2 == Action::StartSearch)
            .unwrap();
        app.on_mouse(click(rect.x + 1, rect.y));
        assert_eq!(
            app.prompt.as_ref().map(|p| p.kind),
            Some(PromptKind::Search)
        );
        // Only the buttons that fit are drawn and clickable.
        app.prompt = None;
        render(&mut app, 50, 20);
        assert!(app.hits.buttons.len() < STATUS.len());
        assert!(app.hits.buttons.iter().all(|(r, _)| r.right() <= 49));
    }

    #[test]
    fn stream_window_has_borders_title_and_counts() {
        let (mut app, _dir) = app_with(&[("test.log", LOG)], false);
        let screen = render(&mut app, 70, 12);
        // The top bar's title row (12 rows: no command row), then one file: no tab
        // strip, the window starts on the second row, double-bordered.
        assert!(screen[0].contains("FastTail v"), "{screen:#?}");
        assert!(screen[1].starts_with('╔'), "{screen:#?}");
        assert!(screen[1].contains("[#1] test.log"), "{screen:#?}");
        assert!(screen[1].contains("FOLLOW"), "{screen:#?}");
        // The stream bar (a 6-row window has room for it, not for the filter row).
        assert!(
            screen[2].contains("[x] Follow") && screen[2].contains("[TXT]"),
            "{screen:#?}"
        );
        assert!(screen[3].starts_with('║') && screen[3].contains("INFO start"));
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
        app.apply(Action::SplitRight);
        let screen = render(&mut app, 80, 14);
        // The top bar's title row, the tab strip, then two windows side by side, the focused one with `=`.
        let (left, right) = screen[2].split_at(40);
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
        let (left, right) = screen[2].split_at(40);
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
    fn alt_v_draws_the_volume_row_and_a_click_goes_to_that_time() {
        let body: String = (0..120)
            .map(|i| {
                let level = if i == 100 { "ERROR" } else { "INFO" };
                format!(
                    "2026-09-28 10:{:02}:{:02} {level} line {i}\n",
                    i / 60,
                    i % 60
                )
            })
            .collect();
        let (mut app, _dir) = app_with(&[("a.log", &body)], false);
        render(&mut app, 100, 20);
        app.apply(Action::VolumeRow);
        assert!(app.tabs[0].engine.timeline_open);
        let deadline = Instant::now() + Duration::from_secs(10);
        while app.tabs[0].engine.histogram_lines() < 120 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
            render(&mut app, 100, 20);
        }
        let screen = render(&mut app, 100, 20);
        assert!(screen.iter().any(|l| l.contains("▤")), "{screen:#?}");
        assert!(!app.hits.volume_cells.is_empty());
        // The first cell is the first second: line 1.
        app.tabs[0].engine.follow_tail = true;
        let (r, _, _) = app.hits.volume_cells[0];
        assert!(app.on_mouse(click(r.x, r.y)));
        render(&mut app, 100, 20);
        assert!(!app.tabs[0].engine.follow_tail);
        assert_eq!(app.tabs[0].cursor_line(), Some(0));
        app.apply(Action::VolumeRow);
        assert!(!app.tabs[0].engine.timeline_open);
    }

    #[test]
    fn alt_k_keeps_searches_and_n_walks_them_when_no_search_is_typed() {
        let body = "ok\ntimeout a\nok\nrefused b\ntimeout c\nok\n";
        let (mut app, _dir) = app_with(&[("a.log", body)], false);
        render(&mut app, 100, 20);
        app.apply(Action::KeepSearch);
        assert_eq!(
            app.message.as_deref(),
            Some("Type a search (/) first, then Alt+K keeps it")
        );
        for q in ["timeout", "refused"] {
            app.search(q);
            app.apply(Action::KeepSearch);
        }
        assert!(app.tabs[0].engine.search_query.is_empty());
        let deadline = Instant::now() + Duration::from_secs(10);
        while app.tabs[0]
            .engine
            .named
            .list
            .iter()
            .any(|n| n.running() || n.total() == 0)
            && Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(10));
            render(&mut app, 100, 20);
        }
        let screen = render(&mut app, 200, 20);
        assert!(
            screen
                .iter()
                .any(|l| l.contains("timeout 2") && l.contains("refused 1")),
            "{screen:#?}"
        );
        assert!(
            !screen.iter().any(|l| l.contains("Not found")),
            "{screen:#?}"
        );
        // From the top: refused on line 4 comes after timeout on line 2.
        app.tabs[0].set_cursor(0);
        app.apply(Action::SearchNext);
        render(&mut app, 100, 20);
        assert_eq!(app.tabs[0].cursor_line(), Some(1));
        app.apply(Action::SearchNext);
        render(&mut app, 100, 20);
        assert_eq!(app.tabs[0].cursor_line(), Some(3));
        // Focused on timeout (its chip), refused is skipped.
        app.tabs[0].engine.named.toggle_focus(0);
        app.apply(Action::SearchNext);
        render(&mut app, 100, 20);
        assert_eq!(app.tabs[0].cursor_line(), Some(4));
        // Alt+K with nothing typed forgets the focused one.
        app.apply(Action::KeepSearch);
        assert_eq!(
            app.tabs[0].engine.named.saved(),
            vec![(2, "refused".to_string())]
        );
    }

    #[test]
    fn alt_i_shows_the_stream_information() {
        use crossterm::event::KeyCode;
        let (mut app, _dir) = app_with(&[("a.log", LOG)], false);
        render(&mut app, 100, 30);
        app.apply(Action::StreamInfo);
        let screen = render(&mut app, 100, 30);
        assert!(
            screen
                .iter()
                .any(|l| l.contains("Stream information · a.log")),
            "{screen:#?}"
        );
        assert!(
            screen
                .iter()
                .any(|l| l.contains("Lines") && l.contains(" 3")),
            "{screen:#?}"
        );
        assert!(screen
            .iter()
            .any(|l| l.contains("Encoding") && l.contains("UTF-8")));
        press(&mut app, KeyCode::Esc);
        assert!(app.info_view.is_none());
    }

    #[test]
    fn clicking_the_second_window_of_a_split_focuses_it() {
        let (mut app, _dir) = app_with(&[("a.log", LOG), ("b.log", LOG)], false);
        app.apply(Action::SplitRight);
        render(&mut app, 80, 14);
        assert_eq!(app.active, 0);
        // Somewhere on the right window's rows.
        assert!(app.on_mouse(click(60, 3)));
        assert_eq!(app.active, 1);
        let screen = render(&mut app, 80, 14);
        let (left, right) = screen[2].split_at(screen[2].char_indices().nth(40).unwrap().0);
        assert!(
            left.starts_with('┌') && right.starts_with('╔'),
            "{screen:#?}"
        );
        // A click on the strip title of stream 1 brings the focus back.
        assert!(app.on_mouse(click(2, 1)));
        assert_eq!(app.active, 0);
    }

    #[test]
    fn clicks_select_rows_and_the_wheel_scrolls_the_window_under_the_pointer() {
        let body: String = (1..=100)
            .map(|i| format!("2026-09-28 10:00:00 INFO line {i}\n"))
            .collect();
        let (mut app, _dir) = app_with(&[("a.log", &body), ("b.log", &body)], false);
        app.apply(Action::SplitRight);
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

    fn drag(column: u16, row: u16) -> MouseEvent {
        mouse(MouseEventKind::Drag(MouseButton::Left), column, row, false)
    }

    fn release(column: u16, row: u16) -> MouseEvent {
        mouse(MouseEventKind::Up(MouseButton::Left), column, row, false)
    }

    fn path_of(app: &App, i: usize) -> PathBuf {
        app.tabs[i].engine.path.clone()
    }

    #[test]
    fn a_dragged_divider_resizes_and_a_dragged_title_moves_the_window() {
        let files = [("a.log", LOG), ("b.log", LOG), ("c.log", LOG)];
        let (mut app, _dir) = app_with(&files, false);
        // One window with three tabs; `s` puts the next one, b, on the right.
        app.apply(Action::SplitRight);
        let (a, b, c) = (path_of(&app, 0), path_of(&app, 1), path_of(&app, 2));
        assert_eq!(app.dock.find_stream(&a), Some(vec![false]));
        assert_eq!(app.dock.find_stream(&c), Some(vec![false]));
        assert_eq!(app.dock.find_stream(&b), Some(vec![true]));
        let screen = render(&mut app, 80, 14);
        assert!(screen[2].contains("[#1] a.log") && screen[2].contains("3:c.log"));

        // The divider between the windows, dragged 8 columns right: 48 of 80.
        app.on_mouse(click(39, 5));
        assert!(app.dock_drag.is_some());
        assert!(app.on_mouse(drag(47, 5)));
        assert!(app.on_mouse(release(47, 5)));
        assert!(matches!(app.dock, Pane::Split { fraction, .. } if fraction == 0.6));
        let screen = render(&mut app, 80, 14);
        let row: Vec<char> = screen[5].chars().collect();
        assert_eq!((row[47], row[48]), ('║', '│'), "{screen:#?}");

        // b's title dropped on the left edge of the other window: b goes left of it.
        assert!(app.on_mouse(click(70, 2)));
        assert_eq!(app.active, 1);
        assert!(app.on_mouse(drag(5, 8)));
        let screen = render(&mut app, 80, 14);
        assert!(
            screen.iter().any(|l| l.contains("drop here")),
            "{screen:#?}"
        );
        assert!(app.on_mouse(release(5, 8)));
        assert_eq!(app.dock.find_stream(&b), Some(vec![false]));
        assert_eq!(app.dock.find_stream(&a), Some(vec![true]));
        assert!(app.dock_dirty);

        // A click on c's tab in the border of the right window shows and focuses it.
        let screen = render(&mut app, 80, 14);
        let Some(col) = screen[2].find("3:c.log") else {
            panic!("{screen:#?}");
        };
        let col = screen[2][..col].chars().count() as u16;
        assert!(app.on_mouse(click(col + 1, 2)));
        assert!(app.on_mouse(release(col + 1, 2)));
        assert_eq!(app.active, 2);
        let screen = render(&mut app, 80, 14);
        assert!(screen[2].contains("[#3] c.log"), "{screen:#?}");
    }

    #[test]
    fn keys_split_move_resize_and_close_windows() {
        let files = [("a.log", LOG), ("b.log", LOG), ("c.log", LOG)];
        let (mut app, _dir) = app_with(&files, false);
        let (a, b, c) = (path_of(&app, 0), path_of(&app, 1), path_of(&app, 2));
        app.apply(Action::ResizeLeft);
        assert_eq!(app.message.as_deref(), Some("No divider that way"));
        // `_`: b below.
        app.apply(Action::SplitDown);
        assert!(matches!(
            app.dock,
            Pane::Split {
                dir: Dir::Vertical,
                ..
            }
        ));
        app.apply(Action::ResizeDown);
        assert!(matches!(app.dock, Pane::Split { fraction, .. } if (fraction - 0.55).abs() < 1e-6));
        // `>`: a joins b's window as its shown tab.
        app.apply(Action::MoveNextPane);
        assert_eq!(app.dock.find_stream(&a), Some(vec![true]));
        assert_eq!(app.dock.find_stream(&c), Some(vec![false]));
        assert_eq!(app.shown_in(&[true]), Some(0));
        // Ctrl+PgDn shows the other tab of the window, b.
        app.apply(Action::NextInPane);
        assert_eq!(app.active, 1);
        // Tab goes to the other window.
        app.apply(Action::NextTab);
        assert_eq!(app.active, 2);
        // Ctrl+W: c's window closes, c joins the remaining one.
        app.apply(Action::ClosePane);
        assert_eq!(app.dock.leaf_paths(), vec![Vec::<bool>::new()]);
        assert_eq!(app.dock.streams(), vec![b.clone(), a.clone(), c.clone()]);
        app.apply(Action::ClosePane);
        assert_eq!(app.message.as_deref(), Some("One window: nothing to close"));
        // Every stream is still drawn somewhere.
        let screen = render(&mut app, 80, 14);
        assert!(screen[2].contains("[#3] c.log"), "{screen:#?}");
    }

    #[test]
    fn alt_m_merges_two_streams_by_time_with_source_tags_and_the_layout_keeps_it() {
        use crossterm::event::KeyCode;
        let a_log = "2026-09-28 10:00:00 INFO a1\n2026-09-28 10:00:02 INFO a2\n";
        let b_log = "2026-09-28 10:00:01 INFO b1\n2026-09-28 10:00:03 INFO b2\n";
        let files = [("a.log", a_log), ("b.log", b_log)];
        let (mut app, _dir) = app_with(&files, false);
        render(&mut app, 100, 20);
        app.apply(Action::MergedView);
        assert_eq!(
            app.merge.as_ref().unwrap().checked(),
            vec![path_of(&app, 0)]
        );
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Char(' '));
        press(&mut app, KeyCode::Enter);
        assert!(app.merge.is_none());
        assert_eq!(app.tabs.len(), 3);
        assert!(app.tabs[2].engine.merged.is_some());
        assert_eq!(app.tabs[2].title, "a.log + b.log");
        let deadline = Instant::now() + Duration::from_secs(10);
        while app.tabs[2].engine.total_lines() < 4 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(30));
            render(&mut app, 100, 20);
        }
        let screen = render(&mut app, 100, 20);
        let rows: Vec<&String> = screen.iter().filter(|l| l.contains("INFO")).collect();
        let order: Vec<&str> = rows
            .iter()
            .filter_map(|l| ["a1", "b1", "a2", "b2"].into_iter().find(|w| l.contains(w)))
            .collect();
        assert_eq!(order, ["a1", "b1", "a2", "b2"], "{screen:#?}");
        assert!(
            rows[0].contains("[a.log]") && rows[1].contains("[b.log]"),
            "{screen:#?}"
        );
        // Line numbers are the source lines: b1 is line 1 of b.log.
        assert!(
            rows[1].trim_start().starts_with("│1") || rows[1].contains(" 1 "),
            "{screen:#?}"
        );
        // Saved under its `merged:` name, rebuilt from the sources in a new run.
        let ron = app.dock_ron();
        assert!(ron.contains("merged:1:"), "{ron}");
        let identity = crate::workspace::saved_identity(&app.tabs[2].engine).unwrap();
        let tabs = (0..2)
            .map(|i| Tab::new(TailEngine::open(path_of(&app, i)).unwrap()))
            .collect();
        let mut again = App::new(tabs, app.palette);
        again.restore_views(&[identity]);
        again.restore_dock(Some(&ron));
        assert_eq!(again.tabs.len(), 3);
        let spool = again.tabs[2].engine.path.clone();
        assert!(again.dock.find_stream(&spool).is_some());
        // Alt+M on the merged view lists its sources: one turned off.
        again.focus_tab(2);
        again.apply(Action::MergedView);
        assert!(again.merge.as_ref().unwrap().view.is_some());
        press(&mut again, KeyCode::Char(' '));
        assert!(!again.tabs[2].engine.merged.as_ref().unwrap().sources[0].enabled);
    }

    #[test]
    fn alt_t_opens_the_filter_as_a_tab_with_source_line_numbers_and_ctrl_k_goes_back() {
        let (mut app, _dir) = app_with(&[("a.log", LOG)], false);
        render(&mut app, 100, 20);
        app.apply(Action::FilterTab);
        assert_eq!(
            app.message.as_deref(),
            Some("No filter to open as a tab: set an include, exclude, level or time filter first")
        );
        app.tabs[0].engine.set_include_filter("ERROR");
        app.apply(Action::FilterTab);
        assert_eq!(app.tabs.len(), 2);
        assert_eq!(app.tabs[1].title, "a.log ▸ ERROR");
        assert_eq!(app.active, 1);
        let deadline = Instant::now() + Duration::from_secs(10);
        while app.tabs[1].engine.total_lines() < 1 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(30));
            render(&mut app, 100, 20);
        }
        let screen = render(&mut app, 100, 20);
        let row = screen
            .iter()
            .find(|l| l.contains("ERROR failed"))
            .expect("the row");
        // The source's line number: the ERROR line is line 3 of a.log.
        assert!(
            row.contains("3 2026-09-28") || row.contains("3▶") || row.contains("3 "),
            "{row}"
        );
        assert!(app.dock_ron().contains("filter:1:"));
        // Ctrl+K shows the line in the source.
        app.tabs[1].set_cursor(0);
        app.apply(Action::ToggleContext);
        render(&mut app, 100, 20);
        assert_eq!(app.active, 0);
        assert_eq!(app.tabs[0].engine.context_line(), Some(2));
    }

    #[test]
    fn at_sets_the_time_anchor_and_a_selection_shows_its_elapsed_time() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let (mut app, _dir) = app_with(&[("a.log", LOG)], false);
        render(&mut app, 100, 20);
        press(&mut app, KeyCode::Char('g'));
        press(&mut app, KeyCode::Char('@'));
        assert_eq!(app.tabs[0].engine.time_anchor(), Some(0));
        assert!(app.tabs[0].engine.show_time_delta);
        assert_eq!(app.message.as_deref(), Some("Time anchor on line 1"));
        let screen = render(&mut app, 100, 20);
        let row = screen.iter().find(|l| l.contains("ERROR failed")).unwrap();
        assert!(row.contains("+2.000"), "{row}");
        // Shift+Down twice: three rows, two seconds.
        app.message = None;
        for _ in 0..2 {
            app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::SHIFT));
        }
        let screen = render(&mut app, 100, 20);
        assert!(
            screen.iter().any(|l| l.contains("Δ +2.000 · 3 rows")),
            "{screen:#?}"
        );
        app.apply(Action::TimeAnchorClear);
        assert_eq!(app.tabs[0].engine.time_anchor(), None);
    }

    #[test]
    fn the_palette_workspace_commands_recent_files_select_all_and_default_session() {
        use crossterm::event::KeyCode;
        let (mut app, dir) = app_with(&[("a.log", LOG)], false);
        app.apply(Action::RecentFiles);
        assert_eq!(app.message.as_deref(), Some("No recent files"));
        let mut settings = crate::tui::workspace::Settings::default();
        let gone = dir.path().join("gone.log");
        settings.config.recent_files = vec![path_of(&app, 0), gone.clone()];
        settings.config.current_session = Some(dir.path().join("s.fasttail-session.ini"));
        settings.config.recent_sessions = vec![dir.path().join("s.fasttail-session.ini")];
        app.settings = Some(settings);
        app.apply(Action::RecentFiles);
        assert_eq!(app.recent_menu, Some(0));
        render(&mut app, 100, 20);
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Delete);
        let config = &app.settings.as_ref().unwrap().config;
        assert_eq!(config.recent_files, vec![path_of(&app, 0)]);
        press(&mut app, KeyCode::Esc);
        assert!(app.recent_menu.is_none());
        app.apply(Action::SelectAll);
        assert!(app.tabs[0].engine.has_selection());
        app.apply(Action::SaveDefaultSession);
        assert!(app
            .settings
            .as_ref()
            .unwrap()
            .config
            .current_session
            .is_none());
        app.apply(Action::ClearRecentSessions);
        assert!(app
            .settings
            .as_ref()
            .unwrap()
            .config
            .recent_sessions
            .is_empty());
        app.apply(Action::RemoveBookmark);
        assert_eq!(app.message.as_deref(), Some("No bookmark on this row"));
    }

    #[test]
    fn a_count_before_e_jumps_to_the_nth_error_and_w_finds_warnings() {
        use crossterm::event::KeyCode;
        let body: String = (1..=100)
            .map(|i| {
                let level = match i {
                    10 | 50 | 90 => "ERROR",
                    30 => "WARN",
                    _ => "INFO",
                };
                format!("2026-09-28 10:00:00 {level} line {i}\n")
            })
            .collect();
        let (mut app, _dir) = app_with(&[("a.log", &body)], false);
        render(&mut app, 80, 20);
        press(&mut app, KeyCode::Char('g'));
        assert_eq!(app.tabs[0].cursor_line(), Some(0));
        // `2e` from row 1: the second ERROR, line 50 (index 49).
        press(&mut app, KeyCode::Char('2'));
        assert_eq!(app.message.as_deref(), Some("Count 2"));
        press(&mut app, KeyCode::Char('e'));
        assert_eq!(app.tabs[0].cursor_line(), Some(49));
        assert!(!app.tabs[0].engine.follow_tail);
        // `E` back to line 10; `E` again wraps to line 90.
        press(&mut app, KeyCode::Char('E'));
        assert_eq!(app.tabs[0].cursor_line(), Some(9));
        press(&mut app, KeyCode::Char('E'));
        assert_eq!(app.tabs[0].cursor_line(), Some(89));
        assert_eq!(app.message.as_deref(), Some("ERROR: back to the last"));
        // `w` finds the WARN line; `12j` moves twelve rows; `0` alone still scrolls.
        press(&mut app, KeyCode::Char('w'));
        assert_eq!(app.tabs[0].cursor_line(), Some(29));
        press(&mut app, KeyCode::Char('1'));
        press(&mut app, KeyCode::Char('2'));
        press(&mut app, KeyCode::Char('j'));
        assert_eq!(app.tabs[0].cursor_line(), Some(41));
        press(&mut app, KeyCode::Char('0'));
        assert!(app.count.is_none());
        // Esc drops a pending count.
        press(&mut app, KeyCode::Char('5'));
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Char('j'));
        assert_eq!(app.tabs[0].cursor_line(), Some(42));
    }

    #[test]
    fn a_key_release_never_acts_twice_in_a_dialog() {
        use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
        let (mut app, _dir) = app_with(&[("a.log", LOG)], false);
        app.apply(Action::Settings);
        let focus = app.settings_form.as_ref().unwrap().focus;
        // Down pressed and released, as the Windows console reports it.
        let down = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        let mut up_again = down;
        up_again.kind = KeyEventKind::Release;
        app.on_key(down);
        assert!(!app.on_key(up_again));
        assert_eq!(app.settings_form.as_ref().unwrap().focus, focus + 1);
    }

    #[test]
    fn the_help_shows_every_entry_on_a_120_by_30_screen() {
        let (mut app, _dir) = app_with(&[("a.log", LOG)], false);
        app.apply(Action::ToggleHelp);
        let screen = render(&mut app, 120, 30);
        for (key, what, _) in HELP {
            assert!(
                screen.iter().any(|l| l.contains(what)),
                "{key} missing: {screen:#?}"
            );
        }
        assert!(!screen.iter().any(|l| l.contains("more (Down)")));
        // A small screen scrolls, and says how much is below on the button row, where
        // it covers no entry.
        let small = render(&mut app, 80, 24);
        let hint = small.iter().find(|l| l.contains("more (Down)")).unwrap();
        assert!(hint.contains("[ OK ]"), "{small:#?}");
    }

    #[test]
    fn ctrl_w_closes_the_stream_and_its_window_with_the_last_tab() {
        let files = [("a.log", LOG), ("b.log", LOG), ("c.log", LOG)];
        let (mut app, _dir) = app_with(&files, false);
        let (a, b, c) = (path_of(&app, 0), path_of(&app, 1), path_of(&app, 2));
        // a and c on the left, b on the right; b focused and closed.
        app.apply(Action::SplitRight);
        app.focus_tab(1);
        app.settings = Some(crate::tui::workspace::Settings::default());
        app.tabs[1].engine.toggle_bookmark(0);
        app.apply(Action::CloseStream);
        assert_eq!(app.message.as_deref(), Some("Closed b.log"));
        let config = &app.settings.as_ref().unwrap().config;
        assert_eq!(
            config.bookmarks_for(&b, 3),
            Some(vec![0]),
            "kept for next time"
        );
        assert_eq!(app.open_paths(), vec![a.clone(), c.clone()]);
        assert_eq!(
            app.dock.leaf_paths(),
            vec![Vec::<bool>::new()],
            "b's window went"
        );
        assert!(app.dock.find_stream(&b).is_none());
        // The focus is on the stream the remaining window shows.
        assert_eq!(app.active, 0);
        assert!(app.dock_dirty);
        // Closing a tab of a window with two keeps the window, showing the other.
        app.apply(Action::CloseStream);
        assert_eq!(app.open_paths(), vec![c.clone()]);
        assert_eq!(app.active, 0);
        let screen = render(&mut app, 80, 14);
        assert!(screen[1].contains("[#1] c.log"), "{screen:#?}");
        // The last one closes too: an empty workspace, where o opens a file.
        app.apply(Action::CloseStream);
        assert!(app.tabs.is_empty());
        assert!(app.dock.streams().is_empty());
        let screen = render(&mut app, 80, 14);
        assert!(screen.iter().any(|l| l.contains(NO_FILE)), "{screen:#?}");
        // Keys that need a stream say so instead of acting.
        for action in [Action::LineDown, Action::NextError, Action::StartSearch] {
            app.apply_counted(action, 3);
            assert_eq!(app.message.as_deref(), Some(NO_FILE));
        }
        app.apply(Action::OpenFile);
        assert!(app.browser.is_some());
        app.browser = None;
        app.open_file(&a);
        assert_eq!(app.open_paths(), vec![a.clone()]);
        assert_eq!(app.dock.streams(), vec![a]);
        assert_eq!(app.active, 0);
    }

    #[test]
    fn floating_windows_move_by_the_title_resize_by_the_corner_and_overlap() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b, c) = (
            dir.path().join("a.log"),
            dir.path().join("b.log"),
            dir.path().join("c.log"),
        );
        for f in [&a, &b, &c] {
            std::fs::write(f, LOG).unwrap();
        }
        let ini = dir.path().join("fasttail.ini");
        crate::config::FastTailConfig {
            open_files: vec![a.clone(), b.clone(), c.clone()],
            ..Default::default()
        }
        .save_to(&ini)
        .unwrap();
        let mut app = app_over(&ini);
        app.restore_dock(None);
        render(&mut app, 100, 30);

        // Alt+F: a floats over the dock, which keeps b and c.
        app.apply(Action::ToggleFloat);
        assert_eq!(app.floats.len(), 1);
        assert_eq!(app.dock.streams(), vec![b.clone(), c.clone()]);
        let screen = render(&mut app, 100, 30);
        let (r, _) = app.hits.floats[0];
        let title: String = screen[r.y as usize]
            .chars()
            .skip(r.x as usize)
            .take(12)
            .collect();
        assert!(title.contains("[#1] a.log"), "{screen:#?}");
        assert!(screen[(r.bottom() - 1) as usize].contains('\u{25e2}'));

        // Dragged by its title, 10 columns right and 2 rows down.
        assert!(app.on_mouse(click(r.x + 3, r.y)));
        assert!(app.on_mouse(drag(r.x + 13, r.y + 2)));
        assert!(app.on_mouse(release(r.x + 13, r.y + 2)));
        assert_eq!(
            (app.floats[0].rect.x, app.floats[0].rect.y),
            (r.x + 10, r.y + 2)
        );
        // Resized by its bottom-right corner to 30 x 8.
        render(&mut app, 100, 30);
        let (r, _) = app.hits.floats[0];
        assert!(app.on_mouse(click(r.right() - 1, r.bottom() - 1)));
        assert!(app.on_mouse(drag(r.x + 29, r.y + 7)));
        assert!(app.on_mouse(release(r.x + 29, r.y + 7)));
        assert_eq!(
            (app.floats[0].rect.width, app.floats[0].rect.height),
            (30, 8)
        );

        // It lies over the dock: a click on its rows focuses a, not the window under it.
        render(&mut app, 100, 30);
        let (r, _) = app.hits.floats[0];
        app.focus_tab(1);
        assert!(app.on_mouse(click(r.x + 2, r.y + 2)));
        assert_eq!(app.active, 0);

        // Saved as a floating window of the GUI's layout, and back at the next start.
        app.save_config();
        let saved = crate::tui::workspace::Settings::read(&ini).config;
        let layout = DockLayout::parse(saved.dock_layout.as_deref().unwrap()).unwrap();
        assert_eq!(layout.windows().len(), 1);
        let mut again = app_over(&ini);
        again.restore_dock(saved.dock_layout.as_deref());
        assert_eq!(again.floats, app.floats);
        assert_eq!(again.dock.streams(), vec![b.clone(), c.clone()]);

        // Alt+F on it docks it back; dragging a title out of the dock floats it again.
        app.apply(Action::ToggleFloat);
        assert!(app.floats.is_empty());
        assert!(app.dock.find_stream(&a).is_some());
        let screen = render(&mut app, 100, 30);
        // Under the top bar's two rows and the tab strip.
        let col = screen[3]
            .find("[#1] a.log")
            .map(|i| screen[3][..i].chars().count());
        let col = col.expect("a's title in the dock") as u16;
        assert!(app.on_mouse(click(col + 2, 3)));
        assert!(app.on_mouse(drag(30, 12)));
        let screen = render(&mut app, 100, 30);
        assert!(
            screen.iter().any(|l| l.contains("float here")),
            "{screen:#?}"
        );
        assert!(app.on_mouse(release(30, 12)));
        assert_eq!(app.floats.len(), 1);
        assert!(app.dock.find_stream(&a).is_none());
    }

    #[test]
    fn the_x_in_a_window_closes_its_stream() {
        let files = [("a.log", LOG), ("b.log", LOG)];
        let (mut app, _dir) = app_with(&files, false);
        app.apply(Action::SplitRight);
        let screen = render(&mut app, 80, 14);
        // Each window has its [x], top right, before the corner.
        let row: Vec<char> = screen[2].chars().collect();
        assert_eq!(row[36..39].iter().collect::<String>(), "[x]", "{screen:#?}");
        assert_eq!(row[76..79].iter().collect::<String>(), "[x]", "{screen:#?}");
        // A click on b's [x] closes b; its window goes with it.
        assert!(app.on_mouse(click(77, 2)));
        assert_eq!(app.tabs.len(), 1);
        assert_eq!(app.tabs[0].title, "a.log");
        assert_eq!(app.dock.leaf_paths(), vec![Vec::<bool>::new()]);
        // The last stream closes too, as with Ctrl+W (one stream: no strip, the window
        // under the top bar).
        render(&mut app, 80, 14);
        assert!(app.on_mouse(click(77, 1)));
        assert!(app.tabs.is_empty(), "{:?}", app.hits.close_buttons);
        render(&mut app, 80, 14);
        assert!(app.hits.close_buttons.is_empty());
    }

    #[test]
    fn the_last_docked_window_floats_and_the_empty_dock_takes_it_back() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.log");
        std::fs::write(&a, LOG).unwrap();
        let ini = dir.path().join("fasttail.ini");
        crate::config::FastTailConfig {
            open_files: vec![a.clone()],
            ..Default::default()
        }
        .save_to(&ini)
        .unwrap();
        let mut app = app_over(&ini);
        app.restore_dock(None);
        render(&mut app, 100, 30);
        // The only window floats: the dock is left empty, and the window is resized.
        app.apply(Action::ToggleFloat);
        assert_eq!(app.floats.len(), 1);
        assert!(app.dock.streams().is_empty());
        let screen = render(&mut app, 100, 30);
        assert!(
            screen.iter().any(|l| l.contains("docks it here")),
            "{screen:#?}"
        );
        let (r, _) = app.hits.floats[0];
        assert!(app.on_mouse(click(r.right() - 1, r.bottom() - 1)));
        assert!(app.on_mouse(drag(r.x + 29, r.y + 7)));
        assert!(app.on_mouse(release(r.x + 29, r.y + 7)));
        assert_eq!(
            (app.floats[0].rect.width, app.floats[0].rect.height),
            (30, 8)
        );
        // Saved with an empty main surface, and so restored.
        app.save_config();
        let saved = crate::tui::workspace::Settings::read(&ini).config;
        let mut again = app_over(&ini);
        again.restore_dock(saved.dock_layout.as_deref());
        assert_eq!(again.floats, app.floats);
        assert!(again.dock.streams().is_empty());
        // Its stream dragged (as a tab of the border is) anywhere on the empty dock
        // docks there whole.
        render(&mut app, 100, 30);
        app.start_move(0, click(40, 3));
        assert!(app.on_mouse(drag(90, 25)));
        let screen = render(&mut app, 100, 30);
        assert!(
            screen.iter().any(|l| l.contains("drop here")),
            "{screen:#?}"
        );
        assert!(app.on_mouse(release(90, 25)));
        assert!(app.floats.is_empty());
        assert_eq!(app.dock.streams(), vec![a.clone()]);
        // Alt+F then Alt+F again: floated and docked back whole.
        app.apply(Action::ToggleFloat);
        app.apply(Action::ToggleFloat);
        assert!(app.floats.is_empty());
        assert_eq!(app.dock, Pane::with_streams(&[a]));
    }

    #[test]
    fn the_layout_is_saved_for_the_gui_and_comes_back_with_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.log");
        let b = dir.path().join("b.log");
        std::fs::write(&a, LOG).unwrap();
        std::fs::write(&b, LOG).unwrap();
        let ini = dir.path().join("fasttail.ini");
        let gui = crate::config::FastTailConfig {
            open_files: vec![a.clone(), b.clone()],
            ..Default::default()
        };
        gui.save_to(&ini).unwrap();
        let mut app = app_over(&ini);
        app.restore_dock(None);
        app.apply(Action::SplitDown);
        app.save_config();
        let saved = crate::tui::workspace::Settings::read(&ini).config;
        let text = saved.dock_layout.clone().expect("layout saved");
        assert_eq!(
            DockLayout::parse(&text).and_then(|l| l.main_pane()),
            Some(app.dock.clone())
        );

        // The next run starts with the same windows.
        let mut again = app_over(&ini);
        again.restore_dock(saved.dock_layout.as_deref());
        assert_eq!(again.dock, app.dock);

        // A session keeps its layout too.
        let session = dir.path().join("two.fasttail-session.ini");
        app.save_session(&session);
        app.apply(Action::ClosePane);
        assert_eq!(app.dock.leaf_paths().len(), 1);
        app.load_session(&session);
        assert_eq!(app.dock.leaf_paths().len(), 2, "{:?}", app.message);
        assert_eq!(app.dock.find_stream(&b), Some(vec![true]));
    }

    #[test]
    fn the_rule_editor_reorders_adds_and_saves_for_every_stream() {
        use crate::tail_engine::HighlightRule;
        use crossterm::event::{KeyCode, KeyEvent};
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.log");
        std::fs::write(&a, "ERROR timeout on db\n").unwrap();
        let ini = dir.path().join("fasttail.ini");
        crate::config::FastTailConfig {
            open_files: vec![a.clone()],
            highlight_rules: vec![
                HighlightRule::new("ERROR", [255, 0, 0], [0, 0, 0], false),
                HighlightRule::new("WARN", [255, 160, 0], [0, 0, 0], false),
                HighlightRule::new("timeout", [255, 255, 0], [0, 0, 0], false),
            ],
            ..Default::default()
        }
        .save_to(&ini)
        .unwrap();
        let mut app = app_over(&ini);
        app.restore_dock(None);
        render(&mut app, 100, 30);
        app.apply(Action::EditRules);
        let screen = render(&mut app, 100, 30);
        assert!(
            screen.iter().any(|l| l.contains("[x]  3.  timeout")),
            "{screen:#?}"
        );
        // A click selects a rule, a second click on it opens its form.
        let (r, _) = app.hits.list_items[1];
        assert!(app.on_mouse(click(r.x + 2, r.y)));
        assert_eq!(app.rules.as_ref().unwrap().list.selected, 1);
        assert!(app.on_mouse(click(r.x + 2, r.y)));
        assert!(app.rules.as_ref().unwrap().form.is_some());
        render(&mut app, 100, 30);
        let cancel = app.hits.dialog.unwrap().cancel.unwrap();
        assert!(app.on_mouse(click(cancel.x + 1, cancel.y)));
        assert!(
            app.rules.as_ref().unwrap().form.is_none(),
            "Cancel drops the form only"
        );
        render(&mut app, 100, 30);
        // Rule 3 to the top: it paints the line now, and the ini lists it first.
        press(&mut app, KeyCode::End);
        for _ in 0..2 {
            app.on_key(KeyEvent::new(KeyCode::Up, KeyModifiers::ALT));
        }
        let first = |app: &App| app.tabs[0].engine.highlight_rules[0].pattern.clone();
        assert_eq!(first(&app), "timeout");
        let saved = crate::tui::workspace::Settings::read(&ini).config;
        assert_eq!(saved.highlight_rules[0].pattern, "timeout");
        assert_eq!(saved.highlight_rules.len(), 3);
        // A new rule FATAL that bookmarks its lines, through the form.
        press(&mut app, KeyCode::Char('a'));
        keys(&mut app, "FATAL");
        let screen = render(&mut app, 100, 30);
        assert!(screen.iter().any(|l| l.contains("Preview")), "{screen:#?}");
        for _ in 0..9 {
            press(&mut app, KeyCode::Down);
        }
        press(&mut app, KeyCode::Char(' '));
        press(&mut app, KeyCode::Enter);
        assert!(app.rules.as_ref().unwrap().form.is_none());
        press(&mut app, KeyCode::Esc);
        assert!(app.rules.is_none());
        let rules = &app.tabs[0].engine.highlight_rules;
        assert_eq!(rules[1].pattern, "FATAL");
        assert!(rules[1].auto_bookmark);
        // An appended FATAL line gets its automatic bookmark.
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new().append(true).open(&a).unwrap();
        writeln!(f, "FATAL disk full").unwrap();
        drop(f);
        let start = Instant::now();
        while app.tabs[0].engine.total_lines() < 2 && start.elapsed() < Duration::from_secs(5) {
            app.tabs[0].engine.poll_updates();
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(app.tabs[0].engine.is_auto_bookmark(1));
        let saved = crate::tui::workspace::Settings::read(&ini).config;
        assert_eq!(saved.highlight_rules[1].pattern, "FATAL");
    }

    #[test]
    fn a_preset_saved_in_the_terminal_is_applied_and_reaches_the_ini() {
        use crossterm::event::{KeyCode, KeyEvent};
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a.log"), dir.path().join("b.log"));
        std::fs::write(&a, "payment ok\nDEBUG payment\nother\n").unwrap();
        std::fs::write(&b, "payment b\nother b\n").unwrap();
        let ini = dir.path().join("fasttail.ini");
        crate::config::FastTailConfig {
            open_files: vec![a.clone(), b.clone()],
            ..Default::default()
        }
        .save_to(&ini)
        .unwrap();
        let mut app = app_over(&ini);
        app.restore_dock(None);
        app.focus_tab(0);
        app.tabs[0].engine.set_include_filter("payment");
        app.tabs[0].engine.set_exclude_filter("DEBUG");
        // Save the stream's filters as "payments".
        app.apply(Action::Presets);
        press(&mut app, KeyCode::Char('s'));
        keys(&mut app, "payments");
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.message.as_deref(), Some("Preset payments saved"));
        let saved = crate::tui::workspace::Settings::read(&ini).config;
        assert_eq!(saved.filter_presets.len(), 1);
        assert_eq!(saved.filter_presets[0].name, "payments");
        assert_eq!(saved.filter_presets[0].state.include, ["payment"]);
        assert_eq!(saved.filter_presets[0].state.exclude, ["DEBUG"]);
        // The list marks it as the focused stream's filters.
        let screen = render(&mut app, 100, 30);
        assert!(
            screen.iter().any(|l| l.contains("= payments")),
            "{screen:#?}"
        );
        // Applied to every stream with A: b gets the same filters, the dialog closes.
        app.on_key(KeyEvent::new(KeyCode::Char('A'), KeyModifiers::SHIFT));
        assert!(app.presets.is_none());
        assert_eq!(app.tabs[1].engine.include_terms(), ["payment"]);
        assert_eq!(app.tabs[1].engine.exclude_terms(), ["DEBUG"]);
        // Renamed, then deleted after the confirmation.
        app.apply(Action::Presets);
        press(&mut app, KeyCode::Char('r'));
        keys(&mut app, " EU");
        press(&mut app, KeyCode::Enter);
        let saved = crate::tui::workspace::Settings::read(&ini).config;
        assert_eq!(saved.filter_presets[0].name, "payments EU");
        press(&mut app, KeyCode::Char('d'));
        let screen = render(&mut app, 100, 30);
        assert!(
            screen
                .iter()
                .any(|l| l.contains("Delete the preset payments EU?")),
            "{screen:#?}"
        );
        press(&mut app, KeyCode::Enter);
        let saved = crate::tui::workspace::Settings::read(&ini).config;
        assert!(saved.filter_presets.is_empty());
    }

    #[test]
    fn the_palette_splits_the_file_from_a_prompt() {
        use crossterm::event::KeyCode;
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.log");
        let text: String = (0..30).map(|i| format!("line {i}\n")).collect();
        std::fs::write(&a, text).unwrap();
        let ini = dir.path().join("fasttail.ini");
        crate::config::FastTailConfig {
            open_files: vec![a.clone()],
            ..Default::default()
        }
        .save_to(&ini)
        .unwrap();
        let mut app = app_over(&ini);
        app.restore_dock(None);
        app.apply(Action::Split);
        assert_eq!(app.prompt.as_ref().map(|p| p.kind), Some(PromptKind::Split));
        app.prompt.as_mut().unwrap().field = TextField::new("12");
        press(&mut app, KeyCode::Enter);
        for _ in 0..20 {
            app.tick();
            if app.split.is_none() {
                break;
            }
        }
        assert!(app.split.is_none());
        assert!(dir.path().join("a.part-003.log").exists());
        assert!(app
            .message
            .as_deref()
            .is_some_and(|m| m.starts_with("3 files written")));
        // Again: the pieces exist, so it asks for `!`.
        app.apply(Action::Split);
        app.prompt.as_mut().unwrap().field = TextField::new("12");
        press(&mut app, KeyCode::Enter);
        assert!(app.split.is_none());
        assert!(app.message.as_deref().is_some_and(|m| m.contains("add !")));
        app.apply(Action::Split);
        app.prompt.as_mut().unwrap().field = TextField::new("bogus");
        press(&mut app, KeyCode::Enter);
        assert!(app
            .message
            .as_deref()
            .is_some_and(|m| m.contains("Cannot split")));
    }

    #[test]
    fn a_line_out_of_order_is_marked_counted_and_walked() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.log");
        std::fs::write(
            &a,
            "2026-09-28 14:00:00 a\n2026-09-28 14:10:00 b\n2026-09-28 14:04:00 back\n",
        )
        .unwrap();
        let ini = dir.path().join("fasttail.ini");
        crate::config::FastTailConfig {
            open_files: vec![a.clone()],
            ..Default::default()
        }
        .save_to(&ini)
        .unwrap();
        let mut app = app_over(&ini);
        app.restore_dock(None);
        let e = &mut app.tabs[0].engine;
        e.ensure_timestamps();
        while !e.timestamps_complete() {
            e.fill_timestamps();
        }
        assert_eq!(e.out_of_order_count(), 1);
        let screen = render(&mut app, 120, 30);
        assert!(
            screen.iter().any(|l| l.contains("1 out of order")),
            "{screen:#?}"
        );
        assert!(screen.iter().any(|l| l.contains("3<")), "{screen:#?}");
        app.apply(Action::NextOutOfOrder);
        assert_eq!(app.tabs[0].cursor_line(), Some(2));
        assert!(app
            .message
            .as_deref()
            .is_some_and(|m| m.contains("-00:06:00")));
    }

    #[test]
    fn the_time_delta_column_and_the_time_display_draw_in_the_rows() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.log");
        std::fs::write(
            &a,
            "2026-09-28T14:00:00Z a\n2026-09-28T14:00:01.200Z b\n2026-09-28T14:00:03Z c\n",
        )
        .unwrap();
        let ini = dir.path().join("fasttail.ini");
        crate::config::FastTailConfig {
            open_files: vec![a.clone()],
            ..Default::default()
        }
        .save_to(&ini)
        .unwrap();
        let mut app = app_over(&ini);
        app.restore_dock(None);
        app.apply(Action::ToggleTimeDelta);
        assert!(app.tabs[0].engine.show_time_delta);
        let e = &mut app.tabs[0].engine;
        e.ensure_timestamps();
        while !e.timestamps_complete() {
            e.fill_timestamps();
        }
        let screen = render(&mut app, 120, 30);
        assert!(
            screen
                .iter()
                .any(|l| l.contains("+1.200 2026-09-28T14:00:01.200Z b")),
            "{screen:#?}"
        );
        assert!(screen.iter().any(|l| l.contains("[x] Δt")), "{screen:#?}");
        // The chip moves on to UTC, then local time; a UTC stamp shown in UTC keeps its text.
        app.apply(Action::SetTimeDisplay(crate::timestamp::TimeDisplay::Utc));
        assert_eq!(
            app.tabs[0].engine.time_display(),
            crate::timestamp::TimeDisplay::Utc
        );
        let screen = render(&mut app, 120, 30);
        assert!(screen.iter().any(|l| l.contains("UTC")), "{screen:#?}");
        assert!(screen.iter().any(|l| l.contains(" c")), "{screen:#?}");
        app.apply(Action::ToggleTimeDelta);
        let screen = render(&mut app, 120, 30);
        assert!(!screen.iter().any(|l| l.contains("+1.200")), "{screen:#?}");
    }

    #[test]
    fn search_results_scope_export_and_report_work_in_the_terminal() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.log");
        let text: String = (1..=20)
            .map(|i| {
                if i % 5 == 0 {
                    format!("ERROR line {i}\n")
                } else {
                    format!("INFO line {i}\n")
                }
            })
            .collect();
        std::fs::write(&a, text).unwrap();
        let ini = dir.path().join("fasttail.ini");
        crate::config::FastTailConfig {
            open_files: vec![a.clone()],
            ..Default::default()
        }
        .save_to(&ini)
        .unwrap();
        let mut app = app_over(&ini);
        app.restore_dock(None);
        app.search("ERROR");
        assert_eq!(app.tabs[0].engine.search_matches, vec![4, 9, 14, 19]);
        // The results list goes to the hit chosen.
        app.apply(Action::SearchResults);
        assert!(app.results_menu.is_some());
        let screen = render(&mut app, 100, 30);
        assert!(
            screen.iter().any(|l| l.contains("4 search results")),
            "{screen:#?}"
        );
        app.go_to_result(2);
        assert_eq!(app.tabs[0].cursor_line(), Some(14));
        assert!(app.results_menu.is_none());
        // A line range: only the hits inside it count.
        app.submit_search_scope("6-15");
        assert_eq!(app.tabs[0].engine.search_matches, vec![9, 14]);
        let screen = render(&mut app, 100, 30);
        assert!(screen.iter().any(|l| l.contains("in range")), "{screen:#?}");
        // The matches as CSV, refused over an existing file until `!`.
        let csv = dir.path().join("m.csv");
        std::fs::write(&csv, "old").unwrap();
        app.start_export(
            crate::export::Scope::Matches,
            crate::export::Format::Csv,
            &csv.display().to_string(),
        );
        assert!(app.export.is_none());
        app.start_export(
            crate::export::Scope::Matches,
            crate::export::Format::Csv,
            &format!("{}!", csv.display()),
        );
        while app.export.is_some() {
            app.step_export();
        }
        let written = std::fs::read_to_string(&csv).unwrap();
        assert!(written.contains("ERROR line 10"), "{written}");
        assert!(!written.contains("ERROR line 5\n"), "{written}");
        // The bookmark report saved to a file.
        app.tabs[0].engine.toggle_bookmark(9);
        let md = dir.path().join("report.md");
        app.write_report(false, &md.display().to_string());
        let report = std::fs::read_to_string(&md).unwrap();
        assert!(report.contains("ERROR line 10"), "{report}");
    }

    #[test]
    fn a_range_after_a_replaced_timestamp_moves_with_it() {
        // `2026-01-01 10:00` (16 bytes) shown as `2026-01-01T09:00:00Z` (20 bytes).
        assert_eq!(shift_range(0..4, (0, 16), 20), 0..20);
        assert_eq!(shift_range(17..20, (0, 16), 20), 21..24);
        assert_eq!(shift_range(2..5, (6, 10), 2), 2..5);
        assert_eq!(shift_range(7..12, (6, 10), 2), 6..10);
    }

    #[test]
    fn the_global_expression_is_shown_and_a_stream_switches_to_an_expression() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.log");
        std::fs::write(&a, "payment\nrefund\nother\n").unwrap();
        let ini = dir.path().join("fasttail.ini");
        let mut config = crate::config::FastTailConfig {
            open_files: vec![a.clone()],
            ..Default::default()
        };
        config.global_filter.enabled = true;
        config.global_filter.expr_mode = true;
        config.global_filter.expression = "payment OR refund".into();
        config.save_to(&ini).unwrap();
        let mut app = app_over(&ini);
        app.restore_dock(None);
        app.apply(Action::EditGlobal);
        let screen = render(&mut app, 120, 30);
        assert!(
            screen
                .iter()
                .any(|l| l.contains("Include (expression)") && l.contains("payment OR refund")),
            "{screen:#?}"
        );
        app.global = None;
        // `I` switches the stream's include side to an expression built from its terms.
        app.tabs[0].engine.set_include_filter("refund");
        app.apply(Action::ToggleFilterExpr);
        assert!(app.tabs[0].engine.filter_expr_mode());
        assert_eq!(app.tabs[0].engine.include_display(), "refund");
        // `i` then edits the expression.
        app.submit_prompt(Prompt {
            kind: PromptKind::Include,
            field: TextField::new("payment OR refund"),
        });
        assert_eq!(app.tabs[0].engine.filter_expr(), "payment OR refund");
        let screen = render(&mut app, 120, 30);
        assert!(screen.iter().any(|l| l.contains("[x] ƒx")), "{screen:#?}");
    }

    #[test]
    fn rules_are_walked_imported_and_exported_and_bookmarks_cleared() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.log");
        let text: String = (1..=12)
            .map(|i| {
                if i % 4 == 0 {
                    format!("timeout {i}\n")
                } else {
                    format!("ok {i}\n")
                }
            })
            .collect();
        std::fs::write(&a, text).unwrap();
        let ini = dir.path().join("fasttail.ini");
        crate::config::FastTailConfig {
            open_files: vec![a.clone()],
            highlight_rules: vec![crate::tail_engine::HighlightRule::new(
                "timeout",
                [255, 255, 0],
                [0, 0, 0],
                false,
            )],
            ..Default::default()
        }
        .save_to(&ini)
        .unwrap();
        let mut app = app_over(&ini);
        app.restore_dock(None);
        app.tabs[0].engine.follow_tail = false;
        app.tabs[0].set_cursor(3);
        // F4 from a matching row: the next line of that rule, then wrapping round.
        app.apply(Action::RuleNext);
        assert_eq!(app.tabs[0].cursor_line(), Some(7));
        app.apply(Action::RulePrev);
        assert_eq!(app.tabs[0].cursor_line(), Some(3));
        // Export, then import adds nothing new; with `!` it replaces.
        let set = dir.path().join("team.fasttail-rules.ini");
        app.export_rules(&set.display().to_string());
        assert!(set.exists());
        app.import_rules(&set.display().to_string());
        assert_eq!(app.settings_mut().config.highlight_rules.len(), 1);
        app.import_rules(&format!("{}!", set.display()));
        assert_eq!(app.settings_mut().config.highlight_rules.len(), 1);
        assert!(app
            .message
            .as_deref()
            .is_some_and(|m| m.contains("replacing")));
        // Clear all bookmarks.
        app.tabs[0].engine.toggle_bookmark(1);
        app.tabs[0].engine.toggle_bookmark(2);
        app.apply(Action::ClearBookmarks);
        assert!(app.tabs[0].engine.bookmarks.is_empty());
    }

    #[test]
    fn lines_are_compared_and_sent_to_the_scratchpad() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.log");
        std::fs::write(
            &a,
            "GET /api took=120\nGET /api took=480\nother\nGET /api took=120\n",
        )
        .unwrap();
        let ini = dir.path().join("fasttail.ini");
        crate::config::FastTailConfig {
            open_files: vec![a.clone()],
            ..Default::default()
        }
        .save_to(&ini)
        .unwrap();
        let mut app = app_over(&ini);
        app.restore_dock(None);
        // Two rows selected: compared at once.
        app.tabs[0].set_cursor(0);
        app.apply(Action::SelectDown);
        app.apply(Action::Compare);
        let v = app.compare.as_ref().expect("compare open");
        assert!(!v.is_equal());
        let screen = render(&mut app, 120, 30);
        assert!(
            screen.iter().any(|l| l.contains("1 changes")),
            "{screen:#?}"
        );
        app.on_compare_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Esc,
            crossterm::event::KeyModifiers::NONE,
        ));
        assert!(app.compare.is_none());
        // One row: marked, then compared with another.
        app.tabs[0].engine.clear_selection();
        app.tabs[0].set_cursor(0);
        app.apply(Action::Compare);
        assert!(app.compare_mark.is_some());
        app.tabs[0].set_cursor(3);
        app.apply(Action::Compare);
        assert!(app.compare.as_ref().is_some_and(|v| v.is_equal()));
        app.compare = None;
        // Sent to the pad under a reference line, saved beside fasttail.ini.
        app.tabs[0].set_cursor(1);
        app.apply(Action::SendToScratchpad);
        let pad = app.scratchpad.as_ref().unwrap();
        assert_eq!(pad.text, "── a.log:2 ──\nGET /api took=480\n");
        app.save_scratchpad();
        let file = dir.path().join("scratchpad.txt");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), pad_text(&app));
        // The editor: Ctrl+G on the line goes to its stream line.
        app.apply(Action::Scratchpad);
        let e = app.pad_editor.as_mut().unwrap();
        e.row = 1;
        app.on_pad_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('g'),
            crossterm::event::KeyModifiers::CONTROL,
        ));
        assert!(app.pad_editor.is_none());
        render(&mut app, 120, 30);
        assert_eq!(app.tabs[0].cursor_line(), Some(1));
    }

    fn pad_text(app: &App) -> String {
        app.scratchpad
            .as_ref()
            .map(|p| p.text.clone())
            .unwrap_or_default()
    }

    #[test]
    fn the_analysis_views_count_filter_and_go_to_lines() {
        use crate::tui::analysis::{build, AnalysisKey, Kind, RowAction};
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.log");
        let mut text = String::new();
        for i in 0..30 {
            let status = if i % 3 == 0 { 500 } else { 200 };
            text.push_str(&format!(
                "ts=2026-10-29T10:00:{i:02}Z level=info status={status} id=op{} msg=\"done {i}\"\n",
                i % 4
            ));
        }
        std::fs::write(&a, text).unwrap();
        let ini = dir.path().join("fasttail.ini");
        crate::config::FastTailConfig {
            open_files: vec![a.clone()],
            ..Default::default()
        }
        .save_to(&ini)
        .unwrap();
        let mut app = app_over(&ini);
        app.restore_dock(None);
        let settle = |app: &mut App| {
            for _ in 0..500 {
                app.tabs[0].engine.poll_updates();
                let e = &app.tabs[0].engine;
                let busy = e.stats.as_ref().is_some_and(|s| !s.done())
                    || e.operations.as_ref().is_some_and(|o| !o.done())
                    || e.patterns.as_ref().is_some_and(|p| p.running())
                    || e.query.as_ref().is_some_and(|q| q.running());
                if !busy {
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        };
        // Statistics of `status`: 200 then 500, Enter includes the value.
        app.open_source_view(
            crate::field_stats::StatsSource::Field("status".into()),
            true,
        );
        settle(&mut app);
        let t = build(Kind::Stats, &mut app.tabs[0].engine, Language::En, false);
        assert_eq!(t.rows[0].cells[0], "200", "{t:?}");
        assert_eq!(t.rows[0].cells[1], "20");
        let screen = render(&mut app, 120, 30);
        assert!(
            screen.iter().any(|l| l.contains("status: 30 lines")),
            "{screen:#?}"
        );
        let enter = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Down,
            crossterm::event::KeyModifiers::NONE,
        );
        app.on_analysis_key(enter);
        app.on_analysis_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));
        assert_eq!(app.tabs[0].engine.include_filter(), "status=500");
        app.tabs[0].engine.set_include_filter("");
        app.analysis = None;
        // Query: a stats pipeline.
        app.open_analysis(Kind::Query);
        app.tabs[0].engine.run_query("stats count() by status");
        settle(&mut app);
        let t = build(Kind::Query, &mut app.tabs[0].engine, Language::En, false);
        assert_eq!(t.rows.len(), 2, "{t:?}");
        app.analysis = None;
        // Operations by `id`: four of them, Enter goes to the first line of one.
        app.open_source_view(crate::field_stats::StatsSource::Field("id".into()), false);
        settle(&mut app);
        let t = build(
            Kind::Operations,
            &mut app.tabs[0].engine,
            Language::En,
            false,
        );
        assert_eq!(t.rows.len(), 4, "{t:?}");
        assert_eq!(t.rows[1].action, RowAction::Line(1));
        let v = app.analysis.as_mut().unwrap();
        v.selected = 0;
        let shown = v.shown(&t);
        let k = v.on_key(
            crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Enter,
                crossterm::event::KeyModifiers::NONE,
            ),
            &t,
        );
        assert!(
            matches!(k, AnalysisKey::Activate(RowAction::Line(_), false)),
            "{shown:?}"
        );
        // Patterns: learnt in the background, listed most frequent first.
        app.analysis = None;
        app.open_analysis(Kind::Patterns);
        settle(&mut app);
        let t = build(Kind::Patterns, &mut app.tabs[0].engine, Language::En, false);
        assert!(!t.rows.is_empty(), "{t:?}");
        // Explain the last ten seconds against the rest.
        let e = &mut app.tabs[0].engine;
        e.ensure_timestamps();
        while !e.timestamps_complete() {
            e.fill_timestamps();
        }
        let (a, b) = (e.line_timestamp(20).unwrap(), e.line_timestamp(29).unwrap());
        e.explain_span((a, b), crate::spike::Baseline::Rest, false);
        for _ in 0..500 {
            app.tabs[0].engine.poll_updates();
            if app.tabs[0]
                .engine
                .explain
                .as_ref()
                .is_some_and(|x| !x.running())
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let t = build(Kind::Explain, &mut app.tabs[0].engine, Language::En, false);
        assert!(
            t.summary
                .iter()
                .any(|l| l.contains("10 entries in the span")),
            "{t:?}"
        );
    }

    #[test]
    fn keys_are_remapped_in_the_keys_dialog_and_settings_toggle_from_the_palette() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.log");
        std::fs::write(&a, "one\ntwo\n").unwrap();
        let ini = dir.path().join("fasttail.ini");
        crate::config::FastTailConfig {
            open_files: vec![a.clone()],
            ..Default::default()
        }
        .save_to(&ini)
        .unwrap();
        let mut app = app_over(&ini);
        app.restore_dock(None);
        app.apply(Action::KeysDialog);
        let follow = keys::REMAPPABLE
            .iter()
            .position(|(n, _, _)| *n == "follow")
            .unwrap();
        app.keys_dialog.as_mut().unwrap().selected = follow;
        let press = |app: &mut App, code| app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('f'));
        let saved = crate::tui::workspace::Settings::read(&ini).config;
        assert_eq!(
            saved.shortcuts.get("tui.follow").map(String::as_str),
            Some("f")
        );
        assert_eq!(
            saved.shortcuts.get("tui.global.toggle").map(String::as_str),
            Some(""),
            "f was taken from the global filter switch"
        );
        let screen = render(&mut app, 100, 40);
        assert!(
            screen
                .iter()
                .any(|l| l.contains("taken from global.toggle")),
            "{screen:#?}"
        );
        press(&mut app, KeyCode::Esc);
        assert!(app.keys_dialog.is_none());
        let before = app.tabs[0].engine.follow_tail;
        press(&mut app, KeyCode::Char('f'));
        assert_ne!(app.tabs[0].engine.follow_tail, before, "f follows now");
        press(&mut app, KeyCode::Char(' '));
        assert_ne!(
            app.tabs[0].engine.follow_tail, before,
            "Space no longer does"
        );
        // A setting toggled from the palette.
        let colors = app.settings_mut().config.level_colors;
        app.apply(Action::ToggleSetting(
            crate::actions::BoolSetting::LevelColors,
        ));
        assert_eq!(app.settings_mut().config.level_colors, !colors);
        assert_eq!(app.palette.level_colors, !colors);
        assert!(crate::tui::palette::entries(Language::En)
            .iter()
            .any(|e| e.action == Action::ToggleSetting(crate::actions::BoolSetting::Sound)));
    }

    #[test]
    fn the_web_view_mcp_server_and_plugins_run_from_the_terminal() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let free = || {
            std::net::TcpListener::bind("127.0.0.1:0")
                .unwrap()
                .local_addr()
                .unwrap()
                .port()
        };
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.log");
        std::fs::write(&a, "one\ntwo\n").unwrap();
        let ini = dir.path().join("fasttail.ini");
        crate::config::FastTailConfig {
            open_files: vec![a.clone()],
            web_enabled: true,
            web_port: free(),
            mcp_enabled: true,
            mcp_port: free(),
            ..Default::default()
        }
        .save_to(&ini)
        .unwrap();
        let mut app = app_over(&ini);
        app.restore_dock(None);
        app.mcp_view.runtime_path = Some(dir.path().join("mcp.runtime"));
        app.tick();
        assert!(app.web_view.connections().is_some(), "{:?}", app.message);
        assert!(app.mcp_view.port().is_some(), "{:?}", app.message);
        let screen = render(&mut app, 120, 30);
        assert!(
            screen
                .iter()
                .any(|l| l.contains("web :") && l.contains("MCP :")),
            "{screen:#?}"
        );
        // Off in Settings: both stop.
        app.settings_mut().config.web_enabled = false;
        app.settings_mut().config.mcp_enabled = false;
        app.tick();
        assert!(app.web_view.connections().is_none());
        assert!(app.mcp_view.port().is_none());
        // A source plugin is approved only after its hash is shown.
        let plugins = dir.path().join("plugins");
        let one = plugins.join("echo");
        std::fs::create_dir_all(&one).unwrap();
        std::fs::write(
            one.join("plugin.toml"),
            "api = 1\nname = \"echo\"\nversion = \"1.0\"\nkind = \"source\"\n[source]\ncommand = \"echo\"\n",
        )
        .unwrap();
        app.plugins = Some(PluginsDialog {
            catalog: crate::plugins::discover(&plugins),
            selected: 0,
            confirm: None,
        });
        let press = |app: &mut App, code| app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
        press(&mut app, KeyCode::Char(' '));
        assert_eq!(
            app.plugins.as_ref().unwrap().confirm.as_deref(),
            Some("echo")
        );
        let screen = render(&mut app, 120, 30);
        assert!(
            screen
                .iter()
                .any(|l| l.contains("y runs the programs of echo")),
            "{screen:#?}"
        );
        press(&mut app, KeyCode::Char('y'));
        assert!(app
            .settings_mut()
            .config
            .plugins
            .approved
            .contains_key("echo"));
        press(&mut app, KeyCode::Char(' '));
        assert!(!app
            .settings_mut()
            .config
            .plugins
            .approved
            .contains_key("echo"));
    }

    #[test]
    fn a_source_location_opens_in_fasttail_at_its_line() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("Orders.java");
        let body: String = (1..=50).map(|i| format!("line {i}\n")).collect();
        std::fs::write(&src, body).unwrap();
        let a = dir.path().join("a.log");
        std::fs::write(&a, format!("boom at {}:42\n", src.display())).unwrap();
        let ini = dir.path().join("fasttail.ini");
        crate::config::FastTailConfig {
            open_files: vec![a.clone()],
            ..Default::default()
        }
        .save_to(&ini)
        .unwrap();
        let mut app = app_over(&ini);
        app.restore_dock(None);
        app.tabs[0].set_cursor(0);
        app.apply(Action::OpenSourceHere);
        assert_eq!(app.tabs.len(), 2, "{:?}", app.message);
        assert!(crate::paths::paths_equal(
            &app.tabs[app.active].engine.path,
            &src
        ));
        render(&mut app, 100, 30);
        assert_eq!(app.tabs[app.active].cursor_line(), Some(41));
    }

    #[test]
    fn the_global_filter_is_edited_applied_after_the_delay_and_switched_with_f() {
        use crossterm::event::KeyCode;
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a.log"), dir.path().join("b.log"));
        std::fs::write(&a, "GET /api\nGET /healthcheck\n").unwrap();
        std::fs::write(&b, "healthcheck ok\nlogin\n").unwrap();
        let ini = dir.path().join("fasttail.ini");
        crate::config::FastTailConfig {
            open_files: vec![a.clone(), b.clone()],
            ..Default::default()
        }
        .save_to(&ini)
        .unwrap();
        let mut app = app_over(&ini);
        app.restore_dock(None);
        let rows = |app: &App, i: usize| app.tabs[i].engine.visible_line_count();
        assert_eq!((rows(&app, 0), rows(&app, 1)), (2, 2));
        // F: on, then the exclude term healthcheck.
        app.apply(Action::EditGlobal);
        let screen = render(&mut app, 100, 30);
        assert!(
            screen.iter().any(|l| l.contains("Exclude (none match)")),
            "{screen:#?}"
        );
        for _ in 0..4 {
            press(&mut app, KeyCode::Up);
        }
        press(&mut app, KeyCode::Char(' '));
        // On, Match case, Regex, Expression, Include, then the exclude row.
        for _ in 0..5 {
            press(&mut app, KeyCode::Down);
        }
        keys(&mut app, "healthcheck");
        // Not yet: it waits 300 ms after the last key.
        app.tick();
        assert_eq!(rows(&app, 1), 2);
        std::thread::sleep(Duration::from_millis(
            crate::global_filter::APPLY_DELAY_MS + 50,
        ));
        assert!(app.tick());
        assert_eq!((rows(&app, 0), rows(&app, 1)), (1, 1));
        let saved = crate::tui::workspace::Settings::read(&ini).config;
        assert!(saved.global_filter.enabled);
        assert_eq!(saved.global_filter.exclude, ["healthcheck"]);
        press(&mut app, KeyCode::Esc);
        assert!(app.global.is_none());
        // f switches it off and on again, at once.
        app.apply(Action::ToggleGlobal);
        assert_eq!(app.message.as_deref(), Some("Global filter off"));
        assert_eq!(rows(&app, 1), 2);
        app.apply(Action::ToggleGlobal);
        assert_eq!(rows(&app, 1), 1);
        // A stream opened later applies it too.
        let c = dir.path().join("c.log");
        std::fs::write(&c, "healthcheck\nother\n").unwrap();
        app.open_file(&c);
        assert_eq!(rows(&app, 2), 1);
    }

    #[test]
    fn tools_run_from_the_menu_by_shortcut_and_for_their_rule_with_the_limits() {
        use crate::external_tools::ExternalTool;
        use crate::tail_engine::HighlightRule;
        use crossterm::event::{KeyCode, KeyEvent};
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("app.log");
        std::fs::write(&a, "start\nERROR one\n").unwrap();
        // The test binary itself: a program every platform can start, that exits at once.
        let me = std::env::current_exe().unwrap().display().to_string();
        let ini = dir.path().join("fasttail.ini");
        crate::config::FastTailConfig {
            open_files: vec![a.clone()],
            highlight_rules: vec![HighlightRule::new("FATAL", [255, 0, 0], [0; 3], false)],
            external_tools: vec![
                ExternalTool {
                    shortcut: Some("Ctrl+Shift+E".into()),
                    ..ExternalTool::new("editor", &me, "--list {file}:{lineno}")
                },
                ExternalTool {
                    bound_rule: Some("FATAL".into()),
                    ..ExternalTool::new("notify", &me, "--list {line}")
                },
                ExternalTool::new("missing", "/nonexistent/fasttail-tool", ""),
            ],
            ..Default::default()
        }
        .save_to(&ini)
        .unwrap();
        let mut app = app_over(&ini);
        app.restore_dock(None);
        let screen = {
            app.apply(Action::Tools);
            render(&mut app, 100, 30)
        };
        assert!(
            screen.iter().any(|l| l.contains("1. editor")),
            "{screen:#?}"
        );
        assert!(
            screen.iter().any(|l| l.contains("rule FATAL")),
            "{screen:#?}"
        );
        // From the menu, on the cursor row.
        press(&mut app, KeyCode::Enter);
        assert!(app.tool_menu.is_none());
        assert_eq!(app.message.as_deref(), Some("Started editor"));
        // A program that cannot start says so.
        app.apply(Action::Tools);
        press(&mut app, KeyCode::Char('3'));
        assert!(app
            .message
            .as_deref()
            .unwrap()
            .starts_with("Cannot start missing"));
        // By its shortcut, from the view.
        app.message = None;
        let cs = KeyModifiers::CONTROL | KeyModifiers::SHIFT;
        assert!(app.on_key(KeyEvent::new(KeyCode::Char('E'), cs)));
        assert_eq!(app.message.as_deref(), Some("Started editor"));
        // Three FATAL lines in a row: the bound tool runs once, the other two are dropped.
        app.run_bound_tools();
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new().append(true).open(&a).unwrap();
        writeln!(f, "FATAL a\nFATAL b\nFATAL c").unwrap();
        drop(f);
        let start = Instant::now();
        while app.tabs[0].engine.total_lines() < 5 && start.elapsed() < Duration::from_secs(5) {
            app.tabs[0].engine.poll_updates();
            std::thread::sleep(Duration::from_millis(20));
        }
        app.run_bound_tools();
        assert_eq!(app.tool_runner.dropped_for("notify"), 2);
        assert_eq!(app.tool_runner.last_error, None);
    }

    #[test]
    fn the_tools_editor_opens_from_settings_and_saves_the_tools() {
        use crossterm::event::KeyCode;
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("app.log");
        std::fs::write(&a, "line\n").unwrap();
        let ini = dir.path().join("fasttail.ini");
        crate::config::FastTailConfig {
            open_files: vec![a.clone()],
            ..Default::default()
        }
        .save_to(&ini)
        .unwrap();
        let mut app = app_over(&ini);
        app.restore_dock(None);
        // Settings: the External tools row, Enter opens the editor over it.
        app.apply(Action::Settings);
        let form = app.settings_form.as_mut().unwrap();
        form.focus = form
            .fields
            .iter()
            .position(|f| f.key == crate::tui::settings::Key::Tools)
            .unwrap();
        press(&mut app, KeyCode::Enter);
        assert!(app.tools_editor.is_some());
        assert!(app.settings_form.is_some(), "Settings stays under it");
        // A new tool: name, program, arguments kept as {file}, then OK.
        press(&mut app, KeyCode::Char('a'));
        keys(&mut app, "editor");
        press(&mut app, KeyCode::Down);
        keys(&mut app, "code");
        let screen = render(&mut app, 100, 30);
        assert!(screen.iter().any(|l| l.contains("{lineno}")), "{screen:#?}");
        press(&mut app, KeyCode::Enter);
        let saved = crate::tui::workspace::Settings::read(&ini).config;
        assert_eq!(saved.external_tools.len(), 1);
        assert_eq!(saved.external_tools[0].name, "editor");
        assert_eq!(saved.external_tools[0].program, "code");
        assert_eq!(saved.external_tools[0].args, "{file}");
        // Esc goes back to Settings, whose row counts the tool.
        press(&mut app, KeyCode::Esc);
        assert!(app.tools_editor.is_none());
        let screen = render(&mut app, 100, 30);
        assert!(
            screen.iter().any(|l| l.contains("1 defined")),
            "{screen:#?}"
        );
        press(&mut app, KeyCode::Esc);
        // The ! menu lists it, and e opens the editor too.
        app.apply(Action::Tools);
        let screen = render(&mut app, 100, 30);
        assert!(
            screen.iter().any(|l| l.contains("1. editor")),
            "{screen:#?}"
        );
        press(&mut app, KeyCode::Char('e'));
        assert!(app.tool_menu.is_none());
        assert!(app.tools_editor.is_some());
    }

    #[test]
    fn the_lock_hides_everything_drops_keys_counts_failures_and_restores_the_state() {
        use crossterm::event::{KeyCode, KeyEvent};
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("secret-app.log");
        std::fs::write(&a, "password=hunter2\n").unwrap();
        let ini = dir.path().join("fasttail.ini");
        crate::config::FastTailConfig {
            open_files: vec![a.clone()],
            lock_pin: crate::lock::scramble_pin("4821"),
            lock_enabled: true,
            screensaver_timeout_mins: 1,
            ..Default::default()
        }
        .save_to(&ini)
        .unwrap();
        let mut app = app_over(&ini);
        app.restore_dock(None);
        app.apply(Action::StartSearch);
        // Ctrl+L: only the PIN dialog, nothing of the stream.
        app.on_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL));
        assert!(app.locked.is_some());
        let screen = render(&mut app, 100, 30).join("\n");
        assert!(screen.contains("FastTail is locked"), "{screen}");
        for hidden in ["hunter2", "secret-app", "1/1", "FOLLOW", "help"] {
            assert!(!screen.contains(hidden), "{hidden} shown: {screen}");
        }
        // Esc, q, Ctrl+C and the mouse do nothing.
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Char('q'));
        app.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(!app.on_mouse(click(5, 5)));
        assert!(app.locked.is_some() && !app.quit);
        // Three wrong PINs start the cooldown; typing is dropped meanwhile.
        for _ in 0..3 {
            keys(&mut app, "0000");
            press(&mut app, KeyCode::Enter);
        }
        let screen = render(&mut app, 100, 30).join("\n");
        assert!(screen.contains("try again in"), "{screen}");
        assert!(app.lock_attempts.cooldown_left(Instant::now()).is_some());
        keys(&mut app, "4821");
        press(&mut app, KeyCode::Enter);
        assert!(app.locked.is_some(), "no attempt during the cooldown");
        // After it, the right PIN opens it, and the open dialog is still there.
        app.lock_attempts = Default::default();
        app.locked = Some(LockScreen::default());
        keys(&mut app, "4821");
        press(&mut app, KeyCode::Enter);
        assert!(app.locked.is_none());
        assert!(app.prompt.is_some(), "the search dialog is back");
        // The maintenance phrase opens it too.
        app.lock();
        keys(&mut app, "joshua");
        press(&mut app, KeyCode::Enter);
        assert!(app.locked.is_none());
        // The idle lock: a minute without a key or mouse event.
        app.idle = crate::lock::IdleClock::new(Instant::now() - Duration::from_secs(61));
        assert!(app.tick());
        assert!(app.locked.is_some());
        // Without a PIN nothing locks.
        app.locked = None;
        app.settings.as_mut().unwrap().config.lock_pin.clear();
        app.lock();
        assert!(app.locked.is_none());
    }

    /// Settings with the graphical interface picked: saved, then the switch dialog.
    /// "Switch now" goes through the test stub (`App::start_gui`): first failing, so the
    /// dialog says why and nothing quits, then succeeding, so the interface quits.
    #[test]
    fn picking_the_graphical_interface_offers_to_switch_and_reports_a_failure() {
        use crate::config::Interface;
        use crossterm::event::KeyCode;
        let (mut app, dir) = app_with(&[("a.log", LOG)], false);
        let ini = dir.path().join("fasttail.ini");
        let mut settings = crate::tui::workspace::Settings {
            path: ini.clone(),
            ..Default::default()
        };
        settings.config.interface = Interface::Tui;
        app.settings = Some(settings);
        app.open_settings();
        let form = app.settings_form.as_mut().unwrap();
        form.focus = form
            .fields
            .iter()
            .position(|f| f.key == crate::tui::settings::Key::Interface)
            .unwrap();
        press(&mut app, KeyCode::Left);
        press(&mut app, KeyCode::Enter);
        if !cfg!(feature = "gui") {
            // The terminal-only build has no choice to make, so nothing to offer.
            assert!(app.interface_switch.is_none());
            assert_eq!(
                app.settings.as_ref().unwrap().config.interface,
                Interface::Tui
            );
            return;
        }
        assert_eq!(
            app.settings.as_ref().unwrap().config.interface,
            Interface::Gui
        );
        assert_eq!(app.interface_switch, Some(InterfaceSwitch::Offer));
        let screen = render(&mut app, 90, 24).join("\n");
        assert!(screen.contains("Switch interface"), "{screen}");
        // "At next start": the choice stays saved, nothing else happens.
        press(&mut app, KeyCode::Esc);
        assert!(app.interface_switch.is_none() && !app.quit);
        // "Switch now" from the dialog again.
        app.interface_switch = Some(InterfaceSwitch::Offer);
        press(&mut app, KeyCode::Enter);
        assert!(!app.quit);
        let Some(InterfaceSwitch::Failed(why)) = app.interface_switch.clone() else {
            panic!("{:?}", app.interface_switch);
        };
        let screen = render(&mut app, 120, 24).join("\n");
        assert!(
            screen.contains("Graphical interface not started"),
            "{screen}"
        );
        assert!(screen.contains(&why[..20.min(why.len())]), "{screen}");
        // The workspace was saved before trying.
        let saved = crate::tui::workspace::Settings::read(&ini).config;
        assert_eq!(saved.interface, Interface::Gui);
        press(&mut app, KeyCode::Enter);
        assert!(app.interface_switch.is_none());
        // Started: the terminal interface quits.
        app.start_gui = || Ok(());
        app.interface_switch = Some(InterfaceSwitch::Offer);
        press(&mut app, KeyCode::Enter);
        assert!(app.quit && app.interface_switch.is_none());
    }

    /// Settings `[ OK ]` with the automatic highlighting on reaches the open streams, and
    /// a row then carries the token spans the terminal colours.
    #[test]
    fn automatic_highlighting_reaches_the_open_streams() {
        use crate::auto_highlight::{TokenKind, TokenKinds};
        use crate::tail_engine::SpanStyle;
        use crossterm::event::KeyCode;
        let (mut app, dir) = app_with(&[("a.log", "GET from 10.0.0.7 took 35ms\n")], false);
        app.settings = Some(crate::tui::workspace::Settings {
            path: dir.path().join("fasttail.ini"),
            ..Default::default()
        });
        assert!(app.tabs[0].engine.auto_tokens().is_empty());
        app.open_settings();
        let form = app.settings_form.as_mut().unwrap();
        form.focus = form
            .fields
            .iter()
            .position(|f| f.key == crate::tui::settings::Key::AutoHighlight)
            .unwrap();
        press(&mut app, KeyCode::Char(' '));
        press(&mut app, KeyCode::Enter);
        assert!(app.settings_form.is_none());
        let engine = &mut app.tabs[0].engine;
        assert_eq!(engine.auto_tokens(), TokenKinds::ALL);
        render(&mut app, 80, 10);
        let engine = &app.tabs[0].engine;
        assert!(engine.has_span_rules());
        let painted = engine.match_row_spans(&engine.get_row(0).expect("row 0"));
        assert!(
            painted
                .spans
                .iter()
                .any(|s| s.style == SpanStyle::Token(TokenKind::Ip)),
            "{:?}",
            painted.spans
        );
    }

    #[test]
    fn the_interface_field_matches_the_build() {
        let form = crate::tui::settings::SettingsForm::from_config(&Default::default());
        let field = &form.fields[0];
        assert_eq!(field.key, crate::tui::settings::Key::Interface);
        match &field.widget {
            #[cfg(feature = "gui")]
            crate::tui::settings::Widget::Radio(r) => {
                assert!(r.options[0].starts_with("graphical"));
            }
            #[cfg(not(feature = "gui"))]
            crate::tui::settings::Widget::Button(text) => {
                assert!(text.contains("not in this build"));
            }
            other => panic!("{other:?}"),
        }
    }
}
