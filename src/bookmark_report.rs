//! Bookmark tags and the Markdown bookmark report.
//!
//! A tag is a `#word` inside a bookmark note: nothing new is stored, so tags are saved,
//! restored and edited exactly as notes are. The report turns the bookmarks of one or
//! more streams into a Markdown write-up: per bookmark its line number, timestamp, note
//! and tags, and the line with its context in a fenced block. The context is read in
//! bounded steps (`ReportJob::step`), so a large report never stalls a frame; this module
//! knows nothing of the interface or of the engine.

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

/// Longest tag, `#` excluded.
pub const MAX_TAG_CHARS: usize = 32;
/// Longest line written in a report; longer ones are cut with `…`.
pub const MAX_REPORT_LINE_CHARS: usize = 2_000;
/// Automatic bookmarks written per stream at most; the rest are counted.
pub const MAX_AUTO_BOOKMARKS: usize = 1_000;
/// Context lines around each bookmark at most.
pub const MAX_REPORT_CONTEXT: usize = 20;
/// Largest report offered to the clipboard.
pub const MAX_CLIPBOARD_BYTES: usize = 4 * 1024 * 1024;

fn is_tag_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '-' | '_' | '.')
}

/// The tag a `#word` names, lowercase and without trailing dots, when it is one: 1 to
/// `MAX_TAG_CHARS` characters from letters, digits, `-`, `_`, `.`, at least one letter.
fn tag_of(word: &str) -> Option<String> {
    let body = word.trim_end_matches('.');
    let count = body.chars().count();
    if count == 0
        || count > MAX_TAG_CHARS
        || !body.chars().all(is_tag_char)
        || !body.chars().any(char::is_alphabetic)
    {
        return None;
    }
    Some(body.to_lowercase())
}

/// The tags of a note in order of appearance, lowercase, each once: every `#word` at the
/// start of the note or after a space, up to the first character a tag cannot hold
/// (`#deploy(prod)` → `deploy`). `issue #42` has none (a tag needs a letter), and `a#b`
/// neither (the `#` must start a word).
pub fn parse_tags(note: &str) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    for word in note.split_whitespace() {
        let Some(rest) = word.strip_prefix('#') else {
            continue;
        };
        // The tag stops at the first character that cannot be part of it (`#db,`).
        let end = rest
            .char_indices()
            .find(|&(_, c)| !is_tag_char(c))
            .map_or(rest.len(), |(i, _)| i);
        if let Some(tag) = tag_of(&rest[..end]) {
            if !tags.contains(&tag) {
                tags.push(tag);
            }
        }
    }
    tags
}

/// A tag typed by the user (`#Deploy`, `deploy`), normalized as `parse_tags` returns it.
pub fn normalize_tag(input: &str) -> Option<String> {
    let input = input.trim();
    tag_of(input.strip_prefix('#').unwrap_or(input))
}

/// How the report orders its bookmarks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReportOrder {
    /// Streams in dock order, bookmarks in line order.
    #[default]
    Stream,
    /// One list across streams by timestamp; untimed bookmarks last.
    Time,
}

impl ReportOrder {
    pub fn as_str(self) -> &'static str {
        match self {
            ReportOrder::Stream => "stream",
            ReportOrder::Time => "time",
        }
    }

    pub fn parse(text: &str) -> Self {
        match text.trim() {
            "time" => ReportOrder::Time,
            _ => ReportOrder::Stream,
        }
    }
}

/// What the report covers and how it is laid out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportOptions {
    /// Context lines before and after each bookmark (at most `MAX_REPORT_CONTEXT`).
    pub context: usize,
    /// Automatic bookmarks too (at most `MAX_AUTO_BOOKMARKS` per stream).
    pub include_auto: bool,
    /// Only bookmarks carrying at least one of these tags (normalized); empty = all.
    pub tags: Vec<String>,
    pub order: ReportOrder,
}

impl Default for ReportOptions {
    fn default() -> Self {
        Self {
            context: 3,
            include_auto: false,
            tags: Vec::new(),
            order: ReportOrder::Stream,
        }
    }
}

/// One bookmark as the report shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportBookmark {
    /// 0-based line index (written 1-based).
    pub line: usize,
    /// Timestamp of the line in milliseconds, for the time order and the span.
    pub millis: Option<i64>,
    /// The timestamp as the view shows it.
    pub time_text: Option<String>,
    pub note: Option<String>,
    pub tags: Vec<String>,
    pub automatic: bool,
}

