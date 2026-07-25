//! Building Permission Layer Tool (BBMP BPAS / AutoPlan) — from HCW-TOOLS.lsp.

use aadt_model::Layer;

use crate::{parse_f64, parse_linetype, Command, Context, Registry};

/// BPLT layer row: name, ACI, linetype, lineweight as acLnWt-style hundredths of mm
/// (18 = 0.18 mm). AP- layers are non-plotting helpers (we still create them).
struct BpltLayer {
    name: &'static str,
    aci: u8,
    linetype: &'static str,
    /// Hundredths of a millimetre (AutoCAD acLnWt).
    lw_enum: i32,
}

/// ISO A-series sheet sizes in metres (width × height, landscape).
const SHEET_A0_M: (f64, f64) = (1.189, 0.841);
const SHEET_A1_M: (f64, f64) = (0.841, 0.594);
const SHEET_A2_M: (f64, f64) = (0.594, 0.420);
const LEGEND_TEXT_H: f64 = 0.30;
const AREA_REPORT_TEXT_H: f64 = 0.20;
const TITLE_BLOCK_H: f64 = 0.060;

const BPLT_LAYERS: &[BpltLayer] = &[
    BpltLayer {
        name: "BP-SITE-BOUNDARY",
        aci: 1,
        linetype: "Continuous",
        lw_enum: 50,
    },
    BpltLayer {
        name: "BP-SITE-SETBACK",
        aci: 3,
        linetype: "DASHED2",
        lw_enum: 25,
    },
    BpltLayer {
        name: "BP-SITE-SURRENDER",
        aci: 3,
        linetype: "DASHED",
        lw_enum: 18,
    },
    BpltLayer {
        name: "BP-ROAD-EDGE",
        aci: 8,
        linetype: "Continuous",
        lw_enum: 35,
    },
    BpltLayer {
        name: "BP-ROAD-CL",
        aci: 8,
        linetype: "CENTER2",
        lw_enum: 18,
    },
    BpltLayer {
        name: "BP-ROAD-WIDENING",
        aci: 8,
        linetype: "DASHED",
        lw_enum: 18,
    },
    BpltLayer {
        name: "BP-DRIVEWAY",
        aci: 52,
        linetype: "Continuous",
        lw_enum: 25,
    },
    BpltLayer {
        name: "BP-COMP-WALL",
        aci: 9,
        linetype: "Continuous",
        lw_enum: 18,
    },
    BpltLayer {
        name: "BP-BUILDING-CUT",
        aci: 7,
        linetype: "Continuous",
        lw_enum: 50,
    },
    BpltLayer {
        name: "BP-BUILDING-VIEW",
        aci: 7,
        linetype: "Continuous",
        lw_enum: 25,
    },
    BpltLayer {
        name: "BP-BUILDING-HATCH",
        aci: 7,
        linetype: "Continuous",
        lw_enum: 13,
    },
    BpltLayer {
        name: "BP-DOOR",
        aci: 252,
        linetype: "Continuous",
        lw_enum: 18,
    },
    BpltLayer {
        name: "BP-WINDOW",
        aci: 252,
        linetype: "Continuous",
        lw_enum: 13,
    },
    BpltLayer {
        name: "BP-STAIR",
        aci: 142,
        linetype: "Continuous",
        lw_enum: 25,
    },
    BpltLayer {
        name: "BP-LIFT",
        aci: 141,
        linetype: "Continuous",
        lw_enum: 18,
    },
    BpltLayer {
        name: "BP-RAMP",
        aci: 140,
        linetype: "Continuous",
        lw_enum: 18,
    },
    BpltLayer {
        name: "BP-CORRIDOR",
        aci: 143,
        linetype: "Continuous",
        lw_enum: 18,
    },
    BpltLayer {
        name: "BP-ROOM",
        aci: 153,
        linetype: "Continuous",
        lw_enum: 18,
    },
    BpltLayer {
        name: "BP-TOILET",
        aci: 153,
        linetype: "Continuous",
        lw_enum: 13,
    },
    BpltLayer {
        name: "BP-BALCONY",
        aci: 144,
        linetype: "Continuous",
        lw_enum: 18,
    },
    BpltLayer {
        name: "BP-TERRACE",
        aci: 96,
        linetype: "Continuous",
        lw_enum: 18,
    },
    BpltLayer {
        name: "BP-PORCH",
        aci: 22,
        linetype: "Continuous",
        lw_enum: 18,
    },
    BpltLayer {
        name: "BP-PARKING",
        aci: 151,
        linetype: "Continuous",
        lw_enum: 18,
    },
    BpltLayer {
        name: "BP-RWH",
        aci: 133,
        linetype: "Continuous",
        lw_enum: 18,
    },
    BpltLayer {
        name: "BP-SHEET-BORDER",
        aci: 8,
        linetype: "Continuous",
        lw_enum: 50,
    },
    BpltLayer {
        name: "BP-TITLE-BLOCK",
        aci: 8,
        linetype: "Continuous",
        lw_enum: 25,
    },
    BpltLayer {
        name: "BP-DIM",
        aci: 2,
        linetype: "Continuous",
        lw_enum: 13,
    },
    BpltLayer {
        name: "BP-TEXT-GENERAL",
        aci: 2,
        linetype: "Continuous",
        lw_enum: 13,
    },
    BpltLayer {
        name: "BP-TEXT-ROOM",
        aci: 2,
        linetype: "Continuous",
        lw_enum: 13,
    },
    BpltLayer {
        name: "BP-AREA-TABLE",
        aci: 2,
        linetype: "Continuous",
        lw_enum: 13,
    },
    BpltLayer {
        name: "BP-NOTES",
        aci: 2,
        linetype: "Continuous",
        lw_enum: 13,
    },
    BpltLayer {
        name: "BP-SECTION",
        aci: 30,
        linetype: "Continuous",
        lw_enum: 35,
    },
    BpltLayer {
        name: "BP-ELEVATION",
        aci: 30,
        linetype: "Continuous",
        lw_enum: 25,
    },
    // AP- AutoPlan helpers (subset of full LISP table)
    BpltLayer {
        name: "AP-DRAWING-BOUND",
        aci: 253,
        linetype: "Continuous",
        lw_enum: 13,
    },
    BpltLayer {
        name: "AP-SITE-GROSS",
        aci: 253,
        linetype: "Continuous",
        lw_enum: 13,
    },
    BpltLayer {
        name: "AP-SITE-NET",
        aci: 253,
        linetype: "Continuous",
        lw_enum: 13,
    },
    BpltLayer {
        name: "AP-ROAD",
        aci: 253,
        linetype: "Continuous",
        lw_enum: 13,
    },
    BpltLayer {
        name: "AP-BLDG-BOUNDARY",
        aci: 253,
        linetype: "Continuous",
        lw_enum: 13,
    },
    BpltLayer {
        name: "AP-FLOOR-LEVEL",
        aci: 253,
        linetype: "Continuous",
        lw_enum: 13,
    },
    BpltLayer {
        name: "AP-BLDG-AREA",
        aci: 253,
        linetype: "Continuous",
        lw_enum: 13,
    },
    BpltLayer {
        name: "AP-ROOM",
        aci: 253,
        linetype: "Continuous",
        lw_enum: 13,
    },
    BpltLayer {
        name: "AP-STAIRCASE",
        aci: 253,
        linetype: "Continuous",
        lw_enum: 13,
    },
    BpltLayer {
        name: "AP-PARKING",
        aci: 253,
        linetype: "Continuous",
        lw_enum: 13,
    },
];

