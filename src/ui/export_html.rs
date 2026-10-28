// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The HTML export: one standalone page, no script and no external resource, with the
//! line numbers and the lines drawn in the colours the view shows them (highlight rules,
//! quick labels, level colours, ANSI colours in render mode, automatic tokens and the
//! search hits) on the current theme. One CSS class per distinct style, written at the
//! end (a `<style>` element applies to the whole page wherever it stands), so the page
//! grows with the lines, not with the styles.

use crate::color::Rgba;
use crate::export::RowWriter;
use crate::tail_engine::{HighlightSpan, HighlightStyle, RowText, SpanStyle, TailEngine};
use crate::theme::CyberTheme;
use std::collections::HashMap;
use std::io::Write;

/// The HTML writer of a stream: a snapshot of the theme and the toggles it draws with.
pub struct HtmlWriter {
    theme: CyberTheme,
    level_colors: bool,
    /// The search query, lower-cased (its hits are marked).
    query: String,
    title: String,
    /// CSS of each class, in the order the classes were made.
    classes: HashMap<String, usize>,
    order: Vec<String>,
}

impl HtmlWriter {
    pub fn new(engine: &TailEngine, theme: CyberTheme, level_colors: bool, title: &str) -> Self {
        Self {
            theme,
            level_colors,
            query: engine.last_searched_query.to_lowercase(),
            title: title.to_string(),
            classes: HashMap::new(),
            order: Vec::new(),
        }
    }

    /// The class of `css`, made on first use.
    fn class(&mut self, css: String) -> usize {
        if let Some(&c) = self.classes.get(&css) {
            return c;
        }
        let c = self.order.len();
        self.classes.insert(css.clone(), c);
        self.order.push(css);
        c
    }
}

/// The row `idx` as the view draws it: the class of its own style (a whole-row rule, else
/// its level colour), and its text cut into runs, each with the class of its style
/// (highlight rules, quick labels, ANSI colours, automatic tokens, search hits of
/// `query`). `class` turns a CSS declaration into a class number. Shared by the HTML
/// export and the web view.
pub fn row_runs(
    engine: &TailEngine,
    theme: &CyberTheme,
    level_colors: bool,
    query: &str,
    idx: usize,
    row: &RowText,
    class: &mut dyn FnMut(String) -> usize,
) -> (Option<usize>, Vec<(String, Option<usize>)>) {
    let text = &row.line;
    let found = engine.match_row_spans(row);
    let base: Option<HighlightStyle> = found
        .rest
        .or_else(|| crate::ui::dock::level_fallback(engine, theme, level_colors, idx));
    let spans: Vec<HighlightSpan> = crate::ui::dock::cell_spans(found.spans, text, query);
    let row_class = base.map(|s| class(css(Some(s.fg), Some(s.bg), s.bold, s.italic, false)));
    let base_fg = base.map_or(theme.text_primary(), |s| s.fg);
    let base_bg = base.map_or(Rgba::TRANSPARENT, |s| s.bg);
    let mut runs = Vec::new();
    let mut pos = 0;
    for sp in &spans {
        let (start, end) = (sp.start.min(text.len()), sp.end.min(text.len()));
        if start < pos
            || start >= end
            || !text.is_char_boundary(start)
            || !text.is_char_boundary(end)
        {
            continue;
        }
        if start > pos {
            runs.push((text[pos..start].to_string(), None));
        }
        let style = match sp.style {
            SpanStyle::Rule(s) => css(Some(s.fg), Some(s.bg), s.bold, s.italic, false),
            SpanStyle::Label(n) => {
                let (fg, bg) = theme.label_style(n);
                css(Some(fg), Some(bg), false, false, false)
            }
            SpanStyle::Ansi(s) => {
                let (fg, bg) = theme.ansi_colors(&s, base_fg, base_bg);
                css(Some(fg), Some(bg), s.bold, s.italic, s.underline)
            }
            SpanStyle::Token(kind) => css(Some(theme.token_color(kind)), None, false, false, false),
        };
        runs.push((text[start..end].to_string(), Some(class(style))));
        pos = end;
    }
    if pos < text.len() {
        runs.push((text[pos..].to_string(), None));
    }
    (row_class, runs)
}

/// `#rrggbb` of a colour (un-premultiplied), `None` for a transparent one.
pub(crate) fn hex(c: Rgba) -> Option<String> {
    if c.a == 0 {
        return None;
    }
    let un = |v: u8| (u32::from(v) * 255 / u32::from(c.a)).min(255);
    Some(format!("#{:02x}{:02x}{:02x}", un(c.r), un(c.g), un(c.b)))
}

/// The CSS of a style: colours, weight, slant, underline.
fn css(fg: Option<Rgba>, bg: Option<Rgba>, bold: bool, italic: bool, underline: bool) -> String {
    let mut out = String::new();
    if let Some(c) = fg.and_then(hex) {
        out.push_str(&format!("color:{c};"));
    }
    if let Some(c) = bg.and_then(hex) {
        out.push_str(&format!("background:{c};"));
    }
    if bold {
        out.push_str("font-weight:bold;");
    }
    if italic {
        out.push_str("font-style:italic;");
    }
    if underline {
        out.push_str("text-decoration:underline;");
    }
    out
}

/// `text` with `<`, `>`, `&` and `"` escaped.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}

impl RowWriter for HtmlWriter {
    fn begin(&mut self, out: &mut dyn Write, _: &TailEngine) -> std::io::Result<()> {
        let bg = hex(self.theme.bg_color()).unwrap_or_else(|| "#000000".into());
        let fg = hex(self.theme.text_primary()).unwrap_or_else(|| "#ffffff".into());
        let dim = hex(self.theme.text_dim()).unwrap_or_else(|| "#888888".into());
        write!(
            out,
            "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\"><title>{}</title>\n\
             <style>body{{background:{bg};color:{fg};margin:8px;\
             font:13px/1.35 ui-monospace,Consolas,monospace}}\
             .l{{white-space:pre-wrap;word-break:break-all}}\
             .n{{color:{dim};user-select:none;display:inline-block;min-width:7ch;\
             text-align:right;margin-right:1ch}}</style></head><body>\n",
            escape(&self.title)
        )
    }

    fn row(
        &mut self,
        out: &mut dyn Write,
        engine: &TailEngine,
        idx: usize,
        row: &RowText,
    ) -> std::io::Result<()> {
        let (theme, level_colors) = (self.theme, self.level_colors);
        let query = std::mem::take(&mut self.query);
        let (row_class, runs) =
            row_runs(engine, &theme, level_colors, &query, idx, row, &mut |css| {
                self.class(css)
            });
        self.query = query;
        match row_class {
            Some(c) => write!(out, "<div class=\"l c{c}\">")?,
            None => out.write_all(b"<div class=\"l\">")?,
        }
        write!(
            out,
            "<span class=\"n\">{}</span>",
            engine.shown_line_number(idx)
        )?;
        for (text, class) in runs {
            match class {
                Some(c) => write!(out, "<span class=\"c{c}\">{}</span>", escape(&text))?,
                None => out.write_all(escape(&text).as_bytes())?,
            }
        }
        out.write_all(b"</div>\n")
    }

    fn end(&mut self, out: &mut dyn Write, _: &TailEngine) -> std::io::Result<()> {
        out.write_all(b"<style>")?;
        for (c, style) in self.order.iter().enumerate() {
            write!(out, ".c{c}{{{style}}}")?;
        }
        out.write_all(b"</style>\n</body></html>\n")
    }
}