/// The bookmarks of one stream, as collected on the interface side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportStream {
    /// Tab title.
    pub title: String,
    /// File path, archive entry or `stdin`.
    pub path: String,
    /// Lines of the stream (the context is clamped to them).
    pub total_lines: usize,
    /// In line order.
    pub bookmarks: Vec<ReportBookmark>,
    /// Automatic bookmarks past `MAX_AUTO_BOOKMARKS`, not written.
    pub auto_left_out: usize,
}

/// Picks the bookmarks the options ask for: manual ones with their notes and tags, the
/// automatic ones when included (the first `MAX_AUTO_BOOKMARKS` in line order), those
/// carrying a chosen tag when a tag filter is set. Returns the bookmarks in line order
/// and how many automatic ones were left out by the cap.
pub fn select_bookmarks(
    manual: &BTreeSet<usize>,
    automatic: &BTreeSet<usize>,
    notes: &BTreeMap<usize, String>,
    options: &ReportOptions,
) -> (Vec<(usize, bool)>, usize) {
    let tagged = |line: usize| -> bool {
        options.tags.is_empty()
            || notes
                .get(&line)
                .is_some_and(|note| parse_tags(note).iter().any(|t| options.tags.contains(t)))
    };
    let mut picked: Vec<(usize, bool)> = manual
        .iter()
        .filter(|&&l| tagged(l))
        .map(|&l| (l, false))
        .collect();
    let mut left_out = 0;
    // Automatic bookmarks carry no note, hence no tag: a tag filter leaves them out.
    if options.include_auto && options.tags.is_empty() {
        let autos: Vec<usize> = automatic
            .iter()
            .copied()
            .filter(|l| !manual.contains(l))
            .collect();
        left_out = autos.len().saturating_sub(MAX_AUTO_BOOKMARKS);
        picked.extend(
            autos
                .into_iter()
                .take(MAX_AUTO_BOOKMARKS)
                .map(|l| (l, true)),
        );
        picked.sort_unstable();
    }
    (picked, left_out)
}

/// One fenced block: the lines `first..=last` of a stream and the bookmarks inside it.
#[derive(Debug, Clone)]
struct Block {
    stream: usize,
    first: usize,
    last: usize,
    /// Indices into the stream's bookmarks.
    bookmarks: Vec<usize>,
}

/// A report being built: the lines to read, read in bounded steps, then the Markdown.
pub struct ReportJob {
    streams: Vec<ReportStream>,
    options: ReportOptions,
    blocks: Vec<Block>,
    /// `(stream, line)` still to read, in order.
    pending: Vec<(usize, usize)>,
    next: usize,
    /// Text of every line read, `None` past the end of a rewritten file.
    lines: BTreeMap<(usize, usize), Option<String>>,
    cancelled: bool,
}

impl ReportJob {
    /// Plans the report: merges overlapping context into blocks (so no line is written
    /// twice) and lists the lines to read. Streams without bookmarks are dropped.
    pub fn new(streams: Vec<ReportStream>, options: ReportOptions) -> Self {
        let streams: Vec<ReportStream> = streams
            .into_iter()
            .filter(|s| !s.bookmarks.is_empty())
            .collect();
        let context = options.context.min(MAX_REPORT_CONTEXT);
        // Every bookmark with its context range, in the order they are written.
        let mut entries: Vec<(usize, usize)> = streams
            .iter()
            .enumerate()
            .flat_map(|(s, stream)| (0..stream.bookmarks.len()).map(move |b| (s, b)))
            .collect();
        if options.order == ReportOrder::Time {
            entries.sort_by_key(|&(s, b)| {
                let bm = &streams[s].bookmarks[b];
                (bm.millis.is_none(), bm.millis.unwrap_or(0), s, bm.line)
            });
        }
        let mut blocks: Vec<Block> = Vec::new();
        for (s, b) in entries {
            let stream = &streams[s];
            let line = stream.bookmarks[b].line;
            let first = line.saturating_sub(context);
            let last = (line + context).min(stream.total_lines.saturating_sub(1).max(line));
            match blocks.last_mut() {
                // Consecutive entries of the same stream whose ranges touch share a block
                // (in time order the later one may sit on an earlier line).
                Some(prev)
                    if prev.stream == s && first <= prev.last + 1 && last + 1 >= prev.first =>
                {
                    prev.first = prev.first.min(first);
                    prev.last = prev.last.max(last);
                    prev.bookmarks.push(b);
                }
                _ => blocks.push(Block {
                    stream: s,
                    first,
                    last,
                    bookmarks: vec![b],
                }),
            }
        }
        let mut seen = BTreeSet::new();
        let pending: Vec<(usize, usize)> = blocks
            .iter()
            .flat_map(|blk| (blk.first..=blk.last).map(move |l| (blk.stream, l)))
            .filter(|key| seen.insert(*key))
            .collect();
        Self {
            streams,
            options,
            blocks,
            pending,
            next: 0,
            lines: BTreeMap::new(),
            cancelled: false,
        }
    }

