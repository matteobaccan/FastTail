//! Collapse of repeated lines: runs of consecutive equal entries of the visible lines
//! shown as one group with a `×N` badge.
//!
//! An entry is a visible line that is not a stack-trace continuation line plus the
//! visible continuation lines that follow it; a visible continuation line whose parent
//! is hidden is an entry of its own. Each line is normalised (leading timestamp and
//! trailing whitespace dropped, numbers masked in `Numbers` mode) into a reused buffer,
//! and an entry is compared byte for byte with the one before it: no hash, no collision.
//!
//! Only the runs of two or more entries are stored, as `Group`s of 24 bytes over visible
//! positions (the index of a line among the visible ones), so a log with no repetition
//! costs nothing. The row mapping of the text view goes through `CollapseState`: rows are
//! visible positions minus the entries hidden by the closed groups before them.

use std::collections::BTreeSet;

use crate::tail_engine::TailEngine;
use crate::timestamp::{leading_span, FormatHint};

/// Entries longer than this many lines are never grouped.
pub const MAX_ENTRY_LINES: usize = 256;
/// Entries whose normalised text is longer than this are never grouped.
pub const MAX_ENTRY_BYTES: usize = 64 * 1024;

/// How lines are compared, per stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CollapseMode {
    #[default]
    Off,
    /// The text after the leading timestamp.
    Exact,
    /// As `Exact`, with numbers, hex values and ids masked.
    Numbers,
}

impl CollapseMode {
    pub const ALL: [CollapseMode; 3] = [
        CollapseMode::Off,
        CollapseMode::Exact,
        CollapseMode::Numbers,
    ];

    /// Name used in the workspace and in session files (`collapse=exact`).
    pub fn name(self) -> &'static str {
        match self {
            CollapseMode::Off => "off",
            CollapseMode::Exact => "exact",
            CollapseMode::Numbers => "numbers",
        }
    }

    pub fn from_name(name: &str) -> Option<CollapseMode> {
        Self::ALL
            .into_iter()
            .find(|m| m.name().eq_ignore_ascii_case(name.trim()))
    }

    /// The mode after this one, for `CTRL + SHIFT + D`: off, exact, numbers, off.
    pub fn next(self) -> CollapseMode {
        match self {
            CollapseMode::Off => CollapseMode::Exact,
            CollapseMode::Exact => CollapseMode::Numbers,
            CollapseMode::Numbers => CollapseMode::Off,
        }
    }

    pub fn is_on(self) -> bool {
        self != CollapseMode::Off
    }
}

/// A run of `count` (at least 2) equal entries of `entry_len` lines each, starting at
/// visible position `pos`. Collapsed, it shows the first entry, its first line at `row`,
/// and hides the others; `open` shows every line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Group {
    pub pos: usize,
    pub row: usize,
    pub count: u32,
    pub entry_len: u16,
    pub open: bool,
}

impl Group {
    fn new(pos: usize, entry_len: usize, count: u32) -> Self {
        Self {
            pos,
            row: 0,
            count,
            entry_len: entry_len as u16,
            open: false,
        }
    }

    /// Visible positions the group covers.
    pub fn lines(&self) -> usize {
        self.entry_len as usize * self.count as usize
    }

    /// One past the last visible position of the group.
    pub fn end(&self) -> usize {
        self.pos + self.lines()
    }

    /// Visible positions the group hides (none while open).
    pub fn hidden(&self) -> usize {
        if self.open {
            0
        } else {
            self.entry_len as usize * (self.count as usize - 1)
        }
    }

    /// Rows the group shows.
    fn shown(&self) -> usize {
        self.lines() - self.hidden()
    }
}

