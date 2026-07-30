//! Deterministic geometric featurizer: turn a drawing element into a fixed-length
//! `f32` descriptor the model consumes. The **same** function featurizes real
//! entities (inference) and the synthetic exemplars used to train the model, so
//! train/inference stay consistent by construction.
//!
//! Layout (indices), all in `[0, 1]` where possible so no external normalization
//! is needed:
//! ```text
//!  0..6  kind one-hot: Line, Polyline, Circle, Arc, Text, Insert
//!  6     closed (closed polyline / circle / block footprint)
//!  7     curvature (1 for circle/arc, else 0)
//!  8     aspect (short/long side of the bbox)
//!  9     size (squashed bbox diagonal)
//! 10     vertex count (normalized)
//! 11     orthogonality (length-weighted fraction of axis-aligned edges)
//! 12     right-angle corner ratio
//! 13     arc sweep / 360
//! 14     radius (squashed)
//! 15     text length (normalized)
//! 16     text height (squashed)
//! 17..25 edge-direction histogram (8 bins over 0..180°)
//! ```

use aadt_geometry::{Bbox, Point2, Polyline, Segment};
use aadt_model::{Document, Entity, Geometry};

use crate::rng::Rng;

/// Length of the feature vector.
pub const FEATURE_DIM: usize = 25;

const ANGLE_BINS: usize = 8;

/// Squash a non-negative magnitude into `[0, 1)` with reference scale `r` (metres).
fn squash(x: f64, r: f64) -> f32 {
    if x <= 0.0 {
        0.0
    } else {
        (x / (x + r)) as f32
    }
}

/// Feature vector for a placed [`Entity`] (uses `doc` only to size block inserts).
pub fn element_features(entity: &Entity, doc: &Document) -> Vec<f32> {
    features_for_geometry(&entity.geometry, doc)
}

/// Feature vector for a bare [`Geometry`]. `doc` is consulted only to look up a
/// block definition for `Insert` footprints; pass a fresh `Document` when none.
pub fn features_for_geometry(g: &Geometry, doc: &Document) -> Vec<f32> {
    let mut f = vec![0.0f32; FEATURE_DIM];

    let kind = match g {
        Geometry::Line(_) => 0,
        Geometry::Polyline(_) => 1,
        Geometry::Circle(_) => 2,
        Geometry::Arc(_) => 3,
        Geometry::Text(_) => 4,
        Geometry::Insert(_) => 5,
    };
    f[kind] = 1.0;

    // Closed / curvature flags.
    f[6] = match g {
        Geometry::Polyline(p) => f32::from(p.closed),
        Geometry::Circle(_) => 1.0,
        Geometry::Insert(_) => 1.0,
        _ => 0.0,
    };
    f[7] = matches!(g, Geometry::Circle(_) | Geometry::Arc(_)) as u8 as f32;

    // Bounding box → aspect + size.
    let pts = geometry_points(g, doc);
    if let Some(bbox) = Bbox::of_points(pts.iter().copied()) {
        let (w, h) = (bbox.width().abs(), bbox.height().abs());
        let long = w.max(h);
        let short = w.min(h);
        f[8] = if long > 1e-9 {
            (short / long) as f32
        } else {
            1.0
        };
        let diag = (w * w + h * h).sqrt();
        f[9] = squash(diag, 1.0);
    }

    // Vertex count, normalized against a cap of 16.
    let vcount = match g {
        Geometry::Line(_) => 2.0,
        Geometry::Polyline(p) => p.vertices.len() as f32,
        Geometry::Insert(_) => 4.0,
        _ => 0.0,
    };
    f[10] = (vcount.min(16.0)) / 16.0;

    // Straight-edge statistics: orthogonality + direction histogram.
    let segs = geometry_segments(g);
    let mut total_len = 0.0f64;
    let mut axis_len = 0.0f64;
    let mut hist = [0.0f64; ANGLE_BINS];
    for s in &segs {
        let len = s.length();
        if len <= 1e-9 {
            continue;
        }
        total_len += len;
        let Some(dir) = s.direction() else { continue };
        let mut deg = dir.y.atan2(dir.x).to_degrees().rem_euclid(180.0);
        if deg >= 180.0 {
            deg -= 180.0;
        }
        // Axis-aligned within ~6° of 0/90/180.
        let m90 = deg.rem_euclid(90.0);
        if m90 <= 6.0 || m90 >= 84.0 {
            axis_len += len;
        }
        let bin = ((deg / 180.0) * ANGLE_BINS as f64) as usize;
        hist[bin.min(ANGLE_BINS - 1)] += len;
    }
    if total_len > 1e-9 {
        f[11] = (axis_len / total_len) as f32;
        for (i, hv) in hist.iter().enumerate() {
            f[17 + i] = (hv / total_len) as f32;
        }
    }

    // Right-angle corner ratio (closed polylines).
    f[12] = right_angle_ratio(g);

    // Curve specifics.
    match g {
        Geometry::Arc(a) => {
            f[13] = (a.sweep() / 360.0) as f32;
            f[14] = squash(a.radius, 1.0);
        }
        Geometry::Circle(c) => {
            f[14] = squash(c.radius, 1.0);
        }
        Geometry::Text(t) => {
            f[15] = (t.value.chars().count().min(32) as f32) / 32.0;
            f[16] = squash(t.height, 0.5);
        }
        _ => {}
    }

    f
}

