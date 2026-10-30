// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use super::*;

fn settle(engine: &mut TailEngine) {
    let start = std::time::Instant::now();
    loop {
        engine.poll_updates();
        let busy = engine.index_pending
            || engine.named.list.iter().any(|n| n.running())
            || engine.named.list.iter().any(|n| n.total() == 0);
        if !busy || start.elapsed() > Duration::from_secs(10) {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn kept_searches_are_highlighted_walked_and_saved_with_the_stream() {
    let mut f = NamedTempFile::new().unwrap();
    write!(f, "ok\ntimeout a\nok\nrefused b\ntimeout c\n").unwrap();
    f.flush().unwrap();
    let mut engine = TailEngine::open(f.path()).unwrap();
    assert_eq!(engine.keep_search("timeout"), Ok(1));
    assert_eq!(engine.keep_search("refused"), Ok(2));
    settle(&mut engine);
    // Both are highlighted in their colours, as quick labels are, but are not the app's.
    assert!(engine.has_span_rules());
    assert!(engine.quick_labels().is_empty());
    // The walk goes through either.
    assert_eq!(engine.next_named(0, true), Some((1, 0)));
    assert_eq!(engine.next_named(1, true), Some((3, 1)));
    assert!(!engine.follow_tail);
    // Saved with the stream and put back.
    let entry = fasttail::workspace::stream_entry(&engine);
    assert_eq!(
        entry.kept,
        vec![(1, "timeout".to_string()), (2, "refused".to_string())]
    );
    let mut again = TailEngine::open(f.path()).unwrap();
    again.restore_named(&entry.kept);
    settle(&mut again);
    assert_eq!(again.named.list[0].hits(), &[1, 4]);
}
