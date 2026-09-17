use regex::Regex;
use std::sync::LazyLock;

/// Compiles a regex once, on first use.
macro_rules! re {
    ($name:ident, $pattern:expr) => {
        static $name: LazyLock<Regex> =
            LazyLock::new(|| Regex::new($pattern).expect("valid static regex"));
    };
}

// --- detection ---------------------------------------------------------------------------

re!(RE_FENCED_CODE, r"(?s)```.*?```|~~~.*?~~~");
re!(RE_INLINE_CODE, r"`[^`\n]*`");
// A real opening tag: a known HTML element name followed by a word boundary, so that
// `Vec<i32>`, `Option<bool>` or `tool <path>` never count as markup.
re!(
    RE_OPENING_TAG,
    r"(?i)<(?:!doctype|html|head|body|div|p|br|hr|b|strong|i|em|u|s|del|strike|h[1-6]|a|img|table|thead|tbody|tr|th|td|ul|ol|li|pre|code|kbd|span|blockquote|section|article|header|footer|main|aside|font|mark|script|style|title)\b[^<>]*>"
);
re!(
    RE_CLOSING_TAG,
    r"(?i)</(?:html|head|body|div|p|b|strong|i|em|u|s|del|strike|h[1-6]|a|table|thead|tbody|tr|th|td|ul|ol|li|pre|code|kbd|span|blockquote|section|article|header|footer|main|aside|font|mark|script|style|title)\s*>"
);
re!(
    RE_VOID_TAG,
    r"(?i)<(?:br|hr|img|!doctype|html|body|meta|link)\b[^<>]*>"
);

/// Heuristically decides whether `text` is HTML rather than Markdown/plain text.
///
/// Fenced and inline code is ignored, and a document only counts as HTML when it contains a
/// real opening tag together with either a closing tag or a void/document-level tag.
pub fn contains_html(text: &str) -> bool {
    if !text.contains('<') {
        return false;
    }
    let without_fences = RE_FENCED_CODE.replace_all(text, "");
    let without_code = RE_INLINE_CODE.replace_all(&without_fences, "");
    RE_OPENING_TAG.is_match(&without_code)
        && (RE_CLOSING_TAG.is_match(&without_code) || RE_VOID_TAG.is_match(&without_code))
}

// --- conversion --------------------------------------------------------------------------

re!(RE_COMMENT, r"(?s)<!--.*?-->");
re!(RE_SCRIPT, r"(?is)<script[^>]*>.*?</script>");
re!(RE_STYLE, r"(?is)<style[^>]*>.*?</style>");
re!(RE_TITLE, r"(?is)<title[^>]*>(.*?)</title>");
re!(RE_HEAD, r"(?is)<head[^>]*>.*?</head>");
re!(RE_H1_PRESENT, r"(?i)<h1\b");
re!(
    RE_PRE_CODE,
    r#"(?is)<pre\b[^>]*>\s*<code(?:\s+class=['"][^'"]*lang-([a-zA-Z0-9_-]+)[^'"]*['"])?[^>]*>(.*?)</code>\s*</pre>"#
);
re!(RE_PRE, r"(?is)<pre\b[^>]*>(.*?)</pre>");
re!(RE_HR, r"(?i)<hr\b\s*/?>");
re!(RE_BR, r"(?i)<br\b\s*/?>");
re!(
    RE_LINK,
    r#"(?is)<a\b\s+[^>]*href=['"]([^'"]*)['"][^>]*>(.*?)</a>"#
);
re!(
    RE_IMG_SRC_ALT,
    r#"(?is)<img\b\s+[^>]*src=['"]([^'"]*)['"][^>]*alt=['"]([^'"]*)['"][^>]*>"#
);
re!(
    RE_IMG_ALT_SRC,
    r#"(?is)<img\b\s+[^>]*alt=['"]([^'"]*)['"][^>]*src=['"]([^'"]*)['"][^>]*>"#
);
re!(
    RE_IMG_SRC,
    r#"(?is)<img\b\s+[^>]*src=['"]([^'"]*)['"][^>]*>"#
);
re!(RE_BOLD, r"(?is)<(?:strong|b)\b[^>]*>(.*?)</(?:strong|b)>");
re!(RE_ITALIC, r"(?is)<(?:em|i)\b[^>]*>(.*?)</(?:em|i)>");
re!(
    RE_STRIKE,
    r"(?is)<(?:del|s|strike)\b[^>]*>(.*?)</(?:del|s|strike)>"
);
re!(RE_CODE, r"(?is)<(?:code|kbd)\b[^>]*>(.*?)</(?:code|kbd)>");
re!(RE_MARK, r"(?is)<mark\b[^>]*>(.*?)</mark>");
re!(RE_UL, r"(?is)<ul\b[^>]*>(.*?)</ul>");
re!(RE_OL, r"(?is)<ol\b[^>]*>(.*?)</ol>");
re!(RE_LI, r"(?is)<li\b[^>]*>(.*?)</li>");
re!(RE_BLOCKQUOTE, r"(?is)<blockquote\b[^>]*>(.*?)</blockquote>");
re!(RE_P, r"(?is)<p\b[^>]*>(.*?)</p>");
re!(
    RE_BLOCK_CONTAINER,
    r"(?is)</?(?:div|section|article|header|footer|main|aside)\b[^>]*>"
);
re!(RE_ANY_TAG, r"<[^>]+>");
re!(RE_BLANK_LINES, r"\n{3,}");
re!(RE_PLACEHOLDER, "\u{E000}PRE(\\d+)\u{E001}");

