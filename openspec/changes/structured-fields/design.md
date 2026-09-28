## Context

A row is drawn from `TailEngine::get_row(line)` (`RowText`: the decoded line, ANSI runs
stripped or kept) and styled by `match_row_spans` (rules, quick labels, ANSI, at most
`MAX_ROW_SPANS` = 64 spans) in `render_extended_rows` / `render_wrapped_rows`
(`src/ui/dock.rs`), after the marker, line-number and time-delta columns. Visibility is
decided by `FilterSpec::visible_in_sequence` (`src/scan_job.rs`) on the UI thread up to
16 MB and in a `Filter` `ScanJob` above; the same `FilterSpec` feeds the `Search` jobs and
the Find results jobs (`src/find_all.rs`). Levels come from `log_level::detect_level`
(first 96 bytes) into `levels: Vec<u8>`; timestamps from `timestamp::detect_timestamp`
(first 64 bytes) into `timestamps: Vec<i64>`, both filled by background jobs on large
files. Per-stream view state lives in `StreamEntry` (`src/session.rs`). `serde_json` and
`regex` are already dependencies.

## Goals / Non-Goals

**Goals:**
- JSON, logfmt and regex fields shown as columns, with no per-line memory and no parse of
  lines that are not drawn or filtered.
- Field filters that use the existing filter pipeline (terms, presets, global filter,
  background jobs, Find results) rather than a second one.
- Existing users see no change: a stream without a parser, or with the text view, behaves
  exactly as in 0.12.0.

**Non-Goals:**
- Query language, sorting, statistics, more formats (see the proposal).

## Decisions

1. **Borrowing scanners, not a tree.** `fields::scan(parser, line, &mut FieldSpans)`
   fills a reused `SmallVec<[FieldSpan; 32]>` with `FieldSpan { key: Range<u32>,
   value: Range<u32>, quoted: bool }` byte ranges into the line; no allocation per line
   once warmed up. JSON: a hand-written scanner over the top-level object that flattens
   nested objects as `a.b` up to depth 3 (deeper objects and arrays are one raw value),
   decodes escapes only when a value is displayed or compared, and stops at the first
   syntax error (fields found so far are kept, the line is marked partial). The key path
   of a nested field is built into a per-scan scratch buffer. Logfmt: `key=value` pairs
   separated by spaces, `"…"` values with `\"` escapes, bare `key` as an empty value; text
   before the first pair (a timestamp and level, common in logfmt) becomes the `_prefix`
   field. Regex: `regex::Regex::captures_read` with the pattern's named groups; the whole
   unmatched line goes to the message column. *Rejected:* `serde_json::Value` per line —
   one allocation per value, ~10× slower on the filter path, and it re-orders nothing we
   need. `serde_json` stays for the existing inline pretty-printer.
2. **Detection.** On open (and again once, when a stream that opened with fewer than 200
   lines first reaches 200), the engine samples the first 200 non-continuation lines
   (at most 256 KB read through the block cache). JSON wins when at least 80 % of them
   start with `{` after optional whitespace (or after a leading timestamp, see
   `timestamp::leading_span`) and scan without error; logfmt wins when at least 80 %
   have 3 or more `key=value` pairs; otherwise no parser. The regex presets are never
   auto-detected: they are offered in the parser menu. Detection runs once; the result
   is shown as a chip (`{}` JSON, `k=v` logfmt, `(?)` regex) and the user can change or
   force it (`fields_parser=`). *Rejected:* re-detecting on every append — a stream's
   columns would change under the user when one odd line arrives.
3. **Column view as a layout of the Text view**, not a new `ViewMode`: filters, search,
   bookmarks, collapse, context lines, wrap and follow keep their code paths; only the row
   painter changes. Columns: the marker, line-number and delta columns as today, then the
   shown fields in the saved order, then `message` (for JSON the `msg` / `message` field
   when present, otherwise the rest of the line; for logfmt `msg`; for regex the text not
   captured). The field catalogue per stream is the union of the keys seen in the detection
   sample and in the rows drawn since, capped at 256 keys (then "more fields not listed");
   by default the first 8 keys of the sample are shown. The header row supports drag to
   reorder, a right-click menu (hide, show all, move left / right, reset), and width drag;
   widths are in character cells. A row that does not parse, or a continuation line, is
   drawn from the first field column across the whole width, dimmed when it does not
   parse. In wrap mode only the last column wraps.
   *Rejected:* a new `ViewMode::Columns` beside Text, HEX and Markdown: every feature
   gated on the Text view (filters, collapse, show in context, wrap) would need a second
   gate, and switching would lose the Text view state.
4. **Row field cache.** `RefCell<FieldRowCache>`: an LRU of at most 1,024 rows keyed by
   `(line, reload_generation, parser_generation)` holding the spans (≤ 32 × 20 bytes) and
   the line text reference; about 700 KB at worst per stream in column view, nothing in
   text view. Scrolling re-parses only rows not in the cache (a 1 KB JSON line scans in
   under 5 µs; 80 rows per frame under 0.5 ms).
   *Rejected:* keeping the parsed fields of every line seen: memory would grow with the
   scrolling, which the engine rule forbids.
