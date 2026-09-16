use crate::i18n::{t, Language};
use crate::tail_engine::{HighlightRule, TailEngine};
use crate::theme::CyberTheme;
use egui::{Color32, RichText, ScrollArea, Stroke, Ui, WidgetText};
use egui_dock::TabViewer;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FastTailTab {
    LogStream(usize),
    Filters,
    Highlights,
    Settings,
}

pub struct DockContext<'a> {
    pub engines: &'a mut Vec<TailEngine>,
    pub theme: &'a mut CyberTheme,
    pub language: &'a mut Language,
    pub global_rules: &'a mut Vec<HighlightRule>,
    pub screensaver_enabled: &'a mut bool,
    pub screensaver_timeout_mins: &'a mut u32,
    pub telemetry_enabled: &'a mut bool,
    pub sound_enabled: &'a mut bool,
    pub borderless: &'a mut bool,
    pub show_line_numbers: &'a mut bool,
    pub font_size: &'a mut f32,
    pub search_query: &'a mut String,
}

pub struct FastTailTabViewer<'a> {
    pub ctx: DockContext<'a>,
}

impl<'a> TabViewer for FastTailTabViewer<'a> {
    type Tab = FastTailTab;

    fn title(&mut self, tab: &mut Self::Tab) -> WidgetText {
        match tab {
            FastTailTab::LogStream(idx) => {
                if let Some(engine) = self.ctx.engines.get(*idx) {
                    let file_name = engine
                        .path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("log");
                    WidgetText::RichText(
                        RichText::new(format!("📄 {}", file_name))
                            .monospace()
                            .color(self.ctx.theme.accent_color()),
                    )
                } else {
                    WidgetText::RichText(RichText::new("📄 Closed").monospace())
                }
            }
            FastTailTab::Filters => WidgetText::RichText(
                RichText::new(format!("🔍 {}", t(*self.ctx.language, "filters")))
                    .monospace()
                    .color(self.ctx.theme.accent_color()),
            ),
            FastTailTab::Highlights => WidgetText::RichText(
                RichText::new(format!("⚡ {}", t(*self.ctx.language, "highlight_rules")))
                    .monospace()
                    .color(self.ctx.theme.warn_color()),
            ),
            FastTailTab::Settings => WidgetText::RichText(
                RichText::new(format!("⚙️ {}", t(*self.ctx.language, "settings")))
                    .monospace()
                    .color(self.ctx.theme.text_primary()),
            ),
        }
    }

    fn ui(&mut self, ui: &mut Ui, tab: &mut Self::Tab) {
        match tab {
            FastTailTab::LogStream(idx) => {
                let idx_val = *idx;
                if idx_val < self.ctx.engines.len() {
                    let engine = &mut self.ctx.engines[idx_val];
                    render_log_stream(
                        ui,
                        engine,
                        self.ctx.theme,
                        *self.ctx.language,
                        self.ctx.search_query,
                        self.ctx.show_line_numbers,
                        *self.ctx.font_size,
                    );
                } else {
                    ui.label(t(*self.ctx.language, "no_file_open"));
                }
            }
            FastTailTab::Filters => {
                render_filters_content(ui, self.ctx.engines, self.ctx.theme, *self.ctx.language);
            }
            FastTailTab::Highlights => {
                render_highlights_content(ui, self.ctx.global_rules, self.ctx.engines, self.ctx.theme, *self.ctx.language);
            }
            FastTailTab::Settings => {
                render_settings_content(
                    ui,
                    self.ctx.theme,
                    self.ctx.language,
                    self.ctx.screensaver_enabled,
                    self.ctx.screensaver_timeout_mins,
                    self.ctx.telemetry_enabled,
                    self.ctx.sound_enabled,
                    self.ctx.borderless,
                    self.ctx.show_line_numbers,
                    self.ctx.font_size,
                );
            }
        }
    }
}

