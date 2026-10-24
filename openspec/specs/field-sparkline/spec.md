# field-sparkline Specification

## Purpose
TBD

## Requirements

### Requirement: Field Plotted Over Time
The user SHALL be able to plot a numeric field of a stream over its timeline histogram, from the field's column header menu or the command palette. Each histogram bucket SHALL show the field's average, maximum, minimum or 95th percentile of the visible lines in that bucket, chosen in the strip's menu, on the field's own scale with its minimum and maximum written at the strip's edge. Values with a consistent unit (durations such as `ms` and `s`, sizes such as `KB` and `MB`) SHALL be normalised; lines without a numeric value SHALL be skipped. Up to three fields SHALL be plotted at once, each in its own theme colour with a legend that removes it. Hovering a bucket SHALL show the plotted values with the line count, and selecting a time range SHALL work as on the histogram alone. The aggregates SHALL be kept per bucket, not per line, and filled by the background timing pass. The plotted fields SHALL be saved per stream; older builds ignore them.

#### Scenario: Latency over the last hour
- **WHEN** the user plots `latency_ms` with the maximum on a log of requests
- **THEN** a line over the histogram shows the highest latency of each bucket, and hovering a bucket shows that value next to its line count.

#### Scenario: Units normalised
- **WHEN** a field's values are written as `850ms` and `1.2s`
- **THEN** they are plotted on one scale in the same unit.

#### Scenario: Filters followed
- **WHEN** an include filter keeps only the lines of one endpoint
- **THEN** the plotted values are computed from those lines only.

### Requirement: Terminal Field Sparkline
The terminal interface SHALL draw the first plotted field of a stream as a sparkline row at the bottom of the stream's window, with its statistic, its lowest and highest value, chosen and removed from the stream's fields list where its statistic is also cycled.

#### Scenario: Sparkline in the terminal
- **WHEN** the user presses `p` on the `ms` field in the terminal's fields list
- **THEN** the stream's window shows a row with `ms avg`, one block character per column over the time the values cover, and the lowest and highest value.
