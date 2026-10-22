// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Pattern streams and folder sources: a directory plus one or more file-name patterns
//! with `*` and `?` wildcards, resolved to the newest matching file or listed in full.
//! The matcher is in-house (no `glob` crate): it works on the file name only and the
//! directory part is taken literally. A pattern path is `dir/pattern`, several patterns
//! are separated by `;` (`logs/*.log;*.txt`), and a `**` folder just before them
//! (`logs/**/app.log`) includes the subfolders of `dir`, walked breadth-first to
//! `MAX_DEPTH` and at most `MAX_ENTRIES` entries per scan, never through a symbolic link
//! or junction to a folder.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// How deep a recursive scan goes below the folder.
pub const MAX_DEPTH: usize = 8;

/// The most directory entries one scan reads.
pub const MAX_ENTRIES: usize = 10_000;

/// The folder component that marks a recursive pattern path.
const RECURSIVE_MARK: &str = "**";

/// True when `name` matches `pattern`, where `*` matches any run of characters (including
/// none) and `?` exactly one. On Windows the comparison ignores ASCII case, like the file
/// system does.
pub fn wildcard_match(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.chars().map(fold).collect();
    let n: Vec<char> = name.chars().map(fold).collect();
    let (mut pi, mut ni) = (0usize, 0usize);
    // Position of the last `*` seen and the name index it was matched against, for
    // backtracking when a later literal fails.
    let mut star: Option<(usize, usize)> = None;
    while ni < n.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == n[ni]) {
            pi += 1;
            ni += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some((pi, ni));
            pi += 1;
        } else if let Some((sp, sn)) = star {
            pi = sp + 1;
            ni = sn + 1;
            star = Some((sp, sn + 1));
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

#[cfg(windows)]
fn fold(c: char) -> char {
    c.to_ascii_lowercase()
}

#[cfg(not(windows))]
fn fold(c: char) -> char {
    c
}

/// A folder source: the folder, the file-name patterns (any of them matches) and whether
/// files in subfolders count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderSpec {
    pub dir: PathBuf,
    pub patterns: Vec<String>,
    pub recursive: bool,
}

/// A matching file found by a scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderMatch {
    pub path: PathBuf,
    pub modified: SystemTime,
}

/// The files a scan found, and whether it stopped at `MAX_ENTRIES`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FolderListing {
    pub files: Vec<FolderMatch>,
    pub truncated: bool,
}

impl FolderSpec {
    /// The spec of `dir` with the `;`-separated `patterns` (blank ones dropped; none left
    /// means `*`).
    pub fn new(dir: PathBuf, patterns: &str, recursive: bool) -> Self {
        let mut list: Vec<String> = patterns
            .split(';')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(str::to_string)
            .collect();
        if list.is_empty() {
            list.push("*".to_string());
        }
        Self {
            dir,
            patterns: list,
            recursive,
        }
    }

    /// The spec a pattern path names (see the module documentation). `None` when the path
    /// names a plain file.
    pub fn parse(path: &Path) -> Option<Self> {
        if !is_pattern_path(path) {
            return None;
        }
        let glob = path.file_name()?.to_str()?;
        let mut dir = match path.parent() {
            Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
            _ => PathBuf::from("."),
        };
        let recursive = dir.file_name().and_then(|n| n.to_str()) == Some(RECURSIVE_MARK);
        if recursive {
            dir = match dir.parent() {
                Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
                _ => PathBuf::from("."),
            };
        }
        Some(Self::new(dir, glob, recursive))
    }

    /// The patterns joined with `;`, as typed.
    pub fn patterns_text(&self) -> String {
        self.patterns.join(";")
    }

    /// The pattern path naming this spec: `dir/pattern` for one pattern without
    /// subfolders, as pattern streams were always written.
    pub fn path(&self) -> PathBuf {
        let dir = if self.recursive {
            self.dir.join(RECURSIVE_MARK)
        } else {
            self.dir.clone()
        };
        dir.join(self.patterns_text())
    }

    /// Whether the file name `name` matches one of the patterns.
    pub fn matches(&self, name: &str) -> bool {
        self.patterns.iter().any(|p| wildcard_match(p, name))
    }

    /// Whether a pattern names the extension of `name` itself (`*.gz` for `app.log.1.gz`),
    /// rather than matching it through a wildcard (`*`).
    pub fn names_extension_of(&self, name: &str) -> bool {
        let Some((_, ext)) = name.rsplit_once('.') else {
            return false;
        };
        let suffix = format!(".{ext}");
        self.patterns.iter().any(|p| {
            p.len() > suffix.len() && wildcard_match(p, name) && ends_with_fold(p, &suffix)
        })
    }

    /// Every regular file that matches, in walk order (folder by folder, breadth-first).
    pub fn list(&self) -> FolderListing {
        self.list_limited(MAX_ENTRIES)
    }

