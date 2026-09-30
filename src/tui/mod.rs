// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The terminal interface (`fasttail-tui`, feature `tui`): the FastTail engine drawn
//! with ratatui and crossterm. `src/bin/fasttail-tui.rs` calls `run`. Started as the
//! feasibility prototype of `docs/tui-feasibility.md`; openspec/changes/tui-interface
//! brings it to parity with the GUI.

mod app;
mod browser;
mod calendar;
mod clipboard;
mod colors;
mod dock;
mod form;
mod hex;
mod keys;
mod mouse;
mod picker;
mod settings;
mod view;
mod workspace;

use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::tail_engine::TailEngine;
use crate::theme::CyberTheme;
use crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    Event,
};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{cursor, execute};
use ratatui::backend::{Backend, CrosstermBackend, TestBackend};
use ratatui::Terminal;

use app::{App, Tab};
use colors::{Palette, TermInfo};
use workspace::Settings;

const USAGE: &str = "\
fasttail-tui - FastTail in the terminal

USAGE:
    fasttail-tui [OPTIONS]              the workspace saved in fasttail.ini
    fasttail-tui [OPTIONS] PATH...      the workspace and these files
    command | fasttail-tui [OPTIONS] -

fasttail.ini is read, never written: theme, highlight rules, global filter, poll
interval, open files and their filters, search, bookmarks, encoding and collapse.

OPTIONS:
    --config <FILE>      Use this configuration file (same as FASTTAIL_CONFIG)
    --session <FILE>     Open a named session (*.fasttail-session.ini)
    --fresh              Start without the saved workspace
    --filter <TEXT>      Include filter for the files named here
    --exclude <TEXT>     Exclude filter for the files named here
    --no-follow          Start the files named here paused
    --split              Start with the first two files side by side
    --search <TEXT>      Search the first file and jump to the first hit
    --theme <NAME>       tron, matrix, blade, light: overrides the ini's theme
    --ascii              Draw borders with +-| instead of box characters
    --no-mouse           Leave the mouse to the terminal (native text selection)
    -h, --help           Print this help

ENVIRONMENT:
    FASTTAIL_CONFIG      Configuration file, as for the GUI
    FASTTAIL_TUI_COLORS  16, 256 or truecolor: overrides the colour detection
    FASTTAIL_TUI_ASCII   set: same as --ascii

Press ? or F1 in the viewer for the keys.
";

