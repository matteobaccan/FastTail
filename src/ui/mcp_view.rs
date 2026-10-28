// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The window's side of the MCP server (`crate::mcp`): starts and stops the server as
//! the settings say and writes `mcp.runtime` for `fasttail --mcp`, answers the requests
//! from the open streams (a search in steps over the frames, so the window never waits),
//! keeps the log of the last requests, and the AI assistants settings window.

use crate::config::FastTailConfig;
use crate::i18n::{t, Language};
use crate::mcp::{Host, McpRequest, McpServer, Outcome, Pending};
use crate::tail_engine::TailEngine;
use crate::theme::CyberTheme;
use egui::RichText;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Requests taken per frame at most.
const PER_FRAME: usize = 32;
/// Time the requests take on the window's thread per frame, at most.
const FRAME_BUDGET: Duration = Duration::from_millis(15);
/// Requests kept in the log.
const LOG_LEN: usize = 200;

/// A request of the log.
pub struct LogEntry {
    pub at: Instant,
    pub tool: String,
    pub args: String,
    /// Bytes of the response (0 while a search runs, or for a notification).
    pub bytes: usize,
}

/// The MCP server of the window and its settings window.
#[derive(Default)]
pub struct McpView {
    server: Option<McpServer>,
    /// What the running server was started with: port, token.
    running: Option<(u16, String)>,
    /// Why the server could not start (a port in use).
    pub error: Option<String>,
    /// The AI assistants settings window.
    pub open: bool,
    /// "Turn on" was ticked and waits for the warning to be confirmed.
    confirm_enable: bool,
    /// The search being answered, its request and its row in the log.
    current: Option<(McpRequest, Pending, u64)>,
    /// The last requests, oldest first, and the number of the next one.
    pub log: VecDeque<LogEntry>,
    logged: u64,
    /// When the requests of the last minute came, for the title-bar indicator.
    recent: VecDeque<Instant>,
    /// Where `mcp.runtime` goes (`mcp::runtime_path` when not set).
    pub runtime_path: Option<PathBuf>,
    /// The folders text being edited.
    roots_text: Option<String>,
}

impl Drop for McpView {
    fn drop(&mut self) {
        self.stop();
    }
}

impl McpView {
    fn stop(&mut self) {
        if self.server.take().is_some() {
            if let Some(path) = &self.runtime_path {
                let _ = std::fs::remove_file(path);
            }
        }
        self.running = None;
        self.current = None;
    }

    /// Starts, restarts or stops the server to match the settings.
    pub fn sync(
        &mut self,
        cfg: &mut FastTailConfig,
        wake: impl FnOnce() -> crate::tail_engine::WakeFn,
    ) {
        if !cfg.mcp_enabled {
            self.stop();
            self.error = None;
            return;
        }
        if cfg.mcp_token.is_empty() {
            cfg.mcp_token = crate::web::new_token();
            let _ = cfg.save();
        }
        let wanted = (cfg.mcp_port, cfg.mcp_token.clone());
        if self.running.as_ref() == Some(&wanted) {
            return;
        }
        self.stop();
        match McpServer::start(cfg.mcp_port, cfg.mcp_token.clone(), Some(wake())) {
            Ok(server) => {
                let path = self
                    .runtime_path
                    .get_or_insert_with(crate::mcp::runtime_path)
                    .clone();
                let _ = crate::mcp::write_runtime(&path, server.addr.port(), &cfg.mcp_token);
                self.server = Some(server);
                self.error = None;
            }
            Err(e) => self.error = Some(e.to_string()),
        }
        self.running = Some(wanted);
    }

    /// The port served on, `None` while the server is off.
    pub fn port(&self) -> Option<u16> {
        self.server.as_ref().map(|s| s.addr.port())
    }

    /// Requests in the last minute, `None` while the server is off.
    pub fn requests_last_minute(&mut self) -> Option<usize> {
        self.server.as_ref()?;
        let now = Instant::now();
        while self
            .recent
            .front()
            .is_some_and(|t| now.duration_since(*t) > Duration::from_secs(60))
        {
            self.recent.pop_front();
        }
        Some(self.recent.len())
    }

    /// Whether a search goes on (the window keeps drawing frames for it).
    pub fn busy(&self) -> bool {
        self.current.is_some()
    }