/// `×N` for the badge: thousands grouped, one decimal and `M` above 999,999.
pub fn badge_text(count: u32) -> String {
    if count > 999_999 {
        return format!("×{:.1}M", count as f64 / 1_000_000.0);
    }
    let digits = count.to_string();
    let mut out = String::from("×");
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Appends the normalised form of `line` to `out`: without the leading timestamp and the
/// whitespace around what is left, and in `Numbers` mode with each UUID, `0x` number, hex
/// word of 8 or more characters holding a digit, and run of decimal digits replaced by
/// one `#`.
pub fn normalize_line(line: &str, mode: CollapseMode, hint: FormatHint, out: &mut Vec<u8>) {
    let start = leading_span(line, hint).unwrap_or(0);
    let rest = line[start..].trim_end();
    let rest = if start > 0 { rest.trim_start() } else { rest };
    if mode != CollapseMode::Numbers {
        out.extend_from_slice(rest.as_bytes());
        return;
    }
    let b = rest.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let boundary = i == 0 || !b[i - 1].is_ascii_alphanumeric();
        if boundary {
            if let Some(len) = masked_token(&b[i..]) {
                out.push(b'#');
                i += len;
                continue;
            }
        }
        if b[i].is_ascii_digit() {
            // A run of decimal digits inside a word (`user41`, `v2`).
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            out.push(b'#');
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
}

/// Length of the token masked whole at the start of `b` (a word boundary): a UUID, a
/// `0x` number, or a hex word of 8 or more characters holding a digit.
fn masked_token(b: &[u8]) -> Option<usize> {
    let ends_word = |len: usize| b.get(len).is_none_or(|c| !c.is_ascii_alphanumeric());
    if b.len() >= 36
        && ends_word(36)
        && b[..36].iter().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => *c == b'-',
            _ => c.is_ascii_hexdigit(),
        })
    {
        return Some(36);
    }
    if b.len() > 2 && b[0] == b'0' && (b[1] == b'x' || b[1] == b'X') && b[2].is_ascii_hexdigit() {
        let digits = b[2..].iter().take_while(|c| c.is_ascii_hexdigit()).count();
        return Some(2 + digits);
    }
    let word = b.iter().take_while(|c| c.is_ascii_alphanumeric()).count();
    if word >= 8
        && b[..word].iter().all(u8::is_ascii_hexdigit)
        && b[..word].iter().any(u8::is_ascii_digit)
    {
        return Some(word);
    }
    None
}

/// The entry being read: its first line and visible position, its line count and
/// normalised text (lines joined by `\n`), and whether it went past the caps.
#[derive(Debug, Clone, Default)]
struct Entry {
    pos: usize,
    line: usize,
    lines: usize,
    text: Vec<u8>,
    oversize: bool,
    /// A continuation line whose parent is hidden.
    orphan: bool,
}

/// The current run of equal entries and the normalised text they share.
#[derive(Debug, Clone)]
struct Run {
    pos: usize,
    entry_len: usize,
    count: u32,
    text: Vec<u8>,
}

impl Run {
    fn group(&self) -> Option<Group> {
        (self.count >= 2).then(|| Group::new(self.pos, self.entry_len, self.count))
    }
}

/// Run detection over the lines of a stream, in file order, fed one line at a time by the
/// engine (synchronous path) or by a `Collapse` scan (background path): both feed the
/// same lines and get the same groups. Completed groups collect in `groups`; the state
/// left at the end is the cursor an append resumes from (`resume`).
#[derive(Debug, Clone)]
pub struct Detector {
    mode: CollapseMode,
    hint: FormatHint,
    run: Option<Run>,
    cur: Option<Entry>,
    /// Whether the last non-continuation line was visible: a visible continuation line
    /// joins the entry in progress only then.
    header_visible: bool,
    /// First line not fed yet.
    next_line: usize,
    /// Completed groups, in file order, not taken yet.
    pub groups: Vec<Group>,
    /// A spare text buffer, swapped with the run's so no entry allocates.
    spare: Vec<u8>,
    /// Lines fed since the detector was created, for the tests of the incremental path.
    lines_fed: u64,
}

