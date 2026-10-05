// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The highlight-rule editor of the terminal (`r`): the ordered rule list of
//! `fasttail.ini` (first match wins, as in the GUI) and a form for one rule. The list is
//! the dialog's state; the caller applies it to every stream and saves it after each
//! change, as the GUI's Highlights window does.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::audio::SoundAlertPreset;
use crate::external_tools::ExternalTool;
use crate::settings_model as model;
use crate::tail_engine::{HighlightRule, QuickLabel};
use crate::tui::form::{CheckBox, ColourField, FieldKey, RadioList, ReorderList, TextField};

/// Colours `[` / `]` step through in a colour field.
pub const SWATCHES: [[u8; 3]; 12] = [
    [255, 255, 255],
    [0, 0, 0],
    [255, 64, 64],
    [255, 160, 0],
    [255, 230, 0],
    [0, 220, 100],
    [0, 220, 255],
    [0, 100, 200],
    [190, 90, 255],
    [255, 80, 200],
    [128, 128, 128],
    [90, 0, 0],
];

/// A rule of the list and the external tools bound to it (by index in the tools list).
#[derive(Debug, Clone, PartialEq)]
pub struct RuleEntry {
    pub rule: HighlightRule,
    pub tools: Vec<usize>,
}

/// What a form field edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleKey {
    Pattern,
    Regex,
    CapturesOnly,
    MatchCase,
    Bold,
    Italic,
    Foreground,
    Background,
    Sound,
    AutoBookmark,
    Tool,
    Enabled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleWidget {
    Text(TextField),
    Check(CheckBox),
    Colour(ColourField),
    Radio(RadioList),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleField {
    pub key: RuleKey,
    pub label: &'static str,
    pub widget: RuleWidget,
}

/// The form of one rule: the list entry it edits (`None` for a new rule).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleForm {
    pub index: Option<usize>,
    pub fields: Vec<RuleField>,
    pub focus: usize,
    /// `[ OK ]` found a field that cannot be applied: problems are shown until edited.
    pub rejected: bool,
    /// The tool choice was changed (otherwise every tool bound to the rule stays).
    tool_edited: bool,
}

/// The editor: the rule list and, while one rule is edited, its form.
#[derive(Debug, Clone, PartialEq)]
pub struct RulesDialog {
    pub list: ReorderList<RuleEntry>,
    pub tool_names: Vec<String>,
    pub form: Option<RuleForm>,
    /// First list row shown when the list is taller than the dialog.
    pub top: usize,
    /// The quick labels, listed below the rules (memory only, set by the caller).
    pub labels: Vec<QuickLabel>,
    /// `Tab` moved the keyboard to the labels; `label_sel` is the selected one.
    pub on_labels: bool,
    pub label_sel: usize,
}

/// What a key did to the dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RulesKey {
    /// The rule list changed: the caller applies and saves it.
    Changed,
    /// Only the view changed (selection, focus, text being typed).
    Moved,
    /// A quick label was removed: the caller takes `labels`.
    Labels,
    /// The dialog closes.
    Close,
    /// The key does nothing here.
    Other,
}

fn check(on: bool) -> RuleWidget {
    RuleWidget::Check(CheckBox { on })
}

