// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Headless print mode: `fasttail --print [OPTIONS] [PATH...]` writes the lines that pass
//! the filters to standard output and exits, without a window.
//!
//! Every input is read once, front to back, through a 64 KB buffer: no line index, no
//! view state, memory independent of the file size. Each line goes through the same code
//! the window uses — the encoding sniff, the ANSI stripping of render mode, the timestamp
//! parsers with inheritance for continuation lines, `FilterSpec::visible_in_sequence` —
//! so the lines printed are the lines the window shows for the same filters.
//!
//! The module imports nothing from `ui`, `egui` or `eframe`: the terminal interface can
//! reuse it as it is (`fasttail-tui --print`, and its fallback when standard output is
//! not a terminal).

use std::collections::VecDeque;

use std::io::{self, BufReader, BufWriter, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

use crate::ansi::{AnsiColor, AnsiStyle, StyleRun};
use crate::cli::{CliArgs, ColorChoice, USAGE};
use crate::compressed::{ArchiveKind, Format};
use crate::config::FastTailConfig;
use crate::log_level::{detect_level, LogLevel};
use crate::scan_job::FilterSpec;
use crate::stdin_source::StdinKind;
use crate::tail_engine::{
    decode_line, highlight_spans, time_window_contains, CompiledHighlight, FileEncoding,
    HighlightStyle, SpanStyle, TailEngine, MAX_LINE_BYTES, NO_TIMESTAMP, TRUNCATED_LINE_MARKER,
};
use crate::theme::CyberTheme;
use crate::timestamp::{detect_timestamp, FormatHint};

/// At least one line was printed.
pub const EXIT_PRINTED: i32 = 0;
/// No line passed the filters.
pub const EXIT_NO_MATCH: i32 = 1;
/// The command line is wrong.
pub const EXIT_USAGE: i32 = 2;
/// An input could not be read (the others were printed).
pub const EXIT_INPUT_ERROR: i32 = 3;

/// Bytes read from an input at a time, and the size of the output buffer.
const IO_BYTES: usize = 64 * 1024;
/// Bytes the encoding sniff looks at, as in the window.
const ENCODING_SAMPLE: usize = crate::tail_engine::ENCODING_SAMPLE_BYTES as usize;
/// Bytes remembered at the head of a followed file to recognise a rewrite.
const FINGERPRINT_LEN: usize = 64;
/// How often a followed file's size is checked when no file event arrives.
const SIZE_CHECK_INTERVAL: Duration = Duration::from_millis(250);
/// How often a followed pattern looks for a newer matching file.
const PATTERN_SCAN_INTERVAL: Duration = crate::tail_engine::PATTERN_SCAN_INTERVAL;

/// Runs print mode for `cli` (`cli.print` set) and returns the process exit code.
pub fn run(cli: &CliArgs) -> i32 {
    console::attach_if_missing();

    let mut inputs: Vec<InputSpec> = cli.paths.iter().cloned().map(InputSpec::Path).collect();
    let piped = crate::stdin_source::classify() == StdinKind::Piped;
    if cli.stdin {
        if !piped {
            eprintln!("fasttail: standard input is not a pipe; nothing to read");
            return EXIT_USAGE;
        }
        inputs.insert(cli.stdin_at.min(inputs.len()), InputSpec::Stdin);
    } else if inputs.is_empty() {
        if !piped {
            eprintln!("fasttail: --print needs a PATH, `-` or piped standard input\n\n{USAGE}");
            return EXIT_USAGE;
        }
        inputs.push(InputSpec::Stdin);
    }

    // Read only: the theme and the rules colour the output, nothing is ever written.
    let config = FastTailConfig::load_read_only();
    let palette = colour_output(cli.color.unwrap_or_default()).map(|truecolor| Palette {
        theme: config.theme,
        rules: config
            .highlight_rules
            .iter()
            .map(CompiledHighlight::compile)
            .collect(),
        truecolor,
        levels: config.level_colors,
        tokens: config.auto_tokens(),
    });
    let matcher = Matcher::from_cli(cli);
    let options = PrinterOptions {
        prefix: inputs.len() > 1 && !cli.no_prefix,
        line_numbers: cli.line_numbers,
        separators: matcher.context > 0,
    };
    let stdout = io::stdout();
    let mut printer = Printer::new(
        BufWriter::with_capacity(IO_BYTES, stdout.lock()),
        palette,
        options,
    );
    let follow = cli.follow == Some(true);
    match print_inputs(inputs, &matcher, &mut printer, follow) {
        Ok(failed) => exit_code(printer.printed, failed),
        Err(err) => output_failed(&err),
    }
}

/// The exit code once every input has been read.
fn exit_code(printed: u64, input_failed: bool) -> i32 {
    if input_failed {
        EXIT_INPUT_ERROR
    } else if printed > 0 {
        EXIT_PRINTED
    } else {
        EXIT_NO_MATCH
    }
}

/// A write to standard output failed: a reader that went away (`| head`) ends the program
/// quietly and successfully; anything else is reported.
fn output_failed(err: &io::Error) -> i32 {
    if err.kind() == io::ErrorKind::BrokenPipe {
        EXIT_PRINTED
    } else {
        eprintln!("fasttail: cannot write to standard output: {err}");
        EXIT_INPUT_ERROR
    }
}

/// Whether to colour, and then whether in 24-bit colour (`Some(truecolor)`).
fn colour_output(choice: ColorChoice) -> Option<bool> {
    let terminal = io::stdout().is_terminal();
    let colour = match choice {
        ColorChoice::Always => true,
        ColorChoice::Never => false,
        ColorChoice::Auto => terminal && std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty()),
    };
    if !colour {
        return None;
    }
    // A Windows console needs virtual-terminal processing to read the sequences; where
    // it cannot be enabled the output stays plain.
    if terminal && console::enable_virtual_terminal() == Some(false) {
        return None;
    }
    let colorterm = std::env::var("COLORTERM").unwrap_or_default();
    Some(cfg!(windows) || matches!(colorterm.as_str(), "truecolor" | "24bit"))
}

/// Prints every input in order, then follows the ones that can be followed. Returns
/// whether an input could not be read; an error is a failed write to standard output.
fn print_inputs(
    inputs: Vec<InputSpec>,
    matcher: &Matcher,
    printer: &mut Printer<impl Write>,
    follow: bool,
) -> io::Result<bool> {
    let mut failed = false;
    let mut followed = Vec::new();
    let mut next_id = 0u64;
    for spec in inputs {
        let mut input = match Input::open(spec, follow, &mut next_id) {
            Ok(input) => input,
            Err(msg) => {
                eprintln!("fasttail: {msg}");
                failed = true;
                continue;
            }
        };
        let hold_partial = input.follow.is_some();
        match input.pump(matcher, printer, !hold_partial) {
            Ok(()) => {}
            Err(Stop::Output(err)) => return Err(err),
            Err(Stop::Input(err)) => {
                eprintln!("fasttail: cannot read {}: {err}", input.name);
                failed = true;
                continue;
            }
        }
        if input.follow.is_some() {
            followed.push(input);
        }
    }
    printer.flush()?;
    if !followed.is_empty() {
        follow_inputs(&mut followed, matcher, printer, &mut next_id)?;
    }
    Ok(failed)
}

