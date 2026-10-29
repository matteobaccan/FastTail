// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Remappable keyboard shortcuts: the bindings of the actions of the registry
//! (`actions`), their defaults (the registry's shortcut labels), the user's overrides
//! (`[shortcuts]` in `fasttail.ini`, one key per action that differs from its default) and
//! the shortcuts of the external tools, checked for conflicts by scope.
//!
//! UI-free: the window turns the map into key dispatch (`ui::keys`). A default binding
//! keeps its handler; an action whose bindings were changed runs through the palette's
//! path when one of its new keys is pressed, and its old default keys stop firing.

use std::collections::{BTreeMap, HashMap};

use crate::actions::{self, ActionId, Scope};
use crate::external_tools::{ExternalTool, KeyName, Mods};

/// Bindings an action holds at most.
pub const MAX_BINDINGS: usize = 2;

/// A key with its modifiers. Unlike a tool shortcut it may have no modifier (`F3`,
/// `Space`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyBinding {
    pub mods: Mods,
    pub key: KeyName,
}

/// Keys that are never bound: text fields and dialogs depend on them.
const RESERVED: [&str; 4] = ["Escape", "Enter", "Tab", "Backspace"];

impl KeyBinding {
    /// Parses `Ctrl+Shift+F9`, `Alt+W`, `F3`, `Space`, `Shift+F2` (case-insensitive, `+`
    /// separated; `Cmd` is `Ctrl`), and the registry's labels (`CTRL + SHIFT + G`,
    /// `CTRL + +`).
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        let mut mods = Mods::default();
        let mut key: Option<KeyName> = None;
        // `CTRL + +` and `Ctrl++`: a trailing `+` after a separator is the key.
        let (body, plus) = match s.strip_suffix('+') {
            Some(rest) if rest.trim_end().ends_with('+') || rest.trim().is_empty() => {
                (rest.trim_end().trim_end_matches('+'), true)
            }
            _ => (s, false),
        };
        for part in body.split('+').map(str::trim).filter(|p| !p.is_empty()) {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" | "cmd" | "command" => mods.ctrl = true,
                "shift" => mods.shift = true,
                "alt" | "option" => mods.alt = true,
                name => {
                    if key.is_some() {
                        return None;
                    }
                    key = Some(key_named(name)?);
                }
            }
        }
        if plus {
            if key.is_some() {
                return None;
            }
            key = KeyName::from_name("Plus");
        }
        Some(KeyBinding { mods, key: key? })
    }

    /// The canonical form written to `fasttail.ini`: `Ctrl+Shift+F9`.
    pub fn to_text(self) -> String {
        let mut out = String::new();
        for (on, name) in [
            (self.mods.ctrl, "Ctrl"),
            (self.mods.alt, "Alt"),
            (self.mods.shift, "Shift"),
        ] {
            if on {
                out.push_str(name);
                out.push('+');
            }
        }
        out.push_str(self.key.name());
        out
    }

    /// As menus and the help write it: `CTRL + SHIFT + F9` (`⌘ ⌥ ⇧` on macOS).
    pub fn label(self) -> String {
        let mac = cfg!(target_os = "macos");
        let mut parts: Vec<&str> = Vec::new();
        if self.mods.ctrl {
            parts.push(if mac { "⌘" } else { "CTRL" });
        }
        if self.mods.alt {
            parts.push(if mac { "⌥" } else { "ALT" });
        }
        if self.mods.shift {
            parts.push(if mac { "⇧" } else { "SHIFT" });
        }
        let key = match self.key.name() {
            "Plus" => "+".to_string(),
            "Minus" => "-".to_string(),
            name => name.to_uppercase(),
        };
        let mut out = parts.join(" + ");
        if !out.is_empty() {
            out.push_str(" + ");
        }
        out.push_str(&key);
        out
    }

    /// Whether the key may not be bound (`Esc`, `Enter`, `Tab`, `Backspace`).
    pub fn reserved(self) -> bool {
        RESERVED.contains(&self.key.name())
    }

    /// Whether it has no modifier but `Shift`: typed text in a field would trigger it.
    pub fn plain(self) -> bool {
        !self.mods.ctrl && !self.mods.alt
    }
}

