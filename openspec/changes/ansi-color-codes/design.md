## Context

A row is produced by `TailEngine::get_line(idx)`, which reads the line's raw bytes through the block cache and decodes them (`decode_line`) according to the stream encoding. Styling comes from `match_highlight_spans(line)`: user rules (whole-row or captures-only), then quick labels, as sorted, non-overlapping byte spans capped at `MAX_ROW_SPANS = 64`, first rule winning per byte; `span_layout_job` turns them into an egui `LayoutJob`. Filters and search on large files run in `scan_job` worker threads that read raw 1 MB chunks and call `decode_line` themselves, so anything the view does to a line must also be done there, or a filter would match a different text than the one shown. The line index, the level cache and the timestamp cache are all per raw line; line boundaries do not change when escapes are removed.

## Goals / Non-Goals

**Goals:**
- Readable coloured container and CI logs.
- One definition of "the text of a line" per stream, used identically by the view, filters, search, rules, level/timestamp detection, copy and export.
- No cost for logs without escape sequences.

**Non-Goals:**
- Terminal emulation: cursor movement, clearing, `\r` overwrite of progress bars, alternate screen. Non-SGR CSI sequences are removed, not interpreted.
- Blink, conceal, strikethrough, font selection; OSC 8 hyperlinks are removed, not made clickable.
- Escapes in HEX view (it shows bytes by definition) or in Markdown view (rendered from the stripped text).

## Decisions

### D1. Three modes, auto default
`AnsiMode = Auto | Render | Strip | Raw`, stored per stream. `Auto` resolves to Render once an SGR sequence (`ESC [ <digits/semicolons> m`) is found in the first 64 KB at open, or later in appended data (checked on each appended chunk until the first hit, so a container that starts colouring after its banner is caught); until then it behaves as Raw. Raw on a file without escapes is byte-identical to the other modes, so the fallback costs nothing. The resolved mode is shown on the toolbar button (`ANSI: auto → render`); choosing a mode explicitly is persisted in the workspace and sessions as `ansi = render|strip|raw` (absent = auto).

*Alternative:* a global setting — rejected, because one workspace commonly mixes a coloured `docker logs` capture and a plain application log whose literal `ESC` bytes, if any, the user wants to see.

### D2. The stripped line is the line
In Render and Strip modes, `get_line` returns the decoded line with every CSI and OSC sequence removed (a lone `ESC` followed by another byte is removed with that byte). Everything that asks the engine for a line's text therefore sees the stripped text: rows, filters in the synchronous path, `find_matches`, highlight rules, quick labels, JSON detection, `copy_selection_text`, export, and the `{line}` placeholder of external tools. The worker path gets the same behaviour by passing the resolved mode in `ScanRange` and stripping after `decode_line` in `scan_job`, so the synchronous and background results stay identical (the existing "same result as the synchronous path" requirement). Level detection (first 96 bytes) and timestamp detection (first 64 bytes) run on the stripped text, which is what makes `ESC[31mERROR` detected as ERROR; their caches are reset from line 0 when the mode changes, like a rewrite.

Changing the mode bumps `buffer_generation`, restarts the filter and search scans and clears the level and timestamp caches; the line index is untouched.

### D3. Byte-offset mapping
Stripping removes bytes, so offsets inside the stripped text differ from offsets in the file. The rule: **all offsets the text features produce are stripped-text offsets**; only code that addresses the file itself converts. `ansi::strip(line) -> Stripped { text, map }` where `map` is a monotonic list of `(stripped_offset, raw_offset)` pairs, one per kept segment (a segment is a run of bytes between two sequences). Converting a stripped offset `s` to raw is a binary search for the last segment with `stripped_offset <= s`, then `raw_offset + (s - stripped_offset)`. The map is only built when a caller needs it (`strip_with_map`); the common path returns the text alone.

Consumers that need raw offsets:
- **HEX view search cursor**: the text-view cursor (`current_search_byte`, line + stripped offset) becomes a file byte offset `line_offset[line] + raw(s)` when the user switches to HEX, preserving the "search cursor preserved across views" behaviour. HEX's own byte search stays on raw bytes and may match inside an escape sequence, which is correct for a byte view.
- Nothing else: line boundaries are raw and unchanged, and the level/timestamp caches are per line.

For multi-byte encodings the order is decode, then strip, so offsets are in the decoded UTF-8 string that egui lays out; UTF-16 logs with escapes are handled because escapes are ASCII code points after decoding.

### D4. Rendering colour runs as spans
In Render mode `ansi::style_runs(line)` walks the SGR state machine over the raw decoded line and yields `(stripped_start, stripped_end, AnsiStyle)` runs for every non-default style. `match_highlight_spans` fills spans in priority order: user rules, quick labels, then ANSI runs in the byte ranges still free, first-wins per byte as today; the level-colouring fallback applies only to rows with no rule match and only to bytes without an ANSI colour. ANSI spans count toward `MAX_ROW_SPANS = 64`; once the budget is used, the remaining bytes keep the base style (a pathological line with hundreds of colour changes stays bounded). A whole-row user rule wins over ANSI colours for the whole row, which matches the existing "user rule keeps priority" behaviour.

Colours: SGR 30–37/90–97 (fg) and 40–47/100–107 (bg) map to a 16-colour palette defined per theme in `theme.rs` (so dark and light themes stay readable); 38;5;n / 48;5;n use the standard xterm 256 table with the first 16 from the theme palette; 38;2;r;g;b is used as given. Bold (1) is rendered the way a rule's bold flag is today (egui "strong" text: a brighter colour, not a heavier monospace weight), italic (3) to italics, underline (4) to an underline stroke, dim (2) to 60% alpha, inverse (7) swaps fg/bg, 0/22/23/24/27/39/49 reset as specified.

`SpanStyle` gains an `Ansi(AnsiStyle)` variant; `span_layout_job` honours underline and inverse. Search-match row tints keep drawing over everything, as they do over rule colours today.

### D5. Raw mode shows escapes visibly
In Raw mode rows show `ESC` as `␛` (U+241B) so the sequences are readable instead of an invisible control glyph; filters, search and copy see the real `\x1b` byte (copy gives exactly the file's text).

### D6. Own parser rather than a crate
Stripping needs CSI and OSC framing and SGR interpretation only. `vte` implements the full VT state machine (more than needed, callback-oriented, allocation-free but harder to map offsets from), `strip-ansi-escapes` allocates per call and offers no styles or offset map. A ~250-line parser with exhaustive tests is smaller than either integration and keeps the `memchr` fast path: a line without `0x1b` is returned untouched with no extra work.

## Risks / Trade-offs

- [Per-row stripping cost on screen] → only rows on screen are stripped; `memchr` skips lines without `ESC`; measured with the existing filter bench on a coloured 100 MB log.
- [Scan jobs slower on coloured logs] → strip is linear and allocation happens only for lines containing `ESC`; acceptable against regex evaluation cost.
- [Regex written against the raw text, e.g. `\x1b\[31m`] → still works in Raw mode; documented in the README.
- [Malformed or truncated sequence at the 1 MB line cap] → an unterminated CSI/OSC is dropped up to the end of the line; tested.
- [Auto mode flips a stream to Render after the user started filtering] → the flip happens at most once per stream, restarts scans like any mode change, and is announced in the stream bar.