    /// Lines to read in all.
    pub fn total_lines(&self) -> usize {
        self.pending.len()
    }

    /// Share of the lines read, `0..=1`.
    pub fn progress(&self) -> f32 {
        if self.pending.is_empty() {
            1.0
        } else {
            self.next as f32 / self.pending.len() as f32
        }
    }

    pub fn is_done(&self) -> bool {
        self.next >= self.pending.len()
    }

    pub fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    /// Streams the report covers (those with bookmarks).
    pub fn stream_count(&self) -> usize {
        self.streams.len()
    }

    /// Bookmarks the report covers.
    pub fn bookmark_count(&self) -> usize {
        self.streams.iter().map(|s| s.bookmarks.len()).sum()
    }

    /// Reads lines with `read(stream, line)` until they are all read or `budget` is
    /// spent. Returns whether every line has been read.
    pub fn step(
        &mut self,
        budget: Duration,
        mut read: impl FnMut(usize, usize) -> Option<String>,
    ) -> bool {
        let started = Instant::now();
        while self.next < self.pending.len() && !self.cancelled {
            let (s, l) = self.pending[self.next];
            let text = read(s, l);
            self.lines.insert((s, l), text);
            self.next += 1;
            // Check the clock every few lines: `Instant::now` is not free.
            if self.next.is_multiple_of(64) && started.elapsed() >= budget {
                break;
            }
        }
        self.is_done()
    }

    /// The Markdown report. `generated` is the creation time as shown, `version` the
    /// FastTail version. Call once every line has been read (`step` returned `true`).
    pub fn markdown(&self, generated: &str, version: &str) -> String {
        let mut out = String::new();
        out.push_str("# FastTail bookmark report\n\n");
        let bookmarks = self.bookmark_count();
        let mut summary = format!(
            "Generated {generated} by FastTail {version} · {} stream{} · {} bookmark{}",
            self.streams.len(),
            if self.streams.len() == 1 { "" } else { "s" },
            bookmarks,
            if bookmarks == 1 { "" } else { "s" },
        );
        let timed = self
            .streams
            .iter()
            .flat_map(|s| s.bookmarks.iter())
            .filter_map(|b| b.millis.zip(b.time_text.as_deref()));
        let first = timed.clone().min_by_key(|&(m, _)| m);
        let last = timed.max_by_key(|&(m, _)| m);
        if let (Some((_, from)), Some((_, to))) = (first, last) {
            summary.push_str(&format!(" · {from} – {to}"));
        }
        out.push_str(&summary);
        out.push_str("\n\n");
        let mut tag_counts: BTreeMap<&str, usize> = BTreeMap::new();
        for b in self.streams.iter().flat_map(|s| s.bookmarks.iter()) {
            for tag in &b.tags {
                *tag_counts.entry(tag.as_str()).or_default() += 1;
            }
        }
        if !tag_counts.is_empty() {
            let list: Vec<String> = tag_counts
                .iter()
                .map(|(tag, n)| format!("#{tag} ({n})"))
                .collect();
            out.push_str(&format!("Tags: {}\n\n", list.join(", ")));
        }
        if !self.options.tags.is_empty() {
            let wanted: Vec<String> = self.options.tags.iter().map(|t| format!("#{t}")).collect();
            out.push_str(&format!("Only bookmarks tagged {}\n\n", wanted.join(", ")));
        }

        let by_time = self.options.order == ReportOrder::Time;
        let mut current_stream: Option<usize> = None;
        let mut previous_millis: Option<i64> = None;
        for block in &self.blocks {
            let stream = &self.streams[block.stream];
            if !by_time && current_stream != Some(block.stream) {
                current_stream = Some(block.stream);
                out.push_str(&format!("## {}\n\n`{}`\n\n", stream.title, stream.path));
                if stream.auto_left_out > 0 {
                    out.push_str(&format!(
                        "{} more automatic bookmarks left out (first {} listed)\n\n",
                        group(stream.auto_left_out),
                        group(MAX_AUTO_BOOKMARKS)
                    ));
                }
            }
            for &b in &block.bookmarks {
                let bm = &stream.bookmarks[b];
                let mut heading = format!("### Line {}", group(bm.line + 1));
                if by_time {
                    heading.push_str(&format!(" · {}", stream.title));
                }
                if let Some(time) = &bm.time_text {
                    heading.push_str(&format!(" · {time}"));
                }
                if by_time {
                    if let (Some(now), Some(before)) = (bm.millis, previous_millis) {
                        heading.push_str(&format!(" (+{})", format_gap(now - before)));
                    }
                    if bm.millis.is_some() {
                        previous_millis = bm.millis;
                    }
                }
                if bm.automatic {
                    heading.push_str(" · automatic");
                }
                if let Some(note) = bm.note.as_deref().filter(|n| !n.trim().is_empty()) {
                    heading.push_str(&format!(" · {}", note.trim()));
                }
                out.push_str(&heading);
                out.push('\n');
            }
            self.write_block(&mut out, block);
        }
        out
    }

