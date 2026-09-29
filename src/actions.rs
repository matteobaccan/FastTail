//! Action registry: every command FastTail can run by name, as the command palette
//! (`ui::palette`) lists it. It is UI-free: ids, i18n names, categories, scopes, enabled
//! conditions and shortcut labels only; the GUI runs the actions and a future terminal
//! interface can list the same ids with its own keys.
//!
//! Adding an action is four small steps:
//! 1. a variant in `ActionId` and a row in `ACTIONS`: stable string id (never renamed, it
//!    is stored in `palette_recent` and will be the `[shortcuts]` key of
//!    `remappable-shortcuts`), i18n name key, category, scope, shortcut label (`None`
//!    without one) and what it needs (`Need`) to be enabled;
//! 2. the name key in `i18n.rs` in every language (a menu label can be reused);
//! 3. the code that runs it: a window action in `FastTailApp::run_action` (`ui/app.rs`),
//!    a stream action in `render_log_stream` (`ui/dock.rs`), which receives it as
//!    `palette_action` on the stream that was focused when the palette opened;
//! 4. when it has a shortcut, its handler's key in the `shortcut_labels_match_the_handlers`
//!    test of `ui/palette.rs`, and when it is a menu item, its label key in `MENU_KEYS`
//!    below.
//!
//! A boolean setting of the Settings page is one row in `BOOL_SETTINGS` (it becomes a
//! "Toggle …" command), an enumerated one a row in `ENUM_SETTINGS` plus its values in
//! `enum_values` / `set_enum_value` (it becomes a command with a value step).

use crate::config::FastTailConfig;
use crate::i18n::{t, Language};
use crate::renderer::RendererChoice;
use crate::tail_engine::{SizeUnit, TailEngine, ViewMode};
use crate::theme::CyberTheme;

/// Commands remembered by the palette (`palette_recent` in `fasttail.ini`).
pub const MAX_RECENT: usize = 8;

/// Identifier of an action. The string form (`Action::key`) is what gets persisted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActionId {
    // Window
    Help,
    About,
    Settings,
    ColorFilters,
    AlwaysOnTop,
    LockNow,
    GlobalFilterBar,
    FindAll,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    PlayAll,
    PauseAll,
    OpenFile,
    OpenPattern,
    ClearRecentFiles,
    // Session
    SessionSaveAs,
    SessionSave,
    SessionLoad,
    SessionClearRecent,
    SessionSaveDefault,
    // Stream
    Follow,
    Monitor,
    GoToLine,
    SelectAll,
    Copy,
    CopyAsShown,
    ExportVisible,
    ExportMatches,
    PresetSave,
    PresetManage,
    // View
    ViewText,
    ViewHex,
    ViewMarkdown,
    LineNumbers,
    TimeDelta,
    Wrap,
    Collapse,
    Timeline,
    SearchPane,
    TimeAnchorSet,
    TimeAnchorClear,
    TimeDisplayWritten,
    TimeDisplayUtc,
    TimeDisplayLocal,
    // Search
    SearchFocus,
    SearchNext,
    SearchPrev,
    SearchClear,
    ShowContext,
    LeaveContext,
    RuleNext,
    RulePrev,
    // Bookmarks
    BookmarkToggle,
    BookmarkNext,
    BookmarkPrev,
    BookmarkNote,
    BookmarkRemove,
    BookmarkClear,
    BookmarkReport,
    BookmarkReportAll,
    // Settings (generated from `BOOL_SETTINGS` / `ENUM_SETTINGS`)
    Toggle(BoolSetting),
    Choose(EnumSetting),
}

/// Group shown on the right of a palette row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    Window,
    Stream,
    View,
    Search,
    Bookmarks,
    Session,
    Settings,
}

impl Category {
    pub fn name_key(self) -> &'static str {
        match self {
            Category::Window => "palette_cat_window",
            Category::Stream => "palette_cat_stream",
            Category::View => "palette_cat_view",
            Category::Search => "palette_cat_search",
            Category::Bookmarks => "palette_cat_bookmarks",
            Category::Session => "palette_cat_session",
            Category::Settings => "palette_cat_settings",
        }
    }
}

/// Where an action applies: the window, the focused stream, or a result list (the
/// scopes of `remappable-shortcuts`; no list action is registered yet).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scope {
    Window,
    Stream,
    List,
}

/// A condition an action needs to run; the first one not met gives the reason shown
/// on its greyed row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Need {
    /// A focused stream.
    Stream,
    /// Not in the HEX view.
    NotHex,
    /// The line views only (not HEX, not Markdown).
    LineView,
    /// A stream that can follow (not a compressed snapshot).
    Followable,
    /// An active search.
    Search,
    /// Text in the search box (what the stream menu's "Export search matches" and the
    /// clear button look at).
    Query,
    /// A current row (`TailEngine::current_row`): the stream has lines.
    Row,
    /// At least one bookmark.
    Bookmarks,
    /// The current row is bookmarked.
    RowBookmarked,
    /// "Show in context" can toggle: already in context, or a filter and a selection.
    ContextToggle,
    /// The context view is shown.
    InContext,
    /// The time delta column is shown.
    TimeDeltaShown,
    /// A time anchor is set.
    TimeAnchor,
    /// A PIN is set (window lock).
    Pin,
    /// A named session is current.
    Session,
}

