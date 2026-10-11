//! Quantity take-off from the geometry on the `MEASURE-*` layers (the kit's TOSTART, MLIN, MBRK,
//! MBML, MCOL, MREC, MPAINT, MCEIL, MFLOOR, MAREA, MSLAB, MCLEAR and MEXPORT).
//!
//! Every quantity is computed here from the drawing geometry and units, never by the caller:
//! lengths of lines and polyline segments, areas of closed outlines, openings deducted where a
//! line on `MEASURE-DEDUCT` lies on a wall (or sits inside a block that carries such a line).
//! Lengths in metric drawings round to 1 cm; in imperial drawings to 1/8 inch. Each command labels
//! the elements on `MEASURE-LABELS`, draws a summary table on `MEASURE-TABLE`, and saves the result
//! in the drawing as `takeoff:<name>` so it can be exported later.

use cadcraft_doc::{Drawing, EntityKind, HAlign, Handle, Space, VAlign};
use cadcraft_geom::{Bounds2, Polyline, Segment, Vec2, point_in_polygon};
use serde_json::{Value, json};

use super::draw::make_text;
use super::hcw::{add_on, ensure_layer};
use super::helpers::line;
use super::*;
use crate::annot::{self, Unit};
use crate::{Result, Session};

pub const L_LINEAR: &str = "MEASURE-LINEAR";
pub const L_FULL: &str = "MEASURE-FULLBRICK";
pub const L_HALF: &str = "MEASURE-HALFBRICK";
pub const L_BEAM: &str = "MEASURE-BEAM";
pub const L_LINTEL: &str = "MEASURE-LINTEL";
pub const L_COLUMN: &str = "MEASURE-COLUMN";
pub const L_COUNT: &str = "MEASURE-COUNT";
pub const L_DEDUCT: &str = "MEASURE-DEDUCT";
pub const L_CEILING: &str = "MEASURE-CEILING";
pub const L_FLOOR: &str = "MEASURE-FLOOR";
pub const L_AREA: &str = "MEASURE-AREA";
pub const L_SLAB: &str = "MEASURE-SLAB";
pub const L_SLAB_DEDUCT: &str = "MEASURE-SLAB-DEDUCT";
pub const L_LABELS: &str = "MEASURE-LABELS";
pub const L_TABLE: &str = "MEASURE-TABLE";

/// Layer, ACI colour, description.
const LAYERS: &[(&str, i16, &str)] = &[
    (L_LINEAR, 5, "Take-off: linear measurement"),
    (L_FULL, 1, "Take-off: full brick walls"),
    (L_HALF, 30, "Take-off: half brick walls"),
    (L_BEAM, 3, "Take-off: concrete beams"),
    (L_LINTEL, 4, "Take-off: concrete lintels"),
    (L_COLUMN, 6, "Take-off: concrete columns"),
    (L_COUNT, 2, "Take-off: rectangles to count"),
    (L_DEDUCT, 40, "Take-off: openings to deduct"),
    (L_CEILING, 142, "Take-off: ceiling outlines"),
    (L_FLOOR, 94, "Take-off: floor outlines"),
    (L_AREA, 150, "Take-off: areas"),
    (L_SLAB, 52, "Take-off: slab outlines"),
    (L_SLAB_DEDUCT, 41, "Take-off: slab openings"),
    (L_LABELS, 7, "Take-off labels"),
    (L_TABLE, 7, "Take-off tables"),
];

pub fn specs() -> Vec<CommandSpec> {
    let p = "{units?: mm|cm|m|in|ft (default the drawing's), at?: [x,y] (table corner), table?: bool (default true), label?: bool (default true), tolerance? (drawing units)}";
    vec![
        CommandSpec::new("takeoff.start", "Start Take-off", run_start)
            .menu(&["Architecture", "Take-off", "Start"])
            .alias(&["tostart", "msetup"])
            .params("{} → {created} (creates the MEASURE-* layers)"),
        CommandSpec::new("takeoff.linear", "Linear", run_linear).menu(&["Architecture", "Take-off", "Linear"]).alias(&["mlin"]).params(p),
        CommandSpec::new("takeoff.brick", "Brickwork", run_brick).menu(&["Architecture", "Take-off", "Brickwork"]).alias(&["mbrk"]).params(p),
        CommandSpec::new("takeoff.beams", "Beams and Lintels", run_beams).menu(&["Architecture", "Take-off", "Beams"]).alias(&["mbml"]).params(p),
        CommandSpec::new("takeoff.columns", "Columns", run_columns).menu(&["Architecture", "Take-off", "Columns"]).alias(&["mcol"]).params(p),
        CommandSpec::new("takeoff.rects", "Rectangles", run_rects).menu(&["Architecture", "Take-off", "Rectangles"]).alias(&["mrec"]).params(p),
        CommandSpec::new("takeoff.paint", "Wall Paint", run_paint)
            .menu(&["Architecture", "Take-off", "Wall Paint"])
            .alias(&["mpaint"])
            .params("{floor?: floor name (default the first), units?, at?, table?, label?, tolerance?}"),
        CommandSpec::new("takeoff.ceiling", "Ceiling", run_ceiling).menu(&["Architecture", "Take-off", "Ceiling"]).alias(&["mceil"]).params(p),
        CommandSpec::new("takeoff.floor", "Floor", run_floor).menu(&["Architecture", "Take-off", "Floor"]).alias(&["mfloor"]).params(p),
        CommandSpec::new("takeoff.area", "Area", run_area).menu(&["Architecture", "Take-off", "Area"]).alias(&["marea", "mare"]).params(p),
        CommandSpec::new("takeoff.slab", "Slab", run_slab).menu(&["Architecture", "Take-off", "Slab"]).alias(&["mslab", "mslb"]).params(p),
        CommandSpec::new("takeoff.clear", "Clear Take-off Labels", run_clear)
            .menu(&["Architecture", "Take-off", "Clear Labels"])
            .alias(&["mclear"])
            .params("{} → {erased} (erases the objects on MEASURE-LABELS and MEASURE-TABLE only)"),
        CommandSpec::new("takeoff.list", "List Take-offs", run_list).params("{name?} → {takeoffs: [name] | the saved result}").noundo(),
        CommandSpec::new("takeoff.export", "Export Take-off", run_export)
            .menu(&["Architecture", "Take-off", "Export CSV"])
            .alias(&["mexport"])
            .params("{path? (file, or a folder), name? (one take-off; default every saved take-off, one CSV each)} → {files}")
            .noundo(),
    ]
}

