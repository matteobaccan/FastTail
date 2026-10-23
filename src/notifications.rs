// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Desktop notifications of highlight rules with "Notify": a matching appended line becomes
//! a notification titled `FastTail · <stream>` with the line and the rule.
//!
//! `Throttle` keeps the flood out: one notification per rule per 10 seconds (later hits
//! counted, `+N more` on the next one, a summary once the rule is quiet) and at most 3 per
//! 10 seconds overall. `Notifier` shows them on a worker thread, so the UI never waits:
//! freedesktop notifications over D-Bus on Linux (a click on one is reported back), a
//! toast through Windows PowerShell on Windows, `osascript` on macOS. No crate is added
//! for it.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};

/// Per rule: one notification in this window.
pub const RULE_WINDOW: Duration = Duration::from_secs(10);
/// Overall: at most `GLOBAL_MAX` in this window.
pub const GLOBAL_WINDOW: Duration = Duration::from_secs(10);
pub const GLOBAL_MAX: usize = 3;
/// Characters of the line shown.
pub const MAX_TEXT: usize = 200;

/// A matching appended line, as the engine queues it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub rule: String,
    pub line: usize,
    pub text: String,
}

/// Where a notification leads when clicked: the stream and the line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub stream: PathBuf,
    pub line: usize,
}

/// A notification to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub title: String,
    pub body: String,
    pub target: Option<Target>,
}

/// The texts a notice is made of (in the user's language).
pub struct Texts<'a> {
    /// `+{0} more`
    pub more: &'a str,
    /// `{0} more matches of {1}`
    pub summary: &'a str,
}

#[derive(Debug, Default)]
struct RuleState {
    last: Option<Instant>,
    pending: u32,
    /// The stream of the last hit counted (the summary names it).
    stream: String,
}

/// Throttle and grouping of the hits (UI-free, so both interfaces use it).
#[derive(Debug, Default)]
pub struct Throttle {
    rules: HashMap<String, RuleState>,
    shown: VecDeque<Instant>,
}

/// `text` cut at `MAX_TEXT` characters, line breaks and control characters out.
pub fn short_text(text: &str) -> String {
    let mut out: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(MAX_TEXT)
        .collect();
    if text.chars().count() > MAX_TEXT {
        out.push('…');
    }
    out
}

impl Throttle {
    fn global_room(&mut self, now: Instant) -> bool {
        while self
            .shown
            .front()
            .is_some_and(|t| now.duration_since(*t) >= GLOBAL_WINDOW)
        {
            self.shown.pop_front();
        }
        self.shown.len() < GLOBAL_MAX
    }

    /// A hit of `stream_title` (path `stream`): the notice to show now, if any.
    pub fn offer(
        &mut self,
        hit: &Hit,
        stream_title: &str,
        stream: &std::path::Path,
        texts: &Texts<'_>,
        now: Instant,
    ) -> Option<Notice> {
        let room = self.global_room(now);
        let state = self.rules.entry(hit.rule.clone()).or_default();
        let recent = state
            .last
            .is_some_and(|t| now.duration_since(t) < RULE_WINDOW);
        if recent || !room {
            state.pending += 1;
            state.stream = stream_title.to_string();
            return None;
        }
        let mut body = short_text(&hit.text);
        if state.pending > 0 {
            body.push_str(&format!(
                " ({})",
                texts.more.replace("{0}", &state.pending.to_string())
            ));
        }
        body.push_str(&format!("\n{}", hit.rule));
        state.pending = 0;
        state.last = Some(now);
        self.shown.push_back(now);
        Some(Notice {
            title: format!("FastTail · {stream_title}"),
            body,
            target: Some(Target {
                stream: stream.to_path_buf(),
                line: hit.line,
            }),
        })
    }

    /// `n` hits of `rule` counted without a notice of their own (they show up as
    /// `+N more` or in the summary).
    pub fn hold(&mut self, rule: &str, stream_title: &str, n: u64, now: Instant) {
        let state = self.rules.entry(rule.to_string()).or_default();
        state.pending = state
            .pending
            .saturating_add(n.min(u64::from(u32::MAX)) as u32);
        state.stream = stream_title.to_string();
        // A rule never notified waits a window before its summary.
        state.last.get_or_insert(now);
    }