impl Detector {
    pub fn new(mode: CollapseMode, hint: FormatHint) -> Self {
        Self {
            mode,
            hint,
            run: None,
            cur: None,
            header_visible: false,
            next_line: 0,
            groups: Vec::new(),
            spare: Vec::new(),
            lines_fed: 0,
        }
    }

    /// Lines fed so far, over every pass (resumed ones included).
    pub fn lines_fed(&self) -> u64 {
        self.lines_fed
    }

    /// Feeds line `line` with its text and, when it is visible, its visible position.
    pub fn feed(&mut self, line: usize, text: &str, pos: Option<usize>) {
        self.next_line = line + 1;
        self.lines_fed += 1;
        let continuation = TailEngine::is_stacktrace_continuation(text);
        if !continuation {
            self.header_visible = pos.is_some();
        }
        let Some(pos) = pos else {
            if !continuation {
                // A later entry began, hidden or not: the open one can gain no more
                // lines, so it is closed now and an append never rewinds to it.
                self.finish_entry();
            }
            return;
        };
        let joins = continuation && self.header_visible && self.cur.is_some();
        if !joins {
            self.finish_entry();
            let mut text = std::mem::take(&mut self.spare);
            text.clear();
            self.cur = Some(Entry {
                pos,
                line,
                lines: 0,
                text,
                oversize: false,
                orphan: continuation,
            });
        }
        let (mode, hint) = (self.mode, self.hint);
        let Some(entry) = self.cur.as_mut() else {
            return;
        };
        entry.lines += 1;
        if entry.oversize {
            return;
        }
        if entry.lines > MAX_ENTRY_LINES {
            entry.oversize = true;
            return;
        }
        if entry.lines > 1 {
            entry.text.push(b'\n');
        }
        normalize_line(text, mode, hint, &mut entry.text);
        if entry.text.len() > MAX_ENTRY_BYTES {
            entry.oversize = true;
            entry.text.clear();
        }
    }

    /// Compares the entry in progress with the run and moves on: one more repetition, or
    /// the end of the run (a group when it holds two entries or more) and a new one.
    fn finish_entry(&mut self) {
        let Some(mut entry) = self.cur.take() else {
            return;
        };
        if entry.oversize {
            // Never grouped, and it breaks the run around it.
            self.close_run();
            entry.text.clear();
            self.spare = entry.text;
            return;
        }
        if let Some(run) = self.run.as_mut() {
            if run.entry_len == entry.lines && run.text == entry.text && run.count < u32::MAX {
                run.count += 1;
                self.spare = entry.text;
                return;
            }
        }
        self.close_run();
        self.run = Some(Run {
            pos: entry.pos,
            entry_len: entry.lines,
            count: 1,
            text: entry.text,
        });
    }

    fn close_run(&mut self) {
        if let Some(run) = self.run.take() {
            if let Some(group) = run.group() {
                self.groups.push(group);
            }
            self.spare = run.text;
        }
    }

    /// The groups the lines fed so far end with, were the stream to end here: the run
    /// with the entry in progress counted in when it repeats. They change as appended
    /// lines complete the last entry, so they are kept apart from `groups`.
    pub fn tail_groups(&self) -> Vec<Group> {
        let mut out = Vec::new();
        let Some(run) = &self.run else {
            return out;
        };
        let repeats = self.cur.as_ref().is_some_and(|e| {
            !e.oversize && e.lines == run.entry_len && e.text == run.text && run.count < u32::MAX
        });
        if repeats {
            out.push(Group::new(run.pos, run.entry_len, run.count + 1));
        } else if let Some(group) = run.group() {
            out.push(group);
        }
        out
    }

    /// Makes the detector ready to go on after an append whose lines from `from` must be
    /// read (again): it drops the entry in progress, which the new lines may complete,
    /// and returns the line to feed from, its first line (or `from` without one). An
    /// entry stays in progress only up to the next non-continuation line, visible or
    /// hidden (see `feed`), so this never rewinds past the last entry of the file: an
    /// append is read from about where it starts, whatever the filter hides before it.
    pub fn resume(&mut self, from: usize) -> usize {
        match self.cur.take() {
            Some(entry) => {
                self.header_visible = !entry.orphan;
                self.spare = entry.text;
                entry.line.min(from)
            }
            None => from.min(self.next_line),
        }
    }
}