impl RuleForm {
    fn new(index: Option<usize>, entry: &RuleEntry, tool_names: &[String]) -> Self {
        let r = &entry.rule;
        let sounds: Vec<&str> = SoundAlertPreset::all().iter().map(|s| s.name()).collect();
        let sound = SoundAlertPreset::all()
            .iter()
            .position(|s| *s == r.sound_alert)
            .unwrap_or(0);
        let mut tools: Vec<&str> = vec!["none"];
        tools.extend(tool_names.iter().map(String::as_str));
        let tool = entry.tools.first().map_or(0, |t| t + 1);
        let field = |key, label, widget| RuleField { key, label, widget };
        let fields = vec![
            field(
                RuleKey::Pattern,
                "Pattern",
                RuleWidget::Text(TextField::new(&r.pattern)),
            ),
            field(RuleKey::Regex, "Regular expression", check(r.is_regex)),
            field(
                RuleKey::CapturesOnly,
                "Paint the groups only (regex)",
                check(r.captures_only),
            ),
            field(RuleKey::MatchCase, "Match case", check(r.case_sensitive)),
            field(RuleKey::Bold, "Bold", check(r.bold)),
            field(RuleKey::Italic, "Italic", check(r.italic)),
            field(
                RuleKey::Foreground,
                "Text colour",
                RuleWidget::Colour(ColourField::new(r.fg_color, SWATCHES.to_vec())),
            ),
            field(
                RuleKey::Background,
                "Background",
                RuleWidget::Colour(ColourField::new(r.bg_color, SWATCHES.to_vec())),
            ),
            field(
                RuleKey::Sound,
                "Sound",
                RuleWidget::Radio(RadioList::new(&sounds, sound)),
            ),
            field(
                RuleKey::AutoBookmark,
                "Bookmark matching lines",
                check(r.auto_bookmark),
            ),
            field(
                RuleKey::Tool,
                "Run tool",
                RuleWidget::Radio(RadioList::new(&tools, tool)),
            ),
            field(RuleKey::Enabled, "Enabled", check(r.enabled)),
        ];
        Self {
            index,
            fields,
            focus: 0,
            rejected: false,
            tool_edited: false,
        }
    }

    fn get(&self, key: RuleKey) -> Option<&RuleWidget> {
        self.fields.iter().find(|f| f.key == key).map(|f| &f.widget)
    }

    fn on(&self, key: RuleKey) -> bool {
        matches!(self.get(key), Some(RuleWidget::Check(c)) if c.on)
    }

    fn colour(&self, key: RuleKey) -> Option<[u8; 3]> {
        match self.get(key) {
            Some(RuleWidget::Colour(c)) => c.value(),
            _ => None,
        }
    }

    fn choice(&self, key: RuleKey) -> usize {
        match self.get(key) {
            Some(RuleWidget::Radio(r)) => r.selected,
            _ => 0,
        }
    }

    /// The rule as the form shows it; colours still being typed keep `fallback`'s.
    pub fn rule(&self, fallback: &HighlightRule) -> HighlightRule {
        let pattern = match self.get(RuleKey::Pattern) {
            Some(RuleWidget::Text(t)) => t.text().to_string(),
            _ => String::new(),
        };
        HighlightRule {
            pattern,
            is_regex: self.on(RuleKey::Regex),
            case_sensitive: self.on(RuleKey::MatchCase),
            fg_color: self
                .colour(RuleKey::Foreground)
                .unwrap_or(fallback.fg_color),
            bg_color: self
                .colour(RuleKey::Background)
                .unwrap_or(fallback.bg_color),
            bold: self.on(RuleKey::Bold),
            italic: self.on(RuleKey::Italic),
            sound_alert: SoundAlertPreset::all()
                .get(self.choice(RuleKey::Sound))
                .copied()
                .unwrap_or_default(),
            enabled: self.on(RuleKey::Enabled),
            captures_only: self.on(RuleKey::CapturesOnly),
            auto_bookmark: self.on(RuleKey::AutoBookmark),
        }
    }

    /// The fields that cannot be applied, with why.
    pub fn problems(&self) -> Vec<(usize, String)> {
        let mut out = Vec::new();
        for (i, f) in self.fields.iter().enumerate() {
            let problem = match (&f.key, &f.widget) {
                (RuleKey::Pattern, RuleWidget::Text(t)) if t.text().is_empty() => {
                    Some("type a word or a regular expression".to_string())
                }
                (RuleKey::Pattern, _) => {
                    model::rule_problem(&self.rule(&HighlightRule::new("", [0; 3], [0; 3], false)))
                }
                (_, RuleWidget::Colour(c)) if c.value().is_none() => {
                    Some("#RRGGBB, or [ ] for a swatch".to_string())
                }
                _ => None,
            };
            if let Some(p) = problem {
                out.push((i, p));
            }
        }
        out
    }

