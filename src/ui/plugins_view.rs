// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Plugins in the window (`crate::plugins`, `crate::plugin_source`): the 🔌 toolbar menu
//! of the enabled source plugins, the parameter window that opens one, and the Plugins
//! settings window (list, enable with the approval of the command, disable, open the
//! folder, reload).

use crate::config::FastTailConfig;
use crate::i18n::{t, Language};
use crate::plugins::{Catalog, Kind, ParamType, Plugin, State};
use crate::theme::CyberTheme;
use egui::RichText;
use std::collections::BTreeMap;
use std::path::Path;

/// The parameter window of a source plugin being opened.
pub struct ParamDialog {
    pub plugin: String,
    pub values: BTreeMap<String, String>,
    error: Option<String>,
}

/// Plugins in the window.
#[derive(Default)]
pub struct PluginsView {
    /// The Plugins settings window.
    pub open: bool,
    catalog: Option<Catalog>,
    /// The source plugin whose approval waits for a confirmation.
    confirm: Option<String>,
    pub dialog: Option<ParamDialog>,
}

/// Opens `dir` in the file manager of the system.
fn open_folder(dir: &Path) {
    let _ = std::fs::create_dir_all(dir);
    let program = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(program)
        .arg(dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

/// The command line a source plugin runs, placeholders as written.
pub fn command_line(plugin: &Plugin) -> String {
    plugin
        .source
        .as_ref()
        .map(|s| {
            std::iter::once(s.command.as_str())
                .chain(s.args.iter().map(String::as_str))
                .map(|a| {
                    if a.is_empty() || a.contains(char::is_whitespace) {
                        format!("\"{a}\"")
                    } else {
                        a.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default()
}

impl PluginsView {
    /// The plugins found (looked for on first use and on Reload).
    pub fn catalog(&mut self) -> &Catalog {
        self.catalog
            .get_or_insert_with(|| crate::plugins::discover(&crate::plugins::default_dir()))
    }

    /// Looks at the plugins folder again and loads the formats of the enabled plugins.
    pub fn reload(&mut self, cfg: &FastTailConfig) {
        self.catalog = Some(crate::plugins::discover(&crate::plugins::default_dir()));
        crate::log_format::load_with_plugins(&cfg.plugins);
    }

    /// Opens the parameter window of `name` with `values` (the defaults for the rest).
    pub fn ask(&mut self, name: &str, values: &BTreeMap<String, String>) {
        let Some(plugin) = self.catalog().find(name).cloned() else {
            return;
        };
        let mut filled = BTreeMap::new();
        for p in plugin.source.iter().flat_map(|s| &s.params) {
            let v = values.get(&p.id).cloned().unwrap_or_else(|| {
                if p.kind == ParamType::Choice && p.default.is_empty() {
                    p.choices.first().cloned().unwrap_or_default()
                } else {
                    p.default.clone()
                }
            });
            filled.insert(p.id.clone(), v);
        }
        self.dialog = Some(ParamDialog {
            plugin: name.to_string(),
            values: filled,
            error: None,
        });
    }

    /// The 🔌 menu: the enabled source plugins (a click asks their values, or opens one
    /// without parameters at once) and the settings. Returns a plugin to open.
    pub fn menu(
        &mut self,
        ui: &mut egui::Ui,
        cfg: &FastTailConfig,
        theme: &CyberTheme,
    ) -> Option<(String, BTreeMap<String, String>)> {
        let lang = cfg.language;
        let mut chosen = None;
        let sources: Vec<Plugin> = self
            .catalog()
            .plugins
            .iter()
            .filter(|p| p.kind == Kind::Source && cfg.plugins.state(p) == State::Enabled)
            .cloned()
            .collect();
        if sources.is_empty() {
            ui.label(
                RichText::new(t(lang, "plug_none"))
                    .italics()
                    .color(theme.text_dim()),
            );
        }
        for plugin in &sources {
            let label = format!("🔌 {}", plugin.name);
            let mut resp = ui.button(RichText::new(label).monospace());
            if !plugin.description.is_empty() {
                resp = resp.on_hover_text(&plugin.description);
            }
            if resp.clicked() {
                let params = plugin.source.as_ref().map_or(0, |s| s.params.len());
                if params == 0 {
                    chosen = Some((plugin.name.clone(), BTreeMap::new()));
                } else {
                    self.ask(&plugin.name, &BTreeMap::new());
                }
                ui.close();
            }
        }
        ui.separator();
        if ui.button(format!("⚙ {}", t(lang, "plug_open"))).clicked() {
            self.open = true;
            ui.close();
        }
        chosen
    }

    /// The parameter window; returns the plugin and the values to open it with.
    pub fn show_dialog(
        &mut self,
        ctx: &egui::Context,
        cfg: &FastTailConfig,
        theme: &CyberTheme,
    ) -> Option<(String, BTreeMap<String, String>)> {
        let lang = cfg.language;
        let name = self.dialog.as_ref()?.plugin.clone();
        let plugin = self.catalog().find(&name).cloned()?;
        let params = plugin.source.clone().unwrap_or_default().params;
        let dialog = self.dialog.as_mut()?;
        let mut open = true;
        let mut chosen = None;
        let mut cancel = false;
        egui::Window::new(
            RichText::new(format!(
                "🔌 {}",
                t(lang, "plug_param_title").replace("{0}", &name)
            ))
            .monospace()
            .color(theme.accent_color()),
        )
        .id(egui::Id::new("fasttail_plugin_params"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(380.0)
        .show(ctx, |ui| {
            if !plugin.description.is_empty() {
                ui.label(RichText::new(&plugin.description).color(theme.text_dim()));
            }
            egui::Grid::new("plugin_params")
                .num_columns(2)
                .show(ui, |ui| {
                    for p in &params {
                        ui.label(&p.label);
                        let value = dialog.values.entry(p.id.clone()).or_default();
                        match p.kind {
                            ParamType::Choice => {
                                egui::ComboBox::from_id_salt(("plugin_param", &p.id))
                                    .selected_text(value.clone())
                                    .show_ui(ui, |ui| {
                                        for c in &p.choices {
                                            ui.selectable_value(value, c.clone(), c);
                                        }
                                    });
                            }
                            kind => {
                                ui.add(
                                    egui::TextEdit::singleline(value)
                                        .password(kind == ParamType::Secret)
                                        .desired_width(220.0),
                                );
                            }
                        }
                        ui.end_row();
                    }
                });
            if params.iter().any(|p| p.kind == ParamType::Secret) {
                ui.label(
                    RichText::new(t(lang, "plug_secret_note"))
                        .small()
                        .color(theme.text_dim()),
                );
            }
            if let Some(e) = &dialog.error {
                ui.label(RichText::new(e).color(theme.warn_color()));
            }
            ui.horizontal(|ui| {
                if ui.button(t(lang, "plug_param_open")).clicked() {
                    let bad = params.iter().find_map(|p| {
                        let v = dialog.values.get(&p.id).map(String::as_str).unwrap_or("");
                        if p.kind == ParamType::Secret && v.is_empty() {
                            return Some(format!("{}: {}", p.label, t(lang, "plug_value_needed")));
                        }
                        crate::plugins::check_value(p, v)
                    });
                    match bad {
                        Some(e) => dialog.error = Some(e),
                        None => chosen = Some((name.clone(), dialog.values.clone())),
                    }
                }
                if ui.button(t(lang, "session_cancel")).clicked() {
                    cancel = true;
                }
            });
        });
        if !open || cancel || chosen.is_some() {
            self.dialog = None;
        }
        chosen
    }

    /// The Plugins settings window; returns whether the approvals changed.
    pub fn show_settings(
        &mut self,
        ctx: &egui::Context,
        cfg: &mut FastTailConfig,
        theme: &CyberTheme,
    ) -> bool {
        if !self.open {
            return false;
        }
        let lang = cfg.language;
        let mut changed = false;
        let mut open = self.open;
        let dir = crate::plugins::default_dir();
        egui::Window::new(
            RichText::new(format!("🔌 {}", t(lang, "plug_title")))
                .monospace()
                .color(theme.accent_color()),
        )
        .id(egui::Id::new("fasttail_plugins"))
        .open(&mut open)
        .resizable(true)
        .default_width(560.0)
        .show(ctx, |ui| {
            ui.label(
                RichText::new(t(lang, "plug_intro").replace("{0}", &dir.display().to_string()))
                    .color(theme.text_dim()),
            );
            ui.horizontal(|ui| {
                if ui.button(t(lang, "plug_open_folder")).clicked() {
                    open_folder(&dir);
                }
                if ui
                    .button(format!("⟳ {}", t(lang, "plug_reload")))
                    .on_hover_text(t(lang, "plug_reload_tip"))
                    .clicked()
                {
                    self.reload(cfg);
                }
            });
            ui.separator();
            changed |= self.plugin_list(ui, cfg, theme, lang);
        });
        self.open = open;
        if !self.open {
            self.confirm = None;
        }
        if changed {
            crate::log_format::load_with_plugins(&cfg.plugins);
        }
        changed
    }

    fn plugin_list(
        &mut self,
        ui: &mut egui::Ui,
        cfg: &mut FastTailConfig,
        theme: &CyberTheme,
        lang: Language,
    ) -> bool {
        let mut changed = false;
        let catalog = self.catalog().clone();
        let format_errors = crate::log_format::library().plugin_errors.clone();
        if catalog.plugins.is_empty() && catalog.broken.is_empty() {
            ui.label(
                RichText::new(
                    t(lang, "plug_empty").replace("{0}", &catalog.dir.display().to_string()),
                )
                .italics()
                .color(theme.text_dim()),
            );
        }
        egui::ScrollArea::vertical()
            .max_height(420.0)
            .show(ui, |ui| {
                for plugin in &catalog.plugins {
                    let state = cfg.plugins.state(plugin);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("{} {}", plugin.name, plugin.version))
                                .monospace()
                                .strong(),
                        );
                        let kind = match plugin.kind {
                            Kind::Source => t(lang, "plug_kind_source"),
                            Kind::Formats => t(lang, "plug_kind_formats"),
                        };
                        ui.label(RichText::new(kind).color(theme.text_dim()));
                        let (text, color) = match state {
                            State::Enabled => (t(lang, "plug_enabled"), theme.accent_color()),
                            State::Disabled => (t(lang, "plug_disabled"), theme.text_dim()),
                            State::Changed => (t(lang, "plug_changed"), theme.warn_color()),
                        };
                        ui.label(RichText::new(text).color(color));
                        if state == State::Enabled {
                            if ui.small_button(t(lang, "plug_disable")).clicked() {
                                cfg.plugins.set_enabled(plugin, false);
                                changed = true;
                            }
                        } else if ui.small_button(t(lang, "plug_enable")).clicked() {
                            if plugin.kind == Kind::Source {
                                self.confirm = Some(plugin.name.clone());
                            } else {
                                cfg.plugins.set_enabled(plugin, true);
                                changed = true;
                            }
                        }
                    });
                    if !plugin.description.is_empty() {
                        ui.label(
                            RichText::new(&plugin.description)
                                .small()
                                .color(theme.text_dim()),
                        );
                    }
                    if plugin.kind == Kind::Source {
                        ui.label(
                            RichText::new(format!("$ {}", command_line(plugin)))
                                .monospace()
                                .small()
                                .color(theme.text_dim()),
                        );
                    }
                    for w in &plugin.warnings {
                        ui.label(
                            RichText::new(format!("⚠ {w}"))
                                .small()
                                .color(theme.warn_color()),
                        );
                    }
                    for (_, file, e) in format_errors.iter().filter(|(p, _, _)| *p == plugin.name) {
                        let name = file
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        ui.label(
                            RichText::new(format!("⚠ {name}: {e}"))
                                .small()
                                .color(theme.error_color()),
                        );
                    }
                    if self.confirm.as_deref() == Some(plugin.name.as_str()) {
                        ui.group(|ui| {
                            ui.label(
                                RichText::new(t(lang, "plug_confirm").replace("{0}", &plugin.name))
                                    .color(theme.warn_color()),
                            );
                            ui.label(RichText::new(command_line(plugin)).monospace());
                            ui.horizontal(|ui| {
                                if ui.button(t(lang, "plug_confirm_ok")).clicked() {
                                    cfg.plugins.set_enabled(plugin, true);
                                    self.confirm = None;
                                    changed = true;
                                }
                                if ui.button(t(lang, "session_cancel")).clicked() {
                                    self.confirm = None;
                                }
                            });
                        });
                    }
                    ui.add_space(6.0);
                }
                for broken in &catalog.broken {
                    let name = broken
                        .dir
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    ui.label(
                        RichText::new(format!(
                            "⚠ {name} — {}: {}",
                            t(lang, "plug_broken"),
                            broken.error
                        ))
                        .monospace()
                        .color(theme.error_color()),
                    );
                }
            });
        changed
    }
}
