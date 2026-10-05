// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The GUI's settings and workspace, read (never written) by the terminal front end:
//! `fasttail.ini` found the way the GUI finds it, the streams it had open with their
//! per-stream state, or a named session file.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crate::config::FastTailConfig;
use crate::scan_job::FilterSpec;
use crate::session::{Session, StreamEntry};
use crate::tail_engine::TailEngine;

/// The configuration of this run and where it came from.
#[derive(Default)]
pub struct Settings {
    pub config: FastTailConfig,
    pub path: PathBuf,
    /// The global filter compiled once for every stream (`None` when off).
    pub global: Option<Arc<FilterSpec>>,
    /// Entries of `[open_files]` that no longer exist: the config reader drops them
    /// silently, the status bar names them.
    pub missing_open_files: Vec<PathBuf>,
}

impl Settings {
    /// Reads `path` without writing anything. `FastTailConfig::load` is not used because
    /// it may write at once: it migrates an old `fasttail.toml` by saving the ini. A
    /// missing or unreadable file gives the defaults. The bytes read are remembered, so
    /// `save` writes only this run's own changes.
    pub fn read(path: &Path) -> Self {
        let found = path.is_file();
        let conf = found.then(|| ini::Ini::load_from_file(path).ok()).flatten();
        if conf.is_some() {
            // What this run loaded: it writes the file only when its own state differs.
            if let Ok(bytes) = std::fs::read(path) {
                crate::config::remember_synced(path, bytes);
            }
        }
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
        config.poll_interval_ms = crate::settings_model::clamp(
            config.poll_interval_ms,
            &crate::settings_model::POLL_INTERVAL_MS,
        );
        let global = config.global_filter.compile();
        Self {
            config,
            path: path.to_path_buf(),
            global,
            missing_open_files,
        }
    }

    /// The GUI's lookup: `FASTTAIL_CONFIG`, then `fasttail.ini` in the current
    /// directory, next to the executable, in the user config directory.
    pub fn locate() -> Self {
        Self::read(&FastTailConfig::config_path())
    }

    /// Writes the configuration when this run's state differs from what it last loaded
    /// or wrote (so an idle terminal never overwrites what a GUI saved meanwhile), with
    /// the GUI's fallback to the user directory when the file cannot be written. Returns
    /// whether the file changed on disk.
    pub fn save(&mut self) -> std::io::Result<bool> {
        match self.config.save_to(&self.path) {
            Ok(written) => Ok(written),
            Err(e) => {
                let user = FastTailConfig::user_config_path().filter(|u| *u != self.path);
                let Some(user) = user else {
                    return Err(e);
                };
                if let Some(parent) = user.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let written = self.config.save_to(&user)?;
                self.path = user;
                Ok(written)
            }
        }
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

    /// Engine settings every stream gets, then the saved state of its path (the same
    /// setup as the GUI, from `crate::workspace`).
    pub fn prepare(&self, engine: &mut TailEngine) {
        crate::workspace::apply_settings(engine, &self.config);
        if self.global.is_some() {
            engine.set_global_filter(self.global.clone());
        }
        let path = engine.path.clone();
        crate::workspace::restore_stream(engine, &self.config, &path);
    }
}

/// What to open: paths in tab order, and the saved ones that no longer exist.
#[derive(Debug, Default, PartialEq)]
pub struct Plan {
    pub paths: Vec<PathBuf>,
    pub missing: Vec<PathBuf>,
    /// The session's `[dock] layout` (none for the default workspace, whose layout is
    /// the configuration's).
    pub dock_layout: Option<String>,
}

/// The streams of the GUI's default workspace (`open_files`, in its tab order), keeping
/// pattern streams, whose newest match is resolved when they open.
pub fn workspace_plan(settings: &Settings) -> Plan {
    let mut plan = Plan {
        missing: settings.missing_open_files.clone(),
        ..Plan::default()
    };
    for entry in Session::from_config(&settings.config).streams {
        // The GUI's derived streams ("Open filter as new tab") are not opened here; the
        // saved workspace keeps them (see `WorkspaceState::keep_derived`).
        if crate::filter_tab::parse_identity(&entry.path).is_some() {
            continue;
        }
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
        paths: loaded
            .session
            .streams
            .into_iter()
            .map(|s| s.path)
            .filter(|p| crate::filter_tab::parse_identity(p).is_none())
            .collect(),
        missing: loaded.missing,
        dock_layout: loaded.session.dock_layout,
    })
}

/// What to open at start, decided as the GUI decides it: a named session (`--session`),
/// else the saved workspace unless `fresh`, then the command-line paths it does not
/// already hold, in the order given.
pub fn start_plan(
    settings: &mut Settings,
    session: Option<&Path>,
    fresh: bool,
    cli_paths: &[PathBuf],
) -> Result<Plan, String> {
    let mut plan = match session {
        Some(file) => session_plan(settings, file)?,
        None if fresh => Plan::default(),
        None => workspace_plan(settings),
    };
    for path in cli_paths {
        if !plan
            .paths
            .iter()
            .any(|p| crate::paths::paths_equal(p, path))
        {
            plan.paths.push(path.clone());
        }
    }
    Ok(plan)
}

/// Opens the streams of `plan` with the ini's settings and saved state. Returns the
/// engines in plan order and a message per stream that could not be opened.
pub fn open_plan(settings: &Settings, plan: &Plan) -> (Vec<TailEngine>, Vec<String>) {
    let mut engines = Vec::new();
    let mut errors = Vec::new();
    for path in &plan.paths {
        match crate::tui::app::open_path(path, &settings.config) {
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
    crate::wildcard::is_pattern_path(path) || crate::compressed::source_exists(path)
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
    use crate::tail_engine::HighlightRule;
    use crate::theme::CyberTheme;
    use crate::tui::app::{buffer_text, App, Tab};
    use crate::tui::colors::{ColorDepth, Palette};
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
    fn the_start_plan_adds_command_line_files_to_the_workspace_as_the_gui_does() {
        let dir = tempfile::tempdir().unwrap();
        let ini = write_workspace(dir.path());
        let a = dir.path().join("a.log");
        let b = dir.path().join("b.log");
        let c = dir.path().join("c.log");
        std::fs::write(&c, "c\n").unwrap();

        let mut settings = Settings::read(&ini);
        let plan = start_plan(&mut settings, None, false, &[b.clone(), c.clone()]).unwrap();
        assert_eq!(
            plan.paths,
            vec![a.clone(), b.clone(), c.clone()],
            "no duplicate b"
        );

        let plan = start_plan(&mut settings, None, true, std::slice::from_ref(&c)).unwrap();
        assert_eq!(plan.paths, vec![c.clone()], "--fresh skips the workspace");

        let missing = dir.path().join("none.fasttail-session.ini");
        assert!(start_plan(&mut settings, Some(&missing), false, &[]).is_err());
    }

    #[test]
    fn the_gui_workspace_opens_with_its_state_and_colours() {
        let dir = tempfile::tempdir().unwrap();
        let ini = write_workspace(dir.path());
        let before = std::fs::read(&ini).unwrap();

        let settings = Settings::read(&ini);
        assert_eq!(settings.path, ini);
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
