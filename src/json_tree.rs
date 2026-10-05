// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! A JSON payload as a foldable tree, in the order the line has it, shared by the GUI
//! (`[+] JSON` under a row) and the terminal interface (`J`). The payload is read once
//! into a flat list of nodes holding byte ranges into it: nothing is copied or unescaped
//! until a node is drawn or copied. `serde_json` is not used because, without its
//! `preserve_order` feature, it sorts the keys.

use std::collections::HashSet;
use std::fmt::Write as _;

/// A payload above this size is not read.
pub const MAX_BYTES: usize = 4 * 1024 * 1024;
/// Children of one container shown before a `… N more` row.
pub const MAX_CHILDREN_SHOWN: usize = 200;
/// Rows "expand all" opens at most.
pub const MAX_EXPAND_ROWS: usize = 5_000;
/// Characters of a string shown before `…`.
pub const MAX_STRING_CHARS: usize = 500;
/// Nesting read at most (deeper is reported as an error).
const MAX_DEPTH: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Object,
    Array,
    String,
    Number,
    Bool,
    Null,
}

impl Kind {
    pub fn is_container(self) -> bool {
        matches!(self, Kind::Object | Kind::Array)
    }
}

/// One value of the payload. Ranges are byte offsets into `Tree::text`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub kind: Kind,
    /// The key's text between its quotes, for a member of an object.
    pub key: Option<(usize, usize)>,
    /// Position among its parent's children.
    pub index: usize,
    /// The whole value (a string with its quotes).
    pub value: (usize, usize),
    pub depth: usize,
    pub parent: Option<usize>,
    /// Direct children.
    pub children: usize,
    /// The node after the last of its descendants.
    pub end: usize,
}

/// A row of the tree as shown: a node, or the `… N more` row closing a long container.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    Node { id: usize, depth: usize },
    More { depth: usize, hidden: usize },
}

/// A change to the open nodes of a tree (see `Tree::fold`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fold {
    /// A container opens or closes.
    Toggle(usize),
    /// The node and every container below it open.
    ExpandAll(usize),
    /// The node and every container below it close.
    CollapseAll(usize),
}

/// Why a line has no tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoTree {
    /// No object or array in the line.
    NotJson,
    /// Above `MAX_BYTES`.
    TooLarge,
}

/// The nodes of a payload; node 0 is the payload itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tree {
    text: String,
    nodes: Vec<Node>,
    /// Where the payload stopped being valid JSON: what came before is in `nodes`.
    error: Option<usize>,
}

/// Where the payload of `line` starts: an object or array making up the trimmed line, or
/// an object after a leading timestamp (as the field parser reads it).
pub fn payload_start(line: &str) -> Option<usize> {
    let trimmed = line.trim();
    let whole = (trimmed.starts_with('{') && trimmed.ends_with('}'))
        || (trimmed.starts_with('[') && trimmed.ends_with(']'));
    if whole {
        return Some(line.len() - line.trim_start().len());
    }
    let start = crate::fields::json_start(line)?;
    line.trim_end().ends_with('}').then_some(start)
}

/// Whether `line` carries a JSON payload (`payload_start`).
pub fn is_json_line(line: &str) -> bool {
    payload_start(line).is_some()
}

impl Tree {
    /// The tree of `line`'s payload.
    pub fn of_line(line: &str) -> Result<Tree, NoTree> {
        let start = payload_start(line).ok_or(NoTree::NotJson)?;
        let payload = line[start..].trim_end();
        if payload.len() > MAX_BYTES {
            return Err(NoTree::TooLarge);
        }
        Ok(Tree::parse(payload))
    }

    /// Reads `payload`; on malformed JSON the tree holds what came before the error.
    pub fn parse(payload: &str) -> Tree {
        let mut reader = Reader {
            b: payload.as_bytes(),
            pos: 0,
            nodes: Vec::new(),
        };
        let error = match reader.value(None, None, 0, 0) {
            Ok(()) => {
                let end = skip_ws(reader.b, reader.pos);
                (end < reader.b.len()).then_some(end)
            }
            Err(at) => Some(at),
        };
        // Containers left open by an error end where the reading stopped.
        let count = reader.nodes.len();
        let stop = reader.pos.min(payload.len());
        for node in &mut reader.nodes {
            if node.end == usize::MAX {
                node.end = count;
                node.value.1 = stop;
            }
        }
        Tree {
            text: payload.to_string(),
            nodes: reader.nodes,
            error,
        }
    }

    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    pub fn node(&self, id: usize) -> &Node {
        &self.nodes[id]
    }

