// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use fasttail::config::FastTailConfig;
use fasttail::workspace::{open_target, OpenOutcome};
use std::path::Path;

#[test]
fn a_debug_output_uri_is_never_a_file() {
    let path = Path::new("dbwin://local");
    assert!(fasttail::system_source::is_system_path(path));
    assert!(!fasttail::wildcard::is_pattern_path(path));
    let bad = open_target(
        Path::new("dbwin://elsewhere"),
        &FastTailConfig::default(),
        None,
    );
    match bad {
        OpenOutcome::Failed { error, report } => {
            assert!(report);
            assert_eq!(
                error.system_text(fasttail::i18n::Language::En).as_deref(),
                Some("Not a debug output address (dbwin://local or dbwin://global)")
            );
        }
        _ => panic!("dbwin://elsewhere opened"),
    }
}

#[cfg(not(windows))]
#[test]
fn elsewhere_than_windows_the_capture_says_so() {
    match open_target(Path::new("dbwin://local"), &FastTailConfig::default(), None) {
        OpenOutcome::Failed { error, .. } => assert_eq!(
            error.system_text(fasttail::i18n::Language::En).as_deref(),
            Some("Debug output capture exists on Windows only")
        ),
        _ => panic!("dbwin://local opened off Windows"),
    }
}

#[cfg(windows)]
#[test]
fn output_debug_string_becomes_a_line_and_a_second_monitor_is_refused() {
    use fasttail::debug_output::{open_engine, DbwinSpec, Filter};
    use std::time::{Duration, Instant};

    extern "system" {
        fn OutputDebugStringW(text: *const u16);
    }
    let dir = tempfile::tempdir().unwrap();
    let spool = fasttail::stdin_source::Settings::from_config(Some(&dir.path().join("s")), 64);
    // This process's own messages are kept only when its id is named.
    let spec = DbwinSpec {
        global: false,
        filter: Filter {
            include: vec![std::process::id().to_string()],
            exclude: Vec::new(),
        },
    };
    let mut engine = match open_engine(spec.clone(), &spool, None) {
        Ok(engine) => engine,
        // A debug monitor of the machine (DebugView) owns the buffer: nothing to test.
        Err(e) if e.key == "dbwin_busy" => return,
        Err(e) => panic!("{e:?}"),
    };
    assert_eq!(engine.path, Path::new(&spec.to_uri()));
    let second = open_engine(spec, &spool, None);
    assert_eq!(second.err().map(|e| e.key), Some("dbwin_busy"));

    let text: Vec<u16> = "fasttail debug test\r\nsecond part\0"
        .encode_utf16()
        .collect();
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut sent = Instant::now() - Duration::from_secs(1);
    loop {
        // Sent again until it shows: the reader may not have handed the buffer yet.
        if sent.elapsed() >= Duration::from_millis(500) {
            unsafe { OutputDebugStringW(text.as_ptr()) };
            sent = Instant::now();
        }
        engine.poll_updates();
        if engine.total_lines() >= 2 {
            break;
        }
        assert!(Instant::now() < deadline, "no debug message arrived");
        std::thread::sleep(Duration::from_millis(20));
    }
    let first = engine.get_line(0).unwrap().into_owned();
    let pid = std::process::id();
    assert!(
        first.contains(&format!(" [{pid} ")) && first.ends_with("] fasttail debug test"),
        "{first}"
    );
    assert_eq!(engine.get_line(1).unwrap(), "  second part");
    // Closing releases the buffer: another capture can start at once.
    drop(engine);
    let again = open_engine(
        DbwinSpec {
            global: false,
            filter: Filter::default(),
        },
        &spool,
        None,
    );
    assert!(again.is_ok(), "{:?}", again.err());
}