/// Fraction of a closed polyline's corners that turn ~90° (rectangles, fixtures).
fn right_angle_ratio(g: &Geometry) -> f32 {
    let Geometry::Polyline(p) = g else {
        return 0.0;
    };
    let v = &p.vertices;
    let n = v.len();
    if n < 3 {
        return 0.0;
    }
    let count = if p.closed { n } else { n.saturating_sub(2) };
    if count == 0 {
        return 0.0;
    }
    let mut right = 0usize;
    let range = if p.closed { 0..n } else { 1..n - 1 };
    for i in range {
        let prev = v[(i + n - 1) % n];
        let cur = v[i];
        let next = v[(i + 1) % n];
        let d1 = cur - prev;
        let d2 = next - cur;
        let (Some(a), Some(b)) = (d1.normalized(), d2.normalized()) else {
            continue;
        };
        // Turn angle between successive travel directions.
        let ang = a.dot(b).clamp(-1.0, 1.0).acos().to_degrees();
        if (ang - 90.0).abs() <= 12.0 {
            right += 1;
        }
    }
    (right as f32) / (count as f32)
}

/// Representative points for a geometry's bounding box.
fn geometry_points(g: &Geometry, doc: &Document) -> Vec<Point2> {
    match g {
        Geometry::Line(s) => vec![s.a, s.b],
        Geometry::Polyline(p) => p.vertices.clone(),
        Geometry::Circle(c) => vec![
            Point2::new(c.center.x - c.radius, c.center.y),
            Point2::new(c.center.x + c.radius, c.center.y),
            Point2::new(c.center.x, c.center.y - c.radius),
            Point2::new(c.center.x, c.center.y + c.radius),
        ],
        Geometry::Arc(a) => a.to_polyline(8).vertices,
        Geometry::Text(t) => {
            let w = (t.value.chars().count().max(1) as f64) * t.height * 0.6;
            vec![
                t.position,
                Point2::new(t.position.x + w, t.position.y + t.height),
            ]
        }
        Geometry::Insert(ins) => insert_footprint(ins, doc),
    }
}

