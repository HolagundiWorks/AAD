//! Walls, doors and windows (the kit's HCWWALL, HCWDOOR, HCWWINDOW and HCWCLEAN).
//!
//! * `wall.draw` draws the two faces of a wall (and the end caps) as lines on `A-WALL` along a
//!   centre line given by points or by selected lines and polylines, with mitred corners.
//! * `wall.door` and `wall.window` cut an opening in the wall lines at a point and place a block
//!   (`DOOR-…`, `WIN-…`) that holds the frame, leaf and swing or glass lines and a deduction line on
//!   `MEASURE-DEDUCT`, so take-off and the opening schedule see it.
//! * `wall.clean` erases zero-length and duplicate lines and joins lines that overlap on one line.

use std::sync::Arc;

use aad_doc::{Block, Common, Drawing, Entity, EntityKind, Handle, Insert};
use aad_geom::{Line, Vec2, Vec3};
use serde_json::{Value, json};

use super::hcw::{add_on, ensure_layer};
use super::helpers::{line, v3};
use super::*;
use crate::annot::{self as ua};
use crate::{Result, Session};

pub(crate) const WALL: &str = "A-WALL";
const DOOR: &str = "A-DOOR";
const WINDOW: &str = "A-WIND";
const DEDUCT: &str = "MEASURE-DEDUCT";

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("wall.draw", "Draw Wall", run_wall)
            .menu(&["Architecture", "Walls and Openings", "Draw Wall"])
            .alias(&["hcwwall"])
            .params("{points?: [[x,y],...] | handles?: [lines, polylines], thickness? (drawing units, default 230 mm), closed?: bool, layer?: default A-WALL} → {lines}"),
        CommandSpec::new("wall.door", "Door", run_door)
            .menu(&["Architecture", "Walls and Openings", "Door"])
            .alias(&["hcwdoor"])
            .params("{at: [x,y], angle?: degrees (default: the nearest wall line), width? | name? (schedule entry), thickness?, swing?: left|right, side?: up|down, layer?, deduct?: bool} → {block, cut}"),
        CommandSpec::new("wall.window", "Window", run_window)
            .menu(&["Architecture", "Walls and Openings", "Window"])
            .alias(&["hcwwindow"])
            .params("{at: [x,y], angle?, width? | name?, thickness?, layer?, deduct?: bool} → {block, cut}"),
        CommandSpec::new("wall.clean", "Clean Lines", run_clean)
            .menu(&["Architecture", "Walls and Openings", "Clean Lines"])
            .alias(&["hcwclean"])
            .params("{handles?, layer?, tolerance?} → {erased, joined} (zero-length and duplicate lines go; lines that overlap on one line become one)"),
    ]
}

fn per_mm(s: &Session) -> f64 {
    s.doc().ok().and_then(ua::drawing_unit).map_or(1.0, |u| 0.001 / u.metres())
}

// ---------------------------------------------------------------------------------------------
// Wall faces

fn normal(a: Vec2, b: Vec2) -> Vec2 {
    let d = b - a;
    let l = d.len();
    if l < 1e-12 { Vec2::ZERO } else { Vec2::new(-d.y / l, d.x / l) }
}

/// The offset of a centre line by `h` to the left (negative: right), with mitred corners.
pub(crate) fn offset_path(pts: &[Vec2], closed: bool, h: f64) -> Vec<Vec2> {
    let n = pts.len();
    let mut out = Vec::new();
    for i in 0..n {
        let prev = if i > 0 {
            Some(pts[i - 1])
        } else if closed {
            Some(pts[n - 1])
        } else {
            None
        };
        let next = if i + 1 < n {
            Some(pts[i + 1])
        } else if closed {
            Some(pts[0])
        } else {
            None
        };
        let p = pts[i];
        let n1 = prev.map(|q| normal(q, p));
        let n2 = next.map(|q| normal(p, q));
        let off = match (n1, n2) {
            (Some(a), Some(b)) => {
                let dot = a.x * b.x + a.y * b.y;
                if 1.0 + dot < 0.1 { a * h } else { (a + b) * (h / (1.0 + dot)) }
            }
            (Some(a), None) => a * h,
            (None, Some(b)) => b * h,
            (None, None) => Vec2::ZERO,
        };
        out.push(p + off);
    }
    out
}