    fn write_block(&self, out: &mut String, block: &Block) {
        let stream = &self.streams[block.stream];
        let marked: BTreeSet<usize> = block
            .bookmarks
            .iter()
            .map(|&b| stream.bookmarks[b].line)
            .collect();
        let width = (block.last + 1).to_string().len();
        let texts: Vec<(usize, String)> = (block.first..=block.last)
            .filter_map(|l| {
                let text = self.lines.get(&(block.stream, l))?;
                // Lines past the end of the file exist only as the bookmark itself.
                if text.is_none() && l >= stream.total_lines && !marked.contains(&l) {
                    return None;
                }
                Some((
                    l,
                    text.as_deref()
                        .map_or_else(|| "(line unavailable)".to_string(), cut_line),
                ))
            })
            .collect();
        let longest_run = texts
            .iter()
            .map(|(_, t)| longest_backtick_run(t))
            .max()
            .unwrap_or(0);
        let fence = "`".repeat((longest_run + 1).max(3));
        out.push_str(&fence);
        out.push_str("text\n");
        for (l, text) in texts {
            let mark = if marked.contains(&l) { '>' } else { ' ' };
            out.push_str(&format!("{mark} {:>width$} | {text}\n", l + 1));
        }
        out.push_str(&fence);
        out.push_str("\n\n");
    }
}

/// A line cut at `MAX_REPORT_LINE_CHARS` characters, with `…` when cut.
fn cut_line(text: &str) -> String {
    match text.char_indices().nth(MAX_REPORT_LINE_CHARS) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text.to_string(),
    }
}

fn longest_backtick_run(text: &str) -> usize {
    let mut best = 0;
    let mut run = 0;
    for c in text.chars() {
        if c == '`' {
            run += 1;
            best = best.max(run);
        } else {
            run = 0;
        }
    }
    best
}

