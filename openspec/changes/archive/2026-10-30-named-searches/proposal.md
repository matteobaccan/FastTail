## Why

lnav 0.15 added **named searches**: several searches stay highlighted at once, each in its
own colour, `n` / `N` move across all of them or one, a row per search in the timeline,
saved in the session. FastTail has one search per stream and colour labels that highlight
but do not navigate.

## What Changes

- *Keep this search* (the search box's `📌`, `Alt+K` in the terminal) keeps the current search
  as a named search with the next free colour; up to 8 per stream. `F3` / `n` walk the kept
  searches when the search box is empty.
- Kept searches are highlighted together; `F3` / `n` go to the next hit of any of them, a
  chip per search focuses one (its hits only), `✖` removes it.
- Each kept search shows its count in its chip (the window's kept searches strip, the
  terminal's filter row). A lane per search in the timeline is left for later.
- Saved per stream in the workspace and sessions (`kept.1=1:timeout`). Both interfaces: the window and the terminal (`fasttail-tui`) get it in the same change.

Target release: **0.32.0** (plan of 2026-10-30, after the competitor scan against 0.30.0; see `docs/competitor-analysis.md` section 8). Priority: **High**. Effort: **M**.

### Non-goals

- Kept searches across streams (the global filter covers that).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `search-and-navigation`: the requirement below is added.

## Impact

`src/tail_engine.rs` a list of named searches over the search job; window search box and timeline; terminal bar chips and keys; config round-trip; texts in every language; README, `docs/ui-design.md`, `docs/tui.md`, CHANGELOG.
