//! Object snap — pure geometry over a [`Document`].
//!
//! Used by the egui viewport; kept out of the `gui` feature so it is unit-tested
//! headlessly. Snap set: **END**, **MID**, **CEN**, **QUA** (quadrant), and
//! **NEA** (nearest-on-object, last resort). Intersection / perpendicular /
//! tangent need cross-entity or drag-base context and are tracked in M9.1.

use std::time::{Duration, Instant};

use aadt_geometry::{Arc, Bbox, Point2, Segment};
use aadt_model::{Document, Geometry};

/// Kind of object snap acquired under the cursor.
///
/// Discriminant order encodes AutoCAD-style priority on a distance tie
/// (earlier wins): Endpoint > Midpoint > Center > Quadrant > Nearest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapKind {
    Endpoint,
    Midpoint,
    Center,
    Quadrant,
    Nearest,
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
        self.resolve_in_spaces(doc, spaces.as_deref(), raw, snap_tol, move_tol, now)
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
            // Quadrants that actually lie on the arc sweep.
            for deg in [0.0, 90.0, 180.0, 270.0] {
                if angle_in_arc(a, deg) {
                    out.push((SnapKind::Quadrant, point_on_circle(a.center, a.radius, deg)));
                }
            }
        }
        Geometry::Circle(c) => {
            out.push((SnapKind::Center, c.center));
            for deg in [0.0, 90.0, 180.0, 270.0] {
                out.push((SnapKind::Quadrant, point_on_circle(c.center, c.radius, deg)));
            }
        }
        Geometry::Text(_) | Geometry::Insert(_) => {}
    }
}

/// Point on a circle of `radius` about `center` at `deg` degrees CCW from +X.
fn point_on_circle(center: Point2, radius: f64, deg: f64) -> Point2 {
    let r = deg.to_radians();
    Point2::new(center.x + radius * r.cos(), center.y + radius * r.sin())
}

/// Whether `deg` (CCW from +X) falls within an arc's start→end sweep.
fn angle_in_arc(a: &Arc, deg: f64) -> bool {
    let rel = (deg - a.start_angle).rem_euclid(360.0);
    rel <= a.sweep() + 1e-9
}

/// Nearest point *on* a geometry to `p` (NEA snap). `None` for points-only
/// geometries (text / inserts) that offer no continuous curve.
fn nearest_on_geometry(g: &Geometry, p: Point2) -> Option<Point2> {
    match g {
        Geometry::Line(s) => Some(nearest_on_segment(*s, p)),
        Geometry::Polyline(poly) => {
            let mut best: Option<(f64, Point2)> = None;
            let mut consider = |seg: Segment| {
                let np = nearest_on_segment(seg, p);
                let d = p.distance(np);
                if best.map(|(bd, _)| d < bd).unwrap_or(true) {
                    best = Some((d, np));
                }
            };
            for w in poly.vertices.windows(2) {
                consider(Segment::new(w[0], w[1]));
            }
            if poly.closed && poly.vertices.len() >= 2 {
                consider(Segment::new(
                    poly.vertices[poly.vertices.len() - 1],
                    poly.vertices[0],
                ));
            }
            best.map(|(_, np)| np)
        }
        Geometry::Circle(c) => (p - c.center).normalized().map(|u| c.center + u * c.radius),
        Geometry::Arc(a) => {
            let v = p - a.center;
            let deg = v.y.atan2(v.x).to_degrees();
            if angle_in_arc(a, deg) {
                v.normalized().map(|u| a.center + u * a.radius)
            } else {
                let (sp, ep) = (a.start_point(), a.end_point());
                Some(if p.distance(sp) <= p.distance(ep) { sp } else { ep })
            }
        }
        Geometry::Text(_) | Geometry::Insert(_) => None,
    }
}

