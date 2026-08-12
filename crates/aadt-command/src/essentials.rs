//! Essential AutoCAD-style commands beyond the original M1 builtins:
//! polygons/curves (as polylines), lengthen/break/join, blocks, layer isolate,
//! clipboard, qselect, props, measure, nav hints, and deferred stubs.

use std::collections::HashMap;
use std::f64::consts::{PI, TAU};

use aadt_geometry::{Circle, Point2, Polyline, Segment};
use aadt_model::{Block, Entity, EntityId, Geometry, ImageRef, Insert, Layer, Text};
use std::path::Path;

use crate::{
    entities_word, parse_entity_id, parse_f64, resolve_targets, Command, Context, Registry,
};

/// Register the essential-command pack onto an existing registry.
pub fn register_essentials(r: &mut Registry) {
    // Draw approximations.
    r.register(Box::new(PolygonCmd));
    r.register(Box::new(EllipseCmd));
    r.register(Box::new(DonutCmd));
    r.register(Box::new(SplineCmd));
    // Modify.
    r.register(Box::new(LengthenCmd));
    r.register(Box::new(BreakCmd));
    r.register(Box::new(JoinCmd));
    r.register(Box::new(MatchPropCmd));
    // Annotate.
    r.register(Box::new(MtextCmd));
    r.register(Box::new(DimStyleCmd));
    // Layers.
    r.register(Box::new(LayIsoCmd));
    r.register(Box::new(LayUnisoCmd));
    r.register(Box::new(LayOffCmd));
    r.register(Box::new(LayOnCmd));
    // Blocks.
    r.register(Box::new(BlockCmd));
    r.register(Box::new(WblockCmd));
    r.register(Box::new(EattEditCmd));
    r.register(Box::new(ObjMetaCmd));
    // Selection.
    r.register(Box::new(QSelectCmd));
    r.register(Box::new(PSelectCmd));
    // Info.
    r.register(Box::new(IdCmd));
    r.register(Box::new(MeasureCmd));
    // Nav.
    r.register(Box::new(ZoomCmd));
    r.register(Box::new(PanCmd));
    r.register(Box::new(RegenCmd));
    // Clipboard.
    r.register(Box::new(CopyClipCmd));
    r.register(Box::new(PasteClipCmd));
    r.register(Box::new(CutClipCmd));
    // Props.
    r.register(Box::new(PropertiesCmd));
    r.register(Box::new(ChangeCmd));
    // Utility.
    r.register(Box::new(UnitsCmd));
    r.register(Box::new(TemplateCmd));
    r.register(Box::new(AuditCmd));
    r.register(Box::new(OverkillCmd));
    // Deferred stubs (aliases exist; clear errors).
    r.register(Box::new(StubCmd {
        name: "XREF",
        aliases: &["XR"],
        help: "XREF — external references (not implemented in this milestone)",
    }));
    r.register(Box::new(ImageCmd));
    r.register(Box::new(BlockLibCmd));
    r.register(Box::new(StubCmd {
        name: "PDFATTACH",
        aliases: &[],
        help: "PDFATTACH — attach PDF underlay (not implemented in this milestone)",
    }));
    r.register(Box::new(StubCmd {
        name: "VP",
        aliases: &[],
        help: "VP — paper-space viewport (not implemented in this milestone)",
    }));
    r.register(Box::new(StubCmd {
        name: "MVIEW",
        aliases: &["MV"],
        help: "MVIEW — make viewport (not implemented in this milestone)",
    }));
    r.register(Box::new(StubCmd {
        name: "UCS",
        aliases: &[],
        help: "UCS — user coordinate system (not implemented in this milestone)",
    }));
    r.register(Box::new(StubCmd {
        name: "PLAN",
        aliases: &[],
        help: "PLAN — plan view of UCS (not implemented in this milestone)",
    }));
    r.register(Box::new(StubCmd {
        name: "OPTIONS",
        aliases: &[],
        help: "OPTIONS — application options dialog (not implemented in this milestone)",
    }));
}

// --- Draw -------------------------------------------------------------------

struct PolygonCmd;
impl Command for PolygonCmd {
    fn name(&self) -> &str {
        "POLYGON"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["POL"]
    }
    fn help(&self) -> &str {
        "POLYGON n cx cy radius [layer] — regular n-gon as a closed polyline"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let n = parse_f64(args, 0, "POLYGON")? as usize;
        if n < 3 {
            return Err("POLYGON: need at least 3 sides".into());
        }
        let cx = parse_f64(args, 1, "POLYGON")?;
        let cy = parse_f64(args, 2, "POLYGON")?;
        let radius = parse_f64(args, 3, "POLYGON")?;
        if radius <= 0.0 {
            return Err("POLYGON: radius must be positive".into());
        }
        let layer = args.get(4).cloned().unwrap_or_else(|| "0".into());
        let verts: Vec<Point2> = (0..n)
            .map(|i| {
                let a = (i as f64 / n as f64) * TAU - PI / 2.0;
                Point2::new(cx + radius * a.cos(), cy + radius * a.sin())
            })
            .collect();
        let id = ctx.doc.transact("POLYGON", |d| {
            d.add_geometry(layer, Geometry::Polyline(Polyline::new(verts, true)))
        });
        Ok(format!("POLYGON ({n} sides) added as entity {}", id.0))
    }
}

struct EllipseCmd;
impl Command for EllipseCmd {
    fn name(&self) -> &str {
        "ELLIPSE"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["EL"]
    }
    fn help(&self) -> &str {
        "ELLIPSE cx cy rx ry [layer] — ellipse as a closed polyline (48 segments)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let cx = parse_f64(args, 0, "ELLIPSE")?;
        let cy = parse_f64(args, 1, "ELLIPSE")?;
        let rx = parse_f64(args, 2, "ELLIPSE")?;
        let ry = parse_f64(args, 3, "ELLIPSE")?;
        if rx <= 0.0 || ry <= 0.0 {
            return Err("ELLIPSE: radii must be positive".into());
        }
        let layer = args.get(4).cloned().unwrap_or_else(|| "0".into());
        let n = 48usize;
        let verts: Vec<Point2> = (0..n)
            .map(|i| {
                let a = (i as f64 / n as f64) * TAU;
                Point2::new(cx + rx * a.cos(), cy + ry * a.sin())
            })
            .collect();
        let id = ctx.doc.transact("ELLIPSE", |d| {
            d.add_geometry(layer, Geometry::Polyline(Polyline::new(verts, true)))
        });
        Ok(format!(
            "ELLIPSE (polyline approx) added as entity {}",
            id.0
        ))
    }
}

struct DonutCmd;
impl Command for DonutCmd {
    fn name(&self) -> &str {
        "DONUT"
    }
    fn help(&self) -> &str {
        "DONUT cx cy r_inner r_outer [layer] — two concentric circles (donut)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let cx = parse_f64(args, 0, "DONUT")?;
        let cy = parse_f64(args, 1, "DONUT")?;
        let mut ri = parse_f64(args, 2, "DONUT")?;
        let mut ro = parse_f64(args, 3, "DONUT")?;
        if ri > ro {
            std::mem::swap(&mut ri, &mut ro);
        }
        if ri < 0.0 || ro <= 0.0 || (ro - ri).abs() < 1e-12 {
            return Err("DONUT: need two distinct non-negative radii".into());
        }
        let layer = args.get(4).cloned().unwrap_or_else(|| "0".into());
        let c = Point2::new(cx, cy);
        let (a, b) = ctx.doc.transact("DONUT", |d| {
            let inner = d.add_geometry(
                layer.clone(),
                Geometry::Circle(Circle {
                    center: c,
                    radius: ri,
                }),
            );
            let outer = d.add_geometry(
                layer,
                Geometry::Circle(Circle {
                    center: c,
                    radius: ro,
                }),
            );
            (inner, outer)
        });
        Ok(format!(
            "DONUT added (inner entity {}, outer entity {})",
            a.0, b.0
        ))
    }
}

