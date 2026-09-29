## Context

Row styling goes through `TailEngine::match_highlight_spans_with(line, ansi)`: user rules
(`compiled_highlights`, first rule wins per byte), then quick labels, then ANSI runs, at
most `MAX_ROW_SPANS` = 64 spans; `row_highlight` / `level_fallback` in `src/ui/dock.rs`
colour rows no rule matched. Selection is whole rows only (`selection: BTreeSet<usize>`).
Navigation keys in use: `F1`, `F2` / `SHIFT + F2` (bookmarks), `F3` / `SHIFT + F3`
(search), `CTRL + A C F G K L O T W`, `CTRL + SHIFT + D F H 1..9`, `ALT + W`; `F4` is free.
Rules are written as `[highlight_N]` sections by `FastTailConfig::save`
(`pattern`, `is_regex`, `case_sensitive`, `fg`, `bg`, `bold`, `italic`, `sound_alert`,
`enabled`, `captures_only`, `bookmark`) and read back in `FastTailConfig::load`.
`timestamp::detect_timestamp(line, hint)` returns the time "on the clock the log printed"
(a zone suffix is read past and not applied, epoch values are UTC) and `leading_span`
its byte length; `local_offset_millis(utc)` asks the OS for the local offset at an instant.

## Goals / Non-Goals

**Goals:**
- Five independent features, each mergeable on its own PR, none costing anything when
  unused.
- No change to what filters, search, copy and export see.

**Non-Goals:**
- Partial-line selection, named zones, user token kinds (see the proposal).

## Decisions

1. **Selection highlight = a token, not a drag selection.** A double-click on a row's text
   picks the token under the pointer (via the row galley's `cursor_from_pos`): the maximal
   run of letters, digits and `_ . : / @ - %`, with trailing `.` and `:` trimmed, at least
   2 and at most 256 bytes. It is stored per stream (`selection_token: Option<String>`)
   and every drawn row gets outline spans (a 1 px box in `theme.accent_color()`, drawn as a
   shape over the text, not a span colour) for each exact, case-sensitive occurrence found
   with `memchr::memmem`. Outlines do not use the 64-span budget, so they never hide a
   rule's colour. It is cleared by `Esc` (when the rows have the keyboard), a double-click
   on empty space or on the same token, a reload, or closing the stream. *Rejected:*
   turning it into a quick label — labels are global and coloured, and the user wants a
   glance, not a new colour. *Rejected:* adding partial text selection — an M-sized change
   of its own (gap 19).
2. **Rule navigation on the UI thread with a time budget.** The row menu shows "Next line
   of rule ▸" with the enabled rules that match that row (by their pattern, regardless of
   priority); picking one sets `rule_cursor: Option<usize>` (rule index, reset when the
   rules change) and jumps. `F4` / `SHIFT + F4` step from the selection anchor (or the top
   row) through the shown rows, testing `CompiledHighlight::is_match` on each line read
   through the block cache, for at most 4 ms per frame; if the target is not found in that
   budget the stream bar shows "seeking rule…" and the walk resumes on the next frame from
   where it stopped (`Esc` cancels). Past the end it wraps once with the search beep,
   like `F3`. The jump goes through `request_jump`, so collapsed groups open and follow
   pauses. *Rejected:* a background `ScanJob` — the engine runs one job at a time, so a
   seek would wait behind a filter or timestamp scan; the budgeted walk starts at once and
   typically finds the next match within the first frame. *Rejected:* reusing automatic
   bookmarks — capped at 10,000 and only for rules flagged "Bookmark matching lines".
3. **Rule set file.** `*.fasttail-rules.ini`: a `[fasttail_rules]` section with
   `version=1`, then `[highlight_0]`… with exactly the keys of `fasttail.ini`, written and
   read by the same functions (`write_rule_sections` / `read_rule_sections` extracted from
   `FastTailConfig::save` / `load`), so the two formats cannot drift. Tool bindings are
   not exported (they refer to local programs). Import shows a preview (count, first
   patterns) and two actions: **Append** (rules whose pattern, regex and case flags equal
   an existing rule are skipped, and the count is reported) and **Replace** (asks for
   confirmation). A file that is not a rule set, or a newer `version`, is refused with a
   message. The native dialogs are `rfd`, as for export.
   *Rejected:* JSON or TOML for the rule file: a second serialisation of the same
   settings that could drift from `fasttail.ini`, and users already edit INI sections.
