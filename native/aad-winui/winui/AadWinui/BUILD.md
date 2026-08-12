# AadWinui — M17 Chrome 2.0 (WinUI 3 C++)

Real Visual Studio project for the shipping Fluent shell ([ADR-0017](../../../docs/adr/0017-winui-shell-rust-engine.md)).

## Build

```bat
nuget restore AadWinui.sln -PackagesDirectory packages
msbuild AadWinui.sln /p:Configuration=Debug /p:Platform=x64
cargo build -p aadt-ffi --release
copy ..\..\..\..\target\release\aadt_ffi.dll x64\Debug\AadWinui\
..\..\..\..\bridge\build-bridge.cmd
copy ..\..\..\..\bridge\Aadt.Bridge.Native\bin\Release\net8.0\win-x64\publish\aadt_bridge.dll x64\Debug\AadWinui\
copy ..\..\..\..\bridge\Aadt.Bridge.Native\bin\Release\net8.0\win-x64\publish\e_sqlite3.dll x64\Debug\AadWinui\
```

MSBuild also copies `aadt_ffi.dll`, `aadt_bridge.dll`, and `e_sqlite3.dll` into `$(OutDir)` when present.

Requires **aadt_ffi.dll ABI ≥ 4** and **aadt_bridge.dll** + **e_sqlite3.dll** (Aorms.Bridge → `%LocalAppData%\AADT\firm.db`).

## Controls

| Input | Action |
|-------|--------|
| CLI Enter | feed / start LINE·CIRCLE·REC |
| Idle LMB | pick → SELECT (opens Properties) |
| Tool LMB | toolAcceptPoint |
| MMB/RMB drag · Wheel | pan · zoom |
| Esc | cancel tool |
| Right rail Prp | Properties auto-hide / pin |
| Properties Erase / CHSPACE | `feed` only |

## MSIX (M17.4)

Debug stays unpackaged (`WindowsPackageType=None`). Release builds an MSIX:

```powershell
.\pack-msix.ps1
```

Output: `winui/AppPackages/AadWinui_*_Test/AadWinui_*.msix`.

First-time sideload needs the self-signed `CN=AAD` cert in **Current User → Trusted Root**
(Windows shows a prompt). Packaging alone is enough without install:

```powershell
certutil -user -addstore Root path\to\AAD-WinUI.cer
Add-AppxPackage -Path path\to\AadWinui_*.msix
```

Or enable **Developer Mode** and run `Add-AppDevPackage.ps1` in the `*_Test` folder.

## Status

| Item | |
|------|--|
| M17.1–3 | ✅ |
| M17.4 MSIX | ✅ Release MSIX + `pack-msix.ps1` |
