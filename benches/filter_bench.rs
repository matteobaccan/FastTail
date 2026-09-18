//! Engine hot-path benchmark: line indexing, include/exclude filters, regex
//! filter, search with match navigation and highlight-rule scanning.
//!
//! Runs only with `cargo bench` (custom harness, `bench` profile inherits the
//! release settings), never with `cargo test`.
//!
//! ```text
//! cargo bench                                   # 200 MB synthetic log in a temp dir
//! FASTTAIL_BENCH_BYTES=1100000000 cargo bench   # the size used for the LTO decision
//! FASTTAIL_BENCH_LOG=/var/log/app.log cargo bench   # an existing log, nothing generated
//! FASTTAIL_BENCH_ROUNDS=5 cargo bench           # rounds per phase (default 3)
//! ```
//!
//! Every phase is timed `rounds` times and the best time is reported.

use fasttail::tail_engine::{HighlightRule, TailEngine};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

const DEFAULT_BYTES: u64 = 200_000_000;
const DEFAULT_ROUNDS: usize = 3;

/// Small deterministic PRNG (xorshift64*), so the generated log is identical
/// on every run and platform without pulling in a `rand` dependency.
struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Writes a synthetic log of roughly `target_bytes` bytes and returns the line count.
fn generate_log(path: &Path, target_bytes: u64) -> std::io::Result<usize> {
    const LEVELS: [&str; 4] = ["INFO", "DEBUG", "WARN", "ERROR"];
    const LEVEL_WEIGHTS: [usize; 4] = [70, 15, 10, 5];
    const SERVICES: [&str; 6] = [
        "payment",
        "auth",
        "gateway",
        "healthcheck",
        "inventory",
        "search",
    ];
    const MESSAGES: [&str; 7] = [
        "request completed status=200 bytes=",
        "cache miss key=user:",
        "connection timeout after ms=",
        "retrying upstream call attempt=",
        "healthcheck ok latency ms=",
        "order validated id=",
        "token refreshed for session ",
    ];

    let mut rng = XorShift(0x9E37_79B9_7F4A_7C15);
    let mut out = BufWriter::with_capacity(1 << 20, std::fs::File::create(path)?);
    let mut written: u64 = 0;
    let mut lines = 0usize;
    while written < target_bytes {
        let roll = rng.below(100);
        let mut acc = 0;
        let mut level = LEVELS[0];
        for (lvl, weight) in LEVELS.iter().zip(LEVEL_WEIGHTS) {
            acc += weight;
            if roll < acc {
                level = lvl;
                break;
            }
        }
        let service = SERVICES[rng.below(SERVICES.len())];
        let message = MESSAGES[rng.below(MESSAGES.len())];
        let number = rng.below(100_000);
        let line = format!(
            "2026-09-18 12:{:02}:{:02}.{:03} [{}] {}-{} req={:09} {}{}\n",
            (lines / 60_000) % 60,
            (lines / 1_000) % 60,
            lines % 1_000,
            level,
            service,
            lines % 17,
            lines,
            message,
            number
        );
        out.write_all(line.as_bytes())?;
        written += line.len() as u64;
        lines += 1;
    }
    out.flush()?;
    Ok(lines)
}

fn best<F: FnMut() -> usize>(rounds: usize, mut f: F) -> (f64, usize) {
    let mut best_ms = f64::MAX;
    let mut result = 0;
    for _ in 0..rounds {
        let t0 = Instant::now();
        result = f();
        best_ms = best_ms.min(t0.elapsed().as_secs_f64() * 1000.0);
    }
    (best_ms, result)
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn main() {
    let rounds = env_u64("FASTTAIL_BENCH_ROUNDS", DEFAULT_ROUNDS as u64).max(1) as usize;

    // Input: an existing log, or a generated one that lives as long as `tmp`.
    let (_tmp, path): (Option<tempfile::TempDir>, PathBuf) =
        match std::env::var("FASTTAIL_BENCH_LOG") {
            Ok(p) if !p.trim().is_empty() => (None, PathBuf::from(p)),
            _ => {
                let bytes = env_u64("FASTTAIL_BENCH_BYTES", DEFAULT_BYTES);
                let dir = tempfile::tempdir().expect("create temp dir");
                let path = dir.path().join("fasttail_bench.log");
                println!("generating {bytes} bytes into {}", path.display());
                let t0 = Instant::now();
                let lines = generate_log(&path, bytes).expect("write synthetic log");
                println!(
                    "generated       {:9.1} ms  ({lines} lines)",
                    t0.elapsed().as_secs_f64() * 1000.0
                );
                (Some(dir), path)
            }
        };
    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    println!(
        "input           {} ({size} bytes), rounds={rounds}",
        path.display()
    );

    let t0 = Instant::now();
    let mut engine = TailEngine::open(&path).expect("open log file");
    let open_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let total = engine.total_lines();
    println!("open+index      {open_ms:9.1} ms  ({total} lines)");

    let report = |name: &str, (ms, n): (f64, usize)| {
        println!("{name:<16}{ms:9.1} ms  ({n} lines)");
    };

    // Case-insensitive plain-text include filter.
    report(
        "include ci",
        best(rounds, || {
            engine.set_include_filter("");
            engine.set_include_filter("error");
            engine.visible_line_count()
        }),
    );

    // Include + exclude, both case-insensitive plain text.
    report(
        "include+exclude",
        best(rounds, || {
            engine.set_exclude_filter("");
            engine.set_include_filter("warn");
            engine.set_exclude_filter("healthcheck");
            engine.visible_line_count()
        }),
    );

    // Regex include filter.
    engine.set_exclude_filter("");
    engine.set_include_filter("");
    engine.filter_is_regex = true;
    report(
        "include regex",
        best(rounds, || {
            engine.set_include_filter("");
            engine.set_include_filter(r"\[(ERROR|WARN)\].*timeout");
            engine.visible_line_count()
        }),
    );
    engine.filter_is_regex = false;
    engine.set_include_filter("");

    // Full-buffer search with match navigation.
    report(
        "search",
        best(rounds, || {
            engine.update_search("");
            engine.update_search("timeout after");
            let mut hops = 0;
            while hops < 1000 && engine.search_next(false).is_some() {
                hops += 1;
            }
            engine.search_matches.len()
        }),
    );

    // Highlight rule evaluation over every line (what rendering + sound alerts do).
    engine.set_highlight_rules(vec![
        HighlightRule::new("ERROR", [255, 80, 80], [40, 0, 0], false),
        HighlightRule::new("warn", [255, 200, 0], [40, 30, 0], false),
        HighlightRule::new(r"req=\d+7 ", [0, 200, 255], [0, 20, 40], true),
    ]);
    report(
        "highlight scan",
        best(rounds, || {
            let mut hits = 0;
            for idx in 0..total {
                if let Some(line) = engine.get_line(idx) {
                    if engine.match_highlight(&line).is_some() {
                        hits += 1;
                    }
                }
            }
            hits
        }),
    );
}