fn render_log_stream(
    ui: &mut Ui,
    engine: &mut TailEngine,
    theme: &CyberTheme,
    lang: Language,
    search_query: &mut String,
    show_line_numbers: &mut bool,
    font_size: f32,
) {
    // Stream status bar
    ui.horizontal(|ui| {
        let follow_text = if engine.follow_tail {
            RichText::new(format!("● {}", t(lang, "tailing")))
                .color(theme.accent_color())
                .monospace()
        } else {
            RichText::new(format!("⏸ {}", t(lang, "paused")))
                .color(theme.warn_color())
                .monospace()
        };

        if ui.button(follow_text).on_hover_text(t(lang, "tip_follow_tail")).clicked() {
            engine.follow_tail = !engine.follow_tail;
        }

        ui.separator();

        // Monitor Active / Stopped toggle (Requirement 7)
        let monitor_text = if engine.is_watching {
            RichText::new(format!("👁 {}", t(lang, "monitor_on")))
                .color(theme.accent_color())
                .monospace()
        } else {
            RichText::new(format!("🛑 {}", t(lang, "monitor_off")))
                .color(theme.warn_color())
                .monospace()
        };
        if ui.button(monitor_text).on_hover_text(t(lang, "tip_monitor")).clicked() {
            engine.is_watching = !engine.is_watching;
        }

        ui.separator();

        // Line numbers toggle (Requirement 8)
        let lines_text = if *show_line_numbers {
            RichText::new("# 123").color(theme.accent_color()).monospace()
        } else {
            RichText::new("# ---").color(theme.text_dim()).monospace()
        };
        if ui.button(lines_text).on_hover_text(t(lang, "show_lines")).clicked() {
            *show_line_numbers = !*show_line_numbers;
        }

        ui.separator();

        // Mode Switcher (TXT vs HEX)
        let mode_label = match engine.view_mode {
            crate::tail_engine::ViewMode::Text => RichText::new("🔤 TXT").color(theme.accent_color()).monospace(),
            crate::tail_engine::ViewMode::Hex => RichText::new("🔢 HEX").color(theme.secondary_accent()).monospace().strong(),
        };
        if ui.button(mode_label).on_hover_text(t(lang, "tip_view_mode")).clicked() {
            engine.view_mode = match engine.view_mode {
                crate::tail_engine::ViewMode::Text => crate::tail_engine::ViewMode::Hex,
                crate::tail_engine::ViewMode::Hex => crate::tail_engine::ViewMode::Text,
            };
        }

        // Encoding selector (only relevant in Text mode)
        if engine.view_mode == crate::tail_engine::ViewMode::Text {
            let mut curr_enc = engine.encoding;
            egui::ComboBox::from_id_salt(format!("enc_sel_{}", engine.path.display()))
                .selected_text(RichText::new(curr_enc.name()).monospace().size(11.0))
                .width(90.0)
                .show_ui(ui, |ui| {
                    for enc in crate::tail_engine::FileEncoding::all() {
                        if ui.selectable_value(&mut curr_enc, *enc, enc.name()).clicked() {
                            engine.set_encoding(*enc);
                        }
                    }
                });
        }

        ui.separator();
        let stats_text = match engine.view_mode {
            crate::tail_engine::ViewMode::Text => format!(
                "{}: {} | {}: {:.2} MB",
                t(lang, "lines"),
                engine.total_lines(),
                t(lang, "file_size"),
                engine.file_size as f64 / (1024.0 * 1024.0)
            ),
            crate::tail_engine::ViewMode::Hex => format!(
                "HEX: {} | {}: {} | {}: {:.2} MB",
                engine.total_hex_rows(16),
                t(lang, "hex_bytes"),
                engine.file_size,
                t(lang, "file_size"),
                engine.file_size as f64 / (1024.0 * 1024.0)
            ),
        };
        ui.label(RichText::new(stats_text).monospace().color(theme.text_dim()));

        if engine.throughput_bps > 0.0 {
            ui.separator();
            ui.label(
                RichText::new(format!(
                    "{}: {:.1} KB/s",
                    t(lang, "throughput"),
                    engine.throughput_bps / 1024.0
                ))
                .monospace()
                .color(theme.secondary_accent()),
            );
        }

        ui.separator();
        ui.label(RichText::new("🔍").monospace());
        ui.add(egui::TextEdit::singleline(search_query).hint_text(t(lang, "search_placeholder")).desired_width(180.0));
        if !search_query.is_empty() && ui.button("✖").clicked() {
            search_query.clear();
        }
    });

    ui.separator();

    // If Hex streaming mode is active, render the binary hex stream
    if engine.view_mode == crate::tail_engine::ViewMode::Hex {
        render_hex_stream(ui, engine, theme, lang, search_query, font_size);
        return;
    }

    // Quick Regex Filter Row directly above log buffer
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("⚡ Includi (Regex):")
                .monospace()
                .size(11.0)
                .color(theme.accent_color()),
        );
        let mut inc = engine.include_filter.clone();
        if ui
            .add(
                egui::TextEdit::singleline(&mut inc)
                    .hint_text("Es. ERROR|CRITICAL|Exception...")
                    .desired_width(180.0),
            )
            .changed()
        {
            engine.set_include_filter(&inc);
        }
        if !engine.include_filter.is_empty() && ui.button("✖").clicked() {
            engine.set_include_filter("");
        }

        ui.separator();

        ui.label(
            RichText::new("🚫 Escludi (Regex):")
                .monospace()
                .size(11.0)
                .color(theme.warn_color()),
        );
        let mut exc = engine.exclude_filter.clone();
        if ui
            .add(
                egui::TextEdit::singleline(&mut exc)
                    .hint_text("Es. healthcheck|ping|DEBUG...")
                    .desired_width(180.0),
            )
            .changed()
        {
            engine.set_exclude_filter(&exc);
        }
        if !engine.exclude_filter.is_empty() && ui.button("✖").clicked() {
            engine.set_exclude_filter("");
        }
    });

    ui.separator();

    let total_lines = engine.total_lines();
    if total_lines == 0 {
        ui.centered_and_justified(|ui| {
            ui.label(RichText::new(t(lang, "no_file_open")).monospace().color(theme.text_dim()));
        });
        return;
    }

    let search_lower = search_query.to_lowercase();
    let has_search = !search_lower.is_empty();

    let row_height = (font_size * 1.45).max(16.0);
    let mut toggle_json = None;
    let scroll_area = ScrollArea::vertical()
        .auto_shrink([false, false])
        .stick_to_bottom(engine.follow_tail);

    scroll_area.show_rows(ui, row_height, total_lines, |ui, row_range| {
        for row_idx in row_range {
            if !engine.is_line_visible(row_idx) {
                continue;
            }
            if let Some(raw_line) = engine.get_line(row_idx) {
                let is_json = TailEngine::is_json_line(&raw_line);
                let is_expanded = engine.expanded_json_lines.contains(&row_idx);
                let highlight = engine.match_highlight(&raw_line);
                let matches_search = has_search && raw_line.to_lowercase().contains(&search_lower);

                ui.horizontal(|ui| {
                    // Line number
                    if *show_line_numbers {
                        ui.label(
                            RichText::new(format!("{:>6} │", row_idx + 1))
                                .monospace()
                                .size(font_size)
                                .color(theme.text_dim().gamma_multiply(0.6)),
                        );
                    }

                    // JSON toggle button
                    if is_json {
                        let btn_label = if is_expanded { "[-] JSON" } else { "[+] JSON" };
                        let btn = ui.button(
                            RichText::new(btn_label)
                                .monospace()
                                .size((font_size - 2.0).max(9.0))
                                .color(theme.secondary_accent()),
                        );
                        if btn.clicked() {
                            toggle_json = Some((row_idx, is_expanded));
                        }
                    }

                    // Content text
                    let mut text = RichText::new(&*raw_line).monospace().size(font_size);
                    if matches_search {
                        text = text.color(Color32::BLACK).background_color(Color32::from_rgb(255, 230, 0));
                    } else if let Some(hl) = highlight {
                        text = text.color(hl.fg).background_color(hl.bg);
                        if hl.bold {
                            text = text.strong();
                        }
                        if hl.italic {
                            text = text.italics();
                        }
                    } else {
                        text = text.color(theme.text_primary());
                    }
                    ui.label(text);
                });

                // Render expanded pretty JSON
                if is_json && is_expanded {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw_line) {
                        if let Ok(pretty) = serde_json::to_string_pretty(&val) {
                            egui::Frame::NONE
                                .fill(theme.panel_bg().linear_multiply(1.3))
                                .stroke(Stroke::new(1.0_f32, theme.border_color().gamma_multiply(0.4)))
                                .inner_margin(egui::Margin::same(6))
                                .show(ui, |ui| {
                                    for line in pretty.lines() {
                                        ui.label(
                                            RichText::new(format!("        {}", line))
                                                .monospace()
                                                .size(11.0)
                                                .color(theme.secondary_accent()),
                                        );
                                    }
                                });
                        }
                    }
                }
            }
        }
    });

    if let Some((idx, was_expanded)) = toggle_json {
        if was_expanded {
            engine.expanded_json_lines.remove(&idx);
        } else {
            engine.expanded_json_lines.insert(idx);
        }
    }
}

