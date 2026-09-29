// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Command line: `fasttail [OPTIONS] [PATH...]`.
//!
//! Hand-written parser: a handful of options and positional paths do not justify a
//! dependency. Paths are resolved against the current directory at parse time. A PATH of
//! exactly `-` means standard input (`stdin`), before or after `--` as in `cat`; a file
//! named `-` is reachable as `./-`.

use std::path::{Path, PathBuf};

use crate::log_level::LogLevel;
use crate::renderer::RendererChoice;

pub const USAGE: &str = "\
FastTail - ultra-fast multi-stream log monitor

USAGE:
    fasttail [OPTIONS] [PATH...]
    fasttail --print [OPTIONS] [PATH...]

ARGS:
    PATH...              Log files to open in addition to the restored workspace
    -                    Read standard input (`command | fasttail -`); piped input is
                         also picked up without it

OPTIONS:
    --gui                Accepted and ignored: FastTail is GUI-only (kept for old shortcuts)
    --fresh              Start with an empty workspace instead of the saved one
    --filter <TEXT>      Include filter applied to the files opened from the command line
                         (given twice, the last one counts)
    --exclude <TEXT>     Exclude filter applied to the files opened from the command line
    --since <TIME>       Start of the time window of the files opened from the command line:
                         14:02, 2026-09-28 14:02, a timestamp copied from a line, or a
                         relative time: now, -15m, -3h, -1h30m, -2d, -1w (units s, m, h,
                         d, w, counted back from now; fixed at start in the window)
    --until <TIME>       End of that time window, in the same forms
    --follow             Enable follow mode on the files opened from the command line
    --no-follow          Disable follow mode on those files
    --renderer <NAME>    Rendering backend: auto (default), glow, wgpu, software
    --config <FILE>      Use this configuration file (same as FASTTAIL_CONFIG)
    --session <FILE>     Load this session file (*.fasttail-session.ini) at startup
    -V, --version        Print the version and exit
    -h, --help           Print this help and exit

PRINT MODE:
    --print              Write the lines that pass the filters to standard output and exit,
                         without opening a window (PATH... or `-`; piped input without PATH)
    --filter <TEXT>      Repeatable up to 8 times: every include term must match
    --exclude <TEXT>     Repeatable up to 8 times: any exclude term hides the line
    --regex              The filter terms are regular expressions
    --case-sensitive     The filter terms match case-sensitively
    --level <LEVEL>      Minimum level: trace, debug, info, warn, error or fatal
    --since, --until     The time window, as above
    --context <N>        Print N lines (0 to 100) before and after each match, `--` between
                         groups
    --follow             Then keep printing the lines appended to the files
    --color <WHEN>       auto (default: when writing to a terminal and NO_COLOR is not set),
                         always or never
    --line-numbers       Prefix each line with its line number
    --no-prefix          Do not prefix each line with its file name when several are given
    Exit codes: 0 lines printed, 1 no line matched, 2 usage error, 3 an input could not be
    read (the others are still printed)
";

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
    /// `-` was given: read standard input as a stream.
    pub stdin: bool,
    /// Where `-` stood among the paths (print mode reads the inputs in order).
    pub stdin_at: usize,
    pub fresh: bool,
    /// Include terms in the order given: print mode uses them all (at most
    /// `MAX_FILTER_TERMS`), the window the last one (`window_filter`).
    pub filter: Vec<String>,
    /// Exclude terms, as `filter` (`window_exclude`).
    pub exclude: Vec<String>,
    /// Start and end of the time window, as typed (validated at parse time).
    pub since: Option<String>,
    pub until: Option<String>,
    /// `--print`: headless print mode (see `print_mode`).
    pub print: bool,
    pub regex: bool,
    pub case_sensitive: bool,
    /// Minimum level (`--level`), never `Unknown`.
    pub level: Option<LogLevel>,
    pub context: Option<usize>,
    pub color: Option<ColorChoice>,
    pub line_numbers: bool,
    pub no_prefix: bool,
    pub follow: Option<bool>,
    pub renderer: Option<RendererChoice>,
    pub config: Option<PathBuf>,
    /// Session file to load at startup, replacing the restored workspace.
    pub session: Option<PathBuf>,
    /// Accepted and ignored: FastTail has been GUI-only since 0.7.1, and the flag is
    /// kept so shortcuts and scripts written for the older build keep working.
    pub gui: bool,
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
                out.paths.push(resolve(arg, cwd));
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
                "--fresh" => out.fresh = true,
                "--follow" => out.follow = Some(true),
                "--no-follow" => out.follow = Some(false),
                "-V" | "--version" => out.show_version = true,
                "-h" | "--help" => out.show_help = true,
                "--filter" => out.filter.push(value("text")?),
                "--exclude" => out.exclude.push(value("text")?),
                "--print" => out.print = true,
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

    /// The include term the window applies: the last `--filter` given, as before print
    /// mode made the option repeatable.
    pub fn window_filter(&self) -> Option<&String> {
        self.filter.last()
    }

    /// The exclude term the window applies: the last `--exclude` given.
    pub fn window_exclude(&self) -> Option<&String> {
        self.exclude.last()
    }

    /// Checks what the options mean together: with `--print`, at most `MAX_FILTER_TERMS`
    /// terms per side; without it, no print-only option.
    fn validate(&self) -> Result<(), CliError> {
        use crate::scan_job::MAX_FILTER_TERMS;
        for (name, terms) in [("--filter", &self.filter), ("--exclude", &self.exclude)] {
            if self.print && terms.len() > MAX_FILTER_TERMS {
                return Err(CliError::Usage(format!(
                    "'{name}' given {} times (at most {MAX_FILTER_TERMS})",
                    terms.len()
                )));
            }
        }
        if !self.print {
            let print_only = [
                ("--level", self.level.is_some()),
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
    fn gui_flag() {
        let a = CliArgs::parse(["--gui"], &cwd()).unwrap();
        assert!(a.gui);
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

    fn usage_error(args: &[&str]) -> String {
        match CliArgs::parse(args.iter().copied(), &cwd()) {
            Err(CliError::Usage(m)) => m,
            Ok(a) => panic!("{args:?} parsed: {a:?}"),
        }
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
