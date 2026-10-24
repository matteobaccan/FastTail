// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use fasttail::config::FastTailConfig;
use fasttail::workspace::{open_target, OpenOutcome};
use std::path::Path;

#[test]
fn an_event_log_uri_is_never_a_file() {
    let path = Path::new("eventlog://Application");
    assert!(fasttail::system_source::is_system_path(path));
    match open_target(
        Path::new("eventlog://System?levels=9"),
        &FastTailConfig::default(),
        None,
    ) {
        OpenOutcome::Failed { error, report } => {
            assert!(report);
            assert_eq!(
                error.system_text(fasttail::i18n::Language::En).as_deref(),
                Some("Levels go from 1 (critical) to 5 (verbose)")
            );
        }
        _ => panic!("a bad pre-filter opened"),
    }
}

#[cfg(not(windows))]
#[test]
fn elsewhere_than_windows_the_event_log_says_so() {
    match open_target(
        Path::new("eventlog://Application"),
        &FastTailConfig::default(),
        None,
    ) {
        OpenOutcome::Failed { error, .. } => assert_eq!(
            error.system_text(fasttail::i18n::Language::En).as_deref(),
            Some("The Windows Event Log is only available on Windows")
        ),
        _ => panic!("eventlog:// opened off Windows"),
    }
}

/// The Application channel of the CI machine: its history comes as event lines, oldest
/// first, with a level and a timestamp the detectors read.
#[cfg(windows)]
#[test]
fn the_application_channel_reads_as_event_lines() {
    use std::time::{Duration, Instant};
    let cfg = FastTailConfig {
        event_log_initial_events: 20,
        ..Default::default()
    };
    let mut engine = match open_target(Path::new("eventlog://Application"), &cfg, None) {
        OpenOutcome::Opened(e) => *e,
        OpenOutcome::Failed { error, .. } => panic!(
            "Application did not open: {:?}",
            error.system_text(fasttail::i18n::Language::En)
        ),
        _ => panic!("unexpected outcome"),
    };
    assert_eq!(
        engine.path,
        std::path::PathBuf::from("eventlog://Application")
    );
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        engine.poll_updates();
        let state = engine.event_log.as_ref().unwrap().state();
        if state == fasttail::eventlog::State::Live && engine.total_lines() > 0 {
            break;
        }
        assert_ne!(
            state,
            fasttail::eventlog::State::Failed,
            "{:?}",
            engine.event_log.as_ref().unwrap().error()
        );
        assert!(Instant::now() < deadline, "no event read");
        std::thread::sleep(Duration::from_millis(20));
    }
    let first = engine.get_line(0).unwrap().into_owned();
    // `2026-…T…+hh:mm LEVEL [Provider] id: message`
    assert!(first.contains(" ["), "{first}");
    assert!(first.contains("]: ") || first.contains("] "), "{first}");
    assert!(
        fasttail::timestamp::leading_span(&first, Default::default()).is_some(),
        "{first}"
    );
    assert!(engine.event_log.as_ref().unwrap().events() >= 1);
}