/// Keeps printing what is appended to `inputs` until the process is interrupted: a file
/// event or the size check every `SIZE_CHECK_INTERVAL` wakes the loop.
fn follow_inputs(
    inputs: &mut [Input],
    matcher: &Matcher,
    printer: &mut Printer<impl Write>,
    next_id: &mut u64,
) -> io::Result<()> {
    use notify::{RecursiveMode, Watcher};
    interrupt::install();
    let (tx, rx) = channel::<notify::Result<notify::Event>>();
    let mut watcher = notify::RecommendedWatcher::new(tx, notify::Config::default()).ok();
    if let Some(watcher) = watcher.as_mut() {
        for input in inputs.iter() {
            if let Some(dir) = input.follow.as_ref().map(Follow::watched_dir) {
                let _ = watcher.watch(&dir, RecursiveMode::NonRecursive);
            }
        }
    }
    loop {
        let _ = rx.recv_timeout(SIZE_CHECK_INTERVAL);
        if interrupt::requested() {
            // Ctrl+C: end with the exit code earned so far.
            return printer.flush();
        }
        while rx.try_recv().is_ok() {}
        for input in inputs.iter_mut() {
            match input.poll(matcher, printer, next_id) {
                Ok(()) => {}
                Err(Stop::Output(err)) => return Err(err),
                // Transient (a file being replaced): the next check tries again.
                Err(Stop::Input(_)) => {}
            }
        }
        printer.flush()?;
    }
}

/// An input as named on the command line.
#[derive(Debug, Clone)]
enum InputSpec {
    Path(PathBuf),
    Stdin,
}

/// Why reading an input stopped early.
enum Stop {
    /// Standard output failed: the whole program stops.
    Output(io::Error),
    /// This input failed: the others go on.
    Input(io::Error),
}

/// What a followed file needs between two reads.
struct Follow {
    /// The file being read (the newest match of a pattern).
    path: PathBuf,
    /// Directory and file-name pattern of a pattern input.
    pattern: Option<(PathBuf, String)>,
    /// Bytes read from the file so far.
    offset: u64,
    /// The first `FINGERPRINT_LEN` bytes read, to recognise a rewrite.
    head: Vec<u8>,
    last_rescan: Instant,
}

impl Follow {
    fn watched_dir(&self) -> PathBuf {
        match &self.pattern {
            Some((dir, _)) => dir.clone(),
            None => self
                .path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .map_or_else(|| PathBuf::from("."), Path::to_path_buf),
        }
    }

    /// Whether the file at `path` still starts with the bytes read first. An unreadable
    /// file counts as unchanged: the next check sees it again.
    fn head_unchanged(&self) -> bool {
        if self.head.is_empty() {
            return true;
        }
        let Ok(mut file) = crate::file_source::open_file_shared(&self.path) else {
            return true;
        };
        let mut now = vec![0u8; self.head.len()];
        match file.read_exact(&mut now) {
            Ok(()) => now == self.head,
            Err(_) => false,
        }
    }
}

/// One input being read: its reader, the line splitter and the filter state.
struct Input {
    name: String,
    /// Distinguishes inputs (and restarts of one) for the context separators.
    id: u64,
    reader: Box<dyn Read>,
    splitter: Splitter,
    pipeline: Pipeline,
    follow: Option<Follow>,
    /// Standard input arrives as it is written: every read is flushed to the output.
    flush_each_read: bool,
    buf: Vec<u8>,
}

impl Input {
    fn open(spec: InputSpec, follow: bool, next_id: &mut u64) -> Result<Self, String> {
        let id = *next_id;
        *next_id += 1;
        let path = match spec {
            InputSpec::Stdin => {
                let stdin = crate::stdin_source::take_stdin()
                    .ok_or_else(|| "standard input is not available".to_string())?;
                return Ok(Self::new(
                    crate::stdin_source::STDIN_TITLE.to_string(),
                    id,
                    Box::new(stdin),
                    None,
                    true,
                ));
            }
            InputSpec::Path(path) => path,
        };
        let pattern = crate::wildcard::split_pattern(&path);
        let file_path = match &pattern {
            Some((dir, glob)) => crate::wildcard::resolve_newest(dir, glob)
                .ok_or_else(|| format!("no file matches {}", display_name(&path)))?,
            None => path.clone(),
        };
        let name = display_name(&file_path);
        if !file_path.is_file() {
            return Err(format!("{name}: no such file"));
        }
        match crate::compressed::archive_kind(&file_path) {
            Some(ArchiveKind::Zip) => {
                return Err(format!(
                    "{name} is a zip archive: its entries cannot be printed (not supported in print mode)"
                ))
            }
            Some(ArchiveKind::Tar(_)) => {
                return Err(format!(
                    "{name} is a tar archive: its entries cannot be printed (not supported in print mode)"
                ))
            }
            Some(ArchiveKind::SevenZ) => {
                return Err(format!(
                    "{name} is a 7z archive: its entries cannot be printed (not supported in print mode)"
                ))
            }
            None => {}
        }
        let file = crate::file_source::open_file_shared(&file_path)
            .map_err(|e| format!("cannot open {name}: {e}"))?;
        match crate::compressed::sniff(&file_path) {
            Format::Compressed(codec) => {
                if follow {
                    eprintln!("fasttail: {name} is compressed and is not followed");
                }
                let reader = crate::compressed::open_decoder(codec, BufReader::new(file));
                Ok(Self::new(name, id, reader, None, false))
            }
            Format::EmptyZip => Err(format!(
                "{name} is a zip archive: its entries cannot be printed (not supported in print mode)"
            )),
            _ => {
                let follow = follow.then(|| Follow {
                    path: file_path.clone(),
                    pattern,
                    offset: 0,
                    head: Vec::new(),
                    last_rescan: Instant::now(),
                });
                Ok(Self::new(name, id, Box::new(file), follow, false))
            }
        }
    }

    fn new(
        name: String,
        id: u64,
        reader: Box<dyn Read>,
        follow: Option<Follow>,
        flush_each_read: bool,
    ) -> Self {
        Self {
            name,
            id,
            reader,
            splitter: Splitter::default(),
            pipeline: Pipeline::default(),
            follow,
            flush_each_read,
            buf: vec![0u8; IO_BYTES],
        }
    }

    /// Reads to the current end of the input and prints what passes. With `finish` the
    /// last line is printed even without its newline (the input is over); otherwise it is
    /// kept until its newline arrives.
    fn pump(
        &mut self,
        matcher: &Matcher,
        printer: &mut Printer<impl Write>,
        finish: bool,
    ) -> Result<(), Stop> {
        let Input {
            name,
            id,
            reader,
            splitter,
            pipeline,
            follow,
            flush_each_read,
            buf,
        } = self;
        let source = Source {
            name: name.as_str(),
            id: *id,
        };
        loop {
            let n = match reader.read(buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(Stop::Input(e)),
            };
            let data = &buf[..n];
            if let Some(f) = follow.as_mut() {
                f.offset += n as u64;
                if f.head.len() < FINGERPRINT_LEN {
                    let take = (FINGERPRINT_LEN - f.head.len()).min(n);
                    f.head.extend_from_slice(&data[..take]);
                }
            }
            let mut sink = |bytes: &[u8], encoding, truncated| {
                pipeline.process(matcher, printer, &source, bytes, encoding, truncated)
            };
            splitter.feed(data, &mut sink).map_err(Stop::Output)?;
            if *flush_each_read {
                // A pipe gives what was written: that is all there is for now.
                splitter.settle(&mut sink).map_err(Stop::Output)?;
                printer.flush().map_err(Stop::Output)?;
            }
        }
        splitter
            .settle(&mut |bytes: &[u8], encoding, truncated| {
                pipeline.process(matcher, printer, &source, bytes, encoding, truncated)
            })
            .map_err(Stop::Output)?;
        if finish {
            splitter
                .finish(|bytes, encoding, truncated| {
                    pipeline.process(matcher, printer, &source, bytes, encoding, truncated)
                })
                .map_err(Stop::Output)?;
        }
        Ok(())
    }