/// What the enabled conditions look at: the focused stream, if any, and a few window
/// facts. Built by the palette each frame it is open.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ActionState {
    pub stream: Option<StreamState>,
    pub has_pin: bool,
    pub has_session: bool,
}

/// Snapshot of the focused stream.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StreamState {
    pub hex: bool,
    pub markdown: bool,
    pub compressed: bool,
    pub has_search: bool,
    pub has_query: bool,
    pub has_row: bool,
    pub has_bookmarks: bool,
    pub row_bookmarked: bool,
    pub has_selection: bool,
    pub filtered: bool,
    pub in_context: bool,
    pub time_delta: bool,
    pub has_anchor: bool,
}

impl StreamState {
    pub fn of(engine: &TailEngine) -> Self {
        let row = engine.current_row();
        Self {
            hex: engine.view_mode == ViewMode::Hex,
            markdown: engine.view_mode == ViewMode::Markdown,
            compressed: engine.is_compressed(),
            has_search: !engine.search_query.trim().is_empty()
                || !engine.last_searched_query.is_empty(),
            has_query: !engine.search_query.trim().is_empty(),
            has_row: row.is_some(),
            has_bookmarks: engine.has_bookmarks(),
            row_bookmarked: row.is_some_and(|r| engine.is_bookmarked(r)),
            has_selection: !engine.selection.is_empty() || engine.selection_anchor.is_some(),
            filtered: engine.is_filter_active(),
            in_context: engine.context_line().is_some(),
            time_delta: engine.show_time_delta,
            has_anchor: engine.time_anchor().is_some(),
        }
    }
}

impl Need {
    /// The i18n key of the reason the need is not met, `None` when it is.
    fn unmet(self, state: &ActionState) -> Option<&'static str> {
        let stream = match (self, state.stream) {
            (Need::Pin, _) => return (!state.has_pin).then_some("lock_needs_pin"),
            (Need::Session, _) => return (!state.has_session).then_some("palette_no_session"),
            (_, None) => return Some("palette_no_stream"),
            (_, Some(s)) => s,
        };
        let met = match self {
            Need::Stream | Need::Pin | Need::Session => true,
            Need::NotHex => !stream.hex,
            Need::LineView => !stream.hex && !stream.markdown,
            Need::Followable => !stream.compressed,
            Need::Search => stream.has_search,
            Need::Query => stream.has_query,
            Need::Row => stream.has_row,
            Need::Bookmarks => stream.has_bookmarks,
            Need::RowBookmarked => stream.row_bookmarked,
            Need::ContextToggle => stream.in_context || (stream.filtered && stream.has_selection),
            Need::InContext => stream.in_context,
            Need::TimeDeltaShown => stream.time_delta,
            Need::TimeAnchor => stream.has_anchor,
        };
        if met {
            return None;
        }
        Some(match self {
            Need::NotHex => "palette_not_hex",
            Need::LineView => "palette_text_only",
            Need::Followable => "compressed_follow_tip",
            Need::Search | Need::Query => "palette_no_search",
            Need::Row => "palette_no_row",
            Need::Bookmarks => "palette_no_bookmarks",
            Need::RowBookmarked => "palette_row_not_bookmarked",
            Need::ContextToggle if !stream.filtered => "palette_no_filter",
            Need::ContextToggle => "palette_no_selection",
            Need::InContext => "palette_not_in_context",
            Need::TimeDeltaShown => "palette_no_time_delta",
            _ => "palette_no_anchor",
        })
    }
}

/// One registered action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Action {
    pub id: ActionId,
    /// Stable string id, e.g. `view.wrap.toggle`.
    pub key: &'static str,
    /// i18n key of the name.
    pub name: &'static str,
    pub category: Category,
    pub scope: Scope,
    /// Shortcut label as the help dialog writes it (`ALT + W`); 0.13.0 keeps the key in
    /// the handler, `remappable-shortcuts` replaces this with its key map.
    pub shortcut: Option<&'static str>,
    pub needs: &'static [Need],
}

impl Action {
    /// Why the action cannot run now (an i18n key), `None` when it can.
    pub fn disabled_reason(&self, state: &ActionState) -> Option<&'static str> {
        let base: &[Need] = if self.scope == Scope::Stream {
            &[Need::Stream]
        } else {
            &[]
        };
        base.iter()
            .chain(self.needs)
            .find_map(|need| need.unmet(state))
    }

    pub fn enabled(&self, state: &ActionState) -> bool {
        self.disabled_reason(state).is_none()
    }
}

const fn action(
    id: ActionId,
    key: &'static str,
    name: &'static str,
    category: Category,
    scope: Scope,
    shortcut: Option<&'static str>,
    needs: &'static [Need],
) -> Action {
    Action {
        id,
        key,
        name,
        category,
        scope,
        shortcut,
        needs,
    }
}

