// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The analysis views of the terminal (`Alt+A`): Statistics of a field, the Query tab,
//! Patterns, Explain a span and Operations, drawn as one table over the windows. Each
//! reads the stream's own job (`TailEngine::stats`, `query`, `patterns`, `explain`,
//! `operations`), the window's, so nothing is computed twice. The table is shared: a
//! header, rows sorted by any column (`s`, `S` reverses), narrowed by typing after `/`,
//! and `Enter` (or `x`) on a row filters on it or goes to its line.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::i18n::Language;
use crate::i18n_tui::{en, tx, txf};
use crate::log_level::LogLevel;
use crate::tail_engine::TailEngine;
use crate::tui::form::{FieldKey, TextField};

/// The view shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Stats,
    Query,
    Patterns,
    Explain,
    Operations,
}

impl Kind {
    pub const ALL: [Kind; 5] = [
        Kind::Stats,
        Kind::Query,
        Kind::Patterns,
        Kind::Explain,
        Kind::Operations,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Kind::Stats => en("Statistics of a field"),
            Kind::Query => en("Query"),
            Kind::Patterns => en("Patterns"),
            Kind::Explain => en("Explain a span"),
            Kind::Operations => en("Operations"),
        }
    }
}

/// What `Enter` (or `x`) on a row does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowAction {
    None,
    /// A value of the Statistics' source: filter on it.
    Value(String),
    /// A stream line: go to it.
    Line(usize),
    /// A pattern: show only its entries (`x`: hide them).
    Pattern(u32),
    /// A `field=value` pair: filter on it.
    Pair(String, String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub cells: Vec<String>,
    pub level: Option<LogLevel>,
    pub action: RowAction,
}

/// What a view shows this frame.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Table {
    /// Lines above the table (what is counted, the progress, a summary).
    pub summary: Vec<String>,
    pub header: Vec<String>,
    /// Columns aligned to the right (numbers).
    pub right: Vec<bool>,
    pub rows: Vec<Row>,
    /// Why nothing could be shown (a query error, a file that could not be read).
    pub error: Option<String>,
}

/// What a key did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnalysisKey {
    Moved,
    /// `Enter` (`false`) or `x` (`true`, the exclude side) on a row.
    Activate(RowAction, bool),
    /// `Enter` in the query field.
    RunQuery(String),
    Close,
    Other,
}

pub struct AnalysisView {
    pub kind: Kind,
    /// The stream it reads.
    pub path: std::path::PathBuf,
    /// The row selected, among the rows shown (after narrowing and sorting).
    pub selected: usize,
    pub top: usize,
    /// The column sorted by, and whether in reverse; `None`: the view's own order.
    pub sort: Option<(usize, bool)>,
    /// The text after `/`: only the rows holding it are shown.
    pub narrow: TextField,
    pub narrowing: bool,
    /// The Query view's pipeline, and whether it has the keyboard.
    pub query: TextField,
    pub editing: bool,
    /// Rows the last frame drew, for the page keys.
    pub page: usize,
}

impl AnalysisView {
    pub fn new(kind: Kind, path: std::path::PathBuf) -> Self {
        Self {
            kind,
            path,
            selected: 0,
            top: 0,
            sort: None,
            narrow: TextField::default(),
            narrowing: false,
            query: TextField::default(),
            editing: kind == Kind::Query,
            page: 10,
        }
    }

    /// The rows of `table` shown, in order: indices into `table.rows`.
    pub fn shown(&self, table: &Table) -> Vec<usize> {
        let needle = self.narrow.text().to_lowercase();
        let mut out: Vec<usize> = (0..table.rows.len())
            .filter(|&i| {
                needle.is_empty()
                    || table.rows[i]
                        .cells
                        .iter()
                        .any(|c| c.to_lowercase().contains(&needle))
            })
            .collect();
        if let Some((col, reverse)) = self.sort {
            let key = |i: &usize| table.rows[*i].cells.get(col).cloned().unwrap_or_default();
            out.sort_by(|a, b| {
                let (x, y) = (key(a), key(b));
                let order = match (sort_number(&x), sort_number(&y)) {
                    (Some(p), Some(q)) => q.total_cmp(&p),
                    _ => x.to_lowercase().cmp(&y.to_lowercase()),
                };
                if reverse {
                    order.reverse()
                } else {
                    order
                }
            });
        }
        out
    }