    /// One check of a followed input: a newer file for a pattern, a truncation or a
    /// rewrite (read again from the start, with a notice), or new bytes to print.
    fn poll(
        &mut self,
        matcher: &Matcher,
        printer: &mut Printer<impl Write>,
        next_id: &mut u64,
    ) -> Result<(), Stop> {
        let switch = match self.follow.as_mut() {
            None => return Ok(()),
            Some(follow) => match &follow.pattern {
                Some((dir, glob)) if follow.last_rescan.elapsed() >= PATTERN_SCAN_INTERVAL => {
                    follow.last_rescan = Instant::now();
                    crate::wildcard::resolve_newest(dir, glob)
                        .filter(|newest| !crate::paths::paths_equal(newest, &follow.path))
                        .map(|newest| (dir.join(glob), newest))
                }
                _ => None,
            },
        };
        if let Some((pattern, newest)) = switch {
            self.restart(newest.clone(), matcher, printer, next_id)?;
            eprintln!(
                "fasttail: {}: switching to {}",
                display_name(&pattern),
                display_name(&newest)
            );
            return self.pump(matcher, printer, false);
        }
        let Some(follow) = self.follow.as_ref() else {
            return Ok(());
        };
        let Ok(meta) = std::fs::metadata(&follow.path) else {
            // Deleted or being replaced: wait for it to come back.
            return Ok(());
        };
        let len = meta.len();
        if len < follow.offset || !follow.head_unchanged() {
            let path = follow.path.clone();
            let name = self.name.clone();
            self.restart(path, matcher, printer, next_id)?;
            eprintln!("fasttail: {name} truncated, reading from the start");
        } else if len == follow.offset {
            return Ok(());
        }
        self.pump(matcher, printer, false)
    }

    /// Reads `path` again from its first byte, as a new input (line numbers, timestamps
    /// and filter state start over). The last line of the old content, held until its
    /// newline, is complete as it is and goes through the filters first. When `path`
    /// cannot be opened nothing changes, and the next check tries again.
    fn restart(
        &mut self,
        path: PathBuf,
        matcher: &Matcher,
        printer: &mut Printer<impl Write>,
        next_id: &mut u64,
    ) -> Result<(), Stop> {
        let file = crate::file_source::open_file_shared(&path).map_err(Stop::Input)?;
        let source = Source {
            name: self.name.as_str(),
            id: self.id,
        };
        let pipeline = &mut self.pipeline;
        self.splitter
            .finish(|bytes, encoding, truncated| {
                pipeline.process(matcher, printer, &source, bytes, encoding, truncated)
            })
            .map_err(Stop::Output)?;
        self.reader = Box::new(file);
        self.splitter = Splitter::default();
        self.pipeline = Pipeline::default();
        self.id = *next_id;
        *next_id += 1;
        self.name = display_name(&path);
        if let Some(follow) = self.follow.as_mut() {
            follow.path = path;
            follow.offset = 0;
            follow.head.clear();
        }
        Ok(())
    }
}

/// How an input is named in notices and in the `file:` prefix: its path relative to the
/// current directory when it lies below it, as typed in the common case.
fn display_name(path: &Path) -> String {
    let relative = std::env::current_dir()
        .ok()
        .and_then(|cwd| path.strip_prefix(cwd).ok().map(Path::to_path_buf));
    relative
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| path.to_path_buf())
        .display()
        .to_string()
}

/// Splits a byte stream into lines with the engine's rules: the encoding sniffed from the
/// first bytes (BOM skipped), `\n` (a UTF-16 unit for UTF-16) ending a line, and a line
/// longer than `MAX_LINE_BYTES` cut there and marked truncated.
#[derive(Default)]
struct Splitter {
    encoding: Option<FileEncoding>,
    /// Bytes of the current line kept so far, newline included, at most `MAX_LINE_BYTES`.
    line: Vec<u8>,
    /// Bytes of the current line seen so far, those past the cap included.
    line_len: usize,
    /// The odd byte of a UTF-16 unit cut by a read.
    odd: Option<u8>,
    /// The first bytes, held until there are enough to sniff the encoding from.
    pending: Vec<u8>,
}

impl Splitter {
    fn push(&mut self, bytes: &[u8]) {
        self.line_len += bytes.len();
        let room = MAX_LINE_BYTES.saturating_sub(self.line.len());
        self.line.extend_from_slice(&bytes[..bytes.len().min(room)]);
    }

    fn emit(
        &mut self,
        f: &mut impl FnMut(&[u8], FileEncoding, bool) -> io::Result<()>,
    ) -> io::Result<()> {
        let truncated = self.line_len > MAX_LINE_BYTES;
        let result = f(&self.line, self.encoding.unwrap_or_default(), truncated);
        self.line.clear();
        self.line_len = 0;
        result
    }

    /// Feeds the next bytes; `f` receives every complete line. The first
    /// `ENCODING_SAMPLE` bytes are held until the encoding is known (see `settle`).
    fn feed(
        &mut self,
        data: &[u8],
        mut f: impl FnMut(&[u8], FileEncoding, bool) -> io::Result<()>,
    ) -> io::Result<()> {
        if self.encoding.is_none() {
            self.pending.extend_from_slice(data);
            if self.pending.len() < ENCODING_SAMPLE {
                return Ok(());
            }
            return self.settle(&mut f);
        }
        self.split(data, &mut f)
    }

    /// Sniffs the encoding from the bytes held so far, when it is not known yet, and
    /// splits them: called once the sample is complete, and when the input has nothing
    /// more to give for now (end of a file read, a read of standard input).
    fn settle(
        &mut self,
        f: &mut impl FnMut(&[u8], FileEncoding, bool) -> io::Result<()>,
    ) -> io::Result<()> {
        if self.encoding.is_some() || self.pending.is_empty() {
            return Ok(());
        }
        let pending = std::mem::take(&mut self.pending);
        let (encoding, _) =
            TailEngine::detect_encoding(&pending[..pending.len().min(ENCODING_SAMPLE)]);
        self.encoding = Some(encoding);
        let bom = match encoding {
            FileEncoding::Utf8 if pending.starts_with(&[0xEF, 0xBB, 0xBF]) => 3,
            FileEncoding::UnicodeLe if pending.starts_with(&[0xFF, 0xFE]) => 2,
            FileEncoding::UnicodeBe if pending.starts_with(&[0xFE, 0xFF]) => 2,
            _ => 0,
        };
        self.split(&pending[bom..], f)
    }

