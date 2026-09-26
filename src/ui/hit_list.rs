//! Virtualized list of search hits: the rows of a stream's search results pane.
//!
//! The list is drawn over a slice of line indices (the stream's `search_matches`) and
//! reads only the lines on screen through the engine's block cache, so it costs the same
//! with ten hits or a million. Each row shows the 1-based line number, the line text cut
//! to the pane width with the query tinted, and `▶` on the current match. A click, or
//! `Enter` on the keyboard selection, commits a hit; the caller makes it the current match
//! and moves the main view there.
//!
//! The keyboard selection is separate from the current match so that walking the list
//! with the arrows does not move the main view on every press. It snaps back to the
//! current match whenever that changes (`F3`, a click, a refresh).
//!
//! `GroupedHitList` draws the hits of several streams in one list (the Find results tab):
//! the groups are flattened into rows, a header per stream and then its hits while the
//! group is expanded, so one `show_rows` virtualizes across every group.

use crate::i18n::{t, Language};
use crate::tail_engine::{find_case_insensitive, TailEngine};
use crate::theme::CyberTheme;
use egui::{Color32, ScrollArea, Stroke, Ui};

/// Background of the query inside a listed line (same yellow as the main view's hits).
const QUERY_BG: Color32 = Color32::from_rgb(255, 230, 0);
/// Characters of a line laid out at most, whatever the pane width.
const MAX_ROW_CHARS: usize = 2048;

/// Keys the list reacts to while it holds keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavKey {
    Up,
    Down,
    PageUp,
    PageDown,
    First,
    Last,
}

/// New selection after `key`, over `len` hits with `page` rows on screen.
pub fn move_selection(selected: usize, len: usize, key: NavKey, page: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let page = page.max(1);
    let last = len - 1;
    let selected = selected.min(last);
    match key {
        NavKey::Up => selected.saturating_sub(1),
        NavKey::Down => (selected + 1).min(last),
        NavKey::PageUp => selected.saturating_sub(page),
        NavKey::PageDown => (selected + page).min(last),
        NavKey::First => 0,
        NavKey::Last => last,
    }
}

/// Per-list state kept in egui memory between frames.
#[derive(Debug, Clone, Default)]
pub struct HitListState {
    /// Keyboard selection (an index into the hits).
    pub selected: usize,
    /// Current match seen last frame: when it changes the selection snaps to it.
    pub synced_current: Option<usize>,
    /// Hit to bring into view on the next frame.
    pub reveal: Option<usize>,
    /// Scroll offset and viewport height of the last frame, to tell whether a hit is
    /// already on screen and how many rows a page holds.
    pub offset: f32,
    pub view_height: f32,
}

impl HitListState {
    /// Follows the current match: when it changed since the last frame, the selection
    /// moves to it and the list scrolls to show it. Returns whether it changed.
    pub fn sync_to_current(&mut self, current: Option<usize>) -> bool {
        if current == self.synced_current {
            return false;
        }
        self.synced_current = current;
        if let Some(c) = current {
            self.selected = c;
            self.reveal = Some(c);
        }
        true
    }

    /// Scroll offset that brings `row` into view, or `None` when it already is. Before
    /// the list has been drawn once nothing is on screen yet.
    pub fn reveal_offset(&self, row: usize, row_height: f32) -> Option<f32> {
        let top = row as f32 * row_height;
        let bottom = top + row_height;
        let view = self.view_height.max(row_height);
        if self.view_height > 0.0 && top >= self.offset && bottom <= self.offset + view {
            None
        } else {
            Some((top - (view - row_height) / 2.0).max(0.0))
        }
    }
}

/// Byte ranges of the case-insensitive occurrences of `query_lower` in `text`.
pub fn query_ranges(text: &str, query_lower: &str) -> Vec<(usize, usize)> {
    find_case_insensitive(text, query_lower)
}

/// `text` cut to at most `max_chars` characters, on a character boundary.
pub fn clip_chars(text: &str, max_chars: usize) -> &str {
    match text.char_indices().nth(max_chars) {
        Some((at, _)) => &text[..at],
        None => text,
    }
}

