## 1. Engine: notes and automatic bookmarks

- [x] 1.1 `TailEngine::bookmark_notes: BTreeMap<usize, String>` with `set_bookmark_note` / `bookmark_note` (trim, line breaks and tabs to spaces, 200 chars, empty removes the note, a note adds the manual bookmark); `toggle_bookmark` and `clear_bookmarks` drop notes; every change sets `bookmarks_dirty` and bumps `bookmarks_generation`
- [x] 1.2 `HighlightRule::auto_bookmark` (serde default `false`) and `CompiledHighlight::auto_bookmark`; extract `CompiledHighlight::is_match(&str)` and use it in `check_sound_alerts`
- [x] 1.3 `auto_bookmarks` and `dismissed_auto` sets, cap from the `auto_bookmark_max` setting, capped flag; `is_bookmarked`, `visible_bookmarks`, `bookmark_next` / `bookmark_prev` use manual ∪ (auto − dismissed); `is_auto_bookmark` for the glyph; `CTRL + F2` on an auto-only row dismisses it
- [x] 1.4 Synchronous recomputation for files up to 16 MB on open, `reload_from_start` and `set_highlight_rules` (rules change clears dismissals); skipped when no flagged rule is enabled
- [x] 1.5 `JobSpec::AutoBookmarks` / `ScanKind::AutoBookmarks` in `src/scan_job.rs` emitting `ScanBatch::Lines`, stopping at the cap; started after the index job for files above 16 MB, progress in the stream bar, stale generations dropped
- [x] 1.6 `collect_auto_bookmarks(prev_lines_count)` on every append beside `collect_tool_hits`, covering only lines past a running job's range
- [x] 1.7 `reload_from_start` clears notes, auto-bookmarks and dismissals; compressed `pending_bookmarks` carry notes and the auto scan starts once the index settles
- [x] 1.8 Unit tests: note normalisation and 200-char cut; note on auto row becomes manual; dismissal; cap (default 10,000 and a custom value) with capped flag; appended lines bookmarked once while a job runs; job result equals the synchronous path (thresholds at 0); truncation clears everything; rule hidden by a higher rule still bookmarks

## 2. Persistence

- [x] 2.0 `auto_bookmark_max` in `[general]` (default 10,000, clamped to 100..100,000), a Settings field under Performance & refresh, and recomputation of the open streams when it changes

- [x] 2.1 `FastTailConfig::bookmarks` carries notes; `set_bookmarks` / `bookmarks_for` take and return them; `[bookmarks]` writes `note_<i>_<line>` via `filter_preset::ini_value` and loads them, ignoring notes of unsaved lines
- [x] 2.2 `[highlight_<n>]` writes and reads `bookmark`
- [x] 2.3 `StreamEntry::bookmark_notes`; session save / load `bookmark_note.<line>`; `from_config` / `apply_to_config` pass notes
- [x] 2.4 `app.rs` persists notes with bookmarks when `bookmarks_dirty`; standard input still saves nothing
- [x] 2.5 Tests: ini and session round-trip with notes (quotes, edge spaces, non-ASCII); old ini without note keys; old rule section without `bookmark`; auto-bookmarks never written

## 3. UI

- [x] 3.1 `row_context_menu` always opens in Text view: "Bookmark note…" one-line editor (`Enter` saves, `ESC` cancels, 200-char limit) and "Remove bookmark"; tool and anchor items after a separator
- [x] 3.2 Marker column: `★` manual, `✏` with note, `☆` auto-only, `▶` keeps priority; tooltip with the note on hover
- [x] 3.3 Overview strip: auto marks at 50 % alpha, note tooltip on manual marks; rebuild on `bookmarks_generation` as today
- [x] 3.4 Rule editor checkbox "Bookmark matching lines"; stream-bar capped notice

## 4. Texts and documentation

- [x] 4.1 New i18n keys (note menu item, editor hint, remove bookmark, rule option and tooltip, capped notice, scan progress label) in all 16 languages; add them to the exhaustive i18n test
- [x] 4.2 README (bookmarks and highlight rules sections, comparison table) and CHANGELOG `[Unreleased]`, noting that older versions drop notes on save

## 5. Wrap-up

- [x] 5.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PR with Linux and Windows CI green
- [x] 5.2 Local preview exe for the maintainer before the 0.12.0 release
- [x] 5.3 After the release, archive the change so `search-and-navigation` and `filters-and-highlighting` gain the new requirements
