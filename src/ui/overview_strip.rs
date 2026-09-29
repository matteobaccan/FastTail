//! Overview strip: a narrow column beside the main view's scroll bar marking where the
//! search hits, the bookmarks (automatic ones dimmer) and the ERROR / FATAL lines sit
//! among the visible rows, with the viewport drawn as a box. A click or a drag scrolls
//! the main view there; hovering a bookmark with a note shows the note.
//!
//! The marks are computed into one byte of flags per pixel and cached; the cache is
//! rebuilt only when an input changes (strip height, rows, hits, bookmarks, level cache,
//! filters), at most four times a second while the stream grows. Visible row `r` of `n`
//! maps to `y = r / n * height`.
//!
//! Hit and bookmark marks are exact. Error marks are exact without a filter (from the
//! engine's per-block error counts, reading the level cache only inside blocks that span
//! more than one pixel) and on filtered views up to `EXACT_FILTERED_ROWS` visible rows;
//! above that each pixel samples at most `SAMPLES_PER_PIXEL` of its rows.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use crate::collapse::CollapseState;
use crate::context_lines::ContextRanges;
use crate::i18n::{t, Language};
use crate::log_level::LogLevel;
use crate::tail_engine::{is_error_level, TailEngine, ERROR_BLOCK_LINES};
use crate::theme::CyberTheme;
use egui::{Color32, Stroke, Ui};

/// Width of the strip in points.
pub const STRIP_WIDTH: f32 = 10.0;
/// Filtered views up to this many visible rows get exact error marks.
pub const EXACT_FILTERED_ROWS: usize = 4_000_000;
/// Visible rows a pixel inspects at most for error marks above `EXACT_FILTERED_ROWS`.
pub const SAMPLES_PER_PIXEL: usize = 256;
/// Minimum time between two rebuilds caused by the stream growing.
const REBUILD_INTERVAL: Duration = Duration::from_millis(250);

pub const MARK_HIT: u8 = 1;
pub const MARK_BOOKMARK: u8 = 2;
pub const MARK_ERROR: u8 = 4;
pub const MARK_AUTO_BOOKMARK: u8 = 8;

/// Pixel row of visible row `row` among `rows`, on a strip `height` pixels tall.
pub fn pixel_of_row(row: usize, rows: usize, height: usize) -> usize {
    if rows == 0 || height == 0 {
        return 0;
    }
    ((row as u128 * height as u128 / rows as u128) as usize).min(height - 1)
}

/// Visible row under the pixel offset `y` of a strip `height` pixels tall.
pub fn row_at(y: f32, height: f32, rows: usize) -> usize {
    if rows == 0 || height <= 0.0 {
        return 0;
    }
    let frac = (y / height).clamp(0.0, 1.0) as f64;
    ((frac * rows as f64) as usize).min(rows - 1)
}

