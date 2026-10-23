# AGENTS.md

Instructions for coding agents (Claude Code, Jules, Codex, ...) working on FastTail.
Human contributors: see `README.md`.

## Project

FastTail is an ultra-fast multi-stream log monitor and tail viewer written in Rust
(egui/eframe GUI, wgpu renderer first with OpenGL fallback). Single crate, library
plus binary. Maintainer: Matteo Baccan. Licence: MIT.

- `src/` engine and shared modules (`tail_engine.rs`, `file_source.rs`, `scan_job.rs`,
  `config.rs`, `i18n.rs`, `actions.rs`, `fields.rs`, ...); `src/ui/` the egui GUI
  (`app.rs`, `dock.rs`, dialogs and panes).
- `tests/integration_tests/` one test binary: `main.rs` plus one module per feature.
  New tests go in the matching feature module (or a new one registered in `main.rs`).
- `benches/` criterion-style benchmarks (`cargo bench`).
- `openspec/` specs (`specs/`) and pending change proposals (`changes/`, shipped ones
  in `changes/archive/`).
- `docs/` design notes: `ui-design.md` (keep it in sync with UI changes),
  `competitor-analysis.md` (roadmap and release plan in section 8).
- `tests/logs/` and `fasttail.ini` in the repo root are the maintainer's local files
  (gitignored): never commit them.

## Build and test

- Toolchain: stable; MSRV in `Cargo.toml` (`rust-version`).
- `cargo check --all-targets`, `cargo clippy --all-targets -- -D warnings`,
  `cargo fmt --check` must all pass (CI enforces them).
- Run **focused tests only** while working:
  `cargo test --test integration_tests <name>` or `cargo test --lib <name>`.
  Do not run the whole suite before every PR: CI runs it on Linux and Windows.
- Gate commits on the command's exit status, never on grepping its output.
- Write small, focused tests; avoid large UI-harness tests unless the change needs them.

## Code conventions

- Every `.rs` file (src, tests, benches, build scripts) starts with this header, then a
  blank line (test `basics::every_rust_file_has_the_source_header` enforces it):

  ```rust
  // FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
  // Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
  // SPDX-License-Identifier: MIT
  ```

- Every user-visible string goes through `i18n::t(lang, key)` and needs a translation
  in **every** language in `src/i18n.rs` (tests check that no key is missing).
- Settings live in `fasttail.ini` via `FastTailConfig` (`src/config.rs`); a new key must
  load, save and round-trip, and older builds must be able to ignore it.
- Hot paths (scanning, filtering, rendering) must not allocate per line; the engine never
  holds whole files in RAM (block cache + streaming scans, no mmap by design).
- Do not add dependencies without a clear reason; mention any new crate in the PR.
- Match the surrounding code: naming, comment density, idioms.

## Workflow

- One change at a time: one branch, one PR, finished (CI green, merged) before the next.
- All work goes through pull requests to `main` (squash merge; the PR title becomes the
  release-notes line). Commit/PR titles use conventional style:
  `feat(scope): ...`, `fix(scope): ...`, `docs: ...`, `chore(release): x.y.z`.
- Feature work follows an OpenSpec change in `openspec/changes/<name>/`
  (`proposal.md`, `design.md`, `tasks.md`, spec deltas); tick tasks as they are done.
  Validate with `openspec validate --strict`.
- Each feature PR updates `CHANGELOG.md` (`[Unreleased]`), `README.md` and
  `docs/ui-design.md` when behaviour or UI changes.
- Bot PRs (Bolt / Palette / Sentinel, Renovate): before merging a bot branch that the
  bot touched again, check `git diff main...<branch> --stat` so a stale tree does not
  revert merged work. Close duplicates of the same fix.
- The failing `github-advanced-security` check on PRs is the Copilot Autofind job
  crashing; it is not a code finding and does not block merging.

## Release process

1. Implement the planned specs (one PR each), handle open PRs.
2. Docs audit: every `.md` (README, `docs/`, `openspec/specs`, CHANGELOG) against the
   code; archive the shipped OpenSpec changes.
3. Give the maintainer a preview build (`cargo build --release`, exe copied under a
   distinct name) and wait for their OK.
4. Release PR `chore(release): x.y.z`: version in `Cargo.toml`/`Cargo.lock`, CHANGELOG
   `[Unreleased]` renamed to `[x.y.z] - date` with compare link. Then tag `vx.y.z`:
   the release workflow builds the archives and uses the CHANGELOG section as the body.
