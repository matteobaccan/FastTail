// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The Keyboard shortcuts window (Settings, or the palette): every action of the registry
//! with its keys, grouped as the palette groups them, searchable. A click on a key records
//! a new one (the next key pressed, `Esc` cancels), `+` adds a second key, `✖` removes
//! one, `↺` resets an action, *Reset all* every one. A key another action holds asks to
//! move it here or cancel. Changes go to `[shortcuts]` (`keymap::set_override`).

use crate::actions::{self, Action, ActionId, Category};
use crate::config::FastTailConfig;
use crate::i18n::{t, Language};
use crate::keymap::{self, KeyBinding, KeyMap, MAX_BINDINGS};
use crate::theme::CyberTheme;
use egui::{RichText, Ui};

/// What the window waits for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pending {
    /// The next key for binding `slot` of the action (`slot` past its keys adds one).
    Record(ActionId, usize),
    /// A key held elsewhere: move it here, or cancel.
    Conflict(ActionId, usize, KeyBinding, Holder),
}

/// Who holds a key already.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Holder {
    Action(ActionId),
    Tool(usize),
}

/// The Keyboard shortcuts window.
#[derive(Default)]
pub struct ShortcutsDialog {
    pub open: bool,
    search: String,
    pending: Option<Pending>,
    /// The key captured this frame while recording (`None` for `Esc`).
    captured: Option<Option<KeyBinding>>,
    /// A key that cannot be bound, to say so.
    reserved: Option<KeyBinding>,
}

/// Actions whose keys egui handles itself (the zoom keys): shown, not rebindable.
fn fixed(id: ActionId) -> bool {
    matches!(
        id,
        ActionId::ZoomIn | ActionId::ZoomOut | ActionId::ZoomReset
    )
}

impl ShortcutsDialog {
    /// Whether a key is being recorded: the window then takes every key press.
    pub fn recording(&self) -> bool {
        self.open && matches!(self.pending, Some(Pending::Record(..)))
    }

