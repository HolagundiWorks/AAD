//! Spatial queries: where things are in the drawing. Read-only, bounded and deterministic; they are
//! what the assistant uses to know the layout of a drawing (an overview grid, what lies in a window,
//! what is nearest a point, the details of named objects), and anyone can call them.
//!
//! Coordinates are drawing units with Y up, in the space being edited (model space or the current
//! layout). Positions come from each object's bounds (`aad_doc::entity_bounds`), so a block
//! reference or a dimension is located by what it draws, not just by its insertion point. Every
//! result has a size limit and says when it was cut (`truncated`); nothing here changes the drawing.

use aad_doc::{Drawing, Entity, EntityKind, Handle};
use aad_geom::{Bounds2, Vec2};
use serde_json::{Value, json};

use super::*;
use crate::{Result, Session};

/// Most objects one query returns.
pub const MAX_OBJECTS: usize = 50;
/// Most objects one pass looks at (a hostile or huge drawing is cut, and the answer says so).
const MAX_SCAN: usize = 2_000_000;
const MAX_CELLS: usize = 12;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("spatial.overview", "Spatial Overview", run_overview)
            .params("{cols?: 1..12 (default 6), rows?: 1..12 (default 4)} → {space, extents, view, cell: [w, h], cells: [{col, row, min, max, count, types, layers}]} (a grid over the drawing: which parts hold what; empty cells are left out; Y up)")
            .noundo(),
        CommandSpec::new("spatial.query", "Objects in a Window", run_query)
            .params("{window: [x0, y0, x1, y1], mode?: inside|crossing (default crossing), layers?: [name], types?: [Line|Circle|Text|...], limit?: 1..50 (default 25)} → {window, matched, shown, truncated, objects: [{handle, type, layer, min, max, center, ...}]} (objects whose bounds are inside, or touch, the window)")
            .noundo(),
        CommandSpec::new("spatial.nearest", "Nearest Objects", run_nearest)
            .params("{point: [x, y], count?: 1..20 (default 5), layers?: [name], types?: [type]} → {point, objects: [{..., distance}]} (distance to the object's bounds, 0 when the point is inside them)")
            .noundo(),
        CommandSpec::new("spatial.describe", "Describe Objects", run_describe)
            .params("{handles: [hex handle] (at most 20)} → {objects: [{handle, type, layer, min, max, center, ...geometry}], missing: [handle]}")
            .noundo(),
    ]
}

fn pt(v: Vec2) -> Value {
    json!([round(v.x), round(v.y)])
}

/// Three decimals are plenty for a position and keep results short.
fn round(x: f64) -> f64 {
    if x.is_finite() { (x * 1000.0).round() / 1000.0 } else { 0.0 }
}

fn bounds_json(b: &Bounds2) -> (Value, Value, Value) {
    (pt(b.min), pt(b.max), pt(b.center()))
}

/// What an object is and where: handle, type, layer, bounds and centre, and the few numbers that
/// matter for its kind (a line's ends, a circle's radius, a text's words, a block's name).
pub(crate) fn describe(d: &Drawing, e: &Entity) -> Value {
    let b = aad_doc::entity_bounds(d, e, 0);
    let mut v = json!({"handle": e.handle.hex(), "type": e.kind.type_name(), "layer": e.common.layer});
    if let Some(o) = v.as_object_mut() {
        if !b.is_empty() {
            let (min, max, center) = bounds_json(&b);
            o.insert("min".into(), min);
            o.insert("max".into(), max);
            o.insert("center".into(), center);
        }
        match &e.kind {
            EntityKind::Line(l) => {
                o.insert("from".into(), pt(l.a.xy()));
                o.insert("to".into(), pt(l.b.xy()));
                o.insert("length".into(), json!(round((l.b.xy() - l.a.xy()).len())));
            }
            EntityKind::Circle(c) => {
                o.insert("center".into(), pt(c.center.xy()));
                o.insert("radius".into(), json!(round(c.radius)));
            }
            EntityKind::Arc(a) => {
                o.insert("center".into(), pt(a.center.xy()));
                o.insert("radius".into(), json!(round(a.radius)));
            }
            EntityKind::LwPolyline(p) => {
                o.insert("vertices".into(), json!(p.vertices.len()));
                o.insert("closed".into(), json!(p.closed));
            }
            EntityKind::Text(t) => {
                o.insert("at".into(), pt(t.insert.xy()));
                o.insert("text".into(), json!(t.value.chars().take(80).collect::<String>()));
                o.insert("height".into(), json!(round(t.height)));
            }
            EntityKind::MText(t) => {
                o.insert("at".into(), pt(t.insert.xy()));
                o.insert("text".into(), json!(t.contents.chars().take(80).collect::<String>()));
            }
            EntityKind::Insert(i) => {
                o.insert("block".into(), json!(i.block));
                o.insert("at".into(), pt(i.insert.xy()));
                o.insert("rotation".into(), json!(round(i.rotation)));
            }
            _ => {}
        }
    }
    v
}

