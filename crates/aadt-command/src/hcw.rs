//! HCW Layer Standard (from HCW-TOOLS.lsp) — 35 firm layers + management commands.

use aadt_model::Layer;

use crate::{parse_linetype, Command, Context, Registry};

/// One HCW standard layer row: name, ACI, linetype token, lw mm, description.
pub struct HcwLayerDef {
    pub name: &'static str,
    pub aci: u8,
    pub linetype: &'static str,
    pub lw_mm: f32,
    pub description: &'static str,
}

/// HCW Layer Standard v4.0 (35 layers).
pub const HCW_LAYERS: &[HcwLayerDef] = &[
    // AN — Annotation
    HcwLayerDef { name: "AN-REF", aci: 250, linetype: "Continuous", lw_mm: 0.09, description: "Construction lines / viewport borders" },
    HcwLayerDef { name: "AN-GRID", aci: 8, linetype: "CENTER2", lw_mm: 0.13, description: "Grid and centre lines" },
    HcwLayerDef { name: "AN-SYMB", aci: 7, linetype: "Continuous", lw_mm: 0.18, description: "North point / symbols / logos" },
    HcwLayerDef { name: "AN-TEXT", aci: 7, linetype: "Continuous", lw_mm: 0.18, description: "All text — general, title block, keynotes" },
    HcwLayerDef { name: "AN-DIMS", aci: 8, linetype: "Continuous", lw_mm: 0.18, description: "Dimensions and leaders" },
    HcwLayerDef { name: "AN-HATCH", aci: 9, linetype: "Continuous", lw_mm: 0.10, description: "Hatching and fill patterns" },
    // A — Architecture
    HcwLayerDef { name: "A-WALL", aci: 1, linetype: "Continuous", lw_mm: 0.35, description: "Walls — new and existing (cut)" },
    HcwLayerDef { name: "A-DEMO", aci: 1, linetype: "DASHED2", lw_mm: 0.25, description: "Elements to be demolished" },
    HcwLayerDef { name: "A-COL", aci: 1, linetype: "Continuous", lw_mm: 0.35, description: "Columns (cut)" },
    HcwLayerDef { name: "A-DOOR", aci: 2, linetype: "Continuous", lw_mm: 0.25, description: "Doors — frame and swing arc" },
    HcwLayerDef { name: "A-WIND", aci: 4, linetype: "Continuous", lw_mm: 0.25, description: "Windows — frame and sill" },
    HcwLayerDef { name: "A-GLAZ", aci: 4, linetype: "HIDDEN", lw_mm: 0.18, description: "Glazing / glass line" },
    HcwLayerDef { name: "A-STAIR", aci: 2, linetype: "Continuous", lw_mm: 0.25, description: "Stairs — treads, stringers, ramps" },
    HcwLayerDef { name: "A-RAIL", aci: 6, linetype: "Continuous", lw_mm: 0.20, description: "Handrails and balustrades" },
    HcwLayerDef { name: "A-ROOF", aci: 1, linetype: "Continuous", lw_mm: 0.25, description: "Roof outline and edge" },
    HcwLayerDef { name: "A-FLR", aci: 3, linetype: "Continuous", lw_mm: 0.18, description: "Floor finish / slab outline" },
    HcwLayerDef { name: "A-CLNG", aci: 3, linetype: "Continuous", lw_mm: 0.18, description: "Ceiling outline and features" },
    HcwLayerDef { name: "A-DETAIL", aci: 1, linetype: "Continuous", lw_mm: 0.25, description: "Detail bubbles and reference marks" },
    // I — Interiors
    HcwLayerDef { name: "I-FURN", aci: 3, linetype: "Continuous", lw_mm: 0.20, description: "Loose furniture" },
    HcwLayerDef { name: "I-JOIN", aci: 5, linetype: "Continuous", lw_mm: 0.20, description: "Fixed joinery" },
    HcwLayerDef { name: "I-FINISH", aci: 30, linetype: "Continuous", lw_mm: 0.18, description: "Finish zones and material areas" },
    HcwLayerDef { name: "I-DECOR", aci: 140, linetype: "Continuous", lw_mm: 0.15, description: "Decorative elements" },
    // E — Electrical
    HcwLayerDef { name: "E-CEIL", aci: 6, linetype: "Continuous", lw_mm: 0.18, description: "Ceiling fittings" },
    HcwLayerDef { name: "E-POINT", aci: 2, linetype: "Continuous", lw_mm: 0.18, description: "Wall points — power, data, switches" },
    HcwLayerDef { name: "E-WIRE", aci: 30, linetype: "Continuous", lw_mm: 0.15, description: "Wiring runs and conduit" },
    // P — Plumbing
    HcwLayerDef { name: "P-FIXT", aci: 6, linetype: "Continuous", lw_mm: 0.18, description: "Plumbing fixtures" },
    HcwLayerDef { name: "P-PIPE", aci: 4, linetype: "Continuous", lw_mm: 0.18, description: "Pipes — supply, drain and vent" },
    // S — Site
    HcwLayerDef { name: "S-BOUND", aci: 1, linetype: "PHANTOM2", lw_mm: 0.35, description: "Site and property boundary" },
    HcwLayerDef { name: "S-ROAD", aci: 8, linetype: "Continuous", lw_mm: 0.25, description: "Road edge and carriageway" },
    HcwLayerDef { name: "S-LAND", aci: 3, linetype: "Continuous", lw_mm: 0.18, description: "Landscaping and planting" },
    HcwLayerDef { name: "S-TREE", aci: 140, linetype: "Continuous", lw_mm: 0.18, description: "Trees — canopy and trunk" },
    HcwLayerDef { name: "S-LEVEL", aci: 2, linetype: "Continuous", lw_mm: 0.18, description: "Spot levels and contours" },
    HcwLayerDef { name: "S-DRAIN", aci: 4, linetype: "Continuous", lw_mm: 0.18, description: "Site drainage and channels" },
    HcwLayerDef { name: "S-FENCE", aci: 8, linetype: "Continuous", lw_mm: 0.20, description: "Fences and boundary walls" },
    // PR — Presentation
    HcwLayerDef { name: "PR-TONE", aci: 9, linetype: "Continuous", lw_mm: 0.10, description: "Presentation tone" },
    HcwLayerDef { name: "PR-CLOUD", aci: 2, linetype: "Continuous", lw_mm: 0.25, description: "Revision clouds" },
];

