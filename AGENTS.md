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
a patch release is cut from `main` (`0.15.1`, `0.15.2`, ...) when something was merged since
the last tag; nothing merged, no release. Steps 1 to 3 above are skipped for it (the
maintainer tests the published build); step 4 applies (release PR, CI green, merge, tag
`vx.y.z`), with the CHANGELOG `[Unreleased]` entries of the day as its section. A minor
version (`0.16.0`, ..., `0.20.0`) is cut only when a milestone of the plan is complete, with
the full process. Work that is not ready stays out of `main` or is described in the
CHANGELOG as a preview.

Fixed decisions: release targets are Windows x86_64, Linux x86_64, Linux ARM64,
macOS ARM64 (no Windows ARM64, no macOS test job); release profile keeps thin LTO and
debug info for crash logs.

## Current state

Update this section in each release PR.

- Latest release: **0.15.0** (2026-09-30): Commander theme and the terminal interface as a
  preview (`fasttail-tui` in every archive).
- Next: nightly patch releases (`0.15.x`) while **0.20.0 terminal interface (TUI)** is
  completed, spec `openspec/changes/tui-interface/`: task group 4 (rule editor, presets,
  global filter, external tools, lock, the rest of 4.5b: `?` with no file, `:` palette,
  registry bindings), then groups 5 to 8 (hand-off, release pipeline, i18n, docs,
  parity checklist). 0.20.0 ships at full GUI parity without the "preview" label.
- Then **0.21.0** analysis (`structured-field-terms`, field statistics, ...),
  **0.22.0** sources/integrations and **0.23.0** binary views (`disassembly-view`); see
  `docs/competitor-analysis.md` section 8.

## For Claude Code sessions with the maintainer

- Talk to the maintainer in Italian; code, comments, commits, PRs and docs stay in English.
- Authorized without asking: running tests, opening PRs, rebasing, pushing branches,
  merging green PRs (own ones included), cutting a release after the preview OK, starting
  the next implementation round. Ask before destructive or unusual actions.
- Implement specs directly on a branch in the main checkout (no worktrees, no parallel
  agent waves); use a separate review agent only for big features.
