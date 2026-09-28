## ADDED Requirements

### Requirement: Collapse Repeated Lines
The text view of a stream SHALL offer a collapse mode among Off (default), Exact and Numbers, chosen from the stream toolbar and cycled with `CTRL + SHIFT + D` while the stream has the keyboard and no text field does. An entry SHALL be a visible line that is not a stack-trace continuation line together with the visible continuation lines that follow it; a visible continuation line without a visible parent SHALL be an entry of its own. Each line SHALL be normalised by removing its leading timestamp, when the stream's timestamp detection finds one at the start of the line, and its trailing whitespace; in Numbers mode each maximal UUID (8-4-4-4-12 hexadecimal digits), `0x`-prefixed hexadecimal number, hexadecimal word of 8 or more characters containing a digit, and run of decimal digits SHALL further be replaced by one placeholder. Two consecutive entries SHALL be equal when they have the same number of lines and their normalised lines are byte-for-byte equal in order. Each run of 2 or more consecutive equal entries SHALL be shown as a group: the first entry's lines, with a `×N` badge on its first row, where N is the number of entries in the run (thousands grouped, `×1.2M` style above 999,999). Entries longer than 256 lines or 64 KiB of normalised text SHALL never be grouped. Runs SHALL be formed over the lines left visible by every active filter and SHALL be recomputed when the filters change. Clicking the badge SHALL expand the group into all its lines and clicking it again SHALL collapse it; the badge tooltip SHALL give the first and last line numbers of the group and, when known, its first and last timestamps. The mode SHALL NOT affect the HEX and rendered Markdown views, and with mode Off the view SHALL behave exactly as without this feature. The mode SHALL be saved with the stream in the workspace and in session files as `collapse=exact` or `collapse=numbers`, written only when not Off; expanded groups SHALL NOT be saved.

#### Scenario: Retry loop in Exact mode
- **WHEN** mode is Exact and lines 10 to 509 read `12:00:0x.xxx WARN connection refused, retrying` with 500 different timestamps, and line 510 is `INFO connected`
- **THEN** the view shows one row for line 10 with the badge `×500`, followed by the row for line 510.

#### Scenario: Numbers mode masks ids
- **WHEN** mode is Numbers and three consecutive lines read `user 41 fetched order 0x1f3a`, `user 42 fetched order 0x1f3b` and `user 43 fetched order 0x2000`
- **THEN** they are shown as one row with the badge `×3`, while in Exact mode they are shown as three rows.

#### Scenario: Identical stack traces collapse as whole entries
- **WHEN** mode is Exact and the same ERROR line followed by the same 12 `at …` continuation lines is logged 40 times in a row
- **THEN** the view shows the first ERROR line with the badge `×40` followed by its 12 continuation lines, and no other row of that run.

#### Scenario: Runs follow the filter
- **WHEN** mode is Exact, lines `A`, `B`, `A` are consecutive and an exclude filter hides `B`
- **THEN** the two `A` lines are shown as one row with `×2`, and removing the filter shows three rows again.

#### Scenario: Expanding a group
- **WHEN** the user clicks the `×500` badge
- **THEN** all 500 lines are shown as rows, and clicking the badge again shows one row.

#### Scenario: Mode restored with the workspace
- **WHEN** a stream is set to Numbers and FastTail is restarted
- **THEN** the stream reopens in Numbers mode with its groups collapsed, and the other streams keep their own mode.

### Requirement: Collapse on Large and Growing Files
On a stream larger than 16 MB, run detection SHALL run on a worker thread after the filter pass, with its progress in the stream bar, and the view SHALL stay usable meanwhile, collapsing the rows already scanned while keeping the line at the top of the view in place. The memory kept per stream for collapsing SHALL be at most 24 bytes per group plus the expanded set, and no memory per visible line SHALL be kept once detection is complete. While the file grows, appended lines SHALL be compared with the last entry, so a repetition at the end increases the last group's count instead of adding a row, and in follow mode the view SHALL stay on the last row. A truncation, rotation, rewrite or re-decode SHALL discard the groups and the expanded set and detect the runs again.

#### Scenario: Large log stays responsive
- **WHEN** mode Numbers is chosen on a 6 GB stream
- **THEN** the interface stays responsive, the stream bar shows the collapse progress, and the rows become collapsed as the scan advances.

#### Scenario: Count grows while following
- **WHEN** follow mode is on, the last row shows `heartbeat ok` with `×12` and the writer appends 3 more `heartbeat ok` lines
- **THEN** the last row shows `×15`, the number of rows does not change and the view stays at the bottom.

#### Scenario: Rotation resets the groups
- **WHEN** a group is expanded and the file is truncated and rewritten
- **THEN** the groups are detected again on the new content and no group is expanded.

### Requirement: Time Delta Across Collapsed Groups
When the time delta column is shown and a group is collapsed, the delta of the row that follows the group SHALL be computed from the effective timestamp of the group's last line, and the delta of the group's head row SHALL be computed from the previous row as for any row.

#### Scenario: Delta after a collapsed run
- **WHEN** a group of 100 lines stamped 14:00:00.000 to 14:00:09.900 is collapsed and the next line is stamped 14:00:10.400
- **THEN** the row after the group shows `+0.500`.
