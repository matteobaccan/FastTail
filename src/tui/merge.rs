// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The merged view dialog (`Alt+M`): the open streams with timestamps as check boxes,
//! `Enter` merging the checked ones by time (`crate::merged_stream`); on a merged view,
//! its sources, turned on and off as the window's source chips do.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use std::path::PathBuf;

/// One row: a stream (by path), its name and whether it is checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub path: PathBuf,
    pub name: String,
    pub checked: bool,
}

/// What a key asks of the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeKey {
    Moved,
    /// `Enter` on a new merge: the checked streams.
    Merge(Vec<PathBuf>),
    /// A source of the merged view turned on or off: its index and its state.
    Source(usize, bool),
    Close,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeDialog {
    pub items: Vec<Item>,
    pub selected: usize,
    /// The merged view whose sources are listed; `None`: a new merge.
    pub view: Option<PathBuf>,
}

impl MergeDialog {
    /// A new merge of `streams`, the ones in `checked` checked (the focused stream).
    pub fn new(streams: Vec<(PathBuf, String)>, checked: &[PathBuf]) -> Self {
        let items = streams
            .into_iter()
            .map(|(path, name)| Item {
                checked: checked.iter().any(|c| crate::paths::paths_equal(c, &path)),
                path,
                name,
            })
            .collect();
        Self {
            items,
            selected: 0,
            view: None,
        }
    }

    /// The sources of merged view `view`, each with whether it is on.
    pub fn sources(view: PathBuf, sources: Vec<(PathBuf, String, bool)>) -> Self {
        Self {
            items: sources
                .into_iter()
                .map(|(path, name, checked)| Item {
                    path,
                    name,
                    checked,
                })
                .collect(),
            selected: 0,
            view: Some(view),
        }
    }

    pub fn checked(&self) -> Vec<PathBuf> {
        self.items
            .iter()
            .filter(|i| i.checked)
            .map(|i| i.path.clone())
            .collect()
    }

    pub fn on_key(&mut self, key: KeyEvent) -> MergeKey {
        if key.kind == KeyEventKind::Release {
            return MergeKey::Other;
        }
        let last = self.items.len().saturating_sub(1);
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => MergeKey::Close,
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
                MergeKey::Moved
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected = (self.selected + 1).min(last);
                MergeKey::Moved
            }
            KeyCode::Char(' ') | KeyCode::Char('x') => {
                // A merged view keeps at least one source on.
                let on = self.items.iter().filter(|i| i.checked).count();
                let view = self.view.is_some();
                let Some(item) = self.items.get_mut(self.selected) else {
                    return MergeKey::Other;
                };
                if view && item.checked && on <= 1 {
                    return MergeKey::Other;
                }
                item.checked = !item.checked;
                match self.view {
                    Some(_) => MergeKey::Source(self.selected, item.checked),
                    None => MergeKey::Moved,
                }
            }
            KeyCode::Enter if self.view.is_some() => MergeKey::Close,
            KeyCode::Enter => MergeKey::Merge(self.checked()),
            _ => MergeKey::Other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn streams() -> Vec<(PathBuf, String)> {
        ["app.log", "db.log", "proxy.log"]
            .iter()
            .map(|n| (PathBuf::from(format!("/l/{n}")), n.to_string()))
            .collect()
    }

    #[test]
    fn checking_streams_and_merging_them() {
        let mut d = MergeDialog::new(streams(), &[PathBuf::from("/l/app.log")]);
        assert_eq!(d.checked(), vec![PathBuf::from("/l/app.log")]);
        d.on_key(key(KeyCode::Down));
        d.on_key(key(KeyCode::Char(' ')));
        assert_eq!(
            d.on_key(key(KeyCode::Enter)),
            MergeKey::Merge(vec![
                PathBuf::from("/l/app.log"),
                PathBuf::from("/l/db.log")
            ])
        );
        assert_eq!(d.on_key(key(KeyCode::Esc)), MergeKey::Close);
    }

    #[test]
    fn a_merged_view_keeps_one_source_on() {
        let sources = streams()
            .into_iter()
            .enumerate()
            .map(|(i, (p, n))| (p, n, i < 2))
            .collect();
        let mut d = MergeDialog::sources(PathBuf::from("/spool/merged"), sources);
        assert_eq!(
            d.on_key(key(KeyCode::Char(' '))),
            MergeKey::Source(0, false)
        );
        d.on_key(key(KeyCode::Down));
        assert_eq!(d.on_key(key(KeyCode::Char(' '))), MergeKey::Other);
        d.on_key(key(KeyCode::Down));
        assert_eq!(d.on_key(key(KeyCode::Char(' '))), MergeKey::Source(2, true));
        assert_eq!(d.on_key(key(KeyCode::Enter)), MergeKey::Close);
    }
}
