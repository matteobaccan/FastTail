// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Key bindings: terminal key events mapped to actions, kept apart from the app so the
//! table can be tested without a terminal.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// What a key does in the log view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    NextTab,
    PrevTab,
    /// Zero-based tab index (ALT + 1..9).
    GotoTab(usize),
    ToggleFollow,
    /// Jump to the last row and follow.
    Bottom,
    Top,
    LineUp,
    LineDown,
    /// SHIFT + UP / DOWN: extends the selection from the cursor row.
    SelectUp,
    SelectDown,
    PageUp,
    PageDown,
    ScrollLeft,
    ScrollRight,
    /// CTRL + LEFT / RIGHT: sideways by `HSCROLL_FAST` columns.
    ScrollLeftFast,
    ScrollRightFast,
    ScrollHome,
    StartSearch,
    SearchNext,
    SearchPrev,
    ClearSearch,
    EditInclude,
    EditExclude,
    CycleCollapse,
    CycleLevel,
    /// Hide the selected lines (or the cursor row) from the view.
    HideLines,
    /// Open the cursor row's source location in the editor (the next one when pressed
    /// again on the same row).
    OpenSource,
    /// Open the stream again with its file's rotations (when the stream bar offers them).
    IncludeRotations,
    /// The column view of the fields on or off.
    ToggleColumns,
    /// `s` `|`: the focused window splits side by side; `_`: stacked. A window with
    /// several tabs gives its shown one to the new window, a window with one takes the
    /// next stream.
    SplitRight,
    SplitDown,
    /// ALT + X: the focused window closes, its tabs joining the window beside it.
    ClosePane,
    /// CTRL + W: the focused stream closes (its window too when it was its last tab).
    CloseStream,
    /// ALT + F: the focused window floats over the dock, or a floating one docks back.
    ToggleFloat,
    /// ALT + arrows: the nearest divider of the focused window moves that way.
    ResizeLeft,
    ResizeRight,
    ResizeUp,
    ResizeDown,
    /// `<` / `>`: the focused stream moves to the previous / next window as a tab.
    MovePrevPane,
    MoveNextPane,
    /// CTRL + PGUP / PGDN: the previous / next tab of the focused window.
    PrevInPane,
    NextInPane,
    ToggleHelp,
    /// `y`: copies the selected rows.
    Copy,
    /// CTRL + C: copies the selection when there is one, else quits.
    CopyOrQuit,
    /// `b` / CTRL + F2: bookmark on the cursor row, on or off.
    ToggleBookmark,
    /// `]` / F2 and `[` / SHIFT + F2: next / previous visible bookmark, wrapping around.
    NextBookmark,
    /// The next / previous line whose time goes back (palette, the bar's chip).
    NextOutOfOrder,
    /// Split file... (palette): asks how to cut.
    Split,
    /// Reconnect an ended system source (palette, the bar's chip).
    Reconnect,
    /// Open system source (palette): asks for a URI.
    OpenSystem,
    /// Open remote file (palette): asks for an `ssh://` URL.
    OpenRemote,
    /// The clipboard text as a new stream (palette).
    PasteStream,
    /// The clipboard text at the end of the focused clipboard stream (palette).
    PasteAppend,
    /// Listen (palette): asks for a listener URL.
    OpenListen,
    OpenDebugOutput,
    /// Open Event Log (palette, Windows): asks for an `eventlog://` URI.
    OpenEventLog,
    PrevOutOfOrder,
    PrevBookmark,
    /// `m`: the note of the cursor row's bookmark.
    EditNote,
    /// CTRL + K: the cursor row in context (filters suspended), and back.
    ToggleContext,
    /// CTRL + SHIFT + F, ALT + /: search all streams (the Find results window).
    FindAll,
    /// CTRL + G: go to a line (`N`, `+N`, `-N`) or a time (`14:02`).
    GoTo,
    /// `:`: the command palette (a number there goes to that line).
    Palette,
    /// `h`: the HEX view of the bytes, and back to the lines.
    ToggleHex,
    /// `d`: the ASM view (x86 instructions), and back to HEX; `D`: its next
    /// architecture.
    ToggleAsm,
    CycleArch,
    /// `a`: how escape sequences show: auto, render, strip, raw.
    CycleAnsi,
    /// `#`: the line numbers of the focused stream, on or off.
    ToggleLineNumbers,
    /// ALT + R: the search results of the focused stream as a list.
    SearchResults,
    /// ALT + S: the part of the view the search covers (lines or times).
    SearchScope,
    /// ALT + E: export visible lines, selected lines or matches as text, CSV or HTML.
    Export,
    /// `Y`: copies the selected rows (or the cursor row) as the view shows them.
    CopyAsShown,
    /// The bookmark report of the focused stream, or of every stream (palette).
    BookmarkReport,
    BookmarkReportAll,
    /// `I`: the include side of the focused stream as an expression, or back to terms.
    ToggleFilterExpr,
    /// F4 / SHIFT + F4: the next / previous line of the navigation rule.
    RuleNext,
    RulePrev,
    /// Remove every bookmark of the focused stream (palette).
    ClearBookmarks,
    /// ALT + O: the cursor row's source location opened in FastTail, at its line.
    OpenSourceHere,
    /// The Log formats dialog (palette).
    LogFormats,
    /// `=`: compare the two selected rows, or mark the selection and compare it with the
    /// next one.
    Compare,
    /// ALT + N: the scratchpad's editor.
    Scratchpad,
    /// ALT + A: the analysis views (Statistics, Query, Patterns, Explain, Operations).
    Analysis,
    /// ALT + M: the merged view dialog (streams to merge by time, or the focused merged
    /// view's sources on and off).
    MergedView,
    /// ALT + T: the focused stream's filter as a new tab of the same file (the window's
    /// "Open filter as new tab").
    FilterTab,
    /// `@`: the time anchor of the Δt column on the cursor row, or off when it is there.
    TimeAnchor,
    /// The time anchor off (palette).
    TimeAnchorClear,
    /// CTRL + A: every visible line selected.
    SelectAll,
    /// The cursor row's bookmark removed (palette).
    RemoveBookmark,
    /// Auto-update of the focused stream on or off (palette, the bar's check box).
    ToggleMonitor,
    /// Save the current session, save the workspace as the default session, clear the
    /// recent sessions (palette).
    SaveCurrentSession,
    SaveDefaultSession,
    ClearRecentSessions,
    /// The recent files: a list to open or remove one; clear the list (palette).
    RecentFiles,
    ClearRecentFiles,
    /// The Patterns view of the focused stream (palette).
    Patterns,
    /// The Keys dialog: the remappable terminal keys (palette).
    KeysDialog,
    /// The Plugins dialog (palette).
    Plugins,
    /// A boolean setting of the palette ("Toggle line numbers in new streams"...).
    ToggleSetting(crate::actions::BoolSetting),
    /// CTRL + N: the selected rows (or the cursor row) sent to the scratchpad.
    SendToScratchpad,
    /// ALT + D: the time delta column of the focused stream, on or off.
    ToggleTimeDelta,
    /// How the rows show their leading timestamp (palette, the bar's chip).
    SetTimeDisplay(crate::timestamp::TimeDisplay),
    /// CTRL + SHIFT + 1..9: the search text as a quick label of colour N (or recoloured,
    /// or removed when it has that colour), as in the GUI.
    Label(u8),
    /// `L`: the next key `1`-`9` is the colour of a quick label (terminals that do not
    /// deliver CTRL + SHIFT + digit).
    LabelPrefix,
    /// `t`: the time range dialog.
    TimeRange,
    /// `o`: open a file by path.
    OpenFile,
    /// `O`: load a session; `S`: save the open streams as a session.
    OpenSession,
    SaveSession,
    /// `T`: the next theme.
    CycleTheme,
    /// `,`: the Settings dialog.
    Settings,
    /// `r`: the highlight-rule editor.
    EditRules,
    /// `p`: the filter presets.
    Presets,
    /// `F`: the global filter editor; `f`: the global filter on or off.
    EditGlobal,
    ToggleGlobal,
    /// `!`: the external tools to run on the cursor row or the selection.
    Tools,
    /// `Ctrl+L`: the PIN lock.
    Lock,
    /// `e` / `E`: the next / previous ERROR (or FATAL) line; `w` / `W`: WARN.
    NextError,
    PrevError,
    NextWarn,
    PrevWarn,
    /// The top bar's Play / Pause, as the GUI's toolbar: every stream follows and is
    /// watched, or none is.
    PlayAll,
    PauseAll,
    /// `J`: the JSON tree of the cursor row.
    JsonTree,
    /// The About window (top bar, palette).
    About,
}

