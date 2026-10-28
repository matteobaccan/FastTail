// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The web view server: a small read-only HTTP/1.1 server on `127.0.0.1` (or every
//! interface when allowed) that serves one embedded page and a JSON API over the open
//! streams. The streams live on the window's thread, so every API request is handed to it
//! through a channel (`WebRequest`) and the window is woken to answer; the page itself and
//! its script are static. Live lines go out as Server-Sent Events: the connection's thread
//! asks the window for what was appended a few times a second.
//!
//! Every request needs the token: `?token=…` once (answered with a cookie, `HttpOnly`,
//! `SameSite=Strict`, and a redirect that strips it from the address), then the cookie.
//! Requests with a foreign `Origin` are refused, responses are `no-store` with a strict
//! Content-Security-Policy, and at most `MAX_CONNECTIONS` browsers are served at once.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::tail_engine::WakeFn;

/// Default port of the web view.
pub const DEFAULT_PORT: u16 = 47820;
/// Browser connections served at once.
pub const MAX_CONNECTIONS: usize = 8;
/// Lines in one page of `/api/lines`.
pub const PAGE_LINES: usize = 500;
/// Name of the cookie holding the token.
const COOKIE: &str = "fasttail_web";
/// Longest request head read.
const MAX_HEAD: usize = 16 * 1024;
/// How long a request waits for the window to answer.
const ANSWER_WAIT: Duration = Duration::from_secs(5);
/// How often an event stream asks for appended lines (at most 5 batches a second).
const EVENT_PERIOD: Duration = Duration::from_millis(200);
/// A comment line keeps an idle event stream open through proxies.
const KEEPALIVE: Duration = Duration::from_secs(15);

const PAGE: &str = include_str!("web/index.html");
const SCRIPT: &str = include_str!("web/app.js");
const BASE_CSS: &str = include_str!("web/base.css");

/// What the page asks the window for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebQuery {
    /// The open streams, the interface texts and the theme's colours.
    Streams,
    /// `count` visible rows of stream `stream` from row `from` (`None`: the last ones).
    Lines {
        stream: String,
        from: Option<usize>,
        count: usize,
    },
    /// The rows appended after the first `after` (the page's view of generation `gen`).
    Tail {
        stream: String,
        after: usize,
        gen: u64,
    },
    /// The next visible row after `row` (or the previous one before it) whose text
    /// contains `query`, without case; `row` may be -1 (search from the first row).
    Search {
        stream: String,
        row: i64,
        forward: bool,
        query: String,
    },
}

/// The window's answer: a JSON body, or `None` when the stream is gone.
pub type WebAnswer = Option<String>;

/// A request handed to the window.
pub struct WebRequest {
    pub query: WebQuery,
    pub reply: Sender<WebAnswer>,
}

/// A random token: 32 bytes as hex, from the operating system's random hash keys.
pub fn new_token() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut out = String::with_capacity(64);
    for i in 0..4u64 {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u64(i);
        h.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos()),
        );
        out.push_str(&format!("{:016x}", h.finish()));
    }
    out
}

/// The address to bind: loopback, or every interface when other computers are allowed.
pub fn bind_address(port: u16, allow_remote: bool) -> SocketAddr {
    let ip = if allow_remote {
        IpAddr::V4(Ipv4Addr::UNSPECIFIED)
    } else {
        IpAddr::V4(Ipv4Addr::LOCALHOST)
    };
    SocketAddr::new(ip, port)
}

/// The address other computers reach this one at (the interface of the default route),
/// found without sending anything.
pub fn lan_address() -> Option<IpAddr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    let ip = socket.local_addr().ok()?.ip();
    (!ip.is_unspecified() && !ip.is_loopback()).then_some(ip)
}

/// A running web view server; stops when dropped.
pub struct WebServer {
    pub addr: SocketAddr,
    pub requests: Receiver<WebRequest>,
    connections: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
}

impl WebServer {
    /// Starts serving on `addr` with `token`; `wake` wakes the window when a request
    /// waits for it.
    pub fn start(addr: SocketAddr, token: String, wake: Option<WakeFn>) -> std::io::Result<Self> {
        let listener = TcpListener::bind(addr)?;
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?;
        let (tx, rx) = channel();
        let connections = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let shared = Arc::new(Shared {
            token,
            tx,
            wake,
            connections: Arc::clone(&connections),
            stop: Arc::clone(&stop),
        });
        std::thread::Builder::new()
            .name("fasttail-web".into())
            .spawn(move || accept(listener, shared))?;
        Ok(Self {
            addr,
            requests: rx,
            connections,
            stop,
        })
    }

    /// Browsers connected now (pages loading and event streams).
    pub fn connections(&self) -> usize {
        self.connections.load(Ordering::Relaxed)
    }
}

