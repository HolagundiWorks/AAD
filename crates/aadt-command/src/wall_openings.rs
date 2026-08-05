//! Cut openings in walls when doors / windows are placed.
//!
//! Two wall representations are supported:
//!
//! 1. **Double-line faces** (typical CAD) — two parallel wall lines at a
//!    recognisable thickness. With **Feet & inches** units: **4½″** / **9″**.
//!    With **Decimal meters**: **100 / 115 / 150 / 200 / 230 mm**. Both faces
//!    are trimmed and joined with jambs into **closed polylines** so hatch
//!    stays inside the wall body.
//! 2. **Single centerline** — one line / open polyline (`A-WALL` /
//!    `AADT-ELEM=wall`). Falls back to splitting that path into residual pieces.
//!
//! Place-tool door/window blocks store `WIDTH` in metres; cut size is converted
//! to the drawing’s model units.

use aadt_geometry::{Point2, Polyline, Segment, Vec2, LENGTH_EPS};
use aadt_model::{
    primary_from_props, Document, Entity, EntityId, Geometry, UnitSystem, INCHES_PER_METER,
};
use std::collections::HashMap;

/// Result of snapping a place-point onto a wall.
#[derive(Debug, Clone, Copy)]
pub struct WallSnap {
    pub wall_id: EntityId,
    pub point: Point2,
    /// Degrees CCW from +X — door/window local +X along the wall.
    pub angle_deg: f64,
}

/// A parallel face pair at a recognisable wall thickness.
#[derive(Debug, Clone)]
struct FacePair {
    a_id: EntityId,
    a: Segment,
    b_id: EntityId,
    b: Segment,
    thickness: f64,
    layer: String,
    attrs: HashMap<String, String>,
}

/// True when an entity should receive door/window openings.
pub fn is_wall_entity(entity: &Entity) -> bool {
    if let Some(elem) = entity
        .attribute("AADT-ELEM")
        .or_else(|| entity.attribute("aadt-elem"))
    {
        if elem.eq_ignore_ascii_case("wall") {
            return matches!(entity.geometry, Geometry::Line(_) | Geometry::Polyline(_));
        }
    }
    let layer = entity.layer.to_ascii_uppercase();
    let wallish = layer == "A-WALL"
        || layer.starts_with("A-WALL")
        || layer.contains("WALL")
        || layer.contains("A-ANNO-WALL");
    wallish
        && matches!(
            entity.geometry,
            Geometry::Line(_) | Geometry::Polyline(Polyline { closed: false, .. })
        )
}

/// Default opening width in **metres** for a place-tool block name.
pub fn default_opening_width_m(block_name: &str) -> f64 {
    match block_name.trim().to_ascii_uppercase().as_str() {
        "DOOR-SINGLE" => 0.9,
        "DOOR-DOUBLE" => 1.8,
        "WINDOW" => 1.2,
        "WINDOW-SLIDER" => 1.8,
        _ => 0.9,
    }
}

/// Default opening width in **model units** (metres or inches) for a block name.
pub fn default_opening_width(doc: &Document, block_name: &str) -> f64 {
    opening_m_to_model(doc, default_opening_width_m(block_name))
}

/// Read WIDTH from a block definition (stored in metres), converted to model units.
pub fn opening_width_for_block(doc: &Document, block_name: &str) -> f64 {
    let width_m = if let Some(block) = doc.block(block_name) {
        ["WIDTH", "width"]
            .iter()
            .find_map(|key| {
                block
                    .params
                    .iter()
                    .find(|p| p.name == *key)
                    .and_then(|p| p.default.parse::<f64>().ok())
                    .filter(|w| *w > LENGTH_EPS)
            })
            .unwrap_or_else(|| default_opening_width_m(block_name))
    } else {
        default_opening_width_m(block_name)
    };
    opening_m_to_model(doc, width_m)
}

fn opening_m_to_model(doc: &Document, width_m: f64) -> f64 {
    match primary_from_props(&doc.properties().units) {
        UnitSystem::Meters => width_m,
        UnitSystem::FeetInches => width_m * INCHES_PER_METER,
    }
}

/// Nominal wall thicknesses to identify in model units.
///
/// - **Feet & inches:** 4½″ and 9″
/// - **Decimal meters:** 115 mm and 230 mm (metric equivalents), plus common
///   drawn sizes 100 / 150 / 200 mm
pub fn nominal_wall_thicknesses(doc: &Document) -> Vec<f64> {
    match primary_from_props(&doc.properties().units) {
        UnitSystem::FeetInches => vec![4.5, 9.0],
        UnitSystem::Meters => vec![0.100, 0.115, 0.150, 0.200, 0.230],
    }
}

