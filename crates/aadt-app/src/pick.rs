//! Click pick — nearest entity within a tolerance (L1 SpatialGrid assist).
//!
//! Distance predicates match the egui viewport. An optional [`SpatialGrid`]
//! narrows candidates via [`aadt_vdb::query_index_point`]; the precise hit is
//! always the same nearest-distance rule as a full scan.
//!
//! Dimensions live in a separate annotation table (no [`EntityId`]) — use
//! [`pick_dimension`] for click select.

use aadt_geometry::{Point2, Segment};
use aadt_model::{Document, EntityId, Geometry};
use aadt_render::{dimension_commands, DrawCommand};
use aadt_vdb::{query_index_point, SpatialGrid};

/// True when an entity may be picked (visible + unlocked layer).
pub fn entity_pickable(doc: &Document, layer: &str) -> bool {
    if !doc.layer_shown(layer) {
        return false;
    }
    !doc.layer(layer).map(|l| l.locked).unwrap_or(false)
}

/// Shortest distance from `p` to drawn geometry (inserts expand to block content).
pub fn entity_distance(doc: &Document, g: &Geometry, p: Point2) -> f64 {
    match g {
        Geometry::Insert(_) => {
            let geoms = selection_geometries(doc, g);
            let mut best = f64::INFINITY;
            for sg in &geoms {
                best = best.min(entity_distance_primitive(sg, p));
            }
            if best.is_finite() {
                best
            } else if let Geometry::Insert(i) = g {
                p.distance(i.position)
            } else {
                f64::INFINITY
            }
        }
        other => entity_distance_primitive(other, p),
    }
}

fn entity_distance_primitive(g: &Geometry, p: Point2) -> f64 {
    match g {
        Geometry::Line(s) => s.distance_to_point(p),
        Geometry::Polyline(pl) => pl
            .segments()
            .iter()
            .map(|s| s.distance_to_point(p))
            .fold(f64::INFINITY, f64::min),
        Geometry::Circle(c) => (p.distance(c.center) - c.radius).abs(),
        Geometry::Arc(a) => {
            if a.contains_angle(a.angle_of(p)) {
                (p.distance(a.center) - a.radius).abs()
            } else {
                p.distance(a.start_point()).min(p.distance(a.end_point()))
            }
        }
        Geometry::Text(t) => p.distance(t.position),
        Geometry::Insert(i) => p.distance(i.position),
        Geometry::Image(img) => {
            let c = img.corners();
            let segs = [
                aadt_geometry::Segment::new(c[0], c[1]),
                aadt_geometry::Segment::new(c[1], c[2]),
                aadt_geometry::Segment::new(c[2], c[3]),
                aadt_geometry::Segment::new(c[3], c[0]),
            ];
            segs.iter()
                .map(|s| s.distance_to_point(p))
                .fold(f64::INFINITY, f64::min)
        }
    }
}

fn selection_geometries(doc: &Document, g: &Geometry) -> Vec<Geometry> {
    match g {
        Geometry::Insert(ins) => match doc.block(&ins.name) {
            Some(block) => block
                .entities
                .iter()
                .map(|be| {
                    be.geometry.instanced_scaled(
                        block.base,
                        ins.position,
                        ins.rotation,
                        ins.scale_x,
                        ins.scale_y,
                    )
                })
                .collect(),
            None => vec![g.clone()],
        },
        other => vec![other.clone()],
    }
}

/// Nearest pickable entity among `candidates` within `tol` of `p`.
pub fn pick_nearest_in_candidates(
    doc: &Document,
    p: Point2,
    tol: f64,
    candidates: impl IntoIterator<Item = EntityId>,
) -> Option<EntityId> {
    let active = doc.active_space();
    let mut best: Option<(f64, EntityId)> = None;
    for id in candidates {
        let Some(e) = doc.get(id) else {
            continue;
        };
        if e.space != active || !entity_pickable(doc, &e.layer) {
            continue;
        }
        let d = entity_distance(doc, &e.geometry, p);
        if d <= tol && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, e.id));
        }
    }
    best.map(|(_, id)| id)
}

/// Linear scan of the active model space (baseline / fallback).
pub fn pick_entity_linear(doc: &Document, p: Point2, tol: f64) -> Option<EntityId> {
    let active = doc.active_space();
    pick_nearest_in_candidates(
        doc,
        p,
        tol,
        doc.iter_entities_in_space(active).map(|e| e.id),
    )
}

/// SpatialGrid aperture → same distance winner as [`pick_entity_linear`].
pub fn pick_entity_indexed(
    doc: &Document,
    index: &SpatialGrid,
    p: Point2,
    tol: f64,
) -> Option<EntityId> {
    let cands = query_index_point(index, p, tol);
    if cands.is_empty() {
        return None;
    }
    pick_nearest_in_candidates(doc, p, tol, cands)
}

/// Prefer indexed pick when `index` is non-empty; otherwise linear.
pub fn pick_entity(doc: &Document, index: Option<&SpatialGrid>, p: Point2, tol: f64) -> Option<EntityId> {
    match index {
        Some(ix) if ix.len() > 0 => pick_entity_indexed(doc, ix, p, tol),
        _ => pick_entity_linear(doc, p, tol),
    }
}

/// Nearest dimension (index into [`Document::dimensions`]) within `tol`.
pub fn pick_dimension(doc: &Document, p: Point2, tol: f64) -> Option<usize> {
    pick_dimension_dist(doc, p, tol).map(|(_, i)| i)
}

