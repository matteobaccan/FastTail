## ADDED Requirements

### Requirement: Search All Streams
The user SHALL be able to run one query across every open stream. The match SHALL be the same case-insensitive text match as the per-stream search, over the lines each stream shows under its own include/exclude, level and time filters; streams in HEX view SHALL be skipped and reported as skipped. Each stream SHALL be searched by its own background job, independent of the stream's own filter and search scans, with at most four jobs (fewer on machines with fewer cores) running at once and the others queued. A new query, a Stop button and closing the results SHALL cancel every job; closing a stream SHALL cancel its job and remove its results. At most 100,000 hits SHALL be stored per stream, with the true total still counted. The results SHALL be a snapshot of each stream at the moment its job started, with a Refresh action to run the query again.

#### Scenario: A request id across three logs
- **WHEN** `gateway.log`, `payment.log` and `audit.log` are open and the user searches all streams for `req-7f3a`
- **THEN** each stream is searched in the background, and the results report 4 matches in `gateway.log`, 2 in `payment.log` and none in `audit.log`.

#### Scenario: Filters of each stream are honoured
- **WHEN** `payment.log` has an exclude filter `healthcheck` and one line contains both `req-7f3a` and `healthcheck`
- **THEN** that line is not among the results, as it would not be for the stream's own search.

#### Scenario: Bounded concurrency
- **WHEN** ten multi-GB streams are open and the user runs a query on an eight-core machine
- **THEN** at most four searches run at the same time, the others show as queued, and the interface stays responsive.

#### Scenario: Cancelling
- **WHEN** a search across streams is running and the user presses Stop
- **THEN** every running and queued job stops, and the results found so far stay listed.

### Requirement: Find Results Tab
The results of a search across streams SHALL be shown in a single "Find results" dock tab, opened or focused by `Ctrl+Shift+F` with the focused stream's query prefilled, and not saved in the dock layout. Results SHALL be grouped by stream, each group headed by the stream name, its match count, its progress while running and a note when only the first 100,000 hits are listed; groups SHALL be collapsible, and the whole list SHALL be virtualized. Clicking a result, or selecting it with the keyboard and pressing `Enter`, SHALL activate that stream's tab, focus its panel, centre the line (or the next visible line when the stream's filters now hide it) and pause follow mode, without changing the stream's own search query. When a stream has been reloaded, truncated, rewritten or switched to another file since its results were found, its group SHALL be marked stale and its results SHALL NOT jump.

#### Scenario: Jumping to a result
- **WHEN** the Find results tab lists a match on line 88,120 of `payment.log`, which is a background tab, and the user clicks it
- **THEN** the `payment.log` tab becomes active and focused, line 88,120 is centred, follow is paused, and the search box of `payment.log` keeps its previous query.

#### Scenario: Stream rewritten after the search
- **WHEN** `gateway.log` is truncated by its writer after the search and the user clicks one of its results
- **THEN** the view does not move and the group says the stream changed and offers Refresh.