/// A rectangle approximating a block insert's footprint (scaled block bbox).
fn insert_footprint(ins: &aadt_model::Insert, doc: &Document) -> Vec<Point2> {
    if let Some(block) = doc.block(&ins.name) {
        let pts: Vec<Point2> = block
            .entities
            .iter()
            .flat_map(|be| raw_points(&be.geometry))
            .collect();
        if let Some(bb) = Bbox::of_points(pts) {
            let w = bb.width().abs() * ins.scale_x.abs().max(1e-6);
            let h = bb.height().abs() * ins.scale_y.abs().max(1e-6);
            return vec![
                ins.position,
                Point2::new(ins.position.x + w, ins.position.y + h),
            ];
        }
    }
    vec![ins.position]
}

/// Points of a geometry without block expansion (block content is already flat).
fn raw_points(g: &Geometry) -> Vec<Point2> {
    match g {
        Geometry::Line(s) => vec![s.a, s.b],
        Geometry::Polyline(p) => p.vertices.clone(),
        Geometry::Circle(c) => vec![
            Point2::new(c.center.x - c.radius, c.center.y - c.radius),
            Point2::new(c.center.x + c.radius, c.center.y + c.radius),
        ],
        Geometry::Arc(a) => a.to_polyline(4).vertices,
        Geometry::Text(t) => vec![t.position],
        Geometry::Insert(ins) => vec![ins.position],
    }
}

