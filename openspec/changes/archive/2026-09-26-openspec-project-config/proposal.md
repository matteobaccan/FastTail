## Why

`openspec/config.yaml` holds only `schema: spec-driven` and the commented examples written
by `openspec init`. Every proposal, design, spec and task list generated so far started
without the project's facts (stack, platforms, the never-in-memory engine rule, the
16 MB worker threshold, i18n in every language) or its conventions (CHANGELOG style, one
PR per change, scenarios on every requirement), so they had to be restated in each prompt
and fixed by hand in review.

## What Changes

- Fill the `context` block of `openspec/config.yaml` with the project background: what
  FastTail is, the Rust / egui / eframe / egui_dock stack and the renderers, the supported
  platforms, the engine memory rule and the 16 MB background threshold, where settings and
  sessions are stored, localization, commit / PR / CHANGELOG conventions and what CI checks.
- Add `rules` per artifact: `proposal` (problem first, non-goals, target release, new
  `fasttail.ini` keys), `specs` (Purpose + Requirements, a scenario per requirement,
  concrete limits), `design` (UI thread vs worker, memory per file/line, large / growing /
  rotated files, Windows file sharing), `tasks` (i18n, docs, CHANGELOG, tests, fmt, archive).
- Remove the commented examples left by `openspec init`.
- No change to the program: nothing under `src/` is touched.

## Capabilities

### New Capabilities
- `spec-authoring`: how OpenSpec artifacts for FastTail are produced — the project context
  and per-artifact rules that `openspec instructions` hands to whoever writes them.

### Modified Capabilities
<!-- none -->

## Impact

- `openspec/config.yaml` only (plus the new `openspec/specs/spec-authoring/spec.md` once
  archived).
- Affects every future `openspec instructions` / `/opsx:propose` output; existing changes
  under `openspec/changes/` are not rewritten.
- No release note: the file is not shipped. Target: the 0.11.0 cycle, merged before the
  pending 0.11.0 changes get their next artifacts.