/// Maps a key of the log view. Only presses count: the Windows console also reports
/// releases (and repeats as presses), which would otherwise act twice.
pub fn map_key(mut key: KeyEvent) -> Option<Action> {
    if key.kind == KeyEventKind::Release {
        return None;
    }
    // The Windows console may report SHIFT + t as a lowercase `t` with the SHIFT
    // modifier, where other terminals send `T`: both mean the capital.
    if let KeyCode::Char(c) = key.code {
        if key.modifiers.contains(KeyModifiers::SHIFT) && c.is_ascii_lowercase() {
            key.code = KeyCode::Char(c.to_ascii_uppercase());
        }
    }
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    if ctrl {
        // CTRL + SHIFT + digit, as the digit or as its shifted US symbol.
        if key.modifiers.contains(KeyModifiers::SHIFT) {
            if let KeyCode::Char(c) = key.code {
                if let Some(n) = "123456789".find(c).or_else(|| "!@#$%^&*(".find(c)) {
                    return Some(Action::Label(n as u8 + 1));
                }
            }
        }
        if key.modifiers.contains(KeyModifiers::SHIFT)
            && matches!(key.code, KeyCode::Char('f') | KeyCode::Char('F'))
        {
            return Some(Action::FindAll);
        }
        return match key.code {
            KeyCode::Char('c') | KeyCode::Char('C') => Some(Action::CopyOrQuit),
            KeyCode::Char('a') => Some(Action::SelectAll),
            KeyCode::Char('b') => Some(Action::PageUp),
            KeyCode::Char('f') => Some(Action::PageDown),
            KeyCode::F(2) => Some(Action::ToggleBookmark),
            KeyCode::Char('k') => Some(Action::ToggleContext),
            KeyCode::Char('g') => Some(Action::GoTo),
            KeyCode::Char('w') => Some(Action::CloseStream),
            KeyCode::Char('l') => Some(Action::Lock),
            KeyCode::Char('o') => Some(Action::OpenSource),
            KeyCode::Char('n') => Some(Action::SendToScratchpad),
            KeyCode::PageUp => Some(Action::PrevInPane),
            KeyCode::PageDown => Some(Action::NextInPane),
            KeyCode::Left => Some(Action::ScrollLeftFast),
            KeyCode::Right => Some(Action::ScrollRightFast),
            _ => None,
        };
    }
    if alt {
        return match key.code {
            KeyCode::Char(c @ '1'..='9') => Some(Action::GotoTab(c as usize - '1' as usize)),
            KeyCode::Char('x') => Some(Action::ClosePane),
            KeyCode::Char('f') => Some(Action::ToggleFloat),
            KeyCode::Char('/') => Some(Action::FindAll),
            KeyCode::Char('d') | KeyCode::Char('D') => Some(Action::ToggleTimeDelta),
            KeyCode::Char('r') | KeyCode::Char('R') => Some(Action::SearchResults),
            KeyCode::Char('s') | KeyCode::Char('S') => Some(Action::SearchScope),
            KeyCode::Char('e') | KeyCode::Char('E') => Some(Action::Export),
            KeyCode::Char('o') | KeyCode::Char('O') => Some(Action::OpenSourceHere),
            KeyCode::Char('n') | KeyCode::Char('N') => Some(Action::Scratchpad),
            KeyCode::Char('a') | KeyCode::Char('A') => Some(Action::Analysis),
            KeyCode::Char('m') | KeyCode::Char('M') => Some(Action::MergedView),
            KeyCode::Char('t') | KeyCode::Char('T') => Some(Action::FilterTab),
            KeyCode::Left => Some(Action::ResizeLeft),
            KeyCode::Right => Some(Action::ResizeRight),
            KeyCode::Up => Some(Action::ResizeUp),
            KeyCode::Down => Some(Action::ResizeDown),
            _ => None,
        };
    }
    if key.modifiers.contains(KeyModifiers::SHIFT) {
        match key.code {
            KeyCode::Up => return Some(Action::SelectUp),
            KeyCode::Down => return Some(Action::SelectDown),
            KeyCode::F(2) => return Some(Action::PrevBookmark),
            KeyCode::F(3) => return Some(Action::SearchPrev),
            KeyCode::F(4) => return Some(Action::RulePrev),
            _ => {}
        }
    }
    Some(match key.code {
        KeyCode::Char('q') => Action::Quit,
        KeyCode::Tab => Action::NextTab,
        KeyCode::BackTab => Action::PrevTab,
        KeyCode::Char(' ') => Action::ToggleFollow,
        KeyCode::End | KeyCode::Char('G') => Action::Bottom,
        KeyCode::Home | KeyCode::Char('g') => Action::Top,
        KeyCode::Up | KeyCode::Char('k') => Action::LineUp,
        KeyCode::Down | KeyCode::Char('j') => Action::LineDown,
        KeyCode::PageUp => Action::PageUp,
        KeyCode::PageDown => Action::PageDown,
        KeyCode::Left => Action::ScrollLeft,
        KeyCode::Right => Action::ScrollRight,
        KeyCode::Char('0') => Action::ScrollHome,
        KeyCode::Char('/') => Action::StartSearch,
        KeyCode::Char('n') | KeyCode::F(3) => Action::SearchNext,
        KeyCode::Char('N') => Action::SearchPrev,
        KeyCode::Esc => Action::ClearSearch,
        KeyCode::Char('i') => Action::EditInclude,
        KeyCode::Char('I') => Action::ToggleFilterExpr,
        KeyCode::F(4) => Action::RuleNext,
        KeyCode::Char('x') => Action::EditExclude,
        KeyCode::Char('c') => Action::CycleCollapse,
        KeyCode::Char('l') => Action::CycleLevel,
        KeyCode::Char('H') => Action::HideLines,
        KeyCode::Char('R') => Action::IncludeRotations,
        KeyCode::Char('C') => Action::ToggleColumns,
        KeyCode::Char('s') | KeyCode::Char('|') => Action::SplitRight,
        KeyCode::Char('_') => Action::SplitDown,
        KeyCode::Char('<') => Action::MovePrevPane,
        KeyCode::Char('>') => Action::MoveNextPane,
        KeyCode::Char('y') => Action::Copy,
        KeyCode::Char('Y') => Action::CopyAsShown,
        KeyCode::Char('=') => Action::Compare,
        KeyCode::Char('b') => Action::ToggleBookmark,
        KeyCode::Char(']') | KeyCode::F(2) => Action::NextBookmark,
        KeyCode::Char('[') => Action::PrevBookmark,
        KeyCode::Char('m') => Action::EditNote,
        KeyCode::Char('J') => Action::JsonTree,
        KeyCode::Char(':') => Action::Palette,
        KeyCode::Char('@') => Action::TimeAnchor,
        KeyCode::Char('h') => Action::ToggleHex,
        KeyCode::Char('d') => Action::ToggleAsm,
        KeyCode::Char('D') => Action::CycleArch,
        KeyCode::Char('a') => Action::CycleAnsi,
        KeyCode::Char('#') => Action::ToggleLineNumbers,
        KeyCode::Char('L') => Action::LabelPrefix,
        KeyCode::Char('t') => Action::TimeRange,
        KeyCode::Char('o') => Action::OpenFile,
        KeyCode::Char('O') => Action::OpenSession,
        KeyCode::Char('S') => Action::SaveSession,
        KeyCode::Char('T') => Action::CycleTheme,
        KeyCode::Char(',') => Action::Settings,
        KeyCode::Char('r') => Action::EditRules,
        KeyCode::Char('p') => Action::Presets,
        KeyCode::Char('F') => Action::EditGlobal,
        KeyCode::Char('f') => Action::ToggleGlobal,
        KeyCode::Char('!') => Action::Tools,
        KeyCode::Char('e') => Action::NextError,
        KeyCode::Char('E') => Action::PrevError,
        KeyCode::Char('w') => Action::NextWarn,
        KeyCode::Char('W') => Action::PrevWarn,
        KeyCode::Char('?') | KeyCode::F(1) => Action::ToggleHelp,
        _ => return None,
    })
}

