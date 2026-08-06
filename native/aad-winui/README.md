# AAD WinUI host (ADR-0017)

Hybrid shell: **C++ chrome + Direct2D canvas**, Rust CAD engine via `aadt_ffi.dll`.

## Layout

| Path | Role |
|------|------|
| `include/aadt_ffi.h` | C ABI (version 2) |
| `include/FfiSession.hpp` | RAII Session wrapper |
| `include/Direct2DCanvas.hpp` | Direct2D pan/zoom painter |
| `src/main.cpp` | **Interim** Win32 host (CLI hero + paper) — ships without Windows App SDK |
| `winui/` | Target WinUI 3 + Fluent 2 project notes / stubs |

Full **WinUI 3** packaging (MSIX, `SwapChainPanel`) is the product shell; the Win32 host proves FFI + Direct2D first.

## Build engine DLL

From repo root:

```bat
cargo build -p aadt-ffi --release
```

DLL (and `.dll.lib`) land in `target/release/aadt_ffi.dll` (or `$CARGO_TARGET_DIR/release` if set).

## Build host (CMake)

### MinGW (no MSVC required — uses `LoadLibrary`, no import lib)

```bat
cargo build -p aadt-ffi --release
cd native\aad-winui
cmake -B build-mingw -G "MinGW Makefiles" -DCMAKE_BUILD_TYPE=Release -DAADT_FFI_DIR=%CD%\..\..\target\release
cmake --build build-mingw
build-mingw\aad-winui.exe
```

If cargo uses a custom `CARGO_TARGET_DIR`, point `-DAADT_FFI_DIR` at that `release` folder (must contain `aadt_ffi.dll`).

### MSVC / Visual Studio

```bat
cd native\aad-winui
cmake -B build -G "Visual Studio 17 2022" -A x64 -DAADT_FFI_DIR=%CD%\..\..\target\release
cmake --build build --config Release
build\Release\aad-winui.exe
```

DLL is copied beside the exe on build.

### Controls

- Command edit — **Enter** feeds `Session` (same as CLI)
- `LINE` or `L` alone — two **LMB** clicks draw a segment
- Idle **LMB** — appends world `x,y` into the command edit
- **F** fit · **Wheel** zoom · **RMB/MMB** pan · **Esc** cancel pick
- Status bar: entity count, revision, cursor world coords
- Try `LINE` then click two points, or `CIRCLE 5 3 2`

## WinUI 3 next steps

1. Create a Blank App (WinUI 3 in Desktop / C++) with Windows App SDK in Visual Studio.
2. Add the same `FfiSession` / Direct2D sources; host D2D on `SwapChainPanel` (or `SurfaceImageSource`).
3. Apply AAD Chrome 2.0: single top strip, taller CLI history, auto-hide NavigationView panes, Properties primary.
4. Deprecate `aadt app` (egui) once command parity is accepted.

Do **not** mutate geometry from XAML — only `aadt_session_feed` / typed FFI.

## Brand

Orange (`#E85D04`) for brand mark / focus only. Canvas strokes stay CAD ink (see Direct2DCanvas).
