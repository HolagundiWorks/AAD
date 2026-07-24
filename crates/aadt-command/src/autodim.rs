//! Quick dim (QDIM) and auto-dimension (AUTODIM) for plan sets.
//!
//! Isolates wall / window / structural layers, then places overall and chain
//! aligned dimensions from entity endpoints.

use aadt_geometry::{Bbox, Point2};
use aadt_model::{Dimension, DimensionKind, Document, EntityId, Geometry, Layer};

use crate::{
    entities_word, parse_f64, resolve_targets, Command, Context, Registry,
};

/// Canonical dimension layer (HCW `AN-DIMS`). Aliases remain accepted via [`is_dim_layer`].
pub const DEFAULT_DIM_LAYER: &str = "AN-DIMS";
const STATION_EPS: f64 = 1e-4;
/// Dim-line clearance as a multiple of (dim text height × dim scale).
const DIM_OFFSET_FACTOR: f64 = 2.5;
const DIM_OFFSET_MIN: f64 = 0.5;
const DIM_TEXT_HEIGHT_FALLBACK: f64 = 0.18;

pub fn register_autodim(r: &mut Registry) {
    r.register(Box::new(QDimCmd));
    r.register(Box::new(AutoDimCmd));
    r.register(Box::new(AutoDimRestoreCmd));
}

/// Walls, openings, and structural layers kept visible during AUTODIM.
pub fn is_structure_layer(name: &str) -> bool {
    let u = name.to_ascii_uppercase();
    const PREFIXES: &[&str] = &[
        "A-WALL", "A-GLAZ", "A-WIND", "A-DOOR", "A-COLS", "A-COL", "S-COLS", "S-COL",
        "S-BEAM", "S-JOIST", "S-WALL", "S-FNDN", "S-SLAB", "S-BRAC", "S-GRID",
    ];
    PREFIXES.iter().any(|p| u.starts_with(p))
}

/// Dimension layers kept on so new dims remain visible after isolate.
pub fn is_dim_layer(name: &str) -> bool {
    let u = name.to_ascii_uppercase();
    matches!(
        u.as_str(),
        "A-ANNO-DIMS" | "A-DIMS" | "AN-DIMS" | "S-DIMS" | "BP-DIM"
    ) || u.starts_with("A-ANNO-DIM")
        || u.starts_with("A-DIM")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Top,
    Bottom,
    Left,
    Right,
}

fn dim_offset(ctx: &Context) -> f64 {
    let props = ctx.doc.properties();
    let h = props
        .dim_text_height
        .parse::<f64>()
        .unwrap_or(DIM_TEXT_HEIGHT_FALLBACK);
    let s = props.dim_scale.parse::<f64>().unwrap_or(1.0);
    (h.max(0.05) * s.max(0.01) * DIM_OFFSET_FACTOR).max(DIM_OFFSET_MIN)
}

fn collect_def_points(ctx: &Context, ids: &[EntityId]) -> Vec<Point2> {
    let mut pts = Vec::new();
    for id in ids {
        let Some(e) = ctx.doc.get(*id) else {
            continue;
        };
        match &e.geometry {
            Geometry::Line(seg) => {
                pts.push(seg.a);
                pts.push(seg.b);
            }
            Geometry::Polyline(p) => {
                pts.extend(p.vertices.iter().copied());
            }
            Geometry::Circle(c) => {
                // Column / round pier: center + cardinal extents.
                pts.push(c.center);
                pts.push(Point2::new(c.center.x - c.radius, c.center.y));
                pts.push(Point2::new(c.center.x + c.radius, c.center.y));
                pts.push(Point2::new(c.center.x, c.center.y - c.radius));
                pts.push(Point2::new(c.center.x, c.center.y + c.radius));
            }
            Geometry::Arc(a) => {
                pts.push(a.center);
                pts.push(a.start_point());
                pts.push(a.end_point());
            }
            Geometry::Text(_) | Geometry::Insert(_) => {}
        }
        // Also harvest segment endpoints (covers line/polyline consistently).
        for seg in e.geometry.segments() {
            pts.push(seg.a);
            pts.push(seg.b);
        }
    }
    pts
}

