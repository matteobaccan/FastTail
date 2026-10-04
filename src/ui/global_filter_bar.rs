// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The global filter bar (see `crate::global_filter`): shown under the menu bar with
//! `CTRL + SHIFT + H` or the `🌐` toolbar button, it edits the terms every stream combines
//! with its own filter. Hiding the bar does not switch the filter off.

use crate::global_filter::GlobalFilter;
use crate::i18n::{t, Language};
use crate::scan_job::MAX_FILTER_TERMS;
use crate::theme::CyberTheme;
use egui::{RichText, Ui};

/// What the bar changed this frame.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct BarOutput {
    /// A term was typed, added or removed: applied after `APPLY_DELAY_MS`.
    pub terms_edited: bool,
    /// The switch or a toggle changed: applied at once.
    pub switched: bool,
    /// The bar was closed.
    pub closed: bool,
}

/// `CTRL + SHIFT + H`, consumed by the app before the dock is drawn (egui would also match
/// a `CTRL + H` shortcut on it).
pub fn consume_shortcut(ctx: &egui::Context) -> bool {
    let shortcut =
        egui::KeyboardShortcut::new(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::H);
    ctx.input_mut(|i| i.consume_shortcut(&shortcut))
}

/// Id of the text box of term `index` on the include (`false`) or exclude (`true`) side.
pub fn term_id(exclude: bool, index: usize) -> egui::Id {
    egui::Id::new(("global_filter_term", exclude, index))
}

pub fn render(ui: &mut Ui, gf: &mut GlobalFilter, theme: &CyberTheme, lang: Language) -> BarOutput {
    let mut out = BarOutput::default();
    let (include_invalid, exclude_invalid) = invalid_flags(ui, gf);
    ui.horizontal_wrapped(|ui| {
        let title_color = if gf.is_applied() {
            theme.accent_color()
        } else {
            theme.text_dim()
        };
        ui.label(
            RichText::new(format!("🌐 {}", t(lang, "global_filter_title")))
                .monospace()
                .strong()
                .color(title_color),
        )
        .on_hover_text(t(lang, "global_filter_tip"));
        if ui
            .checkbox(&mut gf.enabled, t(lang, "global_filter_enabled"))
            .on_hover_text(t(lang, "global_filter_tip"))
            .changed()
        {
            out.switched = true;
        }
        if ui
            .selectable_label(gf.case_sensitive, RichText::new("Aa").monospace())
            .on_hover_text(t(lang, "case_sensitive_tip"))
            .clicked()
        {
            gf.case_sensitive = !gf.case_sensitive;
            out.switched = true;
        }
        if ui
            .selectable_label(gf.is_regex, RichText::new(".*").monospace())
            .on_hover_text(t(lang, "tip_regex_checkbox"))
            .clicked()
        {
            gf.is_regex = !gf.is_regex;
            out.switched = true;
        }
        ui.separator();
        for exclude in [false, true] {
            let (key, color) = if exclude {
                ("filter_terms_none_of", theme.warn_color())
            } else {
                ("filter_terms_all_of", theme.accent_color())
            };
            ui.label(
                RichText::new(format!("{}:", t(lang, key)))
                    .monospace()
                    .color(color),
            );
            let rows = if exclude {
                &mut gf.exclude
            } else {
                &mut gf.include
            };
            if rows.is_empty() {
                rows.push(String::new());
            }
            let mut remove = None;
            for (i, row) in rows.iter_mut().enumerate() {
                let invalid = if exclude {
                    &exclude_invalid
                } else {
                    &include_invalid
                }
                .get(i)
                .copied()
                .unwrap_or(false);
                let mut edit = egui::TextEdit::singleline(row)
                    .id(term_id(exclude, i))
                    .desired_width(140.0);
                if invalid {
                    edit = edit.text_color(theme.warn_color().into());
                }
                let resp = ui.add(edit);
                if resp.changed() {
                    out.terms_edited = true;
                }
                if invalid {
                    resp.on_hover_text(t(lang, "filter_term_invalid"));
                }
                if row.is_empty() && i == 0 {
                    continue;
                }
                if ui
                    .small_button("✖")
                    .on_hover_text(t(lang, "filter_remove_term"))
                    .clicked()
                {
                    remove = Some(i);
                }
            }
            if let Some(i) = remove {
                rows.remove(i);
                out.terms_edited = true;
            }
            if rows.len() < MAX_FILTER_TERMS
                && rows.last().is_some_and(|r| !r.is_empty())
                && ui
                    .small_button("+")
                    .on_hover_text(t(lang, "filter_add_term_tip"))
                    .clicked()
            {
                rows.push(String::new());
            }
            ui.separator();
        }
        if ui
            .small_button("✖")
            .on_hover_text(t(lang, "global_filter_close"))
            .clicked()
        {
            gf.bar_open = false;
            out.closed = true;
        }
    });
    ui.separator();
    out
}

/// Which include and exclude rows hold a regex that does not compile, recomputed only
/// when the terms or the toggles change (compiling up to 16 regexes every frame the bar
/// is shown would be wasted work). Plain-text terms are never invalid.
fn invalid_flags(ui: &Ui, gf: &GlobalFilter) -> (Vec<bool>, Vec<bool>) {
    use std::hash::{Hash, Hasher};
    if !gf.is_regex {
        return (Vec::new(), Vec::new());
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (&gf.include, &gf.exclude, gf.case_sensitive).hash(&mut hasher);
    let key = hasher.finish();
    let id = egui::Id::new("global_filter_bar_invalid");
    if let Some((cached, include, exclude)) =
        ui.data(|d| d.get_temp::<(u64, Vec<bool>, Vec<bool>)>(id))
    {
        if cached == key {
            return (include, exclude);
        }
    }
    let spec = gf.spec();
    let flags = |terms: &[crate::scan_job::FilterTerm]| -> Vec<bool> {
        terms.iter().map(|t| t.invalid).collect()
    };
    let (include, exclude) = (flags(&spec.include), flags(&spec.exclude));
    ui.data_mut(|d| d.insert_temp(id, (key, include.clone(), exclude.clone())));
    (include, exclude)
}