    /// While recording, takes the frame's key presses before any handler sees them and
    /// keeps the first one (`Esc` cancels).
    pub fn capture(&mut self, ctx: &egui::Context) {
        if !self.recording() {
            return;
        }
        ctx.input_mut(|i| {
            for e in &i.events {
                if let egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } = e
                {
                    if self.captured.is_none() {
                        self.captured = Some(if *key == egui::Key::Escape {
                            None
                        } else {
                            crate::ui::keys::binding_of(*key, *modifiers)
                        });
                    }
                }
            }
            i.events.retain(|e| {
                !matches!(
                    e,
                    egui::Event::Key { .. }
                        | egui::Event::Text(_)
                        | egui::Event::Copy
                        | egui::Event::Cut
                )
            });
        });
    }

    /// Draws the window; returns whether `[shortcuts]` (or a tool's shortcut) changed.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        cfg: &mut FastTailConfig,
        map: &KeyMap,
        theme: &CyberTheme,
    ) -> bool {
        if !self.open {
            self.pending = None;
            return false;
        }
        let lang = cfg.language;
        let mut changed = self.take_capture(cfg, map);
        let mut open = self.open;
        egui::Window::new(
            RichText::new(format!("⌨ {}", t(lang, "keys_title")))
                .monospace()
                .color(theme.accent_color()),
        )
        .id(egui::Id::new("fasttail_shortcuts"))
        .open(&mut open)
        .resizable(true)
        .default_width(560.0)
        .default_height(520.0)
        .show(ctx, |ui| {
            changed |= self.contents(ui, cfg, map, theme, lang);
        });
        self.open = open;
        if !self.open {
            self.pending = None;
        }
        changed
    }

    /// Applies the key captured while recording.
    fn take_capture(&mut self, cfg: &mut FastTailConfig, map: &KeyMap) -> bool {
        let Some(captured) = self.captured.take() else {
            return false;
        };
        let Some(Pending::Record(id, slot)) = self.pending else {
            return false;
        };
        self.reserved = None;
        let Some(b) = captured else {
            self.pending = None;
            return false;
        };
        if b.reserved() {
            self.reserved = Some(b);
            return false;
        }
        let scope = actions::get(id).scope;
        let holder = map.holder(b, scope, id).map(Holder::Action).or_else(|| {
            cfg.external_tools
                .iter()
                .position(|tool| {
                    tool.parsed_shortcut()
                        .is_some_and(|sc| sc.mods == b.mods && sc.key == b.key)
                })
                .map(Holder::Tool)
        });
        match holder {
            Some(h) => {
                self.pending = Some(Pending::Conflict(id, slot, b, h));
                false
            }
            None => {
                self.pending = None;
                bind(cfg, map, id, slot, b);
                true
            }
        }
    }

    fn contents(
        &mut self,
        ui: &mut Ui,
        cfg: &mut FastTailConfig,
        map: &KeyMap,
        theme: &CyberTheme,
        lang: Language,
    ) -> bool {
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text(t(lang, "keys_search"))
                    .desired_width(240.0),
            );
            if ui
                .add_enabled(
                    !cfg.shortcuts.is_empty(),
                    egui::Button::new(t(lang, "keys_reset_all")),
                )
                .on_disabled_hover_text(t(lang, "keys_all_default"))
                .clicked()
            {
                cfg.shortcuts.clear();
                self.pending = None;
                changed = true;
            }
        });
        ui.label(
            RichText::new(t(lang, "keys_tip"))
                .small()
                .color(theme.text_dim()),
        );
        if !map.notices.is_empty() {
            ui.label(RichText::new(t(lang, "keys_notices")).color(theme.warn_color()));
            for n in &map.notices {
                ui.label(
                    RichText::new(n)
                        .small()
                        .monospace()
                        .color(theme.warn_color()),
                );
            }
        }
        if let Some(b) = self.reserved {
            ui.label(
                RichText::new(t(lang, "keys_reserved").replace("{0}", &b.label()))
                    .color(theme.warn_color()),
            );
        }
        match self.pending {
            Some(Pending::Record(..)) => {
                ui.label(
                    RichText::new(t(lang, "keys_press"))
                        .strong()
                        .color(theme.accent_color()),
                );
            }
            Some(Pending::Conflict(id, slot, b, holder)) => {
                let who = match holder {
                    Holder::Action(h) => t(lang, actions::get(h).name).to_string(),
                    Holder::Tool(n) => t(lang, "keys_tool").replace(
                        "{0}",
                        cfg.external_tools
                            .get(n)
                            .map_or("", |tool| tool.name.as_str()),
                    ),
                };
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(
                            t(lang, "keys_conflict")
                                .replace("{0}", &b.label())
                                .replace("{1}", &who),
                        )
                        .color(theme.warn_color()),
                    );
                    if ui.button(t(lang, "keys_move")).clicked() {
                        match holder {
                            Holder::Action(h) => {
                                let rest: Vec<KeyBinding> = map
                                    .bindings(h)
                                    .iter()
                                    .copied()
                                    .filter(|k| *k != b)
                                    .collect();
                                keymap::set_override(&mut cfg.shortcuts, h, &rest);
                            }
                            Holder::Tool(n) => {
                                if let Some(tool) = cfg.external_tools.get_mut(n) {
                                    tool.shortcut = None;
                                }
                            }
                        }
                        bind(cfg, map, id, slot, b);
                        self.pending = None;
                        changed = true;
                    }
                    if ui.button(t(lang, "session_cancel")).clicked() {
                        self.pending = None;
                    }
                });
            }
            None => {}
        }
        ui.separator();
        let needle = self.search.trim().to_lowercase();
        let all = actions::all();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for cat in [
                    Category::Window,
                    Category::Stream,
                    Category::View,
                    Category::Search,
                    Category::Bookmarks,
                    Category::Session,
                    Category::Settings,
                ] {
                    let rows: Vec<&Action> = all
                        .iter()
                        .filter(|a| a.category == cat)
                        .filter(|a| {
                            needle.is_empty()
                                || t(lang, a.name).to_lowercase().contains(&needle)
                                || a.key.contains(&needle)
                                || map
                                    .bindings(a.id)
                                    .iter()
                                    .any(|b| b.label().to_lowercase().contains(&needle))
                        })
                        .collect();
                    if rows.is_empty() {
                        continue;
                    }
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(t(lang, cat.name_key()))
                            .strong()
                            .color(theme.accent_color()),
                    );
                    egui::Grid::new(("fasttail_keys", cat.name_key()))
                        .striped(true)
                        .num_columns(2)
                        .spacing([12.0, 3.0])
                        .show(ui, |ui| {
                            for a in rows {
                                changed |= self.row(ui, cfg, map, a, theme, lang);
                                ui.end_row();
                            }
                        });
                }
            });
        changed
    }

    /// One action: its name, its keys (click one to record), `+`, and `↺` when changed.
    fn row(
        &mut self,
        ui: &mut Ui,
        cfg: &mut FastTailConfig,
        map: &KeyMap,
        a: &Action,
        theme: &CyberTheme,
        lang: Language,
    ) -> bool {
        let mut changed = false;
        ui.label(RichText::new(t(lang, a.name)).monospace())
            .on_hover_text(a.key);
        ui.horizontal(|ui| {
            let keys = map.bindings(a.id).to_vec();
            if fixed(a.id) {
                for b in &keys {
                    ui.label(RichText::new(b.label()).monospace().color(theme.text_dim()))
                        .on_hover_text(t(lang, "keys_fixed"));
                }
                return;
            }
            for (slot, b) in keys.iter().enumerate() {
                let recording = self.pending == Some(Pending::Record(a.id, slot));
                let text = if recording {
                    "…".to_string()
                } else {
                    b.label()
                };
                if ui
                    .selectable_label(recording, RichText::new(text).monospace().strong())
                    .on_hover_text(t(lang, "keys_change"))
                    .clicked()
                {
                    self.reserved = None;
                    self.pending = Some(Pending::Record(a.id, slot));
                }
                if ui
                    .small_button("✖")
                    .on_hover_text(t(lang, "keys_remove"))
                    .clicked()
                {
                    let rest: Vec<KeyBinding> = keys.iter().copied().filter(|k| k != b).collect();
                    keymap::set_override(&mut cfg.shortcuts, a.id, &rest);
                    self.pending = None;
                    changed = true;
                }
            }
            if keys.len() < MAX_BINDINGS {
                let slot = keys.len();
                let recording = self.pending == Some(Pending::Record(a.id, slot));
                let text = if recording {
                    "…".to_string()
                } else if keys.is_empty() {
                    format!("+ {}", t(lang, "keys_none"))
                } else {
                    "+".to_string()
                };
                if ui
                    .selectable_label(
                        recording,
                        RichText::new(text).monospace().color(theme.text_dim()),
                    )
                    .on_hover_text(t(lang, "keys_add"))
                    .clicked()
                {
                    self.reserved = None;
                    self.pending = Some(Pending::Record(a.id, slot));
                }
            }
            if keys != keymap::defaults(a.id)
                && ui
                    .small_button("↺")
                    .on_hover_text(t(lang, "keys_reset"))
                    .clicked()
            {
                cfg.shortcuts.remove(a.key);
                self.pending = None;
                changed = true;
            }
        });
        changed
    }
}