/// Nearest point on a finite segment (clamped to its ends).
fn nearest_on_segment(s: Segment, p: Point2) -> Point2 {
    let t = s.project_param(p).clamp(0.0, 1.0);
    s.a + (s.b - s.a) * t
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
    // NEA (nearest-on-object) fallback, used only when no discrete snap is found.
    let mut best_near: Option<(f64, Point2)> = None;
    let entities = doc.iter_entities().filter(|e| {
        spaces
            .map(|ss| ss.contains(&e.space))
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
                SnapKind::Quadrant => 3u8,
                SnapKind::Nearest => 9u8,
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
        if let Some(np) = nearest_on_geometry(&e.geometry, p) {
            let d = p.distance(np);
            if d <= tol && best_near.map(|(bd, _)| d < bd).unwrap_or(true) {
                best_near = Some((d, np));
            }
        }
    }
    // Discrete snaps (END/MID/CEN/QUA) always win over NEA when present.
    match best {
        Some((_, _, k, sp)) => Some((k, sp)),
        None => best_near.map(|(_, np)| (SnapKind::Nearest, np)),
    }
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
    fn circle_offers_center_and_four_quadrants() {
        let g = Geometry::Circle(Circle {
            center: Point2::new(3.0, 4.0),
            radius: 2.0,
        });
        let c = snap_candidates(&g);
        assert_eq!(c.len(), 5); // center + 4 quadrants
        assert!(c
            .iter()
            .any(|(k, p)| *k == SnapKind::Center && (p.x - 3.0).abs() < 1e-9 && (p.y - 4.0).abs() < 1e-9));
        assert_eq!(
            c.iter().filter(|(k, _)| *k == SnapKind::Quadrant).count(),
            4
        );
        // East quadrant is center + (radius, 0).
        assert!(c
            .iter()
            .any(|(k, p)| *k == SnapKind::Quadrant && (p.x - 5.0).abs() < 1e-9 && (p.y - 4.0).abs() < 1e-9));
    }

    #[test]
    fn finds_quadrant_on_circle() {
        let mut doc = Document::new();
        doc.transact("t", |d| {
            d.add_geometry(
                "0",
                Geometry::Circle(Circle {
                    center: Point2::new(3.0, 4.0),
                    radius: 2.0,
                }),
            );
        });
        // Just inside the east quadrant point (5, 4).
        let hit = find_osnap(&doc, Point2::new(4.9, 4.0), 0.5).unwrap();
        assert_eq!(hit.0, SnapKind::Quadrant);
        assert!((hit.1.x - 5.0).abs() < 1e-9 && (hit.1.y - 4.0).abs() < 1e-9);
    }

    #[test]
    fn arc_quadrant_only_within_sweep() {
        // Half-circle sweeping -45°→135°: 0° and 90° are interior quadrants;
        // 180° and 270° are off the arc.
        let g = Geometry::Arc(aadt_geometry::Arc::new(Point2::new(0.0, 0.0), 1.0, -45.0, 135.0));
        let quads: Vec<_> = snap_candidates(&g)
            .into_iter()
            .filter(|(k, _)| *k == SnapKind::Quadrant)
            .collect();
        assert_eq!(quads.len(), 2);
        assert!(quads
            .iter()
            .any(|(_, p)| (p.x - 1.0).abs() < 1e-9 && p.y.abs() < 1e-9)); // 0°
        assert!(quads
            .iter()
            .any(|(_, p)| p.x.abs() < 1e-9 && (p.y - 1.0).abs() < 1e-9)); // 90°
        assert!(!quads.iter().any(|(_, p)| (p.x + 1.0).abs() < 1e-9)); // no 180°
    }

    #[test]
    fn nearest_on_line_when_no_vertex_snap() {
        let mut doc = Document::new();
        doc.transact("t", |d| {
            d.add_geometry(
                "0",
                Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0))),
            );
        });
        // (3, 0.2): far from both ends and the midpoint, but 0.2 above the line.
        let hit = find_osnap(&doc, Point2::new(3.0, 0.2), 0.5).unwrap();
        assert_eq!(hit.0, SnapKind::Nearest);
        assert!((hit.1.x - 3.0).abs() < 1e-9 && hit.1.y.abs() < 1e-9);
    }

    #[test]
    fn endpoint_beats_nearest_at_same_spot() {
        let mut doc = Document::new();
        doc.transact("t", |d| {
            d.add_geometry(
                "0",
                Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0))),
            );
        });
        // Near the endpoint: the discrete END snap must win over NEA.
        let hit = find_osnap(&doc, Point2::new(0.1, 0.05), 0.5).unwrap();
        assert_eq!(hit.0, SnapKind::Endpoint);
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
        let furn = doc
            .space_on_floor(ground, SpaceCategory::Furniture)
            .unwrap();
        let id = doc.add_geometry(
            "A-FURN",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0))),
        );
        doc.set_entity_space(id, furn);
        let _ = doc.set_active_space(walls);
        // Active-only: miss underlay furniture line.
        assert!(find_osnap_in_space(
            &doc,
            Some(walls),
            Point2::new(0.1, 0.0),
            0.5,
            &mut Vec::new()
        )
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