    fn split(
        &mut self,
        mut data: &[u8],
        f: &mut impl FnMut(&[u8], FileEncoding, bool) -> io::Result<()>,
    ) -> io::Result<()> {
        let (nl_lo, nl_hi) = match self.encoding.unwrap_or_default() {
            FileEncoding::UnicodeLe => (0x0A, 0x00),
            FileEncoding::UnicodeBe => (0x00, 0x0A),
            _ => {
                while let Some(i) = memchr::memchr(b'\n', data) {
                    self.push(&data[..=i]);
                    self.emit(f)?;
                    data = &data[i + 1..];
                }
                self.push(data);
                return Ok(());
            }
        };
        let joined;
        let data: &[u8] = match self.odd.take() {
            Some(b) => {
                let mut v = Vec::with_capacity(data.len() + 1);
                v.push(b);
                v.extend_from_slice(data);
                joined = v;
                &joined
            }
            None => data,
        };
        let even = data.len() & !1;
        if even < data.len() {
            self.odd = Some(data[even]);
        }
        let mut start = 0;
        let mut k = 0;
        while k + 2 <= even {
            if data[k] == nl_lo && data[k + 1] == nl_hi {
                self.push(&data[start..k + 2]);
                self.emit(f)?;
                start = k + 2;
            }
            k += 2;
        }
        self.push(&data[start..even]);
        Ok(())
    }

    /// The input is over: the last line, if it has no newline, is complete as it is.
    fn finish(
        &mut self,
        mut f: impl FnMut(&[u8], FileEncoding, bool) -> io::Result<()>,
    ) -> io::Result<()> {
        self.settle(&mut f)?;
        if let Some(b) = self.odd.take() {
            self.push(&[b]);
        }
        if self.line_len > 0 {
            self.emit(&mut f)?;
        }
        Ok(())
    }
}

/// The filters of the command line, compiled once.
struct Matcher {
    filter: FilterSpec,
    since: Option<String>,
    until: Option<String>,
    /// The local time relative times count back from, fixed at start.
    now: i64,
    context: usize,
}

impl Matcher {
    fn from_cli(cli: &CliArgs) -> Self {
        let filter = FilterSpec::build(&cli.filter, &cli.exclude, cli.case_sensitive, cli.regex)
            .with_levels(cli.level.unwrap_or(LogLevel::Unknown), false);
        Self {
            filter,
            since: cli.since.clone(),
            until: cli.until.clone(),
            now: crate::timestamp::local_now_millis(),
            context: cli.context.unwrap_or(0),
        }
    }

    fn has_window(&self) -> bool {
        self.since.is_some() || self.until.is_some()
    }

    /// The window of a stream whose first timestamp is `reference` (the day a bare
    /// `14:02` belongs to, as in the popup).
    fn window(&self, reference: i64) -> (Option<i64>, Option<i64>) {
        let side = |text: &Option<String>, until: bool| {
            text.as_deref()
                .and_then(|t| crate::cli::resolve_time_arg(t, reference, self.now, until))
        };
        (side(&self.since, false), side(&self.until, true))
    }
}

/// Which input a line comes from.
struct Source<'a> {
    name: &'a str,
    id: u64,
}

/// A line kept for context: its number, its text as decoded (escape sequences included)
/// and whether it was cut at the length cap.
struct HeldLine {
    number: u64,
    raw: String,
    truncated: bool,
}

/// The per-input state of the line evaluation.
struct Pipeline {
    number: u64,
    /// Visibility of the last entry, which its stack trace lines follow.
    parent_visible: bool,
    /// Timestamp of the last timed line, inherited by the lines without one.
    inherited: i64,
    hint: FormatHint,
    /// The time window, resolved at the first timestamped line.
    window: Option<(Option<i64>, Option<i64>)>,
    /// The last lines not printed, for the context before the next match.
    before: VecDeque<HeldLine>,
    /// Lines of context still to print after the last match.
    after: usize,
}

impl Default for Pipeline {
    fn default() -> Self {
        Self {
            number: 0,
            parent_visible: false,
            inherited: NO_TIMESTAMP,
            hint: FormatHint::default(),
            window: None,
            before: VecDeque::new(),
            after: 0,
        }
    }
}

impl Pipeline {
    /// Evaluates one line (`bytes`, newline included) and prints it, with its context,
    /// when it passes.
    fn process(
        &mut self,
        matcher: &Matcher,
        printer: &mut Printer<impl Write>,
        source: &Source<'_>,
        bytes: &[u8],
        encoding: FileEncoding,
        truncated: bool,
    ) -> io::Result<()> {
        self.number += 1;
        // The cut bytes hold no newline, so decoding them as a whole line is the same.
        let raw = decode_line(bytes, encoding, false);
        let matched = self.evaluate(matcher, &raw, truncated);
        if matched {
            while let Some(held) = self.before.pop_front() {
                printer.line(source, held.number, &held.raw, held.truncated)?;
            }
            printer.line(source, self.number, &raw, truncated)?;
            printer.printed += 1;
            self.after = matcher.context;
        } else if self.after > 0 {
            self.after -= 1;
            printer.line(source, self.number, &raw, truncated)?;
        } else if matcher.context > 0 {
            if self.before.len() == matcher.context {
                self.before.pop_front();
            }
            self.before.push_back(HeldLine {
                number: self.number,
                raw,
                truncated,
            });
        }
        Ok(())
    }

    /// Whether the line passes: the text as the window's features see it (without escape
    /// sequences, with the truncation marker), timed with inheritance, through the
    /// filter in sequence and the time window.
    fn evaluate(&mut self, matcher: &Matcher, raw: &str, truncated: bool) -> bool {
        let mut text = crate::ansi::strip(raw);
        if truncated {
            text.to_mut().push_str(TRUNCATED_LINE_MARKER);
        }
        if let Some((millis, format)) = detect_timestamp(&text, self.hint) {
            self.hint = format;
            self.inherited = millis;
            if self.window.is_none() && matcher.has_window() {
                // As in the window: a bare time belongs to the day of the stream's first
                // timestamp, whether or not that line passes the filters.
                self.window = Some(matcher.window(millis));
            }
        }
        let (visible, next) = matcher
            .filter
            .visible_in_sequence(&text, self.parent_visible);
        self.parent_visible = next;
        if !visible || !matcher.has_window() {
            return visible;
        }
        if self.inherited == NO_TIMESTAMP {
            // Before the first timed line: cannot be placed in time, hidden as in the window.
            return false;
        }
        let (from, to) = self.window.unwrap_or((None, None));
        time_window_contains(Some(self.inherited), from, to)
    }
}

/// Colours of the output: the theme's level styles and the configured rules.
struct Palette {
    theme: CyberTheme,
    rules: Vec<CompiledHighlight>,
    /// Level colours on (`level_colors` in `fasttail.ini`).
    levels: bool,
    /// Automatic token highlighting as in the window (`auto_highlight` and
    /// `auto_highlight_kinds`), `NONE` while it is off.
    tokens: crate::auto_highlight::TokenKinds,
    /// 24-bit colour; otherwise the nearest of the 256 xterm colours.
    truecolor: bool,
}

/// How the lines are prefixed and grouped.
#[derive(Debug, Clone, Copy, Default)]
struct PrinterOptions {
    /// `name:` before every line (several inputs, no `--no-prefix`).
    prefix: bool,
    /// `number:` before every line.
    line_numbers: bool,
    /// `--` between groups of lines that are not consecutive (context mode).
    separators: bool,
}

/// Writes lines to the output, plain or coloured.
struct Printer<W: Write> {
    out: W,
    palette: Option<Palette>,
    options: PrinterOptions,
    /// The input and the number of the last line written, for the separators.
    last: Option<(u64, u64)>,
    /// Matching lines written (context lines not counted).
    printed: u64,
}

/// Attributes of a piece of coloured text, colours as RGB.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Sgr {
    fg: Option<[u8; 3]>,
    bg: Option<[u8; 3]>,
    bold: bool,
    dim: bool,
    italic: bool,
    underline: bool,
    inverse: bool,
}

