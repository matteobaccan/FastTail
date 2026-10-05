// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! External tools in the terminal: the `!` menu of the tools of `fasttail.ini` to run on
//! the cursor row or the selection, and the key events a tool's shortcut matches. The
//! commands are built by `crate::external_tools` exactly as in the GUI (null standard
//! input, output and error, so a tool never draws over the interface or reads its keys).

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::external_tools::{ExternalTool, KeyName, Mods, Shortcut, PLACEHOLDERS};
use crate::i18n::Language;
use crate::i18n_tui::{en, tx};
use crate::settings_model as model;
use crate::tui::form::{CheckBox, FieldKey, RadioList, ReorderList, TextField};

/// The shortcut `key` is, when it has a modifier and a key a shortcut can name.
pub fn shortcut_of(key: &KeyEvent) -> Option<Shortcut> {
    if key.kind == KeyEventKind::Release {
        return None;
    }
    let m = key.modifiers;
    let mods = Mods {
        ctrl: m.contains(KeyModifiers::CONTROL),
        shift: m.contains(KeyModifiers::SHIFT),
        alt: m.contains(KeyModifiers::ALT),
    };
    if !mods.any() {
        return None;
    }
    let name = match key.code {
        KeyCode::Char(' ') => "Space".to_string(),
        KeyCode::Char(c) => c.to_ascii_uppercase().to_string(),
        KeyCode::F(n) => format!("F{n}"),
        KeyCode::Up => "Up".into(),
        KeyCode::Down => "Down".into(),
        KeyCode::Left => "Left".into(),
        KeyCode::Right => "Right".into(),
        KeyCode::Enter => "Enter".into(),
        KeyCode::Tab | KeyCode::BackTab => "Tab".into(),
        KeyCode::Backspace => "Backspace".into(),
        KeyCode::Insert => "Insert".into(),
        KeyCode::Delete => "Delete".into(),
        KeyCode::Home => "Home".into(),
        KeyCode::End => "End".into(),
        KeyCode::PageUp => "PageUp".into(),
        KeyCode::PageDown => "PageDown".into(),
        KeyCode::Esc => "Escape".into(),
        _ => return None,
    };
    KeyName::from_name(&name).map(|key| Shortcut { mods, key })
}

/// The first tool whose shortcut is `key`.
pub fn tool_for_key(tools: &[ExternalTool], key: &KeyEvent) -> Option<usize> {
    let pressed = shortcut_of(key)?;
    tools
        .iter()
        .position(|t| t.parsed_shortcut() == Some(pressed))
}

/// The `!` menu: the tools, one selected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolMenu {
    pub selected: usize,
    pub count: usize,
}

/// What a key did to the menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuKey {
    Run(usize),
    /// `e`: the tools editor.
    Edit,
    Moved,
    Close,
    Other,
}

impl ToolMenu {
    pub fn new(count: usize) -> Self {
        Self { selected: 0, count }
    }

    pub fn on_key(&mut self, key: KeyEvent) -> MenuKey {
        if key.kind == KeyEventKind::Release {
            return MenuKey::Other;
        }
        let last = self.count.saturating_sub(1);
        match key.code {
            KeyCode::Esc => MenuKey::Close,
            KeyCode::Char('e') => MenuKey::Edit,
            KeyCode::Enter if self.count > 0 => MenuKey::Run(self.selected),
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
                MenuKey::Moved
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected = (self.selected + 1).min(last);
                MenuKey::Moved
            }
            // `1`..`9` run the tool of that number at once.
            KeyCode::Char(c @ '1'..='9') => {
                let i = c as usize - '1' as usize;
                if i < self.count {
                    MenuKey::Run(i)
                } else {
                    MenuKey::Other
                }
            }
            _ => MenuKey::Other,
        }
    }
}

