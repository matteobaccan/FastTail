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