impl Drop for WebServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

struct Shared {
    token: String,
    tx: Sender<WebRequest>,
    wake: Option<WakeFn>,
    connections: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
}

impl Shared {
    /// Asks the window and waits for its answer.
    fn ask(&self, query: WebQuery) -> Option<WebAnswer> {
        let (reply, answer) = channel();
        self.tx.send(WebRequest { query, reply }).ok()?;
        if let Some(wake) = &self.wake {
            wake();
        }
        answer.recv_timeout(ANSWER_WAIT).ok()
    }
}

fn accept(listener: TcpListener, shared: Arc<Shared>) {
    while !shared.stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                let _ = stream.set_nonblocking(false);
                if shared.connections.load(Ordering::Relaxed) >= MAX_CONNECTIONS {
                    let mut stream = stream;
                    let _ = respond(
                        &mut stream,
                        "503 Service Unavailable",
                        "text/plain; charset=utf-8",
                        b"Too many browsers are connected to FastTail.",
                        "",
                    );
                    continue;
                }
                shared.connections.fetch_add(1, Ordering::Relaxed);
                let shared = Arc::clone(&shared);
                let spawned = std::thread::Builder::new()
                    .name("fasttail-web-conn".into())
                    .spawn({
                        let shared = Arc::clone(&shared);
                        move || {
                            serve(stream, &shared);
                            shared.connections.fetch_sub(1, Ordering::Relaxed);
                        }
                    });
                if spawned.is_err() {
                    shared.connections.fetch_sub(1, Ordering::Relaxed);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(_) => std::thread::sleep(Duration::from_millis(50)),
        }
    }
}

/// A parsed request head.
#[derive(Debug, Default)]
struct Request {
    method: String,
    path: String,
    query: HashMap<String, String>,
    headers: HashMap<String, String>,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(String::as_str)
    }

    fn cookie(&self, name: &str) -> Option<&str> {
        self.header("cookie")?
            .split(';')
            .filter_map(|c| c.trim().split_once('='))
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v)
    }

    fn param(&self, name: &str) -> Option<&str> {
        self.query.get(name).map(String::as_str)
    }
}

/// `%xx` and `+` decoded.
fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => {
                let hex = |c: u8| (c as char).to_digit(16);
                match (hex(b[i + 1]), hex(b[i + 2])) {
                    (Some(h), Some(l)) => {
                        out.push((h * 16 + l) as u8);
                        i += 2;
                    }
                    _ => out.push(b'%'),
                }
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn read_request(reader: &mut BufReader<TcpStream>) -> Option<Request> {
    let mut line = String::new();
    let mut budget = MAX_HEAD;
    let n = reader.read_line(&mut line).ok()?;
    if n == 0 || n > budget {
        return None;
    }
    budget -= n;
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_string();
    let target = parts.next()?.to_string();
    let (path, query) = target.split_once('?').unwrap_or((&target, ""));
    let mut req = Request {
        method,
        path: url_decode(path),
        ..Default::default()
    };
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        req.query.insert(url_decode(k), url_decode(v));
    }
    loop {
        line.clear();
        let n = reader.read_line(&mut line).ok()?;
        if n == 0 || n > budget {
            return None;
        }
        budget -= n;
        let l = line.trim_end();
        if l.is_empty() {
            break;
        }
        if let Some((k, v)) = l.split_once(':') {
            req.headers
                .insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }
    Some(req)
}

/// Headers every response carries.
const SECURITY: &str = "Cache-Control: no-store\r\n\
     Content-Security-Policy: default-src 'none'; script-src 'self'; style-src 'self'; \
     connect-src 'self'; img-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'\r\n\
     X-Content-Type-Options: nosniff\r\n\
     Referrer-Policy: no-referrer\r\n";

fn respond(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
    extra: &str,
) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n{SECURITY}{extra}Connection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)
}

/// Whether the request comes from this server's own page: no `Origin`, or the `Host` it
/// was sent to.
fn same_origin(req: &Request) -> bool {
    match (req.header("origin"), req.header("host")) {
        (None, _) => true,
        (Some(origin), Some(host)) => origin == format!("http://{host}"),
        (Some(_), None) => false,
    }
}

/// Compares the tokens in constant time.
fn token_matches(given: &str, token: &str) -> bool {
    given.len() == token.len()
        && given
            .bytes()
            .zip(token.bytes())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
}