    /// Summaries of the rules quiet for a window with hits still counted.
    pub fn tick(&mut self, texts: &Texts<'_>, now: Instant) -> Vec<Notice> {
        let mut out = Vec::new();
        let mut quiet: Vec<String> = self
            .rules
            .iter()
            .filter(|(_, s)| {
                s.pending > 0 && s.last.is_none_or(|t| now.duration_since(t) >= RULE_WINDOW)
            })
            .map(|(r, _)| r.clone())
            .collect();
        quiet.sort();
        for rule in quiet {
            if !self.global_room(now) {
                break;
            }
            let state = self.rules.get_mut(&rule).expect("listed");
            out.push(Notice {
                title: format!("FastTail · {}", state.stream),
                body: texts
                    .summary
                    .replace("{0}", &state.pending.to_string())
                    .replace("{1}", &rule),
                target: None,
            });
            state.pending = 0;
            state.last = Some(now);
            self.shown.push_back(now);
        }
        out
    }
}

/// Whether a hit is notified: always, or (`background`) only when the user does not see
/// the stream.
pub fn wanted(always: bool, window_focused: bool, stream_displayed: bool) -> bool {
    always || !window_focused || !stream_displayed
}

/// Called from a back-end thread when the app has something to read.
pub type Wake = crate::tail_engine::WakeFn;

/// Why notifications cannot be shown on this system (an i18n key), checked once.
pub fn unavailable() -> Option<&'static str> {
    static CHECKED: std::sync::OnceLock<Option<&'static str>> = std::sync::OnceLock::new();
    *CHECKED.get_or_init(platform::check)
}

/// What the interfaces read from the settings for each drain.
pub struct Options {
    pub enabled: bool,
    /// `notify_when=always`.
    pub always: bool,
    /// The window (or terminal) has the focus.
    pub focused: bool,
}

/// The notifications of one interface: the throttle and the back end (started the first
/// time something is shown).
pub struct Center {
    throttle: Throttle,
    notifier: Option<Notifier>,
    wake: Wake,
}

impl Center {
    pub fn new(wake: Wake) -> Self {
        Self {
            throttle: Throttle::default(),
            notifier: None,
            wake,
        }
    }

    fn show(&mut self, notice: Notice) {
        let wake = self.wake.clone();
        self.notifier
            .get_or_insert_with(|| Notifier::start(wake))
            .show(notice);
    }

    /// Takes the hits the engines queued and shows what the throttle lets through; also
    /// the summaries due. Returns how many notices were shown.
    pub fn drain<'e>(
        &mut self,
        engines: impl IntoIterator<Item = &'e mut crate::tail_engine::TailEngine>,
        opts: &Options,
        texts: &Texts<'_>,
        now: Instant,
    ) -> usize {
        let mut out = Vec::new();
        for eng in engines {
            if eng.pending_notify_hits.is_empty() && eng.notify_overflow == 0 {
                continue;
            }
            let hits = std::mem::take(&mut eng.pending_notify_hits);
            let overflow = std::mem::take(&mut eng.notify_overflow);
            if !opts.enabled || !wanted(opts.always, opts.focused, eng.displayed) {
                continue;
            }
            let title = crate::find_all::stream_name(eng);
            for hit in &hits {
                if let Some(n) = self.throttle.offer(hit, &title, &eng.path, texts, now) {
                    out.push(n);
                }
            }
            // The hits beyond the engine's queue are counted on the last rule queued.
            if let (Some(last), true) = (hits.last(), overflow > 0) {
                self.throttle.hold(&last.rule, &title, overflow, now);
            }
        }
        out.extend(self.throttle.tick(texts, now));
        let shown = out.len();
        for n in out {
            self.show(n);
        }
        shown
    }

    /// The "Test notification" button.
    pub fn test(&mut self, title: &str, body: &str) {
        self.show(Notice {
            title: title.to_string(),
            body: body.to_string(),
            target: None,
        });
    }

    /// Whether hits are held back for a later `+N more` or summary.
    pub fn has_held(&self) -> bool {
        self.throttle.rules.values().any(|r| r.pending > 0)
    }

    /// A notification the user clicked, if any.
    pub fn clicked(&self) -> Option<Target> {
        self.notifier.as_ref().and_then(Notifier::clicked)
    }
}

// ---- platform back end ----

/// Shows notices on a worker thread; reports the clicked ones where the platform says.
pub struct Notifier {
    tx: Sender<Notice>,
    clicks: Receiver<Target>,
    /// Why notifications cannot be shown here (an i18n key), if they cannot.
    pub unavailable: Option<&'static str>,
}

impl Notifier {
    /// The back end of this platform; `wake` is called when a click is reported (so an
    /// idle window redraws and reads it).
    pub fn start(wake: Wake) -> Self {
        let (tx, rx) = channel::<Notice>();
        let (click_tx, clicks) = channel::<Target>();
        let unavailable = unavailable();
        if unavailable.is_none() {
            let _ = std::thread::Builder::new()
                .name("fasttail-notify".to_string())
                .spawn(move || platform::run(rx, click_tx, wake));
        }
        Self {
            tx,
            clicks,
            unavailable,
        }
    }

