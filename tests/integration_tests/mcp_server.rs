// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use super::write_lines;
use fasttail::config::FastTailConfig;
use fasttail::i18n::Language;
use fasttail::mcp::{self, Host, McpServer, Outcome};
use fasttail::tail_engine::TailEngine;
use fasttail::ui::mcp_view::McpView;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn open(path: &Path) -> TailEngine {
    let mut engine = TailEngine::open(path).unwrap();
    engine.size_check_interval = Duration::ZERO;
    super::wait_for_jobs(&mut engine);
    engine
}

/// 1000 lines: every 10th an ERROR, every 3rd a DEBUG, the rest INFO.
fn app_log(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    let lines: Vec<String> = (1..=1000)
        .map(|i| {
            let level = if i % 10 == 0 {
                "ERROR"
            } else if i % 3 == 0 {
                "DEBUG"
            } else {
                "INFO"
            };
            format!("2026-10-28 10:00:00 {level} step {i}")
        })
        .collect();
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    write_lines(&path, &refs);
    path
}

struct Fixture {
    engines: Vec<TailEngine>,
    allow_write: bool,
    roots: Vec<PathBuf>,
    opens: Vec<PathBuf>,
}

impl Fixture {
    fn new(engines: Vec<TailEngine>) -> Self {
        Self {
            engines,
            allow_write: false,
            roots: Vec::new(),
            opens: Vec::new(),
        }
    }

    fn send(&mut self, msg: Value) -> Option<Value> {
        let mut refs: Vec<&mut TailEngine> = self.engines.iter_mut().collect();
        let mut host = Host {
            engines: &mut refs,
            allow_write: self.allow_write,
            open_roots: &self.roots,
            lang: Language::En,
            opens: &mut self.opens,
        };
        mcp::handle_now(&msg, &mut host)
    }

    fn call(&mut self, name: &str, args: Value) -> Value {
        let v = self
            .send(json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":name,"arguments":args}}))
            .unwrap();
        v["result"].clone()
    }
}

fn lines_of(result: &Value, key: &str) -> Vec<u64> {
    result["structuredContent"][key]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["line"].as_u64().unwrap())
        .collect()
}

#[test]
fn handshake_lists_read_tools_and_negotiates_the_version() {
    let mut f = Fixture::new(Vec::new());
    // As Claude Desktop sends it.
    let init = f
        .send(json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"claude-ai","version":"0.1.0"}}}))
        .unwrap();
    assert_eq!(init["id"], 0);
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(init["result"]["serverInfo"]["name"], "fasttail");
    assert!(init["result"]["capabilities"]["tools"].is_object());
    // An older client gets its version, an unknown one the newest.
    let old = f
        .send(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}))
        .unwrap();
    assert_eq!(old["result"]["protocolVersion"], "2024-11-05");
    let odd = f
        .send(json!({"jsonrpc":"2.0","id":2,"method":"initialize","params":{"protocolVersion":"1999-01-01"}}))
        .unwrap();
    assert_eq!(odd["result"]["protocolVersion"], mcp::PROTOCOL_VERSIONS[0]);
    assert!(f
        .send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
        .is_none());
    assert_eq!(
        f.send(json!({"jsonrpc":"2.0","id":3,"method":"ping"}))
            .unwrap()["result"],
        json!({})
    );
    let names = |f: &mut Fixture| -> Vec<String> {
        f.send(json!({"jsonrpc":"2.0","id":4,"method":"tools/list"}))
            .unwrap()["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect()
    };
    let read = names(&mut f);
    assert_eq!(read.len(), 7);
    assert!(!read.iter().any(|n| mcp::WRITE_TOOLS.contains(&n.as_str())));
    f.allow_write = true;
    assert_eq!(names(&mut f).len(), 10);
    let unknown = f
        .send(json!({"jsonrpc":"2.0","id":5,"method":"sampling/createMessage"}))
        .unwrap();
    assert_eq!(unknown["error"]["code"], -32601);
    let batch = f
        .send(json!([{"jsonrpc":"2.0","id":6,"method":"ping"}]))
        .unwrap();
    assert_eq!(batch["error"]["code"], -32600);
}