struct SplineCmd;
impl Command for SplineCmd {
    fn name(&self) -> &str {
        "SPLINE"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["SPL"]
    }
    fn help(&self) -> &str {
        "SPLINE x1 y1 x2 y2 [...] [layer] — Catmull-Rom spline as an open polyline"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let mut layer: Option<String> = None;
        let mut nums: Vec<f64> = Vec::new();
        for tok in args {
            if let Ok(n) = tok.parse::<f64>() {
                nums.push(n);
            } else if layer.is_none() {
                layer = Some(tok.clone());
            } else {
                return Err(format!("SPLINE: unexpected argument {tok:?}"));
            }
        }
        if nums.len() < 4 || !nums.len().is_multiple_of(2) {
            return Err("SPLINE: need at least two x y pairs".into());
        }
        let pts: Vec<Point2> = nums
            .chunks_exact(2)
            .map(|c| Point2::new(c[0], c[1]))
            .collect();
        let densified = catmull_rom(&pts, 8);
        let n = densified.len();
        let layer = layer.unwrap_or_else(|| "0".into());
        let id = ctx.doc.transact("SPLINE", |d| {
            d.add_geometry(layer, Geometry::Polyline(Polyline::new(densified, false)))
        });
        Ok(format!(
            "SPLINE (polyline approx, {n} verts) added as entity {}",
            id.0
        ))
    }
}

fn catmull_rom(pts: &[Point2], subdiv: usize) -> Vec<Point2> {
    if pts.len() < 2 {
        return pts.to_vec();
    }
    let mut out = Vec::new();
    let n = pts.len();
    for i in 0..n.saturating_sub(1) {
        let p0 = pts[i.saturating_sub(1)];
        let p1 = pts[i];
        let p2 = pts[(i + 1).min(n - 1)];
        let p3 = pts[(i + 2).min(n - 1)];
        for s in 0..subdiv {
            let t = s as f64 / subdiv as f64;
            let t2 = t * t;
            let t3 = t2 * t;
            let x = 0.5
                * ((2.0 * p1.x)
                    + (-p0.x + p2.x) * t
                    + (2.0 * p0.x - 5.0 * p1.x + 4.0 * p2.x - p3.x) * t2
                    + (-p0.x + 3.0 * p1.x - 3.0 * p2.x + p3.x) * t3);
            let y = 0.5
                * ((2.0 * p1.y)
                    + (-p0.y + p2.y) * t
                    + (2.0 * p0.y - 5.0 * p1.y + 4.0 * p2.y - p3.y) * t2
                    + (-p0.y + 3.0 * p1.y - 3.0 * p2.y + p3.y) * t3);
            out.push(Point2::new(x, y));
        }
    }
    out.push(*pts.last().unwrap());
    out
}

// --- Modify -----------------------------------------------------------------

struct LengthenCmd;
impl Command for LengthenCmd {
    fn name(&self) -> &str {
        "LENGTHEN"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["LEN", "LE"]
    }
    fn help(&self) -> &str {
        "LENGTHEN id delta — extend/shorten a line or open polyline by delta at the end"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let id = parse_entity_id(args.first().ok_or("LENGTHEN: missing id")?, "LENGTHEN")?;
        let delta = parse_f64(args, 1, "LENGTHEN")?;
        let entity = ctx
            .doc
            .get(id)
            .ok_or_else(|| format!("LENGTHEN: no entity {}", id.0))?;
        let next = match &entity.geometry {
            Geometry::Line(s) => {
                let len = s.a.distance(s.b);
                if len < 1e-12 {
                    return Err("LENGTHEN: zero-length line".into());
                }
                let ux = (s.b.x - s.a.x) / len;
                let uy = (s.b.y - s.a.y) / len;
                let b = Point2::new(s.b.x + ux * delta, s.b.y + uy * delta);
                Geometry::Line(Segment::new(s.a, b))
            }
            Geometry::Polyline(p) if !p.closed && p.vertices.len() >= 2 => {
                let mut v = p.vertices.clone();
                let n = v.len();
                let a = v[n - 2];
                let b = v[n - 1];
                let len = a.distance(b);
                if len < 1e-12 {
                    return Err("LENGTHEN: zero-length end segment".into());
                }
                let ux = (b.x - a.x) / len;
                let uy = (b.y - a.y) / len;
                v[n - 1] = Point2::new(b.x + ux * delta, b.y + uy * delta);
                Geometry::Polyline(Polyline::new(v, false))
            }
            _ => {
                return Err(format!(
                    "LENGTHEN: entity {} must be a line or open polyline",
                    id.0
                ))
            }
        };
        ctx.doc.transact("LENGTHEN", |d| {
            d.replace_geometry(id, next);
        });
        Ok(format!("lengthened entity {} by {delta}", id.0))
    }
}

struct BreakCmd;
impl Command for BreakCmd {
    fn name(&self) -> &str {
        "BREAK"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["BR"]
    }
    fn help(&self) -> &str {
        "BREAK id x y [x2 y2] — break a line at one or two points"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let id = parse_entity_id(args.first().ok_or("BREAK: missing id")?, "BREAK")?;
        let px = parse_f64(args, 1, "BREAK")?;
        let py = parse_f64(args, 2, "BREAK")?;
        let p1 = Point2::new(px, py);
        let p2 = if args.len() >= 5 {
            Some(Point2::new(
                parse_f64(args, 3, "BREAK")?,
                parse_f64(args, 4, "BREAK")?,
            ))
        } else {
            None
        };
        let entity = ctx
            .doc
            .get(id)
            .ok_or_else(|| format!("BREAK: no entity {}", id.0))?
            .clone();
        let Geometry::Line(seg) = entity.geometry else {
            return Err(format!("BREAK: entity {} must be a line", id.0));
        };
        let layer = entity.layer;
        let a = project_on_segment(seg.a, seg.b, p1);
        let parts = if let Some(q) = p2 {
            let b = project_on_segment(seg.a, seg.b, q);
            let (lo, hi) = if param_on_seg(seg.a, seg.b, a) <= param_on_seg(seg.a, seg.b, b) {
                (a, b)
            } else {
                (b, a)
            };
            vec![
                Geometry::Line(Segment::new(seg.a, lo)),
                Geometry::Line(Segment::new(hi, seg.b)),
            ]
        } else {
            vec![
                Geometry::Line(Segment::new(seg.a, a)),
                Geometry::Line(Segment::new(a, seg.b)),
            ]
        };
        let n = parts.len();
        ctx.doc.transact("BREAK", |d| {
            d.remove_entity(id);
            for g in parts {
                d.add_geometry(layer.clone(), g);
            }
        });
        Ok(format!("broke entity {} into {n} segments", id.0))
    }
}

fn param_on_seg(a: Point2, b: Point2, p: Point2) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len2 = dx * dx + dy * dy;
    if len2 < 1e-18 {
        return 0.0;
    }
    ((p.x - a.x) * dx + (p.y - a.y) * dy) / len2
}

fn project_on_segment(a: Point2, b: Point2, p: Point2) -> Point2 {
    let t = param_on_seg(a, b, p).clamp(0.0, 1.0);
    Point2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}

struct JoinCmd;
impl Command for JoinCmd {
    fn name(&self) -> &str {
        "JOIN"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["J"]
    }
    fn help(&self) -> &str {
        "JOIN [SELECTION|id...] — join connected lines into a polyline"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let ids = if args.is_empty() {
            resolve_targets(ctx, "SELECTION", "JOIN")?
        } else if args.len() == 1 {
            resolve_targets(ctx, &args[0], "JOIN")?
        } else {
            args.iter()
                .map(|a| parse_entity_id(a, "JOIN"))
                .collect::<Result<Vec<_>, _>>()?
        };
        let mut segs: Vec<(EntityId, Segment, String)> = Vec::new();
        for id in &ids {
            let e = ctx
                .doc
                .get(*id)
                .ok_or_else(|| format!("JOIN: no entity {}", id.0))?;
            match &e.geometry {
                Geometry::Line(s) => segs.push((*id, *s, e.layer.clone())),
                _ => {
                    return Err(format!("JOIN: entity {} is not a line", id.0));
                }
            }
        }
        if segs.len() < 2 {
            return Err("JOIN: need at least two lines".into());
        }
        let layer = segs[0].2.clone();
        let tol = 1e-6;
        let mut verts = vec![segs[0].1.a, segs[0].1.b];
        let mut used = vec![false; segs.len()];
        used[0] = true;
        for _ in 1..segs.len() {
            let end = *verts.last().unwrap();
            let mut found = false;
            for (i, (_, s, _)) in segs.iter().enumerate() {
                if used[i] {
                    continue;
                }
                if s.a.distance(end) < tol {
                    verts.push(s.b);
                    used[i] = true;
                    found = true;
                    break;
                }
                if s.b.distance(end) < tol {
                    verts.push(s.a);
                    used[i] = true;
                    found = true;
                    break;
                }
            }
            if !found {
                return Err("JOIN: lines are not connected end-to-end".into());
            }
        }
        let remove_ids: Vec<_> = segs.iter().map(|(id, _, _)| *id).collect();
        let id = ctx.doc.transact("JOIN", |d| {
            for rid in &remove_ids {
                d.remove_entity(*rid);
            }
            d.add_geometry(layer, Geometry::Polyline(Polyline::new(verts, false)))
        });
        Ok(format!(
            "joined {} lines into polyline entity {}",
            remove_ids.len(),
            id.0
        ))
    }
}