    fn log_request(&mut self, message: &serde_json::Value) -> u64 {
        let (tool, args) = crate::mcp::describe(message).unwrap_or_default();
        if self.log.len() >= LOG_LEN {
            self.log.pop_front();
        }
        self.log.push_back(LogEntry {
            at: Instant::now(),
            tool,
            args,
            bytes: 0,
        });
        self.recent.push_back(Instant::now());
        self.logged += 1;
        self.logged - 1
    }

    fn log_bytes(&mut self, number: u64, bytes: usize) {
        // The entry `number` is at its distance from the newest one.
        let back = (self.logged - 1 - number) as usize;
        if back < self.log.len() {
            let i = self.log.len() - 1 - back;
            self.log[i].bytes = bytes;
        }
    }

    fn reply(&mut self, request: &McpRequest, number: u64, answer: Option<serde_json::Value>) {
        if let Some(v) = &answer {
            self.log_bytes(number, v.to_string().len());
        }
        let _ = request.reply.send(answer);
    }

    /// Answers the requests waiting from `engines`; returns the files `open_file` asked
    /// for, for the window to open.
    pub fn serve(&mut self, engines: &mut [TailEngine], cfg: &FastTailConfig) -> Vec<PathBuf> {
        let mut opens = Vec::new();
        if self.server.is_none() {
            return opens;
        }
        let started = Instant::now();
        let mut taken = 0;
        loop {
            if let Some((request, mut pending, number)) = self.current.take() {
                let left = FRAME_BUDGET.saturating_sub(started.elapsed());
                match pending.step(engines, left) {
                    Some(v) => self.reply(&request, number, Some(v)),
                    None => {
                        self.current = Some((request, pending, number));
                        break;
                    }
                }
            }
            if taken >= PER_FRAME || started.elapsed() >= FRAME_BUDGET {
                break;
            }
            let Some(server) = self.server.as_ref() else {
                break;
            };
            let Ok(request) = server.requests.try_recv() else {
                break;
            };
            taken += 1;
            let number = self.log_request(&request.message);
            let mut host = Host {
                engines: &mut *engines,
                allow_write: cfg.mcp_allow_write,
                open_roots: &cfg.mcp_open_roots,
                lang: cfg.language,
                opens: &mut opens,
            };
            match crate::mcp::handle(&request.message, &mut host) {
                Outcome::Reply(v) => self.reply(&request, number, v),
                Outcome::Pending(p) => self.current = Some((request, p, number)),
            }
        }
        opens
    }

