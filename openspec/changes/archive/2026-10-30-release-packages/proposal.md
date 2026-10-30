## Why

A FastTail release is five archives: a `.zip` for Windows and `.tar.gz` for Linux x86_64,
Linux ARM64 and macOS Apple Silicon. Installing means unpacking by hand, putting the
executables somewhere on `PATH`, and making menu entries and icons yourself; upgrading
means doing it again, and removing means finding the files. Apps with a similar audience
publish installable packages: RustDesk 1.4.9, for example, ships `.deb`, `.rpm`, AppImage
and Flatpak for Linux, `.msi` and `.exe` installers for Windows and `.dmg` for macOS
(Apple Silicon and Intel). Mac users on Intel get no FastTail build at all today.
The maintainer asked for the same coverage of the desktop platforms, starting with the
four packages below.

## What Changes

- **Linux `.deb` and `.rpm`** for x86_64 and ARM64: both executables in `/usr/bin`, a
  desktop entry and icons for the window, the licence and README under `/usr/share/doc`.
  Installed, upgraded and removed with `apt` / `dnf` / `zypper`.
- **Windows `.msi`** for x86_64: a per-user install (no administrator rights) in
  `%LOCALAPPDATA%\Programs\FastTail`, a Start menu shortcut for the window, both
  executables on the user's `PATH`, upgrade in place over an older FastTail, removal from
  "Installed apps".
- **macOS `.dmg`**: `FastTail.app` (the window, with its icon) and `fasttail-tui` next to
  it, a drag-to-Applications layout; a **universal** build (Apple Silicon and Intel), so
  Intel Macs are covered again. A `fasttail-macos-x86_64-<version>.tar.gz` joins the
  Apple Silicon archive.
- **Linux AppImage** for x86_64 and ARM64: one executable file that runs the window on
  most distributions without installing.
- **Every package is checked in CI before the release**: installed (or mounted) on the
  runner, `fasttail --version` and `fasttail-tui --version` run from the installed
  location, then removed.
- The existing archives stay, with the same names, for users and scripts that use them.

Target release: a **0.16.x** patch (a nightly release, AGENTS.md), before the 0.20.0
terminal milestone. Priority: **medium**. Effort: **M (1–2 weeks)**. No key is added to
`fasttail.ini`.

### Non-goals

- Code signing and notarization (a Windows certificate, an Apple Developer account): both
  cost money and are the maintainer's decision; unsigned packages keep the SmartScreen
  and Gatekeeper prompts the archives have today (`docs/distribution-channels.md`).
- Flatpak / Flathub, Arch `pkg.tar.zst` / AUR, Homebrew, Scoop, winget, Chocolatey: package
  *channels* maintained outside this repository, decided separately.
- Windows ARM64 and x86 32-bit, Linux ARMv7: not release targets (AGENTS.md fixed
  decisions), reconsidered in their own change.
- Android, iOS, a web build.
- An `.exe` setup program next to the `.msi`.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `release-pipeline`: the release assets gain the installable packages and the macOS
  x86_64 archive; the archives hold both executables (since 0.15.0); every package is
  install-tested in CI before it is published.

## Impact

- `.github/workflows/build.yml`: packaging steps per matrix entry, a macOS x86_64 build
  on the Apple Silicon runner and a `lipo` step for the universal app, install tests.
- `packaging/` (new): `linux/fasttail.desktop`, `windows/fasttail.wxs` (WiX), `macos/Info.plist`,
  icon sources (PNG sizes, `.ico`, `.icns` generated from `assets/logo.svg`).
- `Cargo.toml`: `[package.metadata.deb]` and `[package.metadata.generate-rpm]`.
- Build tools installed in CI only: `cargo-deb`, `cargo-generate-rpm`, the WiX Toolset
  (`wix`, a .NET tool), `appimagetool`; no new crate in FastTail itself.
- `README.md` (prebuilt binaries table, install instructions per platform),
  `docs/distribution-channels.md`, CHANGELOG, `openspec/specs/release-pipeline`.