struct MatchPropCmd;
impl Command for MatchPropCmd {
    fn name(&self) -> &str {
        "MATCHPROP"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["MA"]
    }
    fn help(&self) -> &str {
        "MATCHPROP source_id target — copy layer from source to target (SELECTION|ALL|id)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let src = parse_entity_id(
            args.first().ok_or("MATCHPROP: missing source")?,
            "MATCHPROP",
        )?;
        let token = args.get(1).ok_or("MATCHPROP: missing target")?;
        let layer = ctx
            .doc
            .get(src)
            .ok_or_else(|| format!("MATCHPROP: no source entity {}", src.0))?
            .layer
            .clone();
        let targets = resolve_targets(ctx, token, "MATCHPROP")?;
        let n = targets.len();
        ctx.doc.transact("MATCHPROP", |d| {
            for id in &targets {
                d.set_entity_layer(*id, layer.clone());
            }
        });
        Ok(format!(
            "matched layer {layer:?} onto {n} {}",
            entities_word(n)
        ))
    }
}

// --- Annotate ---------------------------------------------------------------

struct MtextCmd;
impl Command for MtextCmd {
    fn name(&self) -> &str {
        "MTEXT"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["MT"]
    }
    fn help(&self) -> &str {
        "MTEXT x y height line1 [| line2 ...] [layer] — multi-line text as stacked TEXT"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let x = parse_f64(args, 0, "MTEXT")?;
        let y = parse_f64(args, 1, "MTEXT")?;
        let height = parse_f64(args, 2, "MTEXT")?;
        if height <= 0.0 {
            return Err("MTEXT: height must be positive".into());
        }
        if args.len() < 4 {
            return Err("MTEXT: missing text".into());
        }
        let mut layer = "A-ANNO-TEXT".to_string();
        let mut parts: Vec<String> = Vec::new();
        for tok in &args[3..] {
            if tok.contains('|') {
                for p in tok.split('|') {
                    parts.push(p.trim().to_string());
                }
            } else if args.len() > 4
                && std::ptr::eq(tok, args.last().unwrap())
                && tok.parse::<f64>().is_err()
                && !tok.contains(' ')
            {
                // last non-pipe token may be layer if it looks like a name
                layer = tok.clone();
            } else {
                parts.push(tok.clone());
            }
        }
        // If we mistook text for layer (single word), keep it as text when only one part.
        if parts.is_empty() {
            layer = "A-ANNO-TEXT".into();
            parts.push(args[3].clone());
        }
        let n = parts.len();
        let ids = ctx.doc.transact("MTEXT", |d| {
            let mut ids = Vec::new();
            for (i, line) in parts.iter().enumerate() {
                let py = y - i as f64 * height * 1.25;
                ids.push(d.add_geometry(
                    layer.clone(),
                    Geometry::Text(Text::new(line.clone(), Point2::new(x, py), height, 0.0)),
                ));
            }
            ids
        });
        Ok(format!(
            "MTEXT added ({n} lines, first entity {})",
            ids[0].0
        ))
    }
}

struct DimStyleCmd;
impl Command for DimStyleCmd {
    fn name(&self) -> &str {
        "DIMSTYLE"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["D"]
    }
    fn help(&self) -> &str {
        "DIMSTYLE [name] [text_height] [scale] [secondary] [precision] [fraction] — set or report dimension style"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let mut props = ctx.doc.properties().clone();
        if args.is_empty() {
            return Ok(format!(
                "dimstyle={} text_height={} scale={} secondary={} precision={} fraction={}",
                if props.dimstyle.is_empty() {
                    "STANDARD"
                } else {
                    &props.dimstyle
                },
                if props.dim_text_height.is_empty() {
                    "2.5"
                } else {
                    &props.dim_text_height
                },
                if props.dim_scale.is_empty() {
                    "1"
                } else {
                    &props.dim_scale
                },
                if aadt_model::secondary_enabled(&props.dim_secondary) {
                    "on"
                } else {
                    "off"
                },
                if props.dim_precision.is_empty() {
                    "3"
                } else {
                    &props.dim_precision
                },
                if props.dim_fraction.is_empty() {
                    "decimal"
                } else {
                    &props.dim_fraction
                }
            ));
        }
        // Keyword form: PRECISION n · FRACTION 16|8|4|2|1|decimal · SECONDARY on|off
        if args
            .iter()
            .any(|a| matches!(a.to_ascii_lowercase().as_str(), "precision" | "fraction" | "secondary" | "alt"))
        {
            let mut i = 0;
            while i < args.len() {
                let tok = args[i].to_ascii_lowercase();
                match tok.as_str() {
                    "precision" | "prec" | "dec" => {
                        let p = args
                            .get(i + 1)
                            .ok_or("DIMSTYLE PRECISION needs 0..8")?;
                        let n: u8 = p
                            .parse()
                            .map_err(|_| "DIMSTYLE PRECISION needs 0..8")?;
                        if n > 8 {
                            return Err("DIMSTYLE PRECISION needs 0..8".into());
                        }
                        props.dim_precision = n.to_string();
                        i += 2;
                    }
                    "fraction" | "frac" => {
                        let f = args
                            .get(i + 1)
                            .ok_or("DIMSTYLE FRACTION needs decimal|1|2|4|8|16")?
                            .to_ascii_lowercase();
                        props.dim_fraction = match f.as_str() {
                            "0" | "dec" | "decimal" | "off" | "none" => "0".into(),
                            "1" | "2" | "4" | "8" | "16" => f,
                            _ => {
                                return Err(
                                    "DIMSTYLE FRACTION needs decimal|1|2|4|8|16".into(),
                                )
                            }
                        };
                        i += 2;
                    }
                    "secondary" | "alt" => {
                        let on = args
                            .get(i + 1)
                            .map(|s| {
                                matches!(
                                    s.to_ascii_lowercase().as_str(),
                                    "1" | "on" | "true" | "yes"
                                )
                            })
                            .unwrap_or(true);
                        props.dim_secondary = if on { "1".into() } else { "0".into() };
                        i += 2;
                    }
                    _ => {
                        // Allow leading style name before keywords.
                        if i == 0 {
                            props.dimstyle = args[0].clone();
                            i += 1;
                        } else {
                            return Err(format!("DIMSTYLE: unknown token {:?}", args[i]));
                        }
                    }
                }
            }
        } else {
            props.dimstyle = args[0].clone();
            if let Some(h) = args.get(1) {
                props.dim_text_height = h.clone();
            }
            if let Some(s) = args.get(2) {
                props.dim_scale = s.clone();
            }
            if let Some(alt) = args.get(3) {
                let on = matches!(
                    alt.to_ascii_lowercase().as_str(),
                    "1" | "on" | "true" | "yes" | "secondary"
                );
                props.dim_secondary = if on { "1".into() } else { "0".into() };
            }
            if let Some(p) = args.get(4) {
                let n: u8 = p
                    .parse()
                    .map_err(|_| "DIMSTYLE precision needs 0..8")?;
                if n > 8 {
                    return Err("DIMSTYLE precision needs 0..8".into());
                }
                props.dim_precision = n.to_string();
            }
            if let Some(f) = args.get(5) {
                let f = f.to_ascii_lowercase();
                props.dim_fraction = match f.as_str() {
                    "0" | "dec" | "decimal" | "off" | "none" => "0".into(),
                    "1" | "2" | "4" | "8" | "16" => f,
                    _ => {
                        return Err("DIMSTYLE fraction needs decimal|1|2|4|8|16".into())
                    }
                };
            }
        }
        ctx.doc.transact("DIMSTYLE", |d| {
            d.set_properties(props.clone());
        });
        Ok(format!(
            "dimstyle set to {} (text_height={}, scale={}, secondary={}, precision={}, fraction={})",
            props.dimstyle,
            if props.dim_text_height.is_empty() {
                "2.5"
            } else {
                &props.dim_text_height
            },
            if props.dim_scale.is_empty() {
                "1"
            } else {
                &props.dim_scale
            },
            if aadt_model::secondary_enabled(&props.dim_secondary) {
                "on"
            } else {
                "off"
            },
            if props.dim_precision.is_empty() {
                "3"
            } else {
                &props.dim_precision
            },
            if props.dim_fraction.is_empty() {
                "decimal"
            } else {
                &props.dim_fraction
            }
        ))
    }
}