/// What a field of the tool form edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKey {
    Name,
    Command,
    Args,
    Match,
    Shortcut,
    Shell,
    Rule,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolWidget {
    Text(TextField),
    Check(CheckBox),
    Radio(RadioList),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolField {
    pub key: ToolKey,
    pub label: &'static str,
    pub widget: ToolWidget,
}

/// The form of one tool: the list entry it edits (`None` for a new tool).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolForm {
    pub index: Option<usize>,
    pub fields: Vec<ToolField>,
    pub focus: usize,
    /// `[ OK ]` found a field that cannot be applied: problems are shown until edited.
    pub rejected: bool,
}

/// The tools editor: the list of `[tool.N]` and, while one tool is edited, its form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolsDialog {
    pub list: ReorderList<ExternalTool>,
    /// The patterns of the highlight rules, for the rule a tool runs for.
    pub rules: Vec<String>,
    pub form: Option<ToolForm>,
    pub top: usize,
}

/// What a key did to the editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolsKey {
    /// The tools changed: the caller stores and saves them.
    Changed,
    Moved,
    Close,
    Other,
}

impl ToolForm {
    fn new(index: Option<usize>, tool: &ExternalTool, rules: &[String]) -> Self {
        let text = |t: &str| ToolWidget::Text(TextField::new(t));
        let mut choices: Vec<&str> = vec!["none"];
        choices.extend(rules.iter().map(String::as_str));
        let rule = tool
            .bound_rule
            .as_deref()
            .and_then(|b| rules.iter().position(|r| r == b))
            .map_or(0, |i| i + 1);
        let field = |key, label, widget| ToolField { key, label, widget };
        Self {
            index,
            fields: vec![
                field(ToolKey::Name, en("Name"), text(&tool.name)),
                field(ToolKey::Command, en("Program"), text(&tool.program)),
                field(ToolKey::Args, en("Arguments"), text(&tool.args)),
                field(
                    ToolKey::Match,
                    en("Regex for {match}"),
                    text(tool.match_pattern.as_deref().unwrap_or("")),
                ),
                field(
                    ToolKey::Shortcut,
                    en("Shortcut (Ctrl+Shift+E)"),
                    text(tool.shortcut.as_deref().unwrap_or("")),
                ),
                field(
                    ToolKey::Shell,
                    en("Run through the shell"),
                    ToolWidget::Check(CheckBox { on: tool.use_shell }),
                ),
                field(
                    ToolKey::Rule,
                    en("Run when a rule matches"),
                    ToolWidget::Radio(RadioList::new(&choices, rule)),
                ),
            ],
            focus: 0,
            rejected: false,
        }
    }

    fn text(&self, key: ToolKey) -> &str {
        match self.fields.iter().find(|f| f.key == key).map(|f| &f.widget) {
            Some(ToolWidget::Text(t)) => t.text(),
            _ => "",
        }
    }

    /// The tool as the form shows it.
    pub fn tool(&self, rules: &[String]) -> ExternalTool {
        let optional = |key| {
            let t = self.text(key).trim();
            (!t.is_empty()).then(|| t.to_string())
        };
        let mut shell = false;
        let mut rule = 0;
        for f in &self.fields {
            match &f.widget {
                ToolWidget::Check(c) if f.key == ToolKey::Shell => shell = c.on,
                ToolWidget::Radio(r) if f.key == ToolKey::Rule => rule = r.selected,
                _ => {}
            }
        }
        ExternalTool {
            name: self.text(ToolKey::Name).trim().to_string(),
            program: self.text(ToolKey::Command).trim().to_string(),
            args: self.text(ToolKey::Args).to_string(),
            shortcut: optional(ToolKey::Shortcut),
            bound_rule: rule.checked_sub(1).and_then(|i| rules.get(i).cloned()),
            use_shell: shell,
            match_pattern: optional(ToolKey::Match),
        }
    }

