// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use fasttail::config::FastTailConfig;
use fasttail::net_listener::{Limits, ListenSpec};
use fasttail::tail_engine::TailEngine;
use fasttail::workspace::{open_target, OpenOutcome};
use std::io::Write;
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::path::Path;
use std::time::{Duration, Instant};

/// A port free for TCP and UDP right now.
fn free_port() -> u16 {
    loop {
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        if UdpSocket::bind(("127.0.0.1", port)).is_ok() {
            return port;
        }
    }
}

fn open(url: &str, limits: Limits, dir: &Path) -> TailEngine {
    let spec = ListenSpec::parse(Path::new(url)).unwrap().unwrap();
    let spool = fasttail::stdin_source::Settings::from_config(Some(&dir.join("spool")), 2048);
    fasttail::net_listener::open_engine(spec, limits, &spool, None)
        .unwrap_or_else(|e| panic!("{url}: {e:?}"))
}

fn poll_until(engine: &mut TailEngine, what: &str, cond: impl Fn(&TailEngine) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        engine.poll_updates();
        if cond(engine) {
            return;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn lines(engine: &TailEngine) -> Vec<String> {
    (0..engine.total_lines())
        .filter_map(|i| engine.get_line(i).map(|l| l.into_owned()))
        .collect()
}

#[test]
fn tcp_lines_from_three_clients_are_never_mixed() {
    let dir = tempfile::tempdir().unwrap();
    let port = free_port();
    let mut engine = open(
        &format!("tcp://127.0.0.1:{port}"),
        Limits::default(),
        dir.path(),
    );
    assert_eq!(
        fasttail::find_all::stream_name(&engine),
        format!("tcp :{port}")
    );
    let senders: Vec<_> = (0..3)
        .map(|c| {
            std::thread::spawn(move || {
                let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
                for i in 0..200 {
                    // Written in two pieces, so a line arrives in parts.
                    s.write_all(format!("client {c} ").as_bytes()).unwrap();
                    s.write_all(format!("line {i}\n").as_bytes()).unwrap();
                }
            })
        })
        .collect();
    for s in senders {
        s.join().unwrap();
    }
    poll_until(&mut engine, "600 lines", |e| e.total_lines() >= 600);
    let all = lines(&engine);
    assert_eq!(all.len(), 600);
    for line in &all {
        let rest = line
            .strip_prefix("client ")
            .unwrap_or_else(|| panic!("{line:?}"));
        assert!(
            rest.len() > 7 && rest[1..].starts_with(" line "),
            "{line:?}"
        );
    }
}

#[test]
fn udp_datagrams_carry_the_sender_and_syslog_is_rendered() {
    let dir = tempfile::tempdir().unwrap();
    let port = free_port();
    let mut engine = open(
        &format!("syslog://127.0.0.1:{port}"),
        Limits::default(),
        dir.path(),
    );
    let udp = UdpSocket::bind("127.0.0.1:0").unwrap();
    udp.send_to(
        b"<11>1 2026-10-23T08:15:30Z db1 postgres 77 - - disk full",
        ("127.0.0.1", port),
    )
    .unwrap();
    poll_until(&mut engine, "the datagram", |e| e.total_lines() >= 1);
    // Syslog over TCP with octet counting.
    let mut tcp = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let msg = "<30>1 2026-10-23T08:15:31Z web1 nginx - - - ok";
    tcp.write_all(format!("{} {msg}", msg.len()).as_bytes())
        .unwrap();
    poll_until(&mut engine, "the TCP message", |e| e.total_lines() >= 2);
    let all = lines(&engine);
    assert_eq!(
        all[0],
        "[127.0.0.1] 2026-10-23T08:15:30Z ERROR db1 postgres[77]: disk full"
    );
    assert_eq!(
        all[1],
        "[127.0.0.1] 2026-10-23T08:15:31Z INFO web1 nginx: ok"
    );
}

#[test]
fn the_rate_limit_drops_and_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let port = free_port();
    let limits = Limits {
        lines_per_sec: 100,
        bytes_per_sec: 1 << 20,
    };
    let mut engine = open(&format!("tcp://127.0.0.1:{port}"), limits, dir.path());
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    for i in 0..500 {
        s.write_all(format!("burst {i}\n").as_bytes()).unwrap();
    }
    s.flush().unwrap();
    poll_until(&mut engine, "the dropped marker", |e| {
        lines(e)
            .iter()
            .any(|l| l.contains("messages dropped (rate limit)"))
    });
    let l = engine.listener.as_ref().unwrap();
    assert!(l.dropped() >= 300, "{}", l.dropped());
    assert!(
        lines(&engine)
            .iter()
            .filter(|l| l.starts_with("burst "))
            .count()
            <= 200
    );
}

#[test]
fn a_port_in_use_and_a_bad_url_are_reported() {
    let taken = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = taken.local_addr().unwrap().port();
    let config = FastTailConfig::default();
    match open_target(Path::new(&format!("tcp://127.0.0.1:{port}")), &config, None) {
        OpenOutcome::Failed { error, report } => {
            assert!(report);
            let text = error.system_text(fasttail::i18n::Language::En).unwrap();
            assert!(text.starts_with("Cannot listen on 127.0.0.1"), "{text}");
        }
        _ => panic!("expected a bind failure"),
    }
    match open_target(Path::new("udp://example.com:5140"), &config, None) {
        OpenOutcome::Failed { error, .. } => assert_eq!(
            error.system_text(fasttail::i18n::Language::En).as_deref(),
            Some("Invalid address: use an IP address or localhost")
        ),
        _ => panic!("expected a refusal"),
    }
}

#[test]
fn a_listener_opens_again_on_its_port_right_after_closing() {
    let dir = tempfile::tempdir().unwrap();
    let port = free_port();
    let url = format!("syslog://127.0.0.1:{port}");
    for _ in 0..3 {
        let engine = open(&url, Limits::default(), dir.path());
        drop(engine);
    }
}

/// Sends an HTTP request on `s` and returns the status code and the body.
fn http(s: &mut TcpStream, head: &str, body: &[u8]) -> (u16, Vec<u8>) {
    use std::io::Read;
    s.write_all(head.as_bytes()).unwrap();
    s.write_all(body).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let mut got = Vec::new();
    let mut byte = [0u8; 1];
    while !got.ends_with(b"\r\n\r\n") {
        s.read_exact(&mut byte).unwrap();
        got.push(byte[0]);
    }
    let head = String::from_utf8_lossy(&got).to_string();
    let status: u16 = head[9..12].parse().unwrap();
    let len: usize = head
        .lines()
        .find_map(|l| l.strip_prefix("Content-Length: "))
        .map_or(0, |v| v.trim().parse().unwrap());
    let mut body = vec![0u8; len];
    s.read_exact(&mut body).unwrap();
    (status, body)
}

fn post(path: &str, kind: &str, extra: &str, body: &[u8]) -> String {
    format!(
        "POST {path} HTTP/1.1\r\nHost: x\r\nContent-Type: {kind}\r\n{extra}Content-Length: {}\r\n\r\n",
        body.len()
    )
}

const OTLP_JSON: &str = r#"{"resourceLogs":[{"resource":{"attributes":[{"key":"service.name","value":{"stringValue":"checkout"}}]},"scopeLogs":[{"scope":{"name":"pay"},"logRecords":[{"timeUnixNano":"1761207330123000000","severityNumber":17,"body":{"stringValue":"payment failed"},"traceId":"0102030405060708090a0b0c0d0e0f10"}]}]}]}"#;

#[test]
fn otlp_http_json_and_gzipped_protobuf_on_one_connection() {
    let dir = tempfile::tempdir().unwrap();
    let port = free_port();
    let mut engine = open(
        &format!("otlp://127.0.0.1:{port}"),
        Limits::default(),
        dir.path(),
    );
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let body = OTLP_JSON.as_bytes();
    let (status, reply) = http(
        &mut s,
        &post("/v1/logs", "application/json", "", body),
        body,
    );
    assert_eq!((status, reply.as_slice()), (200, &b"{}"[..]));
    // A protobuf ExportLogsServiceRequest with one record, gzipped, on the same connection.
    let record: &[u8] = &[0x2a, 0x05, 0x0a, 0x03, b'p', b'b', b'!', 0x10, 0x09];
    let mut scope_logs = vec![0x12, record.len() as u8];
    scope_logs.extend_from_slice(record);
    let mut resource_logs = vec![0x12, scope_logs.len() as u8];
    resource_logs.extend_from_slice(&scope_logs);
    let mut request = vec![0x0a, resource_logs.len() as u8];
    request.extend_from_slice(&resource_logs);
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    gz.write_all(&request).unwrap();
    let gz = gz.finish().unwrap();
    let (status, reply) = http(
        &mut s,
        &post(
            "/v1/logs",
            "application/x-protobuf",
            "Content-Encoding: gzip\r\n",
            &gz,
        ),
        &gz,
    );
    assert_eq!((status, reply.len()), (200, 0));
    poll_until(&mut engine, "both records", |e| e.total_lines() >= 2);
    let all = lines(&engine);
    assert_eq!(
        all[0],
        "2025-10-23T08:15:30.123Z ERROR checkout [pay] payment failed trace=0102030405060708090a0b0c0d0e0f10"
    );
    assert!(all[1].ends_with(" INFO - pb!"), "{}", all[1]);
}

#[test]
fn otlp_refuses_what_it_does_not_take() {
    let dir = tempfile::tempdir().unwrap();
    let port = free_port();
    let limits = Limits {
        lines_per_sec: 100,
        bytes_per_sec: 1 << 20,
    };
    let _engine = open(&format!("otlp://127.0.0.1:{port}"), limits, dir.path());
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    assert_eq!(
        http(
            &mut s,
            &post("/v1/traces", "application/json", "", b"{}"),
            b"{}"
        )
        .0,
        404
    );
    assert_eq!(
        http(&mut s, &post("/v1/logs", "text/plain", "", b"x"), b"x").0,
        415
    );
    assert_eq!(
        http(&mut s, "GET /v1/logs HTTP/1.1\r\nHost: x\r\n\r\n", b"").0,
        405
    );
    // 500 records in one request: over the limit, so the next request is told to wait.
    let records: Vec<String> = (0..500)
        .map(|i| format!(r#"{{"body":{{"stringValue":"r{i}"}}}}"#))
        .collect();
    let many = format!(
        r#"{{"resourceLogs":[{{"scopeLogs":[{{"logRecords":[{}]}}]}}]}}"#,
        records.join(",")
    );
    assert_eq!(
        http(
            &mut s,
            &post("/v1/logs", "application/json", "", many.as_bytes()),
            many.as_bytes()
        )
        .0,
        200
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let body = OTLP_JSON.as_bytes();
        let status = http(
            &mut s,
            &post("/v1/logs", "application/json", "", body),
            body,
        )
        .0;
        if status == 429 {
            break;
        }
        assert!(Instant::now() < deadline, "never rate-limited");
        std::thread::sleep(Duration::from_millis(20));
    }
    // Too large: refused before the body is read, and the connection closes.
    let mut big = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let head = "POST /v1/logs HTTP/1.1\r\nContent-Type: application/json\r\nContent-Length: 99999999\r\n\r\n";
    assert_eq!(http(&mut big, head, b"").0, 413);
}
