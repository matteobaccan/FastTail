// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Command line: `fasttail [OPTIONS] [PATH...]`, shared by `fasttail` and `fasttail-tui`.
//!
//! Hand-written parser: a handful of options and positional paths do not justify a
//! dependency. Paths are resolved against the current directory at parse time. A PATH of
//! exactly `-` means standard input (`stdin`), before or after `--` as in `cat`; a file
//! named `-` is reachable as `./-`. Both executables accept every option: the terminal
//! options are ignored by the window, `--renderer` by the terminal.

use std::path::{Path, PathBuf};

use crate::config::Interface;
use crate::log_level::LogLevel;
use crate::renderer::RendererChoice;
use crate::theme::CyberTheme;

/// The arguments and options both executables list, in the same words.
macro_rules! shared_usage {
    () => {
        "
ARGS:
    PATH...              Log files to open in addition to the restored workspace;
                         PATH:LINE (or PATH:LINE:COLUMN, as compilers print it) opens
                         the file at that line
    -                    Read standard input (`command | fasttail -`); piped input is
                         also picked up without it

OPTIONS:
    --tui                Start the terminal interface (on Windows run fasttail-tui.exe)
    --gui                Start the graphical interface, even with interface=tui set in
                         fasttail.ini
    --fresh              Start with an empty workspace instead of the saved one
    --filter <TEXT>      Include filter applied to the files opened from the command line
                         (given twice, the last one counts)
    --exclude <TEXT>     Exclude filter applied to the files opened from the command line
    --filter-expr <EXPR> Include side as an expression (AND, OR, NOT, parentheses,
                         \"phrases\", /regexes/), instead of --filter; also in print mode
    --since <TIME>       Start of the time window of the files opened from the command line:
                         14:02, 2026-09-28 14:02, a timestamp copied from a line, or a
                         relative time: now, -15m, -3h, -1h30m, -2d, -1w (units s, m, h,
                         d, w, counted back from now; in the window it slides with the clock)
    --until <TIME>       End of that time window, in the same forms
    --line <N>           Open the first file at line N (selected and centred, follow
                         paused); with --print, start the output at line N
    --follow             Enable follow mode on the files opened from the command line
    --all                Open every file a pattern PATH matches as its own stream (in the
                         window, a folder group that also opens the files created later)
    --rotations          Open each file with its rotations (app.log.1, app.log.2.gz ...) as
                         one stream, oldest first
    --listen <URL>       Receive lines on a local port: tcp://127.0.0.1:5140,
                         udp://127.0.0.1:5140 or syslog://127.0.0.1:5514 (repeatable)
    --no-follow          Disable follow mode on those files
    --renderer <NAME>    Rendering backend of the window: auto (default), glow, wgpu,
                         software
    --config <FILE>      Use this configuration file (same as FASTTAIL_CONFIG)
    --session <FILE>     Load this session file (*.fasttail-session.ini) at startup
    -V, --version        Print the version and exit
    -h, --help           Print this help and exit

TERMINAL OPTIONS (used by the terminal interface, ignored by the window):
    --split              Start with the first two files side by side
    --search <TEXT>      Search the first file and jump to the first hit
    --theme <NAME>       tron, matrix, blade, light or commander: overrides fasttail.ini
    --ascii              Draw borders with +-| instead of box characters
    --no-mouse           Leave the mouse to the terminal (native text selection)

AI ASSISTANTS:
    --mcp                Serve the Model Context Protocol on standard input and output: to the
                         FastTail window when its MCP server is on, else the files given,
                         read-only, without a window (the print mode filters apply)

PRINT MODE:
    --print              Write the lines that pass the filters to standard output and exit,
                         without opening a window (PATH... or `-`; piped input without PATH)
    --filter <TEXT>      Repeatable up to 8 times: every include term must match
    --exclude <TEXT>     Repeatable up to 8 times: any exclude term hides the line
    --regex              The filter terms are regular expressions
    --case-sensitive     The filter terms match case-sensitively
    --level <LEVEL>      Minimum level: trace, debug, info, warn, error or fatal
    --level-bytes <N>    Look for the level in the first N bytes of a line (16 to 4096;
                         default: the level_detect_bytes setting, 96)
    --since, --until     The time window, as above
    --context <N>        Print N lines (0 to 100) before and after each match, `--` between
                         groups
    --follow             Then keep printing the lines appended to the files
    --color <WHEN>       auto (default: when writing to a terminal and NO_COLOR is not set),
                         always or never
    --line-numbers       Prefix each line with its line number
    --no-prefix          Do not prefix each line with its file name when several are given
    --output <FORMAT>    text (default), json (one object per line), logfmt, csv or template
    --template <TEXT>    With --output template: `{time} {level} {msg}`, field names and
                         {line}, {file}, {time}, {level}, {text}; `{{` is a brace
    --format <NAME>      Read the fields with this parser (json, logfmt, csv, tsv, apache,
                         syslog or a log format's name) instead of detecting it
    Exit codes: 0 lines printed, 1 no line matched, 2 usage error, 3 an input could not be
    read (the others are still printed)
"
    };
}

/// The usage of `fasttail`.
pub const USAGE: &str = concat!(
    "FastTail - ultra-fast multi-stream log monitor

USAGE:
    fasttail [OPTIONS] [PATH...]
    fasttail --print [OPTIONS] [PATH...]
    fasttail --mcp [OPTIONS] [PATH...]
",
    shared_usage!()
);

/// The usage of `fasttail-tui`: the same options, and the environment it reads.
pub const TUI_USAGE: &str = concat!(
    "fasttail-tui - FastTail in the terminal

USAGE:
    fasttail-tui [OPTIONS] [PATH...]
    command | fasttail-tui [OPTIONS] -
    fasttail-tui --print [OPTIONS] [PATH...]
",
    shared_usage!(),
    "
ENVIRONMENT:
    FASTTAIL_CONFIG      Configuration file, as --config
    FASTTAIL_TUI_COLORS  16, 256 or truecolor: overrides the colour detection
    FASTTAIL_TUI_ASCII   set: same as --ascii

Press ? or F1 in the viewer for the keys.
"
);

/// The options followed by a value as a separate argument (`--filter ERROR`), for the
/// hand-offs that pass a command line on without parsing it (`crate::handoff`).
pub const VALUE_OPTIONS: &[&str] = &[
    "--filter",
    "--filter-expr",
    "--exclude",
    "--since",
    "--until",
    "--level",
    "--level-bytes",
    "--line",
    "--context",
    "--output",
    "--template",
    "--format",
    "--color",
    "--colour",
    "--config",
    "--session",
    "--renderer",
    "--search",
    "--theme",
];

/// When print mode colours its output (`--color`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ColorChoice {
    /// Colour when standard output is a terminal and `NO_COLOR` is not set.
    #[default]
    Auto,
    Always,
    Never,
}

impl ColorChoice {
    pub fn parse(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "always" => Some(Self::Always),
            "never" => Some(Self::Never),
            _ => None,
        }
    }
}