use ActionId as A;
use Category as C;
use Need as N;
use Scope as S;

/// The built-in actions, in the order the palette lists them with an empty box.
#[rustfmt::skip]
pub const ACTIONS: &[Action] = &[
    // Stream
    action(A::Follow, "stream.follow.toggle", "act_follow", C::Stream, S::Stream, Some("SPACE"), &[N::Followable]),
    action(A::Monitor, "stream.monitor.toggle", "act_monitor", C::Stream, S::Stream, None, &[]),
    action(A::GoToLine, "stream.goto", "act_goto", C::Stream, S::Stream, Some("CTRL + G"), &[]),
    action(A::SelectAll, "stream.select_all", "act_select_all", C::Stream, S::Stream, Some("CTRL + A"), &[]),
    action(A::Copy, "stream.copy", "copy_rows", C::Stream, S::Stream, Some("CTRL + C"), &[]),
    action(A::CopyAsShown, "stream.copy_as_shown", "copy_as_shown", C::Stream, S::Stream, None, &[N::Row]),
    action(A::ExportVisible, "stream.export.visible", "export_visible", C::Stream, S::Stream, None, &[]),
    action(A::ExportMatches, "stream.export.matches", "export_matches", C::Stream, S::Stream, None, &[N::Query]),
    action(A::PresetSave, "stream.preset.save", "preset_save_current", C::Stream, S::Stream, None, &[]),
    action(A::PresetManage, "stream.preset.manage", "preset_manage", C::Stream, S::Stream, None, &[]),
    // View
    action(A::ViewText, "view.mode.text", "act_view_text", C::View, S::Stream, None, &[]),
    action(A::ViewHex, "view.mode.hex", "act_view_hex", C::View, S::Stream, None, &[]),
    action(A::ViewMarkdown, "view.mode.markdown", "act_view_md", C::View, S::Stream, None, &[]),
    action(A::LineNumbers, "view.line_numbers.toggle", "show_lines", C::View, S::Stream, None, &[N::NotHex]),
    action(A::TimeDelta, "view.time_delta.toggle", "act_time_delta", C::View, S::Stream, None, &[N::NotHex]),
    action(A::Wrap, "view.wrap.toggle", "act_wrap", C::View, S::Stream, Some("ALT + W"), &[N::NotHex]),
    action(A::Collapse, "view.collapse.cycle", "act_collapse", C::View, S::Stream, Some("CTRL + SHIFT + D"), &[N::LineView]),
    action(A::Timeline, "view.timeline.toggle", "act_timeline", C::View, S::Stream, None, &[N::LineView]),
    action(A::SearchPane, "view.search_pane.toggle", "act_search_pane", C::View, S::Window, None, &[]),
    action(A::TimeAnchorSet, "view.time_anchor.set", "time_anchor_set", C::View, S::Stream, None, &[N::TimeDeltaShown, N::Row]),
    action(A::TimeAnchorClear, "view.time_anchor.clear", "time_anchor_clear", C::View, S::Stream, None, &[N::TimeAnchor]),
    action(A::TimeDisplayWritten, "view.time_display.written", "act_time_written", C::View, S::Stream, None, &[N::LineView]),
    action(A::TimeDisplayUtc, "view.time_display.utc", "act_time_utc", C::View, S::Stream, None, &[N::LineView]),
    action(A::TimeDisplayLocal, "view.time_display.local", "act_time_local", C::View, S::Stream, None, &[N::LineView]),
    // Search
    action(A::SearchFocus, "search.focus", "act_search", C::Search, S::Stream, Some("CTRL + F"), &[]),
    action(A::SearchNext, "search.next", "act_search_next", C::Search, S::Stream, Some("F3"), &[N::Search]),
    action(A::SearchPrev, "search.prev", "act_search_prev", C::Search, S::Stream, Some("SHIFT + F3"), &[N::Search]),
    action(A::SearchClear, "search.clear", "clear_search", C::Search, S::Stream, None, &[N::Query]),
    action(A::FindAll, "search.find_all", "act_find_all", C::Search, S::Window, Some("CTRL + SHIFT + F"), &[]),
    action(A::GlobalFilterBar, "search.global_filter.toggle", "act_global_filter", C::Search, S::Window, Some("CTRL + SHIFT + H"), &[]),
    action(A::ShowContext, "search.context.toggle", "context_show", C::Search, S::Stream, Some("CTRL + K"), &[N::ContextToggle]),
    action(A::LeaveContext, "search.context.back", "context_back", C::Search, S::Stream, None, &[N::InContext]),
    action(A::RuleNext, "search.rule.next", "act_rule_next", C::Search, S::Stream, Some("F4"), &[N::LineView, N::Row]),
    action(A::RulePrev, "search.rule.prev", "act_rule_prev", C::Search, S::Stream, Some("SHIFT + F4"), &[N::LineView, N::Row]),
    // Bookmarks
    action(A::BookmarkToggle, "bookmark.toggle", "act_bookmark_toggle", C::Bookmarks, S::Stream, Some("CTRL + F2"), &[]),
    action(A::BookmarkNext, "bookmark.next", "act_bookmark_next", C::Bookmarks, S::Stream, Some("F2"), &[N::Bookmarks]),
    action(A::BookmarkPrev, "bookmark.prev", "act_bookmark_prev", C::Bookmarks, S::Stream, Some("SHIFT + F2"), &[N::Bookmarks]),
    action(A::BookmarkNote, "bookmark.note", "bookmark_note_menu", C::Bookmarks, S::Stream, None, &[N::Row]),
    action(A::BookmarkRemove, "bookmark.remove", "bookmark_remove", C::Bookmarks, S::Stream, None, &[N::RowBookmarked]),
    action(A::BookmarkClear, "bookmark.clear", "clear_bookmarks", C::Bookmarks, S::Stream, None, &[N::Bookmarks]),
    action(A::BookmarkReport, "bookmark.report", "bookmark_report_menu", C::Bookmarks, S::Stream, None, &[N::Bookmarks]),
    action(A::BookmarkReportAll, "bookmark.report.all", "bookmark_report_all", C::Bookmarks, S::Window, None, &[]),
    // Window
    action(A::OpenFile, "window.open_file", "open_file", C::Window, S::Window, None, &[]),
    action(A::OpenPattern, "window.open_pattern", "act_open_pattern", C::Window, S::Window, None, &[]),
    action(A::ClearRecentFiles, "window.recent_files.clear", "clear_recent", C::Window, S::Window, None, &[]),
    action(A::PlayAll, "window.play_all", "act_play_all", C::Window, S::Window, None, &[]),
    action(A::PauseAll, "window.pause_all", "act_pause_all", C::Window, S::Window, None, &[]),
    action(A::Settings, "window.settings", "settings", C::Window, S::Window, None, &[]),
    action(A::ColorFilters, "window.color_filters", "highlight_rules", C::Window, S::Window, None, &[]),
    action(A::Help, "window.help", "toolbar_help", C::Window, S::Window, Some("F1"), &[]),
    action(A::About, "window.about", "toolbar_about", C::Window, S::Window, None, &[]),
    action(A::AlwaysOnTop, "window.always_on_top.toggle", "always_on_top", C::Window, S::Window, Some("CTRL + SHIFT + T"), &[]),
    action(A::LockNow, "window.lock", "lock_now", C::Window, S::Window, Some("CTRL + L"), &[N::Pin]),
    action(A::ZoomIn, "window.zoom.in", "help_zoom_in", C::Window, S::Window, Some("CTRL + +"), &[]),
    action(A::ZoomOut, "window.zoom.out", "help_zoom_out", C::Window, S::Window, Some("CTRL + -"), &[]),
    action(A::ZoomReset, "window.zoom.reset", "help_zoom_reset", C::Window, S::Window, Some("CTRL + 0"), &[]),
    // Session
    action(A::SessionSaveAs, "session.save_as", "session_save_as", C::Session, S::Window, None, &[]),
    action(A::SessionSave, "session.save", "session_save", C::Session, S::Window, None, &[N::Session]),
    action(A::SessionLoad, "session.load", "session_load", C::Session, S::Window, None, &[]),
    action(A::SessionClearRecent, "session.recent.clear", "session_clear_recent", C::Session, S::Window, None, &[]),
    action(A::SessionSaveDefault, "session.save_default", "session_save_default", C::Session, S::Window, None, &[]),
];

