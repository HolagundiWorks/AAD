//! Endpoint / midpoint object snap — pure geometry over a [`Document`].
//!
//! Used by the egui viewport; kept out of the `gui` feature so it is unit-tested
//! headlessly. Lean set only: **END** and **MID** (no center/quadrant/intersection).

use aadt_geometry::Point2;
use aadt_model::{Document, Geometry};

/// Kind of object snap acquired under the cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapKind {
    Endpoint,
    Midpoint,
}

/// Endpoint and midpoint candidates for object snap on one geometry.
pub fn snap_candidates(g: &Geometry) -> Vec<(SnapKind, Point2)> {
    match g {
        Geometry::Line(s) => vec![
            (SnapKind::Endpoint, s.a),
            (SnapKind::Endpoint, s.b),
            (SnapKind::Midpoint, s.midpoint()),
        ],
        Geometry::Polyline(p) => {
            let mut out = Vec::new();
            for v in &p.vertices {
                out.push((SnapKind::Endpoint, *v));
            }
            for w in p.vertices.windows(2) {
                out.push((SnapKind::Midpoint, w[0].midpoint(w[1])));
            }
            if p.closed && p.vertices.len() >= 2 {
                let a = p.vertices[p.vertices.len() - 1];
                let b = p.vertices[0];
                out.push((SnapKind::Midpoint, a.midpoint(b)));
            }
            out
        }
        Geometry::Arc(a) => vec![
            (SnapKind::Endpoint, a.start_point()),
            (SnapKind::Endpoint, a.end_point()),
            (SnapKind::Midpoint, a.point_at(0.5)),
        ],
        // Circles have no endpoints; keep the lean set free of CEN/QUA.
        Geometry::Circle(_) | Geometry::Text(_) | Geometry::Insert(_) => Vec::new(),
    }
}

/// Nearest endpoint/midpoint within `tol` of `p`. Endpoint beats midpoint on a
/// distance tie so corners win over segment centres.
pub fn find_osnap(doc: &Document, p: Point2, tol: f64) -> Option<(SnapKind, Point2)> {
    let mut best: Option<(f64, u8, SnapKind, Point2)> = None;
    for e in doc.entities() {
        for (kind, sp) in snap_candidates(&e.geometry) {
            let d = p.distance(sp);
            if d > tol {
                continue;
            }
            let rank = match kind {
                SnapKind::Endpoint => 0u8,
                SnapKind::Midpoint => 1u8,
            };
            let better = match best {
                None => true,
                Some((bd, br, _, _)) => {
                    d < bd - f64::EPSILON || ((d - bd).abs() <= f64::EPSILON && rank < br)
                }
            };
            if better {
                best = Some((d, rank, kind, sp));
            }
        }
    }
    best.map(|(_, _, k, sp)| (k, sp))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aadt_geometry::{Arc, Point2, Polyline, Segment};
    use aadt_model::Document;

    #[test]
    fn line_offers_ends_and_mid() {
        let g = Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0)));
        let c = snap_candidates(&g);
        assert_eq!(c.len(), 3);
        assert!(c
            .iter()
            .any(|(k, p)| *k == SnapKind::Midpoint && (p.x - 5.0).abs() < 1e-9));
    }

    #[test]
    fn finds_endpoint_within_tolerance() {
        let mut doc = Document::new();
        doc.transact("t", |d| {
            d.add_geometry(
                "0",
                Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0))),
            );
        });
        let hit = find_osnap(&doc, Point2::new(0.2, 0.1), 0.5).unwrap();
        assert_eq!(hit.0, SnapKind::Endpoint);
        assert!((hit.1.x).abs() < 1e-9 && (hit.1.y).abs() < 1e-9);
    }

    #[test]
    fn prefers_endpoint_over_midpoint_on_a_tie() {
        // Cursor equidistant from mid (5,0) and would need equal distance — place
        // at a point closer to an end than mid.
        let mut doc = Document::new();
        doc.transact("t", |d| {
            d.add_geometry(
                "0",
                Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0))),
            );
        });
        let hit = find_osnap(&doc, Point2::new(5.0, 0.05), 1.0).unwrap();
        assert_eq!(hit.0, SnapKind::Midpoint);
        let hit_end = find_osnap(&doc, Point2::new(0.05, 0.05), 1.0).unwrap();
        assert_eq!(hit_end.0, SnapKind::Endpoint);
    }

    #[test]
    fn closed_polyline_has_closing_midpoint() {
        let g = Geometry::Polyline(Polyline::new(
            vec![
                Point2::new(0.0, 0.0),
                Point2::new(4.0, 0.0),
                Point2::new(4.0, 3.0),
            ],
            true,
        ));
        let mids: Vec<_> = snap_candidates(&g)
            .into_iter()
            .filter(|(k, _)| *k == SnapKind::Midpoint)
            .collect();
        assert_eq!(mids.len(), 3, "two edges + closing edge");
    }

    #[test]
    fn arc_midpoint_is_halfway_along_sweep() {
        let a = Arc::new(Point2::new(0.0, 0.0), 10.0, 0.0, 90.0);
        let g = Geometry::Arc(a);
        let mid = snap_candidates(&g)
            .into_iter()
            .find(|(k, _)| *k == SnapKind::Midpoint)
            .unwrap()
            .1;
        let expect = a.point_at(0.5);
        assert!((mid.x - expect.x).abs() < 1e-9);
        assert!((mid.y - expect.y).abs() < 1e-9);
    }
}
