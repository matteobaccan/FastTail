# Release Pipeline Specification

## Purpose
Defines how FastTail is tested, built, cached and published on GitHub Actions: tests on every push and pull request, release binaries only for version tags and manual runs, compressed per-platform archives, and a release profile whose optimisation settings are backed by measurement.
## Requirements
### Requirement: Test Job Independent of Release Builds
The CI workflow SHALL run a `test` job on every push to `main`, every pull request, every `v*` tag and every manual dispatch. The job SHALL run `cargo test` (default features, so both the graphical and the terminal interface are built and tested) on Linux x86_64 and Windows x86_64, `cargo check --no-default-features --features tui --bin fasttail-tui` on Linux x86_64 and Windows x86_64 to prove the terminal build does not need the GUI toolkit, and `cargo fmt --check` on Linux x86_64. The `test` job SHALL NOT build release binaries.

#### Scenario: Pull request feedback
- **WHEN** a pull request targeting `main` is opened or updated
- **THEN** only the `test` job runs, on Linux and Windows, and no release binary is built or uploaded.

#### Scenario: Formatting regression
- **WHEN** a pull request contains code that `cargo fmt --check` rejects
- **THEN** the Linux `test` job fails and the pull request is reported as failing.

#### Scenario: Engine code starts using egui
- **WHEN** a pull request adds a use of `egui::Color32` to `src/tail_engine.rs`
- **THEN** the GUI-free `cargo check` step fails on Linux and Windows and the pull request is reported as failing.

### Requirement: Release Builds Only on Tags and Manual Dispatch
The release `build` matrix (Windows x86_64, Linux x86_64, Linux ARM64, macOS ARM64) SHALL run only when a `v*` tag is pushed or the workflow is dispatched manually. Each build job SHALL compile with `cargo build --release` and SHALL NOT run the test suite. Windows ARM64 is not built (Windows on ARM runs the x86_64 executable under emulation) and macOS runs no test job (the platform has too few users to justify its 10x billing).

#### Scenario: Push to main
- **WHEN** a commit is pushed to `main` without a tag
- **THEN** the release `build` matrix does not run.

#### Scenario: Tag pushed
- **WHEN** the tag `v0.2.0` is pushed
- **THEN** the `test` job and the four `build` jobs start in parallel, and the `release` job starts only after all of them succeed.

### Requirement: Compressed Release Assets
Each build job SHALL package its binaries before upload: Linux and macOS as `fasttail-<os>-<arch>-<version>.tar.gz`, Windows as `fasttail-windows-<arch>-<version>.zip`, each containing the window executable (`fasttail`, `fasttail.exe`) and the terminal executable (`fasttail-tui`, `fasttail-tui.exe`) with `LICENSE` and `README.md` next to them, plus `fasttail-windows-<arch>-symbols-<version>.zip` containing the PDB of each executable, so the download most people need does not carry the debug symbols. The macOS job SHALL produce both `fasttail-macos-arm64-<version>.tar.gz` and `fasttail-macos-x86_64-<version>.tar.gz`. `<version>` is the `Cargo.toml` package version; on a tag run a tag that does not match it (`v` + version) SHALL fail the build job. The release job SHALL publish these archives and the installable packages, never bare binaries. No package registry (crates.io, Scoop, winget, Chocolatey) is published to; `docs/distribution-channels.md` records what each would take.

#### Scenario: Linux asset size
- **WHEN** the Linux x86_64 build job stages its artifact
- **THEN** the uploaded `.tar.gz` is a fraction of the size of the uncompressed executables and extracts to executable `fasttail` and `fasttail-tui` files.

#### Scenario: Windows archive without symbols
- **WHEN** a user extracts `fasttail-windows-x86_64-0.15.2.zip`
- **THEN** it contains `fasttail.exe`, `fasttail-tui.exe`, `LICENSE` and `README.md`, and no debug symbols.