    /// What a client puts in its configuration to start `fasttail --mcp`.
    pub fn stdio_snippet() -> String {
        let exe = std::env::current_exe()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| "fasttail".into());
        serde_json::to_string_pretty(&serde_json::json!({
            "mcpServers": { "fasttail": { "command": exe, "args": ["--mcp"] } }
        }))
        .unwrap_or_default()
    }

    /// The same for a client that connects over HTTP.
    pub fn http_snippet(cfg: &FastTailConfig) -> String {
        serde_json::to_string_pretty(&serde_json::json!({
            "mcpServers": { "fasttail": {
                "type": "http",
                "url": format!("http://127.0.0.1:{}/mcp", cfg.mcp_port),
                "headers": { "Authorization": format!("Bearer {}", cfg.mcp_token) },
            } }
        }))
        .unwrap_or_default()
    }

    /// The AI assistants settings window; returns whether the settings changed.
    pub fn show_settings(
        &mut self,
        ctx: &egui::Context,
        cfg: &mut FastTailConfig,
        theme: &CyberTheme,
    ) -> bool {
        if !self.open {
            return false;
        }
        let lang = cfg.language;
        let mut changed = false;
        let mut open = self.open;
        egui::Window::new(
            RichText::new(format!("✨ {}", t(lang, "mcp_title")))
                .monospace()
                .color(theme.accent_color()),
        )
        .id(egui::Id::new("fasttail_mcp"))
        .open(&mut open)
        .resizable(true)
        .default_width(520.0)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                changed |= self.settings_body(ui, cfg, theme, lang);
            });
        });
        self.open = open;
        if !self.open {
            self.confirm_enable = false;
            self.roots_text = None;
        }
        changed
    }

    fn settings_body(
        &mut self,
        ui: &mut egui::Ui,
        cfg: &mut FastTailConfig,
        theme: &CyberTheme,
        lang: Language,
    ) -> bool {
        let mut changed = false;
        ui.label(RichText::new(t(lang, "mcp_intro")).color(theme.text_dim()));
        ui.add_space(4.0);
        let mut enabled = cfg.mcp_enabled;
        if ui.checkbox(&mut enabled, t(lang, "mcp_enable")).changed() {
            if enabled {
                self.confirm_enable = true;
            } else {
                cfg.mcp_enabled = false;
                changed = true;
            }
        }
        if self.confirm_enable {
            ui.label(RichText::new(t(lang, "mcp_warning")).color(theme.warn_color()));
            ui.horizontal(|ui| {
                if ui.button(t(lang, "mcp_confirm")).clicked() {
                    cfg.mcp_enabled = true;
                    self.confirm_enable = false;
                    changed = true;
                }
                if ui.button(t(lang, "session_cancel")).clicked() {
                    self.confirm_enable = false;
                }
            });
        }
        ui.horizontal(|ui| {
            ui.label(t(lang, "mcp_port"));
            changed |= ui
                .add(egui::DragValue::new(&mut cfg.mcp_port).range(1024..=65535))
                .changed();
        });
        ui.horizontal(|ui| {
            ui.label(t(lang, "mcp_token"));
            let shown = if cfg.mcp_token.len() > 12 {
                format!("{}…", &cfg.mcp_token[..12])
            } else {
                cfg.mcp_token.clone()
            };
            ui.label(RichText::new(shown).monospace());
            if ui
                .add_enabled(
                    !cfg.mcp_token.is_empty(),
                    egui::Button::new(t(lang, "mcp_copy_token")),
                )
                .on_disabled_hover_text(t(lang, "mcp_token_later"))
                .clicked()
            {
                ui.ctx().copy_text(cfg.mcp_token.clone());
            }
            if ui
                .button(t(lang, "mcp_regenerate"))
                .on_hover_text(t(lang, "mcp_regenerate_tip"))
                .clicked()
            {
                cfg.mcp_token = crate::web::new_token();
                changed = true;
            }
        });
        changed |= ui
            .checkbox(&mut cfg.mcp_allow_write, t(lang, "mcp_allow_write"))
            .on_hover_text(t(lang, "mcp_allow_write_tip"))
            .changed();
        ui.label(t(lang, "mcp_roots"));
        let text = self.roots_text.get_or_insert_with(|| {
            cfg.mcp_open_roots
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("; ")
        });
        let edit = ui.add_enabled(
            cfg.mcp_allow_write,
            egui::TextEdit::singleline(text).desired_width(f32::INFINITY),
        );
        let edit = edit.on_disabled_hover_text(t(lang, "mcp_roots_off"));
        if edit.changed() {
            cfg.mcp_open_roots = text
                .split(';')
                .map(str::trim)
                .filter(|r| !r.is_empty())
                .map(PathBuf::from)
                .collect();
            changed = true;
        }
        match (&self.error, self.port()) {
            (Some(e), _) => {
                ui.label(
                    RichText::new(t(lang, "mcp_failed").replace("{0}", e))
                        .color(theme.warn_color()),
                );
            }
            (None, Some(port)) => {
                ui.label(
                    RichText::new(
                        t(lang, "mcp_listening").replace("{0}", &format!("127.0.0.1:{port}")),
                    )
                    .color(theme.text_dim()),
                );
            }
            (None, None) => {}
        }
        ui.separator();
        for (label, snippet) in [
            ("mcp_snippet_stdio", Self::stdio_snippet()),
            ("mcp_snippet_http", Self::http_snippet(cfg)),
        ] {
            ui.horizontal(|ui| {
                ui.label(t(lang, label));
                if ui.small_button(t(lang, "mcp_copy")).clicked() {
                    ui.ctx().copy_text(snippet.clone());
                }
            });
            let mut shown = snippet.as_str();
            ui.add(
                egui::TextEdit::multiline(&mut shown)
                    .code_editor()
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            );
        }
        ui.separator();
        ui.label(RichText::new(t(lang, "mcp_log")).strong());
        if self.log.is_empty() {
            ui.label(RichText::new(t(lang, "mcp_log_empty")).color(theme.text_dim()));
        }
        egui::ScrollArea::vertical()
            .id_salt("mcp_log")
            .max_height(180.0)
            .show(ui, |ui| {
                let now = Instant::now();
                for entry in self.log.iter().rev() {
                    let ago = now.duration_since(entry.at).as_secs();
                    ui.label(
                        RichText::new(format!(
                            "{:>6}  {}  {}  {} B",
                            t(lang, "mcp_ago").replace("{0}", &ago.to_string()),
                            entry.tool,
                            entry.args,
                            entry.bytes
                        ))
                        .monospace()
                        .size(11.0),
                    );
                }
            });
        changed
    }
}