fn centre_paths(s: &Session, p: &Value) -> Result<Vec<(Vec<Vec2>, bool)>> {
    if let Some(pts) = points_param(p, "points") {
        if pts.len() < 2 {
            return Err(bad("wall.draw", "`points` needs two or more points"));
        }
        return Ok(vec![(pts, bool_or(p, "closed", false))]);
    }
    let hs = targets(s, p)?;
    if hs.is_empty() {
        return Err(bad("wall.draw", "give `points`, or select lines and polylines"));
    }
    let d = s.doc()?;
    let mut out = Vec::new();
    for h in hs {
        let Some(e) = d.entity(h) else { continue };
        match &e.kind {
            EntityKind::Line(l) => out.push((vec![l.a.xy(), l.b.xy()], false)),
            EntityKind::LwPolyline(pl) => {
                let mut v: Vec<Vec2> = pl.vertices.iter().map(|x| x.p).collect();
                v.dedup_by(|a, b| a.near(*b, 1e-9));
                if v.len() >= 2 {
                    out.push((v, pl.closed));
                }
            }
            _ => {}
        }
    }
    if out.is_empty() {
        return Err(bad("wall.draw", "the selection has no lines or polylines"));
    }
    Ok(out)
}

fn run_wall(s: &mut Session, p: &Value) -> Result<Value> {
    let thick = f64_or(p, "thickness", 230.0 * per_mm(s));
    if !(thick.is_finite() && thick > 0.0) {
        return Err(bad("wall.draw", "`thickness` must be positive"));
    }
    let layer = str_param(p, "layer").unwrap_or(WALL).to_string();
    let paths = centre_paths(s, p)?;
    ensure_layer(s.doc_mut()?, &layer, 1, "Continuous", 35, "Walls");
    let mut made = Vec::new();
    for (pts, closed) in paths {
        let left = offset_path(&pts, closed, thick / 2.0);
        let right = offset_path(&pts, closed, -thick / 2.0);
        let n = pts.len();
        let count = if closed { n } else { n - 1 };
        for i in 0..count {
            let j = (i + 1) % n;
            for face in [&left, &right] {
                made.push(add_on(s, &layer, line(face[i], face[j]))?.hex());
            }
        }
        if !closed {
            made.push(add_on(s, &layer, line(left[0], right[0]))?.hex());
            made.push(add_on(s, &layer, line(left[n - 1], right[n - 1]))?.hex());
        }
    }
    Ok(json!({ "lines": made, "thickness": thick }))
}

// ---------------------------------------------------------------------------------------------
// Openings

struct Frame {
    origin: Vec2,
    u: Vec2,
    v: Vec2,
}

impl Frame {
    fn to_local(&self, p: Vec2) -> Vec2 {
        let d = p - self.origin;
        Vec2::new(d.x * self.u.x + d.y * self.u.y, d.x * self.v.x + d.y * self.v.y)
    }
    fn to_world(&self, q: Vec2) -> Vec2 {
        self.origin + self.u * q.x + self.v * q.y
    }
}

/// The direction of the wall line nearest to `at`, in radians.
fn nearest_wall_angle(d: &Drawing, layer: &str, at: Vec2, reach: f64) -> Option<f64> {
    let mut best: Option<(f64, f64)> = None;
    for e in d.model.iter().filter(|e| e.common.layer.eq_ignore_ascii_case(layer)) {
        if let EntityKind::Line(l) = &e.kind {
            let seg = Line::new(l.a.xy(), l.b.xy());
            let dist = seg.closest(at).dist(at);
            if dist <= reach && best.is_none_or(|(bd, _)| dist < bd) {
                best = Some((dist, (l.b.y - l.a.y).atan2(l.b.x - l.a.x)));
            }
        }
    }
    best.map(|(_, a)| a)
}