/// Boolean settings of the Settings page, listed as "Toggle …" commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BoolSetting {
    Screensaver,
    LockEnabled,
    Telemetry,
    Sound,
    Borderless,
    LineNumbers,
    TimeDelta,
    LevelColors,
    AutoHighlight,
    OverviewStrip,
    TimelineSearchLane,
    FlashOnAlert,
}

pub struct BoolSettingMeta {
    pub setting: BoolSetting,
    pub key: &'static str,
    /// i18n key of the Settings page label.
    pub name: &'static str,
    pub get: fn(&FastTailConfig) -> bool,
    pub set: fn(&mut FastTailConfig, bool),
}

macro_rules! bool_setting {
    ($setting:ident, $key:literal, $name:literal, $field:ident) => {
        BoolSettingMeta {
            setting: BoolSetting::$setting,
            key: $key,
            name: $name,
            get: |c| c.$field,
            set: |c, v| c.$field = v,
        }
    };
}

#[rustfmt::skip]
pub const BOOL_SETTINGS: &[BoolSettingMeta] = &[
    bool_setting!(Screensaver, "settings.screensaver.toggle", "screensaver", screensaver_enabled),
    bool_setting!(LockEnabled, "settings.lock.toggle", "lock_enable", lock_enabled),
    bool_setting!(Telemetry, "settings.telemetry.toggle", "telemetry", telemetry_enabled),
    bool_setting!(Sound, "settings.sound.toggle", "sound_fx", sound_enabled),
    bool_setting!(Borderless, "settings.borderless.toggle", "borderless", borderless),
    bool_setting!(LineNumbers, "settings.line_numbers.toggle", "default_line_numbers", show_line_numbers),
    bool_setting!(TimeDelta, "settings.time_delta.toggle", "default_time_delta", show_time_delta),
    bool_setting!(LevelColors, "settings.level_colors.toggle", "level_colors", level_colors),
    bool_setting!(AutoHighlight, "settings.auto_highlight.toggle", "auto_highlight", auto_highlight),
    bool_setting!(OverviewStrip, "settings.overview_strip.toggle", "overview_strip", overview_strip),
    bool_setting!(TimelineSearchLane, "settings.timeline_search_lane.toggle", "timeline_search_lane_tip", timeline_search_lane),
    bool_setting!(FlashOnAlert, "settings.flash_on_alert.toggle", "flash_on_alert", flash_on_alert),
];