fn thickness_is_nominal(offset: f64, nominals: &[f64]) -> bool {
    nominals.iter().any(|&n| {
        let tol = (n * 0.12).max(if n >= 1.0 {
            0.35
        } else if n >= 0.05 {
            0.012
        } else {
            0.008
        });
        (offset - n).abs() <= tol
    })
}

/// Max distance from a click to a wall face for snap / pair search.
fn face_snap_tol(doc: &Document) -> f64 {
    let max_th = nominal_wall_thicknesses(doc)
        .into_iter()
        .fold(0.0_f64, f64::max);
    let pad = match primary_from_props(&doc.properties().units) {
        UnitSystem::FeetInches => 2.0,
        UnitSystem::Meters => 0.05,
    };
    max_th * 0.55 + pad
}

/// Snap `pt` onto the nearest wall face / centerline.
pub fn snap_to_wall(doc: &Document, pt: Point2) -> Option<WallSnap> {
    let tol = face_snap_tol(doc);
    let mut best: Option<(f64, EntityId, Point2, f64)> = None;
    for e in doc.entities() {
        if !is_wall_entity(e) {
            continue;
        }
        let Some((d, on, ang)) = closest_on_wall(&e.geometry, pt) else {
            continue;
        };
        if d > tol {
            continue;
        }
        if best.map(|(bd, ..)| d < bd).unwrap_or(true) {
            best = Some((d, e.id, on, ang));
        }
    }
    best.map(|(_, id, point, angle_deg)| WallSnap {
        wall_id: id,
        point,
        angle_deg,
    })
}

fn closest_on_wall(g: &Geometry, pt: Point2) -> Option<(f64, Point2, f64)> {
    match g {
        Geometry::Line(seg) => closest_on_seg(*seg, pt),
        Geometry::Polyline(pl) if !pl.closed && pl.vertices.len() >= 2 => {
            let mut best: Option<(f64, Point2, f64)> = None;
            for w in pl.vertices.windows(2) {
                let Some(hit) = closest_on_seg(Segment::new(w[0], w[1]), pt) else {
                    continue;
                };
                if best.map(|(bd, ..)| hit.0 < bd).unwrap_or(true) {
                    best = Some(hit);
                }
            }
            best
        }
        _ => None,
    }
}

fn closest_on_seg(seg: Segment, pt: Point2) -> Option<(f64, Point2, f64)> {
    let t = seg.project_param(pt).clamp(0.0, 1.0);
    let on = Point2::new(
        seg.a.x + (seg.b.x - seg.a.x) * t,
        seg.a.y + (seg.b.y - seg.a.y) * t,
    );
    let dir = seg.direction()?;
    let ang = dir.y.atan2(dir.x).to_degrees();
    Some((pt.distance(on), on, ang))
}

/// Choose wall tangent (±180°) so the opening of `width` from `hinge` stays on
/// the wall as much as possible.
pub fn orient_opening_along_wall(g: &Geometry, hinge: Point2, width: f64, wall_ang: f64) -> f64 {
    let rad = wall_ang.to_radians();
    let dir = Vec2::new(rad.cos(), rad.sin());
    let end0 = Point2::new(hinge.x + dir.x * width, hinge.y + dir.y * width);
    let end1 = Point2::new(hinge.x - dir.x * width, hinge.y - dir.y * width);
    let d0 = distance_to_wall_path(g, end0);
    let d1 = distance_to_wall_path(g, end1);
    if d1 + 1e-9 < d0 {
        wall_ang + 180.0
    } else {
        wall_ang
    }
}

fn distance_to_wall_path(g: &Geometry, pt: Point2) -> f64 {
    closest_on_wall(g, pt)
        .map(|(d, ..)| d)
        .unwrap_or(f64::INFINITY)
}

fn as_face_segment(g: &Geometry) -> Option<Segment> {
    match g {
        Geometry::Line(s) => Some(*s),
        Geometry::Polyline(pl) if !pl.closed && pl.vertices.len() == 2 => {
            Some(Segment::new(pl.vertices[0], pl.vertices[1]))
        }
        _ => None,
    }
}