    /// The fields that cannot be applied, with why.
    pub fn problems(&self, rules: &[String], lang: Language) -> Vec<(usize, String)> {
        let tool = self.tool(rules);
        let patterns: Vec<&str> = rules.iter().map(String::as_str).collect();
        let bad_shortcut =
            model::tool_problems(&tool, &patterns).contains(&model::ToolProblem::BadShortcut);
        let mut out = Vec::new();
        for (i, f) in self.fields.iter().enumerate() {
            let problem = match f.key {
                ToolKey::Name if tool.name.is_empty() => Some(tx(lang, "type a name").to_string()),
                ToolKey::Command if tool.program.is_empty() => {
                    Some(tx(lang, "type the program to run").to_string())
                }
                ToolKey::Shortcut if bad_shortcut => {
                    Some(tx(lang, "a modifier and a key: Ctrl+Shift+E, Alt+F9").to_string())
                }
                ToolKey::Match => tool
                    .match_pattern
                    .as_deref()
                    .and_then(|m| model::regex_error(m, true)),
                _ => None,
            };
            if let Some(p) = problem {
                out.push((i, p));
            }
        }
        out
    }

    fn on_key(&mut self, key: KeyEvent) -> FieldKey {
        match key.code {
            KeyCode::Tab | KeyCode::Down => return self.move_focus(1),
            KeyCode::BackTab | KeyCode::Up => return self.move_focus(-1),
            KeyCode::Enter => return FieldKey::Submit,
            KeyCode::Esc => return FieldKey::Cancel,
            _ => {}
        }
        let done = match &mut self.fields[self.focus].widget {
            ToolWidget::Text(t) => t.on_key(key),
            ToolWidget::Check(c) => c.on_key(key),
            ToolWidget::Radio(r) if matches!(key.code, KeyCode::Left | KeyCode::Right) => {
                r.on_key(key)
            }
            ToolWidget::Radio(_) => FieldKey::Other,
        };
        if done == FieldKey::Edited {
            self.rejected = false;
        }
        done
    }

    fn move_focus(&mut self, delta: isize) -> FieldKey {
        let n = self.fields.len() as isize;
        self.focus = (self.focus as isize + delta).rem_euclid(n) as usize;
        FieldKey::Edited
    }

    /// `label  value` per field, then the placeholders the arguments take.
    pub fn lines(&self, lang: Language) -> Vec<String> {
        let mut out: Vec<String> = self
            .fields
            .iter()
            .map(|f| {
                let value = match &f.widget {
                    ToolWidget::Text(t) => t.text().to_string(),
                    ToolWidget::Check(c) => c.text(""),
                    ToolWidget::Radio(r) if r.text().chars().count() <= 44 => r.text(),
                    ToolWidget::Radio(r) => format!(
                        "< {} >",
                        r.options.get(r.selected).map(String::as_str).unwrap_or("")
                    ),
                };
                format!("{}{value}", padded(tx(lang, f.label), 26))
            })
            .collect();
        out.push(format!(
            "{}{}",
            padded(tx(lang, "Placeholders"), 26),
            PLACEHOLDERS.join(" ")
        ));
        out
    }
}

fn padded(label: &str, width: usize) -> String {
    let pad = width.saturating_sub(unicode_width::UnicodeWidthStr::width(label));
    format!("{label}{}", " ".repeat(pad))
}

impl ToolsDialog {
    pub fn new(tools: &[ExternalTool], rules: &[crate::tail_engine::HighlightRule]) -> Self {
        Self {
            list: ReorderList::new(tools.to_vec()),
            rules: rules.iter().map(|r| r.pattern.clone()).collect(),
            form: None,
            top: 0,
        }
    }

    pub fn tools(&self) -> Vec<ExternalTool> {
        self.list.items.clone()
    }

    pub fn edit_selected(&mut self) {
        if let Some(t) = self.list.selected_item() {
            self.form = Some(ToolForm::new(Some(self.list.selected), t, &self.rules));
        }
    }

    pub fn add(&mut self) {
        self.form = Some(ToolForm::new(
            None,
            &ExternalTool::new("", "", "{file}"),
            &self.rules,
        ));
    }

