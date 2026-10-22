// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Folder groups ("Open all"): every file of a folder source opens as its own stream, and
//! a matching file created later is reported so the app opens it as well. UI-free: the
//! window and the terminal both drive it. The group remembers every file it has offered
//! (opened or skipped), so a file the user closed is not reopened while the group lives.

use crate::tail_engine::WakeFn;
use crate::wildcard::{FolderMatch, FolderSpec};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Default and bounds of `folder_max_streams`.
pub const DEFAULT_MAX_STREAMS: usize = 32;
pub const MIN_MAX_STREAMS: usize = 1;
pub const MAX_MAX_STREAMS: usize = 256;

/// Rescan cadence of a flat folder, and of a folder with its subfolders.
pub const FLAT_SCAN_INTERVAL: Duration = Duration::from_secs(2);
pub const RECURSIVE_SCAN_INTERVAL: Duration = Duration::from_secs(10);
/// The wait between a watcher event and the scan it triggers.
pub const EVENT_DEBOUNCE: Duration = Duration::from_millis(250);

/// Extensions of compressed files and archives, skipped unless a pattern names them.
const COMPRESSED_EXTENSIONS: &[&str] = &[
    "gz", "gzip", "bz2", "xz", "zst", "zstd", "lz4", "zip", "7z", "tar", "tgz", "txz", "tbz",
    "tbz2", "tzst",
];

/// What a scan of the group produced.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GroupScan {
    /// Files to open, sorted by name.
    pub open: Vec<PathBuf>,
    /// Matching files left out by the cap.
    pub skipped: usize,
    /// The scan stopped at the entry limit.
    pub truncated: bool,
}

/// An open-all folder group.
pub struct FolderGroup {
    pub spec: FolderSpec,
    pub max_streams: usize,
    /// Rescan cadence (`FLAT_SCAN_INTERVAL` or `RECURSIVE_SCAN_INTERVAL`; tests use 0).
    pub scan_interval: Duration,
    offered: HashSet<PathBuf>,
    opened: usize,
    last_scan: Instant,
    dirty: Arc<AtomicBool>,
    _watcher: Option<RecommendedWatcher>,
}

/// True for a compressed file or archive by its name.
pub fn is_compressed_name(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, ext)| {
        COMPRESSED_EXTENSIONS
            .iter()
            .any(|c| ext.eq_ignore_ascii_case(c))
    })
}

impl FolderGroup {
    /// Starts a group on `spec` and lists what to open first: every eligible match, the
    /// `max_streams` most recently modified when there are more, sorted by name. A watcher
    /// on the folder (with its subfolders when the spec includes them) calls `wake` and
    /// brings the next scan forward; without one the timed scan alone runs.
    pub fn start(spec: FolderSpec, max_streams: usize, wake: Option<WakeFn>) -> (Self, GroupScan) {
        let dirty = Arc::new(AtomicBool::new(false));
        let watcher = {
            let dirty = Arc::clone(&dirty);
            RecommendedWatcher::new(
                move |res: notify::Result<notify::Event>| {
                    // A file appearing (created, or renamed into place), not the
                    // writes to the open ones: those would rescan the folder at every append.
                    let appeared = res.is_ok_and(|e| {
                        e.kind.is_create()
                            || matches!(
                                e.kind,
                                notify::EventKind::Modify(notify::event::ModifyKind::Name(_))
                            )
                    });
                    if appeared {
                        dirty.store(true, Ordering::Relaxed);
                        if let Some(wake) = &wake {
                            wake();
                        }
                    }
                },
                notify::Config::default(),
            )
            .ok()
            .and_then(|mut w| {
                let mode = if spec.recursive {
                    RecursiveMode::Recursive
                } else {
                    RecursiveMode::NonRecursive
                };
                w.watch(&spec.dir, mode).ok().map(|()| w)
            })
        };
        let scan_interval = if spec.recursive {
            RECURSIVE_SCAN_INTERVAL
        } else {
            FLAT_SCAN_INTERVAL
        };
        let mut group = Self {
            spec,
            max_streams: max_streams.clamp(MIN_MAX_STREAMS, MAX_MAX_STREAMS),
            scan_interval,
            offered: HashSet::new(),
            opened: 0,
            last_scan: Instant::now(),
            dirty,
            _watcher: watcher,
        };
        let first = group.scan();
        (group, first)
    }

