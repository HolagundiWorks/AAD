//! Sheet composition — place the drawing into the sheet's drawing area at scale.
//!
//! The sheet furniture ([`sheet`](crate::sheet) border, [`titleblock`](crate::titleblock),
//! [`notes`](crate::notes)) lives in millimetres of paper. The model drawing is
//! in its own units (metres, feet, …), so it must be **scaled into paper space**
//! to sit correctly on the sheet — otherwise a 10-metre wall drawn at 10 units
//! lands as a 10-millimetre speck. [`place_drawing`] does that: it fits the
//! model's extents into a drawing area at a single uniform scale (scale-to-fit),
//! reporting the scale it used so the title block can record it.

use aadt_geometry::{Arc, Bbox, Circle, Point2, Polyline, Segment};
use aadt_model::{Document, EntityId, Geometry, Insert, Text};

use crate::sheet::Sheet;

/// A rectangular region of a sheet, in paper millimetres.
#[derive(Debug, Clone, Copy)]
pub struct DrawingArea {
    pub min: Point2,
    pub max: Point2,
}

impl DrawingArea {
    fn width(&self) -> f64 {
        self.max.x - self.min.x
    }
    fn height(&self) -> f64 {
        self.max.y - self.min.y
    }
    fn center(&self) -> Point2 {
        self.min.midpoint(self.max)
    }
}

impl Sheet {
    /// The area a drawing occupies on this sheet: the border interior with a
    /// strip reserved at the bottom for the title block.
    pub fn drawing_area(&self, reserve_bottom: f64) -> DrawingArea {
        let (min, max) = self.inner();
        DrawingArea {
            min: Point2::new(min.x, (min.y + reserve_bottom).min(max.y)),
            max,
        }
    }
}

/// The outcome of composing a drawing onto a sheet.
#[derive(Debug, Clone)]
pub struct Composed {
    /// Paper millimetres per model unit — the scale factor applied.
    pub scale: f64,
    pub ids: Vec<EntityId>,
}

impl Composed {
    /// The drawing scale as a `1:N` label, given how many model units make one
    /// millimetre in the real world (`model_units_per_mm`) — e.g. `1000.0` for a
    /// model drawn in metres. Returns `"NTS"` (not to scale) when the drawing is
    /// empty.
    pub fn scale_label(&self, model_units_per_mm: f64) -> String {
        if self.scale <= 0.0 {
            return "NTS".to_string();
        }
        // paper_mm = model_unit * scale; 1 model_unit = model_units_per_mm real mm.
        // So real_mm : paper_mm = model_units_per_mm : scale.
        let n = model_units_per_mm / self.scale;
        format!("1:{}", n.round() as i64)
    }
}

/// Place `model`'s geometry into `sheet_doc`, scaled to fit `area` and centred,
/// as one undoable unit (`DRAWING`). Returns the fit scale and the placed ids.
/// An empty model places nothing and reports scale `0`.
pub fn place_drawing(sheet_doc: &mut Document, model: &Document, area: &DrawingArea) -> Composed {
    let Some(bbox) = Bbox::of_points(model.points()) else {
        return Composed {
            scale: 0.0,
            ids: Vec::new(),
        };
    };
    let (bw, bh) = (bbox.width().max(1e-9), bbox.height().max(1e-9));
    let scale = (area.width() / bw).min(area.height() / bh).max(0.0);
    let src_center = bbox.min.midpoint(bbox.max);
    let dst_center = area.center();

    // Model point → paper point: centre the model in the area at `scale`.
    let map = |p: Point2| {
        Point2::new(
            dst_center.x + (p.x - src_center.x) * scale,
            dst_center.y + (p.y - src_center.y) * scale,
        )
    };

    let ids = sheet_doc.transact("DRAWING", |d| {
        let mut ids = Vec::new();
        for e in model.entities() {
            ids.push(d.add_entity(
                e.layer.clone(),
                transform(&e.geometry, &map, scale),
                e.attributes.clone(),
            ));
        }
        ids
    });

    Composed { scale, ids }
}

