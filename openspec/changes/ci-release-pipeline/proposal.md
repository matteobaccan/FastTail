## Why

The v0.1.0 tag pipeline failed while publishing the GitHub release: the unstripped Linux binaries weigh ~72 MB each, the upload of `fasttail-linux-x86_64` hit a GitHub server-side timeout ("Unicorn!" HTML error page) after five minutes, and the release was left as an unpublished draft. At the same time every push to `main` and every pull request rebuilds five release binaries with tests folded into the same jobs, costing ~49 billed minutes per run on a private repository (macOS counts 10x, Windows 2x) and giving PR feedback only after ~4.7 minutes, while the repository's Rust caches (~11 GB across `main` and PR branches) already exceed GitHub's 10 GB limit and risk evicting the caches `main` depends on.

## What Changes

- **Compressed release assets**: Linux and macOS binaries are published as `.tar.gz` archives, Windows binaries as `.zip` archives that also carry the `.pdb`, so uploads shrink from ~72 MB to ~20 MB and Windows crash reports gain symbol names.
- **Test jobs decoupled from release builds**: a dedicated `test` job (Linux and Windows) runs `cargo fmt --check` and `cargo test` on push and pull request. The release build matrix runs only on `v*` tags and manual dispatch, and the release job requires both `test` and `build`.
- **Cache saved only from `main`**: `Swatinem/rust-cache` uses `save-if` so pull requests restore the `main` cache but never write new entries, keeping the repository below the 10 GB cache limit.
- **Thin LTO, conditional**: the release profile switches from fat to thin LTO (build of the `fasttail` crate 153 s → 54 s locally) only if a filter/search benchmark on a large file shows no measurable regression; otherwise fat LTO stays.
- **Crash report URL fixed**: the crash log and the crash dialog point to `https://github.com/matteobaccan/FastTail/issues` instead of the non-existent `baccan/fasttail` repository.
- Debug info (`debug = 1`) is kept in the release profile: it is what gives file:line frames in `fasttail_crash.log`.

## Capabilities

### New Capabilities
- `release-pipeline`: how FastTail builds, tests, caches and publishes release binaries on GitHub Actions.
- `crash-reporting`: what the panic handler writes to `fasttail_crash.log` and shows in the crash dialog, including where users are sent to report the crash.

### Modified Capabilities
- (none: no existing spec covers CI or crash reporting)

## Impact

- `.github/workflows/build.yml`: split into `test`, `build` and `release` jobs with new triggers, artifact packaging step and `save-if` on the Rust cache.
- `Cargo.toml`: `[profile.release]` may switch `lto = true` to `lto = "thin"` depending on the benchmark outcome.
- `src/crash_handler.rs`: issues URL in `build_crash_report` and in the dialog text; the crash-handler tests that assert on the report content.
- Release consumers: asset file names change (`fasttail-linux-x86_64.tar.gz`, `fasttail-windows-x86_64.zip`, ...). Nothing has been published yet, so no existing download link breaks, but the README download section must describe the archives.
- `README.md`: download instructions.