fn unique_sorted(mut vals: Vec<f64>, eps: f64) -> Vec<f64> {
    vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mut out = Vec::new();
    for v in vals {
        if out
            .last()
            .map(|p: &f64| (v - *p).abs() > eps)
            .unwrap_or(true)
        {
            out.push(v);
        }
    }
    out
}

fn side_from_pick(bbox: Bbox, p: Point2) -> Side {
    let mid_x = (bbox.min.x + bbox.max.x) * 0.5;
    let mid_y = (bbox.min.y + bbox.max.y) * 0.5;
    let out_l = (bbox.min.x - p.x).max(0.0);
    let out_r = (p.x - bbox.max.x).max(0.0);
    let out_b = (bbox.min.y - p.y).max(0.0);
    let out_t = (p.y - bbox.max.y).max(0.0);
    let outside = out_l.max(out_r).max(out_b).max(out_t);
    if outside > 0.0 {
        if outside == out_t {
            return Side::Top;
        }
        if outside == out_b {
            return Side::Bottom;
        }
        if outside == out_l {
            return Side::Left;
        }
        return Side::Right;
    }
    if (p.x - mid_x).abs() >= (p.y - mid_y).abs() {
        if p.x >= mid_x {
            Side::Right
        } else {
            Side::Left
        }
    } else if p.y >= mid_y {
        Side::Top
    } else {
        Side::Bottom
    }
}

fn make_aligned(layer: &str, p1: Point2, p2: Point2, text_anchor: Point2) -> Option<Dimension> {
    if p1.coincident(p2) {
        return None;
    }
    let measurement = p1.distance(p2);
    if measurement < STATION_EPS {
        return None;
    }
    Some(Dimension {
        layer: layer.to_string(),
        kind: DimensionKind::Aligned,
        p1,
        p2,
        vertex: p1,
        text_anchor,
        text: String::new(),
        measurement,
    })
}

fn chain_dims(side: Side, bbox: Bbox, stations: &[f64], offset: f64, layer: &str) -> Vec<Dimension> {
    let mut dims = Vec::new();
    if stations.len() < 2 {
        return dims;
    }
    match side {
        Side::Top => {
            let y = bbox.max.y;
            let ty = y + offset;
            for w in stations.windows(2) {
                let p1 = Point2::new(w[0], y);
                let p2 = Point2::new(w[1], y);
                let mid = Point2::new((w[0] + w[1]) * 0.5, ty);
                if let Some(d) = make_aligned(layer, p1, p2, mid) {
                    dims.push(d);
                }
            }
        }
        Side::Bottom => {
            let y = bbox.min.y;
            let ty = y - offset;
            for w in stations.windows(2) {
                let p1 = Point2::new(w[0], y);
                let p2 = Point2::new(w[1], y);
                let mid = Point2::new((w[0] + w[1]) * 0.5, ty);
                if let Some(d) = make_aligned(layer, p1, p2, mid) {
                    dims.push(d);
                }
            }
        }
        Side::Left => {
            let x = bbox.min.x;
            let tx = x - offset;
            for w in stations.windows(2) {
                let p1 = Point2::new(x, w[0]);
                let p2 = Point2::new(x, w[1]);
                let mid = Point2::new(tx, (w[0] + w[1]) * 0.5);
                if let Some(d) = make_aligned(layer, p1, p2, mid) {
                    dims.push(d);
                }
            }
        }
        Side::Right => {
            let x = bbox.max.x;
            let tx = x + offset;
            for w in stations.windows(2) {
                let p1 = Point2::new(x, w[0]);
                let p2 = Point2::new(x, w[1]);
                let mid = Point2::new(tx, (w[0] + w[1]) * 0.5);
                if let Some(d) = make_aligned(layer, p1, p2, mid) {
                    dims.push(d);
                }
            }
        }
    }
    dims
}

