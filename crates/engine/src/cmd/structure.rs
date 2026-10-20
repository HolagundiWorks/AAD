//! Structure and vertical circulation (the kit's HCWAXIS, HCWCOLUMN, HCWCOLSCHED, HCWLINTEL,
//! AECSTAIR, HCWLIFT and HCWRAIL).
//!
//! * `grid.axis` draws a column grid from bay widths with numbered and lettered bubbles; the grid
//!   is kept in the drawing so `column.place` can put a column on every intersection.
//! * `column.schedule` marks the columns on `A-COL` (C1, C2 by size) and draws the schedule.
//! * `lintel.draw` draws a lintel line over each door and window block (opening plus a bearing at
//!   each end) on `MEASURE-LINTEL`, ready for the beam take-off.
//! * `stair.plan` draws a straight or dog-legged stair in plan and reports its geometry and
//!   concrete quantities from the inputs.
//! * `lift.shaft` draws a lift shaft in plan; `rail.draw` a handrail with posts along a path.

use aad_doc::{EntityKind, HAlign, VAlign};
use aad_geom::Vec2;
use serde_json::{Value, json};

use super::draw::make_text;
use super::hcw::{add_on, ensure_layer};
use super::helpers::{circle, line, lwpoly, rect_vertices};
use super::*;
use crate::annot::{self as ua};
use crate::{Result, Session};

const GRID: &str = "AN-GRID";
const COL: &str = "A-COL";
const STAIR: &str = "A-STAIR";
const RAIL: &str = "A-RAIL";
const WALL: &str = "A-WALL";
const LINTEL: &str = "MEASURE-LINTEL";

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("grid.axis", "Axis Grid", run_axis)
            .menu(&["Architecture", "Structure", "Axis Grid"])
            .alias(&["hcwaxis"])
            .params("{origin?: [x,y], bays: [width,...] (along x), rows?: [width,...] (along y), extend? (default 1.5 m)} → {xs, ys}"),
        CommandSpec::new("column.place", "Place Columns", run_columns)
            .menu(&["Architecture", "Structure", "Place Columns"])
            .alias(&["column", "hcwcolumn"])
            .params("{size: [w, h] | number, points?: [[x,y],...] (default every grid intersection), window?: [[x,y],[x,y]]} → {placed}"),
        CommandSpec::new("column.schedule", "Column Schedule", run_column_schedule)
            .menu(&["Architecture", "Structure", "Column Schedule"])
            .alias(&["hcwcolsched"])
            .params("{at?, units?} → the `takeoff.columns` result for the columns on A-COL"),
        CommandSpec::new("lintel.draw", "Generate Lintel", run_lintel)
            .menu(&["Architecture", "Structure", "Generate Lintel"])
            .alias(&["hcwlintel"])
            .params("{handles?: door/window block references (default all), bearing? (default 150 mm)} → {lintels}"),
        CommandSpec::new("stair.plan", "Stair", run_stair)
            .menu(&["Architecture", "Structure", "Stair"])
            .alias(&["stair", "aecstair"])
            .params("{at: [x,y], kind?: straight|dogleg (default straight), rise: floor height in metres, width (drawing units), going? (mm, default 250), riserMax? (mm, default 175), landing? (drawing units, dog-leg), waist? (mm, default 150)} → {risers, riser, going, run, concrete}"),
        CommandSpec::new("lift.shaft", "Lift Shaft", run_lift)
            .menu(&["Architecture", "Structure", "Lift Shaft"])
            .alias(&["hcwlift"])
            .params("{at: [x,y] (lower-left of the clear shaft), width, depth, wall? (default 230 mm), car?: [w, d] (default shaft less 200 mm each way)} → {outer, shaft, car}"),
        CommandSpec::new("rail.draw", "Handrail", run_rail)
            .menu(&["Architecture", "Structure", "Handrail"])
            .alias(&["hcwrail"])
            .params("{points: [[x,y],...], post? (spacing, default 1.2 m), size? (post size, default 50 mm)} → {posts}"),
    ]
}

fn mm(s: &Session) -> f64 {
    s.doc().ok().and_then(ua::drawing_unit).map_or(1.0, |u| 0.001 / u.metres())
}