/// Prefix of the terminal's keys in `[shortcuts]` (`tui.follow=f`): the window keeps its
/// own keys under the registry's names.
pub const TUI_PREFIX: &str = "tui.";

/// A terminal key with its modifiers, as `[shortcuts]` writes it: `Ctrl+O`, `Alt+/`, `G`,
/// `#`, `Shift+F4`, `Space`. A character is kept as typed, so `G` is `Shift+g`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TuiKey {
    pub ctrl: bool,
    pub alt: bool,
    /// Only for keys that are not characters (`Shift+F4`, `Shift+Up`).
    pub shift: bool,
    pub code: KeyCode,
}

/// The names of the keys that are not characters.
const NAMED: &[(&str, KeyCode)] = &[
    ("Space", KeyCode::Char(' ')),
    ("Enter", KeyCode::Enter),
    ("Tab", KeyCode::Tab),
    ("Esc", KeyCode::Esc),
    ("Backspace", KeyCode::Backspace),
    ("Delete", KeyCode::Delete),
    ("Insert", KeyCode::Insert),
    ("Up", KeyCode::Up),
    ("Down", KeyCode::Down),
    ("Left", KeyCode::Left),
    ("Right", KeyCode::Right),
    ("Home", KeyCode::Home),
    ("End", KeyCode::End),
    ("PgUp", KeyCode::PageUp),
    ("PgDn", KeyCode::PageDown),
];

