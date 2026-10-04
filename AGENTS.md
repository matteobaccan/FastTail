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

- Latest release: **0.16.0** (2026-09-30): terminal-interface windows (floating windows
  that move, resize and overlap, `[x]` to close, empty workspace, full-screen help). The
  terminal interface is still a preview. 0.15.0 introduced it and the Commander theme.
- Next: nightly patch releases (`0.16.x`) while **0.20.0 terminal interface (TUI)** is
  completed, spec `openspec/changes/tui-interface/` (`tasks.md` is the checklist, ticked
  as work lands):
  - Group 4. What is open:
    - 4.1: the Settings dialog exists; the PIN-lock page and the missing fields are open.
    - 4.2: quick labels in the rule editor (the editor itself is done).
    - 4.4: the global filter.
    - 4.5: external tools.
    - 4.6: the lock screen.
    - 4.5b: the `:` palette with line jump, and registry bindings with the Kitty protocol.
  - Groups 5 to 8: hand-off, release pipeline, i18n, `docs/tui.md`, the parity checklist.
  - 0.20.0 ships at full GUI parity without the "preview" label.
- In parallel, as nightly patches: `openspec/changes/release-packages/` (deb, rpm,
  AppImage, MSI, dmg, one PR per task group, each validated with a `workflow_dispatch`
  build of `build.yml` on its branch before merging; task 5.0, the downloads grid, is done).
- Then **0.21.0** analysis (`structured-field-terms`, field statistics, ...),
  **0.22.0** sources/integrations and **0.23.0** binary views (`disassembly-view`); see
  `docs/competitor-analysis.md` section 8.

### Handover notes (2026-09-30, end of the 0.16.0 session)

- No open PRs. `main` holds everything; the 0.16.0 release PR is the last change.
- Where to resume: `openspec/changes/tui-interface/tasks.md`, group 4 (4.4 global filter
  next; the rule editor, `src/tui/rules.rs`, is the model for the dialogs). The terminal code is in `src/tui/`:
  - `app.rs`: state, keys, mouse, drawing, tests;
  - `dock.rs`: layout geometry and drop zones;
  - `mouse.rs`: the hit map;
  - `keys.rs`: the key map;
  - `form.rs`: the dialog toolkit;
  - `browser.rs`: the Open dialog;
  - `settings.rs`: the Settings form;
  - `rules.rs`: the highlight-rule editor;
  - `presets.rs`: the filter presets.

  Their rules are in the spec's `terminal-interface` delta. `src/dock_layout.rs` is shared
  with the GUI.
- TUI testing: `cargo test --no-default-features --features tui --lib tui::` (111 tests)
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