fn space_entities<'a>(s: &'a Session, d: &'a Drawing) -> impl Iterator<Item = &'a std::sync::Arc<Entity>> + 'a {
    d.space(&s.space()).into_iter().flat_map(|store| store.iter()).take(MAX_SCAN)
}

fn str_list(p: &Value, key: &str) -> Vec<String> {
    p.get(key).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).take(100).map(str::to_string).collect()).unwrap_or_default()
}

/// Layer and type filters from the parameters (case-insensitive; empty = no filter).
struct Filter {
    layers: Vec<String>,
    types: Vec<String>,
}

impl Filter {
    fn from(p: &Value) -> Filter {
        Filter { layers: str_list(p, "layers"), types: str_list(p, "types") }
    }
    fn accepts(&self, e: &Entity) -> bool {
        (self.layers.is_empty() || self.layers.iter().any(|l| l.eq_ignore_ascii_case(&e.common.layer)))
            && (self.types.is_empty() || self.types.iter().any(|t| t.eq_ignore_ascii_case(e.kind.type_name())))
    }
}

fn num(v: &Value) -> Option<f64> {
    v.as_f64().filter(|x| x.is_finite() && x.abs() < 1e12)
}

fn parse_point(cmd: &str, p: &Value, key: &str) -> Result<Vec2> {
    match p.get(key).and_then(Value::as_array).map(Vec::as_slice) {
        Some([x, y]) => match (num(x), num(y)) {
            (Some(x), Some(y)) => Ok(Vec2::new(x, y)),
            _ => Err(bad(cmd, format!("`{key}` must be two finite numbers"))),
        },
        _ => Err(bad(cmd, format!("`{key}` must be [x, y]"))),
    }
}

fn parse_window(cmd: &str, p: &Value) -> Result<Bounds2> {
    match p.get("window").and_then(Value::as_array).map(Vec::as_slice) {
        Some([a, b, c, d]) => match (num(a), num(b), num(c), num(d)) {
            (Some(x0), Some(y0), Some(x1), Some(y1)) if x0 != x1 && y0 != y1 => {
                Ok(Bounds2::new(Vec2::new(x0.min(x1), y0.min(y1)), Vec2::new(x0.max(x1), y0.max(y1))))
            }
            _ => Err(bad(cmd, "`window` must be two different corners [x0, y0, x1, y1]")),
        },
        _ => Err(bad(cmd, "`window` is required: [x0, y0, x1, y1]")),
    }
}

fn limit(p: &Value, default: usize, max: usize) -> usize {
    p.get("limit").and_then(Value::as_u64).map_or(default, |n| (n as usize).clamp(1, max))
}

fn extents_json(b: &Bounds2) -> Value {
    if b.is_empty() { Value::Null } else { json!({"min": pt(b.min), "max": pt(b.max)}) }
}

/// The part of the drawing on screen now (the view and the window's aspect).
pub(crate) fn view_json(s: &Session) -> Value {
    let Ok(st) = s.state() else { return Value::Null };
    let v = st.view();
    let aspect = s.viewport_px.0 / s.viewport_px.1.max(1.0);
    let (h, w) = (v.height, v.height * aspect);
    json!({"center": pt(v.center), "width": round(w), "height": round(h), "min": pt(Vec2::new(v.center.x - w / 2.0, v.center.y - h / 2.0)), "max": pt(Vec2::new(v.center.x + w / 2.0, v.center.y + h / 2.0))})
}

