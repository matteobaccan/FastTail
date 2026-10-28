// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use fasttail::log_format::Library;
use fasttail::plugin_source::{self, Block, Status};
use fasttail::plugins::{self, Approvals, PluginUri};
use fasttail::stdin_source::Settings;
use fasttail::tail_engine::TailEngine;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// A source plugin `name` in `root` running `script` (sh on Unix, cmd elsewhere).
fn source_plugin(root: &Path, name: &str, script: &str, extra: &str) -> PathBuf {
    let dir = root.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let (command, flag) = if cfg!(windows) {
        ("cmd", "/c")
    } else {
        ("sh", "-c")
    };
    let script = script.replace('\\', "\\\\").replace('"', "\\\"");
    std::fs::write(
        dir.join("plugin.toml"),
        format!(
            "api = 1\nname = \"{name}\"\nversion = \"1.0\"\nkind = \"source\"\n[source]\ncommand = \"{command}\"\nargs = [\"{flag}\", \"{script}\"]\n{extra}"
        ),
    )
    .unwrap();
    dir
}

fn settings(dir: &Path) -> Settings {
    Settings::from_config(Some(dir), 64)
}

fn open(uri: &str, catalog: &plugins::Catalog, approvals: &Approvals, spool: &Path) -> TailEngine {
    let mut engine = plugin_source::open_engine(
        PluginUri::parse(Path::new(uri)).unwrap(),
        catalog,
        approvals,
        &settings(spool),
        spool,
        None,
    )
    .unwrap();
    engine.size_check_interval = Duration::ZERO;
    engine
}

