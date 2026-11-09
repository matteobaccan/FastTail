// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Notepad++ filters as highlight rules, for *Import rules…*: the searches of an
//! AnalysePlugin file (elements with a `searchText` attribute, their `searchType`,
//! `matchCase`, `color` and `bgColor`) and the keyword lists of a User Defined Language
//! (`<Keywords name="Keywords1">` with the colours of the `KEYWORDS1` style).
//!
//! The XML is read with a small tolerant scanner over tags and attributes: both formats
//! are flat lists of attributes, and no XML crate is needed for them. Entries the plugin
//! marks hidden (`hide="1"`) are left out: FastTail rules colour lines, they do not hide
//! them.

use crate::tail_engine::HighlightRule;
use regex::Regex;
use std::sync::LazyLock;

static TAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<([A-Za-z_][\w.-]*)((?:\s+[\w:.-]+\s*=\s*(?:"[^"]*"|'[^']*'))*)\s*(/?)>"#)
        .expect("tag pattern")
});
static ATTR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"([\w:.-]+)\s*=\s*(?:"([^"]*)"|'([^']*)')"#).expect("attribute pattern")
});

/// Default text colour when a search has none (the theme's foreground is not known here).
const DEFAULT_FG: [u8; 3] = [0xFF, 0xFF, 0xFF];
const DEFAULT_BG: [u8; 3] = [0x00, 0x00, 0x00];

/// One tag: its name, attributes and, for an opening tag, the text up to its end tag.
struct Tag {
    name: String,
    attrs: Vec<(String, String)>,
    text: String,
}

