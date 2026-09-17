use regex::Regex;

/// Checks if a string contains HTML tags or HTML-like structures
pub fn contains_html(text: &str) -> bool {
    if !text.contains('<') {
        return false;
    }
    let lower = text.to_lowercase();
    lower.contains("<html")
        || lower.contains("<body")
        || lower.contains("<div")
        || lower.contains("<p")
        || lower.contains("<b")
        || lower.contains("<strong")
        || lower.contains("<i")
        || lower.contains("<em")
        || lower.contains("<h1")
        || lower.contains("<h2")
        || lower.contains("<h3")
        || lower.contains("<h4")
        || lower.contains("<h5")
        || lower.contains("<h6")
        || lower.contains("<a ")
        || lower.contains("<table")
        || lower.contains("<ul")
        || lower.contains("<ol")
        || lower.contains("<li")
        || lower.contains("<br")
        || lower.contains("<hr")
        || lower.contains("<pre")
        || lower.contains("<code")
        || lower.contains("<span")
        || lower.contains("</")
        || lower.contains("/>")
}

/// Converts HTML strings to Markdown formatted text
pub fn html_to_markdown(html: &str) -> String {
    let mut out = html.to_string();

    // 1. Remove comments <!-- ... -->
    if let Ok(re) = Regex::new(r"(?s)<!--.*?-->") {
        out = re.replace_all(&out, "").into_owned();
    }

    // 2. Remove <script> and <style> blocks
    if let Ok(re) = Regex::new(r"(?is)<script[^>]*>.*?</script>") {
        out = re.replace_all(&out, "").into_owned();
    }
    if let Ok(re) = Regex::new(r"(?is)<style[^>]*>.*?</style>") {
        out = re.replace_all(&out, "").into_owned();
    }

    // 3. Remove <head> block, but extract <title> to prepend if no h1 exists
    let title_opt = if let Ok(re) = Regex::new(r"(?is)<title[^>]*>(.*?)</title>") {
        re.captures(&out).map(|caps| caps[1].trim().to_string())
    } else {
        None
    };
    if let Ok(re) = Regex::new(r"(?is)<head[^>]*>.*?</head>") {
        out = re.replace_all(&out, "").into_owned();
    }
    if let Some(title) = title_opt {
        if !title.is_empty() && !Regex::new(r"(?i)<h1\b").unwrap().is_match(&out) {
            out = format!("# {}\n\n{}", title, out);
        }
    }

    // 4. Preformatted blocks <pre><code>...</code></pre> and <pre>...</pre>
    if let Ok(re) = Regex::new(r#"(?is)<pre\b[^>]*>\s*<code(?:\s+class=['"][^'"]*lang-([a-zA-Z0-9_-]+)[^'"]*['"])?[^>]*>(.*?)</code>\s*</pre>"#) {
        out = re.replace_all(&out, |caps: &regex::Captures| {
            let lang = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let code = decode_html_entities(&caps[2]);
            format!("\n\n```{}\n{}\n```\n\n", lang, code.trim_matches('\n'))
        }).into_owned();
    }
    if let Ok(re) = Regex::new(r"(?is)<pre\b[^>]*>(.*?)</pre>") {
        out = re.replace_all(&out, |caps: &regex::Captures| {
            let code = decode_html_entities(&caps[1]);
            format!("\n\n```\n{}\n```\n\n", code.trim_matches('\n'))
        }).into_owned();
    }

    // 5. Convert Tables: <table>...</table> -> Markdown table
    out = convert_tables(&out);

    // 6. Headings <h1> through <h6>
    for level in 1..=6 {
        let hashes = "#".repeat(level);
        if let Ok(re) = Regex::new(&format!(r"(?is)<h{level}\b[^>]*>(.*?)</h{level}>")) {
            out = re.replace_all(&out, |caps: &regex::Captures| {
                format!("\n\n{} {}\n\n", hashes, caps[1].trim())
            }).into_owned();
        }
    }

    // 7. Horizontal Rules <hr>
    if let Ok(re) = Regex::new(r"(?i)<hr\b\s*/?>") {
        out = re.replace_all(&out, "\n\n---\n\n").into_owned();
    }

    // 8. Line breaks <br>
    if let Ok(re) = Regex::new(r"(?i)<br\b\s*/?>") {
        out = re.replace_all(&out, "  \n").into_owned();
    }

    // 9. Links <a href="url">text</a>
    if let Ok(re) = Regex::new(r#"(?is)<a\b\s+[^>]*href=['"]([^'"]*)['"][^>]*>(.*?)</a>"#) {
        out = re.replace_all(&out, |caps: &regex::Captures| {
            let url = caps[1].trim();
            let text = caps[2].trim();
            if text.is_empty() {
                format!("<{}>", url)
            } else {
                format!("[{}]({})", text, url)
            }
        }).into_owned();
    }

    // 10. Images <img src="url" alt="text">
    if let Ok(re) = Regex::new(r#"(?is)<img\b\s+[^>]*src=['"]([^'"]*)['"][^>]*alt=['"]([^'"]*)['"][^>]*>"#) {
        out = re.replace_all(&out, "![$2]($1)").into_owned();
    }
    if let Ok(re) = Regex::new(r#"(?is)<img\b\s+[^>]*alt=['"]([^'"]*)['"][^>]*src=['"]([^'"]*)['"][^>]*>"#) {
        out = re.replace_all(&out, "![$1]($2)").into_owned();
    }
    if let Ok(re) = Regex::new(r#"(?is)<img\b\s+[^>]*src=['"]([^'"]*)['"][^>]*>"#) {
        out = re.replace_all(&out, "![]($1)").into_owned();
    }

    // 11. Text formatting: bold, italic, code, strikethrough
    if let Ok(re) = Regex::new(r"(?is)<(?:strong|b)\b[^>]*>(.*?)</(?:strong|b)>") {
        out = re.replace_all(&out, "**$1**").into_owned();
    }
    if let Ok(re) = Regex::new(r"(?is)<(?:em|i)\b[^>]*>(.*?)</(?:em|i)>") {
        out = re.replace_all(&out, "*$1*").into_owned();
    }
    if let Ok(re) = Regex::new(r"(?is)<(?:del|s|strike)\b[^>]*>(.*?)</(?:del|s|strike)>") {
        out = re.replace_all(&out, "~~$1~~").into_owned();
    }
    if let Ok(re) = Regex::new(r"(?is)<(?:code|kbd)\b[^>]*>(.*?)</(?:code|kbd)>") {
        out = re.replace_all(&out, "`$1`").into_owned();
    }
    if let Ok(re) = Regex::new(r"(?is)<mark\b[^>]*>(.*?)</mark>") {
        out = re.replace_all(&out, "**$1**").into_owned();
    }

    // 12. Lists: <ul>, <ol>, <li>
    if let Ok(re) = Regex::new(r"(?is)<ul\b[^>]*>(.*?)</ul>") {
        out = re.replace_all(&out, |caps: &regex::Captures| {
            let inner = &caps[1];
            if let Ok(li_re) = Regex::new(r"(?is)<li\b[^>]*>(.*?)</li>") {
                let items: Vec<String> = li_re.captures_iter(inner)
                    .map(|c| format!("* {}", c[1].trim()))
                    .collect();
                format!("\n\n{}\n\n", items.join("\n"))
            } else {
                format!("\n\n{}\n\n", inner.trim())
            }
        }).into_owned();
    }
    if let Ok(re) = Regex::new(r"(?is)<ol\b[^>]*>(.*?)</ol>") {
        out = re.replace_all(&out, |caps: &regex::Captures| {
            let inner = &caps[1];
            if let Ok(li_re) = Regex::new(r"(?is)<li\b[^>]*>(.*?)</li>") {
                let items: Vec<String> = li_re.captures_iter(inner)
                    .enumerate()
                    .map(|(i, c)| format!("{}. {}", i + 1, c[1].trim()))
                    .collect();
                format!("\n\n{}\n\n", items.join("\n"))
            } else {
                format!("\n\n{}\n\n", inner.trim())
            }
        }).into_owned();
    }
    // Any stray <li>
    if let Ok(re) = Regex::new(r"(?is)<li\b[^>]*>(.*?)</li>") {
        out = re.replace_all(&out, "\n* $1").into_owned();
    }

    // 13. Blockquotes
    if let Ok(re) = Regex::new(r"(?is)<blockquote\b[^>]*>(.*?)</blockquote>") {
        out = re.replace_all(&out, |caps: &regex::Captures| {
            let lines: Vec<String> = caps[1].trim().lines().map(|l| format!("> {}", l)).collect();
            format!("\n\n{}\n\n", lines.join("\n"))
        }).into_owned();
    }

    // 14. Paragraphs and block containers
    if let Ok(re) = Regex::new(r"(?is)<p\b[^>]*>(.*?)</p>") {
        out = re.replace_all(&out, "\n\n$1\n\n").into_owned();
    }
    if let Ok(re) = Regex::new(r"(?is)</?(?:div|section|article|header|footer|main|aside)\b[^>]*>") {
        out = re.replace_all(&out, "\n").into_owned();
    }

    // 15. Strip all remaining structural / unknown HTML tags (e.g. <span>, <body>, <html>, <font>)
    if let Ok(re) = Regex::new(r"<[^>]+>") {
        out = re.replace_all(&out, "").into_owned();
    }

    // 16. Decode HTML entities (&amp;, &lt;, &gt;, &quot;, &#39;, &nbsp;, etc.)
    out = decode_html_entities(&out);

    // 17. Clean up multiple empty lines
    if let Ok(re) = Regex::new(r"\n{3,}") {
        out = re.replace_all(&out, "\n\n").into_owned();
    }

    out.trim().to_string()
}

