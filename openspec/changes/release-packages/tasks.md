## 1. Shared inputs

- [ ] 1.1 `packaging/icons/`: PNG 16–512, `fasttail.ico`, `FastTail.icns` rendered from `assets/logo.svg` by `packaging/icons/render.sh` (committed output)
- [ ] 1.2 `packaging/linux/fasttail.desktop` (Name, Comment, Exec=fasttail %F, Icon=fasttail, Categories=Utility;Development;System;Monitor, MimeType text/plain and text/x-log)
- [ ] 1.3 Linux build jobs on `ubuntu-22.04` / `ubuntu-22.04-arm`; check the archives still build and run; README states the glibc 2.35 floor

## 2. Linux packages

- [ ] 2.1 `[package.metadata.deb]`: assets, `$auto` depends, Recommends for the window's run-time libraries, section, priority, extended description
- [ ] 2.2 `[package.metadata.generate-rpm]`: assets, auto requires, `Recommends` with Fedora names, licence
- [ ] 2.3 Build steps in the Linux jobs (`cargo deb --no-build`, `cargo generate-rpm`), tools cached; install tests (apt on the runner, dnf in `fedora:latest`), both `--version`, removal
- [ ] 2.4 AppImage: `AppDir`, `AppRun`, desktop entry and icon, pinned `appimagetool` with checksum; test with `--appimage-extract-and-run --version`

## 3. Windows installer

- [ ] 3.1 `packaging/windows/fasttail.wxs` (WiX v5): per-user scope, install folder, Start menu shortcut with icon, user `PATH`, fixed UpgradeCode with `MajorUpgrade`, ARP icon and version from `Cargo.toml`
- [ ] 3.2 Windows job: pinned `wix` .NET tool, build the `.msi`; install test (`msiexec /qn`, both `--version` from the folder and from `PATH` in a new shell, `msiexec /x`)
- [ ] 3.3 Upgrade test: install the previous release's MSI, then the new one, one entry left in "Installed apps"

## 4. macOS disk image

- [ ] 4.1 macOS job builds `x86_64-apple-darwin` too; `lipo` universal executables; `fasttail-macos-x86_64-<version>.tar.gz`
- [ ] 4.2 `packaging/macos/Info.plist` and the bundle script (`FastTail.app` with both executables and the `.icns`)
- [ ] 4.3 `hdiutil` `.dmg` with the app, an `Applications` link, README and LICENSE; test: attach, `lipo -archs`, both `--version`, detach

## 5. Release and documentation

- [ ] 5.1 Release job publishes the new assets (flattened as today); a dispatch run on the branch shows all 14 assets before the change merges
- [ ] 5.2 README: prebuilt binaries table with every asset, install / upgrade / remove per platform, unsigned-installer prompts, glibc floor
- [ ] 5.3 `docs/distribution-channels.md`: packages now published as release assets; channels (Flathub, AUR, Homebrew, Scoop, winget) still parked
- [ ] 5.4 CHANGELOG `[Unreleased]`

## 6. Wrap-up

- [ ] 6.1 `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test` green; PR with Linux and Windows CI green and a green dispatch build of every target
- [ ] 6.2 The next nightly release carries the packages; the maintainer installs the `.msi` and one Linux package from the release page
- [ ] 6.3 After the release, archive the change so `release-pipeline` is updated