fn orient_same(a: Segment, b: Segment) -> Segment {
    match (a.direction(), b.direction()) {
        (Some(da), Some(db)) if da.dot(db) < 0.0 => Segment::new(b.b, b.a),
        _ => b,
    }
}

/// Find a parallel mate at 4½″ / 9″ (or metre equivalent) for `primary`.
fn find_face_pair(doc: &Document, primary_id: EntityId) -> Option<FacePair> {
    let primary = doc.get(primary_id)?;
    let a_seg = as_face_segment(&primary.geometry)?;
    let nominals = nominal_wall_thicknesses(doc);
    let mut best: Option<(f64, FacePair)> = None;
    for e in doc.entities() {
        if e.id == primary_id || !is_wall_entity(e) {
            continue;
        }
        let Some(raw_b) = as_face_segment(&e.geometry) else {
            continue;
        };
        if !a_seg.is_parallel(raw_b) {
            continue;
        }
        let b_seg = orient_same(a_seg, raw_b);
        let offset = a_seg.perpendicular_distance(b_seg.midpoint());
        if !thickness_is_nominal(offset, &nominals) {
            continue;
        }
        let overlap = a_seg.parallel_overlap(b_seg);
        if overlap < nominals[0].min(1.0) {
            // Need meaningful shared run (at least ~4.5" or 0.1 m).
            continue;
        }
        let score = (offset
            - nominals
                .iter()
                .copied()
                .min_by(|x, y| (offset - x).abs().total_cmp(&(offset - y).abs()))
                .unwrap_or(offset))
        .abs();
        let pair = FacePair {
            a_id: primary_id,
            a: a_seg,
            b_id: e.id,
            b: b_seg,
            thickness: offset,
            layer: primary.layer.clone(),
            attrs: primary.attributes.clone(),
        };
        if best.as_ref().map(|(s, _)| score < *s).unwrap_or(true) {
            best = Some((score, pair));
        }
    }
    best.map(|(_, p)| p)
}

/// Place helper: snap + orient. For double-line walls the insert sits on the
/// wall **centerline** between the two faces.
pub fn place_opening_pose(doc: &Document, click: Point2, width: f64) -> Option<(Point2, f64)> {
    let snap = snap_to_wall(doc, click)?;
    let entity = doc.get(snap.wall_id)?;
    let ang = orient_opening_along_wall(&entity.geometry, snap.point, width, snap.angle_deg);
    if let Some(pair) = find_face_pair(doc, snap.wall_id) {
        let on_a = project_clamped(pair.a, snap.point);
        let on_b = project_clamped(pair.b, snap.point);
        let mid = on_a.midpoint(on_b);
        let ang = orient_opening_along_wall(&Geometry::Line(pair.a), mid, width, ang);
        return Some((mid, ang));
    }
    Some((snap.point, ang))
}

/// Cut a WIDTH opening at `hinge` along `angle_deg`.
///
/// Double-line walls (4½″ / 9″): both faces are trimmed and closed with jambs
/// into hatchable polylines. Single walls: residual path pieces.
///
/// Returns how many wall entities remain after the cut.
pub fn cut_wall_opening(
    doc: &mut Document,
    hinge: Point2,
    angle_deg: f64,
    width: f64,
) -> Result<usize, String> {
    if width <= LENGTH_EPS {
        return Err("opening width must be positive".into());
    }
    let snap = snap_to_wall(doc, hinge).ok_or("no wall near the opening")?;
    if let Some(pair) = find_face_pair(doc, snap.wall_id) {
        return cut_double_face_opening(doc, pair, hinge, angle_deg, width);
    }
    cut_single_wall_opening(doc, snap.wall_id, hinge, angle_deg, width)
}

fn cut_single_wall_opening(
    doc: &mut Document,
    wall_id: EntityId,
    hinge: Point2,
    angle_deg: f64,
    width: f64,
) -> Result<usize, String> {
    let entity = doc
        .get(wall_id)
        .ok_or_else(|| format!("wall {} missing", wall_id.0))?
        .clone();
    let rad = angle_deg.to_radians();
    let dir = Vec2::new(rad.cos(), rad.sin());
    let far = Point2::new(hinge.x + dir.x * width, hinge.y + dir.y * width);
    let pieces = match &entity.geometry {
        Geometry::Line(seg) => cut_line_gap(*seg, hinge, far)?,
        Geometry::Polyline(pl) if !pl.closed => cut_polyline_gap(pl, hinge, far)?,
        _ => return Err("wall must be a line or open polyline".into()),
    };
    let n = pieces.len();
    let layer = entity.layer.clone();
    let attrs = entity.attributes.clone();
    doc.transact("WALLCUT", |d| {
        d.remove_entity(wall_id);
        for g in pieces {
            d.add_entity(layer.clone(), g, attrs.clone());
        }
    });
    Ok(n)
}