/// Cut `[-w/2, w/2]` out of the wall lines that run along `fr.u` through the band of half
/// thickness `reach`, returning how many lines were cut and the face offsets found.
fn cut_wall_lines(s: &mut Session, layer: &str, fr: &Frame, w: f64, reach: f64) -> Result<(usize, Vec<f64>)> {
    let hits: Vec<(Handle, Vec2, Vec2, Common)> = s
        .doc()?
        .model
        .iter()
        .filter(|e| e.common.layer.eq_ignore_ascii_case(layer))
        .filter_map(|e| match &e.kind {
            EntityKind::Line(l) => Some((e.handle, l.a.xy(), l.b.xy(), e.common.clone())),
            _ => None,
        })
        .collect();
    let mut cut = 0;
    let mut faces = Vec::new();
    for (h, a, b, common) in hits {
        let (la, lb) = (fr.to_local(a), fr.to_local(b));
        // Parallel to the wall, close to the opening's centre line, and overlapping its span.
        if (la.y - lb.y).abs() > 1e-6 * (1.0 + (la.x - lb.x).abs()) || la.y.abs() > reach + 1e-9 {
            continue;
        }
        let (lo, hi) = (la.x.min(lb.x), la.x.max(lb.x));
        let (c0, c1) = (-w / 2.0, w / 2.0);
        if hi <= c0 + 1e-9 || lo >= c1 - 1e-9 {
            continue;
        }
        faces.push(la.y);
        let y = la.y;
        s.doc_mut()?.remove_entity(h);
        let space = s.space();
        for (x0, x1) in [(lo, lo.max(c0).min(hi)), (hi.min(c1).max(lo), hi)] {
            if x1 - x0 > 1e-9 && ((x1 <= c0 + 1e-9) || (x0 >= c1 - 1e-9)) {
                let (p0, p1) = (fr.to_world(Vec2::new(x0, y)), fr.to_world(Vec2::new(x1, y)));
                s.doc_mut()?.add(&space, common.clone(), line(p0, p1))?;
            }
        }
        cut += 1;
    }
    Ok((cut, faces))
}

fn push(d: &mut Drawing, b: &mut Block, layer: &str, kind: EntityKind) {
    d.ensure_layer(layer);
    let h = d.new_handle();
    b.entities.push(Entity { handle: h, common: Common { layer: layer.into(), ..Common::default() }, kind });
}