fn render_hex_stream(
    ui: &mut Ui,
    engine: &mut TailEngine,
    theme: &CyberTheme,
    lang: Language,
    search_query: &str,
    font_size: f32,
) {
    let file_size = engine.file_size as usize;
    if file_size == 0 {
        ui.centered_and_justified(|ui| {
            ui.label(RichText::new(t(lang, "no_file_open")).monospace().color(theme.text_dim()));
        });
        return;
    }

    // Hex column header
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("OFFSET    00 01 02 03 04 05 06 07  08 09 0A 0B 0C 0D 0E 0F   |ASCII           |")
                .monospace()
                .size(font_size)
                .color(theme.accent_color())
                .strong(),
        );
    });
    ui.separator();

    let bytes_per_row = 16;
    let total_rows = engine.total_hex_rows(bytes_per_row);
    let row_height = (font_size * 1.45).max(16.0);

    // Support text search or hex byte search
    let search_clean = search_query.trim().to_lowercase();
    let search_hex_bytes: Option<Vec<u8>> = if !search_clean.is_empty() {
        let no_spaces: String = search_clean
            .chars()
            .filter(|c| !c.is_whitespace() && *c != ':')
            .collect();
        if no_spaces.len() >= 2 && no_spaces.len() % 2 == 0 && no_spaces.chars().all(|c| c.is_ascii_hexdigit()) {
            (0..no_spaces.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&no_spaces[i..i + 2], 16).ok())
                .collect()
        } else {
            None
        }
    } else {
        None
    };

    let scroll_area = ScrollArea::vertical()
        .auto_shrink([false, false])
        .stick_to_bottom(engine.follow_tail);

    scroll_area.show_rows(ui, row_height, total_rows, |ui, row_range| {
        for row_idx in row_range {
            let offset = row_idx * bytes_per_row;
            let chunk = match engine.get_bytes(offset, bytes_per_row) {
                Some(b) => b,
                None => continue,
            };

            ui.horizontal(|ui| {
                // Offset label (8 uppercase hex digits)
                ui.label(
                    RichText::new(format!("{:08X}  ", offset))
                        .monospace()
                        .size(font_size)
                        .color(theme.text_dim().gamma_multiply(0.75)),
                );

                // Search matching
                let mut matches_search = false;
                if !search_clean.is_empty() {
                    let ascii_lossy = String::from_utf8_lossy(chunk).to_lowercase();
                    if ascii_lossy.contains(&search_clean) {
                        matches_search = true;
                    }
                    if let Some(ref target_bytes) = search_hex_bytes {
                        if !target_bytes.is_empty()
                            && chunk
                                .windows(target_bytes.len())
                                .any(|w| w == target_bytes.as_slice())
                        {
                            matches_search = true;
                        }
                    }
                }

                // Format 16 bytes: 8 bytes, space, 8 bytes
                let mut hex_str = String::with_capacity(50);
                for i in 0..16 {
                    if i == 8 {
                        hex_str.push(' ');
                    }
                    if i < chunk.len() {
                        use std::fmt::Write;
                        let _ = write!(&mut hex_str, "{:02X} ", chunk[i]);
                    } else {
                        hex_str.push_str("   ");
                    }
                }

                // Format ASCII representation
                let mut ascii_str = String::with_capacity(20);
                ascii_str.push('|');
                for &b in chunk {
                    if (0x20..=0x7E).contains(&b) {
                        ascii_str.push(b as char);
                    } else {
                        ascii_str.push('·');
                    }
                }
                for _ in chunk.len()..16 {
                    ascii_str.push(' ');
                }
                ascii_str.push('|');

                let mut hex_text = RichText::new(hex_str).monospace().size(font_size);
                let mut ascii_text = RichText::new(ascii_str).monospace().size(font_size);

                if matches_search {
                    hex_text = hex_text
                        .color(Color32::BLACK)
                        .background_color(Color32::from_rgb(255, 230, 0));
                    ascii_text = ascii_text
                        .color(Color32::BLACK)
                        .background_color(Color32::from_rgb(255, 230, 0));
                } else {
                    hex_text = hex_text.color(theme.text_primary());
                    ascii_text = ascii_text.color(theme.secondary_accent());
                }

                ui.label(hex_text);
                ui.label(ascii_text);
            });
        }
    });
}