/// What the marks are computed from.
pub struct MarkInputs<'a> {
    /// Visible rows of the main view.
    pub rows: usize,
    /// The visible lines when a filter is active (sorted), `None` when every line shows.
    pub filtered: Option<&'a [usize]>,
    /// Search hits (line indices, visible lines only).
    pub hits: &'a [usize],
    pub bookmarks: &'a BTreeSet<usize>,
    /// Automatic bookmarks and the dismissed ones among them (see
    /// `TailEngine::auto_bookmarks`).
    pub auto_bookmarks: &'a BTreeSet<usize>,
    pub dismissed_auto: &'a BTreeSet<usize>,
    /// Cached levels (`LogLevel as u8`) of the first lines, and their per-block ERROR /
    /// FATAL counts (`ERROR_BLOCK_LINES` lines per block).
    pub levels: &'a [u8],
    pub error_blocks: &'a [u32],
    /// The collapsed groups when repeated entries are collapsed, and the visible lines
    /// they are formed over: rows are then fewer than lines, and a line a group hides is
    /// marked at the group's row.
    pub collapse: Option<(&'a CollapseState, usize)>,
    /// The context ranges and the file's line count while the rows show context lines
    /// around the matches in `filtered` (see `context_lines`): positions come from them.
    pub context: Option<(&'a ContextRanges, usize)>,
}

impl MarkInputs<'_> {
    /// Visible row of `line`, `None` when a filter hides it.
    fn row_of(&self, line: usize) -> Option<usize> {
        let lines = self.collapse.map_or(self.rows, |(_, lines)| lines);
        let pos = match (self.context, self.filtered) {
            (Some((ranges, total)), Some(_)) => ranges.pos_of_line(line, total)?,
            (_, Some(f)) => f.binary_search(&line).ok()?,
            (_, None) => (line < lines).then_some(line)?,
        };
        Some(match self.collapse {
            Some((state, _)) => state.row_of_pos(pos).0,
            None => pos,
        })
    }

    /// The line at visible position `pos`.
    fn line_at(&self, pos: usize) -> usize {
        match (self.context, self.filtered) {
            (Some((ranges, total)), Some(_)) => ranges.line_at(pos, total).unwrap_or(pos),
            (_, Some(f)) => f[pos],
            (_, None) => pos,
        }
    }

    /// Visible lines of a filtered view, context lines included.
    fn filtered_len(&self, filtered: &[usize]) -> usize {
        match self.context {
            Some((ranges, total)) => ranges.visible_count(total),
            None => filtered.len(),
        }
    }
}

/// One byte of `MARK_*` flags per pixel, and whether the error marks were sampled.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Marks {
    pub pixels: Vec<u8>,
    pub errors_sampled: bool,
}

impl Marks {
    pub fn is_empty(&self) -> bool {
        self.pixels.iter().all(|&p| p == 0)
    }
}

/// Computes the marks of a strip `height` pixels tall.
pub fn compute_marks(height: usize, input: &MarkInputs) -> Marks {
    let mut marks = Marks {
        pixels: vec![0; height],
        errors_sampled: false,
    };
    let rows = input.rows;
    if height == 0 || rows == 0 {
        return marks;
    }
    let px = |row: usize| pixel_of_row(row, rows, height);
    for &line in input.hits {
        if let Some(row) = input.row_of(line) {
            marks.pixels[px(row)] |= MARK_HIT;
        }
    }
    for &line in input.bookmarks {
        if let Some(row) = input.row_of(line) {
            marks.pixels[px(row)] |= MARK_BOOKMARK;
        }
    }
    for &line in input.auto_bookmarks {
        if input.dismissed_auto.contains(&line) || input.bookmarks.contains(&line) {
            continue;
        }
        if let Some(row) = input.row_of(line) {
            marks.pixels[px(row)] |= MARK_AUTO_BOOKMARK;
        }
    }

    let level_is_error = |line: usize| input.levels.get(line).is_some_and(|&v| is_error_level(v));
    if let Some((state, lines)) = input.collapse {
        // Collapsed rows: exact up to `EXACT_FILTERED_ROWS` visible lines, each at its
        // row; above that each pixel samples its rows.
        if lines <= EXACT_FILTERED_ROWS {
            for pos in 0..lines {
                if level_is_error(input.line_at(pos)) {
                    marks.pixels[px(state.row_of_pos(pos).0)] |= MARK_ERROR;
                }
            }
        } else {
            marks.errors_sampled = true;
            for (p, flags) in marks.pixels.iter_mut().enumerate() {
                let from = (p * rows).div_ceil(height);
                let to = ((p + 1) * rows).div_ceil(height).min(rows);
                if from >= to {
                    continue;
                }
                let step = (to - from).div_ceil(SAMPLES_PER_PIXEL).max(1);
                if (from..to)
                    .step_by(step)
                    .any(|row| level_is_error(input.line_at(state.pos_of_row(row))))
                {
                    *flags |= MARK_ERROR;
                }
            }
        }
        return marks;
    }
    match input.filtered {
        None => {
            // Rows are lines. A block with errors that lands on a single pixel marks it;
            // one spread over several pixels is read line by line from the level cache.
            let cached = input.levels.len().min(rows);
            for (block, &count) in input.error_blocks.iter().enumerate() {
                let start = block * ERROR_BLOCK_LINES;
                if count == 0 || start >= cached {
                    continue;
                }
                let end = (start + ERROR_BLOCK_LINES).min(cached);
                let (first, last) = (px(start), px(end - 1));
                if first == last {
                    marks.pixels[first] |= MARK_ERROR;
                } else {
                    for line in start..end {
                        if is_error_level(input.levels[line]) {
                            marks.pixels[px(line)] |= MARK_ERROR;
                        }
                    }
                }
            }
        }
        Some(filtered) if input.filtered_len(filtered) <= EXACT_FILTERED_ROWS => {
            for row in 0..input.filtered_len(filtered) {
                if level_is_error(input.line_at(row)) {
                    marks.pixels[px(row)] |= MARK_ERROR;
                }
            }
        }
        Some(filtered) => {
            marks.errors_sampled = true;
            let shown = input.filtered_len(filtered);
            for (p, flags) in marks.pixels.iter_mut().enumerate() {
                // Rows mapping to pixel `p`: [ceil(p * rows / h), ceil((p + 1) * rows / h)).
                let from = (p * rows).div_ceil(height);
                let to = ((p + 1) * rows).div_ceil(height).min(shown);
                if from >= to {
                    continue;
                }
                let step = (to - from).div_ceil(SAMPLES_PER_PIXEL).max(1);
                if (from..to)
                    .step_by(step)
                    .any(|row| level_is_error(input.line_at(row)))
                {
                    *flags |= MARK_ERROR;
                }
            }
        }
    }
    marks
}

