## Why

Users re-type the same filter combinations every day: "errors of the payment service without health checks", "WARN and above, no metrics lines". Each stream has one include field and one exclude field, so combining two independent conditions means hand-writing a regex such as `^(?=.*payment)(?=.*timeout)` — which the `regex` crate does not even support (no look-around) — or giving up. Tailviewer keeps its "quick filters" saved for reuse; FastTail forgets a combination as soon as the fields are cleared.

## What Changes

- **Several include and exclude terms per stream.** The filter bar keeps its two fields; a `+` button next to each adds another term row (up to 8 per side) in the Filters window. A line is visible when it matches **every** include term and **none** of the exclude terms, then the level and time filters apply as today. Each term is plain text or a regex under the stream's existing case and regex toggles; OR within a term stays the regex `|`. There is no textual query language: no operators, parentheses or escaping rules.
- **Filter presets.** The current filter state of a stream (include terms, exclude terms, case and regex toggles, minimum level and unknown-level toggle, and optionally the time range as typed) can be saved under a name. A `Presets ▾` dropdown in the stream bar applies a preset to that stream, or to all open streams; the dropdown shows the name of the preset the stream currently matches, with `*` once it has been edited.
- Presets are global preferences stored in `fasttail.ini` (`[filter_preset.N]` sections), managed (rename, delete, reorder, update from the current stream) in the Filters window. They are not part of sessions; the per-stream terms are, like today's filters.

## Capabilities

### New Capabilities
- (none)

### Modified Capabilities
- `filters-and-highlighting`: adds multiple ANDed include terms and multiple exclude terms, and named filter presets.

## Impact

- `src/scan_job.rs`: `FilterSpec` holds `Vec` of compiled include and exclude terms (shared with worker threads as today); `included` = all include terms match, `excluded` = any exclude term matches; `visible_in_sequence` (stack-trace continuation) unchanged in structure.
- `src/tail_engine.rs`: `include_filter` / `exclude_filter` become the first term of `include_terms` / `exclude_terms` (the accessors stay for the filter bar and the command line).
- New `src/filter_preset.rs`: `FilterPreset` struct, equality against a stream's state, apply.
- `src/config.rs`: `[filter_preset.N]` sections; `src/session.rs`: extra terms stored as `include_filter.2` … so older builds still read the first term.
- `src/ui/dock.rs`: `+` buttons and term rows in the Filters window, `Presets ▾` dropdown in the stream bar, save/update/manage dialogs.
- i18n: new keys in all 16 languages. README and CHANGELOG.
- No new dependency; filter cost grows linearly with the number of terms, capped at 8 per side.
