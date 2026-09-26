## ADDED Requirements

### Requirement: Project context for OpenSpec artifacts
`openspec/config.yaml` SHALL keep `schema: spec-driven` and SHALL define a `context` block,
in English, stating: what FastTail is; the stack (Rust 2021, egui / eframe / egui_dock, the
wgpu, glow and software renderers, notify, regex); the supported platforms (Windows x86_64,
Linux x86_64 and ARM64, macOS ARM64); the engine rule that a file is never held in memory
(line index of 8 bytes per line plus level and timestamp caches) and that files above 16 MB
filter, search and scan timestamps on a worker thread; that settings live in `fasttail.ini`
and sessions in `*.fasttail-session.ini`; that every user-visible string goes through
`src/i18n.rs` in every language; and the conventions for commits, pull requests, CHANGELOG
entries and CI checks.

#### Scenario: Context reaches every artifact
- **WHEN** `openspec instructions <artifact> --change <name> --json` is run for any of
  `proposal`, `design`, `specs` or `tasks`
- **THEN** the returned `context` contains the project context from `openspec/config.yaml`

#### Scenario: No leftover init examples
- **WHEN** `openspec/config.yaml` is read
- **THEN** it holds no commented example from `openspec init` (TypeScript / React /
  e-commerce placeholders)

### Requirement: Per-artifact rules
`openspec/config.yaml` SHALL define `rules` keyed only by the schema's artifact ids:
- `proposal`: state the user-visible problem before the change, include non-goals, name the
  target release and any key added to `fasttail.ini`;
- `specs`: every spec has `## Purpose` and `## Requirements`, every requirement at least one
  `#### Scenario:` with WHEN / THEN, limits given as numbers;
- `design`: say what runs on the UI thread and what on a worker, the memory cost per file or
  line, and how files above 16 MB, growing, rotated or truncated files and Windows file
  sharing are handled, where relevant;
- `tasks`: include translations in every language, README / docs, CHANGELOG `[Unreleased]`
  and tests, and end with `cargo fmt`, `cargo test` and archiving the change after release.

#### Scenario: Rules reach only their artifact
- **WHEN** `openspec instructions tasks --change <name> --json` is run
- **THEN** `rules` contains the task rules and none of the proposal, design or specs rules

#### Scenario: Configuration stays valid
- **WHEN** `openspec validate --all` is run after the change
- **THEN** it reports no error and no warning about `openspec/config.yaml`
