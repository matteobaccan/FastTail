// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Level detection cost by window size, on the worst case: lines without any level
//! token, so the scan walks the whole window. Compares the default 96 bytes with 1,024
//! (`level_detect_bytes`), plus lines whose level sits at byte 150.
//!
//! ```text
//! cargo bench --bench level_window
//! FASTTAIL_BENCH_LINES=2000000 cargo bench --bench level_window
//! ```

use fasttail::log_level::{detect_level_in, LogLevel};
use std::time::Instant;

fn main() {
    let lines: usize = std::env::var("FASTTAIL_BENCH_LINES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(500_000);
    let words = [
        "request",
        "user=42",
        "path=/api/v1/orders",
        "status=200",
        "took=12ms",
    ];
    let plain: Vec<String> = (0..lines)
        .map(|i| {
            let mut s = format!(
                "2026-10-21T12:00:00.{:03}Z host=web-{:02} ",
                i % 1000,
                i % 50
            );
            while s.len() < 1200 {
                s.push_str(words[i % words.len()]);
                s.push(' ');
            }
            s
        })
        .collect();
    let late: Vec<String> = (0..lines)
        .map(|i| {
            format!(
                "{:<150}ERROR event {i}",
                format!("host=web-{:02} pid=4242", i % 50)
            )
        })
        .collect();
    let bytes: usize = plain.iter().map(String::len).sum();
    println!(
        "{lines} lines of 1,200 bytes ({} MB), best of 3",
        bytes / 1_000_000
    );
    for (name, set) in [("no level", &plain), ("level at 150", &late)] {
        for window in [96usize, 1024] {
            let mut best = f64::MAX;
            let mut found = 0;
            for _ in 0..3 {
                let start = Instant::now();
                found = set
                    .iter()
                    .filter(|l| detect_level_in(l, window) != LogLevel::Unknown)
                    .count();
                best = best.min(start.elapsed().as_secs_f64());
            }
            println!(
                "{name:<13} window {window:>5}: {:>7.1} ms, {:>6.1} ns/line ({found} with a level)",
                best * 1000.0,
                best * 1e9 / lines as f64
            );
        }
    }
}