static RE_HEADINGS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    (1..=6)
        .map(|level| {
            Regex::new(&format!(r"(?is)<h{level}\b[^>]*>(.*?)</h{level}>"))
                .expect("valid static regex")
        })
        .collect()
});

/// Converts HTML strings to Markdown formatted text
pub fn html_to_markdown(html: &str) -> String {
    let mut out = html.to_string();

    // 1. Remove comments <!-- ... -->
    out = RE_COMMENT.replace_all(&out, "").into_owned();

    // 2. Remove <script> and <style> blocks
    out = RE_SCRIPT.replace_all(&out, "").into_owned();
    out = RE_STYLE.replace_all(&out, "").into_owned();

    // 3. Remove <head> block, but extract <title> to prepend if no h1 exists
    let title_opt = RE_TITLE
        .captures(&out)
        .map(|caps| caps[1].trim().to_string());
    out = RE_HEAD.replace_all(&out, "").into_owned();
    if let Some(title) = title_opt {
        if !title.is_empty() && !RE_H1_PRESENT.is_match(&out) {
            out = format!("# {}\n\n{}", title, out);
        }
    }

    // 4. Preformatted blocks. Their content is literal text: it is decoded now, parked in a
    //    placeholder so no later pass can mistake it for markup, and restored at the very end.
    let mut code_blocks: Vec<String> = Vec::new();
    let mut park = |block: String| -> String {
        code_blocks.push(block);
        format!("\n\n\u{E000}PRE{}\u{E001}\n\n", code_blocks.len() - 1)
    };
    out = RE_PRE_CODE
        .replace_all(&out, |caps: &regex::Captures| {
            let lang = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let code = decode_html_entities(&caps[2]);
            park(format!("```{}\n{}\n```", lang, code.trim_matches('\n')))
        })
        .into_owned();
    out = RE_PRE
        .replace_all(&out, |caps: &regex::Captures| {
            let code = decode_html_entities(&caps[1]);
            park(format!("```\n{}\n```", code.trim_matches('\n')))
        })
        .into_owned();

    // 5. Convert Tables: <table>...</table> -> Markdown table
    out = convert_tables(&out);

    // 6. Headings <h1> through <h6>
    for (idx, re) in RE_HEADINGS.iter().enumerate() {
        let hashes = "#".repeat(idx + 1);
        out = re
            .replace_all(&out, |caps: &regex::Captures| {
                format!("\n\n{} {}\n\n", hashes, caps[1].trim())
            })
            .into_owned();
    }

    // 7. Horizontal Rules <hr>
    out = RE_HR.replace_all(&out, "\n\n---\n\n").into_owned();

    // 8. Line breaks <br>
    out = RE_BR.replace_all(&out, "  \n").into_owned();

    // 9. Links <a href="url">text</a>
    out = RE_LINK
        .replace_all(&out, |caps: &regex::Captures| {
            let url = caps[1].trim();
            let text = caps[2].trim();
            // An empty anchor keeps its url as the visible text; a bare `<url>` autolink would
            // be eaten by the final tag strip.
            let text = if text.is_empty() { url } else { text };
            format!("[{}]({})", text, url)
        })
        .into_owned();

    // 10. Images <img src="url" alt="text">
    out = RE_IMG_SRC_ALT.replace_all(&out, "![$2]($1)").into_owned();
    out = RE_IMG_ALT_SRC.replace_all(&out, "![$1]($2)").into_owned();
    out = RE_IMG_SRC.replace_all(&out, "![]($1)").into_owned();

    // 11. Text formatting: bold, italic, code, strikethrough
    out = RE_BOLD.replace_all(&out, "**$1**").into_owned();
    out = RE_ITALIC.replace_all(&out, "*$1*").into_owned();
    out = RE_STRIKE.replace_all(&out, "~~$1~~").into_owned();
    out = RE_CODE.replace_all(&out, "`$1`").into_owned();
    out = RE_MARK.replace_all(&out, "**$1**").into_owned();

    // 12. Lists: <ul>, <ol>, <li>
    out = RE_UL
        .replace_all(&out, |caps: &regex::Captures| {
            let items: Vec<String> = RE_LI
                .captures_iter(&caps[1])
                .map(|c| format!("* {}", c[1].trim()))
                .collect();
            format!("\n\n{}\n\n", items.join("\n"))
        })
        .into_owned();
    out = RE_OL
        .replace_all(&out, |caps: &regex::Captures| {
            let items: Vec<String> = RE_LI
                .captures_iter(&caps[1])
                .enumerate()
                .map(|(i, c)| format!("{}. {}", i + 1, c[1].trim()))
                .collect();
            format!("\n\n{}\n\n", items.join("\n"))
        })
        .into_owned();
    // Any stray <li>
    out = RE_LI.replace_all(&out, "\n* $1").into_owned();

    // 13. Blockquotes
    out = RE_BLOCKQUOTE
        .replace_all(&out, |caps: &regex::Captures| {
            let lines: Vec<String> = caps[1].trim().lines().map(|l| format!("> {}", l)).collect();
            format!("\n\n{}\n\n", lines.join("\n"))
        })
        .into_owned();

    // 14. Paragraphs and block containers
    out = RE_P.replace_all(&out, "\n\n$1\n\n").into_owned();
    out = RE_BLOCK_CONTAINER.replace_all(&out, "\n").into_owned();

    // 15. Strip all remaining structural / unknown HTML tags (e.g. <span>, <body>, <html>, <font>)
    out = RE_ANY_TAG.replace_all(&out, "").into_owned();

    // 16. Decode HTML entities (&amp;, &lt;, &gt;, &quot;, &#39;, &nbsp;, etc.)
    out = decode_html_entities(&out);

    // 17. Clean up multiple empty lines
    out = RE_BLANK_LINES.replace_all(&out, "\n\n").into_owned();

    // 18. Restore the parked code blocks verbatim
    out = RE_PLACEHOLDER
        .replace_all(&out, |caps: &regex::Captures| {
            caps[1]
                .parse::<usize>()
                .ok()
                .and_then(|i| code_blocks.get(i).cloned())
                .unwrap_or_default()
        })
        .into_owned();

    out.trim().to_string()
}

