use fasttail::tail_engine::TailEngine;
use std::io::Write;
use tempfile::NamedTempFile;

/// A log whose entries are one minute apart, with a stack trace hanging off the second
/// one: the continuation lines carry no time of their own.
fn sample_log() -> NamedTempFile {
    let mut tmp = NamedTempFile::new().unwrap();
    writeln!(tmp, "2026-09-18T14:01:00.000Z INFO starting").unwrap();
    writeln!(tmp, "2026-09-18T14:02:05.123Z ERROR boom").unwrap();
    writeln!(tmp, "    at Foo.bar(Foo.java:10)").unwrap();
    writeln!(tmp, "    at Foo.baz(Foo.java:20)").unwrap();
    writeln!(tmp, "2026-09-18T14:03:00.000Z INFO recovered").unwrap();
    writeln!(tmp, "2026-09-18T14:06:00.000Z INFO done").unwrap();
    tmp.flush().unwrap();
    tmp
}

fn millis(iso: &str) -> i64 {
    fasttail::timestamp::detect_timestamp(iso, Default::default())
        .expect("test timestamp")
        .0
}

fn visible(engine: &TailEngine) -> Vec<usize> {
    (0..engine.total_lines())
        .filter(|i| engine.is_line_visible(*i))
        .collect()
}

#[test]
fn continuation_lines_inherit_the_entry_time() {
    let tmp = sample_log();
    let mut engine = TailEngine::open(tmp.path()).unwrap();
    engine.ensure_timestamps();

    let boom = millis("2026-09-18T14:02:05.123Z");
    assert_eq!(engine.line_timestamp(1), Some(boom));
    assert_eq!(engine.line_timestamp(2), Some(boom), "stack frame");
    assert_eq!(engine.line_timestamp(3), Some(boom), "stack frame");
    assert_eq!(
        engine.line_timestamp(4),
        Some(millis("2026-09-18T14:03:00.000Z"))
    );
}

#[test]
fn a_window_keeps_the_entry_together_with_its_stack_trace() {
    let tmp = sample_log();
    let mut engine = TailEngine::open(tmp.path()).unwrap();
    engine.set_time_range(
        Some(millis("2026-09-18T14:02:00.000Z")),
        Some(millis("2026-09-18T14:05:00.000Z")),
    );
    // The error, both of its stack frames, and the recovery - not the 14:01 or 14:06.
    assert_eq!(visible(&engine), vec![1, 2, 3, 4]);
}

#[test]
fn open_ended_windows() {
    let tmp = sample_log();
    let mut engine = TailEngine::open(tmp.path()).unwrap();

    engine.set_time_range(Some(millis("2026-09-18T14:03:00.000Z")), None);
    assert_eq!(visible(&engine), vec![4, 5], "from only");

    engine.set_time_range(None, Some(millis("2026-09-18T14:02:05.123Z")));
    assert_eq!(visible(&engine), vec![0, 1, 2, 3], "to only");

    engine.set_time_range(None, None);
    assert_eq!(visible(&engine), vec![0, 1, 2, 3, 4, 5], "no window");
}

#[test]
fn lines_before_the_first_timestamp_are_hidden_by_a_window() {
    let mut tmp = NamedTempFile::new().unwrap();
    writeln!(tmp, "==== FastTail log banner ====").unwrap();
    writeln!(tmp, "2026-09-18T14:02:00.000Z INFO first timed line").unwrap();
    tmp.flush().unwrap();

    let mut engine = TailEngine::open(tmp.path()).unwrap();
    engine.ensure_timestamps();
    assert_eq!(engine.line_timestamp(0), None, "nothing to inherit yet");

    engine.set_time_range(Some(millis("2026-09-18T14:00:00.000Z")), None);
    assert_eq!(
        visible(&engine),
        vec![1],
        "a line that cannot be placed in time is out of the window"
    );
}

#[test]
fn go_to_time_bisects_an_ordered_log() {
    let tmp = sample_log();
    let mut engine = TailEngine::open(tmp.path()).unwrap();
    engine.ensure_timestamps();

    assert_eq!(
        engine.goto_time(millis("2026-09-18T14:02:05.123Z")),
        Some(1)
    );
    assert_eq!(
        engine.goto_time(millis("2026-09-18T14:02:30.000Z")),
        Some(4),
        "between two entries lands on the next one"
    );
    assert_eq!(
        engine.goto_time(millis("2026-09-18T00:00:00.000Z")),
        Some(0)
    );
    assert_eq!(
        engine.goto_time(millis("2026-09-18T23:00:00.000Z")),
        None,
        "past the end of the log"
    );
}

