## Context

`source_links` resolves files under the source folders and opens the editor; Drain templates give a message's fixed words.

## Goals / Non-Goals

**Goals:** what the proposal lists, over the existing engine and files.

**Non-Goals:** Parsing the languages fully (patterns over the format strings are enough).

## Decisions

### D1. Regex extraction
Each language has a few regexes for its logging calls; the format string becomes a pattern (`{}` / `%s` / `{0}` as wildcards).

### D2. Matched by template
A row is matched by the statements whose longest literal part it contains, then by their full pattern; the statement with the most literal text wins.

## Risks / Trade-offs

- [Large source trees] → the index is built once per run in the background (at most 50,000 files of 2 MB, build folders skipped); it is not cached on disk.

## Open Questions

- Languages first? Decided: all six in the first version (Rust, Java / Kotlin, Python, Go, C#, JavaScript / TypeScript).
