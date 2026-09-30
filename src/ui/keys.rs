// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The GUI side of the neutral shortcut types in `external_tools`: egui keys and
//! modifiers for a `Shortcut`.

use crate::external_tools::{KeyName, Mods, Shortcut};

/// The egui key of `key`. Every `KeyName` is spelled as egui names its keys.
pub fn egui_key(key: KeyName) -> egui::Key {
    egui::Key::from_name(key.name()).expect("KeyName spells egui key names")
}

/// The egui modifiers of `mods`; `ctrl` is Command on macOS.
pub fn egui_modifiers(mods: Mods) -> egui::Modifiers {
    let mut m = egui::Modifiers::NONE;
    if mods.ctrl {
        m |= egui::Modifiers::COMMAND;
    }
    if mods.shift {
        m |= egui::Modifiers::SHIFT;
    }
    if mods.alt {
        m |= egui::Modifiers::ALT;
    }
    m
}

/// The egui modifiers and key of a tool shortcut.
pub fn egui_shortcut(sc: Shortcut) -> (egui::Modifiers, egui::Key) {
    (egui_modifiers(sc.mods), egui_key(sc.key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_name_is_an_egui_key() {
        for key in KeyName::all() {
            assert_eq!(egui_key(key).name(), key.name(), "{key:?}");
        }
    }

    /// The parser before the neutral types: egui resolved the key name. Every input must
    /// give the same key as before, so saved `tool.N` shortcuts keep working.
    fn egui_parse(s: &str) -> Option<(bool, bool, bool, egui::Key)> {
        let (mut ctrl, mut shift, mut alt) = (false, false, false);
        let mut key = None;
        for part in s.split(['+', '-']).map(str::trim).filter(|p| !p.is_empty()) {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" | "cmd" | "command" => ctrl = true,
                "shift" => shift = true,
                "alt" | "option" => alt = true,
                name => {
                    let candidate = if name.len() == 1 {
                        name.to_ascii_uppercase()
                    } else {
                        let mut chars = name.chars();
                        let first = chars.next().unwrap().to_ascii_uppercase();
                        format!("{first}{}", chars.as_str())
                    };
                    if key.is_some() {
                        return None;
                    }
                    key = egui::Key::from_name(&candidate);
                    key?;
                }
            }
        }
        if !(ctrl || shift || alt) {
            return None;
        }
        Some((ctrl, shift, alt, key?))
    }

    #[test]
    fn shortcut_parsing_matches_the_egui_parser() {
        // Every spelling egui knows (names, symbols, aliases), in several cases.
        let mut names: Vec<String> = egui::Key::ALL
            .iter()
            .flat_map(|k| [k.name().to_string(), k.symbol_or_name().to_string()])
            .collect();
        names.extend(
            [
                "Esc",
                "Return",
                "NumpadEnter",
                "Help",
                "Equal",
                "NumpadComma",
                "−",
                "NumpadSubtract",
                "NumpadDecimal",
                "NumpadAdd",
                "NumpadEqual",
                "NumpadDivide",
                "BracketLeft",
                "BracketRight",
                "Backquote",
                "Grave",
                "Digit1",
                "Numpad1",
                "KeyA",
                "ArrowDown",
                "Nope",
                "F36",
                "'",
            ]
            .map(String::from),
        );
        for name in names {
            for spelled in [name.clone(), name.to_lowercase(), name.to_uppercase()] {
                for input in [
                    format!("Ctrl+{spelled}"),
                    format!("shift-alt-{spelled}"),
                    spelled.clone(),
                ] {
                    let neutral = Shortcut::parse(&input)
                        .map(|s| (s.mods.ctrl, s.mods.shift, s.mods.alt, egui_key(s.key)));
                    assert_eq!(neutral, egui_parse(&input), "{input:?}");
                }
            }
        }
    }
}
