//! Dimension prediction: find walls that carry no dimension and propose one.
//!
//! Deterministic throughout — detection order fixes suggestion order, so the
//! same drawing always yields the same proposals with the same ids. Nothing is
//! added to the drawing here; each wall produces a [`Suggestion`] carrying a
//! fully-formed [`Dimension`] that [`crate::apply`] can commit later.

use aadt_detect::{detect, DetectionConfig, Wall};
use aadt_geometry::Point2;
use aadt_model::{Dimension, DimensionKind, Document, ModelSpaceId};

use crate::{Action, SuggestionKind, SuggestionSet};

/// Layer new dimension annotations are proposed on (AIA/NCS annotation layer).
pub const DIM_LAYER: &str = "A-ANNO-DIMS";

/// How far off a wall's centerline the dimension line is offered, in metres.
const OFFSET: f64 = 0.5;

/// A dimension counts as "covering" a wall when both its definition points sit
/// within this distance of the wall's endpoints, in metres. Generous enough to
/// tolerate the offset a drafter puts between wall and dimension line.
const COVER_TOL: f64 = 0.75;

/// Walls shorter than this are not worth dimensioning (metres).
const MIN_LENGTH: f64 = 0.5;

/// Propose a dimension for every detected wall that has none.
///
/// Confidence reflects how cleanly the wall was detected: a wall whose two faces
/// are parallel and equal-length reads unambiguously, a ragged one less so.
pub fn suggest_dimensions(doc: &Document) -> SuggestionSet {
    let det = detect(doc, &DetectionConfig::default());
    let mut set = SuggestionSet::new();

    for wall in &det.walls {
        if wall.length < MIN_LENGTH {
            continue;
        }
        if is_dimensioned(doc, wall) {
            continue;
        }
        let dim = dimension_for(wall);
        let confidence = wall_confidence(wall);
        set.push(
            SuggestionKind::Dimension,
            confidence,
            format!(
                "dimension the {:.3} m wall on {}",
                wall.length, wall.layer
            ),
            format!(
                "wall #{} runs {:.3} m from ({:.3}, {:.3}) to ({:.3}, {:.3}) and carries no dimension",
                wall.id,
                wall.length,
                wall.centerline.a.x,
                wall.centerline.a.y,
                wall.centerline.b.x,
                wall.centerline.b.y,
            ),
            vec![wall.faces.0, wall.faces.1],
            Action::AddDimension(Box::new(dim)),
        );
    }
    set
}

/// Whether some existing dimension already measures this wall.
fn is_dimensioned(doc: &Document, wall: &Wall) -> bool {
    let (a, b) = (wall.centerline.a, wall.centerline.b);
    doc.dimensions().iter().any(|d| {
        // Either orientation counts as covering the wall.
        (near(d.p1, a) && near(d.p2, b)) || (near(d.p1, b) && near(d.p2, a))
    })
}

fn near(p: Point2, q: Point2) -> bool {
    let dx = p.x - q.x;
    let dy = p.y - q.y;
    (dx * dx + dy * dy).sqrt() <= COVER_TOL
}

/// Build the linear dimension for a wall, offset to one side of its centerline.
fn dimension_for(wall: &Wall) -> Dimension {
    let (a, b) = (wall.centerline.a, wall.centerline.b);
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = (dx * dx + dy * dy).sqrt();
    // Unit normal to the wall; degenerate walls are filtered out by MIN_LENGTH,
    // but guard anyway so this never divides by zero.
    let (nx, ny) = if len > 1e-9 {
        (-dy / len, dx / len)
    } else {
        (0.0, 1.0)
    };
    let mid = Point2::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
    Dimension {
        layer: DIM_LAYER.to_string(),
        space: ModelSpaceId(0),
        kind: DimensionKind::Linear,
        p1: a,
        p2: b,
        vertex: a,
        text_anchor: Point2::new(mid.x + nx * OFFSET, mid.y + ny * OFFSET),
        text: String::new(),
        measurement: wall.length,
    }
}

/// Confidence that this wall is a real, well-formed wall worth dimensioning.
///
/// Starts high — the wall came out of deterministic detection — and is reduced
/// for geometry that reads ambiguously: an implausible thickness, or a very
/// short run where face-pairing is easy to get wrong.
fn wall_confidence(wall: &Wall) -> f32 {
    let mut c: f32 = 0.95;
    // Typical masonry/partition thicknesses sit between 75 mm and 500 mm.
    if !(0.075..=0.5).contains(&wall.thickness) {
        c -= 0.25;
    }
    // Short walls are more often detection artefacts than real runs.
    if wall.length < 1.0 {
        c -= 0.15;
    }
    c.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aadt_geometry::Segment;
    use aadt_model::Geometry;

    /// A drawing with one 6 m wall: two parallel faces 230 mm apart.
    fn wall_doc() -> Document {
        let mut doc = Document::new();
        doc.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(6.0, 0.0))),
        );
        doc.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.23), Point2::new(6.0, 0.23))),
        );
        doc
    }

    #[test]
    fn undimensioned_wall_gets_a_suggestion() {
        let doc = wall_doc();
        let set = suggest_dimensions(&doc);
        assert_eq!(set.len(), 1, "one wall, one suggestion");
        let s = set.get(1).unwrap();
        assert_eq!(s.kind, SuggestionKind::Dimension);
        assert!(s.confidence > 0.9, "a clean wall reads confidently");
        assert!(s.action.is_applicable());
        match &s.action {
            Action::AddDimension(d) => {
                assert_eq!(d.layer, DIM_LAYER);
                assert!(
                    (d.measurement - 6.0).abs() < 0.01,
                    "measures the wall length, got {}",
                    d.measurement
                );
            }
            other => panic!("expected AddDimension, got {other:?}"),
        }
    }

    #[test]
    fn suggestions_are_deterministic() {
        let doc = wall_doc();
        let a = suggest_dimensions(&doc);
        let b = suggest_dimensions(&doc);
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x.id, y.id);
            assert_eq!(x.summary, y.summary);
            assert_eq!(x.action, y.action);
        }
    }

    #[test]
    fn an_already_dimensioned_wall_is_skipped() {
        let mut doc = wall_doc();
        // Accept the proposal, then re-run: the wall must not be proposed again.
        let set = suggest_dimensions(&doc);
        let Action::AddDimension(dim) = set.get(1).unwrap().action.clone() else {
            panic!("expected a dimension action");
        };
        doc.add_dimension(*dim);

        let again = suggest_dimensions(&doc);
        assert!(
            again.is_empty(),
            "a dimensioned wall should not be re-proposed, got {} suggestion(s)",
            again.len()
        );
    }

    #[test]
    fn empty_drawing_yields_nothing() {
        assert!(suggest_dimensions(&Document::new()).is_empty());
    }

    #[test]
    fn dimension_text_anchor_sits_off_the_centerline() {
        let doc = wall_doc();
        let set = suggest_dimensions(&doc);
        let Action::AddDimension(d) = set.get(1).unwrap().action.clone() else {
            panic!("expected a dimension action");
        };
        // The wall runs along y ≈ 0.115; the anchor must be offset from it.
        let mid_y = (d.p1.y + d.p2.y) / 2.0;
        assert!(
            (d.text_anchor.y - mid_y).abs() > 0.1,
            "anchor should be offset from the wall line"
        );
    }
}