/// What print mode writes for each line (`--output`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PrintOutput {
    /// The line as it is.
    #[default]
    Text,
    /// One JSON object per line (NDJSON).
    Json,
    Logfmt,
    Csv,
    /// `--template`.
    Template,
}

impl PrintOutput {
    pub fn parse(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "text" => Some(Self::Text),
            "json" | "ndjson" => Some(Self::Json),
            "logfmt" => Some(Self::Logfmt),
            "csv" => Some(Self::Csv),
            "template" => Some(Self::Template),
            _ => None,
        }
    }
}

/// Most lines of context `--context` accepts, as in the window.
pub const MAX_CONTEXT_LINES: usize = 100;

/// Whether `input` is a time `--since` / `--until` accept: a relative time (see
/// `timestamp::parse_relative`), or anything the time range popup reads.
pub fn is_valid_time_arg(input: &str) -> bool {
    crate::timestamp::parse_relative(input, 0).is_some()
        || crate::timestamp::parse_user_time(input, 0).is_some()
}

/// The instant `input` names, `reference` being the day a bare time belongs to and `now`
/// the local time relative times count back from. On the "to" side (`until`) a typed time
/// covers the whole unit it names, as in the popup; a relative time is the exact instant.
pub fn resolve_time_arg(input: &str, reference: i64, now: i64, until: bool) -> Option<i64> {
    if let Some(millis) = crate::timestamp::parse_relative(input, now) {
        return Some(millis);
    }
    let millis = crate::timestamp::parse_user_time(input, reference)?;
    Some(if until {
        crate::timestamp::end_of_typed_time(input, millis)
    } else {
        millis
    })
}

/// Parsed command line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CliArgs {
    pub paths: Vec<PathBuf>,
    /// The line target of each path, 1-based (`app.log:1204`), in the order of `paths`.
    pub path_lines: Vec<Option<usize>>,
    /// `--line N`: the line target of the first path, 1-based.
    pub line: Option<usize>,
    /// `-` was given: read standard input as a stream.
    pub stdin: bool,
    /// Where `-` stood among the paths (print mode reads the inputs in order).
    pub stdin_at: usize,
    pub fresh: bool,
    /// `--all`: a pattern path opens every match, not just the newest.
    pub all: bool,
    /// `--rotations`: a file opens with its rotations (`app.log.1`, `.2.gz`, ...).
    pub rotations: bool,
    /// Include terms in the order given: print mode uses them all (at most
    /// `MAX_FILTER_TERMS`), the window the last one (`window_filter`).
    pub filter: Vec<String>,
    /// Exclude terms, as `filter` (`window_exclude`).
    pub exclude: Vec<String>,
    /// `--filter-expr`: the include side as an expression (instead of `--filter`).
    pub filter_expr: Option<String>,
    /// Start and end of the time window, as typed (validated at parse time).
    pub since: Option<String>,
    pub until: Option<String>,
    /// `--print`: headless print mode (see `print_mode`).
    pub print: bool,
    /// `--mcp`: the stdio MCP server (see `mcp::run_stdio`).
    pub mcp: bool,
    pub regex: bool,
    pub case_sensitive: bool,
    /// Minimum level (`--level`), never `Unknown`.
    pub level: Option<LogLevel>,
    /// `--level-bytes N`: the bytes of a line the level is looked for in (print mode).
    pub level_bytes: Option<usize>,
    pub context: Option<usize>,
    /// `--output`, `--template` and `--format` (print mode).
    pub output: Option<PrintOutput>,
    pub template: Option<String>,
    pub parser_format: Option<String>,
    pub color: Option<ColorChoice>,
    pub line_numbers: bool,
    pub no_prefix: bool,
    pub follow: Option<bool>,
    pub renderer: Option<RendererChoice>,
    pub config: Option<PathBuf>,
    /// Session file to load at startup, replacing the restored workspace.
    pub session: Option<PathBuf>,
    /// `--gui` / `--tui`: the interface asked for, over `interface` in `fasttail.ini`
    /// (`CliArgs::interface`). Both together are a usage error.
    pub gui: bool,
    pub tui: bool,
    /// Terminal options: used by the terminal interface, ignored by the window.
    pub split: bool,
    pub search: Option<String>,
    pub theme: Option<CyberTheme>,
    pub ascii: bool,
    pub no_mouse: bool,
    pub show_version: bool,
    pub show_help: bool,
}