impl Tag {
    fn get(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn tags(xml: &str) -> Vec<Tag> {
    TAG.captures_iter(xml)
        .map(|c| {
            let name = c[1].to_string();
            let attrs = ATTR
                .captures_iter(&c[2])
                .map(|a| {
                    let v = a.get(2).or_else(|| a.get(3)).map_or("", |m| m.as_str());
                    (a[1].to_string(), unescape(v))
                })
                .collect();
            let end = c.get(0).map_or(0, |m| m.end());
            let text = if &c[3] == "/" {
                String::new()
            } else {
                let close = format!("</{name}");
                xml[end..]
                    .find(&close)
                    .map(|i| unescape(xml[end..end + i].trim()))
                    .unwrap_or_default()
            };
            Tag { name, attrs, text }
        })
        .collect()
}

/// A colour as Notepad++ writes it: `RRGGBB` hex (UDL), `#RRGGBB`, or a Windows
/// `COLORREF` (`0x00BBGGRR` or its decimal value, AnalysePlugin).
pub fn parse_color(value: &str) -> Option<[u8; 3]> {
    let v = value.trim();
    let bgr = |n: u32| {
        [
            (n & 0xFF) as u8,
            ((n >> 8) & 0xFF) as u8,
            ((n >> 16) & 0xFF) as u8,
        ]
    };
    if let Some(hex) = v.strip_prefix("0x").or_else(|| v.strip_prefix("0X")) {
        return u32::from_str_radix(hex, 16).ok().map(bgr);
    }
    if let Some(hex) = v.strip_prefix('#') {
        let n = u32::from_str_radix(hex, 16).ok()?;
        return Some([(n >> 16) as u8, (n >> 8) as u8, n as u8]);
    }
    if v.len() == 6 && v.bytes().all(|b| b.is_ascii_hexdigit()) {
        let n = u32::from_str_radix(v, 16).ok()?;
        return Some([(n >> 16) as u8, (n >> 8) as u8, n as u8]);
    }
    v.parse::<u32>().ok().map(bgr)
}

fn flag(tag: &Tag, key: &str) -> bool {
    tag.get(key)
        .is_some_and(|v| matches!(v.trim(), "1" | "true" | "yes"))
}

/// Whether `text` is a Notepad++ XML file this module reads.
pub fn is_npp_xml(text: &str) -> bool {
    let t = text.trim_start_matches('\u{feff}').trim_start();
    t.starts_with('<') && (t.contains("searchText") || t.contains("<UserLang"))
}

/// The highlight rules of a Notepad++ AnalysePlugin file or UDL file; empty when it holds
/// none.
pub fn parse(text: &str) -> Vec<HighlightRule> {
    let tags = tags(text);
    let mut rules = Vec::new();
    // AnalysePlugin searches.
    for t in tags.iter().filter(|t| t.get("searchText").is_some()) {
        let pattern = t.get("searchText").unwrap_or("").to_string();
        if pattern.is_empty() || flag(t, "hide") {
            continue;
        }
        let is_regex = t
            .get("searchType")
            .is_some_and(|s| s.to_ascii_lowercase().contains("regex"));
        let fg = t.get("color").and_then(parse_color).unwrap_or(DEFAULT_FG);
        let bg = t.get("bgColor").and_then(parse_color).unwrap_or(DEFAULT_BG);
        let mut rule = HighlightRule::new(&pattern, fg, bg, is_regex);
        rule.case_sensitive = flag(t, "matchCase");
        rule.bold = flag(t, "bold");
        rule.italic = flag(t, "italic");
        rules.push(rule);
    }
    // UDL keyword lists, `Keywords1` .. `Keywords8`, with the `KEYWORDS<n>` style colours.
    let style = |n: &str| {
        tags.iter().find(|t| {
            t.name.eq_ignore_ascii_case("WordsStyle")
                && t.get("name").is_some_and(|v| v.eq_ignore_ascii_case(n))
        })
    };
    for t in tags
        .iter()
        .filter(|t| t.name.eq_ignore_ascii_case("Keywords"))
    {
        let Some(n) = t
            .get("name")
            .and_then(|v| v.strip_prefix("Keywords"))
            .filter(|n| n.len() == 1 && n.as_bytes()[0].is_ascii_digit())
        else {
            continue;
        };
        let words: Vec<String> = t
            .text
            .split_whitespace()
            .map(|w| regex::escape(w.trim_matches('"')))
            .filter(|w| !w.is_empty())
            .collect();
        if words.is_empty() {
            continue;
        }
        let s = style(&format!("KEYWORDS{n}"));
        let fg = s
            .and_then(|s| s.get("fgColor"))
            .and_then(parse_color)
            .unwrap_or(DEFAULT_FG);
        let bg = s
            .and_then(|s| s.get("bgColor"))
            .and_then(parse_color)
            .unwrap_or(DEFAULT_BG);
        let mut rule = HighlightRule::new(&format!(r"\b(?:{})\b", words.join("|")), fg, bg, true);
        rule.case_sensitive = false;
        rule.captures_only = true;
        rules.push(rule);
    }
    rules
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyse_plugin_searches_become_rules() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8" ?>
<NotepadPlus_AnalysePlugin version="1.0">
  <SearchText searchText="ERROR" searchType="normal" matchCase="1" hide="0" bold="1" color="0x0000FF" bgColor="0xFFFFFF" />
  <SearchText searchText="req=\d+" searchType="regex" matchCase="0" color="0x00FF00" bgColor="0x000000" />
  <SearchText searchText="DEBUG" hide="1" color="0x808080" />
  <SearchText searchText="a &amp; b" />
</NotepadPlus_AnalysePlugin>"#;
        assert!(is_npp_xml(xml));
        let rules = parse(xml);
        assert_eq!(rules.len(), 3);
        assert_eq!(rules[0].pattern, "ERROR");
        assert!(rules[0].case_sensitive && rules[0].bold && !rules[0].is_regex);
        // 0x0000FF is a COLORREF: red.
        assert_eq!(rules[0].fg_color, [0xFF, 0, 0]);
        assert_eq!(rules[0].bg_color, [0xFF, 0xFF, 0xFF]);
        assert!(rules[1].is_regex && !rules[1].case_sensitive);
        assert_eq!(rules[2].pattern, "a & b");
    }

    #[test]
    fn udl_keyword_lists_become_word_rules_with_their_style_colours() {
        let xml = r#"<NotepadPlus>
  <UserLang name="MyLog" ext="log">
    <KeywordLists>
      <Keywords name="Keywords1">ERROR FATAL</Keywords>
      <Keywords name="Keywords2"></Keywords>
      <Keywords name="Comments">00# 01 02</Keywords>
    </KeywordLists>
    <Styles>
      <WordsStyle name="KEYWORDS1" fgColor="FF0000" bgColor="FFFFFF" fontStyle="1" />
    </Styles>
  </UserLang>
</NotepadPlus>"#;
        assert!(is_npp_xml(xml));
        let rules = parse(xml);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].pattern, r"\b(?:ERROR|FATAL)\b");
        assert_eq!(rules[0].fg_color, [0xFF, 0, 0]);
        assert!(rules[0].is_regex);
        assert!(!is_npp_xml("[fasttail_rules]\nversion=1\n"));
        assert_eq!(parse_color("#102030"), Some([0x10, 0x20, 0x30]));
        assert_eq!(parse_color("255"), Some([0xFF, 0, 0]));
    }
}
