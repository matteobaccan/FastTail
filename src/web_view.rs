// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The window's side of the web view (`crate::web`): starts and stops the server as the
//! settings say, answers the page's requests from the open streams (the rows they show,
//! drawn in the colours of the HTML export), and the Web view settings window.

use crate::config::FastTailConfig;
use crate::i18n::{t, Language};
use crate::tail_engine::TailEngine;
use crate::theme::CyberTheme;
use crate::web::{WebQuery, WebServer, PAGE_LINES};
use serde_json::{json, Value};
use std::collections::HashMap;

/// Requests answered per frame at most (the rest wait for the next one).
const PER_FRAME: usize = 32;
/// Rows a search request looks at, at most.
const SEARCH_ROWS: usize = 200_000;
/// Time a search request takes on the window's thread, at most.
const SEARCH_BUDGET: std::time::Duration = std::time::Duration::from_millis(30);

/// Whether `hay` contains `needle_lower` (lower case) without case: ASCII byte by byte
/// without allocating, else through `to_lowercase`.
fn contains_ignore_case(hay: &str, needle_lower: &str) -> bool {
    if !needle_lower.is_ascii() {
        return hay.to_lowercase().contains(needle_lower);
    }
    let (h, n) = (hay.as_bytes(), needle_lower.as_bytes());
    n.is_empty()
        || n.len() <= h.len()
            && h.windows(n.len())
                .any(|w| w.iter().zip(n).all(|(a, b)| a.to_ascii_lowercase() == *b))
}
/// Bytes of a row sent at most.
const MAX_ROW_BYTES: usize = 64 * 1024;

/// The page texts, in the interface language.
const PAGE_TEXTS: [&str; 9] = [
    "web_pause",
    "web_follow",
    "web_search",
    "web_earlier",
    "web_two_panes",
    "web_gone",
    "web_no_match",
    "web_no_streams",
    "web_read_only",
];

/// The web view: the running server and the CSS classes handed out so far.
#[derive(Default)]
pub struct WebView {
    pub(crate) server: Option<WebServer>,
    /// What the running server was started with: port, allow remote, token.
    running: Option<(u16, bool, String)>,
    /// Why the server could not start (a port in use).
    pub error: Option<String>,
    classes: HashMap<String, usize>,
    order: Vec<String>,
    /// The Web view settings window.
    pub open: bool,
    /// "Allow other computers" was ticked and waits for a confirmation.
    #[cfg_attr(not(feature = "gui"), allow(dead_code))]
    pub(crate) confirm_remote: bool,
}

/// The id of a stream in the page: its path.
fn stream_id(engine: &TailEngine) -> String {
    engine.path.to_string_lossy().into_owned()
}

/// Changes whenever the rows a stream shows are no longer the same rows: a reload or a
/// truncation, another filter, time window, set of hidden lines, parser or context view.
/// Appended lines do not change it.
fn generation(engine: &TailEngine) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    engine.reload_generation.hash(&mut h);
    engine.parser_generation.hash(&mut h);
    format!("{:?}", engine.filter_spec()).hash(&mut h);
    engine.time_window().hash(&mut h);
    engine.hidden_ranges().hash(&mut h);
    engine.context_line().hash(&mut h);
    // 53 bits: the page's JavaScript numbers hold them exactly.
    h.finish() & ((1 << 53) - 1)
}

impl WebView {
    /// Starts, restarts or stops the server to match the settings.
    pub fn sync(
        &mut self,
        cfg: &mut FastTailConfig,
        wake: impl FnOnce() -> crate::tail_engine::WakeFn,
    ) {
        if !cfg.web_enabled {
            self.server = None;
            self.running = None;
            self.error = None;
            return;
        }
        if cfg.web_token.is_empty() {
            cfg.web_token = crate::web::new_token();
            let _ = cfg.save();
        }
        let wanted = (cfg.web_port, cfg.web_allow_remote, cfg.web_token.clone());
        if self.running.as_ref() == Some(&wanted) {
            return;
        }
        self.server = None;
        let addr = crate::web::bind_address(cfg.web_port, cfg.web_allow_remote);
        match WebServer::start(addr, cfg.web_token.clone(), Some(wake())) {
            Ok(server) => {
                self.server = Some(server);
                self.error = None;
            }
            Err(e) => self.error = Some(e.to_string()),
        }
        self.running = Some(wanted);
    }