// ---------------------------------------------------------------------------------------------
// Context, units and rounding

pub(crate) struct Ctx {
    unit: Unit,
    /// Label text height in drawing units (about 125 mm of real size).
    th: f64,
    /// Matching distance in drawing units (about 10 mm of real size).
    tol: f64,
}

impl Ctx {
    fn metric(&self) -> bool {
        !matches!(self.unit, Unit::In | Unit::Ft | Unit::FtIn)
    }
    fn len_unit(&self) -> &'static str {
        if self.metric() { "m" } else { "ft" }
    }
    fn area_unit(&self) -> &'static str {
        if self.metric() { "m2" } else { "sq ft" }
    }
    /// A length in drawing units as metres (1 cm steps) or feet (1/8 inch steps).
    fn len(&self, du: f64) -> f64 {
        let m = du * self.unit.metres();
        if self.metric() {
            (m * 100.0).round() / 100.0
        } else {
            let inches = m / 0.0254;
            (inches * 8.0).round() / 8.0 / 12.0
        }
    }
    /// An area in drawing units squared as square metres or square feet (0.01 steps).
    fn area(&self, du2: f64) -> f64 {
        let m2 = du2 * self.unit.metres() * self.unit.metres();
        let v = if self.metric() { m2 } else { m2 / (0.3048 * 0.3048) };
        (v * 100.0).round() / 100.0
    }
}

fn ctx(s: &Session, p: &Value, cmd: &str) -> Result<Ctx> {
    let d = s.doc()?;
    let unit = match str_param(p, "units") {
        Some(u) => Unit::parse(u).ok_or_else(|| bad(cmd, "`units` must be mm, cm, m, in or ft"))?,
        None => annot::drawing_unit(d).ok_or_else(|| bad(cmd, "the drawing's units are not set: set them with `drawing.units` or pass `units`"))?,
    };
    let per_mm = 0.001 / unit.metres();
    let tol = f64_or(p, "tolerance", 10.0 * per_mm);
    if !(tol.is_finite() && tol >= 0.0) {
        return Err(bad(cmd, "`tolerance` must be zero or positive"));
    }
    Ok(Ctx { unit, th: 125.0 * per_mm, tol })
}

// ---------------------------------------------------------------------------------------------
// Elements

fn on_layer(e: &cadcraft_doc::Entity, layer: &str) -> bool {
    e.common.layer.eq_ignore_ascii_case(layer)
}

#[derive(Clone)]
struct Wall {
    layer: String,
    seg: Segment,
    handle: Handle,
}

pub(crate) fn segments_of(kind: &EntityKind) -> Vec<Segment> {
    match kind {
        EntityKind::Line(l) => vec![Segment::Line(cadcraft_geom::Line::new(l.a.xy(), l.b.xy()))],
        EntityKind::Arc(a) => {
            let arc = cadcraft_geom::Arc { center: a.center.xy(), radius: a.radius, start: a.start, end: a.end };
            vec![Segment::Arc { arc, ccw: true }]
        }
        EntityKind::LwPolyline(p) => Polyline { vertices: p.vertices.clone(), closed: p.closed }.segments(),
        _ => Vec::new(),
    }
}

fn walls(d: &Drawing, space: &Space, layers: &[&str]) -> Vec<Wall> {
    let mut out = Vec::new();
    if let Some(store) = d.space(space) {
        for e in store.iter().filter(|e| d.is_visible(e)) {
            if let Some(layer) = layers.iter().find(|l| on_layer(e, l)) {
                for seg in segments_of(&e.kind) {
                    if seg.len() > 1e-12 {
                        out.push(Wall { layer: (*layer).to_string(), seg, handle: e.handle });
                    }
                }
            }
        }
    }
    // Left to right, then bottom to top.
    out.sort_by(|a, b| {
        let (ma, mb) = (a.seg.mid(), b.seg.mid());
        ma.x.total_cmp(&mb.x).then(ma.y.total_cmp(&mb.y)).then(a.handle.0.cmp(&b.handle.0))
    });
    out
}

struct Shape {
    handle: Handle,
    pts: Vec<Vec2>,
    area: f64,
    perimeter: f64,
    centroid: Vec2,
}

fn shape_of(kind: &EntityKind) -> Option<Vec<Vec2>> {
    match kind {
        EntityKind::LwPolyline(p) if p.closed && p.vertices.len() >= 3 => {
            Some(Polyline { vertices: p.vertices.clone(), closed: true }.tessellate(1e-3))
        }
        EntityKind::Circle(c) => {
            let n = 96;
            Some(
                (0..n)
                    .map(|i| {
                        c.center.xy()
                            + Vec2::new(
                                c.radius * (std::f64::consts::TAU * f64::from(i) / f64::from(n)).cos(),
                                c.radius * (std::f64::consts::TAU * f64::from(i) / f64::from(n)).sin(),
                            )
                    })
                    .collect(),
            )
        }
        _ => None,
    }
}

fn polygon_area_centroid(pts: &[Vec2]) -> (f64, Vec2) {
    let (mut a, mut cx, mut cy) = (0.0, 0.0, 0.0);
    let o = pts.first().copied().unwrap_or(Vec2::ZERO);
    for w in pts.windows(2).chain(std::iter::once(&[pts.last().copied().unwrap_or(o), o][..])) {
        let (p, q) = (w[0] - o, w[1] - o);
        let cr = p.x * q.y - q.x * p.y;
        a += cr;
        cx += (p.x + q.x) * cr;
        cy += (p.y + q.y) * cr;
    }
    if a.abs() < 1e-18 {
        return (0.0, o);
    }
    (a.abs() / 2.0, Vec2::new(o.x + cx / (3.0 * a), o.y + cy / (3.0 * a)))
}