/// What the cached marks were built from; any change asks for a rebuild.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CacheKey {
    height: usize,
    rows: usize,
    search_generation: u64,
    bookmarks_generation: u64,
    levels: usize,
    filter_generation: u64,
    buffer_generation: u64,
}

/// Per-stream cache kept in egui memory.
#[derive(Debug, Clone, Default)]
pub struct StripCache {
    key: Option<CacheKey>,
    built_at: Option<Instant>,
    pub marks: Marks,
}

impl StripCache {
    /// Brings the marks up to date for a strip `height` pixels tall. A change of the strip
    /// height or of the bookmarks rebuilds at once; the other inputs move while the stream
    /// grows and rebuild at most every `REBUILD_INTERVAL`. Returns the time after which a
    /// throttled rebuild is due, if one is waiting.
    pub fn refresh(&mut self, engine: &TailEngine, height: usize) -> Option<Duration> {
        let key = CacheKey {
            height,
            rows: engine.visible_line_count(),
            search_generation: engine.search_generation,
            bookmarks_generation: engine.bookmarks_generation,
            levels: engine.cached_levels().len(),
            filter_generation: engine.filter_generation,
            buffer_generation: engine.buffer_generation,
        };
        if self.key == Some(key) {
            return None;
        }
        let urgent = self.key.is_none_or(|old| {
            old.height != key.height || old.bookmarks_generation != key.bookmarks_generation
        });
        if !urgent {
            if let Some(since) = self.built_at.map(|at| at.elapsed()) {
                if since < REBUILD_INTERVAL {
                    return Some(REBUILD_INTERVAL - since);
                }
            }
        }
        let input = MarkInputs {
            rows: key.rows,
            filtered: engine
                .rows_filtered()
                .then_some(engine.filtered_lines.as_slice()),
            hits: &engine.search_matches,
            bookmarks: &engine.bookmarks,
            auto_bookmarks: engine.auto_bookmarks(),
            dismissed_auto: engine.dismissed_auto_bookmarks(),
            levels: engine.cached_levels(),
            error_blocks: engine.error_block_counts(),
            collapse: engine
                .collapse_rows_state()
                .map(|state| (state, engine.visible_lines())),
            context: engine
                .shown_context_ranges()
                .map(|ranges| (ranges, engine.total_lines())),
        };
        self.marks = compute_marks(height, &input);
        self.key = Some(key);
        self.built_at = Some(Instant::now());
        None
    }
}

