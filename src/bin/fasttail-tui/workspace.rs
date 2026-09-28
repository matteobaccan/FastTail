//! The GUI's settings and workspace, read (never written) by the terminal front end:
//! `fasttail.ini` found the way the GUI finds it, the streams it had open with their
//! per-stream state, or a named session file.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use fasttail::ansi::AnsiMode;
use fasttail::collapse::CollapseMode;
use fasttail::config::FastTailConfig;
use fasttail::scan_job::FilterSpec;
use fasttail::session::{Session, StreamEntry};
use fasttail::tail_engine::{FileEncoding, TailEngine};

/// The configuration of this run and where it came from.
pub struct Settings {
    pub config: FastTailConfig,
    pub path: PathBuf,
    pub found: bool,
    /// The global filter compiled once for every stream (`None` when off).
    pub global: Option<Arc<FilterSpec>>,
    /// Entries of `[open_files]` that no longer exist: the config reader drops them
    /// silently, the status bar names them.
    pub missing_open_files: Vec<PathBuf>,
}

impl Settings {
    /// Reads `path` read-only. `FastTailConfig::load` is not used because it may write:
    /// it migrates an old `fasttail.toml` by saving the ini. A missing or unreadable
    /// file gives the defaults.
    pub fn read(path: &Path) -> Self {
        let found = path.is_file();
        let conf = found.then(|| ini::Ini::load_from_file(path).ok()).flatten();
        let mut config = conf
            .as_ref()
            .map(FastTailConfig::from_ini)
            .unwrap_or_default();
        let missing_open_files = conf
            .as_ref()
            .and_then(|c| c.section(Some("open_files")))
            .map(|sec| {
                sec.iter()
                    .map(|(_, v)| PathBuf::from(v))
                    .filter(|p| !config.open_files.contains(p))
                    .collect()
            })
            .unwrap_or_default();
        // The same override the GUI's loader honours.
        if let Some(ms) = std::env::var("FASTTAIL_POLL_INTERVAL_MS")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
        {
            config.poll_interval_ms = ms;
        }
        config.poll_interval_ms = config.poll_interval_ms.clamp(50, 5000);
        let global = config.global_filter.compile();
        Self {
            config,
            path: path.to_path_buf(),
            found,
            global,
            missing_open_files,
        }
    }

    /// The GUI's lookup: `FASTTAIL_CONFIG`, then `fasttail.ini` in the current
    /// directory, next to the executable, in the user config directory.
    pub fn locate() -> Self {
        Self::read(&FastTailConfig::config_path())
    }

    /// Idle poll of the event loop: the GUI's background polling cadence.
    pub fn poll_interval(&self) -> Duration {
        Duration::from_millis(self.config.poll_interval_ms as u64)
    }

    /// Makes `entries` the per-stream state this run applies, as the GUI does when it
    /// loads a session (in memory only: nothing is saved).
    pub fn adopt_session(&mut self, entries: &[StreamEntry]) {
        self.config.streams.clear();
        self.config.bookmarks.clear();
        for e in entries {
            self.config.set_stream_state(e.clone());
            self.config
                .set_bookmarks_with_notes(&e.path, &e.bookmarks, &e.bookmark_notes);
        }
    }

    /// Engine settings every stream gets, then the saved state of its path.
    pub fn prepare(&self, engine: &mut TailEngine) {
        let cfg = &self.config;
        engine.size_check_interval = Duration::from_millis(cfg.size_check_interval_ms as u64);
        engine.auto_bookmark_max = cfg.auto_bookmark_max;
        engine.size_unit = cfg.size_unit;
        engine.set_highlight_rules(cfg.highlight_rules.clone());
        if self.global.is_some() {
            engine.set_global_filter(self.global.clone());
        }
        let path = engine.path.clone();
        restore_bookmarks(engine, cfg, &path);
        apply_stream_state(engine, cfg);
    }
}

/// What to open: paths in tab order, and the saved ones that no longer exist.
#[derive(Debug, Default, PartialEq)]
pub struct Plan {
    pub paths: Vec<PathBuf>,
    pub missing: Vec<PathBuf>,
}

/// The streams of the GUI's default workspace (`open_files`, in its tab order), keeping
/// pattern streams, whose newest match is resolved when they open.
pub fn workspace_plan(settings: &Settings) -> Plan {
    let mut plan = Plan {
        missing: settings.missing_open_files.clone(),
        ..Plan::default()
    };
    for entry in Session::from_config(&settings.config).streams {
        if exists(&entry.path) {
            plan.paths.push(entry.path);
        } else {
            plan.missing.push(entry.path);
        }
    }
    plan
}

