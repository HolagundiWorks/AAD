//! Endpoint / midpoint object snap — pure geometry over a [`Document`].
//!
//! Used by the egui viewport; kept out of the `gui` feature so it is unit-tested
//! headlessly. Lean set only: **END** and **MID** (no center/quadrant/intersection).

use std::time::{Duration, Instant};

use aadt_geometry::{Bbox, Point2};
use aadt_model::{Document, Geometry};

/// Kind of object snap acquired under the cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapKind {
    Endpoint,
    Midpoint,
}

/// Reusable buffers + throttle state for hover osnap.
#[derive(Debug, Default)]
pub struct OsnapState {
    scratch: Vec<(SnapKind, Point2)>,
    last_raw: Option<Point2>,
    last_at: Option<Instant>,
    last_hit: Option<(SnapKind, Point2)>,
}

impl OsnapState {
    /// Default throttle: recompute at most every 8 ms unless the pointer moved
    /// more than `move_tol` (world units; pass ~2 screen pixels / camera.scale).
    pub fn resolve(
        &mut self,
        doc: &Document,
        raw: Point2,
        snap_tol: f64,
        move_tol: f64,
        now: Instant,
    ) -> Option<(SnapKind, Point2)> {
        let throttle = Duration::from_millis(8);
        let moved = self
            .last_raw
            .map(|p| p.distance(raw) > move_tol)
            .unwrap_or(true);
        let aged = self
            .last_at
            .map(|t| now.duration_since(t) >= throttle)
            .unwrap_or(true);
        if !moved && !aged {
            return self.last_hit;
        }
        let hit = find_osnap_into(doc, raw, snap_tol, &mut self.scratch);
        self.last_raw = Some(raw);
        self.last_at = Some(now);
        self.last_hit = hit;
        hit
    }

    pub fn clear(&mut self) {
        self.last_raw = None;
        self.last_at = None;
        self.last_hit = None;
        self.scratch.clear();
    }
}

/// Cheap AABB for snap early-out (endpoints / circle bounds). Expanded by `pad`.
fn snap_bbox(g: &Geometry, pad: f64) -> Option<Bbox> {
    let mut b = match g {
        Geometry::Line(s) => Bbox::of_points([s.a, s.b])?,
        Geometry::Polyline(p) => Bbox::of_points(p.vertices.iter().copied())?,
        Geometry::Arc(a) => {
            // Conservative: full circle around center.
            let r = a.radius;
            Bbox {
                min: Point2::new(a.center.x - r, a.center.y - r),
                max: Point2::new(a.center.x + r, a.center.y + r),
            }
        }
        Geometry::Circle(c) => Bbox {
            min: Point2::new(c.center.x - c.radius, c.center.y - c.radius),
            max: Point2::new(c.center.x + c.radius, c.center.y + c.radius),
        },
        Geometry::Text(_) | Geometry::Insert(_) => return None,
    };
    b.min.x -= pad;
    b.min.y -= pad;
    b.max.x += pad;
    b.max.y += pad;
    Some(b)
}

fn bbox_near(b: Bbox, p: Point2) -> bool {
    // `b` is already expanded by the snap aperture.
    b.contains_point(p)
}

/// Append endpoint/midpoint candidates for one geometry into `out` (no alloc).
pub fn snap_candidates_into(g: &Geometry, out: &mut Vec<(SnapKind, Point2)>) {
    match g {
        Geometry::Line(s) => {
            out.push((SnapKind::Endpoint, s.a));
            out.push((SnapKind::Endpoint, s.b));
            out.push((SnapKind::Midpoint, s.midpoint()));
        }
        Geometry::Polyline(p) => {
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
        }
        Geometry::Arc(a) => {
            out.push((SnapKind::Endpoint, a.start_point()));
            out.push((SnapKind::Endpoint, a.end_point()));
            out.push((SnapKind::Midpoint, a.point_at(0.5)));
        }
        Geometry::Circle(_) | Geometry::Text(_) | Geometry::Insert(_) => {}
    }
}

/// Endpoint and midpoint candidates for object snap on one geometry.
pub fn snap_candidates(g: &Geometry) -> Vec<(SnapKind, Point2)> {
    let mut out = Vec::new();
    snap_candidates_into(g, &mut out);
    out
}

/// Nearest endpoint/midpoint within `tol` of `p`, writing candidates into `scratch`.
pub fn find_osnap_into(
    doc: &Document,
    p: Point2,
    tol: f64,
    scratch: &mut Vec<(SnapKind, Point2)>,
) -> Option<(SnapKind, Point2)> {
    let mut best: Option<(f64, u8, SnapKind, Point2)> = None;
    for e in doc.entities() {
        let Some(bbox) = snap_bbox(&e.geometry, tol) else {
            continue;
        };
        if !bbox_near(bbox, p) {
            continue;
        }
        scratch.clear();
        snap_candidates_into(&e.geometry, scratch);
        for &(kind, sp) in scratch.iter() {
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

/// Nearest endpoint/midpoint within `tol` of `p`. Endpoint beats midpoint on a
/// distance tie so corners win over segment centres.
pub fn find_osnap(doc: &Document, p: Point2, tol: f64) -> Option<(SnapKind, Point2)> {
    let mut scratch = Vec::new();
    find_osnap_into(doc, p, tol, &mut scratch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aadt_geometry::{Point2, Segment};
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
    fn bbox_skips_far_entities() {
        let mut doc = Document::new();
        doc.transact("t", |d| {
            d.add_geometry(
                "0",
                Geometry::Line(Segment::new(Point2::new(100.0, 100.0), Point2::new(110.0, 100.0))),
            );
            d.add_geometry(
                "0",
                Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(1.0, 0.0))),
            );
        });
        let hit = find_osnap(&doc, Point2::new(0.1, 0.0), 0.5).unwrap();
        assert!((hit.1.x).abs() < 1e-9);
    }

    #[test]
    fn throttle_reuses_hit() {
        let mut doc = Document::new();
        doc.transact("t", |d| {
            d.add_geometry(
                "0",
                Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0))),
            );
        });
        let mut st = OsnapState::default();
        let t0 = Instant::now();
        let a = st.resolve(&doc, Point2::new(0.1, 0.0), 0.5, 0.01, t0);
        let b = st.resolve(&doc, Point2::new(0.105, 0.0), 0.5, 0.01, t0);
        assert_eq!(a, b);
    }
}
