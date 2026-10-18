# Release and packaging

Windows is the only supported platform. The macOS, Linux and web steps exist as scripts and CI jobs
for later phases and were **not exercised in this audit**. The step-by-step release procedure and
the list of release artifacts and secrets is in [releasing.md](releasing.md); this page adds the
parts an audit needs.

## Version

The single source is `[workspace.package] version` in the root `Cargo.toml` (currently 0.6.0).
`cargo xtask version` prints it. Tags and history are never removed.

## Windows release build (verified 2026-10-12)

```text
powershell -NoProfile -ExecutionPolicy Bypass -File packagingwindowspackage.ps1 -Arch x64
```

1. `cargo build --release --locked -p aad -p aad-cli --target x86_64-pc-windows-msvc`
   with a static C runtime, so no Visual C++ redistributable is needed.
2. Stage the files under `target/windows-package/`.
3. Build the MSI with WiX v5 (`packaging/windows/aad.wxs`) and the portable zip.
4. Sign with `packaging/windows/sign.ps1` if signing secrets exist; otherwise the artifacts are
   unsigned and the script warns.

Output: `dist/release/o-draft-<version>-windows-x64.msi` and `...-portable.zip`.

## Required at run time

The program needs nothing beside the executable ([PROJECT_STRUCTURE.md](PROJECT_STRUCTURE.md)):
icons, patterns, linetypes, the drafting font and the interface strings are compiled in. The
language model (not integrated into the app yet) would be an extra file.

## Validation

Before a release: `cargo xtask ci`, then the checks in [TESTING.md](TESTING.md), then the installer
checks below.

Performed on 2026-10-12 for the rebuilt 0.6.0 (unsigned, built with `DIST=dist\candidate`, then
moved to `dist/release`):

| Check | Result |
|---|---|
| Build: `package.ps1 -Arch x64` | passed (release build 3.5 minutes); MSI 17 846 272 bytes, zip 22 689 850 bytes |
| Portable zip, extracted to an empty folder | contains `aad.exe`, `aad-cli.exe`, `LICENSE-APACHE`, `LICENSE-MIT`, `README.md`; the CLI reports 0.6.0, reads a DXF, draws, saves and exports a PNG; the GUI starts on the GPU canvas, answers the control channel and quits; a damaged DWG (the BUG-001 fixture) is refused with a message by both the CLI and the GUI and the program stays up (the DWG worker works from the packaged executable) |
| MSI administrative extract | the same two executables as the previous build |
| MSI real install (silent, elevated) | exit 0; `C:\Program Files\AAD\` with both executables; "Add or remove programs" entry "AAD 0.6.0" by Human Centric Works; Start Menu shortcut; no desktop shortcut (opt-in) |
| Installed programs | CLI runs and saves a DXF; GUI starts, shows 46 entities on the GPU canvas, exits cleanly |
| MSI uninstall (silent) | exit 0; install folder, Start Menu entry and uninstall entry all removed |

Limits of that test: it ran on the development machine, not a clean one, and an AAD installation
already existed (the install upgraded it). It was a silent install, so the wizard pages (Welcome,
Shortcuts) were **not** exercised, nor the desktop-shortcut option, upgrade-over-older-version
behaviour with a real older release, file-type registration (the `.dxf` and `.dwg` "Open with"
entries exist in `aad.wxs` but were not read back from the registry), or the x86 and arm64
variants. Signing was not tested (no secrets).

Findings from the test:
- Neither package ships `NOTICE` or `ATTRIBUTION.md`; the MSI ships no licence files, the zip ships
  the two licences and the README. Third-party notices (for example the MPL-2.0 `acadrust`
  dependency, see `NOTICE`) are therefore not delivered with a build. Not changed here: it alters
  packaging, so it needs the owner's decision.
- Both packages are unsigned until signing secrets are configured; Windows SmartScreen will warn.

## Artifact retention

- `dist/release/` holds the validated 0.6.0 installer and portable zip (SHA-256 of the MSI `11D64F87...BEEAE9`, of the zip `ACFD8145...0772D5`); `dist/release/previous-0.6.0-first-build/` holds the first 0.6.0 build (older code, never installed). `dist/` is ignored by git. Keep
  the latest validated build; do not delete it until a replacement has been built and validated.
- `target/` is generated and may be deleted at any time ([BUILD_AND_DEVELOPMENT.md](BUILD_AND_DEVELOPMENT.md)).
- CI artifacts (crash inputs from the mutation tests) are kept by GitHub for its default period.
