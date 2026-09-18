//! External tools: user-configured commands launched from a row (context menu, stream
//! menu, keyboard shortcut) or automatically when a highlight rule matches an appended line.
//!
//! Safety model: placeholders are expanded per argument and every argument reaches the
//! child as its own `argv` entry through `std::process::Command`, so a log line containing
//! `; rm -rf /` is just text. The optional shell mode (`cmd /c` / `sh -c`) is off by default
//! and joins the expanded arguments into one command line, which the shell then parses.
//!
//! Rule-bound runs are throttled to one per second per tool and capped at ten concurrent
//! children; runs above either limit are dropped and counted per tool.

use regex::Regex;
use std::collections::HashMap;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Placeholders accepted in the argument list, in the order shown to the user.
pub const PLACEHOLDERS: [&str; 6] = [
    "{line}",
    "{file}",
    "{dir}",
    "{lineno}",
    "{selection}",
    "{match}",
];

/// Minimum interval between two rule-bound runs of the same tool.
pub const RULE_THROTTLE: Duration = Duration::from_secs(1);
/// Maximum number of children spawned by rule-bound runs still running at the same time.
pub const MAX_CHILDREN: usize = 10;

/// A user-configured command, persisted as a `[tool.N]` section of `fasttail.ini`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExternalTool {
    pub name: String,
    /// Executable to run (looked up on `PATH` when not a path).
    pub program: String,
    /// Argument list, split like a shell command line (double and single quotes group
    /// words); each entry may contain placeholders.
    #[serde(default)]
    pub args: String,
    /// Keyboard shortcut such as `Ctrl+Shift+F9`, see [`Shortcut::parse`].
    #[serde(default)]
    pub shortcut: Option<String>,
    /// Pattern of the highlight rule whose matches on appended lines launch the tool.
    #[serde(default)]
    pub bound_rule: Option<String>,
    /// Run through `cmd /c` (Windows) or `sh -c` (elsewhere) instead of spawning directly.
    #[serde(default)]
    pub use_shell: bool,
    /// Regex whose first capture group (or whole match) fills `{match}`.
    #[serde(default)]
    pub match_pattern: Option<String>,
}

impl ExternalTool {
    pub fn new(name: &str, program: &str, args: &str) -> Self {
        Self {
            name: name.to_string(),
            program: program.to_string(),
            args: args.to_string(),
            shortcut: None,
            bound_rule: None,
            use_shell: false,
            match_pattern: None,
        }
    }

    /// The parsed shortcut, if one is set and valid.
    pub fn parsed_shortcut(&self) -> Option<Shortcut> {
        self.shortcut.as_deref().and_then(Shortcut::parse)
    }
}

/// Data of the row a tool runs on.
#[derive(Debug, Clone, Default)]
pub struct ToolContext {
    pub line: String,
    pub file: String,
    pub dir: String,
    /// 1-based line number.
    pub lineno: usize,
    /// Selected rows as text (one per line), or the row itself when nothing is selected.
    pub selection: String,
}

impl ToolContext {
    pub fn for_row(file: &Path, lineno: usize, line: &str, selection: Option<&str>) -> Self {
        Self {
            line: line.to_string(),
            file: file.display().to_string(),
            dir: file
                .parent()
                .map(|d| d.display().to_string())
                .unwrap_or_default(),
            lineno,
            selection: selection.unwrap_or(line).to_string(),
        }
    }
}

/// Splits an argument string like a shell command line: whitespace separates words,
/// double or single quotes group them, a backslash escapes the next character inside
/// double quotes and outside quotes.
pub fn split_args(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_word = false;
    let mut quote: Option<char> = None;
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        match quote {
            Some('\'') => {
                if c == '\'' {
                    quote = None;
                } else {
                    cur.push(c);
                }
            }
            Some(_) => {
                if c == '"' {
                    quote = None;
                } else if c == '\\' {
                    match chars.peek() {
                        Some('"') | Some('\\') => cur.push(chars.next().unwrap()),
                        _ => cur.push(c),
                    }
                } else {
                    cur.push(c);
                }
            }
            None => {
                if c.is_whitespace() {
                    if in_word {
                        out.push(std::mem::take(&mut cur));
                        in_word = false;
                    }
                } else if c == '"' || c == '\'' {
                    quote = Some(c);
                    in_word = true;
                } else if c == '\\' {
                    in_word = true;
                    match chars.next() {
                        Some(n) => cur.push(n),
                        None => cur.push(c),
                    }
                } else {
                    in_word = true;
                    cur.push(c);
                }
            }
        }
    }
    if in_word {
        out.push(cur);
    }
    out
}

/// Value of `{match}` for a row: the first capture group of the tool's regex, the whole
/// match when the regex has no group, or an empty string.
pub fn match_value(tool: &ExternalTool, line: &str) -> String {
    let Some(pat) = tool.match_pattern.as_deref().filter(|p| !p.is_empty()) else {
        return String::new();
    };
    let Ok(re) = Regex::new(pat) else {
        return String::new();
    };
    re.captures(line)
        .map(|c| {
            c.get(1)
                .or_else(|| c.get(0))
                .map(|m| m.as_str().to_string())
                .unwrap_or_default()
        })
        .unwrap_or_default()
}

