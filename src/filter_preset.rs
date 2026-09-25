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
        }
    }

    /// Whether `engine`'s filter state equals this one. Empty term rows do not count,
    /// the unknown-level toggle only counts where it has an effect (a threshold above
    /// `TRACE`), and the time range only when this state carries one.
    pub fn matches_engine(&self, engine: &TailEngine) -> bool {
        let unknown_matters = self.min_level > LogLevel::Trace;
        self.include == effective(engine.include_terms())
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
    let wanted = name.trim().to_lowercase();
    presets
        .iter()
        .position(|p| p.name.trim().to_lowercase() == wanted)
}

/// Whether `name` is taken by a preset other than the one at `except`.
pub fn name_taken(presets: &[FilterPreset], name: &str, except: Option<usize>) -> bool {
    find(presets, name).is_some_and(|i| Some(i) != except)
}

/// Stores `preset`, replacing the one with the same name (in place, so the order is
/// kept) or appending it. Returns its index.
pub fn upsert(presets: &mut Vec<FilterPreset>, preset: FilterPreset) -> usize {
    match find(presets, &preset.name) {
        Some(i) => {
            presets[i] = preset;
            i
        }
        None => {
            presets.push(preset);
            presets.len() - 1
        }
    }
}

/// Writes one `[filter_preset.N]` section per preset, numbered from 0 in list order.
pub fn write_presets(conf: &mut Ini, presets: &[FilterPreset]) {
    for (i, preset) in presets.iter().enumerate() {
        let s = &preset.state;
        let mut sec = conf.with_section(Some(format!("{SECTION_PREFIX}{i}")));
        sec.set("name", &preset.name);
        for (n, term) in s.include.iter().enumerate() {
            sec.set(format!("include.{}", n + 1), term);
        }
        for (n, term) in s.exclude.iter().enumerate() {
            sec.set(format!("exclude.{}", n + 1), term);
        }
        sec.set("case_sensitive", s.case_sensitive.to_string());
        sec.set("regex", s.is_regex.to_string());
        sec.set("min_level", s.min_level.name());
        sec.set("show_unknown_levels", s.show_unknown_levels.to_string());
        if let Some((from, to)) = &s.time {
            sec.set("time_from", from);
            sec.set("time_to", to);
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
            name,
            state: FilterState {
                include: terms("include"),
                exclude: terms("exclude"),
                case_sensitive: flag("case_sensitive"),
                is_regex: flag("regex"),
                min_level: LogLevel::parse(sec.get("min_level").unwrap_or("")),
                show_unknown_levels: flag("show_unknown_levels"),
                time,
            },
        });
    }
    out
}