fn text(s: &mut Session, layer: &str, at: Vec2, t: &str, h: f64, j: (HAlign, VAlign)) -> Result<()> {
    let kind = make_text(s, at, t, h, 0.0, j);
    add_on(s, layer, kind)?;
    Ok(())
}

fn letters(i: usize) -> String {
    let mut n = i;
    let mut out = String::new();
    loop {
        out.insert(0, (b'A' + (n % 26) as u8) as char);
        if n < 26 {
            return out;
        }
        n = n / 26 - 1;
    }
}

// ---------------------------------------------------------------------------------------------
// Grid and columns

fn widths(p: &Value, key: &str, cmd: &str) -> Result<Vec<f64>> {
    let Some(a) = p.get(key).and_then(Value::as_array) else { return Ok(Vec::new()) };
    if a.len() > 200 {
        return Err(bad(cmd, format!("`{key}` has too many bays")));
    }
    a.iter().map(|v| v.as_f64().filter(|x| x.is_finite() && *x > 0.0).ok_or_else(|| bad(cmd, format!("`{key}` needs positive widths")))).collect()
}

fn run_axis(s: &mut Session, p: &Value) -> Result<Value> {
    let xb = widths(p, "bays", "grid.axis")?;
    let yb = widths(p, "rows", "grid.axis")?;
    if xb.is_empty() && yb.is_empty() {
        return Err(bad("grid.axis", "give `bays` and/or `rows`"));
    }
    let o = point_param(p, "origin").unwrap_or(Vec2::ZERO);
    let m = mm(s);
    let ext = f64_or(p, "extend", 1500.0 * m);
    let r = 300.0 * m;
    ensure_layer(s.doc_mut()?, GRID, 8, "CENTER2", 13, "Grid and centre lines");
    let mut xs = vec![o.x];
    for w in &xb {
        xs.push(xs.last().copied().unwrap_or(o.x) + w);
    }
    let mut ys = vec![o.y];
    for w in &yb {
        ys.push(ys.last().copied().unwrap_or(o.y) + w);
    }
    let (y0, y1) = (o.y - ext, ys.last().copied().unwrap_or(o.y) + ext);
    let (x0, x1) = (o.x - ext, xs.last().copied().unwrap_or(o.x) + ext);
    let c = (HAlign::Center, VAlign::Middle);
    if !xb.is_empty() {
        for (i, x) in xs.iter().enumerate() {
            add_on(s, GRID, line(Vec2::new(*x, y0), Vec2::new(*x, y1)))?;
            add_on(s, GRID, circle(Vec2::new(*x, y0 - r), r))?;
            text(s, GRID, Vec2::new(*x, y0 - r), &(i + 1).to_string(), r * 1.1, c)?;
        }
    }
    if !yb.is_empty() {
        for (i, y) in ys.iter().enumerate() {
            add_on(s, GRID, line(Vec2::new(x0, *y), Vec2::new(x1, *y)))?;
            add_on(s, GRID, circle(Vec2::new(x0 - r, *y), r))?;
            text(s, GRID, Vec2::new(x0 - r, *y), &letters(i), r * 1.1, c)?;
        }
    }
    project::set(
        s.doc_mut()?,
        "grid",
        &json!({ "xs": if xb.is_empty() { Vec::new() } else { xs.clone() }, "ys": if yb.is_empty() { Vec::new() } else { ys.clone() } }),
    );
    Ok(json!({ "xs": xs, "ys": ys }))
}

