use fasttail::config::FastTailConfig;
use fasttail::stdin_source::{is_stdin_path, InputState, STDIN_PATH};
use fasttail::ui::app::StdinOptions;
use fasttail::ui::FastTailApp;
use std::io::Read;
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// A producer fed through a channel: each message is one write, dropping the sender
/// is the end of input.
struct Producer(mpsc::Receiver<Vec<u8>>, Vec<u8>);

impl Read for Producer {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.1.is_empty() {
            match self.0.recv() {
                Ok(data) => self.1 = data,
                Err(_) => return Ok(0),
            }
        }
        let n = self.1.len().min(buf.len());
        buf[..n].copy_from_slice(&self.1[..n]);
        self.1.drain(..n);
        Ok(n)
    }
}

fn producer() -> (mpsc::Sender<Vec<u8>>, Producer) {
    let (tx, rx) = mpsc::channel();
    (tx, Producer(rx, Vec::new()))
}

fn app_with_spool(dir: &Path) -> FastTailApp {
    let config = FastTailConfig {
        spool_dir: Some(dir.to_path_buf()),
        ..Default::default()
    };
    FastTailApp::from_config(config)
}

fn frame(app: &mut FastTailApp, ctx: &egui::Context) {
    let mut out = ctx.run_ui(Default::default(), |ui| app.render_ui(ui));
    out.textures_delta.clear();
}

fn frames_until(
    app: &mut FastTailApp,
    ctx: &egui::Context,
    what: &str,
    mut cond: impl FnMut(&FastTailApp) -> bool,
) {
    let start = Instant::now();
    loop {
        frame(app, ctx);
        if cond(app) {
            return;
        }
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "timed out waiting for {what}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn stdin_engine(app: &FastTailApp) -> Option<&fasttail::tail_engine::TailEngine> {
    app.engines.iter().find(|e| e.is_stdin())
}

#[test]
fn find_results_search_standard_input_under_its_name() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_spool(dir.path());
    let (tx, input) = producer();
    app.start_stdin(input, true, StdinOptions::default());
    let ctx = egui::Context::default();
    tx.send(
        b"INFO a
ERROR b
INFO c
ERROR d
"
        .to_vec(),
    )
    .unwrap();
    frames_until(&mut app, &ctx, "the lines", |app| {
        stdin_engine(app).is_some_and(|e| e.total_lines() == 4)
    });

    app.find_all.input = "error".into();
    app.find_all.start(&app.engines);
    frames_until(&mut app, &ctx, "the search", |app| {
        !app.find_all.is_active()
    });
    let group = &app.find_all.groups[0];
    assert_eq!(group.name, fasttail::stdin_source::STDIN_TITLE);
    assert_eq!(group.hits, vec![1, 3]);
    drop(tx);
}

#[test]
fn dash_opens_at_once_with_the_cli_filter_and_is_never_persisted() {
    let dir = tempfile::tempdir().unwrap();
    let mut log = tempfile::NamedTempFile::new().unwrap();
    std::io::Write::write_all(&mut log, b"file line\n").unwrap();
    let mut app = app_with_spool(dir.path());
    app.open_log_file(log.path().to_path_buf());
    let (tx, input) = producer();
    let options = StdinOptions {
        filter: Some("ERROR".to_string()),
        ..Default::default()
    };
    app.start_stdin(input, true, options);
    // With `-` the stream exists before any byte arrives.
    let engine = stdin_engine(&app).expect("stdin stream opened");
    assert!(is_stdin_path(&engine.path));
    assert_eq!(engine.include_filter(), "ERROR");
    assert!(engine.follow_tail);

    let ctx = egui::Context::default();
    tx.send(b"INFO a\nERROR b\nINFO c\nERROR d\n".to_vec())
        .unwrap();
    frames_until(&mut app, &ctx, "the lines", |app| {
        stdin_engine(app).is_some_and(|e| e.total_lines() == 4)
    });
    let engine = app.engines.iter_mut().find(|e| e.is_stdin()).unwrap();
    engine.set_bookmarks(vec![1]);
    engine.bookmarks_dirty = true;
    frame(&mut app, &ctx);
    app.save_dock_layout();

    // Nothing of it reaches the workspace or the recent files.
    let cfg = &app.config;
    assert!(!cfg.open_files.iter().any(|p| is_stdin_path(p)));
    assert!(cfg.open_files.iter().any(|p| p == log.path()));
    assert!(!cfg.recent_files.iter().any(|p| is_stdin_path(p)));
    assert!(!cfg.bookmarks.iter().any(|(p, _, _)| is_stdin_path(p)));
    let layout = cfg.dock_layout.clone().unwrap_or_default();
    assert!(!layout.contains(STDIN_PATH), "{layout}");
    let mut ini = Vec::new();
    cfg.to_ini().write_to(&mut ini).unwrap();
    assert!(!String::from_utf8_lossy(&ini).contains(STDIN_PATH));

    // Nor a session file, and the save says so.
    let session = app.capture_session();
    assert_eq!(session.streams.len(), 1);
    let file = dir.path().join("work.fasttail-session.ini");
    app.save_session_as(file.clone()).unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(!text.contains(STDIN_PATH));
    assert!(app.save_notice.is_some());

    // Loading a session keeps the stream: standard input cannot be read again.
    app.load_session_file(file, true);
    assert!(stdin_engine(&app).is_some());
    assert_eq!(app.engines.len(), 2);

    drop(tx);
    frames_until(&mut app, &ctx, "the end of input", |app| {
        stdin_engine(app)
            .and_then(|e| e.stdin.as_ref())
            .is_some_and(|s| s.state() == InputState::Ended)
    });
    assert_eq!(stdin_engine(&app).unwrap().total_lines(), 4);
}

#[test]
fn piped_without_dash_opens_on_the_first_byte() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_spool(dir.path());
    let ctx = egui::Context::default();
    let (tx, input) = producer();
    app.start_stdin(input, false, StdinOptions::default());
    frame(&mut app, &ctx);
    assert!(stdin_engine(&app).is_none(), "no tab before the first byte");
    assert!(app.pending_stdin.is_some());

    tx.send(b"hello\n".to_vec()).unwrap();
    frames_until(&mut app, &ctx, "the stream", |app| {
        stdin_engine(app).is_some_and(|e| e.total_lines() == 1)
    });
    assert!(app.pending_stdin.is_none());
    let spool = stdin_engine(&app)
        .and_then(|e| e.stdin.as_ref())
        .map(|s| s.spool_path().to_path_buf())
        .unwrap();
    assert!(spool.is_file());
    // Closing the stream deletes its spool once the copier lets go of it.
    app.engines.retain(|e| !e.is_stdin());
    drop(tx);
    let start = Instant::now();
    while spool.exists() {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "spool left behind"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn piped_without_dash_that_ends_empty_opens_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_spool(dir.path());
    let ctx = egui::Context::default();
    let (tx, input) = producer();
    app.start_stdin(input, false, StdinOptions::default());
    drop(tx);
    frames_until(&mut app, &ctx, "the empty end", |app| {
        app.pending_stdin.is_none()
    });
    assert!(stdin_engine(&app).is_none());
    assert_eq!(app.dock_state.iter_all_tabs().count(), 0);
}
