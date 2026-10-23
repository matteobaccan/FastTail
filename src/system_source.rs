// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! System sources: the systemd journal (`journald://`, `journald://<unit>`,
//! `journald://user/<unit>`) and Docker containers (`docker://<container>`) as streams.
//!
//! A source runs the platform's own command (`journalctl --follow …`, `docker logs
//! --follow …`) with its standard output and error on one pipe, like `2>&1`, and copies it
//! into a spool with the standard-input copier (`stdin_source`); the engine tails the
//! spool. The URI is the stream's path, so the workspace, the recent files and sessions
//! keep it as they keep a file path. Only the programs named in `fasttail.ini`
//! (`docker_command`, `journalctl_command`) are ever run, with a validated unit or
//! container name and arguments passed one by one, never through a shell.
//!
//! When the command exits the stream keeps its lines and says so; Reconnect runs it
//! again with `--since` the last timestamp received and appends to the same spool,
//! dropping the leading lines it had already copied.

use crate::stdin_source::{InputState, Settings, StdinStream};
use crate::tail_engine::{TailEngine, WakeFn};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

pub const DEFAULT_BACKLOG_LINES: u32 = 10_000;
pub const MAX_BACKLOG_LINES: u32 = 1_000_000;
pub const DEFAULT_DOCKER_COMMAND: &str = "docker";
pub const DEFAULT_JOURNALCTL_COMMAND: &str = "journalctl";

const JOURNALD: &str = "journald://";
const DOCKER: &str = "docker://";

/// A system source, as its URI names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SystemUri {
    /// The system journal, or the user's with `user`; one unit or all of them.
    Journald { user: bool, unit: Option<String> },
    /// A container, by name or id.
    Docker { container: String },
}

/// Why a URI is refused: an i18n key.
pub type UriError = &'static str;

/// The programs and the backlog a source runs with (`fasttail.ini`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commands {
    pub docker: String,
    pub journalctl: String,
    pub backlog: u32,
}

impl Default for Commands {
    fn default() -> Self {
        Self {
            docker: DEFAULT_DOCKER_COMMAND.to_string(),
            journalctl: DEFAULT_JOURNALCTL_COMMAND.to_string(),
            backlog: DEFAULT_BACKLOG_LINES,
        }
    }
}

/// True when `path` is written as a system source URI (valid or not).
pub fn is_system_path(path: &Path) -> bool {
    path.to_str()
        .is_some_and(|s| s.starts_with(JOURNALD) || s.starts_with(DOCKER))
}

/// A unit name journalctl accepts and cannot take for an option.
fn valid_unit(unit: &str) -> bool {
    (1..=256).contains(&unit.len())
        && !unit.starts_with('-')
        && unit
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"@_.:\\-".contains(&b))
}

/// A container name (`[A-Za-z0-9][A-Za-z0-9_.-]{0,254}`) or id (12 to 64 hex digits).
fn valid_container(name: &str) -> bool {
    let b = name.as_bytes();
    let is_name = (1..=255).contains(&b.len())
        && b[0].is_ascii_alphanumeric()
        && b[1..]
            .iter()
            .all(|&c| c.is_ascii_alphanumeric() || b"_.-".contains(&c));
    let is_id = (12..=64).contains(&b.len()) && b.iter().all(u8::is_ascii_hexdigit);
    is_name || is_id
}

impl SystemUri {
    /// The source `path` names: `None` when it is no system URI, `Some(Err)` with the
    /// reason when it is one that cannot be used.
    pub fn parse(path: &Path) -> Option<Result<Self, UriError>> {
        let text = path.to_str()?;
        if let Some(rest) = text.strip_prefix(JOURNALD) {
            if !cfg!(target_os = "linux") {
                return Some(Err("system_journald_linux_only"));
            }
            let (user, unit) = match rest.strip_prefix("user") {
                Some("") => (true, ""),
                Some(after) if after.starts_with('/') => (true, &after[1..]),
                _ => (false, rest),
            };
            if unit.is_empty() {
                return Some(Ok(Self::Journald { user, unit: None }));
            }
            return Some(if valid_unit(unit) {
                Ok(Self::Journald {
                    user,
                    unit: Some(unit.to_string()),
                })
            } else {
                Err("system_bad_unit")
            });
        }
        let rest = text.strip_prefix(DOCKER)?;
        Some(if valid_container(rest) {
            Ok(Self::Docker {
                container: rest.to_string(),
            })
        } else {
            Err("system_bad_container")
        })
    }