fn opening(s: &mut Session, p: &Value, cmd: &str, door: bool) -> Result<Value> {
    let at = point_req(cmd, p, "at")?;
    let mm = per_mm(s);
    let layer = str_param(p, "layer").unwrap_or(WALL).to_string();
    // Width: explicit, else a schedule entry's length (metres) in drawing units, else a default.
    let sched = project::get(s.doc()?, "schedule");
    let entry = str_param(p, "name")
        .and_then(|n| sched["openings"].as_array()?.iter().find(|o| o["name"].as_str().is_some_and(|x| x.eq_ignore_ascii_case(n))))
        .cloned();
    if str_param(p, "name").is_some() && entry.is_none() {
        return Err(bad(cmd, "no such entry in the opening schedule"));
    }
    let to_du = |m: f64| m / s.doc().ok().and_then(ua::drawing_unit).map_or(0.001, |u| u.metres());
    let width = match (p.get("width").and_then(Value::as_f64), &entry) {
        (Some(w), _) => w,
        (None, Some(e)) => to_du(e["length"].as_f64().unwrap_or(0.9)),
        (None, None) => (if door { 900.0 } else { 1200.0 }) * mm,
    };
    if !(width.is_finite() && width > 0.0) {
        return Err(bad(cmd, "`width` must be positive"));
    }
    let thick_param = p.get("thickness").and_then(Value::as_f64);
    let angle = match p.get("angle").and_then(Value::as_f64) {
        Some(a) => a.to_radians(),
        None => nearest_wall_angle(s.doc()?, &layer, at, 400.0 * mm).ok_or_else(|| bad(cmd, "no wall line near `at`: give `angle`"))?,
    };
    let u = Vec2::new(angle.cos(), angle.sin());
    let fr = Frame { origin: at, u, v: Vec2::new(-u.y, u.x) };
    let reach = thick_param.map_or(400.0 * mm, |t| t / 2.0 + 1e-6);
    let (cut, faces) = cut_wall_lines(s, &layer, &fr, width, reach)?;
    let thick = thick_param.unwrap_or_else(|| {
        let (lo, hi) = faces.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), f| (l.min(*f), h.max(*f)));
        if hi > lo { hi - lo } else { 230.0 * mm }
    });
    // The block, named by kind, entry or width.
    let tag = match &entry {
        Some(e) => e["name"].as_str().unwrap_or("X").to_string(),
        None => format!("{}", (width / mm).round()),
    };
    let block = format!("{}-{tag}-{}", if door { "DOOR" } else { "WIN" }, (thick / mm).round());
    if s.doc()?.block(&block).is_none() {
        let d = s.doc_mut()?;
        let mut b = Block::new(&block);
        let (hw, ht) = (width / 2.0, thick / 2.0);
        let (jamb_layer, glass) = if door { (DOOR, false) } else { (WINDOW, true) };
        push(d, &mut b, jamb_layer, line(Vec2::new(-hw, -ht), Vec2::new(-hw, ht)));
        push(d, &mut b, jamb_layer, line(Vec2::new(hw, -ht), Vec2::new(hw, ht)));
        if door {
            // Leaf perpendicular to the wall at the hinge side, and its swing.
            push(d, &mut b, DOOR, line(Vec2::new(-hw, ht), Vec2::new(-hw, ht + width)));
            let arc = aad_doc::Arc { center: Vec3::new(-hw, ht, 0.0), radius: width, start: 0.0, end: std::f64::consts::FRAC_PI_2 };
            push(d, &mut b, DOOR, EntityKind::Arc(arc));
        } else if glass {
            for v in [-ht, -ht / 3.0, ht / 3.0, ht] {
                push(d, &mut b, WINDOW, line(Vec2::new(-hw, v), Vec2::new(hw, v)));
            }
        }
        if bool_or(p, "deduct", true) {
            d.ensure_layer(DEDUCT);
            push(d, &mut b, DEDUCT, line(Vec2::new(-hw, 0.0), Vec2::new(hw, 0.0)));
        }
        d.blocks.insert(block.clone(), Arc::new(b));
    }
    // Insert: mirrored for a right-hand swing, turned half a turn to swing to the other side.
    let mut rot = angle;
    if str_param(p, "side") == Some("down") {
        rot += std::f64::consts::PI;
    }
    let sx = if door && str_param(p, "swing") == Some("right") { -1.0 } else { 1.0 };
    let ins = EntityKind::Insert(Insert {
        block: block.clone(),
        insert: v3(at),
        scale: Vec3::new(sx, 1.0, 1.0),
        rotation: rot,
        attribs: Vec::new(),
        cols: 1,
        rows: 1,
        col_spacing: 0.0,
        row_spacing: 0.0,
    });
    let space = s.space();
    let mut common = s.current_common();
    common.layer = (if door { DOOR } else { WINDOW }).to_string();
    let h = s.doc_mut()?.add(&space, common, ins)?;
    // Link the schedule entry to this block when it has none.
    if let Some(e) = &entry {
        let name = e["name"].as_str().unwrap_or("");
        let mut sc = sched.clone();
        if let Some(list) = sc["openings"].as_array_mut()
            && let Some(o) = list.iter_mut().find(|o| o["name"].as_str() == Some(name))
        {
            let cur = o["block"].as_str().unwrap_or("").to_string();
            if !cur.split(';').any(|b| b.trim().eq_ignore_ascii_case(&block)) {
                o["block"] = json!(if cur.is_empty() { block.clone() } else { format!("{cur};{block}") });
                project::set(s.doc_mut()?, "schedule", &sc);
            }
        }
    }
    Ok(json!({ "block": block, "handle": h.hex(), "cut": cut, "thickness": thick, "width": width }))
}