impl TuiKey {
    /// Parses `Ctrl+O`, `alt+/`, `G`, `Shift+F4`, `Space`, `+` (case-insensitive names,
    /// a character as written).
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        let (mut ctrl, mut alt, mut shift) = (false, false, false);
        // The last `+`-separated part is the key; `Ctrl++` is Ctrl and `+`.
        let (mods, key) = match text.rfind('+') {
            Some(at) if at + 1 == text.len() && at > 0 => (&text[..at - 1], "+"),
            Some(at) if at + 1 < text.len() => (&text[..at], &text[at + 1..]),
            _ => ("", text),
        };
        for m in mods.split('+').map(str::trim).filter(|m| !m.is_empty()) {
            match m.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => ctrl = true,
                "alt" => alt = true,
                "shift" => shift = true,
                _ => return None,
            }
        }
        let code = if let Some((_, code)) = NAMED.iter().find(|(n, _)| n.eq_ignore_ascii_case(key))
        {
            *code
        } else if let Some(n) = key
            .strip_prefix(['F', 'f'])
            .and_then(|n| n.parse::<u8>().ok())
            .filter(|n| (1..=12).contains(n))
        {
            KeyCode::F(n)
        } else {
            let mut chars = key.chars();
            let c = chars.next()?;
            if chars.next().is_some() {
                return None;
            }
            // With Ctrl or Alt a letter is written either case (`Ctrl+O`); Shift is named.
            KeyCode::Char(if ctrl || alt {
                c.to_ascii_lowercase()
            } else {
                c
            })
        };
        // A plain character carries its own case: `Shift` only counts with the others.
        let shift = shift && (ctrl || alt || !matches!(code, KeyCode::Char(_)));
        Some(TuiKey {
            ctrl,
            alt,
            shift,
            code,
        })
    }

    /// The key of a terminal event, as `parse` reads it.
    pub fn from_event(mut key: KeyEvent) -> Self {
        if let KeyCode::Char(c) = key.code {
            if key.modifiers.contains(KeyModifiers::SHIFT) && c.is_ascii_lowercase() {
                key.code = KeyCode::Char(c.to_ascii_uppercase());
            }
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        let mut shift = key.modifiers.contains(KeyModifiers::SHIFT);
        let code = match key.code {
            KeyCode::Char(c) if ctrl || alt => KeyCode::Char(c.to_ascii_lowercase()),
            KeyCode::Char(c) => {
                shift = false;
                KeyCode::Char(c)
            }
            KeyCode::BackTab => {
                shift = true;
                KeyCode::Tab
            }
            code => code,
        };
        TuiKey {
            ctrl,
            alt,
            shift,
            code,
        }
    }

    /// As `[shortcuts]` and the Keys dialog write it.
    pub fn text(self) -> String {
        let mut out = String::new();
        for (on, name) in [
            (self.ctrl, "Ctrl+"),
            (self.alt, "Alt+"),
            (self.shift, "Shift+"),
        ] {
            if on {
                out.push_str(name);
            }
        }
        match self.code {
            KeyCode::F(n) => out.push_str(&format!("F{n}")),
            KeyCode::Char(c) if c != ' ' => out.push(if self.ctrl || self.alt {
                c.to_ascii_uppercase()
            } else {
                c
            }),
            code => {
                let name = NAMED
                    .iter()
                    .find(|(_, k)| *k == code)
                    .map_or("?", |(n, _)| n);
                out.push_str(name);
            }
        }
        out
    }
}

