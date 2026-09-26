## Context

OpenSpec 1.3 reads `openspec/config.yaml` and injects its `context` into the instructions of
every artifact, and `rules.<artifact-id>` into the instructions of that artifact only
(`openspec instructions <id> --change <name> --json` returns them as `context` and `rules`).
Both are constraints for the writer, not text copied into the artifact. Today the file has
neither, so the project facts live only in README, CHANGELOG and the maintainers' heads.

## Goals / Non-Goals

**Goals:**
- One short, factual `context` that lets a fresh writer produce artifacts consistent with the
  code and the existing specs.
- Per-artifact `rules` that encode the review comments repeated so far (missing scenarios,
  vague limits, forgotten translations / CHANGELOG / docs, no word on the worker thread).
- `openspec validate` keeps passing; `openspec instructions` shows the new context and rules.

**Non-Goals:**
- Changing the schema (`spec-driven` stays) or adding a custom one.
- Rewriting the artifacts of the changes already pending (filter-presets, stdin-input, ...).
- Duplicating README: the context names facts and rules, it does not document features.

## Decisions

- **Facts that change rarely go in `context`; version numbers mostly stay out.** Stack names
  and platform list are stable; dependency versions (egui 0.36, notify 9) are stated once so
  a writer does not propose APIs from another major, and are refreshed when a major bump
  lands. Alternative considered: no versions at all — rejected, egui APIs differ a lot
  between minors.
- **Limits are written as numbers** (8 bytes per line, 16 MB worker threshold) because the
  specs already state them that way and new requirements must stay consistent.
- **Rules are keyed by the schema's artifact ids** (`proposal`, `design`, `specs`, `tasks`);
  any other key would be ignored by OpenSpec (it warns about unknown artifact ids).
- **English only**, like the specs and docs, even though the maintainers talk in Italian.
- **Keep it small** (well under OpenSpec's context size limit, target < 3 KB): every byte is
  sent with every artifact request.

## Risks / Trade-offs

- [Context drifts from the code, e.g. after a renderer or dependency change] → the
  "update all the documentation" step before each release also reviews
  `openspec/config.yaml`, and a PR that bumps a major dependency named in it updates it.
- [Rules too strict for small changes, e.g. forcing a design section for a one-line fix] →
  rules say what to cover *when relevant* (design.md itself is optional in the schema).
- [Writers copy the context into artifacts] → OpenSpec's own instructions forbid it; review
  catches it.