fn cut_double_face_opening(
    doc: &mut Document,
    pair: FacePair,
    hinge: Point2,
    angle_deg: f64,
    width: f64,
) -> Result<usize, String> {
    let rad = angle_deg.to_radians();
    let dir = Vec2::new(rad.cos(), rad.sin());
    let far = Point2::new(hinge.x + dir.x * width, hinge.y + dir.y * width);

    let (a_lo, a_hi) = ordered_gap_ends(pair.a, hinge, far)?;
    let (b_lo, b_hi) = ordered_gap_ends(pair.b, hinge, far)?;

    // Align "lo" jambs and "hi" jambs across faces by nearest pairing.
    let (b_lo, b_hi) = if a_lo.distance(b_lo) + a_hi.distance(b_hi)
        <= a_lo.distance(b_hi) + a_hi.distance(b_lo)
    {
        (b_lo, b_hi)
    } else {
        (b_hi, b_lo)
    };

    let mut bodies: Vec<Geometry> = Vec::new();
    // Left wall body (start → gap): closed for hatch.
    if let Some(poly) = closed_wall_body(pair.a.a, a_lo, b_lo, pair.b.a) {
        bodies.push(Geometry::Polyline(poly));
    }
    // Right wall body (gap → end): closed for hatch.
    if let Some(poly) = closed_wall_body(a_hi, pair.a.b, pair.b.b, b_hi) {
        bodies.push(Geometry::Polyline(poly));
    }
    if bodies.is_empty() {
        return Err("opening consumed the whole wall".into());
    }

    let n = bodies.len();
    let mut attrs = pair.attrs;
    attrs.insert("AADT-ELEM".into(), "wall".into());
    attrs.insert(
        "AADT-WALL-THICKNESS".into(),
        format!("{:.6}", pair.thickness),
    );
    let layer = pair.layer;
    let a_id = pair.a_id;
    let b_id = pair.b_id;
    doc.transact("WALLCUT", |d| {
        d.remove_entity(a_id);
        d.remove_entity(b_id);
        for g in bodies {
            d.add_entity(layer.clone(), g, attrs.clone());
        }
    });
    Ok(n)
}

/// Closed ring `p0 → p1 → p2 → p3 → p0` when it has area; skips degenerate stubs.
fn closed_wall_body(p0: Point2, p1: Point2, p2: Point2, p3: Point2) -> Option<Polyline> {
    let min_edge = LENGTH_EPS * 50.0;
    // Need a real face run on at least one side.
    if p0.distance(p1) < min_edge && p3.distance(p2) < min_edge {
        return None;
    }
    let mut verts = vec![p0, p1, p2, p3];
    verts.dedup_by(|a, b| a.coincident(*b));
    if verts.len() < 3 {
        return None;
    }
    let poly = Polyline::new(verts, true);
    if poly.area() < min_edge * min_edge {
        return None;
    }
    Some(poly)
}

fn ordered_gap_ends(seg: Segment, hinge: Point2, far: Point2) -> Result<(Point2, Point2), String> {
    let a = project_clamped(seg, hinge);
    let b = project_clamped(seg, far);
    let ta = seg.project_param(a).clamp(0.0, 1.0);
    let tb = seg.project_param(b).clamp(0.0, 1.0);
    if (ta - tb).abs() * seg.length() <= LENGTH_EPS {
        return Err("opening does not land on the wall face".into());
    }
    if ta <= tb {
        Ok((a, b))
    } else {
        Ok((b, a))
    }
}

fn cut_line_gap(seg: Segment, hinge: Point2, far: Point2) -> Result<Vec<Geometry>, String> {
    let (lo, hi) = ordered_gap_ends(seg, hinge, far)?;
    let mut out = Vec::new();
    if seg.a.distance(lo) > LENGTH_EPS * 10.0 {
        out.push(Geometry::Line(Segment::new(seg.a, lo)));
    }
    if hi.distance(seg.b) > LENGTH_EPS * 10.0 {
        out.push(Geometry::Line(Segment::new(hi, seg.b)));
    }
    Ok(out)
}