    /// `[ OK ]` of the form: the tool goes into the list when every field is valid.
    pub fn submit_form(&mut self) -> bool {
        let Some(form) = self.form.as_mut() else {
            return false;
        };
        if !form.problems(&self.rules, Language::En).is_empty() {
            form.rejected = true;
            return false;
        }
        let tool = form.tool(&self.rules);
        let index = form.index;
        self.form = None;
        match index.and_then(|i| self.list.items.get_mut(i)) {
            Some(t) => *t = tool,
            None => self.list.insert(tool),
        }
        true
    }

    pub fn on_key(&mut self, key: KeyEvent) -> ToolsKey {
        if key.kind == KeyEventKind::Release {
            return ToolsKey::Other;
        }
        if let Some(form) = self.form.as_mut() {
            return match form.on_key(key) {
                FieldKey::Submit if self.submit_form() => ToolsKey::Changed,
                FieldKey::Submit | FieldKey::Edited => ToolsKey::Moved,
                FieldKey::Cancel => {
                    self.form = None;
                    ToolsKey::Moved
                }
                FieldKey::Other => ToolsKey::Other,
            };
        }
        let plain = !key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        match key.code {
            KeyCode::Esc => return ToolsKey::Close,
            KeyCode::Enter | KeyCode::F(2) => {
                self.edit_selected();
                return ToolsKey::Moved;
            }
            KeyCode::Char('e') if plain => {
                self.edit_selected();
                return ToolsKey::Moved;
            }
            KeyCode::Insert | KeyCode::Char('a') | KeyCode::Char('n') if plain => {
                self.add();
                return ToolsKey::Moved;
            }
            KeyCode::Char('d') if plain => {
                return self.list_key(KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE));
            }
            _ => {}
        }
        self.list_key(key)
    }

    fn list_key(&mut self, key: KeyEvent) -> ToolsKey {
        let before = (self.list.selected, self.list.items.len());
        let reorders = matches!(key.code, KeyCode::Delete)
            || key.modifiers.contains(KeyModifiers::ALT)
            || matches!(key.code, KeyCode::Char('K') | KeyCode::Char('J'));
        match self.list.on_key(key) {
            FieldKey::Edited
                if reorders && before != (self.list.selected, self.list.items.len()) =>
            {
                ToolsKey::Changed
            }
            FieldKey::Edited => ToolsKey::Moved,
            _ => ToolsKey::Other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(name: &str, shortcut: Option<&str>) -> ExternalTool {
        ExternalTool {
            shortcut: shortcut.map(str::to_string),
            ..ExternalTool::new(name, "true", "")
        }
    }

    #[test]
    fn terminal_keys_match_tool_shortcuts() {
        let tools = vec![
            tool("plain", None),
            tool("editor", Some("Ctrl+Shift+E")),
            tool("f9", Some("Alt+F9")),
            tool("comma", Some("Ctrl+,")),
        ];
        let k = |code, m| KeyEvent::new(code, m);
        let cs = KeyModifiers::CONTROL | KeyModifiers::SHIFT;
        assert_eq!(tool_for_key(&tools, &k(KeyCode::Char('E'), cs)), Some(1));
        assert_eq!(tool_for_key(&tools, &k(KeyCode::Char('e'), cs)), Some(1));
        assert_eq!(
            tool_for_key(&tools, &k(KeyCode::F(9), KeyModifiers::ALT)),
            Some(2)
        );
        assert_eq!(
            tool_for_key(&tools, &k(KeyCode::Char(','), KeyModifiers::CONTROL)),
            Some(3)
        );
        // No modifier, another modifier, or a release: no tool.
        assert_eq!(
            tool_for_key(&tools, &k(KeyCode::Char('e'), KeyModifiers::NONE)),
            None
        );
        assert_eq!(
            tool_for_key(&tools, &k(KeyCode::Char('e'), KeyModifiers::CONTROL)),
            None
        );
        let mut release = k(KeyCode::Char('E'), cs);
        release.kind = KeyEventKind::Release;
        assert_eq!(tool_for_key(&tools, &release), None);
    }

    #[test]
    fn the_menu_selects_and_runs() {
        let mut m = ToolMenu::new(3);
        let k = |code| KeyEvent::new(code, KeyModifiers::NONE);
        assert_eq!(m.on_key(k(KeyCode::Down)), MenuKey::Moved);
        assert_eq!(m.on_key(k(KeyCode::Enter)), MenuKey::Run(1));
        assert_eq!(m.on_key(k(KeyCode::Char('3'))), MenuKey::Run(2));
        assert_eq!(m.on_key(k(KeyCode::Char('4'))), MenuKey::Other);
        assert_eq!(m.on_key(k(KeyCode::Esc)), MenuKey::Close);
        assert_eq!(ToolMenu::new(0).on_key(k(KeyCode::Enter)), MenuKey::Other);
    }

    #[test]
    fn tools_are_added_edited_bound_and_checked() {
        use crate::tail_engine::HighlightRule;
        let k = |code| KeyEvent::new(code, KeyModifiers::NONE);
        let rules = [HighlightRule::new("FATAL", [255, 0, 0], [0; 3], false)];
        let mut d = ToolsDialog::new(&[tool("pager", None)], &rules);
        assert_eq!(d.on_key(k(KeyCode::Char('a'))), ToolsKey::Moved);
        // Empty name and program are refused.
        assert_eq!(d.on_key(k(KeyCode::Enter)), ToolsKey::Moved);
        assert!(d.form.as_ref().unwrap().rejected);
        for c in "code".chars() {
            d.on_key(k(KeyCode::Char(c)));
        }
        d.on_key(k(KeyCode::Down));
        for c in "code".chars() {
            d.on_key(k(KeyCode::Char(c)));
        }
        // A shortcut without a modifier is refused, with one it is kept.
        let form = d.form.as_mut().unwrap();
        form.focus = 4;
        for c in "E".chars() {
            d.on_key(k(KeyCode::Char(c)));
        }
        assert_eq!(d.on_key(k(KeyCode::Enter)), ToolsKey::Moved);
        d.on_key(k(KeyCode::Backspace));
        for c in "Ctrl+Shift+E".chars() {
            d.on_key(k(KeyCode::Char(c)));
        }
        // Bound to FATAL.
        d.form.as_mut().unwrap().focus = 6;
        d.on_key(k(KeyCode::Right));
        assert_eq!(d.on_key(k(KeyCode::Enter)), ToolsKey::Changed);
        let t = &d.tools()[1];
        assert_eq!((t.name.as_str(), t.program.as_str()), ("code", "code"));
        assert_eq!(t.args, "{file}");
        assert_eq!(t.shortcut.as_deref(), Some("Ctrl+Shift+E"));
        assert_eq!(t.bound_rule.as_deref(), Some("FATAL"));
        // A broken {match} regex is refused.
        d.edit_selected();
        d.form.as_mut().unwrap().focus = 3;
        for c in "a(b".chars() {
            d.on_key(k(KeyCode::Char(c)));
        }
        assert_eq!(d.on_key(k(KeyCode::Enter)), ToolsKey::Moved);
        d.on_key(k(KeyCode::Esc));
        // d deletes, Alt+Up moves.
        assert_eq!(
            d.on_key(KeyEvent::new(KeyCode::Up, KeyModifiers::ALT)),
            ToolsKey::Changed
        );
        assert_eq!(d.tools()[0].name, "code");
        assert_eq!(d.on_key(k(KeyCode::Char('d'))), ToolsKey::Changed);
        assert_eq!(d.tools().len(), 1);
        assert_eq!(d.on_key(k(KeyCode::Esc)), ToolsKey::Close);
    }
}
