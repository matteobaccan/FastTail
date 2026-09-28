## ADDED Requirements

### Requirement: Statistics Tab and Sources
The application SHALL offer a Statistics dock tab for a stream, not saved in the dock layout, opened from the stream menu, from the column header menu of a field in the column view, and from a cell's menu. Its source SHALL be either a field of the stream's field parser (see the structured-fields capability) or a regular expression, whose first capture group, or whole match when it has no group, is the value. The statistics SHALL be computed over the entry lines the stream shows under all its filters, excluding context lines and stack-trace continuation lines, on a worker thread with progress shown in the tab for a stream of any size, with partial results shown at most every 250 ms. A change of the stream's filters SHALL recompute them, appended visible lines SHALL be added incrementally, and a truncation, rotation or rewrite SHALL recompute them from scratch. An invalid regular expression SHALL be flagged and compute nothing.

#### Scenario: Statistics of a field
- **WHEN** a logfmt stream has the parser active and the user chooses "Statistics of `status`" in the column header menu
- **THEN** a Statistics tab opens for that stream with `status` as source and fills in its table.

#### Scenario: A regex source without a parser
- **WHEN** a plain-text stream has no field parser and the user enters the source regex `took (\d+)ms`
- **THEN** the tab computes the statistics of the captured numbers of the visible lines matching it.

#### Scenario: Follows the view
- **WHEN** the Statistics tab shows `status` and the user adds the include term `POST`
- **THEN** the statistics are recomputed over the lines containing `POST` only.

#### Scenario: Large file
- **WHEN** the user opens the Statistics tab on a 3 GB stream
- **THEN** the interface stays responsive, the tab shows the progress, and its counts grow until the progress reaches 100%.

### Requirement: Top Values
The tab SHALL list the most frequent values of the source with their count and share of the counted lines, sorted by count descending, as a table with a horizontal bar per value; N SHALL default to 20 and be selectable up to 1,000. Values longer than 256 bytes SHALL be truncated with `…`. Distinct values SHALL be counted exactly up to 100,000; beyond that, further values SHALL be counted in one "other values" row and the distinct count SHALL be shown as a lower bound. For each listed value the tab SHALL show its count per detected level. Lines where the source yields no value SHALL be counted in a "no value" row. "Copy as CSV" SHALL copy the listed rows with their counts, shares and per-level counts.

#### Scenario: Top paths of an access log
- **WHEN** the visible lines hold `path=/api/pay` 700 times, `path=/api/login` 200 times and `path=/health` 100 times
- **THEN** the tab lists `/api/pay` 700 (70%), `/api/login` 200 (20%) and `/health` 100 (10%), in that order, each with a bar proportional to its count.

#### Scenario: Cardinality cap
- **WHEN** the source is a request id with 250,000 distinct values among the visible lines
- **THEN** the tab shows the distinct count as at least 100,000 and an "other values" row.

#### Scenario: CSV copy
- **WHEN** the user presses "Copy as CSV" on a table of 3 values
- **THEN** the clipboard holds a header line and 3 lines with value, count, share and the per-level counts, separated by commas.

### Requirement: Numeric Summary
When at least 90% of the non-empty values of the source read as numbers, optionally followed by one of the unit suffixes `ms`, `s`, `us`, `%`, `B`, `KB`, `MB`, the tab SHALL show their count, minimum, maximum, mean and the 50th, 90th, 95th and 99th percentiles, the percentiles within 1% relative error, and SHALL NOT store every value.

#### Scenario: Latency percentiles
- **WHEN** the source `duration_ms` holds the values 1 to 1,000 once each over the visible lines
- **THEN** the tab shows min 1, max 1,000, mean 500.5, and p50, p90, p95 and p99 within 1% of 500, 900, 950 and 990.

#### Scenario: Text values
- **WHEN** fewer than 90% of the values read as numbers
- **THEN** no numeric summary is shown and the top values are listed as text.

### Requirement: Time Breakdown Chart
On a stream with usable timestamps, the tab SHALL offer a line chart of the counts per time slice of the 5 most frequent values plus the remaining values as one series, or, for a numeric source, of the 50th and 95th percentiles per slice. The slice width SHALL be picked automatically so the span of the counted lines has at most 240 slices, or chosen among 1 second, 1 minute, 5 minutes, 1 hour and 1 day. Opening the chart SHALL time the stream as the timeline histogram does, in the background above 16 MB. Hovering a point SHALL show its slice, series and value.

#### Scenario: Errors over the afternoon
- **WHEN** the source is `status` over a log from 14:00 to 18:00 and the user opens the time chart with 5-minute slices
- **THEN** the chart shows 48 slices with one line per top status value, and hovering the 15:30 point of `503` shows its count in that slice.

#### Scenario: No timestamps
- **WHEN** the stream has no usable timestamps
- **THEN** the time chart is unavailable, with the same hint as the time range.

### Requirement: Filter from a Value
Clicking a listed value SHALL add an include term matching it to the stream: the field term `key=value` for a field source, or, for a regex source, a regular expression term equal to the source with its capture group replaced by the escaped value. `CTRL + click` SHALL add the corresponding exclude term. When the stream already has 8 terms on that side, nothing SHALL be added and a notice SHALL say so.

#### Scenario: Drill down on a status
- **WHEN** the user clicks the value `503` of the source `status`
- **THEN** the stream gains the include term `status=503`, shows only those lines, and the statistics are recomputed over them.

#### Scenario: Excluding a value
- **WHEN** the user `CTRL`-clicks the value `/health` of the source `path`
- **THEN** the stream gains the exclude term `path=/health` and no line with that path is visible.