fn overall_dims(bbox: Bbox, offset: f64, layer: &str) -> Vec<Dimension> {
    let mut dims = Vec::new();
    let far = offset * 2.0;
    // Horizontal overall on top.
    if let Some(d) = make_aligned(
        layer,
        Point2::new(bbox.min.x, bbox.max.y),
        Point2::new(bbox.max.x, bbox.max.y),
        Point2::new((bbox.min.x + bbox.max.x) * 0.5, bbox.max.y + far),
    ) {
        dims.push(d);
    }
    // Vertical overall on left.
    if let Some(d) = make_aligned(
        layer,
        Point2::new(bbox.min.x, bbox.min.y),
        Point2::new(bbox.min.x, bbox.max.y),
        Point2::new(bbox.min.x - far, (bbox.min.y + bbox.max.y) * 0.5),
    ) {
        dims.push(d);
    }
    dims
}

fn snapshot_layers(ctx: &mut Context) -> Vec<(String, bool, bool)> {
    let snap: Vec<(String, bool, bool)> = ctx
        .doc
        .layers()
        .map(|l| (l.name.clone(), l.visible, l.frozen))
        .collect();
    ctx.layer_iso_snapshot = Some(snap.clone());
    snap
}

/// Isolate structure + dim layers into an open transaction (HCW: one undo unit).
fn isolate_structure_layers_into(d: &mut Document, snap: &[(String, bool, bool)]) -> usize {
    let mut kept = 0usize;
    for (name, _, _) in snap {
        let keep = is_structure_layer(name) || is_dim_layer(name);
        d.set_layer_visible(name, keep);
        if keep {
            d.set_layer_frozen(name, false);
            kept += 1;
        }
    }
    if d.layer(DEFAULT_DIM_LAYER).is_none() {
        d.add_layer(Layer::new(DEFAULT_DIM_LAYER));
    }
    d.set_layer_visible(DEFAULT_DIM_LAYER, true);
    d.set_layer_frozen(DEFAULT_DIM_LAYER, false);
    kept
}

pub(crate) fn restore_layer_iso(ctx: &mut Context) -> Result<usize, String> {
    let snap = ctx
        .layer_iso_snapshot
        .take()
        .ok_or("no layer isolate snapshot")?;
    let n = snap.len();
    ctx.doc.transact("LAYUNISO", |d| {
        for (name, vis, frz) in &snap {
            d.set_layer_visible(name, *vis);
            d.set_layer_frozen(name, *frz);
        }
    });
    Ok(n)
}

fn filter_structure_ids(ctx: &Context, ids: &[EntityId]) -> Vec<EntityId> {
    ids.iter()
        .copied()
        .filter(|id| {
            ctx.doc
                .get(*id)
                .map(|e| is_structure_layer(&e.layer))
                .unwrap_or(false)
        })
        .collect()
}

fn build_complete_set(
    ctx: &Context,
    ids: &[EntityId],
    layer: &str,
) -> Result<Vec<Dimension>, String> {
    let pts = collect_def_points(ctx, ids);
    let bbox = Bbox::of_points(pts.iter().copied())
        .ok_or("AUTODIM: no geometry points in target set")?;
    if bbox.width() < STATION_EPS && bbox.height() < STATION_EPS {
        return Err("AUTODIM: selection has no extent".into());
    }
    let offset = dim_offset(ctx);
    let xs = unique_sorted(pts.iter().map(|p| p.x).collect(), STATION_EPS);
    let ys = unique_sorted(pts.iter().map(|p| p.y).collect(), STATION_EPS);

    let mut dims = overall_dims(bbox, offset, layer);
    for side in [Side::Top, Side::Bottom, Side::Left, Side::Right] {
        let stations = match side {
            Side::Top | Side::Bottom => &xs,
            Side::Left | Side::Right => &ys,
        };
        dims.extend(chain_dims(side, bbox, stations, offset, layer));
    }
    if dims.is_empty() {
        return Err("AUTODIM: no dimensions could be placed".into());
    }
    Ok(dims)
}