    pub fn to_uri(&self) -> String {
        match self {
            Self::Journald { user, unit } => format!(
                "{JOURNALD}{}{}",
                if *user { "user/" } else { "" },
                unit.as_deref().unwrap_or("")
            ),
            Self::Docker { container } => format!("{DOCKER}{container}"),
        }
    }

    /// The tab title: `docker: api`, `journal: nginx.service`, `journal: system`.
    pub fn title(&self) -> String {
        match self {
            Self::Journald { user, unit } => match (user, unit) {
                (false, None) => "journal: system".to_string(),
                (true, None) => "journal: user".to_string(),
                (false, Some(u)) => format!("journal: {u}"),
                (true, Some(u)) => format!("journal: user/{u}"),
            },
            Self::Docker { container } => format!("docker: {container}"),
        }
    }

    /// The program and arguments that follow the source: the backlog of `commands`, or
    /// everything after `since` (milliseconds, UTC) for a reconnect.
    pub fn command_line(&self, commands: &Commands, since: Option<i64>) -> (String, Vec<String>) {
        let backlog = commands.backlog.min(MAX_BACKLOG_LINES).to_string();
        match self {
            Self::Journald { user, unit } => {
                let mut args = vec![
                    "--follow".to_string(),
                    "--no-pager".to_string(),
                    "--output=short-iso-precise".to_string(),
                ];
                match since {
                    Some(ms) => args.push(format!("--since={}", journal_time(ms))),
                    None => args.push(format!("--lines={backlog}")),
                }
                if *user {
                    args.push("--user".to_string());
                }
                if let Some(unit) = unit {
                    args.push(format!("--unit={unit}"));
                }
                (commands.journalctl.clone(), args)
            }
            Self::Docker { container } => {
                let mut args = vec![
                    "logs".to_string(),
                    "--follow".to_string(),
                    "--timestamps".to_string(),
                ];
                match since {
                    Some(ms) => args.extend(["--since".to_string(), rfc3339(ms)]),
                    None => args.extend(["--tail".to_string(), backlog]),
                }
                args.extend(["--".to_string(), container.clone()]);
                (commands.docker.clone(), args)
            }
        }
    }
}

/// `YYYY-MM-DDTHH:MM:SS.mmmZ` of a UTC instant.
fn rfc3339(ms: i64) -> String {
    let (date, clock) = split_utc(ms);
    format!("{date}T{clock}Z")
}

/// `YYYY-MM-DD HH:MM:SS.mmm UTC`, which journalctl's `--since` reads.
fn journal_time(ms: i64) -> String {
    let (date, clock) = split_utc(ms);
    format!("{date} {clock} UTC")
}

fn split_utc(ms: i64) -> (String, String) {
    let days = ms.div_euclid(86_400_000);
    let in_day = ms.rem_euclid(86_400_000);
    let (y, m, d) = crate::timestamp::days_to_date(days);
    (
        format!("{y:04}-{m:02}-{d:02}"),
        format!(
            "{:02}:{:02}:{:02}.{:03}",
            in_day / 3_600_000,
            in_day / 60_000 % 60,
            in_day / 1000 % 60,
            in_day % 1000
        ),
    )
}

