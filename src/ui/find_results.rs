//! The Find results dock tab: a query box run over every open stream (see `find_all`) and
//! the matches grouped by stream in one virtualized list.
//!
//! The tab never touches the dock while it is drawn: a committed result is left in
//! `FindAllSession::jump` for the app, which activates the stream's tab afterwards.

use crate::find_all::{FindAllSession, FindState};
use crate::i18n::{t, Language};
use crate::tail_engine::TailEngine;
use crate::theme::CyberTheme;
use crate::ui::dock::{group_thousands, FastTailTab};
use crate::ui::hit_list::{GroupedHitList, HitGroup};
use egui::{RichText, Ui};
use egui_dock::{DockState, NodeIndex, NodePath, SurfaceIndex};

/// Keyboard target of the grouped results list.
pub fn results_list_id() -> egui::Id {
    egui::Id::new("find_results_list")
}

/// The query box of the tab.
pub fn query_input_id() -> egui::Id {
    egui::Id::new("find_results_query")
}

/// `42s`, `3m`, `2h`: how old the snapshot is.
pub fn format_age(secs: u64) -> String {
    match secs {
        0..=59 => format!("{secs}s"),
        60..=3599 => format!("{}m", secs / 60),
        _ => format!("{}h", secs / 3600),
    }
}

/// Header of group `g`: its name and match count, then its state notes.
fn group_header(
    session: &FindAllSession,
    g: usize,
    theme: &CyberTheme,
    lang: Language,
) -> (String, Vec<(String, egui::Color32)>) {
    let group = &session.groups[g];
    let title = format!(
        "{} — {}",
        group.name,
        t(lang, "find_all_group_count").replace("{n}", &group_thousands(group.total))
    );
    let mut notes = Vec::new();
    match group.state {
        FindState::Queued => notes.push((t(lang, "find_all_queued").to_string(), theme.text_dim())),
        FindState::Running => notes.push((
            format!("⏳ {:.0}%", (group.progress * 100.0).clamp(0.0, 100.0)),
            theme.warn_color(),
        )),
        FindState::Stopped => {
            notes.push((t(lang, "find_all_stopped").to_string(), theme.warn_color()))
        }
        FindState::Failed => {
            notes.push((t(lang, "find_all_failed").to_string(), theme.warn_color()))
        }
        FindState::SkippedHex => notes.push((
            t(lang, "find_all_skipped_hex").to_string(),
            theme.text_dim(),
        )),
        FindState::Stale => notes.push((t(lang, "find_all_stale").to_string(), theme.warn_color())),
        FindState::Done => {}
    }
    if group.capped() {
        notes.push((
            format!(
                "({})",
                t(lang, "search_capped").replace("{n}", &group_thousands(session.max_hits()))
            ),
            theme.warn_color(),
        ));
    }
    (title, notes)
}

