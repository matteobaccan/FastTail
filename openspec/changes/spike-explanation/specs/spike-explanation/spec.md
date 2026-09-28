## ADDED Requirements

### Requirement: Explaining a Histogram Span
A right-click on a stream's timeline histogram SHALL open a menu offering "Explain this bar", for the time span of the column under the pointer, and, while a time window is set, "Explain the time window". The time range popup SHALL offer an "Explain" button for its window when both sides can be read. Choosing one SHALL open the stream's Explain tab for that span. The histogram's existing click and drag SHALL keep setting the time window unchanged.

#### Scenario: Explaining a red bar
- **WHEN** the user right-clicks the histogram column covering 14:02:00 to 14:02:59 and picks "Explain this bar"
- **THEN** an Explain tab opens for the stream with the span 14:02:00.000 to 14:02:59.999, and the stream's time window is unchanged.

### Requirement: Explain Tab
The Explain tab SHALL compare the entries of the span with a baseline, by default every timed entry outside the span and, when chosen, the entries of the span of equal length ending where the span starts. It SHALL count every timed entry like the histogram, or, with "Only filtered lines" on, only the entries the stream's filters show. It SHALL show a Volume section (entries per minute on both sides and their ratio), a Levels section (count and share per level on both sides), a Messages section with the message templates of the span (the entry's first line without its leading timestamp, with numbers, hex values, identifiers and IP addresses masked) and, on a stream with an active field parser, a Fields section with `field=value` pairs, skipping a field with more than 1,000 distinct values in the span. In the Messages and Fields sections an item SHALL be listed only when it occurs at least 3 times in the span and its smoothed share is higher in the span than in the baseline, ranked by its count in the span times the logarithm of the share ratio, at most 20 items per section, and an item absent from the baseline SHALL be marked new.

#### Scenario: A new error message in a spike
- **WHEN** a log has a steady flow of INFO lines and, only between 14:02 and 14:04, 1,800 lines `payment <id> failed: gateway timeout`, and the user explains that span
- **THEN** the Messages section lists `payment <*> failed: gateway timeout` first, with 1,800 occurrences and the new mark, and the Levels section shows the ERROR share of the span far above the baseline's.

#### Scenario: A heartbeat is not a difference
- **WHEN** a heartbeat INFO line occurs every second in the span and in the baseline
- **THEN** the heartbeat template is not listed in the Messages section.

#### Scenario: Field values
- **WHEN** a JSON stream's span holds 900 entries with `status=503` against a baseline where `status=503` is rare
- **THEN** the Fields section lists `status=503` with its counts and ratio.

### Requirement: Explain Actions
Clicking a message item SHALL go to its first entry in the span; its menu SHALL offer "Search this message", which sets the stream search to the longest fixed part of the template. Clicking a field item SHALL add the field term `field=value` to the stream's include terms. "Set as time window" SHALL apply the span as the stream's time window.

#### Scenario: From the explanation to the lines
- **WHEN** the user clicks the first message item and then "Set as time window"
- **THEN** the view first shows that message's first line in the span, then only the lines of the span.

### Requirement: Explain Computation and Limits
The comparison SHALL run on a worker thread with progress and Cancel, timing the stream first when it has not been timed, and the sections SHALL fill in as partial results arrive. A baseline of more than 1,000,000 entries SHALL be sampled at a regular interval with counts scaled, and the tab SHALL say so. At most 50,000 templates per side SHALL be counted, further ones as "other". The tab SHALL be a snapshot: appended lines SHALL NOT change it, a Recompute button SHALL run it again, and a truncation or rotation of the file SHALL mark it stale. Nothing of the tab SHALL be persisted.

#### Scenario: Large log
- **WHEN** the user explains a one-minute span of a 6 GB log
- **THEN** the interface stays responsive, the tab shows the progress, and it says the baseline was sampled.