fn place_qdim_chain(
    ctx: &mut Context,
    ids: &[EntityId],
    side_pt: Point2,
    layer: &str,
) -> Result<usize, String> {
    let pts = collect_def_points(ctx, ids);
    let bbox = Bbox::of_points(pts.iter().copied())
        .ok_or("QDIM: no geometry points in target set")?;
    let side = side_from_pick(bbox, side_pt);
    let offset = {
        // Prefer distance from pick to the chosen edge when outside.
        let base = dim_offset(ctx);
        let dist = match side {
            Side::Top => (side_pt.y - bbox.max.y).abs(),
            Side::Bottom => (bbox.min.y - side_pt.y).abs(),
            Side::Left => (bbox.min.x - side_pt.x).abs(),
            Side::Right => (side_pt.x - bbox.max.x).abs(),
        };
        dist.max(base)
    };
    let stations = match side {
        Side::Top | Side::Bottom => unique_sorted(pts.iter().map(|p| p.x).collect(), STATION_EPS),
        Side::Left | Side::Right => unique_sorted(pts.iter().map(|p| p.y).collect(), STATION_EPS),
    };
    let dims = chain_dims(side, bbox, &stations, offset, layer);
    let n = dims.len();
    if n == 0 {
        return Err("QDIM: need at least two distinct stations on that side".into());
    }
    ctx.doc.transact("QDIM", |d| {
        for dim in dims {
            d.add_dimension(dim);
        }
    });
    Ok(n)
}

// --- QDIM -------------------------------------------------------------------

struct QDimCmd;
impl Command for QDimCmd {
    fn name(&self) -> &str {
        "QDIM"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["QUICKDIM"]
    }
    fn help(&self) -> &str {
        "QDIM SELECTION|ALL sx sy [layer] — chain dims along the side nearest (sx,sy)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let token = args
            .first()
            .map(|s| s.as_str())
            .unwrap_or("SELECTION");
        let ids = resolve_targets(ctx, token, "QDIM")?;
        let sx = parse_f64(args, 1, "QDIM")?;
        let sy = parse_f64(args, 2, "QDIM")?;
        let layer = args
            .get(3)
            .cloned()
            .unwrap_or_else(|| DEFAULT_DIM_LAYER.to_string());
        let n = place_qdim_chain(ctx, &ids, Point2::new(sx, sy), &layer)?;
        Ok(format!(
            "QDIM placed {n} dimension(s) on {} {}",
            ids.len(),
            entities_word(ids.len())
        ))
    }
}

// --- AUTODIM ----------------------------------------------------------------

