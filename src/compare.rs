// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Comparing two log lines or two lists of lines (no interface here).
//!
//! A line is cut into tokens (words, spaces, single punctuation characters, a leading
//! timestamp, UUIDs); each token is compared by a key in which the ignore options have
//! replaced what should not count (the timestamp, numbers, hex ids, the amount of
//! whitespace, case). The keys are diffed with `similar` and the result is mapped back
//! to the original byte ranges, so the view highlights the text as written. Lists of
//! lines are diffed line by line on the same keys, with a word diff inside changed rows.

use similar::{Algorithm, DiffOp};
use std::ops::Range;
use std::time::{Duration, Instant};

/// Lines per side a region compare takes at most.
pub const MAX_REGION_LINES: usize = 20_000;
/// Time a region diff may take before a coarser result is used.
pub const REGION_DEADLINE: Duration = Duration::from_secs(2);

/// What a stream's row menu asked for (applied by the app).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareRequest {
    /// The two selected rows against each other.
    SelectedPair,
    /// The selected rows become the marked side.
    Mark,
    /// The selected rows against the marked side.
    WithMark,
}

/// What the compare ignores. Only the leading timestamp is ignored by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompareOptions {
    pub timestamp: bool,
    pub numbers: bool,
    pub ids: bool,
    pub whitespace: bool,
    pub case: bool,
    /// Pretty-print a JSON object in each line (sorted keys) and compare the result.
    pub json: bool,
}

impl Default for CompareOptions {
    fn default() -> Self {
        Self {
            timestamp: true,
            numbers: false,
            ids: false,
            whitespace: false,
            case: false,
            json: false,
        }
    }
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn is_uuid(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 36
        && b[..36].iter().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => *c == b'-',
            _ => c.is_ascii_hexdigit(),
        })
}

/// A hex id: at least 8 hex digits with a digit and a letter among them.
fn is_hex_id(word: &str) -> bool {
    word.len() >= 8
        && word.bytes().all(|b| b.is_ascii_hexdigit())
        && word.bytes().any(|b| b.is_ascii_digit())
        && word.bytes().any(|b| b.is_ascii_alphabetic())
}

/// `word` with every run of digits replaced by `⟨n⟩` (`5ms` → `⟨n⟩ms`).
fn without_numbers(word: &str) -> String {
    let mut out = String::new();
    let mut in_digits = false;
    for c in word.chars() {
        if c.is_ascii_digit() {
            if !in_digits {
                out.push_str("⟨n⟩");
            }
            in_digits = true;
        } else {
            out.push(c);
            in_digits = false;
        }
    }
    out
}

/// The tokens of `line`: byte range and comparison key.
pub fn tokens(line: &str, opts: &CompareOptions) -> Vec<(Range<usize>, String)> {
    let mut out = Vec::new();
    let mut at = 0;
    if opts.timestamp {
        if let Some(end) =
            crate::timestamp::leading_span(line, crate::timestamp::FormatHint::Unknown)
        {
            if end > 0 && line.is_char_boundary(end) {
                out.push((0..end, "⟨ts⟩".to_string()));
                at = end;
            }
        }
    }
    let fold = |s: &str| {
        if opts.case {
            s.to_lowercase()
        } else {
            s.to_string()
        }
    };
    while at < line.len() {
        let rest = &line[at..];
        if is_uuid(rest) {
            let key = if opts.ids {
                "⟨id⟩".to_string()
            } else {
                fold(&rest[..36])
            };
            out.push((at..at + 36, key));
            at += 36;
            continue;
        }
        let c = rest.chars().next().expect("not at the end");
        let len = if is_word_char(c) {
            rest.find(|c: char| !is_word_char(c)).unwrap_or(rest.len())
        } else if c.is_whitespace() {
            rest.find(|c: char| !c.is_whitespace())
                .unwrap_or(rest.len())
        } else {
            c.len_utf8()
        };
        let text = &rest[..len];
        let key = if c.is_whitespace() {
            if opts.whitespace {
                " ".to_string()
            } else {
                text.to_string()
            }
        } else if opts.ids && is_hex_id(text) {
            "⟨id⟩".to_string()
        } else if opts.numbers && text.bytes().any(|b| b.is_ascii_digit()) {
            fold(&without_numbers(text))
        } else {
            fold(text)
        };
        out.push((at..at + len, key));
        at += len;
    }
    out
}

/// The key a whole line is compared by in a region diff.
fn line_key(line: &str, opts: &CompareOptions) -> String {
    tokens(line, opts)
        .into_iter()
        .map(|(_, key)| key)
        .collect::<Vec<_>>()
        .join("\u{1}")
}

/// The word diff of two lines: the changed byte ranges of each side.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WordDiff {
    pub left: Vec<Range<usize>>,
    pub right: Vec<Range<usize>>,
}

impl WordDiff {
    pub fn is_equal(&self) -> bool {
        self.left.is_empty() && self.right.is_empty()
    }
}

