// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use fasttail::config::FastTailConfig;
use fasttail::system_source::{Commands, SystemUri};
use fasttail::tail_engine::TailEngine;
use fasttail::workspace::{open_target, OpenOutcome};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn poll_until(engine: &mut TailEngine, what: &str, cond: impl Fn(&TailEngine) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        engine.poll_updates();
        if cond(engine) {
            return;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn lines(engine: &TailEngine) -> Vec<String> {
    (0..engine.total_lines())
        .filter_map(|i| engine.get_line(i).map(|l| l.into_owned()))
        .collect()
}

/// A `docker` stand-in: two lines (the second on standard error), exit code 3; run
/// with `--since` it prints them again with a third line, as `docker logs` would.
#[cfg(unix)]
fn fake_docker(dir: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("fake-docker");
    std::fs::write(
        &path,
        "#!/bin/sh\n\
         echo '2026-10-23T06:00:00.000Z one'\n\
         echo '2026-10-23T06:00:01.000Z two' >&2\n\
         case \"$*\" in *--since*) echo '2026-10-23T06:00:03.000Z three';; esac\n\
         exit 3\n",
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[cfg(unix)]
#[test]
fn a_docker_source_is_copied_ends_and_reconnects_without_duplicates() {
    let dir = tempfile::tempdir().unwrap();
    let config = FastTailConfig {
        docker_command: fake_docker(dir.path()).to_string_lossy().into_owned(),
        spool_dir: Some(dir.path().join("spool")),
        ..Default::default()
    };
    let mut engine = match open_target(Path::new("docker://api"), &config, None) {
        OpenOutcome::Opened(e) => *e,
        _ => panic!("the source did not open"),
    };
    assert_eq!(engine.path, PathBuf::from("docker://api"));
    assert!(engine.follow_tail);
    assert_eq!(fasttail::find_all::stream_name(&engine), "docker: api");
    poll_until(&mut engine, "the end of the source", |e| {
        e.system_end().is_some()
    });
    assert_eq!(
        lines(&engine),
        [
            "2026-10-23T06:00:00.000Z one",
            "2026-10-23T06:00:01.000Z two"
        ]
    );
    let (code, last) = engine.system_end().unwrap();
    assert_eq!(code, Some(3));
    assert_eq!(last, "2026-10-23T06:00:01.000Z two");

    engine.system.as_mut().unwrap().reconnect_requested = true;
    assert_eq!(
        engine.apply_reconnect_request(&config.system_commands()),
        None
    );
    assert!(engine.system_end().is_none());
    poll_until(&mut engine, "the end after the reconnect", |e| {
        e.system_end().is_some() && e.total_lines() >= 3
    });
    assert_eq!(
        lines(&engine),
        [
            "2026-10-23T06:00:00.000Z one",
            "2026-10-23T06:00:01.000Z two",
            "2026-10-23T06:00:03.000Z three"
        ]
    );
}

#[test]
fn a_command_that_cannot_run_and_a_bad_uri_are_reported() {
    let config = FastTailConfig {
        docker_command: "fasttail-no-such-program".into(),
        ..Default::default()
    };
    match open_target(Path::new("docker://api"), &config, None) {
        OpenOutcome::Failed { error, report } => {
            assert!(report);
            let text = error.system_text(fasttail::i18n::Language::En).unwrap();
            assert!(
                text.starts_with("Cannot run fasttail-no-such-program"),
                "{text}"
            );
        }
        _ => panic!("expected a failure"),
    }
    match open_target(Path::new("docker://-v"), &config, None) {
        OpenOutcome::Failed { error, .. } => assert_eq!(
            error.system_text(fasttail::i18n::Language::En).as_deref(),
            Some("Not a valid container name or id")
        ),
        _ => panic!("expected a refusal"),
    }
}

#[test]
fn system_sources_are_saved_without_bookmarks_and_kept_in_sessions() {
    let uri = SystemUri::Docker {
        container: "api".into(),
    };
    assert_eq!(uri.to_uri(), "docker://api");
    let commands = Commands::default();
    assert_eq!(commands.backlog, 10_000);
    let mut config = FastTailConfig::default();
    config.open_files = vec![PathBuf::from("docker://api")];
    let loaded = FastTailConfig::from_ini(&config.to_ini());
    assert_eq!(loaded.open_files, [PathBuf::from("docker://api")]);
}

#[cfg(unix)]
#[test]
fn the_window_opens_a_system_source_from_a_path() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = fasttail::ui::app::FastTailApp::from_config(FastTailConfig {
        docker_command: fake_docker(dir.path()).to_string_lossy().into_owned(),
        spool_dir: Some(dir.path().join("spool")),
        ..Default::default()
    });
    app.open_log_file(PathBuf::from("docker://api"));
    assert_eq!(app.engines.len(), 1);
    assert_eq!(app.engines[0].path, PathBuf::from("docker://api"));
    assert!(app.engines[0].system.is_some());
}

#[test]
fn a_program_given_as_a_relative_path_is_refused() {
    let config = FastTailConfig {
        docker_command: "./docker".into(),
        ..Default::default()
    };
    match open_target(Path::new("docker://api"), &config, None) {
        OpenOutcome::Failed { error, .. } => assert_eq!(
            error.system_text(fasttail::i18n::Language::En).as_deref(),
            Some("./docker: the program must be a name on PATH or an absolute path")
        ),
        _ => panic!("expected a refusal"),
    }
}