/// A group seen from the view: its size, whether it is expanded, and the file lines it
/// spans (the first and last line of the group, and of its hidden entries when closed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CollapsedRow {
    pub count: u32,
    pub open: bool,
    pub first_line: usize,
    pub last_line: usize,
    pub hidden: Option<(usize, usize)>,
}

/// The groups of a stream and the row mapping over them.
#[derive(Debug, Default)]
pub struct CollapseState {
    pub mode: CollapseMode,
    groups: Vec<Group>,
    /// Groups before this index are final; the others are the detector's tail groups.
    final_len: usize,
    /// First lines of the groups the user expanded, kept across a regrouping.
    open: BTreeSet<usize>,
    /// Visible positions hidden by the closed groups.
    hidden: usize,
    /// Detection state where the scanned lines end; `None` until a detection completes.
    detector: Option<Box<Detector>>,
}

impl CollapseState {
    pub fn groups(&self) -> &[Group] {
        &self.groups
    }

    pub fn detector(&self) -> Option<&Detector> {
        self.detector.as_deref()
    }

    pub fn detector_mut(&mut self) -> Option<&mut Detector> {
        self.detector.as_deref_mut()
    }

    pub fn take_detector(&mut self) -> Option<Box<Detector>> {
        self.detector.take()
    }

    pub fn set_detector(&mut self, detector: Box<Detector>) {
        self.detector = Some(detector);
    }

    /// Forgets the groups and the detection state; the expanded groups are kept.
    pub fn clear_groups(&mut self) {
        self.groups = Vec::new();
        self.final_len = 0;
        self.hidden = 0;
        self.detector = None;
    }

    /// Forgets everything, the expanded groups included (a reload, a mode change).
    pub fn reset(&mut self) {
        self.clear_groups();
        self.open.clear();
    }

    /// Drops the tail groups, which an append is about to recompute.
    pub fn drop_tail(&mut self) {
        self.groups.truncate(self.final_len);
        self.rebuild_rows_from(self.final_len);
    }

    /// Appends final groups, in file order after the ones held. `line_of` maps a visible
    /// position to its file line, so a group whose first line was expanded stays so.
    pub fn push_final(&mut self, groups: Vec<Group>, line_of: impl Fn(usize) -> Option<usize>) {
        if groups.is_empty() {
            return;
        }
        self.groups.truncate(self.final_len);
        let from = self.groups.len();
        self.push_groups(groups, &line_of);
        self.final_len = self.groups.len();
        self.rebuild_rows_from(from);
    }

    /// Replaces the tail groups.
    pub fn set_tail(&mut self, groups: Vec<Group>, line_of: impl Fn(usize) -> Option<usize>) {
        self.groups.truncate(self.final_len);
        self.push_groups(groups, &line_of);
        self.rebuild_rows_from(self.final_len);
    }

    fn push_groups(&mut self, groups: Vec<Group>, line_of: &impl Fn(usize) -> Option<usize>) {
        for mut group in groups {
            group.open = !self.open.is_empty()
                && line_of(group.pos).is_some_and(|line| self.open.contains(&line));
            self.groups.push(group);
        }
    }

    /// Keeps only the expanded lines that still head a group, once a detection completes.
    pub fn prune_open(&mut self, line_of: impl Fn(usize) -> Option<usize>) {
        if self.open.is_empty() {
            return;
        }
        self.open = self
            .groups
            .iter()
            .filter(|g| g.open)
            .filter_map(|g| line_of(g.pos))
            .collect();
    }

