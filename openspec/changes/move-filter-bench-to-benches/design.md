## Context

The benchmark was added during `ci-release-pipeline` as `examples/filter_bench.rs`. It opens a large log through `TailEngine`, times indexing, include/exclude filtering, regex filtering, search with F3 hops and highlight-rule scanning, and prints the best of N rounds. The 1.1 GB input was produced by a Python script kept outside the repository.

Cargo target semantics that drive the decision:

| Target | Built by `cargo test` | Default profile | Runs on |
|---|---|---|---|
| `examples/` | yes (build only) | dev unless `--release` | manual |
| `tests/` with `#[ignore]` | yes, and linked into the test binary | dev unless `--release` | `cargo test -- --ignored` |
| `benches/` with `harness = false` | no | `bench` (inherits `release`: thin LTO, `codegen-units = 1`, `debug = 1`) | `cargo bench` |

## Goals / Non-Goals

**Goals:**
- Zero build cost for the benchmark on `cargo test`, hence on CI.
- The benchmark always measures the same optimisation settings as the shipped binary.
- Reproducible input: anyone can run it without external files.
- One command to run it.

**Non-Goals:**
- Turning the benchmark into a pass/fail test with time thresholds. CI runners are too noisy; the output stays a report read by a human.
- Using a benchmark framework such as Criterion. The phases take seconds each and one run per phase is enough; a dependency would add compile time for no gain.
- Tracking results over time in the repository.

## Decisions

**D1. `benches/filter_bench.rs` with `harness = false`.**
Declared in `Cargo.toml` as `[[bench]] name = "filter_bench" harness = false`. With a custom harness the file has its own `main`, so the existing code moves almost unchanged. `cargo test` does not build bench targets, and `cargo bench` uses the `bench` profile, which inherits every setting of `[profile.release]`. Alternative `tests/` + `#[ignore]` rejected: still compiled on every `cargo test`, and unoptimised unless the caller remembers `--release`.

**D2. Input selection by environment variables.**
`FASTTAIL_BENCH_LOG=<path>` uses an existing file unchanged (for real-world logs). Otherwise the harness writes a synthetic log into a `tempfile::tempdir()` and deletes it on exit. `FASTTAIL_BENCH_BYTES` sets the generated size, default 200 MB, so a first run finishes in well under a minute; the LTO comparison used 1.1 GB and can be reproduced with `FASTTAIL_BENCH_BYTES=1100000000`. `FASTTAIL_BENCH_ROUNDS` sets the rounds per phase, default 3. Environment variables rather than CLI flags because `cargo bench` forwards arguments awkwardly (`cargo bench -- args` also reaches libtest-style harnesses) and env vars work identically on every shell.

**D3. Generator ported to Rust inside the bench file.**
Same shape as the Python script: timestamp, level drawn from a weighted list (70% INFO, 15% DEBUG, 10% WARN, 5% ERROR), one of six service names, a request counter and one of seven message templates. A small xorshift PRNG with a fixed seed makes the file deterministic across runs and platforms; no `rand` dependency. Lines are ~90 bytes, so 200 MB is about 2.2 million lines and 1.1 GB about 12.5 million.

**D4. Output format unchanged.**
One line per phase: name, best milliseconds, and the line count that phase produced (a sanity check that both binaries under comparison did the same work). The harness prints the input path, its size and line count first, so two runs can be matched.

**D5. Remove `examples/` entirely.**
The directory has no other content. Keeping an empty or duplicate example would reintroduce the `cargo test` build cost.

## Risks / Trade-offs

- [`cargo bench` on stable requires `harness = false` for custom mains] → that is exactly the configuration used; no nightly features.
- [The 200 MB default is small enough that some phases finish in a few hundred milliseconds] → fine for a smoke run; the design documents the 1.1 GB setting for decisions such as LTO.
- [Generated log differs from the old Python one byte for byte] → irrelevant; comparisons are always made between two binaries on the same generated input, never against historical numbers.
- [Temp directory needs ~1 GB free for the large setting] → the harness prints the path and size before generating; `FASTTAIL_BENCH_LOG` avoids generation entirely.

## Migration Plan

1. Add the bench target and file, delete the example, run `cargo test` (must not compile the bench) and `cargo bench` (must build in the `bench` profile and print the report).
2. No release or CI change; the workflow does not reference the example.
3. Rollback: revert the commit.

## Open Questions

- None.
