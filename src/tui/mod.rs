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
mod global;
mod hex;
mod keys;
mod mouse;
mod palette;
mod picker;
mod presets;
mod rules;
mod settings;
mod tools;
mod view;
mod workspace;

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::cli::{CliArgs, TUI_USAGE};
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

/// The command line: the options every FastTail executable takes (`crate::cli`), plus
/// `--bench`, `--capture`, `--stats` and the measurement aids, left out of the usage:
/// they are for the tests and the measurements in `docs/tui-feasibility.md`.
#[derive(Debug)]
struct Options {
    cli: CliArgs,
    stats: Option<String>,
    bench: Option<String>,
    capture: Option<(u16, u16)>,
    /// Capture with the search dialog open (for the docs), not advertised.
    capture_dialog: bool,
    /// Measurement aids, not advertised: quit after this many seconds, and page down
    /// this many times once the first file is indexed.
    quit_after: Option<u64>,
    scroll_test: Option<usize>,
}

/// Takes the hidden options out of `args` (those before a `--`) and parses the rest as
/// `fasttail` does, relative paths against `cwd`.
fn parse_args(args: impl IntoIterator<Item = String>, cwd: &Path) -> Result<Options, String> {
    let mut args = args.into_iter();
    let mut rest = Vec::new();
    let mut opts = Options {
        cli: CliArgs::default(),
        stats: None,
        bench: None,
        capture: None,
        capture_dialog: false,
        quit_after: None,
        scroll_test: None,
    };
    let value = |args: &mut dyn Iterator<Item = String>, name: &str| {
        args.next().ok_or_else(|| format!("{name} needs a value"))
    };
    let number = |s: String| s.parse().map_err(|_| format!("bad number: {s}"));
    while let Some(a) = args.next() {
        match a.as_str() {
            "--" => {
                rest.push(a);
                rest.extend(args.by_ref());
            }
            "--stats" => opts.stats = Some(value(&mut args, &a)?),
            "--bench" => opts.bench = Some(value(&mut args, &a)?),
            "--capture-dialog" => opts.capture_dialog = true,
            "--capture" => {
                let v = value(&mut args, &a)?;
                let (w, h) = v
                    .split_once('x')
                    .ok_or("--capture wants WxH, e.g. 100x24")?;
                opts.capture = Some((number(w.into())? as u16, number(h.into())? as u16));
            }
            // Read by `run` before parsing; nothing more to do here.
            crate::handoff::HANDOFF_FLAG => {}
            "--quit-after" => opts.quit_after = Some(number(value(&mut args, &a)?)? as u64),
            "--scroll-test" => opts.scroll_test = Some(number(value(&mut args, &a)?)?),
            _ => rest.push(a),
        }
    }
    opts.cli = CliArgs::parse(rest, cwd).map_err(|e| e.to_string())?;
    Ok(opts)
}

/// `--gui`: starts `fasttail` from this executable's directory with the same arguments
/// (`--gui` included, see `crate::handoff`) and returns the exit code: 0 once started, 1
/// when it is missing or does not start, 2 in a build without the graphical interface.
fn hand_off_to_gui(args: &[String]) -> i32 {
    use crate::handoff;
    if !cfg!(feature = "gui") {
        eprintln!("fasttail-tui: {}", handoff::terminal_only_message());
        return 2;
    }
    let name = handoff::gui_exe_name();
    let exe = handoff::sibling_exe(&name).unwrap_or_else(|| PathBuf::from(&name));
    if !exe.is_file() {
        let dir = exe.parent().unwrap_or(Path::new("."));
        eprintln!("fasttail-tui: {name} was not found in {}", dir.display());
        return 1;
    }
    let args = handoff::forwarded_args(args, handoff::HANDOFF_FLAG, None);
    match handoff::start_detached(&exe, &args) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("fasttail-tui: cannot start {}: {e}", exe.display());
            1
        }
    }
}

