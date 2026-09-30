## MODIFIED Requirements

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
The `release` job SHALL depend on both `test` and `build`, SHALL upload every archive and package produced by the matrix, and SHALL leave the GitHub release published (not draft). The release body SHALL be the `CHANGELOG.md` section whose heading matches the tag version (`v0.3.0` -> `## [0.3.0]`), followed by the GitHub-generated notes (merged pull requests, new contributors, compare link); a tag without a matching section SHALL still publish with the generated notes only.

#### Scenario: Successful tag pipeline
- **WHEN** all `test` and `build` jobs of a `v*` tag succeed
- **THEN** a published release for that tag exists with 14 assets: the five archives, the macOS x86_64 archive, the Windows symbols archive, two `.deb`, two `.rpm`, the `.msi`, the `.dmg` and two AppImages.

#### Scenario: Test failure blocks release
- **WHEN** the `test` job fails on a tag run
- **THEN** the `release` job does not run and no release is created.

## ADDED Requirements

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