    /// Byte offset in the payload where it stopped being valid JSON.
    pub fn error(&self) -> Option<usize> {
        self.error
    }

    /// A member's key, unescaped.
    pub fn key(&self, id: usize) -> Option<String> {
        let (a, b) = self.nodes.get(id)?.key?;
        Some(unescape(&self.text[a..b]))
    }

    /// A scalar as shown in the tree: a string in quotes, cut at `MAX_STRING_CHARS`.
    pub fn display_value(&self, id: usize) -> String {
        let node = &self.nodes[id];
        match node.kind {
            Kind::String => {
                let text = self.scalar_text(id);
                let mut cut: String = text.chars().take(MAX_STRING_CHARS).collect();
                if cut.len() < text.len() {
                    cut.push('…');
                }
                format!("\"{cut}\"")
            }
            Kind::Object | Kind::Array => String::new(),
            _ => self.raw(id).to_string(),
        }
    }

    /// A scalar's text: a string unescaped, a number, `true`, `false` or `null` as written.
    pub fn scalar_text(&self, id: usize) -> String {
        let node = &self.nodes[id];
        match node.kind {
            Kind::String => {
                let (a, b) = node.value;
                let inner = &self.text[(a + 1).min(b)..b.saturating_sub(1).max(a + 1)];
                unescape(inner)
            }
            _ => self.raw(id).to_string(),
        }
    }

    fn raw(&self, id: usize) -> &str {
        let (a, b) = self.nodes[id].value;
        &self.text[a..b]
    }

    /// The text "copy value" puts on the clipboard: a scalar's text, or a container as
    /// indented JSON in the payload's order.
    pub fn value_text(&self, id: usize) -> String {
        if !self.nodes[id].kind.is_container() {
            return self.scalar_text(id);
        }
        let mut out = String::new();
        self.write_pretty(id, 0, &mut out);
        out
    }

    fn write_pretty(&self, id: usize, indent: usize, out: &mut String) {
        let node = &self.nodes[id];
        let (open, close) = match node.kind {
            Kind::Object => ('{', '}'),
            Kind::Array => ('[', ']'),
            _ => {
                out.push_str(self.raw(id));
                return;
            }
        };
        if node.children == 0 {
            out.push(open);
            out.push(close);
            return;
        }
        out.push(open);
        out.push('\n');
        let mut child = id + 1;
        let mut first = true;
        while child < node.end {
            if !first {
                out.push_str(",\n");
            }
            first = false;
            out.push_str(&"  ".repeat(indent + 1));
            if let Some((a, b)) = self.nodes[child].key {
                let _ = write!(out, "\"{}\": ", &self.text[a..b]);
            }
            self.write_pretty(child, indent + 1, out);
            child = self.nodes[child].end;
        }
        out.push('\n');
        out.push_str(&"  ".repeat(indent));
        out.push(close);
    }

    /// The path of a node: `.key` for identifier keys, `["key"]` otherwise, `[n]` for an
    /// array item, without a leading dot (`items[0].id`). The payload itself is `""`.
    pub fn path(&self, id: usize) -> String {
        let mut steps = Vec::new();
        let mut at = id;
        while let Some(parent) = self.nodes[at].parent {
            let step = match self.nodes[parent].kind {
                Kind::Array => format!("[{}]", self.nodes[at].index),
                _ => {
                    let key = self.key(at).unwrap_or_default();
                    if is_identifier(&key) {
                        format!(".{key}")
                    } else {
                        let quoted = key.replace('\\', "\\\\").replace('"', "\\\"");
                        format!("[\"{quoted}\"]")
                    }
                }
            };
            steps.push(step);
            at = parent;
        }
        steps.reverse();
        let path = steps.concat();
        path.strip_prefix('.').map(str::to_string).unwrap_or(path)
    }

    /// The containers open when a line is first expanded: the payload itself.
    pub fn default_open(&self) -> HashSet<usize> {
        let mut open = HashSet::new();
        if self.nodes.first().is_some_and(|n| n.kind.is_container()) {
            open.insert(0);
        }
        open
    }

    /// The rows shown under `open`: the payload's children, each open container's own
    /// below it, at most `MAX_CHILDREN_SHOWN` per container. Depths start at 0 for the
    /// payload's children (the payload is not a row).
    pub fn rows(&self, open: &HashSet<usize>) -> Vec<Row> {
        let mut out = Vec::new();
        if self.nodes.first().is_some_and(|n| n.kind.is_container()) {
            self.push_children(0, 0, open, &mut out);
        }
        out
    }

