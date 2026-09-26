//! Command line: `fasttail [OPTIONS] [PATH...]`.
//!
//! Hand-written parser: a handful of options and positional paths do not justify a
//! dependency. Paths are resolved against the current directory at parse time. A PATH of
//! exactly `-` means standard input (`stdin`), before or after `--` as in `cat`; a file
//! named `-` is reachable as `./-`.

use std::path::{Path, PathBuf};

use crate::renderer::RendererChoice;

pub const USAGE: &str = "\
FastTail - ultra-fast multi-stream log monitor

USAGE:
    fasttail [OPTIONS] [PATH...]

ARGS:
    PATH...              Log files to open in addition to the restored workspace
    -                    Read standard input (`command | fasttail -`); piped input is
                         also picked up without it

OPTIONS:
    --gui                Accepted and ignored: FastTail is GUI-only (kept for old shortcuts)
    --fresh              Start with an empty workspace instead of the saved one
    --filter <TEXT>      Include filter applied to the files opened from the command line
    --exclude <TEXT>     Exclude filter applied to the files opened from the command line
    --follow             Enable follow mode on the files opened from the command line
    --no-follow          Disable follow mode on those files
    --renderer <NAME>    Rendering backend: auto (default), glow, wgpu, software
    --config <FILE>      Use this configuration file (same as FASTTAIL_CONFIG)
    --session <FILE>     Load this session file (*.fasttail-session.ini) at startup
    -V, --version        Print the version and exit
    -h, --help           Print this help and exit
";

/// Parsed command line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CliArgs {
    pub paths: Vec<PathBuf>,
    /// `-` was given: read standard input as a stream.
    pub stdin: bool,
    pub fresh: bool,
    pub filter: Option<String>,
    pub exclude: Option<String>,
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
                "--filter" => out.filter = Some(value("text")?),
                "--exclude" => out.exclude = Some(value("text")?),
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
        Ok(out)
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
        assert!(!a.fresh && a.filter.is_none() && a.follow.is_none());
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
        assert_eq!(a.filter.as_deref(), Some("ERROR"));
        assert_eq!(a.exclude.as_deref(), Some("health"));
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
        assert_eq!(a.filter.as_deref(), Some("ERROR"));
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
}