/// The terminal actions a key can be given in `[shortcuts]`: their `tui.` name, the
/// action, and the default keys as `docs/tui.md` lists them.
pub const REMAPPABLE: &[(&str, Action, &str)] = &[
    ("quit", Action::Quit, "q"),
    ("follow", Action::ToggleFollow, "Space"),
    ("top", Action::Top, "g Home"),
    ("bottom", Action::Bottom, "G End"),
    ("search", Action::StartSearch, "/"),
    ("search.next", Action::SearchNext, "n F3"),
    ("search.prev", Action::SearchPrev, "N Shift+F3"),
    ("search.clear", Action::ClearSearch, "Esc"),
    ("search.all", Action::FindAll, "Alt+/"),
    ("search.results", Action::SearchResults, "Alt+R"),
    ("search.scope", Action::SearchScope, "Alt+S"),
    ("goto", Action::GoTo, "Ctrl+G"),
    ("palette", Action::Palette, ":"),
    ("include", Action::EditInclude, "i"),
    ("include.expression", Action::ToggleFilterExpr, "I"),
    ("exclude", Action::EditExclude, "x"),
    ("level", Action::CycleLevel, "l"),
    ("collapse", Action::CycleCollapse, "c"),
    ("ansi", Action::CycleAnsi, "a"),
    ("line_numbers", Action::ToggleLineNumbers, "#"),
    ("time_delta", Action::ToggleTimeDelta, "Alt+D"),
    ("time_range", Action::TimeRange, "t"),
    ("context", Action::ToggleContext, "Ctrl+K"),
    ("hide", Action::HideLines, "H"),
    ("columns", Action::ToggleColumns, "C"),
    ("rotations", Action::IncludeRotations, "R"),
    ("hex", Action::ToggleHex, "h"),
    ("asm", Action::ToggleAsm, "d"),
    ("asm.arch", Action::CycleArch, "D"),
    ("json", Action::JsonTree, "J"),
    ("copy", Action::Copy, "y"),
    ("copy.as_shown", Action::CopyAsShown, "Y"),
    ("export", Action::Export, "Alt+E"),
    ("compare", Action::Compare, "="),
    ("bookmark", Action::ToggleBookmark, "b Ctrl+F2"),
    ("bookmark.next", Action::NextBookmark, "] F2"),
    ("bookmark.prev", Action::PrevBookmark, "[ Shift+F2"),
    ("bookmark.note", Action::EditNote, "m"),
    ("error.next", Action::NextError, "e"),
    ("error.prev", Action::PrevError, "E"),
    ("warn.next", Action::NextWarn, "w"),
    ("warn.prev", Action::PrevWarn, "W"),
    ("rule.next", Action::RuleNext, "F4"),
    ("rule.prev", Action::RulePrev, "Shift+F4"),
    ("source.editor", Action::OpenSource, "Ctrl+O"),
    ("source.here", Action::OpenSourceHere, "Alt+O"),
    ("scratchpad", Action::Scratchpad, "Alt+N"),
    ("scratchpad.send", Action::SendToScratchpad, "Ctrl+N"),
    ("analysis", Action::Analysis, "Alt+A"),
    ("merged_view", Action::MergedView, "Alt+M"),
    ("filter_tab", Action::FilterTab, "Alt+T"),
    ("time_anchor", Action::TimeAnchor, "@"),
    ("split.right", Action::SplitRight, "s |"),
    ("split.down", Action::SplitDown, "_"),
    ("window.close", Action::ClosePane, "Alt+X"),
    ("window.float", Action::ToggleFloat, "Alt+F"),
    ("stream.close", Action::CloseStream, "Ctrl+W"),
    ("stream.move_prev", Action::MovePrevPane, "<"),
    ("stream.move_next", Action::MoveNextPane, ">"),
    ("open", Action::OpenFile, "o"),
    ("session.open", Action::OpenSession, "O"),
    ("session.save", Action::SaveSession, "S"),
    ("theme", Action::CycleTheme, "T"),
    ("settings", Action::Settings, ","),
    ("rules", Action::EditRules, "r"),
    ("presets", Action::Presets, "p"),
    ("global", Action::EditGlobal, "F"),
    ("global.toggle", Action::ToggleGlobal, "f"),
    ("tools", Action::Tools, "!"),
    ("lock", Action::Lock, "Ctrl+L"),
    ("help", Action::ToggleHelp, "? F1"),
    ("plugins", Action::Plugins, ""),
    ("keys", Action::KeysDialog, ""),
];