fn push_range(ranges: &mut Vec<Range<usize>>, range: Range<usize>) {
    if range.is_empty() {
        return;
    }
    match ranges.last_mut() {
        Some(last) if last.end == range.start => last.end = range.end,
        _ => ranges.push(range),
    }
}

/// Compares two lines token by token.
pub fn diff_words(a: &str, b: &str, opts: &CompareOptions) -> WordDiff {
    let ta = tokens(a, opts);
    let tb = tokens(b, opts);
    let ka: Vec<&str> = ta.iter().map(|(_, k)| k.as_str()).collect();
    let kb: Vec<&str> = tb.iter().map(|(_, k)| k.as_str()).collect();
    let mut diff = WordDiff::default();
    for op in similar::capture_diff_slices(Algorithm::Myers, &ka, &kb) {
        match op {
            DiffOp::Equal { .. } => {}
            DiffOp::Delete {
                old_index, old_len, ..
            } => {
                for (r, _) in &ta[old_index..old_index + old_len] {
                    push_range(&mut diff.left, r.clone());
                }
            }
            DiffOp::Insert {
                new_index, new_len, ..
            } => {
                for (r, _) in &tb[new_index..new_index + new_len] {
                    push_range(&mut diff.right, r.clone());
                }
            }
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => {
                for (r, _) in &ta[old_index..old_index + old_len] {
                    push_range(&mut diff.left, r.clone());
                }
                for (r, _) in &tb[new_index..new_index + new_len] {
                    push_range(&mut diff.right, r.clone());
                }
            }
        }
    }
    diff
}

/// What a row of a region diff shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    Equal,
    Changed,
    Removed,
    Added,
}

/// One aligned row: the index of its line on each side (`None` on the side without one).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub kind: RowKind,
    pub left: Option<usize>,
    pub right: Option<usize>,
    /// Word highlights of a changed row.
    pub words: Option<WordDiff>,
}

/// The diff of two lists of lines.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RegionDiff {
    pub rows: Vec<Row>,
    /// Rows where a block of changes starts (for `F7` / `SHIFT + F7`).
    pub changes: Vec<usize>,
    /// The deadline was reached: the result is correct but may be less minimal.
    pub coarse: bool,
}

/// Compares two lists of lines (each capped by the caller at `MAX_REGION_LINES`).
pub fn diff_regions(a: &[String], b: &[String], opts: &CompareOptions) -> RegionDiff {
    let ka: Vec<String> = a.iter().map(|l| line_key(l, opts)).collect();
    let kb: Vec<String> = b.iter().map(|l| line_key(l, opts)).collect();
    let started = Instant::now();
    let ops = similar::capture_diff_deadline(
        Algorithm::Myers,
        &ka,
        0..ka.len(),
        &kb,
        0..kb.len(),
        Some(started + REGION_DEADLINE),
    );
    let mut diff = RegionDiff {
        coarse: started.elapsed() >= REGION_DEADLINE,
        ..Default::default()
    };
    let start_change = |rows: &Vec<Row>, changes: &mut Vec<usize>| {
        changes.push(rows.len());
    };
    for op in ops {
        match op {
            DiffOp::Equal {
                old_index,
                new_index,
                len,
            } => {
                for i in 0..len {
                    diff.rows.push(Row {
                        kind: RowKind::Equal,
                        left: Some(old_index + i),
                        right: Some(new_index + i),
                        words: None,
                    });
                }
            }
            DiffOp::Delete {
                old_index, old_len, ..
            } => {
                start_change(&diff.rows, &mut diff.changes);
                for i in 0..old_len {
                    diff.rows.push(Row {
                        kind: RowKind::Removed,
                        left: Some(old_index + i),
                        right: None,
                        words: None,
                    });
                }
            }
            DiffOp::Insert {
                new_index, new_len, ..
            } => {
                start_change(&diff.rows, &mut diff.changes);
                for i in 0..new_len {
                    diff.rows.push(Row {
                        kind: RowKind::Added,
                        left: None,
                        right: Some(new_index + i),
                        words: None,
                    });
                }
            }
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => {
                start_change(&diff.rows, &mut diff.changes);
                let paired = old_len.min(new_len);
                for i in 0..paired {
                    let (l, r) = (old_index + i, new_index + i);
                    diff.rows.push(Row {
                        kind: RowKind::Changed,
                        left: Some(l),
                        right: Some(r),
                        words: Some(diff_words(&a[l], &b[r], opts)),
                    });
                }
                for i in paired..old_len {
                    diff.rows.push(Row {
                        kind: RowKind::Removed,
                        left: Some(old_index + i),
                        right: None,
                        words: None,
                    });
                }
                for i in paired..new_len {
                    diff.rows.push(Row {
                        kind: RowKind::Added,
                        left: None,
                        right: Some(new_index + i),
                        words: None,
                    });
                }
            }
        }
    }
    diff
}

