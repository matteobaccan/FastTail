## Context

The engine already runs every analysis as a background job owned by the stream
(`TailEngine::stats`, `query`, `patterns`, `explain`, `operations`) and the window only
draws their results. The terminal's dock has `DockTab::Statistics`, `Query`, `Patterns`,
`Explain` and `Compare` panels that only carry a name.

## Goals / Non-Goals

**Goals:** the window's feature in the terminal, over the same engine and files.

**Non-Goals:** Charts beyond sparklines and text bars. Editing a tab's settings that the window keeps only in its own panels (colours, sizes).

## Decisions

### D1. Same jobs, terminal views
Each tab reads the engine state the window reads; nothing is computed twice and a tab
opened in one interface shows the same counts in the other after a hand-off.

### D2. Tables as lists
Every result is a virtualised list with a header row, `Up` / `Down` / `PgUp` / `PgDn`,
`s` to cycle the sort column and `/` to narrow, the keys of the existing terminal lists.

### D3. Bars and sparklines in text
Shares and Gantt bars are drawn with `█▌` (`#` in ASCII mode), time series with the
sparkline row's blocks.

## Risks / Trade-offs

- [Five views in one change is large] → one PR per tab, Statistics first (the others reuse its table).
- [Narrow terminals] → columns drop from the right; the value column keeps at least 20 cells.

## Open Questions

- Open the Statistics tab from the column view with a key on the cursor's column? Proposed: yes, `S`.
