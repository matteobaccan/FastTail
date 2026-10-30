// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The next or previous line of the cursor row's operation (lnav's `o` / `O`): the line
//! whose operation id (the Operations view's source, else the first of the usual id
//! fields) equals the row's, among the visible lines.

use crate::field_stats::{Scratch, StatsSource};
use crate::tail_engine::TailEngine;

/// The id fields tried, in order, when the Operations view is not open.
pub const ID_KEYS: [&str; 6] = [
    "trace_id",
    "request_id",
    "req_id",
    "opid",
    "correlation_id",
    "thread",
];

/// Visible lines looked at, at most, for one jump.
pub const MAX_SCAN: usize = 2_000_000;

/// What a jump found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpJump {
    /// The line, and the id followed (`request_id=abc`).
    Found(usize, String),
    /// The row carries no operation id.
    NoId,
    /// No other line of the operation that way (or within `MAX_SCAN` lines).
    NotFound(String),
}

impl TailEngine {
    /// The operation id of line `line` and the source it was read with: the Operations
    /// view's source when it is open, else the first `ID_KEYS` field the line has.
    pub fn operation_id_of(&self, line: usize) -> Option<(StatsSource, String, String)> {
        let text = self.get_line(line)?;
        let parser = self.field_parser().map(|p| p.as_ref());
        let mut scratch = Scratch::default();
        let read = |source: &StatsSource, scratch: &mut Scratch| {
            source.with_value(parser, &text, scratch, |v| {
                v.filter(|v| !v.is_empty()).map(str::to_string)
            })
        };
        if let Some(ops) = &self.operations {
            let value = read(&ops.source, &mut scratch)?;
            return Some((ops.source.clone(), ops.source.label(), value));
        }
        ID_KEYS.iter().find_map(|key| {
            let source = StatsSource::Field((*key).to_string());
            let value = read(&source, &mut scratch)?;
            Some((source, (*key).to_string(), value))
        })
    }

    /// The next (`forward`) or previous visible line after or before `from` with the same
    /// operation id; the cursor goes there (follow stops).
    pub fn next_same_operation(&mut self, from: usize, forward: bool) -> OpJump {
        let Some((source, key, id)) = self.operation_id_of(from) else {
            return OpJump::NoId;
        };
        let label = format!("{key}={id}");
        let Some(start) = self.get_visible_row_of_line(from) else {
            return OpJump::NotFound(label);
        };
        let rows = self.visible_line_count();
        let parser = self.field_parser().cloned();
        let mut scratch = Scratch::default();
        let mut row = start;
        for _ in 0..MAX_SCAN {
            row = match forward {
                true if row + 1 < rows => row + 1,
                false if row > 0 => row - 1,
                _ => break,
            };
            let Some(line) = self.get_actual_line_idx(row) else {
                break;
            };
            let Some(text) = self.get_line(line) else {
                continue;
            };
            let same = source.with_value(parser.as_deref(), &text, &mut scratch, |v| {
                v == Some(id.as_str())
            });
            if same {
                self.follow_tail = false;
                self.reveal_line(line);
                self.scroll_to_line = Some(line);
                return OpJump::Found(line, label);
            }
        }
        OpJump::NotFound(label)
    }
}
