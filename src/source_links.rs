// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Source locations in log lines (`src/main.rs:120:5`, `at Foo.bar(Foo.java:42)`,
//! `File "app.py", line 12`, `in X.cs:line 33`, `--> src/x.rs:3:9`) and opening them in the
//! user's editor at that line.
//!
//! Locations are found with a short ordered list of patterns, only on the rows the user
//! points at (never during scans). A relative path is resolved against the source roots
//! of Settings, a Java or Kotlin frame through its package; the editor command is a
//! template whose `{file}`, `{line}` and `{col}` are each expanded inside one argument,
//! and it runs without a shell.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;

/// A source location found in a line: its bytes in the line, the path as written, the
/// line and column, and for a JVM frame the package the file is in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub start: usize,
    pub end: usize,
    pub path: String,
    pub line: u32,
    pub col: Option<u32>,
    /// `com.acme` for `at com.acme.Orders.save(Orders.java:42)`.
    pub package: Option<String>,
}

/// Where a request opens a location.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// The editor command of Settings.
    Editor,
    /// FastTail itself: the file as a stream, at that line.
    Stream,
}

/// Editor command presets: label, template.
pub const PRESETS: [(&str, &str); 6] = [
    ("VS Code", "code -g {file}:{line}:{col}"),
    ("JetBrains (IntelliJ IDEA)", "idea --line {line} {file}"),
    ("Sublime Text", "subl {file}:{line}:{col}"),
    ("Notepad++", "notepad++ -n{line} {file}"),
    (
        "Vim in a terminal",
        "x-terminal-emulator -e vim +{line} {file}",
    ),
    ("Emacs", "emacsclient -n +{line}:{col} {file}"),
];

/// A file extension: a letter, then up to 9 letters or digits.
const EXT: &str = r"\.[A-Za-z][A-Za-z0-9]{0,9}";

struct Patterns {
    /// Each with the groups `path`, `line`, optional `col` and `pkg`.
    list: Vec<Regex>,
}

fn patterns() -> &'static Patterns {
    static P: OnceLock<Patterns> = OnceLock::new();
    P.get_or_init(|| {
        let sources = [
            // JVM frames: `at com.acme.Orders.save(Orders.java:42)`.
            r"at (?P<pkg>(?:[A-Za-z_$][\w$]*\.)*)[A-Za-z_$][\w$]*\.[\w$<>]+\((?P<path>[\w$]+\.(?:java|kt|kts|scala|groovy|clj)):(?P<line>\d+)\)".to_string(),
            // Python: `File "app/x.py", line 12`.
            r#"File "(?P<path>[^"]+)", line (?P<line>\d+)"#.to_string(),
            // .NET: `in C:\src\X.cs:line 33`.
            r"\bin (?P<path>(?:[A-Za-z]:)?[^\s:]+\.(?:cs|vb|fs)):line (?P<line>\d+)".to_string(),
            // Rust and compilers: `--> src/x.rs:3:9`.
            r"--> (?P<path>[^\s:]+):(?P<line>\d+):(?P<col>\d+)".to_string(),
            // Any `path.ext:line[:col]` (Go, Node, gcc, test runners, `file:line`).
            format!(
                r"(?P<path>(?:\b[A-Za-z]:[\\/])?[\w.~\\/-]*[\w-]{EXT}):(?P<line>\d+)(?::(?P<col>\d+))?"
            ),
        ];
        Patterns {
            list: sources
                .iter()
                .map(|s| Regex::new(s).expect("a valid source location pattern"))
                .collect(),
        }
    })
}