#### Scenario: Symbols for a crash dump
- **WHEN** a user extracts `fasttail-windows-x86_64-symbols-0.15.2.zip` into the directory of the executables
- **THEN** `fasttail.pdb` and `fasttail_tui.pdb` sit next to them, so a crash backtrace of either resolves function names.

#### Scenario: Intel Mac archive
- **WHEN** a user on an Intel Mac extracts `fasttail-macos-x86_64-0.15.2.tar.gz`
- **THEN** `fasttail` and `fasttail-tui` are x86_64 executables that start.

#### Scenario: Tag and version disagree
- **WHEN** the tag `v0.15.3` is pushed while `Cargo.toml` still says `0.15.2`
- **THEN** every build job fails before uploading, and no release is created.

### Requirement: Release Published Non-Draft with All Assets
The `release` job SHALL depend on both `test` and `build`, SHALL upload every archive and package produced by the matrix, and SHALL leave the GitHub release published (not draft). The release body SHALL start with a downloads grid, one row per architecture (x86-64, ARM64) and one column per system (Windows, Linux, macOS), each cell linking every published asset for that architecture and system by its kind (MSI, ZIP, DEB, RPM, AppImage, tar.gz, DMG) and "—" where there is none, followed by a note that every download holds both executables, the link to the Windows symbols and how to pass the unsigned-build prompts; the grid SHALL be generated from the assets actually published (`.github/scripts/downloads_table.py`). Then comes the `CHANGELOG.md` section whose heading matches the tag version (`v0.3.0` -> `## [0.3.0]`), followed by the GitHub-generated notes (merged pull requests, new contributors, compare link); a tag without a matching section SHALL still publish with the grid and the generated notes only.

#### Scenario: Successful tag pipeline
- **WHEN** all `test` and `build` jobs of a `v*` tag succeed
- **THEN** a published release for that tag exists with 14 assets: the five archives, the macOS x86_64 archive, the Windows symbols archive, two `.deb`, two `.rpm`, the `.msi`, the `.dmg` and two AppImages.

#### Scenario: Downloads grid follows the assets
- **WHEN** the 0.15.0 release publishes the Windows ZIP, the Linux x86_64 and ARM64 archives and the macOS ARM64 archive
- **THEN** the body starts with a grid whose x86-64 row links the ZIP and the Linux archive and shows "—" for macOS, and whose ARM64 row links the Linux and macOS archives and shows "—" for Windows; once the packages exist, the same cells also link the MSI, DEB, RPM, AppImage and DMG.

#### Scenario: Test failure blocks release
- **WHEN** the `test` job fails on a tag run
- **THEN** the `release` job does not run and no release is created.

### Requirement: Rust Cache Written Only from main
The Rust build cache SHALL be restored on every job but saved only by runs on `refs/heads/main`: the `test` cache by pushes to `main`, the release-profile `build` cache by manual dispatches on `main` (the build matrix never runs on plain pushes). Pull request and tag runs SHALL restore the latest `main` cache and SHALL NOT create cache entries.

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

### Requirement: Release Size Budget for the Terminal Interface
Each build job SHALL print the size in bytes of every executable and archive it stages. Against the previous release, the 0.20.0 terminal interface SHALL cost at most: 8 MB for `fasttail-tui.exe` uncompressed; 4 MB more for the Windows zip; 1 MB more for `fasttail.exe`; 5 MB more for the uncompressed Linux and macOS `fasttail` executable and 2 MB more for its `.tar.gz`; and, for each Linux terminal-only archive, 25 MB for the uncompressed `fasttail-tui` (debug info kept) and 8 MB for the `.tar.gz`. A release over budget SHALL NOT be published until the maintainer accepts the new sizes in the change's design or the budget is restored.

#### Scenario: Sizes in the build log
- **WHEN** the Windows build job of `v0.20.0` stages its archives
- **THEN** its log lists the byte sizes of `fasttail.exe`, `fasttail-tui.exe`, the zip and the symbols zip.

