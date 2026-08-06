# ADR-0001 — Application language and UI stack

- **Status:** Superseded for the **interactive shell** by
  [ADR-0017](0017-winui-shell-rust-engine.md) (WinUI 3 + Direct2D; Rust engine via
  FFI). **Still accepted** for: Rust CAD engine, 2D-only product, no Electron.
- **Context:** Spec §11 #1 — C++/Qt vs. Rust (Slint/egui). Determines team
  skillset needs and long-term maintenance cost.

## Decision (historical — shell portion superseded)

Build AADT in **Rust**, from-scratch native (no adoption of an existing CAD
core). The product is **2D CAD only** — no 3D modeler.

The interactive shell **originally** shipped as **egui + a custom 2D viewport**
(egui painter; `wgpu` when entity-count performance demands it):

- Command-line-first; immediate-mode UI suited early drafting.
- Single Rust toolchain kept the GUI a thin driver of `Session`.

**Current shell (ADR-0017):** C++/WinRT **WinUI 3** + Fluent 2 + **2D, driving
the same `Session` through `aadt-ffi`. Do not put drafting logic in C#/XAML.
egui remains a maintenance fallback (`aadt app`).

**Slint** remains a viable alternative chrome if WinUI is abandoned; canvas stays
custom 2D / Direct2D / wgpu.

## Consequences

- Engine crates stay Rust (`unsafe_code = forbid` except render/ffi).
- Windows GUI builds require MSVC + cargo (see `native/aad-winui/`).
- Full C++ rewrite of the engine is out of scope (ADR-0017).

## Alternatives considered

- **C++/Qt:** largest CAD ecosystem; Qt licensing and dual language weighed against
  early velocity — engine stayed Rust.
- **WinUI 3 as chrome only:** accepted in ADR-0017 as the shipping shell.
- **Electron / web UI:** fights command-first CAD and offline local-first feel.
