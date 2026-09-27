//! The global filter: include and exclude terms, with their own case and regex toggles,
//! that every stream combines with its own filter (see `FilterSpec::with_global`). The
//! app compiles them once per edit into one shared `Arc<FilterSpec>` handed to every
//! stream, including those opened later. Persisted in the `[global_filter]` section of
//! `fasttail.ini`; not part of sessions or filter presets.

use crate::filter_preset::ini_value;
use crate::scan_job::{FilterSpec, MAX_FILTER_TERMS};
use ini::Ini;
use std::sync::Arc;

/// Section of `fasttail.ini` holding the global filter.
pub const SECTION: &str = "global_filter";

/// Delay between the last edit of a term and its application to the streams, so typing
/// does not restart every stream's background filter job at each key.
pub const APPLY_DELAY_MS: u64 = 300;

/// Bytes of streams refiltered on the UI thread per frame when the global filter changes
/// (streams above the job threshold refilter in the background and do not count): the
/// others follow on the next frames instead of freezing one.
pub const SYNC_BUDGET_BYTES: u64 = 32 * 1024 * 1024;

/// What decides the compiled set: two filters with the same key hide the same lines.
pub type AppliedKey = (bool, bool, Vec<String>, Vec<String>);

/// What the user set in the global filter bar. Term rows are kept as typed (empty rows
/// included) while editing; only the non-empty ones are saved and applied.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GlobalFilter {
    /// Applied to the streams (the terms are kept when it is off).
    pub enabled: bool,
    /// The bar under the menu bar is shown.
    pub bar_open: bool,
    pub case_sensitive: bool,
    pub is_regex: bool,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
}

impl GlobalFilter {
    /// Whether any include or exclude term is non-empty.
    pub fn has_terms(&self) -> bool {
        self.include
            .iter()
            .chain(&self.exclude)
            .any(|t| !t.is_empty())
    }

    /// Whether the streams apply it now: switched on, with at least one term.
    pub fn is_applied(&self) -> bool {
        self.enabled && self.has_terms()
    }

    /// The terms compiled with the bar's toggles, whether or not the filter is on (the
    /// bar flags a regex that does not compile from it).
    pub fn spec(&self) -> FilterSpec {
        FilterSpec::build(
            &self.include,
            &self.exclude,
            self.case_sensitive,
            self.is_regex,
        )
    }

    /// The key of the set the streams apply (`None` when off or without a term): the
    /// toggles and the non-empty terms, so an empty row or a bar closed without an edit
    /// changes nothing.
    pub fn applied_key(&self) -> Option<AppliedKey> {
        let non_empty = |list: &[String]| -> Vec<String> {
            list.iter().filter(|t| !t.is_empty()).cloned().collect()
        };
        self.is_applied().then(|| {
            (
                self.case_sensitive,
                self.is_regex,
                non_empty(&self.include),
                non_empty(&self.exclude),
            )
        })
    }

    /// The set the streams apply: `None` when off or without a term.
    pub fn compile(&self) -> Option<Arc<FilterSpec>> {
        self.is_applied().then(|| Arc::new(self.spec()))
    }

    /// Writes the `[global_filter]` section, only when it differs from the default (off,
    /// no term, bar closed, toggles off), so an untouched installation keeps its file
    /// unchanged; empty term rows count as no term.
    pub fn write(&self, conf: &mut Ini) {
        if !self.enabled
            && !self.bar_open
            && !self.case_sensitive
            && !self.is_regex
            && !self.has_terms()
        {
            return;
        }
        let mut sec = conf.with_section(Some(SECTION));
        sec.set("enabled", self.enabled.to_string());
        sec.set("bar_open", self.bar_open.to_string());
        sec.set("case_sensitive", self.case_sensitive.to_string());
        sec.set("regex", self.is_regex.to_string());
        for (side, terms) in [("include", &self.include), ("exclude", &self.exclude)] {
            for (n, term) in terms.iter().filter(|t| !t.is_empty()).enumerate() {
                sec.set(format!("{side}.{}", n + 1), ini_value(term));
            }
        }
    }

    /// Reads the `[global_filter]` section; missing keys take the defaults.
    pub fn read(conf: &Ini) -> Self {
        let Some(sec) = conf.section(Some(SECTION)) else {
            return Self::default();
        };
        let flag = |key: &str| sec.get(key).and_then(|v| v.parse().ok()).unwrap_or(false);
        let terms = |side: &str| -> Vec<String> {
            (1..=MAX_FILTER_TERMS)
                .filter_map(|n| sec.get(format!("{side}.{n}")))
                .filter(|t| !t.is_empty())
                .map(str::to_string)
                .collect()
        };
        Self {
            enabled: flag("enabled"),
            bar_open: flag("bar_open"),
            case_sensitive: flag("case_sensitive"),
            is_regex: flag("regex"),
            include: terms("include"),
            exclude: terms("exclude"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terms(list: &[&str]) -> Vec<String> {
        list.iter().map(|t| t.to_string()).collect()
    }

    #[test]
    fn compiled_only_when_on_with_a_term() {
        let mut g = GlobalFilter {
            include: terms(&["", ""]),
            ..Default::default()
        };
        assert!(g.compile().is_none(), "off");
        g.enabled = true;
        assert!(g.compile().is_none(), "no non-empty term");
        g.exclude = terms(&["healthcheck"]);
        let spec = g.compile().expect("on with a term");
        assert!(spec.excluded("GET /healthcheck"));
    }

    #[test]
    fn round_trips_through_the_ini_and_stays_out_when_default() {
        let mut conf = Ini::new();
        GlobalFilter::default().write(&mut conf);
        assert!(conf.section(Some(SECTION)).is_none());

        let g = GlobalFilter {
            enabled: true,
            bar_open: true,
            case_sensitive: true,
            is_regex: false,
            include: terms(&["\"status\":500", " ERROR "]),
            exclude: terms(&["healthcheck"]),
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fasttail.ini");
        let mut conf = Ini::new();
        g.write(&mut conf);
        conf.write_to_file(&path).unwrap();
        assert_eq!(GlobalFilter::read(&Ini::load_from_file(&path).unwrap()), g);
    }
}