// --- Layers -----------------------------------------------------------------

struct LayIsoCmd;
impl Command for LayIsoCmd {
    fn name(&self) -> &str {
        "LAYISO"
    }
    fn help(&self) -> &str {
        "LAYISO layer — isolate a layer (turn all others off); use LAYUNISO to restore"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let keep = args.first().ok_or("LAYISO: missing layer name")?.clone();
        if ctx.doc.layer(&keep).is_none() {
            return Err(format!("LAYISO: no layer {keep:?}"));
        }
        let snap: Vec<(String, bool, bool)> = ctx
            .doc
            .layers()
            .map(|l| (l.name.clone(), l.visible, l.frozen))
            .collect();
        ctx.layer_iso_snapshot = Some(snap.clone());
        let n = snap.len().saturating_sub(1);
        ctx.doc.transact("LAYISO", |d| {
            for (name, _, _) in &snap {
                if name != &keep {
                    d.set_layer_visible(name, false);
                } else {
                    d.set_layer_visible(name, true);
                }
            }
        });
        Ok(format!("isolated layer {keep:?} ({n} other layer(s) off)"))
    }
}

struct LayUnisoCmd;
impl Command for LayUnisoCmd {
    fn name(&self) -> &str {
        "LAYUNISO"
    }
    fn help(&self) -> &str {
        "LAYUNISO — restore layer visibility after LAYISO"
    }
    fn run(&self, ctx: &mut Context, _args: &[String]) -> Result<String, String> {
        let n = crate::autodim::restore_layer_iso(ctx).map_err(|e| format!("LAYUNISO: {e}"))?;
        Ok(format!("restored visibility for {n} layer(s)"))
    }
}

struct LayOffCmd;
impl Command for LayOffCmd {
    fn name(&self) -> &str {
        "LAYOFF"
    }
    fn help(&self) -> &str {
        "LAYOFF layer — turn a layer off"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let name = args.first().ok_or("LAYOFF: missing layer")?.clone();
        if !ctx
            .doc
            .transact("LAYOFF", |d| d.set_layer_visible(&name, false))
        {
            return Err(format!("LAYOFF: no layer {name:?}"));
        }
        Ok(format!("layer {name:?} off"))
    }
}

struct LayOnCmd;
impl Command for LayOnCmd {
    fn name(&self) -> &str {
        "LAYON"
    }
    fn help(&self) -> &str {
        "LAYON [layer] — turn a layer on, or all layers if omitted"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        if let Some(name) = args.first() {
            if !ctx
                .doc
                .transact("LAYON", |d| d.set_layer_visible(name, true))
            {
                return Err(format!("LAYON: no layer {name:?}"));
            }
            return Ok(format!("layer {name:?} on"));
        }
        let names: Vec<String> = ctx.doc.layers().map(|l| l.name.clone()).collect();
        let n = names.len();
        ctx.doc.transact("LAYON", |d| {
            for name in &names {
                d.set_layer_visible(name, true);
            }
        });
        Ok(format!("turned on {n} layer(s)"))
    }
}

// --- Blocks -----------------------------------------------------------------

fn selection_or_args(ctx: &Context, args: &[String], cmd: &str) -> Result<Vec<EntityId>, String> {
    if args.is_empty() {
        resolve_targets(ctx, "SELECTION", cmd)
    } else {
        args.iter()
            .map(|a| parse_entity_id(a, cmd))
            .collect::<Result<Vec<_>, _>>()
    }
}

struct BlockCmd;
impl Command for BlockCmd {
    fn name(&self) -> &str {
        "BLOCK"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["B"]
    }
    fn help(&self) -> &str {
        "BLOCK name bx by [id...] — define a block from SELECTION (or ids) at base point"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let name = args.first().ok_or("BLOCK: missing name")?.clone();
        let bx = parse_f64(args, 1, "BLOCK")?;
        let by = parse_f64(args, 2, "BLOCK")?;
        let ids = if args.len() > 3 {
            args[3..]
                .iter()
                .map(|a| parse_entity_id(a, "BLOCK"))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            resolve_targets(ctx, "SELECTION", "BLOCK")?
        };
        let base = Point2::new(bx, by);
        let mut block = Block::new(name.clone(), base);
        for id in &ids {
            let e = ctx
                .doc
                .get(*id)
                .ok_or_else(|| format!("BLOCK: no entity {}", id.0))?;
            let g = e.geometry.translated(-bx, -by);
            block.push(e.layer.clone(), g);
        }
        let n = block.entities.len();
        ctx.doc.transact("BLOCK", |d| {
            d.add_block(block);
        });
        Ok(format!("block {name:?} defined with {n} entities"))
    }
}

struct WblockCmd;
impl Command for WblockCmd {
    fn name(&self) -> &str {
        "WBLOCK"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["W"]
    }
    fn help(&self) -> &str {
        "WBLOCK name bx by [id...] — write selection to a block definition and erase sources"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let name = args.first().ok_or("WBLOCK: missing name")?.clone();
        let bx = parse_f64(args, 1, "WBLOCK")?;
        let by = parse_f64(args, 2, "WBLOCK")?;
        let ids = if args.len() > 3 {
            args[3..]
                .iter()
                .map(|a| parse_entity_id(a, "WBLOCK"))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            resolve_targets(ctx, "SELECTION", "WBLOCK")?
        };
        let base = Point2::new(bx, by);
        let mut block = Block::new(name.clone(), base);
        for id in &ids {
            let e = ctx
                .doc
                .get(*id)
                .ok_or_else(|| format!("WBLOCK: no entity {}", id.0))?;
            block.push(e.layer.clone(), e.geometry.translated(-bx, -by));
        }
        let n = block.entities.len();
        let remove = ids.clone();
        ctx.doc.transact("WBLOCK", |d| {
            d.add_block(block);
            for id in &remove {
                d.remove_entity(*id);
            }
        });
        ctx.selection.clear();
        Ok(format!(
            "WBLOCK {name:?}: wrote {n} entities to block table and erased sources (SAVE drawing to persist)"
        ))
    }
}

struct EattEditCmd;
impl Command for EattEditCmd {
    fn name(&self) -> &str {
        "EATTEDIT"
    }
    fn help(&self) -> &str {
        "EATTEDIT id key value — set an attribute on an INSERT (or any entity)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let id = parse_entity_id(args.first().ok_or("EATTEDIT: missing id")?, "EATTEDIT")?;
        let key = args.get(1).ok_or("EATTEDIT: missing key")?.clone();
        let value = args.get(2..).unwrap_or(&[]).join(" ");
        if value.is_empty() {
            return Err("EATTEDIT: missing value".into());
        }
        if ctx.doc.get(id).is_none() {
            return Err(format!("EATTEDIT: no entity {}", id.0));
        }
        ctx.doc.transact("EATTEDIT", |d| {
            d.set_attribute(id, &key, value.clone());
        });
        Ok(format!("set attribute {key}={value:?} on entity {}", id.0))
    }
}

/// Per-selection metadata for AQC Estimation / BBS takeoff (see docs/AQC-META-TAGS.md).
struct ObjMetaCmd;
impl ObjMetaCmd {
    const KEYS: &'static [&'static str] = &[
        "MARK",
        "WALL_NAME",
        "NOS",
        "AQC_CODE",
        "COLUMN_NO",
        "CODE",
        "TRADE",
        "LEVEL",
        "ELEMENT",
        "MATERIAL",
        "FIRERATING",
        "PHASE",
        "IFC",
        "UNIFORMAT",
        "MASTERFORMAT",
    ];
}
impl Command for ObjMetaCmd {
    fn name(&self) -> &str {
        "OBJMETA"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["OBJECTMETA", "META", "AQCMETA"]
    }
    fn help(&self) -> &str {
        "OBJMETA [KEY VALUE] — report or set AQC/BBS tags on the SELECTION \
         (MARK / WALL_NAME / NOS / AQC_CODE / COLUMN_NO / CODE / TRADE / LEVEL / ELEMENT / …)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        if ctx.selection.is_empty() {
            return Err("OBJMETA: no selection — SELECT objects first".into());
        }
        let ids = ctx.selection.clone();
        if args.is_empty() {
            let mut lines = Vec::new();
            for id in &ids {
                let Some(e) = ctx.doc.get(*id) else { continue };
                let mut parts = vec![format!("#{} {}", id.0, e.layer)];
                for key in Self::KEYS {
                    if let Some(v) = e.attribute(key) {
                        if !v.is_empty() {
                            parts.push(format!("{key}={v}"));
                        }
                    }
                }
                lines.push(parts.join("  ·  "));
            }
            return Ok(lines.join("\n"));
        }
        if args.len() < 2 {
            return Err("OBJMETA: need KEY and VALUE to set (or no args to report)".into());
        }
        let key = args[0].to_ascii_uppercase();
        if !Self::KEYS.contains(&key.as_str()) {
            return Err(format!(
                "OBJMETA: unknown key {key:?}; use one of {}",
                Self::KEYS.join(" / ")
            ));
        }
        let value = args[1..].join(" ");
        if key == "PHASE"
            && !matches!(
                value.to_ascii_lowercase().as_str(),
                "new" | "existing" | "demolish"
            )
        {
            return Err("OBJMETA: PHASE must be New, Existing, or Demolish".into());
        }
        let n = ctx.doc.transact("OBJMETA", |d| {
            let mut n = 0;
            for id in &ids {
                if d.set_attribute(*id, &key, value.clone()) {
                    n += 1;
                }
            }
            n
        });
        Ok(format!("set {key}={value:?} on {n} object(s)"))
    }
}