    /// Browsers connected, `None` while the web view is off.
    pub fn connections(&self) -> Option<usize> {
        self.server.as_ref().map(|s| s.connections())
    }

    /// The address of the page with its token.
    pub fn link(cfg: &FastTailConfig) -> String {
        format!("http://127.0.0.1:{}/?token={}", cfg.web_port, cfg.web_token)
    }

    /// Answers the requests waiting, from `engines`.
    pub fn serve(
        &mut self,
        engines: &[&TailEngine],
        theme: &CyberTheme,
        level_colors: bool,
        lang: Language,
    ) {
        let Some(server) = self.server.as_ref() else {
            return;
        };
        // What each stream shows, for the event streams (only while a browser is here).
        if server.connections() > 0 {
            server.publish(
                engines
                    .iter()
                    .map(|e| (stream_id(e), (e.visible_lines(), generation(e))))
                    .collect(),
            );
        }
        let mut pending = Vec::new();
        while pending.len() < PER_FRAME {
            match server.requests.try_recv() {
                Ok(r) => pending.push(r),
                Err(_) => break,
            }
        }
        for req in pending {
            let answer = self.answer(&req.query, engines, theme, level_colors, lang);
            let _ = req.reply.send(answer.map(|v| v.to_string()));
        }
    }

    /// The answer to `query` (`None` when its stream is not open).
    pub fn answer(
        &mut self,
        query: &WebQuery,
        engines: &[&TailEngine],
        theme: &CyberTheme,
        level_colors: bool,
        lang: Language,
    ) -> Option<Value> {
        let find = |id: &str| engines.iter().find(|e| stream_id(e) == id);
        match query {
            WebQuery::Streams => {
                let streams: Vec<Value> = engines
                    .iter()
                    .map(|e| {
                        json!({
                            "id": stream_id(e),
                            "name": crate::find_all::stream_name(e),
                            "lines": e.visible_lines(),
                        })
                    })
                    .collect();
                let texts: serde_json::Map<String, Value> = PAGE_TEXTS
                    .iter()
                    .map(|k| (k.to_string(), Value::from(t(lang, k))))
                    .chain(std::iter::once((
                        "web_streams".to_string(),
                        Value::from(t(lang, "web_streams")),
                    )))
                    .collect();
                let var = |name: &str, c| {
                    crate::export_html::hex(c)
                        .map_or_else(String::new, |h| format!("--{name}:{h};"))
                };
                let vars = format!(
                    "{}{}{}{}",
                    var("bg", theme.bg_color()),
                    var("fg", theme.text_primary()),
                    var("dim", theme.text_dim()),
                    var("accent", theme.accent_color())
                );
                Some(json!({ "streams": streams, "texts": texts, "theme": vars }))
            }
            WebQuery::Lines {
                stream,
                from,
                count,
            } => {
                let e = find(stream)?;
                let total = e.visible_lines();
                let count = (*count).min(PAGE_LINES);
                let first = from
                    .unwrap_or_else(|| total.saturating_sub(count))
                    .min(total);
                let last = (first + count).min(total);
                let (rows, css) = self.rows(e, first, last, theme, level_colors);
                Some(json!({
                    "rows": rows,
                    "css": css,
                    "first": first,
                    "next": last,
                    "total": total,
                    "gen": generation(e),
                }))
            }
            WebQuery::Tail { stream, after, gen } => {
                let e = find(stream)?;
                let total = e.visible_lines();
                let now = generation(e);
                if now != *gen || *after > total {
                    return Some(json!({ "reset": true, "rows": [], "next": total, "gen": now }));
                }
                let last = (after + PAGE_LINES).min(total);
                let (rows, css) = self.rows(e, *after, last, theme, level_colors);
                Some(json!({
                    "rows": rows,
                    "css": css,
                    "first": after,
                    "next": last,
                    "gen": now,
                }))
            }
            WebQuery::Search {
                stream,
                row,
                forward,
                query,
            } => {
                let e = find(stream)?;
                let total = e.visible_lines();
                let needle = query.trim().to_lowercase();
                if needle.is_empty() {
                    return Some(json!({ "row": null }));
                }
                let hit = |pos: usize| {
                    e.line_at(pos)
                        .and_then(|idx| e.get_line(idx))
                        .is_some_and(|l| contains_ignore_case(&l, &needle))
                };
                // A bounded look on the window's thread: past SEARCH_ROWS rows or
                // SEARCH_BUDGET, the page asks again from where this one stopped (`more`).
                let started = std::time::Instant::now();
                let out_of_time = |n: usize| {
                    n >= SEARCH_ROWS || (n % 1024 == 1023 && started.elapsed() >= SEARCH_BUDGET)
                };
                if *forward {
                    let start = row.saturating_add(1).max(0) as usize;
                    for (n, p) in (start..total).enumerate() {
                        if hit(p) {
                            return Some(json!({ "row": p }));
                        }
                        if out_of_time(n) {
                            return Some(json!({ "row": null, "more": p }));
                        }
                    }
                } else {
                    let end = (*row).clamp(0, total as i64) as usize;
                    for (n, p) in (0..end).rev().enumerate() {
                        if hit(p) {
                            return Some(json!({ "row": p }));
                        }
                        if out_of_time(n) {
                            return Some(json!({ "row": null, "more": p }));
                        }
                    }
                }
                Some(json!({ "row": null }))
            }
        }
    }

