## ADDED Requirements

### Requirement: Kept Search Lanes
The window's timeline strip SHALL draw one lane per kept search, in the search's colour, marking the columns that hold its hits.

#### Scenario: Two kept searches in the timeline
- **WHEN** `timeout` and `refused` are kept searches and the timeline is open
- **THEN** the strip shows two lanes in their colours, each marked where its search hits.