5. **Field filter terms.** A term is a field term when (a) the stream has an active parser
   and (b) it matches `^([A-Za-z_@][A-Za-z0-9_.@-]*)\s*(=|!=|~=|>=|<=|>|<)\s*(.*)$` and is
   not wrapped in double quotes (a quoted term is literal text without the quotes).
   `FilterTerm` gains `field: Option<FieldTerm { key, op, values: SmallVec<[Value; 2]> }>`,
   built in `FilterSpec::build` when the spec has a parser. Semantics: `=` equal to one of
   the `|` alternatives (case per the `Aa` toggle; with `.*` each alternative is a regex
   matched against the whole value), `!=` equal to none, `~=` contains (regex with `.*`),
   `>`, `>=`, `<`, `<=` numeric (`f64` parse of the value; a non-numeric value fails).
   A line without the field fails every field term, so on the exclude side it is not
   excluded. `included` / `excluded` scan the line's fields at most once per call (a
   stack-local `FieldSpans`), only when the spec has a field term. Stream terms, global
   terms, presets and the Find results all go through the same `FilterSpec`, so they
   support field terms with no extra code. A global field term applies to each stream with
   that stream's parser; on a stream without a parser it is text, like any term there.
   *Rejected:* a separate "field filter" box beside the terms: a second filter pipeline
   for the jobs, presets, global filter and Find results to learn, for no gain in
   expressiveness. *Rejected:* recognising field terms on every stream: `retry=0` typed
   today as text would change meaning on plain logs.
6. **OR / NOT.** OR is expressed by value alternatives (`level=error|fatal`) and, across
   fields, by a text or regex term; NOT by the exclude side or `!=`. *Rejected for
   0.14.0:* a boolean expression language (`(a AND b) OR NOT c`) — it needs a parser, a
   precedence model, an editor with errors, and it changes the meaning of text people
   already type into terms; better as its own "query bar" change once fields exist.
7. **Level and timestamp from fields (phase 3).** When the parser is active, the level of a
   line is the value of its first level field (`level`, `lvl`, `severity`, `log.level`,
   mapped with `log_level::token_level`), falling back to `detect_level`; the timestamp is
   the first time field (`ts`, `time`, `timestamp`, `@timestamp`, `t`) read with the
   existing parsers (ISO 8601, epoch s / ms), falling back to `detect_timestamp`.
   `JobSpec::Levels` and `JobSpec::Timestamps` receive `Option<Arc<FieldParser>>` so the
   background and synchronous paths agree. Changing the parser clears the level and
   timestamp caches (a new `reload`-like reset without touching the line index).
   *Rejected:* a user-configured level and time field per stream: the common names cover
   the formats in the wild (Bunyan, pino, zap, logrus, ECS, Serilog compact); a setting
   can be added if a real log needs it.
8. **Search, rules, copy, export.** They keep working on the line text, unchanged: a text
   search for `503` still finds it anywhere. In the column view, hit tints and rule spans
   are computed on the line as today and mapped to the cells by byte range (a span
   crossing a cell boundary is split); whole-row rule styles colour the row. "Copy" and
   "Export visible lines..." write the raw lines; "Copy as shown" in the column view
   writes the shown cells separated by tabs, one row per line.
   *Rejected:* making search field-aware (`status:503`): field terms already give that
   in the filter, and a search that sometimes matches text and sometimes fields is harder
   to predict.
9. **Persistence.** In `StreamEntry`: `fields_parser` (`off`, `json`, `logfmt`, `regex`,
   `apache`, `syslog`; absent = auto), `fields_regex` (through `filter_preset::ini_value`),
   `fields_view=true` (only when on), `fields_columns=ts,level,status,msg` (the shown
   fields in order; absent = default), `fields_width.<field>=<cells>`. Older builds ignore
   all of them.
   *Rejected:* one packed `fields=` value holding parser, columns and widths: harder to
   read and to merge by hand, and an older build could not ignore just one part.

Threads and memory: detection and row parsing run on the UI thread (bounded: 256 KB sample
once, ≤ 80 rows per frame); field filters run where filters already run (UI thread up to
16 MB, `Filter` job above) at a target of at least 100 MB/s per core on JSON with one field
term (measured by the new bench). Memory per stream: the catalogue (≤ 256 keys), the row
cache (≤ 1,024 rows, only in column view); nothing per line. Growing files: appended lines
are filtered with the same spec; the catalogue grows with the rows drawn. Rotation and
truncation clear the row cache with `reload_generation`. Windows sharing is unchanged.

## Risks / Trade-offs

- [`level=error` typed as text today now means a field term] → only on streams with an
  active parser, where it matches the same lines in logfmt; quoting keeps the old meaning;
  the term row shows a small `ƒ` badge on field terms so the difference is visible.
- [Auto-detection picks JSON on a log that is only half JSON] → 80 % threshold, the chip
  shows the choice, and the parser menu can force another one or off.
- [Wide JSON objects with hundreds of keys] → catalogue cap 256, 8 shown by default,
  depth 3.
- [A field filter on a 10 GB JSON log is slower than a text filter] → it runs in the
  background with progress, as any filter; a text pre-check (the value must occur in the
  line) skips the field scan on most lines for `=` and `~=` terms.
- [Scope] → three phases, each shippable; phase 3 can move to 0.15.0 without leaving a
  half feature.

## Migration Plan

Additive. A stream restored without `fields_*` keys runs detection and opens in the text
view, so it looks as before; only the chip is new. Rollback: remove the view and the
parser; saved keys are ignored and field terms become text terms again.

## Open Questions

- Should detection also run on a stream restored from a session that saved
  `fields_parser`? (Proposed: no, the saved choice wins.)
- Default columns: the first 8 keys of the sample, or a fixed preference list (`ts`,
  `level`, `logger`, `msg` first)?
- Phase 3 in 0.14.0 or 0.15.0?
- Is `~=` the right "contains" operator, or `:` (as in Kibana / Loki `|=`)?