/// A JSON object or array in `line` (from its first `{` or `[` to the end), pretty-printed
/// with sorted keys, one line per entry; `None` when there is none.
pub fn canonical_json(line: &str) -> Option<Vec<String>> {
    let start = line.find(['{', '['])?;
    let value: serde_json::Value = serde_json::from_str(line[start..].trim_end()).ok()?;
    if !(value.is_object() || value.is_array()) {
        return None;
    }
    // `serde_json` keeps object keys sorted (no `preserve_order`).
    let pretty = serde_json::to_string_pretty(&value).ok()?;
    Some(pretty.lines().map(str::to_string).collect())
}

/// The compare as `diff -u` text, with `left` / `right` as the file names.
pub fn unified(left_name: &str, right_name: &str, a: &[String], b: &[String]) -> String {
    let join = |lines: &[String]| {
        let mut text = lines.join("\n");
        text.push('\n');
        text
    };
    let (left, right) = (join(a), join(b));
    similar::TextDiff::from_lines(&left, &right)
        .unified_diff()
        .context_radius(3)
        .header(left_name, right_name)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|l| l.to_string()).collect()
    }

    #[test]
    fn one_changed_token_in_a_long_line_is_all_that_is_highlighted() {
        let a = "2026-09-18T14:02:05.100Z INFO payment id=7f3a amount=120 currency=EUR status=ok";
        let b =
            "2026-09-18T14:09:41.200Z INFO payment id=7f3a amount=120 currency=EUR status=failed";
        let diff = diff_words(a, b, &CompareOptions::default());
        assert_eq!(diff.left, vec![a.find("ok").unwrap()..a.len()]);
        assert_eq!(diff.right, vec![b.find("failed").unwrap()..b.len()]);
        // Without ignoring the timestamp, it differs too.
        let strict = CompareOptions {
            timestamp: false,
            ..Default::default()
        };
        assert!(diff_words(a, b, &strict).left.len() > 1);
    }

    #[test]
    fn ignore_options_hide_numbers_ids_whitespace_and_case() {
        let a =
            "req 1234 id=deadbeef01 took   5ms  USER=bob uuid 123e4567-e89b-12d3-a456-426614174000";
        let b =
            "req 9876 id=cafebabe99 took 7ms USER=Bob uuid 00000000-e89b-12d3-a456-426614174999";
        let all = CompareOptions {
            numbers: true,
            ids: true,
            whitespace: true,
            case: true,
            ..Default::default()
        };
        assert!(
            diff_words(a, b, &all).is_equal(),
            "{:?}",
            diff_words(a, b, &all)
        );
        assert!(!diff_words(a, b, &CompareOptions::default()).is_equal());
    }

    #[test]
    fn json_with_reordered_keys_differs_only_in_the_changed_value() {
        let a = r#"INFO body {"b":2,"a":{"x":1,"y":"same"}}"#;
        let b = r#"INFO body {"a":{"y":"same","x":9},"b":2}"#;
        let (ja, jb) = (canonical_json(a).unwrap(), canonical_json(b).unwrap());
        let diff = diff_regions(&ja, &jb, &CompareOptions::default());
        let changed: Vec<&Row> = diff
            .rows
            .iter()
            .filter(|r| r.kind != RowKind::Equal)
            .collect();
        assert_eq!(changed.len(), 1, "{:?}", diff.rows);
        assert_eq!(ja[changed[0].left.unwrap()].trim(), "\"x\": 1,");
        assert!(canonical_json("no json here").is_none());
    }

    #[test]
    fn regions_align_inserted_removed_and_changed_lines() {
        let a = texts(&["start", "step 1", "step 2", "old only", "end"]);
        let b = texts(&["start", "step 1", "inserted", "step 2 changed", "end"]);
        let diff = diff_regions(&a, &b, &CompareOptions::default());
        let kinds: Vec<RowKind> = diff.rows.iter().map(|r| r.kind).collect();
        assert_eq!(kinds.first(), Some(&RowKind::Equal));
        assert_eq!(kinds.last(), Some(&RowKind::Equal));
        assert!(kinds.contains(&RowKind::Changed) || kinds.contains(&RowKind::Added));
        assert!(!diff.changes.is_empty());
        assert!(!diff.coarse);
        // Every line of both sides appears once, in order.
        let left: Vec<usize> = diff.rows.iter().filter_map(|r| r.left).collect();
        let right: Vec<usize> = diff.rows.iter().filter_map(|r| r.right).collect();
        assert_eq!(left, (0..a.len()).collect::<Vec<_>>());
        assert_eq!(right, (0..b.len()).collect::<Vec<_>>());
    }

    #[test]
    fn unified_diff_text() {
        let a = texts(&["one", "two", "three"]);
        let b = texts(&["one", "2", "three"]);
        let text = unified("a.log:1-3", "b.log:1-3", &a, &b);
        assert!(text.starts_with("--- a.log:1-3\n+++ b.log:1-3\n"), "{text}");
        assert!(text.contains("-two\n+2\n"), "{text}");
    }
}
