## 2026-04-18 - SIMD-accelerated ASCII case-insensitive search
**Learning:** Sliding window checking (`windows(n).any(...)`) for ASCII string matching in Rust hot loops introduces significant branch overhead. Using `memchr::memchr2` on the first character's lowercase and uppercase byte variants leverages SIMD vector instructions to skip non-candidate byte positions at 10+ GB/s.
**Action:** In Rust string/log parsing hot paths, replace linear window checks for multi-byte needles with `memchr2` candidate scanning before running substring slice comparisons.

## 2026-04-18 - Branch-table dispatch for fixed token sets
**Learning:** Linear iteration over static slices of token pairs (`TABLE.iter().find(...)`) in header-parsing hot loops invokes `eq_ignore_ascii_case` up to N times per word token, adding overhead for non-matching words (e.g. timestamps, IDs). Checking non-alphabetic ASCII start bytes (`!word[0].is_ascii_alphabetic()`) and using a `match word.len()` jump table eliminates redundant slice comparisons.
**Action:** In log parsing hot loops, filter non-alphabetic tokens early and dispatch fixed token sets via `match word.len()` jump tables.
