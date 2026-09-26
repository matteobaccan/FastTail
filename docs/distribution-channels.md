# Distribution channels — parked, not abandoned

FastTail ships as a GitHub release: an archive per platform, downloaded by hand. Four
other channels would make it installable with one command, and this page records what
each of them costs, so the decision can be taken with the facts rather than re-researched.

**Nothing here is wired up.** The pipeline builds and publishes the GitHub release, full
stop. The automation described below was written and then parked on purpose: it is easy
to bring back (see the history of `.github/workflows/build.yml` between v0.8.0 and v0.9.0,
pull requests #60 to #62), and bringing it back is a decision about maintenance, not about
code. Chocolatey was never automated.

| Channel | What it gives | What it costs |
|---|---|---|
| crates.io | `cargo install fasttail` | a token in CI; a published version can never be replaced, only superseded |
| Scoop | `scoop install fasttail` from our own bucket | a manifest to keep honest; no third-party approval |
| winget | `winget install MatteoBaccan.FastTail` | one manual first submission, a personal access token, and a review per release |
| Chocolatey | `choco install fasttail` | human moderation for new packages, a nuspec with an install script to maintain |

## crates.io

The registry publishes *sources*: `cargo install fasttail` downloads them and compiles
with the user's toolchain. `Cargo.toml` is already prepared for it (`readme`, `keywords`,
`categories`, `rust-version` and an `exclude` list that keeps the crate to the code, the
licence and the user documentation — 36 files, ~320 KiB, verified with `cargo package`).

What is missing is only the decision and a token: crates.io → *Account settings → API
Tokens*, scopes `publish-new` and `publish-update`, stored as the `CARGO_REGISTRY_TOKEN`
secret. A published version cannot be replaced or deleted, only yanked, so the job that
did this checked that the tag matched `Cargo.toml` before publishing.

## Scoop

A manifest in a repository *is* a bucket — no approval from anyone. The manifest that was
prepared pointed at the Windows zip, declared `fasttail.exe` as the binary, persisted
`fasttail.ini`, and carried `checkver`/`autoupdate` so a scheduled run could move it to
each new release and recompute the hash.

## winget

`winget install MatteoBaccan.FastTail` needs a manifest in Microsoft's community repository,
[microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs). Manifests get there by
pull request: a bot validates them, installs the package in a sandbox and, once a
maintainer approves, merges. From then on every release is one more pull request.

The catch that decides the shape of any automation: the action that opens those pull
requests (`vedantmgoyal9/winget-releaser`) **updates an existing package and cannot
create one**, so the first version must be submitted by hand whatever we do afterwards.
It also needs a classic personal access token with `public_repo` and `workflow`, because
it pushes the branch to *your* fork of `winget-pkgs`.

### The first submission, by hand

1. Install the helper (it is itself a winget package):

   ```powershell
   winget install Microsoft.WingetCreate
   ```

2. Let it build the manifest from the release asset, answering its questions:

   ```powershell
   wingetcreate new https://github.com/matteobaccan/FastTail/releases/download/v0.10.1/fasttail-windows-x86_64-0.10.1.zip
   ```

   What it asks for, and what FastTail answers:

   | Field | Value |
   |---|---|
   | Package identifier | `MatteoBaccan.FastTail` |
   | Publisher | `Matteo Baccan` |
   | Package name | `FastTail` |
   | Moniker | `fasttail` |
   | Licence | `MIT` |
   | Licence URL | `https://github.com/matteobaccan/FastTail/blob/main/LICENSE` |
   | Short description | `Ultra-fast multi-stream log monitor and tail viewer with a Cyberpunk UI` |
   | Publisher URL / Package URL | `https://github.com/matteobaccan/FastTail` |
   | Release notes URL | `https://github.com/matteobaccan/FastTail/releases/tag/v0.9.0` |
   | Tags | `log` `tail` `logging` `monitoring` `viewer` |
   | Installer type | `zip`, nested `portable` |
   | Nested installer path | `fasttail.exe` |
   | Command alias | `fasttail` |

   From v0.9.0 the asset is a zip holding a single executable (the symbols travel in
   their own zip; earlier zips also carried `fasttail.pdb`); from v0.10.1 its name carries
   the version (`fasttail-windows-x86_64-0.10.1.zip`) and it also holds `LICENSE` and
   `README.md`, next to the executable. winget treats it as a **portable**
   package: it unpacks it and puts `fasttail` on the PATH. Nothing is written to the
   registry and uninstalling removes the files.

3. `wingetcreate` submits the pull request for you when you give it a GitHub token, or
   writes the manifest under `manifests/m/MatteoBaccan/FastTail/0.9.0/` for you to submit
   yourself. Expect the validation bot to take a few minutes and a maintainer a few days.

### The token an automation would need

If the later submissions are ever automated, the action needs a token of yours:

1. GitHub → **Settings → Developer settings → Personal access tokens → Tokens (classic)**,
   new token with the `public_repo` and `workflow` scopes.
2. Repository → **Settings → Secrets and variables → Actions**, new secret named
   `WINGET_TOKEN`.
3. Fork [microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs) to your account
   (the action pushes the branch there).

The parked job was written so that, without the secret, it printed a warning and let the
release carry on; bringing it back that way means nothing breaks while the token is
pending.

## Chocolatey

Not investigated beyond the obvious: new packages go through human moderation and the
package is maintained as a nuspec with its own install script. It is the most expensive
of the four and the least used by this audience; worth revisiting only if people ask.

## macOS signing

Unrelated to package managers but the same kind of decision: the macOS build is unsigned,
so Gatekeeper blocks the first launch (the README says how to get past it). Removing that
prompt needs a paid Apple Developer account, then `codesign` and `notarytool` in the
pipeline.
