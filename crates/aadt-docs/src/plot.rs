//! Plot composition — border + scaled drawing + title plate, ready for PDF.
//!
//! Always plots to PDF (no direct print). Sheet orientation **and** title-block
//! layout are chosen from the drawing (or selection) extents:
//! - wide → landscape sheet + vertical title strip on the right
//! - tall → portrait sheet + horizontal title band along the bottom

use aadt_geometry::{Bbox, Point2};
use aadt_model::{Document, EntityId, Geometry};

use crate::compose::{place_drawing, Composed, DrawingArea};
use crate::sheet::{Orientation, PaperSize, Sheet};
use crate::titleblock::{NameplatePreset, TitleBlock, TitleBlockLayout, STRIP_GAP_MM};
use crate::view_preset::{document_with_view_preset, ViewPreset};

/// Choose sheet + title-block orientation from a bounding box.
/// Taller than wide → portrait (bottom title band); otherwise landscape (right strip).
pub fn orientation_from_bbox(b: Bbox) -> Orientation {
    if b.height() > b.width() * 1.05 {
        Orientation::Portrait
    } else {
        Orientation::Landscape
    }
}

/// Choose orientation from an arbitrary point set (e.g. selection or whole model).
pub fn orientation_from_points(pts: &[Point2]) -> Orientation {
    match Bbox::of_points(pts.iter().copied()) {
        Some(b) => orientation_from_bbox(b),
        None => Orientation::Landscape,
    }
}

/// Choose sheet orientation from the model's full extents.
pub fn orientation_from_extents(model: &Document) -> Orientation {
    orientation_from_points(&model.points())
}

/// Orientation from a selection when non-empty; otherwise the whole drawing.
pub fn orientation_from_selection(model: &Document, selection: &[EntityId]) -> Orientation {
    if selection.is_empty() {
        return orientation_from_extents(model);
    }
    let pts = points_of_entities(model, selection);
    if pts.is_empty() {
        orientation_from_extents(model)
    } else {
        orientation_from_points(&pts)
    }
}

fn points_of_entities(model: &Document, ids: &[EntityId]) -> Vec<Point2> {
    let mut pts = Vec::new();
    for id in ids {
        let Some(e) = model.get(*id) else { continue };
        match &e.geometry {
            Geometry::Line(s) => {
                pts.push(s.a);
                pts.push(s.b);
            }
            Geometry::Polyline(p) => pts.extend(p.vertices.iter().copied()),
            Geometry::Circle(c) => {
                pts.push(Point2::new(c.center.x - c.radius, c.center.y));
                pts.push(Point2::new(c.center.x + c.radius, c.center.y));
                pts.push(Point2::new(c.center.x, c.center.y - c.radius));
                pts.push(Point2::new(c.center.x, c.center.y + c.radius));
            }
            Geometry::Arc(a) => pts.extend(a.to_polyline(16).vertices),
            Geometry::Text(t) => pts.push(t.position),
            Geometry::Insert(ins) => pts.push(ins.position),
            Geometry::Image(img) => pts.extend(img.corners()),
        }
    }
    pts
}

fn geom_bbox(g: &Geometry) -> Option<Bbox> {
    let pts: Vec<Point2> = match g {
        Geometry::Line(s) => vec![s.a, s.b],
        Geometry::Polyline(p) => p.vertices.clone(),
        Geometry::Circle(c) => vec![
            Point2::new(c.center.x - c.radius, c.center.y - c.radius),
            Point2::new(c.center.x + c.radius, c.center.y + c.radius),
        ],
        Geometry::Arc(a) => a.to_polyline(16).vertices,
        Geometry::Text(t) => vec![t.position],
        Geometry::Insert(ins) => vec![ins.position],
        Geometry::Image(img) => img.corners().to_vec(),
    };
    Bbox::of_points(pts)
}

/// Copy layers/blocks from `src` into `dst`.
fn copy_tables(dst: &mut Document, src: &Document) {
    dst.set_properties(src.properties().clone());
    for layer in src.layers() {
        if dst.layer(&layer.name).is_none() {
            dst.add_layer(layer.clone());
        }
    }
    for block in src.blocks() {
        if dst.block(&block.name).is_none() {
            dst.add_block(block.clone());
        }
    }
}

