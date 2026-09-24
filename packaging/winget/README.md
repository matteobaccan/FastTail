# Submitting FastTail to winget

`winget install FastTail` needs a manifest in Microsoft's community repository,
[microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs). Manifests get there by
pull request: a bot validates them, installs the package in a sandbox and, once a
maintainer approves, merges. From then on every release is one more pull request.

The release pipeline opens those pull requests by itself (`🪟 Submit to winget` in
`.github/workflows/build.yml`), but **only from the second version onwards**: the action
it uses updates an existing package and cannot create one. So the first version is
submitted by hand, once.

## Once: the first submission

1. Install the helper (it is itself a winget package):

   ```powershell
   winget install Microsoft.WingetCreate
   ```

2. Let it build the manifest from the release asset, answering its questions:

   ```powershell
   wingetcreate new https://github.com/matteobaccan/FastTail/releases/download/v0.8.0/fasttail-windows-x86_64.zip
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
   | Release notes URL | `https://github.com/matteobaccan/FastTail/releases/tag/v0.8.0` |
   | Tags | `log` `tail` `logging` `monitoring` `viewer` |
   | Installer type | `zip`, nested `portable` |
   | Nested installer path | `fasttail.exe` |
   | Command alias | `fasttail` |

   The asset is a zip holding a single executable, so winget treats it as a **portable**
   package: it unpacks it and puts `fasttail` on the PATH. Nothing is written to the
   registry and uninstalling removes the files.

3. `wingetcreate` submits the pull request for you when you give it a GitHub token, or
   writes the manifest under `manifests/m/MatteoBaccan/FastTail/0.8.0/` for you to submit
   yourself. Expect the validation bot to take a few minutes and a maintainer a few days.

## Once: the token the pipeline needs

The action that opens the later pull requests pushes to *your* fork of `winget-pkgs`, so
it needs a token of yours:

1. GitHub → **Settings → Developer settings → Personal access tokens → Tokens (classic)**,
   new token with the `public_repo` and `workflow` scopes.
2. Repository → **Settings → Secrets and variables → Actions**, new secret named
   `WINGET_TOKEN`.
3. Fork [microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs) to your account
   (the action pushes the branch there).

Without the secret the job prints a warning and the release carries on, so nothing breaks
while this is pending.

## Afterwards

Nothing. Tag a release and the job opens the pull request with the new version, pointing
at `fasttail-windows-x86_64.zip` from that release.

## The other Windows package managers

- **Scoop** is served from this repository: `bucket/fasttail.json` is a complete manifest,
  so `scoop bucket add fasttail https://github.com/matteobaccan/FastTail` followed by
  `scoop install fasttail` works without anyone else's approval. The manifest carries
  `checkver` and `autoupdate`, and `.github/workflows/excavator.yml` runs them nightly to
  point it at the newest release.
- **Chocolatey** is deliberately not automated: new packages go through human moderation
  and the package has to be maintained as a nuspec with its own install script. Worth
  doing if requests for it appear, not before.