/// The UTC instant of the timestamp a source line starts with.
fn line_instant(line: &[u8]) -> Option<i64> {
    let head = String::from_utf8_lossy(&line[..line.len().min(64)]);
    let stamp =
        crate::timestamp::detect_timestamp_zoned(&head, crate::timestamp::FormatHint::Iso8601)?;
    Some(stamp.millis - i64::from(stamp.zone_minutes.unwrap_or(0)) * 60_000)
}

/// Starts `program args` with standard output and error on one pipe; returns the child
/// and the reading end.
fn spawn(program: &str, args: &[String]) -> std::io::Result<(Child, std::io::PipeReader)> {
    let (reader, writer) = std::io::pipe()?;
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(writer.try_clone()?)
        .stderr(writer);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let child = cmd.spawn()?;
    // The command holds the writing ends: dropped here, so the pipe ends with the child.
    drop(cmd);
    Ok((child, reader))
}

/// Drops the leading lines a reconnect gets again: those before the last instant
/// received, and those at that instant already copied. Everything from the first new line
/// on passes untouched.
struct SkipSeen<R> {
    inner: R,
    instant: i64,
    seen: Vec<Vec<u8>>,
    passing: bool,
    pending: Vec<u8>,
    out: Vec<u8>,
    out_pos: usize,
}

impl<R: Read> SkipSeen<R> {
    fn new(inner: R, instant: i64, seen: Vec<Vec<u8>>) -> Self {
        Self {
            inner,
            instant,
            seen,
            passing: false,
            pending: Vec::new(),
            out: Vec::new(),
            out_pos: 0,
        }
    }

    /// True when `line` (newline excluded) was copied before the reconnect.
    fn already_seen(&self, line: &[u8]) -> bool {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        match line_instant(line) {
            Some(t) if t < self.instant => true,
            Some(t) if t == self.instant => self.seen.iter().any(|s| s == line),
            Some(_) => false,
            // A continuation line goes with the line before it, which was dropped.
            None => true,
        }
    }

    /// Sorts the complete lines of `pending`; at the end of the input the rest too.
    fn sort_pending(&mut self, at_end: bool) {
        while !self.passing {
            let end = match self.pending.iter().position(|&b| b == b'\n') {
                Some(nl) => nl,
                None if at_end && !self.pending.is_empty() => self.pending.len(),
                None => return,
            };
            if self.already_seen(&self.pending[..end]) {
                self.pending.drain(..(end + 1).min(self.pending.len()));
            } else {
                self.passing = true;
                self.out = std::mem::take(&mut self.pending);
                self.out_pos = 0;
            }
        }
    }
}

impl<R: Read> Read for SkipSeen<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        loop {
            if self.out_pos < self.out.len() {
                let n = buf.len().min(self.out.len() - self.out_pos);
                buf[..n].copy_from_slice(&self.out[self.out_pos..self.out_pos + n]);
                self.out_pos += n;
                return Ok(n);
            }
            if self.passing {
                return self.inner.read(buf);
            }
            let mut chunk = [0u8; 8192];
            let n = self.inner.read(&mut chunk)?;
            self.pending.extend_from_slice(&chunk[..n]);
            self.sort_pending(n == 0);
            if n == 0 && !self.passing {
                return Ok(0);
            }
        }
    }
}

/// A running (or ended) system source, owned by its engine (`TailEngine::system`).
pub struct SystemStream {
    pub uri: SystemUri,
    child: Option<Child>,
    /// Exit code once the command ended (`None` inside: killed by a signal).
    exit: Option<Option<i32>>,
    settings: Settings,
    wake: Option<WakeFn>,
    /// The stream bar's Reconnect was clicked (applied by the front end, which holds
    /// the commands).
    pub reconnect_requested: bool,
    /// The output copier; declared last so the child is killed before the spool goes.
    pub copier: StdinStream,
}