/// Gives action `id` key `b` in `slot` (replacing it, or added past its keys).
fn bind(cfg: &mut FastTailConfig, map: &KeyMap, id: ActionId, slot: usize, b: KeyBinding) {
    let mut keys = map.bindings(id).to_vec();
    keys.retain(|k| *k != b);
    if slot < keys.len() {
        keys[slot] = b;
    } else {
        keys.push(b);
    }
    keys.truncate(MAX_BINDINGS);
    keymap::set_override(&mut cfg.shortcuts, id, &keys);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binding_a_key_writes_only_what_differs() {
        let mut cfg = FastTailConfig::default();
        let map = KeyMap::build(&cfg.shortcuts, &[]);
        let ctrl_b = KeyBinding::parse("Ctrl+B").unwrap();
        bind(&mut cfg, &map, ActionId::BookmarkToggle, 0, ctrl_b);
        assert_eq!(cfg.shortcuts["bookmark.toggle"], "Ctrl+B");
        let map = KeyMap::build(&cfg.shortcuts, &[]);
        // A second key after it.
        bind(
            &mut cfg,
            &map,
            ActionId::BookmarkToggle,
            1,
            KeyBinding::parse("Ctrl+F2").unwrap(),
        );
        assert_eq!(cfg.shortcuts["bookmark.toggle"], "Ctrl+B, Ctrl+F2");
        assert!(fixed(ActionId::ZoomIn) && !fixed(ActionId::Wrap));
    }
}
