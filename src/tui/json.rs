// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The JSON dialog (`J`): the tree of the cursor row's payload, as the GUI shows it under
//! an expanded `[+] JSON` row. A dialog rather than rows inside the window, so the
//! window's row arithmetic (cursor, selection, mouse rows) stays as it is.

use crate::json_tree::{Fold, Kind, Row, Tree};
use crossterm::event::{KeyCode, KeyEvent};
use std::collections::HashSet;

/// The dialog's state.
pub struct JsonDialog {
    /// The line shown (as numbered in the gutter).
    pub line_number: usize,
    pub tree: Tree,
    open: HashSet<usize>,
    /// The selected row.
    pub selected: usize,
}

/// What a key asks the app to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsonKey {
    Close,
    /// Copy this text: the value, or the path when `path` is true.
    Copy {
        text: String,
        path: bool,
    },
    /// "Expand all" stopped at the row cap.
    Cut,
    Handled,
}

/// How a row reads: indent, fold marker, key and value, and the kind of value.
pub struct RowText {
    pub prefix: String,
    pub key: Option<String>,
    pub value: String,
    pub kind: Option<Kind>,
}

impl JsonDialog {
    pub fn new(line_number: usize, tree: Tree) -> Self {
        let open = tree.default_open();
        Self {
            line_number,
            tree,
            open,
            selected: 0,
        }
    }

    pub fn rows(&self) -> Vec<Row> {
        self.tree.rows(&self.open)
    }

    /// The text of `row`.
    pub fn row_text(&self, row: Row) -> RowText {
        match row {
            Row::Node { id, depth } => {
                let node = self.tree.node(id);
                let marker = match (node.kind.is_container(), self.open.contains(&id)) {
                    (false, _) => "  ",
                    (true, true) => "v ",
                    (true, false) => "> ",
                };
                let value = match node.kind {
                    Kind::Object => format!("{{{} keys}}", node.children),
                    Kind::Array => format!("[{} items]", node.children),
                    _ => self.tree.display_value(id),
                };
                RowText {
                    prefix: format!("{}{marker}", "  ".repeat(depth)),
                    key: self.tree.key(id),
                    value,
                    kind: Some(node.kind),
                }
            }
            Row::More { depth, hidden } => RowText {
                prefix: "  ".repeat(depth + 1),
                key: None,
                value: format!("... {hidden} more"),
                kind: None,
            },
        }
    }

    fn selected_node(&self) -> Option<usize> {
        match self.rows().get(self.selected) {
            Some(Row::Node { id, .. }) => Some(*id),
            _ => None,
        }
    }

    /// Folds or unfolds the container on row `index` (a click), selecting it.
    pub fn toggle_row(&mut self, index: usize) {
        self.selected = index;
        if let Some(id) = self.selected_node() {
            self.tree.fold(&mut self.open, Fold::Toggle(id));
        }
        self.clamp();
    }

    fn clamp(&mut self) {
        self.selected = self.selected.min(self.rows().len().saturating_sub(1));
    }

    pub fn on_key(&mut self, key: KeyEvent) -> JsonKey {
        let count = self.rows().len();
        let node = self.selected_node();
        let container = node.is_some_and(|id| self.tree.node(id).kind.is_container());
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return JsonKey::Close,
            KeyCode::Up | KeyCode::Char('k') => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected = (self.selected + 1).min(count.saturating_sub(1))
            }
            KeyCode::PageUp => self.selected = self.selected.saturating_sub(10),
            KeyCode::PageDown => self.selected = (self.selected + 10).min(count.saturating_sub(1)),
            KeyCode::Home => self.selected = 0,
            KeyCode::End => self.selected = count.saturating_sub(1),
            KeyCode::Right | KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('l') => {
                if let (Some(id), true) = (node, container) {
                    if self.open.contains(&id) && key.code != KeyCode::Right {
                        self.open.remove(&id);
                    } else {
                        self.open.insert(id);
                    }
                }
            }
            KeyCode::Left | KeyCode::Char('h') => {
                if let Some(id) = node {
                    if container && self.open.contains(&id) {
                        self.open.remove(&id);
                    } else if let Some(parent) = self.tree.node(id).parent.filter(|&p| p != 0) {
                        // To the parent's row.
                        let at = self
                            .rows()
                            .iter()
                            .position(|r| matches!(r, Row::Node { id, .. } if *id == parent));
                        self.selected = at.unwrap_or(self.selected);
                    }
                }
            }
            KeyCode::Char('*') => {
                let target = node.filter(|_| container).unwrap_or(0);
                let cut = self.tree.fold(&mut self.open, Fold::ExpandAll(target));
                self.clamp();
                if cut {
                    return JsonKey::Cut;
                }
            }
            KeyCode::Char('-') => {
                let target = node.filter(|_| container).unwrap_or(0);
                self.tree.fold(&mut self.open, Fold::CollapseAll(target));
                if target == 0 {
                    self.selected = 0;
                }
                self.clamp();
            }
            KeyCode::Char('y') => {
                if let Some(id) = node {
                    return JsonKey::Copy {
                        text: self.tree.value_text(id),
                        path: false,
                    };
                }
            }
            KeyCode::Char('Y') => {
                if let Some(id) = node {
                    return JsonKey::Copy {
                        text: self.tree.path(id),
                        path: true,
                    };
                }
            }
            _ => {}
        }
        JsonKey::Handled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn keys_fold_move_and_copy() {
        let tree = Tree::of_line(r#"{"user":{"id":7,"tags":["a"]},"ok":true}"#).unwrap();
        let mut d = JsonDialog::new(12, tree);
        assert_eq!(d.rows().len(), 2);
        // Right unfolds `user`, Down reaches `id`.
        d.on_key(key(KeyCode::Right));
        assert_eq!(d.rows().len(), 4);
        d.on_key(key(KeyCode::Down));
        assert_eq!(d.row_text(d.rows()[1]).key.as_deref(), Some("id"));
        assert_eq!(
            d.on_key(key(KeyCode::Char('Y'))),
            JsonKey::Copy {
                text: "user.id".into(),
                path: true
            }
        );
        // Left on a leaf goes to its parent; Left again folds it.
        d.on_key(key(KeyCode::Left));
        assert_eq!(d.selected, 0);
        d.on_key(key(KeyCode::Left));
        assert_eq!(d.rows().len(), 2);
        // `*` opens everything, `-` folds everything back.
        d.on_key(key(KeyCode::Char('*')));
        assert_eq!(d.rows().len(), 5);
        d.on_key(key(KeyCode::Char('-')));
        assert_eq!(d.rows().len(), 2);
        assert_eq!(d.on_key(key(KeyCode::Esc)), JsonKey::Close);
    }
}