    /// Recomputes the rows of the groups from index `from` on, and the hidden total.
    fn rebuild_rows_from(&mut self, from: usize) {
        let mut hidden = match from.checked_sub(1).and_then(|i| self.groups.get(i)) {
            Some(prev) => prev.pos - prev.row + prev.hidden(),
            None => 0,
        };
        for group in &mut self.groups[from..] {
            group.row = group.pos - hidden;
            hidden += group.hidden();
        }
        self.hidden = hidden;
    }

    /// Rows of a view of `visible` positions.
    pub fn row_count(&self, visible: usize) -> usize {
        visible.saturating_sub(self.hidden)
    }

    /// Index of the last group starting at or before row `row`.
    fn group_at_row(&self, row: usize) -> Option<usize> {
        self.groups.partition_point(|g| g.row <= row).checked_sub(1)
    }

    /// Index of the last group starting at or before position `pos`.
    fn group_at_pos(&self, pos: usize) -> Option<usize> {
        self.groups.partition_point(|g| g.pos <= pos).checked_sub(1)
    }

    /// Visible position of the first line of row `row`.
    pub fn pos_of_row(&self, row: usize) -> usize {
        let Some(g) = self.group_at_row(row).map(|i| &self.groups[i]) else {
            return row;
        };
        if row < g.row + g.shown() {
            g.pos + (row - g.row)
        } else {
            row + (g.pos - g.row) + g.hidden()
        }
    }

    /// Row showing position `pos`, and whether the position is hidden in a closed group
    /// (the row is then the group's first row).
    pub fn row_of_pos(&self, pos: usize) -> (usize, bool) {
        let Some(g) = self.group_at_pos(pos).map(|i| &self.groups[i]) else {
            return (pos, false);
        };
        if pos < g.end() {
            if pos < g.pos + g.shown() {
                (g.row + (pos - g.pos), false)
            } else {
                (g.row, true)
            }
        } else {
            (pos - (g.pos - g.row) - g.hidden(), false)
        }
    }

    /// The group whose first row is `row`.
    pub fn group_heading_row(&self, row: usize) -> Option<Group> {
        self.group_at_row(row)
            .map(|i| self.groups[i])
            .filter(|g| g.row == row)
    }

    /// The group covering position `pos`.
    pub fn group_of_pos(&self, pos: usize) -> Option<Group> {
        self.group_at_pos(pos)
            .map(|i| self.groups[i])
            .filter(|g| pos < g.end())
    }