// --- Selection --------------------------------------------------------------

struct QSelectCmd;
impl Command for QSelectCmd {
    fn name(&self) -> &str {
        "QSELECT"
    }
    fn help(&self) -> &str {
        "QSELECT LAYER name | TYPE LINE|POLYLINE|CIRCLE|ARC|TEXT|INSERT — quick select"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let kind = args
            .first()
            .ok_or("QSELECT: missing LAYER or TYPE")?
            .to_ascii_uppercase();
        let ids: Vec<EntityId> = match kind.as_str() {
            "LAYER" => {
                let name = args.get(1).ok_or("QSELECT LAYER: missing name")?;
                ctx.doc
                    .entities()
                    .iter()
                    .filter(|e| &e.layer == name)
                    .map(|e| e.id)
                    .collect()
            }
            "TYPE" => {
                let ty = args
                    .get(1)
                    .ok_or("QSELECT TYPE: missing type")?
                    .to_ascii_uppercase();
                ctx.doc
                    .entities()
                    .iter()
                    .filter(|e| {
                        matches!(
                            (&ty[..], &e.geometry),
                            ("LINE", Geometry::Line(_))
                                | ("POLYLINE" | "PLINE", Geometry::Polyline(_))
                                | ("CIRCLE", Geometry::Circle(_))
                                | ("ARC", Geometry::Arc(_))
                                | ("TEXT", Geometry::Text(_))
                                | ("INSERT", Geometry::Insert(_))
                        )
                    })
                    .map(|e| e.id)
                    .collect()
            }
            _ => {
                return Err("QSELECT: use LAYER <name> or TYPE <LINE|…>".into());
            }
        };
        let mut ids = ids;
        ids.sort();
        ctx.prev_selection = std::mem::take(&mut ctx.selection);
        let n = ids.len();
        ctx.selection = ids;
        Ok(format!("qselect: {n} {}", entities_word(n)))
    }
}

struct PSelectCmd;
impl Command for PSelectCmd {
    fn name(&self) -> &str {
        "PSELECT"
    }
    fn help(&self) -> &str {
        "PSELECT — restore the previous selection"
    }
    fn run(&self, ctx: &mut Context, _args: &[String]) -> Result<String, String> {
        let prev = std::mem::take(&mut ctx.prev_selection);
        if prev.is_empty() {
            return Err("PSELECT: no previous selection".into());
        }
        ctx.prev_selection = std::mem::take(&mut ctx.selection);
        let exists = |id: &EntityId| ctx.doc.get(*id).is_some();
        ctx.selection = prev.into_iter().filter(|id| exists(id)).collect();
        let n = ctx.selection.len();
        Ok(format!("previous selection: {n} {}", entities_word(n)))
    }
}

// --- Info -------------------------------------------------------------------

struct IdCmd;
impl Command for IdCmd {
    fn name(&self) -> &str {
        "ID"
    }
    fn help(&self) -> &str {
        "ID x y — report point coordinates"
    }
    fn run(&self, _ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let x = parse_f64(args, 0, "ID")?;
        let y = parse_f64(args, 1, "ID")?;
        Ok(format!("X = {x}  Y = {y}"))
    }
}

struct MeasureCmd;
impl Command for MeasureCmd {
    fn name(&self) -> &str {
        "MEASURE"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["MEA"]
    }
    fn help(&self) -> &str {
        "MEASURE id [segment] — report length of a line/polyline (or one segment index)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let id = parse_entity_id(args.first().ok_or("MEASURE: missing id")?, "MEASURE")?;
        let e = ctx
            .doc
            .get(id)
            .ok_or_else(|| format!("MEASURE: no entity {}", id.0))?;
        match &e.geometry {
            Geometry::Line(s) => Ok(format!("length {}", s.a.distance(s.b))),
            Geometry::Polyline(p) => {
                if let Some(raw) = args.get(1) {
                    let i: usize = raw
                        .parse()
                        .map_err(|_| "MEASURE: segment index must be an integer")?;
                    if i + 1 >= p.vertices.len() {
                        return Err("MEASURE: segment index out of range".into());
                    }
                    let d = p.vertices[i].distance(p.vertices[i + 1]);
                    Ok(format!("segment {i} length {d}"))
                } else {
                    let mut total = 0.0;
                    for w in p.vertices.windows(2) {
                        total += w[0].distance(w[1]);
                    }
                    if p.closed && p.vertices.len() > 2 {
                        total += p.vertices.last().unwrap().distance(p.vertices[0]);
                    }
                    Ok(format!("length {total}"))
                }
            }
            Geometry::Circle(c) => Ok(format!("circumference {}", TAU * c.radius)),
            Geometry::Arc(a) => Ok(format!("arc length {}", a.length())),
            _ => Err(format!("MEASURE: entity {} has no measurable length", id.0)),
        }
    }
}

// --- Nav --------------------------------------------------------------------

struct ZoomCmd;
impl Command for ZoomCmd {
    fn name(&self) -> &str {
        "ZOOM"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["Z"]
    }
    fn help(&self) -> &str {
        "ZOOM A|ALL | E|EXTENTS | W x1 y1 x2 y2 | S factor | P — viewport zoom (GUI: Z then A for All)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let sub = args
            .first()
            .map(|s| s.to_ascii_uppercase())
            .unwrap_or_else(|| "E".into());
        let hint = match sub.as_str() {
            "E" | "EXTENTS" | "ALL" => "extents".to_string(),
            "P" | "PREVIOUS" => "previous".to_string(),
            "S" | "SCALE" => {
                let f = parse_f64(args, 1, "ZOOM")?;
                format!("scale:{f}")
            }
            "W" | "WINDOW" => {
                let x1 = parse_f64(args, 1, "ZOOM")?;
                let y1 = parse_f64(args, 2, "ZOOM")?;
                let x2 = parse_f64(args, 3, "ZOOM")?;
                let y2 = parse_f64(args, 4, "ZOOM")?;
                format!("window:{x1},{y1},{x2},{y2}")
            }
            _ => return Err("ZOOM: use E, W x1 y1 x2 y2, S factor, or P".into()),
        };
        ctx.nav_hint = Some(hint.clone());
        Ok(format!("zoom {hint}"))
    }
}

struct PanCmd;
impl Command for PanCmd {
    fn name(&self) -> &str {
        "PAN"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["P"]
    }
    fn help(&self) -> &str {
        "PAN [dx dy] — pan the viewport (GUI: middle-drag; command sets a hint)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        if args.len() >= 2 {
            let dx = parse_f64(args, 0, "PAN")?;
            let dy = parse_f64(args, 1, "PAN")?;
            ctx.nav_hint = Some(format!("pan:{dx},{dy}"));
            Ok(format!("pan by ({dx}, {dy})"))
        } else {
            ctx.nav_hint = Some("pan".into());
            Ok("pan — drag with the middle mouse button in the viewport".into())
        }
    }
}

