//! `fasttail-tui`: a terminal front end over the FastTail engine. A feasibility
//! prototype (see `docs/tui-feasibility.md`), built only with `--features tui`.

mod app;
mod clipboard;
mod colors;
mod keys;
mod mouse;
mod view;
mod workspace;

use std::io::{self, Write};
use std::time::{Duration, Instant};

use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{cursor, execute};
use fasttail::tail_engine::TailEngine;
use fasttail::theme::CyberTheme;
use ratatui::backend::{Backend, CrosstermBackend, TestBackend};
use ratatui::Terminal;

use app::{App, Split, SplitDir, Tab};
use colors::{Palette, TermInfo};
use workspace::{Plan, Settings};

const USAGE: &str = "\
fasttail-tui - FastTail in the terminal (prototype)

USAGE:
    fasttail-tui [OPTIONS]              the GUI's workspace from fasttail.ini
    fasttail-tui [OPTIONS] PATH...      just these files (with their saved state)
    command | fasttail-tui [OPTIONS] -

fasttail.ini is read, never written: theme, highlight rules, global filter, poll
interval, open files and their filters, search, bookmarks, encoding and collapse.

OPTIONS:
    --config <FILE>      Use this configuration file (same as FASTTAIL_CONFIG)
    --session <FILE>     Open a named session (*.fasttail-session.ini)
    --filter <TEXT>      Include filter for every file
    --exclude <TEXT>     Exclude filter for every file
    --no-follow          Start paused instead of following the end
    --split              Start with the first two files side by side
    --search <TEXT>      Search the first file and jump to the first hit
    --theme <NAME>       tron, matrix, blade, light: overrides the ini's theme
    --ascii              Draw borders with +-| instead of box characters
    --no-mouse           Leave the mouse to the terminal (native text selection)
    --stats <FILE>       Write frame timings to FILE on exit
    --bench <FILE>       Headless render benchmark on FILE (no terminal needed)
    --capture <WxH>      Print one frame of the given files as text and exit
    -h, --help           Print this help

ENVIRONMENT:
    FASTTAIL_CONFIG      Configuration file, as for the GUI
    FASTTAIL_TUI_COLORS  16 or truecolor: overrides the colour detection
    FASTTAIL_TUI_ASCII   set: same as --ascii

Press ? or F1 in the viewer for the keys.
";

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
    /// Measurement aids, not advertised: quit after this many seconds, and page down
    /// this many times once the first file is indexed.
    quit_after: Option<u64>,
    scroll_test: Option<usize>,
}

fn parse_args() -> Result<Options, String> {
    let mut o = Options::default();
    let mut args = std::env::args().skip(1);
    let value = |args: &mut dyn Iterator<Item = String>, name: &str| {
        args.next().ok_or_else(|| format!("{name} needs a value"))
    };
    let number = |s: String| s.parse().map_err(|_| format!("bad number: {s}"));
    while let Some(a) = args.next() {
        match a.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            "-" => o.stdin = true,
            "--filter" => o.filter = Some(value(&mut args, &a)?),
            "--exclude" => o.exclude = Some(value(&mut args, &a)?),
            "--no-follow" => o.no_follow = true,
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
        _ => configured,
    }
}

fn main() {
    let opts = match parse_args() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("fasttail-tui: {e}\n\n{USAGE}");
            std::process::exit(2);
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
        bench(file, palette);
        return;
    }
    // Read-only: nothing below writes the ini back.
    let mut settings = Settings::locate();
    let mut palette = Palette::new(
        theme_of(opts.theme.as_deref(), settings.config.theme),
        term.depth(),
        opts.ascii || term.ascii_borders(),
    );
    palette.level_colors = settings.config.level_colors;

    // What to open: a named session, the files named on the command line (with the
    // state the ini keeps for them), or the GUI's workspace.
    let plan = if let Some(file) = &opts.session {
        match workspace::session_plan(&mut settings, &app::absolute(file)) {
            Ok(plan) => plan,
            Err(e) => {
                eprintln!("fasttail-tui: {e}");
                std::process::exit(2);
            }
        }
    } else if !opts.paths.is_empty() {
        Plan {
            paths: opts.paths.iter().map(|p| app::absolute(p)).collect(),
            missing: Vec::new(),
        }
    } else {
        workspace::workspace_plan(&settings)
    };
    let (engines, errors) = workspace::open_plan(&settings, &plan);
    let mut tabs: Vec<Tab> = engines.into_iter().map(Tab::new).collect();
    let mut notices: Vec<String> = errors;
    notices.extend(workspace::missing_notice(&plan.missing));
    let piped = fasttail::stdin_source::classify() == fasttail::stdin_source::StdinKind::Piped;
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
    if tabs.is_empty() {
        for n in &notices {
            eprintln!("fasttail-tui: {n}");
        }
        let origin = if settings.found {
            format!("the workspace in {}", settings.path.display())
        } else {
            format!("no {} found", settings.path.display())
        };
        eprintln!("fasttail-tui: nothing to open ({origin})\n\n{USAGE}");
        std::process::exit(2);
    }
    for tab in &mut tabs {
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

    let mut app = App::new(tabs, palette);
    app.active = focus;
    app.mouse = !opts.no_mouse;
    app.idle_poll = settings.poll_interval();
    if !notices.is_empty() {
        app.message = Some(notices.join("  |  "));
    }
    if opts.split && app.tabs.len() > 1 {
        app.split = Some(Split {
            dir: SplitDir::SideBySide,
            other: 1,
        });
    }
    if let Some(text) = &opts.search {
        app.search(text);
    }
    if let Some((w, h)) = opts.capture {
        if opts.capture_dialog {
            app.apply(keys::Action::StartSearch);
        }
        capture(&mut app, w, h);
        return;
    }
    let mut stats = FrameStats::default();
    let result = run_terminal(&mut app, &mut stats, &opts);
    if let Err(e) = result {
        eprintln!("fasttail-tui: {e}");
    }
    if let Some(file) = &opts.stats {
        let _ = std::fs::write(file, stats.report(palette));
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
    let _ = execute!(io::stdout(), DisableMouseCapture);
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
    terminal::enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen, cursor::Hide)?;
    if app.mouse {
        // On Windows this also turns QuickEdit off while the app runs (crossterm sets
        // the console mode without it and restores the old mode on disable).
        execute!(io::stdout(), EnableMouseCapture)?;
    }
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
            if !loading && !app.tabs[app.active].engine.index_pending {
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
        if event::poll(app::poll_timeout(app.busy(), app.idle_poll))? {
            // Drain everything that queued up, then draw once.
            loop {
                match event::read()? {
                    Event::Key(key) => dirty |= app.on_key(key),
                    Event::Mouse(m) => dirty |= app.on_mouse(m),
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
fn bench(file: &str, palette: Palette) {
    let path = app::absolute(file);
    let t = Instant::now();
    let engine = match app::open_path(
        &path,
        &fasttail::config::FastTailConfig::default().compressed_settings(),
    ) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("fasttail-tui: {e}");
            std::process::exit(1);
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
            e.set_collapse_mode(fasttail::collapse::CollapseMode::Numbers);
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
}
