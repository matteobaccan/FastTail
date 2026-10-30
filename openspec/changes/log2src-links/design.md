## Context

`source_links` resolves files under the source folders and opens the editor; Drain templates give a message's fixed words.

## Goals / Non-Goals

**Goals:** what the proposal lists, over the existing engine and files.

**Non-Goals:** Parsing the languages fully (patterns over the format strings are enough).

## Decisions

### D1. Regex extraction
Each language has a few regexes for its logging calls; the format string becomes a pattern (`{}` / `%s` / `{0}` as wildcards).

### D2. Matched by template
A row is matched through its Drain template's fixed words first, then the full pattern.

## Risks / Trade-offs

- [Large source trees] → the index is built once in the background and cached beside the source folder list.

## Open Questions

- Languages first? Proposed: Rust, Java, Python, then Go, C#, JavaScript.