fn run_columns(s: &mut Session, p: &Value) -> Result<Value> {
    let (w, h) = match p.get("size") {
        Some(Value::Array(a)) if a.len() == 2 => (a.first().and_then(Value::as_f64), a.get(1).and_then(Value::as_f64)),
        Some(v) => (v.as_f64(), v.as_f64()),
        None => (None, None),
    };
    let (Some(w), Some(h)) = (w.filter(|x| x.is_finite() && *x > 0.0), h.filter(|x| x.is_finite() && *x > 0.0)) else {
        return Err(bad("column.place", "`size` must be a positive number or [width, depth]"));
    };
    let pts: Vec<Vec2> = match points_param(p, "points") {
        Some(v) => v,
        None => {
            let g = project::get(s.doc()?, "grid");
            let xs: Vec<f64> = g["xs"].as_array().map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
            let ys: Vec<f64> = g["ys"].as_array().map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
            if xs.is_empty() || ys.is_empty() {
                return Err(bad("column.place", "no grid: draw one with `grid.axis` (both bays and rows) or give `points`"));
            }
            xs.iter().flat_map(|x| ys.iter().map(move |y| Vec2::new(*x, *y))).collect()
        }
    };
    let win = points_param(p, "window").filter(|v| v.len() == 2);
    if pts.len() > 20_000 {
        return Err(bad("column.place", "too many columns"));
    }
    ensure_layer(s.doc_mut()?, COL, 1, "Continuous", 35, "Columns (cut)");
    let mut placed = 0;
    for c in pts {
        if let Some(wn) = &win
            && !(c.x >= wn[0].x.min(wn[1].x) && c.x <= wn[0].x.max(wn[1].x) && c.y >= wn[0].y.min(wn[1].y) && c.y <= wn[0].y.max(wn[1].y))
        {
            continue;
        }
        add_on(s, COL, lwpoly(rect_vertices(Vec2::new(c.x - w / 2.0, c.y - h / 2.0), Vec2::new(c.x + w / 2.0, c.y + h / 2.0)), true))?;
        placed += 1;
    }
    Ok(json!({ "placed": placed }))
}

fn run_column_schedule(s: &mut Session, p: &Value) -> Result<Value> {
    let mut q = p.as_object().cloned().unwrap_or_default();
    q.insert("layer".into(), json!(COL));
    let spec = find_command("takeoff.columns").ok_or_else(|| bad("column.schedule", "no take-off"))?;
    (spec.run)(s, &Value::Object(q))
}

// ---------------------------------------------------------------------------------------------
// Lintels

fn run_lintel(s: &mut Session, p: &Value) -> Result<Value> {
    let m = mm(s);
    let bearing = f64_or(p, "bearing", 150.0 * m);
    if !(bearing.is_finite() && bearing >= 0.0) {
        return Err(bad("lintel.draw", "`bearing` must be zero or positive"));
    }
    let only: Option<Vec<aad_doc::Handle>> = if p.get("handles").is_some() { Some(targets(s, p)?) } else { None };
    let found: Vec<(Vec2, Vec2)> = {
        let d = s.doc()?;
        d.model
            .iter()
            .filter(|e| only.as_ref().is_none_or(|v| v.contains(&e.handle)))
            .filter_map(|e| {
                let EntityKind::Insert(i) = &e.kind else { return None };
                if !(i.block.starts_with("DOOR-") || i.block.starts_with("WIN-")) {
                    return None;
                }
                // The opening width is the span of the block's deduction line.
                let b = d.block(&i.block)?;
                let ded = b.entities.iter().find_map(|x| match &x.kind {
                    EntityKind::Line(l) if x.common.layer.eq_ignore_ascii_case("MEASURE-DEDUCT") => Some((l.a.x, l.b.x)),
                    _ => None,
                })?;
                let half = (ded.0 - ded.1).abs() * i.scale.x.abs() / 2.0 + bearing;
                let u = Vec2::new(i.rotation.cos(), i.rotation.sin());
                let c = i.insert.xy();
                Some((c - u * half, c + u * half))
            })
            .collect()
    };
    if found.is_empty() {
        return Err(bad("lintel.draw", "no door or window blocks to put lintels over"));
    }
    ensure_layer(s.doc_mut()?, LINTEL, 4, "Continuous", 25, "Take-off: concrete lintels");
    for (a, b) in &found {
        add_on(s, LINTEL, line(*a, *b))?;
    }
    Ok(json!({ "lintels": found.len() }))
}

// ---------------------------------------------------------------------------------------------
// Stairs