/// What happened in the list this frame.
#[derive(Debug, Default, Clone, Copy)]
pub struct HitListOutput {
    /// Hit committed by a click or `Enter` (an index into the hits).
    pub committed: Option<usize>,
    /// The list holds keyboard focus after this frame.
    pub has_focus: bool,
}

/// A virtualized list of hits of one stream.
pub struct HitList<'a> {
    id: egui::Id,
    hits: &'a [usize],
    current: Option<usize>,
    query_lower: String,
    font_size: f32,
    level_colors: bool,
}

impl<'a> HitList<'a> {
    /// `hits` are line indices in file order, `current` the index of the current hit.
    pub fn new(id: egui::Id, hits: &'a [usize], current: Option<usize>, query: &str) -> Self {
        Self {
            id,
            hits,
            current,
            query_lower: query.trim().to_lowercase(),
            font_size: 13.0,
            level_colors: true,
        }
    }

    pub fn font_size(mut self, font_size: f32) -> Self {
        self.font_size = font_size;
        self
    }

    /// Colour the line text by its detected level (the Settings option).
    pub fn level_colors(mut self, level_colors: bool) -> Self {
        self.level_colors = level_colors;
        self
    }

    /// Whether the list with this id holds keyboard focus.
    pub fn has_focus(ctx: &egui::Context, id: egui::Id) -> bool {
        ctx.memory(|m| m.has_focus(id))
    }

