// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use super::write_lines;
use fasttail::config::FastTailConfig;
use fasttail::i18n::Language;
use fasttail::tail_engine::TailEngine;
use fasttail::theme::CyberTheme;
use fasttail::ui::web_view::WebView;
use fasttail::web::WebQuery;
use serde_json::Value;
use std::io::{Read, Write};
use std::path::Path;
use std::time::{Duration, Instant};

fn open(path: &Path) -> TailEngine {
    let mut engine = TailEngine::open(path).unwrap();
    engine.size_check_interval = Duration::ZERO;
    super::wait_for_jobs(&mut engine);
    engine
}

fn ask(view: &mut WebView, engines: &[TailEngine], q: WebQuery) -> Option<Value> {
    view.answer(&q, engines, &CyberTheme::Tron, true, Language::En)
}

fn lines(numbers: std::ops::Range<usize>) -> Vec<String> {
    numbers
        .map(|i| {
            if i % 10 == 0 {
                format!("2026-10-28 10:00:00 ERROR step {i} <script>alert(1)</script>")
            } else {
                format!("2026-10-28 10:00:00 INFO step {i}")
            }
        })
        .collect()
}

fn text_of(row: &Value) -> String {
    row["t"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r[0].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn pages_tails_resets_and_text_stays_text() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app.log");
    let body = lines(0..1200);
    let refs: Vec<&str> = body.iter().map(String::as_str).collect();
    write_lines(&path, &refs);
    let mut engines = vec![open(&path)];
    let id = path.to_string_lossy().into_owned();
    let mut view = WebView::default();

    let streams = ask(&mut view, &engines, WebQuery::Streams).unwrap();
    assert_eq!(streams["streams"][0]["id"], id.as_str());
    assert_eq!(streams["streams"][0]["lines"], 1200);
    assert_eq!(streams["texts"]["web_pause"], "Pause");
    assert!(streams["theme"].as_str().unwrap().contains("--bg:#"));

    // The tail: the last 500 rows.
    let tail = ask(
        &mut view,
        &engines,
        WebQuery::Lines {
            stream: id.clone(),
            from: None,
            count: 9999,
        },
    )
    .unwrap();
    let rows = tail["rows"].as_array().unwrap();
    assert_eq!(
        (rows.len(), tail["first"].as_u64(), tail["next"].as_u64()),
        (500, Some(700), Some(1200))
    );
    assert_eq!(rows[0]["n"], 701);
    // An ERROR row has its level class, whose CSS comes with the page; markup is text.
    let error = rows.iter().find(|r| r["n"] == 711).unwrap();
    assert!(error["c"].is_u64());
    assert!(tail["css"][format!("c{}", error["c"])].is_string());
    assert!(text_of(error).ends_with("<script>alert(1)</script>"));

    // Earlier rows, from 0.
    let page = ask(
        &mut view,
        &engines,
        WebQuery::Lines {
            stream: id.clone(),
            from: Some(0),
            count: 3,
        },
    )
    .unwrap();
    assert_eq!(page["rows"].as_array().unwrap().len(), 3);
    assert_eq!(page["next"], 3);
    let gen = page["gen"].as_u64().unwrap();

    // Nothing new; then two appended lines.
    let none = ask(
        &mut view,
        &engines,
        WebQuery::Tail {
            stream: id.clone(),
            after: 1200,
            gen,
        },
    )
    .unwrap();
    assert!(none["rows"].as_array().unwrap().is_empty() && none["reset"].is_null());
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    writeln!(f, "2026-10-28 10:00:01 WARN appended a").unwrap();
    writeln!(f, "2026-10-28 10:00:01 INFO appended b").unwrap();
    drop(f);
    let deadline = Instant::now() + Duration::from_secs(10);
    while engines[0].visible_lines() < 1202 {
        assert!(Instant::now() < deadline, "the append was not read");
        engines[0].poll_updates();
        std::thread::sleep(Duration::from_millis(5));
    }
    let more = ask(
        &mut view,
        &engines,
        WebQuery::Tail {
            stream: id.clone(),
            after: 1200,
            gen,
        },
    )
    .unwrap();
    let rows = more["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert!(text_of(&rows[1]).ends_with("appended b"));
    assert_eq!(more["next"], 1202);

    // Search, forward from the start and backward from the end.
    let hit = ask(
        &mut view,
        &engines,
        WebQuery::Search {
            stream: id.clone(),
            row: -1,
            forward: true,
            query: "APPENDED".into(),
        },
    )
    .unwrap();
    assert_eq!(hit["row"], 1200);
    let back = ask(
        &mut view,
        &engines,
        WebQuery::Search {
            stream: id.clone(),
            row: 1200,
            forward: false,
            query: "step 1199".into(),
        },
    )
    .unwrap();
    assert_eq!(back["row"], 1199);

    // A filter changes the rows: the page reloads.
    engines[0].set_include_filter("ERROR");
    super::wait_for_jobs(&mut engines[0]);
    let reset = ask(
        &mut view,
        &engines,
        WebQuery::Tail {
            stream: id.clone(),
            after: 1202,
            gen,
        },
    )
    .unwrap();
    assert_eq!(reset["reset"], true);
    let filtered = ask(
        &mut view,
        &engines,
        WebQuery::Lines {
            stream: id.clone(),
            from: None,
            count: 500,
        },
    )
    .unwrap();
    assert_eq!(filtered["total"], 120);
    let gen2 = filtered["gen"].as_u64().unwrap();
    assert_ne!(gen, gen2);

    // A truncation resets too.
    engines[0].set_include_filter("");
    super::wait_for_jobs(&mut engines[0]);
    let gen3 = ask(
        &mut view,
        &engines,
        WebQuery::Lines {
            stream: id.clone(),
            from: Some(0),
            count: 1,
        },
    )
    .unwrap()["gen"]
        .as_u64()
        .unwrap();
    std::fs::write(&path, "2026-10-28 10:00:02 INFO fresh\n").unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while engines[0].total_lines() != 1 {
        assert!(Instant::now() < deadline, "the truncation was not seen");
        engines[0].poll_updates();
        std::thread::sleep(Duration::from_millis(5));
    }
    let reset = ask(
        &mut view,
        &engines,
        WebQuery::Tail {
            stream: id.clone(),
            after: 1202,
            gen: gen3,
        },
    )
    .unwrap();
    assert_eq!(reset["reset"], true);

    // A stream that is not open.
    assert!(ask(
        &mut view,
        &engines,
        WebQuery::Lines {
            stream: "nope".into(),
            from: None,
            count: 1
        }
    )
    .is_none());
}

#[test]
fn the_event_stream_sends_appended_lines() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("live.log");
    write_lines(&path, &["2026-10-28 10:00:00 INFO start"]);
    let mut engines = vec![open(&path)];
    let id = path.to_string_lossy().into_owned();
    let mut cfg = FastTailConfig {
        web_enabled: true,
        web_port: 0,
        ..Default::default()
    };
    // Port 0 is refused by the settings; bind a free one first.
    let free = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    cfg.web_port = free.local_addr().unwrap().port();
    drop(free);
    let mut view = WebView::default();
    let wake: fasttail::tail_engine::WakeFn = std::sync::Arc::new(|| {});
    view.sync(&mut cfg, || wake.clone());
    assert!(view.error.is_none(), "{:?}", view.error);
    assert_eq!(
        cfg.web_token.len(),
        64,
        "a token is made on the first enable"
    );
    let gen = ask(
        &mut view,
        &engines,
        WebQuery::Lines {
            stream: id.clone(),
            from: Some(0),
            count: 1,
        },
    )
    .unwrap()["gen"]
        .as_u64()
        .unwrap();
    let port = cfg.web_port;
    let token = cfg.web_token.clone();
    let path_q = format!(
        "/api/events?s={}&after=1&gen={gen}",
        id.replace('/', "%2F")
            .replace('\\', "%5C")
            .replace(':', "%3A")
    );
    let client = std::thread::spawn(move || {
        let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(
            s,
            "GET {path_q} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nCookie: fasttail_web={token}\r\n\r\n"
        )
        .unwrap();
        s.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
        let mut got = String::new();
        let mut buf = [0u8; 4096];
        let until = Instant::now() + Duration::from_secs(15);
        while !got.contains("appended live") && Instant::now() < until {
            match s.read(&mut buf) {
                Ok(0) => break,
                Err(_) => continue,
                Ok(n) => got.push_str(&String::from_utf8_lossy(&buf[..n])),
            }
        }
        got
    });
    let mut appended = false;
    let deadline = Instant::now() + Duration::from_secs(20);
    while !client.is_finished() {
        assert!(Instant::now() < deadline, "no event arrived");
        if !appended && view.connections() == Some(1) {
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap();
            writeln!(f, "2026-10-28 10:00:01 INFO appended live").unwrap();
            appended = true;
        }
        engines[0].poll_updates();
        view.serve(&engines, &CyberTheme::Tron, true, Language::En);
        std::thread::sleep(Duration::from_millis(10));
    }
    let got = client.join().unwrap();
    assert!(
        got.starts_with("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream"),
        "{got}"
    );
    assert!(got.contains("event: lines\ndata: "), "{got}");
    assert!(got.contains("appended live"));
    // Off again: the server stops.
    cfg.web_enabled = false;
    view.sync(&mut cfg, || wake.clone());
    assert_eq!(view.connections(), None);
}
