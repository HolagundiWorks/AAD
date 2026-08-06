# WinUI 3 + Fluent 2 target shell

This folder holds the **product** UI direction (ADR-0017). The interim executable in
`../src/main.cpp` is Win32+Direct2D so the repo builds without the Windows App SDK
(MinGW or MSVC). Full WinUI packaging needs Visual Studio 2022 + Windows App SDK.

## Status on this machine

If VS / WinAppSDK are not installed, keep iterating on the interim host. The stubs
here (`MainWindow.xaml*`, `App.xaml*`, `packages.config` notes) are the layout contract
to paste into a Blank App (WinUI 3 C++) project.

## Recommended VS project

1. Visual Studio 2022 → **Blank App, Packaged (WinUI 3 in Desktop)** → C++.
2. Windows App SDK (latest stable).
3. Add existing:
   - `../include/*`
   - `../src/FfiSession.cpp`
   - `../src/Direct2DCanvas.cpp` (adapt to `SwapChainPanel` / composition surface)
4. Prefer `AADT_FFI_DYNAMIC` + `LoadLibrary` (same as interim host) so you do not need
   `aadt_ffi.dll.lib` from MSVC. Or link the import lib from `cargo build -p aadt-ffi --release`.
5. Chrome layout (AAD Chrome 2.0) — see `MainWindow.xaml` sketch.

## Wire-up checklist

| UI | FFI |
|----|-----|
| CLI TextBox Enter | `FfiSession::feed` |
| Idle / after feed | `rebuildLines` → Direct2D paint |
| Canvas LMB (LINE tool) | host pick → `feed("LINE x1 y1 x2 y2")` |
| Status | `entityCount` / `revision` / `dirty` |
| Never | mutate geometry from XAML / ViewModels |

## Brand

Orange `#E85D04` for brand mark / focus only. Canvas strokes stay CAD ink.