    pub fn show(
        self,
        ui: &mut Ui,
        engine: &TailEngine,
        theme: &CyberTheme,
        lang: Language,
    ) -> HitListOutput {
        let id = self.id;
        let len = self.hits.len();
        let font_id = egui::FontId::monospace(self.font_size);
        let font_row_h = ui.ctx().fonts_mut(|f| f.row_height(&font_id));
        let char_w = ui.ctx().fonts_mut(|f| f.glyph_width(&font_id, '0'));
        let row_height = (font_row_h * 1.25).max(16.0).ceil();

        let mut state: HitListState = ui.data(|d| d.get_temp(id)).unwrap_or_default();
        state.sync_to_current(self.current);
        if len > 0 {
            state.selected = state.selected.min(len - 1);
        }

        // The list itself is the focusable widget: rows take the clicks, the list takes
        // the keys. Registered first so the rows drawn over it win the pointer.
        let outer = ui.available_rect_before_wrap();
        let list_resp = ui.interact(outer, id, egui::Sense::focusable_noninteractive());
        let mut committed = None;
        let focused = list_resp.has_focus();
        if focused {
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    id,
                    egui::EventFilter {
                        tab: false,
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        escape: true,
                    },
                )
            });
            let page = ((state.view_height / row_height).floor() as usize).max(1);
            for key in take_nav_keys(ui) {
                state.selected = move_selection(state.selected, len, key, page);
                state.reveal = Some(state.selected);
            }
            if len > 0 && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
                committed = Some(state.selected);
            }
            if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                // Back to the main view: its navigation keys work again.
                ui.memory_mut(|m| m.surrender_focus(id));
            }
        }

        if len == 0 {
            ui.centered_and_justified(|ui| {
                ui.label(
                    egui::RichText::new(t(lang, "search_pane_no_matches"))
                        .monospace()
                        .color(theme.text_dim()),
                );
            });
            ui.data_mut(|d| d.insert_temp(id, state));
            return HitListOutput {
                committed: None,
                has_focus: focused,
            };
        }

        // A list scrolled to its last row sticks there as hits arrive, unless a hit has to
        // be brought into view this frame.
        let reveal = state
            .reveal
            .take()
            .and_then(|row| state.reveal_offset(row, row_height));
        let mut scroll = ScrollArea::vertical()
            .id_salt(id.with("scroll"))
            .auto_shrink([false, false])
            .stick_to_bottom(reveal.is_none());
        if let Some(offset) = reveal {
            scroll = scroll.vertical_scroll_offset(offset);
        }

        let num_w = char_w * 9.0;
        let marker_w = char_w * 2.0;
        let mut clicked = None;
        let selected = state.selected;
        let current = self.current;
        let query_lower = &self.query_lower;
        let level_colors = self.level_colors;
        let hits = self.hits;
        let output = scroll.show_rows(ui, row_height, len, |ui, rows| {
            let painter = ui.painter().clone();
            for hit in rows {
                let line = hits[hit];
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), row_height),
                    egui::Sense::hover(),
                );
                let is_current = current == Some(hit);
                let is_selected = focused && selected == hit;
                if is_current {
                    let c = theme.accent_color();
                    painter.rect_filled(
                        rect,
                        0.0,
                        Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 60),
                    );
                }
                if is_selected {
                    painter.rect_stroke(
                        rect.shrink(0.5),
                        0.0,
                        Stroke::new(1.0, theme.secondary_accent()),
                        egui::StrokeKind::Inside,
                    );
                }
                let text_top = rect.top() + (row_height - font_row_h) / 2.0;
                if is_current {
                    painter.text(
                        egui::pos2(rect.left() + 2.0, text_top),
                        egui::Align2::LEFT_TOP,
                        "▶",
                        font_id.clone(),
                        theme.accent_color(),
                    );
                }
                painter.text(
                    egui::pos2(rect.left() + 2.0 + marker_w, text_top),
                    egui::Align2::LEFT_TOP,
                    format!("{:>7}│", line + 1),
                    font_id.clone(),
                    if is_current {
                        theme.accent_color()
                    } else {
                        theme.text_dim().gamma_multiply(0.7)
                    },
                );
                let text_x = rect.left() + 2.0 + marker_w + num_w;
                paint_line_text(
                    ui,
                    &painter,
                    engine,
                    line,
                    egui::pos2(text_x, text_top),
                    rect.right(),
                    &LineStyle {
                        font_id: &font_id,
                        char_w,
                        query_lower,
                        level_colors,
                        dim: false,
                    },
                    theme,
                );
                let click = ui.interact(rect, id.with(("hit_row", hit)), egui::Sense::click());
                if click.clicked() {
                    clicked = Some(hit);
                }
            }
        });
        state.offset = output.state.offset.y;
        state.view_height = output.inner_rect.height();

        if let Some(hit) = clicked {
            state.selected = hit;
            committed = Some(hit);
            ui.memory_mut(|m| m.request_focus(id));
        }
        let has_focus = ui.memory(|m| m.has_focus(id));
        if has_focus {
            // Focus outline: two keyboard targets share the panel, show which one listens.
            ui.painter().rect_stroke(
                output.inner_rect,
                0.0,
                Stroke::new(1.0, theme.accent_color().gamma_multiply(0.8)),
                egui::StrokeKind::Inside,
            );
        }
        ui.data_mut(|d| d.insert_temp(id, state));
        HitListOutput {
            committed,
            has_focus,
        }
    }
}

/// One row of a grouped hit list: the header of a group, or a hit of a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupRow {
    Header(usize),
    Hit { group: usize, hit: usize },
}

/// Groups flattened into the rows of one list: a header per group, then its hits while
/// it is expanded. Row `i` is resolved by a binary search over the first row of each
/// group, so building it is O(groups) and a lookup O(log groups), whatever the hits.
#[derive(Debug, Clone, Default)]
pub struct GroupLayout {
    starts: Vec<usize>,
    len: usize,
}

impl GroupLayout {
    /// `groups` gives each group's hit count and whether it is expanded.
    pub fn new(groups: impl IntoIterator<Item = (usize, bool)>) -> Self {
        let mut starts = Vec::new();
        let mut len = 0;
        for (hits, expanded) in groups {
            starts.push(len);
            len += 1 + if expanded { hits } else { 0 };
        }
        Self { starts, len }
    }

    /// Rows in the list, headers included.
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// What row `row` shows.
    pub fn row(&self, row: usize) -> Option<GroupRow> {
        if row >= self.len {
            return None;
        }
        let group = self.starts.partition_point(|&s| s <= row) - 1;
        Some(match row - self.starts[group] {
            0 => GroupRow::Header(group),
            n => GroupRow::Hit { group, hit: n - 1 },
        })
    }