fn run_stair(s: &mut Session, p: &Value) -> Result<Value> {
    let at = point_req("stair.plan", p, "at")?;
    let rise = f64_req("stair.plan", p, "rise")?;
    let width = f64_req("stair.plan", p, "width")?;
    if !(rise.is_finite() && rise > 0.0 && rise < 12.0 && width.is_finite() && width > 0.0) {
        return Err(bad("stair.plan", "`rise` (metres, below 12) and `width` must be positive"));
    }
    let kind = str_param(p, "kind").unwrap_or("straight");
    if !["straight", "dogleg"].contains(&kind) {
        return Err(bad("stair.plan", "`kind` must be straight or dogleg"));
    }
    let unit = ua::drawing_unit(s.doc()?).map_or(0.001, |u| u.metres());
    let m = mm(s);
    let going = f64_or(p, "going", 250.0) * m;
    let riser_max = f64_or(p, "riserMax", 175.0);
    let waist = f64_or(p, "waist", 150.0);
    if !(going > 0.0 && riser_max > 50.0 && waist > 0.0) {
        return Err(bad("stair.plan", "`going`, `riserMax` and `waist` must be positive"));
    }
    let rise_mm = rise * 1000.0;
    let risers = (rise_mm / riser_max).ceil().max(2.0) as usize;
    if risers > 200 {
        return Err(bad("stair.plan", "too many risers"));
    }
    let riser = rise_mm / risers as f64;
    let treads = risers - 1;
    ensure_layer(s.doc_mut()?, STAIR, 2, "Continuous", 25, "Stairs");
    let th = 125.0 * m;
    let (flight_treads, run);
    let mut landing = 0.0;
    if kind == "straight" {
        flight_treads = treads;
        run = treads as f64 * going;
        let (a, b) = (at, Vec2::new(at.x + run, at.y + width));
        add_on(s, STAIR, lwpoly(rect_vertices(a, b), true))?;
        for i in 1..treads {
            let x = at.x + i as f64 * going;
            add_on(s, STAIR, line(Vec2::new(x, at.y), Vec2::new(x, at.y + width)))?;
        }
        add_on(s, STAIR, line(Vec2::new(at.x + going * 0.5, at.y + width / 2.0), Vec2::new(at.x + run - going * 0.5, at.y + width / 2.0)))?;
        text(s, STAIR, Vec2::new(at.x + run / 2.0, at.y + width / 2.0 + th), "UP", th, (HAlign::Center, VAlign::Middle))?;
    } else {
        // Two equal flights side by side joined by a half landing.
        landing = f64_or(p, "landing", width);
        let first = treads.div_ceil(2);
        let second = treads - first;
        flight_treads = first.max(second);
        run = flight_treads as f64 * going;
        let total_w = 2.0 * width;
        add_on(s, STAIR, lwpoly(rect_vertices(at, Vec2::new(at.x + run + landing, at.y + total_w)), true))?;
        add_on(s, STAIR, line(Vec2::new(at.x, at.y + width), Vec2::new(at.x + run, at.y + width)))?;
        add_on(s, STAIR, line(Vec2::new(at.x + run, at.y), Vec2::new(at.x + run, at.y + total_w)))?;
        for i in 1..first {
            let x = at.x + i as f64 * going;
            add_on(s, STAIR, line(Vec2::new(x, at.y), Vec2::new(x, at.y + width)))?;
        }
        for i in 1..second {
            let x = at.x + i as f64 * going;
            add_on(s, STAIR, line(Vec2::new(x, at.y + width), Vec2::new(x, at.y + total_w)))?;
        }
        text(s, STAIR, Vec2::new(at.x + run / 2.0, at.y + width / 2.0), "UP", th, (HAlign::Center, VAlign::Middle))?;
    }
    text(s, STAIR, Vec2::new(at.x, at.y - th * 1.5), &format!("{risers} R @ {riser:.0}  T @ {:.0}", going / m), th, (HAlign::Left, VAlign::Middle))?;
    // Concrete from the inputs: the waist slab along the slope plus the steps, per flight, in m3.
    let (w_m, g_m, r_m) = (width * unit, going * unit, riser / 1000.0);
    let slope = (g_m * g_m + r_m * r_m).sqrt();
    let flights = if kind == "straight" { 1.0 } else { 2.0 };
    let steps = if kind == "straight" { treads } else { treads.div_ceil(2) + (treads - treads.div_ceil(2)) } as f64;
    let waist_vol = steps * slope * w_m * (waist / 1000.0);
    let step_vol = steps * 0.5 * g_m * r_m * w_m;
    let landing_vol = if kind == "dogleg" { landing * unit * 2.0 * w_m * (waist / 1000.0) } else { 0.0 };
    let concrete = ((waist_vol + step_vol + landing_vol) * 1000.0).round() / 1000.0;
    Ok(json!({
        "kind": kind, "risers": risers, "riser": (riser * 10.0).round() / 10.0, "treads": treads, "going": going / m,
        "run": run, "flights": flights, "tallestFlightTreads": flight_treads, "concrete": concrete, "concreteUnit": "m3",
    }))
}

