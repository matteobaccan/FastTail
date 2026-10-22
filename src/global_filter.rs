// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

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

/// What decides the compiled set: two filters with the same key hide the same lines (the
/// toggles, the include terms or the expression, the exclude terms).
pub type AppliedKey = (bool, bool, Vec<String>, Vec<String>, Option<String>);

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
    /// Expression mode: the include side is `expression` (`mode=expr`, `expression=`);
    /// the include terms are kept for the way back.
    pub expr_mode: bool,
    pub expression: String,
    /// The last expression that compiled: what filters while the one typed does not, as
    /// a stream's does (`note_expression`). Not saved.
    pub last_valid_expression: String,
}

impl GlobalFilter {
    /// Whether any include or exclude term is non-empty (in expression mode, the
    /// expression in place of the include terms).
    pub fn has_terms(&self) -> bool {
        let include = if self.expr_mode {
            !self.effective_expression().trim().is_empty()
        } else {
            self.include.iter().any(|t| !t.is_empty())
        };
        include || self.exclude.iter().any(|t| !t.is_empty())
    }

    /// The toggles the expression is compiled with.
    fn toggles(&self) -> crate::filter_expr::Toggles {
        crate::filter_expr::Toggles {
            case_sensitive: self.case_sensitive,
            is_regex: self.is_regex,
            fields: false,
        }
    }

    /// Why the expression does not compile, in expression mode (its include side then
    /// filters nothing).
    pub fn expr_error(&self) -> Option<crate::filter_expr::ExprError> {
        if !self.expr_mode {
            return None;
        }
        crate::filter_expr::FilterExpr::compile(&self.expression, self.toggles()).err()
    }

    /// Switches between include terms and an expression; the first switch to an empty
    /// expression builds it from the include terms.
    pub fn set_expr_mode(&mut self, on: bool) {
        if on && self.expression.trim().is_empty() {
            self.expression = crate::filter_expr::from_terms(&self.include, false);
        }
        self.expr_mode = on;
        self.note_expression();
    }

    /// Records the expression as the last valid one when it compiles (after an edit).
    pub fn note_expression(&mut self) {
        let compiles =
            crate::filter_expr::FilterExpr::compile(&self.expression, self.toggles()).is_ok();
        if compiles {
            self.last_valid_expression = self.expression.clone();
        }
    }

    /// The expression that filters: the one typed when it compiles, else the last one
    /// that did.
    pub fn effective_expression(&self) -> &str {
        match crate::filter_expr::FilterExpr::compile(&self.expression, self.toggles()) {
            Ok(_) => &self.expression,
            Err(_) => &self.last_valid_expression,
        }
    }

    /// Whether the streams apply it now: switched on, with at least one term.
    pub fn is_applied(&self) -> bool {
        self.enabled && self.has_terms()
    }

    /// The terms compiled with the bar's toggles, whether or not the filter is on (the
    /// bar flags a regex that does not compile from it).
    pub fn spec(&self) -> FilterSpec {
        let none: [String; 0] = [];
        let include: &[String] = if self.expr_mode { &none } else { &self.include };
        let expr = self
            .expr_mode
            .then(|| {
                crate::filter_expr::FilterExpr::compile(self.effective_expression(), self.toggles())
            })
            .and_then(Result::ok)
            .flatten()
            .map(Arc::new);
        FilterSpec::build(include, &self.exclude, self.case_sensitive, self.is_regex)
            .with_expr(expr)
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
                if self.expr_mode {
                    Vec::new()
                } else {
                    non_empty(&self.include)
                },
                non_empty(&self.exclude),
                self.expr_mode
                    .then(|| self.effective_expression().trim().to_string()),
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
            && self.include.iter().all(|t| t.is_empty())
            && self.expression.is_empty()
            && !self.expr_mode
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
        if self.expr_mode {
            sec.set("mode", "expr");
        }
        if !self.expression.is_empty() {
            sec.set("expression", ini_value(&self.expression));
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
        let mut out = Self {
            enabled: flag("enabled"),
            bar_open: flag("bar_open"),
            case_sensitive: flag("case_sensitive"),
            is_regex: flag("regex"),
            include: terms("include"),
            exclude: terms("exclude"),
            expr_mode: sec
                .get("mode")
                .is_some_and(|m| m.trim().eq_ignore_ascii_case("expr")),
            expression: sec.get("expression").unwrap_or("").to_string(),
            last_valid_expression: String::new(),
        };
        out.note_expression();
        out
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
            expr_mode: true,
            expression: r#"(a OR "b c") NOT /d\/e/"#.into(),
            last_valid_expression: r#"(a OR "b c") NOT /d\/e/"#.into(),
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fasttail.ini");
        let mut conf = Ini::new();
        g.write(&mut conf);
        conf.write_to_file(&path).unwrap();
        assert_eq!(GlobalFilter::read(&Ini::load_from_file(&path).unwrap()), g);
    }
}
