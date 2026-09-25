//! Temporary spool files: data that has no random-access file of its own (a decompressed
//! archive entry, later standard input) is written to a regular file that the unchanged
//! tail engine then opens and indexes like any other log.
//!
//! Spool files live in `spool_dir` from `fasttail.ini` when set, else in
//! `<temp>/fasttail-spool`, and are named `<pid>-<counter>-<name>`: the pid tells which
//! process owns a file. A spool is deleted when its `SpoolFile` handle is dropped (the
//! stream is closed or reloaded), every spool of the process is deleted at normal exit
//! (`remove_own`), and `sweep` deletes at startup the spools whose process is no longer
//! running, which covers crashes and kills.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Name of the spool directory under the system temporary directory.
pub const SPOOL_DIR_NAME: &str = "fasttail-spool";
/// Longest kept part of the name a spool is created for (the rest is dropped).
const MAX_NAME_CHARS: usize = 80;

/// Counter making the spool names of one process unique.
static SPOOL_COUNTER: AtomicU64 = AtomicU64::new(1);

/// The spool directory: `configured` when set (and not empty), else
/// `<temp>/fasttail-spool`.
pub fn spool_dir(configured: Option<&Path>) -> PathBuf {
    match configured {
        Some(dir) if !dir.as_os_str().is_empty() => dir.to_path_buf(),
        _ => std::env::temp_dir().join(SPOOL_DIR_NAME),
    }
}

/// Creates the spool directory if needed; on Unix it is readable by its owner only
/// (`%TEMP%` is per-user on Windows already).
pub fn ensure_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// `name` reduced to a safe file name: its last path component, with the characters
/// that are not portable in a file name replaced by `_`, capped to 80 characters.
pub fn sanitize(name: &str) -> String {
    let base = name
        .rsplit(['/', '\\'])
        .find(|part| !part.is_empty())
        .unwrap_or("");
    let cleaned: String = base
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*') {
                '_'
            } else {
                c
            }
        })
        .take(MAX_NAME_CHARS)
        .collect();
    let cleaned = cleaned.trim_matches([' ', '.']).to_string();
    if cleaned.is_empty() {
        "stream".to_string()
    } else {
        cleaned
    }
}

/// File name of the spool `counter` of process `pid` for `name`.
pub fn spool_name(pid: u32, counter: u64, name: &str) -> String {
    format!("{pid}-{counter}-{}", sanitize(name))
}

/// Owner pid encoded in a spool file name, if the name has the spool shape.
pub fn owner_pid(file_name: &str) -> Option<u32> {
    let (pid, rest) = file_name.split_once('-')?;
    let (counter, _) = rest.split_once('-')?;
    if counter.is_empty() || !counter.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    pid.parse().ok()
}

/// A spool file owned by this process, deleted from disk when dropped.
#[derive(Debug)]
pub struct SpoolFile {
    path: PathBuf,
}

impl SpoolFile {
    /// Creates a new empty spool for `name` in `dir` and returns it with a write handle.
    /// The handle shares read, write and delete access (like `open_file_shared`), so the
    /// engine can read the file while it is written and it can be deleted on Windows.
    pub fn create(dir: &Path, name: &str) -> std::io::Result<(SpoolFile, File)> {
        ensure_dir(dir)?;
        let counter = SPOOL_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = dir.join(spool_name(std::process::id(), counter, name));
        let file = open_for_writing(&path, true)?;
        Ok((SpoolFile { path }, file))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Empties the spool and returns a fresh write handle (a reload writes it again).
    pub fn rewrite(&self) -> std::io::Result<File> {
        let file = open_for_writing(&self.path, false)?;
        file.set_len(0)?;
        Ok(file)
    }
}

impl Drop for SpoolFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn open_for_writing(path: &Path, create_new: bool) -> std::io::Result<File> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true);
    if create_new {
        options.create_new(true);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(7); // FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE
    }
    options.open(path)
}

/// Spool files in `dir` with their owner pid.
fn spools_in(dir: &Path) -> Vec<(PathBuf, u32)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .filter_map(|e| {
            let pid = owner_pid(&e.file_name().to_string_lossy())?;
            Some((e.path(), pid))
        })
        .collect()
}

