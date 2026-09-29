use fasttail::ansi::{AnsiStyle, StyleRun};
use fasttail::auto_highlight::{TokenKind, TokenKinds};
use fasttail::config::FastTailConfig;
use fasttail::tail_engine::{HighlightRule, QuickLabel, SpanStyle, TailEngine, MAX_ROW_SPANS};
use std::io::Write;
use tempfile::NamedTempFile;

fn engine() -> (NamedTempFile, TailEngine) {
    let mut tmp = NamedTempFile::new().unwrap();
    writeln!(
        tmp,
        "10.0.4.17 GET https://api.example.com/v1/orders 503 in 1.25s"
    )
    .unwrap();
    tmp.flush().unwrap();
    let e = TailEngine::open(tmp.path()).unwrap();
    (tmp, e)
}

fn tokens(spans: &[fasttail::tail_engine::HighlightSpan], line: &str) -> Vec<(String, TokenKind)> {
    spans
        .iter()
        .filter_map(|s| match s.style {
            SpanStyle::Token(k) => Some((line[s.start..s.end].to_string(), k)),
            _ => None,
        })
        .collect()
}

#[test]
fn off_by_default_and_on_paints_the_access_log() {
    let (_tmp, mut e) = engine();
    let line = "10.0.4.17 GET https://api.example.com/v1/orders 503 in 1.25s";
    assert!(!e.has_span_rules());
    assert!(e.match_highlight_spans(line).spans.is_empty());
    e.set_auto_tokens(TokenKinds::ALL);
    assert!(e.has_span_rules());
    let hl = e.match_highlight_spans(line);
    assert_eq!(
        tokens(&hl.spans, line),
        vec![
            ("10.0.4.17".to_string(), TokenKind::Ip),
            (
                "https://api.example.com/v1/orders".to_string(),
                TokenKind::Url
            ),
            ("1.25s".to_string(), TokenKind::Duration),
        ]
    );
}

#[test]
fn rules_labels_and_ansi_win_over_tokens() {
    let (_tmp, mut e) = engine();
    e.set_auto_tokens(TokenKinds::ALL);
    let uuid_line = "id 550e8400-e29b-41d4-a716-446655440000 failed";
    // A whole-row rule keeps the UUID in its colour.
    e.set_highlight_rules(vec![HighlightRule::new(
        "failed",
        [255, 0, 0],
        [0, 0, 0],
        false,
    )]);
    let hl = e.match_highlight_spans(uuid_line);
    assert!(hl.rest.is_some());
    assert!(tokens(&hl.spans, uuid_line).is_empty());
    e.set_highlight_rules(Vec::new());
    // A quick label over the address wins; the duration still gets its colour.
    e.set_quick_labels(&[QuickLabel {
        text: "10.0.0.1".into(),
        color: 2,
    }]);
    let line = "from 10.0.0.1 in 5ms";
    let hl = e.match_highlight_spans(line);
    assert_eq!(
        tokens(&hl.spans, line),
        vec![("5ms".to_string(), TokenKind::Duration)]
    );
    e.set_quick_labels(&[]);
    // ANSI colours of the log win over a token under them.
    let run = StyleRun {
        start: 5,
        end: 13,
        style: AnsiStyle::default(),
    };
    let hl = e.match_highlight_spans_with(line, &[run]);
    assert!(matches!(hl.spans[0].style, SpanStyle::Ansi(_)));
    assert_eq!(
        tokens(&hl.spans, line),
        vec![("5ms".to_string(), TokenKind::Duration)]
    );
}

#[test]
fn tokens_share_the_span_budget() {
    let (_tmp, mut e) = engine();
    e.set_auto_tokens(TokenKinds::ALL);
    let line = "1s ".repeat(200);
    let hl = e.match_highlight_spans(&line);
    assert_eq!(hl.spans.len(), MAX_ROW_SPANS);
}

#[test]
fn settings_round_trip_and_default() {
    let cfg = FastTailConfig::default();
    assert!(!cfg.auto_highlight);
    assert_eq!(cfg.auto_tokens(), TokenKinds::NONE);
    let ini = cfg.to_ini();
    let back = FastTailConfig::from_ini(&ini);
    assert!(!back.auto_highlight);
    assert_eq!(back.auto_highlight_kinds, TokenKinds::ALL);

    let mut cfg = FastTailConfig::default();
    cfg.auto_highlight = true;
    cfg.auto_highlight_kinds.set(TokenKind::Path, false);
    let back = FastTailConfig::from_ini(&cfg.to_ini());
    assert!(back.auto_highlight);
    assert!(!back.auto_highlight_kinds.contains(TokenKind::Path));
    assert!(back.auto_highlight_kinds.contains(TokenKind::Ip));
    assert_eq!(back.auto_tokens(), back.auto_highlight_kinds);

    // A file without the keys: off, every kind.
    let mut ini = ini::Ini::new();
    ini.with_section(Some("general")).set("theme", "Tron");
    let old = FastTailConfig::from_ini(&ini);
    assert!(!old.auto_highlight);
    assert_eq!(old.auto_highlight_kinds, TokenKinds::ALL);
}
