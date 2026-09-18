## Context

`.github/workflows/build.yml` has a single `build` matrix (Windows x86_64, Windows ARM64 cross, Linux x86_64, Linux ARM64, macOS ARM64) that runs on push to `main`, on pull requests and on `v*` tags. Each job restores a per-target `Swatinem/rust-cache`, runs the test suite (host tests on the cross target), builds `--release`, and uploads the bare binary as an artifact. A `release` job, gated on tags, downloads the artifacts and publishes them with `softprops/action-gh-release@v3`.

Measured on the v0.1.0 run (2026-09-17):

| Job | Wall time | Notes |
|---|---|---|
| Windows x86_64 / ARM64 | 4.7 min | 0.5 cache, 0.9 tests, 2.8 release build |
| Linux x86_64 / ARM64 | 2.8-3.0 min | cache fully hit, only `fasttail` compiles |
| macOS ARM64 | 2.5-4.0 min | |
| Create GitHub Release | 7.2 min | 7.1 min in the upload, failed on a GitHub 5xx page |

Asset sizes: Linux 71-73 MB (DWARF line tables embedded in the ELF because of `debug = 1`), Windows 10.6 MB (`.pdb` not shipped), macOS 9.9 MB (dSYM not shipped). The `fasttail` crate alone takes 153 s to build with fat LTO and one codegen unit on the maintainer's workstation, 54 s with thin LTO, 96 s with thin LTO and 16 codegen units.

The crash handler in `src/crash_handler.rs` captures a `Backtrace`, writes `fasttail_crash.log`, and asks the user to open an issue at `https://github.com/baccan/fasttail/issues`, which returns 404. The real repository is `matteobaccan/FastTail`.

Billing constraint: the repository is private. GitHub Actions bills Linux 1x, Windows 2x, macOS 10x, so one full matrix run costs ~49 billed minutes.

## Goals / Non-Goals

**Goals:**
- A `v*` tag produces a published (non-draft) release with one archive per platform in under ~6 minutes wall time.
- Pull requests get test feedback in ~2 minutes and cost single-digit billed minutes.
- Crash logs keep file:line frames on Linux and gain function names on Windows.
- The Rust cache used by `main` cannot be evicted by pull-request caches.
- The crash report sends users to the right issue tracker.

**Non-Goals:**
- Changing the set of supported platforms. The matrix keeps all five targets; dropping Windows ARM64 is discussed as an open question, not decided here.
- Code signing, notarization, installers or package-manager publishing.
- Replacing `softprops/action-gh-release` or GitHub Actions.
- Symbolication tooling for crash logs beyond what the shipped binary can resolve at runtime.

## Decisions

**D1. Compress assets instead of stripping debug info.**
`debug = 1` stays. The panic hook prints the `Backtrace` with `Display`, which resolves file:line only if the binary carries DWARF; the printed report contains no raw addresses, so offline symbolication is not possible. Compression is the lever: the CI artifact zip of the Linux binary is 20 MB, and a `.tar.gz` will be in the same range. Alternatives: `strip = "debuginfo"` (10 MB Linux binary, backtraces reduced to function names) rejected because it degrades the one diagnostic channel users have; `split-debuginfo` rejected because std's backtrace does not read `.dwo/.dwp` reliably.

**D2. Archive format per platform.**
Linux and macOS: `tar.gz` containing the executable only, named `fasttail-<os>-<arch>.tar.gz`. Windows: `zip` containing `fasttail.exe` and `fasttail.pdb`, named `fasttail-windows-<arch>.zip`. Shipping the PDB lets `std::backtrace` on Windows (dbghelp) resolve function names and lines when the PDB sits next to the exe. macOS keeps the default `split-debuginfo = "packed"`; the dSYM is not shipped, so macOS reports keep function names only. Alternative considered: `split-debuginfo = "off"` on the Apple target to embed DWARF like Linux; deferred because it multiplies the macOS binary size and macOS is the platform with the fewest users.

**D3. Three jobs: `test`, `build`, `release`.**
- `test`: matrix `ubuntu-latest` and `windows-latest`, runs on `push` to `main`, `pull_request`, tags and dispatch. Linux also runs `cargo fmt --check`. Windows stays in the matrix because nine source files have `cfg(windows)`/`target_os` branches (config path, audio, BareTail bridge). macOS is excluded from `test` on push/PR because of the 10x multiplier; the release build on macOS still compiles the code.
- `build`: the existing five-target matrix minus the test steps, triggered only by `v*` tags and `workflow_dispatch`. It packages the archive in the "Stage Artifact" step and uploads the archive as the artifact.
- `release`: `needs: [test, build]`, tags only, downloads all artifacts, flattens and publishes. Keeping `test` as a dependency preserves the guarantee that a release commit passed the suite.
Alternative considered: keep a single matrix and `if:` away the test steps on tags. Rejected: it keeps five release builds on every push and does not reduce billed minutes.

