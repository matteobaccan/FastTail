// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The Scratchpad tab of the window: its find box, clear button and editor over the
//! session's pad (`crate::scratchpad`).

use crate::i18n::{t, Language};
use crate::theme::CyberTheme;
use egui::{RichText, Ui};

pub use crate::scratchpad::*;

/// Brings the Scratchpad tab to the front, adding it below the first leaf when it is
/// not in the dock.
pub fn open_tab(dock: &mut egui_dock::DockState<crate::ui::dock::FastTailTab>) {
    use crate::ui::dock::FastTailTab;
    use egui_dock::{DockState, NodeIndex, NodePath, SurfaceIndex};
    let tab = FastTailTab::Scratchpad;
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
            let [_, new] = surface.split_right(NodeIndex(leaf), 0.62, vec![tab]);
            dock.set_focused_node_and_surface(NodePath::new(SurfaceIndex::main(), new));
        }
        None => surface.push_to_first_leaf(tab),
    }
}

/// Id of the pad's editor.
pub fn editor_id() -> egui::Id {
    egui::Id::new("fasttail_scratchpad_editor")
}

/// The body of the Scratchpad tab.
pub fn render(
    ui: &mut Ui,
    pad: &mut Scratchpad,
    theme: &CyberTheme,
    lang: Language,
    font_size: f32,
) {
    let mut find_next = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new("🔍").monospace());
        let resp = ui.add(
            egui::TextEdit::singleline(&mut pad.find)
                .hint_text(t(lang, "scratch_find_hint"))
                .desired_width(200.0),
        );
        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            find_next = true;
        }
        if ui
            .add_enabled(!pad.find.is_empty(), egui::Button::new("▼"))
            .on_hover_text(t(lang, "search_next"))
            .on_disabled_hover_text(t(lang, "scratch_find_hint"))
            .clicked()
        {
            find_next = true;
        }
        if !pad.find.is_empty()
            && ui
                .small_button("✖")
                .on_hover_text(t(lang, "clear_search"))
                .clicked()
        {
            pad.find.clear();
        }
        ui.separator();
        if ui
            .button(format!("💾 {}", t(lang, "scratch_save_as")))
            .clicked()
        {
            let target = rfd::FileDialog::new()
                .set_title(t(lang, "scratch_title"))
                .set_file_name("scratchpad.txt")
                .add_filter("Text (*.txt)", &["txt"])
                .save_file();
            if let Some(path) = target {
                pad.notice = Some(
                    match crate::config::overwrite_regular_file(&path, pad.text.as_bytes()) {
                        Ok(()) => (
                            t(lang, "report_saved").replace("{path}", &path.display().to_string()),
                            false,
                        ),
                        Err(e) => (e.to_string(), true),
                    },
                );
            }
        }
        if pad.confirm_clear {
            ui.label(
                RichText::new(t(lang, "scratch_clear_confirm"))
                    .monospace()
                    .color(theme.warn_color()),
            );
            if ui.button(t(lang, "session_ok")).clicked() {
                pad.text.clear();
                pad.paths.clear();
                pad.touch();
                pad.confirm_clear = false;
            }
            if ui.button(t(lang, "session_cancel")).clicked() {
                pad.confirm_clear = false;
            }
        } else if ui
            .add_enabled(
                !pad.text.is_empty(),
                egui::Button::new(format!("✖ {}", t(lang, "scratch_clear"))),
            )
            .on_disabled_hover_text(t(lang, "scratch_empty_tip"))
            .clicked()
        {
            pad.confirm_clear = true;
        }
        ui.label(
            RichText::new(format!(
                "{} KB / {} KB",
                pad.text.len().div_ceil(1024),
                MAX_SCRATCHPAD_BYTES / 1024
            ))
            .monospace()
            .size(10.5)
            .color(theme.text_dim()),
        );
    });
    if let Some((text, warn)) = &pad.notice {
        ui.label(RichText::new(text).monospace().size(11.0).color(if *warn {
            theme.warn_color()
        } else {
            theme.accent_color()
        }));
    }
    ui.label(
        RichText::new(t(lang, "scratch_tip"))
            .monospace()
            .size(10.5)
            .color(theme.text_dim()),
    );
    if pad.text.len() > LARGE_PAD_BYTES {
        ui.label(
            RichText::new(t(lang, "scratch_large"))
                .monospace()
                .size(10.5)
                .color(theme.warn_color()),
        );
    }

    let id = editor_id();
    if find_next && !pad.find.is_empty() {
        // The next occurrence after the cursor (case-insensitive), wrapping around.
        let state = egui::text_edit::TextEditState::load(ui.ctx(), id).unwrap_or_default();
        let from_char = state
            .cursor
            .char_range()
            .map(|r| r.primary.index.0.max(r.secondary.index.0))
            .unwrap_or(0);
        // Matched on the text itself, so the offsets are the text's own.
        let matcher = regex::RegexBuilder::new(&regex::escape(&pad.find))
            .case_insensitive(true)
            .build()
            .ok();
        let from_byte = pad
            .text
            .char_indices()
            .nth(from_char)
            .map_or(pad.text.len(), |(i, _)| i);
        let hit = matcher.and_then(|m| {
            m.find_at(&pad.text, from_byte)
                .or_else(|| m.find(&pad.text))
                .map(|found| (found.start(), found.end()))
        });
        match hit {
            Some((at, to)) => {
                let start = pad.text[..at].chars().count();
                let end = start + pad.text[at..to].chars().count();
                let mut state = state;
                state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::two(
                        egui::text::CCursor::new(start),
                        egui::text::CCursor::new(end),
                    )));
                state.store(ui.ctx(), id);
                ui.memory_mut(|m| m.request_focus(id));
                pad.notice = None;
            }
            None => pad.notice = Some((t(lang, "scratch_not_found").to_string(), true)),
        }
    }

    // ALT + Enter on a reference line: taken before the editor, which would otherwise
    // insert a line break (its Enter ignores an extra ALT), read at the cursor as it is.
    let alt_enter_at = (ui.memory(|m| m.has_focus(id))
        && ui.input_mut(|i| i.consume_key(egui::Modifiers::ALT, egui::Key::Enter)))
    .then(|| {
        egui::text_edit::TextEditState::load(ui.ctx(), id)
            .and_then(|s| s.cursor.char_range())
            .map(|r| r.primary.index.0)
    })
    .flatten();
    let font = egui::FontId::monospace(font_size);
    let output = egui::ScrollArea::both()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::TextEdit::multiline(&mut pad.text)
                .id(id)
                .font(font)
                .code_editor()
                .desired_width(f32::INFINITY)
                .desired_rows(20)
                .lock_focus(true)
                .show(ui)
        })
        .inner;
    if output.response.changed() {
        pad.touch();
    }
    // CTRL + click, or ALT + Enter, on a reference line shows the line it names.
    let ctrl_click = (output.response.clicked() && ui.input(|i| i.modifiers.command))
        .then(|| output.cursor_range.map(|r| r.primary.index.0))
        .flatten();
    if let Some(char_idx) = alt_enter_at.or(ctrl_click) {
        let byte = pad
            .text
            .char_indices()
            .nth(char_idx)
            .map_or(pad.text.len(), |(i, _)| i);
        if let Some((name, line)) = parse_reference(line_at(&pad.text, byte)) {
            pad.jump_request = Some((name, line));
        }
    }
}