/// One connection: one request (the page closes after each answer, the event stream
/// stays open).
fn serve(stream: TcpStream, shared: &Shared) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));
    let Ok(read_half) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(read_half);
    let mut stream = stream;
    let Some(req) = read_request(&mut reader) else {
        return;
    };
    if req.method != "GET" {
        let _ = respond(
            &mut stream,
            "405 Method Not Allowed",
            "text/plain",
            b"",
            "Allow: GET\r\n",
        );
        return;
    }
    if !same_origin(&req) {
        let _ = respond(&mut stream, "403 Forbidden", "text/plain", b"", "");
        return;
    }
    // The token in the address: a cookie, and the same address without it.
    if let Some(given) = req.param("token") {
        if token_matches(given, &shared.token) {
            let cookie = format!(
                "Set-Cookie: {COOKIE}={}; HttpOnly; SameSite=Strict; Path=/\r\nLocation: {}\r\n",
                shared.token,
                if req.path.is_empty() { "/" } else { &req.path }
            );
            let _ = respond(&mut stream, "303 See Other", "text/plain", b"", &cookie);
        } else {
            let _ = respond(
                &mut stream,
                "401 Unauthorized",
                "text/plain; charset=utf-8",
                b"Wrong token: copy the link again from FastTail (Settings, Web view).",
                "",
            );
        }
        return;
    }
    if !req
        .cookie(COOKIE)
        .is_some_and(|c| token_matches(c, &shared.token))
    {
        let _ = respond(
            &mut stream,
            "401 Unauthorized",
            "text/plain; charset=utf-8",
            b"Open the link with the token from FastTail (Settings, Web view).",
            "",
        );
        return;
    }
    let json = "application/json; charset=utf-8";
    let stream_id = req.param("s").unwrap_or_default().to_string();
    let number = |name: &str| req.param(name).and_then(|v| v.parse::<usize>().ok());
    let query = match req.path.as_str() {
        "/" | "/index.html" => {
            let _ = respond(
                &mut stream,
                "200 OK",
                "text/html; charset=utf-8",
                PAGE.as_bytes(),
                "",
            );
            return;
        }
        "/app.js" => {
            let _ = respond(
                &mut stream,
                "200 OK",
                "text/javascript; charset=utf-8",
                SCRIPT.as_bytes(),
                "",
            );
            return;
        }
        "/base.css" => {
            let _ = respond(
                &mut stream,
                "200 OK",
                "text/css; charset=utf-8",
                BASE_CSS.as_bytes(),
                "",
            );
            return;
        }
        "/api/events" => {
            events(
                stream,
                shared,
                stream_id,
                number("after").unwrap_or(0),
                req.param("gen").and_then(|g| g.parse().ok()).unwrap_or(0),
            );
            return;
        }
        "/api/streams" => WebQuery::Streams,
        "/api/lines" => WebQuery::Lines {
            stream: stream_id,
            from: number("from"),
            count: number("count").unwrap_or(PAGE_LINES).clamp(1, PAGE_LINES),
        },
        "/api/search" => WebQuery::Search {
            stream: stream_id,
            row: req.param("row").and_then(|v| v.parse().ok()).unwrap_or(-1),
            forward: req.param("dir") != Some("prev"),
            query: req.param("q").unwrap_or_default().to_string(),
        },
        _ => {
            let _ = respond(&mut stream, "404 Not Found", "text/plain", b"", "");
            return;
        }
    };
    match shared.ask(query) {
        Some(Some(body)) => {
            let _ = respond(&mut stream, "200 OK", json, body.as_bytes(), "");
        }
        Some(None) => {
            let _ = respond(&mut stream, "404 Not Found", json, b"{}", "");
        }
        None => {
            let _ = respond(&mut stream, "503 Service Unavailable", json, b"{}", "");
        }
    }
}