fn project_clamped(seg: Segment, p: Point2) -> Point2 {
    let t = seg.project_param(p).clamp(0.0, 1.0);
    Point2::new(
        seg.a.x + (seg.b.x - seg.a.x) * t,
        seg.a.y + (seg.b.y - seg.a.y) * t,
    )
}

fn cut_polyline_gap(pl: &Polyline, hinge: Point2, far: Point2) -> Result<Vec<Geometry>, String> {
    let verts = &pl.vertices;
    if verts.len() < 2 {
        return Err("wall polyline too short".into());
    }
    let total: f64 = verts.windows(2).map(|w| w[0].distance(w[1])).sum();
    if total <= LENGTH_EPS {
        return Err("wall polyline has zero length".into());
    }
    let s0 = path_length_at(verts, hinge).clamp(0.0, total);
    let s1 = path_length_at(verts, far).clamp(0.0, total);
    let (lo, hi) = if s0 <= s1 { (s0, s1) } else { (s1, s0) };
    if hi - lo <= LENGTH_EPS {
        return Err("opening does not land on the wall".into());
    }
    let mut out = Vec::new();
    if let Some(left) = polyline_slice(verts, 0.0, lo) {
        if left.len() >= 2 {
            out.push(Geometry::Polyline(Polyline::new(left, false)));
        }
    }
    if let Some(right) = polyline_slice(verts, hi, total) {
        if right.len() >= 2 {
            out.push(Geometry::Polyline(Polyline::new(right, false)));
        }
    }
    Ok(out)
}

fn path_length_at(verts: &[Point2], p: Point2) -> f64 {
    let mut cum = 0.0;
    let mut best = (f64::INFINITY, 0.0);
    for w in verts.windows(2) {
        let seg = Segment::new(w[0], w[1]);
        let d = seg.distance_to_point(p);
        if d < best.0 {
            let along = match seg.direction() {
                Some(dir) => (p - w[0]).dot(dir).clamp(0.0, seg.length()),
                None => 0.0,
            };
            best = (d, cum + along);
        }
        cum += seg.length();
    }
    best.1
}

fn point_at_length(verts: &[Point2], s: f64) -> Point2 {
    if verts.is_empty() {
        return Point2::new(0.0, 0.0);
    }
    if s <= LENGTH_EPS {
        return verts[0];
    }
    let mut rem = s;
    for w in verts.windows(2) {
        let len = w[0].distance(w[1]);
        if rem <= len + LENGTH_EPS {
            if len <= LENGTH_EPS {
                return w[1];
            }
            let t = (rem / len).clamp(0.0, 1.0);
            return Point2::new(
                w[0].x + (w[1].x - w[0].x) * t,
                w[0].y + (w[1].y - w[0].y) * t,
            );
        }
        rem -= len;
    }
    *verts.last().unwrap()
}

