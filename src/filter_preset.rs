// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Filter presets: a stream's whole filter state (include and exclude terms, case and
//! regex toggles, minimum level and unknown-level toggle, optionally the time range as
//! typed) saved under a name in `fasttail.ini` and applied again to one stream or to all.
//!
//! The presets drop-down label is derived every frame from the stream's state, nothing
//! is stored besides the name of the preset last applied to the stream (in memory): a
//! stream equal to a preset shows its name, one edited since a preset was applied shows
//! `name *`.

use crate::log_level::LogLevel;
use crate::scan_job::MAX_FILTER_TERMS;
use crate::tail_engine::TailEngine;
use ini::Ini;

/// Section prefix of a preset in `fasttail.ini` (`[filter_preset.0]`, `[filter_preset.1]`…).
const SECTION_PREFIX: &str = "filter_preset.";

/// The filter state of a stream, as a preset stores it and applies it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FilterState {
    /// Non-empty include terms, in order (all must match).
    pub include: Vec<String>,
    /// Non-empty exclude terms, in order (none may match).
    pub exclude: Vec<String>,
    pub case_sensitive: bool,
    pub is_regex: bool,
    /// `Unknown` = no level filter.
    pub min_level: LogLevel,
    pub show_unknown_levels: bool,
    /// Time range as typed (`from`, `to`), only when saved with "include time range":
    /// kept as text so a bare `14:02` follows the day of the log it is applied to.
    pub time: Option<(String, String)>,
    /// Expression mode with this expression (`mode=expr`, `expression=`); applying the
    /// preset switches the stream to it. `None`: term mode.
    pub expr: Option<String>,
}

/// Non-empty terms of a row list, in order.
fn effective(terms: &[String]) -> Vec<String> {
    terms
        .iter()
        .filter(|t| !t.is_empty())
        .take(MAX_FILTER_TERMS)
        .cloned()
        .collect()
}

impl FilterState {
    /// The current state of `engine`, with its time range when `with_time` is set.
    pub fn of_engine(engine: &TailEngine, with_time: bool) -> Self {
        Self {
            include: effective(engine.include_terms()),
            exclude: effective(engine.exclude_terms()),
            case_sensitive: engine.filter_case_sensitive,
            is_regex: engine.filter_is_regex,
            min_level: engine.min_level,
            show_unknown_levels: engine.show_unknown_levels,
            time: with_time.then(|| {
                (
                    engine.time_from_text.trim().to_string(),
                    engine.time_to_text.trim().to_string(),
                )
            }),
            // The expression that filters (the last valid one while the text is broken).
            expr: engine
                .filter_expr_mode()
                .then(|| engine.applied_expr().unwrap_or_default().trim().to_string()),
        }
    }

    /// Whether `engine`'s filter state equals this one. Empty term rows do not count,
    /// the unknown-level toggle only counts where it has an effect (a threshold above
    /// `TRACE`), and the time range only when this state carries one.
    pub fn matches_engine(&self, engine: &TailEngine) -> bool {
        let unknown_matters = self.min_level > LogLevel::Trace;
        let include_side = match &self.expr {
            Some(expr) => {
                engine.filter_expr_mode()
                    && expr.as_str() == engine.applied_expr().unwrap_or_default().trim()
            }
            None => !engine.filter_expr_mode() && self.include == effective(engine.include_terms()),
        };
        include_side
            && self.exclude == effective(engine.exclude_terms())
            && self.case_sensitive == engine.filter_case_sensitive
            && self.is_regex == engine.filter_is_regex
            && self.min_level == engine.min_level
            && (!unknown_matters || self.show_unknown_levels == engine.show_unknown_levels)
            && self.time.as_ref().is_none_or(|(from, to)| {
                from == engine.time_from_text.trim() && to == engine.time_to_text.trim()
            })
    }
}

/// A named filter state.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FilterPreset {
    pub name: String,
    pub state: FilterState,
    /// Listed first (`pinned=true`); older builds ignore it.
    pub pinned: bool,
    /// When it was last applied, in seconds since the epoch (`last_used=`, 0: never):
    /// the five most recent are listed after the pinned ones.
    pub last_used: u64,
}

impl FilterPreset {
    /// Makes `engine`'s filter state this preset's, in one recomputation, and remembers
    /// the preset as the one last applied to the stream.
    pub fn apply_to(&self, engine: &mut TailEngine) {
        engine.set_filter_state(&self.state);
        engine.applied_preset = Some(self.name.clone());
    }
}

