// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! From a log message to the logging statement that wrote it (as lnav's `log2src`).
//!
//! The source folders of Settings (`source_roots`) are walked once, on a background
//! thread, and the logging calls of Rust, Java / Kotlin, Python, Go, C# and JavaScript /
//! TypeScript files are read with a few regexes per language: `info!("payment {} declined",
//! id)`, `logger.info("...")`, `log.Printf("...")`. Each call's format string becomes a
//! pattern, its placeholders (`{}`, `{0}`, `{name}`, `%s`, `%d`, `%v`, `${x}`) matching any
//! text, its literal words matched as written. A row is matched by the statements whose
//! longest literal it contains, then whose whole pattern it matches; the longest literal
//! text wins.
//!
//! No language is parsed fully: format strings built at run time, or calls split over
//! lines before the string, are not found. That is the trade-off for no new crate.

use regex::Regex;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::LazyLock;

/// Files read at most per index, and the largest one read.
pub const MAX_FILES: usize = 50_000;
pub const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
/// Folder levels walked below a source root.
const MAX_DEPTH: usize = 24;
/// A statement whose literal text is shorter than this is too generic to point at.
const MIN_LITERAL: usize = 6;
/// Folders never walked.
const SKIPPED_DIRS: [&str; 10] = [
    ".git",
    ".hg",
    ".svn",
    "target",
    "node_modules",
    "build",
    "dist",
    "vendor",
    ".venv",
    "__pycache__",
];

/// A logging statement found in the sources.
#[derive(Debug, Clone)]
pub struct Statement {
    pub file: PathBuf,
    /// 1-based line of the call.
    pub line: u32,
    /// The format string as written.
    pub format: String,
    /// The literal parts of the format string, the longest first checked.
    literals: Vec<String>,
    pattern: Regex,
}

impl Statement {
    /// Characters of literal text: the more, the more specific the statement.
    pub fn weight(&self) -> usize {
        self.literals.iter().map(String::len).sum()
    }

    fn matches(&self, row: &str) -> bool {
        let Some(longest) = self.literals.iter().max_by_key(|l| l.len()) else {
            return false;
        };
        row.contains(longest.as_str()) && self.pattern.is_match(row)
    }
}

const STR: &str = r#""((?:[^"\\]|\\.)*)""#;

/// The regexes of the logging calls of a file with extension `ext`; the format string is
/// the first group.
fn patterns(ext: &str) -> &'static [Regex] {
    static RUST: LazyLock<Vec<Regex>> = LazyLock::new(|| {
        vec![Regex::new(&format!(
            r#"\b(?:trace|debug|info|warn|error|println|eprintln|panic)!\s*\(\s*(?:target\s*:\s*[^,]+,\s*)?{STR}"#
        ))
        .expect("rust")]
    });
    static JVM: LazyLock<Vec<Regex>> = LazyLock::new(|| {
        vec![Regex::new(&format!(
            r#"\b(?:log|logger|LOG|LOGGER|_log|_logger)\s*\.\s*(?:trace|debug|info|warn|warning|error|fatal)\s*\(\s*{STR}"#
        ))
        .expect("jvm")]
    });
    static PYTHON: LazyLock<Vec<Regex>> = LazyLock::new(|| {
        vec![Regex::new(
            r#"\b(?:logging|log|logger|_log|_logger|LOG)\s*\.\s*(?:debug|info|warning|warn|error|exception|critical)\s*\(\s*[fFrR]?(?:"((?:[^"\\]|\\.)*)"|'((?:[^'\\]|\\.)*)')"#,
        )
        .expect("python")]
    });
    static GO: LazyLock<Vec<Regex>> = LazyLock::new(|| {
        vec![Regex::new(&format!(
            r#"\b(?:log|logger|l|zap\.S\(\)|logrus|slog)\s*\.\s*(?:Printf|Println|Print|Fatalf|Fatal|Panicf|Infof|Errorf|Warnf|Debugf|Info|Error|Warn|Debug)\s*\(\s*(?:ctx\s*,\s*)?{STR}"#
        ))
        .expect("go")]
    });
    static CSHARP: LazyLock<Vec<Regex>> = LazyLock::new(|| {
        vec![Regex::new(&format!(
            r#"\b(?:_?logger|_?log|Log|Logger)\s*\.\s*(?:Log)?(?:Trace|Debug|Information|Info|Warning|Warn|Error|Critical|Fatal)\s*\(\s*(?:[A-Za-z_][\w.]*\s*,\s*)?\$?@?{STR}"#
        ))
        .expect("csharp")]
    });
    static JS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
        vec![Regex::new(
            r#"\b(?:console|logger|log)\s*\.\s*(?:log|debug|info|warn|error)\s*\(\s*(?:"((?:[^"\\]|\\.)*)"|'((?:[^'\\]|\\.)*)'|`((?:[^`\\]|\\.)*)`)"#,
        )
        .expect("js")]
    });
    match ext {
        "rs" => &RUST,
        "java" | "kt" | "scala" | "groovy" => &JVM,
        "py" => &PYTHON,
        "go" => &GO,
        "cs" => &CSHARP,
        "js" | "mjs" | "cjs" | "ts" | "tsx" | "jsx" => &JS,
        _ => &[],
    }
}