#[test]
fn go_to_time_scans_when_the_log_jumps_back() {
    let mut tmp = NamedTempFile::new().unwrap();
    writeln!(tmp, "2026-09-18T14:05:00.000Z INFO late first").unwrap();
    writeln!(tmp, "2026-09-18T14:01:00.000Z INFO earlier").unwrap();
    writeln!(tmp, "2026-09-18T14:09:00.000Z INFO last").unwrap();
    tmp.flush().unwrap();

    let mut engine = TailEngine::open(tmp.path()).unwrap();
    engine.ensure_timestamps();
    // A bisection over these would miss line 1; the linear fallback does not.
    assert_eq!(
        engine.goto_time(millis("2026-09-18T14:00:00.000Z")),
        Some(0)
    );
    assert_eq!(
        engine.goto_time(millis("2026-09-18T14:06:00.000Z")),
        Some(2)
    );
    assert_eq!(
        engine.goto_time(millis("2026-09-18T14:02:00.000Z")),
        Some(0)
    );
}

#[test]
fn a_log_we_cannot_time_disables_the_controls() {
    let mut untimed = NamedTempFile::new().unwrap();
    for i in 0..300 {
        writeln!(untimed, "plain line {i} with no timestamp at all").unwrap();
    }
    untimed.flush().unwrap();
    let mut engine = TailEngine::open(untimed.path()).unwrap();
    engine.ensure_timestamps();
    assert_eq!(engine.timestamp_rate(), Some(0.0));
    assert!(!engine.timestamps_usable());

    let mut timed = NamedTempFile::new().unwrap();
    for i in 0..300 {
        writeln!(timed, "2026-09-18T14:0{}:00.000Z line {i}", i % 10).unwrap();
    }
    timed.flush().unwrap();
    let mut engine = TailEngine::open(timed.path()).unwrap();
    engine.ensure_timestamps();
    assert_eq!(engine.timestamp_rate(), Some(1.0));
    assert!(engine.timestamps_usable());
}

#[test]
fn a_log_mostly_made_of_stack_traces_can_be_timed() {
    // One timestamped entry followed by a three-line stack trace: a quarter of the lines
    // carry a timestamp of their own, every line inherits one, so a window places them all.
    let mut tmp = NamedTempFile::new().unwrap();
    for i in 0..100 {
        writeln!(tmp, "2026-09-18 14:{:02}:00 ERROR failure {i}", i % 60).unwrap();
        writeln!(tmp, "java.lang.IllegalStateException: boom").unwrap();
        writeln!(tmp, "\tat com.example.Service.run(Service.java:42)").unwrap();
        writeln!(tmp, "\tat java.base/java.lang.Thread.run(Thread.java:1583)").unwrap();
    }
    tmp.flush().unwrap();
    let mut engine = TailEngine::open(tmp.path()).unwrap();
    engine.ensure_timestamps();
    assert!(engine.timestamps_usable());

    // Most lines before the first timestamp: a window would hide them, the hint stays.
    let mut late = NamedTempFile::new().unwrap();
    for i in 0..300 {
        writeln!(late, "banner line {i}").unwrap();
    }
    writeln!(late, "2026-09-18 14:00:00 INFO started").unwrap();
    late.flush().unwrap();
    let mut engine = TailEngine::open(late.path()).unwrap();
    engine.ensure_timestamps();
    assert!(!engine.timestamps_usable());
}

#[test]
fn a_date_alone_covers_the_whole_day() {
    let mut tmp = NamedTempFile::new().unwrap();
    writeln!(tmp, "2026-09-17T23:59:59.000Z before").unwrap();
    writeln!(tmp, "2026-09-18T00:00:00.000Z first").unwrap();
    writeln!(tmp, "2026-09-18T12:00:00.000Z middle").unwrap();
    writeln!(tmp, "2026-09-18T23:59:59.500Z last").unwrap();
    writeln!(tmp, "2026-09-19T00:00:00.000Z after").unwrap();
    tmp.flush().unwrap();
    let mut engine = TailEngine::open(tmp.path()).unwrap();
    assert_eq!(
        engine.apply_time_range_text("2026-09-18", "2026-09-18"),
        (true, true)
    );
    // From 00:00:00.000 through 23:59:59.999 of that day.
    assert_eq!(visible(&engine), vec![1, 2, 3]);
}