impl SystemStream {
    /// Runs the command of `uri` and starts copying its output.
    pub fn start(
        uri: SystemUri,
        commands: &Commands,
        settings: &Settings,
        wake: Option<WakeFn>,
    ) -> std::io::Result<Self> {
        let (program, args) = uri.command_line(commands, None);
        let (child, reader) = spawn(&program, &args)?;
        let label = match &uri {
            SystemUri::Journald { .. } => "journald",
            SystemUri::Docker { .. } => "docker",
        };
        let copier = match StdinStream::start_named(reader, settings, wake.clone(), label) {
            Ok(c) => c,
            Err(e) => {
                kill(child);
                return Err(e);
            }
        };
        Ok(Self {
            uri,
            child: Some(child),
            exit: None,
            settings: settings.clone(),
            wake,
            reconnect_requested: false,
            copier,
        })
    }

    /// `Some(code)` once the command ended and its output is copied.
    pub fn ended(&self) -> Option<Option<i32>> {
        self.exit
    }

    pub fn is_running(&self) -> bool {
        self.exit.is_none()
    }
}

impl Drop for SystemStream {
    fn drop(&mut self) {
        if let Some(child) = self.child.take() {
            kill(child);
        }
    }
}

/// Kills `child` and reaps it on a detached thread.
fn kill(mut child: Child) {
    let _ = child.kill();
    let _ = std::thread::Builder::new()
        .name("fasttail-reap".to_string())
        .spawn(move || {
            let _ = child.wait();
        });
}

/// Opens the stream of `uri`: its command runs, `path` is the URI, follow is on.
pub fn open_engine(
    uri: SystemUri,
    commands: &Commands,
    settings: &Settings,
    wake: Option<WakeFn>,
) -> std::io::Result<TailEngine> {
    let stream = SystemStream::start(uri, commands, settings, wake.clone())?;
    let mut engine = match wake {
        Some(wake) => TailEngine::open_with_wake(stream.copier.spool_path(), wake)?,
        None => TailEngine::open(stream.copier.spool_path())?,
    };
    engine.path = PathBuf::from(stream.uri.to_uri());
    engine.follow_tail = true;
    engine.system = Some(stream);
    Ok(engine)
}

/// Lines from the end looked at for the last instant and the lines that share it.
const RECONNECT_LOOKBACK: usize = 2000;

impl TailEngine {
    /// The URI of a system source stream.
    pub fn system_uri(&self) -> Option<&SystemUri> {
        self.system.as_ref().map(|s| &s.uri)
    }

    /// Called from `poll_updates`: once the copier saw the end of the pipe, collects the
    /// command's exit code.
    pub(crate) fn poll_system_exit(&mut self) {
        let Some(s) = self.system.as_mut() else {
            return;
        };
        if s.exit.is_some() || s.copier.state() == InputState::Reading {
            return;
        }
        let Some(child) = s.child.as_mut() else {
            return;
        };
        match child.try_wait() {
            Ok(Some(status)) => {
                s.exit = Some(status.code());
                s.child = None;
            }
            Ok(None) => {
                // The pipe closed but the command still runs: it ends soon (or was
                // killed); asked again on the next poll.
            }
            Err(_) => {
                s.exit = Some(None);
                s.child = None;
            }
        }
    }

    /// Once the source ended: its exit code (`None` when killed by a signal) and the last
    /// line it wrote, cut to 160 characters (often the command's error).
    pub fn system_end(&self) -> Option<(Option<i32>, String)> {
        let code = self.system.as_ref()?.ended()?;
        let last = self
            .total_lines()
            .checked_sub(1)
            .and_then(|i| self.get_line(i))
            .map(|l| l.trim_end().chars().take(160).collect())
            .unwrap_or_default();
        Some((code, last))
    }

    /// Applies a Reconnect asked for from the stream bar; the error, if any, as text.
    pub fn apply_reconnect_request(&mut self, commands: &Commands) -> Option<String> {
        let s = self.system.as_mut()?;
        if !std::mem::take(&mut s.reconnect_requested) {
            return None;
        }
        self.reconnect_system(commands).err().map(|e| e.to_string())
    }

