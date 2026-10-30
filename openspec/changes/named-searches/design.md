## Context

The search job finds the hits of one matcher; quick labels highlight text but keep no hit list.

## Goals / Non-Goals

**Goals:** what the proposal lists, over the existing engine and files.

**Non-Goals:** Kept searches across streams (the global filter covers that).

## Decisions

### D1. One job per kept search
Each kept search runs a one-stream `FindAllSession` (the Find all job, `MAX_HITS` listed, the total counted), so it never cancels the stream's own search and matches as it does; a reloaded stream searches again at once, a grown one at most every 3 s. Eight searches are eight passes, fine for the file sizes the jobs already handle.

### D2. Labels unchanged
Quick labels stay what they are (highlight only); a label can be made a kept search from its menu.

## Risks / Trade-offs

- [Many hits per search] → each list capped at the current search's cap, the counts exact.

## Open Questions

- Share colours with the quick labels? Decided: the same colours 1 to 8, highlighted through the quick label path (`TailEngine::quick_labels` keeps returning only the app's labels).