/// `--bench`, `--capture` and `--stats` are left out of the usage: they are for the tests
/// and the measurements in `docs/tui-feasibility.md`.
#[derive(Default)]
struct Options {
    paths: Vec<String>,
    stdin: bool,
    filter: Option<String>,
    exclude: Option<String>,
    no_follow: bool,
    split: bool,
    theme: Option<String>,
    config: Option<String>,
    session: Option<String>,
    ascii: bool,
    no_mouse: bool,
    stats: Option<String>,
    bench: Option<String>,
    capture: Option<(u16, u16)>,
    search: Option<String>,
    /// Capture with the search dialog open (for the docs), not advertised.
    capture_dialog: bool,
    help: bool,
    fresh: bool,
    /// Measurement aids, not advertised: quit after this many seconds, and page down
    /// this many times once the first file is indexed.
    quit_after: Option<u64>,
    scroll_test: Option<usize>,
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Options, String> {
    let mut o = Options::default();
    let mut args = args.into_iter();
    let value = |args: &mut dyn Iterator<Item = String>, name: &str| {
        args.next().ok_or_else(|| format!("{name} needs a value"))
    };
    let number = |s: String| s.parse().map_err(|_| format!("bad number: {s}"));
    while let Some(a) = args.next() {
        match a.as_str() {
            "-h" | "--help" => o.help = true,
            "-" => o.stdin = true,
            "--filter" => o.filter = Some(value(&mut args, &a)?),
            "--exclude" => o.exclude = Some(value(&mut args, &a)?),
            "--no-follow" => o.no_follow = true,
            "--fresh" => o.fresh = true,
            "--split" => o.split = true,
            "--theme" => o.theme = Some(value(&mut args, &a)?),
            "--config" => o.config = Some(value(&mut args, &a)?),
            "--session" => o.session = Some(value(&mut args, &a)?),
            "--ascii" => o.ascii = true,
            "--no-mouse" => o.no_mouse = true,
            "--stats" => o.stats = Some(value(&mut args, &a)?),
            "--bench" => o.bench = Some(value(&mut args, &a)?),
            "--search" => o.search = Some(value(&mut args, &a)?),
            "--capture-dialog" => o.capture_dialog = true,
            "--capture" => {
                let v = value(&mut args, &a)?;
                let (w, h) = v
                    .split_once('x')
                    .ok_or("--capture wants WxH, e.g. 100x24")?;
                o.capture = Some((number(w.into())? as u16, number(h.into())? as u16));
            }
            "--quit-after" => o.quit_after = Some(number(value(&mut args, &a)?)? as u64),
            "--scroll-test" => o.scroll_test = Some(number(value(&mut args, &a)?)?),
            s if s.starts_with("--") => return Err(format!("unknown option {s}")),
            _ => o.paths.push(a),
        }
    }
    Ok(o)
}

/// `--theme` when given (and known), else the ini's theme.
fn theme_of(name: Option<&str>, configured: CyberTheme) -> CyberTheme {
    match name.map(|s| s.to_ascii_lowercase()).as_deref() {
        Some("tron") => CyberTheme::Tron,
        Some("matrix") => CyberTheme::Matrix,
        Some("blade") => CyberTheme::Blade,
        Some("light") => CyberTheme::Light,
        Some("commander") => CyberTheme::Commander,
        _ => configured,
    }
}

/// Runs the terminal interface with `args` (the command line without the program
/// name) and returns the process exit code.
pub fn run(args: impl IntoIterator<Item = String>) -> i32 {
    let opts = match parse_args(args) {
        Ok(o) if o.help => {
            print!("{USAGE}");
            return 0;
        }
        Ok(o) => o,
        Err(e) => {
            eprintln!("fasttail-tui: {e}\n\n{USAGE}");
            return 2;
        }
    };
    if let Some(cfg) = &opts.config {
        // As the GUI does: the config lookup reads FASTTAIL_CONFIG first.
        std::env::set_var("FASTTAIL_CONFIG", cfg);
    }
    let term = TermInfo::from_env();
    if let Some(file) = &opts.bench {
        let palette = Palette::new(
            theme_of(opts.theme.as_deref(), CyberTheme::Tron),
            term.depth(),
            opts.ascii || term.ascii_borders(),
        );
        return bench(file, palette);
    }
    // Read like the GUI does; the run writes it back only with its own changes.
    let mut settings = Settings::locate();
    let mut palette = Palette::new(
        theme_of(opts.theme.as_deref(), settings.config.theme),
        term.depth(),
        opts.ascii || term.ascii_borders(),
    );
    palette.level_colors = settings.config.level_colors;

    // What to open, as in the GUI: a named session or the saved workspace (not with
    // --fresh), then the files named on the command line.
    let cli_paths: Vec<PathBuf> = opts.paths.iter().map(|p| app::absolute(p)).collect();
    let session = opts.session.as_deref().map(app::absolute);
    let plan =
        match workspace::start_plan(&mut settings, session.as_deref(), opts.fresh, &cli_paths) {
            Ok(plan) => plan,
            Err(e) => {
                eprintln!("fasttail-tui: {e}");
                return 2;
            }
        };
    let (engines, errors) = workspace::open_plan(&settings, &plan);
    let mut tabs: Vec<Tab> = engines.into_iter().map(Tab::new).collect();
    let mut notices: Vec<String> = errors;
    notices.extend(workspace::missing_notice(&plan.missing));
    let piped = crate::stdin_source::classify() == crate::stdin_source::StdinKind::Piped;
    let mut focus = 0;
    if opts.stdin || piped {
        match app::open_stdin(&settings.config.stdin_settings()) {
            Ok(mut engine) => {
                settings.prepare(&mut engine);
                focus = tabs.len();
                tabs.push(Tab::new(engine));
            }
            Err(e) => notices.push(e),
        }
    }
    // Nothing to open is an empty workspace, as in the GUI: `o` opens a file there.
    // The command-line filters and --no-follow apply to the streams named on the command
    // line and to standard input, as in the GUI; the restored ones keep their own state.
    for tab in &mut tabs {
        let named = tab.engine.is_stdin()
            || cli_paths
                .iter()
                .any(|p| crate::paths::paths_equal(p, &tab.engine.path));
        if !named {
            continue;
        }
        if let Some(f) = &opts.filter {
            tab.engine.set_include_filter(f);
        }
        if let Some(f) = &opts.exclude {
            tab.engine.set_exclude_filter(f);
        }
        if opts.no_follow {
            tab.engine.follow_tail = false;
        }
    }
    // The first stream named on the command line has the focus (standard input wins).
    if !opts.stdin && !piped {
        if let Some(i) = tabs.iter().position(|t| {
            cli_paths
                .iter()
                .any(|p| crate::paths::paths_equal(p, &t.engine.path))
        }) {
            focus = i;
        }
    }

    let mut app = App::new(tabs, palette);
    app.active = focus;
    app.mouse = !opts.no_mouse;
    app.idle_poll = settings.poll_interval();
    // Streams opened later (`o`, the entry picker) get the same setup.
    app.settings = Some(settings);
    if !notices.is_empty() {
        app.message = Some(notices.join("  |  "));
    }
    let layout = app
        .settings
        .as_ref()
        .and_then(|s| s.config.dock_layout.clone());
    app.restore_dock(layout.as_deref());
    if opts.split {
        app.split_first_two();
    }
    if let Some(text) = &opts.search {
        app.search(text);
    }
    if let Some((w, h)) = opts.capture {
        if opts.capture_dialog {
            app.apply(keys::Action::StartSearch);
        }
        capture(&mut app, w, h);
        return 0;
    }
    let mut stats = FrameStats::default();
    // The interactive run saves fasttail.ini as the GUI does: every 2 s when it changed,
    // and once more on the way out.
    app.autosave = true;
    let result = run_terminal(&mut app, &mut stats, &opts);
    app.save_config();
    if let Some(file) = &opts.stats {
        let _ = std::fs::write(file, stats.report(palette));
    }
    match result {
        Ok(()) => 0,
        Err(e) => {
            // The terminal is already restored: the message lands on the main screen.
            eprintln!("fasttail-tui: {e}");
            1
        }
    }
}

/// Why the terminal could not be taken over: raw mode refused, as mintty does without
/// `winpty` (its pipes are not a console), or no terminal at all.
fn no_raw_mode() -> &'static str {
    if cfg!(windows) {
        "this terminal cannot be used interactively (raw mode refused). Run fasttail-tui \
         in Windows Terminal, cmd or PowerShell, or through winpty (winpty fasttail-tui) \
         in mintty / Git Bash"
    } else {
        "no interactive terminal (raw mode refused): run fasttail-tui in a terminal"
    }
}

