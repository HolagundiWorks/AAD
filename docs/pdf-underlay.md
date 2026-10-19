# PDF underlay for take-off

A project whose drawings exist only as PDF can be measured in O-Draft. The PDF page is read, drawn as ordinary
polylines on its own layer, calibrated to a real distance, and measured over with the same `takeoff.*` tools as
any drawing. This is item 3.5 of the O-CoreOp hand-off.

## Design

- **Vector PDFs only.** A plan exported from CAD is a PDF of vector paths. The reader (`crates/io/src/pdfread.rs`,
  `lopdf`, MIT, native builds only) interprets the page's path operators (`m l c v y re h`, `S s f F f* B b`,
  `q Q cm`, colour and width), including form XObjects (nesting capped at 8), and returns polylines in PDF points
  with the page origin at the lower left and the page rotation applied. Curves become 8 straight segments.
- **No new entity type.** Each path becomes an `LwPolyline` on the underlay layer (default `PDF-UNDERLAY`, grey).
  Snaps, selection, plotting and DXF save and reopen work as for any entity, and the take-off tools measure over it
  by drawing on the `MEASURE-*` layers as before. Nothing changes in the doc, DXF or render layers.
- **Not drawn, and said so:** text, images, shadings, clipping. A white path, and a fill covering the page, are
  taken to be the paper and dropped. A page with no vector lines (a scan, or only text) is refused with that reason
  and nothing is added.
- **Untrusted input:** at most 4 million operations, 200,000 paths and 2 million points per page, a 256 MB file
  limit, a 14,400 pt page limit, finite numbers only, encrypted PDFs refused. Errors are results, not panics.
- **The web build has no reader** (its random-number dependency has no web backend); the command then says PDF
  reading is not available.

## Commands (Architecture > Take-off)

| Command | What it does |
|---|---|
| `underlay.attach {path, page?, at?, scale?, layer?}` | Reads the page and draws it. `at` is the lower left of the page (default `[0, 0]`); `scale` is drawing millimetres per paper millimetre (default 1, so the page arrives at paper size). Refuses a layer that already holds an underlay. The result warns that the scale is not known yet |
| `underlay.calibrate {p1, p2, length, unit?, layer?}` | Two points on the underlay a known real distance apart (`unit`: mm, cm, m, in, ft; default mm). Scales every line of the layer about `p1` so the points are that far apart, keeps the record (`scale`, `calibrated`). Refuses the same point twice, a non-positive length, an unknown unit and a factor beyond 1 : 1,000,000 |
| `underlay.list` | The attached underlays: layer, file, page, scale, calibrated, line count |
| `underlay.remove {layer?}` | Deletes the layer's lines and the record |

The attach and calibrate steps are single undo steps. The records are kept in the drawing's project data under
`underlay`.

## Workflow for a PDF-only project

1. Set the drawing units (`drawing.units`), then `underlay.attach` the plan.
2. Pick a dimension printed on the plan, or a known wall length, and `underlay.calibrate` with its two end points.
3. Draw the measure lines over the PDF on `MEASURE-FULLBRICK`, `MEASURE-COLUMN` and the other measure layers
   (osnap picks the PDF's line ends), run the `takeoff.*` tools, then `takeoff.record` as for any drawing.

## Tests

`apps/aad-cli/tests/underlay.rs` (real PDF writer and reader): a plan is plotted to PDF, attached to a fresh
drawing, calibrated from its 9 m edge (the 6 m side follows, scaled about the first point), a measure line over it
exports as a 9000 mm wall, undo restores the scale, removing leaves nothing; bad requests (missing file, page 0 or 2,
negative or huge scale, junk file, same layer twice, zero or negative length, unknown unit) change nothing; a page
with only text is refused with a reason. `crates/io/src/pdfread.rs` unit tests: a hand-made page gives its paths in
points under a transform, bad and truncated input is an error, a form that draws itself ends.

## Not done

- **Scanned (raster) PDFs.** Showing an image as an underlay needs an image entity and a decoder; none exists.
  The command says so instead of drawing nothing.
- Text on the PDF is not drawn, so a dimension printed in it has to be read by eye.
- A dialog or a pick-two-points prompt for calibration in the WinUI shell; today it is the JSON form (the
  menu entries run it with defaults and report what is missing).
- Several pages of one PDF in one step (attach each page on its own layer).
- Not run on PDFs from real CAD programs: only on PDFs this program writes and on hand-made pages.
