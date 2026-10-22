// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Boolean filter expressions: the include side of a filter as one expression, operands
//! joined by `AND`, `OR`, `NOT` (upper case only) and parentheses, two operands side by
//! side meaning `AND`. An operand is a bare word, a `"quoted phrase"`, a `/regex/`, or on
//! a stream with a field parser a field term (`status>=500`). Bare and quoted operands
//! follow the stream's case and regex toggles like a term; `/…/` is always a regex.
//!
//! The parser is hand-written (five rules); the result is a small tree in a `Vec`
//! (indices, no boxes) evaluated with short-circuit and no allocation per line.

use crate::scan_job::{FilterTerm, LineFields};

/// Longest expression accepted, in bytes.
pub const MAX_EXPR_BYTES: usize = 1024;
/// Most operands of one expression.
pub const MAX_OPERANDS: usize = 32;
/// Deepest nesting of parentheses and `NOT`s.
pub const MAX_DEPTH: usize = 16;

/// Why an expression does not compile: the byte where it goes wrong, the i18n key of the
/// message, and a detail (a regex error) when there is one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExprError {
    pub pos: usize,
    pub key: &'static str,
    pub detail: Option<String>,
}

impl ExprError {
    fn at(pos: usize, key: &'static str) -> Self {
        Self {
            pos,
            key,
            detail: None,
        }
    }
}

/// One node of the tree; children are indices into `FilterExpr::nodes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Node {
    /// An operand: its index in `FilterExpr::operands`.
    Leaf(usize),
    Not(usize),
    And(usize, usize),
    Or(usize, usize),
}

/// A compiled operand: the term, and whether it is a `/regex/` (always a regex, whatever
/// the stream's toggle).
#[derive(Debug, Clone)]
pub struct Operand {
    pub term: FilterTerm,
    pub forced_regex: bool,
}

/// A compiled expression.
#[derive(Debug, Clone)]
pub struct FilterExpr {
    /// The expression as typed.
    pub text: String,
    nodes: Vec<Node>,
    root: usize,
    pub operands: Vec<Operand>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    Open,
    Close,
    And,
    Or,
    Not,
    Word(String),
    Phrase(String),
    Regex(String),
}

/// Splits `text` into tokens with their byte positions.
fn tokenize(text: &str) -> Result<Vec<(usize, Tok)>, ExprError> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        match c {
            b'(' => {
                out.push((start, Tok::Open));
                i += 1;
            }
            b')' => {
                out.push((start, Tok::Close));
                i += 1;
            }
            b'"' | b'/' => {
                // `"` closes a phrase, `/` a regex; a backslash escapes the closer (and,
                // in a phrase, itself).
                let close = c;
                let mut value = String::new();
                let mut j = i + 1;
                let mut closed = false;
                while j < b.len() {
                    let d = b[j];
                    if d == b'\\' && j + 1 < b.len() {
                        let next = b[j + 1];
                        if next == close || (close == b'"' && next == b'\\') {
                            value.push(next as char);
                            j += 2;
                            continue;
                        }
                    }
                    if d == close {
                        closed = true;
                        break;
                    }
                    // Whole characters, so a multi-byte one is copied intact.
                    let ch = text[j..].chars().next().unwrap_or('\0');
                    value.push(ch);
                    j += ch.len_utf8().max(1);
                }
                if !closed {
                    let key = if close == b'"' {
                        "expr_err_unclosed_quote"
                    } else {
                        "expr_err_unclosed_regex"
                    };
                    return Err(ExprError::at(start, key));
                }
                out.push((
                    start,
                    if close == b'"' {
                        Tok::Phrase(value)
                    } else {
                        Tok::Regex(value)
                    },
                ));
                i = j + 1;
            }
            _ => {
                let mut j = i;
                while j < b.len() && !b[j].is_ascii_whitespace() && b[j] != b'(' && b[j] != b')' {
                    j += 1;
                }
                let word = &text[i..j];
                out.push((
                    start,
                    match word {
                        "AND" => Tok::And,
                        "OR" => Tok::Or,
                        "NOT" => Tok::Not,
                        _ => Tok::Word(word.to_string()),
                    },
                ));
                i = j;
            }
        }
    }
    Ok(out)
}