/// Like [`pick_dimension`], but also returns the hit distance.
pub fn pick_dimension_dist(doc: &Document, p: Point2, tol: f64) -> Option<(f64, usize)> {
    let active = doc.active_space();
    let props = doc.properties();
    let mut best: Option<(f64, usize)> = None;
    for (i, dim) in doc.dimensions().iter().enumerate() {
        if dim.space != active || !entity_pickable(doc, &dim.layer) {
            continue;
        }
        let d = dimension_hit_dist(dim, props, p);
        if d <= tol && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, i));
        }
    }
    best
}

/// Nearest pickable entity within `tol`, with distance (for closest-wins vs dims).
pub fn pick_entity_dist(doc: &Document, p: Point2, tol: f64) -> Option<(f64, EntityId)> {
    let active = doc.active_space();
    let mut best: Option<(f64, EntityId)> = None;
    for e in doc.iter_entities_in_space(active) {
        if !entity_pickable(doc, &e.layer) {
            continue;
        }
        let d = entity_distance(doc, &e.geometry, p);
        if d <= tol && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, e.id));
        }
    }
    best
}

/// Min distance from `p` to a dimension's drawn primitives.
pub fn dimension_hit_dist(
    dim: &aadt_model::Dimension,
    props: &aadt_model::DrawingProperties,
    p: Point2,
) -> f64 {
    let mut best = f64::INFINITY;
    for cmd in dimension_commands(dim, props) {
        match cmd {
            DrawCommand::Polyline { points, .. } => {
                for w in points.windows(2) {
                    best = best.min(Segment::new(w[0], w[1]).distance_to_point(p));
                }
            }
            DrawCommand::Text { position, .. } => best = best.min(p.distance(position)),
            _ => {}
        }
    }
    best
}

/// Dimensions caught by a box. Window needs every drawn point inside; crossing
/// needs any point inside.
pub fn dimension_box_select(doc: &Document, lo: Point2, hi: Point2, crossing: bool) -> Vec<usize> {
    let active = doc.active_space();
    let props = doc.properties();
    let in_box = |p: Point2| p.x >= lo.x && p.x <= hi.x && p.y >= lo.y && p.y <= hi.y;
    let mut hits = Vec::new();
    for (i, dim) in doc.dimensions().iter().enumerate() {
        if dim.space != active || !entity_pickable(doc, &dim.layer) {
            continue;
        }
        let (mut all_in, mut any_in, mut saw) = (true, false, false);
        for cmd in dimension_commands(dim, props) {
            match cmd {
                DrawCommand::Polyline { points, .. } => {
                    for &pt in &points {
                        saw = true;
                        let inside = in_box(pt);
                        any_in |= inside;
                        all_in &= inside;
                    }
                }
                DrawCommand::Text { position, .. } => {
                    saw = true;
                    let inside = in_box(position);
                    any_in |= inside;
                    all_in &= inside;
                }
                _ => {}
            }
        }
        if saw && if crossing { any_in } else { all_in } {
            hits.push(i);
        }
    }
    hits
}

/// Append world-space segments for selected dimension indices into `out`
/// (x0,y0,x1,y1 floats) for selection highlight.
pub fn push_dimension_selection_segs(doc: &Document, indices: &[usize], out: &mut Vec<f32>) {
    let props = doc.properties();
    let dims = doc.dimensions();
    for &idx in indices {
        let Some(dim) = dims.get(idx) else {
            continue;
        };
        for cmd in dimension_commands(dim, props) {
            if let DrawCommand::Polyline { points, .. } = cmd {
                for w in points.windows(2) {
                    out.push(w[0].x as f32);
                    out.push(w[0].y as f32);
                    out.push(w[1].x as f32);
                    out.push(w[1].y as f32);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aadt_geometry::{Circle, Point2, Segment};
    use aadt_model::{Document, Geometry};
    use aadt_vdb::build_index;

    #[test]
    fn indexed_pick_matches_linear_on_sparse_grid() {
        let mut doc = Document::new();
        for i in 0..25 {
            let x = (i % 5) as f64 * 100.0;
            let y = (i / 5) as f64 * 100.0;
            doc.add_geometry("0", Geometry::Circle(Circle::new(Point2::new(x, y), 1.0)));
        }
        let index = build_index(&doc);
        let p = Point2::new(200.0, 100.0);
        let tol = 2.0;
        let linear = pick_entity_linear(&doc, p, tol).unwrap();
        let indexed = pick_entity_indexed(&doc, &index, p, tol).unwrap();
        assert_eq!(linear, indexed);
        let cands = query_index_point(&index, p, tol);
        assert_eq!(cands.len(), 1);
        assert!(cands.len() < doc.len());
    }

    #[test]
    fn locked_layer_not_picked() {
        let mut doc = Document::new();
        doc.add_layer(aadt_model::Layer::new("LOCK"));
        doc.set_layer_locked("LOCK", true);
        doc.add_geometry(
            "LOCK",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0))),
        );
        let index = build_index(&doc);
        assert!(pick_entity(&doc, Some(&index), Point2::new(5.0, 0.0), 1.0).is_none());
        assert!(pick_entity_linear(&doc, Point2::new(5.0, 0.0), 1.0).is_none());
    }

    #[test]
    fn empty_aperture_returns_none() {
        let mut doc = Document::new();
        doc.add_geometry(
            "0",
            Geometry::Circle(Circle::new(Point2::new(0.0, 0.0), 1.0)),
        );
        let index = build_index(&doc);
        assert!(pick_entity_indexed(&doc, &index, Point2::new(50.0, 50.0), 1.0).is_none());
    }
}