struct AutoDimCmd;
impl Command for AutoDimCmd {
    fn name(&self) -> &str {
        "AUTODIM"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["ADIM"]
    }
    fn help(&self) -> &str {
        "AUTODIM [SELECTION|ALL] [RESTORE] — isolate structure layers and dimension the set"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let mut target = "SELECTION";
        let mut restore = false;
        for a in args {
            let u = a.to_ascii_uppercase();
            match u.as_str() {
                "RESTORE" => restore = true,
                "SELECTION" | "ALL" => target = a.as_str(),
                _ => {
                    return Err(format!(
                        "AUTODIM: unknown argument {a:?} (use SELECTION|ALL [RESTORE])"
                    ));
                }
            }
        }
        // Empty selection → ALL when SELECTION requested with nothing selected.
        let ids = if target.eq_ignore_ascii_case("SELECTION") && ctx.selection.is_empty() {
            resolve_targets(ctx, "ALL", "AUTODIM")?
        } else {
            resolve_targets(ctx, target, "AUTODIM")?
        };

        let structure = filter_structure_ids(ctx, &ids);
        let targets = if structure.is_empty() {
            // Fall back to all ids if none match structure prefixes (e.g. custom layers).
            ids.clone()
        } else {
            structure
        };

        let dims = build_complete_set(ctx, &targets, DEFAULT_DIM_LAYER)?;
        let n = dims.len();

        // One undoable transaction: isolate (unless RESTORE) + dimensions together.
        let kept = if restore {
            ctx.layer_iso_snapshot = None;
            ctx.doc.transact("AUTODIM", |d| {
                for dim in dims {
                    d.add_dimension(dim);
                }
            });
            0usize
        } else {
            let snap = snapshot_layers(ctx);
            ctx.doc.transact("AUTODIM", |d| {
                let kept = isolate_structure_layers_into(d, &snap);
                for dim in dims {
                    d.add_dimension(dim);
                }
                kept
            })
        };

        let mut msg = format!(
            "AUTODIM placed {n} dimension(s) on {} {}",
            targets.len(),
            entities_word(targets.len()),
        );
        if restore {
            msg.push_str("; layers unchanged (RESTORE)");
        } else {
            msg.push_str(&format!(
                "; {kept} structure/dim layer(s) isolated; use LAYUNISO or AUTODIMRESTORE to restore"
            ));
        }
        Ok(msg)
    }
}

