// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Source plugin streams (`plugin://<name>?<param>=<value>…`): the plugin's program runs
//! without a shell, its parameters expanded one argument each, with standard input
//! closed, the plugin folder as working directory and `FASTTAIL_PLUGIN_API=1`; its
//! standard output is copied into a spool with the standard-input copier and the engine
//! tails the spool. The last 4 KB of its standard error are kept for the stream bar.
//!
//! A stream whose plugin is missing, not approved, changed since its approval, or whose
//! values are incomplete (a secret is never stored) opens all the same, empty, saying so,
//! with its identity kept: Start runs it once the cause is gone. With `restart =
//! "on-exit"` an ended program runs again after 1, 2, 5, 10, then every 30 seconds.
//! Closing the stream ends the program and the processes it started.

use std::collections::{BTreeMap, VecDeque};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::plugins::{Approvals, Catalog, Plugin, PluginUri, Restart, State};
use crate::stdin_source::{InputState, Settings, StdinStream};
use crate::tail_engine::{TailEngine, WakeFn};

/// Bytes of standard error kept.
pub const STDERR_BYTES: usize = 4096;
/// Seconds before each restart; the last one repeats.
const RESTART_DELAYS: [u64; 5] = [1, 2, 5, 10, 30];
/// A program that ran this long starts the back-off again from the first delay.
const STEADY: Duration = Duration::from_secs(60);

/// Why a plugin stream does not run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Block {
    /// No plugin of that name in the plugins folder.
    Missing,
    /// Not approved in Settings ▸ Plugins (or a formats plugin, which runs nothing).
    NotEnabled,
    /// Approved once, but its manifest changed since.
    Changed,
    /// A value is missing or wrong (a secret, asked again after a restore).
    NeedsValues,
}

/// What the stream bar shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Blocked(Block),
    Running,
    /// Waiting to run again: the attempt and when.
    Restarting {
        attempt: u32,
        at: Instant,
    },
    /// Ended, with its exit code (`None`: killed by a signal) and not restarted.
    Ended(Option<i32>),
    /// It could not be started (the reason in `error`).
    Failed,
}

/// A plugin stream, owned by its engine (`TailEngine::plugin`).
pub struct PluginStream {
    /// The identity with every value, secrets included (they live in memory only).
    pub uri: PluginUri,
    status: Status,
    restart: Restart,
    /// The program, its arguments and the plugin folder, as of the last start.
    program: PathBuf,
    args: Vec<String>,
    dir: PathBuf,
    child: Option<Child>,
    started: Instant,
    attempts: u32,
    stderr: Arc<Mutex<VecDeque<u8>>>,
    /// Why the program could not start.
    pub error: Option<String>,
    settings: Settings,
    wake: Option<WakeFn>,
    /// The stream bar's Start was clicked (the front end asks the values or approves).
    pub start_requested: bool,
    /// The output copier; declared last so the child is ended before the spool goes.
    pub copier: StdinStream,
}

/// Whether `plugin` may run with `uri`'s values, and why not.
pub fn block_of(plugin: Option<&Plugin>, approvals: &Approvals, uri: &PluginUri) -> Option<Block> {
    let Some(plugin) = plugin else {
        return Some(Block::Missing);
    };
    let Some(source) = plugin.source.as_ref() else {
        return Some(Block::NotEnabled);
    };
    match approvals.state(plugin) {
        State::Enabled => {}
        State::Changed => return Some(Block::Changed),
        State::Disabled => return Some(Block::NotEnabled),
    }
    let bad = source.params.iter().any(|p| {
        let value = uri.values.get(&p.id);
        match (p.kind, value) {
            (crate::plugins::ParamType::Secret, None) => true,
            (_, None) => crate::plugins::check_value(p, &p.default).is_some(),
            (_, Some(v)) => crate::plugins::check_value(p, v).is_some(),
        }
    });
    bad.then_some(Block::NeedsValues)
}

impl PluginStream {
    pub fn status(&self) -> Status {
        self.status
    }

    /// The plugin's name, the stream's title.
    pub fn title(&self) -> String {
        self.uri.name.clone()
    }

    /// The last 4 KB its program wrote on standard error.
    pub fn stderr_tail(&self) -> String {
        self.stderr
            .lock()
            .map(|b| {
                let (a, c) = b.as_slices();
                let mut v = a.to_vec();
                v.extend_from_slice(c);
                String::from_utf8_lossy(&v).into_owned()
            })
            .unwrap_or_default()
    }

