//! Command palette (CTRL + SHIFT + P): a floating box that lists every action of the
//! registry (`crate::actions`) and every boolean or enumerated setting, filtered by a
//! fuzzy match on the localized and the English name as the user types. The palette only
//! picks: the app runs what it returns (`PaletteOutcome`), window actions itself and
//! stream actions through the stream that was focused when the palette opened.

use crate::actions::{self, Action, ActionId, ActionState, EnumSetting, MAX_RECENT};
use crate::config::FastTailConfig;
use crate::i18n::{t, Language};
use eframe::egui;
use egui::{Color32, RichText, Stroke};
use std::path::PathBuf;

/// Opens and closes the palette.
pub const PALETTE_SHORTCUT: egui::KeyboardShortcut = egui::KeyboardShortcut::new(
    egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
    egui::Key::P,
);
pub const PALETTE_SHORTCUT_LABEL: &str = "CTRL + SHIFT + P";

/// Rows moved by PgUp / PgDown.
const PAGE_ROWS: usize = 10;
/// Extra score of a command run recently, so it wins over an equal match.
const RECENT_BONUS: i32 = 20;

const SCORE_MATCH: i32 = 16;
const BONUS_CONSECUTIVE: i32 = 12;
const BONUS_WORD_START: i32 = 10;
const BONUS_NAME_START: i32 = 14;
/// Penalty of a gap between two matched characters, and of each character it skips.
const PENALTY_GAP_START: i32 = 3;
const PENALTY_GAP: i32 = 1;

/// Folds a character for matching: lower case, and Latin letters without their accents
/// (`é` → `e`, `ß` → `s`, `ł` → `l`), so `numeros` finds `Números`. Other scripts (CJK,
/// Cyrillic) are compared as they are, after lower-casing.
pub fn fold_char(c: char) -> char {
    let lower = c.to_lowercase().next().unwrap_or(c);
    match lower {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => 'a',
        'ç' | 'ć' | 'č' | 'ĉ' | 'ċ' => 'c',
        'ď' | 'đ' => 'd',
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' | 'ě' => 'e',
        'ğ' | 'ĝ' | 'ġ' | 'ģ' => 'g',
        'ì' | 'í' | 'î' | 'ï' | 'ī' | 'į' | 'ı' => 'i',
        'ł' | 'ľ' | 'ĺ' | 'ļ' => 'l',
        'ñ' | 'ń' | 'ň' | 'ņ' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ő' => 'o',
        'ř' | 'ŕ' => 'r',
        'ś' | 'š' | 'ş' | 'ș' | 'ß' => 's',
        'ť' | 'ţ' | 'ț' => 't',
        'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' | 'ų' => 'u',
        'ý' | 'ÿ' => 'y',
        'ź' | 'ż' | 'ž' => 'z',
        other => other,
    }
}

/// Subsequence score of `query` in `candidate` (fzf style), `None` when some query
/// character is missing. Every matched character scores, more when it follows the
/// previous match, starts a word or starts the name; gaps cost a little. Whitespace in
/// the query is ignored, case and Latin accents are folded (`fold_char`).
pub fn fuzzy_score(query: &str, candidate: &str) -> Option<i32> {
    let q: Vec<char> = query
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(fold_char)
        .collect();
    if q.is_empty() {
        return Some(0);
    }
    let orig: Vec<char> = candidate.chars().collect();
    let c: Vec<char> = orig.iter().map(|&ch| fold_char(ch)).collect();
    let (m, n) = (q.len(), c.len());
    if m > n {
        return None;
    }
    let bonus: Vec<i32> = (0..n)
        .map(|j| {
            if j == 0 {
                return BONUS_NAME_START;
            }
            let (prev, ch) = (orig[j - 1], orig[j]);
            if !prev.is_alphanumeric() && ch.is_alphanumeric() {
                BONUS_WORD_START
            } else if prev.is_lowercase() && ch.is_uppercase() {
                BONUS_WORD_START / 2
            } else {
                0
            }
        })
        .collect();
    const NONE: i32 = i32::MIN / 4;
    // `prev[j]`: best score of the query so far with its last character matched at `j`.
    let mut prev = vec![NONE; n];
    for (i, &qc) in q.iter().enumerate() {
        let mut cur = vec![NONE; n];
        // Best `prev[k] + k * PENALTY_GAP` over k <= j - 2 (a gap of at least one char).
        let mut best_gap = NONE;
        for j in 0..n {
            if i > 0 && j >= 2 && prev[j - 2] > NONE {
                best_gap = best_gap.max(prev[j - 2] + (j as i32 - 2) * PENALTY_GAP);
            }
            if c[j] != qc {
                continue;
            }
            let here = SCORE_MATCH + bonus[j];
            if i == 0 {
                // A later first match costs a little: the start of the name wins.
                cur[j] = here - (j as i32).min(15);
                continue;
            }
            let consecutive = if j >= 1 && prev[j - 1] > NONE {
                prev[j - 1] + BONUS_CONSECUTIVE
            } else {
                NONE
            };
            let gapped = if best_gap > NONE {
                best_gap - (j as i32 - 1) * PENALTY_GAP - PENALTY_GAP_START
            } else {
                NONE
            };
            let best = consecutive.max(gapped);
            if best > NONE {
                cur[j] = best + here;
            }
        }
        prev = cur;
    }
    prev.into_iter().filter(|s| *s > NONE).max()
}