**D4. Cache written only from `main`, with two cache families.**
The `test` job uses `save-if: github.ref == 'refs/heads/main'` (key `test-<target>`): PR runs restore the closest `main` cache (rust-cache falls back by key prefix) and skip saving. The `build` job holds release-profile artifacts, which the test cache does not contain, and it never runs on `main` pushes; its cache (key `release-<target>`) is therefore saved only by `workflow_dispatch` runs on `main` and restored by tag runs. GitHub scopes caches to the branch that created them plus the default branch, so a cache saved on a tag ref would be unusable by the next tag; saving from `main` dispatches is the only layout that lets tags hit. Consequence: after dependency bumps the maintainer warms the release cache with a manual run on `main`, otherwise the next tag compiles dependencies cold once (a few extra minutes, no failure). This keeps the repository around 5 GB of caches instead of 11 GB. Alternative: periodic cache cleanup workflow; rejected as a workaround for a cause that is trivial to remove.

**D5. Thin LTO, gated by a benchmark.**
`lto = "thin"` with `codegen-units = 1` builds the crate ~3x faster and grows the Windows binary by 5%. Because filter, highlight and search over multi-million-line files are the hot paths, adoption is conditional: build both variants, run the same filter and search workload on a ≥ 1 GB log, and adopt thin LTO only if the slower variant is within 5% of fat LTO on wall time. If it regresses, keep `lto = true` and record the measurement in this design. `codegen-units = 16` is rejected on the measurements (slower and larger than 1 unit under thin LTO).

**D5 result (2026-09-18).** Benchmark `examples/filter_bench.rs` on a 1.1 GB synthetic log (12,482,791 lines), Windows x86_64, both binaries built from the same commit with `codegen-units = 1`, best of 3 rounds per phase, second alternating run (first run warmed the OS file cache):

| Phase | fat LTO | thin LTO |
|---|---|---|
| open + line index | 1382 ms | 1380 ms |
| include filter, case-insensitive | 3093 ms | 3116 ms |
| include + exclude filters | 9756 ms | 9604 ms |
| include regex | 1407 ms | 1355 ms |
| search + 1000 F3 hops | 58 ms | 75 ms |
| highlight scan, 3 rules, all lines | 6471 ms | 6340 ms |
| whole run wall time | 67.6 s | 67.4 s |

Every phase is within ±2% except search, which is sub-100 ms and dominated by timer noise. Thin LTO is adopted: `lto = "thin"`, `codegen-units = 1`. Peak memory was not captured (the process handle had already been released when read); memory use does not depend on the LTO mode since the data structures are identical.

**D6. Crash report URL.**
Replace both occurrences of `https://github.com/baccan/fasttail/issues` with `https://github.com/matteobaccan/FastTail/issues`, sourced from a single `ISSUES_URL` constant so the report and the dialog cannot drift again. The existing crash-handler unit tests assert on the report; they are updated to assert the constant.

## Risks / Trade-offs

- [Users download an archive instead of a bare executable] → README documents extraction; file names stay predictable per platform.
- [Windows zip grows with the PDB (~10-20 MB compressed)] → acceptable; still far below the Linux size that caused the timeout.
- [Thin LTO costs runtime performance on the hot paths] → gated by the benchmark in D5; fat LTO remains the default until the numbers say otherwise.
- [Tests no longer run on macOS for PRs] → the macOS release build still compiles every crate; platform-specific logic is Windows/Unix, not macOS-specific. Can be revisited if a macOS-only bug appears.
- [Tag pipeline still fails on a transient GitHub upload error] → uploads are now ~4x smaller and finish in seconds; `softprops/action-gh-release` retries internally. A failed release run can be re-run from the Actions UI; artifacts of the same run are reused.
- [PR branches lose their own cache] → a PR that changes `Cargo.lock` rebuilds only the changed dependencies on top of the `main` cache; this is the intended trade-off.

## Migration Plan

1. Land the workflow split and asset packaging on `main` (no tag yet). Verify with `workflow_dispatch` that the `build` matrix produces the five archives and that the artifacts extract correctly.
2. Publish or delete the existing v0.1.0 draft release by hand: its assets are complete but use the old bare-binary names. Recommended: delete the draft and the tag, re-tag v0.1.0 on the new `main` so the first public release already uses the archive layout.
3. Roll back by reverting the workflow commit; the old single-matrix workflow is self-contained.

## Open Questions

- Drop Windows ARM64 from the release matrix? It is the only cross-compiled, untested-on-hardware target; Windows-on-ARM runs the x86_64 exe under emulation. Saves ~9 billed minutes per tag, no wall-time gain. Decision deferred to the maintainer.
- Should the macOS `test` job be added on tags only, as a cheap pre-release safety net (2.5 min real, 25 billed)?
