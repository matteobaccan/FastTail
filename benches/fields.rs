// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Field scanner throughput: JSON, logfmt and the Apache regex over ~100 MB of
//! generated lines held in memory (no file I/O), plus one field lookup per line as a
//! field filter term does.
//!
//! ```text
//! cargo bench --bench fields
//! FASTTAIL_BENCH_BYTES=20000000 cargo bench --bench fields   # a smaller run
//! ```
//!
//! Each parser is timed three times and the best time is reported.

use fasttail::fields::{scan, FieldParser, FieldSpans, ParserChoice};
use std::time::Instant;

const DEFAULT_BYTES: usize = 100_000_000;
const LEVELS: [&str; 4] = ["info", "debug", "warn", "error"];

fn lines(target: usize, make: impl Fn(usize) -> String) -> Vec<String> {
    let mut out = Vec::new();
    let mut bytes = 0;
    let mut i = 0;
    while bytes < target {
        let line = make(i);
        bytes += line.len() + 1;
        out.push(line);
        i += 1;
    }
    out
}

fn run(name: &str, parser: &FieldParser, lines: &[String], key: &str) {
    let bytes: usize = lines.iter().map(|l| l.len() + 1).sum();
    let mut spans = FieldSpans::new();
    let mut best = f64::MAX;
    let mut found = 0usize;
    for _ in 0..3 {
        found = 0;
        let start = Instant::now();
        for line in lines {
            scan(parser, line, &mut spans);
            if spans.get(line, key).is_some_and(|f| f.raw == "error") {
                found += 1;
            }
        }
        best = best.min(start.elapsed().as_secs_f64());
    }
    println!(
        "{name:<8} {:>8.1} MB in {best:>6.3} s  {:>7.1} MB/s  ({found} lines with {key}=error)",
        bytes as f64 / 1e6,
        bytes as f64 / 1e6 / best
    );
}

fn main() {
    let target = std::env::var("FASTTAIL_BENCH_BYTES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_BYTES);
    let json = lines(target, |i| {
        format!(
            r#"{{"ts":"2024-05-01T10:{:02}:{:02}.{:03}Z","level":"{}","logger":"svc.{}","msg":"request {i} handled \"ok\"","http":{{"method":"GET","status":{},"path":"/api/v1/items/{i}"}},"duration_ms":{}}}"#,
            i / 60 % 60,
            i % 60,
            i % 1000,
            LEVELS[i % 4],
            i % 7,
            200 + (i % 5) * 100,
            i % 997
        )
    });
    run("json", &FieldParser::Json, &json, "level");
    drop(json);
    let logfmt = lines(target, |i| {
        format!(
            r#"ts=2024-05-01T10:{:02}:{:02}Z level={} logger=svc.{} msg="request {i} handled" method=GET status={} path=/api/v1/items/{i} duration_ms={}"#,
            i / 60 % 60,
            i % 60,
            LEVELS[i % 4],
            i % 7,
            200 + (i % 5) * 100,
            i % 997
        )
    });
    run("logfmt", &FieldParser::Logfmt, &logfmt, "level");
    drop(logfmt);
    let apache = lines(target, |i| {
        format!(
            r#"10.0.{}.{} - - [01/May/2024:10:{:02}:{:02} +0000] "GET /api/v1/items/{i} HTTP/1.1" {} {} "-" "curl/8.0""#,
            i % 256,
            i % 200,
            i / 60 % 60,
            i % 60,
            200 + (i % 5) * 100,
            i % 5000
        )
    });
    let parser = ParserChoice::Apache.build().unwrap().unwrap();
    run("apache", &parser, &apache, "status");
}