    /// Sends a key to the focused field; `Tab`, `↑` and `↓` move between fields.
    /// `Enter` and `Esc` come back to the dialog.
    fn on_key(&mut self, key: KeyEvent) -> FieldKey {
        match key.code {
            KeyCode::Tab | KeyCode::Down => return self.move_focus(1),
            KeyCode::BackTab | KeyCode::Up => return self.move_focus(-1),
            KeyCode::Enter => return FieldKey::Submit,
            KeyCode::Esc => return FieldKey::Cancel,
            _ => {}
        }
        let field = &mut self.fields[self.focus];
        let done = match &mut field.widget {
            RuleWidget::Text(t) => t.on_key(key),
            RuleWidget::Check(c) => c.on_key(key),
            RuleWidget::Colour(c) => c.on_key(key),
            RuleWidget::Radio(r) if matches!(key.code, KeyCode::Left | KeyCode::Right) => {
                r.on_key(key)
            }
            RuleWidget::Radio(_) => FieldKey::Other,
        };
        if done == FieldKey::Edited {
            self.rejected = false;
            if field.key == RuleKey::Tool {
                self.tool_edited = true;
            }
        }
        done
    }

    fn move_focus(&mut self, delta: isize) -> FieldKey {
        let n = self.fields.len() as isize;
        self.focus = (self.focus as isize + delta).rem_euclid(n) as usize;
        FieldKey::Edited
    }

    /// `label  value` per field, as drawn.
    pub fn lines(&self) -> Vec<String> {
        self.fields
            .iter()
            .map(|f| {
                let value = match &f.widget {
                    RuleWidget::Text(t) => t.text().to_string(),
                    RuleWidget::Check(c) => c.text(""),
                    RuleWidget::Colour(c) => format!("{}   [ ] swatch", c.field.text()),
                    RuleWidget::Radio(r) if r.text().chars().count() <= 44 => r.text(),
                    RuleWidget::Radio(r) => format!(
                        "< {} >",
                        r.options.get(r.selected).map(String::as_str).unwrap_or("")
                    ),
                };
                format!("{:<30}{value}", f.label)
            })
            .collect()
    }
}