/// What the presets drop-down of a stream shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetLabel<'a> {
    /// No preset applied and none equal to the stream's state: the plain menu title.
    None,
    /// The stream's state equals this preset.
    Matches(&'a str),
    /// This preset was applied last and the state was edited since (`name *`).
    Modified(&'a str),
}

/// The drop-down label of `engine` among `presets` (see the module documentation).
pub fn preset_label<'a>(presets: &'a [FilterPreset], engine: &TailEngine) -> PresetLabel<'a> {
    if let Some(applied) = engine
        .applied_preset
        .as_deref()
        .and_then(|name| find(presets, name))
        .map(|i| &presets[i])
    {
        return if applied.state.matches_engine(engine) {
            PresetLabel::Matches(&applied.name)
        } else {
            PresetLabel::Modified(&applied.name)
        };
    }
    presets
        .iter()
        .find(|p| p.state.matches_engine(engine))
        .map_or(PresetLabel::None, |p| PresetLabel::Matches(&p.name))
}

/// Index of the preset named `name`, without regard to case.
pub fn find(presets: &[FilterPreset], name: &str) -> Option<usize> {
    presets.iter().position(|p| same_name(&p.name, name))
}

/// Whether two preset names are the same, without regard to case or surrounding spaces.
pub fn same_name(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

/// Whether `name` is taken by a preset other than the one at `except`.
pub fn name_taken(presets: &[FilterPreset], name: &str, except: Option<usize>) -> bool {
    find(presets, name).is_some_and(|i| Some(i) != except)
}

/// Recently used presets listed after the pinned ones.
pub const RECENT_SHOWN: usize = 5;

/// The order presets are listed in: the pinned ones (in list order), then the
/// `RECENT_SHOWN` most recently applied, then the others by name. Indices into `presets`.
pub fn display_order(presets: &[FilterPreset]) -> Vec<usize> {
    let mut out: Vec<usize> = (0..presets.len()).filter(|&i| presets[i].pinned).collect();
    let mut recent: Vec<usize> = (0..presets.len())
        .filter(|&i| !presets[i].pinned && presets[i].last_used > 0)
        .collect();
    recent.sort_by_key(|&i| std::cmp::Reverse(presets[i].last_used));
    recent.truncate(RECENT_SHOWN);
    out.extend(&recent);
    let mut rest: Vec<usize> = (0..presets.len()).filter(|i| !out.contains(i)).collect();
    rest.sort_by_key(|&i| presets[i].name.to_lowercase());
    out.extend(rest);
    out
}

/// Records that preset `i` was applied now (its place among the recent ones).
pub fn mark_used(presets: &mut [FilterPreset], i: usize) {
    if let Some(p) = presets.get_mut(i) {
        p.last_used = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(1, |d| d.as_secs().max(1));
    }
}

/// Stores `preset`, replacing the one with the same name (in place, so the order is
/// kept, and its pin and last use with it) or appending it. Returns its index.
pub fn upsert(presets: &mut Vec<FilterPreset>, mut preset: FilterPreset) -> usize {
    match find(presets, &preset.name) {
        Some(i) => {
            preset.pinned |= presets[i].pinned;
            preset.last_used = preset.last_used.max(presets[i].last_used);
            presets[i] = preset;
            i
        }
        None => {
            presets.push(preset);
            presets.len() - 1
        }
    }
}

/// A term as an ini value that reads back unchanged. rust-ini trims the ends of a value
/// and takes a leading `"` or `'` as a quote, so `"status":500` would come back as
/// `status:500` and ` ERROR ` as `ERROR`. Such a value is written as quoted chunks, which
/// the parser joins: runs without `'` inside `'…'`, runs of `'` inside `"…"`. Backslashes
/// are escaped by the writer and restored by the reader, quoted or not.
pub fn ini_value(term: &str) -> String {
    let needs_quotes = term.starts_with(['"', '\'']) || term.trim() != term;
    if !needs_quotes {
        return term.to_string();
    }
    let mut out = String::with_capacity(term.len() + 4);
    let mut rest = term;
    while !rest.is_empty() {
        let quotes = rest.len() - rest.trim_start_matches('\'').len();
        let (chunk, wrap) = if quotes > 0 {
            (&rest[..quotes], '"')
        } else {
            (&rest[..rest.find('\'').unwrap_or(rest.len())], '\'')
        };
        out.push(wrap);
        out.push_str(chunk);
        out.push(wrap);
        rest = &rest[chunk.len()..];
    }
    out
}

/// Writes one `[filter_preset.N]` section per preset, numbered from 0 in list order.
pub fn write_presets(conf: &mut Ini, presets: &[FilterPreset]) {
    for (i, preset) in presets.iter().enumerate() {
        let s = &preset.state;
        let mut sec = conf.with_section(Some(format!("{SECTION_PREFIX}{i}")));
        sec.set("name", &preset.name);
        for (n, term) in s.include.iter().enumerate() {
            sec.set(format!("include.{}", n + 1), ini_value(term));
        }
        for (n, term) in s.exclude.iter().enumerate() {
            sec.set(format!("exclude.{}", n + 1), ini_value(term));
        }
        sec.set("case_sensitive", s.case_sensitive.to_string());
        sec.set("regex", s.is_regex.to_string());
        sec.set("min_level", s.min_level.name());
        sec.set("show_unknown_levels", s.show_unknown_levels.to_string());
        if let Some((from, to)) = &s.time {
            sec.set("time_from", from);
            sec.set("time_to", to);
        }
        if let Some(expr) = &s.expr {
            sec.set("mode", "expr");
            sec.set("expression", ini_value(expr));
        }
        if preset.pinned {
            sec.set("pinned", "true");
        }
        if preset.last_used > 0 {
            sec.set("last_used", preset.last_used.to_string());
        }
    }
}

/// Reads the `[filter_preset.N]` sections from 0 up to the first gap. A preset without a
/// name, or with the name of an earlier one, is skipped; missing keys take the defaults
/// of an empty filter.
pub fn read_presets(conf: &Ini) -> Vec<FilterPreset> {
    let mut out: Vec<FilterPreset> = Vec::new();
    let mut idx = 0;
    while let Some(sec) = conf.section(Some(format!("{SECTION_PREFIX}{idx}"))) {
        idx += 1;
        let name = sec.get("name").unwrap_or("").trim().to_string();
        if name.is_empty() || find(&out, &name).is_some() {
            continue;
        }
        let terms = |side: &str| -> Vec<String> {
            (1..=MAX_FILTER_TERMS)
                .filter_map(|n| sec.get(format!("{side}.{n}")))
                .filter(|t| !t.is_empty())
                .map(str::to_string)
                .collect()
        };
        let flag = |key: &str| sec.get(key).and_then(|v| v.parse().ok()).unwrap_or(false);
        let time = match (sec.get("time_from"), sec.get("time_to")) {
            (None, None) => None,
            (from, to) => Some((from.unwrap_or("").to_string(), to.unwrap_or("").to_string())),
        };
        out.push(FilterPreset {
            pinned: flag("pinned"),
            last_used: sec
                .get("last_used")
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(0),
            name,
            state: FilterState {
                include: terms("include"),
                exclude: terms("exclude"),
                case_sensitive: flag("case_sensitive"),
                is_regex: flag("regex"),
                min_level: LogLevel::parse(sec.get("min_level").unwrap_or("")),
                show_unknown_levels: flag("show_unknown_levels"),
                time,
                expr: sec
                    .get("mode")
                    .is_some_and(|m| m.trim().eq_ignore_ascii_case("expr"))
                    .then(|| sec.get("expression").unwrap_or("").to_string()),
            },
        });
    }
    out
}

#[cfg(test)]
mod order_tests {
    use super::*;

    fn p(name: &str, pinned: bool, last_used: u64) -> FilterPreset {
        FilterPreset {
            name: name.into(),
            pinned,
            last_used,
            ..FilterPreset::default()
        }
    }

    #[test]
    fn pinned_first_then_recent_then_by_name_and_saved() {
        let presets = vec![
            p("zeta", false, 0),
            p("errors", true, 0),
            p("alpha", false, 0),
            p("old", false, 10),
            p("new", false, 30),
        ];
        let names: Vec<&str> = display_order(&presets)
            .into_iter()
            .map(|i| presets[i].name.as_str())
            .collect();
        assert_eq!(names, ["errors", "new", "old", "alpha", "zeta"]);
        let mut conf = Ini::new();
        write_presets(&mut conf, &presets);
        let back = read_presets(&conf);
        assert!(back[1].pinned && !back[0].pinned);
        assert_eq!(back[4].last_used, 30);
        // Saving a preset again keeps its pin.
        let mut list = back;
        upsert(&mut list, p("errors", false, 0));
        assert!(list[1].pinned);
        mark_used(&mut list, 2);
        assert!(list[2].last_used > 1_000_000);
    }
}