/// Sub-document containing only entities / hatches / dims that intersect `window`.
pub fn document_in_window(model: &Document, window: Bbox) -> Document {
    let mut out = Document::new();
    copy_tables(&mut out, model);
    for e in model.entities() {
        let Some(bb) = geom_bbox(&e.geometry) else {
            continue;
        };
        if window.intersects(bb) {
            out.add_entity(e.layer.clone(), e.geometry.clone(), e.attributes.clone());
        }
    }
    for h in model.hatches() {
        if let Some(bb) = Bbox::of_points(h.outer().vertices.iter().copied()) {
            if window.intersects(bb) {
                out.add_hatch(h.clone());
            }
        }
    }
    for d in model.dimensions() {
        if let Some(bb) = Bbox::of_points([d.p1, d.p2, d.vertex, d.text_anchor]) {
            if window.intersects(bb) {
                out.add_dimension(d.clone());
            }
        }
    }
    out
}

/// Sub-document of the selected entities (plus hatches/dims whose boxes meet them).
pub fn document_from_selection(model: &Document, selection: &[EntityId]) -> Document {
    if selection.is_empty() {
        return model.clone();
    }
    let pts = points_of_entities(model, selection);
    if let Some(bb) = Bbox::of_points(pts) {
        // Pad slightly so nearby hatches that frame the selection still plot.
        let pad = (bb.width().max(bb.height()) * 0.02).max(1e-6);
        let window = Bbox {
            min: Point2::new(bb.min.x - pad, bb.min.y - pad),
            max: Point2::new(bb.max.x + pad, bb.max.y + pad),
        };
        let mut out = Document::new();
        copy_tables(&mut out, model);
        for id in selection {
            if let Some(e) = model.get(*id) {
                out.add_entity(e.layer.clone(), e.geometry.clone(), e.attributes.clone());
            }
        }
        for h in model.hatches() {
            if let Some(hbb) = Bbox::of_points(h.outer().vertices.iter().copied()) {
                if window.intersects(hbb) {
                    out.add_hatch(h.clone());
                }
            }
        }
        for d in model.dimensions() {
            if let Some(dbb) = Bbox::of_points([d.p1, d.p2, d.vertex, d.text_anchor]) {
                if window.intersects(dbb) {
                    out.add_dimension(d.clone());
                }
            }
        }
        out
    } else {
        model.clone()
    }
}

/// Drawing area with the title-block reservation matching sheet orientation.
pub fn drawing_area_for_sheet(sheet: &Sheet) -> DrawingArea {
    drawing_area_for_preset(sheet, NameplatePreset::Professional)
}

/// Drawing area reserved beside/above the nameplate for `preset`.
pub fn drawing_area_for_preset(sheet: &Sheet, preset: NameplatePreset) -> DrawingArea {
    let layout = TitleBlockLayout::for_orientation(sheet.orientation);
    match layout {
        TitleBlockLayout::VerticalRight => {
            let w = TitleBlock::strip_width_mm_for(sheet, preset);
            sheet.drawing_area(w + STRIP_GAP_MM)
        }
        TitleBlockLayout::HorizontalBottom => {
            let h = TitleBlock::strip_height_mm_for(sheet, preset);
            sheet.drawing_area_bottom(h + STRIP_GAP_MM)
        }
    }
}

/// Compose a paper-space sheet: border, fit drawing, title block.
///
/// Sheet orientation and title-block layout are derived from model extents
/// (landscape → right vertical strip; portrait → bottom horizontal band).
pub fn compose_plot_sheet(
    model: &Document,
    paper: PaperSize,
    tb: &TitleBlock,
) -> (Document, Sheet, Composed) {
    compose_plot_sheet_oriented(model, paper, tb, orientation_from_extents(model))
}

/// Same as [`compose_plot_sheet`], but orientation follows `selection` when set.
/// Content is still the full model — prefer [`compose_plot_sheet_for_area`].
pub fn compose_plot_sheet_for_selection(
    model: &Document,
    paper: PaperSize,
    tb: &TitleBlock,
    selection: &[EntityId],
) -> (Document, Sheet, Composed) {
    compose_plot_sheet_oriented(
        model,
        paper,
        tb,
        orientation_from_selection(model, selection),
    )
}

