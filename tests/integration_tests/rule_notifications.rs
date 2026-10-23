// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use super::*;
use fasttail::notifications::{Center, Options, Texts};

const TEXTS: Texts<'static> = Texts {
    more: "+{0} more",
    summary: "{0} more matches of {1}",
};

fn notify_rule(pattern: &str) -> HighlightRule {
    let mut rule = HighlightRule::new(pattern, [255, 0, 0], [0, 0, 0], false);
    rule.notify = true;
    rule
}

#[test]
fn test_notify_hits_only_for_appended_lines_of_notify_rules() {
    let mut tmp = NamedTempFile::new().unwrap();
    writeln!(tmp, "ERROR already there").unwrap();
    tmp.flush().unwrap();
    let mut engine = TailEngine::open(tmp.path()).unwrap();
    let mut off = notify_rule("WARN");
    off.enabled = false;
    engine.set_highlight_rules(vec![
        notify_rule("ERROR"),
        off,
        HighlightRule::new("INFO", [0, 255, 0], [0, 0, 0], false),
    ]);
    engine.poll_updates();
    // The lines present at open never notify.
    assert!(engine.pending_notify_hits.is_empty());

    writeln!(tmp, "INFO fine").unwrap();
    writeln!(tmp, "WARN disabled rule").unwrap();
    writeln!(tmp, "ERROR disk full").unwrap();
    tmp.flush().unwrap();
    engine.poll_updates();
    let hits = std::mem::take(&mut engine.pending_notify_hits);
    assert_eq!(hits.len(), 1);
    assert_eq!((hits[0].rule.as_str(), hits[0].line), ("ERROR", 3));
    assert_eq!(hits[0].text, "ERROR disk full");
}

#[test]
fn test_notify_queue_is_capped_with_an_overflow_count() {
    let mut tmp = NamedTempFile::new().unwrap();
    writeln!(tmp, "start").unwrap();
    tmp.flush().unwrap();
    let mut engine = TailEngine::open(tmp.path()).unwrap();
    engine.set_highlight_rules(vec![notify_rule("ERROR")]);
    let extra = 10;
    for i in 0..fasttail::tail_engine::MAX_PENDING_NOTIFY_HITS + extra {
        writeln!(tmp, "ERROR {i}").unwrap();
    }
    tmp.flush().unwrap();
    engine.poll_updates();
    assert_eq!(
        engine.pending_notify_hits.len(),
        fasttail::tail_engine::MAX_PENDING_NOTIFY_HITS
    );
    assert_eq!(engine.notify_overflow, extra as u64);
}

#[test]
fn test_drain_drops_hits_when_switched_off_or_the_stream_is_seen() {
    let mut tmp = NamedTempFile::new().unwrap();
    writeln!(tmp, "start").unwrap();
    tmp.flush().unwrap();
    let mut engine = TailEngine::open(tmp.path()).unwrap();
    engine.set_highlight_rules(vec![notify_rule("ERROR")]);
    let mut center = Center::new(std::sync::Arc::new(|| {}));
    let now = std::time::Instant::now();
    let mut append = |engine: &mut TailEngine| {
        writeln!(tmp, "ERROR boom").unwrap();
        tmp.flush().unwrap();
        engine.poll_updates();
        assert_eq!(engine.pending_notify_hits.len(), 1);
    };

    // Switched off: the queue is emptied, nothing shown.
    append(&mut engine);
    let off = Options {
        enabled: false,
        always: true,
        focused: false,
    };
    assert_eq!(center.drain([&mut engine], &off, &TEXTS, now), 0);
    assert!(engine.pending_notify_hits.is_empty());

    // `background`: the stream on screen in the focused window is not notified.
    append(&mut engine);
    engine.displayed = true;
    let background = Options {
        enabled: true,
        always: false,
        focused: true,
    };
    assert_eq!(center.drain([&mut engine], &background, &TEXTS, now), 0);
    assert!(engine.pending_notify_hits.is_empty());
    assert!(!center.has_held());
}

#[test]
fn test_rule_notify_flag_round_trips_through_the_ini() {
    let config = FastTailConfig {
        highlight_rules: vec![
            notify_rule("ERROR"),
            HighlightRule::new("INFO", [0, 255, 0], [0, 0, 0], false),
        ],
        ..Default::default()
    };
    let loaded = FastTailConfig::from_ini(&config.to_ini());
    let flags: Vec<bool> = loaded.highlight_rules.iter().map(|r| r.notify).collect();
    assert_eq!(flags, vec![true, false]);
}
