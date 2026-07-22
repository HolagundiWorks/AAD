# ADR-0001 — Application language and UI stack

- **Status:** Accepted (Rust + egui for the interactive shell; canvas remains
  custom 2D; WinUI only as a future chrome option via FFI)
- **Context:** Spec §11 #1 — C++/Qt vs. Rust (Slint/egui). Determines team
  skillset needs and long-term maintenance cost.

## Decision

Build AADT in **Rust**, from-scratch native (no adoption of an existing CAD
core). The product is **2D CAD only** — no 3D modeler.

For the interactive shell, ship **egui + a custom 2D viewport** (egui painter
today; `wgpu` when entity-count performance demands it):

- The product is **command-line-first** (spec §2, principle 2); an immediate-mode
  UI (egui) suits a command line + docked panels + a custom-drawn viewport far
  better than a retained widget tree.
- A single Rust toolchain keeps the GUI a thin driver of `Session` — identical
  undo/command path to the headless CLI.
- `wgpu` remains the planned GPU backend for large drawings; it is not a gate on
  drafting usability.

**WinUI 3** (or other native Windows chrome) is **not** the near-term shell. If
wanted later, it wraps the same `Session` through the thin `aadt-ffi` cdylib —
chrome swap only, no engine rewrite. Do not put drafting logic in C#/XAML.

**Slint** remains a viable alternative for chrome; the canvas would still be
custom 2D. This choice is isolated to `aadt-app` / `aadt-render` / `aadt-ffi`.

## Consequences

- Memory safety and a single toolchain (`cargo`) across the engine and shell.
- Smaller existing 2D-CAD ecosystem to borrow from than C++ — accepted.
- `unsafe` is workspace-forbidden in engine crates; `aadt-render` / `aadt-ffi`
  may relax this locally for GPU or C ABI as needed.
- Windows-native polish is deferred; drafting power ships first.

## Alternatives considered

- **C++/Qt:** largest CAD ecosystem, but from-scratch removes most reuse; Qt
  licensing and C++ safety costs weigh against it.
- **WinUI 3 now:** strong Windows chrome, but forces a dual-runtime FFI boundary
  around every command and frame; slows the lean-drafting + LISP push for little
  drafting gain.
- **Electron / web UI:** fights command-first CAD and offline local-first feel.