fn lw_mm(enum_int: i32) -> f32 {
    (enum_int as f32) / 100.0
}

/// Create / update BPLT layers inside an open transaction. Returns count processed.
fn apply_bplt_layers_into(d: &mut aadt_model::Document) -> usize {
    let mut n = 0usize;
    for def in BPLT_LAYERS {
        let lt = parse_linetype(def.linetype);
        let mm = lw_mm(def.lw_enum);
        if d.layer(def.name).is_none() {
            d.add_layer(
                Layer::new(def.name)
                    .with_color(def.aci)
                    .with_linetype(lt)
                    .with_lineweight_mm(mm),
            );
        } else {
            d.set_layer_color(def.name, def.aci);
            d.set_layer_linetype(def.name, lt);
            d.set_layer_lineweight_mm(def.name, mm);
        }
        n += 1;
    }
    n
}

/// Create / update BPLT layers. Returns count processed.
pub fn apply_bplt_layers(ctx: &mut Context) -> usize {
    ctx.doc.transact("BPLT LAYERS", apply_bplt_layers_into)
}

pub fn register_bplt(r: &mut Registry) {
    r.register(Box::new(BpltStartCmd));
    r.register(Box::new(BpltLayersCmd));
    r.register(Box::new(BpltCopyCmd));
    r.register(Box::new(BpltCheckCmd));
    r.register(Box::new(BpltAreaCmd));
    r.register(Box::new(BpltReportCmd));
    r.register(Box::new(BpltTitleBlockCmd));
    r.register(Box::new(BpltHelpCmd));
}