/// What the caller should do after parsing.
#[derive(Debug, PartialEq, Eq)]
pub enum CliError {
    /// Unknown option or missing value; the message is ready for stderr.
    Usage(String),
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CliError::Usage(m) => write!(f, "{m}"),
        }
    }
}

impl CliArgs {
    /// Parses the process arguments (without the program name).
    pub fn from_env() -> Result<Self, CliError> {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self::parse(std::env::args().skip(1), &cwd)
    }

    /// Parses `args` (without the program name), resolving relative paths against `cwd`.
    pub fn parse<I, S>(args: I, cwd: &Path) -> Result<Self, CliError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut out = CliArgs::default();
        let mut iter = args.into_iter();
        let mut only_paths = false;

        while let Some(arg) = iter.next() {
            let arg = arg.as_ref();
            if arg == "-" {
                if out.stdin {
                    return Err(CliError::Usage("standard input given twice".to_string()));
                }
                out.stdin = true;
                out.stdin_at = out.paths.len();
                continue;
            }
            if only_paths || !arg.starts_with('-') {
                let (path, line) = split_line_target(arg, cwd);
                out.paths.push(path);
                out.path_lines.push(line);
                continue;
            }
            // `--opt=value` form
            let (name, inline) = match arg.split_once('=') {
                Some((n, v)) => (n, Some(v.to_string())),
                None => (arg, None),
            };
            let mut value = |what: &str| -> Result<String, CliError> {
                if let Some(v) = inline.clone() {
                    return Ok(v);
                }
                iter.next()
                    .map(|v| v.as_ref().to_string())
                    .ok_or_else(|| CliError::Usage(format!("option '{name}' needs a {what}")))
            };
            match name {
                "--" => only_paths = true,
                "--gui" => out.gui = true,
                "--tui" => out.tui = true,
                "--split" => out.split = true,
                "--ascii" => out.ascii = true,
                "--no-mouse" => out.no_mouse = true,
                "--search" => out.search = Some(value("text")?),
                "--theme" => {
                    let v = value("name")?;
                    out.theme = Some(parse_theme(&v).ok_or_else(|| {
                        CliError::Usage(format!(
                            "unknown theme '{v}' (expected tron, matrix, blade, light or commander)"
                        ))
                    })?);
                }
                "--fresh" => out.fresh = true,
                "--all" => out.all = true,
                "--rotations" => out.rotations = true,
                "--follow" => out.follow = Some(true),
                "--no-follow" => out.follow = Some(false),
                "-V" | "--version" => out.show_version = true,
                "-h" | "--help" => out.show_help = true,
                "--filter" => out.filter.push(value("text")?),
                "--filter-expr" => out.filter_expr = Some(value("expression")?),
                "--exclude" => out.exclude.push(value("text")?),
                "--print" => out.print = true,
                "--mcp" => out.mcp = true,
                "--regex" => out.regex = true,
                "--case-sensitive" => out.case_sensitive = true,
                "--line-numbers" => out.line_numbers = true,
                "--no-prefix" => out.no_prefix = true,
                "--since" | "--until" => {
                    let v = value("time")?;
                    if !is_valid_time_arg(&v) {
                        return Err(CliError::Usage(format!(
                            "cannot read the time '{v}' of '{name}' (expected 14:02, \
                             2026-09-28 14:02, a timestamp or -15m, -3h, -2d)"
                        )));
                    }
                    if name == "--since" {
                        out.since = Some(v);
                    } else {
                        out.until = Some(v);
                    }
                }
                "--level" => {
                    let v = value("level")?;
                    let level = LogLevel::parse(&v);
                    if level == LogLevel::Unknown {
                        return Err(CliError::Usage(format!(
                            "unknown level '{v}' (expected trace, debug, info, warn, error or fatal)"
                        )));
                    }
                    out.level = Some(level);
                }
                "--level-bytes" => {
                    let v = value("number")?;
                    let n = v
                        .trim()
                        .parse::<usize>()
                        .ok()
                        .and_then(crate::log_level::valid_window)
                        .ok_or_else(|| {
                            CliError::Usage(format!(
                                "invalid level window '{v}' (expected {} to {} bytes)",
                                crate::log_level::MIN_WINDOW,
                                crate::log_level::MAX_WINDOW
                            ))
                        })?;
                    out.level_bytes = Some(n);
                }
                "--listen" => {
                    let v = value("URL")?;
                    if !crate::net_listener::is_listen_path(Path::new(&v)) {
                        return Err(CliError::Usage(format!(
                            "invalid listener '{v}' (expected tcp://, udp:// or syslog://ADDRESS:PORT)"
                        )));
                    }
                    out.paths.push(PathBuf::from(v));
                    out.path_lines.push(None);
                }
                "--line" => {
                    let v = value("number")?;
                    let n = v
                        .trim()
                        .parse::<usize>()
                        .ok()
                        .filter(|n| *n >= 1)
                        .ok_or_else(|| {
                            CliError::Usage(format!(
                                "invalid line '{v}' (expected a number from 1)"
                            ))
                        })?;
                    out.line = Some(n);
                }
                "--context" => {
                    let v = value("number")?;
                    let n = v
                        .trim()
                        .parse::<usize>()
                        .ok()
                        .filter(|n| *n <= MAX_CONTEXT_LINES)
                        .ok_or_else(|| {
                            CliError::Usage(format!(
                                "invalid context '{v}' (expected 0 to {MAX_CONTEXT_LINES})"
                            ))
                        })?;
                    out.context = Some(n);
                }
                "--output" => {
                    let v = value("format")?;
                    out.output = Some(PrintOutput::parse(&v).ok_or_else(|| {
                        CliError::Usage(format!(
                            "unknown output '{v}' (expected text, json, logfmt, csv or template)"
                        ))
                    })?);
                }
                "--template" => out.template = Some(value("text")?),
                "--format" => out.parser_format = Some(value("name")?),
                "--color" | "--colour" => {
                    let v = value("value")?;
                    out.color = Some(ColorChoice::parse(&v).ok_or_else(|| {
                        CliError::Usage(format!(
                            "unknown colour mode '{v}' (expected auto, always or never)"
                        ))
                    })?);
                }
                "--config" => out.config = Some(resolve(&value("file path")?, cwd)),
                "--session" => out.session = Some(resolve(&value("file path")?, cwd)),
                "--renderer" => {
                    let v = value("name")?;
                    out.renderer = Some(RendererChoice::parse(&v).ok_or_else(|| {
                        CliError::Usage(format!(
                            "unknown renderer '{v}' (expected auto, glow, wgpu or software)"
                        ))
                    })?);
                }
                other => {
                    return Err(CliError::Usage(format!("unknown option '{other}'")));
                }
            }
        }
        out.validate()?;
        Ok(out)
    }

    /// The 1-based line `path` is to open at: its own `:LINE`, else `--line` for the first
    /// path.
    pub fn line_of(&self, path: &Path) -> Option<usize> {
        let at = self
            .paths
            .iter()
            .position(|p| p == path || crate::paths::paths_equal(p, path))?;
        self.path_lines
            .get(at)
            .copied()
            .flatten()
            .or(self.line.filter(|_| at == 0))
    }

    /// The include term the window applies: the last `--filter` given, as before print
    /// mode made the option repeatable.
    /// The path a file of the command line opens as: with `--rotations`, the file with
    /// its rotations (`rotated:<file>`).
    pub fn opened_path(&self, path: &std::path::Path) -> std::path::PathBuf {
        if self.rotations && path.is_file() {
            crate::rotation::identity(path)
        } else {
            path.to_path_buf()
        }
    }

    pub fn window_filter(&self) -> Option<&String> {
        self.filter.last()
    }

    /// The exclude term the window applies: the last `--exclude` given.
    pub fn window_exclude(&self) -> Option<&String> {
        self.exclude.last()
    }

    /// The interface to start: `--tui` or `--gui`, else `configured` (`interface` in
    /// `fasttail.ini`).
    pub fn interface(&self, configured: Interface) -> Interface {
        if self.tui {
            Interface::Tui
        } else if self.gui {
            Interface::Gui
        } else {
            configured
        }
    }

    /// Checks what the options mean together: one interface at most; with `--print`, at
    /// most `MAX_FILTER_TERMS` terms per side; without it, no print-only option.
    fn validate(&self) -> Result<(), CliError> {
        use crate::scan_job::MAX_FILTER_TERMS;
        if self.tui && self.gui {
            return Err(CliError::Usage(
                "--tui and --gui cannot be given together".to_string(),
            ));
        }
        if let Some(expr) = &self.filter_expr {
            if !self.filter.is_empty() {
                return Err(CliError::Usage(
                    "--filter-expr and --filter cannot be given together".to_string(),
                ));
            }
            let toggles = crate::filter_expr::Toggles {
                case_sensitive: self.case_sensitive,
                is_regex: self.regex,
                fields: false,
            };
            if let Err(e) = crate::filter_expr::FilterExpr::compile(expr, toggles) {
                let message = crate::i18n::t(crate::i18n::Language::En, e.key);
                let detail = e.detail.map(|d| format!(" ({d})")).unwrap_or_default();
                return Err(CliError::Usage(format!(
                    "--filter-expr: {message} at character {}{detail}",
                    expr.char_indices().take_while(|(i, _)| *i < e.pos).count() + 1
                )));
            }
        }
        for (name, terms) in [("--filter", &self.filter), ("--exclude", &self.exclude)] {
            if self.print && terms.len() > MAX_FILTER_TERMS {
                return Err(CliError::Usage(format!(
                    "'{name}' given {} times (at most {MAX_FILTER_TERMS})",
                    terms.len()
                )));
            }
        }
        match (self.output, &self.template) {
            (Some(PrintOutput::Template), None) => {
                return Err(CliError::Usage(
                    "--output template needs --template <TEXT>".to_string(),
                ))
            }
            (o, Some(_)) if o != Some(PrintOutput::Template) => {
                return Err(CliError::Usage(
                    "--template is only valid with --output template".to_string(),
                ))
            }
            _ => {}
        }
        if !self.print {
            let print_only = [
                ("--output", self.output.is_some()),
                ("--template", self.template.is_some()),
                ("--format", self.parser_format.is_some()),
                ("--level", self.level.is_some()),
                ("--level-bytes", self.level_bytes.is_some()),
                ("--context", self.context.is_some()),
                ("--color", self.color.is_some()),
                ("--line-numbers", self.line_numbers),
                ("--no-prefix", self.no_prefix),
                ("--regex", self.regex),
                ("--case-sensitive", self.case_sensitive),
            ];
            if let Some((name, _)) = print_only.iter().find(|(_, given)| *given) {
                return Err(CliError::Usage(format!(
                    "option '{name}' is only valid with --print"
                )));
            }
        }
        Ok(())
    }
}