struct RegenCmd;
impl Command for RegenCmd {
    fn name(&self) -> &str {
        "REGEN"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["RE"]
    }
    fn help(&self) -> &str {
        "REGEN — regenerate the display (drawlist rebuilds automatically)"
    }
    fn run(&self, ctx: &mut Context, _args: &[String]) -> Result<String, String> {
        // Bump revision so GUI caches refresh.
        let mut props = ctx.doc.properties().clone();
        // no-op touch via set_properties only if we change something — instead
        // rely on message; GUI always rebuilds drawlist from document.
        let _ = &mut props;
        Ok(format!("regenerated ({} entities)", ctx.doc.len()))
    }
}

// --- Clipboard --------------------------------------------------------------

fn clip_payload(ctx: &Context, ids: &[EntityId]) -> Result<Vec<Entity>, String> {
    let mut out = Vec::new();
    for id in ids {
        let e = ctx
            .doc
            .get(*id)
            .ok_or_else(|| format!("clipboard: no entity {}", id.0))?;
        out.push(e.clone());
    }
    Ok(out)
}

struct CopyClipCmd;
impl Command for CopyClipCmd {
    fn name(&self) -> &str {
        "COPYCLIP"
    }
    fn help(&self) -> &str {
        "COPYCLIP [SELECTION|id...] — copy entities to the internal clipboard"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let ids = selection_or_args(ctx, args, "COPYCLIP")?;
        ctx.clipboard = clip_payload(ctx, &ids)?;
        Ok(format!(
            "copied {} {} to clipboard",
            ctx.clipboard.len(),
            entities_word(ctx.clipboard.len())
        ))
    }
}

struct CutClipCmd;
impl Command for CutClipCmd {
    fn name(&self) -> &str {
        "CUTCLIP"
    }
    fn help(&self) -> &str {
        "CUTCLIP [SELECTION|id...] — cut entities to the internal clipboard"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let ids = selection_or_args(ctx, args, "CUTCLIP")?;
        ctx.clipboard = clip_payload(ctx, &ids)?;
        let n = ids.len();
        ctx.doc.transact("CUTCLIP", |d| {
            for id in &ids {
                d.remove_entity(*id);
            }
        });
        ctx.selection.clear();
        Ok(format!("cut {n} {} to clipboard", entities_word(n)))
    }
}

struct PasteClipCmd;
impl Command for PasteClipCmd {
    fn name(&self) -> &str {
        "PASTECLIP"
    }
    fn help(&self) -> &str {
        "PASTECLIP [dx dy] — paste clipboard entities, optionally translated"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        if ctx.clipboard.is_empty() {
            return Err("PASTECLIP: clipboard is empty".into());
        }
        let dx = if args.is_empty() {
            0.0
        } else {
            parse_f64(args, 0, "PASTECLIP")?
        };
        let dy = if args.len() < 2 {
            0.0
        } else {
            parse_f64(args, 1, "PASTECLIP")?
        };
        let payload = ctx.clipboard.clone();
        let n = payload.len();
        let mut new_ids = Vec::new();
        ctx.doc.transact("PASTECLIP", |d| {
            for e in &payload {
                let g = e.geometry.translated(dx, dy);
                new_ids.push(d.add_entity(e.layer.clone(), g, e.attributes.clone()));
            }
        });
        ctx.prev_selection = std::mem::take(&mut ctx.selection);
        ctx.selection = new_ids;
        Ok(format!("pasted {n} {}", entities_word(n)))
    }
}

// --- Props ------------------------------------------------------------------

struct PropertiesCmd;
impl Command for PropertiesCmd {
    fn name(&self) -> &str {
        "PROPERTIES"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["PR"]
    }
    fn help(&self) -> &str {
        "PROPERTIES [id|SELECTION] — list entity or drawing properties"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        if args.is_empty() {
            let p = ctx.doc.properties();
            return Ok(format!(
                "drawing: title={:?} project={:?} units={:?} entities={}",
                p.title,
                p.project,
                p.units,
                ctx.doc.len()
            ));
        }
        let ids = if args[0].eq_ignore_ascii_case("SELECTION") {
            resolve_targets(ctx, "SELECTION", "PROPERTIES")?
        } else {
            vec![parse_entity_id(&args[0], "PROPERTIES")?]
        };
        let mut lines = Vec::new();
        for id in ids {
            let e = ctx
                .doc
                .get(id)
                .ok_or_else(|| format!("PROPERTIES: no entity {}", id.0))?;
            let kind = match &e.geometry {
                Geometry::Line(_) => "LINE",
                Geometry::Polyline(_) => "POLYLINE",
                Geometry::Circle(_) => "CIRCLE",
                Geometry::Arc(_) => "ARC",
                Geometry::Text(_) => "TEXT",
                Geometry::Insert(_) => "INSERT",
                Geometry::Image(_) => "IMAGE",
            };
            lines.push(format!(
                "entity {}  type={kind}  layer={}  attrs={}",
                id.0,
                e.layer,
                e.attributes.len()
            ));
        }
        Ok(lines.join("\n"))
    }
}

struct ChangeCmd;
impl Command for ChangeCmd {
    fn name(&self) -> &str {
        "CHANGE"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["CH"]
    }
    fn help(&self) -> &str {
        "CHANGE target LAYER name — change properties of SELECTION|ALL|id"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let token = args.first().ok_or("CHANGE: missing target")?;
        let ids = resolve_targets(ctx, token, "CHANGE")?;
        let mut it = args[1..].iter();
        let mut layer: Option<String> = None;
        while let Some(tok) = it.next() {
            match tok.to_ascii_uppercase().as_str() {
                "LAYER" | "LA" => {
                    layer = Some(it.next().ok_or("CHANGE: LAYER needs a name")?.clone());
                }
                other => {
                    return Err(format!("CHANGE: unknown option {other:?} (try LAYER)"));
                }
            }
        }
        let Some(layer) = layer else {
            return Err("CHANGE: specify LAYER <name>".into());
        };
        let n = ids.len();
        ctx.doc.transact("CHANGE", |d| {
            for id in &ids {
                d.set_entity_layer(*id, layer.clone());
            }
        });
        Ok(format!(
            "changed {n} {} to layer {layer:?}",
            entities_word(n)
        ))
    }
}

// --- Utility ----------------------------------------------------------------

struct UnitsCmd;
impl Command for UnitsCmd {
    fn name(&self) -> &str {
        "UNITS"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["UN"]
    }
    fn help(&self) -> &str {
        "UNITS [ft-in|m] [SECONDARY on|off] [PRECISION 0..8] [FRACTION decimal|1|2|4|8|16] — set or report linear units"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let mut props = ctx.doc.properties().clone();
        if args.is_empty() {
            let primary = aadt_model::primary_from_props(&props.units);
            let sec = aadt_model::secondary_enabled(&props.dim_secondary);
            return Ok(format!(
                "units={} ({}) secondary={} precision={} fraction={} dimstyle={}",
                primary.as_token(),
                primary.label(),
                if sec { "on" } else { "off" },
                if props.dim_precision.is_empty() {
                    "3"
                } else {
                    &props.dim_precision
                },
                if props.dim_fraction.is_empty() {
                    "decimal"
                } else {
                    &props.dim_fraction
                },
                if props.dimstyle.is_empty() {
                    primary.dimstyle_name()
                } else {
                    &props.dimstyle
                }
            ));
        }
        let mut i = 0;
        while i < args.len() {
            let tok = args[i].to_ascii_lowercase();
            if tok == "secondary" || tok == "alt" || tok == "dimalt" {
                let on = args
                    .get(i + 1)
                    .map(|s| matches!(s.to_ascii_lowercase().as_str(), "1" | "on" | "true" | "yes"))
                    .unwrap_or(true);
                props.dim_secondary = if on { "1".into() } else { "0".into() };
                i += 2;
                continue;
            }
            if tok == "precision" || tok == "prec" {
                let p = args
                    .get(i + 1)
                    .ok_or("UNITS PRECISION needs 0..8")?;
                let n: u8 = p.parse().map_err(|_| "UNITS PRECISION needs 0..8")?;
                if n > 8 {
                    return Err("UNITS PRECISION needs 0..8".into());
                }
                props.dim_precision = n.to_string();
                i += 2;
                continue;
            }
            if tok == "fraction" || tok == "frac" {
                let f = args
                    .get(i + 1)
                    .ok_or("UNITS FRACTION needs decimal|1|2|4|8|16")?
                    .to_ascii_lowercase();
                props.dim_fraction = match f.as_str() {
                    "0" | "dec" | "decimal" | "off" | "none" => "0".into(),
                    "1" | "2" | "4" | "8" | "16" => f,
                    _ => return Err("UNITS FRACTION needs decimal|1|2|4|8|16".into()),
                };
                i += 2;
                continue;
            }
            let sys = aadt_model::UnitSystem::parse(&args[i])
                .ok_or_else(|| format!("UNITS: unknown unit {:?} (use ft-in or m)", args[i]))?;
            props.units = sys.as_token().to_string();
            // Units dictate dimension style name when unset or previously auto.
            if props.dimstyle.is_empty()
                || props.dimstyle.eq_ignore_ascii_case("STANDARD")
                || props.dimstyle.eq_ignore_ascii_case("ARCHITECTURAL")
                || props.dimstyle.eq_ignore_ascii_case("METRIC")
            {
                props.dimstyle = sys.dimstyle_name().to_string();
            }
            i += 1;
        }
        ctx.doc.transact("UNITS", |d| {
            d.set_properties(props.clone());
        });
        let primary = aadt_model::primary_from_props(&props.units);
        Ok(format!(
            "units set to {} · dimstyle={} · secondary={} · precision={} · fraction={}",
            primary.as_token(),
            props.dimstyle,
            if aadt_model::secondary_enabled(&props.dim_secondary) {
                "on"
            } else {
                "off"
            },
            if props.dim_precision.is_empty() {
                "3"
            } else {
                &props.dim_precision
            },
            if props.dim_fraction.is_empty() {
                "decimal"
            } else {
                &props.dim_fraction
            }
        ))
    }
}