/// Straight edges a geometry contributes (arcs/circles/text/inserts: none).
fn geometry_segments(g: &Geometry) -> Vec<Segment> {
    match g {
        Geometry::Line(s) => vec![*s],
        Geometry::Polyline(p) => p.segments(),
        _ => Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Synthetic exemplar generation (training only).
// ---------------------------------------------------------------------------

use crate::classes::ElementClass;

/// Generate one random synthetic geometry for `class`, used to train the model.
/// Deliberately covers the shape variety each class shows in real drawings; the
/// classifier learns the deterministic features above, not these shapes directly.
pub(crate) fn synth_geometry(class: ElementClass, rng: &mut Rng) -> Geometry {
    let rot = rng.range(-4.0, 4.0); // near-axis-aligned by default
    match class {
        ElementClass::Wall => {
            let len = rng.range(1.5, 8.0);
            if rng.unit() < 0.5 {
                axis_line(len, rot, rng)
            } else {
                rect(len, rng.range(0.12, 0.30), rot, rng)
            }
        }
        ElementClass::Column => {
            if rng.unit() < 0.5 {
                Geometry::Circle(aadt_geometry::Circle::new(
                    origin(rng),
                    rng.range(0.10, 0.40) as f64,
                ))
            } else {
                let s = rng.range(0.20, 0.55);
                rect(s, s, rng.range(0.0, 90.0), rng)
            }
        }
        ElementClass::Beam => {
            // Long and very thin.
            rect(rng.range(2.0, 9.0), rng.range(0.06, 0.16), rot, rng)
        }
        ElementClass::Lintel => {
            // Medium, thin, horizontal (over an opening).
            rect(
                rng.range(0.8, 2.2),
                rng.range(0.10, 0.22),
                rng.range(-2.0, 2.0),
                rng,
            )
        }
        ElementClass::Door => {
            let r = rng.range(0.7, 1.1) as f64;
            let start = rng.range(0.0, 360.0) as f64;
            let sweep = rng.range(80.0, 100.0) as f64;
            Geometry::Arc(aadt_geometry::Arc::new(
                origin(rng),
                r,
                start,
                start + sweep,
            ))
        }
        ElementClass::Glazing => {
            // Short, thin, axis-aligned (window in a wall).
            if rng.unit() < 0.5 {
                axis_line(rng.range(0.6, 1.8), rot, rng)
            } else {
                rect(rng.range(0.6, 1.8), rng.range(0.08, 0.16), rot, rng)
            }
        }
        ElementClass::Furniture => {
            // Medium block, varied aspect and free rotation, sometimes irregular.
            let l = rng.range(0.6, 2.2);
            let t = l * rng.range(0.4, 0.95);
            if rng.unit() < 0.3 {
                irregular_poly(l, t, rng)
            } else {
                rect(l, t, rng.range(0.0, 180.0), rng)
            }
        }
        ElementClass::Equipment => {
            if rng.unit() < 0.4 {
                Geometry::Circle(aadt_geometry::Circle::new(
                    origin(rng),
                    rng.range(0.2, 0.6) as f64,
                ))
            } else {
                let s = rng.range(0.4, 1.2);
                rect(s, s * rng.range(0.7, 1.0), rng.range(0.0, 90.0), rng)
            }
        }
        ElementClass::Stair => stair_comb(rng),
        ElementClass::Annotation => {
            let n = rng.below(10) + 2;
            let s = "A".repeat(n);
            Geometry::Text(aadt_model::Text::new(
                s,
                origin(rng),
                rng.range(0.1, 0.4) as f64,
                rng.range(0.0, 90.0) as f64,
            ))
        }
    }
}

fn origin(rng: &mut Rng) -> Point2 {
    Point2::new(rng.range(-2.0, 2.0) as f64, rng.range(-2.0, 2.0) as f64)
}

fn axis_line(len: f32, rot_deg: f32, rng: &mut Rng) -> Geometry {
    let c = origin(rng);
    let vertical = rng.unit() < 0.5;
    let base = if vertical { 90.0 } else { 0.0 };
    let ang = (base + rot_deg).to_radians() as f64;
    let half = (len as f64) * 0.5;
    let d = Point2::new(half * ang.cos(), half * ang.sin());
    Geometry::Line(Segment::new(
        Point2::new(c.x - d.x, c.y - d.y),
        Point2::new(c.x + d.x, c.y + d.y),
    ))
}

fn rect(len: f32, thick: f32, rot_deg: f32, rng: &mut Rng) -> Geometry {
    let c = origin(rng);
    let (hl, ht) = ((len as f64) * 0.5, (thick as f64) * 0.5);
    let corners = [
        Point2::new(-hl, -ht),
        Point2::new(hl, -ht),
        Point2::new(hl, ht),
        Point2::new(-hl, ht),
    ];
    let rad = (rot_deg as f64).to_radians();
    let verts = corners
        .iter()
        .map(|p| {
            let rp = Point2::new(p.x, p.y).rotate_about(Point2::new(0.0, 0.0), rad);
            Point2::new(c.x + rp.x, c.y + rp.y)
        })
        .collect();
    Geometry::Polyline(Polyline::new(verts, true))
}

fn irregular_poly(len: f32, thick: f32, rng: &mut Rng) -> Geometry {
    let c = origin(rng);
    let n = rng.below(3) + 4; // 4..6 vertices
    let mut verts = Vec::with_capacity(n);
    for i in 0..n {
        let ang = (i as f64 / n as f64) * std::f64::consts::TAU;
        let rr = rng.range(0.6, 1.0) as f64;
        verts.push(Point2::new(
            c.x + (len as f64) * 0.5 * rr * ang.cos(),
            c.y + (thick as f64) * 0.5 * rr * ang.sin(),
        ));
    }
    Geometry::Polyline(Polyline::new(verts, true))
}

/// A comb/zigzag polyline standing in for a stair run (many orthogonal treads).
fn stair_comb(rng: &mut Rng) -> Geometry {
    let c = origin(rng);
    let treads = rng.below(6) + 5; // 5..10
    let tread = rng.range(0.25, 0.35) as f64;
    let depth = rng.range(0.8, 1.4) as f64;
    let mut verts = Vec::with_capacity(treads * 2 + 1);
    let mut x = c.x;
    let mut up = true;
    verts.push(Point2::new(x, c.y));
    for _ in 0..treads {
        let y = if up { c.y + depth } else { c.y };
        verts.push(Point2::new(x, y));
        x += tread;
        verts.push(Point2::new(x, y));
        up = !up;
    }
    Geometry::Polyline(Polyline::new(verts, false))
}
