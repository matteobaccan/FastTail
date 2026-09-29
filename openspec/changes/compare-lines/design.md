## Context

Each stream keeps its own row selection; copy expands collapsed rows and strips ANSI per
the stream's mode. The inline JSON detector (`log-intelligence`) finds a JSON object in a
line. Tabs are `FastTailTab` variants; `FindResults` is the precedent for a tab that is
not saved in the layout. No diff code exists in the project.

## Goals / Non-Goals

**Goals:** one click from two selected lines to a readable diff; noise (timestamps, ids)
ignorable; back to the source lines.

**Non-Goals:** file diff, merge, persistence.

## Decisions

### D1. `similar` for the diff
Myers / patience diff with word-level (`TextDiff::from_words`) and line-level modes,
pure Rust, no dependencies, widely used. *Alternative:* hand-written Myers — rejected,
not worth maintaining.

### D2. Normalise, diff, map back
Ignore options replace spans by placeholders before diffing (leading timestamp → `⟨ts⟩`,
numbers → `⟨n⟩`, hex ids and UUIDs → `⟨id⟩`, runs of spaces → one, case folded), keeping
a span map so the view highlights the original text. The leading timestamp uses the
span found by timestamp detection.

### D3. The compare mark
One mark per window: either a line or a selection (list of line indices of one stream,
text captured at marking time, so later truncation does not change it). A gutter marker
shows it; marking again replaces it; closing its stream clears it.

### D4. Size caps and threads
Two lines: done on the UI thread (lines are capped at the long line cap). Regions: up
to 20,000 lines per side, diffed on a worker with a deadline (`similar`'s timeout, 2 s);
on timeout the result falls back to a coarser line diff and says so.

### D5. JSON mode
When both lines contain a JSON object, "Compare as JSON" canonicalises both (sorted
keys, 2-space indent) and runs a line diff over the pretty text, so a changed field
appears as one changed row.

### D6. Keys
`F7` / `SHIFT + F7` (unused in 0.12.0) move between changes when the Compare tab has
focus; with `remappable-shortcuts` they become `compare.next` / `compare.prev`.

### As implemented (0.14.0)
- **Normalisation (D2)** keys each token instead of rewriting the text: the diff runs on
  the keys and maps back to the tokens' byte ranges, so no span map is needed. "Ignore
  numbers" replaces every digit run inside a word (`5ms` → `⟨n⟩ms`).
- **Threads (D4):** regions are diffed on the UI thread with `similar`'s 2 s deadline
  (the result is flagged coarse when it is reached); no worker thread.
- **The mark (D3)** is shown in the row menu entry (*Compare the selection with …*) and
  a notice in the stream bar, not as a gutter marker.

## Risks / Trade-offs

- [Large regions slow] → caps and worker deadline (D4).
- [Placeholder ignores hide a real difference] → the options are visible toggles; the
  timestamp one is the only default.

## Migration Plan

None.

## Open Questions

- Should "Compare selected lines" also work with more than two selected rows by
  comparing the first against each other? Proposed: no; exactly two.
- Default for "ignore numbers"? Proposed: off; request ids often matter.