/// The source locations of `line`, in the order they appear; an earlier pattern wins
/// where two overlap. Times (`12:30:45`), host names with a port in a URL and IP
/// addresses are not locations.
pub fn find(line: &str) -> Vec<Location> {
    let mut out: Vec<Location> = Vec::new();
    for re in &patterns().list {
        for caps in re.captures_iter(line) {
            let (Some(path), Some(num)) = (caps.name("path"), caps.name("line")) else {
                continue;
            };
            let whole = caps.get(0).expect("the whole match");
            let (start, end) = (path.start().min(whole.start()), whole.end());
            if out.iter().any(|l| start < l.end && l.start < end) {
                continue;
            }
            // `http://host.example.com:8080` or `user@host.com:22` is no file.
            let before = &line[..path.start()];
            if before.ends_with("//") || before.ends_with('@') || path.as_str().starts_with("//") {
                continue;
            }
            let Ok(number) = num.as_str().parse::<u32>() else {
                continue;
            };
            if number == 0 {
                continue;
            }
            out.push(Location {
                start,
                end,
                path: path.as_str().to_string(),
                line: number,
                col: caps.name("col").and_then(|c| c.as_str().parse().ok()),
                package: caps
                    .name("pkg")
                    .map(|p| p.as_str().trim_end_matches('.').to_string())
                    .filter(|p| !p.is_empty()),
            });
        }
    }
    out.sort_by_key(|l| l.start);
    out
}

/// The location whose bytes hold `at`, if any.
pub fn at(line: &str, at: usize) -> Option<Location> {
    find(line).into_iter().find(|l| l.start <= at && at < l.end)
}

/// The file `loc` names: an absolute path that exists, else the first source root that
/// holds it as written, or (a JVM frame) under its package's folders.
pub fn resolve(loc: &Location, roots: &[PathBuf]) -> Option<PathBuf> {
    let written = Path::new(loc.path.trim_start_matches("./"));
    if written.is_absolute() {
        return written.is_file().then(|| written.to_path_buf());
    }
    for root in roots {
        let direct = root.join(written);
        if direct.is_file() {
            return Some(direct);
        }
        if let Some(pkg) = &loc.package {
            let mut nested = root.clone();
            for part in pkg.split('.') {
                nested.push(part);
            }
            nested.push(written);
            if nested.is_file() {
                return Some(nested);
            }
        }
    }
    None
}

/// The program and arguments that open `file` at `line` (`col`, 1 when unknown) with the
/// editor command `template`; `None` for an empty template (the system opener then).
pub fn command(
    template: &str,
    file: &Path,
    line: u32,
    col: Option<u32>,
) -> Option<(String, Vec<String>)> {
    let file = file.to_string_lossy();
    let (line, col) = (line.to_string(), col.unwrap_or(1).to_string());
    let mut words = crate::external_tools::split_args(template)
        .into_iter()
        .map(|w| {
            w.replace("{file}", &file)
                .replace("{line}", &line)
                .replace("{col}", &col)
        });
    let program = words.next()?;
    Some((program, words.collect()))
}

/// Opens `file` at `line` with `template` (the system opener for an empty one), without
/// a shell; the error as text.
pub fn open_in_editor(
    template: &str,
    file: &Path,
    line: u32,
    col: Option<u32>,
) -> Result<(), String> {
    let (program, args) = command(template, file, line, col).unwrap_or_else(|| {
        let opener = if cfg!(windows) {
            "explorer"
        } else if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        (
            opener.to_string(),
            vec![file.to_string_lossy().into_owned()],
        )
    });
    let mut cmd = std::process::Command::new(&program);
    cmd.args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    match cmd.spawn() {
        Ok(mut child) => {
            // Reaped on a thread: an editor may run for hours.
            let _ = std::thread::Builder::new()
                .name("fasttail-editor".into())
                .spawn(move || {
                    let _ = child.wait();
                });
            Ok(())
        }
        Err(e) => Err(format!("{program}: {e}")),
    }
}

/// `source_roots=` of `fasttail.ini`: folders separated by `;`.
pub fn parse_roots(text: &str) -> Vec<PathBuf> {
    text.split(';')
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .map(PathBuf::from)
        .collect()
}