impl RulesDialog {
    /// The editor over `rules`, each with the tools of `tools` bound to its pattern.
    pub fn new(rules: &[HighlightRule], tools: &[ExternalTool]) -> Self {
        let entries = rules
            .iter()
            .map(|r| RuleEntry {
                rule: r.clone(),
                tools: tools
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| t.bound_rule.as_deref() == Some(r.pattern.as_str()))
                    .map(|(i, _)| i)
                    .collect(),
            })
            .collect();
        Self {
            list: ReorderList::new(entries),
            tool_names: tools.iter().map(|t| t.name.clone()).collect(),
            form: None,
            top: 0,
            labels: Vec::new(),
            on_labels: false,
            label_sel: 0,
        }
    }

    pub fn rules(&self) -> Vec<HighlightRule> {
        self.list.items.iter().map(|e| e.rule.clone()).collect()
    }

    /// Binds `tools` to the patterns of the list: a tool claimed by a rule follows its
    /// pattern (a renamed rule keeps its tool), one that was bound to a rule of
    /// `before` and is claimed by none is unbound; any other binding stays as it was.
    pub fn bind_tools(&self, before: &[HighlightRule], tools: &mut [ExternalTool]) {
        for (i, tool) in tools.iter_mut().enumerate() {
            let claimed = self.list.items.iter().find(|e| e.tools.contains(&i));
            match claimed {
                Some(e) => tool.bound_rule = Some(e.rule.pattern.clone()),
                None => {
                    let was_ours = tool
                        .bound_rule
                        .as_deref()
                        .is_some_and(|b| before.iter().any(|r| r.pattern == b));
                    if was_ours {
                        tool.bound_rule = None;
                    }
                }
            }
        }
    }

    /// Opens the form of the selected rule.
    pub fn edit_selected(&mut self) {
        if let Some(entry) = self.list.selected_item() {
            self.form = Some(RuleForm::new(
                Some(self.list.selected),
                entry,
                &self.tool_names,
            ));
        }
    }

    /// Opens the form of a new rule (white on blue, as the GUI's Add).
    pub fn add(&mut self) {
        let entry = RuleEntry {
            rule: HighlightRule::new("", [255, 255, 255], [0, 100, 200], false),
            tools: Vec::new(),
        };
        self.form = Some(RuleForm::new(None, &entry, &self.tool_names));
    }

    /// `[ OK ]` of the form: the rule goes into the list (a new one after the selected
    /// rule) when every field is valid; otherwise the problems are marked.
    pub fn submit_form(&mut self) -> bool {
        let Some(form) = self.form.as_mut() else {
            return false;
        };
        if !form.problems().is_empty() {
            form.rejected = true;
            return false;
        }
        let fallback = form
            .index
            .and_then(|i| self.list.items.get(i))
            .map(|e| e.rule.clone())
            .unwrap_or_else(|| HighlightRule::new("", [255; 3], [0; 3], false));
        let rule = form.rule(&fallback);
        let tool = form.choice(RuleKey::Tool).checked_sub(1);
        let tool_edited = form.tool_edited;
        let index = form.index;
        self.form = None;
        let tools = |old: Vec<usize>| {
            if tool_edited {
                tool.into_iter().collect()
            } else {
                old
            }
        };
        match index.and_then(|i| self.list.items.get_mut(i)) {
            Some(entry) => {
                entry.rule = rule;
                entry.tools = tools(std::mem::take(&mut entry.tools));
            }
            None => self.list.insert(RuleEntry {
                rule,
                tools: tools(Vec::new()),
            }),
        }
        // A tool runs for one rule: the others give it up.
        if tool_edited {
            if let Some(t) = tool {
                let me = self.list.selected;
                for (i, e) in self.list.items.iter_mut().enumerate() {
                    if i != me {
                        e.tools.retain(|x| *x != t);
                    }
                }
            }
        }
        true
    }

    pub fn on_key(&mut self, key: KeyEvent) -> RulesKey {
        if key.kind == crossterm::event::KeyEventKind::Release {
            return RulesKey::Other;
        }
        if let Some(form) = self.form.as_mut() {
            return match form.on_key(key) {
                FieldKey::Submit if self.submit_form() => RulesKey::Changed,
                FieldKey::Submit => RulesKey::Moved,
                FieldKey::Cancel => {
                    self.form = None;
                    RulesKey::Moved
                }
                FieldKey::Edited => RulesKey::Moved,
                FieldKey::Other => RulesKey::Other,
            };
        }
        let plain = !key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        if matches!(key.code, KeyCode::Tab | KeyCode::BackTab) {
            self.on_labels = !self.on_labels && !self.labels.is_empty();
            return RulesKey::Moved;
        }
        if self.on_labels {
            return self.label_key(key);
        }
        match key.code {
            KeyCode::Esc => return RulesKey::Close,
            KeyCode::Enter | KeyCode::F(2) => {
                self.edit_selected();
                return RulesKey::Moved;
            }
            KeyCode::Char('e') if plain => {
                self.edit_selected();
                return RulesKey::Moved;
            }
            KeyCode::Insert | KeyCode::Char('a') | KeyCode::Char('n') if plain => {
                self.add();
                return RulesKey::Moved;
            }
            KeyCode::Char(' ') => {
                let i = self.list.selected;
                return match self.list.items.get_mut(i) {
                    Some(e) => {
                        e.rule.enabled = !e.rule.enabled;
                        RulesKey::Changed
                    }
                    None => RulesKey::Other,
                };
            }
            KeyCode::Char('d') if plain => {
                let delete = KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE);
                return self.list_key(delete);
            }
            _ => {}
        }
        self.list_key(key)
    }

    /// Keys of the quick labels: the arrows choose one, `d` / `Delete` removes it.
    fn label_key(&mut self, key: KeyEvent) -> RulesKey {
        let n = self.labels.len();
        match key.code {
            KeyCode::Esc => RulesKey::Close,
            KeyCode::Up | KeyCode::Char('k') => {
                self.label_sel = self.label_sel.saturating_sub(1);
                RulesKey::Moved
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.label_sel = (self.label_sel + 1).min(n.saturating_sub(1));
                RulesKey::Moved
            }
            KeyCode::Delete | KeyCode::Char('d') if self.label_sel < n => {
                self.labels.remove(self.label_sel);
                self.label_sel = self.label_sel.min(self.labels.len().saturating_sub(1));
                self.on_labels = !self.labels.is_empty();
                RulesKey::Labels
            }
            _ => RulesKey::Other,
        }
    }

    /// Keys of the list itself: a move or a deletion changes the rules (a move at
    /// the end of the list changes nothing), the arrows only the selection.
    fn list_key(&mut self, key: KeyEvent) -> RulesKey {
        let before = (self.list.selected, self.list.items.len());
        let reorders = matches!(key.code, KeyCode::Delete)
            || key.modifiers.contains(KeyModifiers::ALT)
            || matches!(key.code, KeyCode::Char('K') | KeyCode::Char('J'))
            || (key.modifiers.contains(KeyModifiers::SHIFT)
                && matches!(key.code, KeyCode::Char('k') | KeyCode::Char('j')));
        match self.list.on_key(key) {
            FieldKey::Edited
                if reorders && before != (self.list.selected, self.list.items.len()) =>
            {
                RulesKey::Changed
            }
            FieldKey::Edited => RulesKey::Moved,
            _ => RulesKey::Other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn rule(p: &str) -> HighlightRule {
        HighlightRule::new(p, [255, 0, 0], [0, 0, 0], false)
    }

    fn typed(d: &mut RulesDialog, s: &str) {
        for c in s.chars() {
            d.on_key(key(KeyCode::Char(c)));
        }
    }

    #[test]
    fn rules_are_added_edited_reordered_toggled_and_deleted() {
        let mut d = RulesDialog::new(&[rule("ERROR"), rule("WARN")], &[]);
        // Add: a new rule after the selected one, white on blue.
        assert_eq!(d.on_key(key(KeyCode::Char('a'))), RulesKey::Moved);
        typed(&mut d, "timeout");
        assert_eq!(d.on_key(key(KeyCode::Enter)), RulesKey::Changed);
        let patterns: Vec<String> = d.rules().iter().map(|r| r.pattern.clone()).collect();
        assert_eq!(patterns, ["ERROR", "timeout", "WARN"]);
        assert_eq!(d.rules()[1].bg_color, [0, 100, 200]);
        // Alt+Up moves it to the top: first match wins there.
        assert_eq!(
            d.on_key(KeyEvent::new(KeyCode::Up, KeyModifiers::ALT)),
            RulesKey::Changed
        );
        assert_eq!(d.rules()[0].pattern, "timeout");
        // Space switches it off, Enter edits it: bold and a typed colour.
        assert_eq!(d.on_key(key(KeyCode::Char(' '))), RulesKey::Changed);
        assert!(!d.rules()[0].enabled);
        d.on_key(key(KeyCode::Enter));
        let form = d.form.as_mut().unwrap();
        form.focus = form
            .fields
            .iter()
            .position(|f| f.key == RuleKey::Bold)
            .unwrap();
        d.on_key(key(KeyCode::Char(' ')));
        let form = d.form.as_mut().unwrap();
        form.focus = form
            .fields
            .iter()
            .position(|f| f.key == RuleKey::Foreground)
            .unwrap();
        for _ in 0..7 {
            d.on_key(key(KeyCode::Backspace));
        }
        typed(&mut d, "#00ff00");
        assert_eq!(d.on_key(key(KeyCode::Enter)), RulesKey::Changed);
        let r = &d.rules()[0];
        assert!(r.bold && !r.enabled);
        assert_eq!(r.fg_color, [0, 255, 0]);
        // `d` deletes; Up / Down only move the selection.
        assert_eq!(d.on_key(key(KeyCode::Down)), RulesKey::Moved);
        assert_eq!(d.on_key(key(KeyCode::Char('d'))), RulesKey::Changed);
        let patterns: Vec<String> = d.rules().iter().map(|r| r.pattern.clone()).collect();
        assert_eq!(patterns, ["timeout", "WARN"]);
        assert_eq!(d.on_key(key(KeyCode::Esc)), RulesKey::Close);
    }

    #[test]
    fn tab_reaches_the_quick_labels_and_d_removes_one() {
        let mut d = RulesDialog::new(&[rule("ERROR")], &[]);
        // Without labels Tab stays on the rules.
        d.on_key(key(KeyCode::Tab));
        assert!(!d.on_labels);
        d.labels = vec![
            QuickLabel {
                text: "payment".into(),
                color: 1,
            },
            QuickLabel {
                text: "timeout".into(),
                color: 4,
            },
        ];
        d.on_key(key(KeyCode::Tab));
        assert!(d.on_labels);
        d.on_key(key(KeyCode::Down));
        assert_eq!(d.on_key(key(KeyCode::Char('d'))), RulesKey::Labels);
        assert_eq!(d.labels.len(), 1);
        assert_eq!(d.labels[0].text, "payment");
        assert_eq!(d.rules().len(), 1, "the rules are untouched");
        assert_eq!(d.on_key(key(KeyCode::Delete)), RulesKey::Labels);
        assert!(d.labels.is_empty() && !d.on_labels, "back to the rules");
    }

    #[test]
    fn a_form_with_problems_is_not_applied() {
        let mut d = RulesDialog::new(&[rule("ERROR")], &[]);
        d.add();
        // An empty pattern.
        assert_eq!(d.on_key(key(KeyCode::Enter)), RulesKey::Moved);
        assert!(d.form.as_ref().unwrap().rejected);
        // A broken regular expression.
        typed(&mut d, "a(b");
        let form = d.form.as_mut().unwrap();
        if let RuleWidget::Check(c) = &mut form.fields[1].widget {
            c.on = true;
        }
        assert_eq!(form.problems().len(), 1);
        assert_eq!(d.on_key(key(KeyCode::Enter)), RulesKey::Moved);
        // Esc leaves the list as it was.
        assert_eq!(d.on_key(key(KeyCode::Esc)), RulesKey::Moved);
        assert!(d.form.is_none());
        assert_eq!(d.rules(), vec![rule("ERROR")]);
    }

    #[test]
    fn a_renamed_rule_keeps_its_tool_and_a_deleted_one_frees_it() {
        let tool = |name: &str, bound: Option<&str>| ExternalTool {
            bound_rule: bound.map(str::to_string),
            ..ExternalTool::new(name, "true", "")
        };
        let mut tools = vec![
            tool("pager", Some("ERROR")),
            tool("mail", None),
            tool("other", Some("gone")),
        ];
        let before = vec![rule("ERROR"), rule("WARN")];
        let mut d = RulesDialog::new(&before, &tools);
        assert_eq!(d.list.items[0].tools, vec![0]);
        // ERROR renamed to FATAL keeps "pager"; WARN gets "mail".
        d.edit_selected();
        for _ in 0..5 {
            d.on_key(key(KeyCode::Backspace));
        }
        typed(&mut d, "FATAL");
        assert_eq!(d.on_key(key(KeyCode::Enter)), RulesKey::Changed);
        d.on_key(key(KeyCode::Down));
        d.edit_selected();
        let form = d.form.as_mut().unwrap();
        form.focus = form
            .fields
            .iter()
            .position(|f| f.key == RuleKey::Tool)
            .unwrap();
        d.on_key(key(KeyCode::Right));
        d.on_key(key(KeyCode::Right));
        assert_eq!(d.on_key(key(KeyCode::Enter)), RulesKey::Changed);
        d.bind_tools(&before, &mut tools);
        assert_eq!(tools[0].bound_rule.as_deref(), Some("FATAL"));
        assert_eq!(tools[1].bound_rule.as_deref(), Some("WARN"));
        // A binding to a rule the list never had stays.
        assert_eq!(tools[2].bound_rule.as_deref(), Some("gone"));
        // Deleting FATAL frees "pager".
        let before = d.rules();
        d.on_key(key(KeyCode::Up));
        d.on_key(key(KeyCode::Delete));
        d.bind_tools(&before, &mut tools);
        assert_eq!(tools[0].bound_rule, None);
        assert_eq!(tools[1].bound_rule.as_deref(), Some("WARN"));
    }
}