/// The key named `name` (lower case): one character, `f9`, `space`, `pagedown`.
fn key_named(name: &str) -> Option<KeyName> {
    let symbol = match name {
        "-" => Some("Minus"),
        "+" => Some("Plus"),
        "=" => Some("Equals"),
        "," => Some("Comma"),
        "." => Some("Period"),
        "/" => Some("Slash"),
        ";" => Some("Semicolon"),
        "[" => Some("OpenBracket"),
        "]" => Some("CloseBracket"),
        _ => None,
    };
    if let Some(symbol) = symbol {
        return KeyName::from_name(symbol);
    }
    let candidate = if name.chars().count() == 1 {
        name.to_ascii_uppercase()
    } else {
        let mut chars = name.chars();
        let first = chars.next()?.to_ascii_uppercase();
        format!("{first}{}", chars.as_str())
    };
    KeyName::from_name(&candidate)
}

/// Parses a list of bindings separated by `,` (`F3, Ctrl+N`); an empty text is no
/// binding. `None` when an entry does not parse.
pub fn parse_list(s: &str) -> Option<Vec<KeyBinding>> {
    s.split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(KeyBinding::parse)
        .collect()
}

/// Bindings as `fasttail.ini` stores them: `F3, Ctrl+N`, empty for none.
pub fn list_text(bindings: &[KeyBinding]) -> String {
    bindings
        .iter()
        .map(|b| b.to_text())
        .collect::<Vec<_>>()
        .join(", ")
}

/// The default bindings of `id`: its registry label, if any.
pub fn defaults(id: ActionId) -> Vec<KeyBinding> {
    actions::get(id)
        .shortcut
        .and_then(KeyBinding::parse)
        .into_iter()
        .collect()
}

/// Whether two scopes see the same keys: the window sees every key; a stream and a list
/// are active in different places.
pub fn scopes_overlap(a: Scope, b: Scope) -> bool {
    a == b || a == Scope::Window || b == Scope::Window
}

/// The bindings of every action, with what changed from the defaults.
#[derive(Debug, Clone, Default)]
pub struct KeyMap {
    /// The bindings of each action that has at least one (defaults or overrides).
    bindings: HashMap<ActionId, Vec<KeyBinding>>,
    /// Actions whose bindings differ from their defaults: their keys dispatch through
    /// the palette's path (`changed_lookup`).
    changed: HashMap<KeyBinding, ActionId>,
    /// Default keys of changed actions that no action holds now: they must not fire.
    retired: Vec<KeyBinding>,
    /// External tools whose shortcut is a built-in binding (the built-in wins).
    pub tool_conflicts: Vec<usize>,
    /// What loading found wrong: unknown ids, bad values, keys taken twice.
    pub notices: Vec<String>,
}

impl KeyMap {
    /// The defaults, then `overrides` (`[shortcuts]`: action key → bindings text), then
    /// the tools' shortcuts. Overrides apply in table order; a key already taken in an
    /// overlapping scope is dropped from the later action and reported.
    pub fn build(overrides: &BTreeMap<String, String>, tools: &[ExternalTool]) -> Self {
        let mut map = KeyMap::default();
        let all = actions::all();
        let by_key: HashMap<&str, &actions::Action> = all.iter().map(|a| (a.key, a)).collect();
        for (key, value) in overrides {
            // The terminal's own keys (`tui.follow`) are read by `fasttail-tui`.
            if key.starts_with("tui.") {
                continue;
            }
            if !by_key.contains_key(key.as_str()) {
                map.notices
                    .push(format!("[shortcuts] {key}: unknown action"));
            } else if parse_list(value).is_none() {
                map.notices
                    .push(format!("[shortcuts] {key}={value}: not a key"));
            }
        }
        // Wanted bindings: the override when it parses, else the default. The overrides
        // are placed first (in table order), so a key the user gave an action wins over
        // the default of another, which loses it with a notice.
        let wanted: Vec<(&actions::Action, Vec<KeyBinding>, bool)> = all
            .iter()
            .map(|a| match overrides.get(a.key).and_then(|v| parse_list(v)) {
                Some(list) => (a, list, true),
                None => (a, defaults(a.id), false),
            })
            .collect();
        let mut taken: Vec<(KeyBinding, Scope, ActionId)> = Vec::new();
        let mut changed_ids: Vec<ActionId> = Vec::new();
        for pass_overrides in [true, false] {
            for (a, list, overridden) in &wanted {
                if *overridden != pass_overrides {
                    continue;
                }
                if *overridden && *list != defaults(a.id) {
                    changed_ids.push(a.id);
                }
                let mut kept = Vec::new();
                for b in list.iter().copied().take(MAX_BINDINGS) {
                    if b.reserved() {
                        map.notices
                            .push(format!("{}: {} is reserved", a.key, b.to_text()));
                        continue;
                    }
                    if let Some((_, _, other)) = taken
                        .iter()
                        .find(|(k, s, _)| *k == b && scopes_overlap(*s, a.scope))
                    {
                        map.notices.push(format!(
                            "{}: {} is already {}",
                            a.key,
                            b.to_text(),
                            actions::get(*other).key
                        ));
                        if !changed_ids.contains(&a.id) {
                            changed_ids.push(a.id);
                        }
                        continue;
                    }
                    taken.push((b, a.scope, a.id));
                    kept.push(b);
                }
                if !kept.is_empty() {
                    map.bindings.insert(a.id, kept);
                }
            }
        }
        for id in &changed_ids {
            for b in map.bindings.get(id).into_iter().flatten() {
                map.changed.insert(*b, *id);
            }
        }
        // A default key of a changed action fires nothing unless an action holds it now.
        for id in &changed_ids {
            for b in defaults(*id) {
                if !taken.iter().any(|(k, _, _)| *k == b) {
                    map.retired.push(b);
                }
            }
        }
        for (n, tool) in tools.iter().enumerate() {
            let Some(sc) = tool.parsed_shortcut() else {
                continue;
            };
            let b = KeyBinding {
                mods: sc.mods,
                key: sc.key,
            };
            if taken.iter().any(|(k, _, _)| *k == b) {
                map.tool_conflicts.push(n);
            }
        }
        map
    }

