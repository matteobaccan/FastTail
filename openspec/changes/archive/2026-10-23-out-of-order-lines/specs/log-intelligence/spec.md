## ADDED Requirements

### Requirement: Out-of-Order Timestamps
A line whose timestamp is earlier than the timestamp of the previous timed line by more than a tolerance (1 second by default, `out_of_order_tolerance_ms` in the settings, 0 turning it off) SHALL be marked out of order: a mark in the gutter with the size of the jump in its tooltip, in both interfaces. Lines without a timestamp SHALL be skipped in the comparison. The stream bar SHALL show the number of out-of-order lines when there is any, and the user SHALL be able to walk to the next and previous one. The timeline histogram SHALL mark the buckets that received out-of-order lines, and the time range popup SHALL say how many of the lines it counts are out of order. Filters, search and the histogram SHALL keep using each line's own timestamp. Detection SHALL run in the background timing pass and on appended lines, keeping only the out-of-order lines. When more than half of the timed lines go backwards, the marks SHALL be replaced by one notice that the timestamps go backwards.

#### Scenario: A clock jump back
- **WHEN** line 5000 is stamped 14:05:00 and line 5001 is stamped 13:59:48
- **THEN** line 5001 shows the out-of-order mark with a jump of `-00:05:12`, and the stream bar shows `1 out of order`.

#### Scenario: Small skew is ignored
- **WHEN** two writers interleave lines whose timestamps differ by at most 300 ms out of order
- **THEN** no line is marked with the default tolerance.

#### Scenario: Stack traces in between
- **WHEN** an untimed stack trace sits between two timed lines in order
- **THEN** no line is marked.