/// Expands every placeholder of one argument.
pub fn expand_argument(arg: &str, ctx: &ToolContext, matched: &str) -> String {
    if !arg.contains('{') {
        return arg.to_string();
    }
    // Single pass over the template: a value substituted for one placeholder is never
    // scanned again, so a log line containing the literal text `{file}` reaches the
    // tool unchanged instead of being expanded a second time.
    let mut out = String::with_capacity(arg.len() + 64);
    let mut rest = arg;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open..];
        match after.find('}') {
            Some(close) => {
                let name = &after[1..close];
                let value: Option<std::borrow::Cow<str>> = match name {
                    "line" => Some(ctx.line.as_str().into()),
                    "file" => Some(ctx.file.as_str().into()),
                    "dir" => Some(ctx.dir.as_str().into()),
                    "lineno" => Some(ctx.lineno.to_string().into()),
                    "selection" => Some(ctx.selection.as_str().into()),
                    "match" => Some(matched.into()),
                    _ => None,
                };
                match value {
                    Some(v) => {
                        out.push_str(&v);
                        rest = &after[close + 1..];
                    }
                    None => {
                        // Unknown placeholder or stray brace: keep the brace literally.
                        out.push('{');
                        rest = &after[1..];
                    }
                }
            }
            None => {
                out.push_str(after);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// The expanded argument list of a tool for a row, one entry per argv element.
pub fn expanded_args(tool: &ExternalTool, ctx: &ToolContext) -> Vec<String> {
    let matched = match_value(tool, &ctx.line);
    split_args(&tool.args)
        .iter()
        .map(|a| expand_argument(a, ctx, &matched))
        .collect()
}

/// Builds the command for a tool on a row. Without the shell flag the program is spawned
/// directly with one argv entry per expanded argument; with it, `cmd /c` (Windows) or
/// `sh -c` runs the program and the arguments joined by spaces.
pub fn build_command(tool: &ExternalTool, ctx: &ToolContext) -> Command {
    let args = expanded_args(tool, ctx);
    let mut cmd = if tool.use_shell {
        let mut line = tool.program.clone();
        for a in &args {
            line.push(' ');
            line.push_str(a);
        }
        #[cfg(windows)]
        let mut cmd = {
            let mut c = Command::new("cmd");
            c.arg("/c").arg(line);
            c
        };
        #[cfg(not(windows))]
        let mut cmd = {
            let mut c = Command::new("sh");
            c.arg("-c").arg(line);
            c
        };
        cmd.stdin(Stdio::null());
        cmd
    } else {
        let mut c = Command::new(&tool.program);
        c.args(&args);
        c.stdin(Stdio::null());
        c
    };
    cmd.stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: console tools do not flash a console window.
        cmd.creation_flags(0x0800_0000);
    }
    cmd
}

/// A parsed keyboard shortcut: modifier flags plus the egui key name (`F9`, `E`, `Num1`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shortcut {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub key: egui::Key,
}

impl Shortcut {
    /// Parses `Ctrl+Shift+F9`, `ctrl+alt+e`, `Alt+1` (case-insensitive, `+` or `-`
    /// separated). Bare digits and letters map to egui's `1`..`9` and `A`..`Z`; every
    /// other key uses egui's own name (`F1`..`F35`, `Enter`, `Home`, ...). At least one
    /// modifier is required so a tool cannot swallow plain typing.
    pub fn parse(s: &str) -> Option<Self> {
        let mut out = Shortcut {
            ctrl: false,
            shift: false,
            alt: false,
            key: egui::Key::Space,
        };
        let mut key: Option<egui::Key> = None;
        for part in s.split(['+', '-']).map(str::trim).filter(|p| !p.is_empty()) {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" | "cmd" | "command" => out.ctrl = true,
                "shift" => out.shift = true,
                "alt" | "option" => out.alt = true,
                name => {
                    let candidate = if name.len() == 1 {
                        // egui names digits "1".."9" and letters "A".."Z".
                        name.to_ascii_uppercase()
                    } else {
                        // Capitalise the first letter so `f9`, `enter`, `home` resolve.
                        let mut chars = name.chars();
                        let first = chars.next().unwrap().to_ascii_uppercase();
                        format!("{first}{}", chars.as_str())
                    };
                    if key.is_some() {
                        return None;
                    }
                    key = egui::Key::from_name(&candidate);
                    key?;
                }
            }
        }
        if !(out.ctrl || out.shift || out.alt) {
            return None;
        }
        out.key = key?;
        Some(out)
    }

    pub fn modifiers(&self) -> egui::Modifiers {
        let mut m = egui::Modifiers::NONE;
        if self.ctrl {
            m |= egui::Modifiers::COMMAND;
        }
        if self.shift {
            m |= egui::Modifiers::SHIFT;
        }
        if self.alt {
            m |= egui::Modifiers::ALT;
        }
        m
    }
}

