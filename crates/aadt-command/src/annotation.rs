//! Drawing annotation furniture: tables, counts, auto-number, section marks,
//! callouts, revision triangles, and revision clouds — all as ordinary
//! LINE / POLYLINE / TEXT / CIRCLE geometry.

use std::collections::BTreeMap;

use aadt_geometry::{Circle, Point2, Polyline, Segment};
use aadt_model::{Geometry, Text};

use crate::{entities_word, parse_f64, resolve_targets, Command, Context, Registry};

pub fn register_annotation(r: &mut Registry) {
    r.register(Box::new(TableCmd));
    r.register(Box::new(CountCmd));
    r.register(Box::new(AutoNumCmd));
    r.register(Box::new(SectionLineCmd));
    r.register(Box::new(CalloutCmd));
    r.register(Box::new(RevMarkCmd));
    r.register(Box::new(RevCloudCmd));
}

// --- TABLE ------------------------------------------------------------------

struct TableCmd;
impl Command for TableCmd {
    fn name(&self) -> &str {
        "TABLE"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["TB"]
    }
    fn help(&self) -> &str {
        "TABLE x y rows cols [cell_w cell_h] [layer] — insert an empty ruled table"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let x = parse_f64(args, 0, "TABLE")?;
        let y = parse_f64(args, 1, "TABLE")?;
        let rows = parse_f64(args, 2, "TABLE")? as usize;
        let cols = parse_f64(args, 3, "TABLE")? as usize;
        if rows == 0 || cols == 0 || rows > 200 || cols > 50 {
            return Err("TABLE: rows/cols must be 1..=200 / 1..=50".into());
        }
        let mut cell_w = 2.5;
        let mut cell_h = 0.5;
        let mut layer = "A-ANNO-TABL".to_string();
        let mut i = 4;
        if let Some(w) = args.get(i).and_then(|s| s.parse::<f64>().ok()) {
            cell_w = w;
            i += 1;
            if let Some(h) = args.get(i).and_then(|s| s.parse::<f64>().ok()) {
                cell_h = h;
                i += 1;
            }
        }
        if let Some(l) = args.get(i) {
            layer = l.clone();
        }
        if cell_w <= 0.0 || cell_h <= 0.0 {
            return Err("TABLE: cell size must be positive".into());
        }
        let origin = Point2::new(x, y);
        let tw = cols as f64 * cell_w;
        let th = rows as f64 * cell_h;
        let n = ctx.doc.transact("TABLE", |d| {
            let mut ids = Vec::new();
            for r in 0..=rows {
                let yy = origin.y - r as f64 * cell_h;
                ids.push(d.add_geometry(
                    layer.clone(),
                    Geometry::Line(Segment::new(
                        Point2::new(origin.x, yy),
                        Point2::new(origin.x + tw, yy),
                    )),
                ));
            }
            for c in 0..=cols {
                let xx = origin.x + c as f64 * cell_w;
                ids.push(d.add_geometry(
                    layer.clone(),
                    Geometry::Line(Segment::new(
                        Point2::new(xx, origin.y),
                        Point2::new(xx, origin.y - th),
                    )),
                ));
            }
            ids.len()
        });
        Ok(format!(
            "TABLE {rows}x{cols} placed at ({x}, {y}) — {n} entities"
        ))
    }
}

// --- COUNT ------------------------------------------------------------------

struct CountCmd;
impl Command for CountCmd {
    fn name(&self) -> &str {
        "COUNT"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["OBJCOUNT"]
    }
    fn help(&self) -> &str {
        "COUNT [ALL|LAYER|TYPE] — report object counts by layer and geometry type"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let mode = args
            .first()
            .map(|s| s.to_ascii_uppercase())
            .unwrap_or_else(|| "ALL".into());
        let mut by_layer: BTreeMap<String, usize> = BTreeMap::new();
        let mut by_type: BTreeMap<String, usize> = BTreeMap::new();
        let mut total = 0usize;
        for e in ctx.doc.entities() {
            let include = match mode.as_str() {
                "ALL" => true,
                "LAYER" => {
                    let name = args.get(1).ok_or("COUNT LAYER: missing name")?;
                    &e.layer == name
                }
                "TYPE" => {
                    let ty = args
                        .get(1)
                        .ok_or("COUNT TYPE: missing type")?
                        .to_ascii_uppercase();
                    geom_kind(&e.geometry) == ty
                }
                _ => {
                    return Err("COUNT: use ALL, LAYER <name>, or TYPE <LINE|…>".into());
                }
            };
            if !include {
                continue;
            }
            total += 1;
            *by_layer.entry(e.layer.clone()).or_default() += 1;
            *by_type.entry(geom_kind(&e.geometry)).or_default() += 1;
        }
        let mut lines = vec![format!("object count: {total}")];
        if mode == "ALL" || mode == "LAYER" {
            lines.push("by layer:".into());
            for (k, n) in &by_layer {
                lines.push(format!("  {k}: {n}"));
            }
        }
        if mode == "ALL" || mode == "TYPE" {
            lines.push("by type:".into());
            for (k, n) in &by_type {
                lines.push(format!("  {k}: {n}"));
            }
        }
        let hatches = ctx.doc.hatches().len();
        let dims = ctx.doc.dimensions().len();
        if mode == "ALL" {
            lines.push(format!("hatches: {hatches}"));
            lines.push(format!("dimensions: {dims}"));
        }
        Ok(lines.join("\n"))
    }
}

