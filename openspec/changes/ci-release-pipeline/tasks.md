## 1. Crash Report URL

- [x] 1.1 Add an `ISSUES_URL` constant (`https://github.com/matteobaccan/FastTail/issues`) in `src/crash_handler.rs` and use it in `build_crash_report` and in the dialog text
- [x] 1.2 Update the crash-handler unit tests to assert the report contains `ISSUES_URL` and not `baccan/fasttail`

## 2. Workflow Split

- [x] 2.1 Add a `test` job to `.github/workflows/build.yml` (matrix ubuntu-latest + windows-latest) running `cargo fmt --check` on Linux and `cargo test` on both, triggered by push to `main`, pull_request, `v*` tags and workflow_dispatch
- [x] 2.2 Restrict the `build` matrix to `v*` tags and workflow_dispatch (`if:` on `github.ref` / `github.event_name`) and remove its test steps
- [x] 2.3 Add `save-if: ${{ github.ref == 'refs/heads/main' }}` to every `Swatinem/rust-cache` step
- [x] 2.4 Make the `release` job depend on `[test, build]`

## 3. Asset Packaging

- [x] 3.1 Replace the "Stage Artifact" step: Linux/macOS produce `fasttail-<os>-<arch>.tar.gz` with the executable, Windows produces `fasttail-windows-<arch>.zip` with `fasttail.exe` and `fasttail.pdb`
- [x] 3.2 Point `upload-artifact` and the release `files:` glob at the archives and verify that `find`-based flattening still yields one archive per target
- [x] 3.3 Run the workflow via `workflow_dispatch` on `main`, download the five archives and check that each extracts to a runnable binary (and that the Windows zip contains the PDB)
- [x] 3.4 Update the "Prebuilt binaries" section of `README.md` with the archive names and extraction commands

## 4. LTO Benchmark

- [x] 4.1 Build two release binaries locally, `lto = true` and `lto = "thin"` (both `codegen-units = 1`), from the same commit
- [x] 4.2 Run the same include filter, exclude filter and search workload on a log of at least 1 GB with each binary and record wall time and peak memory
- [x] 4.3 Apply the decision: set `lto = "thin"` in `Cargo.toml` if within 5% of fat LTO, otherwise keep `lto = true`; record the numbers in `design.md`

## 5. Release Recovery

- [ ] 5.1 Delete the draft v0.1.0 release and its tag, or publish it as-is, per maintainer decision
- [ ] 5.2 Tag the release commit on the new `main` and verify the tag pipeline ends with a published release containing five archives in under ~6 minutes