/// Converts HTML `<table>...</table>` elements into Markdown tables
fn convert_tables(html: &str) -> String {
    let Ok(table_re) = Regex::new(r"(?is)<table[^>]*>(.*?)</table>") else {
        return html.to_string();
    };
    let Ok(tr_re) = Regex::new(r"(?is)<tr[^>]*>(.*?)</tr>") else {
        return html.to_string();
    };
    let Ok(th_re) = Regex::new(r"(?is)<th[^>]*>(.*?)</th>") else {
        return html.to_string();
    };
    let Ok(td_re) = Regex::new(r"(?is)<td[^>]*>(.*?)</td>") else {
        return html.to_string();
    };

    table_re.replace_all(html, |caps: &regex::Captures| {
        let table_body = &caps[1];
        let mut rows: Vec<Vec<String>> = Vec::new();
        let mut is_header = Vec::new();

        for tr_cap in tr_re.captures_iter(table_body) {
            let tr_content = &tr_cap[1];
            let mut row = Vec::new();
            let mut row_is_th = false;

            for th_cap in th_re.captures_iter(tr_content) {
                row_is_th = true;
                let cell = th_cap[1].trim().replace('\n', " ");
                row.push(cell);
            }

            if !row_is_th {
                for td_cap in td_re.captures_iter(tr_content) {
                    let cell = td_cap[1].trim().replace('\n', " ");
                    row.push(cell);
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

        let mut md_table = String::new();
        md_table.push_str("\n\n");

        let has_explicit_th = is_header.iter().any(|&h| h);
        let header_row_idx = if has_explicit_th {
            is_header.iter().position(|&h| h).unwrap()
        } else {
            0
        };

        // Header row
        let header_row = &rows[header_row_idx];
        md_table.push_str("| ");
        for c in 0..num_cols {
            let val = header_row.get(c).map(|s| s.as_str()).unwrap_or("");
            md_table.push_str(val);
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
                let val = row.get(c).map(|s| s.as_str()).unwrap_or("");
                md_table.push_str(val);
                md_table.push_str(" | ");
            }
            md_table.push('\n');
        }

        md_table.push('\n');
        md_table
    }).into_owned()
}

/// Decodes standard HTML entities
pub fn decode_html_entities(text: &str) -> String {
    let mut s = text.replace("&nbsp;", " ")
        .replace("&amp;", "&")
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
    if let Ok(re) = Regex::new(r"&#(\d+);") {
        s = re.replace_all(&s, |caps: &regex::Captures| {
            if let Ok(num) = caps[1].parse::<u32>() {
                if let Some(ch) = char::from_u32(num) {
                    return ch.to_string();
                }
            }
            caps[0].to_string()
        }).into_owned();
    }

    // Numeric hex entities &#x1F600;
    if let Ok(re) = Regex::new(r"(?i)&#x([0-9a-f]+);") {
        s = re.replace_all(&s, |caps: &regex::Captures| {
            if let Ok(num) = u32::from_str_radix(&caps[1], 16) {
                if let Some(ch) = char::from_u32(num) {
                    return ch.to_string();
                }
            }
            caps[0].to_string()
        }).into_owned();
    }

    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_html_headings_and_formatting() {
        let html = "<h1>Title</h1><p>This is <b>bold</b> and <i>italic</i> and <code>code</code>.</p>";
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
        let html = "<table><tr><th>Name</th><th>Age</th></tr><tr><td>Alice</td><td>30</td></tr></table>";
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
}
