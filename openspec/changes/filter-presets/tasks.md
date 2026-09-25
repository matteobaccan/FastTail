## 1. Multiple terms

- [x] 1.1 `FilterSpec` with include / exclude term vectors (text, lower-case, compiled regex), all-of include, any-of exclude, `is_active`, invalid-regex flag per term; cap 8 per side
- [x] 1.2 `TailEngine`: `include_terms` / `exclude_terms` with the first term behind the existing `include_filter` / `exclude_filter` accessors; one recomputation per change
- [x] 1.3 Tests: two include terms ANDed, two exclude terms, empty rows ignored, stack-trace continuation follows its parent, synchronous and background scans agree above the 16 MB threshold, invalid regex term

## 2. Presets

- [x] 2.1 Add `src/filter_preset.rs`: `FilterPreset` (name, terms, toggles, min level, unknown toggle, optional time texts), equality with a stream state, apply (time range untouched when the preset has none)
- [x] 2.2 `fasttail.ini`: `[filter_preset.N]` load/save with renumbering, unique names, defaults for missing keys
- [x] 2.3 Tests: round trip, apply to one and to all streams, apply with and without time range, bare `14:02` follows the target log's day, dropdown label (`P`, `P *`, none)

## 3. UI

- [x] 3.1 Stream bar: `+` beside the include and exclude fields, `+N` badge when extra terms exist, `Presets ▾` dropdown with apply, apply to all, save, update, manage
- [x] 3.2 Filters window: term rows per side ("all of" / "none of"), add / remove, per-row error flag; preset manager (rename, delete with confirmation, reorder)
- [x] 3.3 Save dialog: name field, "include time range" checkbox, overwrite confirmation

## 4. Persistence of terms

- [x] 4.1 `StreamEntry` extra terms as `include_filter.2 …` / `exclude_filter.2 …` in workspace and session files
- [x] 4.2 Tests: session round trip with extra terms; a session written by the previous version loads with one term per side

## 5. i18n and docs

- [x] 5.1 New keys (presets menu entries, dialogs, "all of" / "none of", badges, errors) in all 16 languages; i18n coverage test passes
- [x] 5.2 README: combined terms (with the `a|b` and `(?i)` notes) and presets
- [x] 5.3 CHANGELOG `[Unreleased]` entry
