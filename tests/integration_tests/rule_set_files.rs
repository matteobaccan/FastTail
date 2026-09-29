use fasttail::audio::SoundAlertPreset;
use fasttail::config::{
    append_rules, parse_rule_set, read_rule_set, rule_set_to_ini, write_rule_set, FastTailConfig,
    RuleSetError, RULE_SET_VERSION,
};
use fasttail::tail_engine::HighlightRule;

fn ini_text(ini: &ini::Ini) -> String {
    let mut buf = Vec::new();
    ini.write_to(&mut buf).unwrap();
    String::from_utf8(buf).unwrap()
}

fn full_rule() -> HighlightRule {
    let mut rule = HighlightRule::captures(r"duration_ms=(\d{4,})", [250, 200, 10], [5, 6, 7]);
    rule.bold = true;
    rule.italic = true;
    rule.case_sensitive = true;
    rule.sound_alert = SoundAlertPreset::Critical;
    rule.auto_bookmark = true;
    rule.enabled = false;
    rule
}

#[test]
fn round_trip_keeps_every_key() {
    let rules = vec![
        full_rule(),
        HighlightRule::new("ERROR", [255, 0, 0], [0, 0, 0], false),
        HighlightRule::new("a=b;c # not a comment", [1, 2, 3], [4, 5, 6], false),
    ];
    let text = ini_text(&rule_set_to_ini(&rules));
    assert!(text.contains("[fasttail_rules]"));
    assert!(text.contains(&format!("version={RULE_SET_VERSION}")));
    assert_eq!(parse_rule_set(&text).unwrap(), rules);
}

#[test]
fn same_keys_as_fasttail_ini() {
    let mut cfg = FastTailConfig::default();
    cfg.highlight_rules = vec![full_rule()];
    let config_ini = cfg.to_ini();
    let set_ini = rule_set_to_ini(&cfg.highlight_rules);
    let a = config_ini.section(Some("highlight_0")).unwrap();
    let b = set_ini.section(Some("highlight_0")).unwrap();
    let keys = |p: &ini::Properties| {
        let mut v: Vec<(String, String)> = p
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        v.sort();
        v
    };
    assert_eq!(keys(a), keys(b));
    // Tool bindings are not part of a rule set.
    assert!(set_ini
        .sections()
        .all(|s| !s.unwrap_or("").starts_with("tool.")));
}

#[test]
fn append_skips_duplicates_and_reports_them() {
    let exported: Vec<HighlightRule> = (0..20)
        .map(|i| HighlightRule::new(&format!("rule{i}"), [1, 1, 1], [0, 0, 0], false))
        .collect();
    let mut mine = vec![
        HighlightRule::new("mine1", [9, 9, 9], [0, 0, 0], false),
        // Same pattern and flags, other colours: a duplicate.
        HighlightRule::new("rule7", [200, 0, 0], [0, 0, 0], false),
        // Same pattern, regex: not a duplicate of the plain-text rule.
        HighlightRule::new("rule8", [9, 9, 9], [0, 0, 0], true),
    ];
    let skipped = append_rules(&mut mine, exported);
    assert_eq!(skipped, 1);
    assert_eq!(mine.len(), 22);
    assert_eq!(mine[1].fg_color, [200, 0, 0], "the existing rule is kept");
    // A case-sensitive twin is a different rule too.
    let mut twin = HighlightRule::new("mine1", [9, 9, 9], [0, 0, 0], false);
    twin.case_sensitive = true;
    assert_eq!(append_rules(&mut mine, vec![twin]), 0);
    assert_eq!(mine.len(), 23);
}

#[test]
fn replace_through_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("team.fasttail-rules.ini");
    let rules = vec![full_rule()];
    write_rule_set(&path, &rules).unwrap();
    assert_eq!(read_rule_set(&path).unwrap(), rules);
    // Written again over the same file: truncated, not appended.
    write_rule_set(&path, &[]).unwrap();
    assert!(read_rule_set(&path).unwrap().is_empty());
}

#[test]
fn refusals() {
    // fasttail.ini itself has rules but no [fasttail_rules] section.
    let mut cfg = FastTailConfig::default();
    cfg.highlight_rules = vec![full_rule()];
    let config_text = ini_text(&cfg.to_ini());
    assert_eq!(parse_rule_set(&config_text), Err(RuleSetError::NotARuleSet));
    assert_eq!(
        parse_rule_set("just some text"),
        Err(RuleSetError::NotARuleSet)
    );
    assert_eq!(
        parse_rule_set("[fasttail_rules]\nversion=2\n[highlight_0]\npattern=x\n"),
        Err(RuleSetError::NewerVersion(2))
    );
    assert_eq!(
        parse_rule_set("[fasttail_rules]\nversion=abc\n"),
        Err(RuleSetError::NotARuleSet)
    );
    assert_eq!(
        parse_rule_set("[fasttail_rules]\nversion=1\n").unwrap(),
        Vec::<HighlightRule>::new()
    );
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(
        read_rule_set(&dir.path().join("missing.ini")),
        Err(RuleSetError::Io(_))
    ));
    assert!(matches!(
        read_rule_set(dir.path()),
        Err(RuleSetError::Io(_))
    ));
    // A huge file is refused without being parsed.
    let big = dir.path().join("big.fasttail-rules.ini");
    let mut text = String::from(
        "[fasttail_rules]
version=1
",
    );
    while text.len() as u64 <= fasttail::config::MAX_RULE_SET_BYTES {
        text.push_str(
            "; padding padding padding padding padding padding padding
",
        );
    }
    std::fs::write(&big, text).unwrap();
    assert_eq!(read_rule_set(&big), Err(RuleSetError::NotARuleSet));
}