re!(RE_TABLE, r"(?is)<table[^>]*>(.*?)</table>");
re!(RE_TR, r"(?is)<tr[^>]*>(.*?)</tr>");
re!(RE_TH, r"(?is)<th[^>]*>(.*?)</th>");
re!(RE_TD, r"(?is)<td[^>]*>(.*?)</td>");

/// Converts HTML `<table>...</table>` elements into Markdown tables
fn convert_tables(html: &str) -> String {
    RE_TABLE
        .replace_all(html, |caps: &regex::Captures| {
            let table_body = &caps[1];
            let mut rows: Vec<Vec<String>> = Vec::new();
            let mut is_header = Vec::new();

            for tr_cap in RE_TR.captures_iter(table_body) {
                let tr_content = &tr_cap[1];
                let mut row = Vec::new();
                let mut row_is_th = false;

                for th_cap in RE_TH.captures_iter(tr_content) {
                    row_is_th = true;
                    row.push(th_cap[1].trim().replace('\n', " "));
                }

                if !row_is_th {
                    for td_cap in RE_TD.captures_iter(tr_content) {
                        row.push(td_cap[1].trim().replace('\n', " "));
                    }
                }

                if !row.is_empty() {
                    rows.push(row);
                    is_header.push(row_is_th);
                }
            }

            if rows.is_empty() {
                return String::new();
            }

            let num_cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
            if num_cols == 0 {
                return String::new();
            }

            let header_row_idx = is_header.iter().position(|&h| h).unwrap_or(0);

            let mut md_table = String::from("\n\n");

            // Header row
            let header_row = &rows[header_row_idx];
            md_table.push_str("| ");
            for c in 0..num_cols {
                md_table.push_str(header_row.get(c).map(|s| s.as_str()).unwrap_or(""));
                md_table.push_str(" | ");
            }
            md_table.push('\n');

            // Separator row
            md_table.push_str("| ");
            for _ in 0..num_cols {
                md_table.push_str("--- | ");
            }
            md_table.push('\n');

            // Data rows
            for (i, row) in rows.iter().enumerate() {
                if i == header_row_idx {
                    continue;
                }
                md_table.push_str("| ");
                for c in 0..num_cols {
                    md_table.push_str(row.get(c).map(|s| s.as_str()).unwrap_or(""));
                    md_table.push_str(" | ");
                }
                md_table.push('\n');
            }

            md_table.push('\n');
            md_table
        })
        .into_owned()
}