    /// Row showing `target`, if it is in the list (a hit of a collapsed group is not).
    pub fn row_of(&self, target: GroupRow) -> Option<usize> {
        let (group, offset) = match target {
            GroupRow::Header(g) => (g, 0),
            GroupRow::Hit { group, hit } => (group, hit + 1),
        };
        let start = *self.starts.get(group)?;
        let end = self.starts.get(group + 1).copied().unwrap_or(self.len);
        (start + offset < end).then_some(start + offset)
    }
}

/// One group of a `GroupedHitList`: the hits of one stream and how its header reads.
pub struct HitGroup<'a> {
    pub engine: &'a TailEngine,
    /// Line indices in file order.
    pub hits: &'a [usize],
    /// Header text (stream name and match count).
    pub title: String,
    /// Notes after the title (progress, capped, stale...), each in its colour.
    pub notes: Vec<(String, Color32)>,
    pub collapsed: bool,
    /// The rows are drawn faded (a stale group).
    pub dim: bool,
}

/// What happened in a grouped list this frame.
#[derive(Debug, Default, Clone, Copy)]
pub struct GroupedOutput {
    /// Hit `(group, hit)` committed by a click or `Enter`.
    pub committed: Option<(usize, usize)>,
    /// Group whose header was clicked or got `Enter`: collapse or expand it.
    pub toggled: Option<usize>,
    pub has_focus: bool,
}

/// A virtualized list of the hits of several streams, grouped under collapsible headers.
/// Rows are drawn like `HitList`'s (line number, level colour, query tint) and the
/// keyboard works the same way; only the rows on screen read their line.
pub struct GroupedHitList<'a> {
    id: egui::Id,
    groups: &'a [HitGroup<'a>],
    query_lower: String,
    font_size: f32,
    level_colors: bool,
}