/// The theme `--theme` names, in any case.
fn parse_theme(name: &str) -> Option<CyberTheme> {
    match name.trim().to_ascii_lowercase().as_str() {
        "tron" => Some(CyberTheme::Tron),
        "matrix" => Some(CyberTheme::Matrix),
        "blade" => Some(CyberTheme::Blade),
        "light" => Some(CyberTheme::Light),
        "commander" => Some(CyberTheme::Commander),
        _ => None,
    }
}

/// Applies `--since` / `--until` to a stream opened from the command line, as typed in
/// the time range popup: a relative value (`-15m`) makes a live window, which slides with
/// the clock on the stream's display clock. Both interfaces call it.
pub fn apply_time_window(
    engine: &mut crate::tail_engine::TailEngine,
    since: Option<&str>,
    until: Option<&str>,
) {
    if since.is_none() && until.is_none() {
        return;
    }
    let text = |value: Option<&str>| value.unwrap_or_default().to_string();
    let (from_ok, to_ok) = engine.apply_time_range_text(&text(since), &text(until));
    engine.time_range_error = !from_ok || !to_ok;
}

/// `arg` as a path and its line target: `app.log:1204` or `app.log:1204:7` (as compilers
/// print locations; the column is ignored) when `app.log` exists and the whole argument
/// does not, so a drive letter (`C:\logs\a.log`) and a file really named `trace:12` stay
/// paths. A line 0 or too large for a number is not a target.
fn split_line_target(arg: &str, cwd: &Path) -> (PathBuf, Option<usize>) {
    // A system source (`docker://api`, `journald://nginx.service`) is kept as written.
    if crate::system_source::is_system_path(Path::new(arg)) {
        return (PathBuf::from(arg), None);
    }
    let whole = resolve(arg, cwd);
    if whole.exists() {
        return (whole, None);
    }
    let number = |s: &str| {
        (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
            .then(|| s.parse::<usize>().ok())
            .flatten()
    };
    let mut parts = arg.rsplitn(3, ':');
    let last = parts.next().unwrap_or_default();
    let (base, line) = match (parts.next(), parts.next()) {
        // `path:line:column`
        (Some(line), Some(base)) if number(line).is_some() && number(last).is_some() => {
            (base, number(line))
        }
        // `path:line` (`base` may itself hold a colon, as `C:\a.log:12` does)
        _ => match arg.rsplit_once(':') {
            Some((base, line)) if number(line).is_some() => (base, number(line)),
            _ => return (whole, None),
        },
    };
    let path = resolve(base, cwd);
    match line.filter(|n| *n >= 1) {
        Some(n) if !base.is_empty() && path.exists() => (path, Some(n)),
        _ => (whole, None),
    }
}

fn resolve(arg: &str, cwd: &Path) -> PathBuf {
    let p = PathBuf::from(arg);
    if p.is_absolute() {
        p
    } else {
        cwd.join(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listen_urls_are_streams() {
        let a = CliArgs::parse(
            ["--listen", "syslog://127.0.0.1:5514", "udp://[::1]:9"],
            &cwd(),
        )
        .unwrap();
        assert_eq!(
            a.paths,
            [
                PathBuf::from("syslog://127.0.0.1:5514"),
                PathBuf::from("udp://[::1]:9")
            ]
        );
        assert!(CliArgs::parse(["--listen", "app.log"], &cwd()).is_err());
    }

    #[test]
    fn system_source_uris_are_kept_as_written() {
        let a = CliArgs::parse(
            [
                "docker://api",
                "journald://nginx.service",
                "dbwin://local",
                "x.log",
            ],
            &cwd(),
        )
        .unwrap();
        assert_eq!(
            a.paths,
            [
                PathBuf::from("docker://api"),
                PathBuf::from("journald://nginx.service"),
                PathBuf::from("dbwin://local"),
                cwd().join("x.log")
            ]
        );
        assert_eq!(a.path_lines, [None, None, None, None]);
    }

    fn cwd() -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(r"C:\work")
        } else {
            PathBuf::from("/work")
        }
    }

    #[test]
    fn positional_paths_are_resolved_against_cwd() {
        let a = CliArgs::parse(
            ["app.log", &cwd().join("abs.log").to_string_lossy()],
            &cwd(),
        )
        .unwrap();
        assert_eq!(a.paths[0], cwd().join("app.log"));
        assert_eq!(a.paths[1], cwd().join("abs.log"));
        assert!(!a.fresh && a.filter.is_empty() && a.follow.is_none());
    }

    #[test]
    fn options_in_both_forms() {
        let a = CliArgs::parse(
            [
                "--fresh",
                "--filter",
                "ERROR",
                "--exclude=health",
                "--no-follow",
                "--renderer",
                "wgpu",
                "--config",
                "cfg.ini",
                "x.log",
            ],
            &cwd(),
        )
        .unwrap();
        assert!(a.fresh);
        assert_eq!(a.filter, vec!["ERROR"]);
        assert_eq!(a.exclude, vec!["health"]);
        assert_eq!(a.follow, Some(false));
        assert_eq!(a.renderer, Some(RendererChoice::Wgpu));
        assert_eq!(a.config, Some(cwd().join("cfg.ini")));
        assert_eq!(a.paths, vec![cwd().join("x.log")]);
    }

    #[test]
    fn software_renderer_accepted() {
        let a = CliArgs::parse(["--renderer", "cpu"], &cwd()).unwrap();
        assert_eq!(a.renderer, Some(RendererChoice::Software));
    }

    #[test]
    fn double_dash_ends_options() {
        let a = CliArgs::parse(["--", "--fresh", "-h"], &cwd()).unwrap();
        assert_eq!(a.paths, vec![cwd().join("--fresh"), cwd().join("-h")]);
        assert!(!a.fresh && !a.show_help);
    }

    #[test]
    fn dash_means_standard_input() {
        let a = CliArgs::parse(["-"], &cwd()).unwrap();
        assert!(a.stdin);
        assert!(a.paths.is_empty());
        // After `--` too, as in `cat`.
        let a = CliArgs::parse(["--", "-"], &cwd()).unwrap();
        assert!(a.stdin && a.paths.is_empty());
        // With a filter and other paths, in any order.
        let a = CliArgs::parse(["--filter", "ERROR", "app.log", "-", "err.log"], &cwd()).unwrap();
        assert!(a.stdin);
        assert_eq!(a.filter, vec!["ERROR"]);
        assert_eq!(a.paths, vec![cwd().join("app.log"), cwd().join("err.log")]);
        let a = CliArgs::parse(["app.log"], &cwd()).unwrap();
        assert!(!a.stdin);
    }

    #[test]
    fn dash_twice_is_a_usage_error() {
        assert!(matches!(
            CliArgs::parse(["-", "-"], &cwd()),
            Err(CliError::Usage(m)) if m.contains("standard input")
        ));
        assert!(matches!(
            CliArgs::parse(["-", "--", "-"], &cwd()),
            Err(CliError::Usage(_))
        ));
    }

    #[test]
    fn a_file_named_dash_is_reached_as_dot_slash_dash() {
        let a = CliArgs::parse(["./-"], &cwd()).unwrap();
        assert!(!a.stdin);
        assert_eq!(a.paths, vec![cwd().join("./-")]);
        assert_eq!(a.paths[0].file_name().unwrap(), "-");
    }

    #[test]
    fn help_and_version_flags() {
        assert!(CliArgs::parse(["-h"], &cwd()).unwrap().show_help);
        assert!(CliArgs::parse(["--version"], &cwd()).unwrap().show_version);
    }

    #[test]
    fn interface_flags_override_the_ini() {
        let a = CliArgs::parse(["--gui"], &cwd()).unwrap();
        assert!(a.gui && !a.tui);
        assert_eq!(a.interface(Interface::Tui), Interface::Gui);
        let a = CliArgs::parse(["--tui", "app.log"], &cwd()).unwrap();
        assert_eq!(a.interface(Interface::Gui), Interface::Tui);
        let a = CliArgs::parse(["app.log"], &cwd()).unwrap();
        assert_eq!(a.interface(Interface::Tui), Interface::Tui);
        assert_eq!(a.interface(Interface::Gui), Interface::Gui);
    }

    #[test]
    fn value_options_are_exactly_those_that_need_a_value() {
        for name in VALUE_OPTIONS {
            let m = usage_error(&["--print", name]);
            assert!(m.contains(name) && m.contains("needs"), "{name}: {m}");
        }
        for flag in [
            "--tui",
            "--gui",
            "--fresh",
            "--follow",
            "--split",
            "--ascii",
            "--print",
            "--all",
            "--mcp",
            "--rotations",
        ] {
            assert!(!VALUE_OPTIONS.contains(&flag));
            assert!(CliArgs::parse(["--print", flag], &cwd()).is_ok(), "{flag}");
        }
    }

    #[test]
    fn tui_and_gui_together_are_a_usage_error() {
        assert!(usage_error(&["--tui", "--gui"]).contains("--tui and --gui"));
        assert!(usage_error(&["--gui", "app.log", "--tui"]).contains("--tui and --gui"));
    }

    #[test]
    fn terminal_options_are_accepted_by_every_executable() {
        let a = CliArgs::parse(
            [
                "--split",
                "--search",
                "timeout",
                "--theme=Matrix",
                "--ascii",
                "--no-mouse",
                "app.log",
            ],
            &cwd(),
        )
        .unwrap();
        assert!(a.split && a.ascii && a.no_mouse);
        assert_eq!(a.search.as_deref(), Some("timeout"));
        assert_eq!(a.theme, Some(CyberTheme::Matrix));
        assert_eq!(a.paths, vec![cwd().join("app.log")]);
        for (name, theme) in [
            ("tron", CyberTheme::Tron),
            ("blade", CyberTheme::Blade),
            ("LIGHT", CyberTheme::Light),
            ("commander", CyberTheme::Commander),
        ] {
            let a = CliArgs::parse(["--theme", name], &cwd()).unwrap();
            assert_eq!(a.theme, Some(theme), "{name}");
        }
        assert!(usage_error(&["--theme", "neon"]).contains("neon"));
        assert!(usage_error(&["--search"]).contains("--search"));
        // The window ignores them, and they leave the rest alone.
        let a = CliArgs::parse(["--no-mouse"], &cwd()).unwrap();
        assert!(a.paths.is_empty() && !a.gui && !a.tui);
    }

    #[test]
    fn both_usages_list_the_shared_and_terminal_options() {
        for usage in [USAGE, TUI_USAGE] {
            for option in [
                "--tui",
                "--gui",
                "--fresh",
                "--since",
                "--renderer",
                "--split",
                "--search",
                "--theme",
                "--ascii",
                "--no-mouse",
                "--print",
            ] {
                assert!(usage.contains(option), "{option}");
            }
        }
        assert!(TUI_USAGE.starts_with("fasttail-tui"));
        assert!(
            TUI_USAGE.contains("FASTTAIL_TUI_COLORS") && TUI_USAGE.contains("FASTTAIL_TUI_ASCII")
        );
        assert!(!USAGE.contains("FASTTAIL_TUI_COLORS"));
    }

    #[test]
    fn errors_are_usage_errors() {
        assert!(matches!(
            CliArgs::parse(["--bogus"], &cwd()),
            Err(CliError::Usage(m)) if m.contains("--bogus")
        ));
        assert!(matches!(
            CliArgs::parse(["--filter"], &cwd()),
            Err(CliError::Usage(m)) if m.contains("--filter")
        ));
        assert!(matches!(
            CliArgs::parse(["--renderer", "vulkan"], &cwd()),
            Err(CliError::Usage(m)) if m.contains("vulkan")
        ));
    }

    #[test]
    fn filter_expr_is_checked_and_excludes_filter() {
        let a = CliArgs::parse(["--filter-expr", "a OR (b NOT c)", "x.log"], &cwd()).unwrap();
        assert_eq!(a.filter_expr.as_deref(), Some("a OR (b NOT c)"));
        assert!(VALUE_OPTIONS.contains(&"--filter-expr"));
        assert!(usage_error(&["--filter-expr", "a", "--filter", "b"]).contains("--filter"));
        let bad = usage_error(&["--filter-expr", "(a OR"]);
        assert!(bad.contains("character 6"), "{bad}");
        assert!(
            usage_error(&["--print", "--regex", "--filter-expr", "ok ("]).contains("--filter-expr")
        );
        // Too long with multi-byte characters: no panic, a position in characters.
        let long = "€".repeat(400);
        assert!(usage_error(&["--filter-expr", &long]).contains("character 342"));
    }

    fn usage_error(args: &[&str]) -> String {
        match CliArgs::parse(args.iter().copied(), &cwd()) {
            Err(CliError::Usage(m)) => m,
            Ok(a) => panic!("{args:?} parsed: {a:?}"),
        }
    }

    #[test]
    fn print_output_options() {
        let a = CliArgs::parse(
            [
                "--print",
                "--output=template",
                "--template",
                "{time} {msg}",
                "--format",
                "logfmt",
                "app.log",
            ],
            &cwd(),
        )
        .unwrap();
        assert_eq!(a.output, Some(PrintOutput::Template));
        assert_eq!(a.template.as_deref(), Some("{time} {msg}"));
        assert_eq!(a.parser_format.as_deref(), Some("logfmt"));
        assert_eq!(
            CliArgs::parse(["--print", "--output", "JSON", "a.log"], &cwd())
                .unwrap()
                .output,
            Some(PrintOutput::Json)
        );
        assert!(usage_error(&["--print", "--output", "xml"]).contains("unknown output"));
        assert!(usage_error(&["--print", "--output", "template"]).contains("--template"));
        assert!(usage_error(&["--print", "--template", "{x}"]).contains("--output template"));
        assert!(usage_error(&["--output", "json"]).contains("only valid with --print"));
        assert!(usage_error(&["--format", "json"]).contains("only valid with --print"));
    }

    #[test]
    fn print_mode_options_in_both_forms() {
        let a = CliArgs::parse(
            [
                "--print",
                "--filter",
                "payment",
                "--filter=timeout",
                "--exclude",
                "healthcheck",
                "--regex",
                "--case-sensitive",
                "--level=error",
                "--since",
                "-15m",
                "--until=2026-09-28 14:05",
                "--context",
                "3",
                "--color=never",
                "--line-numbers",
                "--no-prefix",
                "--follow",
                "gateway.log",
            ],
            &cwd(),
        )
        .unwrap();
        assert!(a.print && a.regex && a.case_sensitive && a.line_numbers && a.no_prefix);
        assert_eq!(a.filter, vec!["payment", "timeout"]);
        assert_eq!(a.exclude, vec!["healthcheck"]);
        assert_eq!(a.level, Some(LogLevel::Error));
        assert_eq!(a.since.as_deref(), Some("-15m"));
        assert_eq!(a.until.as_deref(), Some("2026-09-28 14:05"));
        assert_eq!(a.context, Some(3));
        assert_eq!(a.color, Some(ColorChoice::Never));
        assert_eq!(a.follow, Some(true));
        assert_eq!(a.paths, vec![cwd().join("gateway.log")]);
        let a = CliArgs::parse(
            ["--print", "--level", "WARNING", "--color", "always"],
            &cwd(),
        )
        .unwrap();
        assert_eq!(a.level, Some(LogLevel::Warn));
        assert_eq!(a.color, Some(ColorChoice::Always));
    }

    #[test]
    fn a_line_target_is_split_off_only_when_unambiguous() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = dir.path();
        std::fs::write(cwd.join("app.log"), "x\n").unwrap();
        let parse = |args: &[&str]| CliArgs::parse(args.iter().copied(), cwd).unwrap();
        let log = cwd.join("app.log");

        let a = parse(&["app.log:1204"]);
        assert_eq!(
            (a.paths.clone(), a.line_of(&log)),
            (vec![log.clone()], Some(1204))
        );
        let a = parse(&["app.log:1204:7"]);
        assert_eq!(a.line_of(&log), Some(1204), "the column is ignored");
        // Missing file, line 0, a number too large, no digits: the argument is the path.
        for arg in [
            "gone.log:12",
            "app.log:0",
            "app.log:99999999999999999999999999",
            "app.log:x",
            "app.log:",
        ] {
            let a = parse(&[arg]);
            assert_eq!(a.paths, vec![cwd.join(arg)], "{arg}");
            assert_eq!(a.line_of(&a.paths[0]), None, "{arg}");
        }
        // A file really named with a colon and digits is opened as it is.
        #[cfg(unix)]
        {
            std::fs::write(cwd.join("trace:12"), "x\n").unwrap();
            let a = parse(&["trace:12"]);
            assert_eq!(a.paths, vec![cwd.join("trace:12")]);
            assert_eq!(a.line_of(&a.paths[0]), None);
        }
        // A drive letter is not a line.
        let (path, line) = split_line_target(r"C:\logs\a.log", cwd);
        assert_eq!((path, line), (resolve(r"C:\logs\a.log", cwd), None));
        // --line: the first path only; a path's own line wins.
        std::fs::write(cwd.join("b.log"), "x\n").unwrap();
        let b = cwd.join("b.log");
        let a = parse(&["--line", "40", "app.log", "b.log"]);
        assert_eq!((a.line_of(&log), a.line_of(&b)), (Some(40), None));
        let a = parse(&["--line", "40", "app.log:7", "b.log:9"]);
        assert_eq!((a.line_of(&log), a.line_of(&b)), (Some(7), Some(9)));
        assert!(usage_error(&["--line", "0"]).contains("invalid line"));
        assert!(usage_error(&["--line", "ten"]).contains("ten"));
        assert!(VALUE_OPTIONS.contains(&"--line"));
    }

    #[test]
    fn level_bytes_is_a_print_option_from_16_to_4096() {
        let a = CliArgs::parse(["--print", "--level-bytes", "256"], &cwd()).unwrap();
        assert_eq!(a.level_bytes, Some(256));
        assert!(usage_error(&["--print", "--level-bytes", "8"]).contains("16 to 4096"));
        assert!(usage_error(&["--print", "--level-bytes", "lots"]).contains("lots"));
        assert!(usage_error(&["--level-bytes", "256"]).contains("--print"));
        assert!(VALUE_OPTIONS.contains(&"--level-bytes"));
    }

    #[test]
    fn filter_terms_repeat_up_to_eight() {
        let mut args = vec!["--print"];
        for _ in 0..8 {
            args.extend(["--filter", "x", "--exclude", "y"]);
        }
        let a = CliArgs::parse(args.iter().copied(), &cwd()).unwrap();
        assert_eq!((a.filter.len(), a.exclude.len()), (8, 8));
        args.extend(["--filter", "ninth"]);
        assert!(usage_error(&args).contains("--filter"));
        let mut args = vec!["--print"];
        for _ in 0..9 {
            args.extend(["--exclude", "y"]);
        }
        assert!(usage_error(&args).contains("--exclude"));
    }

    #[test]
    fn invalid_print_values_are_usage_errors() {
        assert!(usage_error(&["--print", "--level", "loud"]).contains("loud"));
        assert!(usage_error(&["--print", "--color", "sometimes"]).contains("sometimes"));
        assert!(usage_error(&["--print", "--context", "101"]).contains("101"));
        assert!(usage_error(&["--print", "--context", "-1"]).contains("-1"));
        assert!(usage_error(&["--print", "--since", "yesterday"]).contains("yesterday"));
        assert!(usage_error(&["--print", "--until=-3x"]).contains("-3x"));
        assert!(usage_error(&["--print", "--level"]).contains("--level"));
    }

    #[test]
    fn print_only_options_need_print() {
        for opt in [
            &["--level", "error"][..],
            &["--context", "2"],
            &["--color", "never"],
            &["--line-numbers"],
            &["--no-prefix"],
            &["--regex"],
            &["--case-sensitive"],
        ] {
            let mut args = opt.to_vec();
            args.push("app.log");
            let m = usage_error(&args);
            assert!(m.contains(opt[0]) && m.contains("--print"), "{m}");
        }
        // The window takes the time window and repeated terms (it uses the last).
        let a = CliArgs::parse(
            [
                "--since", "14:02", "--until", "-1h", "--filter", "a", "--filter", "b",
            ],
            &cwd(),
        )
        .unwrap();
        assert!(!a.print);
        assert_eq!(a.since.as_deref(), Some("14:02"));
        assert_eq!(a.until.as_deref(), Some("-1h"));
        assert_eq!(a.filter, vec!["a", "b"]);
    }

    #[test]
    fn standard_input_keeps_its_place_among_the_paths() {
        let a = CliArgs::parse(["--print", "a.log", "-", "b.log"], &cwd()).unwrap();
        assert!(a.stdin);
        assert_eq!(a.stdin_at, 1);
        let a = CliArgs::parse(["--print", "-"], &cwd()).unwrap();
        assert!(a.stdin && a.paths.is_empty() && a.stdin_at == 0);
        let a = CliArgs::parse(["--print", "a.log"], &cwd()).unwrap();
        assert!(!a.stdin);
    }

    #[test]
    fn relative_times() {
        let now = 1_000_000_000_000;
        assert_eq!(
            resolve_time_arg("-1h", 0, now, false),
            Some(now - 3_600_000)
        );
        // A relative "to" is the exact instant, not widened to the end of a unit.
        assert_eq!(resolve_time_arg("-1h", 0, now, true), Some(now - 3_600_000));
        assert_eq!(resolve_time_arg("now", 0, now, true), Some(now));
        assert_eq!(
            resolve_time_arg("-1h30m", 0, now, false),
            Some(now - 90 * 60_000)
        );
        // A bare time on the reference day; the "to" side covers the whole minute.
        let day = 20_000 * 86_400_000;
        assert_eq!(
            resolve_time_arg("14:02", day + 5_000, now, false),
            Some(day + (14 * 60 + 2) * 60_000)
        );
        assert_eq!(
            resolve_time_arg("14:02", day + 5_000, now, true),
            Some(day + (14 * 60 + 2) * 60_000 + 59_999)
        );
        for ok in [
            "2026-09-28 14:02",
            "2026-09-28T14:02:03.120Z",
            "now",
            "-2w",
            "-45s",
        ] {
            assert!(is_valid_time_arg(ok), "{ok}");
        }
        assert!(!is_valid_time_arg("noon"));
        assert!(!is_valid_time_arg("-3x"));
    }

    #[test]
    fn without_print_the_last_filter_counts_and_there_is_no_term_limit() {
        let mut args = Vec::new();
        for i in 0..10 {
            args.push("--filter".to_string());
            args.push(format!("f{i}"));
            args.push("--exclude".to_string());
            args.push(format!("x{i}"));
        }
        let a = CliArgs::parse(args.iter(), &cwd()).unwrap();
        assert_eq!(a.window_filter().map(String::as_str), Some("f9"));
        assert_eq!(a.window_exclude().map(String::as_str), Some("x9"));
        let a = CliArgs::parse(["app.log"], &cwd()).unwrap();
        assert!(a.window_filter().is_none() && a.window_exclude().is_none());
    }
}
