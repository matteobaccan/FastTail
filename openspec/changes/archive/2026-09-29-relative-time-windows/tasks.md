## 1. Parsing

- [x] 1.1 `timestamp::parse_relative` (`-<N><unit>…`, `now`) wired into `parse_user_time`; "to" side not widened; shared with `headless-print`
- [x] 1.2 "Now" in the stream's source zone when `quick-wins-0-13` provides one, local otherwise
- [x] 1.3 Tests: every unit, combined forms, invalid forms (`-`, `-5x`, `- 5m`), `now`, existing forms unchanged

## 2. Live window

- [x] 2.1 Re-evaluation every 5 s and on append batches while a side is relative; repaint scheduling
- [x] 2.2 Non-decreasing timestamp flag in the cache; trim / extend `filtered_lines` at the ends when set; refilter at most once a minute otherwise (background above 16 MB)
- [x] 2.3 Tests: a followed fixture with a moving clock drops old lines and shows new ones without refiltering; unordered fixture refilters once a minute; search and collapse follow; presets keep the relative text

## 3. UI and command line

- [x] 3.1 "Relative to now" shortcut row; "Last hour" renamed "Last hour of the log"
- [x] 3.2 `⟳` label, tooltip with the text and the bounds in force, empty-window hint with the last line's time
- [x] 3.3 `--since` / `--until` without `--print` in `src/cli.rs`, applied to the streams opened from the command line (stdin included); usage error on unreadable values; CLI tests

## 4. Texts and documentation

- [x] 4.1 New i18n keys (shortcuts, renamed shortcut, tooltip, hint, CLI help) in all 16 languages; add them to the exhaustive i18n test
- [x] 4.2 README (time range section, command line), `--help` text and CHANGELOG `[Unreleased]`

## 5. Wrap-up

- [x] 5.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PR with Linux and Windows CI green
- [x] 5.2 Local preview exe for the maintainer before the 0.13.0 release
- [x] 5.3 After the release, archive the change so `filters-and-highlighting` and `command-line` gain the new and modified requirements