/// Prints one frame as text once the files are indexed (the captures in the docs).
fn capture(app: &mut App, w: u16, h: u16) {
    wait_idle(app);
    // One more tick applies what the finished jobs asked for (the first search hit).
    app.tick();
    let mut terminal = Terminal::new(TestBackend::new(w, h)).expect("test backend");
    terminal.draw(|f| app.draw(f)).expect("draw");
    for line in app::buffer_text(terminal.backend().buffer()) {
        println!("{}", line.trim_end());
    }
}

/// Polls until no background job runs in any stream.
fn wait_idle(app: &mut App) {
    loop {
        app.tick();
        let busy = app.tabs.iter().any(|t| {
            let e = &t.engine;
            e.scan_progress().is_some()
                || e.index_pending
                || e.compressed.as_ref().is_some_and(|c| c.is_running())
        });
        if !busy {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Puts the terminal back: mouse capture and raw mode off, main screen, cursor shown.
/// Disabling a capture that was never enabled is harmless.
fn restore_terminal() {
    let _ = execute!(io::stdout(), DisableMouseCapture, DisableBracketedPaste);
    let _ = terminal::disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen, cursor::Show);
}

fn run_terminal(app: &mut App, stats: &mut FrameStats, opts: &Options) -> io::Result<()> {
    // A panic must not leave the terminal in raw mode on the alternate screen: restore it
    // first, then let the default hook print the message where the user can read it.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        default_hook(info);
    }));
    terminal::enable_raw_mode()
        .map_err(|e| io::Error::new(e.kind(), format!("{} ({e})", no_raw_mode())))?;
    execute!(io::stdout(), EnterAlternateScreen, cursor::Hide)?;
    if app.mouse {
        // On Windows this also turns QuickEdit off while the app runs (crossterm sets
        // the console mode without it and restores the old mode on disable).
        execute!(io::stdout(), EnableMouseCapture)?;
    }
    // A paste arrives as one event (into the field being edited) rather than as keys
    // that would act on the view; a terminal without it still types the keys.
    let _ = execute!(io::stdout(), EnableBracketedPaste);
    // `Stdout` flushes through a 1 KiB line buffer: a full frame would reach the console
    // in dozens of small writes, each a round trip to the terminal host. One large
    // buffer sends the frame in a single write when ratatui flushes.
    let out = io::BufWriter::with_capacity(1 << 18, io::stdout());
    let mut terminal = Terminal::new(CrosstermBackend::new(out))?;
    stats.size = terminal
        .size()
        .map(|s| (s.width, s.height))
        .unwrap_or((0, 0));
    let result = event_loop(&mut terminal, app, stats, opts);
    restore_terminal();
    result
}