    pub fn on_key(&mut self, key: KeyEvent, table: &Table) -> AnalysisKey {
        if key.kind == KeyEventKind::Release {
            return AnalysisKey::Other;
        }
        if self.editing {
            if key.code == KeyCode::Tab || key.code == KeyCode::Down {
                self.editing = false;
                return AnalysisKey::Moved;
            }
            return match self.query.on_key(key) {
                FieldKey::Submit => AnalysisKey::RunQuery(self.query.text().trim().to_string()),
                FieldKey::Cancel => AnalysisKey::Close,
                FieldKey::Edited => AnalysisKey::Moved,
                FieldKey::Other => AnalysisKey::Other,
            };
        }
        if self.narrowing {
            match self.narrow.on_key(key) {
                FieldKey::Submit => self.narrowing = false,
                FieldKey::Cancel => {
                    self.narrowing = false;
                    self.narrow = TextField::default();
                }
                FieldKey::Edited => self.selected = 0,
                FieldKey::Other => return AnalysisKey::Other,
            }
            return AnalysisKey::Moved;
        }
        let shown = self.shown(table);
        let last = shown.len().saturating_sub(1);
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return AnalysisKey::Close,
            KeyCode::Tab | KeyCode::BackTab if self.kind == Kind::Query => self.editing = true,
            KeyCode::Up | KeyCode::Char('k') if self.selected == 0 && self.kind == Kind::Query => {
                self.editing = true
            }
            KeyCode::Up | KeyCode::Char('k') => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => self.selected = (self.selected + 1).min(last),
            KeyCode::PageUp => self.selected = self.selected.saturating_sub(self.page.max(1)),
            KeyCode::PageDown => self.selected = (self.selected + self.page.max(1)).min(last),
            KeyCode::Home | KeyCode::Char('g') => self.selected = 0,
            KeyCode::End | KeyCode::Char('G') => self.selected = last,
            KeyCode::Char('/') => self.narrowing = true,
            KeyCode::Char('s') => {
                let n = table.header.len().max(1);
                self.sort = match self.sort {
                    None => Some((0, false)),
                    Some((c, _)) if c + 1 >= n => None,
                    Some((c, _)) => Some((c + 1, false)),
                };
                self.selected = 0;
            }
            KeyCode::Char('S') => {
                if let Some((_, reverse)) = self.sort.as_mut() {
                    *reverse = !*reverse;
                }
            }
            KeyCode::Enter | KeyCode::Char('x') => {
                let exclude = key.code == KeyCode::Char('x') || ctrl;
                return match shown.get(self.selected) {
                    Some(&i) => AnalysisKey::Activate(table.rows[i].action.clone(), exclude),
                    None => AnalysisKey::Other,
                };
            }
            _ => return AnalysisKey::Other,
        }
        AnalysisKey::Moved
    }
}

/// A cell read as a number for sorting (`1,204`, `12.5%`, `+1.200`), `None` for text.
fn sort_number(cell: &str) -> Option<f64> {
    let t = cell
        .trim()
        .trim_end_matches('%')
        .trim_start_matches(['+', 'x', '×'])
        .replace(',', "");
    t.parse::<f64>().ok()
}

