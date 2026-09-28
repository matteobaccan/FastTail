## MODIFIED Requirements

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

### Requirement: Compressed Release Assets
Each build job SHALL package its binaries before upload: Linux and macOS as `fasttail-<os>-<arch>-<version>.tar.gz` containing the `fasttail` executable, which includes both interfaces; Windows as `fasttail-windows-<arch>-<version>.zip` containing `fasttail.exe` and `fasttail-tui.exe`; each with `LICENSE` and `README.md` next to the executables; plus `fasttail-windows-<arch>-symbols-<version>.zip` containing `fasttail.pdb` and `fasttail-tui.pdb`, so the download most people need does not carry the debug symbols. The Linux x86_64 and ARM64 jobs SHALL also publish a terminal-only archive `fasttail-tui-linux-x86_64-<version>.tar.gz` and `fasttail-tui-linux-arm64-<version>.tar.gz`, containing the `fasttail-tui` executable built with `cargo build --release --no-default-features --features tui --bin fasttail-tui`, with `LICENSE` and `README.md`; the job SHALL fail when `cargo tree` for that build lists `eframe`, `egui`, `wgpu` or `winit`. The Windows job SHALL build with `cargo build --release --bins`, the other jobs with the default features. `<version>` is the `Cargo.toml` package version; on a tag run a tag that does not match it (`v` + version) SHALL fail the build job. The release job SHALL publish these archives, never bare binaries. No package registry (crates.io, Scoop, winget, Chocolatey) is published to; `docs/distribution-channels.md` records what each would take.

#### Scenario: Linux asset size
- **WHEN** the Linux x86_64 build job stages its artifacts
- **THEN** `fasttail-linux-x86_64-<version>.tar.gz` is a fraction of the uncompressed ELF (about 20 MB instead of about 72 MB) and extracts to an executable `fasttail` whose `--help` lists `--tui`.

#### Scenario: Terminal-only archive on a server
- **WHEN** a user extracts `fasttail-tui-linux-arm64-0.20.0.tar.gz` on a headless ARM64 server without any graphics library and runs `./fasttail-tui /var/log/syslog`
- **THEN** the terminal interface opens the file, and `ldd fasttail-tui` lists no X11, Wayland, OpenGL or Vulkan library.

#### Scenario: Windows archive without symbols
- **WHEN** a user extracts `fasttail-windows-x86_64-0.20.0.zip`
- **THEN** it contains `fasttail.exe`, `fasttail-tui.exe`, `LICENSE` and `README.md`, and no debug symbols.

#### Scenario: Symbols for a crash dump
- **WHEN** a user extracts `fasttail-windows-x86_64-symbols-0.20.0.zip` into the directory of `fasttail.exe`
- **THEN** `fasttail.pdb` and `fasttail-tui.pdb` sit next to their executables, so a crash backtrace of either resolves function names.

#### Scenario: Tag and version disagree
- **WHEN** the tag `v0.10.2` is pushed while `Cargo.toml` still says `0.10.1`
- **THEN** every build job fails before uploading, and no release is created.

### Requirement: Release Published Non-Draft with All Assets
The `release` job SHALL depend on both `test` and `build`, SHALL upload every archive produced by the matrix, the Linux terminal-only archives included, and SHALL leave the GitHub release published (not draft). The release body SHALL be the `CHANGELOG.md` section whose heading matches the tag version (`v0.3.0` -> `## [0.3.0]`), followed by the GitHub-generated notes (merged pull requests, new contributors, compare link); a tag without a matching section SHALL still publish with the generated notes only.

#### Scenario: Successful tag pipeline
- **WHEN** all `test` and `build` jobs of a `v*` tag succeed
- **THEN** a published release for that tag exists with one archive per matrix target, the Windows symbols archive, and the two Linux terminal-only archives.

#### Scenario: Test failure blocks release
- **WHEN** the `test` job fails on a tag run
- **THEN** the `release` job does not run and no release is created.

## ADDED Requirements

### Requirement: Release Size Budget for the Terminal Interface
Each build job SHALL print the size in bytes of every executable and archive it stages. Against the previous release, the 0.20.0 terminal interface SHALL cost at most: 8 MB for `fasttail-tui.exe` uncompressed; 4 MB more for the Windows zip; 1 MB more for `fasttail.exe`; 5 MB more for the uncompressed Linux and macOS `fasttail` executable and 2 MB more for its `.tar.gz`; and, for each Linux terminal-only archive, 25 MB for the uncompressed `fasttail-tui` (debug info kept) and 8 MB for the `.tar.gz`. A release over budget SHALL NOT be published until the maintainer accepts the new sizes in the change's design or the budget is restored.

#### Scenario: Sizes in the build log
- **WHEN** the Windows build job of `v0.20.0` stages its archives
- **THEN** its log lists the byte sizes of `fasttail.exe`, `fasttail-tui.exe`, the zip and the symbols zip.

#### Scenario: Terminal-only archive over budget
- **WHEN** a dependency update makes `fasttail-tui-linux-x86_64-<version>.tar.gz` 9 MB
- **THEN** the release is held, and the design records either the accepted size or the fix.