/// A candidate row for `rank`: its localized and English names and whether it was run
/// recently (its position in the recent list).
pub struct Candidate<'a> {
    pub name: &'a str,
    pub english: &'a str,
    pub recent: Option<usize>,
}

/// Indices of the candidates that match `query`, best first. With an empty query every
/// candidate is listed, the recent ones first (most recent first), then the others in
/// registry order.
pub fn rank(query: &str, candidates: &[Candidate]) -> Vec<usize> {
    if query.trim().is_empty() {
        let mut recent: Vec<(usize, usize)> = candidates
            .iter()
            .enumerate()
            .filter_map(|(i, c)| c.recent.map(|r| (r, i)))
            .collect();
        recent.sort();
        let mut order: Vec<usize> = recent.into_iter().map(|(_, i)| i).collect();
        order.extend((0..candidates.len()).filter(|i| candidates[*i].recent.is_none()));
        return order;
    }
    let mut scored: Vec<(i32, usize)> = candidates
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            let local = fuzzy_score(query, c.name);
            let english = fuzzy_score(query, c.english);
            let best = local.max(english)?;
            Some((best + c.recent.map_or(0, |_| RECENT_BONUS), i))
        })
        .collect();
    // Stable: equal scores keep the registry order.
    scored.sort_by_key(|s| std::cmp::Reverse(s.0));
    scored.into_iter().map(|(_, i)| i).collect()
}

/// One command row as the palette shows it.
#[derive(Debug, Clone)]
pub struct PaletteEntry {
    pub action: Action,
    pub name: String,
    pub english: String,
    /// Localized category name.
    pub category: &'static str,
    /// Why it cannot run now, localized; `None` when it can.
    pub disabled: Option<&'static str>,
    /// On / off state of a toggle, shown as a check box.
    pub checked: Option<bool>,
}

fn action_name(action: &Action, lang: Language) -> String {
    match action.id {
        ActionId::Toggle(_) => t(lang, "palette_toggle").replace("{name}", t(lang, action.name)),
        ActionId::Choose(_) => format!("{}…", t(lang, action.name)),
        _ => t(lang, action.name).to_string(),
    }
}

/// The command rows for the current configuration and state.
pub fn entries(cfg: &FastTailConfig, state: &ActionState) -> Vec<PaletteEntry> {
    let lang = cfg.language;
    actions::all()
        .into_iter()
        .map(|action| {
            let checked = match action.id {
                ActionId::Toggle(s) => Some((actions::bool_setting(s).get)(cfg)),
                ActionId::AlwaysOnTop => Some(cfg.always_on_top),
                ActionId::SearchPane => Some(cfg.search_pane),
                ActionId::GlobalFilterBar => Some(cfg.global_filter.bar_open),
                _ => None,
            };
            PaletteEntry {
                name: action_name(&action, lang),
                english: action_name(&action, Language::En),
                category: t(lang, action.category.name_key()),
                disabled: action.disabled_reason(state).map(|key| t(lang, key)),
                checked,
                action,
            }
        })
        .collect()
}