    pub fn show(&self, notice: Notice) {
        if self.unavailable.is_none() {
            let _ = self.tx.send(notice);
        }
    }

    /// A notification the user clicked, if any.
    pub fn clicked(&self) -> Option<Target> {
        self.clicks.try_recv().ok()
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::{Notice, Target};
    use std::collections::HashMap;
    use std::sync::mpsc::{Receiver, Sender};
    use std::sync::{Arc, Mutex};

    const DEST: &str = "org.freedesktop.Notifications";
    const PATH: &str = "/org/freedesktop/Notifications";

    pub fn check() -> Option<&'static str> {
        let ask = || -> zbus::Result<bool> {
            let conn = zbus::blocking::Connection::session()?;
            let dbus = zbus::blocking::fdo::DBusProxy::new(&conn)?;
            let name = zbus::names::BusName::try_from(DEST).map_err(zbus::Error::from)?;
            Ok(dbus.name_has_owner(name)?)
        };
        (!ask().unwrap_or(false)).then_some("notify_no_service")
    }

    pub fn run(rx: Receiver<Notice>, clicks: Sender<Target>, wake: super::Wake) {
        let Ok(conn) = zbus::blocking::Connection::session() else {
            return;
        };
        let targets: Arc<Mutex<HashMap<u32, Target>>> = Arc::default();
        // Clicks: the server's ActionInvoked signal for the default action.
        {
            let (conn, targets) = (conn.clone(), targets.clone());
            let _ = std::thread::Builder::new()
                .name("fasttail-notify-click".to_string())
                .spawn(move || {
                    let Ok(proxy) = zbus::blocking::Proxy::new(&conn, DEST, PATH, DEST) else {
                        return;
                    };
                    let Ok(signals) = proxy.receive_signal("ActionInvoked") else {
                        return;
                    };
                    for msg in signals {
                        let Ok((id, action)) = msg.body().deserialize::<(u32, String)>() else {
                            continue;
                        };
                        if action != "default" {
                            continue;
                        }
                        let target = targets.lock().ok().and_then(|mut t| t.remove(&id));
                        if let Some(target) = target {
                            if clicks.send(target).is_err() {
                                return;
                            }
                            wake();
                        }
                    }
                });
        }
        let Ok(proxy) = zbus::blocking::Proxy::new(&conn, DEST, PATH, DEST) else {
            return;
        };
        for notice in rx {
            let actions: Vec<&str> = if notice.target.is_some() {
                vec!["default", "Open"]
            } else {
                Vec::new()
            };
            let hints: HashMap<&str, zbus::zvariant::Value<'_>> = HashMap::new();
            let reply: zbus::Result<u32> = proxy.call(
                "Notify",
                &(
                    "FastTail",
                    0u32,
                    "fasttail",
                    notice.title.as_str(),
                    notice.body.as_str(),
                    actions,
                    hints,
                    -1i32,
                ),
            );
            if let (Ok(id), Some(target)) = (reply, notice.target) {
                if let Ok(mut t) = targets.lock() {
                    // Old ones are forgotten: the server dismisses them anyway.
                    if t.len() > 256 {
                        t.clear();
                    }
                    t.insert(id, target);
                }
            }
        }
    }
}

#[cfg(windows)]
mod platform {
    use super::{Notice, Target};
    use std::sync::mpsc::{Receiver, Sender};

    const APP_ID: &str = "FastTail";

    pub fn check() -> Option<&'static str> {
        None
    }

    /// The AppUserModelID toasts are shown under (its display name), written once.
    fn register() {
        let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
        if let Ok((key, _)) =
            hkcu.create_subkey(format!(r"Software\Classes\AppUserModelId\{APP_ID}"))
        {
            let _ = key.set_value("DisplayName", &"FastTail");
        }
    }

    fn xml(text: &str) -> String {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&apos;")
    }

    /// The PowerShell script that shows `notice` (passed encoded, so nothing is quoted on
    /// a command line).
    pub(super) fn script(notice: &Notice) -> String {
        let toast = format!(
            "<toast><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual></toast>",
            xml(&notice.title),
            xml(&notice.body)
        );
        format!(
            "$null=[Windows.UI.Notifications.ToastNotificationManager,Windows.UI.Notifications,ContentType=WindowsRuntime];\
             $d=New-Object Windows.Data.Xml.Dom.XmlDocument;$d.LoadXml('{}');\
             [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('{APP_ID}').Show([Windows.UI.Notifications.ToastNotification]::new($d))",
            toast.replace('\'', "''")
        )
    }

    pub fn run(rx: Receiver<Notice>, _clicks: Sender<Target>, _wake: super::Wake) {
        register();
        for notice in rx {
            let utf16: Vec<u8> = script(&notice)
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect();
            let mut cmd = std::process::Command::new("powershell.exe");
            cmd.args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-EncodedCommand",
                &super::base64(&utf16),
            ])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
            {
                use std::os::windows::process::CommandExt;
                cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
            }
            if let Ok(mut child) = cmd.spawn() {
                let _ = child.wait();
            }
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::{Notice, Target};
    use std::sync::mpsc::{Receiver, Sender};

    pub fn check() -> Option<&'static str> {
        None
    }

    fn quoted(text: &str) -> String {
        format!(
            "\"{}\"",
            text.replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace(['\n', '\r'], " — ")
        )
    }

    pub fn run(rx: Receiver<Notice>, _clicks: Sender<Target>, _wake: super::Wake) {
        for notice in rx {
            let script = format!(
                "display notification {} with title {}",
                quoted(&notice.body),
                quoted(&notice.title)
            );
            let _ = std::process::Command::new("osascript")
                .args(["-e", &script])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
    }
}