pub fn bool_setting(setting: BoolSetting) -> &'static BoolSettingMeta {
    BOOL_SETTINGS
        .iter()
        .find(|m| m.setting == setting)
        .expect("every BoolSetting has a row in BOOL_SETTINGS")
}

/// Enumerated settings: a command that opens a value step in the palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EnumSetting {
    Theme,
    Language,
    Renderer,
    SizeUnit,
}

pub struct EnumSettingMeta {
    pub setting: EnumSetting,
    pub key: &'static str,
    pub name: &'static str,
}

pub const ENUM_SETTINGS: &[EnumSettingMeta] = &[
    EnumSettingMeta {
        setting: EnumSetting::Theme,
        key: "settings.theme",
        name: "theme",
    },
    EnumSettingMeta {
        setting: EnumSetting::Language,
        key: "settings.language",
        name: "language",
    },
    EnumSettingMeta {
        setting: EnumSetting::Renderer,
        key: "settings.renderer",
        name: "renderer",
    },
    EnumSettingMeta {
        setting: EnumSetting::SizeUnit,
        key: "settings.size_unit",
        name: "act_size_unit",
    },
];

const THEMES: [CyberTheme; 4] = [
    CyberTheme::Tron,
    CyberTheme::Matrix,
    CyberTheme::Blade,
    CyberTheme::Light,
];
const SIZE_UNITS: [SizeUnit; 4] = [SizeUnit::Bytes, SizeUnit::MB, SizeUnit::GB, SizeUnit::Hex];

/// One value of an enumerated setting in the palette's value step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumValue {
    pub label: String,
    /// The label in English, matched as well as the localized one.
    pub english: String,
    pub current: bool,
}

fn renderer_label(lang: Language, choice: RendererChoice) -> String {
    match choice {
        RendererChoice::Auto => t(lang, "renderer_auto").to_string(),
        RendererChoice::Glow => t(lang, "renderer_glow").to_string(),
        RendererChoice::Wgpu => t(lang, "renderer_wgpu").to_string(),
        RendererChoice::Software => format!(
            "{} \u{2014} {}",
            t(lang, "renderer_software"),
            t(lang, "renderer_not_recommended")
        ),
    }
}

/// The values of `setting`, labelled in `cfg.language`, with the current one marked.
/// The language list starts with "System language", as in the Settings page.
pub fn enum_values(setting: EnumSetting, cfg: &FastTailConfig) -> Vec<EnumValue> {
    let lang = cfg.language;
    let value = |label: String, english: String, current: bool| EnumValue {
        label,
        english,
        current,
    };
    match setting {
        EnumSetting::Theme => THEMES
            .iter()
            .map(|th| {
                let name = |l| match th {
                    CyberTheme::Light => t(l, "theme_light").to_string(),
                    _ => th.name().to_string(),
                };
                value(name(lang), name(Language::En), *th == cfg.theme)
            })
            .collect(),
        EnumSetting::Language => {
            let detected = Language::detect();
            let system = |l| format!("{} ({})", t(l, "language_system"), detected.name());
            let mut values = vec![value(system(lang), system(Language::En), cfg.language_auto)];
            values.extend(Language::ALL.iter().map(|l| {
                value(
                    l.name().to_string(),
                    l.code().to_string(),
                    !cfg.language_auto && *l == cfg.language,
                )
            }));
            values
        }
        EnumSetting::Renderer => RendererChoice::ALL
            .iter()
            .map(|r| {
                value(
                    renderer_label(lang, *r),
                    renderer_label(Language::En, *r),
                    *r == cfg.renderer,
                )
            })
            .collect(),
        EnumSetting::SizeUnit => SIZE_UNITS
            .iter()
            .map(|u| {
                let name = format!("{u:?}");
                value(name.clone(), name, *u == cfg.size_unit)
            })
            .collect(),
    }
}

/// Applies value `index` of `enum_values(setting, ..)` to the configuration. The caller
/// saves it and applies what lives outside the configuration (the streams' size unit).
pub fn set_enum_value(setting: EnumSetting, cfg: &mut FastTailConfig, index: usize) {
    match setting {
        EnumSetting::Theme => {
            if let Some(theme) = THEMES.get(index) {
                cfg.theme = *theme;
            }
        }
        EnumSetting::Language => {
            if index == 0 {
                cfg.language_auto = true;
                cfg.language = Language::detect();
            } else if let Some(lang) = Language::ALL.get(index - 1) {
                cfg.language_auto = false;
                cfg.language = *lang;
            }
        }
        EnumSetting::Renderer => {
            if let Some(choice) = RendererChoice::ALL.get(index) {
                cfg.renderer = *choice;
            }
        }
        EnumSetting::SizeUnit => {
            if let Some(unit) = SIZE_UNITS.get(index) {
                cfg.size_unit = *unit;
            }
        }
    }
}

