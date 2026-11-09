## Why

logana imports Notepad++ AnalysePlugin filters; FastTail imports BareTail's settings and
its own rule sets, but users moving from Notepad++ re-type their filters.

## What Changes

- *Import rules…* also reads Notepad++ AnalysePlugin XML (`<SearchText>`, colours, case,
  regex) and UDL keyword lists, turning them into highlight rules; searches marked hidden
  are left out (rules colour lines, they do not hide them). Both interfaces: the window and the terminal (`fasttail-tui`) get it in the same change.

Target release: **0.33.0** (plan of 2026-10-30, after the competitor scan against 0.30.0; see `docs/competitor-analysis.md` section 8). Priority: **Low**. Effort: **S**.

### Non-goals

- Notepad++ themes.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `baretail-migration`: the requirement below is added.

## Impact

`src/config.rs` rule-set import gains a small hand-written reader for the two XML shapes (no new crate); texts; docs.
