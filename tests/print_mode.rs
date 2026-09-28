//! Print mode (`fasttail --print`), run as a process: what reaches standard output and
//! standard error, the exit codes, and the same result as the window's export.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

use fasttail::tail_engine::TailEngine;

#[test]
fn without_print_since_and_until_set_the_window_of_the_streams_opened() {
    use fasttail::cli::CliArgs;
    use fasttail::config::FastTailConfig;
    use fasttail::ui::FastTailApp;
    let fx = Fixture::new();
    fx.write("app.log", LOG);
    let cli = CliArgs::parse(
        [
            "--since", "14:03", "--until", "14:04", "--filter", "o", "app.log",
        ],
        fx.dir.path(),
    )
    .unwrap();
    let mut app = FastTailApp::from_config(FastTailConfig::default());
    app.apply_cli(&cli);
    let engine = &app.engines[0];
    assert_eq!(engine.time_from_text, "14:03");
    assert_eq!(engine.time_to_text, "14:04");
    assert!(!engine.time_range_error);
    // WARN 14:03:00 and DEBUG 14:04:59, both containing an "o".
    assert_eq!(engine.visible_line_count(), 2);

    let cli = CliArgs::parse(["--since", "-1h", "app.log"], fx.dir.path()).unwrap();
    let mut app = FastTailApp::from_config(FastTailConfig::default());
    app.apply_cli(&cli);
    let engine = &app.engines[0];
    // A relative time is fixed to the instant it names at start, with its milliseconds.
    assert_eq!(engine.time_from_text.len(), "2026-09-18 14:02:10.123".len());
    assert!(!engine.time_range_error);
    assert_eq!(engine.visible_line_count(), 0);

    // Repeated terms: the last one counts, as before print mode, and past 8 is fine.
    let mut args: Vec<String> = Vec::new();
    for term in [
        "zzz1", "zzz2", "zzz3", "zzz4", "zzz5", "zzz6", "zzz7", "zzz8",
    ] {
        args.extend(["--filter".to_string(), term.to_string()]);
    }
    args.extend(
        [
            "--filter",
            "payment",
            "--exclude",
            "zzz",
            "--exclude",
            "refused",
            "app.log",
        ]
        .map(String::from),
    );
    let cli = CliArgs::parse(args.iter(), fx.dir.path()).unwrap();
    let mut app = FastTailApp::from_config(FastTailConfig::default());
    app.apply_cli(&cli);
    let engine = &app.engines[0];
    assert_eq!(engine.include_filter(), "payment");
    assert_eq!(engine.exclude_filter(), "refused");
}

const LOG: &str = "\
banner without a timestamp
2026-09-18 14:00:00 INFO gateway started
2026-09-18 14:02:10 ERROR payment timeout on order 17
    at Gateway.call(Gateway.java:12)
    at Worker.run(Worker.java:40)
2026-09-18 14:03:00 WARN healthcheck payment timeout
2026-09-18 14:04:59 DEBUG polling
2026-09-18 14:05:30 ERROR payment refused
Caused by: java.io.IOException: reset
2026-09-18 14:06:00 INFO done
";

/// A directory for the test with an empty configuration, so the user's own
/// `fasttail.ini` (rules, theme) never colours the output.
struct Fixture {
    dir: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("fasttail.ini"), "").unwrap();
        Self { dir }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    fn write(&self, name: &str, content: impl AsRef<[u8]>) -> PathBuf {
        let path = self.path(name);
        std::fs::write(&path, content).unwrap();
        path
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_fasttail"));
        cmd.args(args)
            .current_dir(self.dir.path())
            .env("FASTTAIL_CONFIG", self.path("fasttail.ini"))
            .env_remove("NO_COLOR")
            .env("COLORTERM", "truecolor")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        cmd
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command(args).output().unwrap()
    }
}