#[test]
fn reads_follow_the_filters_and_context_gives_the_neighbours() {
    let dir = tempfile::tempdir().unwrap();
    let path = app_log(dir.path(), "app.log");
    let mut engine = open(&path);
    engine.set_exclude_filter("DEBUG");
    super::wait_for_jobs(&mut engine);
    let mut f = Fixture::new(vec![engine]);

    let streams = f.call("list_streams", json!({}));
    let s = &streams["structuredContent"]["streams"][0];
    assert_eq!(s["name"], "app.log");
    assert_eq!(s["lines"], 1000);
    assert_eq!(s["filtered"], true);

    // The last 100 visible lines: no DEBUG line.
    let last = f.call("read_lines", json!({"last": 100}));
    let rows = last["structuredContent"]["lines"].as_array().unwrap();
    assert_eq!(rows.len(), 100);
    assert!(rows
        .iter()
        .all(|r| !r["text"].as_str().unwrap().contains("DEBUG")));
    assert_eq!(rows.last().unwrap()["line"], 1000);
    assert_eq!(rows.last().unwrap()["level"], "ERROR");
    // Every line with scope all; from and count, and where to go on.
    let all = f.call("read_lines", json!({"scope":"all","from":3,"count":2}));
    assert_eq!(lines_of(&all, "lines"), vec![3, 4]);
    assert_eq!(all["structuredContent"]["next_line"], 5);
    let visible = f.call("read_lines", json!({"from":3,"count":2}));
    assert_eq!(lines_of(&visible, "lines"), vec![4, 5]);
    // The limit: 2000 lines at most, asked for more.
    let many = f.call("read_lines", json!({"scope":"all","from":1,"count":5000}));
    assert_eq!(lines_of(&many, "lines").len(), 1000);

    let ctx = f.call(
        "get_context",
        json!({"stream":"app.log","line":812,"before":5,"after":5}),
    );
    assert_eq!(lines_of(&ctx, "lines"), (807..=817).collect::<Vec<u64>>());
    let target: Vec<&Value> = ctx["structuredContent"]["lines"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|l| l["target"] == true)
        .collect();
    assert_eq!(target[0]["line"], 812);
    let out = f.call("get_context", json!({"line": 5000}));
    assert_eq!(out["isError"], true);

    let filters = f.call("get_filters", json!({}));
    assert_eq!(filters["structuredContent"]["exclude"], json!(["DEBUG"]));
    let levels = f.call("level_summary", json!({}));
    assert_eq!(levels["structuredContent"]["levels"]["ERROR"], 100);
}