pub fn roots_text(roots: &[PathBuf]) -> String {
    roots
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(line: &str) -> Location {
        let found = find(line);
        assert_eq!(found.len(), 1, "{line}: {found:?}");
        found.into_iter().next().unwrap()
    }

    #[test]
    fn frames_of_each_language_are_found() {
        let java = one("\tat com.acme.Orders.save(Orders.java:42)");
        assert_eq!((java.path.as_str(), java.line), ("Orders.java", 42));
        assert_eq!(java.package.as_deref(), Some("com.acme"));
        let kotlin = one("    at app.Main$run$1.invoke(Main.kt:7)");
        assert_eq!(
            (kotlin.path.as_str(), kotlin.package.as_deref()),
            ("Main.kt", Some("app"))
        );
        let py = one(r#"  File "/srv/app/views.py", line 12, in index"#);
        assert_eq!((py.path.as_str(), py.line), ("/srv/app/views.py", 12));
        let net = one(r"   at Shop.Orders.Save() in C:\src\Shop\Orders.cs:line 33");
        assert_eq!(
            (net.path.as_str(), net.line),
            (r"C:\src\Shop\Orders.cs", 33)
        );
        let rust = one("  --> src/main.rs:3:9");
        assert_eq!(
            (rust.path.as_str(), rust.line, rust.col),
            ("src/main.rs", 3, Some(9))
        );
        let go = one("main.go:42: undefined: foo");
        assert_eq!((go.path.as_str(), go.line, go.col), ("main.go", 42, None));
        let node = one("    at handler (/app/server.js:10:5)");
        assert_eq!(
            (node.path.as_str(), node.line, node.col),
            ("/app/server.js", 10, Some(5))
        );
        let panic = one("thread 'main' panicked at src/lib.rs:120:5:");
        assert_eq!((panic.path.as_str(), panic.line), ("src/lib.rs", 120));
    }

    #[test]
    fn times_hosts_and_addresses_are_not_locations() {
        assert!(find("12:30:45 INFO started").is_empty());
        assert!(find("2026-10-28 12:30:45.123 INFO ok").is_empty());
        assert!(find("GET http://api.example.com:8080/x").is_empty());
        assert!(find("connect 10.0.0.1:5432 refused").is_empty());
        assert!(find("ssh deploy@web1.example.com:22").is_empty());
        assert!(find("at line 0: a.rs:0").is_empty(), "line 0 is no line");
    }

    #[test]
    fn the_location_under_a_byte_and_several_in_a_line() {
        let line = "a.rs:1 then b.go:2";
        assert_eq!(find(line).len(), 2);
        assert_eq!(at(line, 13).unwrap().path, "b.go");
        assert!(at(line, 8).is_none());
    }

    #[test]
    fn relative_paths_resolve_against_the_roots() {
        let dir = tempfile::tempdir().unwrap();
        let java = dir.path().join("src/main/java/com/acme");
        std::fs::create_dir_all(&java).unwrap();
        std::fs::write(java.join("Orders.java"), "class Orders {}").unwrap();
        std::fs::create_dir_all(dir.path().join("svc/src")).unwrap();
        std::fs::write(dir.path().join("svc/src/main.rs"), "fn main() {}").unwrap();
        let roots = vec![
            dir.path().join("nothing"),
            dir.path().join("src/main/java"),
            dir.path().join("svc"),
        ];
        let frame = one("at com.acme.Orders.save(Orders.java:42)");
        assert_eq!(resolve(&frame, &roots), Some(java.join("Orders.java")));
        let rust = one("--> src/main.rs:3:9");
        assert_eq!(
            resolve(&rust, &roots),
            Some(dir.path().join("svc/src/main.rs"))
        );
        assert_eq!(resolve(&one("src/missing.rs:3"), &roots), None);
        let absolute = dir.path().join("svc/src/main.rs");
        let loc = Location {
            path: absolute.to_string_lossy().into_owned(),
            ..rust.clone()
        };
        assert_eq!(resolve(&loc, &[]), Some(absolute));
    }

    #[test]
    fn the_editor_command_keeps_each_argument_whole() {
        let file = Path::new("/my src/a b.rs");
        let (program, args) = command("code -g {file}:{line}:{col}", file, 12, None).unwrap();
        assert_eq!(program, "code");
        assert_eq!(
            args,
            vec!["-g".to_string(), "/my src/a b.rs:12:1".to_string()]
        );
        let (_, args) = command("idea --line {line} {file}", file, 7, Some(3)).unwrap();
        assert_eq!(args, vec!["--line", "7", "/my src/a b.rs"]);
        assert!(command("  ", file, 1, None).is_none());
        assert_eq!(
            parse_roots(" /a ; ;D:\\src"),
            vec![PathBuf::from("/a"), PathBuf::from("D:\\src")]
        );
    }
}
