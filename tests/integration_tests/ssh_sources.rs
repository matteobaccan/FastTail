// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use fasttail::config::FastTailConfig;
use fasttail::workspace::{open_target, OpenOutcome};
use std::path::Path;

#[cfg(unix)]
mod unix {
    use super::*;
    use fasttail::ssh_source::{Disconnect, SshState};
    use fasttail::tail_engine::TailEngine;
    use std::io::Write;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    /// An `ssh` stand-in that runs the remote script here, as a server would.
    fn fake_ssh(dir: &Path, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join("fake-ssh");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    const RUN_SCRIPT: &str = "for a; do last=$a; done; exec sh -c \"$last\"";

    fn config(dir: &Path, ssh: &Path, initial_mb: u32) -> FastTailConfig {
        FastTailConfig {
            ssh_program: ssh.to_string_lossy().into_owned(),
            spool_dir: Some(dir.join("spool")),
            remote_initial_mb: initial_mb,
            ..Default::default()
        }
    }

    fn open(url: &str, config: &FastTailConfig) -> TailEngine {
        match open_target(Path::new(url), config, None) {
            OpenOutcome::Opened(e) => *e,
            _ => panic!("{url} did not open"),
        }
    }

    fn poll_until(engine: &mut TailEngine, what: &str, cond: impl Fn(&TailEngine) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(15);
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

    fn append(path: &Path, text: &str) {
        let mut f = std::fs::OpenOptions::new().append(true).open(path).unwrap();
        f.write_all(text.as_bytes()).unwrap();
    }

    #[test]
    fn a_remote_file_is_followed_and_resumed_after_a_drop_without_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("app.log");
        std::fs::write(&log, "one\ntwo\n").unwrap();
        let ssh = fake_ssh(dir.path(), RUN_SCRIPT);
        let config = config(dir.path(), &ssh, 64);
        let url = format!("ssh://web1{}", log.display());
        let mut engine = open(&url, &config);
        assert_eq!(fasttail::find_all::stream_name(&engine), "web1:app.log");
        poll_until(&mut engine, "the first lines", |e| e.total_lines() >= 2);
        append(&log, "three\n");
        poll_until(&mut engine, "the appended line", |e| e.total_lines() >= 3);
        assert_eq!(engine.ssh.as_ref().unwrap().state(), SshState::Following);

        engine.ssh.as_ref().unwrap().drop_connection_for_test();
        append(&log, "four\nfive\n");
        poll_until(&mut engine, "the lines written during the drop", |e| {
            e.total_lines() >= 5
        });
        std::thread::sleep(Duration::from_millis(300));
        engine.poll_updates();
        assert_eq!(lines(&engine), ["one", "two", "three", "four", "five"]);
    }

    #[test]
    fn a_tail_fetch_starts_at_a_whole_line_and_whole_fetches_everything() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("big.log");
        let text: String = (0..30_000).map(|i| format!("line {i:05}\n")).collect();
        std::fs::write(&log, &text).unwrap();
        let ssh = fake_ssh(dir.path(), RUN_SCRIPT);
        // 0 MB from the end: only new lines.
        let mut engine = open(
            &format!("ssh://h{}", log.display()),
            &config(dir.path(), &ssh, 0),
        );
        poll_until(&mut engine, "the connection", |e| {
            e.ssh.as_ref().unwrap().state() == SshState::Following
        });
        append(&log, "fresh\n");
        poll_until(&mut engine, "the new line", |e| e.total_lines() >= 1);
        assert_eq!(lines(&engine), ["fresh"]);

        let mut whole = open(
            &format!("ssh://h{}#whole", log.display()),
            &config(dir.path(), &ssh, 0),
        );
        poll_until(&mut whole, "the whole file", |e| e.total_lines() >= 30_001);
        assert_eq!(whole.get_line(0).unwrap(), "line 00000");
    }