pub fn render_filters_content(ui: &mut Ui, engines: &mut [TailEngine], theme: &CyberTheme, lang: Language) {
    let active_streams = engines.iter().filter(|e| !e.include_filter.is_empty() || !e.exclude_filter.is_empty()).count();

    ui.horizontal(|ui| {
        ui.heading(
            RichText::new(format!("🔍 {}", t(lang, "filters")))
                .monospace()
                .color(theme.accent_color()),
        );
        if active_streams > 0 {
            ui.label(
                RichText::new(format!("({} {})", active_streams, t(lang, "active_count")))
                    .monospace()
                    .color(theme.accent_color())
                    .strong(),
            );
        }
    });

    ui.label(
        RichText::new(t(lang, "visibility_filters_desc"))
            .monospace()
            .size(11.0)
            .color(theme.text_dim()),
    );

    ui.add_space(8.0);

    for engine in engines.iter_mut() {
        let file_name = engine
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Log")
            .to_string();

        ui.group(|ui| {
            ui.label(RichText::new(format!("📄 {}", file_name)).monospace().strong());

            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{}:", t(lang, "filter_include"))).monospace());
                let mut inc = engine.include_filter.clone();
                if ui.add(egui::TextEdit::singleline(&mut inc).hint_text("Es. ERROR|CRITICAL|Exception...")).changed() {
                    engine.set_include_filter(&inc);
                }
                if !engine.include_filter.is_empty() && ui.button("✖").clicked() {
                    engine.set_include_filter("");
                }
            });

            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{}:", t(lang, "filter_exclude"))).monospace());
                let mut exc = engine.exclude_filter.clone();
                if ui.add(egui::TextEdit::singleline(&mut exc).hint_text("Es. healthcheck|ping|DEBUG...")).changed() {
                    engine.set_exclude_filter(&exc);
                }
                if !engine.exclude_filter.is_empty() && ui.button("✖").clicked() {
                    engine.set_exclude_filter("");
                }
            });
        });
        ui.add_space(4.0);
    }
}

