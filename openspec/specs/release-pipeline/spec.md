# Release Pipeline Specification

## Purpose
Defines how FastTail is tested, built, cached and published on GitHub Actions: tests on every push and pull request, release binaries only for version tags and manual runs, compressed per-platform archives, and a release profile whose optimisation settings are backed by measurement.

## Requirements

### Requirement: Test Job Independent of Release Builds
The CI workflow SHALL run a `test` job on every push to `main`, every pull request, every `v*` tag and every manual dispatch. The job SHALL run `cargo test` on Linux x86_64 and Windows x86_64, and `cargo fmt --check` on Linux x86_64. The `test` job SHALL NOT build release binaries.

#### Scenario: Pull request feedback
- **WHEN** a pull request targeting `main` is opened or updated
- **THEN** only the `test` job runs, on Linux and Windows, and no release binary is built or uploaded.

#### Scenario: Formatting regression
- **WHEN** a pull request contains code that `cargo fmt --check` rejects
- **THEN** the Linux `test` job fails and the pull request is reported as failing.

### Requirement: Release Builds Only on Tags and Manual Dispatch
The release `build` matrix (Windows x86_64, Windows ARM64, Linux x86_64, Linux ARM64, macOS ARM64) SHALL run only when a `v*` tag is pushed or the workflow is dispatched manually. Each build job SHALL compile with `cargo build --release` and SHALL NOT run the test suite.

#### Scenario: Push to main
- **WHEN** a commit is pushed to `main` without a tag
- **THEN** the release `build` matrix does not run.

#### Scenario: Tag pushed
- **WHEN** the tag `v0.2.0` is pushed
- **THEN** the `test` job and the five `build` jobs start in parallel, and the `release` job starts only after all of them succeed.

### Requirement: Compressed Release Assets
Each build job SHALL package its binary before upload: Linux and macOS as `fasttail-<os>-<arch>.tar.gz` containing the executable, Windows as `fasttail-windows-<arch>.zip` containing `fasttail.exe` and `fasttail.pdb`. The release job SHALL publish these archives, never bare binaries.

#### Scenario: Linux asset size
- **WHEN** the Linux x86_64 build job stages its artifact
- **THEN** the uploaded asset is a `.tar.gz` whose size is a fraction of the uncompressed ELF (about 20 MB instead of about 72 MB) and extracts to an executable `fasttail`.

#### Scenario: Windows archive carries symbols
- **WHEN** a user extracts `fasttail-windows-x86_64.zip`
- **THEN** `fasttail.exe` and `fasttail.pdb` sit in the same directory, so a crash backtrace resolves function names.

### Requirement: Release Published Non-Draft with All Assets
The `release` job SHALL depend on both `test` and `build`, SHALL upload every archive produced by the matrix, and SHALL leave the GitHub release published (not draft) with generated release notes.

#### Scenario: Successful tag pipeline
- **WHEN** all `test` and `build` jobs of a `v*` tag succeed
- **THEN** a published release for that tag exists with one archive per matrix target.

#### Scenario: Test failure blocks release
- **WHEN** the `test` job fails on a tag run
- **THEN** the `release` job does not run and no release is created.

### Requirement: Rust Cache Written Only from main
The Rust build cache SHALL be restored on every job but saved only by runs on `refs/heads/main`. Pull request and tag runs SHALL restore the latest `main` cache and SHALL NOT create cache entries.

#### Scenario: Pull request run
- **WHEN** the `test` job runs for a pull request
- **THEN** it restores the `main` cache for its target and, at the end of the job, saves nothing.

### Requirement: Debug Info Retained in Release Binaries
The release profile SHALL keep `debug = 1` so that panic backtraces in `fasttail_crash.log` include file and line for frames of the `fasttail` crate on Linux. Release binaries SHALL NOT be stripped.

#### Scenario: Linux crash log
- **WHEN** a release build panics on Linux
- **THEN** the callstack in `fasttail_crash.log` shows `fasttail::` frames with `src/<file>.rs:<line>` locations.

### Requirement: Link-Time Optimization Choice Backed by Measurement
The release profile SHALL use thin LTO with one codegen unit only if the `filter_bench` benchmark (`cargo bench`, `FASTTAIL_BENCH_BYTES` of at least 1 GB) shows the thin build within 5% of the fat build on filtering and search. Otherwise the profile SHALL keep fat LTO. The measurement SHALL be recorded in the change design.

#### Scenario: Thin LTO within budget
- **WHEN** the benchmark shows the thin-LTO build within 5% of fat LTO on the filter and search workload
- **THEN** `Cargo.toml` sets `lto = "thin"` and `codegen-units = 1`, and the numbers are recorded in `design.md`.

#### Scenario: Thin LTO regresses
- **WHEN** the benchmark shows the thin-LTO build more than 5% slower than fat LTO
- **THEN** `Cargo.toml` keeps `lto = true` and the numbers are recorded in `design.md`.

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