/// Polls until `done` holds (20 s at most).
fn wait(engine: &mut TailEngine, done: impl Fn(&TailEngine) -> bool) {
    let started = Instant::now();
    while !done(engine) {
        engine.poll_updates();
        assert!(started.elapsed() < Duration::from_secs(20), "timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn lines(engine: &TailEngine) -> Vec<String> {
    (0..engine.total_lines())
        .filter_map(|i| engine.get_line(i).map(|l| l.trim_end().to_string()))
        .collect()
}

#[test]
fn approved_plugin_runs_and_its_end_is_reported() {
    let root = tempfile::tempdir().unwrap();
    let spool = tempfile::tempdir().unwrap();
    let script = if cfg!(windows) {
        "echo one& echo two& echo boom 1>&2& exit /b 3"
    } else {
        "echo one; echo two; echo boom >&2; exit 3"
    };
    source_plugin(root.path(), "demo", script, "");
    let catalog = plugins::discover(root.path());
    assert_eq!(catalog.plugins.len(), 1, "{:?}", catalog.broken);

    // Not approved: it opens empty and says so.
    let mut approvals = Approvals::default();
    let blocked = open("plugin://demo", &catalog, &approvals, spool.path());
    assert_eq!(
        blocked.plugin_status(),
        Some(Status::Blocked(Block::NotEnabled))
    );
    assert_eq!(blocked.path, PathBuf::from("plugin://demo"));
    assert_eq!(blocked.source_title().as_deref(), Some("demo"));

    approvals.set_enabled(&catalog.plugins[0], true);
    let mut engine = open("plugin://demo", &catalog, &approvals, spool.path());
    wait(&mut engine, |e| {
        matches!(e.plugin_status(), Some(Status::Ended(_)))
    });
    assert_eq!(engine.plugin_status(), Some(Status::Ended(Some(3))));
    assert_eq!(lines(&engine), vec!["one", "two"]);
    assert!(engine
        .plugin
        .as_ref()
        .unwrap()
        .stderr_tail()
        .contains("boom"));

    // A changed manifest needs a new approval.
    let changed = root.path().join("demo").join("plugin.toml");
    let text = std::fs::read_to_string(&changed).unwrap() + "\n# edited\n";
    std::fs::write(&changed, text).unwrap();
    let catalog = plugins::discover(root.path());
    let again = open("plugin://demo", &catalog, &approvals, spool.path());
    assert_eq!(again.plugin_status(), Some(Status::Blocked(Block::Changed)));
}

#[test]
fn on_exit_runs_the_program_again() {
    let root = tempfile::tempdir().unwrap();
    let spool = tempfile::tempdir().unwrap();
    source_plugin(root.path(), "tick", "echo tick", "restart = \"on-exit\"\n");
    let catalog = plugins::discover(root.path());
    let mut approvals = Approvals::default();
    approvals.set_enabled(&catalog.plugins[0], true);
    let mut engine = open("plugin://tick", &catalog, &approvals, spool.path());
    wait(&mut engine, |e| {
        matches!(
            e.plugin_status(),
            Some(Status::Restarting { attempt: 1, .. })
        )
    });
    // The first delay is a second: then it runs and prints again.
    wait(&mut engine, |e| e.total_lines() >= 2);
    assert_eq!(lines(&engine)[..2], ["tick", "tick"]);
}

#[test]
fn secrets_stay_out_of_the_identity_and_are_asked_again() {
    let root = tempfile::tempdir().unwrap();
    let spool = tempfile::tempdir().unwrap();
    source_plugin(
        root.path(),
        "api",
        "echo hi",
        "[[source.param]]\nid = \"region\"\ndefault = \"eu\"\n[[source.param]]\nid = \"token\"\ntype = \"secret\"\n",
    );
    let catalog = plugins::discover(root.path());
    let mut approvals = Approvals::default();
    approvals.set_enabled(&catalog.plugins[0], true);
    let mut engine = open(
        "plugin://api?region=us&token=s3cret",
        &catalog,
        &approvals,
        spool.path(),
    );
    let saved = engine.path.to_string_lossy().into_owned();
    assert_eq!(saved, "plugin://api?region=us");
    wait(&mut engine, |e| e.total_lines() >= 1);
    // Restored from the workspace: the token is asked again, region kept.
    let mut restored = open(&saved, &catalog, &approvals, spool.path());
    assert_eq!(
        restored.plugin_status(),
        Some(Status::Blocked(Block::NeedsValues))
    );
    let s = restored.plugin.as_mut().unwrap();
    assert_eq!(s.uri.values.get("region").map(String::as_str), Some("us"));
    let mut values = s.uri.values.clone();
    values.insert("token".into(), "again".into());
    let plugin = catalog.find("api");
    assert!(s
        .start(plugin, &approvals, Some(values), spool.path())
        .is_ok());
    wait(&mut restored, |e| e.total_lines() >= 1);
    assert_eq!(restored.path.to_string_lossy(), "plugin://api?region=us");
}

#[cfg(unix)]
#[test]
fn a_hostile_value_is_one_argument() {
    let root = tempfile::tempdir().unwrap();
    let spool = tempfile::tempdir().unwrap();
    let dir = root.path().join("echo");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("plugin.toml"),
        "api = 1\nname = \"echo\"\nversion = \"1\"\nkind = \"source\"\n[source]\ncommand = \"printf\"\nargs = [\"%s\\n\", \"{topic}\"]\n[[source.param]]\nid = \"topic\"\n",
    )
    .unwrap();
    let catalog = plugins::discover(root.path());
    let mut approvals = Approvals::default();
    approvals.set_enabled(&catalog.plugins[0], true);
    let mut engine = open(
        "plugin://echo?topic=orders%3B%20touch%20pwned",
        &catalog,
        &approvals,
        spool.path(),
    );
    wait(&mut engine, |e| {
        matches!(e.plugin_status(), Some(Status::Ended(_)))
    });
    assert_eq!(lines(&engine), vec!["orders; touch pwned"]);
    assert!(!dir.join("pwned").exists());
}

#[test]
fn discovery_lists_broken_manifests_with_their_error() {
    let root = tempfile::tempdir().unwrap();
    let bad = root.path().join("future");
    std::fs::create_dir_all(&bad).unwrap();
    std::fs::write(
        bad.join("plugin.toml"),
        "api = 2\nname = \"future\"\nversion = \"1\"\nkind = \"formats\"\n",
    )
    .unwrap();
    let catalog = plugins::discover(root.path());
    assert!(catalog.plugins.is_empty());
    assert!(catalog.broken[0]
        .error
        .contains("unsupported API version 2"));
    // Reload: a folder added later is found.
    source_plugin(root.path(), "late", "echo x", "");
    assert_eq!(plugins::discover(root.path()).plugins.len(), 1);
}

#[test]
fn bundled_formats_load_when_the_plugin_is_enabled() {
    let root = tempfile::tempdir().unwrap();
    let formats = tempfile::tempdir().unwrap();
    let acme = root.path().join("acme");
    std::fs::create_dir_all(&acme).unwrap();
    std::fs::write(
        acme.join("plugin.toml"),
        "api = 1\nname = \"acme\"\nversion = \"1\"\nkind = \"formats\"\n",
    )
    .unwrap();
    std::fs::write(
        acme.join("acme-server.fasttail-format.ini"),
        "[format]\nname = acme-server\nregex = ^(?P<ts>\\d\\d:\\d\\d:\\d\\d) (?P<level>[A-Z]+) (?P<msg>.*)$\nlevel_field = level\nlevel_map = SEVERE=error\nmessage_field = msg\nsample.1 = 10:00:00 SEVERE disk full\n",
    )
    .unwrap();
    std::fs::write(
        acme.join("broken.fasttail-format.ini"),
        "[format]\nname = broken\nregex = ^OK (?P<msg>.*)$\nsample.1 = not ok\n",
    )
    .unwrap();
    let catalog = plugins::discover(root.path());
    let mut approvals = Approvals::default();
    let lib = Library::load_with_plugins(formats.path(), &catalog, &approvals);
    let format = lib
        .find("acme/acme-server")
        .expect("loaded: formats need no approval");
    assert!(format.builtin && format.plugin.as_deref() == Some("acme"));
    assert!(lib.find("acme/broken").is_none());
    assert!(lib.plugin_errors[0].2.contains("sample 1 does not match"));
    let lines = ["10:00:00 SEVERE disk full", "10:00:01 INFO ok"];
    let found = fasttail::log_format::detect(&lib, Some("server.log"), &lines).unwrap();
    assert_eq!(found.name(), "acme/acme-server");
    let level = fasttail::fields::with_spans(|spans| {
        found.scan(lines[0], spans);
        found.level(spans, lines[0])
    });
    assert_eq!(level, Some(fasttail::log_level::LogLevel::Error));

    // Turned off: its formats are not loaded.
    approvals.set_enabled(&catalog.plugins[0], false);
    let lib = Library::load_with_plugins(formats.path(), &catalog, &approvals);
    assert!(lib.find("acme/acme-server").is_none());
}

#[cfg(target_os = "linux")]
#[test]
fn closing_the_stream_ends_the_processes_it_started() {
    let root = tempfile::tempdir().unwrap();
    let spool = tempfile::tempdir().unwrap();
    source_plugin(root.path(), "sleepy", "sleep 300 & echo $!; wait", "");
    let catalog = plugins::discover(root.path());
    let mut approvals = Approvals::default();
    approvals.set_enabled(&catalog.plugins[0], true);
    let mut engine = open("plugin://sleepy", &catalog, &approvals, spool.path());
    wait(&mut engine, |e| e.total_lines() >= 1);
    let pid: u32 = lines(&engine)[0].parse().unwrap();
    let alive = |pid: u32| {
        std::fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|s| {
            !s.split_whitespace()
                .nth(2)
                .is_some_and(|st| st == "Z" || st == "X")
        })
    };
    assert!(alive(pid));
    drop(engine);
    let started = Instant::now();
    while alive(pid) {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the child still runs"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}
