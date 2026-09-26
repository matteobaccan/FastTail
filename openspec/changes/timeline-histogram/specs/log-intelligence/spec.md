## ADDED Requirements

### Requirement: Timeline Histogram
A stream SHALL offer a timeline histogram, toggled from the time range controls, that shows the number of lines over the stream's time span as bars stacked by detected level (ERROR and FATAL, WARN, INFO, DEBUG and TRACE, unknown) in the theme's level colours, counting every timed line of the stream regardless of the active filters. Opening the histogram SHALL build the timestamp cache if needed, in the background for large files, and the bars SHALL fill in as lines are timed. The histogram SHALL be maintained incrementally from the timestamp and level caches, with at most 2,048 buckets whose width starts at one second and doubles as the span grows, and SHALL stay consistent when the file grows, is truncated or is rewritten. Clicking a bar SHALL set the time range filter to that bar's span and dragging across bars SHALL set it to the dragged span, writing the bounds into the from/to fields so the window behaves exactly like a typed one; the current window SHALL be shaded on the histogram. Hovering a bar SHALL show its time span and its counts per level. An optional lane SHALL mark where the lines matching the current search fall. The histogram SHALL be unavailable, with the same hint as the time fields, when the stream has no usable timestamps. Its shown state and the search lane option SHALL be persisted in `fasttail.ini`.

#### Scenario: Spotting an error burst
- **WHEN** a day-long log has a steady volume of INFO lines and 180 ERROR lines between 14:02 and 14:04, and the user opens the histogram
- **THEN** the bars covering 14:02 to 14:04 carry a visible ERROR segment in the error colour, and hovering one of them shows its span and its ERROR count.

#### Scenario: Selecting a window by dragging
- **WHEN** the user drags across the bars from the one starting at 14:02:00 to the one ending at 14:04:59
- **THEN** the from field reads the 14:02:00 bound, the to field reads the 14:04:59 bound, only lines stamped from 14:02:00.000 to 14:04:59.999 are visible, and the selected span is shaded on the histogram.

#### Scenario: Histogram on a large untimed log
- **WHEN** the user opens the histogram on a 6 GB stream that has never been timed
- **THEN** the interface stays responsive, the stream bar shows the timing progress, and the bars grow as the lines are timed.

#### Scenario: Filters do not hide the histogram's data
- **WHEN** an include filter `payment` is active and the user opens the histogram
- **THEN** the bars count every timed line of the stream, not only the lines containing `payment`.

#### Scenario: Log rewritten
- **WHEN** the writer truncates the log to zero and writes new lines
- **THEN** the histogram is emptied and then shows only the new lines.