/// The streams of a named session file, which also becomes the per-stream state.
pub fn session_plan(settings: &mut Settings, file: &Path) -> Result<Plan, String> {
    let loaded =
        Session::load_from(file).map_err(|e| format!("cannot load {}: {e}", file.display()))?;
    settings.adopt_session(&loaded.session.streams);
    Ok(Plan {
        paths: loaded.session.streams.into_iter().map(|s| s.path).collect(),
        missing: loaded.missing,
    })
}

/// Opens the streams of `plan` with the ini's settings and saved state. Returns the
/// engines in plan order and a message per stream that could not be opened.
pub fn open_plan(settings: &Settings, plan: &Plan) -> (Vec<TailEngine>, Vec<String>) {
    let compressed = settings.config.compressed_settings();
    let mut engines = Vec::new();
    let mut errors = Vec::new();
    for path in &plan.paths {
        match crate::app::open_path(path, &compressed) {
            Ok(mut engine) => {
                settings.prepare(&mut engine);
                engines.push(engine);
            }
            Err(e) => errors.push(e),
        }
    }
    (engines, errors)
}

fn exists(path: &Path) -> bool {
    fasttail::wildcard::is_pattern_path(path) || fasttail::compressed::source_exists(path)
}

/// The saved bookmarks of `path` (the GUI's `restore_bookmarks`): a compressed stream
/// starts on an empty spool, so its bookmarks wait for the index to cover them.
fn restore_bookmarks(engine: &mut TailEngine, cfg: &FastTailConfig, path: &Path) {
    if let Some(c) = engine.compressed.as_mut() {
        if let Some((_, lines, notes)) = cfg.saved_bookmarks(path) {
            c.pending_bookmarks = lines.clone();
            c.pending_bookmark_notes = notes.clone();
        }
    } else if let Some((lines, notes)) = cfg.bookmarks_with_notes_for(path, engine.total_lines()) {
        engine.set_bookmarks_with_notes(lines, notes);
    }
}

/// The saved filters, search, encoding, ANSI and collapse mode of the engine's path, in
/// the GUI's order (`apply_stream_state` in `ui/app.rs`).
fn apply_stream_state(engine: &mut TailEngine, cfg: &FastTailConfig) {
    let Some(entry) = cfg.stream_state_for(&engine.path).cloned() else {
        return;
    };
    if let Some(mode) = entry.ansi.as_deref().and_then(AnsiMode::from_name) {
        engine.set_ansi_mode(mode);
    }
    if let Some(enc) = entry.encoding.as_deref().and_then(FileEncoding::from_name) {
        if enc != engine.encoding {
            // Re-decoding rebuilds the index and drops bookmarks: restore them after.
            let bookmarks: Vec<usize> = engine.bookmarks.iter().copied().collect();
            let notes = engine.bookmark_notes.clone();
            engine.set_encoding(enc);
            if !bookmarks.is_empty() && bookmarks.iter().all(|&l| l < engine.total_lines()) {
                engine.set_bookmarks_with_notes(bookmarks, notes);
            }
        }
    }
    let terms = |first: &String, extra: &[String]| -> Vec<String> {
        std::iter::once(first.clone())
            .chain(extra.iter().cloned())
            .collect()
    };
    let include = terms(&entry.include_filter, &entry.include_extra);
    let exclude = terms(&entry.exclude_filter, &entry.exclude_extra);
    if include.iter().chain(&exclude).any(|t| !t.is_empty()) {
        engine.set_filter_terms(include, exclude);
    }
    if !entry.search_query.is_empty() {
        engine.search_query = entry.search_query.clone();
        engine.update_search(&entry.search_query);
    }
    if let Some(mode) = entry.collapse.as_deref().and_then(CollapseMode::from_name) {
        engine.set_collapse_mode(mode);
    }
}