impl Sgr {
    fn of_rule(style: &HighlightStyle) -> Self {
        Self {
            fg: Some(style.fg.rgb()),
            bg: Some(style.bg.rgb()),
            bold: style.bold,
            italic: style.italic,
            ..Default::default()
        }
    }

    /// The line's own ANSI attributes over `base`: the theme's palette for the base
    /// colours (bold brightening 0-7, as in the window), the terminal for the rest.
    fn of_ansi(style: &AnsiStyle, base: &Sgr, theme: CyberTheme) -> Self {
        let fg = match style.fg {
            Some(AnsiColor::Indexed(i)) if style.bold && i < 8 => Some(AnsiColor::Indexed(i + 8)),
            other => other,
        };
        Self {
            fg: fg.map(|c| theme.ansi_color(c).rgb()).or(base.fg),
            bg: style.bg.map(|c| theme.ansi_color(c).rgb()).or(base.bg),
            bold: style.bold || base.bold,
            dim: style.dim,
            italic: style.italic || base.italic,
            underline: style.underline,
            inverse: style.inverse,
        }
    }

    /// The SGR sequence setting these attributes from the plain style.
    fn sequence(&self, truecolor: bool) -> String {
        let mut codes: Vec<String> = vec!["0".into()];
        for (on, code) in [
            (self.bold, "1"),
            (self.dim, "2"),
            (self.italic, "3"),
            (self.underline, "4"),
            (self.inverse, "7"),
        ] {
            if on {
                codes.push(code.into());
            }
        }
        for (colour, layer) in [(self.fg, 38), (self.bg, 48)] {
            if let Some([r, g, b]) = colour {
                codes.push(if truecolor {
                    format!("{layer};2;{r};{g};{b}")
                } else {
                    format!("{layer};5;{}", nearest_xterm([r, g, b]))
                });
            }
        }
        format!("\x1b[{}m", codes.join(";"))
    }
}

/// The xterm colour (16 to 255: the cube and the greys, which do not depend on the
/// terminal's theme) nearest to `rgb`.
fn nearest_xterm(rgb: [u8; 3]) -> u8 {
    let distance = |c: [u8; 3]| -> i32 {
        (0..3)
            .map(|i| {
                let d = i32::from(c[i]) - i32::from(rgb[i]);
                d * d
            })
            .sum()
    };
    (16..=255u8)
        .min_by_key(|&i| distance(crate::ansi::xterm_color(i)))
        .unwrap_or(16)
}

impl<W: Write> Printer<W> {
    fn new(out: W, palette: Option<Palette>, options: PrinterOptions) -> Self {
        Self {
            out,
            palette,
            options,
            last: None,
            printed: 0,
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }

    /// Writes one line: the separator when it does not follow the last one written, the
    /// prefixes, then the text without its escape sequences, coloured or not.
    fn line(
        &mut self,
        source: &Source<'_>,
        number: u64,
        raw: &str,
        truncated: bool,
    ) -> io::Result<()> {
        if self.options.separators {
            if let Some((id, last)) = self.last {
                if id != source.id || number != last + 1 {
                    self.write_marked("--", "36")?;
                    self.out.write_all(b"\n")?;
                }
            }
        }
        self.last = Some((source.id, number));
        if self.options.prefix {
            self.write_marked(source.name, "35")?;
            self.out.write_all(b":")?;
        }
        if self.options.line_numbers {
            self.write_marked(&number.to_string(), "32")?;
            self.out.write_all(b":")?;
        }
        if self.palette.is_none() {
            self.out.write_all(crate::ansi::strip(raw).as_bytes())?;
            if truncated {
                self.out.write_all(TRUNCATED_LINE_MARKER.as_bytes())?;
            }
        } else {
            let (mut text, runs) = crate::ansi::strip_and_style(raw);
            if truncated {
                text.push_str(TRUNCATED_LINE_MARKER);
            }
            self.write_coloured(&text, &runs)?;
        }
        self.out.write_all(b"\n")
    }

    /// `text` in the terminal's basic colour `code` when colouring (grep's colours for
    /// file names, line numbers and separators).
    fn write_marked(&mut self, text: &str, code: &str) -> io::Result<()> {
        if self.palette.is_some() {
            write!(self.out, "\x1b[{code}m{text}\x1b[0m")
        } else {
            self.out.write_all(text.as_bytes())
        }
    }

    /// `text` with the window's colours: the first matching rule (whole row, or its
    /// captures), else the level style, with the line's own ANSI colours below the rules.
    fn write_coloured(&mut self, text: &str, runs: &[StyleRun]) -> io::Result<()> {
        let Some(palette) = self.palette.as_ref() else {
            return self.out.write_all(text.as_bytes());
        };
        let highlight = highlight_spans(&palette.rules, &[], text, runs, palette.tokens);
        let base = match (
            &highlight.rest,
            palette
                .levels
                .then(|| palette.theme.level_style(detect_level(text)))
                .flatten(),
        ) {
            (Some(rule), _) => Sgr::of_rule(rule),
            (None, Some(level)) => Sgr {
                fg: Some(level.fg.rgb()),
                bg: (level.bg.a > 0).then(|| level.bg.rgb()),
                bold: level.bold,
                ..Default::default()
            },
            (None, None) => Sgr::default(),
        };
        let mut pieces: Vec<(usize, usize, Sgr)> = Vec::new();
        let mut pos = 0;
        for span in &highlight.spans {
            if span.start > pos {
                pieces.push((pos, span.start, base));
            }
            let style = match span.style {
                SpanStyle::Rule(rule) => Sgr::of_rule(&rule),
                SpanStyle::Ansi(ansi) => Sgr::of_ansi(&ansi, &base, palette.theme),
                SpanStyle::Label(_) => base,
                SpanStyle::Token(kind) => Sgr {
                    fg: Some(palette.theme.token_color(kind).rgb()),
                    ..base
                },
            };
            pieces.push((span.start, span.end, style));
            pos = span.end;
        }
        if pos < text.len() {
            pieces.push((pos, text.len(), base));
        }
        let truecolor = palette.truecolor;
        for (start, end, style) in pieces {
            let piece = &text[start..end];
            if style == Sgr::default() {
                self.out.write_all(piece.as_bytes())?;
            } else {
                let sequence = style.sequence(truecolor);
                self.out.write_all(sequence.as_bytes())?;
                self.out.write_all(piece.as_bytes())?;
                self.out.write_all(b"\x1b[0m")?;
            }
        }
        Ok(())
    }
}

/// Ctrl+C while following: a flag the follow loop checks, so the program ends with the
/// exit code earned so far instead of being killed.
mod interrupt {
    use std::sync::atomic::{AtomicBool, Ordering};

    static REQUESTED: AtomicBool = AtomicBool::new(false);

    pub fn requested() -> bool {
        REQUESTED.load(Ordering::Relaxed)
    }

    /// What the handler does, for the tests.
    #[cfg(test)]
    pub fn request() {
        REQUESTED.store(true, Ordering::Relaxed);
    }

    #[cfg(windows)]
    pub fn install() {
        #[link(name = "kernel32")]
        extern "system" {
            fn SetConsoleCtrlHandler(
                handler: Option<unsafe extern "system" fn(u32) -> i32>,
                add: i32,
            ) -> i32;
        }
        const CTRL_C_EVENT: u32 = 0;
        const CTRL_BREAK_EVENT: u32 = 1;
        unsafe extern "system" fn on_ctrl(kind: u32) -> i32 {
            if kind == CTRL_C_EVENT || kind == CTRL_BREAK_EVENT {
                REQUESTED.store(true, Ordering::Relaxed);
                1
            } else {
                0
            }
        }
        // SAFETY: registers a handler that only stores to an atomic.
        unsafe {
            SetConsoleCtrlHandler(Some(on_ctrl), 1);
        }
    }

