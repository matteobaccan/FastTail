// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use fasttail::i18n::Language;
use fasttail::tail_engine::{HighlightRule, RuleNavError, RuleSeekStep, TailEngine};
use std::io::Write;
use std::time::Duration;

const LONG: Duration = Duration::from_secs(10);

fn engine(lines: &[&str]) -> (tempfile::NamedTempFile, TailEngine) {
    let mut tmp = tempfile::NamedTempFile::new().unwrap();
    for l in lines {
        writeln!(tmp, "{l}").unwrap();
    }
    tmp.flush().unwrap();
    let mut e = TailEngine::open(tmp.path()).unwrap();
    e.set_highlight_rules(vec![
        HighlightRule::new("ERROR", [255, 0, 0], [0, 0, 0], false),
        HighlightRule::new(r"duration_ms=\d{4,}", [255, 255, 0], [0, 0, 0], true),
    ]);
    (tmp, e)
}

fn walk(e: &mut TailEngine) -> RuleSeekStep {
    e.step_rule_seek(LONG, Language::En).unwrap()
}

const LOG: &[&str] = &[
    "0 INFO start",
    "1 query duration_ms=12",
    "2 query duration_ms=4500",
    "3 ERROR boom",
    "4 query duration_ms=9000",
    "5 INFO idle",
    "6 query duration_ms=7000",
];

#[test]
fn next_and_previous_line_of_the_picked_rule() {
    let (_tmp, mut e) = engine(LOG);
    assert_eq!(
        e.rules_matching("2 query duration_ms=4500"),
        vec![(1, r"duration_ms=\d{4,}".to_string())]
    );
    // Picked in the row menu on line 2.
    e.start_rule_seek(Some(1), true, 2).unwrap();
    assert_eq!(
        walk(&mut e),
        RuleSeekStep::Found {
            line: 4,
            wrapped: false
        }
    );
    assert_eq!(e.selection_anchor, Some(4));
    assert!(!e.follow_tail);
    e.start_rule_seek(None, true, 4).unwrap();
    assert_eq!(
        walk(&mut e),
        RuleSeekStep::Found {
            line: 6,
            wrapped: false
        }
    );
    // Past the end: wraps once.
    e.start_rule_seek(None, true, 6).unwrap();
    assert_eq!(
        walk(&mut e),
        RuleSeekStep::Found {
            line: 2,
            wrapped: true
        }
    );
    // Backwards.
    e.start_rule_seek(None, false, 2).unwrap();
    assert_eq!(
        walk(&mut e),
        RuleSeekStep::Found {
            line: 6,
            wrapped: true
        }
    );
    e.start_rule_seek(None, false, 6).unwrap();
    assert_eq!(
        walk(&mut e),
        RuleSeekStep::Found {
            line: 4,
            wrapped: false
        }
    );
}

#[test]
fn without_a_picked_rule_the_first_matching_rule_is_used() {
    let (_tmp, mut e) = engine(LOG);
    assert_eq!(e.rule_cursor(), None);
    e.start_rule_seek(None, true, 3).unwrap();
    assert_eq!(e.rule_cursor(), Some(0));
    // ERROR only on line 3: after a whole walk it is found again, wrapped.
    assert_eq!(
        walk(&mut e),
        RuleSeekStep::Found {
            line: 3,
            wrapped: true
        }
    );
    // A row no rule matches, without a navigation rule.
    let (_tmp2, mut f) = engine(LOG);
    assert_eq!(
        f.start_rule_seek(None, true, 0),
        Err(RuleNavError::NoRuleOnRow)
    );
    assert_eq!(f.step_rule_seek(LONG, Language::En), None);
}

#[test]
fn filtered_and_collapsed_rows() {
    let (_tmp, mut e) = engine(LOG);
    // The filter hides line 4: the walk goes from 2 to 6.
    e.set_exclude_filter("9000");
    e.start_rule_seek(Some(1), true, 2).unwrap();
    assert_eq!(
        walk(&mut e),
        RuleSeekStep::Found {
            line: 6,
            wrapped: false
        }
    );
    // A start the filters hide: the next visible line after it.
    e.start_rule_seek(None, true, 4).unwrap();
    assert_eq!(
        walk(&mut e),
        RuleSeekStep::Found {
            line: 6,
            wrapped: false
        }
    );
    e.start_rule_seek(None, false, 4).unwrap();
    assert_eq!(
        walk(&mut e),
        RuleSeekStep::Found {
            line: 2,
            wrapped: false
        }
    );
}

#[test]
fn hidden_in_a_collapsed_group_is_revealed() {
    let lines = [
        "a INFO x",
        "b ERROR same",
        "b ERROR same",
        "b ERROR same",
        "c INFO y",
    ];
    let (_tmp, mut e) = engine(&lines);
    e.set_collapse_mode(fasttail::collapse::CollapseMode::Exact);
    e.start_rule_seek(Some(0), true, 2).unwrap();
    assert_eq!(
        walk(&mut e),
        RuleSeekStep::Found {
            line: 3,
            wrapped: false
        }
    );
    assert_eq!(e.pending_jump, Some(3));
}

#[test]
fn walk_resumes_after_the_budget_and_can_be_cancelled() {
    let mut lines: Vec<String> = (0..50_000).map(|i| format!("{i} INFO filler")).collect();
    lines.push("50000 ERROR at last".into());
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    let (_tmp, mut e) = engine(&refs);
    e.start_rule_seek(Some(0), true, 0).unwrap();
    let mut frames = 0;
    let found = loop {
        frames += 1;
        match e.step_rule_seek(Duration::ZERO, Language::En).unwrap() {
            RuleSeekStep::Pending => {
                assert!(e.rule_seek_pattern().is_some());
                continue;
            }
            other => break other,
        }
    };
    assert!(frames > 1, "a zero budget must take several frames");
    assert_eq!(
        found,
        RuleSeekStep::Found {
            line: 50_000,
            wrapped: false
        }
    );
    assert_eq!(e.rule_seek_pattern(), None);

    e.start_rule_seek(None, true, 0).unwrap();
    assert_eq!(
        e.step_rule_seek(Duration::ZERO, Language::En),
        Some(RuleSeekStep::Pending)
    );
    assert!(e.cancel_rule_seek());
    assert_eq!(e.step_rule_seek(LONG, Language::En), None);
}

#[test]
fn no_line_matches_and_rules_change() {
    let (_tmp, mut e) = engine(LOG);
    e.set_exclude_filter("ERROR");
    // The rule picked on a row the filter now hides: nothing shown matches it.
    e.start_rule_seek(Some(0), true, 0).unwrap();
    assert_eq!(walk(&mut e), RuleSeekStep::NotFound);
    assert_eq!(e.rule_cursor(), Some(0));
    // A change of the rules forgets the navigation rule.
    e.set_highlight_rules(vec![HighlightRule::new(
        "INFO",
        [1, 1, 1],
        [0, 0, 0],
        false,
    )]);
    assert_eq!(e.rule_cursor(), None);
}
