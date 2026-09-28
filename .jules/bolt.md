## 2026-04-18 - SIMD-accelerated ASCII case-insensitive search
**Learning:** Sliding window checking (`windows(n).any(...)`) for ASCII string matching in Rust hot loops introduces significant branch overhead. Using `memchr::memchr2` on the first character's lowercase and uppercase byte variants leverages SIMD vector instructions to skip non-candidate byte positions at 10+ GB/s.
**Action:** In Rust string/log parsing hot paths, replace linear window checks for multi-byte needles with `memchr2` candidate scanning before running substring slice comparisons.

## 2026-04-18 - Direct SIMD ASCII search without `haystack.is_ascii()` and single-byte `memchr`
**Learning:** Calling `haystack.is_ascii()` in string-matching hot loops iterates over the entire haystack upfront on every rule check. Because UTF-8 guarantees ASCII bytes (0..127) never overlap with multi-byte sequence bytes (128..255), an ASCII needle can be searched directly via SIMD `memchr`/`memchr2` on `haystack.as_bytes()`. Additionally, when the first byte's lowercase and uppercase variants match (`first_lower == first_upper`, e.g. numbers, symbols, spaces, punctuation), using single-byte `memchr::memchr` instead of `memchr2` avoids multi-byte SIMD vector overhead.
**Action:** Omit `haystack.is_ascii()` when `needle.is_ascii()` in UTF-8 text processing hot paths, and branch `first_lower == first_upper` to single-byte `memchr`.

## 2026-04-18 - Avoid large stack buffer copies and fixed cap risks in hot loops
**Learning:** Replacing dynamic small Vec instances (which hold 1–2 elements in practice) in a loop with fixed stack arrays like `[(usize, usize); 64]` (1 KB each) causes 1 KB of stack copying per iteration on every span subtraction (up to 64 KB per call). Furthermore, fixed arrays risk silently dropping interval pieces if splitting pushes the count past the fixed cap.
**Action:** Prefer standard dynamic Vec or prudent small-vec over copying large fixed stack buffers in tight interval-deduction loops.

## 2026-04-18 - Small stack buffer with dynamic Vec overflow for hot interval deduction
**Learning:** Using dynamic `Vec` instances in hot row span deduction loops (`claim_span`) causes millions of heap allocations on multi-megabyte logs. Replacing them with `smallvec::SmallVec<[(usize, usize); 8]>` (128 bytes inline, spilling to the heap past 8) eliminates heap allocation churn while guaranteeing zero truncation risk and avoiding large stack frame copying overhead.
**Action:** Use `SmallVec` (already in the dependency tree) rather than a hand-written inline buffer for interval subtraction and piece tracking in tight text/span layout loops.

## 2026-04-18 - Single-pass CSI sequence validation & single-byte SIMD search
**Learning:** Re-iterating over ANSI parameter slices after finding the sequence termination byte adds unnecessary overhead on every escape sequence. Validating SGR parameter flags in a single pass during byte scanning avoids double iteration over parameters. Furthermore, single-byte ASCII searches (`needle.len() == 1`) in case-insensitive routines can call `memchr::memchr` / `memchr::memchr2` / `memchr_iter` directly without slice length loops or `eq_ignore_ascii_case` overhead.
**Action:** Track SGR parameter validity inline during single-pass CSI parsing, and branch `needle.len() == 1` directly to SIMD `memchr` / `memchr2` routines.

## 2026-04-18 - Length-branched dispatch for static token tables
**Learning:** Iterating over static slice tables with `eq_ignore_ascii_case` in log level and token parsing hot paths adds multi-pass iteration and unnecessary case-folding loops. Branching on `word.len()` and `word[0]` combined with direct byte slice comparisons (`== b"INFO"` or `== b"info"`) bypasses case-folding iterations for standard log tokens.
**Action:** When parsing fixed string token sets, branch on word length and first byte, providing direct byte string comparison fast paths before falling back to case-insensitive matching.