    /// The last instant the stream holds and the lines stamped with it, newest first.
    fn last_instant(&self) -> Option<(i64, Vec<Vec<u8>>)> {
        let total = self.total_lines();
        let mut found: Option<i64> = None;
        let mut seen = Vec::new();
        for idx in (total.saturating_sub(RECONNECT_LOOKBACK)..total).rev() {
            let Some(text) = self.get_line(idx) else {
                continue;
            };
            let text = text.trim_end_matches(['\r', '\n']);
            match (line_instant(text.as_bytes()), found) {
                (Some(t), None) => {
                    found = Some(t);
                    seen.push(text.as_bytes().to_vec());
                }
                (Some(t), Some(f)) if t == f => seen.push(text.as_bytes().to_vec()),
                (Some(_), Some(_)) => break,
                (None, _) => {}
            }
        }
        found.map(|t| (t, seen))
    }

    /// Runs the ended source's command again, after the last line received, appending
    /// to the same spool.
    pub fn reconnect_system(&mut self, commands: &Commands) -> std::io::Result<()> {
        let last = self.last_instant();
        let Some(s) = self.system.as_mut() else {
            return Err(std::io::Error::other("not a system source"));
        };
        if s.exit.is_none() {
            return Err(std::io::Error::other("the source is still running"));
        }
        let mut commands = commands.clone();
        let since = match &last {
            Some((t, _)) => Some(*t),
            // Nothing stamped yet: the backlog again would repeat what is there.
            None => {
                commands.backlog = 0;
                None
            }
        };
        let (program, args) = s.uri.command_line(&commands, since);
        let (child, reader) = spawn(&program, &args)?;
        let resumed = match last {
            Some((t, seen)) => {
                s.copier
                    .resume(SkipSeen::new(reader, t, seen), &s.settings, s.wake.clone())
            }
            None => s.copier.resume(reader, &s.settings, s.wake.clone()),
        };
        if let Err(e) = resumed {
            kill(child);
            return Err(e);
        }
        s.child = Some(child);
        s.exit = None;
        self.follow_tail = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Option<Result<SystemUri, UriError>> {
        SystemUri::parse(Path::new(s))
    }

    #[test]
    fn docker_uris_are_validated() {
        assert_eq!(
            parse("docker://api"),
            Some(Ok(SystemUri::Docker {
                container: "api".into()
            }))
        );
        assert!(matches!(parse("docker://web_1.prod-2"), Some(Ok(_))));
        assert!(matches!(parse("docker://4f2a9c1b7e3d"), Some(Ok(_))));
        for bad in [
            "docker://",
            "docker://-f",
            "docker://--since=0",
            "docker://a b",
            "docker://a;rm",
            "docker://_x",
        ] {
            assert_eq!(parse(bad), Some(Err("system_bad_container")), "{bad}");
        }
        assert_eq!(parse("/var/log/syslog"), None);
        assert_eq!(parse("dockerfile.log"), None);
        assert!(is_system_path(Path::new("docker://x")));
        assert!(!is_system_path(Path::new("app.log")));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn journald_uris_are_validated() {
        let j = |user, unit: Option<&str>| {
            Some(Ok(SystemUri::Journald {
                user,
                unit: unit.map(String::from),
            }))
        };
        assert_eq!(parse("journald://"), j(false, None));
        assert_eq!(
            parse("journald://nginx.service"),
            j(false, Some("nginx.service"))
        );
        assert_eq!(
            parse("journald://user/syncthing.service"),
            j(true, Some("syncthing.service"))
        );
        assert_eq!(parse("journald://user"), j(true, None));
        assert_eq!(
            parse("journald://getty@tty1.service"),
            j(false, Some("getty@tty1.service"))
        );
        for bad in [
            "journald://-k",
            "journald://a b",
            "journald://x;y",
            "journald://a/b",
        ] {
            assert_eq!(parse(bad), Some(Err("system_bad_unit")), "{bad}");
        }
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn journald_is_linux_only() {
        assert_eq!(
            parse("journald://nginx.service"),
            Some(Err("system_journald_linux_only"))
        );
    }

    #[test]
    fn uris_and_titles_round_trip() {
        for (uri, title) in [
            ("docker://api", "docker: api"),
            ("journald://", "journal: system"),
            ("journald://nginx.service", "journal: nginx.service"),
            ("journald://user/a.service", "journal: user/a.service"),
        ] {
            let Some(Ok(parsed)) = parse(uri) else {
                // journald off Linux.
                continue;
            };
            assert_eq!(parsed.to_uri(), uri);
            assert_eq!(parsed.title(), title);
        }
    }

    #[test]
    fn command_lines() {
        let commands = Commands {
            docker: "podman".into(),
            journalctl: "journalctl".into(),
            backlog: 500,
        };
        let docker = SystemUri::Docker {
            container: "api".into(),
        };
        assert_eq!(
            docker.command_line(&commands, None),
            (
                "podman".to_string(),
                [
                    "logs",
                    "--follow",
                    "--timestamps",
                    "--tail",
                    "500",
                    "--",
                    "api"
                ]
                .map(String::from)
                .to_vec()
            )
        );
        let ms = crate::timestamp::date_to_days(2026, 10, 23).unwrap() * 86_400_000
            + 6 * 3_600_000
            + 15 * 60_000
            + 30_123;
        assert_eq!(
            docker.command_line(&commands, Some(ms)).1[3..5],
            [
                "--since".to_string(),
                "2026-10-23T06:15:30.123Z".to_string()
            ]
        );
        let journal = SystemUri::Journald {
            user: true,
            unit: Some("a.service".into()),
        };
        assert_eq!(
            journal.command_line(&commands, None).1,
            [
                "--follow",
                "--no-pager",
                "--output=short-iso-precise",
                "--lines=500",
                "--user",
                "--unit=a.service"
            ]
            .map(String::from)
            .to_vec()
        );
        assert_eq!(
            journal.command_line(&commands, Some(ms)).1[3],
            "--since=2026-10-23 06:15:30.123 UTC"
        );
        let system = SystemUri::Journald {
            user: false,
            unit: None,
        };
        assert_eq!(system.command_line(&commands, None).1.len(), 4);
    }

    #[test]
    fn both_timestamp_layouts_give_the_utc_instant() {
        let docker = line_instant(b"2026-10-23T06:15:30.123456789Z hello").unwrap();
        let journal =
            line_instant(b"2026-10-23T08:15:30.123456+02:00 host nginx[1]: hello").unwrap();
        assert_eq!(docker, journal);
        assert_eq!(rfc3339(docker), "2026-10-23T06:15:30.123Z");
    }

    #[test]
    fn a_reconnect_drops_what_it_had_already() {
        let input = b"2026-10-23T06:00:00.000Z old\n\
                      2026-10-23T06:00:01.000Z same\n\
                      2026-10-23T06:00:01.000Z twin\n\
                      2026-10-23T06:00:01.000Z new at the same instant\n\
                      continuation of it\n\
                      2026-10-23T06:00:02.000Z later\n";
        let t = line_instant(b"2026-10-23T06:00:01.000Z").unwrap();
        let seen = vec![
            b"2026-10-23T06:00:01.000Z same".to_vec(),
            b"2026-10-23T06:00:01.000Z twin".to_vec(),
        ];
        let mut out = String::new();
        SkipSeen::new(&input[..], t, seen)
            .read_to_string(&mut out)
            .unwrap();
        assert_eq!(
            out,
            "2026-10-23T06:00:01.000Z new at the same instant\n\
             continuation of it\n\
             2026-10-23T06:00:02.000Z later\n"
        );
        let mut none = String::new();
        SkipSeen::new(&b"2026-10-23T05:00:00.000Z gone\n"[..], t, Vec::new())
            .read_to_string(&mut none)
            .unwrap();
        assert!(none.is_empty());
    }
}