/// The body of the Find results tab.
pub fn render_find_results(
    ui: &mut Ui,
    session: &mut FindAllSession,
    engines: &[TailEngine],
    theme: &CyberTheme,
    lang: Language,
    font_size: f32,
    level_colors: bool,
) {
    let mut run = false;
    let mut refresh = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new("🔍").monospace());
        let input_id = query_input_id();
        let resp = ui.add(
            egui::TextEdit::singleline(&mut session.input)
                .hint_text(t(lang, "find_all_hint"))
                .desired_width((ui.available_width() - 260.0).clamp(160.0, 420.0))
                .id(input_id),
        );
        if std::mem::take(&mut session.focus_input) {
            // Ctrl+Shift+F: the box takes the keyboard with its text selected, so Enter
            // runs the prefilled query and typing replaces it.
            ui.memory_mut(|m| m.request_focus(input_id));
            let mut state =
                egui::text_edit::TextEditState::load(ui.ctx(), input_id).unwrap_or_default();
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::two(
                    egui::text::CCursor::new(0),
                    egui::text::CCursor::new(session.input.chars().count()),
                )));
            state.store(ui.ctx(), input_id);
        }
        if resp.has_focus()
            && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
        {
            run = true;
        }
        if resp.has_focus()
            && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            ui.memory_mut(|m| m.surrender_focus(input_id));
        }
        if ui
            .button(RichText::new(t(lang, "find_all_run")).monospace())
            .on_hover_text(t(lang, "tip_find_all"))
            .clicked()
        {
            run = true;
        }
        if ui
            .add_enabled(
                session.is_active(),
                egui::Button::new(
                    RichText::new(format!("■ {}", t(lang, "find_all_stop"))).monospace(),
                ),
            )
            .on_hover_text(t(lang, "find_all_stop"))
            .on_disabled_hover_text(t(lang, "find_all_stop"))
            .clicked()
        {
            session.stop();
        }
        if ui
            .add_enabled(
                !session.query.is_empty(),
                egui::Button::new(
                    RichText::new(format!("⟳ {}", t(lang, "find_all_refresh"))).monospace(),
                ),
            )
            .on_hover_text(t(lang, "tip_find_all_refresh"))
            .on_disabled_hover_text(t(lang, "tip_find_all_refresh"))
            .clicked()
        {
            refresh = true;
        }
    });
    if run {
        session.start(engines);
    } else if refresh {
        session.refresh(engines);
    }

    // Summary: matches over streams, what still runs, how old the snapshot is.
    ui.horizontal_wrapped(|ui| {
        if session.query.is_empty() {
            ui.label(
                RichText::new(t(lang, "find_all_empty"))
                    .monospace()
                    .size(11.0)
                    .color(theme.text_dim()),
            );
            return;
        }
        ui.label(
            RichText::new(
                t(lang, "find_all_summary")
                    .replace("{hits}", &group_thousands(session.total_hits()))
                    .replace("{streams}", &session.streams_with_hits().to_string())
                    .replace("{total}", &session.groups.len().to_string()),
            )
            .monospace()
            .size(11.0)
            .color(theme.accent_color()),
        );
        if session.is_active() {
            ui.label(
                RichText::new(format!(
                    "⏳ {}",
                    t(lang, "find_all_progress")
                        .replace("{running}", &session.running_count().to_string())
                        .replace("{queued}", &session.queued_count().to_string())
                ))
                .monospace()
                .size(11.0)
                .color(theme.warn_color()),
            );
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
        if let Some(at) = session.started_at {
            ui.label(
                RichText::new(
                    t(lang, "find_all_snapshot")
                        .replace("{age}", &format_age(at.elapsed().as_secs())),
                )
                .monospace()
                .size(11.0)
                .color(theme.text_dim()),
            )
            .on_hover_text(t(lang, "tip_find_all_refresh"));
            // The age ticks: every second while it counts seconds, then every half minute.
            let tick = if at.elapsed().as_secs() < 60 { 1 } else { 30 };
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs(tick));
        }
    });
    ui.separator();

    let groups: Vec<HitGroup> = (0..session.groups.len())
        .filter_map(|g| {
            let group = &session.groups[g];
            let engine = engines.iter().find(|e| e.path == group.path)?;
            let (title, notes) = group_header(session, g, theme, lang);
            Some(HitGroup {
                engine,
                hits: &group.hits,
                title,
                notes,
                collapsed: group.collapsed,
                dim: group.state == FindState::Stale,
            })
        })
        .collect();
    // A stream closed this frame has no engine left: its group goes on the next poll,
    // until then nothing is listed rather than misattributed rows.
    if groups.len() != session.groups.len() {
        return;
    }
    let output = GroupedHitList::new(results_list_id(), &groups, &session.query)
        .font_size(font_size)
        .level_colors(level_colors)
        .show(ui, theme);
    drop(groups);
    if let Some(g) = output.toggled {
        if let Some(group) = session.groups.get_mut(g) {
            group.collapsed = !group.collapsed;
        }
    }
    if let Some((g, hit)) = output.committed {
        if session.commit(g, hit) {
            // The stream takes the keyboard after the jump: F3 walks its own matches.
            ui.memory_mut(|m| m.surrender_focus(results_list_id()));
        }
    }
}

/// Ctrl+Shift+F. The app consumes it before the dock is drawn: egui matches shortcuts
/// logically (extra Shift ignored), so the stream's Ctrl+F would fire on it as well.
pub fn consume_find_all_shortcut(ctx: &egui::Context) -> bool {
    let shortcut =
        egui::KeyboardShortcut::new(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::F);
    ctx.input_mut(|i| i.consume_shortcut(&shortcut))
}

/// Opens the Find results tab, or focuses it when it is already docked. A new tab is
/// split below the first leaf of the main surface, so the stream above stays in view.
pub fn open_find_results_tab(dock: &mut DockState<FastTailTab>) {
    let tab = FastTailTab::FindResults;
    if let Some(path) = dock.find_tab(&tab) {
        let _ = dock.set_active_tab(path);
        dock.set_focused_node_and_surface(path.node_path());
        return;
    }
    if dock.iter_all_tabs().count() == 0 {
        *dock = DockState::new(vec![tab]);
        return;
    }
    let surface = dock.main_surface_mut();
    match surface.iter().position(|n| n.is_leaf()) {
        Some(leaf) => {
            let [_, new] = surface.split_below(NodeIndex(leaf), 0.62, vec![tab]);
            dock.set_focused_node_and_surface(NodePath::new(SurfaceIndex::main(), new));
        }
        None => surface.push_to_first_leaf(tab),
    }
}

/// Applies the jump a committed result left in the session: the stream's tab becomes
/// active and focused, and the stream centres the line (the next visible one when its
/// filters now hide it) with follow paused. Its own search query is left alone.
/// Returns whether a jump happened.
pub fn apply_find_jump(
    session: &mut FindAllSession,
    engines: &mut [TailEngine],
    dock: &mut DockState<FastTailTab>,
    lang: Language,
) -> bool {
    let Some((path, line)) = session.jump.take() else {
        return false;
    };
    let Some(engine) = engines.iter_mut().find(|e| e.path == path) else {
        return false;
    };
    engine.request_jump(line, lang);
    if let Some(tab) = dock.find_tab(&FastTailTab::LogStream(engine.path.clone())) {
        let _ = dock.set_active_tab(tab);
        dock.set_focused_node_and_surface(tab.node_path());
    }
    true
}

/// The dock as it is saved: without the Find results tab (results are not persisted).
pub fn without_find_results(dock: &DockState<FastTailTab>) -> DockState<FastTailTab> {
    let mut saved = dock.clone();
    saved.retain_tabs(|tab| *tab != FastTailTab::FindResults);
    saved
}