#[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
mod platform {
    use super::{Notice, Target};
    use std::sync::mpsc::{Receiver, Sender};

    pub fn check() -> Option<&'static str> {
        Some("notify_no_service")
    }

    pub fn run(_rx: Receiver<Notice>, _clicks: Sender<Target>, _wake: super::Wake) {}
}

/// Standard base64 of `bytes` (PowerShell's `-EncodedCommand`).
#[cfg_attr(not(windows), allow(dead_code))]
fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXTS: Texts<'static> = Texts {
        more: "+{0} more",
        summary: "{0} more matches of {1}",
    };

    fn hit(rule: &str, line: usize) -> Hit {
        Hit {
            rule: rule.into(),
            line,
            text: format!("line {line}"),
        }
    }

    #[test]
    fn one_per_rule_per_window_then_more_and_a_summary() {
        let mut t = Throttle::default();
        let now = Instant::now();
        let path = std::path::Path::new("app.log");
        let first = t
            .offer(&hit("ERROR", 1), "app.log", path, &TEXTS, now)
            .unwrap();
        assert_eq!(first.title, "FastTail · app.log");
        assert_eq!(first.body, "line 1\nERROR");
        assert_eq!(first.target.unwrap().line, 1);
        assert!(t
            .offer(&hit("ERROR", 2), "app.log", path, &TEXTS, now)
            .is_none());
        assert!(t
            .offer(
                &hit("ERROR", 3),
                "app.log",
                path,
                &TEXTS,
                now + Duration::from_millis(900)
            )
            .is_none());
        // A later hit says how many were held back.
        let later = now + RULE_WINDOW;
        let next = t
            .offer(&hit("ERROR", 9), "app.log", path, &TEXTS, later)
            .unwrap();
        assert_eq!(next.body, "line 9 (+2 more)\nERROR");
        // Without one, a summary once the rule is quiet.
        assert!(t
            .offer(&hit("ERROR", 10), "app.log", path, &TEXTS, later)
            .is_none());
        assert!(t.tick(&TEXTS, later + Duration::from_secs(1)).is_empty());
        let summary = t.tick(&TEXTS, later + RULE_WINDOW);
        assert_eq!(summary.len(), 1);
        assert_eq!(summary[0].body, "1 more matches of ERROR");
        assert!(summary[0].target.is_none());
    }

    #[test]
    fn three_per_window_overall() {
        let mut t = Throttle::default();
        let now = Instant::now();
        let path = std::path::Path::new("a");
        let shown = ["A", "B", "C", "D", "E"]
            .iter()
            .filter(|r| t.offer(&hit(r, 1), "a", path, &TEXTS, now).is_some())
            .count();
        assert_eq!(shown, GLOBAL_MAX);
        // The two held back come as summaries once there is room.
        assert_eq!(t.tick(&TEXTS, now + GLOBAL_WINDOW).len(), 2);
    }

    #[test]
    fn visibility_and_text() {
        assert!(wanted(true, true, true));
        assert!(!wanted(false, true, true));
        assert!(wanted(false, false, true));
        assert!(wanted(false, true, false));
        let long = "x".repeat(300);
        assert_eq!(short_text(&long).chars().count(), MAX_TEXT + 1);
        assert_eq!(short_text("a\tb\u{1b}[31m"), "a b [31m");
    }

    #[test]
    fn base64_of_known_values() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn held_hits_come_back_in_the_summary() {
        let mut t = Throttle::default();
        let now = Instant::now();
        t.hold("ERROR", "app.log", 5, now);
        assert!(t.tick(&TEXTS, now).is_empty());
        let summary = t.tick(&TEXTS, now + RULE_WINDOW);
        assert_eq!(summary[0].body, "5 more matches of ERROR");
        assert_eq!(summary[0].title, "FastTail · app.log");
    }
}