    /// Expands or collapses the group starting at position `pos`, whose first line is
    /// `line`. Returns false when no group starts there.
    pub fn set_open(&mut self, pos: usize, line: usize, open: bool) -> bool {
        let Some(i) = self
            .group_at_pos(pos)
            .filter(|&i| self.groups[i].pos == pos)
        else {
            return false;
        };
        if self.groups[i].open == open {
            return true;
        }
        self.groups[i].open = open;
        if open {
            self.open.insert(line);
        } else {
            self.open.remove(&line);
        }
        self.rebuild_rows_from(i);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normalized(line: &str, mode: CollapseMode) -> String {
        let mut out = Vec::new();
        normalize_line(line, mode, FormatHint::Unknown, &mut out);
        String::from_utf8(out).unwrap()
    }

    /// Feeds `lines` (all visible) and returns the groups, tail included.
    fn detect(lines: &[&str], mode: CollapseMode) -> Vec<(usize, usize, u32)> {
        detect_visible(lines, &vec![true; lines.len()], mode)
    }

    fn detect_visible(
        lines: &[&str],
        visible: &[bool],
        mode: CollapseMode,
    ) -> Vec<(usize, usize, u32)> {
        let mut det = Detector::new(mode, FormatHint::Unknown);
        let mut pos = 0;
        for (i, line) in lines.iter().enumerate() {
            let p = visible[i].then(|| {
                pos += 1;
                pos - 1
            });
            det.feed(i, line, p);
        }
        let mut groups = std::mem::take(&mut det.groups);
        groups.extend(det.tail_groups());
        groups
            .iter()
            .map(|g| (g.pos, g.entry_len as usize, g.count))
            .collect()
    }

    #[test]
    fn modes_have_names_and_cycle() {
        for mode in CollapseMode::ALL {
            assert_eq!(CollapseMode::from_name(mode.name()), Some(mode));
        }
        assert_eq!(
            CollapseMode::from_name(" Numbers "),
            Some(CollapseMode::Numbers)
        );
        assert_eq!(CollapseMode::from_name("signature"), None);
        assert_eq!(CollapseMode::Off.next(), CollapseMode::Exact);
        assert_eq!(CollapseMode::Exact.next(), CollapseMode::Numbers);
        assert_eq!(CollapseMode::Numbers.next(), CollapseMode::Off);
    }

    #[test]
    fn badge_groups_thousands_and_shortens_millions() {
        assert_eq!(badge_text(2), "×2");
        assert_eq!(badge_text(500), "×500");
        assert_eq!(badge_text(1_000), "×1,000");
        assert_eq!(badge_text(999_999), "×999,999");
        assert_eq!(badge_text(1_200_000), "×1.2M");
        assert_eq!(badge_text(u32::MAX), "×4295.0M");
    }

    #[test]
    fn exact_drops_the_timestamp_and_trailing_whitespace_only() {
        let mode = CollapseMode::Exact;
        assert_eq!(
            normalized("2026-09-18T14:02:05.123Z WARN retry 3  \t", mode),
            "WARN retry 3"
        );
        assert_eq!(
            normalized("12:00:01.250 WARN retrying", mode),
            "WARN retrying"
        );
        assert_eq!(
            normalized("  at Foo.bar(Foo.java:10)", mode),
            "  at Foo.bar(Foo.java:10)"
        );
        assert_eq!(normalized("user 41 id 0x1f", mode), "user 41 id 0x1f");
    }

    #[test]
    fn numbers_masks_each_token_class() {
        let mode = CollapseMode::Numbers;
        assert_eq!(
            normalized("user 41 fetched order 0x1f3a", mode),
            "user # fetched order #"
        );
        assert_eq!(normalized("port 8080 retry 3", mode), "port # retry #");
        assert_eq!(normalized("user41 v2", mode), "user# v#");
        assert_eq!(
            normalized("req 123e4567-e89b-12d3-a456-426614174000 done", mode),
            "req # done"
        );
        assert_eq!(
            normalized("commit deadbeef42 pushed", mode),
            "commit # pushed"
        );
        assert_eq!(normalized("addr 0XFFEE", mode), "addr #");
        assert_eq!(normalized("took 1.25s", mode), "took #.#s");
        // Words that are not numbers stay: short hex, hex without a digit, plain words.
        assert_eq!(
            normalized("cafe deadbeef facade", mode),
            "cafe deadbeef facade"
        );
        assert_eq!(
            normalized("abc1234 end", mode),
            "abc# end",
            "7 characters: digits only"
        );
        assert_eq!(normalized("0x without digits", mode), "#x without digits");
        assert_eq!(normalized("città 7", mode), "città #");
    }

    #[test]
    fn equal_runs_become_groups_and_singles_do_not() {
        let lines = ["a", "b", "b", "b", "c", "a", "a"];
        assert_eq!(
            detect(&lines, CollapseMode::Exact),
            vec![(1, 1, 3), (5, 1, 2)]
        );
        assert!(detect(&["a", "b", "a"], CollapseMode::Exact).is_empty());
    }

    #[test]
    fn stack_traces_collapse_as_whole_entries() {
        let mut lines = Vec::new();
        for _ in 0..40 {
            lines.push("ERROR boom");
            lines.extend(std::iter::repeat_n("    at Foo.bar(Foo.java:10)", 12));
        }
        lines.push("INFO next");
        assert_eq!(detect(&lines, CollapseMode::Exact), vec![(0, 13, 40)]);
        // A trace with one more frame is another entry.
        let lines = ["E", "  at a", "E", "  at a", "  at b"];
        assert!(detect(&lines, CollapseMode::Exact).is_empty());
    }

    #[test]
    fn orphan_continuation_lines_are_entries_of_their_own() {
        // The parent of the two frames is hidden: each frame stands alone, and two equal
        // ones repeat.
        let lines = ["HEADER", "  at a", "  at a", "next"];
        let visible = [false, true, true, true];
        assert_eq!(
            detect_visible(&lines, &visible, CollapseMode::Exact),
            vec![(0, 1, 2)]
        );
        // A hidden frame inside a visible entry leaves the entry whole.
        let lines = ["E", "  at x", "  at a", "E", "  at y", "  at a"];
        let visible = [true, false, true, true, false, true];
        assert_eq!(
            detect_visible(&lines, &visible, CollapseMode::Exact),
            vec![(0, 2, 2)]
        );
    }

    #[test]
    fn entries_past_the_caps_never_group() {
        let long = "x".repeat(MAX_ENTRY_BYTES + 1);
        assert!(detect(&[&long, &long, &long], CollapseMode::Exact).is_empty());
        let mut trace = vec!["E"];
        trace.extend(std::iter::repeat_n("  at a", MAX_ENTRY_LINES));
        let twice: Vec<&str> = trace.iter().chain(trace.iter()).copied().collect();
        assert!(detect(&twice, CollapseMode::Exact).is_empty());
        // One line under the cap groups.
        let short: Vec<&str> = trace[..MAX_ENTRY_LINES]
            .iter()
            .chain(trace[..MAX_ENTRY_LINES].iter())
            .copied()
            .collect();
        assert_eq!(
            detect(&short, CollapseMode::Exact),
            vec![(0, MAX_ENTRY_LINES, 2)]
        );
        // An oversized entry breaks the run around it.
        assert!(detect(&["a", &long, "a"], CollapseMode::Exact).is_empty());
    }

    #[test]
    fn the_count_saturates_and_starts_a_new_group() {
        let mut det = Detector::new(CollapseMode::Exact, FormatHint::Unknown);
        det.feed(0, "a", Some(0));
        det.feed(1, "a", Some(1));
        det.feed(2, "a", Some(2));
        det.run.as_mut().unwrap().count = u32::MAX;
        det.feed(3, "a", Some(3));
        det.feed(4, "a", Some(4));
        let groups: Vec<u32> = det
            .groups
            .iter()
            .chain(det.tail_groups().iter())
            .map(|g| g.count)
            .collect();
        // The saturated run, then the three entries that came after it.
        assert_eq!(groups, vec![u32::MAX, 3]);
    }

    #[test]
    fn resuming_after_an_append_matches_a_full_pass() {
        let first = ["x", "hb", "hb", "E", "  at a"];
        let more = ["  at b", "E", "  at a", "  at b", "hb"];
        let all: Vec<&str> = first.iter().chain(more.iter()).copied().collect();

        let mut det = Detector::new(CollapseMode::Exact, FormatHint::Unknown);
        for (i, line) in first.iter().enumerate() {
            det.feed(i, line, Some(i));
        }
        let mut groups = std::mem::take(&mut det.groups);
        assert!(groups.is_empty(), "the hb run is still open");
        assert_eq!(det.tail_groups().len(), 1);
        // The last line is re-read, as the engine does after an append.
        let from = det.resume(first.len() - 1);
        assert_eq!(from, 3, "the last entry starts at line 3");
        for (i, line) in all.iter().enumerate().skip(from) {
            det.feed(i, line, Some(i));
        }
        groups.append(&mut det.groups);
        groups.extend(det.tail_groups());
        let resumed: Vec<(usize, usize, u32)> = groups
            .iter()
            .map(|g| (g.pos, g.entry_len as usize, g.count))
            .collect();
        assert_eq!(resumed, detect(&all, CollapseMode::Exact));
        assert_eq!(resumed, vec![(1, 1, 2), (3, 3, 2)]);
    }

    #[test]
    fn a_hidden_entry_closes_the_open_one_so_resume_never_rewinds_to_it() {
        let mut det = Detector::new(CollapseMode::Exact, FormatHint::Unknown);
        det.feed(0, "ERROR a", Some(0));
        det.feed(1, "ERROR a", Some(1));
        // A million hidden lines later, the last line of the file.
        det.feed(2, "INFO hidden", None);
        det.feed(1_000_000, "INFO hidden", None);
        assert_eq!(det.tail_groups(), vec![Group::new(0, 1, 2)]);
        assert_eq!(det.resume(1_000_000), 1_000_000, "no rewind to line 1");
        // A hidden continuation line keeps the entry open: it may still gain lines.
        let mut det = Detector::new(CollapseMode::Exact, FormatHint::Unknown);
        det.feed(0, "ERROR a", Some(0));
        det.feed(1, "  at hidden", None);
        assert_eq!(det.resume(1), 0);
    }

    fn state_with(groups: &[(usize, usize, u32)]) -> CollapseState {
        let mut state = CollapseState {
            mode: CollapseMode::Exact,
            ..Default::default()
        };
        state.push_final(
            groups
                .iter()
                .map(|&(pos, len, count)| Group::new(pos, len, count))
                .collect(),
            Some,
        );
        state
    }

    #[test]
    fn rows_skip_the_hidden_entries_both_ways() {
        // 20 positions: a ×3 group of 1 line at 2, a ×2 group of 3 lines at 10.
        let state = state_with(&[(2, 1, 3), (10, 3, 2)]);
        assert_eq!(state.row_count(20), 20 - 2 - 3);
        let rows: Vec<usize> = (0..state.row_count(20))
            .map(|r| state.pos_of_row(r))
            .collect();
        assert_eq!(
            rows,
            vec![0, 1, 2, 5, 6, 7, 8, 9, 10, 11, 12, 16, 17, 18, 19]
        );
        for (row, &pos) in rows.iter().enumerate() {
            assert_eq!(state.row_of_pos(pos), (row, false));
        }
        assert_eq!(state.row_of_pos(3), (2, true));
        assert_eq!(state.row_of_pos(4), (2, true));
        assert_eq!(state.row_of_pos(15), (8, true));
        assert_eq!(state.group_heading_row(8).map(|g| g.pos), Some(10));
        assert_eq!(state.group_heading_row(9), None);
        assert_eq!(state.group_of_pos(14).map(|g| g.pos), Some(10));
        assert_eq!(state.group_of_pos(16), None);
    }

    #[test]
    fn expanding_shows_every_line_and_survives_a_regrouping() {
        let mut state = state_with(&[(2, 1, 3), (10, 3, 2)]);
        assert!(state.set_open(2, 2, true));
        assert_eq!(state.row_count(20), 17);
        assert_eq!(state.pos_of_row(4), 4);
        assert_eq!(state.row_of_pos(15), (10, true));
        assert!(!state.set_open(3, 3, true), "no group starts there");
        // A regrouping keeps the expanded group expanded.
        state.clear_groups();
        state.push_final(vec![Group::new(2, 1, 4)], Some);
        assert!(state.groups()[0].open);
        state.prune_open(Some);
        assert!(state.set_open(2, 2, false));
        assert_eq!(state.row_count(20), 17);
    }

    #[test]
    fn tail_groups_are_replaced_not_stacked() {
        let mut state = state_with(&[(2, 1, 3)]);
        state.set_tail(vec![Group::new(8, 1, 2)], Some);
        assert_eq!(state.row_count(20), 17);
        state.set_tail(vec![Group::new(8, 1, 5)], Some);
        assert_eq!(state.row_count(20), 14);
        state.drop_tail();
        assert_eq!(state.row_count(20), 18);
    }
}
