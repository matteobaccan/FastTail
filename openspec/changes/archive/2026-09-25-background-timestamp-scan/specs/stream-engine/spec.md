## MODIFIED Requirements

### Requirement: Background Scans with Progress and Cancellation
Include/exclude filtering and search SHALL run synchronously for files up to 16 MB and on a worker thread above, delivering results in order as they are found. The per-line timestamp scan (see the log-intelligence capability) SHALL follow the same rule, measured on the bytes not timed yet: synchronously when they are at most 16 MB, on a worker thread above. The stream bar SHALL show the scan kind, the progress percentage and the count so far. A newer filter, search or reload request SHALL cancel the running scan, and results of a cancelled scan SHALL never reach the view; the exceptions are a timestamp scan that a time window is waiting on, during which filter and search requests SHALL be deferred until it completes, and a level scan, which a timestamp request SHALL preempt. A cancelled timestamp or level scan SHALL resume from the first line it had not reached, not from the start of the file. Lines appended during a scan SHALL be evaluated by the incremental paths once the scan completes. A time window SHALL be applied to the lines returned by a background filter or search scan, so a window never forces a filter or search onto the interface thread.

#### Scenario: Typing a filter on a large file
- **WHEN** the user types an include filter on a 400 MB stream
- **THEN** the keystroke is not blocked, matching rows appear progressively with `filtering 37%` in the stream bar, and typing another character cancels the previous scan.

#### Scenario: Same result as the synchronous path
- **WHEN** the same filter is applied to a file below and above the threshold
- **THEN** the set of visible lines is identical.

#### Scenario: First time range on a multi-gigabyte log
- **WHEN** the user types a "from" time on a 5 GB stream that has not been timed yet
- **THEN** the interface stays responsive, the stream bar shows `timing lines 12%` rising to 100%, and the window is applied when the scan completes.

#### Scenario: Timestamp scan resumed after a search
- **WHEN** a timestamp scan started by a go-to-time request is 40% through a large stream and the user types a search query
- **THEN** the search runs first, and the timestamp scan then continues from the line it had reached rather than from line 1.

#### Scenario: Filter with a time window on a large file
- **WHEN** a time window is set on a fully timed 2 GB stream and the user types an include filter
- **THEN** the filter runs on the worker thread with progress in the stream bar, and only lines inside the window appear.
