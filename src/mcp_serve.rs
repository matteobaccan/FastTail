// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The interface's side of the MCP server (the window's and the terminal's) (`crate::mcp`): starts and stops the server as
//! the settings say and writes `mcp.runtime` for `fasttail --mcp`, answers the requests
//! from the open streams (a search in steps over the frames, so the window never waits),
//! keeps the log of the last requests, and the AI assistants settings window.

use crate::config::FastTailConfig;
use crate::mcp::{Host, McpRequest, McpServer, Outcome, Pending};
use crate::tail_engine::TailEngine;
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
    #[cfg_attr(not(feature = "gui"), allow(dead_code))]
    pub(crate) confirm_enable: bool,
    /// The searches being answered, oldest first, with their requests and log rows.
    current: VecDeque<(McpRequest, Pending, u64)>,
    /// The last requests, oldest first, and the number of the next one.
    pub log: VecDeque<LogEntry>,
    logged: u64,
    /// When the requests of the last minute came, for the title-bar indicator.
    recent: VecDeque<Instant>,
    /// Where `mcp.runtime` goes (`mcp::runtime_path` when not set).
    pub runtime_path: Option<PathBuf>,
    /// The folders text being edited.
    #[cfg_attr(not(feature = "gui"), allow(dead_code))]
    pub(crate) roots_text: Option<String>,
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
        self.current.clear();
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
        !self.current.is_empty()
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
    pub fn serve(&mut self, engines: &mut [&mut TailEngine], cfg: &FastTailConfig) -> Vec<PathBuf> {
        let mut opens = Vec::new();
        if self.server.is_none() {
            return opens;
        }
        let started = Instant::now();
        // The oldest search gets half of the frame; the other requests are taken all the
        // same, so a long search never holds up a ping or a read.
        if let Some((request, mut pending, number)) = self.current.pop_front() {
            match pending.step(engines, FRAME_BUDGET / 2) {
                Some(v) => self.reply(&request, number, Some(v)),
                None => self.current.push_back((request, pending, number)),
            }
        }
        let mut taken = 0;
        while taken < PER_FRAME && started.elapsed() < FRAME_BUDGET {
            let Some(server) = self.server.as_ref() else {
                break;
            };
            let Ok(request) = server.requests.try_recv() else {
                break;
            };
            taken += 1;
            let number = self.log_request(&request.message);
            // `notifications/cancelled`: the search it names is dropped.
            if request.message.get("method").and_then(|m| m.as_str())
                == Some("notifications/cancelled")
            {
                let id = &request.message["params"]["requestId"];
                self.current.retain(|(_, p, _)| p.id() != id);
            }
            let mut host = Host {
                engines: &mut *engines,
                allow_write: cfg.mcp_allow_write,
                open_roots: &cfg.mcp_open_roots,
                lang: cfg.language,
                opens: &mut opens,
            };
            match crate::mcp::handle(&request.message, &mut host) {
                Outcome::Reply(v) => self.reply(&request, number, v),
                Outcome::Pending(p) => self.current.push_back((request, p, number)),
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
}