/// An event stream of appended lines (`event: lines`), resets (`event: reset`) and the
/// end of the stream (`event: gone`), until the browser goes away or the server stops.
fn events(mut stream: TcpStream, shared: &Shared, id: String, mut after: usize, mut gen: u64) {
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n{SECURITY}Connection: keep-alive\r\n\r\n"
    );
    if stream.write_all(head.as_bytes()).is_err() {
        return;
    }
    let mut last_write = Instant::now();
    while !shared.stop.load(Ordering::Relaxed) {
        std::thread::sleep(EVENT_PERIOD);
        let answer = shared.ask(WebQuery::Tail {
            stream: id.clone(),
            after,
            gen,
        });
        let message = match answer {
            None => None,
            Some(None) => {
                let _ = stream.write_all(b"event: gone\ndata: {}\n\n");
                return;
            }
            Some(Some(body)) => {
                // The answer says how far it reaches: the page's next `after` and `gen`.
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) {
                    let reset = v["reset"].as_bool().unwrap_or(false);
                    after = v["next"].as_u64().unwrap_or(after as u64) as usize;
                    gen = v["gen"].as_u64().unwrap_or(gen);
                    let has_rows = v["rows"].as_array().is_some_and(|r| !r.is_empty());
                    if reset {
                        Some(format!("event: reset\ndata: {body}\n\n"))
                    } else if has_rows {
                        Some(format!("event: lines\ndata: {body}\n\n"))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
        };
        let write = match message {
            Some(m) => stream.write_all(m.as_bytes()),
            None if last_write.elapsed() >= KEEPALIVE => stream.write_all(b": keepalive\n\n"),
            None => continue,
        };
        if write.is_err() {
            return;
        }
        last_write = Instant::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    /// A server whose "window" answers every query with its debug form.
    fn server() -> (WebServer, String) {
        let token = new_token();
        let srv = WebServer::start(bind_address(0, false), token.clone(), None).unwrap();
        (srv, token)
    }

    fn answer_all(srv: &WebServer) {
        while let Ok(req) = srv.requests.try_recv() {
            let body = match &req.query {
                WebQuery::Lines { stream, .. } if stream == "gone" => None,
                q => Some(serde_json::json!({ "q": format!("{q:?}") }).to_string()),
            };
            let _ = req.reply.send(body);
        }
    }

    fn get(srv: &WebServer, path: &str, headers: &str) -> String {
        let mut s = TcpStream::connect(srv.addr).unwrap();
        write!(
            s,
            "GET {path} HTTP/1.1\r\nHost: {}\r\n{headers}\r\n",
            srv.addr
        )
        .unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut out = String::new();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut buf = [0u8; 4096];
        loop {
            answer_all(srv);
            s.set_read_timeout(Some(Duration::from_millis(20))).unwrap();
            match s.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => out.push_str(&String::from_utf8_lossy(&buf[..n])),
                Err(_) if Instant::now() < deadline => continue,
                Err(_) => break,
            }
        }
        out
    }

    #[test]
    fn the_token_gives_a_cookie_and_the_cookie_opens_the_api() {
        let (srv, token) = server();
        assert!(get(&srv, "/", "").starts_with("HTTP/1.1 401"));
        assert!(get(&srv, "/?token=nope", "").starts_with("HTTP/1.1 401"));
        let r = get(&srv, &format!("/?token={token}"), "");
        assert!(r.starts_with("HTTP/1.1 303"), "{r}");
        assert!(r.contains(&format!(
            "Set-Cookie: fasttail_web={token}; HttpOnly; SameSite=Strict"
        )));
        assert!(r.contains("Location: /\r\n"));
        let cookie = format!("Cookie: fasttail_web={token}\r\n");
        let page = get(&srv, "/", &cookie);
        assert!(page.starts_with("HTTP/1.1 200") && page.contains("<!DOCTYPE html>"));
        assert!(page.contains("Cache-Control: no-store") && page.contains("script-src 'self'"));
        let api = get(&srv, "/api/lines?s=a%2Fb.log&from=10&count=9999", &cookie);
        assert!(
            api.contains("\"Lines { stream: \\\"a/b.log\\\", from: Some(10), count: 500 }\""),
            "{api}"
        );
        assert!(get(&srv, "/api/lines?s=gone", &cookie).starts_with("HTTP/1.1 404"));
        // A page of another site may not call the API.
        let foreign = format!("{cookie}Origin: http://evil.example\r\n");
        assert!(get(&srv, "/api/streams", &foreign).starts_with("HTTP/1.1 403"));
        let own = format!("{cookie}Origin: http://{}\r\n", srv.addr);
        assert!(get(&srv, "/api/streams", &own).starts_with("HTTP/1.1 200"));
    }

    #[test]
    fn decoding_and_tokens() {
        assert_eq!(url_decode("a%2Fb+c%3D"), "a/b c=");
        assert_eq!(url_decode("100%"), "100%");
        assert_eq!(new_token().len(), 64);
        assert_ne!(new_token(), new_token());
        assert!(
            token_matches("abc", "abc")
                && !token_matches("abd", "abc")
                && !token_matches("ab", "abc")
        );
        assert_eq!(bind_address(1, false).ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));
    }

    #[test]
    fn browsers_past_the_cap_are_turned_away() {
        let (srv, token) = server();
        // Connections that never send a request hold their slot.
        let held: Vec<TcpStream> = (0..MAX_CONNECTIONS)
            .map(|_| TcpStream::connect(srv.addr).unwrap())
            .collect();
        let deadline = Instant::now() + Duration::from_secs(5);
        while srv.connections() < MAX_CONNECTIONS {
            assert!(
                Instant::now() < deadline,
                "the connections were not counted"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        let r = get(&srv, &format!("/?token={token}"), "");
        assert!(r.starts_with("HTTP/1.1 503"), "{r}");
        drop(held);
    }
}