/// What portion of the model to place on the sheet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlotContent<'a> {
    /// Entire drawing extents.
    Extents,
    /// Selected entities (falls back to extents when empty).
    Selection(&'a [EntityId]),
    /// Explicit world-space window (two-corner pick).
    Window(Bbox),
}

/// Compose a plot sheet using only the chosen content area for fit + geometry.
pub fn compose_plot_sheet_for_area(
    model: &Document,
    paper: PaperSize,
    tb: &TitleBlock,
    content: PlotContent<'_>,
) -> (Document, Sheet, Composed) {
    compose_plot_sheet_for_area_with_preset(model, paper, tb, content, None)
}

/// [`compose_plot_sheet_for_area`] with an optional sheet-bound [`ViewPreset`] (L6).
pub fn compose_plot_sheet_for_area_with_preset(
    model: &Document,
    paper: PaperSize,
    tb: &TitleBlock,
    content: PlotContent<'_>,
    preset: Option<ViewPreset>,
) -> (Document, Sheet, Composed) {
    let (source, orient) = match content {
        PlotContent::Extents => (model.clone(), orientation_from_extents(model)),
        PlotContent::Selection(ids) => {
            let src = document_from_selection(model, ids);
            let orient = orientation_from_selection(model, ids);
            (src, orient)
        }
        PlotContent::Window(window) => {
            let src = document_in_window(model, window);
            let orient = orientation_from_bbox(window);
            (src, orient)
        }
    };
    compose_plot_sheet_oriented_with_preset(&source, paper, tb, orient, preset)
}

/// Fraction of the drawing area reserved as margin on **each** side inside the
/// sheet border (and outside the title-block reservation). Plot composition
/// centres the model in the remaining rectangle.
pub const PLOT_DRAWING_MARGIN: f64 = 0.05;

/// Compose with an explicit orientation (used by tests and callers that already decided).
pub fn compose_plot_sheet_oriented(
    model: &Document,
    paper: PaperSize,
    tb: &TitleBlock,
    orientation: Orientation,
) -> (Document, Sheet, Composed) {
    compose_plot_sheet_oriented_with_preset(model, paper, tb, orientation, None)
}