    #[cfg(unix)]
    pub fn install() {
        use std::os::raw::c_int;
        extern "C" {
            fn signal(signum: c_int, handler: usize) -> usize;
        }
        const SIGINT: c_int = 2;
        extern "C" fn on_sigint(_: c_int) {
            REQUESTED.store(true, Ordering::Relaxed);
        }
        // SAFETY: the handler only stores to an atomic, which is async-signal-safe.
        unsafe {
            signal(SIGINT, on_sigint as extern "C" fn(c_int) as usize);
        }
    }

    #[cfg(not(any(windows, unix)))]
    pub fn install() {}
}

/// The console of a GUI-subsystem executable on Windows (nothing to do elsewhere).
pub mod console {
    #[cfg(windows)]
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Standard output or standard error was pointed at the parent's console by
    /// `attach_if_missing`: the shell did not wait for this GUI-subsystem program and has
    /// already printed its prompt above our output (see `release`).
    #[cfg(windows)]
    static WROTE_TO_PARENT: AtomicBool = AtomicBool::new(false);

    /// Makes standard output and standard error usable from a terminal: a handle that is
    /// already there (redirected to a file or a pipe) is used as it is; when one is
    /// missing — a GUI-subsystem program started from a console — the parent's console is
    /// attached and the missing handles point to it.
    #[cfg(windows)]
    pub fn attach_if_missing() {
        use std::os::windows::io::IntoRawHandle;
        let missing: Vec<u32> = [STD_OUTPUT_HANDLE, STD_ERROR_HANDLE]
            .into_iter()
            // SAFETY: plain Win32 query.
            .filter(|&id| is_missing(unsafe { GetStdHandle(id) }))
            .collect();
        if missing.is_empty() {
            return;
        }
        // SAFETY: plain Win32 call; failure (no parent console) leaves nothing to write to.
        if unsafe { AttachConsole(ATTACH_PARENT_PROCESS) } == 0 {
            return;
        }
        for id in missing {
            // SAFETY: plain Win32 query.
            if !is_missing(unsafe { GetStdHandle(id) }) {
                continue;
            }
            let Ok(conout) = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open("CONOUT$")
            else {
                continue;
            };
            // The handle stays open for the life of the process.
            let handle = conout.into_raw_handle();
            // SAFETY: `handle` is a valid console handle owned by nobody else.
            if unsafe { SetStdHandle(id, handle) } != 0 {
                WROTE_TO_PARENT.store(true, Ordering::Relaxed);
            }
        }
    }

    #[cfg(not(windows))]
    pub fn attach_if_missing() {}

    /// Called before exiting after writing to the parent's console. `cmd` and PowerShell
    /// do not wait for a GUI-subsystem program: their prompt is printed at once, our
    /// output lands after it, and the console looks as if it waited for a key. One Enter
    /// put in the console's input makes the shell print a fresh prompt below the output
    /// (an empty command in a shell that did wait, harmless). A no-op when nothing was
    /// written to the parent's console (output redirected, or launched from Explorer).
    #[cfg(windows)]
    pub fn release() {
        use std::os::windows::io::AsRawHandle;
        if !WROTE_TO_PARENT.swap(false, Ordering::Relaxed) {
            return;
        }
        use std::io::Write;
        let _ = std::io::stdout().flush();
        let _ = std::io::stderr().flush();
        let Ok(conin) = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("CONIN$")
        else {
            return;
        };
        const KEY_EVENT: u16 = 0x0001;
        const VK_RETURN: u16 = 0x0D;
        const ENTER_SCAN_CODE: u16 = 0x1C;
        let key = |down: i32| InputRecord {
            event_type: KEY_EVENT,
            key: KeyEventRecord {
                key_down: down,
                repeat_count: 1,
                virtual_key_code: VK_RETURN,
                virtual_scan_code: ENTER_SCAN_CODE,
                unicode_char: u16::from(b'\r'),
                control_key_state: 0,
            },
        };
        let records = [key(1), key(0)];
        let mut written = 0u32;
        // SAFETY: `records` is a valid array of `INPUT_RECORD`s for the call's duration and
        // `conin` an open console input handle.
        unsafe {
            WriteConsoleInputW(
                conin.as_raw_handle(),
                records.as_ptr(),
                records.len() as u32,
                &mut written,
            );
        }
    }

    #[cfg(not(windows))]
    pub fn release() {}

    /// `KEY_EVENT_RECORD`.
    #[cfg(windows)]
    #[repr(C)]
    struct KeyEventRecord {
        key_down: i32,
        repeat_count: u16,
        virtual_key_code: u16,
        virtual_scan_code: u16,
        unicode_char: u16,
        control_key_state: u32,
    }

    #[cfg(all(test, windows))]
    #[test]
    fn input_record_has_the_win32_layout() {
        assert_eq!(std::mem::size_of::<KeyEventRecord>(), 16);
        assert_eq!(std::mem::size_of::<InputRecord>(), 20);
        assert_eq!(std::mem::offset_of!(InputRecord, key), 4);
    }

    /// `INPUT_RECORD` holding a key event (the union's largest member is 16 bytes, as
    /// `KEY_EVENT_RECORD`).
    #[cfg(windows)]
    #[repr(C)]
    struct InputRecord {
        event_type: u16,
        key: KeyEventRecord,
    }

    /// Turns on virtual-terminal processing on a console standard output. `None` when
    /// standard output is not a console (nothing to turn on), else whether the
    /// sequences will be read.
    #[cfg(windows)]
    pub fn enable_virtual_terminal() -> Option<bool> {
        const ENABLE_VIRTUAL_TERMINAL_PROCESSING: u32 = 0x0004;
        // SAFETY: plain Win32 calls on the process's own standard output handle.
        unsafe {
            let handle = GetStdHandle(STD_OUTPUT_HANDLE);
            if is_missing(handle) {
                return None;
            }
            let mut mode = 0u32;
            if GetConsoleMode(handle, &mut mode) == 0 {
                return None;
            }
            if mode & ENABLE_VIRTUAL_TERMINAL_PROCESSING != 0 {
                return Some(true);
            }
            Some(SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING) != 0)
        }
    }

    #[cfg(not(windows))]
    pub fn enable_virtual_terminal() -> Option<bool> {
        None
    }

    #[cfg(windows)]
    type Handle = *mut std::ffi::c_void;
    #[cfg(windows)]
    const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
    #[cfg(windows)]
    const STD_ERROR_HANDLE: u32 = -12i32 as u32;
    #[cfg(windows)]
    const ATTACH_PARENT_PROCESS: u32 = u32::MAX;

    #[cfg(windows)]
    fn is_missing(handle: Handle) -> bool {
        handle.is_null() || handle as isize == -1
    }