/// Create or update all HCW layers into an open transaction. Returns (created, updated).
fn apply_hcw_layers_into(d: &mut aadt_model::Document) -> (usize, usize) {
    let mut created = 0usize;
    let mut updated = 0usize;
    for def in HCW_LAYERS {
        let lt = parse_linetype(def.linetype);
        if d.layer(def.name).is_none() {
            d.add_layer(
                Layer::new(def.name)
                    .with_color(def.aci)
                    .with_linetype(lt)
                    .with_lineweight_mm(def.lw_mm),
            );
            created += 1;
        } else {
            d.set_layer_color(def.name, def.aci);
            d.set_layer_linetype(def.name, lt);
            d.set_layer_lineweight_mm(def.name, def.lw_mm);
            updated += 1;
        }
    }
    (created, updated)
}

/// Create or update all HCW layers. Returns (created, updated).
pub fn apply_hcw_layers(ctx: &mut Context) -> (usize, usize) {
    ctx.doc
        .transact("HCWLAYERS", |d| apply_hcw_layers_into(d))
}

pub fn register_hcw(r: &mut Registry) {
    r.register(Box::new(HcwLayersCmd));
    r.register(Box::new(HcwResetCmd));
    r.register(Box::new(HcwInfoCmd));
    r.register(Box::new(HcwPurgeCmd));
    r.register(Box::new(HcwAuditCmd));
    r.register(Box::new(HcwLockCmd));
    r.register(Box::new(HcwUnlockCmd));
    r.register(Box::new(HcwAudit2Cmd));
    r.register(Box::new(HcwByBlockCmd));
    r.register(Box::new(HcwMoveCmd));
    r.register(Box::new(HcwScheduleCmd));
    r.register(Box::new(HcwLayerStateCmd));
    r.register(Box::new(HcwLegendCmd));
    r.register(Box::new(HcwHelpCmd));
}

