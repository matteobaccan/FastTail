## ADDED Requirements

### Requirement: On-Demand Performance Benchmark Harness
The repository SHALL provide a benchmark target `benches/filter_bench.rs` declared with `harness = false` that times the engine hot paths (line indexing, include and exclude filtering, regex filtering, search with match navigation, highlight-rule scanning) and prints the best time per phase. It SHALL run only through `cargo bench` and SHALL NOT be compiled by `cargo test` or `cargo build`. Its input SHALL be either the file named by `FASTTAIL_BENCH_LOG` or a deterministic synthetic log generated in a temporary directory, of `FASTTAIL_BENCH_BYTES` bytes (default 200 MB), removed after the run.

#### Scenario: Running the benchmark without any input
- **WHEN** the developer runs `cargo bench` with no environment variables set
- **THEN** the harness generates a 200 MB log in a temporary directory, prints its path, size and line count, prints one timing line per phase, and deletes the generated file on exit.

#### Scenario: Running on a real log
- **WHEN** `FASTTAIL_BENCH_LOG` points to an existing file
- **THEN** the harness uses that file unchanged, generates nothing, and reports the same phases.

#### Scenario: cargo test does not build the benchmark
- **WHEN** the developer runs `cargo test`
- **THEN** the bench target is not compiled and the test run time is unaffected by the benchmark's existence.

## MODIFIED Requirements

### Requirement: Link-Time Optimization Choice Backed by Measurement
The release profile SHALL use thin LTO with one codegen unit only if the `filter_bench` benchmark (`cargo bench`, `FASTTAIL_BENCH_BYTES` of at least 1 GB) shows the thin build within 5% of the fat build on filtering and search. Otherwise the profile SHALL keep fat LTO. The measurement SHALL be recorded in the change design.

#### Scenario: Thin LTO within budget
- **WHEN** the benchmark shows the thin-LTO build within 5% of fat LTO on the filter and search workload
- **THEN** `Cargo.toml` sets `lto = "thin"` and `codegen-units = 1`, and the numbers are recorded in `design.md`.

#### Scenario: Thin LTO regresses
- **WHEN** the benchmark shows the thin-LTO build more than 5% slower than fat LTO
- **THEN** `Cargo.toml` keeps `lto = true` and the numbers are recorded in `design.md`.