/// The part of the main view on screen, in visible rows.
#[derive(Debug, Clone, Copy)]
pub struct Viewport {
    pub top_row: usize,
    pub rows_on_screen: usize,
}

/// Draws the strip in `rect` from the cached marks and returns the visible row to centre
/// the main view on when the user clicked or dragged it.
pub fn paint(
    ui: &mut Ui,
    rect: egui::Rect,
    engine: &TailEngine,
    marks: &Marks,
    viewport: Viewport,
    theme: &CyberTheme,
    lang: Language,
) -> Option<usize> {
    let rows = engine.visible_line_count();
    let id = egui::Id::new("overview_strip").with(&engine.path);
    let response = ui.interact(rect, id, egui::Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, theme.panel_bg().gamma_multiply(0.85));
    painter.line_segment(
        [rect.left_top(), rect.left_bottom()],
        Stroke::new(1.0, theme.border_color().gamma_multiply(0.4)),
    );

    // A line search scope is shaded: the rows it covers, over the whole strip width.
    if let crate::tail_engine::SearchScope::Lines { first, last } = engine.search_scope() {
        if rows > 0 {
            let top_row = engine.row_of_line_or_next(*first);
            let end_row = last.map_or(rows, |l| engine.row_of_line_or_next(l.saturating_add(1)));
            if end_row > top_row {
                let top = rect.top() + row_frac(top_row, rows) * rect.height();
                let bottom = rect.top() + row_frac(end_row, rows).min(1.0) * rect.height();
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(rect.left(), top),
                        egui::pos2(rect.right(), bottom.max(top + 2.0)),
                    ),
                    0.0,
                    theme.accent_color().gamma_multiply(0.18),
                );
            }
        }
    }

    // Runs of equal flags become one rectangle per mark kind.
    let height = marks.pixels.len();
    let scale = if height > 0 {
        rect.height() / height as f32
    } else {
        1.0
    };
    let error = theme.level_color(LogLevel::Error);
    let hit = theme.warn_color();
    let bookmark = theme.secondary_accent();
    let w = rect.width();
    // Automatic bookmarks at half strength, under the manual ones, so these stay visible
    // among a rule's many marks.
    let lanes: [(u8, f32, f32, Color32); 4] = [
        (MARK_ERROR, 1.0, w * 0.45, error),
        (MARK_HIT, w * 0.4, w, hit),
        (
            MARK_AUTO_BOOKMARK,
            1.0,
            w * 0.35,
            bookmark.gamma_multiply(0.5),
        ),
        (MARK_BOOKMARK, 1.0, w * 0.35, bookmark),
    ];
    for (flag, x0, x1, color) in lanes {
        let mut y = 0;
        while y < height {
            if marks.pixels[y] & flag == 0 {
                y += 1;
                continue;
            }
            let start = y;
            while y < height && marks.pixels[y] & flag != 0 {
                y += 1;
            }
            // At least a point tall so a single mark stays visible.
            let top = rect.top() + start as f32 * scale;
            let bottom = (rect.top() + y as f32 * scale).max(top + 1.5);
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(rect.left() + x0, top),
                    egui::pos2(rect.left() + x1, bottom),
                ),
                0.0,
                color,
            );
        }
    }
    if let Some(line) = engine.current_search_line() {
        if let Some(row) = engine.get_visible_row_of_line(line) {
            let y = rect.top() + row_frac(row, rows) * rect.height();
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(rect.left(), y - 1.0),
                    egui::pos2(rect.right(), y + 1.0),
                ),
                0.0,
                theme.accent_color(),
            );
        }
    }
    if rows > 0 {
        let top = rect.top() + row_frac(viewport.top_row, rows) * rect.height();
        let bottom = rect.top()
            + row_frac(viewport.top_row + viewport.rows_on_screen, rows).min(1.0) * rect.height();
        painter.rect_stroke(
            egui::Rect::from_min_max(
                egui::pos2(rect.left() + 0.5, top),
                egui::pos2(rect.right() - 0.5, bottom.max(top + 3.0)),
            ),
            1.0,
            Stroke::new(1.0, theme.text_primary().gamma_multiply(0.7)),
            egui::StrokeKind::Inside,
        );
    }

    let pointer_row = response
        .hover_pos()
        .or(response.interact_pointer_pos())
        .map(|pos| row_at(pos.y - rect.top(), rect.height(), rows));
    if let Some(row) = pointer_row.filter(|_| response.hovered() && rows > 0) {
        let line = engine.get_actual_line_idx(row).unwrap_or(row);
        let mut tip = format!("{} {}", t(lang, "overview_line"), line + 1);
        // Notes of the bookmarks drawn within a couple of pixels of the pointer.
        if let Some(pos) = response.hover_pos() {
            let y = pos.y - rect.top();
            let first = row_at(y - 2.0, rect.height(), rows);
            let last = row_at(y + 2.0, rect.height(), rows);
            if let (Some(from), Some(to)) = (
                engine.get_actual_line_idx(first),
                engine.get_actual_line_idx(last),
            ) {
                for (note_line, note) in engine
                    .bookmark_notes
                    .range(from..=to)
                    .filter(|(l, _)| engine.get_visible_row_of_line(**l).is_some())
                    .take(5)
                {
                    tip.push_str(&format!("\n✏ {}: {note}", note_line + 1));
                }
            }
        }
        if marks.errors_sampled {
            tip.push('\n');
            tip.push_str(t(lang, "overview_sampled"));
        }
        response.clone().on_hover_text_at_pointer(tip);
    }
    if (response.clicked() || response.dragged()) && rows > 0 {
        return response
            .interact_pointer_pos()
            .map(|pos| row_at(pos.y - rect.top(), rect.height(), rows));
    }
    None
}