    fn push_children(&self, id: usize, depth: usize, open: &HashSet<usize>, out: &mut Vec<Row>) {
        let node = &self.nodes[id];
        let mut child = id + 1;
        let mut shown = 0;
        while child < node.end {
            if shown == MAX_CHILDREN_SHOWN {
                out.push(Row::More {
                    depth,
                    hidden: node.children - shown,
                });
                return;
            }
            out.push(Row::Node { id: child, depth });
            if self.nodes[child].kind.is_container() && open.contains(&child) {
                self.push_children(child, depth + 1, open, out);
            }
            child = self.nodes[child].end;
            shown += 1;
        }
    }

    /// Opens `id` and every container below it, a level at a time, until the tree would
    /// show more than `MAX_EXPAND_ROWS` rows. Returns true when it stopped for that.
    pub fn expand_all(&self, id: usize, open: &mut HashSet<usize>) -> bool {
        let node = &self.nodes[id];
        if !node.kind.is_container() {
            return false;
        }
        open.insert(id);
        let mut depth = node.depth + 1;
        loop {
            let level: Vec<usize> = (id + 1..node.end)
                .filter(|&c| self.nodes[c].depth == depth && self.nodes[c].kind.is_container())
                .collect();
            if level.is_empty() {
                return false;
            }
            for &c in &level {
                open.insert(c);
            }
            if self.rows(open).len() > MAX_EXPAND_ROWS {
                for c in &level {
                    open.remove(c);
                }
                return true;
            }
            depth += 1;
        }
    }

    /// Applies `action` to `open`. Returns true when "expand all" stopped at
    /// `MAX_EXPAND_ROWS`.
    pub fn fold(&self, open: &mut HashSet<usize>, action: Fold) -> bool {
        match action {
            Fold::Toggle(id) => {
                if self.nodes.get(id).is_some_and(|n| n.kind.is_container()) && !open.remove(&id) {
                    open.insert(id);
                }
                false
            }
            Fold::ExpandAll(id) => id < self.nodes.len() && self.expand_all(id, open),
            Fold::CollapseAll(id) => {
                if id < self.nodes.len() {
                    self.collapse_all(id, open);
                }
                false
            }
        }
    }

    /// Folds `id` and every container below it (the payload itself stays open).
    pub fn collapse_all(&self, id: usize, open: &mut HashSet<usize>) {
        for c in id..self.nodes[id].end {
            if c != 0 {
                open.remove(&c);
            }
        }
    }
}

