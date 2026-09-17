## 2026-09-17 - Optimize case-insensitive log line matching
**Learning:** `line.to_lowercase()` per line in render and tailing loops was allocating strings on every line search/filter/highlight check. Replacing with ASCII window slice matching (`eq_ignore_ascii_case`) and pre-lowercased pattern storage avoids heap allocations completely on ASCII log files.
**Action:** Use zero-allocation ASCII window searching (`eq_ignore_ascii_case`) for hot path log streaming filters and highlight matching.
