## Context

`FilterSpec` (in `scan_job.rs`) compiles one include and one exclude pattern with the stream's case-sensitivity and regex toggles; it is cloned into worker threads for filter and search scans on files above 16 MB and evaluated inline below. Visibility is `!excluded && included && level_passes`, then the engine applies the time range; stack-trace continuation lines follow their parent unless excluded. Stream filters are persisted per stream in the workspace (`StreamEntry.include_filter` / `exclude_filter`) and in session files; the minimum level and time range are not part of sessions by design. Global preferences live in `fasttail.ini`, with repeated sections such as `[highlight_N]` and `[tool.N]`.

## Goals / Non-Goals

**Goals:**
- Express "A and B, without C or D" without regex tricks.
- Save and re-apply a whole filter setup in two clicks, on one stream or all.
- Keep plain-text terms literal: whatever the user types is what is searched.

**Non-Goals:**
- A query language (boolean operators, parentheses, field selectors such as `level:`).
- OR between terms (a regex `a|b` inside one term covers it).
- Per-term case or regex toggles.
- Repeated `--filter` / `--exclude` on the command line (possible follow-up; the command-line requirement is left untouched here).
- Presets inside session files, or sharing presets between machines beyond copying `fasttail.ini`.

## Decisions

### D1. Term rows instead of a syntax
The combination is expressed by the UI, not by text: the stream bar keeps one include and one exclude field (the first term of each side); a `+` button beside each opens the Filters window on that stream with its term list, where rows can be added (up to 8 per side), edited and removed. The stream bar shows `+2` next to a field when extra terms are active, so a hidden condition is never invisible.

Semantics: `visible = none(exclude_terms match) && all(include_terms match) && level_passes && in_time_range`. Empty rows are ignored. For continuation lines: `!any_excluded && ((all_included && level) || parent_visible)`, identical in shape to today.

*Alternatives:* (a) an operator syntax in the single field (`payment && !health`) — conflicts with plain-text search for `&&`, `!` and `-` which are common in shell and CI logs, needs quoting rules and an error UI; (b) a comma-separated list — commas are common in log text; (c) a full query language as in lnav's SQL — far beyond "simple".

### D2. Shared case and regex toggles
All terms of a stream use the stream's existing case-sensitive and regex toggles. One toggle pair keeps the bar compact and matches how users think of "the filter"; a term that needs different behaviour can use regex inline flags (`(?i)`), documented in the README.

### D3. `FilterSpec` with term vectors
`FilterSpec { include: Vec<Term>, exclude: Vec<Term>, … }` where `Term` holds the text, its lower-case form and the optional compiled regex. `is_active` becomes "any non-empty term, or a level". A term whose regex fails to compile is flagged in its row and treated as matching nothing for include / nothing for exclude, as today's single field does. Cost is linear in the number of terms; with the cap of 8 per side the worst case is 16 evaluations per line, still dominated by reading the line.

### D4. Presets as global preferences
```ini
[filter_preset.0]
name = payment errors
include.1 = payment
include.2 = timeout
exclude.1 = healthcheck
case_sensitive = false
regex = false
min_level = WARN
show_unknown_levels = false
time_from =
time_to =
```
Keys `include.N` / `exclude.N` run from 1; `time_from` / `time_to` are present only when the preset was saved with "include time range" ticked, and store the text as typed so a bare `14:02` keeps following the day of whichever log it is applied to (the existing time-range rule). Names are unique case-insensitively; sections are renumbered on save, like the highlight rules.

Applying a preset replaces the stream's include terms, exclude terms, toggles, minimum level and unknown-level toggle; it replaces the time range only when the preset carries one and otherwise leaves it as it is. It triggers one filter recomputation (one background scan on a large file), not one per field.

*Alternative:* presets in session files — sessions describe a workspace, presets are the user's vocabulary across workspaces; mixing them would make loading a colleague's session overwrite one's presets.

### D5. Dropdown state derived, not stored
The dropdown label is computed each frame by comparing the stream's filter state with the presets: equal to preset P → `P`; last applied P but edited since → `P *`; otherwise `Presets`. Only the "last applied" name is kept in memory per stream; nothing is persisted, so there is no stale state after a restart (the label reappears when the restored filters equal a preset).

Menu entries: each preset (click = apply to this stream, a second entry "apply to all open streams"), separator, "Save current as preset…", "Update P from this stream" (when `P *`), "Manage presets…" (opens the Filters window section: rename, delete with confirmation, reorder).

### D6. Persistence of extra terms in workspace and sessions
`StreamEntry` gains `include_terms` / `exclude_terms` for terms 2..8, written as `include_filter.2 …` next to the existing `include_filter`, so a session saved by this version still opens in an older one with the first term of each side.

## Risks / Trade-offs

- [Users expect OR between rows] → rows are labelled "all of" (include) and "none of" (exclude), and the README shows the `a|b` regex for OR.
- [Hidden extra terms surprise the user] → `+N` badge on the stream bar fields, and the empty-stream message already says that the active filters hide all lines.
- [Applying a preset to all streams starts many background scans] → scans are per stream and cancellable as today; applying to ten 400 MB streams costs what typing the filter in each would.
- [Preset with a time range applied to a log without timestamps] → the time part is skipped and the time controls keep their existing "no usable timestamps" hint.