#[test]
fn search_counts_everything_and_returns_at_most_the_cap() {
    let dir = tempfile::tempdir().unwrap();
    let a = app_log(dir.path(), "a.log");
    let b = app_log(dir.path(), "b.log");
    let mut f = Fixture::new(vec![open(&a), open(&b)]);

    let one = f.call("search", json!({"stream":"a.log","query":"error"}));
    let data = &one["structuredContent"];
    assert_eq!(
        (data["total"].as_u64(), data["truncated"].as_bool()),
        (Some(100), Some(false))
    );
    assert_eq!(data["hits"][0]["line"], 10);
    // Both streams, past the hit cap: the total is still counted.
    let both = f.call("search", json!({"query":"step"}));
    let data = &both["structuredContent"];
    assert_eq!(data["total"], 2000);
    assert_eq!(data["hits"].as_array().unwrap().len(), mcp::MAX_HITS);
    assert_eq!(data["truncated"], true);
    assert!(data["hits"][0]["stream"]
        .as_str()
        .unwrap()
        .ends_with("a.log"));
    // A regular expression, case-sensitive.
    let re = f.call(
        "search",
        json!({"stream":"b.log","query":"step 99\\d$","regex":true,"case_sensitive":true}),
    );
    assert_eq!(lines_of(&re, "hits"), (990..=999).collect::<Vec<u64>>());
    let bad = f.call("search", json!({"query":"(","regex":true}));
    assert_eq!(bad["isError"], true);
    // Several streams and no name: the assistant is told to choose.
    let which = f.call("read_lines", json!({}));
    assert_eq!(which["isError"], true);

    // In steps, as the window runs it: the same answer.
    let mut refs: Vec<&mut TailEngine> = f.engines.iter_mut().collect();
    let mut host = Host {
        engines: &mut refs,
        allow_write: false,
        open_roots: &[],
        lang: Language::En,
        opens: &mut Vec::new(),
    };
    let msg = json!({"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"search","arguments":{"query":"error"}}});
    let Outcome::Pending(mut pending) = mcp::handle(&msg, &mut host) else {
        panic!("a search runs in steps");
    };
    let mut steps = 0;
    let answer = loop {
        steps += 1;
        let refs: Vec<&mut TailEngine> = f.engines.iter_mut().collect();
        if let Some(v) = pending.step(&refs, Duration::ZERO) {
            break v;
        }
    };
    assert!(steps > 1);
    assert_eq!(answer["id"], 9);
    assert_eq!(answer["result"]["structuredContent"]["total"], 200);
}

#[test]
fn write_tools_need_permission_and_stay_in_the_roots() {
    let dir = tempfile::tempdir().unwrap();
    let path = app_log(dir.path(), "app.log");
    let allowed = tempfile::tempdir().unwrap();
    let inside = app_log(allowed.path(), "inside.log");
    let mut f = Fixture::new(vec![open(&path)]);

    let refused = f.call("add_bookmark", json!({"line": 10}));
    assert_eq!(refused["isError"], true);
    assert!(refused["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("disabled"));
    assert!(f.engines[0].bookmarks.is_empty());

    f.allow_write = true;
    let added = f.call("add_bookmark", json!({"line": 10, "note": "first failure"}));
    assert_eq!(added["isError"], false);
    assert!(f.engines[0].bookmarks.contains(&9));
    assert_eq!(
        f.engines[0].view_notice.as_deref(),
        Some("Assistant: bookmark on line 10")
    );
    let marks = f.call("list_bookmarks", json!({}));
    let m = &marks["structuredContent"]["bookmarks"][0];
    assert_eq!(
        (m["line"].as_u64(), m["note"].as_str()),
        (Some(10), Some("first failure"))
    );

    f.call("set_search", json!({"query": "timeout"}));
    assert_eq!(f.engines[0].search_query, "timeout");

    // open_file: no roots, a path outside them, a path inside.
    let none = f.call("open_file", json!({"path": inside.to_string_lossy()}));
    assert_eq!(none["isError"], true);
    f.roots = vec![allowed.path().to_path_buf()];
    let outside = f.call("open_file", json!({"path": path.to_string_lossy()}));
    assert_eq!(outside["isError"], true);
    let sneaky = allowed
        .path()
        .join("..")
        .join(dir.path().file_name().unwrap())
        .join("app.log");
    assert_eq!(
        f.call("open_file", json!({"path": sneaky.to_string_lossy()}))["isError"],
        true
    );
    assert!(f.opens.is_empty());
    let ok = f.call("open_file", json!({"path": inside.to_string_lossy()}));
    assert_eq!(ok["isError"], false);
    assert_eq!(f.opens, vec![std::fs::canonicalize(&inside).unwrap()]);
}

#[test]
fn resources_are_the_open_streams() {
    let dir = tempfile::tempdir().unwrap();
    let path = app_log(dir.path(), "my app.log");
    let mut f = Fixture::new(vec![open(&path)]);
    let list = f
        .send(json!({"jsonrpc":"2.0","id":1,"method":"resources/list"}))
        .unwrap();
    let uri = list["result"]["resources"][0]["uri"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(uri.starts_with("fasttail://stream/") && uri.ends_with("my%20app.log"));
    let read = f
        .send(json!({"jsonrpc":"2.0","id":2,"method":"resources/read","params":{"uri":uri}}))
        .unwrap();
    let text = read["result"]["contents"][0]["text"].as_str().unwrap();
    let data: Value = serde_json::from_str(text).unwrap();
    assert_eq!(data["lines"].as_array().unwrap().len(), 1000);
}

/// Posts a raw request to `port`; the status code and the body.
fn http(port: u16, head: &str, body: &str) -> (u16, String) {
    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(
        s,
        "{head}\r\nHost: 127.0.0.1:{port}\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    let status = out.split_whitespace().nth(1).unwrap().parse().unwrap();
    let body = out
        .split_once("\r\n\r\n")
        .map(|(_, b)| b.to_string())
        .unwrap_or_default();
    (status, body)
}

/// A server answered from `engines` on a thread, as the window does.
fn serve_on_thread(engines: Vec<TailEngine>, token: &str) -> (u16, std::sync::mpsc::Sender<()>) {
    let server = McpServer::start(0, token.to_string(), None).unwrap();
    let port = server.addr.port();
    let (stop, stopped) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut engines = engines;
        while stopped.try_recv().is_err() {
            match server.requests.recv_timeout(Duration::from_millis(20)) {
                Ok(req) => {
                    let mut opens = Vec::new();
                    let mut refs: Vec<&mut TailEngine> = engines.iter_mut().collect();
                    let mut host = Host {
                        engines: &mut refs,
                        allow_write: false,
                        open_roots: &[],
                        lang: Language::En,
                        opens: &mut opens,
                    };
                    let _ = req.reply.send(mcp::handle_now(&req.message, &mut host));
                }
                Err(_) => continue,
            }
        }
    });
    (port, stop)
}

#[test]
fn http_needs_the_token_and_a_local_origin() {
    let dir = tempfile::tempdir().unwrap();
    let path = app_log(dir.path(), "app.log");
    let token = "0123456789abcdef0123456789abcdef";
    let (port, stop) = serve_on_thread(vec![open(&path)], token);
    let list = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list_streams","arguments":{}}}"#;

    let (status, _) = http(port, "POST /mcp HTTP/1.1", list);
    assert_eq!(status, 401);
    let (status, _) = http(
        port,
        "POST /mcp HTTP/1.1\r\nAuthorization: Bearer wrong",
        list,
    );
    assert_eq!(status, 401);
    let auth = format!("Authorization: Bearer {token}");
    let (status, _) = http(
        port,
        &format!("POST /mcp HTTP/1.1\r\n{auth}\r\nOrigin: http://evil.example"),
        list,
    );
    assert_eq!(status, 403);
    let (status, _) = http(port, &format!("GET /mcp HTTP/1.1\r\n{auth}"), "");
    assert_eq!(status, 405);
    let (status, body) = http(
        port,
        &format!("POST /mcp HTTP/1.1\r\n{auth}\r\nOrigin: http://localhost:6274"),
        list,
    );
    assert_eq!(status, 200);
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        v["result"]["structuredContent"]["streams"][0]["name"],
        "app.log"
    );
    let (status, _) = http(
        port,
        &format!("POST /mcp HTTP/1.1\r\n{auth}"),
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
    );
    assert_eq!(status, 202);

    // The stdio bridge through the same server.
    let input = format!(
        "{list}\n\n{}\n",
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#
    );
    let mut output = Vec::new();
    mcp::proxy(port, token, input.as_bytes(), &mut output).unwrap();
    let text = String::from_utf8(output).unwrap();
    assert_eq!(text.lines().count(), 1);
    let v: Value = serde_json::from_str(text.trim()).unwrap();
    assert_eq!(v["id"], 1);
    // A wrong token: the client gets an error for its request.
    let mut output = Vec::new();
    mcp::proxy(port, "nope", list.as_bytes(), &mut output).unwrap();
    let v: Value = serde_json::from_slice(&output).unwrap();
    assert!(v["error"]["message"].as_str().unwrap().contains("401"));
    let _ = stop.send(());
}

#[test]
fn bridge_without_a_window_serves_the_files() {
    let dir = tempfile::tempdir().unwrap();
    let path = app_log(dir.path(), "app.log");
    let mut engines = vec![TailEngine::open(&path).unwrap()];
    let input = concat!(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        "\n",
        "not json\n",
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"list_streams","arguments":{}}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"add_bookmark","arguments":{"line":1}}}"#,
        "\n",
    );
    let mut output = Vec::new();
    mcp::serve_headless(&mut engines, input.as_bytes(), &mut output).unwrap();
    let answers: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(answers.len(), 4);
    assert_eq!(answers[1]["error"]["code"], -32700);
    let s = &answers[2]["result"]["structuredContent"]["streams"][0];
    assert_eq!(
        (s["name"].as_str(), s["lines"].as_u64()),
        (Some("app.log"), Some(1000))
    );
    // Read-only: write tools are refused.
    assert_eq!(answers[3]["result"]["isError"], true);
}