/// Placeholders of the format strings: `{}`, `{0}`, `{name}`, `{:?}`, `%s`, `%-5d`, `%v`,
/// `%+v`, `${x}`, `{{`/`}}` excluded.
static PLACEHOLDER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\$\{[^}]*\}|\{[\w.:?#<>^+\-]*\}|%[-+ #0]*\d*(?:\.\d+)?[a-zA-Z]")
        .expect("placeholder")
});

/// The pattern of format string `format`: its literal parts and the regex matching a row
/// it printed. `None` when it holds too little literal text.
fn compile(format: &str) -> Option<(Vec<String>, Regex)> {
    let format = format
        .replace("\\n", " ")
        .replace("\\t", " ")
        .replace("\\\"", "\"")
        .replace("{{", "{")
        .replace("}}", "}");
    let mut literals = Vec::new();
    let mut last = 0;
    for m in PLACEHOLDER.find_iter(&format) {
        literals.push(format[last..m.start()].to_string());
        last = m.end();
    }
    literals.push(format[last..].to_string());
    let literal_len: usize = literals.iter().map(|l| l.trim().len()).sum();
    if literal_len < MIN_LITERAL {
        return None;
    }
    let body = literals
        .iter()
        .map(|l| regex::escape(l.trim()))
        .collect::<Vec<_>>()
        .join(".*?");
    let regex = Regex::new(&body).ok()?;
    let literals = literals
        .into_iter()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    Some((literals, regex))
}

/// The logging statements of source file `file` whose text is `text`.
pub fn extract(file: &Path, text: &str) -> Vec<Statement> {
    let ext = file
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let mut out = Vec::new();
    for re in patterns(&ext) {
        for c in re.captures_iter(text) {
            let Some(m) = c.iter().skip(1).flatten().next() else {
                continue;
            };
            let Some((literals, pattern)) = compile(m.as_str()) else {
                continue;
            };
            let at = c.get(0).map_or(0, |w| w.start());
            let line = text[..at].bytes().filter(|&b| b == b'\n').count() as u32 + 1;
            out.push(Statement {
                file: file.to_path_buf(),
                line,
                format: m.as_str().to_string(),
                literals,
                pattern,
            });
        }
    }
    out
}

/// The statements of every source file under `roots`.
pub fn index(roots: &[PathBuf]) -> Vec<Statement> {
    let mut out = Vec::new();
    let mut files = 0usize;
    let mut stack: Vec<(PathBuf, usize)> = roots.iter().map(|r| (r.clone(), 0)).collect();
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                let name = entry.file_name();
                let skip = name
                    .to_str()
                    .is_some_and(|n| SKIPPED_DIRS.contains(&n) || n.starts_with('.'));
                if !skip && depth < MAX_DEPTH {
                    stack.push((path, depth + 1));
                }
                continue;
            }
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if !kind.is_file() || patterns(&ext).is_empty() {
                continue;
            }
            if entry.metadata().map_or(true, |m| m.len() > MAX_FILE_BYTES) {
                continue;
            }
            files += 1;
            if files > MAX_FILES {
                return out;
            }
            if let Ok(text) = std::fs::read_to_string(&path) {
                out.extend(extract(&path, &text));
            }
        }
    }
    out
}

/// The statements matching row `row`, the most specific first.
pub fn matches<'a>(statements: &'a [Statement], row: &str) -> Vec<&'a Statement> {
    let mut found: Vec<&Statement> = statements.iter().filter(|s| s.matches(row)).collect();
    found.sort_by_key(|s| std::cmp::Reverse(s.weight()));
    found
}

