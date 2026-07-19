# ADR-0001 — Application language and UI stack

- **Status:** Proposed (Rust decided; UI toolkit recommended, revisit before M1)
- **Context:** Spec §11 #1 — C++/Qt vs. Rust (Slint/egui). Determines team
  skillset needs and long-term maintenance cost.

## Decision

Build AADT in **Rust**, from-scratch native (no adoption of an existing CAD
core). For the interactive shell, the recommended toolkit is **egui + a custom
`wgpu` canvas**:

- The product is **command-line-first** (spec §2, principle 2); an immediate-mode
  UI (egui) suits a command line + docked panels + a custom-drawn viewport far
  better than a retained widget tree.
- The CAD viewport needs direct GPU control for 60fps pan/zoom on 100k+ entities
  (spec §4.3). A custom `wgpu` canvas gives that regardless of the widget layer.
- `wgpu` is the mature Rust GPU abstraction and satisfies the "OpenGL or Vulkan"
  rendering requirement (spec §3.1) portably across Windows/macOS/Linux.

**Slint** remains a viable alternative for the chrome (its declarative model and
tooling are strong); the canvas would still be custom `wgpu`. This choice is
isolated to `aadt-app`/`aadt-render` and does not affect the engine crates.

## Consequences

- Memory safety and a single toolchain (`cargo`) across the whole workspace.
- Smaller existing 2D-CAD ecosystem to borrow from than C++ — accepted, since
  the from-scratch decision already implies building the kernel ourselves.
- `unsafe` is workspace-forbidden today; `aadt-render` will relax this locally
  when it integrates `wgpu`.

## Alternatives considered

- **C++/Qt:** largest CAD ecosystem and the language of ODA/LibreCAD/QCAD, but
  the from-scratch decision removes most of that reuse benefit, and Qt licensing
  and C++ safety costs weigh against it.