fn polyline_slice(verts: &[Point2], s0: f64, s1: f64) -> Option<Vec<Point2>> {
    if s1 - s0 <= LENGTH_EPS {
        return None;
    }
    let start = point_at_length(verts, s0);
    let end = point_at_length(verts, s1);
    let mut out = vec![start];
    let mut cum = 0.0;
    for w in verts.windows(2) {
        let len = w[0].distance(w[1]);
        let seg_end = cum + len;
        if seg_end > s0 + LENGTH_EPS && seg_end < s1 - LENGTH_EPS {
            out.push(w[1]);
        }
        cum = seg_end;
    }
    if out.last().map(|p| !p.coincident(end)).unwrap_or(true) {
        out.push(end);
    }
    out.dedup_by(|a, b| a.coincident(*b));
    if out.len() < 2 {
        None
    } else {
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aadt_model::Document;

    fn wall_line(doc: &mut Document, a: Point2, b: Point2) -> EntityId {
        let mut attrs = HashMap::new();
        attrs.insert("AADT-ELEM".into(), "wall".into());
        doc.add_entity("A-WALL", Geometry::Line(Segment::new(a, b)), attrs)
    }

    fn set_ft_in(doc: &mut Document) {
        let mut p = doc.properties().clone();
        p.units = "ft-in".into();
        doc.set_properties(p);
    }

    fn set_meters(doc: &mut Document) {
        let mut p = doc.properties().clone();
        p.units = "m".into();
        doc.set_properties(p);
    }

    #[test]
    fn cut_opens_a_gap_in_a_line_wall() {
        let mut doc = Document::new();
        let _id = wall_line(&mut doc, Point2::new(0.0, 0.0), Point2::new(10.0, 0.0));
        let n = cut_wall_opening(&mut doc, Point2::new(4.0, 0.05), 0.0, 0.9).unwrap();
        assert_eq!(n, 2);
    }

    #[test]
    fn double_face_9in_wall_becomes_closed_bodies() {
        let mut doc = Document::new();
        set_ft_in(&mut doc);
        // Two parallel faces 9" apart (model inches).
        wall_line(&mut doc, Point2::new(0.0, 0.0), Point2::new(240.0, 0.0));
        wall_line(&mut doc, Point2::new(0.0, 9.0), Point2::new(240.0, 9.0));
        // 36" door opening.
        let n = cut_wall_opening(&mut doc, Point2::new(100.0, 4.5), 0.0, 36.0).unwrap();
        assert_eq!(n, 2, "left + right closed wall bodies");
        assert_eq!(doc.entities().len(), 2);
        for e in doc.entities() {
            let Geometry::Polyline(pl) = &e.geometry else {
                panic!("expected closed polyline, got {:?}", e.geometry);
            };
            assert!(pl.closed, "wall body must be closed for hatch");
            assert!(pl.area() > 1.0, "wall body has area");
        }
    }

    #[test]
    fn double_face_metric_230mm_wall_is_recognised() {
        let mut doc = Document::new();
        set_meters(&mut doc);
        wall_line(&mut doc, Point2::new(0.0, 0.0), Point2::new(12.0, 0.0));
        wall_line(&mut doc, Point2::new(0.0, 0.230), Point2::new(12.0, 0.230));
        let width = opening_width_for_block(&doc, "DOOR-SINGLE");
        assert!((width - 0.9).abs() < 1e-9);
        let n = cut_wall_opening(&mut doc, Point2::new(4.0, 0.115), 0.0, width).unwrap();
        assert_eq!(n, 2);
        assert!(doc.entities().iter().all(|e| matches!(
            &e.geometry,
            Geometry::Polyline(pl) if pl.closed
        )));
    }

    #[test]
    fn opening_width_follows_drawing_units() {
        let mut doc = Document::new();
        set_ft_in(&mut doc);
        let inch = opening_width_for_block(&doc, "DOOR-SINGLE");
        assert!((inch - 0.9 * INCHES_PER_METER).abs() < 1e-6);
        set_meters(&mut doc);
        let metre = opening_width_for_block(&doc, "DOOR-SINGLE");
        assert!((metre - 0.9).abs() < 1e-9);
    }

    #[test]
    fn double_face_4_5in_wall_is_recognised() {
        let mut doc = Document::new();
        set_ft_in(&mut doc);
        wall_line(&mut doc, Point2::new(0.0, 0.0), Point2::new(120.0, 0.0));
        wall_line(&mut doc, Point2::new(0.0, 4.5), Point2::new(120.0, 4.5));
        let n = cut_wall_opening(&mut doc, Point2::new(40.0, 2.25), 0.0, 30.0).unwrap();
        assert_eq!(n, 2);
        assert!(doc.entities().iter().all(|e| matches!(
            &e.geometry,
            Geometry::Polyline(pl) if pl.closed
        )));
    }

    #[test]
    fn snap_aligns_to_wall_and_orients_opening() {
        let mut doc = Document::new();
        wall_line(&mut doc, Point2::new(0.0, 0.0), Point2::new(10.0, 0.0));
        let (pt, ang) = place_opening_pose(&doc, Point2::new(3.0, 0.1), 0.9).unwrap();
        assert!((pt.y).abs() < 1e-6);
        assert!((pt.x - 3.0).abs() < 1e-6);
        let a = ang.rem_euclid(360.0);
        assert!(a < 1.0 || (a - 360.0).abs() < 1.0, "got {ang}");
    }

    #[test]
    fn place_pose_on_double_face_uses_centerline() {
        let mut doc = Document::new();
        set_ft_in(&mut doc);
        wall_line(&mut doc, Point2::new(0.0, 0.0), Point2::new(100.0, 0.0));
        wall_line(&mut doc, Point2::new(0.0, 9.0), Point2::new(100.0, 9.0));
        let (pt, _) = place_opening_pose(&doc, Point2::new(50.0, 0.2), 36.0).unwrap();
        assert!(
            (pt.y - 4.5).abs() < 1e-6,
            "insert on centerline, got y={}",
            pt.y
        );
    }
}