fn row_frac(row: usize, rows: usize) -> f32 {
    if rows == 0 {
        0.0
    } else {
        row as f32 / rows as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn levels_with_errors(lines: usize, errors: &[usize]) -> (Vec<u8>, Vec<u32>) {
        let mut levels = vec![LogLevel::Info as u8; lines];
        let mut blocks = vec![0u32; lines.div_ceil(ERROR_BLOCK_LINES)];
        for &e in errors {
            levels[e] = LogLevel::Error as u8;
            blocks[e / ERROR_BLOCK_LINES] += 1;
        }
        (levels, blocks)
    }

    #[test]
    fn rows_map_to_pixels_proportionally() {
        assert_eq!(pixel_of_row(0, 1000, 100), 0);
        assert_eq!(pixel_of_row(500, 1000, 100), 50);
        assert_eq!(pixel_of_row(999, 1000, 100), 99);
        assert_eq!(pixel_of_row(5, 10, 100), 50, "fewer rows than pixels");
        assert_eq!(row_at(50.0, 100.0, 1000), 500);
        assert_eq!(row_at(-3.0, 100.0, 1000), 0);
        assert_eq!(row_at(250.0, 100.0, 1000), 999);
    }

    #[test]
    fn unfiltered_marks_are_exact() {
        let rows = 100_000;
        let (levels, blocks) = levels_with_errors(rows, &[50_000, 99_999]);
        let bookmarks: BTreeSet<usize> = [10].into_iter().collect();
        let input = MarkInputs {
            rows,
            filtered: None,
            hits: &[25_000],
            bookmarks: &bookmarks,
            auto_bookmarks: &BTreeSet::new(),
            dismissed_auto: &BTreeSet::new(),
            levels: &levels,
            error_blocks: &blocks,
            collapse: None,
            context: None,
        };
        // 1000 px: 100 rows per pixel, so every block spans many pixels.
        let marks = compute_marks(1000, &input);
        assert!(!marks.errors_sampled);
        let flagged: Vec<(usize, u8)> = marks
            .pixels
            .iter()
            .enumerate()
            .filter(|(_, &f)| f != 0)
            .map(|(p, &f)| (p, f))
            .collect();
        assert_eq!(
            flagged,
            vec![
                (0, MARK_BOOKMARK),
                (250, MARK_HIT),
                (500, MARK_ERROR),
                (999, MARK_ERROR)
            ]
        );
        // 10 px: a whole block per pixel, marked from the block count alone.
        let marks = compute_marks(10, &input);
        assert_eq!(marks.pixels[5] & MARK_ERROR, MARK_ERROR);
        assert_eq!(marks.pixels[9] & MARK_ERROR, MARK_ERROR);
        assert_eq!(marks.pixels[4] & MARK_ERROR, 0);
    }

    #[test]
    fn automatic_bookmarks_have_their_own_mark_unless_manual_or_dismissed() {
        let bookmarks: BTreeSet<usize> = [30].into_iter().collect();
        let auto: BTreeSet<usize> = [10, 20, 30].into_iter().collect();
        let dismissed: BTreeSet<usize> = [20].into_iter().collect();
        let input = MarkInputs {
            rows: 100,
            filtered: None,
            hits: &[],
            bookmarks: &bookmarks,
            auto_bookmarks: &auto,
            dismissed_auto: &dismissed,
            levels: &[],
            error_blocks: &[],
            collapse: None,
            context: None,
        };
        let marks = compute_marks(100, &input);
        assert_eq!(marks.pixels[10], MARK_AUTO_BOOKMARK);
        assert_eq!(marks.pixels[20], 0, "dismissed");
        assert_eq!(marks.pixels[30], MARK_BOOKMARK, "manual wins");
    }

    #[test]
    fn uncached_levels_carry_no_error_mark() {
        let (levels, blocks) = levels_with_errors(1000, &[900]);
        let input = MarkInputs {
            rows: 2000,
            filtered: None,
            hits: &[],
            bookmarks: &BTreeSet::new(),
            auto_bookmarks: &BTreeSet::new(),
            dismissed_auto: &BTreeSet::new(),
            levels: &levels[..500],
            error_blocks: &blocks,
            collapse: None,
            context: None,
        };
        assert!(compute_marks(100, &input).is_empty());
    }

    #[test]
    fn filtered_marks_use_visible_rows() {
        // Every other line is visible; hit and error on hidden lines are not marked.
        let lines = 1000;
        let filtered: Vec<usize> = (0..lines).step_by(2).collect();
        let (levels, blocks) = levels_with_errors(lines, &[400, 401]);
        let bookmarks: BTreeSet<usize> = [3, 998].into_iter().collect();
        let input = MarkInputs {
            rows: filtered.len(),
            filtered: Some(&filtered),
            hits: &[100],
            bookmarks: &bookmarks,
            auto_bookmarks: &BTreeSet::new(),
            dismissed_auto: &BTreeSet::new(),
            levels: &levels,
            error_blocks: &blocks,
            collapse: None,
            context: None,
        };
        let marks = compute_marks(500, &input);
        assert!(!marks.errors_sampled);
        assert_eq!(marks.pixels[50], MARK_HIT, "line 100 is visible row 50");
        assert_eq!(marks.pixels[200], MARK_ERROR, "line 400 is row 200");
        assert_eq!(marks.pixels[499], MARK_BOOKMARK, "line 998 is the last row");
        assert_eq!(
            marks.pixels.iter().filter(|&&p| p != 0).count(),
            3,
            "hidden bookmark and hidden error leave no mark"
        );
    }

    #[test]
    fn collapsed_rows_carry_the_marks_of_their_hidden_lines() {
        use crate::collapse::{CollapseMode, Group};
        // 100 lines; lines 10..60 are one group of 50 shown as row 10.
        let mut state = CollapseState::default();
        state.mode = CollapseMode::Exact;
        let group = Group {
            pos: 10,
            row: 0,
            count: 50,
            entry_len: 1,
            open: false,
        };
        state.push_final(vec![group], Some);
        let rows = state.row_count(100);
        assert_eq!(rows, 51);
        let (levels, blocks) = levels_with_errors(100, &[45, 90]);
        let bookmarks: BTreeSet<usize> = [80].into_iter().collect();
        let input = MarkInputs {
            rows,
            filtered: None,
            hits: &[30],
            bookmarks: &bookmarks,
            // An automatic bookmark the group hides is drawn at the group's row.
            auto_bookmarks: &[20].into_iter().collect(),
            dismissed_auto: &BTreeSet::new(),
            levels: &levels,
            error_blocks: &blocks,
            collapse: Some((&state, 100)),
            context: None,
        };
        let marks = compute_marks(rows, &input);
        assert_eq!(
            marks.pixels[10],
            MARK_HIT | MARK_ERROR | MARK_AUTO_BOOKMARK,
            "hidden hit, error and automatic bookmark"
        );
        assert_eq!(marks.pixels[31], MARK_BOOKMARK, "line 80 is row 31");
        assert_eq!(marks.pixels[41], MARK_ERROR, "line 90 is row 41");
        assert_eq!(marks.pixels.iter().filter(|&&p| p != 0).count(), 3);
    }

    #[test]
    fn context_rows_carry_marks_at_their_row() {
        // Matches on lines 10 and 50 with one line of context: rows are lines 9, 10, 11,
        // 49, 50, 51.
        let matches = [10, 50];
        let ranges = ContextRanges::build(1, &matches);
        let mut levels = vec![LogLevel::Info as u8; 100];
        levels[49] = LogLevel::Error as u8;
        levels[30] = LogLevel::Error as u8; // hidden
        let bookmarks: BTreeSet<usize> = [11, 70].into_iter().collect();
        let input = MarkInputs {
            rows: 6,
            filtered: Some(&matches),
            hits: &[9],
            bookmarks: &bookmarks,
            auto_bookmarks: &BTreeSet::new(),
            dismissed_auto: &BTreeSet::new(),
            levels: &levels,
            error_blocks: &[],
            collapse: None,
            context: Some((&ranges, 100)),
        };
        let marks = compute_marks(6, &input);
        assert_eq!(
            marks.pixels,
            vec![MARK_HIT, 0, MARK_BOOKMARK, MARK_ERROR, 0, 0]
        );
    }

    #[test]
    fn huge_filtered_views_sample_the_errors() {
        let rows = EXACT_FILTERED_ROWS + 1000;
        let filtered: Vec<usize> = (0..rows).collect();
        let mut levels = vec![LogLevel::Info as u8; rows];
        // A dense run of errors is found; an isolated one may be missed, as disclosed.
        for v in &mut levels[rows / 2..rows / 2 + 20_000] {
            *v = LogLevel::Fatal as u8;
        }
        let input = MarkInputs {
            rows,
            filtered: Some(&filtered),
            hits: &[],
            bookmarks: &BTreeSet::new(),
            auto_bookmarks: &BTreeSet::new(),
            dismissed_auto: &BTreeSet::new(),
            levels: &levels,
            error_blocks: &[],
            collapse: None,
            context: None,
        };
        let marks = compute_marks(100, &input);
        assert!(marks.errors_sampled);
        assert_eq!(marks.pixels[50] & MARK_ERROR, MARK_ERROR);
        assert_eq!(marks.pixels[10], 0);

        // One row under the threshold: exact again.
        let exact: Vec<usize> = (0..EXACT_FILTERED_ROWS).collect();
        let input = MarkInputs {
            rows: exact.len(),
            filtered: Some(&exact),
            ..input
        };
        assert!(!compute_marks(100, &input).errors_sampled);
    }
}