/// Runs the terminal interface with `args` (the command line without the program
/// name) and returns the process exit code. Started by a hand-off (`--handoff`), in a
/// console that closes with the process, an error exit waits for a key first so the
/// message can be read.
pub fn run(args: impl IntoIterator<Item = String>) -> i32 {
    let args: Vec<String> = args.into_iter().collect();
    let handed_off = args
        .iter()
        .take_while(|a| *a != "--")
        .any(|a| a == crate::handoff::HANDOFF_FLAG);
    let code = run_args(args);
    if handed_off && code != 0 {
        wait_for_key();
    }
    code
}

/// Says that a key closes the console, and waits for one.
fn wait_for_key() {
    let lang = crate::config::FastTailConfig::load_read_only().language;
    eprintln!("\n{}", crate::i18n::t(lang, "handoff_press_key"));
    if terminal::enable_raw_mode().is_ok() {
        loop {
            match event::read() {
                Ok(Event::Key(key)) if key.kind == event::KeyEventKind::Press => break,
                Ok(_) => {}
                Err(_) => break,
            }
        }
        let _ = terminal::disable_raw_mode();
    } else {
        let _ = io::stdin().read_line(&mut String::new());
    }
}

fn run_args(args: Vec<String>) -> i32 {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let opts = match parse_args(args.iter().cloned(), &cwd) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("fasttail-tui: {e}\n\n{TUI_USAGE}");
            return 2;
        }
    };
    let cli = &opts.cli;
    if cli.show_help || cli.show_version {
        if cli.show_version {
            println!("fasttail-tui {}", env!("CARGO_PKG_VERSION"));
        }
        if cli.show_help {
            print!("{TUI_USAGE}");
        }
        return 0;
    }
    if let Some(cfg) = &cli.config {
        // As the GUI does: the config lookup reads FASTTAIL_CONFIG first.
        std::env::set_var("FASTTAIL_CONFIG", cfg);
    }
    if cli.print {
        // Headless, as `fasttail --print`: the configuration is only read.
        return crate::print_mode::run(cli);
    }
    if cli.gui {
        return hand_off_to_gui(&args);
    }
    let term = TermInfo::from_env();
    if let Some(file) = &opts.bench {
        let palette = Palette::new(
            cli.theme.unwrap_or(CyberTheme::Tron),
            term.depth(),
            cli.ascii || term.ascii_borders(),
        );
        return bench(file, palette);
    }
    // Read like the GUI does; the run writes it back only with its own changes.
    let mut settings = Settings::locate();
    let mut palette = Palette::new(
        cli.theme.unwrap_or(settings.config.theme),
        term.depth(),
        cli.ascii || term.ascii_borders(),
    );
    palette.level_colors = settings.config.level_colors;

    // What to open, as in the GUI: a named session or the saved workspace (not with
    // --fresh), then the files named on the command line.
    let cli_paths = &cli.paths;
    let plan =
        match workspace::start_plan(&mut settings, cli.session.as_deref(), cli.fresh, cli_paths) {
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
    if cli.stdin || piped {
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
    // The command-line filters, follow mode and time window apply to the streams named on
    // the command line and to standard input, as in the GUI; the restored ones keep their
    // own state.
    for tab in &mut tabs {
        let named = tab.engine.is_stdin()
            || cli_paths
                .iter()
                .any(|p| crate::paths::paths_equal(p, &tab.engine.path));
        if !named {
            continue;
        }
        if let Some(f) = cli.window_filter() {
            tab.engine.set_include_filter(f);
        }
        if let Some(f) = cli.window_exclude() {
            tab.engine.set_exclude_filter(f);
        }
        if let Some(follow) = cli.follow {
            tab.engine.follow_tail = follow;
        }
        crate::cli::apply_time_window(&mut tab.engine, cli.since.as_deref(), cli.until.as_deref());
    }
    // The first stream named on the command line has the focus (standard input wins).
    if !cli.stdin && !piped {
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
    app.mouse = !cli.no_mouse;
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
    if cli.split {
        app.split_first_two();
    }
    if let Some(text) = &cli.search {
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
        let _ = crate::config::overwrite_regular_file(
            std::path::Path::new(file),
            stats.report(palette).as_bytes(),
        );
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

/// The Kitty keyboard protocol was pushed at start and must be popped at exit.
static KITTY_KEYS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Turns on the Kitty keyboard protocol when the terminal supports it (kitty, WezTerm,
/// foot, Ghostty, recent iTerm2 and Alacritty; never the Windows console): with
/// unambiguous escape codes, keys such as `Ctrl+Shift+3` and a lone `Esc` reach the
/// interface as typed. Only the disambiguation flag is asked for, so text, `Enter`,
/// `Tab` and `Backspace` keep their usual codes.
fn enable_kitty_keys() {
    use crossterm::event::{KeyboardEnhancementFlags, PushKeyboardEnhancementFlags};
    if matches!(terminal::supports_keyboard_enhancement(), Ok(true))
        && execute!(
            io::stdout(),
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        )
        .is_ok()
    {
        KITTY_KEYS.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Puts the terminal back: the keyboard protocol popped, mouse capture and raw mode off,
/// main screen, cursor shown. Disabling a capture that was never enabled is harmless.
fn restore_terminal() {
    if KITTY_KEYS.swap(false, std::sync::atomic::Ordering::SeqCst) {
        let _ = execute!(io::stdout(), crossterm::event::PopKeyboardEnhancementFlags);
    }
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
    enable_kitty_keys();
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Options, String> {
        parse_args(args.iter().map(|s| s.to_string()), Path::new("/work"))
    }

    #[test]
    fn hidden_options_are_taken_out_before_the_shared_parser() {
        let o = parse(&[
            "--capture",
            "100x24",
            "--filter",
            "ERROR",
            "--quit-after",
            "3",
            "--theme",
            "blade",
            "app.log",
        ])
        .unwrap();
        assert_eq!(o.capture, Some((100, 24)));
        assert_eq!(o.quit_after, Some(3));
        assert_eq!(o.cli.window_filter().map(String::as_str), Some("ERROR"));
        assert_eq!(o.cli.theme, Some(CyberTheme::Blade));
        assert_eq!(o.cli.paths, vec![Path::new("/work").join("app.log")]);
    }

    #[test]
    fn after_a_double_dash_everything_is_a_path() {
        let o = parse(&["--", "--capture", "-"]).unwrap();
        assert!(o.capture.is_none());
        assert!(o.cli.stdin);
        assert_eq!(o.cli.paths, vec![Path::new("/work").join("--capture")]);
    }

    #[test]
    fn the_terminal_executable_takes_every_fasttail_option() {
        let o = parse(&[
            "--tui",
            "--renderer",
            "wgpu",
            "--since=-15m",
            "--follow",
            "--version",
        ])
        .unwrap();
        assert!(o.cli.tui && o.cli.show_version);
        assert_eq!(o.cli.since.as_deref(), Some("-15m"));
        assert_eq!(o.cli.follow, Some(true));
        assert!(parse(&["--tui", "--gui"])
            .unwrap_err()
            .contains("--tui and --gui"));
        assert!(parse(&["--bogus"]).unwrap_err().contains("--bogus"));
        assert!(parse(&["--capture", "wide"]).is_err());
    }

    #[test]
    fn the_hand_off_flag_is_hidden_and_accepted() {
        let o = parse(&["--handoff", "app.log"]).unwrap();
        assert_eq!(o.cli.paths, vec![Path::new("/work").join("app.log")]);
        assert!(!TUI_USAGE.contains(crate::handoff::HANDOFF_FLAG));
        // `fasttail` itself does not take it: only a hand-off passes it on.
        assert!(CliArgs::parse(["--handoff"], Path::new("/work")).is_err());
    }
}