fn run_overview(s: &mut Session, p: &Value) -> Result<Value> {
    let cols = p.get("cols").and_then(Value::as_u64).map_or(6, |n| (n as usize).clamp(1, MAX_CELLS));
    let rows = p.get("rows").and_then(Value::as_u64).map_or(4, |n| (n as usize).clamp(1, MAX_CELLS));
    let d = s.doc()?;
    let space = s.space();
    let ext = d.extents(&space);
    let space_name = match &space {
        aad_doc::Space::Model => "Model".to_string(),
        aad_doc::Space::Paper(n) => n.clone(),
    };
    if ext.is_empty() || ext.width() <= 0.0 && ext.height() <= 0.0 {
        return Ok(
            json!({"space": space_name, "extents": extents_json(&ext), "view": view_json(s), "cells": [], "note": "nothing to lay out: the space is empty"}),
        );
    }
    let (cw, ch) = ((ext.width() / cols as f64).max(1e-9), (ext.height() / rows as f64).max(1e-9));
    type Cell = (usize, std::collections::BTreeMap<&'static str, usize>, std::collections::BTreeMap<String, usize>);
    let mut grid: std::collections::BTreeMap<(usize, usize), Cell> = std::collections::BTreeMap::new();
    let mut scanned = 0usize;
    for e in space_entities(s, d) {
        scanned += 1;
        let b = aad_doc::entity_bounds(d, e, 0);
        if b.is_empty() {
            continue;
        }
        // The cell holding the object's centre: every object is counted once.
        let c = b.center();
        let col = (((c.x - ext.min.x) / cw).floor().max(0.0) as usize).min(cols - 1);
        let row = (((c.y - ext.min.y) / ch).floor().max(0.0) as usize).min(rows - 1);
        let cell = grid.entry((row, col)).or_default();
        cell.0 += 1;
        *cell.1.entry(e.kind.type_name()).or_default() += 1;
        *cell.2.entry(e.common.layer.clone()).or_default() += 1;
    }
    let cells: Vec<Value> = grid
        .iter()
        .map(|((row, col), (n, types, layers))| {
            let (x0, y0) = (ext.min.x + *col as f64 * cw, ext.min.y + *row as f64 * ch);
            let mut top: Vec<(&String, &usize)> = layers.iter().collect();
            top.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
            let top: std::collections::BTreeMap<&String, &usize> = top.into_iter().take(5).collect();
            json!({"col": col, "row": row, "min": pt(Vec2::new(x0, y0)), "max": pt(Vec2::new(x0 + cw, y0 + ch)), "count": n, "types": types, "layers": top})
        })
        .collect();
    Ok(json!({
        "space": space_name, "extents": extents_json(&ext), "view": view_json(s), "grid": [cols, rows], "cell": [round(cw), round(ch)],
        "scanned": scanned, "cells": cells,
        "note": "row 0 is the bottom (smallest Y), col 0 the left (smallest X); each object is counted in the cell holding its centre",
    }))
}

fn run_query(s: &mut Session, p: &Value) -> Result<Value> {
    let w = parse_window("spatial.query", p)?;
    let inside = p.get("mode").and_then(Value::as_str).is_some_and(|m| m.eq_ignore_ascii_case("inside"));
    let filter = Filter::from(p);
    let max = limit(p, 25, MAX_OBJECTS);
    let d = s.doc()?;
    let (mut matched, mut objects) = (0usize, Vec::new());
    for e in space_entities(s, d) {
        if !filter.accepts(e) {
            continue;
        }
        let b = aad_doc::entity_bounds(d, e, 0);
        if b.is_empty() {
            continue;
        }
        let hit = if inside { b.min.x >= w.min.x && b.max.x <= w.max.x && b.min.y >= w.min.y && b.max.y <= w.max.y } else { b.intersects(&w) };
        if hit {
            matched += 1;
            if objects.len() < max {
                objects.push(describe(d, e));
            }
        }
    }
    Ok(
        json!({"window": [round(w.min.x), round(w.min.y), round(w.max.x), round(w.max.y)], "mode": if inside { "inside" } else { "crossing" }, "matched": matched, "shown": objects.len(), "truncated": matched > objects.len(), "objects": objects}),
    )
}

fn run_nearest(s: &mut Session, p: &Value) -> Result<Value> {
    let at = parse_point("spatial.nearest", p, "point")?;
    let count = limit_named(p, "count", 5, 20);
    let filter = Filter::from(p);
    let d = s.doc()?;
    let mut found: Vec<(f64, Handle, Value)> = Vec::new();
    for e in space_entities(s, d) {
        if !filter.accepts(e) {
            continue;
        }
        let b = aad_doc::entity_bounds(d, e, 0);
        if b.is_empty() {
            continue;
        }
        let dx = (b.min.x - at.x).max(0.0).max(at.x - b.max.x);
        let dy = (b.min.y - at.y).max(0.0).max(at.y - b.max.y);
        let dist = (dx * dx + dy * dy).sqrt();
        // Keep only the best `count` so far (a bounded list, whatever the drawing size).
        if found.len() < count || found.last().is_some_and(|l| dist < l.0) {
            let mut v = describe(d, e);
            if let Some(o) = v.as_object_mut() {
                o.insert("distance".into(), json!(round(dist)));
            }
            found.push((dist, e.handle, v));
            found.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.0.cmp(&b.1.0)));
            found.truncate(count);
        }
    }
    Ok(json!({"point": pt(at), "objects": found.into_iter().map(|f| f.2).collect::<Vec<_>>()}))
}

fn limit_named(p: &Value, key: &str, default: usize, max: usize) -> usize {
    p.get(key).and_then(Value::as_u64).map_or(default, |n| (n as usize).clamp(1, max))
}

fn run_describe(s: &mut Session, p: &Value) -> Result<Value> {
    let list = p.get("handles").and_then(Value::as_array).ok_or_else(|| bad("spatial.describe", "`handles` must be a list of handles"))?;
    if list.len() > 20 {
        return Err(bad("spatial.describe", "at most 20 handles"));
    }
    let d = s.doc()?;
    let (mut objects, mut missing) = (Vec::new(), Vec::new());
    for h in list {
        let text = h.as_str().unwrap_or("");
        match u64::from_str_radix(text.trim_start_matches("0x"), 16).ok().and_then(|n| d.entity(Handle(n))) {
            Some(e) => objects.push(describe(d, e)),
            None => missing.push(text.to_string()),
        }
    }
    Ok(json!({"objects": objects, "missing": missing}))
}