    /// Runs the program for `plugin` with `values` (after Start, or the values asked
    /// again). Refused while it runs, or when it may not run.
    pub fn start(
        &mut self,
        plugin: Option<&Plugin>,
        approvals: &Approvals,
        values: Option<BTreeMap<String, String>>,
        config_dir: &Path,
    ) -> Result<(), Block> {
        if matches!(self.status, Status::Running) {
            return Ok(());
        }
        if let Some(values) = values {
            self.uri.values = values;
        }
        if let Some(block) = block_of(plugin, approvals, &self.uri) {
            self.status = Status::Blocked(block);
            return Err(block);
        }
        let plugin = plugin.expect("not blocked: the plugin exists");
        let source = plugin.source.as_ref().expect("a source plugin");
        self.program = crate::plugins::resolve_command(&plugin.dir, &source.command);
        self.args = crate::plugins::expand_args(source, &self.uri.values, &plugin.dir, config_dir);
        self.dir = plugin.dir.clone();
        self.restart = source.restart;
        self.attempts = 0;
        self.launch();
        Ok(())
    }

    /// Starts the program as last set up and copies its output after what the spool
    /// holds.
    fn launch(&mut self) {
        // The copier of an empty placeholder ends at once; wait for it to have.
        let waited = Instant::now();
        while self.copier.is_reading() && waited.elapsed() < Duration::from_secs(2) {
            std::thread::sleep(Duration::from_millis(5));
        }
        if let Ok(mut b) = self.stderr.lock() {
            b.clear();
        }
        match spawn(
            &self.program,
            &self.args,
            &self.dir,
            Arc::clone(&self.stderr),
        ) {
            Ok((child, stdout)) => {
                if let Err(e) = self
                    .copier
                    .resume(stdout, &self.settings, self.wake.clone())
                {
                    kill_tree(child);
                    self.fail(e.to_string());
                    return;
                }
                self.child = Some(child);
                self.started = Instant::now();
                self.status = Status::Running;
                self.error = None;
            }
            Err(e) => self.fail(format!("{}: {e}", self.program.display())),
        }
    }

    fn fail(&mut self, error: String) {
        self.error = Some(error);
        self.schedule_or(Status::Failed);
    }

    /// After an end: the next restart when the plugin asks for them, else `end`.
    fn schedule_or(&mut self, end: Status) {
        if self.restart == Restart::OnExit {
            if self.started.elapsed() >= STEADY {
                self.attempts = 0;
            }
            let delay = RESTART_DELAYS[(self.attempts as usize).min(RESTART_DELAYS.len() - 1)];
            self.attempts += 1;
            self.status = Status::Restarting {
                attempt: self.attempts,
                at: Instant::now() + Duration::from_secs(delay),
            };
        } else {
            self.status = end;
        }
    }
}

impl Drop for PluginStream {
    fn drop(&mut self) {
        if let Some(child) = self.child.take() {
            kill_tree(child);
        }
    }
}

/// Starts `program` with `args` in `dir`: its standard output to read, its standard
/// error into `stderr` (the last `STDERR_BYTES`).
fn spawn(
    program: &Path,
    args: &[String],
    dir: &Path,
    stderr: Arc<Mutex<VecDeque<u8>>>,
) -> std::io::Result<(Child, std::process::ChildStdout)> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .current_dir(dir)
        .env("FASTTAIL_PLUGIN_API", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Its own process group, so closing the stream ends what it started too.
        cmd.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd.spawn()?;
    let stdout = child.stdout.take().expect("piped");
    if let Some(mut err) = child.stderr.take() {
        let _ = std::thread::Builder::new()
            .name("fasttail-plugin-stderr".into())
            .spawn(move || {
                let mut buf = [0u8; 1024];
                while let Ok(n) = err.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    if let Ok(mut ring) = stderr.lock() {
                        ring.extend(&buf[..n]);
                        while ring.len() > STDERR_BYTES {
                            ring.pop_front();
                        }
                    }
                }
            });
    }
    Ok((child, stdout))
}

