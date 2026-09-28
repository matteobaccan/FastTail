## Why

A 2 GB service log holds a few hundred kinds of message repeated millions of times with
different ids, durations and users. Collapse (0.12.0) folds *consecutive* repeats only;
the level filter and the histogram say how many errors there are, not which messages they
are. To learn "what does this log actually say", the user scrolls, or guesses search terms
one at a time. gonzo clusters lines with the Drain algorithm, Grafana Loki's Logs
Drilldown shows the patterns of a stream with their volume, and Kibana has `CATEGORIZE`;
the post-0.12.0 competitor scan lists pattern grouping as gap 6 (value medium) and a
market signal ("patterns and summaries").

## What Changes

- A **Patterns** tab for a stream (stream menu "Patterns…", or `CTRL + SHIFT + G` on the
  focused stream): every entry the stream shows is assigned to a **pattern**, a template
  where the variable parts are masked as `<*>`
  (`payment <*> failed after <*> ms: gateway timeout`).
- Patterns are learnt with a **Drain-style** fixed-depth parse tree over the entry's first
  line, after removing the leading timestamp and masking numbers, hex values, UUIDs and IP
  addresses (the masks of collapse's Numbers mode plus IPs).
- The tab lists, per pattern: **count**, **share** of the view, a **sparkline** of its
  volume over the stream's time span (over line positions when the stream has no usable
  timestamps), **first seen** and **last seen** (line number and timestamp), and the
  template with its masked parts dimmed. It is sortable by each column and has a text box
  that narrows the list to templates containing a text.
- **Click a pattern** to filter the stream to it; `CTRL + click` adds patterns to the
  selection (OR between them); the row menu offers **Hide this pattern** (exclude). The
  pattern filter is shown as a chip in the stream bar (`⧉ 2 patterns`), combines with
  every other filter (AND), and is cleared with the chip's `✖`.
- **Scope**: the patterns are learnt from the lines the stream shows under its other
  filters, and are relearnt when those filters change; the pattern filter itself does not
  trigger relearning, so the list stays stable while the user clicks through it.
- **Background job**: learning always runs on a worker with progress in the tab (a stream
  of any size); appended lines are added incrementally; truncation or rotation relearns.
- **Limits**: 2,000 patterns per stream (further entries are counted under "other
  patterns"), the first 4,096 bytes and 128 tokens of a line, 64 sparkline buckets.
- Not persisted: patterns, the tab and the pattern filter are rebuilt per run. No new key
  in `fasttail.ini`.

Target release: **0.14.0** (structured logs and analysis), per the release plan in
`docs/competitor-analysis.md` section 8. Priority: **medium**. Effort: **M (1–3 weeks)**.

### Non-goals

- Patterns across streams (one tab per stream), or in a merged view.
- Naming, pinning or saving patterns, or alerting on new patterns.
- Patterns computed on field values (JSON messages are clustered on the raw line; with
  `structured-fields`, clustering the `msg` field is a follow-up).
- AI summaries of patterns.
- Replacing collapse: collapse folds consecutive repeats in the text view; patterns group
  across the whole view in a separate tab.

## Capabilities

### New Capabilities

- `pattern-grouping`: the Patterns tab, how patterns are learnt, the pattern filter, the
  background job and the limits.

### Modified Capabilities

(none)

## Impact

- `src/patterns.rs` (new): tokenizer and masks, the Drain tree (`PatternTree`), frozen
  snapshots (`Arc<PatternSnapshot>`) for classification without insertion, per-pattern
  statistics.
- `src/scan_job.rs`: `JobSpec::Patterns` / `ScanBatch::Patterns`; `FilterSpec` gains
  `patterns: Option<Arc<PatternFilter>>` (included and excluded pattern ids plus the
  snapshot).
- `src/tail_engine.rs`: pattern state per stream, incremental feed of appended lines,
  reset on reload.
- `src/ui/patterns_tab.rs` (new) and `src/ui/dock.rs` / `app.rs`: the tab, the chip, the
  shortcut, the row menu.
- `src/i18n.rs` (every language), help dialog, README, CHANGELOG, tests, a bench.