fn geom_kind(g: &Geometry) -> String {
    match g {
        Geometry::Line(_) => "LINE".into(),
        Geometry::Polyline(_) => "POLYLINE".into(),
        Geometry::Circle(_) => "CIRCLE".into(),
        Geometry::Arc(_) => "ARC".into(),
        Geometry::Text(_) => "TEXT".into(),
        Geometry::Insert(_) => "INSERT".into(),
    }
}

// --- AUTONUM ----------------------------------------------------------------

struct AutoNumCmd;
impl Command for AutoNumCmd {
    fn name(&self) -> &str {
        "AUTONUM"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["NUMBER", "NUM"]
    }
    fn help(&self) -> &str {
        "AUTONUM target prefix start [height] — number TEXT entities (or place tags at inserts)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let token = args.first().ok_or("AUTONUM: missing target")?;
        let prefix = args.get(1).cloned().unwrap_or_else(|| "".into());
        let start = args.get(2).and_then(|s| s.parse::<i64>().ok()).unwrap_or(1);
        let height = args
            .get(3)
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.25);
        let ids = resolve_targets(ctx, token, "AUTONUM")?;
        let mut n = 0i64;
        let mut next = start;
        ctx.doc.transact("AUTONUM", |d| {
            for id in &ids {
                let Some(e) = d.get(*id) else { continue };
                let label = format!("{prefix}{next}");
                match &e.geometry {
                    Geometry::Text(t) => {
                        let mut t2 = t.clone();
                        t2.value = label;
                        d.replace_geometry(*id, Geometry::Text(t2));
                        next += 1;
                        n += 1;
                    }
                    Geometry::Insert(ins) => {
                        let p = Point2::new(ins.position.x, ins.position.y + height * 1.2);
                        d.add_geometry(
                            "A-ANNO-IDEN",
                            Geometry::Text(Text::new(label, p, height, 0.0)),
                        );
                        next += 1;
                        n += 1;
                    }
                    Geometry::Circle(c) => {
                        d.add_geometry(
                            "A-ANNO-IDEN",
                            Geometry::Text(Text::new(
                                label,
                                Point2::new(c.center.x, c.center.y),
                                height,
                                0.0,
                            )),
                        );
                        next += 1;
                        n += 1;
                    }
                    _ => {}
                }
            }
        });
        // Persist next number hint on drawing properties.
        let mut props = ctx.doc.properties().clone();
        props.autonum_prefix = prefix.clone();
        props.autonum_next = next.to_string();
        ctx.doc.set_properties(props);
        Ok(format!(
            "autonumbered {n} {} (next = {prefix}{next})",
            entities_word(n as usize)
        ))
    }
}

// --- SECTIONLINE ------------------------------------------------------------

struct SectionLineCmd;
impl Command for SectionLineCmd {
    fn name(&self) -> &str {
        "SECTIONLINE"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["SECLINE", "CUTLINE"]
    }
    fn help(&self) -> &str {
        "SECTIONLINE x1 y1 x2 y2 [label] — plan section cut with end bubbles"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let a = Point2::new(
            parse_f64(args, 0, "SECTIONLINE")?,
            parse_f64(args, 1, "SECTIONLINE")?,
        );
        let b = Point2::new(
            parse_f64(args, 2, "SECTIONLINE")?,
            parse_f64(args, 3, "SECTIONLINE")?,
        );
        if a.coincident(b) {
            return Err("SECTIONLINE: ends must differ".into());
        }
        let label = args.get(4).cloned().unwrap_or_else(|| "A".into());
        let len = a.distance(b);
        let ux = (b.x - a.x) / len;
        let uy = (b.y - a.y) / len;
        // Perpendicular for arrow ticks.
        let px = -uy;
        let py = ux;
        let tick = (len * 0.04).clamp(0.15, 0.6);
        let r = tick * 0.9;
        let ht = r * 0.9;
        ctx.doc.transact("SECTIONLINE", |d| {
            d.add_geometry("A-SECT-IDEN", Geometry::Line(Segment::new(a, b)));
            // End ticks.
            for p in [a, b] {
                d.add_geometry(
                    "A-SECT-IDEN",
                    Geometry::Line(Segment::new(
                        Point2::new(p.x - px * tick, p.y - py * tick),
                        Point2::new(p.x + px * tick, p.y + py * tick),
                    )),
                );
            }
            // Bubbles offset past ends.
            let ba = Point2::new(a.x - ux * (r + tick), a.y - uy * (r + tick));
            let bb = Point2::new(b.x + ux * (r + tick), b.y + uy * (r + tick));
            for (c, lab) in [(ba, &label), (bb, &label)] {
                d.add_geometry(
                    "A-SECT-IDEN",
                    Geometry::Circle(Circle {
                        center: c,
                        radius: r,
                    }),
                );
                d.add_geometry(
                    "A-SECT-IDEN",
                    Geometry::Text(Text::new(
                        lab.clone(),
                        Point2::new(c.x - ht * 0.35, c.y - ht * 0.35),
                        ht,
                        0.0,
                    )),
                );
            }
        });
        Ok(format!("section line '{label}' placed"))
    }
}