pub fn render_highlights_content(
    ui: &mut Ui,
    global_rules: &mut Vec<HighlightRule>,
    engines: &mut [TailEngine],
    theme: &CyberTheme,
    lang: Language,
) {
    let total_rules = global_rules.len();
    let active_rules = global_rules.iter().filter(|r| r.enabled && !r.pattern.is_empty()).count();

    ui.horizontal(|ui| {
        ui.heading(
            RichText::new(format!("⚡ {}", t(lang, "highlight_rules")))
                .monospace()
                .color(theme.warn_color()),
        );
        ui.label(
            RichText::new(format!("({}/{} {})", active_rules, total_rules, t(lang, "active_count")))
                .monospace()
                .color(theme.accent_color())
                .strong(),
        );
    });

    ui.label(
        RichText::new(t(lang, "color_filters_desc"))
            .monospace()
            .size(11.0)
            .color(theme.text_dim()),
    );

    ui.label(
        RichText::new(t(lang, "rules_order_hint"))
            .monospace()
            .size(11.0)
            .color(theme.secondary_accent()),
    );

    ui.add_space(8.0);

    let mut rules_changed = false;
    let mut to_remove = None;
    let mut to_move_up = None;
    let mut to_move_down = None;

    ScrollArea::vertical().show(ui, |ui| {
        let rules_len = global_rules.len();
        for (i, rule) in global_rules.iter_mut().enumerate() {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut rule.enabled, "").changed() {
                        rules_changed = true;
                    }

                    // Move Up / Move Down buttons for priority reordering
                    if i > 0 && ui.button("⬆").on_hover_text(t(lang, "move_up")).clicked() {
                        to_move_up = Some(i);
                    }
                    if i + 1 < rules_len && ui.button("⬇").on_hover_text(t(lang, "move_down")).clicked() {
                        to_move_down = Some(i);
                    }

                    ui.label(RichText::new(format!("#{}:", i + 1)).monospace());
                    if ui.add(egui::TextEdit::singleline(&mut rule.pattern).hint_text("Testo o Regex...")).changed() {
                        rules_changed = true;
                    }
                    if ui.checkbox(&mut rule.is_regex, "Regex").on_hover_text("Abilita interpretazione come Regular Expression (Regex)").changed() {
                        rules_changed = true;
                    }

                    // Bold and Italic toggles
                    if ui.checkbox(&mut rule.bold, "B").on_hover_text(t(lang, "bold")).changed() {
                        rules_changed = true;
                    }
                    if ui.checkbox(&mut rule.italic, "I").on_hover_text(t(lang, "italic")).changed() {
                        rules_changed = true;
                    }

                    ui.label(RichText::new("FG:").monospace().size(11.0));
                    if ui.color_edit_button_srgb(&mut rule.fg_color).changed() {
                        rules_changed = true;
                    }

                    ui.label(RichText::new("BG:").monospace().size(11.0));
                    if ui.color_edit_button_srgb(&mut rule.bg_color).changed() {
                        rules_changed = true;
                    }

                    // Sample preview
                    let fg = Color32::from_rgb(rule.fg_color[0], rule.fg_color[1], rule.fg_color[2]);
                    let bg = Color32::from_rgb(rule.bg_color[0], rule.bg_color[1], rule.bg_color[2]);
                    let mut preview = RichText::new(" PREVIEW ")
                        .color(fg)
                        .background_color(bg)
                        .monospace();
                    if rule.bold {
                        preview = preview.strong();
                    }
                    if rule.italic {
                        preview = preview.italics();
                    }
                    ui.label(preview);

                    if ui.button("🗑").clicked() {
                        to_remove = Some(i);
                        rules_changed = true;
                    }
                });
            });
        }
    });

    if let Some(i) = to_move_up {
        global_rules.swap(i, i - 1);
        rules_changed = true;
    }
    if let Some(i) = to_move_down {
        global_rules.swap(i, i + 1);
        rules_changed = true;
    }
    if let Some(idx) = to_remove {
        global_rules.remove(idx);
    }

    if ui.button(RichText::new(t(lang, "add_rule")).monospace()).on_hover_text("Aggiungi una nuova regola filtro").clicked() {
        global_rules.push(HighlightRule::new("", [255, 255, 255], [0, 100, 200], false));
        rules_changed = true;
    }

    if rules_changed {
        for engine in engines {
            engine.set_highlight_rules(global_rules.clone());
        }
    }
}