struct BpltStartCmd;
impl Command for BpltStartCmd {
    fn name(&self) -> &str {
        "BPLTSTART"
    }
    fn help(&self) -> &str {
        "BPLTSTART — BBMP BPAS / AutoPlan setup: metres units + BP-/AP- layers"
    }
    fn run(&self, ctx: &mut Context, _args: &[String]) -> Result<String, String> {
        let mut props = ctx.doc.properties().clone();
        props.units = "m".into();
        props.dimstyle = "METRIC".into();
        let n = ctx.doc.transact("BPLTSTART", |d| {
            d.set_properties(props);
            apply_bplt_layers_into(d)
        });
        Ok(format!(
            "BPLTSTART — units=m · {n} BP-/AP- layers ready · current drafting layer BP-BUILDING-CUT (set via Layers). AutoPlan: draw closed polys, copy to AP- layers, verify before .apz export."
        ))
    }
}

struct BpltLayersCmd;
impl Command for BpltLayersCmd {
    fn name(&self) -> &str {
        "BPLTLAYERS"
    }
    fn help(&self) -> &str {
        "BPLTLAYERS x y — place a compact BP-/AP- layer name legend"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let x = parse_f64(args, 0, "BPLTLAYERS")?;
        let y = parse_f64(args, 1, "BPLTLAYERS")?;
        let h = LEGEND_TEXT_H;
        let mut entries = Vec::new();
        let mut yy = y;
        entries.push((
            "Building Permission Layer Tool — Layer Legend".to_string(),
            yy,
            h * 1.5,
        ));
        yy -= h * 2.0;
        for def in BPLT_LAYERS {
            entries.push((def.name.to_string(), yy, h));
            yy -= h * 1.3;
        }
        let n = entries.len();
        ctx.doc.transact("BPLTLAYERS", |d| {
            apply_bplt_layers_into(d);
            if d.layer("BP-TEXT-GENERAL").is_none() {
                d.add_layer(Layer::new("BP-TEXT-GENERAL").with_color(2));
            }
            for (text, yy, ht) in &entries {
                d.add_geometry(
                    "BP-TEXT-GENERAL",
                    aadt_model::Geometry::Text(aadt_model::Text::new(
                        text.clone(),
                        aadt_geometry::Point2::new(x, *yy),
                        *ht,
                        0.0,
                    )),
                );
            }
        });
        Ok(format!("BPLTLAYERS: {n} legend rows at ({x}, {y})"))
    }
}

struct BpltCopyCmd;
impl Command for BpltCopyCmd {
    fn name(&self) -> &str {
        "BPLTCOPY"
    }
    fn help(&self) -> &str {
        "BPLTCOPY [id…] — copy SELECTION from BP-* to matching AP-* layer"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let ids = if args.is_empty() {
            crate::resolve_targets(ctx, "SELECTION", "BPLTCOPY")?
        } else {
            args.iter()
                .map(|a| crate::parse_entity_id(a, "BPLTCOPY"))
                .collect::<Result<Vec<_>, _>>()?
        };
        apply_bplt_layers(ctx);
        let mut copied = 0usize;
        let mut skipped = 0usize;
        // Collect clones first (can't borrow doc mutably while iterating).
        let mut to_add: Vec<(String, aadt_model::Geometry)> = Vec::new();
        for id in &ids {
            let Some(e) = ctx.doc.get(*id) else {
                skipped += 1;
                continue;
            };
            let lay = e.layer.clone();
            if !lay.to_ascii_uppercase().starts_with("BP-") {
                skipped += 1;
                continue;
            }
            let ap = format!("AP-{}", &lay[3..]);
            if ctx.doc.layer(&ap).is_none() {
                skipped += 1;
                continue;
            }
            to_add.push((ap, e.geometry.clone()));
            copied += 1;
        }
        ctx.doc.transact("BPLTCOPY", |d| {
            for (layer, g) in to_add {
                d.add_geometry(layer, g);
            }
        });
        Ok(format!(
            "BPLTCOPY done. Copied: {copied}  Skipped: {skipped}"
        ))
    }
}