/// Every action the palette lists: the built-in ones, then one per boolean and per
/// enumerated setting.
pub fn all() -> Vec<Action> {
    let mut list = ACTIONS.to_vec();
    list.extend(BOOL_SETTINGS.iter().map(|m| Action {
        id: ActionId::Toggle(m.setting),
        key: m.key,
        name: m.name,
        category: Category::Settings,
        scope: Scope::Window,
        shortcut: None,
        needs: &[],
    }));
    list.extend(ENUM_SETTINGS.iter().map(|m| Action {
        id: ActionId::Choose(m.setting),
        key: m.key,
        name: m.name,
        category: Category::Settings,
        scope: Scope::Window,
        shortcut: None,
        needs: &[],
    }));
    list
}

/// The action with the stable id `key`.
pub fn find(key: &str) -> Option<Action> {
    all().into_iter().find(|a| a.key == key)
}

/// The action registered as `id`.
pub fn get(id: ActionId) -> Action {
    all()
        .into_iter()
        .find(|a| a.id == id)
        .expect("every ActionId is registered")
}

/// Frames a stream action picked in the palette waits for its stream to be drawn.
pub const PENDING_FRAMES: u8 = 3;

/// A stream action picked in the palette, waiting for its stream to be drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingAction {
    pub path: std::path::PathBuf,
    pub id: ActionId,
    /// Frames left before it is dropped.
    pub frames_left: u8,
}

impl PendingAction {
    pub fn new(path: std::path::PathBuf, id: ActionId) -> Self {
        Self {
            path,
            id,
            frames_left: PENDING_FRAMES,
        }
    }

    /// After a frame: gone once its stream took it; otherwise (the tab was not drawn,
    /// e.g. a file dropped while the palette was open made another tab active) it waits
    /// for the next frames, unless its stream is closed or its frames are spent.
    pub fn after_frame(self, taken: bool, stream_open: bool) -> Option<Self> {
        if taken || !stream_open || self.frames_left == 0 {
            return None;
        }
        Some(Self {
            frames_left: self.frames_left - 1,
            ..self
        })
    }
}

/// Records `key` as the most recent palette command (at most `MAX_RECENT` kept).
pub fn push_recent(recent: &mut Vec<String>, key: &str) {
    recent.retain(|k| k != key);
    recent.insert(0, key.to_string());
    recent.truncate(MAX_RECENT);
}

/// Parses `palette_recent` (comma-separated ids): unknown and repeated ids are dropped.
pub fn parse_recent(text: &str) -> Vec<String> {
    let mut recent: Vec<String> = Vec::new();
    for key in text.split(',').map(str::trim) {
        if find(key).is_some() && !recent.iter().any(|k| k == key) {
            recent.push(key.to_string());
        }
    }
    recent.truncate(MAX_RECENT);
    recent
}

/// i18n keys of the menu items (title bar, toolbar, stream menu, row context menu,
/// presets and session menus). Each needs a registry entry; the ones listed in
/// `MENU_KEYS_WITHOUT_ACTION` are files, sessions, presets or tools picked by name, or
/// placeholders.
pub const MENU_KEYS: &[&str] = &[
    "open_file",
    "clear_recent",
    "session_save_as",
    "session_save",
    "session_load",
    "session_recent",
    "session_clear_recent",
    "session_save_default",
    "session_no_recent",
    "no_recent_files",
    "toolbar_play",
    "toolbar_pause",
    "settings",
    "highlight_rules",
    "toolbar_help",
    "toolbar_about",
    "export_visible",
    "export_matches",
    "clear_bookmarks",
    "bookmark_report_menu",
    "bookmark_report_all",
    "ext_tools_menu",
    "copy_rows",
    "copy_as_shown",
    "bookmark_note_menu",
    "bookmark_remove",
    "context_show",
    "context_back",
    "time_anchor_set",
    "time_anchor_clear",
    "presets_none",
    "preset_apply_all",
    "preset_save_current",
    "preset_update",
    "preset_manage",
    "selection_hl_menu",
    "selection_hl_clear",
    "rule_next_menu",
];

