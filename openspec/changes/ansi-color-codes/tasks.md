## 1. Parser

- [ ] 1.1 Add `src/ansi.rs`: `has_escape` (`memchr`), `strip`, `strip_with_map` (segment map, stripped → raw conversion), `style_runs` (SGR state machine: 16/bright, 256, 24-bit, bold, dim, italic, underline, inverse, resets), CSI and OSC framing (BEL and `ESC \` terminators), unterminated sequences
- [ ] 1.2 Unit tests: plain line untouched without allocation, nested/combined SGR, resets, 256 and truecolour, OSC 8 link removed, non-SGR CSI removed, truncated sequence at end of line, offset map round trip, `style_runs` offsets match the stripped text

## 2. Engine

- [ ] 2.1 `AnsiMode` per stream; auto-detection on the first 64 KB at open and on appended chunks until the first SGR hit
- [ ] 2.2 `get_line` returns the stripped text in Render/Strip; `␛` substitution for display in Raw
- [ ] 2.3 Level and timestamp detection, JSON detection, `copy_selection_text`, export and external-tool placeholders use the stripped line; mode change bumps `buffer_generation`, resets level/timestamp caches and restarts filter/search scans
- [ ] 2.4 `ScanRange` carries the resolved mode; `scan_job` strips after `decode_line` for filter, search and level jobs
- [ ] 2.5 Map the text search cursor to a raw byte offset when switching to HEX
- [ ] 2.6 `match_highlight_spans`: ANSI runs after user rules and quick labels, within `MAX_ROW_SPANS`; level fallback only on uncoloured bytes of rows without a rule match
- [ ] 2.7 Tests: `ESC[31mERROR` detected as ERROR; include filter `\bERROR\b` matches it; synchronous and background filter results identical on a coloured file above the job threshold; copy returns text without escapes in Render/Strip and with them in Raw; rule precedence over ANSI colours; span cap respected

## 3. Theme and UI

- [ ] 3.1 16-colour ANSI palette per theme in `theme.rs`, checked for contrast on the light theme
- [ ] 3.2 `SpanStyle::Ansi` and underline/inverse/dim in `span_layout_job` (normal and wrapped rows)
- [ ] 3.3 Toolbar button `ANSI: auto/render/strip/raw` showing the resolved mode; stream bar notice when auto switches to render
- [ ] 3.4 Persist the explicit mode as `ansi` in the workspace and session `StreamEntry`; tests for the round trip and for old files without the key

## 4. i18n and docs

- [ ] 4.1 New keys (mode names, button tooltip, auto-switch notice) in all 16 languages; i18n coverage test passes
- [ ] 4.2 README: ANSI colours in the feature list, note on regexes against raw escapes (Raw mode); comparison table
- [ ] 4.3 CHANGELOG `[Unreleased]` entry