fn event_loop<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    stats: &mut FrameStats,
    opts: &Options,
) -> io::Result<()>
where
    B::Error: Into<io::Error>,
{
    let started = Instant::now();
    let mut dirty = true;
    let mut scroll_left = opts.scroll_test;
    while !app.quit {
        dirty |= app.tick();
        let loading = app.busy();
        let mut phase = if loading {
            Phase::Loading
        } else {
            Phase::Other
        };
        if let Some(n) = scroll_left.as_mut() {
            // Measurement aid: once indexed, page down one page per tick.
            if !loading
                && app
                    .tabs
                    .get(app.active)
                    .is_none_or(|t| !t.engine.index_pending)
            {
                if *n == 0 {
                    app.quit = true;
                } else {
                    *n -= 1;
                    app.apply(keys::Action::PageDown);
                    dirty = true;
                    phase = Phase::Scroll;
                }
            }
        }
        if dirty {
            let t = Instant::now();
            let mut render = Duration::ZERO;
            terminal
                .draw(|f| {
                    let r = Instant::now();
                    app.draw(f);
                    render = r.elapsed();
                })
                .map_err(Into::into)?;
            stats.record(phase, t.elapsed(), render);
            dirty = false;
        }
        if opts
            .quit_after
            .is_some_and(|s| started.elapsed() >= Duration::from_secs(s))
        {
            break;
        }
        // Waits for input at most one tick; then the engines are polled again.
        if event::poll(app::poll_timeout(app.busy()))? {
            // Drain everything that queued up, then draw once.
            loop {
                match event::read()? {
                    Event::Key(key) => dirty |= app.on_key(key),
                    Event::Mouse(m) => dirty |= app.on_mouse(m),
                    Event::Paste(text) => dirty |= app.on_paste(&text),
                    Event::Resize(_, _) => {
                        terminal.autoresize().map_err(Into::into)?;
                        dirty = true;
                    }
                    _ => {}
                }
                if app.quit || !event::poll(Duration::ZERO)? {
                    break;
                }
            }
        }
    }
    Ok(())
}

/// What the loop was doing when a frame was drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// A background job (index, levels, filter, search) was running.
    Loading,
    /// A page-down of `--scroll-test`: every row of the window changes.
    Scroll,
    Other,
}

/// Frame timings, for `--stats` and the benchmark: the whole `draw` (widgets, diff,
/// write to the terminal) and the widget part alone (engine reads plus layout).
#[derive(Default)]
struct FrameStats {
    samples: Vec<(Phase, Duration, Duration)>,
    size: (u16, u16),
}

