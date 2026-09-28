## Context

`structured-fields` (0.14.0) adds `FieldParser { Json, Logfmt, Regex }` per stream,
detection over a 200-line sample, the column view, field terms and, in phase 3, level and
timestamp read from fields. Its regex parser is one pattern stored per stream
(`fields_regex`), plus two built-in presets. Entry boundaries come from
`TailEngine::is_stacktrace_continuation`, a fixed heuristic (indentation, `at `,
`Caused by:`, `Traceback`…) used by the filter, the timestamp inheritance and collapse.
Timestamps are read by `timestamp::detect_timestamp` with a `FormatHint`. `regex` is a
dependency; its engine has linear-time matching, so user patterns cannot hang a scan.

## Goals / Non-Goals

**Goals:** reusable, shareable formats that feed every `structured-fields` feature; odd
timestamps and custom entry boundaries; detection that picks the right format without the
user touching the parser menu; no cost for streams that use no format.

**Non-Goals:** foreign format import, regex generation, mixed formats per stream (see the
proposal).

## Decisions

### D1. One file per format
`<config dir>/formats/<name>.fasttail-format.ini`, one `[format]` section. Values that
contain `;`, `#` or leading spaces go through `filter_preset::ini_value`. The folder is
read at startup and when the dialog saves; a file that fails validation is listed in the
dialog with its error and not used. At most 256 formats are loaded.
*Alternative:* `[format_N]` sections in `fasttail.ini` — rejected: a format could not be
shared or versioned without copying sections by hand (the problem rule sets had).

### D2. Keys
`name`, `regex` / `regex.2`…`regex.8`, `timestamp_field`, `timestamp_format`,
`timestamp_zone` (`local`, `utc`, `±HH:MM`; used with `quick-wins-0-13` time display),
`level_field`, `level_map` (`token=level` pairs, case-insensitive; unmapped values go
through `log_level::token_level`), `message_field`, `entry_start`, `files`
(`;`-separated globs), `columns`, `sample.1`…`sample.20`. Validation: every pattern
compiles, has at least one named group, and every field role names a group of every
pattern that defines it; a timestamp format parses at least one sample when samples exist.

### D3. Timestamp formats
A strftime subset: `%Y %y %m %d %e %H %I %M %S %p %b %j %z %f %3f %6f %9f %%`, plus the
whole-value forms `epoch_s` and `epoch_ms`. Parsed by a hand-written matcher in
`timestamp.rs` into the same millisecond value the cache stores; a year-less format takes
the year the way the RFC 3164 parser does. Without `timestamp_format` the field is read by
`detect_timestamp`, as `structured-fields` does. *Alternative:* the `chrono` crate —
rejected: a new dependency for a parser this small, and the cache convention (the log's
own clock) is FastTail's.

### D4. Entry start pattern
When set, a line is a continuation exactly when it does not match `entry_start`; the
stream's continuation rule becomes a value (`EntryRule::Heuristic | Pattern(Arc<Regex>)`)
passed to the filter, timestamp, level and collapse paths and their jobs, instead of the
static function. Changing the format of a stream rebuilds the filter, the timestamp and
level caches and collapse, as a parser change already does.

### D5. Detection
Order: (1) formats whose `files` globs match the file name, (2) every other user format,
(3) JSON and logfmt as in `structured-fields`. For (1) and (2) the sample's entries (by
each format's own entry rule) are matched against the format's `RegexSet`; a format needs
80 % of the entries; the highest ratio wins, then the most named groups, then the name.
A glob match that reaches 80 % wins over any non-glob format. Budget: one `RegexSet` per
format over at most 200 entries; with 256 formats the worst case stays under 100 ms, run
once per stream like today's detection. A saved `fields_parser` skips detection.

### D6. Dialog and preview
Pattern editing is a draft; the preview recompiles 300 ms after the last keystroke and
shows a table of the captures for the samples and the focused stream's first 200 lines,
the match ratio, the unmatched lines (dimmed), and the time and level each line would get.
"Create format from these lines…" opens a new draft with the selected lines as samples
and an empty pattern. Save writes the file and re-runs detection on streams whose parser
is auto and is still undecided, never on streams where the user picked one.

### D7. Built-in formats
The `structured-fields` presets (Apache / nginx combined, RFC 3164 syslog) are compiled-in
formats, read-only, listed with a lock; "Duplicate" writes an editable copy.

Threads and memory: formats are compiled once (`Arc<LogFormat>` shared by the jobs);
per-line cost is one `RegexSet` + one `captures_read` on the rows drawn and on the
filtered lines, as the `structured-fields` regex parser. Nothing per line is stored.
Growing files and rotation behave as with any parser.

## Risks / Trade-offs

- [A slow pattern on a 5 GB log] → linear-time engine; the preview shows µs per line so
  the author sees the cost; the filter job reports progress as today.
- [Detection picks the wrong user format] → glob association first, the chip names the
  format, the parser menu forces another one.
- [An entry start pattern that matches nothing makes the whole file one entry] → the
  preview shows the entry count; the collapse and filter paths already cap an entry at
  `MAX_ENTRY_LINES` for grouping; the dialog warns when fewer than 1 % of the sample
  lines start an entry.
- [Renaming a format breaks saved streams] → a stream whose `format:<name>` is missing
  falls back to auto detection and says so in the chip tooltip.

## Migration Plan

Additive. Streams saved with `fields_parser=apache` or `syslog` map to the built-in
formats. Without a `formats` folder nothing changes.

## Open Questions

- Import of lnav's JSON formats (the largest existing library)? Proposed: a follow-up
  converter for the regex-based ones.
- Should the formats folder be configurable (a team share)? Proposed: a second read-only
  folder from `[general] formats_dir`, later.
- Should formats carry highlight rules (lnav's `highlights`)? Proposed: no, rule sets
  (`quick-wins-0-13`) already share rules.
