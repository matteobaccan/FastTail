## Why

Container logs (`docker logs`, `kubectl logs`), CI job logs and most modern CLI tools write ANSI SGR escape sequences (`ESC[32m`, `ESC[0m`) for colour. FastTail shows them as raw bytes: the rows are cluttered with `[32m` fragments, a level such as `ESC[31mERROR` is not detected because the token is not preceded by a word boundary, an include filter `\bERROR\b` misses it, and copied text carries control bytes. lnav and most terminals render them; FastTail should either render or remove them.

## What Changes

- Each text stream gets an ANSI mode: **Render** (colours and bold/italic/underline become styled spans, the sequences are hidden), **Strip** (sequences hidden, no colour), **Raw** (bytes shown as they are, `ESC` drawn as `␛`).
- The default mode is auto-detected when the stream opens (and on appended data until a first sequence is seen): Render when SGR sequences are found, otherwise Raw, which on a log without sequences is identical and costs nothing.
- In Render and Strip modes every text consumer — include/exclude filters (including background scan jobs), search, highlight rules and quick labels, level and timestamp detection, JSON detection, copy and export, external-tool placeholders — works on the line with the sequences removed. HEX view always shows the file bytes.
- User highlight rules and quick labels take precedence over ANSI colours; ANSI colours take precedence over the level-colouring fallback. ANSI spans share the 64-span budget of a row.
- The mode is chosen from a toolbar button (`ANSI: auto/render/strip/raw`) and persisted per stream in the workspace and sessions.

## Capabilities

### New Capabilities
- `ansi-escape-codes`: detection, render / strip / raw modes of ANSI escape sequences and the rule that all text features see the stripped line.

### Modified Capabilities
- `filters-and-highlighting`: the 64-span row budget and first-wins rule now include ANSI colour spans ranked below user rules and quick labels.

## Impact

- New `src/ansi.rs`: a zero-allocation fast path (`memchr` for `0x1b`), a parser for CSI (`ESC [ … final`) and OSC (`ESC ] … BEL|ESC \`) sequences, SGR state (16 base colours, bright colours, 256-colour and 24-bit, bold, dim, italic, underline, inverse, reset), producing the stripped text, a segment map to raw offsets and style runs.
- `src/tail_engine.rs`: `ansi_mode` per stream; `get_line` returns the stripped text in Render/Strip; `match_highlight_spans` merges ANSI runs after rules and labels; level/timestamp detection and `copy_selection_text` use the stripped line; search byte offsets are in stripped coordinates and mapped to raw for the HEX view.
- `src/scan_job.rs`: `ScanRange` carries the ANSI mode; `decode_line` output is stripped before filter, search and level evaluation.
- `src/theme.rs`: an ANSI 16-colour palette per theme, readable on the light theme.
- `src/ui/dock.rs`: toolbar button, `span_layout_job` honours underline and inverse.
- `src/session.rs` / `src/config.rs`: `ansi` key in `StreamEntry`.
- i18n: new keys in all 16 languages. README and CHANGELOG.
- No new dependency (the parser is small and only SGR needs interpreting; `vte` / `strip-ansi-escapes` were considered, see design).