fn shapes(d: &Drawing, space: &Space, layer: &str) -> Vec<Shape> {
    let mut out = Vec::new();
    if let Some(store) = d.space(space) {
        for e in store.iter().filter(|e| d.is_visible(e) && on_layer(e, layer)) {
            let Some(pts) = shape_of(&e.kind) else { continue };
            let (area, centroid) = polygon_area_centroid(&pts);
            let perimeter: f64 = pts.windows(2).map(|w| w[0].dist(w[1])).sum::<f64>() + pts.first().zip(pts.last()).map_or(0.0, |(a, b)| a.dist(*b));
            out.push(Shape { handle: e.handle, pts, area, perimeter, centroid });
        }
    }
    out.sort_by(|a, b| a.centroid.x.total_cmp(&b.centroid.x).then(a.centroid.y.total_cmp(&b.centroid.y)).then(a.handle.0.cmp(&b.handle.0)));
    out
}

/// Wall marks in sorted order: a prefix per layer and a running two-digit number (`FB01`, `HB01`).
fn wall_names(ws: &[Wall], sets: &[(&str, &str)]) -> Vec<String> {
    let mut counters: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    ws.iter()
        .map(|w| {
            let prefix = sets.iter().find(|(l, _)| *l == w.layer).map_or("W", |(_, pfx)| *pfx);
            let n = counters.entry(prefix).or_insert(0);
            *n += 1;
            format!("{prefix}{:02}", *n)
        })
        .collect()
}

struct Deduction {
    a: Vec2,
    b: Vec2,
    from_block: bool,
}

impl Deduction {
    fn len(&self) -> f64 {
        self.a.dist(self.b)
    }
    fn mid(&self) -> Vec2 {
        (self.a + self.b) * 0.5
    }
}