#[test]
fn the_visible_span_follows_the_window() {
    let tmp = sample_log();
    let mut engine = TailEngine::open(tmp.path()).unwrap();
    engine.ensure_timestamps();
    assert_eq!(
        engine.visible_time_span(),
        Some((
            millis("2026-09-18T14:01:00.000Z"),
            millis("2026-09-18T14:06:00.000Z")
        ))
    );

    engine.set_time_range(
        Some(millis("2026-09-18T14:02:00.000Z")),
        Some(millis("2026-09-18T14:05:00.000Z")),
    );
    assert_eq!(
        engine.visible_time_span(),
        Some((
            millis("2026-09-18T14:02:05.123Z"),
            millis("2026-09-18T14:03:00.000Z")
        ))
    );
}

#[test]
fn the_go_to_box_takes_a_time_as_well_as_a_line() {
    let tmp = sample_log();
    let mut engine = TailEngine::open(tmp.path()).unwrap();
    engine.ensure_timestamps();

    // A time, in the shapes a person types.
    let by_clock = engine.resolve_goto("14:03", 0).expect("HH:MM");
    assert_eq!(by_clock.line, 4);
    let by_second = engine.resolve_goto("14:02:05", 0).expect("HH:MM:SS");
    assert_eq!(by_second.line, 1);
    let by_stamp = engine
        .resolve_goto("2026-09-18T14:06:00.000Z", 0)
        .expect("a timestamp copied out of the log");
    assert_eq!(by_stamp.line, 5);

    // Line numbers still work, and still mean lines.
    assert_eq!(engine.resolve_goto("3", 0).unwrap().line, 2);
    assert_eq!(engine.resolve_goto("+2", 1).unwrap().line, 3);
    // A time nothing reaches, and plain rubbish.
    assert!(engine.resolve_goto("23:59", 0).is_none());
    assert!(engine.resolve_goto("later", 0).is_none());
}

#[test]
fn appended_lines_are_timed_too() {
    let tmp = sample_log();
    let mut engine = TailEngine::open(tmp.path()).unwrap();
    engine.set_time_range(Some(millis("2026-09-18T14:00:00.000Z")), None);
    let before = engine.total_lines();

    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(tmp.path())
        .unwrap();
    writeln!(file, "2026-09-18T14:10:00.000Z INFO appended").unwrap();
    file.flush().unwrap();
    engine.poll_updates();

    assert_eq!(engine.total_lines(), before + 1);
    assert_eq!(
        engine.line_timestamp(before),
        Some(millis("2026-09-18T14:10:00.000Z")),
        "a line that arrived after the window was set must still be placed in time"
    );
    assert!(engine.is_line_visible(before));
}

#[test]
fn go_to_time_works_on_a_stream_nobody_filtered_by_time() {
    // No window was ever set, so nothing has built the timestamp cache yet.
    let tmp = sample_log();
    let mut engine = TailEngine::open(tmp.path()).unwrap();
    let target = engine
        .resolve_goto("14:03", 0)
        .expect("a time on a fresh stream");
    assert_eq!(target.line, 4);
}

#[test]
fn the_visible_span_shows_without_a_time_window() {
    let tmp = sample_log();
    let engine = TailEngine::open(tmp.path()).unwrap();
    assert_eq!(
        engine.visible_time_span(),
        Some((
            millis("2026-09-18T14:01:00.000Z"),
            millis("2026-09-18T14:06:00.000Z")
        ))
    );
}

#[test]
fn a_date_and_minute_is_a_valid_bound() {
    let tmp = sample_log();
    let mut engine = TailEngine::open(tmp.path()).unwrap();
    let (from_ok, to_ok) = engine.apply_time_range_text("2026-09-18 14:02", "2026-09-18 14:03");
    assert!(from_ok && to_ok, "YYYY-MM-DD HH:MM on both sides");
    // Through 14:03:59.999: the error, its stack trace and the recovery.
    assert_eq!(visible(&engine), vec![1, 2, 3, 4]);
}
