## ADDED Requirements

### Requirement: Query Tab and Pipeline Syntax
The stream menu SHALL offer "Query…", opening a Query dock tab for that stream, not saved in the dock layout, with a query box, Run and Stop buttons and a result table. A query SHALL be a pipeline of at most 16 stages joined by `|`, at most 64 KB long, applied in order to the lines the stream shows under its filters, with the stages: `where <expr>` (the filter expression syntax of the filters-and-highlighting capability, field operands included), `parse /regex/` (adds the regex's named groups as fields), `fields a, b` (keeps and orders columns; `line`, `ts`, `level` and `text` are always available), `stats <aggregations> [by k1, k2]` with the aggregations `count()`, `sum(x)`, `avg(x)`, `min(x)`, `max(x)`, `dc(x)`, `first(x)`, `last(x)` and `p50(x)`, `p90(x)`, `p95(x)`, `p99(x)`, each optionally named with `as`, `timeslice <duration>` (adds `_slice`), `sort [-]col, …` and `head N`. A query that does not parse SHALL be reported with its stage number and position and SHALL NOT run. The last 20 queries run SHALL be kept in a history persisted in the `[query]` section of `fasttail.ini` as `history.1`…`history.20`.

#### Scenario: Errors per endpoint
- **WHEN** a logfmt stream shows 1,000 lines, 30 of them with `status=503 path=/api/pay` and 10 with `status=500 path=/api/login`, and the user runs `where status>=500 | stats count() as errors by path | sort -errors`
- **THEN** the table has the columns `path` and `errors` and the rows `/api/pay 30` and `/api/login 10`, in that order.

#### Scenario: Parsing a plain-text log
- **WHEN** a plain-text stream has lines `took 120ms user=ann` and the user runs `parse /took (?P<ms>\d+)ms user=(?P<user>\w+)/ | stats avg(ms) by user`
- **THEN** the table has one row per user with the average of their captured durations.

#### Scenario: Syntax error
- **WHEN** the user runs `where status>=500 | stats count( by path`
- **THEN** the tab reports an error in stage 2 at the position of the missing parenthesis and the table is unchanged.

### Requirement: Query Execution and Results
A query SHALL run on a worker thread with progress shown in the tab, whatever the size of the stream, and Stop SHALL cancel it; the result SHALL be a snapshot of the visible lines when it started, and a truncation, rotation or rewrite during the run SHALL cancel it with a notice. A result SHALL hold at most 100,000 rows; a `stats` stage SHALL hold at most 100,000 groups, and a query exceeding that SHALL stop with a "too many groups" error. `dc` SHALL be exact up to 100,000 distinct values and a lower bound beyond, and the percentiles SHALL be within 1% relative error. Numeric aggregations SHALL skip values that do not read as numbers and the tab SHALL report how many were skipped. The result table SHALL be virtualized and sortable by clicking a column header, SHALL offer "Copy as CSV", and for a query without `stats` a double-click on a row SHALL bring the stream to the front with that line centred.

#### Scenario: Large file
- **WHEN** the user runs `stats count() by level` on a 3 GB stream
- **THEN** the interface stays responsive, the tab shows progress, and the table lists one row per level when the run completes.

#### Scenario: Stop
- **WHEN** a query is running and the user presses Stop
- **THEN** the job is cancelled and the table keeps its previous content.

#### Scenario: Jump to a line
- **WHEN** the user runs `where timeout | fields line, ts, text | head 5` and double-clicks the third row
- **THEN** the stream comes to the front with that row's line centred and follow mode paused.

#### Scenario: Too many groups
- **WHEN** a query groups by a request id with 300,000 distinct values
- **THEN** it stops with a "too many groups" error naming the limit of 100,000.