/// The toggles and the parser an expression's operands are compiled with.
#[derive(Debug, Clone, Copy)]
pub struct Toggles {
    pub case_sensitive: bool,
    pub is_regex: bool,
    /// The stream has a field parser: a bare `key=value` form is a field term.
    pub fields: bool,
}

struct Parser<'a> {
    toks: &'a [(usize, Tok)],
    at: usize,
    end: usize,
    nodes: Vec<Node>,
    operands: Vec<Operand>,
    toggles: Toggles,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.at).map(|(_, t)| t)
    }

    fn pos(&self) -> usize {
        self.toks.get(self.at).map_or(self.end, |(p, _)| *p)
    }

    fn push(&mut self, node: Node) -> usize {
        self.nodes.push(node);
        self.nodes.len() - 1
    }

    fn or(&mut self, depth: usize) -> Result<usize, ExprError> {
        let mut left = self.and(depth)?;
        while self.peek() == Some(&Tok::Or) {
            self.at += 1;
            let right = self.and(depth)?;
            left = self.push(Node::Or(left, right));
        }
        Ok(left)
    }

    fn and(&mut self, depth: usize) -> Result<usize, ExprError> {
        let mut left = self.unary(depth)?;
        loop {
            match self.peek() {
                Some(Tok::And) => self.at += 1,
                // Side by side: an implicit AND.
                Some(Tok::Word(_) | Tok::Phrase(_) | Tok::Regex(_) | Tok::Not | Tok::Open) => {}
                _ => return Ok(left),
            }
            let right = self.unary(depth)?;
            left = self.push(Node::And(left, right));
        }
    }

    fn unary(&mut self, depth: usize) -> Result<usize, ExprError> {
        if depth > MAX_DEPTH {
            return Err(ExprError::at(self.pos(), "expr_err_too_deep"));
        }
        match self.peek() {
            Some(Tok::Not) => {
                self.at += 1;
                let inner = self.unary(depth + 1)?;
                Ok(self.push(Node::Not(inner)))
            }
            Some(Tok::Open) => {
                let open = self.pos();
                self.at += 1;
                if self.peek() == Some(&Tok::Close) {
                    return Err(ExprError::at(self.pos(), "expr_err_missing_operand"));
                }
                let inner = self.or(depth + 1)?;
                if self.peek() != Some(&Tok::Close) {
                    return Err(ExprError::at(open, "expr_err_unclosed_paren"));
                }
                self.at += 1;
                Ok(inner)
            }
            Some(Tok::Word(_) | Tok::Phrase(_) | Tok::Regex(_)) => self.operand(),
            Some(Tok::Close) => Err(ExprError::at(self.pos(), "expr_err_unexpected_paren")),
            _ => Err(ExprError::at(self.pos(), "expr_err_missing_operand")),
        }
    }

    fn operand(&mut self) -> Result<usize, ExprError> {
        let (pos, tok) = self.toks[self.at].clone();
        self.at += 1;
        if self.operands.len() >= MAX_OPERANDS {
            return Err(ExprError::at(pos, "expr_err_too_many"));
        }
        let t = self.toggles;
        let (term, forced_regex) = match tok {
            Tok::Word(w) => (
                FilterTerm::build(&w, t.case_sensitive, t.is_regex, t.fields),
                false,
            ),
            Tok::Phrase(p) => (
                FilterTerm::build(&p, t.case_sensitive, t.is_regex, false),
                false,
            ),
            Tok::Regex(r) => (FilterTerm::build(&r, t.case_sensitive, true, false), true),
            _ => return Err(ExprError::at(pos, "expr_err_missing_operand")),
        };
        if term.needle.is_empty() && term.field.is_none() && !term.invalid {
            return Err(ExprError::at(pos, "expr_err_missing_operand"));
        }
        if term.invalid {
            let detail = (t.is_regex || forced_regex)
                .then(|| regex::Regex::new(&term.needle).err().map(|e| e.to_string()))
                .flatten();
            return Err(ExprError {
                pos,
                key: "expr_err_invalid_operand",
                detail,
            });
        }
        self.operands.push(Operand { term, forced_regex });
        Ok(self.push(Node::Leaf(self.operands.len() - 1)))
    }
}