#[test]
fn window_view_logs_requests_and_writes_the_runtime_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = app_log(dir.path(), "app.log");
    let mut engines = [open(&path)];
    let mut cfg = FastTailConfig {
        mcp_enabled: true,
        mcp_port: 0,
        mcp_token: "0123456789abcdef0123456789abcdef".into(),
        ..Default::default()
    };
    let runtime = dir.path().join("mcp.runtime");
    let mut view = McpView::default();
    view.runtime_path = Some(runtime.clone());
    view.sync(&mut cfg, || std::sync::Arc::new(|| {}));
    let port = view.port().expect("the server runs");
    assert_eq!(
        mcp::read_runtime(&runtime),
        Some((port, cfg.mcp_token.clone()))
    );
    assert_eq!(view.requests_last_minute(), Some(0));

    let token = cfg.mcp_token.clone();
    let client = std::thread::spawn(move || {
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"search","arguments":{"query":"error"}}}"#;
        (0..3)
            .map(|_| mcp::post(port, &token, body).unwrap())
            .collect::<Vec<_>>()
    });
    let started = Instant::now();
    while !client.is_finished() {
        let mut refs: Vec<&mut TailEngine> = engines.iter_mut().collect();
        let opens = view.serve(&mut refs, &cfg);
        assert!(opens.is_empty());
        assert!(started.elapsed() < Duration::from_secs(30));
        std::thread::sleep(Duration::from_millis(2));
    }
    for (status, body) in client.join().unwrap() {
        assert_eq!(status, 200);
        let v: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["result"]["structuredContent"]["total"], 100);
    }
    assert_eq!(view.requests_last_minute(), Some(3));
    assert_eq!(view.log.len(), 3);
    assert!(view.log.iter().all(|e| e.tool == "search" && e.bytes > 0));
    assert!(view.log[0].args.contains("error"));

    // Off: the port is freed and the runtime file removed.
    cfg.mcp_enabled = false;
    view.sync(&mut cfg, || std::sync::Arc::new(|| {}));
    assert!(view.port().is_none() && !runtime.exists());
    assert!(std::net::TcpStream::connect(("127.0.0.1", port)).is_err());
}

