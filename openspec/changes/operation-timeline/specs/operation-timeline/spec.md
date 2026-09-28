## ADDED Requirements

### Requirement: Operations Tab and Sources
A stream SHALL offer an Operations tab, opened from the stream menu ("Operations…") and, on a stream with an active field parser, from the column header menu. Its id source SHALL be a field of the stream's parser or a regular expression with exactly one capture group, applied to the first line of each entry; its optional name source SHALL be a field or the first line's text after its timestamp. An operation SHALL be the set of entries the stream shows under its filters whose id source yields the same value; entries without a value SHALL belong to no operation, and continuation lines SHALL belong to their entry's operation.

#### Scenario: Operations by request id
- **WHEN** the user opens the Operations tab on a log whose lines carry `req=7f3a`, `req=91c2` and so on, with the regex source `req=(\w+)`
- **THEN** the tab lists one row per request id.

#### Scenario: Filters apply
- **WHEN** the stream's exclude term is `healthcheck` and a request's only lines are health checks
- **THEN** that request is not listed.

### Requirement: Operation Table and Gantt Bars
Each operation row SHALL show the id, the name, the start and end (the earliest and latest timestamp of its lines), the duration, the line count and the worst level, and a bar from start to end on a time axis shared by all rows, coloured with the theme's colour of the worst level; an operation whose start equals its end SHALL be drawn as a tick, and an operation without timestamps SHALL be listed without a bar. The table SHALL be sortable by start (the default), duration, line count and worst level, narrowed by a text box matching id or name and by a minimum duration. The axis SHALL zoom with the mouse wheel around the pointer, pan with a drag, and shade the stream's time window when one is set.

#### Scenario: Finding the slow request
- **WHEN** the user sorts the Operations tab by duration
- **THEN** the first row is the operation with the longest span between its first and last timestamp, and its bar is the longest on the axis.

#### Scenario: A failed request stands out
- **WHEN** one request has an ERROR line among INFO lines
- **THEN** its row shows ERROR as worst level and its bar is drawn in the error colour.

### Requirement: Navigation from an Operation
Clicking an operation SHALL go to its first line in the stream. Its menu SHALL offer "Filter to this operation", which adds the field term `key=value` for a field source, or an include term matching the captured value in the pattern's context for a regex source; "Set as time window", which applies its span; and "Copy as CSV" for the table. Hovering a row SHALL show its first line.

#### Scenario: Reading one request
- **WHEN** the user picks "Filter to this operation" on the operation `7f3a` of a JSON stream whose id source is the field `req`
- **THEN** the include terms gain `req=7f3a` and the view shows only that request's lines.

### Requirement: Operations on Large and Growing Files
The operations SHALL be computed on a worker thread with progress and Cancel, timing the stream first when needed, and the table SHALL fill in as partial results arrive. At most 100,000 operations SHALL be tracked; further ids SHALL be counted and reported in the tab, not listed. While the file grows, appended lines SHALL extend or add operations without recomputing the others. A change of the filters or of the sources, a truncation or a rotation SHALL recompute the operations. Nothing of the tab SHALL be persisted.

#### Scenario: Following a live service
- **WHEN** follow mode is on and the writer appends a line for an operation already listed
- **THEN** that operation's end, duration and line count grow and its bar extends, without a full recomputation.

#### Scenario: Too many ids
- **WHEN** a log has 250,000 distinct ids
- **THEN** the tab lists 100,000 operations and says that 150,000 more were not tracked.