/// The index of the source folders, built on a background thread.
pub enum Index {
    Building(Receiver<Vec<Statement>>, Vec<PathBuf>),
    Ready(Vec<Statement>, Vec<PathBuf>),
}

impl Index {
    /// Starts indexing `roots`.
    pub fn start(roots: Vec<PathBuf>) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        let walk = roots.clone();
        let _ = std::thread::Builder::new()
            .name("fasttail-log2src".into())
            .spawn(move || {
                let _ = tx.send(index(&walk));
            });
        Index::Building(rx, roots)
    }

    /// The statements once the index is built (polling it).
    pub fn ready(&mut self) -> Option<&[Statement]> {
        if let Index::Building(rx, roots) = self {
            match rx.try_recv() {
                Ok(statements) => *self = Index::Ready(statements, std::mem::take(roots)),
                Err(TryRecvError::Empty) => return None,
                Err(TryRecvError::Disconnected) => {
                    *self = Index::Ready(Vec::new(), std::mem::take(roots))
                }
            }
        }
        match self {
            Index::Ready(s, _) => Some(s),
            Index::Building(..) => None,
        }
    }

    /// Whether it was built over `roots` (Settings may have changed them since).
    pub fn covers(&self, roots: &[PathBuf]) -> bool {
        match self {
            Index::Building(_, r) | Index::Ready(_, r) => r == roots,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statements_of_several_languages_match_the_rows_they_printed() {
        let rust = "fn pay(id: u32) {\n    info!(\"payment {} declined for {}\", id, user);\n    debug!(\"x\");\n}\n";
        let s = extract(Path::new("src/pay.rs"), rust);
        assert_eq!(s.len(), 1, "the short one is too generic: {s:?}");
        assert_eq!(s[0].line, 2);
        let java = "class A {\n  void f() {\n    log.warn(\"Order {} not found in {}\", id, store);\n  }\n}\n";
        let py = "import logging\nlogging.error('cannot open %s: %d retries', path, n)\n";
        let go = "func f() {\n\tlog.Printf(\"listening on %s:%d\", host, port)\n}\n";
        let cs = "_logger.LogInformation(\"User {UserId} logged in from {Ip}\", id, ip);\n";
        let js = "console.error(`upload of ${name} failed after ${n} tries`);\n";
        let mut all = s;
        all.extend(extract(Path::new("A.java"), java));
        all.extend(extract(Path::new("x.py"), py));
        all.extend(extract(Path::new("main.go"), go));
        all.extend(extract(Path::new("Auth.cs"), cs));
        all.extend(extract(Path::new("up.ts"), js));
        assert_eq!(all.len(), 6, "{all:#?}");
        let first = |row: &str| matches(&all, row).first().map(|s| (s.file.clone(), s.line));
        assert_eq!(
            first("2026-10-30 10:00:00 INFO payment 42 declined for bob"),
            Some((PathBuf::from("src/pay.rs"), 2))
        );
        assert_eq!(
            first("WARN Order 7 not found in main"),
            Some((PathBuf::from("A.java"), 3))
        );
        assert_eq!(
            first("ERROR cannot open /tmp/a: 3 retries"),
            Some((PathBuf::from("x.py"), 2))
        );
        assert_eq!(
            first("listening on 0.0.0.0:8080"),
            Some((PathBuf::from("main.go"), 2))
        );
        assert_eq!(
            first("info: User 12 logged in from 10.0.0.1"),
            Some((PathBuf::from("Auth.cs"), 1))
        );
        assert_eq!(
            first("upload of a.png failed after 3 tries"),
            Some((PathBuf::from("up.ts"), 1))
        );
        assert!(first("nothing like it").is_none());
    }

    #[test]
    fn the_index_walks_the_source_folders_and_skips_build_output() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::create_dir_all(dir.path().join("target/debug")).unwrap();
        std::fs::write(
            dir.path().join("src/a.rs"),
            "fn f() { error!(\"disk full on volume {}\", v); }\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("target/debug/b.rs"),
            "fn g() { error!(\"generated statement {}\", v); }\n",
        )
        .unwrap();
        let mut idx = Index::start(vec![dir.path().to_path_buf()]);
        let start = std::time::Instant::now();
        while idx.ready().is_none() && start.elapsed().as_secs() < 10 {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let s = idx.ready().unwrap();
        assert_eq!(s.len(), 1);
        assert!(s[0].file.ends_with("src/a.rs"));
        assert!(idx.covers(&[dir.path().to_path_buf()]));
    }
}
