// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use fasttail::actions::ActionId;
use fasttail::config::FastTailConfig;
use fasttail::keymap::{KeyBinding, KeyMap};
use fasttail::ui::keys::take_remapped;
use std::collections::BTreeMap;

fn press(key: egui::Key, modifiers: egui::Modifiers) -> egui::RawInput {
    egui::RawInput {
        events: vec![egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }],
        ..Default::default()
    }
}

/// One frame with `input`: the actions the key map took, and whether the handlers of the
/// default keys still see `seen` pressed.
fn frame(
    ctx: &egui::Context,
    map: &KeyMap,
    input: egui::RawInput,
    seen: (egui::Modifiers, egui::Key),
    focus_text: bool,
) -> (Vec<ActionId>, bool) {
    let mut taken = Vec::new();
    let mut still = false;
    let mut text = String::new();
    let mut output = ctx.run_ui(input, |ui| {
        let edit = ui.add(egui::TextEdit::singleline(&mut text).id_salt("field"));
        if focus_text {
            edit.request_focus();
        }
        taken = take_remapped(ui.ctx(), map);
        still = ui.input_mut(|i| i.consume_key(seen.0, seen.1));
    });
    output.textures_delta.clear();
    (taken, still)
}

#[test]
fn a_rebound_action_fires_on_its_new_key_and_its_old_key_fires_nothing() {
    let mut cfg = FastTailConfig::default();
    fasttail::keymap::set_override(
        &mut cfg.shortcuts,
        ActionId::BookmarkToggle,
        &[KeyBinding::parse("Ctrl+B").unwrap()],
    );
    let ini = cfg.to_ini();
    let cfg = FastTailConfig::from_ini(&ini);
    let map = KeyMap::build(&cfg.shortcuts, &cfg.external_tools);
    let ctx = egui::Context::default();
    let ctrl = egui::Modifiers::COMMAND;
    let (taken, _) = frame(
        &ctx,
        &map,
        press(egui::Key::B, ctrl),
        (ctrl, egui::Key::B),
        false,
    );
    assert_eq!(taken, vec![ActionId::BookmarkToggle]);
    // Ctrl+F2 is retired: its handler no longer sees it.
    let (taken, still) = frame(
        &ctx,
        &map,
        press(egui::Key::F2, ctrl),
        (ctrl, egui::Key::F2),
        false,
    );
    assert!(taken.is_empty());
    assert!(!still, "the old key must not reach the handler");
    // A key nobody remapped reaches its handler as before.
    let (taken, still) = frame(
        &ctx,
        &map,
        press(egui::Key::F3, egui::Modifiers::NONE),
        (egui::Modifiers::NONE, egui::Key::F3),
        false,
    );
    assert!(taken.is_empty() && still);
    // Without overrides nothing is touched.
    let defaults = KeyMap::build(&BTreeMap::new(), &[]);
    let (taken, still) = frame(
        &ctx,
        &defaults,
        press(egui::Key::F2, ctrl),
        (ctrl, egui::Key::F2),
        false,
    );
    assert!(taken.is_empty() && still);
}

#[test]
fn a_plain_key_is_left_to_a_text_field() {
    let mut shortcuts = BTreeMap::new();
    shortcuts.insert("view.wrap.toggle".to_string(), "J".to_string());
    let map = KeyMap::build(&shortcuts, &[]);
    let ctx = egui::Context::default();
    let none = egui::Modifiers::NONE;
    // A first frame gives the field the keyboard.
    let _ = frame(
        &ctx,
        &map,
        egui::RawInput::default(),
        (none, egui::Key::J),
        true,
    );
    let (taken, still) = frame(
        &ctx,
        &map,
        press(egui::Key::J, none),
        (none, egui::Key::J),
        true,
    );
    assert!(taken.is_empty(), "typing J in a field does not wrap");
    assert!(still);
    // Without a focused field J toggles wrap.
    ctx.memory_mut(|m| m.stop_text_input());
    let (taken, _) = frame(
        &ctx,
        &map,
        press(egui::Key::J, none),
        (none, egui::Key::J),
        false,
    );
    assert_eq!(taken, vec![ActionId::Wrap]);
}

#[test]
fn the_shortcuts_window_draws_every_group() {
    let mut cfg = FastTailConfig::default();
    cfg.shortcuts
        .insert("search.next".to_string(), "F3, Ctrl+N".to_string());
    let map = KeyMap::build(&cfg.shortcuts, &cfg.external_tools);
    let mut dialog = fasttail::ui::shortcuts_dialog::ShortcutsDialog::default();
    dialog.open = true;
    let ctx = egui::Context::default();
    for _ in 0..2 {
        let mut output = ctx.run_ui(Default::default(), |ui| {
            let changed = dialog.show(ui.ctx(), &mut cfg, &map, &fasttail::theme::CyberTheme::Tron);
            assert!(!changed);
        });
        output.textures_delta.clear();
    }
    assert!(dialog.open && !dialog.recording());
    assert_eq!(map.bindings(ActionId::SearchNext).len(), 2);
}