5. After the release: refresh `docs/competitor-analysis.md` (BareTail, Tailviewer, klogg,
   lnav, LogExpert, ...) and propose the next features as OpenSpec changes.

**Nightly release (maintainer decision, 2026-09-30).** Every evening after 18:00 (Europe/Rome)
a patch release of the current minor is cut from `main` (`0.16.1`, `0.16.2`, ...) when something was merged since
the last tag; nothing merged, no release. Steps 1 to 3 above are skipped for it (the
maintainer tests the published build); step 4 applies (release PR, CI green, merge), with the CHANGELOG `[Unreleased]` entries
of the day as its section. The tag is automatic: `.github/workflows/release-tag.yml` tags
`vx.y.z` when the version reaches `main` and starts `build.yml` on the tag (agent
sessions cannot push tags). A minor
version is cut when a milestone of the plan is complete, or when the maintainer asks for one
after testing a preview build (0.16.0), with the full process. Work that is not ready stays out of `main` or is described in the
CHANGELOG as a preview.

Fixed decisions: release targets are Windows x86_64, Linux x86_64, Linux ARM64,
macOS ARM64 (no Windows ARM64, no macOS test job); release profile keeps thin LTO and
debug info for crash logs.

## Current state

Update this section in each release PR.

- Latest release: **0.22.0** (2026-10-23, minor, after the maintainer's OK on the preview 0.21.10): the time and formats milestone (merged timeline, log formats, boolean filter expressions, out-of-order lines, split, CSV / HTML export); the six changes archived. 0.21.10 (2026-10-22, nightly): export as CSV or HTML (`export-formats`); the six 0.22.0 changes are code complete, the minor waits for the preview. 0.21.9 (2026-10-22, nightly): split a file by lines, size or time (`split-log`). 0.21.8 (2026-10-22, nightly): lines whose timestamp goes back marked, counted and walked (`out-of-order-lines`). 0.21.7 (2026-10-22, nightly): filter expressions in the window: ƒx toggle, global filter and presets (`boolean-filter-expressions` code complete). 0.21.6 (2026-10-22, nightly): boolean filter expressions: engine, sessions, `--filter-expr` (first part of `boolean-filter-expressions`). 0.21.5 (2026-10-22, nightly): Log formats dialog with live preview (`custom-log-formats` code complete; archived after the 0.22.0 preview). 0.21.4 (2026-10-22, nightly): entry start pattern of a log format (`custom-log-formats` part 2). 0.21.3 (2026-10-22, nightly): named log formats (format files, timestamp formats, detection; first part of `custom-log-formats`). 0.21.2 (2026-10-22, nightly): merged views saved in the workspace and sessions (`merged-timeline-view` task 3.2; the change waits for the 0.22.0 preview). 0.21.1 (2026-10-22, nightly): merged timeline view of several streams by timestamp (`merged-timeline-view`, first change of the 0.22.0 plan; saving the view follows). 0.21.0 (2026-10-22, minor, after the maintainer's OK on the preview): the structured data milestone (open at a line, hide lines, Linux tray icon, folder sources, field filter terms and column view, CSV / TSV tables); `structured-field-terms` and `tabular-files` archived. 0.20.28 (2026-10-22, nightly): CSV / TSV files open in the column view (maintainer decision), `fields_view=false` kept when turned off. 0.20.27 (2026-10-22, nightly): CSV records over several lines on one row in the column view (`tabular-files` code complete; the six 0.21.0 changes are in, the minor waits for the preview). 0.20.26 (2026-10-22, nightly): column view of the fields in the terminal interface (`C`, `[x] Columns`, `[Fields...]`), `tabular-files` task 2.2. 0.20.25 (2026-10-22, nightly): CSV / TSV files as tables (delimited field parser, header detection, `csv_separator=` / `csv_header=`), first part of `tabular-files`. 0.20.24 (2026-10-22, nightly): column view: the message column wraps with wrap on, character selection inside one cell (`structured-field-terms` code complete; archived after the 0.21.0 preview). 0.20.23 (2026-10-22, nightly): search hits and rule colours inside the column view cells, Copy as shown as tab-separated cells (`structured-field-terms` tasks 3.1-3.3). 0.20.22 (2026-10-22, nightly): level and time read from the fields of structured lines (`level`/`severity`, `ts`/`@timestamp`), second group of `structured-field-terms`. 0.20.21 (2026-10-22, nightly): field filter terms (`status>=500`, `level=error|fatal`, `user~=bot`) on streams with a field parser, first group of `structured-field-terms`. 0.20.20 (2026-10-22, nightly): folder sources: Open folder dialog, several patterns and subfolders for pattern streams, Open all folder groups with auto-open of new files, `--all`, file removed state; `folder-source` archived (preview of the dialog with 0.21.0). 0.20.19 (2026-10-22, nightly): tray icon on Linux (StatusNotifierItem through `ksni`; KDE, Xfce, GNOME with AppIndicator), `linux-tray-icon` kept open for the hand check. 0.20.18 (2026-10-22, nightly): hide lines by hand (`Ctrl+H`, `H` in the terminal; `hide-lines` archived, second change of the 0.21.0 plan). 0.20.17 (2026-10-21, nightly): open a file at a line (`app.log:1204`, `--line N`, `--print --line`; `open-at-line` archived, the first change of the 0.21.0 plan), README install / upgrade / remove table. 0.20.16 (2026-10-21, nightly): universal macOS disk image and Intel archive; tooltips on the disabled HEX column buttons (#378), rule set handle check (#377); `release-packages` group 4 done. 0.20.15 (2026-10-21, nightly): per-user Windows installer (`.msi`, WiX v5), install-tested in the release job; `release-packages` group 3 done. 0.20.14 (2026-10-21, nightly): Linux packages: `.deb`, `.rpm` and AppImage for x86_64 and ARM64, install-tested in the release job; `release-packages` group 2 done. 0.20.13 (2026-10-21, nightly): the view mode is a drop-down of the usable modes (`view-mode-select`, archived), the help window wraps and can be made narrow. 0.20.12 (2026-10-21, nightly): new application icon (the logo's hexagon emblem, `assets/icon.svg`, rendered by `packaging/icons/render.py`), Linux desktop entry, Linux builds on Ubuntu 22.04 (glibc 2.35 floor); `release-packages` group 1 done. 0.20.11 (2026-10-21, nightly): configurable level detection window (`level_detect_bytes`, per-stream `level_bytes`, `--level-bytes`); `level-detect-window` archived. 0.20.10 (2026-10-21, nightly): Settings and Keys dialogs of the terminal move, resize and close with `[x]`; `tui-movable-dialogs` archived. 0.20.9 (2026-10-21, nightly): cursor line and first column (`Ln` / `Col`) in the terminal window border; `tui-cursor-position` archived. 0.20.8 (2026-10-21, nightly): Follow and Auto-update (formerly Monitor) as check boxes in the window and the terminal; `clearer-stream-toggles` archived. 0.20.7 (2026-10-21, nightly): faster timestamp detection, small GUI control fixes, HTML entity fix, `lzma-rust2` 0.23 with `sevenz-rust2` 0.24.1; `disassembly-view` archived. 0.20.6 (2026-10-16, nightly): tooltips on the archive picker headers and
  on the disabled Stop / Export buttons, `arboard` 3.7.0, `zip` 9.0.4 (egui 0.37, Renovate
  #330 / #335, still open: it needs code changes and a preview). 0.20.5 (2026-10-14):
  Bookmark report (all streams) disabled with
  no stream open, `zip` 9.0.3. 0.20.4 (2026-10-13): `zip` 9.0.2 and `rodio` 0.23. 0.20.3
  (2026-10-12, nightly): dependency updates (`sevenz-rust2` 0.24
  with `lzma-rust2` 0.22, `ruzstd`, `serde_json`). 0.20.2 (2026-10-11): SIMD byte search in the HEX view, tooltips
  on two disabled GUI controls, `zip` 9 and lockfile updates, taken from the bot PRs after a
  review (the duplicates and the changes without a measurable gain were closed). 0.20.1
  brought the ASM view (`disassembly-view`, `src/disasm.rs`, dependency `iced-x86`) and the
  fix of issue #161; 0.20.0 made the terminal interface complete.
- Next: nightly patches (`0.22.x`), then **0.23.0** remote and system sources (`system-sources`, `ssh-sources`, `network-listener`, `otlp-receiver`, `rule-notifications`, `debug-output-capture`).
- 0.22.0 shipped (time and formats): `merged-timeline-view`, `custom-log-formats`,
  `boolean-filter-expressions`, `out-of-order-lines`, `split-log` and `export-formats` archived.
- In parallel, as nightly patches: `openspec/changes/release-packages/` (deb, rpm,
  AppImage, MSI, dmg, one PR per task group, each validated with a `workflow_dispatch`
  build of `build.yml` on its branch before merging; task 5.0, the downloads grid, group 1, icons, desktop entry and the glibc 2.35 floor, group 2, the Linux packages, group 3, the Windows MSI, and group 4, the macOS disk image, are done; group 5, docs and the release check, is left).
- The four nightly patches requested on 2026-10-06 shipped: `clearer-stream-toggles` (0.20.8),
  `tui-cursor-position` (0.20.9), `tui-movable-dialogs` (0.20.10), `level-detect-window`
  (0.20.11).
- 0.21.0 shipped (structured data): `open-at-line`, `hide-lines`, `folder-source`,
  `structured-field-terms` and `tabular-files` archived; `linux-tray-icon` stays open until
  the maintainer checks it on KDE and GNOME (task 2.1).
- Then, re-planned on 2026-10-06 to about one large, two medium and three small changes
  per minor: **0.21.0** structured data (`tabular-files` with the terminal column view,
  `structured-field-terms`, `folder-source`, `linux-tray-icon`, `open-at-line`,
  `hide-lines`), **0.22.0** time and formats, **0.23.0** remote and system sources,
  **0.24.0** queries and statistics, **0.25.0** patterns and analysis, **0.26.0**
  extensibility, **0.27.0** integrations; see `docs/competitor-analysis.md` section 8.

### Handover notes (2026-10-05, 0.20.1)

- No open PRs. `main` holds everything; the 0.20.1 release PR is the last change, then the
  post-release archive of `disassembly-view` (task 6.3).
- Not checked by hand yet: the #161 fix on Windows (minimise with a rule that has a sound)
  and the ASM view on real executables (preview `target/fasttail-asm-preview.exe`).
- Merge only after every check passed: on 2026-10-05 a merge that did not wait for green
  let a `cargo fmt` failure reach `main` (fixed by #273).
- The terminal code is in `src/tui/` (`app.rs` state, keys, mouse, drawing and tests;
  `bars.rs`, `dock.rs`, `mouse.rs`, `keys.rs`, `form.rs`, `browser.rs`, `settings.rs`,
  `rules.rs`, `presets.rs`, `global.rs`, `tools.rs`, `palette.rs`, `json.rs`); its texts in
  `src/i18n_tui.rs` (`tx` / `txf` / `en`, keyed by the English;
  `every_terminal_text_is_translated` fails on a text without its row). The guide is
  `docs/tui.md`.
- Not checked by hand: Linux in a terminal and over SSH (CI builds and tests it).
- TUI testing: `cargo test --no-default-features --features tui --lib` (about 530 tests)
  and `cargo clippy --no-default-features --features tui --all-targets -- -D warnings` on
  top of the usual checks.
- Preview build for the maintainer:
  1. `cargo build --release --no-default-features --features tui --bin fasttail-tui --target x86_64-pc-windows-gnu`
  2. `x86_64-w64-mingw32-strip`
  3. Send the exe.
- Open questions for the maintainer, not yet answered:
  - PgUp / PgDn acceleration while the key is held. Today queued presses are already
    drawn once, so the limit is the key-repeat rate.
  - Whether "search all streams" (the GUI's `Ctrl+Shift+F` Find results) should come to
    the terminal. The `terminal-interface` spec currently excludes it from 0.20.0.
    Propose it as its own OpenSpec change if wanted.
- The nightly routine (claude.ai Routines, 18:10 Europe/Rome) fires into the session that
  created it. A session started elsewhere follows the rule above by hand if the routine
  did not run.

## For Claude Code sessions with the maintainer

- Talk to the maintainer in Italian; code, comments, commits, PRs and docs stay in English.
- Authorized without asking: running tests, opening PRs, rebasing, pushing branches,
  merging green PRs (own ones included), cutting a release after the preview OK, starting
  the next implementation round. Ask before destructive or unusual actions.
- Implement specs directly on a branch in the main checkout (no worktrees, no parallel
  agent waves); use a separate review agent only for big features.
