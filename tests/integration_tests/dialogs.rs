// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use fasttail::config::FastTailConfig;
use fasttail::ui::FastTailApp;

fn frame(app: &mut FastTailApp, ctx: &egui::Context, events: Vec<egui::Event>) {
    let input = egui::RawInput {
        events,
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ui| app.render_ui(ui));
    out.textures_delta.clear();
}

fn escape() -> Vec<egui::Event> {
    vec![egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]
}

#[test]
fn escape_closes_only_the_topmost_dialog() {
    let dir = tempfile::tempdir().unwrap();
    let config = FastTailConfig {
        spool_dir: Some(dir.path().to_path_buf()),
        settings_open: true,
        help_open: true,
        ..Default::default()
    };
    let mut app = FastTailApp::from_config(config);
    let ctx = egui::Context::default();
    frame(&mut app, &ctx, vec![]);
    // Help is drawn after Settings, so it opened on top.
    frame(&mut app, &ctx, escape());
    assert!(!app.config.help_open);
    assert!(app.config.settings_open, "the dialog below stays open");
    frame(&mut app, &ctx, escape());
    assert!(!app.config.settings_open);
}