struct BpltCheckCmd;
impl Command for BpltCheckCmd {
    fn name(&self) -> &str {
        "BPLTCHECK"
    }
    fn help(&self) -> &str {
        "BPLTCHECK — AutoPlan pre-submission: required AP- closed polys"
    }
    fn run(&self, ctx: &mut Context, _args: &[String]) -> Result<String, String> {
        let required = [
            "AP-DRAWING-BOUND",
            "AP-SITE-GROSS",
            "AP-SITE-NET",
            "AP-ROAD",
            "AP-BLDG-BOUNDARY",
            "AP-FLOOR-LEVEL",
            "AP-BLDG-AREA",
            "AP-ROOM",
        ];
        let mut lines = vec!["BPLTCHECK — required AP- closed polylines:".to_string()];
        let mut fail = 0usize;
        for name in required {
            let count = ctx
                .doc
                .entities()
                .iter()
                .filter(|e| {
                    e.layer == name
                        && matches!(&e.geometry, aadt_model::Geometry::Polyline(p) if p.closed)
                })
                .count();
            if count == 0 {
                fail += 1;
                lines.push(format!("  FAIL  {name} *** MISSING ***"));
            } else {
                lines.push(format!("  PASS  {name} ({count} closed)"));
            }
        }
        lines.push(if fail == 0 {
            "RESULT: PASS — ready for AutoPlan".into()
        } else {
            format!("RESULT: FAIL — {fail} required layer(s) missing")
        });
        Ok(lines.join("\n"))
    }
}

struct BpltAreaCmd;
impl Command for BpltAreaCmd {
    fn name(&self) -> &str {
        "BPLTAREA"
    }
    fn help(&self) -> &str {
        "BPLTAREA x y — area statement table from AP- closed polylines"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let x = parse_f64(args, 0, "BPLTAREA")?;
        let y = parse_f64(args, 1, "BPLTAREA")?;
        use std::collections::BTreeMap;
        let mut areas: BTreeMap<String, f64> = BTreeMap::new();
        for e in ctx.doc.entities() {
            if !e.layer.to_ascii_uppercase().starts_with("AP-") {
                continue;
            }
            if let aadt_model::Geometry::Polyline(p) = &e.geometry {
                if p.closed {
                    *areas.entry(e.layer.clone()).or_default() += p.area();
                }
            }
        }
        if areas.is_empty() {
            return Err("BPLTAREA: no AP- closed polylines found".into());
        }
        apply_bplt_layers(ctx);
        let mut total = 0.0;
        let mut lines = vec!["AREA STATEMENT".to_string()];
        for (lay, a) in &areas {
            total += a;
            lines.push(format!("{lay}: {a:.3} m2"));
        }
        lines.push(format!("TOTAL: {total:.3} m2"));
        let h = AREA_REPORT_TEXT_H;
        let mut yy = y;
        let placed = lines.len();
        ctx.doc.transact("BPLTAREA", |d| {
            for line in &lines {
                d.add_geometry(
                    "BP-AREA-TABLE",
                    aadt_model::Geometry::Text(aadt_model::Text::new(
                        line.clone(),
                        aadt_geometry::Point2::new(x, yy),
                        h,
                        0.0,
                    )),
                );
                yy -= h * 1.4;
            }
        });
        Ok(format!(
            "BPLTAREA: {placed} lines placed · total {total:.3} m2"
        ))
    }
}