/// The terminal keys in force: the defaults of `map_key`, with the `tui.` entries of
/// `[shortcuts]` over them. An action given keys there loses its default keys; a key
/// given to an action is taken from the one that had it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeyMap {
    /// The keys set in `[shortcuts]`, and their action.
    pub overrides: Vec<(TuiKey, Action)>,
    /// The actions whose keys `[shortcuts]` sets (an empty value: no key at all).
    pub replaced: Vec<Action>,
    /// Entries that could not be read (an unknown name, a key that does not parse).
    pub notices: Vec<String>,
}

impl KeyMap {
    pub fn build(shortcuts: &std::collections::BTreeMap<String, String>) -> Self {
        let mut map = KeyMap::default();
        for (key, value) in shortcuts {
            let Some(name) = key.strip_prefix(TUI_PREFIX) else {
                continue;
            };
            let Some(&(_, action, _)) = REMAPPABLE.iter().find(|(n, _, _)| *n == name) else {
                map.notices
                    .push(format!("[shortcuts] {key}: unknown action"));
                continue;
            };
            let mut keys = Vec::new();
            for part in value.split(',').map(str::trim).filter(|p| !p.is_empty()) {
                match TuiKey::parse(part) {
                    Some(k) => keys.push(k),
                    None => map
                        .notices
                        .push(format!("[shortcuts] {key}={value}: not a key")),
                }
            }
            map.replaced.push(action);
            for k in keys {
                map.overrides.retain(|(o, _)| *o != k);
                map.overrides.push((k, action));
            }
        }
        map
    }

    /// The action of `key`: a key set in `[shortcuts]`, else the default unless its
    /// action was given other keys or its key was given to another action.
    pub fn map(&self, key: KeyEvent) -> Option<Action> {
        if key.kind == KeyEventKind::Release {
            return None;
        }
        let k = TuiKey::from_event(key);
        if let Some((_, a)) = self.overrides.iter().find(|(o, _)| *o == k) {
            return Some(*a);
        }
        let a = map_key(key)?;
        (!self.replaced.contains(&a)).then_some(a)
    }

    /// The keys of `action` as the Keys dialog shows them: the ones set, or the default.
    pub fn keys_text(&self, name: &str) -> (String, bool) {
        let Some(&(_, action, default)) = REMAPPABLE.iter().find(|(n, _, _)| *n == name) else {
            return (String::new(), false);
        };
        if !self.replaced.contains(&action) {
            return (default.to_string(), false);
        }
        let keys: Vec<String> = self
            .overrides
            .iter()
            .filter(|(_, a)| *a == action)
            .map(|(k, _)| k.text())
            .collect();
        (keys.join(" "), true)
    }

    /// The remappable action that `key` reaches now, by its name.
    pub fn holder(&self, key: TuiKey) -> Option<&'static str> {
        let action = self
            .overrides
            .iter()
            .find(|(o, _)| *o == key)
            .map(|(_, a)| *a)
            .or_else(|| {
                // A default key: the action it maps to, unless that one was remapped.
                let ev = to_event(key)?;
                map_key(ev).filter(|a| !self.replaced.contains(a))
            })?;
        REMAPPABLE
            .iter()
            .find(|(_, a, _)| *a == action)
            .map(|(n, _, _)| *n)
    }
}