impl FilterExpr {
    /// Compiles `text`; `Ok(None)` for an empty expression (no condition).
    pub fn compile(text: &str, toggles: Toggles) -> Result<Option<Self>, ExprError> {
        if text.len() > MAX_EXPR_BYTES {
            return Err(ExprError::at(MAX_EXPR_BYTES, "expr_err_too_long"));
        }
        let toks = tokenize(text)?;
        if toks.is_empty() {
            return Ok(None);
        }
        let mut p = Parser {
            toks: &toks,
            at: 0,
            end: text.len(),
            nodes: Vec::new(),
            operands: Vec::new(),
            toggles,
        };
        let root = p.or(0)?;
        if p.at < toks.len() {
            let key = match toks[p.at].1 {
                Tok::Close => "expr_err_unexpected_paren",
                _ => "expr_err_missing_operand",
            };
            return Err(ExprError::at(p.pos(), key));
        }
        Ok(Some(Self {
            text: text.to_string(),
            nodes: p.nodes,
            root,
            operands: p.operands,
        }))
    }

    /// Whether a field term is among the operands.
    pub fn has_field_terms(&self) -> bool {
        self.operands.iter().any(|o| o.term.field.is_some())
    }

    /// Whether `line` passes the expression.
    pub fn eval(&self, line: &str, toggles: Toggles, fields: &mut LineFields) -> bool {
        self.eval_node(self.root, line, toggles, fields)
    }

    fn eval_node(&self, node: usize, line: &str, t: Toggles, fields: &mut LineFields) -> bool {
        match self.nodes[node] {
            Node::Leaf(i) => {
                let o = &self.operands[i];
                o.term
                    .matches(line, t.case_sensitive, t.is_regex || o.forced_regex, fields)
            }
            Node::Not(a) => !self.eval_node(a, line, t, fields),
            Node::And(a, b) => {
                self.eval_node(a, line, t, fields) && self.eval_node(b, line, t, fields)
            }
            Node::Or(a, b) => {
                self.eval_node(a, line, t, fields) || self.eval_node(b, line, t, fields)
            }
        }
    }
}

