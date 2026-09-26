## 1. Configuration

- [ ] 1.1 Replace the commented `openspec init` examples in `openspec/config.yaml` with a `context` block covering the facts listed in the `spec-authoring` spec (check each against Cargo.toml, README and `src/i18n.rs`)
- [ ] 1.2 Add `rules` for `proposal`, `specs`, `design` and `tasks` as listed in the spec
- [ ] 1.3 Keep the file under 3 KB and in English

## 2. Verification

- [ ] 2.1 Run `openspec instructions proposal|design|specs|tasks --change openspec-project-config --json` and check `context` is present in each and `rules` holds only that artifact's rules
- [ ] 2.2 Run `openspec validate --all` (warnings are printed twice): no error, no config warning

## 3. Wrap-up

- [ ] 3.1 Open a `chore/openspec-config` PR (no CHANGELOG entry: the file is not shipped)
- [ ] 3.2 After merge, archive the change so `openspec/specs/spec-authoring/spec.md` is created with `## Purpose` and `## Requirements`
