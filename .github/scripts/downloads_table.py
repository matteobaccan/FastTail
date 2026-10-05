#!/usr/bin/env python3
# FastTail -- the downloads grid at the top of a GitHub Release body.
#
# Usage: downloads_table.py <asset dir> <tag> <owner/repo>
# Prints a Markdown table, one row per architecture and one column per system, linking
# every asset found in <asset dir> by its file name. Assets that do not exist yet (a
# package not built for this release) simply leave their cell empty, so the grid follows
# the release pipeline as it grows.

import os
import re
import sys

# (row, column, label, pattern); patterns match the asset names build.yml produces.
ASSETS = [
    ("x86-64", "Windows", "MSI", r"^fasttail-windows-x86_64-.*\.msi$"),
    ("x86-64", "Windows", "ZIP", r"^fasttail-windows-x86_64-[0-9].*\.zip$"),
    ("x86-64", "Linux", "DEB", r"^fasttail_.*_amd64\.deb$"),
    ("x86-64", "Linux", "RPM", r"^fasttail-.*\.x86_64\.rpm$"),
    ("x86-64", "Linux", "AppImage", r"^fasttail-linux-x86_64-.*\.AppImage$"),
    ("x86-64", "Linux", "tar.gz", r"^fasttail-linux-x86_64-.*\.tar\.gz$"),
    ("x86-64", "Linux", "terminal only", r"^fasttail-tui-linux-x86_64-.*\.tar\.gz$"),
    ("x86-64", "macOS", "DMG", r"^fasttail-macos-universal-.*\.dmg$"),
    ("x86-64", "macOS", "tar.gz", r"^fasttail-macos-x86_64-.*\.tar\.gz$"),
    ("ARM64", "Linux", "DEB", r"^fasttail_.*_arm64\.deb$"),
    ("ARM64", "Linux", "RPM", r"^fasttail-.*\.aarch64\.rpm$"),
    ("ARM64", "Linux", "AppImage", r"^fasttail-linux-arm64-.*\.AppImage$"),
    ("ARM64", "Linux", "tar.gz", r"^fasttail-linux-arm64-.*\.tar\.gz$"),
    ("ARM64", "Linux", "terminal only", r"^fasttail-tui-linux-arm64-.*\.tar\.gz$"),
    ("ARM64", "macOS", "DMG", r"^fasttail-macos-universal-.*\.dmg$"),
    ("ARM64", "macOS", "tar.gz", r"^fasttail-macos-arm64-.*\.tar\.gz$"),
]
ROWS = [("x86-64", "**x86-64** (64-bit)"), ("ARM64", "**ARM64** (AArch64, Apple Silicon)")]
COLUMNS = ["Windows", "Linux", "macOS"]


def main() -> None:
    folder, tag, repo = sys.argv[1:4]
    names = sorted(os.listdir(folder))
    base = f"https://github.com/{repo}/releases/download/{tag}"
    cells = {}
    for row, col, label, pattern in ASSETS:
        for name in names:
            if re.match(pattern, name):
                cells.setdefault((row, col), []).append(f"[{label}]({base}/{name})")
    print("## Downloads")
    print()
    print("| Architecture | " + " | ".join(COLUMNS) + " |")
    print("|---|" + "---|" * len(COLUMNS))
    for key, title in ROWS:
        line = [" ".join(cells.get((key, col), [])) or "—" for col in COLUMNS]
        print(f"| {title} | " + " | ".join(line) + " |")
    print()
    symbols = [n for n in names if "-symbols-" in n]
    notes = [
        "Every download but the *terminal only* ones holds both programs: `fasttail` (the window) and `fasttail-tui` "
        "(the terminal interface)."
    ]
    if any(n.startswith("fasttail-tui-linux-") for n in names):
        notes.append(
            "*terminal only*: `fasttail-tui` alone, built without the window, for servers "
            "and SSH sessions with no graphical libraries."
        )
    if symbols:
        links = ", ".join(f"[{n}]({base}/{n})" for n in symbols)
        notes.append(f"Windows debug symbols, only needed to read a crash dump: {links}.")
    notes.append(
        "The builds are not signed: Windows SmartScreen asks to confirm (More info → Run "
        "anyway) and macOS asks once (System Settings → Privacy & Security → Open anyway)."
    )
    for n in notes:
        print(n)
        print()


if __name__ == "__main__":
    main()