struct TemplateCmd;
impl Command for TemplateCmd {
    fn name(&self) -> &str {
        "TEMPLATE"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["TMPL"]
    }
    fn help(&self) -> &str {
        "TEMPLATE LAYERS | FROMBLOCK <name> — nameplate template layers / register block"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let sub = args
            .first()
            .map(|s| s.to_ascii_uppercase())
            .unwrap_or_default();
        match sub.as_str() {
            "LAYERS" | "LAYER" => {
                const LAYERS: &[(&str, u8, f32)] = &[
                    ("TMPL_BORDER", 250, 0.70),
                    ("TMPL_VIEW", 8, 0.35),
                    ("TMPL_TEXT", 7, 0.25),
                ];
                let mut added = 0usize;
                ctx.doc.transact("TEMPLATE LAYERS", |d| {
                    for &(name, color, lw) in LAYERS {
                        if d.layer(name).is_none() {
                            d.add_layer(Layer::new(name).with_color(color).with_lineweight_mm(lw));
                            added += 1;
                        }
                    }
                });
                Ok(format!(
                    "template layers ready (added {added}: TMPL_BORDER, TMPL_VIEW, TMPL_TEXT)"
                ))
            }
            "FROMBLOCK" | "BLOCK" => {
                let name = args
                    .get(1)
                    .cloned()
                    .ok_or("TEMPLATE FROMBLOCK: missing block name")?;
                if ctx.doc.block(&name).is_none() {
                    return Err(format!("TEMPLATE FROMBLOCK: no block named {name:?}"));
                }
                let mut props = ctx.doc.properties().clone();
                props.nameplate_block = name.clone();
                ctx.doc.set_properties(props);
                Ok(format!("nameplate template block set to {name}"))
            }
            _ => Err("TEMPLATE: use LAYERS or FROMBLOCK <name>".into()),
        }
    }
}

struct AuditCmd;
impl Command for AuditCmd {
    fn name(&self) -> &str {
        "AUDIT"
    }
    fn help(&self) -> &str {
        "AUDIT — report orphan layers and missing block references"
    }
    fn run(&self, ctx: &mut Context, _args: &[String]) -> Result<String, String> {
        let used_layers: std::collections::HashSet<String> =
            ctx.doc.entities().iter().map(|e| e.layer.clone()).collect();
        let orphan_layers: Vec<_> = ctx
            .doc
            .layers()
            .filter(|l| !used_layers.contains(&l.name) && l.name != "0")
            .map(|l| l.name.clone())
            .collect();
        let mut missing_blocks = Vec::new();
        for e in ctx.doc.entities() {
            if let Geometry::Insert(Insert { name, .. }) = &e.geometry {
                if ctx.doc.block(name).is_none() {
                    missing_blocks.push(format!("entity {} → block {name:?}", e.id.0));
                }
            }
        }
        let mut lines = vec![format!(
            "audit: {} entities, {} layers, {} block(s)",
            ctx.doc.len(),
            ctx.doc.layers().count(),
            ctx.doc.blocks().len()
        )];
        if orphan_layers.is_empty() && missing_blocks.is_empty() {
            lines.push("no issues found".into());
        } else {
            if !orphan_layers.is_empty() {
                lines.push(format!("unused layers: {}", orphan_layers.join(", ")));
            }
            for m in missing_blocks {
                lines.push(format!("missing block ref: {m}"));
            }
        }
        Ok(lines.join("\n"))
    }
}

struct OverkillCmd;
impl Command for OverkillCmd {
    fn name(&self) -> &str {
        "OVERKILL"
    }
    fn help(&self) -> &str {
        "OVERKILL — delete duplicate/coincident entities (LINE, POLYLINE, CIRCLE, ARC, TEXT, INSERT)"
    }
    fn run(&self, ctx: &mut Context, _args: &[String]) -> Result<String, String> {
        // Quantize to 1e-6 so import/export float noise doesn't hide a real
        // duplicate. Per geometry kind, per layer (two coincident entities on
        // different layers are a legitimate overlay, not a duplicate) — and, for
        // LINE/POLYLINE, order-independent, since a copy walked or drawn in the
        // opposite direction is still the same duplicate line.
        let quant = |v: f64| (v * 1e6).round() as i64;
        let pt = |p: Point2| format!("{},{}", quant(p.x), quant(p.y));
        let dup_key = |e: &Entity| -> String {
            let shape = match &e.geometry {
                Geometry::Line(s) => {
                    let (a, b) = if (s.a.x, s.a.y) <= (s.b.x, s.b.y) {
                        (s.a, s.b)
                    } else {
                        (s.b, s.a)
                    };
                    format!("LINE|{}|{}", pt(a), pt(b))
                }
                Geometry::Polyline(p) => {
                    let fwd: Vec<String> = p.vertices.iter().map(|&v| pt(v)).collect();
                    let rev: Vec<String> = fwd.iter().rev().cloned().collect();
                    let canon = if fwd <= rev { &fwd } else { &rev };
                    format!("POLYLINE|{}|{}", p.closed, canon.join(";"))
                }
                Geometry::Circle(c) => format!("CIRCLE|{}|{}", pt(c.center), quant(c.radius)),
                Geometry::Arc(a) => format!(
                    "ARC|{}|{}|{}|{}",
                    pt(a.center),
                    quant(a.radius),
                    quant(a.start_angle),
                    quant(a.end_angle)
                ),
                Geometry::Text(t) => format!(
                    "TEXT|{}|{}|{}|{}",
                    t.value,
                    pt(t.position),
                    quant(t.height),
                    quant(t.rotation)
                ),
                Geometry::Insert(i) => format!(
                    "INSERT|{}|{}|{}|{}|{}",
                    i.name,
                    pt(i.position),
                    quant(i.rotation),
                    quant(i.scale_x),
                    quant(i.scale_y)
                ),
                Geometry::Image(img) => format!(
                    "IMAGE|{}|{}|{}|{}|{}|{}",
                    img.path,
                    pt(img.position),
                    quant(img.rotation),
                    quant(img.width),
                    quant(img.height),
                    quant(img.opacity as f64)
                ),
            };
            format!("{}|{shape}", e.layer)
        };
        let mut seen: HashMap<String, EntityId> = HashMap::new();
        let mut kill = Vec::new();
        for e in ctx.doc.entities() {
            match seen.entry(dup_key(e)) {
                std::collections::hash_map::Entry::Vacant(v) => {
                    v.insert(e.id);
                }
                std::collections::hash_map::Entry::Occupied(_) => kill.push(e.id),
            }
        }
        let n = kill.len();
        if n == 0 {
            return Ok("overkill: no duplicates".into());
        }
        ctx.doc.transact("OVERKILL", |d| {
            for id in &kill {
                d.remove_entity(*id);
            }
        });
        Ok(format!(
            "overkill: removed {n} duplicate {}",
            entities_word(n)
        ))
    }
}

// --- Image / library --------------------------------------------------------