/// Like [`compose_plot_sheet_oriented`], but optionally applies a [`ViewPreset`] to a
/// **clone** of the model so sheet output does not mutate live layer visibility (L6).
pub fn compose_plot_sheet_oriented_with_preset(
    model: &Document,
    paper: PaperSize,
    tb: &TitleBlock,
    orientation: Orientation,
    preset: Option<ViewPreset>,
) -> (Document, Sheet, Composed) {
    let owned;
    let model = if let Some(p) = preset {
        owned = document_with_view_preset(model, p);
        &owned
    } else {
        model
    };
    let sheet = Sheet::new(paper, orientation, 10.0);
    let mut doc = Document::new();
    doc.set_properties(model.properties().clone());
    sheet.place(&mut doc);

    // Fit inside the border with 5% clearance on all sides, centred in that pad.
    let area = drawing_area_for_preset(&sheet, tb.preset).inset_fraction(PLOT_DRAWING_MARGIN);
    let composed = place_drawing(&mut doc, model, &area);

    let mut filled = tb.clone();
    if filled.scale.trim().is_empty() {
        filled.scale = composed.scale_label(1000.0);
    }
    filled.place_on_sheet(&mut doc, &sheet);

    (doc, sheet, composed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aadt_geometry::Segment;
    use aadt_model::Geometry;

    fn tall_model() -> Document {
        let mut d = Document::new();
        d.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(1.0, 0.0))),
        );
        d.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(0.0, 10.0))),
        );
        d
    }

    fn wide_model() -> Document {
        let mut d = Document::new();
        d.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0))),
        );
        d.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(0.0, 1.0))),
        );
        d
    }

    #[test]
    fn tall_drawing_gets_portrait() {
        assert_eq!(
            orientation_from_extents(&tall_model()),
            Orientation::Portrait
        );
    }

    #[test]
    fn wide_drawing_gets_landscape() {
        assert_eq!(
            orientation_from_extents(&wide_model()),
            Orientation::Landscape
        );
    }

    #[test]
    fn compose_portrait_uses_bottom_title_band() {
        let model = tall_model();
        let tb = TitleBlock {
            project: "Test".into(),
            sheet_title: "Plan".into(),
            drawing_number: "A-101".into(),
            ..Default::default()
        };
        let (sheet_doc, sheet, _) = compose_plot_sheet(&model, PaperSize::IsoA3, &tb);
        assert_eq!(sheet.orientation, Orientation::Portrait);
        let (min, _max) = sheet.inner();
        let strip_h = TitleBlock::strip_height_mm(&sheet);
        assert!(sheet_doc.entities().into_iter().any(|e| matches!(
            &e.geometry,
            Geometry::Text(t) if t.value == "A-101" && t.position.y <= min.y + strip_h + 1.0
        )));
    }

    #[test]
    fn compose_landscape_uses_right_title_strip() {
        let model = wide_model();
        let tb = TitleBlock {
            project: "Wide".into(),
            drawing_number: "A-201".into(),
            ..Default::default()
        };
        let (sheet_doc, sheet, _) = compose_plot_sheet(&model, PaperSize::IsoA3, &tb);
        assert_eq!(sheet.orientation, Orientation::Landscape);
        let (_min, max) = sheet.inner();
        let strip_w = TitleBlock::strip_width_mm(&sheet);
        assert!(sheet_doc.entities().into_iter().any(|e| matches!(
            &e.geometry,
            Geometry::Text(t) if t.value == "A-201" && t.position.x >= max.x - strip_w - 1.0
        )));
    }

    #[test]
    fn selection_overrides_orientation() {
        let mut model = wide_model();
        let id = model.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(
                Point2::new(100.0, 0.0),
                Point2::new(100.0, 50.0),
            )),
        );
        assert_eq!(
            orientation_from_selection(&model, &[id]),
            Orientation::Portrait
        );
        assert_eq!(orientation_from_extents(&model), Orientation::Landscape);
    }

    #[test]
    fn plot_drawing_is_centred_with_five_percent_margin_inside_border() {
        let model = wide_model();
        let tb = TitleBlock::default();
        let (sheet_doc, sheet, _) = compose_plot_sheet(&model, PaperSize::IsoA3, &tb);

        let full = drawing_area_for_preset(&sheet, tb.preset);
        let padded = full.inset_fraction(PLOT_DRAWING_MARGIN);

        // Model geometry (A-WALL) must sit inside the 5%-inset area.
        let wall_pts: Vec<_> = sheet_doc
            .entities()
            .into_iter()
            .filter(|e| e.layer == "A-WALL")
            .flat_map(|e| match &e.geometry {
                Geometry::Line(s) => vec![s.a, s.b],
                _ => Vec::new(),
            })
            .collect();
        assert!(!wall_pts.is_empty());
        for p in &wall_pts {
            assert!(
                p.x >= padded.min.x - 1e-6 && p.x <= padded.max.x + 1e-6,
                "x inside 5% pad"
            );
            assert!(
                p.y >= padded.min.y - 1e-6 && p.y <= padded.max.y + 1e-6,
                "y inside 5% pad"
            );
        }

        // Centred: bbox mid ≈ padded area mid (within a mm).
        let bb = Bbox::of_points(wall_pts.iter().copied()).unwrap();
        let mid = bb.min.midpoint(bb.max);
        let area_mid = padded.center();
        assert!(
            (mid.x - area_mid.x).abs() < 1.0 && (mid.y - area_mid.y).abs() < 1.0,
            "drawing centred in padded area: mid={mid:?} area={area_mid:?}"
        );

        // Not jammed to the full drawing-area edge (margin actually used).
        let fill_w = bb.width() / full.width();
        let fill_h = bb.height() / full.height();
        assert!(
            fill_w.max(fill_h) < 0.96,
            "must leave ~5% margin vs full area, fill={}",
            fill_w.max(fill_h)
        );
    }
}