    #[test]
    fn a_tail_fetch_that_starts_on_a_line_start_keeps_that_line() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("lines.log");
        // 3 MB of 1 KB lines: the last 1 MB starts exactly on a line start.
        let line = format!("{}\n", "x".repeat(1023));
        std::fs::write(&log, line.repeat(3 * 1024)).unwrap();
        let ssh = fake_ssh(dir.path(), RUN_SCRIPT);
        let mut engine = open(
            &format!("ssh://h{}", log.display()),
            &config(dir.path(), &ssh, 1),
        );
        poll_until(&mut engine, "the last megabyte", |e| {
            e.total_lines() >= 1024
        });
        std::thread::sleep(Duration::from_millis(200));
        engine.poll_updates();
        assert_eq!(engine.total_lines(), 1024);
        assert_eq!(engine.get_line(0).unwrap().len(), 1023);
    }

    #[test]
    fn a_remote_pattern_follows_the_newest_match_and_switches() {
        let dir = tempfile::tempdir().unwrap();
        let logs = dir.path().join("logs");
        std::fs::create_dir(&logs).unwrap();
        std::fs::write(logs.join("app-1.log"), "first file\n").unwrap();
        let ssh = fake_ssh(dir.path(), RUN_SCRIPT);
        let settings = fasttail::ssh_source::SshSettings {
            program: ssh.to_string_lossy().into_owned(),
            initial_bytes: 1 << 20,
            rescan: Duration::from_millis(200),
        };
        let spool =
            fasttail::stdin_source::Settings::from_config(Some(&dir.path().join("spool")), 2048);
        let url = format!("ssh://web1{}/app-*.log", logs.display());
        let spec = fasttail::ssh_source::SshSpec::parse(Path::new(&url))
            .unwrap()
            .unwrap();
        let mut engine = fasttail::ssh_source::open_engine(spec, &settings, &spool, None).unwrap();
        poll_until(&mut engine, "the first match", |e| e.total_lines() >= 1);
        assert_eq!(
            fasttail::find_all::stream_name(&engine),
            "web1:app-*.log ▸ app-1.log"
        );
        let newer = logs.join("app-2.log");
        std::fs::write(&newer, "second file\n").unwrap();
        let later = std::time::SystemTime::now() + Duration::from_secs(60);
        std::fs::File::options()
            .write(true)
            .open(&newer)
            .unwrap()
            .set_modified(later)
            .unwrap();
        poll_until(&mut engine, "the switch", |e| {
            e.get_line(0).as_deref() == Some("second file")
        });
        assert_eq!(lines(&engine), ["second file"]);
        assert_eq!(
            engine.ssh.as_ref().unwrap().switched().as_deref(),
            Some("app-2.log")
        );
    }

    #[test]
    fn a_refused_key_and_a_missing_file_stop_without_retrying() {
        let dir = tempfile::tempdir().unwrap();
        let denied = fake_ssh(
            dir.path(),
            "echo 'deploy@web1: Permission denied (publickey,password).' >&2; exit 255",
        );
        let mut engine = open(
            "ssh://web1/var/log/app.log",
            &config(dir.path(), &denied, 64),
        );
        poll_until(&mut engine, "the refusal", |e| {
            matches!(e.ssh.as_ref().unwrap().state(), SshState::Disconnected(_))
        });
        assert_eq!(
            engine.ssh.as_ref().unwrap().state(),
            SshState::Disconnected(Disconnect::Password)
        );

        let ssh = fake_ssh(dir.path(), RUN_SCRIPT);
        let missing = dir.path().join("nope.log");
        let mut engine = open(
            &format!("ssh://web1{}", missing.display()),
            &config(dir.path(), &ssh, 64),
        );
        poll_until(&mut engine, "the missing file", |e| {
            matches!(e.ssh.as_ref().unwrap().state(), SshState::Disconnected(_))
        });
        assert_eq!(
            engine.ssh.as_ref().unwrap().state(),
            SshState::Disconnected(Disconnect::Missing)
        );
        // Created later: Reconnect finds it.
        std::fs::write(&missing, "late\n").unwrap();
        engine.ssh.as_mut().unwrap().reconnect_requested = true;
        assert_eq!(engine.apply_ssh_request(), None);
        poll_until(&mut engine, "the line after reconnecting", |e| {
            e.total_lines() >= 1
        });
        assert_eq!(lines(&engine), ["late"]);
    }

    #[test]
    fn a_file_replaced_during_a_drop_starts_the_stream_again() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("app.log");
        std::fs::write(&log, "old 1\nold 2\n").unwrap();
        let ssh = fake_ssh(dir.path(), RUN_SCRIPT);
        let mut engine = open(
            &format!("ssh://h{}", log.display()),
            &config(dir.path(), &ssh, 64),
        );
        poll_until(&mut engine, "the first lines", |e| e.total_lines() >= 2);
        engine.ssh.as_ref().unwrap().drop_connection_for_test();
        // Rotated: a new file (a new inode) under the same name.
        std::fs::remove_file(&log).unwrap();
        std::fs::write(&log, "new 1\n").unwrap();
        poll_until(&mut engine, "the new file", |e| {
            e.get_line(0).as_deref() == Some("new 1")
        });
        assert_eq!(lines(&engine), ["new 1"]);
        assert_eq!(
            engine.piped().unwrap().restart_reason(),
            Some(fasttail::stdin_source::RestartReason::Replaced)
        );
    }
}

#[test]
fn bad_urls_and_relative_programs_are_refused() {
    let config = FastTailConfig::default();
    match open_target(Path::new("ssh://-oProxyCommand=calc/tmp/x"), &config, None) {
        OpenOutcome::Failed { error, .. } => assert_eq!(
            error.system_text(fasttail::i18n::Language::En).as_deref(),
            Some("Invalid host name")
        ),
        _ => panic!("expected a refusal"),
    }
    let config = FastTailConfig {
        ssh_program: "./ssh".into(),
        ..Default::default()
    };
    assert!(matches!(
        open_target(Path::new("ssh://h/tmp/x"), &config, None),
        OpenOutcome::Failed { .. }
    ));
}