re!(RE_ENTITY_DEC, r"&#(\d+);");
re!(RE_ENTITY_HEX, r"(?i)&#x([0-9a-f]+);");

/// Decodes standard HTML entities. `&amp;` is decoded last so that an escaped entity such
/// as `&amp;lt;` yields the literal text `&lt;` instead of being decoded twice.
pub fn decode_html_entities(text: &str) -> String {
    let mut s = text
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&copy;", "©")
        .replace("&reg;", "®")
        .replace("&euro;", "€")
        .replace("&mdash;", "—")
        .replace("&ndash;", "–");

    // Numeric decimal entities &#123;
    s = RE_ENTITY_DEC
        .replace_all(&s, |caps: &regex::Captures| {
            caps[1]
                .parse::<u32>()
                .ok()
                .and_then(char::from_u32)
                .map(|ch| ch.to_string())
                .unwrap_or_else(|| caps[0].to_string())
        })
        .into_owned();

    // Numeric hex entities &#x1F600;
    s = RE_ENTITY_HEX
        .replace_all(&s, |caps: &regex::Captures| {
            u32::from_str_radix(&caps[1], 16)
                .ok()
                .and_then(char::from_u32)
                .map(|ch| ch.to_string())
                .unwrap_or_else(|| caps[0].to_string())
        })
        .into_owned();

    s.replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_html_headings_and_formatting() {
        let html =
            "<h1>Title</h1><p>This is <b>bold</b> and <i>italic</i> and <code>code</code>.</p>";
        let md = html_to_markdown(html);
        assert!(md.contains("# Title"));
        assert!(md.contains("**bold**"));
        assert!(md.contains("*italic*"));
        assert!(md.contains("`code`"));
    }

    #[test]
    fn test_html_lists_and_links() {
        let html = r#"<p>Check <a href="https://example.com">this link</a></p><ul><li>Alpha</li><li>Beta</li></ul>"#;
        let md = html_to_markdown(html);
        assert!(md.contains("[this link](https://example.com)"));
        assert!(md.contains("* Alpha"));
        assert!(md.contains("* Beta"));
    }

    #[test]
    fn test_html_tables() {
        let html =
            "<table><tr><th>Name</th><th>Age</th></tr><tr><td>Alice</td><td>30</td></tr></table>";
        let md = html_to_markdown(html);
        assert!(md.contains("| Name | Age |"));
        assert!(md.contains("| --- | --- |"));
        assert!(md.contains("| Alice | 30 |"));
    }

    #[test]
    fn test_html_entities() {
        let html = "AT&amp;T &lt;corp&gt; &quot;rocks&quot; &copy; 2026";
        let md = html_to_markdown(html);
        assert_eq!(md, "AT&T <corp> \"rocks\" © 2026");
    }

    #[test]
    fn test_escaped_entities_are_not_double_decoded() {
        assert_eq!(
            decode_html_entities("&amp;lt;b&amp;gt; &amp;#39;"),
            "&lt;b&gt; &#39;"
        );
    }

    #[test]
    fn test_pre_content_survives_tag_stripping() {
        let md = html_to_markdown("<pre><code class=\"lang-rust\">let v: Vec&lt;i32&gt; = vec![];\n&lt;b&gt;x&lt;/b&gt;</code></pre>");
        assert_eq!(md, "```rust\nlet v: Vec<i32> = vec![];\n<b>x</b>\n```");
    }

    #[test]
    fn test_contains_html_requires_real_markup() {
        assert!(!contains_html("Vec<i32> and Option<bool> and <path>"));
        assert!(!contains_html("`<div>` in inline code"));
        assert!(contains_html("<div><p>x</p></div>"));
        assert!(contains_html("a<br>b"));
    }
}
