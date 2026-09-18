//! Filter / highlight / search benchmark on a large log file.
//!
//! Used to compare release profiles (fat vs thin LTO) on the engine hot paths.
//!
//! ```text
//! cargo build --release --example filter_bench
//! target/release/examples/filter_bench <big.log> [rounds]
//! ```
//!
//! Every phase is timed `rounds` times (default 3) and the best time is reported.

use fasttail::tail_engine::{HighlightRule, TailEngine};
use std::time::Instant;

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

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .expect("usage: filter_bench <log file> [rounds]");
    let rounds: usize = args.next().and_then(|r| r.parse().ok()).unwrap_or(3);

    let t0 = Instant::now();
    let mut engine = TailEngine::open(&path).expect("open log file");
    let open_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let total = engine.total_lines();
    println!("open+index      {open_ms:9.1} ms  ({total} lines)");

    let report = |name: &str, (ms, n): (f64, usize)| {
        println!("{name:<16}{ms:9.1} ms  ({n} lines)");
    };

    // Case-insensitive plain-text include filter (the path optimised in PR #11).
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
