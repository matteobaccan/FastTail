use super::*;

#[test]
fn test_wrap_persists_in_config_with_cap() {
    use fasttail::config::MAX_WRAPPED_FILES;
    let mut cfg = FastTailConfig::default();
    let path = std::path::PathBuf::from(if cfg!(windows) {
        r"C:\logs\app.log"
    } else {
        "/logs/app.log"
    });
    assert!(!cfg.wrap_for(&path), "off by default");

    cfg.set_wrap(&path, true);
    let restored = FastTailConfig::from_ini(&cfg.to_ini());
    assert!(restored.wrap_for(&path), "wrap survives the INI round trip");
    assert!(
        !restored.wrap_for(&path.with_file_name("other.log")),
        "other files stay unwrapped"
    );

    // Turning it off removes the entry.
    cfg.set_wrap(&path, false);
    assert!(!cfg.wrap_for(&path));
    assert!(cfg.wrapped_files.is_empty());
    assert!(cfg.to_ini().section(Some("wrapped_files")).is_none());

    // Toggling twice keeps a single entry; the file count is capped.
    cfg.set_wrap(&path, true);
    cfg.set_wrap(&path, true);
    assert_eq!(cfg.wrapped_files.len(), 1);
    for i in 0..MAX_WRAPPED_FILES + 10 {
        cfg.set_wrap(&path.with_file_name(format!("f{i}.log")), true);
    }
    assert_eq!(cfg.wrapped_files.len(), MAX_WRAPPED_FILES);
    assert!(
        !cfg.wrap_for(&path),
        "the oldest entry is dropped past the cap"
    );
}

#[test]
fn test_engine_wrap_toggle_keeps_top_row_and_marks_dirty() {
    use fasttail::wrap_layout::WrapScroll;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wrap.log");
    std::fs::write(&path, "a\nb\nc\nd\n").unwrap();
    let mut engine = TailEngine::open(&path).unwrap();
    assert!(!engine.wrap_lines);
    assert!(!engine.wrap_dirty);

    engine.follow_tail = false;
    engine.set_wrap_lines(true, 2);
    assert!(engine.wrap_lines && engine.wrap_dirty);
    assert_eq!(engine.wrap_anchor.row, 2, "the viewport keeps its top row");
    assert_eq!(engine.wrap_request, None);

    engine.wrap_dirty = false;
    engine.set_wrap_lines(true, 3);
    assert!(!engine.wrap_dirty, "no-op toggle does not mark dirty");

    // Back to extend mode marks dirty again; the renderer re-derives the scroll offset.
    engine.set_wrap_lines(false, 5);
    assert!(!engine.wrap_lines && engine.wrap_dirty);

    // With follow mode on, enabling wrap asks the renderer for the bottom.
    engine.follow_tail = true;
    engine.set_wrap_lines(true, 0);
    assert_eq!(engine.wrap_request, Some(WrapScroll::Bottom));
    assert!(engine.wrap_at_bottom);
}

#[test]
fn test_wrap_i18n_keys() {
    for lang in [
        Language::En,
        Language::It,
        Language::Fr,
        Language::Es,
        Language::Zh,
    ] {
        for key in [
            "tip_wrap",
            "help_desc_wrap",
            "open_pattern",
            "open_pattern_tip",
            "open_pattern_desc",
            "open_pattern_hint",
            "open_pattern_invalid",
            "open_pattern_go",
            "open_pattern_cancel",
            "pattern_waiting",
            "pattern_switched",
        ] {
            assert_ne!(t(lang, key), "Unknown", "{key} missing for {lang:?}");
        }
    }
}