struct HcwLayersCmd;
impl Command for HcwLayersCmd {
    fn name(&self) -> &str {
        "HCWLAYERS"
    }
    fn help(&self) -> &str {
        "HCWLAYERS — create/update Holgundi Consulting Works layer standard (35 layers)"
    }
    fn run(&self, ctx: &mut Context, _args: &[String]) -> Result<String, String> {
        let (c, u) = apply_hcw_layers(ctx);
        Ok(format!(
            "HCW Layer Standard v4.0 — created {c}, updated {u} ({} total)",
            HCW_LAYERS.len()
        ))
    }
}

struct HcwResetCmd;
impl Command for HcwResetCmd {
    fn name(&self) -> &str {
        "HCWRESET"
    }
    fn help(&self) -> &str {
        "HCWRESET — repair / reapply all HCW layer properties"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        HcwLayersCmd.run(ctx, args)
    }
}

struct HcwInfoCmd;
impl Command for HcwInfoCmd {
    fn name(&self) -> &str {
        "HCWINFO"
    }
    fn help(&self) -> &str {
        "HCWINFO — print HCW layer table"
    }
    fn run(&self, _ctx: &mut Context, _args: &[String]) -> Result<String, String> {
        let mut lines = vec![format!(
            "HCW Layer Standard v4.0 — {} layers:",
            HCW_LAYERS.len()
        )];
        for d in HCW_LAYERS {
            lines.push(format!(
                "  {:12} ACI{:3} {:10} {:4.2}mm  {}",
                d.name, d.aci, d.linetype, d.lw_mm, d.description
            ));
        }
        Ok(lines.join("\n"))
    }
}

struct HcwPurgeCmd;
impl Command for HcwPurgeCmd {
    fn name(&self) -> &str {
        "HCWPURGE"
    }
    fn help(&self) -> &str {
        "HCWPURGE — purge unused layers (PURGE LAYERS)"
    }
    fn run(&self, ctx: &mut Context, _args: &[String]) -> Result<String, String> {
        use std::collections::HashSet;
        let mut used: HashSet<String> = HashSet::from(["0".to_string()]);
        for e in ctx.doc.entities() {
            used.insert(e.layer.clone());
        }
        for h in ctx.doc.hatches() {
            used.insert(h.layer.clone());
        }
        for d in ctx.doc.dimensions() {
            used.insert(d.layer.clone());
        }
        for b in ctx.doc.blocks() {
            for be in &b.entities {
                used.insert(be.layer.clone());
            }
        }
        let remove: Vec<String> = ctx
            .doc
            .layers()
            .map(|l| l.name.clone())
            .filter(|n| !used.contains(n))
            .collect();
        let n = remove.len();
        if n == 0 {
            return Ok("HCWPURGE: nothing to purge".into());
        }
        ctx.doc.transact("HCWPURGE", |d| {
            for name in &remove {
                d.remove_layer(name);
            }
        });
        Ok(format!("HCWPURGE: purged {n} unused layer(s)"))
    }
}

struct HcwAuditCmd;
impl Command for HcwAuditCmd {
    fn name(&self) -> &str {
        "HCWAUDIT"
    }
    fn help(&self) -> &str {
        "HCWAUDIT — run drawing AUDIT"
    }
    fn run(&self, ctx: &mut Context, _args: &[String]) -> Result<String, String> {
        // Reuse audit if we can call it — soft check: entity count + missing inserts.
        let n = ctx.doc.len();
        let missing: usize = ctx
            .doc
            .entities()
            .iter()
            .filter(|e| {
                if let aadt_model::Geometry::Insert(ins) = &e.geometry {
                    ctx.doc.block(&ins.name).is_none()
                } else {
                    false
                }
            })
            .count();
        Ok(format!(
            "HCWAUDIT — {n} entities, {missing} insert(s) with missing block defs"
        ))
    }
}

fn freeze_prefix(ctx: &mut Context, prefix: &str, frozen: bool) -> usize {
    let names: Vec<String> = ctx
        .doc
        .layers()
        .filter(|l| l.name.starts_with(prefix))
        .map(|l| l.name.clone())
        .collect();
    let n = names.len();
    let label = if frozen { "HCWLOCK" } else { "HCWUNLOCK" };
    ctx.doc.transact(label, |d| {
        for name in &names {
            d.set_layer_frozen(name, frozen);
        }
    });
    n
}

