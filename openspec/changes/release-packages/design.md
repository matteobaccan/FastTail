## Context

`build.yml` builds one release binary pair (`fasttail`, `fasttail-tui`, default features)
per target on four runners (Windows x86_64 MSVC, Linux x86_64 and ARM64 GNU, macOS ARM64),
packs each into a `.zip` / `.tar.gz`, and on a `v*` tag (pushed, or dispatched by
`release-tag.yml`) publishes them with the CHANGELOG section. The window links
dynamically to glibc (Linux) and loads OpenGL / Vulkan / X11 / Wayland libraries at run
time; the terminal executable needs only the C library. `assets/logo.svg` is the only
vector icon; `assets/icon.png` is 128 px.

## Goals / Non-Goals

**Goals:**
- One install, upgrade and removal path per desktop OS, from the release page.
- Intel Macs covered again.
- No regression of the archives, and no package published untested.

**Non-Goals:**
- Signing, package channels, new architectures (see the proposal).

## Decisions

1. **Packages are built from the binaries the job already compiled**, never recompiled:
   what the archive holds is what the package installs. Packaging runs after the
   existing "Package release asset" step, in the same job.
2. **Linux `.deb` / `.rpm`: `cargo-deb` and `cargo-generate-rpm`**, configured in
   `Cargo.toml` metadata, installed in CI with `cargo install --locked` (cached).
   Contents: `/usr/bin/fasttail`, `/usr/bin/fasttail-tui`,
   `/usr/share/applications/fasttail.desktop`, `/usr/share/icons/hicolor/{128,256,512}x…/apps/fasttail.png`
   and `scalable/apps/fasttail.svg`, `/usr/share/doc/fasttail/{README.md,LICENSE}`.
   Dependencies: the C library from the linker (`$auto` for deb, the rpm auto-requires);
   the libraries the window loads at run time as `Recommends` (deb: `libgl1`,
   `libvulkan1`, `libxkbcommon0`, `libwayland-client0`; rpm: `Recommends:` with the Fedora
   names), so a headless server can install the package for `fasttail-tui` alone.
   Names: `fasttail_<version>_amd64.deb`, `fasttail_<version>_arm64.deb`,
   `fasttail-<version>-1.x86_64.rpm`, `fasttail-<version>-1.aarch64.rpm` (the conventions
   of each format, which `apt` and `dnf` users expect). One `.rpm` serves Fedora, RHEL and
   openSUSE (no SUSE-specific build: the payload has no distribution-specific paths).
   *Rejected:* `nfpm` (a Go tool, a second packaging description to keep in step with
   `Cargo.toml`).
3. **glibc floor.** Packages run where the binary runs: the Linux jobs move to
   `ubuntu-22.04` / `ubuntu-22.04-arm` (glibc 2.35: Ubuntu 22.04+, Debian 12+, Fedora 36+,
   openSUSE Leap 15.5 needs 2.31 and is out), documented in the README. The archives gain
   the same floor, which today is 2.39 (`ubuntu-latest`). *Open:* RHEL 9 / Rocky 9 (glibc
   2.34) would need a manylinux-style container build; left for a request.
4. **Windows `.msi`: WiX Toolset v5 (`wix` .NET global tool)** with a hand-written
   `packaging/windows/fasttail.wxs`. Per-user scope (`Scope="perUser"`): no UAC prompt,
   install in `%LOCALAPPDATA%\Programs\FastTail`, per-user Start menu shortcut to
   `fasttail.exe` with the icon, the install folder appended to the user `PATH` (for
   `fasttail-tui`), a fixed `UpgradeCode` with `MajorUpgrade` so a newer MSI replaces an
   older one, and "Installed apps" entry with the icon and version. `fasttail.ini` is
   never touched (it lives in the user profile). Name:
   `fasttail-windows-x86_64-<version>.msi`. *Rejected:* `cargo-wix` (WiX v3, which the
   current Windows runner images no longer carry); a per-machine MSI (needs administrator
   rights, which many users of a log viewer on a work PC do not have).
5. **macOS `.dmg`, universal.** The Apple Silicon runner also builds
   `x86_64-apple-darwin` (the SDK has both), `lipo` merges each executable, and the bundle
   `FastTail.app/Contents/{MacOS/fasttail, MacOS/fasttail-tui, Resources/FastTail.icns,
   Info.plist}` is built by a script from `packaging/macos/Info.plist` (bundle id
   `io.github.matteobaccan.fasttail`, version from `Cargo.toml`). `hdiutil create` makes
   `fasttail-macos-universal-<version>.dmg` holding the app, an `Applications` symlink,
   `README.md` and `LICENSE`. The Intel binary is also packed as
   `fasttail-macos-x86_64-<version>.tar.gz`; the Apple Silicon archive keeps its name and
   content. *Rejected:* a separate `macos-13` Intel runner (being retired by GitHub, and
   the cross build is faster).
6. **AppImage: `appimagetool`** on an `AppDir` with `AppRun` → `usr/bin/fasttail`, the
   desktop entry and the icon; `fasttail-tui` is included in `usr/bin` and reachable with
   `--appimage-extract` or, once 0.20.0 adds `fasttail --tui`, from the AppImage itself.
   No graphics library is bundled: OpenGL / Vulkan must come from the host driver.
   Names: `fasttail-linux-x86_64-<version>.AppImage`,
   `fasttail-linux-arm64-<version>.AppImage`.
7. **Icons from `assets/logo.svg`**, rendered once to `packaging/icons/` (PNG 16 to 512,
   `fasttail.ico`, `FastTail.icns`) with a script (`rsvg-convert`, ImageMagick, `iconutil`
   or `png2icns`) and committed, so the release jobs do not depend on image tools.
8. **Install tests before upload.** Linux: `apt-get install ./…deb` on the runner, run both
   `--version`, `apt-get remove`; the `.rpm` in a `fedora:latest` container (x86_64 and
   ARM64 runners both run Docker) with `dnf install`; the AppImage run with
   `--appimage-extract-and-run --version` (no FUSE on runners). Windows: `msiexec /i … /qn`,
   run both from the install folder and via the new `PATH` in a fresh shell, `msiexec /x`.
   macOS: `hdiutil attach`, `lipo -archs` shows `x86_64 arm64`, run both from the mounted
   app, `hdiutil detach`. A failure fails the job, so no release is published without its
   packages (the release job already needs every build job).

Threads and memory: not applicable (build-time only). Build time: packaging adds about
1–3 minutes per job; the macOS job compiles a second target (about +6 minutes), still
under the Windows job, which bounds the release. Asset count per release: 5 → 14.

## Risks / Trade-offs

- [Unsigned installers] → SmartScreen ("More info → Run anyway") and Gatekeeper ("Open
  anyway") prompts, as with the archives; README explains both; signing is a separate
  decision.
- [glibc floor moves from 2.39 down to 2.35 but not lower] → documented; the `.tar.gz`
  gains older distributions too.
- [WiX / appimagetool downloads in CI] → pinned versions and checksums; a failed download
  fails the build before the tag's release is created.
- [Asset list grows] → names keep `<os>-<arch>-<version>` except where the package format
  has its own convention (`.deb`, `.rpm`); the README table lists all of them.

## Open Questions

- RHEL 9 / Rocky 9 support (glibc 2.34) through a container build.
- Whether the `.msi` should offer a per-machine install as an option.
