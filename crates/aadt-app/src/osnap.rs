//! Endpoint / midpoint / center object snap — pure geometry over a [`Document`].
//!
//! Used by the egui viewport; kept out of the `gui` feature so it is unit-tested
//! headlessly. Lean set: **END**, **MID**, and **CEN** (no quadrant/intersection).

use std::time::{Duration, Instant};

use aadt_geometry::{Bbox, Point2};
use aadt_model::{Document, Geometry};

/// Kind of object snap acquired under the cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapKind {
    Endpoint,
    Midpoint,
    Center,
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
        self.resolve_in_space(doc, None, raw, snap_tol, move_tol, now)
    }

    /// Resolve snap limited to one model space (active drafting space).
    pub fn resolve_in_space(
        &mut self,
        doc: &Document,
        space: Option<aadt_model::ModelSpaceId>,
        raw: Point2,
        snap_tol: f64,
        move_tol: f64,
        now: Instant,
    ) -> Option<(SnapKind, Point2)> {
        let spaces = space.map(|s| vec![s]);
        self.resolve_in_spaces(
            doc,
            spaces.as_deref(),
            raw,
            snap_tol,
            move_tol,
            now,
        )
    }

    /// Resolve snap across multiple model spaces (active + TRACE / underlay).
    pub fn resolve_in_spaces(
        &mut self,
        doc: &Document,
        spaces: Option<&[aadt_model::ModelSpaceId]>,
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
        let hit = find_osnap_in_spaces(doc, spaces, raw, snap_tol, &mut self.scratch);
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

/// Append endpoint/midpoint/center candidates for one geometry into `out` (no alloc).
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
            out.push((SnapKind::Center, a.center));
        }
        Geometry::Circle(c) => {
            out.push((SnapKind::Center, c.center));
        }
        Geometry::Text(_) | Geometry::Insert(_) => {}
    }
}

/// Endpoint, midpoint, and center candidates for object snap on one geometry.
pub fn snap_candidates(g: &Geometry) -> Vec<(SnapKind, Point2)> {
    let mut out = Vec::new();
    snap_candidates_into(g, &mut out);
    out
}

/// Nearest snap within `tol` of `p`, writing candidates into `scratch`.
/// When `space` is `Some`, only that model space is searched (drafting target).
pub fn find_osnap_into(
    doc: &Document,
    p: Point2,
    tol: f64,
    scratch: &mut Vec<(SnapKind, Point2)>,
) -> Option<(SnapKind, Point2)> {
    find_osnap_in_space(doc, None, p, tol, scratch)
}

/// Like [`find_osnap_into`], optionally limited to one model space.
/// Skips entities on off/frozen layers; locked layers remain snappable (AutoCAD).
pub fn find_osnap_in_space(
    doc: &Document,
    space: Option<aadt_model::ModelSpaceId>,
    p: Point2,
    tol: f64,
    scratch: &mut Vec<(SnapKind, Point2)>,
) -> Option<(SnapKind, Point2)> {
    let spaces = space.map(|s| vec![s]);
    find_osnap_in_spaces(doc, spaces.as_deref(), p, tol, scratch)
}

/// Nearest snap among entities in any of `spaces` (or the whole document when `None`).
pub fn find_osnap_in_spaces(
    doc: &Document,
    spaces: Option<&[aadt_model::ModelSpaceId]>,
    p: Point2,
    tol: f64,
    scratch: &mut Vec<(SnapKind, Point2)>,
) -> Option<(SnapKind, Point2)> {
    let mut best: Option<(f64, u8, SnapKind, Point2)> = None;
    let entities = doc.iter_entities().filter(|e| {
        spaces
            .map(|ss| ss.iter().any(|s| *s == e.space))
            .unwrap_or(true)
    });
    for e in entities {
        if !doc.layer_shown(&e.layer) {
            continue;
        }
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
                SnapKind::Center => 2u8,
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

/// Nearest snap within `tol` of `p`. Endpoint beats midpoint/center on a
/// distance tie so corners win over segment centres.
pub fn find_osnap(doc: &Document, p: Point2, tol: f64) -> Option<(SnapKind, Point2)> {
    let mut scratch = Vec::new();
    find_osnap_into(doc, p, tol, &mut scratch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aadt_geometry::{Circle, Point2, Segment};
    use aadt_model::{Document, Layer};

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
    fn circle_offers_center() {
        let g = Geometry::Circle(Circle {
            center: Point2::new(3.0, 4.0),
            radius: 2.0,
        });
        let c = snap_candidates(&g);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].0, SnapKind::Center);
        assert!((c[0].1.x - 3.0).abs() < 1e-9 && (c[0].1.y - 4.0).abs() < 1e-9);
    }

    #[test]
    fn finds_center_within_tolerance() {
        let mut doc = Document::new();
        doc.transact("t", |d| {
            d.add_geometry(
                "0",
                Geometry::Circle(Circle {
                    center: Point2::new(5.0, 5.0),
                    radius: 3.0,
                }),
            );
        });
        let hit = find_osnap(&doc, Point2::new(5.1, 5.0), 0.5).unwrap();
        assert_eq!(hit.0, SnapKind::Center);
        assert!((hit.1.x - 5.0).abs() < 1e-9 && (hit.1.y - 5.0).abs() < 1e-9);
    }

    #[test]
    fn off_layer_not_snappable() {
        let mut doc = Document::new();
        doc.transact("t", |d| {
            d.add_layer(Layer::new("OFF"));
            d.add_geometry(
                "OFF",
                Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0))),
            );
            d.set_layer_visible("OFF", false);
        });
        assert!(find_osnap(&doc, Point2::new(0.1, 0.0), 0.5).is_none());
    }

    #[test]
    fn snap_candidate_spaces_include_trace_neighbours() {
        use aadt_model::SpaceCategory;
        let mut doc = Document::new();
        let ground = doc.active_floor_id().unwrap();
        let level1 = doc.add_floor("Level 1", 3.0);
        doc.set_trace(true, false);
        let _ = doc.set_active_floor(level1);
        let spaces = doc.snap_candidate_spaces();
        let ground_walls = doc.space_on_floor(ground, SpaceCategory::Walls).unwrap();
        assert!(spaces.contains(&ground_walls));
        let active = doc.active_space();
        assert!(spaces.contains(&active));
    }

    #[test]
    fn finds_endpoint_on_underlay_space_when_listed() {
        use aadt_model::SpaceCategory;
        let mut doc = Document::new();
        let ground = doc.active_floor_id().unwrap();
        let walls = doc.space_on_floor(ground, SpaceCategory::Walls).unwrap();
        let furn = doc.space_on_floor(ground, SpaceCategory::Furniture).unwrap();
        let id = doc.add_geometry(
            "A-FURN",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0))),
        );
        doc.set_entity_space(id, furn);
        let _ = doc.set_active_space(walls);
        // Active-only: miss underlay furniture line.
        assert!(find_osnap_in_space(&doc, Some(walls), Point2::new(0.1, 0.0), 0.5, &mut Vec::new())
            .is_none());
        // Snap candidates include underlay.
        let spaces = doc.snap_candidate_spaces();
        let hit = find_osnap_in_spaces(
            &doc,
            Some(spaces.as_slice()),
            Point2::new(0.1, 0.0),
            0.5,
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(hit.0, SnapKind::Endpoint);
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
                Geometry::Line(Segment::new(
                    Point2::new(100.0, 100.0),
                    Point2::new(110.0, 100.0),
                )),
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
