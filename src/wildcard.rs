//! Pattern streams: a directory plus a file-name pattern with `*` and `?` wildcards,
//! resolved to the newest matching file. The matcher is in-house (no `glob` crate): it
//! works on the file name only, the directory part is taken literally and nothing is
//! recursive, so `logs/app-*.log` can never surprise the user by walking subfolders.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

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

/// True when the last component of `path` contains a wildcard, i.e. the path names a
/// pattern stream rather than a file.
pub fn is_pattern_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.contains('*') || n.contains('?'))
        .unwrap_or(false)
}

/// Splits a pattern path into its directory and file-name pattern. `None` when the last
/// component holds no wildcard or the path has no directory part.
pub fn split_pattern(path: &Path) -> Option<(PathBuf, String)> {
    if !is_pattern_path(path) {
        return None;
    }
    let glob = path.file_name()?.to_str()?.to_string();
    let dir = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    };
    Some((dir, glob))
}

/// The newest regular file in `dir` whose name matches `glob`: latest modification time,
/// ties broken by the greater name (so `app-2026-09-19.log` wins over `app-2026-09-18.log`
/// when both carry the same timestamp). `None` when nothing matches or the directory
/// cannot be read.
pub fn resolve_newest(dir: &Path, glob: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut best: Option<(SystemTime, String, PathBuf)> = None;
    for entry in entries.flatten() {
        let name = match entry.file_name().into_string() {
            Ok(n) => n,
            Err(_) => continue,
        };
        if !wildcard_match(glob, &name) {
            continue;
        }
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let better = match &best {
            None => true,
            Some((t, n, _)) => modified > *t || (modified == *t && name > *n),
        };
        if better {
            best = Some((modified, name, entry.path()));
        }
    }
    best.map(|(_, _, p)| p)
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
    fn splits_pattern_paths_only() {
        assert!(is_pattern_path(Path::new("logs/app-*.log")));
        assert!(is_pattern_path(Path::new("app-?.log")));
        assert!(!is_pattern_path(Path::new("logs/app.log")));
        let (dir, glob) = split_pattern(Path::new("logs/app-*.log")).unwrap();
        assert_eq!(dir, PathBuf::from("logs"));
        assert_eq!(glob, "app-*.log");
        let (dir, glob) = split_pattern(Path::new("*.log")).unwrap();
        assert_eq!(dir, PathBuf::from("."));
        assert_eq!(glob, "*.log");
        assert!(split_pattern(Path::new("logs/app.log")).is_none());
    }
}