fn is_identifier(key: &str) -> bool {
    let mut chars = key.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn skip_ws(b: &[u8], mut pos: usize) -> usize {
    while pos < b.len() && matches!(b[pos], b' ' | b'\t' | b'\n' | b'\r') {
        pos += 1;
    }
    pos
}

/// The text of a JSON string's content with its escapes resolved; an escape that does not
/// read is kept as written.
fn unescape(s: &str) -> String {
    if !s.contains('\\') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some('/') => out.push('/'),
            Some('b') => out.push('\u{8}'),
            Some('f') => out.push('\u{c}'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('u') => {
                let hex = |chars: &mut std::iter::Peekable<std::str::Chars>| -> Option<u32> {
                    let h: String = (0..4).filter_map(|_| chars.next()).collect();
                    (h.len() == 4)
                        .then(|| u32::from_str_radix(&h, 16).ok())
                        .flatten()
                };
                match hex(&mut chars) {
                    Some(hi @ 0xD800..=0xDBFF) => {
                        let mut ahead = chars.clone();
                        let lo = (ahead.next() == Some('\\') && ahead.next() == Some('u'))
                            .then(|| hex(&mut ahead))
                            .flatten()
                            .filter(|lo| (0xDC00..=0xDFFF).contains(lo));
                        match lo {
                            Some(lo) => {
                                chars = ahead;
                                let code = 0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00);
                                out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                            }
                            None => out.push('\u{fffd}'),
                        }
                    }
                    Some(code) => out.push(char::from_u32(code).unwrap_or('\u{fffd}')),
                    None => out.push('\u{fffd}'),
                }
            }
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

struct Reader<'a> {
    b: &'a [u8],
    pos: usize,
    nodes: Vec<Node>,
}

impl Reader<'_> {
    fn push(
        &mut self,
        kind: Kind,
        key: Option<(usize, usize)>,
        index: usize,
        depth: usize,
        parent: Option<usize>,
    ) -> usize {
        self.nodes.push(Node {
            kind,
            key,
            index,
            value: (self.pos, usize::MAX),
            depth,
            parent,
            children: 0,
            end: usize::MAX,
        });
        self.nodes.len() - 1
    }

    fn close(&mut self, id: usize) {
        self.nodes[id].value.1 = self.pos;
        self.nodes[id].end = self.nodes.len();
    }

    /// Reads one value at the current position (whitespace first).
    fn value(
        &mut self,
        parent: Option<usize>,
        key: Option<(usize, usize)>,
        index: usize,
        depth: usize,
    ) -> Result<(), usize> {
        self.pos = skip_ws(self.b, self.pos);
        if depth > MAX_DEPTH {
            return Err(self.pos);
        }
        let Some(&c) = self.b.get(self.pos) else {
            return Err(self.pos);
        };
        match c {
            b'{' | b'[' => {
                let object = c == b'{';
                let kind = if object { Kind::Object } else { Kind::Array };
                let id = self.push(kind, key, index, depth, parent);
                self.pos += 1;
                let close = if object { b'}' } else { b']' };
                self.pos = skip_ws(self.b, self.pos);
                if self.b.get(self.pos) == Some(&close) {
                    self.pos += 1;
                    self.close(id);
                    return Ok(());
                }
                loop {
                    let member_key = if object {
                        self.pos = skip_ws(self.b, self.pos);
                        if self.b.get(self.pos) != Some(&b'"') {
                            return Err(self.pos);
                        }
                        let (a, b) = self.string()?;
                        self.pos = skip_ws(self.b, self.pos);
                        if self.b.get(self.pos) != Some(&b':') {
                            return Err(self.pos);
                        }
                        self.pos += 1;
                        Some((a + 1, b - 1))
                    } else {
                        None
                    };
                    let n = self.nodes[id].children;
                    self.value(Some(id), member_key, n, depth + 1)?;
                    self.nodes[id].children += 1;
                    self.pos = skip_ws(self.b, self.pos);
                    match self.b.get(self.pos) {
                        Some(b',') => self.pos += 1,
                        Some(&x) if x == close => {
                            self.pos += 1;
                            self.close(id);
                            return Ok(());
                        }
                        _ => return Err(self.pos),
                    }
                }
            }
            b'"' => {
                let id = self.push(Kind::String, key, index, depth, parent);
                self.string()?;
                self.close(id);
                Ok(())
            }
            b't' | b'f' | b'n' => {
                let (word, kind): (&[u8], Kind) = match c {
                    b't' => (b"true", Kind::Bool),
                    b'f' => (b"false", Kind::Bool),
                    _ => (b"null", Kind::Null),
                };
                if !self.b[self.pos..].starts_with(word) {
                    return Err(self.pos);
                }
                let id = self.push(kind, key, index, depth, parent);
                self.pos += word.len();
                self.close(id);
                Ok(())
            }
            b'-' | b'0'..=b'9' => {
                let id = self.push(Kind::Number, key, index, depth, parent);
                while self.pos < self.b.len()
                    && matches!(
                        self.b[self.pos],
                        b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9'
                    )
                {
                    self.pos += 1;
                }
                self.close(id);
                Ok(())
            }
            _ => Err(self.pos),
        }
    }

    /// Reads a string at the current position (on its opening quote). Returns its range,
    /// quotes included.
    fn string(&mut self) -> Result<(usize, usize), usize> {
        let start = self.pos;
        let mut pos = self.pos + 1;
        while pos < self.b.len() {
            match self.b[pos] {
                b'\\' => pos += 2,
                b'"' => {
                    self.pos = pos + 1;
                    return Ok((start, self.pos));
                }
                _ => pos += 1,
            }
        }
        self.pos = self.b.len();
        Err(start)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(tree: &Tree, open: &HashSet<usize>) -> Vec<String> {
        tree.rows(open)
            .into_iter()
            .map(|row| match row {
                Row::Node { id, depth } => {
                    let node = tree.node(id);
                    let key = tree.key(id).map(|k| format!("{k}: ")).unwrap_or_default();
                    let value = match node.kind {
                        Kind::Object => format!("{{{}}}", node.children),
                        Kind::Array => format!("[{}]", node.children),
                        _ => tree.display_value(id),
                    };
                    format!("{}{key}{value}", "  ".repeat(depth))
                }
                Row::More { depth, hidden } => format!("{}… {hidden} more", "  ".repeat(depth)),
            })
            .collect()
    }

    #[test]
    fn the_tree_keeps_the_order_of_the_line_and_folds_below_the_first_level() {
        let tree =
            Tree::of_line(r#"{"b":1,"a":{"c":[1,2]},"s":"x\"y","n":null,"t":true}"#).unwrap();
        assert_eq!(tree.error(), None);
        let mut open = tree.default_open();
        assert_eq!(
            shown(&tree, &open),
            vec!["b: 1", "a: {1}", r#"s: "x"y""#, "n: null", "t: true"]
        );
        // Unfold `a`, then everything.
        open.insert(2);
        assert_eq!(shown(&tree, &open)[1..3], ["a: {1}", "  c: [2]"]);
        assert!(!tree.expand_all(0, &mut open));
        assert_eq!(shown(&tree, &open)[2..5], ["  c: [2]", "    1", "    2"]);
        tree.collapse_all(2, &mut open);
        assert_eq!(shown(&tree, &open).len(), 5);
    }

    #[test]
    fn json_after_a_timestamp_is_found_and_plain_text_is_not() {
        let tree = Tree::of_line(r#"2026-10-05T10:00:00Z {"level":"error","id":7}"#).unwrap();
        assert_eq!(tree.rows(&tree.default_open()).len(), 2);
        assert_eq!(Tree::of_line("plain text"), Err(NoTree::NotJson));
        assert!(is_json_line("  [1, 2] "));
        assert!(!is_json_line("ERROR {not at the end"));
    }

    #[test]
    fn paths_and_values_to_copy() {
        let tree =
            Tree::of_line(r#"{"items":[{"id":7}],"x-req":{"a b":"q\"\\"},"_ok":1}"#).unwrap();
        let id_of = |key: &str| {
            (0..tree.nodes().len())
                .find(|&i| tree.key(i).as_deref() == Some(key))
                .unwrap()
        };
        assert_eq!(tree.path(id_of("id")), "items[0].id");
        assert_eq!(tree.path(id_of("a b")), r#"["x-req"]["a b"]"#);
        assert_eq!(tree.path(id_of("_ok")), "_ok");
        assert_eq!(tree.value_text(id_of("a b")), r#"q"\"#);
        assert_eq!(
            tree.value_text(id_of("items")),
            "[\n  {\n    \"id\": 7\n  }\n]"
        );
        assert_eq!(tree.value_text(id_of("_ok")), "1");
    }

    #[test]
    fn escapes_and_long_strings() {
        let tree = Tree::parse(r#"["è😀\n", "bad \uZZ"]"#);
        assert_eq!(tree.scalar_text(1), "è😀\n");
        assert_eq!(tree.scalar_text(2), "bad \u{fffd}");
        let long = format!("[\"{}\"]", "x".repeat(600));
        let tree = Tree::parse(&long);
        let shown = tree.display_value(1);
        assert_eq!(shown.chars().count(), MAX_STRING_CHARS + 3);
        assert!(shown.ends_with("…\""));
    }

    #[test]
    fn long_containers_are_cut_and_invalid_json_keeps_what_was_read() {
        let items: Vec<String> = (0..250).map(|i| i.to_string()).collect();
        let tree = Tree::parse(&format!("[{}]", items.join(",")));
        let rows = tree.rows(&tree.default_open());
        assert_eq!(rows.len(), MAX_CHILDREN_SHOWN + 1);
        assert_eq!(
            rows.last(),
            Some(&Row::More {
                depth: 0,
                hidden: 50
            })
        );

        let tree = Tree::parse(r#"{"a":1,"b":{"c":tru}"#);
        assert_eq!(tree.error(), Some(16));
        assert_eq!(shown(&tree, &tree.default_open()), vec!["a: 1", "b: {0}"]);
    }

    #[test]
    fn expand_all_stops_before_too_many_rows() {
        let row: Vec<String> = (0..100).map(|i| i.to_string()).collect();
        let inner: Vec<String> = (0..100).map(|_| format!("[{}]", row.join(","))).collect();
        let tree = Tree::parse(&format!("[[{}]]", inner.join(",")));
        let mut open = tree.default_open();
        assert!(tree.expand_all(0, &mut open));
        assert!(tree.rows(&open).len() <= MAX_EXPAND_ROWS);
        assert!(open.contains(&1));
    }

    #[test]
    fn a_huge_payload_is_refused() {
        let line = format!("[\"{}\"]", "x".repeat(MAX_BYTES));
        assert_eq!(Tree::of_line(&line), Err(NoTree::TooLarge));
    }
}