// --- CALLOUT ----------------------------------------------------------------

struct CalloutCmd;
impl Command for CalloutCmd {
    fn name(&self) -> &str {
        "CALLOUT"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["COUT", "BUBBLE"]
    }
    fn help(&self) -> &str {
        "CALLOUT ax ay tx ty [radius] text... — bubble callout with leader"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let ax = parse_f64(args, 0, "CALLOUT")?;
        let ay = parse_f64(args, 1, "CALLOUT")?;
        let tx = parse_f64(args, 2, "CALLOUT")?;
        let ty = parse_f64(args, 3, "CALLOUT")?;
        let mut i = 4;
        let mut radius = 0.35;
        if let Some(r) = args.get(i).and_then(|s| s.parse::<f64>().ok()) {
            radius = r;
            i += 1;
        }
        let text = args.get(i..).unwrap_or(&[]).join(" ");
        if text.is_empty() {
            return Err("CALLOUT: missing text".into());
        }
        if radius <= 0.0 {
            return Err("CALLOUT: radius must be positive".into());
        }
        let arrow = Point2::new(ax, ay);
        let center = Point2::new(tx, ty);
        let ht = radius * 0.85;
        ctx.doc.transact("CALLOUT", |d| {
            d.add_geometry("A-ANNO-NOTE", Geometry::Line(Segment::new(arrow, center)));
            d.add_geometry("A-ANNO-NOTE", Geometry::Circle(Circle { center, radius }));
            d.add_geometry(
                "A-ANNO-NOTE",
                Geometry::Text(Text::new(
                    text.clone(),
                    Point2::new(
                        center.x - ht * 0.35 * text.chars().count() as f64 * 0.15,
                        center.y - ht * 0.35,
                    ),
                    ht,
                    0.0,
                )),
            );
        });
        Ok(format!("callout '{text}' placed"))
    }
}

// --- REVMARK ----------------------------------------------------------------

struct RevMarkCmd;
impl Command for RevMarkCmd {
    fn name(&self) -> &str {
        "REVMARK"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["REVCLOUDMARK", "REVT"]
    }
    fn help(&self) -> &str {
        "REVMARK x y [letter] [size] — revision triangle mark"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let x = parse_f64(args, 0, "REVMARK")?;
        let y = parse_f64(args, 1, "REVMARK")?;
        let letter = args.get(2).cloned().unwrap_or_else(|| "A".into());
        let size = args
            .get(3)
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.5);
        if size <= 0.0 {
            return Err("REVMARK: size must be positive".into());
        }
        let c = Point2::new(x, y);
        let h = size * 0.866; // equilateral height factor
        let verts = vec![
            Point2::new(c.x, c.y + h * 0.67),
            Point2::new(c.x - size * 0.5, c.y - h * 0.33),
            Point2::new(c.x + size * 0.5, c.y - h * 0.33),
        ];
        let ht = size * 0.35;
        ctx.doc.transact("REVMARK", |d| {
            d.add_geometry(
                "A-ANNO-REVS",
                Geometry::Polyline(Polyline::new(verts, true)),
            );
            d.add_geometry(
                "A-ANNO-REVS",
                Geometry::Text(Text::new(
                    letter.clone(),
                    Point2::new(c.x - ht * 0.3, c.y - ht * 0.35),
                    ht,
                    0.0,
                )),
            );
        });
        Ok(format!("revision mark '{letter}' placed"))
    }
}

// --- REVCLOUD ---------------------------------------------------------------

