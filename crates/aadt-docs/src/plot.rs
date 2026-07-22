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
        }
    }
    pts
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

/// Compose with an explicit orientation (used by tests and callers that already decided).
pub fn compose_plot_sheet_oriented(
    model: &Document,
    paper: PaperSize,
    tb: &TitleBlock,
    orientation: Orientation,
) -> (Document, Sheet, Composed) {
    let sheet = Sheet::new(paper, orientation, 10.0);
    let mut doc = Document::new();
    doc.set_properties(model.properties().clone());
    sheet.place(&mut doc);

    let area = drawing_area_for_preset(&sheet, tb.preset);
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
        let mut tb = TitleBlock::default();
        tb.project = "Test".into();
        tb.sheet_title = "Plan".into();
        tb.drawing_number = "A-101".into();
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
        let mut tb = TitleBlock::default();
        tb.project = "Wide".into();
        tb.drawing_number = "A-201".into();
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
            Geometry::Line(Segment::new(Point2::new(100.0, 0.0), Point2::new(100.0, 50.0))),
        );
        assert_eq!(
            orientation_from_selection(&model, &[id]),
            Orientation::Portrait
        );
        assert_eq!(orientation_from_extents(&model), Orientation::Landscape);
    }
}