    #[cfg(windows)]
    #[link(name = "kernel32")]
    extern "system" {
        fn GetStdHandle(id: u32) -> Handle;
        fn SetStdHandle(id: u32, handle: Handle) -> i32;
        fn AttachConsole(process_id: u32) -> i32;
        fn GetConsoleMode(handle: Handle, mode: *mut u32) -> i32;
        fn SetConsoleMode(handle: Handle, mode: u32) -> i32;
        fn WriteConsoleInputW(
            input: Handle,
            records: *const InputRecord,
            count: u32,
            written: *mut u32,
        ) -> i32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cli(args: &[&str]) -> CliArgs {
        let mut all = vec!["--print"];
        all.extend_from_slice(args);
        CliArgs::parse(all, Path::new(".")).unwrap()
    }

    /// Runs `text` through the splitter and the pipeline as one input.
    fn print_text(args: &[&str], text: &[u8], chunk: usize) -> (String, u64) {
        let cli = cli(args);
        let matcher = Matcher::from_cli(&cli);
        let options = PrinterOptions {
            prefix: false,
            line_numbers: cli.line_numbers,
            separators: matcher.context > 0,
        };
        let mut printer = Printer::new(Vec::new(), None, options);
        let mut splitter = Splitter::default();
        let mut pipeline = Pipeline::default();
        let source = Source { name: "t", id: 0 };
        for part in text.chunks(chunk.max(1)) {
            splitter
                .feed(part, |b, e, t| {
                    pipeline.process(&matcher, &mut printer, &source, b, e, t)
                })
                .unwrap();
        }
        splitter
            .finish(|b, e, t| pipeline.process(&matcher, &mut printer, &source, b, e, t))
            .unwrap();
        let printed = printer.printed;
        (String::from_utf8(printer.out).unwrap(), printed)
    }

    const LOG: &str = "\
2026-09-18 14:00:00 INFO start
2026-09-18 14:02:10 ERROR payment timeout
    at Gateway.call(Gateway.java:12)
2026-09-18 14:03:00 WARN healthcheck payment slow
2026-09-18 14:05:30 ERROR payment refused
2026-09-18 14:06:00 INFO done
";

    #[test]
    fn filters_follow_their_stack_traces_in_any_chunking() {
        for chunk in [1, 2, 7, 4096] {
            let (out, n) = print_text(&["--filter", "ERROR"], LOG.as_bytes(), chunk);
            assert_eq!(
                out,
                "2026-09-18 14:02:10 ERROR payment timeout\n    at Gateway.call(Gateway.java:12)\n2026-09-18 14:05:30 ERROR payment refused\n",
                "chunk {chunk}"
            );
            assert_eq!(n, 3);
        }
    }

    #[test]
    fn include_terms_and_exclude_terms() {
        let (out, _) = print_text(
            &[
                "--filter",
                "payment",
                "--filter",
                "e",
                "--exclude",
                "healthcheck",
            ],
            LOG.as_bytes(),
            64,
        );
        // Both terms must match, the excluded WARN line is gone, the stack trace follows.
        assert_eq!(
            out,
            "2026-09-18 14:02:10 ERROR payment timeout\n    at Gateway.call(Gateway.java:12)\n2026-09-18 14:05:30 ERROR payment refused\n"
        );
        let (out, n) = print_text(&["--filter", "no-such-text"], LOG.as_bytes(), 64);
        assert_eq!((out.as_str(), n), ("", 0));
    }

    #[test]
    fn level_and_time_window() {
        let (out, _) = print_text(&["--level", "warn"], LOG.as_bytes(), 64);
        assert_eq!(out.lines().count(), 4, "{out}");
        // A bare time on the day of the first timestamp; the end covers its minute.
        let (out, _) = print_text(
            &["--since", "14:02", "--until", "14:03"],
            LOG.as_bytes(),
            64,
        );
        assert_eq!(
            out,
            "2026-09-18 14:02:10 ERROR payment timeout\n    at Gateway.call(Gateway.java:12)\n2026-09-18 14:03:00 WARN healthcheck payment slow\n"
        );
        // Relative: this log is years old, so nothing is in the last hour.
        let (out, _) = print_text(&["--since", "-1h"], LOG.as_bytes(), 64);
        assert_eq!(out, "");
    }

    #[test]
    fn a_bare_time_is_on_the_day_of_the_first_timestamp_whatever_the_filter() {
        // Day one has no ERROR; the first ERROR is on day two. `--since 23:55` means 23:55
        // of day one (the first timestamp of the log), as in the window: both ERROR lines
        // are after it.
        let text = "\
2026-09-18 23:50:00 INFO day one
2026-09-18 23:56:00 INFO late
2026-09-19 00:10:00 ERROR day two
2026-09-19 23:58:00 ERROR late on day two
";
        let (out, _) = print_text(
            &["--filter", "ERROR", "--since", "23:55"],
            text.as_bytes(),
            64,
        );
        assert_eq!(
            out,
            "2026-09-19 00:10:00 ERROR day two\n2026-09-19 23:58:00 ERROR late on day two\n"
        );
        // The first ERROR just after midnight is after 23:55 of the day before.
        let text = "\
2026-09-18 23:50:00 INFO before
2026-09-19 00:05:00 ERROR after midnight
2026-09-19 00:10:00 INFO later
";
        let (out, n) = print_text(
            &["--filter", "ERROR", "--since", "23:55"],
            text.as_bytes(),
            5,
        );
        assert_eq!(out, "2026-09-19 00:05:00 ERROR after midnight\n");
        assert_eq!(n, 1);
    }

    #[test]
    fn a_relative_until_is_exact_to_the_millisecond() {
        // `--until now` must not reach past the current instant, and `-1h30m` is read.
        let cli = cli(&["--since", "-1h30m", "--until", "now"]);
        let matcher = Matcher::from_cli(&cli);
        let (from, to) = matcher.window(0);
        assert_eq!(to, Some(matcher.now));
        assert_eq!(from, Some(matcher.now - 90 * 60_000));
    }

    fn follow_input(path: &Path) -> (Input, u64) {
        let mut next_id = 0;
        let input = Input::open(InputSpec::Path(path.to_path_buf()), true, &mut next_id)
            .unwrap_or_else(|e| panic!("{e}"));
        (input, next_id)
    }

    fn plain_printer() -> Printer<Vec<u8>> {
        let options = PrinterOptions {
            prefix: false,
            line_numbers: false,
            separators: false,
        };
        Printer::new(Vec::new(), None, options)
    }

    #[test]
    fn a_truncation_first_prints_the_held_last_line() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.log");
        std::fs::write(&path, "one ERROR\ntwo ERROR without newline").unwrap();
        let matcher = Matcher::from_cli(&cli(&["--filter", "ERROR"]));
        let mut printer = plain_printer();
        let (mut input, mut next_id) = follow_input(&path);
        input.pump(&matcher, &mut printer, false).ok().unwrap();
        assert_eq!(String::from_utf8_lossy(&printer.out), "one ERROR\n");
        std::fs::write(&path, "new ERROR\n").unwrap();
        input
            .poll(&matcher, &mut printer, &mut next_id)
            .ok()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&printer.out),
            "one ERROR\ntwo ERROR without newline\nnew ERROR\n"
        );
    }

    #[test]
    fn a_restart_on_a_file_that_cannot_be_opened_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.log");
        std::fs::write(&path, "one\ntwo held").unwrap();
        let matcher = Matcher::from_cli(&cli(&[]));
        let mut printer = plain_printer();
        let (mut input, mut next_id) = follow_input(&path);
        input.pump(&matcher, &mut printer, false).ok().unwrap();
        let (name, id) = (input.name.clone(), input.id);
        let missing = dir.path().join("app-2.log");
        assert!(matches!(
            input.restart(missing, &matcher, &mut printer, &mut next_id),
            Err(Stop::Input(_))
        ));
        // Same file, same stream, the partial line still held for its newline.
        assert_eq!((input.name.as_str(), input.id), (name.as_str(), id));
        assert_eq!(input.follow.as_ref().unwrap().path, path);
        assert_eq!(String::from_utf8_lossy(&printer.out), "one\n");
        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b" done\n")
            .unwrap();
        input
            .poll(&matcher, &mut printer, &mut next_id)
            .ok()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&printer.out),
            "one\ntwo held done\n"
        );
    }

    #[test]
    fn ctrl_c_ends_the_follow_loop_with_the_output_flushed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.log");
        std::fs::write(&path, "one\n").unwrap();
        let matcher = Matcher::from_cli(&cli(&[]));
        let mut printer = plain_printer();
        let (input, mut next_id) = follow_input(&path);
        let mut inputs = vec![input];
        interrupt::request();
        let started = Instant::now();
        follow_inputs(&mut inputs, &matcher, &mut printer, &mut next_id).unwrap();
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn lines_before_the_first_timestamp_are_hidden_by_a_window() {
        let text = "banner\n2026-09-18 14:02:10 INFO a\nnext\n";
        let (out, _) = print_text(&["--since", "2026-09-18"], text.as_bytes(), 64);
        assert_eq!(out, "2026-09-18 14:02:10 INFO a\nnext\n");
        let (out, _) = print_text(&[], text.as_bytes(), 64);
        assert_eq!(out, text);
    }

    #[test]
    fn context_groups_are_separated() {
        let text: String = (1..=12).map(|i| format!("line {i}\n")).collect();
        let (out, n) = print_text(
            &[
                "--filter",
                "line 3",
                "--filter",
                "3",
                "--context",
                "1",
                "--line-numbers",
            ],
            text.as_bytes(),
            5,
        );
        assert_eq!(out, "2:line 2\n3:line 3\n4:line 4\n");
        assert_eq!(n, 1);
        let (out, _) = print_text(
            &["--regex", "--filter", "^line (2|4|9)$", "--context", "1"],
            text.as_bytes(),
            5,
        );
        assert_eq!(
            out,
            "line 1\nline 2\nline 3\nline 4\nline 5\n--\nline 8\nline 9\nline 10\n"
        );
    }

    #[test]
    fn crlf_last_line_without_newline_and_escapes() {
        let text = b"a\r\nb \x1b[31mred\x1b[0m\r\nlast";
        let (out, n) = print_text(&[], text, 3);
        assert_eq!(out, "a\nb red\nlast\n");
        assert_eq!(n, 3);
    }

    #[test]
    fn utf16_with_bom_and_odd_chunks() {
        let mut bytes = vec![0xFF, 0xFE];
        for unit in "één\nERROR due\n".encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        for chunk in [1, 3, 5, 100] {
            let (out, _) = print_text(&["--filter", "error"], &bytes, chunk);
            assert_eq!(out, "ERROR due\n", "chunk {chunk}");
        }
    }

    #[test]
    fn long_lines_are_cut_with_the_marker() {
        let mut text = vec![b'x'; MAX_LINE_BYTES + 10];
        text.extend_from_slice(b"\nshort\n");
        let (out, _) = print_text(&[], &text, 100_000);
        let mut lines = out.lines();
        let first = lines.next().unwrap();
        assert_eq!(first.len(), MAX_LINE_BYTES + TRUNCATED_LINE_MARKER.len());
        assert!(first.ends_with(TRUNCATED_LINE_MARKER));
        assert_eq!(lines.next(), Some("short"));
    }

    #[test]
    fn colour_uses_rules_levels_and_the_lines_own_ansi() {
        let palette = Palette {
            theme: CyberTheme::Tron,
            rules: vec![CompiledHighlight::compile(
                &crate::tail_engine::HighlightRule::new("timeout", [255, 0, 0], [0, 0, 0], false),
            )],
            truecolor: true,
            levels: true,
            tokens: crate::auto_highlight::TokenKinds::NONE,
        };
        let mut printer = Printer::new(Vec::new(), Some(palette), PrinterOptions::default());
        let source = Source { name: "t", id: 0 };
        printer
            .line(&source, 1, "ERROR payment timeout", false)
            .unwrap();
        printer.line(&source, 2, "ERROR refused", false).unwrap();
        printer
            .line(&source, 3, "plain \x1b[32mgreen\x1b[0m", false)
            .unwrap();
        printer.line(&source, 4, "plain", false).unwrap();
        let out = String::from_utf8(printer.out).unwrap();
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(
            lines[0],
            "\x1b[0;38;2;255;0;0;48;2;0;0;0mERROR payment timeout\x1b[0m"
        );
        let error = CyberTheme::Tron.level_color(LogLevel::Error).rgb();
        assert_eq!(
            lines[1],
            format!(
                "\x1b[0;38;2;{};{};{}mERROR refused\x1b[0m",
                error[0], error[1], error[2]
            )
        );
        let green = CyberTheme::Tron.ansi_color(AnsiColor::Indexed(2)).rgb();
        assert_eq!(
            lines[2],
            format!(
                "plain \x1b[0;38;2;{};{};{}mgreen\x1b[0m",
                green[0], green[1], green[2]
            )
        );
        assert_eq!(lines[3], "plain");
    }

    #[test]
    fn automatic_tokens_are_coloured_below_the_rules() {
        use crate::auto_highlight::{TokenKind, TokenKinds};
        let palette = Palette {
            theme: CyberTheme::Tron,
            rules: vec![CompiledHighlight::compile(
                &crate::tail_engine::HighlightRule::new("refused", [255, 0, 0], [0, 0, 0], false),
            )],
            truecolor: true,
            levels: false,
            tokens: TokenKinds::ALL,
        };
        let mut printer = Printer::new(Vec::new(), Some(palette), PrinterOptions::default());
        let source = Source { name: "t", id: 0 };
        printer.line(&source, 1, "from 10.0.0.1 ok", false).unwrap();
        printer.line(&source, 2, "10.0.0.1 refused", false).unwrap();
        let out = String::from_utf8(printer.out).unwrap();
        let lines: Vec<&str> = out.lines().collect();
        let ip = CyberTheme::Tron.token_color(TokenKind::Ip).rgb();
        assert_eq!(
            lines[0],
            format!(
                "from [0;38;2;{};{};{}m10.0.0.1[0m ok",
                ip[0], ip[1], ip[2]
            )
        );
        // A whole-row rule wins over the token.
        assert_eq!(lines[1], "[0;38;2;255;0;0;48;2;0;0;0m10.0.0.1 refused[0m");
    }

    #[test]
    fn nearest_xterm_colours() {
        assert_eq!(nearest_xterm([255, 0, 0]), 196);
        assert_eq!(nearest_xterm([0, 0, 0]), 16);
        assert_eq!(nearest_xterm([128, 128, 128]), 244);
        let sgr = Sgr {
            fg: Some([255, 0, 0]),
            bold: true,
            ..Default::default()
        };
        assert_eq!(sgr.sequence(false), "\x1b[0;1;38;5;196m");
    }

    #[test]
    fn exit_codes() {
        assert_eq!(exit_code(3, false), EXIT_PRINTED);
        assert_eq!(exit_code(0, false), EXIT_NO_MATCH);
        assert_eq!(exit_code(3, true), EXIT_INPUT_ERROR);
        assert_eq!(
            output_failed(&io::Error::from(io::ErrorKind::BrokenPipe)),
            EXIT_PRINTED
        );
    }
}