/// The terminal event of `key` (for looking a default key up).
fn to_event(key: TuiKey) -> Option<KeyEvent> {
    let mut mods = KeyModifiers::NONE;
    if key.ctrl {
        mods |= KeyModifiers::CONTROL;
    }
    if key.alt {
        mods |= KeyModifiers::ALT;
    }
    if key.shift {
        mods |= KeyModifiers::SHIFT;
    }
    Some(KeyEvent::new(key.code, mods))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn bookmark_keys() {
        let k = |code, m| map_key(press(code, m));
        let none = KeyModifiers::NONE;
        assert_eq!(k(KeyCode::Char('b'), none), Some(Action::ToggleBookmark));
        assert_eq!(
            k(KeyCode::F(2), KeyModifiers::CONTROL),
            Some(Action::ToggleBookmark)
        );
        assert_eq!(k(KeyCode::Char(']'), none), Some(Action::NextBookmark));
        assert_eq!(k(KeyCode::F(2), none), Some(Action::NextBookmark));
        assert_eq!(k(KeyCode::Char('['), none), Some(Action::PrevBookmark));
        assert_eq!(
            k(KeyCode::F(2), KeyModifiers::SHIFT),
            Some(Action::PrevBookmark)
        );
        assert_eq!(k(KeyCode::Char('m'), none), Some(Action::EditNote));
        let ctrl = KeyModifiers::CONTROL;
        assert_eq!(k(KeyCode::Char('k'), ctrl), Some(Action::ToggleContext));
        assert_eq!(k(KeyCode::Char('g'), ctrl), Some(Action::GoTo));
        assert_eq!(k(KeyCode::Char(':'), none), Some(Action::Palette));
        assert_eq!(k(KeyCode::Char('#'), none), Some(Action::ToggleLineNumbers));
        let ctrl_shift = KeyModifiers::CONTROL | KeyModifiers::SHIFT;
        assert_eq!(k(KeyCode::Char('3'), ctrl_shift), Some(Action::Label(3)));
        assert_eq!(k(KeyCode::Char('#'), ctrl_shift), Some(Action::Label(3)));
        assert_eq!(k(KeyCode::Char('L'), none), Some(Action::LabelPrefix));
        assert_eq!(k(KeyCode::Char('h'), none), Some(Action::ToggleHex));
        assert_eq!(k(KeyCode::Char('d'), none), Some(Action::ToggleAsm));
        assert_eq!(k(KeyCode::Char('D'), none), Some(Action::CycleArch));
        assert_eq!(k(KeyCode::Char('a'), none), Some(Action::CycleAnsi));
        assert_eq!(k(KeyCode::Char('t'), none), Some(Action::TimeRange));
        assert_eq!(k(KeyCode::Char('o'), none), Some(Action::OpenFile));
        assert_eq!(k(KeyCode::F(3), none), Some(Action::SearchNext));
        assert_eq!(
            k(KeyCode::F(3), KeyModifiers::SHIFT),
            Some(Action::SearchPrev)
        );
        let shift = KeyModifiers::SHIFT;
        assert_eq!(k(KeyCode::Char('O'), shift), Some(Action::OpenSession));
        assert_eq!(k(KeyCode::Char('S'), shift), Some(Action::SaveSession));
        assert_eq!(k(KeyCode::Char('T'), shift), Some(Action::CycleTheme));
        assert_eq!(k(KeyCode::Char(','), none), Some(Action::Settings));
        assert_eq!(k(KeyCode::Char('r'), none), Some(Action::EditRules));
        assert_eq!(k(KeyCode::Char('p'), none), Some(Action::Presets));
        assert_eq!(k(KeyCode::Char('F'), none), Some(Action::EditGlobal));
        assert_eq!(k(KeyCode::Char('f'), none), Some(Action::ToggleGlobal));
        assert_eq!(k(KeyCode::Char('!'), none), Some(Action::Tools));
        assert_eq!(k(KeyCode::Char('l'), ctrl), Some(Action::Lock));
        assert_eq!(
            k(KeyCode::Char('g'), none),
            Some(Action::Top),
            "plain g is still top"
        );
    }

    #[test]
    fn quit_keys() {
        assert_eq!(
            map_key(press(KeyCode::Char('q'), KeyModifiers::NONE)),
            Some(Action::Quit)
        );
        assert_eq!(
            map_key(press(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(Action::CopyOrQuit)
        );
        assert_eq!(
            map_key(press(KeyCode::Char('y'), KeyModifiers::NONE)),
            Some(Action::Copy)
        );
        // A plain `c` cycles the collapse mode instead.
        assert_eq!(
            map_key(press(KeyCode::Char('c'), KeyModifiers::NONE)),
            Some(Action::CycleCollapse)
        );
    }

    #[test]
    fn tabs_and_alt_digits() {
        assert_eq!(
            map_key(press(KeyCode::Tab, KeyModifiers::NONE)),
            Some(Action::NextTab)
        );
        assert_eq!(
            map_key(press(KeyCode::Char('1'), KeyModifiers::ALT)),
            Some(Action::GotoTab(0))
        );
        assert_eq!(
            map_key(press(KeyCode::Char('9'), KeyModifiers::ALT)),
            Some(Action::GotoTab(8))
        );
        // Without ALT a digit is not a tab switch.
        assert_eq!(map_key(press(KeyCode::Char('3'), KeyModifiers::NONE)), None);
    }

    #[test]
    fn shifted_letters_arrive_as_capitals() {
        // Terminals report SHIFT + g as `G` with the SHIFT modifier set.
        assert_eq!(
            map_key(press(KeyCode::Char('G'), KeyModifiers::SHIFT)),
            Some(Action::Bottom)
        );
        assert_eq!(
            map_key(press(KeyCode::Char('N'), KeyModifiers::SHIFT)),
            Some(Action::SearchPrev)
        );
    }

    #[test]
    fn shift_with_a_lowercase_letter_is_the_capital() {
        let shift = KeyModifiers::SHIFT;
        let k = |c| map_key(press(KeyCode::Char(c), shift));
        assert_eq!(k('t'), Some(Action::CycleTheme), "not the time range");
        assert_eq!(k('g'), Some(Action::Bottom));
        assert_eq!(k('n'), Some(Action::SearchPrev));
        assert_eq!(k('o'), Some(Action::OpenSession));
        assert_eq!(k('s'), Some(Action::SaveSession));
        assert_eq!(
            map_key(press(KeyCode::Char('t'), KeyModifiers::NONE)),
            Some(Action::TimeRange)
        );
    }

    #[test]
    fn releases_are_ignored() {
        let mut key = press(KeyCode::Char('q'), KeyModifiers::NONE);
        key.kind = KeyEventKind::Release;
        assert_eq!(map_key(key), None);
    }

    #[test]
    fn dock_keys() {
        let k = |code, m| map_key(press(code, m));
        let (none, shift) = (KeyModifiers::NONE, KeyModifiers::SHIFT);
        let (ctrl, alt) = (KeyModifiers::CONTROL, KeyModifiers::ALT);
        assert_eq!(k(KeyCode::Char('s'), none), Some(Action::SplitRight));
        assert_eq!(k(KeyCode::Char('|'), shift), Some(Action::SplitRight));
        assert_eq!(k(KeyCode::Char('_'), shift), Some(Action::SplitDown));
        assert_eq!(k(KeyCode::Char('w'), ctrl), Some(Action::CloseStream));
        assert_eq!(k(KeyCode::Char('x'), alt), Some(Action::ClosePane));
        assert_eq!(k(KeyCode::Char('f'), alt), Some(Action::ToggleFloat));
        assert_eq!(k(KeyCode::Char('d'), alt), Some(Action::ToggleTimeDelta));
        assert_eq!(k(KeyCode::Char('r'), alt), Some(Action::SearchResults));
        assert_eq!(k(KeyCode::Char('s'), alt), Some(Action::SearchScope));
        assert_eq!(k(KeyCode::Char('e'), alt), Some(Action::Export));
        assert_eq!(k(KeyCode::Char('o'), alt), Some(Action::OpenSourceHere));
        assert_eq!(k(KeyCode::Char('n'), alt), Some(Action::Scratchpad));
        assert_eq!(k(KeyCode::Char('a'), alt), Some(Action::Analysis));
        assert_eq!(k(KeyCode::Char('n'), ctrl), Some(Action::SendToScratchpad));
        assert_eq!(
            k(KeyCode::Char('='), KeyModifiers::NONE),
            Some(Action::Compare)
        );
        assert_eq!(k(KeyCode::F(4), KeyModifiers::NONE), Some(Action::RuleNext));
        assert_eq!(
            k(KeyCode::F(4), KeyModifiers::SHIFT),
            Some(Action::RulePrev)
        );
        assert_eq!(
            k(KeyCode::Char('I'), KeyModifiers::NONE),
            Some(Action::ToggleFilterExpr)
        );
        assert_eq!(
            k(KeyCode::Char('Y'), KeyModifiers::NONE),
            Some(Action::CopyAsShown)
        );
        assert_eq!(k(KeyCode::Char('e'), none), Some(Action::NextError));
        assert_eq!(k(KeyCode::Char('E'), shift), Some(Action::PrevError));
        assert_eq!(k(KeyCode::Char('w'), none), Some(Action::NextWarn));
        assert_eq!(k(KeyCode::Char('W'), shift), Some(Action::PrevWarn));
        assert_eq!(k(KeyCode::Left, alt), Some(Action::ResizeLeft));
        assert_eq!(k(KeyCode::Left, ctrl), Some(Action::ScrollLeftFast));
        assert_eq!(k(KeyCode::Right, ctrl), Some(Action::ScrollRightFast));
        assert_eq!(k(KeyCode::Right, none), Some(Action::ScrollRight));
        assert_eq!(k(KeyCode::Down, alt), Some(Action::ResizeDown));
        assert_eq!(k(KeyCode::Char('<'), none), Some(Action::MovePrevPane));
        assert_eq!(k(KeyCode::Char('>'), shift), Some(Action::MoveNextPane));
        assert_eq!(k(KeyCode::PageDown, ctrl), Some(Action::NextInPane));
        assert_eq!(k(KeyCode::PageUp, ctrl), Some(Action::PrevInPane));
    }

    #[test]
    fn tui_keys_parse_and_print_back() {
        for text in [
            "Ctrl+O",
            "Alt+/",
            "G",
            "#",
            "Shift+F4",
            "Space",
            "F1",
            "Ctrl+Shift+F",
            "=",
        ] {
            let k = TuiKey::parse(text).unwrap_or_else(|| panic!("{text}"));
            assert_eq!(k.text(), text, "{text}");
        }
        assert_eq!(TuiKey::parse("ctrl+o"), TuiKey::parse("Ctrl+O"));
        assert!(TuiKey::parse("Hyper+X").is_none());
        assert!(TuiKey::parse("ab").is_none());
        let g = press(KeyCode::Char('G'), KeyModifiers::SHIFT);
        assert_eq!(TuiKey::from_event(g), TuiKey::parse("G").unwrap());
        let co = press(KeyCode::Char('o'), KeyModifiers::CONTROL);
        assert_eq!(TuiKey::from_event(co), TuiKey::parse("Ctrl+O").unwrap());
    }

    #[test]
    fn shortcuts_move_keys_between_actions() {
        let mut s = std::collections::BTreeMap::new();
        s.insert("tui.follow".to_string(), "f".to_string());
        s.insert("tui.help".to_string(), String::new());
        s.insert("tui.nonsense".to_string(), "x".to_string());
        s.insert("view.line_numbers.toggle".to_string(), "Ctrl+L".to_string());
        let map = KeyMap::build(&s);
        let k = |c| map.map(press(KeyCode::Char(c), KeyModifiers::NONE));
        assert_eq!(
            k('f'),
            Some(Action::ToggleFollow),
            "the key moved to follow"
        );
        assert_eq!(k(' '), None, "follow lost its default key");
        assert_eq!(k('?'), None, "help has no key");
        assert_eq!(k('q'), Some(Action::Quit), "the others keep theirs");
        assert_eq!(map.notices.len(), 1, "{:?}", map.notices);
        assert_eq!(map.keys_text("follow"), ("f".to_string(), true));
        assert_eq!(map.keys_text("quit"), ("q".to_string(), false));
        assert_eq!(map.holder(TuiKey::parse("f").unwrap()), Some("follow"));
        assert_eq!(map.holder(TuiKey::parse("q").unwrap()), Some("quit"));
    }
}