struct HcwLockCmd;
impl Command for HcwLockCmd {
    fn name(&self) -> &str {
        "HCWLOCK"
    }
    fn help(&self) -> &str {
        "HCWLOCK <prefix> — freeze layers by name prefix (e.g. E-)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let prefix = args.first().ok_or("HCWLOCK: missing prefix (e.g. E-)")?;
        let n = freeze_prefix(ctx, prefix, true);
        Ok(format!("Locked (frozen) {n} layer(s) with prefix '{prefix}'"))
    }
}

struct HcwUnlockCmd;
impl Command for HcwUnlockCmd {
    fn name(&self) -> &str {
        "HCWUNLOCK"
    }
    fn help(&self) -> &str {
        "HCWUNLOCK <prefix> — thaw layers by name prefix"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let prefix = args.first().ok_or("HCWUNLOCK: missing prefix")?;
        let n = freeze_prefix(ctx, prefix, false);
        Ok(format!("Unlocked (thawed) {n} layer(s) with prefix '{prefix}'"))
    }
}

struct HcwAudit2Cmd;
impl Command for HcwAudit2Cmd {
    fn name(&self) -> &str {
        "HCWAUDIT2"
    }
    fn help(&self) -> &str {
        "HCWAUDIT2 — list objects not on a standard HCW layer"
    }
    fn run(&self, ctx: &mut Context, _args: &[String]) -> Result<String, String> {
        use std::collections::HashMap;
        let valid: std::collections::HashSet<&str> =
            HCW_LAYERS.iter().map(|d| d.name).collect();
        let mut rogue: HashMap<String, usize> = HashMap::new();
        for e in ctx.doc.entities() {
            if !valid.contains(e.layer.as_str()) {
                *rogue.entry(e.layer.clone()).or_default() += 1;
            }
        }
        if rogue.is_empty() {
            return Ok("HCWAUDIT2: All objects are on standard HCW layers. PASS.".into());
        }
        let mut lines = vec![format!(
            "HCWAUDIT2: {} non-standard layer(s):",
            rogue.len()
        )];
        let mut keys: Vec<_> = rogue.keys().cloned().collect();
        keys.sort();
        for k in keys {
            lines.push(format!("  {k}: {} object(s)", rogue[&k]));
        }
        Ok(lines.join("\n"))
    }
}

struct HcwByBlockCmd;
impl Command for HcwByBlockCmd {
    fn name(&self) -> &str {
        "HCWBYBLOCK"
    }
    fn help(&self) -> &str {
        "HCWBYBLOCK — note: entity-level colour/lw overrides are not stored (ByLayer model)"
    }
    fn run(&self, _ctx: &mut Context, _args: &[String]) -> Result<String, String> {
        Ok(
            "HCWBYBLOCK: AiADT entities use ByLayer properties — no explicit overrides to report. PASS."
                .into(),
        )
    }
}

struct HcwMoveCmd;
impl Command for HcwMoveCmd {
    fn name(&self) -> &str {
        "HCWMOVE"
    }
    fn help(&self) -> &str {
        "HCWMOVE <layer> [id…] — move SELECTION (or ids) to an HCW layer"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let layer = args.first().ok_or("HCWMOVE: missing target layer")?.clone();
        if ctx.doc.layer(&layer).is_none() {
            return Err(format!("HCWMOVE: layer {layer:?} not found — run HCWLAYERS first"));
        }
        let ids = if args.len() > 1 {
            args[1..]
                .iter()
                .map(|a| crate::parse_entity_id(a, "HCWMOVE"))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            crate::resolve_targets(ctx, "SELECTION", "HCWMOVE")?
        };
        let n = ids.len();
        ctx.doc.transact("HCWMOVE", |d| {
            for id in &ids {
                d.set_entity_layer(*id, layer.clone());
            }
        });
        Ok(format!("HCWMOVE: {n} object(s) moved to {layer}"))
    }
}