// ---------------------------------------------------------------------------------------------
// Lift and rail

fn run_lift(s: &mut Session, p: &Value) -> Result<Value> {
    let at = point_req("lift.shaft", p, "at")?;
    let (w, d) = (f64_req("lift.shaft", p, "width")?, f64_req("lift.shaft", p, "depth")?);
    let m = mm(s);
    let wall = f64_or(p, "wall", 230.0 * m);
    if !(w > 0.0 && d > 0.0 && wall > 0.0 && w.is_finite() && d.is_finite()) {
        return Err(bad("lift.shaft", "`width`, `depth` and `wall` must be positive"));
    }
    let car = match points_param(p, "car") {
        Some(v) if v.len() == 1 => (v[0].x, v[0].y),
        _ => (w - 200.0 * m, d - 200.0 * m),
    };
    let (cw, cd) = match p.get("car").and_then(Value::as_array) {
        Some(a) if a.len() == 2 => (a[0].as_f64().unwrap_or(car.0), a[1].as_f64().unwrap_or(car.1)),
        _ => car,
    };
    if !(cw > 0.0 && cd > 0.0 && cw <= w && cd <= d) {
        return Err(bad("lift.shaft", "the car must fit inside the shaft"));
    }
    ensure_layer(s.doc_mut()?, WALL, 1, "Continuous", 35, "Walls");
    let (lo, hi) = (at, Vec2::new(at.x + w, at.y + d));
    add_on(s, WALL, lwpoly(rect_vertices(lo, hi), true))?;
    add_on(s, WALL, lwpoly(rect_vertices(Vec2::new(lo.x - wall, lo.y - wall), Vec2::new(hi.x + wall, hi.y + wall)), true))?;
    // The car, centred against the back wall, with its door side to the front (low y).
    let cx = at.x + (w - cw) / 2.0;
    let cy = at.y + d - cd - 50.0 * m;
    add_on(s, "A-DETAIL", lwpoly(rect_vertices(Vec2::new(cx, cy), Vec2::new(cx + cw, cy + cd)), true))?;
    add_on(s, "A-DETAIL", line(Vec2::new(cx, cy), Vec2::new(cx + cw, cy)))?;
    add_on(s, "A-DETAIL", line(Vec2::new(cx, cy + cd), Vec2::new(cx + cw, cy)))?;
    add_on(s, "A-DETAIL", line(Vec2::new(cx + cw, cy + cd), Vec2::new(cx, cy)))?;
    Ok(json!({ "shaft": [w, d], "outer": [w + 2.0 * wall, d + 2.0 * wall], "car": [cw, cd] }))
}

fn run_rail(s: &mut Session, p: &Value) -> Result<Value> {
    let pts = points_param(p, "points").filter(|v| v.len() >= 2).ok_or_else(|| bad("rail.draw", "`points` needs two or more points"))?;
    let m = mm(s);
    let spacing = f64_or(p, "post", 1200.0 * m);
    let size = f64_or(p, "size", 50.0 * m);
    if !(spacing > 0.0 && size > 0.0 && spacing.is_finite() && size.is_finite()) {
        return Err(bad("rail.draw", "`post` and `size` must be positive"));
    }
    ensure_layer(s.doc_mut()?, RAIL, 6, "Continuous", 20, "Handrails");
    add_on(s, RAIL, lwpoly(pts.iter().map(|p| aad_geom::PolyVertex::new(*p)).collect(), false))?;
    let mut posts = 0;
    let mut place = |s: &mut Session, c: Vec2| -> Result<()> {
        add_on(s, RAIL, lwpoly(rect_vertices(Vec2::new(c.x - size / 2.0, c.y - size / 2.0), Vec2::new(c.x + size / 2.0, c.y + size / 2.0)), true))?;
        posts += 1;
        Ok(())
    };
    place(s, pts[0])?;
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = a.dist(b);
        let n = (len / spacing).ceil().max(1.0) as usize;
        if n > 5000 {
            return Err(bad("rail.draw", "too many posts"));
        }
        for i in 1..=n {
            place(s, a + (b - a) * (i as f64 / n as f64))?;
        }
    }
    Ok(json!({ "posts": posts }))
}
