## Context

The search job finds the hits of one matcher; quick labels highlight text but keep no hit list.

## Goals / Non-Goals

**Goals:** what the proposal lists, over the existing engine and files.

**Non-Goals:** Kept searches across streams (the global filter covers that).

## Decisions

### D1. One job, several matchers
The search job takes a list of matchers and tags each hit with its search, so the cost stays one pass.

### D2. Labels unchanged
Quick labels stay what they are (highlight only); a label can be made a kept search from its menu.

## Risks / Trade-offs

- [Many hits per search] → each list capped at the current search's cap, the counts exact.

## Open Questions

- Share colours with the quick labels? Proposed: the same eight colours, a kept search taking a free one.