struct AutoDimRestoreCmd;
impl Command for AutoDimRestoreCmd {
    fn name(&self) -> &str {
        "AUTODIMRESTORE"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["ADIMRESTORE"]
    }
    fn help(&self) -> &str {
        "AUTODIMRESTORE — restore layer visibility after AUTODIM (same as LAYUNISO)"
    }
    fn run(&self, ctx: &mut Context, _args: &[String]) -> Result<String, String> {
        let n = restore_layer_iso(ctx).map_err(|e| format!("AUTODIMRESTORE: {e}"))?;
        Ok(format!("restored visibility for {n} layer(s)"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aadt_geometry::{Polyline, Segment};
    use aadt_model::Geometry;
    use crate::Interpreter;

    fn interp() -> Interpreter {
        Interpreter::with_builtins()
    }

    #[test]
    fn structure_layer_prefixes() {
        assert!(is_structure_layer("A-WALL"));
        assert!(is_structure_layer("A-WALL-PRHT"));
        assert!(is_structure_layer("A-WIND"));
        assert!(is_structure_layer("A-GLAZ"));
        assert!(is_structure_layer("S-BEAM"));
        assert!(is_structure_layer("A-COL"));
        assert!(!is_structure_layer("A-FURN"));
        assert!(is_dim_layer("A-ANNO-DIMS"));
        assert!(is_dim_layer("AN-DIMS"));
    }

    #[test]
    fn autodim_rectangle_places_dims_and_isolates() {
        let i = interp();
        let mut ctx = Context::new();
        i.eval_line(&mut ctx, "LAYER A-WALL").unwrap();
        i.eval_line(&mut ctx, "LAYER A-FURN").unwrap();
        i.eval_line(&mut ctx, "LAYER A-ANNO-TEXT").unwrap();
        // Closed wall rectangle 0,0 → 10,0 → 10,8 → 0,8.
        ctx.doc.transact("t", |d| {
            d.add_geometry(
                "A-WALL",
                Geometry::Polyline(Polyline::new(
                    vec![
                        Point2::new(0.0, 0.0),
                        Point2::new(10.0, 0.0),
                        Point2::new(10.0, 8.0),
                        Point2::new(0.0, 8.0),
                    ],
                    true,
                )),
            );
            d.add_geometry(
                "A-FURN",
                Geometry::Line(Segment::new(Point2::new(1.0, 1.0), Point2::new(2.0, 1.0))),
            );
        });
        assert!(ctx.doc.layer("A-FURN").unwrap().visible);

        let msg = i.eval_line(&mut ctx, "AUTODIM ALL").unwrap().unwrap();
        assert!(msg.contains("dimension"), "{msg}");
        assert!(ctx.doc.dimensions().len() >= 2);
        assert!(!ctx.doc.layer("A-FURN").unwrap().visible);
        assert!(!ctx.doc.layer("A-ANNO-TEXT").unwrap().visible);
        assert!(ctx.doc.layer("A-WALL").unwrap().visible);
        assert!(ctx.layer_iso_snapshot.is_some());

        i.eval_line(&mut ctx, "AUTODIMRESTORE").unwrap();
        assert!(ctx.doc.layer("A-FURN").unwrap().visible);
        assert!(ctx.layer_iso_snapshot.is_none());
    }

    #[test]
    fn qdim_chain_on_two_segments() {
        let i = interp();
        let mut ctx = Context::new();
        i.eval_line(&mut ctx, "LAYER A-WALL").unwrap();
        ctx.doc.transact("t", |d| {
            d.add_geometry(
                "A-WALL",
                Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(4.0, 0.0))),
            );
            d.add_geometry(
                "A-WALL",
                Geometry::Line(Segment::new(Point2::new(4.0, 0.0), Point2::new(10.0, 0.0))),
            );
        });
        i.eval_line(&mut ctx, "SELECT ALL").unwrap();
        // Pick above the line → top chain with stations 0,4,10 → 2 dims.
        let msg = i.eval_line(&mut ctx, "QDIM SELECTION 5 2").unwrap().unwrap();
        assert!(msg.contains("2 dimension") || msg.contains("dimension"), "{msg}");
        assert_eq!(ctx.doc.dimensions().len(), 2);
    }

    #[test]
    fn autodim_restore_flag() {
        let i = interp();
        let mut ctx = Context::new();
        i.eval_line(&mut ctx, "LAYER A-WALL").unwrap();
        i.eval_line(&mut ctx, "LAYER A-FURN").unwrap();
        ctx.doc.transact("t", |d| {
            d.add_geometry(
                "A-WALL",
                Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(5.0, 0.0))),
            );
            d.add_geometry(
                "A-WALL",
                Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(0.0, 3.0))),
            );
        });
        i.eval_line(&mut ctx, "AUTODIM ALL RESTORE").unwrap();
        assert!(ctx.doc.layer("A-FURN").unwrap().visible);
        assert!(ctx.layer_iso_snapshot.is_none());
        assert!(!ctx.doc.dimensions().is_empty());
    }

    #[test]
    fn autodim_undo_restores_layers_and_dims() {
        let i = interp();
        let mut ctx = Context::new();
        i.eval_line(&mut ctx, "LAYER A-WALL").unwrap();
        i.eval_line(&mut ctx, "LAYER A-FURN").unwrap();
        ctx.doc.transact("t", |d| {
            d.add_geometry(
                "A-WALL",
                Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(8.0, 0.0))),
            );
            d.add_geometry(
                "A-WALL",
                Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(0.0, 4.0))),
            );
            d.add_geometry(
                "A-FURN",
                Geometry::Line(Segment::new(Point2::new(1.0, 1.0), Point2::new(2.0, 1.0))),
            );
        });
        assert!(ctx.doc.layer("A-FURN").unwrap().visible);
        let before_dims = ctx.doc.dimensions().len();
        i.eval_line(&mut ctx, "AUTODIM ALL").unwrap();
        assert!(!ctx.doc.layer("A-FURN").unwrap().visible);
        assert!(ctx.doc.dimensions().len() > before_dims);

        i.eval_line(&mut ctx, "UNDO").unwrap();
        assert!(ctx.doc.layer("A-FURN").unwrap().visible);
        assert_eq!(ctx.doc.dimensions().len(), before_dims);
    }
}
