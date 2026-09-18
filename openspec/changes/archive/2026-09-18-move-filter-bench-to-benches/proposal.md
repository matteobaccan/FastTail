## Why

`examples/filter_bench.rs` is the harness behind the thin-LTO decision, but as an example it is compiled by every `cargo test` (a cost on each push and pull request), it builds with whatever profile the caller picks (the dev profile by default, which measures unoptimised code), and its input generator lives outside the repository, so the measurement cannot be reproduced by anyone else. Moving it to `benches/` makes it build only on demand, always with the release-derived `bench` profile, and self-sufficient.

## What Changes

- Move the benchmark to `benches/filter_bench.rs` declared in `Cargo.toml` with `harness = false`, so it runs only with `cargo bench` and is not compiled by `cargo test`, `cargo build` or CI.
- Embed a deterministic log generator in the benchmark: `FASTTAIL_BENCH_LOG` points to an existing log to use as-is; otherwise a synthetic log of `FASTTAIL_BENCH_BYTES` bytes (default 200 MB) is generated in a temporary directory and removed afterwards.
- Remove the `examples/` directory.
- Document how to run the benchmark in the README and in the release-pipeline spec, which now names `cargo bench` as the measurement tool for the LTO choice.

## Capabilities

### New Capabilities
- (none)

### Modified Capabilities
- `release-pipeline`: adds a requirement for an on-demand performance benchmark harness and updates the LTO measurement requirement to reference it.

## Impact

- `Cargo.toml`: new `[[bench]]` entry with `harness = false`; no new dependencies (the generator uses a small inline PRNG, `tempfile` is already a dev-dependency).
- `benches/filter_bench.rs`: new file, content of the former example plus the generator.
- `examples/filter_bench.rs`: removed.
- `README.md`: a short "Benchmarks" note under Building & Installation.
- CI: unchanged; `cargo test` no longer compiles the benchmark.
