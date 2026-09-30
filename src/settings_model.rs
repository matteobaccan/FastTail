// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! What a setting may hold, shared by `FastTailConfig::load` (the ini and the
//! `FASTTAIL_*` variables), the GUI Settings and editors and the terminal dialogs, so
//! every place accepts exactly the same values: the ranges of the numeric settings and
//! the checks of preset names, external tools, highlight rules and the global filter.

use std::ops::RangeInclusive;

/// `[general] poll_interval_ms`: how often the streams are polled.
pub const POLL_INTERVAL_MS: RangeInclusive<u32> = 50..=5000;
/// `[general] size_check_interval_ms`: how often a file's size is checked.
pub const SIZE_CHECK_INTERVAL_MS: RangeInclusive<u32> = 50..=10000;
/// `[general] max_fps`: frame cap of the GPU renderers.
pub const MAX_FPS: RangeInclusive<u32> = 5..=240;
/// `[general] max_fps_software`: frame cap of the software renderer.
pub const MAX_FPS_SOFTWARE: RangeInclusive<u32> = 5..=120;
/// `[general] mouse_throttle_ms`: minimum time between two pointer-move repaints.
pub const MOUSE_THROTTLE_MS: RangeInclusive<u64> = 0..=1000;
/// `[general] markdown_max_mb`: largest file the Markdown view renders.
pub const MARKDOWN_MAX_MB: RangeInclusive<u32> = 1..=100;
/// `[general] auto_bookmark_max`: automatic bookmarks kept per stream.
pub const AUTO_BOOKMARK_MAX: RangeInclusive<usize> =
    crate::tail_engine::MIN_AUTO_BOOKMARK_MAX..=crate::tail_engine::MAX_AUTO_BOOKMARK_MAX;
/// `[general] compressed_max_gb`: largest decompressed spool.
pub const COMPRESSED_MAX_GB: RangeInclusive<u32> =
    crate::compressed::MIN_MAX_GB..=crate::compressed::MAX_MAX_GB;
/// `[general] stdin_spool_max_mb`: size the standard-input spool restarts from empty at.
pub const STDIN_SPOOL_MAX_MB: RangeInclusive<u32> =
    crate::stdin_source::MIN_MAX_MB..=crate::stdin_source::MAX_MAX_MB;
/// `[general] screensaver_timeout_mins`: idle minutes before the screensaver (GUI) or the
/// idle lock (terminal); 0 turns it off.
pub const SCREENSAVER_TIMEOUT_MINS: RangeInclusive<u32> = 0..=120;
/// `[general] time_delta_gap_ms`: gap the time delta column marks, up to a day.
pub const TIME_DELTA_GAP_MS: RangeInclusive<u64> = 0..=86_400_000;

/// `value` brought into `range`.
pub fn clamp<T: Ord + Copy>(value: T, range: &RangeInclusive<T>) -> T {
    value.clamp(*range.start(), *range.end())
}

/// Why a filter preset name cannot be used as it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetNameProblem {
    Empty,
    /// Another preset has it (`except` is the one being renamed).
    Taken,
}

/// Checks a preset name (trimmed). Saving under a taken name overwrites that preset, so
/// the save dialog only refuses `Empty`; a rename refuses both.
pub fn preset_name_problem(
    presets: &[crate::filter_preset::FilterPreset],
    name: &str,
    except: Option<usize>,
) -> Option<PresetNameProblem> {
    let name = name.trim();
    if name.is_empty() {
        Some(PresetNameProblem::Empty)
    } else if crate::filter_preset::name_taken(presets, name, except) {
        Some(PresetNameProblem::Taken)
    } else {
        None
    }
}

/// What is wrong with an external tool's settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolProblem {
    /// The shortcut cannot be read (see `external_tools::Shortcut::parse`).
    BadShortcut,
    /// The tool is bound to a highlight rule that no longer exists.
    MissingRule,
}

/// The problems of `tool`, given the patterns of the current highlight rules.
pub fn tool_problems(
    tool: &crate::external_tools::ExternalTool,
    rule_patterns: &[&str],
) -> Vec<ToolProblem> {
    let mut problems = Vec::new();
    if tool.shortcut.is_some() && tool.parsed_shortcut().is_none() {
        problems.push(ToolProblem::BadShortcut);
    }
    if let Some(bound) = tool.bound_rule.as_deref() {
        if !rule_patterns.contains(&bound) {
            problems.push(ToolProblem::MissingRule);
        }
    }
    problems
}

