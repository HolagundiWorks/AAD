# AAD installer

Packaging for **AAD — Accelerated Architectural Drafting** (Windows x64).

## Quick build

```powershell
powershell -ExecutionPolicy Bypass -File installer\build.ps1
```

This:
1. builds the release binary (`cargo build --release -p aadt-cli --features app`),
2. writes a **portable zip** → `installer\Output\AAD-portable-<ver>.zip`
   (unzip anywhere, run `AAD.exe app`), and
3. if **Inno Setup 6** is installed, compiles
   `installer\Output\AAD-Setup-<ver>.exe`.

## Full installer (setup.exe)

Requires [Inno Setup 6](https://jrsoftware.org/isinfo.php). Then:

```powershell
cargo build --release -p aadt-cli --features app
iscc installer\aad.iss
```

`AAD-Setup-<ver>.exe` is a **per-user** installer (no admin prompt): it installs
to the user's Programs folder, adds Start-Menu and optional desktop shortcuts
that launch `AAD.exe app` with the working directory set to the install folder,
bundles the runtime `libraries\`, and registers an uninstaller.

## What ships

| Item | Notes |
|---|---|
| `AAD.exe` | The desktop shell — the `aadt` CLI binary, installed as `AAD.exe`. The GUI opens via the `app` subcommand, which the shortcuts pass. |
| `libraries\` | Block / hatch / detail / font libraries loaded at runtime (resolved next to the executable). Includes `libraries\details\*.md`, the sheets `RECOMMEND DETAIL` indexes. |
| `README.md`, `NOTICE.md` | Docs + third-party notices. |

Brand icons and UI fonts are embedded in the executable.

> The version string is `0.0.0`, matching the workspace `Cargo.toml`. Bump it
> there and in `aad.iss` / `build.ps1` for a release.