struct BpltReportCmd;
impl Command for BpltReportCmd {
    fn name(&self) -> &str {
        "BPLTREPORT"
    }
    fn help(&self) -> &str {
        "BPLTREPORT — print BP-/AP- layer object counts (CSV-like)"
    }
    fn run(&self, ctx: &mut Context, _args: &[String]) -> Result<String, String> {
        use std::collections::BTreeMap;
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for e in ctx.doc.entities() {
            let u = e.layer.to_ascii_uppercase();
            if u.starts_with("BP-") || u.starts_with("AP-") {
                *counts.entry(e.layer.clone()).or_default() += 1;
            }
        }
        let mut lines = vec!["Layer,Type,ObjectCount".to_string()];
        for (lay, n) in counts {
            let ty = if lay.to_ascii_uppercase().starts_with("AP-") {
                "AP"
            } else {
                "BP"
            };
            lines.push(format!("{lay},{ty},{n}"));
        }
        Ok(lines.join("\n"))
    }
}

struct BpltTitleBlockCmd;
impl Command for BpltTitleBlockCmd {
    fn name(&self) -> &str {
        "BPLTTITLEBLOCK"
    }
    fn help(&self) -> &str {
        "BPLTTITLEBLOCK A0|A1|A2 x y — sheet border + title strip (metres)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let size = args
            .first()
            .map(|s| s.to_ascii_uppercase())
            .unwrap_or_else(|| "A1".into());
        let (w, h) = match size.as_str() {
            "A0" => SHEET_A0_M,
            "A2" => SHEET_A2_M,
            _ => SHEET_A1_M,
        };
        let x = parse_f64(args, 1, "BPLTTITLEBLOCK")?;
        let y = parse_f64(args, 2, "BPLTTITLEBLOCK")?;
        apply_bplt_layers(ctx);
        let corners = vec![
            aadt_geometry::Point2::new(x, y),
            aadt_geometry::Point2::new(x + w, y),
            aadt_geometry::Point2::new(x + w, y + h),
            aadt_geometry::Point2::new(x, y + h),
        ];
        let title_h = TITLE_BLOCK_H;
        let strip = vec![
            aadt_geometry::Point2::new(x, y),
            aadt_geometry::Point2::new(x + w, y),
            aadt_geometry::Point2::new(x + w, y + title_h),
            aadt_geometry::Point2::new(x, y + title_h),
        ];
        ctx.doc.transact("BPLTTITLEBLOCK", |d| {
            d.add_geometry(
                "BP-SHEET-BORDER",
                aadt_model::Geometry::Polyline(aadt_geometry::Polyline::new(corners, true)),
            );
            d.add_geometry(
                "BP-TITLE-BLOCK",
                aadt_model::Geometry::Polyline(aadt_geometry::Polyline::new(strip, true)),
            );
            d.add_geometry(
                "BP-TITLE-BLOCK",
                aadt_model::Geometry::Text(aadt_model::Text::new(
                    "PROJECT NAME",
                    aadt_geometry::Point2::new(x + 0.01, y + title_h + 0.005),
                    0.012,
                    0.0,
                )),
            );
            d.add_geometry(
                "BP-TITLE-BLOCK",
                aadt_model::Geometry::Text(aadt_model::Text::new(
                    format!("Sheet: {size}"),
                    aadt_geometry::Point2::new(x + w - 0.12, y + 0.01),
                    0.008,
                    0.0,
                )),
            );
        });
        Ok(format!(
            "BPLTTITLEBLOCK: {size} ({:.0} x {:.0} mm) at ({x}, {y})",
            w * 1000.0,
            h * 1000.0
        ))
    }
}

struct BpltHelpCmd;
impl Command for BpltHelpCmd {
    fn name(&self) -> &str {
        "BPLTHELP"
    }
    fn help(&self) -> &str {
        "BPLTHELP — Building Permission Layer Tool help"
    }
    fn run(&self, _ctx: &mut Context, _args: &[String]) -> Result<String, String> {
        Ok(
            "BPLT — BPLTSTART (metres + layers) · BPLTLAYERS x y · BPLTCOPY · BPLTCHECK · \
             BPLTAREA x y · BPLTREPORT · BPLTTITLEBLOCK A1 x y"
                .into(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Interpreter;

    #[test]
    fn bpltstart_sets_metres_and_layers() {
        let i = Interpreter::with_builtins();
        let mut ctx = Context::new();
        i.eval_line(&mut ctx, "BPLTSTART").unwrap();
        assert_eq!(ctx.doc.properties().units, "m");
        assert!(ctx.doc.layer("BP-BUILDING-CUT").is_some());
        assert!(ctx.doc.layer("AP-ROOM").is_some());
    }
}