/// Lines on the deduction layer, including the ones inside block references (transformed to
/// drawing coordinates).
fn deductions(d: &Drawing, space: &Space) -> Vec<Deduction> {
    let mut out = Vec::new();
    let Some(store) = d.space(space) else { return out };
    for e in store.iter().filter(|e| d.is_visible(e)) {
        match &e.kind {
            EntityKind::Line(l) if on_layer(e, L_DEDUCT) => out.push(Deduction { a: l.a.xy(), b: l.b.xy(), from_block: false }),
            EntityKind::LwPolyline(p) if on_layer(e, L_DEDUCT) => {
                for seg in (Polyline { vertices: p.vertices.clone(), closed: p.closed }).segments() {
                    out.push(Deduction { a: seg.start(), b: seg.end(), from_block: false });
                }
            }
            EntityKind::Insert(i) => {
                let Some(b) = d.block(&i.block) else { continue };
                let (sin, cos) = i.rotation.sin_cos();
                let tf = |p: cadcraft_geom::Vec3| {
                    let (x, y) = ((p.x - b.base.x) * i.scale.x, (p.y - b.base.y) * i.scale.y);
                    Vec2::new(i.insert.x + x * cos - y * sin, i.insert.y + x * sin + y * cos)
                };
                for be in b.entities.iter() {
                    if let EntityKind::Line(l) = &be.kind
                        && on_layer(be, L_DEDUCT)
                    {
                        out.push(Deduction { a: tf(l.a), b: tf(l.b), from_block: true });
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// A deduction matched to a wall: its position along the wall orders the openings.
struct Matched {
    wall: usize,
    len: f64,
    t: f64,
}

/// Match each deduction to the nearest wall within `tol`. A standalone deduction that touches no
/// wall is returned in the second list; a block deduction that touches none belongs to another wall
/// and is ignored.
fn match_deductions(walls: &[Wall], deds: &[Deduction], tol: f64) -> (Vec<Matched>, Vec<usize>) {
    let mut matched = Vec::new();
    let mut orphans = Vec::new();
    for (di, dd) in deds.iter().enumerate() {
        let m = dd.mid();
        let best = walls
            .iter()
            .enumerate()
            .map(|(i, w)| (i, w.seg.closest(m).dist(m)))
            .filter(|(_, dist)| *dist <= tol)
            .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
        match best {
            Some((wi, _)) => {
                let w = &walls[wi];
                let (s0, s1) = (w.seg.start(), w.seg.end());
                let dir = s1 - s0;
                let t = if dir.len() > 1e-12 { (m - s0).dot(dir) / dir.len() } else { 0.0 };
                matched.push(Matched { wall: wi, len: dd.len(), t });
            }
            None if !dd.from_block => orphans.push(di),
            None => {}
        }
    }
    (matched, orphans)
}

// ---------------------------------------------------------------------------------------------
// Output: labels, tables, saved results

fn group_letter(i: usize) -> String {
    let mut n = i;
    let mut s = String::new();
    loop {
        s.insert(0, (b'A' + (n % 26) as u8) as char);
        if n < 26 {
            break;
        }
        n = n / 26 - 1;
    }
    s
}

fn label(s: &mut Session, c: &Ctx, at: Vec2, text: &str) -> Result<Handle> {
    ensure_layer(s.doc_mut()?, L_LABELS, 7, "Continuous", 18, "Take-off labels");
    let kind = make_text(s, at, text, c.th, 0.0, (HAlign::Center, VAlign::Middle));
    add_on(s, L_LABELS, kind)
}

/// Draw a table (title, header, rows) with its top-left corner at `at`, on `MEASURE-TABLE`.
fn draw_table(s: &mut Session, c: &Ctx, at: Vec2, title: &str, header: &[String], rows: &[Vec<String>]) -> Result<()> {
    ensure_layer(s.doc_mut()?, L_TABLE, 7, "Continuous", 18, "Take-off tables");
    let th = c.th;
    let ncols = header.len();
    let mut widths: Vec<f64> = header.iter().map(|h| h.chars().count() as f64).collect();
    for r in rows {
        for (i, cell) in r.iter().enumerate().take(ncols) {
            if let Some(w) = widths.get_mut(i) {
                *w = w.max(cell.chars().count() as f64);
            }
        }
    }
    let widths: Vec<f64> = widths.iter().map(|w| w * th * 0.75 + th * 1.2).collect();
    let total: f64 = widths.iter().sum();
    let rh = th * 1.8;
    let mut y = at.y;
    // Title row, then the header and body rows.
    let kind = make_text(s, Vec2::new(at.x + th * 0.4, y - rh * 0.5), title, th * 1.2, 0.0, (HAlign::Left, VAlign::Middle));
    add_on(s, L_TABLE, kind)?;
    y -= rh;
    for (ri, row) in std::iter::once(header).chain(rows.iter().map(Vec::as_slice)).enumerate() {
        let mut x = at.x;
        for (ci, w) in widths.iter().enumerate() {
            let text = row.get(ci).map_or("", String::as_str);
            let kind =
                make_text(s, Vec2::new(x + th * 0.4, y - rh * 0.5), text, if ri == 0 { th } else { th * 0.9 }, 0.0, (HAlign::Left, VAlign::Middle));
            add_on(s, L_TABLE, kind)?;
            x += w;
        }
        add_on(s, L_TABLE, line(Vec2::new(at.x, y - rh), Vec2::new(at.x + total, y - rh)))?;
        y -= rh;
    }
    let (top, bottom) = (at.y - rh, y);
    add_on(s, L_TABLE, line(Vec2::new(at.x, at.y), Vec2::new(at.x + total, at.y)))?;
    let mut x = at.x;
    for w in std::iter::once(&0.0).chain(widths.iter()) {
        x += w;
        add_on(s, L_TABLE, line(Vec2::new(x, top), Vec2::new(x, bottom)))?;
    }
    add_on(s, L_TABLE, line(Vec2::new(at.x, at.y), Vec2::new(at.x, bottom)))?;
    Ok(())
}

/// Where the next table goes: `at`, else below the drawing's extents.
fn table_origin(s: &Session, c: &Ctx, p: &Value) -> Vec2 {
    if let Some(at) = point_param(p, "at") {
        return at;
    }
    let ext = s.doc().map(|d| d.extents(&Space::Model)).unwrap_or(Bounds2::EMPTY);
    if ext.is_empty() { Vec2::ZERO } else { Vec2::new(ext.min.x, ext.min.y - 4.0 * c.th) }
}

struct Report {
    name: &'static str,
    title: String,
    header: Vec<String>,
    rows: Vec<Vec<String>>,
    /// Per-element details for the caller.
    items: Vec<Value>,
    totals: Value,
    warnings: Vec<String>,
}

fn finish(s: &mut Session, c: &Ctx, p: &Value, r: Report) -> Result<Value> {
    if bool_or(p, "table", true) && !r.rows.is_empty() {
        let at = table_origin(s, c, p);
        draw_table(s, c, at, &r.title, &r.header, &r.rows)?;
    }
    let result = json!({
        "name": r.name, "title": r.title, "lengthUnit": c.len_unit(), "areaUnit": c.area_unit(),
        "columns": r.header, "rows": r.rows, "items": r.items, "totals": r.totals, "warnings": r.warnings,
    });
    project::set(s.doc_mut()?, &format!("takeoff:{}", r.name), &result);
    Ok(result)
}

fn run_start(s: &mut Session, _p: &Value) -> Result<Value> {
    let d = s.doc_mut()?;
    let mut created = 0;
    for (name, aci, desc) in LAYERS {
        if ensure_layer(d, name, *aci, "Continuous", 18, desc) {
            created += 1;
        }
    }
    Ok(json!({ "created": created, "layers": LAYERS.iter().map(|l| l.0).collect::<Vec<_>>() }))
}

// ---------------------------------------------------------------------------------------------
// Wall measurements

fn measure_walls(s: &mut Session, p: &Value, cmd: &str, name: &'static str, title: &str, sets: &[(&str, &str)]) -> Result<Value> {
    let c = ctx(s, p, cmd)?;
    let space = s.space();
    let layers: Vec<&str> = sets.iter().map(|(l, _)| *l).collect();
    let (ws, ded, tol) = {
        let d = s.doc()?;
        (walls(d, &space, &layers), deductions(d, &space), c.tol)
    };
    let (matched, orphans) = match_deductions(&ws, &ded, tol);
    // Numbering per prefix in sorted order; openings numbered along each wall.
    let names = wall_names(&ws, sets);
    let mut groups: Vec<(String, f64)> = Vec::new();
    struct Row {
        group: String,
        len: f64,
        ded: f64,
    }
    let mut rowsv = Vec::new();
    let mut items = Vec::new();
    for (i, w) in ws.iter().enumerate() {
        let len = c.len(w.seg.len());
        let prefix = sets.iter().find(|(l, _)| *l == w.layer).map_or("W", |(_, pfx)| *pfx);
        let key = format!("{prefix}-{len}");
        let gi = match groups.iter().position(|(k, _)| *k == key) {
            Some(g) => g,
            None => {
                groups.push((key, len));
                groups.len() - 1
            }
        };
        let letter =
            group_letter(groups.iter().filter(|(k, _)| k.starts_with(&format!("{prefix}-"))).position(|(k, _)| *k == groups[gi].0).unwrap_or(0));
        let mut mine: Vec<&Matched> = matched.iter().filter(|m| m.wall == i).collect();
        mine.sort_by(|a, b| a.t.total_cmp(&b.t));
        let ded_names: Vec<(String, f64)> = mine.iter().enumerate().map(|(k, m)| (format!("{}-D{}", names[i], k + 1), c.len(m.len))).collect();
        let ded_total: f64 = ded_names.iter().fold(0.0, |a, (_, l)| a + l);
        let mid = w.seg.mid();
        label(s, &c, Vec2::new(mid.x, mid.y + c.th * 1.2), &names[i])?;
        for (k, m) in mine.iter().enumerate() {
            let wm = w.seg.closest(w.seg.mid());
            let _ = wm;
            let at = w.seg.start() + (w.seg.end() - w.seg.start()).normalized() * m.t;
            label(s, &c, Vec2::new(at.x, at.y - c.th * 1.2), &ded_names[k].0)?;
        }
        items.push(json!({ "mark": names[i], "group": format!("{prefix}-{letter}"), "handle": w.handle.hex(), "length": len, "openings": ded_names.iter().map(|(n, l)| json!({"name": n, "length": l})).collect::<Vec<_>>(), "net": (len - ded_total).max(0.0) }));
        rowsv.push(Row { group: format!("{prefix}-{letter}"), len, ded: ded_total });
    }
    for di in &orphans {
        let m = ded[*di].mid();
        label(s, &c, Vec2::new(m.x, m.y - c.th * 1.2), "D? NO LINE")?;
    }
    // Group summary: same group, same length.
    let mut order: Vec<String> = Vec::new();
    for r in &rowsv {
        if !order.contains(&r.group) {
            order.push(r.group.clone());
        }
    }
    let mut rows = Vec::new();
    let (mut tl, mut td, mut tn) = (0.0, 0.0, 0.0);
    for g in &order {
        let members: Vec<&Row> = rowsv.iter().filter(|r| r.group == *g).collect();
        let each = members.first().map_or(0.0, |r| r.len);
        let total: f64 = members.iter().fold(0.0, |a, r| a + r.len);
        let ded: f64 = members.iter().fold(0.0, |a, r| a + r.ded);
        let net = (total - ded).max(0.0);
        tl += total;
        td += ded;
        tn += net;
        rows.push(vec![g.clone(), members.len().to_string(), format!("{each:.2}"), format!("{total:.2}"), format!("{ded:.2}"), format!("{net:.2}")]);
    }
    if !rows.is_empty() {
        rows.push(vec!["TOTAL".into(), rowsv.len().to_string(), String::new(), format!("{tl:.2}"), format!("{td:.2}"), format!("{tn:.2}")]);
    }
    let mut warnings = Vec::new();
    if ws.is_empty() {
        warnings.push(format!("nothing found on {}", layers.join(", ")));
    }
    if !orphans.is_empty() {
        warnings.push(format!("{} deduction line(s) lie on no wall (labelled D? NO LINE) and are not counted", orphans.len()));
    }
    let header =
        ["Name", "Count", &format!("Length ({})", c.len_unit()), "Total", "Openings", "Net total"].iter().map(|x| (*x).to_string()).collect();
    finish(
        s,
        &c,
        p,
        Report {
            name,
            title: title.into(),
            header,
            rows,
            items,
            totals: json!({ "count": rowsv.len(), "length": (tl * 100.0).round() / 100.0, "openings": (td * 100.0).round() / 100.0, "net": (tn * 100.0).round() / 100.0 }),
            warnings,
        },
    )
}

fn run_linear(s: &mut Session, p: &Value) -> Result<Value> {
    measure_walls(s, p, "takeoff.linear", "Linear", "LINEAR TAKE-OFF", &[(L_LINEAR, "L")])
}

fn run_brick(s: &mut Session, p: &Value) -> Result<Value> {
    measure_walls(s, p, "takeoff.brick", "Bricks", "BRICKWORK", &[(L_FULL, "FB"), (L_HALF, "HB")])
}

fn run_beams(s: &mut Session, p: &Value) -> Result<Value> {
    measure_walls(s, p, "takeoff.beams", "Beams", "CONCRETE BEAMS AND LINTELS", &[(L_BEAM, "BM"), (L_LINTEL, "LT")])
}

// ---------------------------------------------------------------------------------------------
// Closed outlines

/// Group shapes of equal rounded `key`; returns the table rows and per-item details.
fn area_groups(s: &mut Session, c: &Ctx, shapes: &[Shape], prefix: &str, with_perimeter: bool) -> Result<(Vec<Vec<String>>, Vec<Value>, f64)> {
    let mut keys: Vec<f64> = Vec::new();
    let mut items = Vec::new();
    let mut members: Vec<Vec<f64>> = Vec::new();
    for (i, sh) in shapes.iter().enumerate() {
        let a = c.area(sh.area);
        let g = match keys.iter().position(|k| (*k - a).abs() < 1e-9) {
            Some(g) => g,
            None => {
                keys.push(a);
                members.push(Vec::new());
                keys.len() - 1
            }
        };
        members[g].push(a);
        let name = format!("{prefix}-{}", group_letter(g));
        label(s, c, sh.centroid, &name)?;
        items.push(
            json!({ "mark": format!("{prefix}{:02}", i + 1), "group": name, "handle": sh.handle.hex(), "area": a, "perimeter": c.len(sh.perimeter) }),
        );
    }
    let mut rows = Vec::new();
    let mut grand = 0.0;
    for (g, k) in keys.iter().enumerate() {
        let n = members[g].len();
        let total = (k * n as f64 * 100.0).round() / 100.0;
        grand += total;
        let per: f64 = shapes.iter().filter(|sh| (c.area(sh.area) - k).abs() < 1e-9).map(|sh| c.len(sh.perimeter)).next().unwrap_or(0.0);
        let mut row = vec![format!("{prefix}-{}", group_letter(g)), n.to_string(), format!("{k:.2}"), format!("{total:.2}")];
        if with_perimeter {
            row.push(format!("{per:.2}"));
        }
        rows.push(row);
    }
    if !rows.is_empty() {
        let mut row = vec!["TOTAL".into(), shapes.len().to_string(), String::new(), format!("{grand:.2}")];
        if with_perimeter {
            row.push(String::new());
        }
        rows.push(row);
    }
    Ok((rows, items, (grand * 100.0).round() / 100.0))
}

fn area_command(s: &mut Session, p: &Value, cmd: &str, layer: &str, name: &'static str, title: &str, prefix: &str) -> Result<Value> {
    let c = ctx(s, p, cmd)?;
    let space = s.space();
    let sh = shapes(s.doc()?, &space, layer);
    let (rows, items, grand) = area_groups(s, &c, &sh, prefix, false)?;
    let warnings = if sh.is_empty() { vec![format!("no closed outlines on {layer}")] } else { Vec::new() };
    let header = ["Name", "Count", &format!("Area ({})", c.area_unit()), "Total"].iter().map(|x| (*x).to_string()).collect();
    finish(s, &c, p, Report { name, title: title.into(), header, rows, items, totals: json!({ "count": sh.len(), "area": grand }), warnings })
}

fn run_ceiling(s: &mut Session, p: &Value) -> Result<Value> {
    area_command(s, p, "takeoff.ceiling", L_CEILING, "Ceiling", "CEILING PAINT", "CP")
}

fn run_floor(s: &mut Session, p: &Value) -> Result<Value> {
    area_command(s, p, "takeoff.floor", L_FLOOR, "Floor", "FLOOR AREA", "FL")
}

fn run_area(s: &mut Session, p: &Value) -> Result<Value> {
    let c = ctx(s, p, "takeoff.area")?;
    let space = s.space();
    let sh = shapes(s.doc()?, &space, L_AREA);
    let mut rows = Vec::new();
    let mut items = Vec::new();
    let mut total = 0.0;
    for (i, x) in sh.iter().enumerate() {
        let (a, per) = (c.area(x.area), c.len(x.perimeter));
        total += a;
        label(s, &c, x.centroid, &format!("A{}", i + 1))?;
        rows.push(vec![format!("A{}", i + 1), format!("{a:.2}"), format!("{per:.2}")]);
        items.push(json!({ "mark": format!("A{}", i + 1), "handle": x.handle.hex(), "area": a, "perimeter": per }));
    }
    if !rows.is_empty() {
        rows.push(vec!["TOTAL".into(), format!("{:.2}", (total * 100.0).round() / 100.0), String::new()]);
    }
    let header = ["Name", &format!("Area ({})", c.area_unit()), &format!("Perimeter ({})", c.len_unit())].iter().map(|x| (*x).to_string()).collect();
    let warnings = if sh.is_empty() { vec![format!("no closed outlines on {L_AREA}")] } else { Vec::new() };
    finish(
        s,
        &c,
        p,
        Report {
            name: "Areas",
            title: "AREAS".into(),
            header,
            rows,
            items,
            totals: json!({ "count": sh.len(), "area": (total * 100.0).round() / 100.0 }),
            warnings,
        },
    )
}

fn run_slab(s: &mut Session, p: &Value) -> Result<Value> {
    let c = ctx(s, p, "takeoff.slab")?;
    let space = s.space();
    let (slabs, holes) = {
        let d = s.doc()?;
        (shapes(d, &space, L_SLAB), shapes(d, &space, L_SLAB_DEDUCT))
    };
    let mut rows = Vec::new();
    let mut items = Vec::new();
    let mut grand = 0.0;
    let mut warnings = Vec::new();
    let mut used = vec![false; holes.len()];
    for (i, sl) in slabs.iter().enumerate() {
        let mut ded = 0.0;
        for (hi, h) in holes.iter().enumerate() {
            if point_in_polygon(&sl.pts, h.centroid) {
                ded += h.area;
                used[hi] = true;
            }
        }
        let (gross, d2, net) = (c.area(sl.area), c.area(ded), c.area((sl.area - ded).max(0.0)));
        grand += net;
        label(s, &c, sl.centroid, &format!("S{}", i + 1))?;
        rows.push(vec![format!("S{}", i + 1), format!("{gross:.2}"), format!("{d2:.2}"), format!("{net:.2}")]);
        items.push(json!({ "mark": format!("S{}", i + 1), "handle": sl.handle.hex(), "gross": gross, "openings": d2, "net": net }));
    }
    for (hi, u) in used.iter().enumerate() {
        if !u {
            warnings.push(format!("opening {} lies in no slab outline and is not counted", holes[hi].handle.hex()));
        }
    }
    if slabs.is_empty() {
        warnings.push(format!("no closed outlines on {L_SLAB}"));
    } else {
        rows.push(vec!["TOTAL".into(), String::new(), String::new(), format!("{:.2}", (grand * 100.0).round() / 100.0)]);
    }
    let header = ["Name", &format!("Gross ({})", c.area_unit()), "Openings", "Net"].iter().map(|x| (*x).to_string()).collect();
    finish(
        s,
        &c,
        p,
        Report {
            name: "Slab",
            title: "SLABS".into(),
            header,
            rows,
            items,
            totals: json!({ "count": slabs.len(), "net": (grand * 100.0).round() / 100.0 }),
            warnings,
        },
    )
}

/// Width and breadth of a four-sided outline (longer side first), or `None`.
fn rect_size(sh: &Shape) -> Option<(f64, f64)> {
    let p = &sh.pts;
    let n = if p.len() > 1 && p.first().zip(p.last()).is_some_and(|(a, b)| a.near(*b, 1e-9)) { p.len() - 1 } else { p.len() };
    if n != 4 {
        return None;
    }
    let (a, b) = (p[0].dist(p[1]), p[1].dist(p[2]));
    let ok = (p[0].dist(p[2]) - p[1].dist(p[3])).abs() <= 1e-6 * (a + b);
    ok.then_some(if a >= b { (a, b) } else { (b, a) })
}

fn run_columns(s: &mut Session, p: &Value) -> Result<Value> {
    let c = ctx(s, p, "takeoff.columns")?;
    let space = s.space();
    let all = shapes(s.doc()?, &space, L_COLUMN);
    let mut sizes: Vec<(f64, f64)> = Vec::new();
    let mut counts: Vec<usize> = Vec::new();
    let mut items = Vec::new();
    let mut warnings = Vec::new();
    for (i, sh) in all.iter().enumerate() {
        let Some((l, b)) = rect_size(sh) else {
            warnings.push(format!("{} is not a four-sided outline and is not counted", sh.handle.hex()));
            continue;
        };
        let key = (c.len(l), c.len(b));
        let g = match sizes.iter().position(|k| *k == key) {
            Some(g) => g,
            None => {
                sizes.push(key);
                counts.push(0);
                sizes.len() - 1
            }
        };
        counts[g] += 1;
        label(s, &c, sh.centroid, &format!("C{}", g + 1))?;
        items.push(json!({ "mark": format!("C{}", g + 1), "handle": sh.handle.hex(), "length": key.0, "breadth": key.1, "index": i }));
    }
    let mut rows: Vec<Vec<String>> =
        sizes.iter().enumerate().map(|(g, (l, b))| vec![format!("C{}", g + 1), format!("{l:.2} x {b:.2}"), counts[g].to_string()]).collect();
    if !rows.is_empty() {
        rows.push(vec!["TOTAL".into(), String::new(), counts.iter().sum::<usize>().to_string()]);
    }
    let header = ["Mark", &format!("Size ({})", c.len_unit()), "Count"].iter().map(|x| (*x).to_string()).collect();
    finish(
        s,
        &c,
        p,
        Report {
            name: "Columns",
            title: "COLUMN SCHEDULE".into(),
            header,
            rows,
            items,
            totals: json!({ "count": counts.iter().sum::<usize>(), "marks": sizes.len() }),
            warnings,
        },
    )
}

fn run_rects(s: &mut Session, p: &Value) -> Result<Value> {
    let c = ctx(s, p, "takeoff.rects")?;
    let space = s.space();
    let all = shapes(s.doc()?, &space, L_COUNT);
    let mut keys: Vec<(f64, f64)> = Vec::new();
    let mut counts: Vec<usize> = Vec::new();
    let mut items = Vec::new();
    let mut warnings = Vec::new();
    for sh in &all {
        let Some((l, b)) = rect_size(sh) else {
            warnings.push(format!("{} is not a four-sided outline and is not counted", sh.handle.hex()));
            continue;
        };
        let key = (c.len(l), c.len(b));
        let g = match keys.iter().position(|k| *k == key) {
            Some(g) => g,
            None => {
                keys.push(key);
                counts.push(0);
                keys.len() - 1
            }
        };
        counts[g] += 1;
        label(s, &c, sh.centroid, &format!("R-{}", group_letter(g)))?;
        items.push(json!({ "mark": format!("R-{}", group_letter(g)), "handle": sh.handle.hex() }));
    }
    let mut rows: Vec<Vec<String>> = keys
        .iter()
        .enumerate()
        .map(|(g, (l, b))| {
            let area = ((l * b) * 100.0).round() / 100.0;
            vec![
                format!("R-{}", group_letter(g)),
                format!("{l:.2}"),
                format!("{b:.2}"),
                counts[g].to_string(),
                format!("{area:.2}"),
                format!("{:.2}", 2.0 * (l + b)),
            ]
        })
        .collect();
    if !rows.is_empty() {
        rows.push(vec!["TOTAL".into(), String::new(), String::new(), counts.iter().sum::<usize>().to_string(), String::new(), String::new()]);
    }
    let header = ["Name", "Length", "Breadth", "Count", "Area", "Perimeter"].iter().map(|x| (*x).to_string()).collect();
    finish(
        s,
        &c,
        p,
        Report {
            name: "Rectangles",
            title: "RECTANGLES".into(),
            header,
            rows,
            items,
            totals: json!({ "count": counts.iter().sum::<usize>() }),
            warnings,
        },
    )
}

// ---------------------------------------------------------------------------------------------
// Wall paint

fn run_paint(s: &mut Session, p: &Value) -> Result<Value> {
    let c = ctx(s, p, "takeoff.paint")?;
    let space = s.space();
    let floors = project::floors(s.doc()?);
    let floor = match str_param(p, "floor") {
        Some(n) => floors
            .iter()
            .find(|f| f["name"].as_str().is_some_and(|x| x.eq_ignore_ascii_case(n)))
            .ok_or_else(|| bad("takeoff.paint", format!("no floor named `{n}`")))?,
        None => floors.first().ok_or_else(|| bad("takeoff.paint", "no floors"))?,
    };
    // Heights are metres in the project data; convert to drawing units for the length x height product.
    let height_m = floor["ceiling"].as_f64().unwrap_or(2.85);
    let sched = project::get(s.doc()?, "schedule");
    let (ws, ded) = {
        let d = s.doc()?;
        (walls(d, &space, &[L_FULL, L_HALF, L_LINEAR]), deductions(d, &space))
    };
    let (matched, orphans) = match_deductions(&ws, &ded, c.tol);
    let pnames = wall_names(&ws, &[(L_FULL, "FB"), (L_HALF, "HB"), (L_LINEAR, "L")]);
    let to_m = c.unit.metres();
    let mut total_wall = 0.0;
    let mut total_open = 0.0;
    let mut items = Vec::new();
    let mut rows = Vec::new();
    for (i, w) in ws.iter().enumerate() {
        let len_m = w.seg.len() * to_m;
        let wall_area = len_m * height_m;
        let mut open_area = 0.0;
        let mut mine: Vec<&Matched> = matched.iter().filter(|m| m.wall == i).collect();
        mine.sort_by(|a, b| a.t.total_cmp(&b.t));
        for (k, m) in mine.iter().enumerate() {
            let measured = m.len * to_m;
            let label = format!("{}-D{}", pnames[i], k + 1);
            let mapped = sched["map"][&label]
                .as_str()
                .and_then(|n| sched["openings"].as_array()?.iter().find(|o| o["name"].as_str() == Some(n)))
                .and_then(|o| Some(o["length"].as_f64()? * o["height"].as_f64()?));
            open_area += mapped.unwrap_or(measured * height_m);
        }
        total_wall += wall_area;
        total_open += open_area;
        items.push(json!({ "handle": w.handle.hex(), "length": len_m, "height": height_m, "wallArea": wall_area, "openings": open_area, "net": (wall_area - open_area).max(0.0) }));
        let mid = w.seg.mid();
        label(s, &c, Vec2::new(mid.x, mid.y + c.th * 1.2), &format!("{:.2}", ((wall_area - open_area).max(0.0) * 100.0).round() / 100.0))?;
    }
    let conv = |m2: f64| if c.metric() { m2 } else { m2 / (0.3048 * 0.3048) };
    let (tw, to, tn) = (conv(total_wall), conv(total_open), conv((total_wall - total_open).max(0.0)));
    if !ws.is_empty() {
        rows.push(vec![ws.len().to_string(), format!("{height_m:.2}"), format!("{tw:.2}"), format!("{to:.2}"), format!("{tn:.2}")]);
    }
    let header =
        ["Walls", "Height (m)", &format!("Wall area ({})", c.area_unit()), "Openings", "Net paint"].iter().map(|x| (*x).to_string()).collect();
    let mut warnings = Vec::new();
    if ws.is_empty() {
        warnings.push("no walls found on the brick or linear layers".to_string());
    }
    if !orphans.is_empty() {
        warnings.push(format!("{} deduction line(s) lie on no wall and are not counted", orphans.len()));
    }
    finish(
        s,
        &c,
        p,
        Report {
            name: "WallPaint",
            title: "WALL PAINT".into(),
            header,
            rows,
            items,
            totals: json!({ "wallArea": (tw * 100.0).round() / 100.0, "openings": (to * 100.0).round() / 100.0, "net": (tn * 100.0).round() / 100.0, "height": height_m }),
            warnings,
        },
    )
}

// ---------------------------------------------------------------------------------------------
// Clear, list, export

fn run_clear(s: &mut Session, _p: &Value) -> Result<Value> {
    let hs: Vec<Handle> = {
        let d = s.doc()?;
        d.model.iter().filter(|e| on_layer(e, L_LABELS) || on_layer(e, L_TABLE)).map(|e| e.handle).collect()
    };
    let d = s.doc_mut()?;
    for h in &hs {
        d.remove_entity(*h);
    }
    Ok(json!({ "erased": hs.len() }))
}

fn run_list(s: &mut Session, p: &Value) -> Result<Value> {
    let d = s.doc()?;
    match str_param(p, "name") {
        Some(n) => {
            let v = project::get(d, &format!("takeoff:{n}"));
            if v.is_null() { Err(bad("takeoff.list", format!("no saved take-off `{n}`"))) } else { Ok(v) }
        }
        None => Ok(json!({ "takeoffs": d.project.keys().filter_map(|k| k.strip_prefix("takeoff:")).collect::<Vec<_>>() })),
    }
}

fn csv_cell(s: &str) -> String {
    if s.contains([',', '"', '\n']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() }
}

/// A saved take-off as CSV text.
pub(crate) fn to_csv(v: &Value) -> String {
    let mut out = String::new();
    let row = |cells: Vec<String>| cells.iter().map(|c| csv_cell(c)).collect::<Vec<_>>().join(",") + "\n";
    out += &row(vec![v["title"].as_str().unwrap_or("").to_string()]);
    out += &row(v["columns"].as_array().map(|a| a.iter().map(|c| c.as_str().unwrap_or("").to_string()).collect()).unwrap_or_default());
    for r in v["rows"].as_array().into_iter().flatten() {
        out += &row(r.as_array().map(|a| a.iter().map(|c| c.as_str().unwrap_or("").to_string()).collect()).unwrap_or_default());
    }
    out
}

fn run_export(s: &mut Session, p: &Value) -> Result<Value> {
    let d = s.doc()?;
    let names: Vec<String> = match str_param(p, "name") {
        Some(n) => vec![n.to_string()],
        None => d.project.keys().filter_map(|k| k.strip_prefix("takeoff:")).map(str::to_string).collect(),
    };
    if names.is_empty() {
        return Err(bad("takeoff.export", "no saved take-off: run a take-off command first"));
    }
    let docs: Vec<(String, String)> = names
        .iter()
        .map(|n| {
            let v = project::get(d, &format!("takeoff:{n}"));
            if v.is_null() { Err(bad("takeoff.export", format!("no saved take-off `{n}`"))) } else { Ok((n.clone(), to_csv(&v))) }
        })
        .collect::<Result<_>>()?;
    #[cfg(target_arch = "wasm32")]
    {
        let _ = docs;
        Err(bad("takeoff.export", "files cannot be written on the web; read the result with `takeoff.list`"))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let base = s.state()?.path.clone();
        let target = match str_param(p, "path") {
            Some(t) => std::path::PathBuf::from(t),
            None => match &base {
                Some(b) => std::path::Path::new(b).with_extension(""),
                None => std::env::temp_dir().join("aad-takeoff"),
            },
        };
        let (dir, single) = if target.extension().is_some() && docs.len() == 1 {
            (target.parent().map(std::path::Path::to_path_buf).unwrap_or_default(), Some(target.clone()))
        } else {
            (target.clone(), None)
        };
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(&dir).map_err(|e| bad("takeoff.export", format!("{}: {e}", dir.display())))?;
        }
        let stem = base
            .as_deref()
            .and_then(|b| std::path::Path::new(b).file_stem().map(|x| x.to_string_lossy().to_string()))
            .unwrap_or_else(|| "drawing".into());
        let mut files = Vec::new();
        for (n, csv) in docs {
            let file = single.clone().unwrap_or_else(|| dir.join(format!("{stem}-{n}.csv")));
            std::fs::write(&file, csv).map_err(|e| bad("takeoff.export", format!("{}: {e}", file.display())))?;
            files.push(file.to_string_lossy().to_string());
        }
        Ok(json!({ "files": files }))
    }
}

/// A context for drawing tables only (text height `th`, no units).
pub(crate) fn table_ctx(th: f64) -> Ctx {
    Ctx { unit: Unit::M, th, tol: 0.0 }
}

/// Draw a table with its top-left corner at `at` on `MEASURE-TABLE`.
pub(crate) fn table(s: &mut Session, c: &Ctx, at: Vec2, title: &str, header: &[String], rows: &[Vec<String>]) -> Result<()> {
    draw_table(s, c, at, title, header, rows)
}