/// The compile error of a regular expression, `None` when it compiles.
pub fn regex_error(pattern: &str, case_sensitive: bool) -> Option<String> {
    regex::RegexBuilder::new(pattern)
        .case_insensitive(!case_sensitive)
        .build()
        .err()
        .map(|e| e.to_string())
}

/// Why a highlight rule will not match as written: a regex rule whose pattern does not
/// compile (the engine then matches it as plain text).
pub fn rule_problem(rule: &crate::tail_engine::HighlightRule) -> Option<String> {
    if rule.is_regex {
        regex_error(&rule.pattern, rule.case_sensitive)
    } else {
        None
    }
}

/// The first term of a regex global filter that does not compile, with its error.
pub fn global_filter_problem(
    filter: &crate::global_filter::GlobalFilter,
) -> Option<(String, String)> {
    if !filter.is_regex {
        return None;
    }
    filter
        .include
        .iter()
        .chain(&filter.exclude)
        .filter(|t| !t.is_empty())
        .find_map(|t| regex_error(t, filter.case_sensitive).map(|e| (t.clone(), e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_keeps_values_in_range() {
        assert_eq!(clamp(10, &POLL_INTERVAL_MS), 50);
        assert_eq!(clamp(300, &POLL_INTERVAL_MS), 300);
        assert_eq!(clamp(9999, &POLL_INTERVAL_MS), 5000);
        assert_eq!(clamp(500, &SCREENSAVER_TIMEOUT_MINS), 120);
    }

    #[test]
    fn every_default_is_inside_its_range() {
        let cfg = crate::config::FastTailConfig::default();
        assert!(POLL_INTERVAL_MS.contains(&cfg.poll_interval_ms));
        assert!(SIZE_CHECK_INTERVAL_MS.contains(&cfg.size_check_interval_ms));
        assert!(MAX_FPS.contains(&cfg.max_fps));
        assert!(MAX_FPS_SOFTWARE.contains(&cfg.max_fps_software));
        assert!(MOUSE_THROTTLE_MS.contains(&cfg.mouse_throttle_ms));
        assert!(MARKDOWN_MAX_MB.contains(&cfg.markdown_max_mb));
        assert!(AUTO_BOOKMARK_MAX.contains(&cfg.auto_bookmark_max));
        assert!(COMPRESSED_MAX_GB.contains(&cfg.compressed_max_gb));
        assert!(STDIN_SPOOL_MAX_MB.contains(&cfg.stdin_spool_max_mb));
        assert!(SCREENSAVER_TIMEOUT_MINS.contains(&cfg.screensaver_timeout_mins));
        assert!(TIME_DELTA_GAP_MS.contains(&cfg.time_delta_gap_ms));
    }

    #[test]
    fn presets_tools_rules_and_filters_are_checked() {
        use crate::external_tools::ExternalTool;
        use crate::filter_preset::FilterPreset;
        let presets = vec![FilterPreset {
            name: "errors".to_string(),
            state: Default::default(),
        }];
        assert_eq!(
            preset_name_problem(&presets, "  ", None),
            Some(PresetNameProblem::Empty)
        );
        assert_eq!(
            preset_name_problem(&presets, "errors", None),
            Some(PresetNameProblem::Taken)
        );
        assert_eq!(preset_name_problem(&presets, "errors", Some(0)), None);

        let mut tool = ExternalTool::new("edit", "code", "{file}");
        assert!(tool_problems(&tool, &[]).is_empty());
        tool.shortcut = Some("Ctrl+Nope".to_string());
        tool.bound_rule = Some("ERROR".to_string());
        assert_eq!(
            tool_problems(&tool, &["WARN"]),
            vec![ToolProblem::BadShortcut, ToolProblem::MissingRule]
        );
        assert_eq!(
            tool_problems(&tool, &["ERROR"]),
            vec![ToolProblem::BadShortcut]
        );

        let mut rule = crate::tail_engine::HighlightRule::new("a(b", [0; 3], [0; 3], true);
        assert!(rule_problem(&rule).is_some());
        rule.is_regex = false;
        assert!(
            rule_problem(&rule).is_none(),
            "plain text always matches as written"
        );

        let mut filter = crate::global_filter::GlobalFilter {
            include: vec!["ok".to_string(), "[x".to_string()],
            ..Default::default()
        };
        assert!(global_filter_problem(&filter).is_none(), "plain terms");
        filter.is_regex = true;
        assert_eq!(
            global_filter_problem(&filter).map(|(t, _)| t),
            Some("[x".to_string())
        );
    }
}
