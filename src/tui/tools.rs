// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! External tools in the terminal: the `!` menu of the tools of `fasttail.ini` to run on
//! the cursor row or the selection, and the key events a tool's shortcut matches. The
//! commands are built by `crate::external_tools` exactly as in the GUI (null standard
//! input, output and error, so a tool never draws over the interface or reads its keys).

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::external_tools::{ExternalTool, KeyName, Mods, Shortcut};

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
}
