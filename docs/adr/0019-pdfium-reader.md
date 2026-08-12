# ADR-0019 — PDFium for interactive PDF open / render

- **Status:** Accepted
- **Date:** 2026-08-06
- **Scope:** L3.3 — open arbitrary PDFs, rasterize pages, freehand in page space,
  raster sheet diff. Complements L3.1 sheet preview and L3.2 MarkupList freehand.
- **Related:** [`aadt-pdf`](../../crates/aadt-pdf/), ROADMAP Later L3.

## Context

M7 / L3.1 ship a **vector plot writer** and Review markups on the composed sheet
preview. Opening third-party PDFs needs a real reader. Options: PDFium, MuPDF,
or pure-Rust parsers. Workspace rules ask for an ADR before heavy native deps
and keep `unsafe_code = forbid` on engine crates.

## Decision

1. **Reader:** [pdfium-render](https://crates.io/crates/pdfium-render) behind
   Cargo feature `pdfium` on `aadt-pdf` (enabled from `aadt-app` `gui`).
2. **Bind at runtime** to `pdfium.dll` / `libpdfium.so` (bblanchon binaries).
   Missing DLL → clear UI error; never panic `Session::feed`.
3. **No `unsafe` in AADT sources** — only the binding crate uses FFI.
4. **Coordinate space:** page points (origin bottom-left, PDF convention mapped
   to Y-up world for markups). Freehand on an open PDF stores `Markup.page`.
5. **Raster diff:** compare two page RGBA buffers (deterministic % changed +
   narrate); does not require PDFium when both bitmaps are supplied.
6. **Licensing:** PDFium (BSD-3 / Apache-2.0 style) is compatible with
   Apache-2.0 AADT; document binary redistrib in packaging notes.
7. **WinUI:** egui Review is the first consumer; WinUI PDF open follows the same
   `aadt-pdf` APIs later (no second reader).

## Consequences

- Optional native binary beside the exe (or PATH).
- CI without `pdfium.dll` still builds; PDF-open tests skip or use synthetic RGBA.
- Plot writer stays hand-rolled (no PDFium dependency for export).

## Alternatives considered

- **MuPDF:** similar native cost; PDFium chosen for Chromium parity + binaries.
- **Pure Rust PDF:** incomplete for robust page rasterization today.
- **Always-link PDFium:** rejected — breaks hermetic default builds.