/// Ends `child` and the processes it started (its process group on Unix, its tree on
/// Windows), and reaps it on a detached thread.
fn kill_tree(mut child: Child) {
    let pid = child.id().to_string();
    #[cfg(unix)]
    let tree = Command::new("kill")
        // `--`: procps' kill would read `-<pgid>` as an option.
        .args(["-KILL", "--", &format!("-{pid}")])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    #[cfg(windows)]
    let tree = {
        use std::os::windows::process::CommandExt;
        Command::new("taskkill")
            .args(["/T", "/F", "/PID", &pid])
            .creation_flags(0x0800_0000)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
    };
    #[cfg(not(any(unix, windows)))]
    let tree: std::io::Result<std::process::ExitStatus> = Err(std::io::Error::other("no tree"));
    let _ = tree;
    let _ = child.kill();
    let _ = std::thread::Builder::new()
        .name("fasttail-reap".to_string())
        .spawn(move || {
            let _ = child.wait();
        });
}

/// Opens the stream of `uri`: its program runs when it may, else the stream opens empty
/// saying why. `path` is the URI without the secrets.
pub fn open_engine(
    uri: PluginUri,
    catalog: &Catalog,
    approvals: &Approvals,
    settings: &Settings,
    config_dir: &Path,
    wake: Option<WakeFn>,
) -> std::io::Result<TailEngine> {
    let plugin = catalog.find(&uri.name);
    let copier = StdinStream::start_named(std::io::empty(), settings, wake.clone(), "plugin")?;
    let mut stream = PluginStream {
        uri,
        status: Status::Blocked(Block::NotEnabled),
        restart: Restart::Never,
        program: PathBuf::new(),
        args: Vec::new(),
        dir: PathBuf::new(),
        child: None,
        started: Instant::now(),
        attempts: 0,
        stderr: Arc::new(Mutex::new(VecDeque::new())),
        error: None,
        settings: settings.clone(),
        wake: wake.clone(),
        start_requested: false,
        copier,
    };
    let _ = stream.start(plugin, approvals, None, config_dir);
    let mut engine = match wake {
        Some(wake) => TailEngine::open_with_wake(stream.copier.spool_path(), wake)?,
        None => TailEngine::open(stream.copier.spool_path())?,
    };
    engine.path = PathBuf::from(stream.uri.to_uri(plugin));
    engine.follow_tail = true;
    engine.plugin = Some(stream);
    Ok(engine)
}

impl TailEngine {
    /// Called from `poll_updates`: once the program's output is copied and indexed,
    /// collects its exit code; runs it again when a restart is due.
    pub(crate) fn poll_plugin(&mut self) {
        let index_pending = self.index_pending;
        let Some(s) = self.plugin.as_mut() else {
            return;
        };
        match s.status {
            Status::Running => {
                if s.copier.state() == InputState::Reading
                    || (s.copier.state() == InputState::Ended && !s.copier.is_finalized())
                    || index_pending
                {
                    return;
                }
                let Some(child) = s.child.as_mut() else {
                    return;
                };
                let code = match child.try_wait() {
                    Ok(Some(status)) => status.code(),
                    Ok(None) => return,
                    Err(_) => None,
                };
                s.child = None;
                s.schedule_or(Status::Ended(code));
            }
            Status::Restarting { at, .. } if Instant::now() >= at => s.launch(),
            _ => {}
        }
    }

    /// The plugin stream's state while it is not running normally (for the stream bar).
    pub fn plugin_status(&self) -> Option<Status> {
        self.plugin.as_ref().map(PluginStream::status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_say_why_a_stream_does_not_run() {
        let mut p = crate::plugins::parse_manifest(
            "api = 1\nname='p'\nversion='1'\nkind='source'\n[source]\ncommand='x'\n[[source.param]]\nid='t'\ntype='secret'",
        )
        .unwrap();
        p.hash = "h".into();
        let mut a = Approvals::default();
        let uri = PluginUri::parse(Path::new("plugin://p")).unwrap();
        assert_eq!(block_of(None, &a, &uri), Some(Block::Missing));
        assert_eq!(block_of(Some(&p), &a, &uri), Some(Block::NotEnabled));
        a.set_enabled(&p, true);
        assert_eq!(block_of(Some(&p), &a, &uri), Some(Block::NeedsValues));
        let full = PluginUri::parse(Path::new("plugin://p?t=s")).unwrap();
        assert_eq!(block_of(Some(&p), &a, &full), None);
        p.hash = "other".into();
        assert_eq!(block_of(Some(&p), &a, &full), Some(Block::Changed));
    }
}