/// A count with thousands separators.
fn count(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// `part` of `whole` in percent, one decimal.
fn share(part: u64, whole: u64) -> String {
    if whole == 0 {
        return "-".into();
    }
    format!("{:.1}%", part as f64 * 100.0 / whole as f64)
}

/// A bar `cells` wide for `part` of `whole`.
pub fn bar(part: u64, whole: u64, cells: usize, ascii: bool) -> String {
    if whole == 0 {
        return String::new();
    }
    let filled = ((part as f64 / whole as f64) * cells as f64).round() as usize;
    let block = if ascii { "#" } else { "█" };
    block.repeat(filled.min(cells))
}

/// Cells of the sparkline of a value's or a pattern's count over time.
const SPARK_CELLS: usize = 16;

fn progress(lang: Language, p: f32, running: bool) -> Option<String> {
    running.then(|| txf(lang, "counting... {0}%", &[&format!("{:.0}", p * 100.0)]))
}

/// The table of `kind` over `engine` this frame.
pub fn build(kind: Kind, engine: &mut TailEngine, lang: Language, ascii: bool) -> Table {
    match kind {
        Kind::Stats => stats(engine, lang, ascii),
        Kind::Query => query(engine, lang),
        Kind::Patterns => patterns(engine, lang, ascii),
        Kind::Explain => explain(engine, lang),
        Kind::Operations => operations(engine, lang, ascii),
    }
}

fn stats(engine: &mut TailEngine, lang: Language, ascii: bool) -> Table {
    let Some(st) = engine.stats.as_mut() else {
        return Table {
            error: Some(
                tx(
                    lang,
                    "No statistics: Alt+A then Statistics asks for a field",
                )
                .into(),
            ),
            ..Table::default()
        };
    };
    let top = st.top();
    let r = &st.result;
    let mut summary = vec![txf(
        lang,
        "{0}: {1} lines, {2} with a value, {3} distinct values",
        &[
            &st.source.label(),
            &count(r.lines),
            &count(r.with_value),
            &format!(
                "{}{}",
                count(r.distinct() as u64),
                if r.capped { "+" } else { "" }
            ),
        ],
    )];
    if r.is_numeric() {
        let h = r.hist();
        let q = |x: f64| h.percentile(x).map_or("-".to_string(), |v| r.show(v));
        summary.push(format!(
            "min {}  p50 {}  p95 {}  p99 {}  max {}  {} {}",
            q(0.0),
            q(0.5),
            q(0.95),
            q(0.99),
            q(1.0),
            tx(lang, "mean"),
            h.mean().map_or("-".to_string(), |v| r.show(v)),
        ));
    }
    summary.extend(progress(lang, st.progress, !st.done()));
    if st.failed {
        summary.push(tx(lang, "The file could not be read").into());
    }
    let whole = r.with_value.max(1);
    let lvl = |v: &crate::field_stats::ValueStats, l: LogLevel| v.levels[l as usize];
    // On a timed stream each value's count per time slice, as the window's chart.
    let slices = r.slices.map(|s| s.count).filter(|&n| n > 1);
    let over_time = |v: &crate::field_stats::ValueStats| {
        slices.map(|n| crate::tui::spark::counts(&v.series(n), SPARK_CELLS, ascii))
    };
    let mut rows: Vec<Row> = top
        .iter()
        .map(|(value, v)| {
            let mut cells = vec![
                value.to_string(),
                count(v.count),
                share(v.count, whole),
                bar(v.count, whole, 16, ascii),
                count(lvl(v, LogLevel::Error) + lvl(v, LogLevel::Fatal)),
                count(lvl(v, LogLevel::Warn)),
            ];
            cells.extend(over_time(v));
            Row {
                cells,
                level: None,
                action: RowAction::Value(value.to_string()),
            }
        })
        .collect();
    if r.other.count > 0 {
        let mut cells = vec![
            tx(lang, "(other values)").into(),
            count(r.other.count),
            share(r.other.count, whole),
            bar(r.other.count, whole, 16, ascii),
            String::new(),
            String::new(),
        ];
        cells.extend(over_time(&r.other));
        rows.push(Row {
            cells,
            level: None,
            action: RowAction::None,
        });
    }
    let mut header = vec![
        tx(lang, "value").into(),
        tx(lang, "count").into(),
        "%".into(),
        String::new(),
        "ERROR".into(),
        "WARN".into(),
    ];
    let mut right = vec![false, true, true, false, true, true];
    if slices.is_some() {
        header.push(tx(lang, "over time").into());
        right.push(false);
    }
    Table {
        summary,
        header,
        right,
        rows,
        error: None,
    }
}

fn query(engine: &mut TailEngine, lang: Language) -> Table {
    let Some(q) = engine.query.as_ref() else {
        return Table {
            summary: vec![tx(
                lang,
                "Type a query, Enter runs it: where level=error | stats count() by path",
            )
            .into()],
            ..Table::default()
        };
    };
    let mut summary = Vec::new();
    summary.extend(progress(lang, q.progress, q.running()));
    match &q.result {
        None => Table {
            summary,
            ..Table::default()
        },
        Some(Err(e)) => Table {
            summary,
            error: Some(format!(
                "{} ({} {}): {}",
                crate::i18n::t(lang, e.key),
                tx(lang, "stage"),
                e.stage + 1,
                e.detail
            )),
            ..Table::default()
        },
        Some(Ok(t)) => {
            summary.push(txf(
                lang,
                "{0} rows from {1} lines{2}",
                &[
                    &count(t.rows.len() as u64),
                    &count(t.lines_read),
                    &if t.truncated {
                        format!(" ({})", tx(lang, "more rows left out"))
                    } else {
                        String::new()
                    },
                ],
            ));
            let right: Vec<bool> = (0..t.columns.len())
                .map(|c| {
                    !t.rows.is_empty()
                        && t.rows
                            .iter()
                            .take(50)
                            .all(|r| r.get(c).is_none_or(|v| sort_number(v).is_some()))
                })
                .collect();
            let rows = t
                .rows
                .iter()
                .enumerate()
                .map(|(i, r)| Row {
                    cells: r.clone(),
                    level: None,
                    action: t
                        .lines
                        .as_ref()
                        .and_then(|l| l.get(i))
                        .map_or(RowAction::None, |&l| RowAction::Line(l)),
                })
                .collect();
            Table {
                summary,
                header: t.columns.clone(),
                right,
                rows,
                error: None,
            }
        }
    }
}

fn patterns(engine: &mut TailEngine, lang: Language, ascii: bool) -> Table {
    let Some(p) = engine.patterns.as_ref() else {
        return Table::default();
    };
    let mut summary = vec![txf(
        lang,
        "{0} patterns over {1} entries",
        &[
            &count(p.counts.ids().count() as u64),
            &count(p.counts.entries),
        ],
    )];
    summary.extend(progress(lang, p.progress, p.running()));
    let Some(tree) = p.snapshot.clone() else {
        return Table {
            summary,
            ..Table::default()
        };
    };
    let whole = p.counts.entries.max(1);
    let mut ids: Vec<u32> = p.counts.ids().collect();
    ids.sort_by_key(|&i| std::cmp::Reverse(p.counts.per[i as usize].count));
    let rows = ids
        .into_iter()
        .map(|id| {
            let s = &p.counts.per[id as usize];
            let volume: Vec<u64> = s.buckets.iter().map(|&b| u64::from(b)).collect();
            Row {
                cells: vec![
                    count(s.count),
                    share(s.count, whole),
                    bar(s.count, whole, 10, ascii),
                    crate::tui::spark::counts(&volume, SPARK_CELLS, ascii),
                    count(s.first_line as u64 + 1),
                    tree.template(id).map(|t| t.text()).unwrap_or_default(),
                ],
                level: None,
                action: RowAction::Pattern(id),
            }
        })
        .collect();
    Table {
        summary,
        header: vec![
            tx(lang, "count").into(),
            "%".into(),
            String::new(),
            tx(lang, "over time").into(),
            tx(lang, "first").into(),
            tx(lang, "pattern").into(),
        ],
        right: vec![true, true, false, false, true, false],
        rows,
        error: None,
    }
}

fn explain(engine: &mut TailEngine, lang: Language) -> Table {
    let Some(e) = engine.explain.as_ref() else {
        return Table {
            error: Some(tx(lang, "No span: Alt+A then Explain asks for one").into()),
            ..Table::default()
        };
    };
    let (a, b) = e.sides.span;
    let mut summary = vec![txf(
        lang,
        "{0} -> {1} against the rest of the log",
        &[
            &crate::timestamp::format_millis(a),
            &crate::timestamp::format_millis(b),
        ],
    )];
    summary.extend(progress(lang, e.progress, e.running()));
    let Some(x) = e.result.as_ref() else {
        return Table {
            summary,
            ..Table::default()
        };
    };
    let span_ms = (b - a).max(1);
    summary.push(txf(
        lang,
        "{0} entries in the span ({1}/min), {2} in the rest ({3}/min)",
        &[
            &count(x.entries[0]),
            &format!("{:.1}", x.per_minute(0, span_ms)),
            &count(x.base_seen),
            &format!("{:.1}", x.per_minute(1, (x.time[1].1 - x.time[1].0).max(1))),
        ],
    ));
    let ratio = |r: Option<f64>| r.map_or("new".to_string(), |r| format!("x{r:.1}"));
    let mut rows: Vec<Row> = x
        .messages()
        .into_iter()
        .map(|(r, first)| Row {
            cells: vec![
                ratio(r.ratio),
                count(r.span),
                count(r.base),
                x.drain
                    .template(r.item)
                    .map(|t| t.text())
                    .unwrap_or_default(),
            ],
            level: None,
            action: if first == usize::MAX {
                RowAction::None
            } else {
                RowAction::Line(first)
            },
        })
        .collect();
    rows.extend(x.field_pairs().into_iter().map(|r| Row {
        cells: vec![
            ratio(r.ratio),
            count(r.span),
            count(r.base),
            format!("{}={}", r.item.0, r.item.1),
        ],
        level: None,
        action: RowAction::Pair(r.item.0.clone(), r.item.1.clone()),
    }));
    Table {
        summary,
        header: vec![
            tx(lang, "more").into(),
            tx(lang, "span").into(),
            tx(lang, "rest").into(),
            tx(lang, "message or field").into(),
        ],
        right: vec![true, true, true, false],
        rows,
        error: None,
    }
}

fn operations(engine: &mut TailEngine, lang: Language, ascii: bool) -> Table {
    let Some(o) = engine.operations.as_ref() else {
        return Table {
            error: Some(
                tx(
                    lang,
                    "No operations: Alt+A then Operations asks for a field",
                )
                .into(),
            ),
            ..Table::default()
        };
    };
    let map = &o.map;
    let mut summary = vec![txf(
        lang,
        "{0} operations by {1}",
        &[&count(map.len() as u64), &o.source.label()],
    )];
    summary.extend(progress(lang, o.progress, !o.done()));
    let span = map.span();
    let list = crate::operations::listed(map, "", 0, crate::operations::OpSort::Start);
    let rows = list
        .into_iter()
        .map(|i| {
            let op = &map.ops[i];
            let gantt = match (span, op.timed()) {
                (Some((s, e)), true) => {
                    let width = 20usize;
                    let total = (e - s).max(1) as f64;
                    let from = (((op.start - s) as f64 / total) * width as f64) as usize;
                    let to = ((((op.end - s) as f64 / total) * width as f64).ceil() as usize)
                        .max(from + 1)
                        .min(width);
                    let block = if ascii { '#' } else { '█' };
                    let mut bar = " ".repeat(from.min(width));
                    bar.extend(std::iter::repeat_n(block, to.saturating_sub(from)));
                    bar
                }
                _ => String::new(),
            };
            Row {
                cells: vec![
                    op.id.to_string(),
                    op.name.to_string(),
                    if op.timed() {
                        crate::timestamp::format_millis(op.start)
                    } else {
                        String::new()
                    },
                    op.duration()
                        .map(crate::timestamp::format_delta)
                        .unwrap_or_default(),
                    count(op.lines),
                    LogLevel::from_u8(op.worst).short().to_string(),
                    gantt,
                ],
                level: Some(LogLevel::from_u8(op.worst)),
                action: RowAction::Line(op.first_line),
            }
        })
        .collect();
    Table {
        summary,
        header: vec![
            "id".into(),
            tx(lang, "name").into(),
            tx(lang, "start").into(),
            tx(lang, "duration").into(),
            tx(lang, "lines").into(),
            tx(lang, "level").into(),
            String::new(),
        ],
        right: vec![false, false, false, true, true, false, false],
        rows,
        error: None,
    }
}

/// A Statistics or Operations source as typed: `/regex/` for the first capture group of
/// a regex, else a field name.
pub fn parse_source(text: &str) -> Option<crate::field_stats::StatsSource> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    match t.strip_prefix('/').and_then(|r| r.strip_suffix('/')) {
        Some(re) => regex::Regex::new(re)
            .ok()
            .map(crate::field_stats::StatsSource::Capture),
        None => Some(crate::field_stats::StatsSource::Field(t.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn table() -> Table {
        let row = |a: &str, n: &str| Row {
            cells: vec![a.into(), n.into()],
            level: None,
            action: RowAction::Value(a.into()),
        };
        Table {
            header: vec!["value".into(), "count".into()],
            right: vec![false, true],
            rows: vec![row("beta", "2"), row("alpha", "1,200"), row("gamma", "30")],
            ..Table::default()
        }
    }

    #[test]
    fn sorting_narrowing_and_activating() {
        let t = table();
        let mut v = AnalysisView::new(Kind::Stats, "a.log".into());
        assert_eq!(v.shown(&t), vec![0, 1, 2]);
        v.on_key(key(KeyCode::Char('s')), &t);
        assert_eq!(v.shown(&t), vec![1, 0, 2], "by value");
        v.on_key(key(KeyCode::Char('s')), &t);
        assert_eq!(v.shown(&t), vec![1, 2, 0], "by count, largest first");
        v.on_key(key(KeyCode::Char('S')), &t);
        assert_eq!(v.shown(&t), vec![0, 2, 1]);
        v.on_key(key(KeyCode::Char('/')), &t);
        for c in "gam".chars() {
            v.on_key(key(KeyCode::Char(c)), &t);
        }
        v.on_key(key(KeyCode::Enter), &t);
        assert_eq!(v.shown(&t), vec![2]);
        assert_eq!(
            v.on_key(key(KeyCode::Char('x')), &t),
            AnalysisKey::Activate(RowAction::Value("gamma".into()), true)
        );
    }

    #[test]
    fn sources_are_fields_or_slash_regexes() {
        assert!(matches!(
            parse_source("status"),
            Some(crate::field_stats::StatsSource::Field(k)) if k == "status"
        ));
        assert!(matches!(
            parse_source("/took=(\\d+)/"),
            Some(crate::field_stats::StatsSource::Capture(_))
        ));
        assert!(parse_source("/(/").is_none());
        assert!(parse_source(" ").is_none());
    }
}
