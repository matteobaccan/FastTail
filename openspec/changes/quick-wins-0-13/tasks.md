## 1. Selection highlight

- [x] 1.1 Token pick on double-click through the row galley (`cursor_from_pos`), token rules (character class, trim, 2–256 bytes), `selection_token` per stream; cleared by `Esc`, reload, double-click on empty space or on the same token
- [x] 1.2 Outline shapes for every exact occurrence in the drawn rows (normal and wrap layouts), outside the 64-span budget; row menu entry "Highlight 'token'"
- [x] 1.3 Tests: token extraction around the pointer (UUID, IP with port, path, trailing punctuation), occurrences per row, clearing rules

## 2. Rule navigation

- [x] 2.1 Row menu "Next line of rule ▸" listing the enabled rules matching the row; `rule_cursor` per stream, reset on rules change
- [x] 2.2 `F4` / `SHIFT + F4`: budgeted walk (4 ms per frame) over the shown rows from the anchor, resume across frames with a "seeking rule…" notice, `Esc` cancels, wrap with beep, jump through `request_jump`; fallback to the first rule matching the selected row
- [x] 2.3 Tests: next / previous across filtered and collapsed rows, wrap-around, resume after the budget, no rule chosen and none matching

## 3. Rule-set import / export

- [x] 3.1 Extract `write_rule_sections` / `read_rule_sections` from `FastTailConfig::save` / `load`; `*.fasttail-rules.ini` with `[fasttail_rules] version=1`
- [x] 3.2 Highlights dialog: "Export rules…", "Import rules…" with preview, Append (skip duplicates, report count) and Replace (confirm); refuse non-rule files and newer versions
- [x] 3.3 Tests: round trip of every key, append skipping duplicates, replace, refusal cases

## 4. Time display

- [x] 4.1 `timestamp::detect_timestamp_zoned` (offset and span) and `format_in_zone`; unit tests for ISO with `Z` / `+02:00` / no zone, syslog, Apache, epoch s / ms, daylight-saving boundaries for local
- [x] 4.2 Per-stream `time_display` and `time_source_zone` with a menu in the stream bar (as written, UTC, local, fixed offset; source zone); substitution in the drawn row with span shifting and the original in the tooltip
- [x] 4.3 Time span control, time range popup (input and output), histogram axis, go to time and delta tooltip in the display zone; the timestamp cache unchanged
- [x] 4.4 `StreamEntry.time_display` / `time_source_zone` written only when not default; round-trip and old-file tests

## 5. Automatic highlighting

- [x] 5.1 `src/auto_highlight.rs` scanner for URL, UUID, IPv4, IPv6, duration and path; unit tests with positive and negative cases (versions `1.2.3`, fractions `1/2`, times `14:02:05`) and a micro-benchmark
- [x] 5.2 Spans appended after ANSI in `match_highlight_spans_with`, 64-span budget, row layout cache
- [x] 5.3 `theme.token_color(kind)` for the four themes with a contrast test (≥ 4.5:1 on the theme background)
- [x] 5.4 Settings: `auto_highlight` switch and per-kind toggles; `[general] auto_highlight`, `auto_highlight_kinds` read and written; round-trip test

## 6. Texts and documentation

- [x] 6.1 New i18n keys (menu entries, notices, dialog texts, Settings labels, time display names, help entries for `F4` / `SHIFT + F4` and the double-click) in all 16 languages; add them to the exhaustive i18n test
- [x] 6.2 README (feature list, shortcuts table, time section, Settings, FAQ on time zones) and CHANGELOG `[Unreleased]`, one entry per feature

## 7. Wrap-up

- [ ] 7.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; one PR per feature (or one per two) with Linux and Windows CI green
- [ ] 7.2 Local preview exe for the maintainer before the 0.13.0 release
- [ ] 7.3 After the release, archive the change so `filters-and-highlighting`, `search-and-navigation`, `log-intelligence` and `cyber-themes` gain the new requirements