#### Scenario: Terminal-only archive over budget
- **WHEN** a dependency update makes `fasttail-tui-linux-x86_64-<version>.tar.gz` 9 MB
- **THEN** the release is held, and the design records either the accepted size or the fix.

### Requirement: Installable Packages
Each release SHALL publish installable packages built from the same executables as its archives:
- Linux x86_64 and ARM64: `fasttail_<version>_<amd64|arm64>.deb` and `fasttail-<version>-1.<x86_64|aarch64>.rpm`, installing `/usr/bin/fasttail`, `/usr/bin/fasttail-tui`, a desktop entry `fasttail.desktop`, `fasttail` icons in the hicolor theme (128, 256 and 512 px and scalable) and `README.md` and `LICENSE` under `/usr/share/doc/fasttail`. The C library SHALL be a dependency; the graphics and windowing libraries the window loads at run time SHALL only be recommended, so the package installs on a server without a desktop.
- Linux x86_64 and ARM64: `fasttail-linux-<arch>-<version>.AppImage`, starting the window, with `fasttail-tui` inside.
- Windows x86_64: `fasttail-windows-x86_64-<version>.msi`, a per-user installation needing no administrator rights, in `%LOCALAPPDATA%\Programs\FastTail`, with a Start menu shortcut to the window with its icon, the installation folder added to the user's `PATH`, an entry in the installed apps with the version, and a newer MSI replacing an older installation in place. Installing or removing SHALL NOT change `fasttail.ini`.
- macOS: `fasttail-macos-universal-<version>.dmg` holding `FastTail.app` (the window, with `fasttail-tui` in `Contents/MacOS`) with executables for both arm64 and x86_64, an `Applications` link, `README.md` and `LICENSE`.
The Linux packages SHALL run on distributions with glibc 2.35 or newer. Packages are unsigned; the README SHALL say which prompt each system shows and how to proceed.

#### Scenario: Installing on Debian
- **WHEN** a user runs `sudo apt install ./fasttail_0.15.2_amd64.deb` on Debian 12
- **THEN** `fasttail` and `fasttail-tui` are on `PATH`, FastTail appears in the desktop menu with its icon, and `sudo apt remove fasttail` removes them.

#### Scenario: A server without a desktop
- **WHEN** the `.rpm` is installed with `dnf install --setopt=install_weak_deps=False` on a Fedora server
- **THEN** it installs without the graphics libraries and `fasttail-tui app.log` runs.

#### Scenario: Windows upgrade without administrator rights
- **WHEN** a user without administrator rights runs the 0.15.3 MSI over an installed 0.15.2
- **THEN** no elevation prompt appears, the installed apps list one FastTail at 0.15.3, and the settings in `fasttail.ini` are unchanged.

#### Scenario: One disk image for every Mac
- **WHEN** a user on an Intel Mac and a user on an Apple Silicon Mac each drag `FastTail.app` from the same `.dmg` to Applications
- **THEN** it starts natively on both.

### Requirement: Packages Tested Before Release
Every build job SHALL install or mount each package it produces on its runner, run `fasttail --version` and `fasttail-tui --version` from the installed location, and remove it, before uploading: the `.deb` with `apt` on the runner, the `.rpm` with `dnf` in a Fedora container, the AppImage with `--appimage-extract-and-run`, the `.msi` with `msiexec` (also checking `fasttail-tui` through the new `PATH` in a new shell), the `.dmg` with `hdiutil`, checking with `lipo` that both architectures are present. A failing check SHALL fail the job, so the release is not published.

#### Scenario: A broken installer
- **WHEN** a change to the WiX source leaves `fasttail-tui.exe` out of the MSI
- **THEN** the Windows build job fails its install test and the tag's release is not created.

#### Scenario: A missing architecture
- **WHEN** the universal step keeps only the arm64 executable
- **THEN** the `lipo -archs` check fails the macOS job before upload.