/// An expression equal to the include `terms` (all of them, ANDed): words as they are,
/// anything else quoted. Used the first time a stream switches to expression mode.
pub fn from_terms(terms: &[String]) -> String {
    terms
        .iter()
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .map(|t| {
            let plain = !t.contains(|c: char| c.is_whitespace() || "()\"/".contains(c))
                && !matches!(t, "AND" | "OR" | "NOT");
            if plain {
                t.to_string()
            } else {
                format!("\"{}\"", t.replace('\\', "\\\\").replace('"', "\\\""))
            }
        })
        .collect::<Vec<_>>()
        .join(" AND ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan_job::FilterSpec;

    fn text() -> Toggles {
        Toggles {
            case_sensitive: false,
            is_regex: false,
            fields: false,
        }
    }

    fn spec_with(expr: &str, toggles: Toggles) -> FilterSpec {
        FilterSpec::build::<&str>(&[], &[], toggles.case_sensitive, toggles.is_regex).with_expr(
            FilterExpr::compile(expr, toggles)
                .unwrap()
                .map(std::sync::Arc::new),
        )
    }

    fn passes(expr: &str, line: &str) -> bool {
        spec_with(expr, text()).included(line)
    }

    fn err(expr: &str) -> ExprError {
        FilterExpr::compile(expr, text()).unwrap_err()
    }

    #[test]
    fn precedence_implicit_and_and_lower_case_words() {
        // NOT > AND > OR: `a OR b AND c` is `a OR (b AND c)`.
        assert!(passes("payment OR refund AND timeout", "payment ok"));
        assert!(!passes("payment OR refund AND timeout", "refund ok"));
        assert!(passes("(payment OR refund) AND timeout", "refund timeout"));
        assert!(
            !passes("(payment OR refund) timeout", "refund ok"),
            "implicit AND"
        );
        assert!(passes("error NOT retry", "error in worker"));
        assert!(!passes("error NOT retry", "error, retry 2"));
        assert!(
            passes("NOT NOT error", "ERROR"),
            "case-insensitive by toggle"
        );
        // Lower-case keywords are words.
        assert!(passes("not found", "page not found"));
        assert!(!passes("not found", "page found"));
        assert!(passes("error or warning", "error or warning seen"));
    }

    #[test]
    fn phrases_regexes_and_escapes() {
        assert!(passes("\"circuit open\"", "the circuit open now"));
        assert!(!passes("\"circuit open\"", "open circuit"));
        assert!(passes(r#""say \"hi\"""#, r#"they say "hi""#));
        assert!(passes(r"/time(out|d out)/ OR refused", "timed out"));
        assert!(passes(r"/a\/b/", "path a/b"));
        assert!(
            passes("/^ERR/", "ERR x"),
            "case toggle off: case-insensitive"
        );
        let cs = Toggles {
            case_sensitive: true,
            ..text()
        };
        assert!(!spec_with("/^err/", cs).included("ERR x"));
        // The regex toggle makes bare words regexes too.
        let re = Toggles {
            is_regex: true,
            ..text()
        };
        assert!(spec_with("time.ut", re).included("timeout"));
        assert!(passes("é", "café"), "multi-byte words");
        assert!(passes("\"café au\"", "un café au lait"));
    }

    #[test]
    fn errors_say_where_and_why() {
        assert_eq!(err("(payment").key, "expr_err_unclosed_paren");
        assert_eq!(err("(payment").pos, 0);
        assert_eq!(err("(payment OR").pos, 11, "the operand missing at the end");
        assert_eq!(err("payment OR").key, "expr_err_missing_operand");
        assert_eq!(err("payment OR").pos, 10);
        assert_eq!(err("a )").key, "expr_err_unexpected_paren");
        assert_eq!(err("a )").pos, 2);
        assert_eq!(err("()").key, "expr_err_missing_operand");
        assert_eq!(err("AND a").pos, 0);
        assert_eq!(err("\"open").key, "expr_err_unclosed_quote");
        assert_eq!(err("a /x").key, "expr_err_unclosed_regex");
        assert!(FilterExpr::compile("a /x/", text()).is_ok());
        let bad = err("ok /(/");
        assert_eq!((bad.key, bad.pos), ("expr_err_invalid_operand", 3));
        assert!(bad.detail.is_some(), "the regex error");
        // Limits.
        assert_eq!(err(&"a ".repeat(600)).key, "expr_err_too_long");
        assert_eq!(err(&"w ".repeat(MAX_OPERANDS + 1)).key, "expr_err_too_many");
        let deep = format!(
            "{}a{}",
            "(".repeat(MAX_DEPTH + 1),
            ")".repeat(MAX_DEPTH + 1)
        );
        assert_eq!(err(&deep).key, "expr_err_too_deep");
        assert!(
            FilterExpr::compile("   ", text()).unwrap().is_none(),
            "empty: no condition"
        );
    }

    #[test]
    fn field_operands_mix_with_text_on_a_stream_with_a_parser() {
        let parser = std::sync::Arc::new(crate::fields::FieldParser::Json);
        let t = Toggles {
            fields: true,
            ..text()
        };
        let expr = FilterExpr::compile("(status>=500 OR \"circuit open\") NOT env=test", t)
            .unwrap()
            .map(std::sync::Arc::new);
        assert!(expr.as_ref().unwrap().has_field_terms());
        let spec = FilterSpec::build::<&str>(&[], &[], false, false)
            .with_parser(Some(parser))
            .with_expr(expr);
        assert!(spec.included(r#"{"status":503,"env":"prod"}"#));
        assert!(!spec.included(r#"{"status":503,"env":"test"}"#));
        assert!(spec.included(r#"{"status":200,"msg":"circuit open"}"#));
        assert!(!spec.included(r#"{"status":200,"msg":"fine"}"#));
    }

    #[test]
    fn terms_become_an_expression() {
        let terms = vec![
            "payment".to_string(),
            "".into(),
            "time out".into(),
            "OR".into(),
        ];
        let expr = from_terms(&terms);
        assert_eq!(expr, r#"payment AND "time out" AND "OR""#);
        assert!(passes(&expr, "payment time out OR"));
        assert!(!passes(&expr, "payment time out"));
    }
}