struct HcwScheduleCmd;
impl Command for HcwScheduleCmd {
    fn name(&self) -> &str {
        "HCWSCHEDULE"
    }
    fn help(&self) -> &str {
        "HCWSCHEDULE — print HCW layer schedule (name, ACI, count)"
    }
    fn run(&self, ctx: &mut Context, _args: &[String]) -> Result<String, String> {
        use std::collections::HashMap;
        let mut counts: HashMap<String, usize> = HashMap::new();
        for e in ctx.doc.entities() {
            *counts.entry(e.layer.clone()).or_default() += 1;
        }
        let mut lines = vec!["Layer,ACI,Linetype,LW_mm,InDrawing,ObjectCount".to_string()];
        for d in HCW_LAYERS {
            let exists = ctx.doc.layer(d.name).is_some();
            let n = counts.get(d.name).copied().unwrap_or(0);
            lines.push(format!(
                "{},{},{},{:.2},{},{}",
                d.name,
                d.aci,
                d.linetype,
                d.lw_mm,
                if exists { "Yes" } else { "No" },
                n
            ));
        }
        Ok(lines.join("\n"))
    }
}

struct HcwLayerStateCmd;
impl Command for HcwLayerStateCmd {
    fn name(&self) -> &str {
        "HCWLAYERSTATE"
    }
    fn help(&self) -> &str {
        "HCWLAYERSTATE — session layer freeze snapshots (use LAYISO / layer manager for now)"
    }
    fn run(&self, _ctx: &mut Context, _args: &[String]) -> Result<String, String> {
        Ok(
            "HCWLAYERSTATE: use LAYISO / LAYUNISO and the Layers panel; named states not yet persisted"
                .into(),
        )
    }
}

struct HcwLegendCmd;
impl Command for HcwLegendCmd {
    fn name(&self) -> &str {
        "HCWLEGEND"
    }
    fn help(&self) -> &str {
        "HCWLEGEND x y — place a compact HCW layer name list at point"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let x = crate::parse_f64(args, 0, "HCWLEGEND")?;
        let y = crate::parse_f64(args, 1, "HCWLEGEND")?;
        let h = 0.25;
        let entries: Vec<(String, f64)> = {
            let mut out = Vec::new();
            let mut yy = y;
            out.push(("HCW LAYER STANDARD v4.0".to_string(), yy));
            yy -= h * 2.0;
            for def in HCW_LAYERS {
                out.push((format!("{}  {}", def.name, def.description), yy));
                yy -= h * 1.4;
            }
            out
        };
        let n = entries.len();
        ctx.doc.transact("HCWLEGEND", |d| {
            apply_hcw_layers_into(d);
            if d.layer("AN-TEXT").is_none() {
                d.add_layer(Layer::new("AN-TEXT").with_color(7));
            }
            for (i, (text, yy)) in entries.iter().enumerate() {
                let ht = if i == 0 { h * 1.4 } else { h };
                d.add_geometry(
                    "AN-TEXT",
                    aadt_model::Geometry::Text(aadt_model::Text::new(
                        text.clone(),
                        aadt_geometry::Point2::new(x, *yy),
                        ht,
                        0.0,
                    )),
                );
            }
        });
        Ok(format!(
            "HCWLEGEND: {n} labels placed at ({x}, {y})"
        ))
    }
}

struct HcwHelpCmd;
impl Command for HcwHelpCmd {
    fn name(&self) -> &str {
        "HCWHELP"
    }
    fn help(&self) -> &str {
        "HCWHELP — Holgundi toolkit command list"
    }
    fn run(&self, _ctx: &mut Context, _args: &[String]) -> Result<String, String> {
        Ok(
            "HCW-TOOLS — HCWLAYERS HCWRESET HCWINFO HCWPURGE HCWAUDIT HCWLOCK HCWUNLOCK \
             HCWAUDIT2 HCWMOVE HCWSCHEDULE HCWLEGEND | BPLTSTART BPLTLAYERS | \
             MROOM/IROOM room labels | FIXTXT TextIncrement"
                .into(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Interpreter;

    #[test]
    fn hcwlayers_creates_standard_set() {
        let i = Interpreter::with_builtins();
        let mut ctx = Context::new();
        i.eval_line(&mut ctx, "HCWLAYERS").unwrap();
        assert!(ctx.doc.layer("A-WALL").is_some());
        assert!(ctx.doc.layer("AN-TEXT").is_some());
        assert!(ctx.doc.layer("PR-CLOUD").is_some());
    }
}