#[test]
fn reading_from_a_line_ignores_collapsed_groups() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("retry.log");
    let mut lines: Vec<String> = (0..10).map(|i| format!("start {i}")).collect();
    lines.extend((0..500).map(|_| "retry".to_string()));
    lines.push("done".into());
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    write_lines(&path, &refs);
    let mut engine = open(&path);
    engine.set_collapse_mode(fasttail::collapse::CollapseMode::Exact);
    super::wait_for_jobs(&mut engine);
    let mut f = Fixture::new(vec![engine]);
    let from = f.call("read_lines", json!({"from": 511, "count": 1}));
    assert_eq!(lines_of(&from, "lines"), vec![511]);
    let from = f.call("read_lines", json!({"from": 12, "count": 2}));
    assert_eq!(lines_of(&from, "lines"), vec![12, 13]);
}

#[test]
fn open_file_judges_the_path_as_written_first() {
    let allowed = tempfile::tempdir().unwrap();
    let roots = vec![allowed.path().to_path_buf()];
    // Elsewhere: refused before the file system is asked (no "does not exist").
    let far = if cfg!(windows) {
        PathBuf::from(r"\\attacker.example\share\x.log")
    } else {
        PathBuf::from("/nowhere/at/all.log")
    };
    let e = mcp::allowed_file(&far, &roots).unwrap_err();
    assert!(e.contains("outside"), "{e}");
    let e = mcp::allowed_file(Path::new("relative.log"), &roots).unwrap_err();
    assert!(e.contains("absolute"), "{e}");
    let missing = allowed.path().join("missing.log");
    let e = mcp::allowed_file(&missing, &roots).unwrap_err();
    assert!(e.contains("does not exist"), "{e}");
}

#[test]
fn batches_get_one_array_and_the_bridge_knows_the_window() {
    let token = "0123456789abcdef0123456789abcdef";
    let (port, stop) = serve_on_thread(Vec::new(), token);
    assert!(mcp::is_window_server(port));
    let other = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let other_port = other.local_addr().unwrap().port();
    std::thread::spawn(move || {
        if let Ok((mut s, _)) = other.accept() {
            let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
        }
    });
    assert!(!mcp::is_window_server(other_port));

    let batch = r#"[{"jsonrpc":"2.0","id":1,"method":"ping"},{"jsonrpc":"2.0","method":"notifications/initialized"},{"jsonrpc":"2.0","id":2,"method":"ping"}]"#;
    let (status, body) = mcp::post(port, token, batch).unwrap();
    assert_eq!(status, 200);
    let v: Value = serde_json::from_str(&body).unwrap();
    let ids: Vec<u64> = v
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["id"].as_u64().unwrap())
        .collect();
    assert_eq!(ids, vec![1, 2]);
    let (status, _) = mcp::post(port, token, "[]").unwrap();
    assert_eq!(status, 400);
    // An explicit null stream is no stream.
    let null = r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"list_bookmarks","arguments":{"stream":null}}}"#;
    let (_, body) = mcp::post(port, token, null).unwrap();
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["result"]["isError"], false);
    let _ = stop.send(());
}
