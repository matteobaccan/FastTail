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

#[test]
fn the_next_line_of_the_rows_operation_follows_its_request_id() {
    use fasttail::operation_jump::OpJump;
    let mut f = NamedTempFile::new().unwrap();
    write!(
        f,
        "level=info request_id=a msg=start\n\
         level=info request_id=b msg=start\n\
         plain line\n\
         level=warn request_id=a msg=slow\n\
         level=info request_id=b msg=end\n\
         level=info request_id=a msg=end\n"
    )
    .unwrap();
    f.flush().unwrap();
    let mut engine = TailEngine::open(f.path()).unwrap();
    let start = std::time::Instant::now();
    while engine.field_parser().is_none() && start.elapsed() < Duration::from_secs(5) {
        engine.poll_updates();
    }
    assert_eq!(
        engine.next_same_operation(0, true),
        OpJump::Found(3, "request_id=a".into())
    );
    assert_eq!(
        engine.next_same_operation(3, true),
        OpJump::Found(5, "request_id=a".into())
    );
    assert_eq!(
        engine.next_same_operation(5, true),
        OpJump::NotFound("request_id=a".into())
    );
    assert_eq!(
        engine.next_same_operation(4, false),
        OpJump::Found(1, "request_id=b".into())
    );
    assert_eq!(engine.next_same_operation(2, true), OpJump::NoId);
}