    /// The pattern path naming the group (see `FolderSpec::path`).
    pub fn path(&self) -> PathBuf {
        self.spec.path()
    }

    /// How many files the group opened so far.
    pub fn opened(&self) -> usize {
        self.opened
    }

    /// Whether `path` is a file this group offered.
    pub fn offered(&self, path: &std::path::Path) -> bool {
        self.offered.contains(path)
    }

    /// The files created since the last scan, when a scan is due: after a watcher event
    /// (debounced) or every `scan_interval`. Empty otherwise.
    pub fn poll(&mut self) -> GroupScan {
        let since = self.last_scan.elapsed();
        let due = (self.dirty.load(Ordering::Relaxed)
            && since >= EVENT_DEBOUNCE.min(self.scan_interval))
            || since >= self.scan_interval;
        if !due {
            return GroupScan::default();
        }
        self.scan()
    }

    /// Lists the folder and offers the files not seen yet, up to the cap.
    pub fn scan(&mut self) -> GroupScan {
        self.last_scan = Instant::now();
        self.dirty.store(false, Ordering::Relaxed);
        let listing = self.spec.list();
        let fresh: Vec<FolderMatch> = listing
            .files
            .into_iter()
            .filter(|m| !self.offered.contains(&m.path))
            .collect();
        let room = self.max_streams.saturating_sub(self.opened);
        let (open, skipped) = pick(&self.spec, fresh, room);
        // Skipped files are offered too: they never open later.
        self.offered.extend(open.iter().cloned());
        self.offered.extend(skipped.iter().cloned());
        self.opened += open.len();
        GroupScan {
            open,
            skipped: skipped.len(),
            truncated: listing.truncated,
        }
    }
}

/// Of `files`, the eligible ones (compressed files only when a pattern names them): the
/// `room` most recently modified, sorted by name, and the ones left out.
fn pick(spec: &FolderSpec, files: Vec<FolderMatch>, room: usize) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut fresh: Vec<FolderMatch> = files.into_iter().filter(|m| eligible(spec, m)).collect();
    let mut skipped = Vec::new();
    if fresh.len() > room {
        // The newest first; the rest are left out.
        fresh.sort_by(|a, b| {
            b.modified
                .cmp(&a.modified)
                .then_with(|| b.path.cmp(&a.path))
        });
        skipped = fresh.drain(room..).map(|m| m.path).collect();
    }
    fresh.sort_by(|a, b| a.path.cmp(&b.path));
    (fresh.into_iter().map(|m| m.path).collect(), skipped)
}

fn eligible(spec: &FolderSpec, m: &FolderMatch) -> bool {
    let name = m
        .path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    !is_compressed_name(name) || spec.names_extension_of(name)
}

/// `--all` without a window: each pattern path of `paths` replaced by the files an Open
/// all group would open (at most `max_streams` per pattern), the other paths kept.
pub fn expand_all(paths: &[PathBuf], max_streams: usize) -> Vec<PathBuf> {
    let max = max_streams.clamp(MIN_MAX_STREAMS, MAX_MAX_STREAMS);
    let mut out = Vec::new();
    for path in paths {
        match FolderSpec::parse(path) {
            Some(spec) => {
                let files = spec.list().files;
                out.extend(pick(&spec, files, max).0);
            }
            None => out.push(path.clone()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compressed_names() {
        assert!(is_compressed_name("app.log.1.gz"));
        assert!(is_compressed_name("bundle.ZIP"));
        assert!(!is_compressed_name("app.log"));
        assert!(!is_compressed_name("gz"));
    }
}