pub const MENU_KEYS_WITHOUT_ACTION: &[&str] = &[
    // Paths picked by name.
    "session_recent",
    "session_no_recent",
    "no_recent_files",
    // Presets and tools picked by name.
    "presets_none",
    "preset_apply_all",
    "preset_update",
    "ext_tools_menu",
    // Toolbar buttons with a palette name of their own.
    "toolbar_play",
    "toolbar_pause",
    // The token under the pointer, and the rules matching the row (the palette has
    // "Next / previous line of rule" on F4 / SHIFT + F4).
    "selection_hl_menu",
    "selection_hl_clear",
    "rule_next_menu",
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn with_stream(stream: StreamState) -> ActionState {
        ActionState {
            stream: Some(stream),
            ..Default::default()
        }
    }

    #[test]
    fn ids_are_unique_and_found_back() {
        let list = all();
        let keys: HashSet<_> = list.iter().map(|a| a.key).collect();
        assert_eq!(keys.len(), list.len(), "duplicate action key");
        let ids: HashSet<_> = list.iter().map(|a| a.id).collect();
        assert_eq!(ids.len(), list.len(), "duplicate ActionId");
        for a in &list {
            assert_eq!(find(a.key), Some(*a));
            assert_eq!(get(a.id), *a);
            assert!(
                a.key
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '.' || c == '_'),
                "{}",
                a.key
            );
        }
    }

    #[test]
    fn every_name_and_reason_has_a_text() {
        let reasons = [
            "palette_no_stream",
            "palette_not_hex",
            "palette_text_only",
            "palette_no_search",
            "palette_no_row",
            "palette_no_bookmarks",
            "palette_row_not_bookmarked",
            "palette_no_selection",
            "palette_no_filter",
            "palette_not_in_context",
            "palette_no_time_delta",
            "palette_no_anchor",
            "palette_no_session",
            "compressed_follow_tip",
            "lock_needs_pin",
        ];
        let categories = [
            Category::Window,
            Category::Stream,
            Category::View,
            Category::Search,
            Category::Bookmarks,
            Category::Session,
            Category::Settings,
        ];
        for lang in Language::ALL {
            for a in all() {
                assert_ne!(t(*lang, a.name), "Unknown", "{lang:?} {}", a.name);
            }
            for key in reasons {
                assert_ne!(t(*lang, key), "Unknown", "{lang:?} {key}");
            }
            for c in categories {
                assert_ne!(t(*lang, c.name_key()), "Unknown", "{lang:?} {c:?}");
            }
        }
    }

    #[test]
    fn every_menu_item_has_an_action() {
        let sources = [include_str!("ui/app.rs"), include_str!("ui/dock.rs")];
        let names: HashSet<_> = all().iter().map(|a| a.name).collect();
        for key in MENU_KEYS {
            let quoted = format!("\"{key}\"");
            assert!(
                sources.iter().any(|s| s.contains(&quoted)),
                "{key} is no longer a menu label: drop it from MENU_KEYS"
            );
            assert!(
                names.contains(key) || MENU_KEYS_WITHOUT_ACTION.contains(key),
                "menu item {key} has no action in the registry"
            );
        }
        // Play and Pause are registered under names that say "all streams".
        assert!(names.contains("act_play_all") && names.contains("act_pause_all"));
    }

    #[test]
    fn stream_actions_need_a_stream() {
        let none = ActionState::default();
        for a in all() {
            let reason = a.disabled_reason(&none);
            if a.scope == Scope::Stream {
                assert_eq!(reason, Some("palette_no_stream"), "{}", a.key);
            }
        }
        assert_eq!(
            get(ActionId::Follow).disabled_reason(&none),
            Some("palette_no_stream")
        );
        assert!(get(ActionId::Help).enabled(&none));
        assert!(get(ActionId::FindAll).enabled(&none));
        assert!(get(ActionId::Toggle(BoolSetting::LevelColors)).enabled(&none));
    }

    #[test]
    fn search_actions_need_a_search() {
        let state = with_stream(StreamState::default());
        for id in [
            ActionId::SearchNext,
            ActionId::SearchPrev,
            ActionId::ExportMatches,
        ] {
            assert_eq!(get(id).disabled_reason(&state), Some("palette_no_search"));
        }
        let searching = with_stream(StreamState {
            has_search: true,
            ..Default::default()
        });
        assert!(get(ActionId::SearchNext).enabled(&searching));
        // A search already run but its box emptied: nothing to export or clear.
        for id in [ActionId::ExportMatches, ActionId::SearchClear] {
            assert_eq!(
                get(id).disabled_reason(&searching),
                Some("palette_no_search")
            );
        }
        let typed = with_stream(StreamState {
            has_search: true,
            has_query: true,
            ..Default::default()
        });
        assert!(get(ActionId::ExportMatches).enabled(&typed));
        assert!(get(ActionId::SearchClear).enabled(&typed));
    }

    #[test]
    fn pending_action_waits_for_its_stream() {
        let path = std::path::PathBuf::from("a.log");
        let fresh = PendingAction::new(path.clone(), ActionId::BookmarkToggle);
        // Taken by its stream: gone.
        assert_eq!(fresh.clone().after_frame(true, true), None);
        // Stream closed meanwhile: dropped.
        assert_eq!(fresh.clone().after_frame(false, false), None);
        // Tab not drawn this frame: kept, for PENDING_FRAMES frames at most.
        let mut pending = Some(fresh);
        for _ in 0..PENDING_FRAMES {
            pending = pending.and_then(|p| p.after_frame(false, true));
            assert!(pending.is_some());
        }
        assert_eq!(pending.and_then(|p| p.after_frame(false, true)), None);
    }

    #[test]
    fn row_actions_need_a_row() {
        let rows = [
            ActionId::BookmarkNote,
            ActionId::CopyAsShown,
            ActionId::TimeAnchorSet,
        ];
        let empty = with_stream(StreamState {
            time_delta: true,
            ..Default::default()
        });
        for id in rows {
            assert_eq!(
                get(id).disabled_reason(&empty),
                Some("palette_no_row"),
                "{id:?}"
            );
        }
        let lines = with_stream(StreamState {
            time_delta: true,
            has_row: true,
            ..Default::default()
        });
        for id in rows {
            assert!(get(id).enabled(&lines), "{id:?}");
        }
    }

    #[test]
    fn text_actions_are_off_in_the_hex_view() {
        let hex = with_stream(StreamState {
            hex: true,
            ..Default::default()
        });
        assert_eq!(
            get(ActionId::Wrap).disabled_reason(&hex),
            Some("palette_not_hex")
        );
        assert_eq!(
            get(ActionId::Collapse).disabled_reason(&hex),
            Some("palette_text_only")
        );
        let md = with_stream(StreamState {
            markdown: true,
            ..Default::default()
        });
        assert!(get(ActionId::Wrap).enabled(&md));
        assert_eq!(
            get(ActionId::Collapse).disabled_reason(&md),
            Some("palette_text_only")
        );
        assert!(get(ActionId::ViewText).enabled(&hex));
    }

    #[test]
    fn other_conditions() {
        let plain = with_stream(StreamState::default());
        assert_eq!(
            get(ActionId::ShowContext).disabled_reason(&plain),
            Some("palette_no_filter")
        );
        let filtered = with_stream(StreamState {
            filtered: true,
            ..Default::default()
        });
        assert_eq!(
            get(ActionId::ShowContext).disabled_reason(&filtered),
            Some("palette_no_selection")
        );
        let compressed = with_stream(StreamState {
            compressed: true,
            ..Default::default()
        });
        assert_eq!(
            get(ActionId::Follow).disabled_reason(&compressed),
            Some("compressed_follow_tip")
        );
        let none = ActionState::default();
        assert_eq!(
            get(ActionId::LockNow).disabled_reason(&none),
            Some("lock_needs_pin")
        );
        assert_eq!(
            get(ActionId::SessionSave).disabled_reason(&none),
            Some("palette_no_session")
        );
        assert_eq!(
            get(ActionId::BookmarkNext).disabled_reason(&plain),
            Some("palette_no_bookmarks")
        );
    }

    #[test]
    fn bool_settings_read_and_write_their_field() {
        let mut cfg = FastTailConfig::default();
        for m in BOOL_SETTINGS {
            let before = (m.get)(&cfg);
            (m.set)(&mut cfg, !before);
            assert_eq!((m.get)(&cfg), !before, "{}", m.key);
            assert_eq!(bool_setting(m.setting).key, m.key);
        }
    }

    #[test]
    fn enum_values_mark_the_current_one_and_apply() {
        let mut cfg = FastTailConfig {
            language_auto: false,
            ..Default::default()
        };
        for m in ENUM_SETTINGS {
            let values = enum_values(m.setting, &cfg);
            assert!(values.len() >= 4, "{}", m.key);
            assert_eq!(values.iter().filter(|v| v.current).count(), 1, "{}", m.key);
        }
        set_enum_value(EnumSetting::Theme, &mut cfg, 3);
        assert_eq!(cfg.theme, CyberTheme::Light);
        assert!(enum_values(EnumSetting::Theme, &cfg)[3].current);
        set_enum_value(EnumSetting::SizeUnit, &mut cfg, 2);
        assert_eq!(cfg.size_unit, SizeUnit::GB);
        let german = 1 + Language::ALL
            .iter()
            .position(|l| *l == Language::De)
            .unwrap();
        set_enum_value(EnumSetting::Language, &mut cfg, german);
        assert_eq!(cfg.language, Language::De);
        assert!(!cfg.language_auto);
        set_enum_value(EnumSetting::Language, &mut cfg, 0);
        assert!(cfg.language_auto);
        set_enum_value(EnumSetting::Renderer, &mut cfg, 2);
        assert_eq!(cfg.renderer, RendererChoice::Wgpu);
        // Out of range: nothing changes.
        set_enum_value(EnumSetting::Renderer, &mut cfg, 99);
        assert_eq!(cfg.renderer, RendererChoice::Wgpu);
    }

    #[test]
    fn recent_keeps_eight_distinct_known_ids() {
        let mut recent = Vec::new();
        for a in all().iter().take(10) {
            push_recent(&mut recent, a.key);
        }
        assert_eq!(recent.len(), MAX_RECENT);
        assert_eq!(recent[0], all()[9].key);
        push_recent(&mut recent, all()[5].key);
        assert_eq!(recent[0], all()[5].key);
        assert_eq!(recent.len(), MAX_RECENT);
        let parsed = parse_recent(" view.wrap.toggle , nope,view.wrap.toggle,search.next");
        assert_eq!(parsed, vec!["view.wrap.toggle", "search.next"]);
        assert!(parse_recent("").is_empty());
    }
}