struct RevCloudCmd;
impl Command for RevCloudCmd {
    fn name(&self) -> &str {
        "REVCLOUD"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["RCLOUD"]
    }
    fn help(&self) -> &str {
        "REVCLOUD x1 y1 x2 y2 [arc] — rectangular revision cloud (scalloped polyline)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let x1 = parse_f64(args, 0, "REVCLOUD")?;
        let y1 = parse_f64(args, 1, "REVCLOUD")?;
        let x2 = parse_f64(args, 2, "REVCLOUD")?;
        let y2 = parse_f64(args, 3, "REVCLOUD")?;
        let arc = args
            .get(4)
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.25);
        let (xmin, xmax) = (x1.min(x2), x1.max(x2));
        let (ymin, ymax) = (y1.min(y2), y1.max(y2));
        if xmax - xmin < arc * 2.0 || ymax - ymin < arc * 2.0 {
            return Err("REVCLOUD: box too small for arc size".into());
        }
        let verts = scalloped_rect(xmin, ymin, xmax, ymax, arc);
        let n = verts.len();
        ctx.doc.transact("REVCLOUD", |d| {
            d.add_geometry(
                "A-ANNO-REVS",
                Geometry::Polyline(Polyline::new(verts, true)),
            );
        });
        Ok(format!("revision cloud placed ({n} vertices)"))
    }
}

fn scalloped_rect(xmin: f64, ymin: f64, xmax: f64, ymax: f64, arc: f64) -> Vec<Point2> {
    let mut out = Vec::new();
    let bump = |ax: f64, ay: f64, bx: f64, by: f64, outward_nx: f64, outward_ny: f64| {
        let len = ((bx - ax).hypot(by - ay)).max(1e-9);
        let n = ((len / arc).floor() as usize).max(1);
        let mut pts = Vec::new();
        for i in 0..n {
            let t0 = i as f64 / n as f64;
            let t1 = (i + 1) as f64 / n as f64;
            let p0 = Point2::new(ax + (bx - ax) * t0, ay + (by - ay) * t0);
            let p1 = Point2::new(ax + (bx - ax) * t1, ay + (by - ay) * t1);
            let mid = p0.midpoint(p1);
            let crest = Point2::new(
                mid.x + outward_nx * arc * 0.55,
                mid.y + outward_ny * arc * 0.55,
            );
            // Quarter-ish bulge via two samples.
            pts.push(p0);
            pts.push(Point2::new((p0.x + crest.x) * 0.5, (p0.y + crest.y) * 0.5));
            pts.push(crest);
            pts.push(Point2::new((p1.x + crest.x) * 0.5, (p1.y + crest.y) * 0.5));
            let _ = p1;
        }
        pts.push(Point2::new(bx, by));
        pts
    };
    out.extend(bump(xmin, ymin, xmax, ymin, 0.0, -1.0));
    out.extend(bump(xmax, ymin, xmax, ymax, 1.0, 0.0));
    out.extend(bump(xmax, ymax, xmin, ymax, 0.0, 1.0));
    out.extend(bump(xmin, ymax, xmin, ymin, -1.0, 0.0));
    // Dedup near-duplicates.
    let mut cleaned = Vec::new();
    for p in out {
        if cleaned
            .last()
            .map(|q: &Point2| q.distance(p) > 1e-9)
            .unwrap_or(true)
        {
            cleaned.push(p);
        }
    }
    cleaned
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Interpreter;

    #[test]
    fn table_emits_grid() {
        let i = Interpreter::with_builtins();
        let mut ctx = Context::new();
        i.eval_line(&mut ctx, "TABLE 0 0 2 3 1 0.5").unwrap();
        // 3 horiz * 2 rows+1 + 4 vert * 3 cols+1 = 3+4 wait: (rows+1)+(cols+1) lines
        // 2 rows → 3 horiz, 3 cols → 4 vert = 7
        assert_eq!(ctx.doc.len(), 7);
    }

    #[test]
    fn count_reports_entities() {
        let i = Interpreter::with_builtins();
        let mut ctx = Context::new();
        i.eval_line(&mut ctx, "LINE 0 0 1 0 A").unwrap();
        i.eval_line(&mut ctx, "CIRCLE 0 0 1 B").unwrap();
        let msg = i.eval_line(&mut ctx, "COUNT").unwrap().unwrap();
        assert!(msg.contains("object count: 2"));
        assert!(msg.contains("LINE: 1"));
    }

    #[test]
    fn callout_and_revmark() {
        let i = Interpreter::with_builtins();
        let mut ctx = Context::new();
        i.eval_line(&mut ctx, "CALLOUT 0 0 2 2 NOTE").unwrap();
        i.eval_line(&mut ctx, "REVMARK 5 5 B").unwrap();
        assert!(ctx.doc.len() >= 5);
    }

    #[test]
    fn sectionline_places_bubbles() {
        let i = Interpreter::with_builtins();
        let mut ctx = Context::new();
        i.eval_line(&mut ctx, "SECTIONLINE 0 0 10 0 A").unwrap();
        assert!(ctx
            .doc
            .entities()
            .iter()
            .any(|e| matches!(e.geometry, Geometry::Circle(_))));
    }
}