4. **Time display = substitution of the leading timestamp only.**
   `detect_timestamp_zoned(line, hint) -> Option<(i64, Option<i32>, usize, FormatHint)>`
   adds the zone offset in minutes when the line has one (`Z` = 0, epoch = 0) and the span
   length. Per stream: `time_display` among `written` (default), `utc`, `local` or a fixed
   `±HH:MM`, and `time_source_zone` among `local` (default), `utc` or `±HH:MM` for
   timestamps without a zone. When the display is not `written`, the drawn row replaces
   the span with `YYYY-MM-DD HH:MM:SS.mmm` in the display zone (fraction digits as printed,
   at most 3; `Z` or `+HH:MM` appended when the display is UTC or an offset), computed
   from `printed − source_offset + display_offset` (local offsets through
   `local_offset_millis`, so daylight saving is right for each instant). Highlight spans
   are computed on the original text and shifted by the length difference; spans inside
   the timestamp are dropped for that row. The original timestamp is in the tooltip of the
   substituted text. Epoch timestamps are converted only when the display is not
   `written`. The time span control, the time range popup (both directions: typed times are
   read in the display zone), the histogram axis, go to time and the time delta tooltip use
   the display zone; the timestamp cache itself is unchanged (still the printed clock), so
   no rescan is needed when the display changes. *Rejected:* converting the cache — every
   change of zone would re-time a multi-GB log.
5. **Automatic highlighting scanner.** `auto_highlight::scan(line, kinds, &mut spans)` is a
   single left-to-right pass without regex: at each token start it tries, cheapest first,
   URL (`scheme://` for http, https, ftp, ws, wss, file, then non-space), UUID (8-4-4-4-12
   hex), IPv4 (four 0–255 octets, optional `:port`), IPv6 (hex groups of 1–4 digits with at
   least three `:` separators or a `::`, so a clock `14:02:05` is not one; bracketed with
   a port allowed), duration (a number followed by `ns`, `µs`, `us`,
   `ms`, `s`, `m`, `h`, `d`, or a chain like `2m30s`), path (`/` followed by a path
   character after a space or line start, `~/`, a drive `C:\`, or `\\server\`). Each span
   gets `theme.token_color(kind)`, foreground only. They are appended after the ANSI runs
   in `match_highlight_spans_with`, so rules, labels and the log's own colours win, and
   before the level colouring fallback; the 64-span budget applies. The result is cached
   with the row layout like other spans, so the cost is paid once per drawn row
   (target: under 2 µs for a 200-byte line). Settings: `auto_highlight` (default `false`)
   and `auto_highlight_kinds` (default all five).
   *Rejected:* built-in regex rules per kind: one regex per kind per row costs several
   passes, and the IPv6 and path patterns that avoid false positives are unreadable; a
   hand-written scanner is one pass and testable case by case.
6. **Theme token colours.** Each theme defines five foreground colours chosen for contrast
   against its background (at least 4.5:1, checked by a unit test with the WCAG formula):
   Tron cyans and violet, Matrix greens and amber, Blade amber and magenta, Light dark
   blues and teal.
   *Rejected:* one accent colour for every kind: it cannot tell an IP from a path, which
   is the point of colouring them.

Threads and memory: everything runs on the UI thread. Selection token: one string per
stream. Rule seek: bounded at 4 ms per frame. Time display: nothing stored per line.
Automatic highlighting: spans live in the existing per-row layout cache. Files above
16 MB, growing, rotated or truncated files: selection outlines and token spans follow the
rows drawn; the rule seek re-reads through the block cache, so it sees appended lines and
restarts from the anchor after a reload. Windows sharing: rule-set files are written with
`create_export_file`, the logs are only read as today.

## Risks / Trade-offs

- [`F4` clashes with a user external tool shortcut] → the built-in action wins while a
  stream has the keyboard, as for `CTRL + K`; the help and README list the key.
- [A rule seek over a 10 GB file where the rule never matches takes a long time] → it is
  budgeted, shows progress, and `Esc` cancels; it never blocks a frame.
- [Displayed times differ from the text a filter or search sees] → the tooltip shows the
  original; the README says filters and search match the text as written.
- [Automatic path detection colours fractions like `1/2`] → a path must start with `/` at
  a token start followed by a letter, digit, `.` or `_`, and must contain a second `/` or
  a `.` extension.

## Migration Plan

Additive. No key present means today's behaviour: display as written, automatic
highlighting off. Rule-set files are new. Rollback: remove the features; saved keys are
ignored by older builds.

## Open Questions

- Automatic highlighting off by default (proposed) or on for new installations only?
- Should the selection highlight also paint the token in the other open streams?
- Should the time display be a global default for new streams too (like line numbers), or
  per stream only (proposed)?
