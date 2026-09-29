// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use fasttail::tail_engine::{token_at, token_occurrences, TailEngine, MAX_TOKEN_OUTLINES};
use std::io::Write;

fn pick(text: &str, at: &str) -> Option<String> {
    let byte = text.find(at).unwrap();
    token_at(text, byte).map(|(s, e)| text[s..e].to_string())
}

#[test]
fn token_around_the_pointer() {
    let uuid_line = "id=550e8400-e29b-41d4-a716-446655440000 done";
    // `=` is not a token character: the UUID stands alone.
    assert_eq!(
        pick(uuid_line, "a716").as_deref(),
        Some("550e8400-e29b-41d4-a716-446655440000")
    );
    assert_eq!(
        pick("connecting to 10.0.4.17:8443.", "4.17").as_deref(),
        Some("10.0.4.17:8443")
    );
    assert_eq!(
        pick("open /var/log/app.log: denied", "log/app").as_deref(),
        Some("/var/log/app.log")
    );
    assert_eq!(
        pick("user bob@example.com ok", "example").as_deref(),
        Some("bob@example.com")
    );
    assert_eq!(pick("req-7f3a failed", "7f3a").as_deref(), Some("req-7f3a"));
    assert_eq!(pick("città: Udine", "tà").as_deref(), Some("città"));
}

#[test]
fn pointer_just_past_a_word_and_on_blanks() {
    let text = "alpha  beta";
    // On the space right after `alpha`: the word before the pointer.
    assert_eq!(token_at(text, 5), Some((0, 5)));
    // Between two spaces: nothing.
    assert_eq!(token_at(text, 6), None);
    // A single character, a run of punctuation that trims to nothing, too long.
    assert_eq!(token_at("a b", 0), None);
    assert_eq!(token_at("x ..:: y", 3), None);
    let long = "a".repeat(300);
    assert_eq!(token_at(&long, 10), None);
    let edge = "b".repeat(256);
    assert_eq!(token_at(&edge, 0), Some((0, 256)));
    assert_eq!(token_at("", 0), None);
    assert_eq!(token_at("abc", 99), Some((0, 3)));
}

#[test]
fn occurrences_are_exact_and_case_sensitive() {
    let row = "req-7f3a start; REQ-7F3A other; req-7f3a end; xreq-7f3ay";
    let hits = token_occurrences(row, "req-7f3a");
    assert_eq!(hits.len(), 3);
    for (s, e) in &hits {
        assert_eq!(&row[*s..*e], "req-7f3a");
    }
    assert!(token_occurrences(row, "").is_empty());
    let many = "ab ".repeat(500);
    assert_eq!(token_occurrences(&many, "ab").len(), MAX_TOKEN_OUTLINES);
}

#[test]
fn toggling_and_clearing() {
    let mut tmp = tempfile::NamedTempFile::new().unwrap();
    writeln!(tmp, "req-7f3a one").unwrap();
    tmp.flush().unwrap();
    let mut e = TailEngine::open(tmp.path()).unwrap();
    assert_eq!(e.selection_token(), None);
    e.toggle_selection_token(Some("req-7f3a"));
    assert_eq!(e.selection_token(), Some("req-7f3a"));
    // Another token replaces it; the same one again clears it.
    e.toggle_selection_token(Some("one"));
    assert_eq!(e.selection_token(), Some("one"));
    e.toggle_selection_token(Some("one"));
    assert_eq!(e.selection_token(), None);
    // Empty space clears it.
    e.toggle_selection_token(Some("one"));
    e.toggle_selection_token(None);
    assert_eq!(e.selection_token(), None);
    // Esc clears it.
    e.toggle_selection_token(Some("one"));
    assert!(e.clear_selection_token());
    assert!(!e.clear_selection_token());
    // A reload (the file rewritten shorter) clears it.
    e.toggle_selection_token(Some("one"));
    std::fs::write(tmp.path(), "x\n").unwrap();
    e.poll_updates();
    assert_eq!(e.selection_token(), None);
}