    /// The bindings of `id`.
    pub fn bindings(&self, id: ActionId) -> &[KeyBinding] {
        self.bindings.get(&id).map_or(&[], Vec::as_slice)
    }

    /// The label of the first binding of `id` (`CTRL + F`), if any.
    pub fn label(&self, id: ActionId) -> Option<String> {
        self.bindings(id).first().map(|b| b.label())
    }

    /// The action a changed binding runs (`None` for a default binding: its handler runs).
    pub fn changed_action(&self, b: KeyBinding) -> Option<ActionId> {
        self.changed.get(&b).copied()
    }

    /// Whether `b` is a default key whose action moved away from it.
    pub fn is_retired(&self, b: KeyBinding) -> bool {
        self.retired.contains(&b)
    }

    /// Whether any binding differs from the defaults (dispatch has work to do).
    pub fn has_changes(&self) -> bool {
        !self.changed.is_empty() || !self.retired.is_empty()
    }

    /// The action holding `b` in a scope overlapping `scope`, other than `except`.
    pub fn holder(&self, b: KeyBinding, scope: Scope, except: ActionId) -> Option<ActionId> {
        self.bindings
            .iter()
            .filter(|(id, _)| **id != except)
            .find(|(id, list)| list.contains(&b) && scopes_overlap(actions::get(**id).scope, scope))
            .map(|(id, _)| *id)
    }
}