/// Map a geometry through a point transform, scaling sizes (radii, text height)
/// by `scale`. Rotations and arc angles are preserved (a uniform scale does not
/// rotate). An `Insert`'s referenced block is not itself scaled — only its
/// insertion point moves — since [`Insert`] carries no scale factor; that is the
/// same limitation the renderer's instancing has.
fn transform(geom: &Geometry, map: &impl Fn(Point2) -> Point2, scale: f64) -> Geometry {
    match geom {
        Geometry::Line(s) => Geometry::Line(Segment::new(map(s.a), map(s.b))),
        Geometry::Polyline(p) => Geometry::Polyline(Polyline::new(
            p.vertices.iter().map(|v| map(*v)).collect(),
            p.closed,
        )),
        Geometry::Circle(c) => Geometry::Circle(Circle::new(map(c.center), c.radius * scale)),
        Geometry::Arc(a) => Geometry::Arc(Arc::new(
            map(a.center),
            a.radius * scale,
            a.start_angle,
            a.end_angle,
        )),
        Geometry::Text(t) => Geometry::Text(Text::new(
            t.value.clone(),
            map(t.position),
            t.height * scale,
            t.rotation,
        )),
        Geometry::Insert(i) => {
            Geometry::Insert(Insert::new(i.name.clone(), map(i.position), i.rotation))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sheet::{Orientation, PaperSize};

    fn model_10x8() -> Document {
        let mut doc = Document::new();
        // A 10 × 8 rectangle of walls, in "metres".
        doc.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0))),
        );
        doc.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(Point2::new(10.0, 0.0), Point2::new(10.0, 8.0))),
        );
        doc.add_geometry(
            "S-COLS",
            Geometry::Circle(Circle::new(Point2::new(5.0, 4.0), 0.5)),
        );
        doc
    }

    #[test]
    fn drawing_is_scaled_into_the_area_and_centred() {
        let sheet = Sheet::new(PaperSize::IsoA3, Orientation::Landscape, 10.0);
        let area = sheet.drawing_area(60.0);
        let mut sheet_doc = Document::new();
        let composed = place_drawing(&mut sheet_doc, &model_10x8(), &area);

        assert!(composed.scale > 0.0);
        assert_eq!(sheet_doc.len(), 3, "all three model entities placed");

        // Every placed point sits inside the drawing area.
        for p in sheet_doc.points() {
            assert!(
                p.x >= area.min.x - 1e-6 && p.x <= area.max.x + 1e-6,
                "x in area"
            );
            assert!(
                p.y >= area.min.y - 1e-6 && p.y <= area.max.y + 1e-6,
                "y in area"
            );
        }
    }

    #[test]
    fn scale_makes_the_drawing_fill_one_dimension_of_the_area() {
        let sheet = Sheet::new(PaperSize::IsoA1, Orientation::Landscape, 10.0);
        let area = sheet.drawing_area(60.0);
        let mut sheet_doc = Document::new();
        place_drawing(&mut sheet_doc, &model_10x8(), &area);

        // The 10×8 model has aspect 1.25; the drawing area is wider, so height
        // is the binding constraint: scaled height ≈ area height.
        let bbox = Bbox::of_points(sheet_doc.points()).unwrap();
        let filled = (bbox.height() / area.height()).max(bbox.width() / area.width());
        assert!(
            filled > 0.99,
            "fit touches one edge of the area, got {filled}"
        );
    }

    #[test]
    fn a_circle_radius_scales_with_the_drawing() {
        let sheet = Sheet::new(PaperSize::IsoA3, Orientation::Landscape, 10.0);
        let area = sheet.drawing_area(60.0);
        let mut sheet_doc = Document::new();
        let composed = place_drawing(&mut sheet_doc, &model_10x8(), &area);

        let circle = sheet_doc
            .entities()
            .into_iter()
            .find_map(|e| match &e.geometry {
                Geometry::Circle(c) => Some(*c),
                _ => None,
            })
            .expect("the column circle");
        assert!((circle.radius - 0.5 * composed.scale).abs() < 1e-6);
    }

    #[test]
    fn placement_is_one_undoable_unit() {
        let sheet = Sheet::new(PaperSize::IsoA3, Orientation::Landscape, 10.0);
        let area = sheet.drawing_area(60.0);
        let mut sheet_doc = Document::new();
        place_drawing(&mut sheet_doc, &model_10x8(), &area);
        assert_eq!(sheet_doc.undo_label(), Some("DRAWING"));
        sheet_doc.undo();
        assert_eq!(sheet_doc.len(), 0);
    }

    #[test]
    fn an_empty_model_places_nothing() {
        let sheet = Sheet::a1_landscape();
        let area = sheet.drawing_area(60.0);
        let mut sheet_doc = Document::new();
        let composed = place_drawing(&mut sheet_doc, &Document::new(), &area);
        assert_eq!(composed.scale, 0.0);
        assert_eq!(sheet_doc.len(), 0);
    }

    #[test]
    fn scale_label_reads_as_a_ratio() {
        // A model in metres: 1000 model-mm per paper-mm baseline. If scale places
        // 1 metre as 10 mm of paper (scale = 10 paper-mm per model-unit, model
        // unit = metre = 1000 mm), the ratio is 1000/10 = 1:100.
        let composed = Composed {
            scale: 10.0,
            ids: vec![],
        };
        assert_eq!(composed.scale_label(1000.0), "1:100");
    }
}
