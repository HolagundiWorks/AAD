//! Region selection — polygon **window** / **crossing** and **fence** over a
//! [`Document`]. Pure geometry, kept out of the `gui` feature so it is unit-tested
//! headlessly (the egui viewport draws the region; this decides membership).
//!
//! - **Window** (drag left→right): entities *fully enclosed* by the region.
//! - **Crossing** (drag right→left): entities enclosed *or* touched by it.
//! - **Fence**: entities an open poly-line crosses.
//!
//! Curved geometry (circle / arc) is sampled into a poly-line for the tests.

use aadt_geometry::{Point2, Segment};
use aadt_model::{Document, EntityId, Geometry};

/// Samples used to approximate a full circle / arc for region tests.
const CURVE_SAMPLES: usize = 32;

/// How a polygon region selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionMode {
    /// Fully-enclosed only (AutoCAD window box).
    Window,
    /// Enclosed or touched (AutoCAD crossing box).
    Crossing,
}

/// Entities selected by a closed polygon region. Skips hidden layers.
/// `polygon` is the region ring (implicitly closed); WPolygon/CPolygon and the
/// rectangular window/crossing box are all this with different rings.
pub fn select_in_polygon(doc: &Document, polygon: &[Point2], mode: RegionMode) -> Vec<EntityId> {
    if polygon.len() < 3 {
        return Vec::new();
    }
    let ring = ring_edges(polygon);
    let mut out = Vec::new();
    for e in doc.iter_entities() {
        if !doc.layer_shown(&e.layer) {
            continue;
        }
        let pts = geometry_points(&e.geometry);
        if pts.is_empty() {
            continue;
        }
        let segs = geometry_segments(&e.geometry);
        let all_in = pts.iter().all(|p| point_in_polygon(polygon, *p));
        let crosses = segs
            .iter()
            .any(|s| ring.iter().any(|r| s.segment_intersection(*r).is_some()));
        let any_in = pts.iter().any(|p| point_in_polygon(polygon, *p));
        let selected = match mode {
            // Fully enclosed: every representative point inside and no boundary cross.
            RegionMode::Window => all_in && !crosses,
            // Enclosed or touched.
            RegionMode::Crossing => any_in || crosses,
        };
        if selected {
            out.push(e.id);
        }
    }
    out
}

/// Entities an **open** fence poly-line crosses. Skips hidden layers.
pub fn select_by_fence(doc: &Document, fence: &[Point2]) -> Vec<EntityId> {
    let edges = open_edges(fence);
    if edges.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for e in doc.iter_entities() {
        if !doc.layer_shown(&e.layer) {
            continue;
        }
        let segs = geometry_segments(&e.geometry);
        if segs
            .iter()
            .any(|s| edges.iter().any(|f| s.segment_intersection(*f).is_some()))
        {
            out.push(e.id);
        }
    }
    out
}

/// Closed ring of edges through the polygon vertices (last → first included).
fn ring_edges(poly: &[Point2]) -> Vec<Segment> {
    let mut edges = open_edges(poly);
    if poly.len() >= 3 {
        edges.push(Segment::new(poly[poly.len() - 1], poly[0]));
    }
    edges
}

/// Open chain of edges through the points (no closing edge).
fn open_edges(pts: &[Point2]) -> Vec<Segment> {
    pts.windows(2).map(|w| Segment::new(w[0], w[1])).collect()
}

/// Representative points of a geometry for enclosure tests.
fn geometry_points(g: &Geometry) -> Vec<Point2> {
    match g {
        Geometry::Line(s) => vec![s.a, s.midpoint(), s.b],
        Geometry::Polyline(p) => p.vertices.clone(),
        Geometry::Circle(c) => sample_circle(c.center, c.radius),
        Geometry::Arc(a) => (0..=CURVE_SAMPLES)
            .map(|k| a.point_at(k as f64 / CURVE_SAMPLES as f64))
            .collect(),
        Geometry::Text(t) => vec![t.position],
        Geometry::Insert(i) => vec![i.position],
    }
}

/// Line-like edges of a geometry for crossing tests (curves sampled).
fn geometry_segments(g: &Geometry) -> Vec<Segment> {
    match g {
        Geometry::Line(s) => vec![*s],
        Geometry::Polyline(p) => {
            let mut segs = open_edges(&p.vertices);
            if p.closed && p.vertices.len() >= 2 {
                segs.push(Segment::new(
                    p.vertices[p.vertices.len() - 1],
                    p.vertices[0],
                ));
            }
            segs
        }
        Geometry::Circle(c) => ring_edges(&sample_circle(c.center, c.radius)),
        Geometry::Arc(a) => open_edges(
            &(0..=CURVE_SAMPLES)
                .map(|k| a.point_at(k as f64 / CURVE_SAMPLES as f64))
                .collect::<Vec<_>>(),
        ),
        Geometry::Text(_) | Geometry::Insert(_) => Vec::new(),
    }
}

