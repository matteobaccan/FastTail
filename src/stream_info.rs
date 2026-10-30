// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Stream information: what a stream is and what it costs (path and size, lines, the time
//! the index took, the memory of the index and the block cache, the encoding, the field
//! parser, the timestamps and the source kind), for the window's dialog and the terminal's.
//!
//! Memory is counted from the sizes of the index vectors and the cached blocks, not read
//! from the allocator.

use crate::fields::FieldParser;
use crate::tail_engine::TailEngine;

/// A row of the information: what it says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
    Path,
    Source,
    Size,
    Lines,
    Visible,
    IndexTime,
    IndexMemory,
    CacheMemory,
    Encoding,
    Parser,
    Timestamps,
}

impl Item {
    /// The key of its label in `crate::i18n`.
    pub fn key(self) -> &'static str {
        match self {
            Item::Path => "info_path",
            Item::Source => "info_source",
            Item::Size => "info_size",
            Item::Lines => "info_lines",
            Item::Visible => "info_visible",
            Item::IndexTime => "info_index_time",
            Item::IndexMemory => "info_index_memory",
            Item::CacheMemory => "info_cache_memory",
            Item::Encoding => "info_encoding",
            Item::Parser => "info_parser",
            Item::Timestamps => "info_timestamps",
        }
    }
}

/// What kind of source a stream reads (its key in `crate::i18n`).
pub fn source_kind(engine: &TailEngine) -> &'static str {
    if engine.merged.is_some() {
        "info_kind_merged"
    } else if engine.derived.is_some() {
        "info_kind_filter_tab"
    } else if engine.is_stdin() {
        "info_kind_stdin"
    } else if engine.is_clipboard() {
        "info_kind_clipboard"
    } else if engine.compressed.is_some() {
        "info_kind_compressed"
    } else if engine.ssh.is_some() {
        "info_kind_ssh"
    } else if engine.system.is_some() {
        "info_kind_system"
    } else if engine.listener.is_some() {
        "info_kind_listener"
    } else if engine.debug_output.is_some() {
        "info_kind_debug"
    } else if engine.plugin.is_some() {
        "info_kind_plugin"
    } else if engine.rotation.is_some() {
        "info_kind_rotation"
    } else if engine.is_pattern() {
        "info_kind_pattern"
    } else {
        "info_kind_file"
    }
}

/// Bytes of the line index: offsets, cached levels and timestamps, visible lines.
pub fn index_bytes(engine: &TailEngine) -> u64 {
    let lines = engine.line_offsets.len() as u64;
    let visible = engine.filtered_lines.len() as u64;
    lines * 8
        + engine.cached_levels().len() as u64
        + engine.timestamp_cache().0.len() as u64 * 8
        + visible * 8
}

/// A byte count as `512 B`, `1.5 KB`, `2.0 MB`, `3.1 GB`.
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 3] = ["KB", "MB", "GB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut v = bytes as f64 / 1024.0;
    let mut unit = 0;
    while v >= 1024.0 && unit + 1 < UNITS.len() {
        v /= 1024.0;
        unit += 1;
    }
    format!("{v:.1} {}", UNITS[unit])
}

/// The parser's name as the stream bar shows it, `None` without one.
pub fn parser_name(engine: &TailEngine) -> Option<String> {
    Some(match engine.field_parser()?.as_ref() {
        FieldParser::Json => "JSON".to_string(),
        FieldParser::Logfmt => "logfmt".to_string(),
        FieldParser::Regex(_) => "regex".to_string(),
        FieldParser::Delimited(_) => "CSV / TSV".to_string(),
        FieldParser::Format(f) => f.name().to_string(),
    })
}

/// The rows of `engine`'s information. Values are language-neutral except `Source`,
/// `IndexTime` (while indexing), `Parser` (without one) and `Timestamps`, which carry an
/// i18n key the front end translates (`Value::Key`).
pub fn rows(engine: &TailEngine) -> Vec<(Item, Value)> {
    let plain = |s: String| Value::Text(s);
    let mut out = vec![
        (Item::Path, plain(engine.path.display().to_string())),
        (Item::Source, Value::Key(source_kind(engine))),
        (
            Item::Size,
            plain(format!(
                "{} ({} bytes)",
                human_bytes(engine.file_size),
                engine.file_size
            )),
        ),
        (Item::Lines, plain(engine.total_lines().to_string())),
        (Item::Visible, plain(engine.visible_lines().to_string())),
    ];
    out.push((
        Item::IndexTime,
        match (engine.index_pending, engine.index_ms) {
            (true, _) | (false, None) => Value::Key("info_indexing"),
            (false, Some(ms)) if ms < 1000 => plain(format!("{ms} ms")),
            (false, Some(ms)) => plain(format!("{:.2} s", ms as f64 / 1000.0)),
        },
    ));
    out.push((Item::IndexMemory, plain(human_bytes(index_bytes(engine)))));
    out.push((
        Item::CacheMemory,
        plain(human_bytes(engine.source.cached_bytes() as u64)),
    ));
    out.push((Item::Encoding, plain(engine.encoding.name().to_string())));
    out.push((
        Item::Parser,
        match parser_name(engine) {
            Some(name) => plain(name),
            None => Value::Key("info_none"),
        },
    ));
    out.push((
        Item::Timestamps,
        if engine.timestamps_usable() {
            Value::Key("info_yes")
        } else {
            Value::Key("info_no")
        },
    ));
    out
}

/// A value: text as it is, or an i18n key to translate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Text(String),
    Key(&'static str),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn a_file_reports_its_lines_size_and_kind() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(
            f,
            "2026-10-30 10:00:00 INFO a\n2026-10-30 10:00:01 WARN b\n"
        )
        .unwrap();
        f.flush().unwrap();
        let mut e = TailEngine::open(f.path()).unwrap();
        e.poll_updates();
        let rows = rows(&e);
        let get = |item| {
            rows.iter()
                .find(|(i, _)| *i == item)
                .map(|(_, v)| v.clone())
        };
        assert_eq!(get(Item::Lines), Some(Value::Text("2".into())));
        assert_eq!(get(Item::Source), Some(Value::Key("info_kind_file")));
        assert_eq!(get(Item::Encoding), Some(Value::Text("UTF-8".into())));
        assert!(matches!(get(Item::Size), Some(Value::Text(t)) if t.ends_with("(54 bytes)")));
        assert!(index_bytes(&e) >= 16);
        assert_eq!(human_bytes(1536), "1.5 KB");
        assert_eq!(human_bytes(3 * 1024 * 1024), "3.0 MB");
    }
}