/// Modifiers and key of a shortcut label such as `CTRL + SHIFT + F` or `SPACE` (`CTRL`
/// is egui's command key, so the label also holds on macOS).
pub fn parse_label(label: &str) -> Option<egui::KeyboardShortcut> {
    let mut modifiers = egui::Modifiers::NONE;
    let mut key = None;
    for token in label.split(" + ").map(str::trim) {
        match token {
            "CTRL" => modifiers = modifiers.plus(egui::Modifiers::COMMAND),
            "SHIFT" => modifiers = modifiers.plus(egui::Modifiers::SHIFT),
            "ALT" => modifiers = modifiers.plus(egui::Modifiers::ALT),
            name => {
                let mut chars = name.chars();
                let first = chars.next()?;
                let proper: String = first
                    .to_uppercase()
                    .chain(chars.as_str().to_lowercase().chars())
                    .collect();
                key = Some(egui::Key::from_name(name).or_else(|| egui::Key::from_name(&proper))?);
            }
        }
    }
    Some(egui::KeyboardShortcut::new(modifiers, key?))
}

/// What the user picked, for the app to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteOutcome {
    Run(Action),
    /// Value `index` of `actions::enum_values(setting, ..)`.
    SetValue(EnumSetting, usize),
}

/// A row of either step, as drawn.
struct Row {
    label: String,
    english: String,
    category: &'static str,
    shortcut: Option<&'static str>,
    disabled: Option<&'static str>,
    checked: Option<bool>,
    current: bool,
    recent: Option<usize>,
    action: Option<Action>,
}

/// State of the palette between frames.
#[derive(Debug, Default)]
pub struct CommandPalette {
    open: bool,
    query: String,
    selected: usize,
    /// The value step of this setting is shown instead of the commands.
    values_of: Option<EnumSetting>,
    /// Stream focused when the palette opened: stream actions apply to it.
    target: Option<PathBuf>,
    scroll_to_selected: bool,
    /// Pass in which a click outside closed the palette: the same click on the title
    /// bar button must not open it again.
    closed_by_click: Option<u64>,
}

impl CommandPalette {
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// The title bar button: opens the palette on `target`, or closes it.
    pub fn toggle(&mut self, ctx: &egui::Context, target: Option<PathBuf>) {
        if self.open {
            self.close();
        } else if self.closed_by_click != Some(ctx.cumulative_pass_nr()) {
            self.open(target);
        }
        ctx.request_repaint();
    }

    pub fn open(&mut self, target: Option<PathBuf>) {
        *self = Self {
            open: true,
            target,
            ..Default::default()
        };
    }

    pub fn close(&mut self) {
        self.open = false;
        self.values_of = None;
    }

    /// The stream focused when the palette opened.
    pub fn target(&self) -> Option<&PathBuf> {
        self.target.as_ref()
    }

    fn input_id() -> egui::Id {
        egui::Id::new("command_palette_input")
    }