fn sample_circle(center: Point2, radius: f64) -> Vec<Point2> {
    (0..CURVE_SAMPLES)
        .map(|k| {
            let a = k as f64 / CURVE_SAMPLES as f64 * std::f64::consts::TAU;
            Point2::new(center.x + radius * a.cos(), center.y + radius * a.sin())
        })
        .collect()
}

/// Even-odd ray-cast point-in-polygon.
fn point_in_polygon(poly: &[Point2], p: Point2) -> bool {
    let n = poly.len();
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (pi, pj) = (poly[i], poly[j]);
        let intersects = (pi.y > p.y) != (pj.y > p.y)
            && p.x < (pj.x - pi.x) * (p.y - pi.y) / (pj.y - pi.y) + pi.x;
        if intersects {
            inside = !inside;
        }
        j = i;
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;
    use aadt_geometry::{Circle, Segment as Seg};
    use aadt_model::{Document, Geometry, Layer};

    fn square(lo: f64, hi: f64) -> Vec<Point2> {
        vec![
            Point2::new(lo, lo),
            Point2::new(hi, lo),
            Point2::new(hi, hi),
            Point2::new(lo, hi),
        ]
    }

    #[test]
    fn window_takes_only_fully_enclosed() {
        let mut doc = Document::new();
        let inside = doc.add_geometry(
            "0",
            Geometry::Line(Seg::new(Point2::new(2.0, 2.0), Point2::new(8.0, 8.0))),
        );
        let crossing = doc.add_geometry(
            "0",
            Geometry::Line(Seg::new(Point2::new(5.0, 5.0), Point2::new(15.0, 5.0))),
        );
        let _outside = doc.add_geometry(
            "0",
            Geometry::Line(Seg::new(Point2::new(20.0, 20.0), Point2::new(30.0, 30.0))),
        );
        let region = square(0.0, 10.0);

        let win = select_in_polygon(&doc, &region, RegionMode::Window);
        assert_eq!(win, vec![inside], "window: only the enclosed line");

        let cross = select_in_polygon(&doc, &region, RegionMode::Crossing);
        assert!(cross.contains(&inside) && cross.contains(&crossing));
        assert_eq!(cross.len(), 2, "crossing: enclosed + touching, not outside");
    }

    #[test]
    fn crossing_catches_a_circle_that_straddles_the_edge() {
        let mut doc = Document::new();
        let id = doc.add_geometry(
            "0",
            Geometry::Circle(Circle {
                center: Point2::new(10.0, 5.0),
                radius: 3.0,
            }),
        );
        let region = square(0.0, 10.0); // circle straddles x=10 edge
        assert!(select_in_polygon(&doc, &region, RegionMode::Crossing).contains(&id));
        assert!(
            select_in_polygon(&doc, &region, RegionMode::Window).is_empty(),
            "not fully enclosed"
        );
    }

    #[test]
    fn fence_selects_what_it_crosses() {
        let mut doc = Document::new();
        let hit = doc.add_geometry(
            "0",
            Geometry::Line(Seg::new(Point2::new(0.0, 5.0), Point2::new(10.0, 5.0))),
        );
        let _miss = doc.add_geometry(
            "0",
            Geometry::Line(Seg::new(Point2::new(0.0, 50.0), Point2::new(10.0, 50.0))),
        );
        // Vertical fence at x=5 crosses the first line only.
        let fence = vec![Point2::new(5.0, 0.0), Point2::new(5.0, 10.0)];
        assert_eq!(select_by_fence(&doc, &fence), vec![hit]);
    }

    #[test]
    fn hidden_layers_are_not_selected() {
        let mut doc = Document::new();
        doc.add_layer(Layer::new("OFF"));
        doc.add_geometry(
            "OFF",
            Geometry::Line(Seg::new(Point2::new(2.0, 2.0), Point2::new(8.0, 8.0))),
        );
        doc.set_layer_visible("OFF", false);
        assert!(select_in_polygon(&doc, &square(0.0, 10.0), RegionMode::Crossing).is_empty());
    }
}