impl<'a> GroupedHitList<'a> {
    pub fn new(id: egui::Id, groups: &'a [HitGroup<'a>], query: &str) -> Self {
        Self {
            id,
            groups,
            query_lower: query.trim().to_lowercase(),
            font_size: 13.0,
            level_colors: true,
        }
    }

    pub fn font_size(mut self, font_size: f32) -> Self {
        self.font_size = font_size;
        self
    }

    pub fn level_colors(mut self, level_colors: bool) -> Self {
        self.level_colors = level_colors;
        self
    }

    /// The flattened rows of `groups`.
    pub fn layout(groups: &[HitGroup]) -> GroupLayout {
        GroupLayout::new(groups.iter().map(|g| (g.hits.len(), !g.collapsed)))
    }

    /// Id of the header row of group `group` (tests click it).
    pub fn header_id(id: egui::Id, group: usize) -> egui::Id {
        id.with(("group_header", group))
    }

    /// Id of the row of hit `hit` of group `group`.
    pub fn hit_id(id: egui::Id, group: usize, hit: usize) -> egui::Id {
        id.with(("group_hit", group, hit))
    }

    pub fn show(self, ui: &mut Ui, theme: &CyberTheme) -> GroupedOutput {
        let id = self.id;
        let layout = Self::layout(self.groups);
        let len = layout.len();
        let font_id = egui::FontId::monospace(self.font_size);
        let font_row_h = ui.ctx().fonts_mut(|f| f.row_height(&font_id));
        let char_w = ui.ctx().fonts_mut(|f| f.glyph_width(&font_id, '0'));
        let row_height = (font_row_h * 1.25).max(16.0).ceil();

        let mut state: HitListState = ui.data(|d| d.get_temp(id)).unwrap_or_default();
        if len > 0 {
            state.selected = state.selected.min(len - 1);
        }

        let outer = ui.available_rect_before_wrap();
        let list_resp = ui.interact(outer, id, egui::Sense::focusable_noninteractive());
        let mut out = GroupedOutput::default();
        let focused = list_resp.has_focus();
        // Enter on the selection: a header toggles its group, a hit is committed.
        let activate = |row: usize, out: &mut GroupedOutput| match layout.row(row) {
            Some(GroupRow::Header(g)) => out.toggled = Some(g),
            Some(GroupRow::Hit { group, hit }) => out.committed = Some((group, hit)),
            None => {}
        };
        if focused {
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    id,
                    egui::EventFilter {
                        tab: false,
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        escape: true,
                    },
                )
            });
            let page = ((state.view_height / row_height).floor() as usize).max(1);
            for key in take_nav_keys(ui) {
                state.selected = move_selection(state.selected, len, key, page);
                state.reveal = Some(state.selected);
            }
            if len > 0 && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
                activate(state.selected, &mut out);
            }
            if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                ui.memory_mut(|m| m.surrender_focus(id));
            }
        }
        if len == 0 {
            ui.data_mut(|d| d.insert_temp(id, state));
            out.has_focus = focused;
            return out;
        }

        let reveal = state
            .reveal
            .take()
            .and_then(|row| state.reveal_offset(row, row_height));
        let mut scroll = ScrollArea::vertical()
            .id_salt(id.with("scroll"))
            .auto_shrink([false, false]);
        if let Some(offset) = reveal {
            scroll = scroll.vertical_scroll_offset(offset);
        }

        let num_w = char_w * 9.0;
        let marker_w = char_w * 2.0;
        let mut clicked = None;
        let selected = state.selected;
        let groups = self.groups;
        let style = LineStyle {
            font_id: &font_id,
            char_w,
            query_lower: &self.query_lower,
            level_colors: self.level_colors,
            dim: false,
        };
        let output = scroll.show_rows(ui, row_height, len, |ui, rows| {
            let painter = ui.painter().clone();
            for row in rows {
                let Some(kind) = layout.row(row) else {
                    continue;
                };
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), row_height),
                    egui::Sense::hover(),
                );
                let text_top = rect.top() + (row_height - font_row_h) / 2.0;
                let row_id = match kind {
                    GroupRow::Header(g) => {
                        let group = &groups[g];
                        let c = theme.accent_color();
                        painter.rect_filled(
                            rect,
                            0.0,
                            Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 28),
                        );
                        let arrow = if group.collapsed { "▶" } else { "▼" };
                        let title_color = if group.dim {
                            theme.text_dim()
                        } else {
                            theme.accent_color()
                        };
                        let format = |color| egui::TextFormat {
                            font_id: font_id.clone(),
                            color,
                            ..Default::default()
                        };
                        let mut job = egui::text::LayoutJob::default();
                        job.append(
                            &format!("{arrow} {}", group.title),
                            0.0,
                            format(title_color),
                        );
                        for (note, color) in &group.notes {
                            job.append(&format!("  {note}"), 0.0, format(*color));
                        }
                        let galley = ui.ctx().fonts_mut(|f| f.layout_job(job));
                        painter.galley(
                            egui::pos2(rect.left() + 2.0, text_top),
                            galley,
                            title_color,
                        );
                        Self::header_id(id, g)
                    }
                    GroupRow::Hit { group: g, hit } => {
                        let group = &groups[g];
                        let line = group.hits[hit];
                        // Indented under the header by the marker column of `HitList`.
                        let left = rect.left() + 2.0 + marker_w;
                        painter.text(
                            egui::pos2(left, text_top),
                            egui::Align2::LEFT_TOP,
                            format!("{:>7}│", line + 1),
                            font_id.clone(),
                            theme.text_dim().gamma_multiply(0.7),
                        );
                        paint_line_text(
                            ui,
                            &painter,
                            group.engine,
                            line,
                            egui::pos2(left + num_w, text_top),
                            rect.right(),
                            &LineStyle {
                                dim: group.dim,
                                ..style
                            },
                            theme,
                        );
                        Self::hit_id(id, g, hit)
                    }
                };
                if focused && selected == row {
                    painter.rect_stroke(
                        rect.shrink(0.5),
                        0.0,
                        Stroke::new(1.0, theme.secondary_accent()),
                        egui::StrokeKind::Inside,
                    );
                }
                if ui.interact(rect, row_id, egui::Sense::click()).clicked() {
                    clicked = Some(row);
                }
            }
        });
        state.offset = output.state.offset.y;
        state.view_height = output.inner_rect.height();

        if let Some(row) = clicked {
            state.selected = row;
            activate(row, &mut out);
            ui.memory_mut(|m| m.request_focus(id));
        }
        out.has_focus = ui.memory(|m| m.has_focus(id));
        if out.has_focus {
            ui.painter().rect_stroke(
                output.inner_rect,
                0.0,
                Stroke::new(1.0, theme.accent_color().gamma_multiply(0.8)),
                egui::StrokeKind::Inside,
            );
        }
        ui.data_mut(|d| d.insert_temp(id, state));
        out
    }
}