    /// `list` reading at most `max_entries` directory entries.
    pub fn list_limited(&self, max_entries: usize) -> FolderListing {
        let mut out = FolderListing::default();
        let mut queue = VecDeque::from([(self.dir.clone(), 0usize)]);
        let mut seen = 0usize;
        while let Some((dir, depth)) = queue.pop_front() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                if seen >= max_entries {
                    out.truncated = true;
                    return out;
                }
                seen += 1;
                // The entry's own type: a link to a folder is not a folder here.
                let Ok(kind) = entry.file_type() else {
                    continue;
                };
                if kind.is_dir() {
                    if self.recursive && depth < MAX_DEPTH {
                        queue.push_back((entry.path(), depth + 1));
                    }
                    continue;
                }
                let Ok(name) = entry.file_name().into_string() else {
                    continue;
                };
                if !self.matches(&name) {
                    continue;
                }
                // A link to a file counts as the file.
                let Ok(meta) = std::fs::metadata(entry.path()) else {
                    continue;
                };
                if !meta.is_file() {
                    continue;
                }
                out.files.push(FolderMatch {
                    path: entry.path(),
                    modified: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                });
            }
        }
        out
    }

    /// The newest matching file: latest modification time, ties broken by the greater
    /// name (so `app-2026-09-19.log` wins over `app-2026-09-18.log` when both carry the
    /// same timestamp). `None` when nothing matches or the folder cannot be read.
    pub fn newest(&self) -> Option<PathBuf> {
        newest_of(&self.list().files).map(|m| m.path.clone())
    }
}

/// The newest of `files`, by modification time and then by file name.
pub fn newest_of(files: &[FolderMatch]) -> Option<&FolderMatch> {
    let name = |m: &FolderMatch| m.path.file_name().map(|n| n.to_os_string());
    files
        .iter()
        .fold(None, |best: Option<&FolderMatch>, m| match best {
            Some(b)
                if m.modified < b.modified || (m.modified == b.modified && name(m) <= name(b)) =>
            {
                Some(b)
            }
            _ => Some(m),
        })
}

fn ends_with_fold(text: &str, suffix: &str) -> bool {
    let t: Vec<char> = text.chars().map(fold).collect();
    let s: Vec<char> = suffix.chars().map(fold).collect();
    t.ends_with(&s)
}

/// True when `path` names a pattern stream rather than a file: its last component holds a
/// wildcard or a `;` list, or the folder before it is `**`.
pub fn is_pattern_path(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    if name.contains('*') || name.contains('?') {
        return true;
    }
    let recursive = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        == Some(RECURSIVE_MARK);
    // A real file whose name holds `;` stays a file.
    recursive || (name.contains(';') && !path.is_file())
}

/// The newest regular file in `dir` whose name matches `glob` (one pattern, or several
/// separated by `;`), not looking into subfolders. See `FolderSpec::newest`.
pub fn resolve_newest(dir: &Path, glob: &str) -> Option<PathBuf> {
    FolderSpec::new(dir.to_path_buf(), glob, false).newest()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_star_and_question_mark() {
        assert!(wildcard_match("app-*.log", "app-2026-09-18.log"));
        assert!(wildcard_match("app-*.log", "app-.log"));
        assert!(!wildcard_match("app-*.log", "app-2026.txt"));
        assert!(wildcard_match("app-????-??-??.log", "app-2026-09-18.log"));
        assert!(!wildcard_match("app-????.log", "app-20260918.log"));
        assert!(wildcard_match("*", "anything"));
        assert!(wildcard_match("*", ""));
        assert!(!wildcard_match("?", ""));
        assert!(wildcard_match("*.*", "a.b.c"));
        assert!(wildcard_match("a*b*c", "aXXbYYc"));
        assert!(!wildcard_match("a*b*c", "aXXbYY"));
        assert!(wildcard_match("**x", "x"));
    }

    #[test]
    fn parses_pattern_paths_only() {
        assert!(is_pattern_path(Path::new("logs/app-*.log")));
        assert!(is_pattern_path(Path::new("app-?.log")));
        assert!(is_pattern_path(Path::new("logs/**/app.log")));
        assert!(is_pattern_path(Path::new("logs/a.log;b.log")));
        assert!(!is_pattern_path(Path::new("logs/app.log")));
        let spec = FolderSpec::parse(Path::new("logs/app-*.log")).unwrap();
        assert_eq!(spec.dir, PathBuf::from("logs"));
        assert_eq!(spec.patterns, vec!["app-*.log"]);
        assert!(!spec.recursive);
        assert_eq!(spec.path(), PathBuf::from("logs/app-*.log"));
        let spec = FolderSpec::parse(Path::new("*.log")).unwrap();
        assert_eq!(spec.dir, PathBuf::from("."));
        let spec = FolderSpec::parse(Path::new("logs/**/*.log; *.txt")).unwrap();
        assert_eq!(spec.dir, PathBuf::from("logs"));
        assert_eq!(spec.patterns, vec!["*.log", "*.txt"]);
        assert!(spec.recursive);
        assert_eq!(
            spec.path(),
            Path::new("logs").join("**").join("*.log;*.txt")
        );
        assert_eq!(FolderSpec::parse(&spec.path()), Some(spec));
        assert!(FolderSpec::parse(Path::new("logs/app.log")).is_none());
    }

    #[test]
    fn matches_any_pattern_and_names_extensions() {
        let spec = FolderSpec::new(PathBuf::from("."), "*.log;*.TXT;;", false);
        assert_eq!(spec.patterns.len(), 2);
        assert!(spec.matches("a.log"));
        assert_eq!(spec.matches("a.txt"), cfg!(windows));
        assert!(!spec.matches("a.gz"));
        let all = FolderSpec::new(PathBuf::from("."), "*", false);
        assert!(all.matches("app.log.1.gz"));
        assert!(!all.names_extension_of("app.log.1.gz"));
        let gz = FolderSpec::new(PathBuf::from("."), "*;*.gz", false);
        assert!(gz.names_extension_of("app.log.1.gz"));
        assert!(!gz.names_extension_of("app.log"));
        assert_eq!(
            FolderSpec::new(PathBuf::from("."), " ", false).patterns,
            vec!["*"]
        );
    }
}