    /// Draws the palette when it is open and returns what was picked this frame. It takes
    /// the navigation keys before anything else sees them; the caller drops the other key
    /// events afterwards so that no stream reacts to them.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        cfg: &FastTailConfig,
        state: &ActionState,
    ) -> Option<PaletteOutcome> {
        if !self.open {
            return None;
        }
        let lang = cfg.language;
        let theme = cfg.theme;
        let none = egui::Modifiers::NONE;
        let back_allowed = self.values_of.is_some() && self.query.is_empty();
        let (esc, enter, up, down, page_up, page_down, back) = ctx.input_mut(|i| {
            (
                i.consume_key(none, egui::Key::Escape),
                i.consume_key(none, egui::Key::Enter),
                i.consume_key(none, egui::Key::ArrowUp),
                i.consume_key(none, egui::Key::ArrowDown),
                i.consume_key(none, egui::Key::PageUp),
                i.consume_key(none, egui::Key::PageDown),
                back_allowed && i.consume_key(none, egui::Key::Backspace),
            )
        });
        if esc {
            self.close();
            return None;
        }
        if back {
            self.values_of = None;
            self.selected = 0;
        }

        let rows = self.rows(cfg, state);
        let screen = ctx.content_rect();
        let width = (screen.width() - 32.0).clamp(240.0, 620.0);
        let pos = egui::pos2(screen.center().x - width / 2.0, screen.top() + 64.0);
        let mut clicked: Option<usize> = None;

        let area = egui::Area::new(egui::Id::new("command_palette"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .fill(theme.panel_bg())
                    .stroke(Stroke::new(1.5, theme.accent_color()))
                    .inner_margin(8)
                    .show(ui, |ui| {
                        ui.set_width(width);
                        if let Some(setting) = self.values_of {
                            let name = actions::ENUM_SETTINGS
                                .iter()
                                .find(|m| m.setting == setting)
                                .map_or("", |m| t(lang, m.name));
                            ui.label(
                                RichText::new(format!("‹ {name}"))
                                    .monospace()
                                    .strong()
                                    .color(theme.accent_color()),
                            );
                        }
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut self.query)
                                .hint_text(t(lang, "palette_hint"))
                                .desired_width(f32::INFINITY)
                                .font(egui::TextStyle::Monospace)
                                .id(Self::input_id()),
                        );
                        resp.request_focus();
                        if resp.changed() {
                            self.selected = 0;
                            self.scroll_to_selected = true;
                        }
                        ui.add_space(4.0);

                        let candidates: Vec<Candidate> = rows
                            .iter()
                            .map(|r| Candidate {
                                name: &r.label,
                                english: &r.english,
                                recent: r.recent,
                            })
                            .collect();
                        let order = rank(&self.query, &candidates);
                        self.move_selection(order.len(), up, down, page_up, page_down);

                        if order.is_empty() {
                            ui.label(
                                RichText::new(t(lang, "palette_no_match"))
                                    .monospace()
                                    .italics()
                                    .color(theme.text_dim()),
                            );
                        }
                        egui::ScrollArea::vertical()
                            .max_height(360.0)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                for (pos, &idx) in order.iter().enumerate() {
                                    let resp = draw_row(ui, &rows[idx], pos == self.selected, cfg);
                                    if pos == self.selected && self.scroll_to_selected {
                                        resp.scroll_to_me(None);
                                    }
                                    if resp.clicked() {
                                        self.selected = pos;
                                        clicked = Some(idx);
                                    }
                                }
                            });
                        self.scroll_to_selected = false;
                        if enter {
                            clicked = clicked.or(order.get(self.selected).copied());
                        }
                    });
            });

        // A click outside the palette closes it.
        let outside = ctx.input(|i| {
            i.pointer.any_click()
                && i.pointer
                    .interact_pos()
                    .is_some_and(|p| !area.response.rect.contains(p))
        });
        if outside {
            self.close();
            self.closed_by_click = Some(ctx.cumulative_pass_nr());
            return None;
        }

        let index = clicked?;
        let row = &rows[index];
        if row.disabled.is_some() {
            return None;
        }
        match (self.values_of, row.action) {
            (Some(setting), _) => {
                self.close();
                Some(PaletteOutcome::SetValue(setting, index))
            }
            (None, Some(action)) => {
                if let ActionId::Choose(setting) = action.id {
                    // The value step, with the current value selected.
                    self.values_of = Some(setting);
                    self.query.clear();
                    self.selected = actions::enum_values(setting, cfg)
                        .iter()
                        .position(|v| v.current)
                        .unwrap_or(0);
                    self.scroll_to_selected = true;
                    ctx.request_repaint();
                    return None;
                }
                self.close();
                Some(PaletteOutcome::Run(action))
            }
            (None, None) => None,
        }
    }

    fn move_selection(&mut self, len: usize, up: bool, down: bool, page_up: bool, page_down: bool) {
        if len == 0 {
            self.selected = 0;
            return;
        }
        let last = len - 1;
        let before = self.selected;
        if up {
            self.selected = self.selected.saturating_sub(1);
        }
        if down {
            self.selected = (self.selected + 1).min(last);
        }
        if page_up {
            self.selected = self.selected.saturating_sub(PAGE_ROWS);
        }
        if page_down {
            self.selected = (self.selected + PAGE_ROWS).min(last);
        }
        self.selected = self.selected.min(last);
        if self.selected != before || up || down || page_up || page_down {
            self.scroll_to_selected = true;
        }
    }

    fn rows(&self, cfg: &FastTailConfig, state: &ActionState) -> Vec<Row> {
        if let Some(setting) = self.values_of {
            return actions::enum_values(setting, cfg)
                .into_iter()
                .map(|v| Row {
                    label: v.label,
                    english: v.english,
                    category: "",
                    shortcut: None,
                    disabled: None,
                    checked: None,
                    current: v.current,
                    recent: None,
                    action: None,
                })
                .collect();
        }
        let recent = &cfg.palette_recent[..cfg.palette_recent.len().min(MAX_RECENT)];
        entries(cfg, state)
            .into_iter()
            .map(|e| Row {
                recent: recent.iter().position(|k| k == e.action.key),
                label: e.name,
                english: e.english,
                category: e.category,
                shortcut: e.action.shortcut,
                disabled: e.disabled,
                checked: e.checked,
                current: false,
                action: Some(e.action),
            })
            .collect()
    }
}