/// Keys the list reacts to while it holds keyboard focus, consumed this frame.
fn take_nav_keys(ui: &mut Ui) -> Vec<NavKey> {
    ui.input_mut(|i| {
        let mut keys = Vec::new();
        let mut take = |mods, key, nav| {
            if i.consume_key(mods, key) {
                keys.push(nav);
            }
        };
        take(egui::Modifiers::NONE, egui::Key::ArrowUp, NavKey::Up);
        take(egui::Modifiers::NONE, egui::Key::ArrowDown, NavKey::Down);
        take(egui::Modifiers::NONE, egui::Key::PageUp, NavKey::PageUp);
        take(egui::Modifiers::NONE, egui::Key::PageDown, NavKey::PageDown);
        take(egui::Modifiers::COMMAND, egui::Key::Home, NavKey::First);
        take(egui::Modifiers::COMMAND, egui::Key::End, NavKey::Last);
        keys
    })
}

/// How the text of a listed line is drawn.
#[derive(Clone, Copy)]
struct LineStyle<'a> {
    font_id: &'a egui::FontId,
    char_w: f32,
    query_lower: &'a str,
    level_colors: bool,
    /// Faded (the rows of a stale group).
    dim: bool,
}

/// Draws the text of line `line` of `engine` from `pos`, cut at `right`, coloured by its
/// level and with the query tinted. Only the rows on screen read their line.
#[allow(clippy::too_many_arguments)]
fn paint_line_text(
    ui: &Ui,
    painter: &egui::Painter,
    engine: &TailEngine,
    line: usize,
    pos: egui::Pos2,
    right: f32,
    style: &LineStyle,
    theme: &CyberTheme,
) {
    let fits =
        (((right - pos.x) / style.char_w.max(1.0)).floor().max(0.0) as usize).min(MAX_ROW_CHARS);
    let Some(raw) = engine.get_line(line) else {
        return;
    };
    let text = clip_chars(&raw, fits);
    let mut fg = if style.level_colors {
        theme
            .level_style(engine.level_of(line))
            .map(|s| s.fg)
            .unwrap_or(theme.text_primary())
    } else {
        theme.text_primary()
    };
    if style.dim {
        fg = fg.gamma_multiply(0.45);
    }
    let job = row_job(text, style.query_lower, style.font_id, fg);
    let galley = ui.ctx().fonts_mut(|f| f.layout_job(job));
    painter.galley(pos, galley, fg);
}