pub fn render_settings_content(
    ui: &mut Ui,
    theme: &mut CyberTheme,
    lang: &mut Language,
    screensaver_enabled: &mut bool,
    screensaver_timeout_mins: &mut u32,
    telemetry_enabled: &mut bool,
    sound_enabled: &mut bool,
    borderless: &mut bool,
    show_line_numbers: &mut bool,
    font_size: &mut f32,
) {
    ui.heading(RichText::new(format!("⚙ {}", t(*lang, "settings").to_uppercase())).monospace());
    ui.add_space(10.0);

    // Theme selector
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{}:", t(*lang, "theme"))).monospace());
        if ui.selectable_value(theme, CyberTheme::Tron, "Tron").clicked() {}
        if ui.selectable_value(theme, CyberTheme::Matrix, "Matrix").clicked() {}
        if ui.selectable_value(theme, CyberTheme::Blade, "Blade").clicked() {}
    });

    ui.add_space(6.0);

    // Language selector
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{}:", t(*lang, "language"))).monospace());
        if ui.selectable_value(lang, Language::En, "English").clicked() {}
        if ui.selectable_value(lang, Language::It, "Italiano").clicked() {}
        if ui.selectable_value(lang, Language::Fr, "Français").clicked() {}
        if ui.selectable_value(lang, Language::Es, "Español").clicked() {}
        if ui.selectable_value(lang, Language::Zh, "中文").clicked() {}
    });

    ui.add_space(6.0);

    // Font size selector
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{}:", t(*lang, "font_size"))).monospace());
        if ui.button(" - ").on_hover_text("Riduci font (Ctrl -)").clicked() {
            *font_size = (*font_size - 1.0).max(8.0);
        }
        ui.label(RichText::new(format!("{:.0} pt", *font_size)).monospace().strong());
        if ui.button(" + ").on_hover_text("Aumenta font (Ctrl +)").clicked() {
            *font_size = (*font_size + 1.0).min(32.0);
        }
        if ui.button("100%").on_hover_text("Reimposta font predefinito (13 pt)").clicked() {
            *font_size = 13.0;
        }
    });

    ui.add_space(6.0);
    ui.separator();
    ui.add_space(6.0);

    // Screensaver controls
    ui.checkbox(screensaver_enabled, t(*lang, "screensaver"));
    if *screensaver_enabled {
        ui.horizontal(|ui| {
            ui.label(t(*lang, "screensaver_timeout"));
            ui.add(egui::DragValue::new(screensaver_timeout_mins).range(1..=120));
        });
    }

    ui.add_space(6.0);
    ui.checkbox(telemetry_enabled, t(*lang, "telemetry"));
    ui.checkbox(sound_enabled, t(*lang, "sound_fx"));
    ui.checkbox(borderless, t(*lang, "borderless"));
    ui.checkbox(show_line_numbers, t(*lang, "show_lines"));
}