fn run_door(s: &mut Session, p: &Value) -> Result<Value> {
    opening(s, p, "wall.door", true)
}

fn run_window(s: &mut Session, p: &Value) -> Result<Value> {
    opening(s, p, "wall.window", false)
}

// ---------------------------------------------------------------------------------------------
// Clean

fn run_clean(s: &mut Session, p: &Value) -> Result<Value> {
    let tol = f64_or(p, "tolerance", 1e-6 * per_mm(s).max(1e-9)).max(1e-12);
    let layer = str_param(p, "layer").map(str::to_string);
    let only: Option<Vec<Handle>> = if p.get("handles").is_some() { Some(targets(s, p)?) } else { None };
    let lines: Vec<(Handle, Vec2, Vec2, String)> = s
        .doc()?
        .model
        .iter()
        .filter(|e| layer.as_deref().is_none_or(|l| e.common.layer.eq_ignore_ascii_case(l)) && only.as_ref().is_none_or(|v| v.contains(&e.handle)))
        .filter_map(|e| match &e.kind {
            EntityKind::Line(l) => Some((e.handle, l.a.xy(), l.b.xy(), e.common.layer.clone())),
            _ => None,
        })
        .collect();
    let mut erased: Vec<Handle> = Vec::new();
    let mut alive: Vec<(Handle, Vec2, Vec2, String)> = Vec::new();
    for (h, a, b, l) in lines {
        if a.dist(b) <= tol {
            erased.push(h);
            continue;
        }
        let dup = alive.iter().any(|(_, c, d, ly)| *ly == l && ((c.near(a, tol) && d.near(b, tol)) || (c.near(b, tol) && d.near(a, tol))));
        if dup {
            erased.push(h);
        } else {
            alive.push((h, a, b, l));
        }
    }
    // Join lines that overlap or touch on one straight line (same layer).
    let mut joined = 0;
    let mut i = 0;
    while i < alive.len() {
        let mut merged = false;
        let mut j = i + 1;
        while j < alive.len() {
            let (_, a, b, l) = alive[i].clone();
            let (_, c, d, l2) = alive[j].clone();
            let dir = (b - a).normalized();
            let col = |p: Vec2| (p - a).cross(dir).abs() <= tol * 10.0;
            if l == l2 && col(c) && col(d) {
                let t = |p: Vec2| (p - a).dot(dir);
                let (lo1, hi1) = (t(a).min(t(b)), t(a).max(t(b)));
                let (lo2, hi2) = (t(c).min(t(d)), t(c).max(t(d)));
                if lo2 <= hi1 + tol && lo1 <= hi2 + tol {
                    let (lo, hi) = (lo1.min(lo2), hi1.max(hi2));
                    let (na, nb) = (a + dir * lo, a + dir * hi);
                    erased.push(alive[j].0);
                    alive[i].1 = na;
                    alive[i].2 = nb;
                    alive.remove(j);
                    joined += 1;
                    merged = true;
                    continue;
                }
            }
            j += 1;
        }
        if merged {
            continue;
        }
        i += 1;
    }
    let d = s.doc_mut()?;
    for h in &erased {
        d.remove_entity(*h);
    }
    for (h, a, b, _) in &alive {
        d.modify_entity(*h, |e| {
            if let EntityKind::Line(l) = &mut e.kind {
                l.a = v3(*a);
                l.b = v3(*b);
            }
        })?;
    }
    Ok(json!({ "erased": erased.len(), "joined": joined }))
}