/// One row: a check box or current-value dot, the name (elided), and on the right the
/// reason it is greyed, the category and the shortcut.
fn draw_row(ui: &mut egui::Ui, row: &Row, selected: bool, cfg: &FastTailConfig) -> egui::Response {
    let theme = cfg.theme;
    let height = 22.0;
    let (rect, resp) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height),
        egui::Sense::click(),
    );
    let painter = ui.painter_at(rect);
    let accent = theme.accent_color();
    if selected {
        painter.rect_filled(rect, 3.0, accent.gamma_multiply(0.22));
        painter.rect_stroke(
            rect,
            3.0,
            Stroke::new(1.0, accent.gamma_multiply(0.6)),
            egui::StrokeKind::Inside,
        );
    } else if resp.hovered() {
        painter.rect_filled(rect, 3.0, accent.gamma_multiply(0.10));
    }
    let disabled = row.disabled.is_some();
    let name_color = if disabled {
        theme.text_dim().gamma_multiply(0.7)
    } else {
        theme.text_primary()
    };
    let small = egui::FontId::monospace(11.0);
    let mid_y = rect.center().y;

    // Right side, laid out from the right edge leftwards.
    let mut right = rect.right() - 6.0;
    let mut place = |text: &str, color: Color32, painter: &egui::Painter| {
        if text.is_empty() {
            return;
        }
        let galley = painter.layout_no_wrap(text.to_string(), small.clone(), color);
        right -= galley.size().x;
        painter.galley(
            egui::pos2(right, mid_y - galley.size().y / 2.0),
            galley,
            color,
        );
        right -= 12.0;
    };
    if let Some(shortcut) = row.shortcut {
        place(shortcut, accent, &painter);
    }
    place(row.category, theme.text_dim(), &painter);
    if let Some(reason) = row.disabled {
        place(reason, theme.warn_color().gamma_multiply(0.85), &painter);
    }

    let marker = match (row.checked, row.current) {
        (Some(true), _) => "☑ ",
        (Some(false), _) => "☐ ",
        (None, true) => "● ",
        (None, false) => "  ",
    };
    let left = rect.left() + 6.0;
    let mut job = egui::text::LayoutJob::simple_singleline(
        format!("{marker}{}", row.label),
        egui::FontId::monospace(12.5),
        name_color,
    );
    job.wrap = egui::text::TextWrapping {
        max_width: (right - left).max(40.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    let galley = painter.layout_job(job);
    painter.galley(
        egui::pos2(left, mid_y - galley.size().y / 2.0),
        galley,
        name_color,
    );
    match row.disabled {
        Some(reason) => resp.on_hover_text(reason),
        None => resp,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::{BoolSetting, Scope};

    fn english() -> FastTailConfig {
        FastTailConfig {
            language: Language::En,
            language_auto: false,
            ..Default::default()
        }
    }

    fn german() -> FastTailConfig {
        FastTailConfig {
            language: Language::De,
            language_auto: false,
            ..Default::default()
        }
    }

    fn ranked(cfg: &FastTailConfig, query: &str) -> Vec<PaletteEntry> {
        let entries = entries(cfg, &ActionState::default());
        let recent = &cfg.palette_recent;
        let candidates: Vec<Candidate> = entries
            .iter()
            .map(|e| Candidate {
                name: &e.name,
                english: &e.english,
                recent: recent.iter().position(|k| k == e.action.key),
            })
            .collect();
        rank(query, &candidates)
            .into_iter()
            .map(|i| entries[i].clone())
            .collect()
    }

    #[test]
    fn subsequence_with_case_and_accent_folding() {
        assert!(fuzzy_score("wrap", "Toggle line wrap").is_some());
        assert!(fuzzy_score("WRAP", "toggle line wrap").is_some());
        assert!(fuzzy_score("numeros", "Números de línea").is_some());
        assert!(fuzzy_score("zeilenumbruch", "Zeilenumbruch umschalten").is_some());
        assert!(fuzzy_score("xyz", "Toggle line wrap").is_none());
        assert!(fuzzy_score("wrapp", "wrap").is_none());
        assert_eq!(fuzzy_score("  ", "anything"), Some(0));
        // Spaces in the query are ignored.
        assert!(fuzzy_score("line wrap", "Toggle line wrap").is_some());
        // CJK is compared by characters.
        assert!(fuzzy_score("换行", "切换自动换行").is_some());
        assert!(fuzzy_score("自行", "切换自动换行").is_some());
        assert!(fuzzy_score("书签", "切换自动换行").is_none());
        assert_eq!(fold_char('É'), 'e');
        assert_eq!(fold_char('ł'), 'l');
        assert_eq!(fold_char('Ж'), 'ж');
    }

    #[test]
    fn consecutive_and_word_starts_rank_higher() {
        let run = fuzzy_score("wrap", "Toggle line wrap").unwrap();
        let scattered = fuzzy_score("wrap", "Show raw rows as plain").unwrap();
        assert!(run > scattered, "{run} <= {scattered}");
        let word_starts = fuzzy_score("tl", "Toggle line wrap").unwrap();
        let inside = fuzzy_score("tl", "Settle").unwrap();
        assert!(word_starts > inside, "{word_starts} <= {inside}");
        let at_start = fuzzy_score("exp", "Export visible lines").unwrap();
        let later = fuzzy_score("exp", "Clear and export").unwrap();
        assert!(at_start > later);
    }

    #[test]
    fn english_name_finds_a_german_command() {
        let cfg = german();
        let first = &ranked(&cfg, "wrap")[0];
        assert_eq!(first.action.id, ActionId::Wrap);
        assert_ne!(first.name, first.english, "listed under its German name");
        let first = &ranked(&cfg, "collapse")[0];
        assert_eq!(first.action.id, ActionId::Collapse);
        assert_eq!(first.name, t(Language::De, "act_collapse"));
    }

    #[test]
    fn search_all_streams_shows_its_shortcut() {
        let cfg = english();
        let first = &ranked(&cfg, "search all streams")[0];
        assert_eq!(first.action.id, ActionId::FindAll);
        assert_eq!(first.action.shortcut, Some("CTRL + SHIFT + F"));
    }

    #[test]
    fn recent_commands_come_first() {
        let mut cfg = english();
        let all = ranked(&cfg, "");
        assert_eq!(all.len(), actions::all().len(), "every command is listed");
        actions::push_recent(&mut cfg.palette_recent, "search.next");
        actions::push_recent(&mut cfg.palette_recent, "view.time_delta.toggle");
        let list = ranked(&cfg, "");
        assert_eq!(list[0].action.id, ActionId::TimeDelta);
        assert_eq!(list[1].action.id, ActionId::SearchNext);
        assert_eq!(list.len(), all.len());
        // Typing: a recent command wins an equal match.
        let delta = ranked(&cfg, "time delta");
        assert_eq!(delta[0].action.id, ActionId::TimeDelta);
    }

    #[test]
    fn entries_show_state_and_reasons() {
        let cfg = english();
        let list = entries(&cfg, &ActionState::default());
        let follow = list
            .iter()
            .find(|e| e.action.id == ActionId::Follow)
            .unwrap();
        assert_eq!(follow.disabled, Some(t(Language::En, "palette_no_stream")));
        let colors = list
            .iter()
            .find(|e| e.action.id == ActionId::Toggle(BoolSetting::LevelColors))
            .unwrap();
        assert_eq!(colors.checked, Some(cfg.level_colors));
        assert!(colors.name.contains(t(Language::En, "level_colors")));
        let theme = list
            .iter()
            .find(|e| e.action.id == ActionId::Choose(EnumSetting::Theme))
            .unwrap();
        assert!(theme.name.ends_with('…'));
    }

    #[test]
    fn labels_parse() {
        for a in actions::all() {
            if let Some(label) = a.shortcut {
                assert!(parse_label(label).is_some(), "{label}");
            }
        }
        assert_eq!(parse_label(PALETTE_SHORTCUT_LABEL), Some(PALETTE_SHORTCUT));
        assert_eq!(
            parse_label("SPACE"),
            Some(egui::KeyboardShortcut::new(
                egui::Modifiers::NONE,
                egui::Key::Space
            ))
        );
        assert!(parse_label("CTRL + NOPE").is_none());
    }

    /// The keys the handlers consume (`app.rs`, `dock.rs`, `find_results.rs`,
    /// `global_filter_bar.rs`, and egui's own zoom keys): a label must name the same key.
    #[test]
    fn shortcut_labels_match_the_handlers() {
        use egui::{Key, Modifiers as M};
        let handlers: &[(ActionId, M, Key)] = &[
            (ActionId::Follow, M::NONE, Key::Space),
            (ActionId::GoToLine, M::COMMAND, Key::G),
            (ActionId::SelectAll, M::COMMAND, Key::A),
            (ActionId::Copy, M::COMMAND, Key::C),
            (ActionId::Wrap, M::ALT, Key::W),
            (ActionId::Collapse, M::COMMAND.plus(M::SHIFT), Key::D),
            (ActionId::SearchFocus, M::CTRL, Key::F),
            (ActionId::SearchNext, M::NONE, Key::F3),
            (ActionId::SearchPrev, M::SHIFT, Key::F3),
            (ActionId::FindAll, M::CTRL.plus(M::SHIFT), Key::F),
            (ActionId::GlobalFilterBar, M::CTRL.plus(M::SHIFT), Key::H),
            (ActionId::ShowContext, M::COMMAND, Key::K),
            (ActionId::BookmarkToggle, M::COMMAND, Key::F2),
            (ActionId::BookmarkNext, M::NONE, Key::F2),
            (ActionId::BookmarkPrev, M::SHIFT, Key::F2),
            (ActionId::Help, M::NONE, Key::F1),
            (ActionId::AlwaysOnTop, M::COMMAND.plus(M::SHIFT), Key::T),
            (ActionId::LockNow, M::CTRL, Key::L),
            (ActionId::ZoomIn, M::COMMAND, Key::Plus),
            (ActionId::ZoomOut, M::COMMAND, Key::Minus),
            (ActionId::ZoomReset, M::COMMAND, Key::Num0),
        ];
        // CTRL and the command key are the same key on Windows and Linux.
        let norm = |m: M| (m.ctrl || m.command, m.shift, m.alt);
        for a in actions::all() {
            let Some(label) = a.shortcut else { continue };
            let parsed = parse_label(label).unwrap();
            let (_, mods, key) = handlers
                .iter()
                .find(|(id, _, _)| *id == a.id)
                .unwrap_or_else(|| panic!("{} has a shortcut but no handler entry here", a.key));
            assert_eq!(parsed.logical_key, *key, "{}", a.key);
            assert_eq!(norm(parsed.modifiers), norm(*mods), "{}", a.key);
        }
        for (id, _, _) in handlers {
            assert!(
                actions::get(*id).shortcut.is_some(),
                "{id:?} lost its label"
            );
        }
        // The palette's own key is free.
        for a in actions::all() {
            assert_ne!(a.shortcut, Some(PALETTE_SHORTCUT_LABEL));
        }
    }

    #[test]
    fn stream_rows_are_greyed_without_a_stream() {
        let cfg = english();
        for e in entries(&cfg, &ActionState::default()) {
            if e.action.scope == Scope::Stream {
                assert!(e.disabled.is_some(), "{}", e.action.key);
            }
        }
    }
}