/// Deletes the spool files in `dir` whose owning process is not running. A pid reused
/// by an unrelated process only delays the cleanup to a later start. Returns the number
/// of files removed.
pub fn sweep(dir: &Path) -> usize {
    let spools = spools_in(dir);
    if spools.is_empty() {
        return 0;
    }
    let mut pids: Vec<sysinfo::Pid> = spools
        .iter()
        .map(|(_, pid)| sysinfo::Pid::from_u32(*pid))
        .collect();
    pids.sort_unstable();
    pids.dedup();
    let mut system = sysinfo::System::new();
    system.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::Some(&pids),
        true,
        sysinfo::ProcessRefreshKind::nothing(),
    );
    let own = std::process::id();
    spools
        .into_iter()
        .filter(|(_, pid)| *pid != own && system.process(sysinfo::Pid::from_u32(*pid)).is_none())
        .filter(|(path, _)| std::fs::remove_file(path).is_ok())
        .count()
}

/// Deletes every spool file of this process in `dir` (normal exit). Returns the number
/// of files removed.
pub fn remove_own(dir: &Path) -> usize {
    let own = std::process::id();
    spools_in(dir)
        .into_iter()
        .filter(|(_, pid)| *pid == own)
        .filter(|(path, _)| std::fs::remove_file(path).is_ok())
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn names_carry_the_pid_and_a_safe_base_name() {
        assert_eq!(spool_name(42, 7, "server.log"), "42-7-server.log");
        assert_eq!(spool_name(1, 2, "logs/app/server.log"), "1-2-server.log");
        assert_eq!(spool_name(1, 2, r"C:\x\a:b*c?.log"), "1-2-a_b_c_.log");
        assert_eq!(spool_name(1, 2, ""), "1-2-stream");
        assert_eq!(spool_name(1, 2, "..."), "1-2-stream");
        let long = "x".repeat(300);
        assert_eq!(sanitize(&long).chars().count(), MAX_NAME_CHARS);
        assert_eq!(owner_pid("42-7-server.log"), Some(42));
        assert_eq!(owner_pid("42-7-a-b.log"), Some(42));
        assert_eq!(owner_pid("server.log"), None);
        assert_eq!(owner_pid("x-7-server.log"), None);
        assert_eq!(owner_pid("42-x-server.log"), None);
    }

    #[test]
    fn default_dir_is_under_temp_and_a_setting_overrides_it() {
        assert_eq!(spool_dir(None), std::env::temp_dir().join(SPOOL_DIR_NAME));
        assert_eq!(
            spool_dir(Some(Path::new(""))),
            std::env::temp_dir().join(SPOOL_DIR_NAME)
        );
        assert_eq!(
            spool_dir(Some(Path::new("/big/disk"))),
            PathBuf::from("/big/disk")
        );
    }

    #[test]
    fn a_spool_is_deleted_when_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let (spool, mut file) = SpoolFile::create(dir.path(), "app.log").unwrap();
        file.write_all(b"hello\n").unwrap();
        let path = spool.path().to_path_buf();
        assert!(path.is_file());
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        assert_eq!(owner_pid(&name), Some(std::process::id()));
        assert!(name.ends_with("-app.log"));
        // A reader holding the file open does not prevent the deletion.
        let reader = crate::file_source::open_file_shared(&path).unwrap();
        drop(file);
        drop(spool);
        assert!(!path.exists());
        drop(reader);
    }

    #[test]
    fn rewrite_empties_the_spool() {
        let dir = tempfile::tempdir().unwrap();
        let (spool, mut file) = SpoolFile::create(dir.path(), "a.log").unwrap();
        file.write_all(b"0123456789").unwrap();
        drop(file);
        let mut again = spool.rewrite().unwrap();
        assert_eq!(std::fs::metadata(spool.path()).unwrap().len(), 0);
        again.write_all(b"ab").unwrap();
        assert_eq!(std::fs::read(spool.path()).unwrap(), b"ab");
    }

    #[test]
    fn sweep_removes_dead_owners_and_keeps_live_ones() {
        let dir = tempfile::tempdir().unwrap();
        // A pid far above any real one (pids are well below 2^31 on every platform).
        let dead = dir.path().join(spool_name(u32::MAX - 1, 1, "dead.log"));
        std::fs::write(&dead, b"x").unwrap();
        let (mine, _file) = SpoolFile::create(dir.path(), "mine.log").unwrap();
        let unrelated = dir.path().join("notes.txt");
        std::fs::write(&unrelated, b"x").unwrap();

        assert_eq!(sweep(dir.path()), 1);
        assert!(!dead.exists());
        assert!(mine.path().exists());
        assert!(unrelated.exists());

        // At exit every spool of the process goes, other files stay.
        assert_eq!(remove_own(dir.path()), 1);
        assert!(!mine.path().exists());
        assert!(unrelated.exists());
    }

    #[test]
    fn sweep_of_a_missing_dir_is_a_no_op() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(sweep(&dir.path().join("absent")), 0);
    }
}
