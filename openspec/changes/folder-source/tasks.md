## 1. Matching and scanning

- [ ] 1.1 `FolderSpec` in `src/wildcard.rs`: several patterns, subfolders to depth 8, 10 000 entries per scan, no link following; `resolve_newest` over a spec; single-pattern form serialises to `dir/pattern`
- [ ] 1.2 Pattern streams in `src/tail_engine.rs` take a `FolderSpec`; recursive rescan through a `notify` watcher (debounced) plus a walk every 10 s, flat folders unchanged (2 s)
- [ ] 1.3 `src/folder_source.rs`: `FolderGroup` with known files, cap `folder_max_streams`, compressed files skipped unless named, new-file events
- [ ] 1.4 "File removed" stream state that keeps the lines; reappearing file tailed as a rotation
- [ ] 1.5 Tests: multi-pattern and case rules, depth limit, entry limit, symlink loop not followed, newest across subfolders, group cap keeps the newest, new file detected, compressed skip, deleted file keeps lines

## 2. UI and persistence

- [ ] 2.1 Open folder dialog (folder, patterns, subfolders, mode) replacing the pattern prompt; folder drop opens it
- [ ] 2.2 Open all: group opened in a new dock node; later files added as tabs without focus change; notice for skipped files
- [ ] 2.3 `folder_patterns`, `folder_recursive` stream keys; `[folder_N]` group sections in workspace and sessions; `folder_max_streams` in `[general]`; old files load unchanged (tests)
- [ ] 2.4 `--all` command-line flag (if the open question is accepted)

## 3. Texts and documentation

- [ ] 3.1 New i18n keys (dialog, modes, cap notice, scan limit, file removed) in all 16 languages; add them to the exhaustive i18n test
- [ ] 3.2 README (opening files section, comparison table) and CHANGELOG `[Unreleased]`

## 4. Wrap-up

- [ ] 4.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PR with Linux and Windows CI green
- [ ] 4.2 Local preview exe for the maintainer before the release
- [ ] 4.3 After the release, archive the change so `folder-source` becomes a spec