/// `overrides` with action `id` given `bindings`: an entry only when they differ from the
/// defaults (an empty value for none).
pub fn set_override(
    overrides: &mut BTreeMap<String, String>,
    id: ActionId,
    bindings: &[KeyBinding],
) {
    let key = actions::get(id).key.to_string();
    if bindings == defaults(id).as_slice() {
        overrides.remove(&key);
    } else {
        overrides.insert(key, list_text(bindings));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(s: &str) -> KeyBinding {
        KeyBinding::parse(s).unwrap_or_else(|| panic!("{s}"))
    }

    #[test]
    fn bindings_parse_and_write_back_canonically() {
        for (input, canonical) in [
            ("ctrl+shift+f9", "Ctrl+Shift+F9"),
            ("Cmd+B", "Ctrl+B"),
            ("F3", "F3"),
            ("space", "Space"),
            ("Shift + F2", "Shift+F2"),
            ("CTRL + SHIFT + G", "Ctrl+Shift+G"),
            ("CTRL + +", "Ctrl+Plus"),
            ("CTRL + -", "Ctrl+Minus"),
            ("alt+w", "Alt+W"),
        ] {
            let parsed = b(input);
            assert_eq!(parsed.to_text(), canonical, "{input}");
            assert_eq!(b(&parsed.to_text()), parsed);
        }
        assert!(KeyBinding::parse("Ctrl+Nope").is_none());
        assert!(KeyBinding::parse("Ctrl+A+B").is_none());
        assert_eq!(
            parse_list("F3, Ctrl+N").unwrap(),
            vec![b("F3"), b("Ctrl+N")]
        );
        assert_eq!(parse_list("").unwrap(), vec![]);
        assert!(parse_list("F3, ???").is_none());
        assert_eq!(list_text(&[b("F3"), b("ctrl+n")]), "F3, Ctrl+N");
        assert!(b("Escape").reserved());
        assert!(b("Shift+F2").plain() && !b("Ctrl+F2").plain());
    }

    #[test]
    fn every_registry_label_is_a_default_binding() {
        for a in actions::all() {
            if let Some(label) = a.shortcut {
                assert!(KeyBinding::parse(label).is_some(), "{} {label}", a.key);
            }
        }
        let map = KeyMap::build(&BTreeMap::new(), &[]);
        assert!(
            !map.has_changes() && map.notices.is_empty(),
            "{:?}",
            map.notices
        );
        assert_eq!(map.bindings(ActionId::BookmarkToggle), [b("Ctrl+F2")]);
        assert_eq!(
            map.label(ActionId::FindAll).as_deref(),
            Some(b("Ctrl+Shift+F").label().as_str())
        );
    }

    #[test]
    fn a_rebound_action_runs_on_its_new_key_and_not_the_old_one() {
        let mut o = BTreeMap::new();
        set_override(&mut o, ActionId::BookmarkToggle, &[b("Ctrl+B")]);
        assert_eq!(o["bookmark.toggle"], "Ctrl+B");
        let map = KeyMap::build(&o, &[]);
        assert_eq!(
            map.changed_action(b("Ctrl+B")),
            Some(ActionId::BookmarkToggle)
        );
        assert!(map.is_retired(b("Ctrl+F2")));
        assert_eq!(map.changed_action(b("Ctrl+F2")), None);
        // Back to the default: no entry.
        set_override(&mut o, ActionId::BookmarkToggle, &[b("Ctrl+F2")]);
        assert!(o.is_empty());
        // Unbound: an empty value, and the old key retired.
        set_override(&mut o, ActionId::Wrap, &[]);
        assert_eq!(o["view.wrap.toggle"], "");
        let map = KeyMap::build(&o, &[]);
        assert!(map.bindings(ActionId::Wrap).is_empty());
        assert!(map.is_retired(b("Alt+W")));
    }

    #[test]
    fn conflicts_follow_the_scopes() {
        // Search next (stream, F3) given F1, the help key (window): the user's binding
        // wins, the help loses it with a notice.
        let mut o = BTreeMap::new();
        o.insert("search.next".to_string(), "F1".to_string());
        let map = KeyMap::build(&o, &[]);
        assert_eq!(map.bindings(ActionId::SearchNext), [b("F1")]);
        assert!(map.bindings(ActionId::Help).is_empty());
        assert_eq!(map.notices.len(), 1, "{:?}", map.notices);
        assert_eq!(map.changed_action(b("F1")), Some(ActionId::SearchNext));
        // F3 is free now and fires nothing; F1 is held, so it is not retired.
        assert!(map.is_retired(b("F3")));
        assert!(!map.is_retired(b("F1")));
        // A stream key and a list key may be the same.
        let mut o = BTreeMap::new();
        o.insert("search.next".to_string(), "Ctrl+J".to_string());
        o.insert("view.wrap.toggle".to_string(), "Ctrl+J".to_string());
        let map = KeyMap::build(&o, &[]);
        assert_eq!(
            map.notices.len(),
            1,
            "two stream actions on one key: {:?}",
            map.notices
        );
        // Unknown ids and bad values are reported, the rest applies.
        let mut o = BTreeMap::new();
        o.insert("no.such.action".to_string(), "F9".to_string());
        o.insert("view.wrap.toggle".to_string(), "Ctrl+???".to_string());
        let map = KeyMap::build(&o, &[]);
        assert_eq!(map.notices.len(), 2);
        assert_eq!(map.bindings(ActionId::Wrap), [b("Alt+W")]);
        assert_eq!(
            map.holder(b("Alt+W"), Scope::Stream, ActionId::Help),
            Some(ActionId::Wrap)
        );
    }

    #[test]
    fn a_tool_on_a_built_in_key_is_flagged() {
        let tool = |sc: &str| {
            let mut t = ExternalTool::new("t", "echo", "{line}");
            t.shortcut = Some(sc.into());
            t
        };
        let map = KeyMap::build(
            &BTreeMap::new(),
            &[tool("Ctrl+Shift+F"), tool("Ctrl+Shift+F9")],
        );
        assert_eq!(map.tool_conflicts, vec![0]);
    }
}