    /// Rows `first..last` of `e` with the CSS of the classes they use.
    fn rows(
        &mut self,
        e: &TailEngine,
        first: usize,
        last: usize,
        theme: &CyberTheme,
        level_colors: bool,
    ) -> (Vec<Value>, Value) {
        let query = e.last_searched_query.to_lowercase();
        let mut used: Vec<usize> = Vec::new();
        let mut rows = Vec::with_capacity(last.saturating_sub(first));
        for pos in first..last {
            let Some(idx) = e.line_at(pos) else {
                continue;
            };
            let Some(mut row) = e.get_row(idx) else {
                continue;
            };
            if row.line.len() > MAX_ROW_BYTES {
                let mut end = MAX_ROW_BYTES;
                while !row.line.is_char_boundary(end) {
                    end -= 1;
                }
                row.line.truncate(end);
                row.line.push('…');
            }
            let (classes, order) = (&mut self.classes, &mut self.order);
            let mut class = |css: String| -> usize {
                let c = *classes.entry(css.clone()).or_insert_with(|| {
                    order.push(css);
                    order.len() - 1
                });
                if !used.contains(&c) {
                    used.push(c);
                }
                c
            };
            let (row_class, runs) =
                crate::export_html::row_runs(e, theme, level_colors, &query, idx, &row, &mut class);
            let runs: Vec<Value> = runs.into_iter().map(|(text, c)| json!([text, c])).collect();
            rows.push(json!({
                "r": pos,
                "n": e.shown_line_number(idx),
                "b": e.is_bookmarked(idx),
                "c": row_class,
                "t": runs,
            }));
        }
        let css: serde_json::Map<String, Value> = used
            .iter()
            .map(|&c| (format!("c{c}"), Value::from(self.order[c].clone())))
            .collect();
        (rows, Value::Object(css))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_ignores_case_without_allocating_for_ascii() {
        assert!(contains_ignore_case("GET /Orders 500", "orders 5"));
        assert!(!contains_ignore_case("GET /", "get /x"));
        assert!(contains_ignore_case("Größe ÄNDERN", "änd"));
        assert!(contains_ignore_case("abc", ""));
    }
}
