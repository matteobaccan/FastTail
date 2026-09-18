## Why

Almost every log carries a level token (`ERROR`, `WARN`, `INFO`, `DEBUG`, `TRACE`, `FATAL`), and Tailviewer's most used features are colouring rows by level and filtering by minimum level. FastTail requires the user to build highlight rules by hand for each of them and has no level filter at all.

## What Changes

- The engine detects the level of each line with a small set of built-in patterns (bracketed and bare tokens, syslog severities, common Java/.NET/Python layouts), cached per line.
- Built-in level colouring, enabled by default and switchable per theme, applies when no user highlight rule matches the line (user rules keep priority).
- A level selector in the stream bar filters by minimum level (`>= WARN`) and combines with include/exclude filters.
- Per-stream counters per level in the stream status bar.

## Capabilities

### New Capabilities
- (none)

### Modified Capabilities
- `log-intelligence`: adds level detection, level colouring and level filtering.

## Impact

- `src/tail_engine.rs`: `LogLevel` enum, `detect_level(&str)`, per-line level cache (`Vec<u8>` parallel to `line_offsets`), `min_level` filter folded into `is_line_visible`, counters.
- `src/theme.rs`: level palette per theme.
- `src/ui/dock.rs`: level selector, counters, colouring fallback; `src/config.rs`: `level_colors_enabled`, per-stream `min_level`; `src/i18n.rs`: keys.
- `benches/filter_bench.rs`: a level-filter phase.
