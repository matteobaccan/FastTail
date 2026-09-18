## 1. Bench Target

- [x] 1.1 Add `[[bench]] name = "filter_bench" harness = false` to `Cargo.toml`
- [x] 1.2 Create `benches/filter_bench.rs` from `examples/filter_bench.rs`: same phases and output format, input chosen by `FASTTAIL_BENCH_LOG`, rounds by `FASTTAIL_BENCH_ROUNDS` (default 3)
- [x] 1.3 Add the deterministic log generator (xorshift PRNG, fixed seed, weighted levels, six services, seven message templates) writing into a `tempfile::tempdir()` with `FASTTAIL_BENCH_BYTES` bytes (default 200 MB), and print path, size and line count before the phases
- [x] 1.4 Delete `examples/filter_bench.rs` and the `examples/` directory

## 2. Verification

- [x] 2.1 Run `cargo test` and confirm the bench target is not compiled (no `filter_bench` in the build output)
- [x] 2.2 Run `cargo bench` with no variables and confirm generation, the per-phase report and cleanup of the temporary file
- [x] 2.3 Run `cargo bench` with `FASTTAIL_BENCH_LOG` set to an existing log and confirm nothing is generated
- [x] 2.4 Run `cargo fmt --check` and `cargo clippy --benches` clean

## 3. Documentation

- [x] 3.1 Add a "Benchmarks" note to the Building & Installation section of `README.md` with the `cargo bench` command and the three environment variables
