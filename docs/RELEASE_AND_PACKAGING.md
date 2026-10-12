# Release and packaging

Windows is the only supported platform. The macOS, Linux and web steps exist as scripts and CI jobs
for later phases and were **not exercised in this audit**. The step-by-step release procedure and
the list of release artifacts and secrets is in [releasing.md](releasing.md); this page adds the
parts an audit needs.

## Version

The single source is `[workspace.package] version` in the root `Cargo.toml` (currently 0.6.0).
`cargo xtask version` prints it. Tags and history are never removed.

## Windows release build (verified for 0.6.0 earlier in the project; not re-run in this audit)

```text
pwsh packaging/windows/package.ps1 -Arch x64
```

1. `cargo build --release --locked -p cadcraft -p cadcraft-cli --target x86_64-pc-windows-msvc`
   with a static C runtime, so no Visual C++ redistributable is needed.
2. Stage the files under `target/windows-package/`.
3. Build the MSI with WiX v5 (`packaging/windows/cadcraft.wxs`) and the portable zip.
4. Sign with `packaging/windows/sign.ps1` if signing secrets exist; otherwise the artifacts are
   unsigned and the script warns.

Output: `dist/release/cadcraft-<version>-windows-x64.msi` and `...-portable.zip`.

## Required at run time

The program needs nothing beside the executable ([PROJECT_STRUCTURE.md](PROJECT_STRUCTURE.md)):
icons, patterns, linetypes, the drafting font and the interface strings are compiled in. The
language model (not integrated into the app yet) would be an extra file.

## Validation

Before a release: `cargo xtask ci`, then the checks in [TESTING.md](TESTING.md), then install the
MSI on a clean Windows machine and open a DXF and a DWG. The last step has **not** been done for
0.6.0: the MSI was built but not installed and run on a clean machine ([QUALITY_BASELINE.md](QUALITY_BASELINE.md)).

## Artifact retention

- `dist/` is ignored by git and holds the only copy of the 0.6.0 installer and portable zip. Keep
  the latest validated build; do not delete it until a replacement has been built and validated.
- `target/` is generated and may be deleted at any time ([BUILD_AND_DEVELOPMENT.md](BUILD_AND_DEVELOPMENT.md)).
- CI artifacts (crash inputs from the mutation tests) are kept by GitHub for its default period.