/// Outcome of a rule-bound run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunOutcome {
    Spawned,
    /// The tool ran less than the throttle interval ago.
    Throttled,
    /// The concurrent-children cap was reached.
    CapReached,
    Failed(String),
}

/// Spawns tools and enforces the rule-bound limits. Children are kept only to count the
/// running ones; they are never waited for and never killed.
pub struct ToolRunner {
    children: Vec<Child>,
    last_run: HashMap<String, Instant>,
    dropped: HashMap<String, u64>,
    throttle: Duration,
    max_children: usize,
    /// Last spawn error, for the UI.
    pub last_error: Option<String>,
}

impl Default for ToolRunner {
    fn default() -> Self {
        Self::with_limits(RULE_THROTTLE, MAX_CHILDREN)
    }
}

impl ToolRunner {
    pub fn with_limits(throttle: Duration, max_children: usize) -> Self {
        Self {
            children: Vec::new(),
            last_run: HashMap::new(),
            dropped: HashMap::new(),
            throttle,
            max_children,
            last_error: None,
        }
    }

    /// Forgets the children that have exited.
    pub fn reap(&mut self) {
        self.children
            .retain_mut(|c| matches!(c.try_wait(), Ok(None)));
    }

    /// Children spawned by rule-bound runs that are still running.
    pub fn running(&mut self) -> usize {
        self.reap();
        self.children.len()
    }

    /// Rule-bound runs dropped so far for a tool (throttle or cap).
    pub fn dropped_for(&self, tool_name: &str) -> u64 {
        self.dropped.get(tool_name).copied().unwrap_or(0)
    }

    /// Runs a tool on a row from a user gesture: no throttle, no cap, the child handle is
    /// dropped (fire and forget).
    pub fn run_manual(&mut self, tool: &ExternalTool, ctx: &ToolContext) -> Result<(), String> {
        match build_command(tool, ctx).spawn() {
            Ok(_child) => {
                self.last_error = None;
                Ok(())
            }
            Err(e) => {
                let msg = format!("{}: {e}", tool.program);
                self.last_error = Some(msg.clone());
                Err(msg)
            }
        }
    }

    /// Runs a tool because its bound rule matched a line: at most once per throttle
    /// interval per tool and never above the children cap; dropped runs are counted.
    pub fn run_bound(&mut self, tool: &ExternalTool, ctx: &ToolContext) -> RunOutcome {
        if let Some(last) = self.last_run.get(&tool.name) {
            if last.elapsed() < self.throttle {
                *self.dropped.entry(tool.name.clone()).or_insert(0) += 1;
                return RunOutcome::Throttled;
            }
        }
        self.reap();
        if self.children.len() >= self.max_children {
            *self.dropped.entry(tool.name.clone()).or_insert(0) += 1;
            return RunOutcome::CapReached;
        }
        match build_command(tool, ctx).spawn() {
            Ok(child) => {
                self.children.push(child);
                self.last_run.insert(tool.name.clone(), Instant::now());
                self.last_error = None;
                RunOutcome::Spawned
            }
            Err(e) => {
                let msg = format!("{}: {e}", tool.program);
                self.last_error = Some(msg.clone());
                RunOutcome::Failed(msg)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_expand_in_a_single_pass() {
        let ctx = ToolContext {
            line: "payload {file} and {match} stay literal".to_string(),
            file: "app.log".to_string(),
            dir: "C:/logs".to_string(),
            lineno: 7,
            selection: String::new(),
        };
        let out = expand_argument("{lineno}:{line}|{file}|{nope}|{", &ctx, "M");
        assert_eq!(
            out,
            "7:payload {file} and {match} stay literal|app.log|{nope}|{"
        );
    }

    #[test]
    fn splits_like_a_shell() {
        assert_eq!(split_args("a b  c"), vec!["a", "b", "c"]);
        assert_eq!(
            split_args(r#"-g "{file}:{lineno}" 'two words' esc\ aped"#),
            vec!["-g", "{file}:{lineno}", "two words", "esc aped"]
        );
        assert_eq!(
            split_args(r#""quoted \"inner\"""#),
            vec![r#"quoted "inner""#]
        );
        assert!(split_args("   ").is_empty());
    }

    #[test]
    fn shortcut_parsing() {
        let s = Shortcut::parse("Ctrl+Shift+F9").unwrap();
        assert!(s.ctrl && s.shift && !s.alt);
        assert_eq!(s.key, egui::Key::F9);
        assert_eq!(Shortcut::parse("alt+1").unwrap().key, egui::Key::Num1);
        assert_eq!(Shortcut::parse("ctrl-e").unwrap().key, egui::Key::E);
        assert!(Shortcut::parse("F9").is_none(), "a modifier is required");
        assert!(Shortcut::parse("Ctrl+Nope").is_none());
        assert!(Shortcut::parse("Ctrl+A+B").is_none());
    }
}
