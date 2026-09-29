use super::write_lines;
use fasttail::bookmark_report::{ReportJob, ReportOptions, ReportOrder};
use fasttail::config::FastTailConfig;
use fasttail::find_all::stream_name;
use fasttail::i18n::{t, Language};
use fasttail::tail_engine::TailEngine;
use fasttail::ui::FastTailApp;
use std::io::Write;
use std::time::{Duration, Instant};

const LOG: &[&str] = &[
    "2026-09-18T14:00:00.000Z INFO service up",
    "2026-09-18T14:01:00.000Z INFO deploy v42 started",
    "2026-09-18T14:02:00.000Z ERROR OutOfMemoryError",
    "    at Cache.grow(Cache.java:10)",
    "2026-09-18T14:03:00.000Z INFO deploy v42 done",
    "2026-09-18T14:04:00.000Z WARN retry storm",
];

fn engine(lines: &[&str]) -> (tempfile::TempDir, TailEngine) {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("app.log");
    write_lines(&log, lines);
    let engine = TailEngine::open(&log).unwrap();
    (dir, engine)
}

fn report(engines: &[&TailEngine], options: ReportOptions) -> String {
    let streams = engines
        .iter()
        .map(|e| e.bookmark_report_stream(stream_name(e), &options))
        .collect();
    let mut job = ReportJob::new(streams, options);
    assert!(job.step(Duration::from_secs(5), |s, l| engines[s]
        .get_line(l)
        .map(|c| c.into_owned())));
    job.markdown("now", "test")
}

#[test]
fn tags_come_from_notes_and_jump_under_filters_with_wrap_around() {
    let (_dir, mut engine) = engine(LOG);
    engine.set_bookmark_note(1, "deploy start #Deploy");
    engine.set_bookmark_note(2, "first OOM #oom");
    engine.set_bookmark_note(4, "#deploy done.");
    assert_eq!(engine.bookmark_tags(1), vec!["deploy"]);
    let counts = engine.bookmark_tag_counts();
    assert_eq!(counts.get("deploy"), Some(&2));
    assert_eq!(counts.get("oom"), Some(&1));

    assert_eq!(engine.bookmark_next_tagged(0, "deploy"), Some((1, false)));
    assert_eq!(engine.bookmark_next_tagged(1, "deploy"), Some((4, false)));
    assert_eq!(
        engine.bookmark_next_tagged(4, "deploy"),
        Some((1, true)),
        "wraps"
    );
    assert_eq!(engine.bookmark_next_tagged(0, "missing"), None);

    // A bookmark the filter hides is skipped.
    engine.set_include_filter("done");
    assert_eq!(engine.bookmark_next_tagged(0, "deploy"), Some((4, false)));
    assert_eq!(engine.bookmark_next_tagged(4, "deploy"), Some((4, true)));
}

#[test]
fn a_stream_report_has_timestamps_notes_context_and_the_tag_summary() {
    let (_dir, mut engine) = engine(LOG);
    engine.set_bookmark_note(2, "first OOM #oom");
    engine.toggle_bookmark(5);
    let md = report(
        &[&engine],
        ReportOptions {
            context: 1,
            ..Default::default()
        },
    );
    assert!(
        md.contains("· 1 stream · 2 bookmarks · 2026-09-18 14:02:00 – 2026-09-18 14:04:00"),
        "{md}"
    );
    assert!(md.contains("Tags: #oom (1)"), "{md}");
    assert!(md.contains("## app.log\n"), "{md}");
    assert!(
        md.contains("### Line 3 · 2026-09-18 14:02:00 · first OOM #oom\n"),
        "{md}"
    );
    assert!(
        md.contains("> 3 | 2026-09-18T14:02:00.000Z ERROR OutOfMemoryError\n"),
        "{md}"
    );
    assert!(
        md.contains("  4 |     at Cache.grow(Cache.java:10)\n"),
        "{md}"
    );
    // Lines 3 and 6 with one line of context: 2..=4 and 5..=6 touch, one block.
    assert_eq!(md.matches("```text").count(), 1, "{md}");

    // A tag filter keeps only the tagged bookmark.
    let tagged = report(
        &[&engine],
        ReportOptions {
            tags: vec!["oom".into()],
            ..Default::default()
        },
    );
    assert!(
        tagged.contains("1 bookmark") && !tagged.contains("Line 6"),
        "{tagged}"
    );
    assert_eq!(engine.bookmark_report_count(&ReportOptions::default()), 2);
}

#[test]
fn time_order_across_two_streams() {
    let (_a, mut a) = engine(&["2026-09-18T14:05:00.000Z ERROR late", "x"]);
    let (_b, mut b) = engine(&["2026-09-18T14:01:00.000Z INFO early"]);
    a.set_bookmark_note(0, "late one");
    b.set_bookmark_note(0, "early one");
    let md = report(
        &[&a, &b],
        ReportOptions {
            context: 0,
            order: ReportOrder::Time,
            ..Default::default()
        },
    );
    let early = md.find("early one").unwrap();
    let late = md.find("late one").unwrap();
    assert!(early < late, "{md}");
    assert!(md.contains("(+4m 00s)"), "{md}");
}

#[test]
fn a_compressed_stream_reports_from_its_spool() {
    let dir = tempfile::tempdir().unwrap();
    let gz = dir.path().join("app.log.gz");
    let text: String = LOG.iter().map(|l| format!("{l}\n")).collect();
    let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    enc.write_all(text.as_bytes()).unwrap();
    std::fs::write(&gz, enc.finish().unwrap()).unwrap();
    let mut app = FastTailApp::from_config(FastTailConfig {
        spool_dir: Some(dir.path().join("spool")),
        ..FastTailConfig::default()
    });
    app.open_log_file(gz.clone());
    let engine = &mut app.engines[0];
    let start = Instant::now();
    while !engine.compressed.as_ref().unwrap().is_finalized() {
        assert!(start.elapsed() < Duration::from_secs(20), "never settled");
        engine.poll_updates();
        std::thread::sleep(Duration::from_millis(2));
    }
    engine.set_bookmark_note(5, "storm");
    let md = report(&[&*engine], ReportOptions::default());
    assert!(
        md.contains("> 6 | 2026-09-18T14:04:00.000Z WARN retry storm"),
        "{md}"
    );
    assert!(md.contains("app.log.gz"), "{md}");
}

#[test]
fn report_keys_are_translated_everywhere() {
    for lang in Language::ALL {
        for key in ["bookmark_report_menu", "report_summary", "goto_no_tag"] {
            assert!(!t(*lang, key).is_empty(), "{key} in {lang:?}");
        }
    }
    assert!(t(Language::It, "goto_hint").contains("#tag"));
}