struct ImageCmd;
impl Command for ImageCmd {
    fn name(&self) -> &str {
        "IMAGE"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["PLACEIMAGE", "IMAGEATTACH"]
    }
    fn help(&self) -> &str {
        "IMAGE path x y [width] [height] [layer] — place Geometry::Image at a point"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        // Path may contain spaces: find the first float pair (x y).
        let mut xi = None;
        for (i, a) in args.iter().enumerate() {
            if a.parse::<f64>().is_ok()
                && args
                    .get(i + 1)
                    .is_some_and(|b| b.parse::<f64>().is_ok())
            {
                xi = Some(i);
                break;
            }
        }
        let xi = xi.ok_or("IMAGE: missing path x y")?;
        if xi == 0 {
            return Err("IMAGE: missing path".into());
        }
        let path = args[..xi].join(" ");
        let x: f64 = args[xi]
            .parse()
            .map_err(|_| format!("IMAGE: x {:?} is not a number", args[xi]))?;
        let y: f64 = args[xi + 1]
            .parse()
            .map_err(|_| format!("IMAGE: y {:?} is not a number", args[xi + 1]))?;
        let mut rest = &args[xi + 2..];
        let width = rest
            .first()
            .and_then(|s| s.parse::<f64>().ok())
            .filter(|w| *w > 0.0)
            .unwrap_or(1.0);
        if rest.first().is_some_and(|s| s.parse::<f64>().is_ok()) {
            rest = &rest[1..];
        }
        let height = rest
            .first()
            .and_then(|s| s.parse::<f64>().ok())
            .filter(|h| *h > 0.0)
            .unwrap_or(width);
        if rest.first().is_some_and(|s| s.parse::<f64>().is_ok()) {
            rest = &rest[1..];
        }
        let layer = rest
            .first()
            .cloned()
            .unwrap_or_else(|| "A-FURN-PRES".into());
        let img = ImageRef::new(path, Point2::new(x, y), width, height);
        let id = ctx.doc.transact("IMAGE", |d| {
            d.add_geometry(layer, Geometry::Image(img))
        });
        Ok(format!("IMAGE placed as entity {}", id.0))
    }
}

struct BlockLibCmd;
impl Command for BlockLibCmd {
    fn name(&self) -> &str {
        "BLOCKLIB"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["BLIB"]
    }
    fn help(&self) -> &str {
        "BLOCKLIB [list|import path [name]|load-all] — DXF/SVG library (opensource inches→m)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let root = aadt_library::default_library_root();
        match args.first().map(|s| s.to_ascii_lowercase()).as_deref() {
            None | Some("list") => {
                let blocks = aadt_library::list_blocks(&root).map_err(|e| e.to_string())?;
                if blocks.is_empty() {
                    return Ok(format!("no blocks in {}", root.join("blocks").display()));
                }
                let lines: Vec<_> = blocks
                    .iter()
                    .map(|b| format!("  {} ({})", b.name, b.path.display()))
                    .collect();
                Ok(format!("{} block(s):\n{}", blocks.len(), lines.join("\n")))
            }
            Some("import") => {
                let path = args.get(1).ok_or("BLOCKLIB import: missing path")?;
                let name = args.get(2).map(String::as_str);
                let imported = ctx.doc.transact("BLOCKLIB", |d| {
                    aadt_library::import_block(d, Path::new(path), name).map_err(|e| e.to_string())
                })?;
                Ok(format!("imported block {imported:?}"))
            }
            Some("load-all") | Some("loadall") | Some("import-all") => {
                let names = ctx.doc.transact("BLOCKLIB", |d| {
                    aadt_library::import_all_blocks(d, &root).map_err(|e| e.to_string())
                })?;
                Ok(format!(
                    "loaded {} library block(s) (opensource scaled inches→metres)",
                    names.len()
                ))
            }
            Some(other) => Err(format!(
                "BLOCKLIB: unknown subcommand {other:?}; try list, import, or load-all"
            )),
        }
    }
}

// --- Stubs ------------------------------------------------------------------

struct StubCmd {
    name: &'static str,
    aliases: &'static [&'static str],
    help: &'static str,
}

impl Command for StubCmd {
    fn name(&self) -> &str {
        self.name
    }
    fn aliases(&self) -> &'static [&'static str] {
        self.aliases
    }
    fn help(&self) -> &str {
        self.help
    }
    fn run(&self, _ctx: &mut Context, _args: &[String]) -> Result<String, String> {
        Err(format!("{}: not implemented in this milestone", self.name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Interpreter;

    fn interp() -> Interpreter {
        Interpreter::with_builtins()
    }

    #[test]
    fn polygon_and_aliases() {
        let i = interp();
        let mut ctx = Context::new();
        i.eval_line(&mut ctx, "POL 4 0 0 10").unwrap();
        assert_eq!(ctx.doc.len(), 1);
        assert!(matches!(
            ctx.doc.entities()[0].geometry,
            Geometry::Polyline(ref p) if p.closed && p.vertices.len() == 4
        ));
    }

    #[test]
    fn layiso_and_uniso() {
        let i = interp();
        let mut ctx = Context::new();
        i.eval_line(&mut ctx, "LAYER A").unwrap();
        i.eval_line(&mut ctx, "LAYER B").unwrap();
        i.eval_line(&mut ctx, "LINE 0 0 1 0 A").unwrap();
        i.eval_line(&mut ctx, "LAYISO A").unwrap();
        assert!(!ctx.doc.layer("B").unwrap().visible);
        i.eval_line(&mut ctx, "LAYUNISO").unwrap();
        assert!(ctx.doc.layer("B").unwrap().visible);
    }

    #[test]
    fn matchprop_changes_layer() {
        let i = interp();
        let mut ctx = Context::new();
        i.eval_line(&mut ctx, "LINE 0 0 1 0 A").unwrap();
        i.eval_line(&mut ctx, "LINE 0 1 1 1 B").unwrap();
        i.eval_line(&mut ctx, "MATCHPROP 0 1").unwrap();
        assert_eq!(ctx.doc.get(EntityId(1)).unwrap().layer, "A");
    }

    #[test]
    fn join_two_lines() {
        let i = interp();
        let mut ctx = Context::new();
        i.eval_line(&mut ctx, "LINE 0 0 1 0").unwrap();
        i.eval_line(&mut ctx, "LINE 1 0 2 0").unwrap();
        i.eval_line(&mut ctx, "JOIN 0 1").unwrap();
        assert_eq!(ctx.doc.len(), 1);
        assert!(matches!(
            ctx.doc.entities()[0].geometry,
            Geometry::Polyline(_)
        ));
    }

    #[test]
    fn overkill_removes_dup_line() {
        let i = interp();
        let mut ctx = Context::new();
        i.eval_line(&mut ctx, "LINE 0 0 1 0 L").unwrap();
        i.eval_line(&mut ctx, "LINE 0 0 1 0 L").unwrap();
        i.eval_line(&mut ctx, "OVERKILL").unwrap();
        assert_eq!(ctx.doc.len(), 1);
    }

    #[test]
    fn stubs_error_clearly() {
        let i = interp();
        let mut ctx = Context::new();
        let err = i.eval_line(&mut ctx, "XREF").unwrap_err();
        assert!(err.contains("not implemented"));
    }

    #[test]
    fn template_layers_and_fromblock() {
        let i = interp();
        let mut ctx = Context::new();
        i.eval_line(&mut ctx, "TEMPLATE LAYERS").unwrap();
        assert!(ctx.doc.layer("TMPL_BORDER").is_some());
        assert!(ctx.doc.layer("TMPL_VIEW").is_some());
        assert!(ctx.doc.layer("TMPL_TEXT").is_some());
        i.eval_line(&mut ctx, "LINE 0 0 1 0 TMPL_BORDER").unwrap();
        i.eval_line(&mut ctx, "BLOCK NAMEPLATE 0 0 0").unwrap();
        i.eval_line(&mut ctx, "TEMPLATE FROMBLOCK NAMEPLATE")
            .unwrap();
        assert_eq!(ctx.doc.properties().nameplate_block, "NAMEPLATE");
    }

    #[test]
    fn li_and_le_aliases_resolve() {
        let r = crate::builtins();
        assert!(r.resolve("LI").is_some());
        assert_eq!(r.resolve("LI").unwrap().name(), "LIST");
        assert!(r.resolve("LS").is_some());
        assert_eq!(r.resolve("LE").unwrap().name(), "LENGTHEN");
        assert_eq!(r.resolve("DISTANCE").unwrap().name(), "DIST");
        assert!(!r
            .resolve("MLEADER")
            .unwrap()
            .aliases()
            .iter()
            .any(|a| a.eq_ignore_ascii_case("LE")));
        assert!(r.resolve("POL").is_some());
        assert!(r.resolve("MA").is_some());
    }
}