impl FrameStats {
    fn record(&mut self, phase: Phase, total: Duration, render: Duration) {
        self.samples.push((phase, total, render));
    }

    fn summary(label: &str, mut s: Vec<Duration>) -> String {
        if s.is_empty() {
            return String::new();
        }
        s.sort();
        let ms = |d: Duration| d.as_secs_f64() * 1000.0;
        let total: Duration = s.iter().sum();
        let pct = |p: f64| ms(s[((s.len() - 1) as f64 * p) as usize]);
        format!(
            "  {label:<8} n={:<4} avg={:>8.3}ms p50={:>8.3}ms p95={:>8.3}ms max={:>8.3}ms\n",
            s.len(),
            ms(total) / s.len() as f64,
            pct(0.5),
            pct(0.95),
            ms(s[s.len() - 1]),
        )
    }

    fn report(&self, palette: Palette) -> String {
        let mut out = format!(
            "terminal {}x{} depth={:?} ascii={}\n",
            self.size.0, self.size.1, palette.depth, palette.ascii
        );
        for phase in [Phase::Loading, Phase::Scroll, Phase::Other] {
            let of = |pick: fn(&(Phase, Duration, Duration)) -> Duration| {
                self.samples
                    .iter()
                    .filter(|s| s.0 == phase)
                    .map(pick)
                    .collect::<Vec<_>>()
            };
            let total = of(|s| s.1);
            if total.is_empty() {
                continue;
            }
            out.push_str(&format!("{phase:?}\n"));
            out.push_str(&Self::summary("draw", total));
            out.push_str(&Self::summary("widgets", of(|s| s.2)));
        }
        out
    }
}

/// Headless benchmark: indexes `file`, then draws frames into an in-memory 200x60
/// terminal at positions spread over the file, plain, with a search, filtered and
/// collapsed. It measures the engine row fetch plus ratatui's layout and diff; the
/// terminal's own output cost is what `--stats` adds on a real console.
fn bench(file: &str, palette: Palette) -> i32 {
    let path = app::absolute(file);
    let t = Instant::now();
    let engine = match app::open_path(&path, &crate::config::FastTailConfig::default()) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("fasttail-tui: {e}");
            return 1;
        }
    };
    let mut app = App::new(vec![Tab::new(engine)], palette);
    wait_idle(&mut app);
    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    println!(
        "opened {} ({:.1} MB, {} lines); index + levels done in {:.2}s",
        path.display(),
        size as f64 / 1e6,
        app.tabs[0].engine.total_lines(),
        t.elapsed().as_secs_f64()
    );
    let mut terminal = Terminal::new(TestBackend::new(200, 60)).expect("test backend");
    let mut run = |label: &str, app: &mut App| {
        let mut samples = Vec::new();
        app.tabs[0].engine.follow_tail = false;
        let rows = app.tabs[0].engine.visible_line_count();
        let frames = 300;
        for i in 0..frames {
            app.tabs[0].top = rows / frames * i;
            let t = Instant::now();
            terminal.draw(|f| app.draw(f)).expect("draw");
            samples.push(t.elapsed());
        }
        print!("{}", FrameStats::summary(label, samples));
        let _ = io::stdout().flush();
    };
    run("plain", &mut app);

    type Step = (&'static str, fn(&mut TailEngine));
    let steps: [Step; 3] = [
        ("search", |e| {
            e.search_query = "error".into();
            e.update_search("error");
        }),
        ("include", |e| {
            e.update_search("");
            e.search_query.clear();
            e.set_include_filter("GET");
        }),
        ("collapse", |e| {
            e.set_include_filter("");
            e.set_collapse_mode(crate::collapse::CollapseMode::Numbers);
        }),
    ];
    for (label, step) in steps {
        let t = Instant::now();
        step(&mut app.tabs[0].engine);
        wait_idle(&mut app);
        let e = &app.tabs[0].engine;
        println!(
            "{label}: done in {:.2}s ({} rows, {} search hits)",
            t.elapsed().as_secs_f64(),
            e.visible_line_count(),
            e.search_total()
        );
        run(label, &mut app);
    }
    0
}
