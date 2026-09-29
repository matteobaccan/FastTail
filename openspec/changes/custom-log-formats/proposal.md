## Why

`structured-fields` gives FastTail JSON, logfmt and one ad-hoc regex per stream. Most
in-house logs are none of these: a Log4j / Logback pattern with a thread in brackets, a
.NET service with its own date order, a device log whose entries span several lines and
whose timestamp FastTail cannot read (`28.09.26 14:02:11,123`). Today the user retypes
the same regex on every stream, cannot share it with the team, and the stack-trace
heuristic decides where an entry ends even when the log says otherwise. lnav ships a
format library with user formats in JSON files and auto-detection, LogExpert has
columnizers, LogViewPlus has named parsers with auto-detection; the post-0.12.0 scan
lists structured logs as gap 1 and the most-voted lnav requests ask for mixed and forced
formats.

## What Changes

- **Named log formats**, one per file, `*.fasttail-format.ini` in a `formats` folder next
  to `fasttail.ini` (easy to share, diff and commit). A format holds:
  - `name`; one to 8 **line patterns** (`regex`, `regex.2`…), regexes with named groups
    tried in order;
  - the **timestamp field** and an optional **timestamp format** (a strftime subset such
    as `%d.%m.%y %H:%M:%S,%3f`, or `epoch_s` / `epoch_ms`) for dates the built-in parser
    does not read, and an optional **source zone**;
  - an optional **level field** with a **level map** (`W=warn,E=error,SEVERE=error`) and a
    **message field**;
  - an optional **entry start pattern**: a line matching it starts a new entry, any other
    line continues the previous one (replaces the stack-trace heuristic for that stream);
  - **file globs** (`payments-*.log`) that associate the format with file names, the
    default **columns** for the column view, and **sample lines** used as tests and for
    detection.
- **Detection** extends `structured-fields` detection: formats whose globs match the file
  name are tried first, then every user format on the 200-line sample (a format wins when
  at least 80 % of the sampled entries match one of its patterns; most matched entries
  first, then most named groups), then the built-in JSON and logfmt detection. The chip
  shows the format name; the parser menu lists every format and can force one.
- A **Log formats** dialog (Settings, and "Create format from these lines…" in the row
  menu): list, new, duplicate, delete, import and export of files; a pattern editor with
  a **live preview** of the captures on the samples and on the focused stream's first
  200 lines (match ratio, unmatched lines, time and level as read), and errors with their
  position. The Apache / nginx and syslog presets of `structured-fields` become read-only
  built-in formats that can be duplicated.
- Everything `structured-fields` does with a parser works with a format: column view,
  field filter terms, level and timestamp from fields, and through them the level filter,
  time range, histogram, time delta, `merged-timeline-view` and `field-statistics`.
- Per stream, `fields_parser=format:<name>` in the workspace and session files; no new
  key in `fasttail.ini` besides the folder being fixed next to it.

Target release: **0.21.0** (structured logs and analysis, continued; planned for 0.14.0, moved after the terminal interface of 0.20.0), per the release plan in
`docs/competitor-analysis.md` section 8. Priority: **Medium**. Effort: **M**.

### Non-goals

- Importing lnav JSON formats, LogExpert columnizers or Log4j `PatternLayout` strings
  (possible follow-ups; see the design's open questions).
- Generating a regex automatically from sample lines.
- Formats for XML, CSV with a header, CLEF or binary logs.
- Per-format highlight rules, SQL tables or computed fields.
- Several formats mixed in one stream line by line (one format per stream; the patterns of
  one format cover variants).

## Capabilities

### New Capabilities

- `log-formats`: format files and their keys, timestamp formats, level maps, entry start
  patterns, detection and association by file name, the Log formats dialog and preview.

### Modified Capabilities

None (the parser behaviour it plugs into is defined by the pending `structured-fields`
change).

## Impact

- `src/log_format.rs` (new): `LogFormat` (patterns as one `regex::RegexSet` plus the
  compiled `Regex`es, field roles, level map, entry start, globs, samples), loading and
  validation of the `formats` folder, writing a format file (values through
  `filter_preset::ini_value`).
- `src/fields.rs` (from `structured-fields`): `FieldParser::Format(Arc<LogFormat>)`;
  detection over user formats before JSON / logfmt.
- `src/timestamp.rs`: `parse_with_format(text, &TimeFormat)` for the strftime subset and
  epoch values, reusing `date_to_days`.
- `src/tail_engine.rs`: `is_stacktrace_continuation` (today a static function with five
  callers) becomes a per-stream entry rule, the format's entry start when set; the level
  and timestamp caches and `scan_job` `Levels` / `Timestamps` / `Filter` / `Collapse` jobs
  take the rule.
- `src/wildcard.rs`: file-glob matching for the association.
- `src/ui/log_formats.rs` (new): dialog, pattern editor, preview table; row menu entry in
  `src/ui/dock.rs`.
- `src/session.rs`: `fields_parser=format:<name>`.
- `src/i18n.rs` (16 languages), help dialog, README, CHANGELOG, tests, a detection bench.
- **TUI (0.20.0, PR #132)**: formats are UI-free files, so the TUI gets the same parsing
  and detection; editing stays in the GUI (the TUI can list and force a format).
