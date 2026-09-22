## 2026-04-18 - SIMD-accelerated ASCII case-insensitive search
**Learning:** Sliding window checking (`windows(n).any(...)`) for ASCII string matching in Rust hot loops introduces significant branch overhead. Using `memchr::memchr2` on the first character's lowercase and uppercase byte variants leverages SIMD vector instructions to skip non-candidate byte positions at 10+ GB/s.
**Action:** In Rust string/log parsing hot paths, replace linear window checks for multi-byte needles with `memchr2` candidate scanning before running substring slice comparisons.

## 2026-04-18 - Zero-allocation span deduction in highlighting hot paths
**Learning:** Performing interval deduction in per-row line highlighting hot paths with heap-allocated vectors (`vec![]`, `Vec::with_capacity()`) causes millions of small heap allocations during log rendering and highlight scanning. Replacing dynamic `Vec` instances with stack-allocated fixed arrays (`[(usize, usize); MAX_ROW_SPANS]`) eliminates heap allocations entirely for span deduction.
**Action:** In per-row layout or highlight hot paths bounded by a fixed maximum cap, use stack-allocated fixed arrays instead of `Vec` allocations.