/// `12,345`.
fn group(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Time between two bookmarks: `850ms`, `42s`, `3m 12s`, `2h 05m`, `3d 04h`.
fn format_gap(millis: i64) -> String {
    let millis = millis.max(0);
    let secs = millis / 1000;
    match secs {
        0 => format!("{millis}ms"),
        1..=59 => format!("{secs}s"),
        60..=3_599 => format!("{}m {:02}s", secs / 60, secs % 60),
        3_600..=86_399 => format!("{}h {:02}m", secs / 3600, (secs / 60) % 60),
        _ => format!("{}d {:02}h", secs / 86_400, (secs / 3600) % 24),
    }
}

/// Name of the saved report: `fasttail-report-2026-09-29.md`.
pub fn report_file_name(date: &str) -> String {
    format!("fasttail-report-{date}.md")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_are_hash_words_with_a_letter() {
        assert_eq!(parse_tags("#deploy start"), vec!["deploy"]);
        assert_eq!(
            parse_tags("OOM after #Deploy and #db-pool."),
            vec!["deploy", "db-pool"]
        );
        assert_eq!(parse_tags("see #db. and #db, again"), vec!["db"]);
        assert!(parse_tags("issue #42").is_empty(), "a tag needs a letter");
        assert!(parse_tags("a#b").is_empty(), "the # must start a word");
        assert!(parse_tags("# alone").is_empty());
        let long = "a".repeat(MAX_TAG_CHARS);
        assert_eq!(parse_tags(&format!("#{long}")), vec![long.clone()]);
        assert!(parse_tags(&format!("#{long}b")).is_empty(), "33 characters");
        assert_eq!(parse_tags("#café #Überlast"), vec!["café", "überlast"]);
        assert_eq!(parse_tags("#v1.2 #a_b"), vec!["v1.2", "a_b"]);
        assert_eq!(
            parse_tags("#a/b #deploy(prod) #oom's"),
            vec!["a", "deploy", "oom"]
        );
    }

    #[test]
    fn typed_tags_normalize() {
        assert_eq!(normalize_tag("#Deploy"), Some("deploy".into()));
        assert_eq!(normalize_tag(" deploy "), Some("deploy".into()));
        assert_eq!(normalize_tag("#42"), None);
        assert_eq!(normalize_tag("#"), None);
    }

    fn bm(line: usize, millis: Option<i64>, note: Option<&str>) -> ReportBookmark {
        ReportBookmark {
            line,
            millis,
            time_text: millis.map(|m| format!("t{m}")),
            note: note.map(str::to_string),
            tags: note.map(parse_tags).unwrap_or_default(),
            automatic: false,
        }
    }

    fn stream(title: &str, total: usize, bookmarks: Vec<ReportBookmark>) -> ReportStream {
        ReportStream {
            title: title.into(),
            path: format!("/logs/{title}"),
            total_lines: total,
            bookmarks,
            auto_left_out: 0,
        }
    }

    fn build(streams: Vec<ReportStream>, options: ReportOptions) -> String {
        let texts: Vec<Vec<String>> = streams
            .iter()
            .map(|s| {
                (0..s.total_lines)
                    .map(|l| format!("{} line {}", s.title, l + 1))
                    .collect()
            })
            .collect();
        let mut job = ReportJob::new(streams, options);
        assert!(job.step(Duration::from_secs(5), |s, l| texts[s].get(l).cloned()));
        job.markdown("2026-09-29 10:00", "0.13.0")
    }

    #[test]
    fn one_stream_golden() {
        let md = build(
            vec![stream(
                "app.log",
                20,
                vec![bm(9, Some(5_000), Some("retry storm #deploy"))],
            )],
            ReportOptions {
                context: 1,
                ..Default::default()
            },
        );
        let want = "# FastTail bookmark report\n\n\
Generated 2026-09-29 10:00 by FastTail 0.13.0 · 1 stream · 1 bookmark · t5000 – t5000\n\n\
Tags: #deploy (1)\n\n\
## app.log\n\n`/logs/app.log`\n\n\
### Line 10 · t5000 · retry storm #deploy\n\
```text\n\
\x20  9 | app.log line 9\n\
> 10 | app.log line 10\n\
\x20 11 | app.log line 11\n\
```\n\n";
        assert_eq!(md, want);
    }

    #[test]
    fn overlapping_context_is_one_block_and_clamped_at_the_ends() {
        let md = build(
            vec![stream(
                "a",
                6,
                vec![bm(0, None, None), bm(2, None, None), bm(5, None, None)],
            )],
            ReportOptions {
                context: 1,
                ..Default::default()
            },
        );
        assert_eq!(md.matches("```text").count(), 1, "{md}");
        assert_eq!(md.matches("a line 2\n").count(), 1, "no line written twice");
        assert!(
            md.contains("> 1 | a line 1\n") && md.contains("> 6 | a line 6\n"),
            "{md}"
        );
    }

    #[test]
    fn time_order_merges_a_later_bookmark_on_an_earlier_line() {
        let md = build(
            vec![stream(
                "a",
                200,
                vec![
                    bm(97, Some(2), Some("second")),
                    bm(99, Some(1), Some("first")),
                ],
            )],
            ReportOptions {
                context: 3,
                order: ReportOrder::Time,
                ..Default::default()
            },
        );
        assert_eq!(md.matches("```text").count(), 1, "{md}");
        assert!(
            md.contains(
                "   95 | a line 95
"
            ),
            "context of the earlier line kept: {md}"
        );
        assert_eq!(
            md.matches(
                "a line 99
"
            )
            .count(),
            1,
            "{md}"
        );
    }

    #[test]
    fn a_line_with_backticks_lengthens_the_fence() {
        let streams = vec![stream("a", 1, vec![bm(0, None, None)])];
        let mut job = ReportJob::new(streams, ReportOptions::default());
        job.step(Duration::from_secs(1), |_, _| Some("code ```` here".into()));
        let md = job.markdown("now", "x");
        assert!(md.contains("`````text\n"), "{md}");
        assert!(md.trim_end().ends_with("`````"), "{md}");
    }

    #[test]
    fn time_order_crosses_streams_with_gaps_and_untimed_last() {
        let md = build(
            vec![
                stream(
                    "a",
                    10,
                    vec![
                        bm(1, Some(60_000), Some("oom")),
                        bm(5, None, Some("banner")),
                    ],
                ),
                stream("b", 10, vec![bm(3, Some(0), Some("deploy"))]),
            ],
            ReportOptions {
                context: 0,
                order: ReportOrder::Time,
                ..Default::default()
            },
        );
        let deploy = md.find("deploy").unwrap();
        let oom = md.find("oom").unwrap();
        let banner = md.find("banner").unwrap();
        assert!(deploy < oom && oom < banner, "{md}");
        assert!(
            md.contains("### Line 2 · a · t60000 (+1m 00s) · oom"),
            "{md}"
        );
        assert!(!md.contains("## a\n"), "no stream sections in time order");
    }

    #[test]
    fn a_line_past_the_end_is_unavailable_and_long_lines_are_cut() {
        let streams = vec![stream("a", 2, vec![bm(1, None, None)])];
        let mut job = ReportJob::new(streams, ReportOptions::default());
        let long = "x".repeat(MAX_REPORT_LINE_CHARS + 5);
        job.step(Duration::from_secs(1), |_, l| {
            (l == 0).then(|| long.clone())
        });
        let md = job.markdown("now", "x");
        assert!(md.contains("> 2 | (line unavailable)"), "{md}");
        assert!(
            md.contains(&format!("{}…", "x".repeat(MAX_REPORT_LINE_CHARS))),
            "{md}"
        );
    }

    #[test]
    fn selection_honours_tags_and_caps_automatic_bookmarks() {
        let manual: BTreeSet<usize> = [1, 5, 9].into();
        let notes: BTreeMap<usize, String> =
            [(1, "#deploy".to_string()), (5, "#oom".to_string())].into();
        let automatic: BTreeSet<usize> = (0..MAX_AUTO_BOOKMARKS + 10).collect();
        let tagged = ReportOptions {
            tags: vec!["deploy".into()],
            include_auto: true,
            ..Default::default()
        };
        assert_eq!(
            select_bookmarks(&manual, &automatic, &notes, &tagged),
            (vec![(1, false)], 0)
        );
        let all = ReportOptions {
            include_auto: true,
            ..Default::default()
        };
        let (picked, left) = select_bookmarks(&manual, &automatic, &notes, &all);
        assert_eq!(picked.iter().filter(|(_, a)| !a).count(), 3);
        assert_eq!(
            picked.iter().filter(|(_, a)| *a).count(),
            MAX_AUTO_BOOKMARKS
        );
        assert_eq!(left, 10 - 3, "the manual lines are not counted twice");
        assert!(picked.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn steps_stop_on_budget_and_cancel() {
        let streams = vec![stream(
            "a",
            10_000,
            (0..1_000).map(|l| bm(l * 10, None, None)).collect(),
        )];
        let mut job = ReportJob::new(streams, ReportOptions::default());
        // 1,000 bookmarks with 3 lines each side, the first clamped at line 0.
        assert_eq!(job.total_lines(), 6_997);
        assert!(!job.step(Duration::ZERO, |_, _| Some(String::new())));
        assert!(job.progress() > 0.0 && job.progress() < 1.0);
        job.cancel();
        assert!(job.is_cancelled());
    }
}