/// "Skipped 2 missing files: a.log, b.log" for the status bar.
pub fn missing_notice(missing: &[PathBuf]) -> Option<String> {
    if missing.is_empty() {
        return None;
    }
    let names: Vec<String> = missing
        .iter()
        .map(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| p.display().to_string())
        })
        .collect();
    let what = if missing.len() == 1 { "file" } else { "files" };
    Some(format!(
        "Skipped {} missing {what}: {}",
        missing.len(),
        names.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{buffer_text, App, Tab};
    use crate::colors::{ColorDepth, Palette};
    use fasttail::tail_engine::HighlightRule;
    use fasttail::theme::CyberTheme;
    use ratatui::backend::TestBackend;
    use ratatui::style::Color;
    use ratatui::Terminal;

    /// An ini as the GUI writes it: two open files and a third that was deleted, an
    /// include filter and a bookmark on the first, a highlight rule, the Matrix theme.
    fn write_workspace(dir: &Path) -> PathBuf {
        let a = dir.join("a.log");
        let b = dir.join("b.log");
        std::fs::write(
            &a,
            "10:00 INFO start\n10:01 ERROR disk failed\n10:02 ERROR other\n",
        )
        .unwrap();
        std::fs::write(&b, "10:00 INFO b one\n10:01 WARN b two\n").unwrap();
        let mut cfg = FastTailConfig {
            theme: CyberTheme::Matrix,
            open_files: vec![a.clone(), b.clone(), dir.join("gone.log")],
            highlight_rules: vec![HighlightRule::new("disk", [255, 0, 0], [0, 0, 80], false)],
            ..FastTailConfig::default()
        };
        let mut entry = StreamEntry::new(a.clone());
        entry.include_filter = "ERROR".into();
        cfg.set_stream_state(entry);
        cfg.set_bookmarks_with_notes(&a, &[2], &Default::default());
        let ini = dir.join("fasttail.ini");
        cfg.to_ini().write_to_file(&ini).unwrap();
        ini
    }

    #[test]
    fn the_gui_workspace_opens_with_its_state_and_colours() {
        let dir = tempfile::tempdir().unwrap();
        let ini = write_workspace(dir.path());
        let before = std::fs::read(&ini).unwrap();

        let settings = Settings::read(&ini);
        assert!(settings.found);
        assert_eq!(settings.config.theme, CyberTheme::Matrix);
        let plan = workspace_plan(&settings);
        assert_eq!(plan.paths.len(), 2);
        assert_eq!(
            missing_notice(&plan.missing).unwrap(),
            "Skipped 1 missing file: gone.log"
        );
        let (engines, errors) = open_plan(&settings, &plan);
        assert!(errors.is_empty(), "{errors:?}");
        let tabs: Vec<Tab> = engines.into_iter().map(Tab::new).collect();
        // The GUI's tab order, the saved filter and bookmark on the first stream.
        assert_eq!(tabs[0].title, "a.log");
        assert_eq!(tabs[1].title, "b.log");
        assert_eq!(tabs[0].engine.include_filter(), "ERROR");
        assert_eq!(tabs[0].engine.visible_line_count(), 2);
        assert!(tabs[0].engine.is_bookmarked(2));
        assert_eq!(tabs[1].engine.visible_line_count(), 2);

        let palette = Palette::new(settings.config.theme, ColorDepth::TrueColor, false);
        let mut app = App::new(tabs, palette);
        app.tick();
        let mut terminal = Terminal::new(TestBackend::new(80, 12)).unwrap();
        terminal.draw(|f| app.draw(f)).unwrap();
        let screen = buffer_text(terminal.backend().buffer());
        assert!(screen[0].contains("1:a.log") && screen[0].contains("2:b.log"));
        assert!(screen.iter().any(|l| l.contains("+ERROR")), "{screen:#?}");
        assert!(!screen.iter().any(|l| l.contains("INFO start")));
        // The row the rule matches carries the rule's colours.
        let (y, line) = screen
            .iter()
            .enumerate()
            .find(|(_, l)| l.contains("disk failed"))
            .unwrap();
        let x = line.chars().position(|c| c == 'd').unwrap() as u16;
        let cell = terminal.backend().buffer().cell((x, y as u16)).unwrap();
        assert_eq!(cell.fg, Color::Rgb(255, 0, 0));
        assert_eq!(cell.bg, Color::Rgb(0, 0, 80));
        // The other ERROR row keeps the level colour, not the rule's background.
        let (y, line) = screen
            .iter()
            .enumerate()
            .find(|(_, l)| l.contains("ERROR other"))
            .unwrap();
        let x = line.chars().position(|c| c == 'o').unwrap() as u16;
        let cell = terminal.backend().buffer().cell((x, y as u16)).unwrap();
        assert_ne!(cell.bg, Color::Rgb(0, 0, 80));

        // Nothing was written back.
        assert_eq!(std::fs::read(&ini).unwrap(), before);
    }

    #[test]
    fn a_named_session_replaces_the_per_stream_state() {
        let dir = tempfile::tempdir().unwrap();
        let ini = write_workspace(dir.path());
        let mut settings = Settings::read(&ini);
        let mut entry = StreamEntry::new(dir.path().join("b.log"));
        entry.exclude_filter = "two".into();
        let session = Session {
            streams: vec![entry],
            dock_layout: None,
        };
        let file = dir.path().join("x.fasttail-session.ini");
        session.save_to(&file).unwrap();
        let plan = session_plan(&mut settings, &file).unwrap();
        let (engines, _) = open_plan(&settings, &plan);
        assert_eq!(engines.len(), 1);
        assert_eq!(engines[0].exclude_filter(), "two");
        assert_eq!(engines[0].visible_line_count(), 1);
    }
}