fn stdout(out: &Output) -> String {
    String::from_utf8(out.stdout.clone()).unwrap()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn several_terms_exclude_and_exit_codes() {
    let fx = Fixture::new();
    fx.write("app.log", LOG);
    let out = fx.run(&[
        "--print",
        "--filter",
        "payment",
        "--filter",
        "timeout",
        "--exclude",
        "healthcheck",
        "app.log",
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "2026-09-18 14:02:10 ERROR payment timeout on order 17\n    at Gateway.call(Gateway.java:12)\n    at Worker.run(Worker.java:40)\n"
    );

    let out = fx.run(&["--print", "--filter", "no-such-text", "app.log"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());

    // A print-only option without --print: usage, code 2, no window.
    let out = fx.run(&["--level", "error", "app.log"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("--print"), "{}", stderr(&out));

    // No input at all and no piped standard input.
    let out = fx.run(&["--print"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("USAGE"), "{}", stderr(&out));

    // A missing input and a zip archive: code 3, the readable input still printed.
    fx.write(
        "bundle.zip",
        b"PK\x05\x06\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
    );
    let out = fx.run(&[
        "--print",
        "--filter",
        "refused",
        "missing.log",
        "bundle.zip",
        "app.log",
    ]);
    assert_eq!(out.status.code(), Some(3));
    assert_eq!(
        stdout(&out),
        "app.log:2026-09-18 14:05:30 ERROR payment refused\napp.log:Caused by: java.io.IOException: reset\n"
    );
    let err = stderr(&out);
    assert!(err.contains("missing.log"), "{err}");
    assert!(
        err.contains("zip") && err.contains("not supported in print mode"),
        "{err}"
    );
}

#[test]
fn level_and_time_windows() {
    let fx = Fixture::new();
    fx.write("app.log", LOG);
    let out = fx.run(&["--print", "--level", "error", "app.log"]);
    assert_eq!(
        stdout(&out),
        "2026-09-18 14:02:10 ERROR payment timeout on order 17\n    at Gateway.call(Gateway.java:12)\n    at Worker.run(Worker.java:40)\n2026-09-18 14:05:30 ERROR payment refused\nCaused by: java.io.IOException: reset\n"
    );

    // A bare time belongs to the day of the first timestamp; "until 14:04" covers 14:04:59.
    let out = fx.run(&["--print", "--since", "14:03", "--until", "14:04", "app.log"]);
    assert_eq!(
        stdout(&out),
        "2026-09-18 14:03:00 WARN healthcheck payment timeout\n2026-09-18 14:04:59 DEBUG polling\n"
    );
    let out = fx.run(&["--print", "--since=2026-09-18 14:05", "app.log"]);
    assert_eq!(
        stdout(&out),
        "2026-09-18 14:05:30 ERROR payment refused\nCaused by: java.io.IOException: reset\n2026-09-18 14:06:00 INFO done\n"
    );

    // Relative times count back from the local clock.
    let now = fasttail::timestamp::local_now_millis();
    let stamp = |minutes_ago: i64| fasttail::timestamp::format_millis(now - minutes_ago * 60_000);
    fx.write(
        "recent.log",
        format!(
            "{} ERROR old\n{} ERROR recent\n  detail\n",
            stamp(180),
            stamp(10)
        ),
    );
    let out = fx.run(&["--print", "--since", "-1h", "recent.log"]);
    assert_eq!(
        stdout(&out),
        format!("{} ERROR recent\n  detail\n", stamp(10))
    );
    let out = fx.run(&["--print", "--until", "-2h", "recent.log"]);
    assert_eq!(stdout(&out), format!("{} ERROR old\n", stamp(180)));
}

#[test]
fn context_prefixes_and_line_numbers() {
    let fx = Fixture::new();
    let lines: String = (1..=10).map(|i| format!("row {i}\n")).collect();
    fx.write("a.log", &lines);
    fx.write("b.log", "row 1\nrow 2\n");
    let out = fx.run(&[
        "--print",
        "--regex",
        "--filter",
        "^row (2|8)$",
        "--context",
        "1",
        "--line-numbers",
        "a.log",
        "b.log",
    ]);
    assert_eq!(
        stdout(&out),
        "a.log:1:row 1\na.log:2:row 2\na.log:3:row 3\n--\na.log:7:row 7\na.log:8:row 8\na.log:9:row 9\n--\nb.log:1:row 1\nb.log:2:row 2\n"
    );
    let out = fx.run(&[
        "--print",
        "--no-prefix",
        "--filter",
        "row 2",
        "a.log",
        "b.log",
    ]);
    assert_eq!(stdout(&out), "row 2\nrow 2\n");
    let out = fx.run(&["--print", "--case-sensitive", "--filter", "ROW", "a.log"]);
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn compressed_input_and_standard_input() {
    use flate2::write::GzEncoder;
    let fx = Fixture::new();
    fx.write("gateway.log", "payment timeout at gateway\nok\n");
    let mut gz = GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(b"payment timeout in archive\nok\n").unwrap();
    fx.write("payment.log.1.gz", gz.finish().unwrap());
    let out = fx.run(&[
        "--print",
        "--filter",
        "payment",
        "gateway.log",
        "payment.log.1.gz",
    ]);
    assert_eq!(
        stdout(&out),
        "gateway.log:payment timeout at gateway\npayment.log.1.gz:payment timeout in archive\n"
    );

    // Piped input without any PATH, and `-` in its place among the paths.
    let pipe = |args: &[&str], input: &'static [u8]| -> Output {
        let mut child = fx.command(args).stdin(Stdio::piped()).spawn().unwrap();
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(input).unwrap();
        drop(stdin);
        child.wait_with_output().unwrap()
    };
    let out = pipe(
        &["--print", "--exclude", "DEBUG"],
        b"INFO a\nDEBUG b\nINFO c",
    );
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(stdout(&out), "INFO a\nINFO c\n");
    let out = pipe(
        &["--print", "--filter", "payment", "-", "gateway.log"],
        b"payment from the pipe\n",
    );
    assert_eq!(
        stdout(&out),
        "stdin:payment from the pipe\ngateway.log:payment timeout at gateway\n"
    );
}

#[test]
fn colours_only_when_asked_or_on_a_terminal() {
    let fx = Fixture::new();
    fx.write(
        "app.log",
        "INFO plain \x1b[31mred\x1b[0m\nINFO request timeout\n",
    );
    // Not a terminal: no escape sequence at all, the log's own ones removed.
    let out = fx.run(&["--print", "app.log"]);
    assert_eq!(stdout(&out), "INFO plain red\nINFO request timeout\n");

    std::fs::write(
        fx.path("fasttail.ini"),
        "[highlight_0]\npattern=timeout\nfg=255,0,0\nbg=0,0,0\n",
    )
    .unwrap();
    let out = fx.run(&["--print", "--color", "always", "app.log"]);
    let text = stdout(&out);
    assert!(
        text.contains("\x1b[0;38;2;255;0;0;48;2;0;0;0mINFO request timeout\x1b[0m"),
        "{text:?}"
    );
    // The line's own red, through the theme's palette.
    assert!(
        text.contains("red\x1b[0m") && text.contains("\x1b[0;38;2;"),
        "{text:?}"
    );
    let out = fx.run(&["--print", "--color", "never", "app.log"]);
    assert!(!stdout(&out).contains('\x1b'));
}

#[test]
fn a_closed_output_ends_quietly() {
    let fx = Fixture::new();
    let big: String = (0..300_000).map(|i| format!("line {i}\n")).collect();
    fx.write("big.log", big);
    let mut child = fx.command(&["--print", "big.log"]).spawn().unwrap();
    let mut first = [0u8; 16];
    child
        .stdout
        .as_mut()
        .unwrap()
        .read_exact(&mut first)
        .unwrap();
    drop(child.stdout.take());
    let status = child.wait().unwrap();
    assert_eq!(status.code(), Some(0));
}

/// Opens the log in the window's engine with the same filters and returns what "Export
/// visible lines..." writes.
fn window_export(path: &Path, include: &[&str], exclude: &[&str], window: (&str, &str)) -> String {
    let mut engine = TailEngine::open(path).unwrap();
    engine.set_filter_terms(
        include.iter().map(|s| s.to_string()).collect(),
        exclude.iter().map(|s| s.to_string()).collect(),
    );
    let (from_ok, to_ok) = engine.apply_time_range_text(window.0, window.1);
    assert!(from_ok && to_ok);
    let mut out = Vec::new();
    engine.export_visible(&mut out).unwrap();
    String::from_utf8(out).unwrap()
}

/// Include terms, exclude terms and the time window (`""` for an open side).
type Case = (
    &'static [&'static str],
    &'static [&'static str],
    (&'static str, &'static str),
);

#[test]
fn same_lines_as_the_window_export() {
    let fx = Fixture::new();
    let log = fx.write("app.log", LOG);
    let cases: [Case; 5] = [
        (&["ERROR"], &[], ("14:02", "14:05")),
        (&["payment"], &["healthcheck"], ("", "")),
        (&[], &[], ("14:03", "")),
        (&["at "], &[], ("", "")),
        (
            &["o"],
            &["DEBUG", "done"],
            ("2026-09-18 14:00", "2026-09-18 14:05:30"),
        ),
    ];
    for (include, exclude, (since, until)) in cases {
        let mut args = vec!["--print"];
        for term in include {
            args.extend(["--filter", term]);
        }
        for term in exclude {
            args.extend(["--exclude", term]);
        }
        if !since.is_empty() {
            args.extend(["--since", since]);
        }
        if !until.is_empty() {
            args.extend(["--until", until]);
        }
        args.push("app.log");
        let printed = stdout(&fx.run(&args));
        let exported = window_export(&log, include, exclude, (since, until));
        assert_eq!(printed, exported, "{args:?}");
        assert!(!printed.is_empty(), "{args:?}");
    }
}

/// Lines of a stream, read on a thread so a test can wait for them with a timeout.
fn lines_of(stream: impl Read + Send + 'static) -> Receiver<String> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stream).lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    rx
}

fn expect_line(rx: &Receiver<String>, want: &str, within: Duration) {
    let deadline = Instant::now() + within;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(left) {
            Ok(line) if line.contains(want) => return,
            Ok(_) => continue,
            Err(_) => panic!("no line containing {want:?} within {within:?}"),
        }
    }
}

struct Killed(Child);

impl Drop for Killed {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn append(path: &Path, text: &str) {
    let mut f = std::fs::OpenOptions::new().append(true).open(path).unwrap();
    f.write_all(text.as_bytes()).unwrap();
}

#[test]
fn follow_prints_appends_truncations_and_newer_pattern_files() {
    let fx = Fixture::new();
    let log = fx.write("app.log", "ERROR first\nINFO skip\n");
    let mut child = Killed(
        fx.command(&["--print", "--follow", "--filter", "ERROR", "app.log"])
            .spawn()
            .unwrap(),
    );
    let out = lines_of(child.0.stdout.take().unwrap());
    let err = lines_of(child.0.stderr.take().unwrap());
    expect_line(&out, "ERROR first", Duration::from_secs(5));

    append(&log, "INFO no\nERROR appended\n");
    expect_line(&out, "ERROR appended", Duration::from_secs(1));
    // A line is printed only once its newline arrives.
    append(&log, "ERROR half");
    std::thread::sleep(Duration::from_millis(400));
    append(&log, " done\n");
    expect_line(&out, "ERROR half done", Duration::from_secs(1));

    std::fs::write(&log, "ERROR after truncation\n").unwrap();
    expect_line(&err, "truncated", Duration::from_secs(1));
    expect_line(&out, "ERROR after truncation", Duration::from_secs(1));
    drop(child);

    // A pattern follows the newest matching file.
    fx.write("svc-1.log", "ERROR in one\n");
    let mut child = Killed(
        fx.command(&["--print", "--follow", "svc-*.log"])
            .spawn()
            .unwrap(),
    );
    let out = lines_of(child.0.stdout.take().unwrap());
    let err = lines_of(child.0.stderr.take().unwrap());
    expect_line(&out, "ERROR in one", Duration::from_secs(5));
    std::thread::sleep(Duration::from_millis(50));
    fx.write("svc-2.log", "ERROR in two\n");
    // The directory is rescanned every 2 seconds.
    expect_line(&err, "switching to svc-2.log", Duration::from_secs(4));
    expect_line(&out, "ERROR in two", Duration::from_secs(1));
    drop(child);
}