/// Layout of one listed line: the text in `fg`, the query occurrences on the search
/// yellow in black.
fn row_job(
    text: &str,
    query_lower: &str,
    font_id: &egui::FontId,
    fg: Color32,
) -> egui::text::LayoutJob {
    let plain = egui::TextFormat {
        font_id: font_id.clone(),
        color: fg,
        ..Default::default()
    };
    let tinted = egui::TextFormat {
        font_id: font_id.clone(),
        color: Color32::BLACK,
        background: QUERY_BG,
        ..Default::default()
    };
    let mut job = egui::text::LayoutJob::default();
    let mut pos = 0;
    for (start, end) in query_ranges(text, query_lower) {
        if start > pos {
            job.append(&text[pos..start], 0.0, plain.clone());
        }
        job.append(&text[start..end], 0.0, tinted.clone());
        pos = end;
    }
    if pos < text.len() {
        job.append(&text[pos..], 0.0, plain);
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_moves_and_clamps() {
        assert_eq!(move_selection(0, 10, NavKey::Up, 5), 0);
        assert_eq!(move_selection(0, 10, NavKey::Down, 5), 1);
        assert_eq!(move_selection(9, 10, NavKey::Down, 5), 9);
        assert_eq!(move_selection(3, 10, NavKey::PageDown, 5), 8);
        assert_eq!(move_selection(8, 10, NavKey::PageDown, 5), 9);
        assert_eq!(move_selection(3, 10, NavKey::PageUp, 5), 0);
        assert_eq!(move_selection(4, 10, NavKey::First, 5), 0);
        assert_eq!(move_selection(4, 10, NavKey::Last, 5), 9);
        // A stale selection past a shrunk list lands on the last hit.
        assert_eq!(move_selection(50, 10, NavKey::Up, 5), 8);
        assert_eq!(move_selection(3, 0, NavKey::Down, 5), 0);
    }

    #[test]
    fn selection_snaps_to_the_current_match_only_when_it_changes() {
        let mut state = HitListState::default();
        assert!(state.sync_to_current(Some(4)));
        assert_eq!((state.selected, state.reveal), (4, Some(4)));
        // The user walks the list: the current match did not change, nothing snaps back.
        state.selected = 9;
        state.reveal = None;
        assert!(!state.sync_to_current(Some(4)));
        assert_eq!(state.selected, 9);
        // F3 moves the current match: the selection follows it.
        assert!(state.sync_to_current(Some(5)));
        assert_eq!((state.selected, state.reveal), (5, Some(5)));
    }

    #[test]
    fn reveal_scrolls_only_when_the_row_is_off_screen() {
        let state = HitListState {
            offset: 100.0,
            view_height: 100.0,
            ..Default::default()
        };
        assert_eq!(state.reveal_offset(6, 20.0), None, "rows 5..10 are visible");
        let off = state.reveal_offset(40, 20.0).unwrap();
        assert!(
            off <= 800.0 && off + 100.0 >= 820.0,
            "row 40 lands inside the view"
        );
        assert_eq!(state.reveal_offset(0, 20.0), Some(0.0));
        // Never drawn: even the first row has to be scrolled to.
        assert_eq!(HitListState::default().reveal_offset(0, 20.0), Some(0.0));
    }

    #[test]
    fn groups_flatten_into_headers_and_hits() {
        // Three groups: 2 hits, 3 hits collapsed, 1 hit.
        let layout = GroupLayout::new([(2, true), (3, false), (1, true)]);
        assert_eq!(layout.len(), 6);
        let rows: Vec<GroupRow> = (0..layout.len()).filter_map(|r| layout.row(r)).collect();
        assert_eq!(
            rows,
            vec![
                GroupRow::Header(0),
                GroupRow::Hit { group: 0, hit: 0 },
                GroupRow::Hit { group: 0, hit: 1 },
                GroupRow::Header(1),
                GroupRow::Header(2),
                GroupRow::Hit { group: 2, hit: 0 },
            ]
        );
        assert_eq!(layout.row(6), None);
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(layout.row_of(*row), Some(i));
        }
        assert_eq!(
            layout.row_of(GroupRow::Hit { group: 1, hit: 0 }),
            None,
            "a collapsed group lists no hit"
        );
        assert_eq!(layout.row_of(GroupRow::Header(3)), None);
        // An empty group still has its header; no group, no rows.
        assert_eq!(GroupLayout::new([(0, true)]).len(), 1);
        assert!(GroupLayout::new([]).is_empty());
        // The keyboard walks the flattened rows across groups.
        assert_eq!(move_selection(2, layout.len(), NavKey::Down, 5), 3);
        assert_eq!(move_selection(0, layout.len(), NavKey::Last, 5), 5);
    }

    #[test]
    fn query_tint_ranges_and_clipping() {
        assert_eq!(
            query_ranges("Timeout after TIMEOUT", "timeout"),
            vec![(0, 7), (14, 21)]
        );
        assert_eq!(clip_chars("città ok", 5), "città");
        assert_eq!(clip_chars("abc", 10), "abc");
        let job = row_job(
            "a timeout b",
            "timeout",
            &egui::FontId::monospace(12.0),
            Color32::WHITE,
        );
        assert_eq!(job.sections.len(), 3);
        assert_eq!(job.sections[1].format.background, QUERY_BG);
    }
}
