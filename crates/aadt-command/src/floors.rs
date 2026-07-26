//! Floor / category model-space commands (ADR-0009).

use aadt_geometry::Point2;
use aadt_model::SpaceCategory;

use crate::{Command, Context};

pub struct FloorCmd;
impl Command for FloorCmd {
    fn name(&self) -> &str {
        "FLOOR"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["FLOORS", "LEVEL", "LEVELS"]
    }
    fn help(&self) -> &str {
        "FLOOR [name|index] — list floors, or set the active floor"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        ctx.doc.ensure_default_spaces();
        if args.is_empty() {
            let active = ctx.doc.active_floor_id();
            let mut lines = Vec::new();
            for f in ctx.doc.floors() {
                let mark = if Some(f.id) == active { "*" } else { " " };
                lines.push(format!(
                    "{mark} [{}] {}  elev={:.3} m",
                    f.index, f.name, f.elevation_m
                ));
            }
            return Ok(lines.join("\n"));
        }
        let key = args.join(" ");
        if let Ok(idx) = key.parse::<u32>() {
            let id = ctx
                .doc
                .floors()
                .into_iter()
                .find(|f| f.index == idx)
                .map(|f| f.id)
                .ok_or_else(|| format!("no floor with index {idx}"))?;
            ctx.doc.transact("FLOOR", |d| d.set_active_floor(id))?;
            return Ok(format!(
                "active floor → {}",
                ctx.doc
                    .floors()
                    .iter()
                    .find(|f| f.id == id)
                    .map(|f| f.name.as_str())
                    .unwrap_or("?")
            ));
        }
        let id = ctx
            .doc
            .floors()
            .into_iter()
            .find(|f| f.name.eq_ignore_ascii_case(&key))
            .map(|f| f.id)
            .ok_or_else(|| format!("unknown floor {key:?}"))?;
        let name = ctx
            .doc
            .floor(id)
            .map(|f| f.name.clone())
            .unwrap_or_default();
        ctx.doc.transact("FLOOR", |d| d.set_active_floor(id))?;
        Ok(format!("active floor → {name}"))
    }
}

pub struct FloorAddCmd;
impl Command for FloorAddCmd {
    fn name(&self) -> &str {
        "FLOORADD"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["ADDFLOOR", "LEVELADD"]
    }
    fn help(&self) -> &str {
        "FLOORADD name [elevation_m] — append a floor above with five category spaces"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        if args.is_empty() {
            return Err("FLOORADD needs a name".into());
        }
        let default_step =
            parse_height_metres(ctx.doc.properties().floor_height.trim()).unwrap_or(3.0);
        let elev = args
            .last()
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or_else(|| {
                ctx.doc
                    .floors()
                    .last()
                    .map(|f| f.elevation_m + default_step)
                    .unwrap_or(default_step)
            });
        let name = if args.last().and_then(|s| s.parse::<f64>().ok()).is_some() && args.len() > 1 {
            args[..args.len() - 1].join(" ")
        } else {
            args.join(" ")
        };
        let id = ctx
            .doc
            .transact("FLOORADD", |d| d.add_floor(name.clone(), elev));
        let _ = ctx.doc.set_active_floor(id);
        Ok(format!(
            "added floor {name} (elev {elev:.3} m) with Walls/Structural/Lintel/BeamLayout/Furniture"
        ))
    }
}

pub struct SpaceAddCmd;
impl Command for SpaceAddCmd {
    fn name(&self) -> &str {
        "SPACEADD"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["ADDSPACE", "NEWSPACE"]
    }
    fn help(&self) -> &str {
        "SPACEADD name — add a custom model space on the active floor"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        ctx.doc.ensure_default_spaces();
        if args.is_empty() {
            return Err("SPACEADD needs a name".into());
        }
        let label = args.join(" ");
        let floor = ctx
            .doc
            .active_floor_id()
            .ok_or_else(|| "no active floor".to_string())?;
        let id = {
            let floor = floor;
            let label = label.clone();
            ctx.doc.transact("SPACEADD", |d| {
                let id = d.add_custom_space(floor, label)?;
                d.set_active_space(id)?;
                Ok::<_, String>(id)
            })?
        };
        let name = ctx
            .doc
            .model_space(id)
            .map(|s| s.name.clone())
            .unwrap_or(label);
        Ok(format!(
            "added model space {name} (writes as {})",
            aadt_model::SpaceCategory::Custom.default_layer()
        ))
    }
}

/// Parse a free-text height (`3000`, `3000 mm`, `3.0 m`, `3m`) into metres.
fn parse_height_metres(raw: &str) -> Option<f64> {
    let s = raw.trim().to_ascii_lowercase();
    if s.is_empty() {
        return None;
    }
    let cleaned: String = s
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    let n: f64 = cleaned.parse().ok()?;
    if s.contains("mm") || (!s.contains('m') && n > 30.0) {
        Some(n / 1000.0)
    } else {
        Some(n)
    }
}

pub struct CategoryCmd;
impl Command for CategoryCmd {
    fn name(&self) -> &str {
        "CATEGORY"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["CAT", "MODELSPACE", "MSPACE"]
    }
    fn help(&self) -> &str {
        "CATEGORY Walls|Structural|Lintel|BeamLayout|Furniture — set active model space on the current floor"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        ctx.doc.ensure_default_spaces();
        if args.is_empty() {
            let ms = ctx
                .doc
                .active_model_space()
                .ok_or_else(|| "no active model space".to_string())?;
            return Ok(format!(
                "active: {}  (layer shim {})",
                ms.name,
                ms.category.default_layer()
            ));
        }
        let cat = SpaceCategory::parse(&args.join(" ")).ok_or_else(|| {
            "unknown category; try Walls Structural Lintel BeamLayout Furniture".to_string()
        })?;
        ctx.doc
            .transact("CATEGORY", |d| d.set_active_category(cat))?;
        let ms = ctx.doc.active_model_space().unwrap();
        Ok(format!(
            "active model space → {} (writes as {})",
            ms.name,
            ms.category.default_layer()
        ))
    }
}

pub struct TraceCmd;
impl Command for TraceCmd {
    fn name(&self) -> &str {
        "TRACE"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["TRACEFLOOR"]
    }
    fn help(&self) -> &str {
        "TRACE BELOW|ABOVE|BOTH|OFF — ghost adjacent floors while drafting"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let mode = args
            .first()
            .map(|s| s.to_ascii_uppercase())
            .unwrap_or_else(|| "BOTH".into());
        let (below, above) = match mode.as_str() {
            "BELOW" | "DOWN" => (true, false),
            "ABOVE" | "UP" => (false, true),
            "BOTH" | "ON" => (true, true),
            "OFF" | "NONE" => (false, false),
            other => return Err(format!("TRACE {other:?}: use BELOW, ABOVE, BOTH, or OFF")),
        };
        ctx.doc.transact("TRACE", |d| d.set_trace(below, above));
        Ok(format!(
            "trace below={} above={}",
            if below { "on" } else { "off" },
            if above { "on" } else { "off" }
        ))
    }
}

pub struct OriginCmd;
impl Command for OriginCmd {
    fn name(&self) -> &str {
        "ORIGIN"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["REFPOINT", "REFORIGIN"]
    }
    fn help(&self) -> &str {
        "ORIGIN [x y] — show or set the shared building reference origin"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        if args.is_empty() {
            let o = ctx.doc.reference_origin();
            return Ok(format!("reference origin = ({:.6}, {:.6})", o.x, o.y));
        }
        if args.len() < 2 {
            return Err("ORIGIN needs x y".into());
        }
        let x: f64 = args[0]
            .parse()
            .map_err(|_| format!("bad x {:?}", args[0]))?;
        let y: f64 = args[1]
            .parse()
            .map_err(|_| format!("bad y {:?}", args[1]))?;
        ctx.doc
            .transact("ORIGIN", |d| d.set_reference_origin(Point2::new(x, y)));
        Ok(format!("reference origin → ({x:.6}, {y:.6})"))
    }
}

pub struct ChSpaceCmd;
impl Command for ChSpaceCmd {
    fn name(&self) -> &str {
        "CHSPACE"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["MOVESPACE", "TOSPACE"]
    }
    fn help(&self) -> &str {
        "CHSPACE [id…] — move selection (or ids) into the active model space"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        ctx.doc.ensure_default_spaces();
        let ids: Vec<aadt_model::EntityId> = if args.is_empty() {
            ctx.selection.clone()
        } else {
            args.iter()
                .filter_map(|s| s.parse::<u64>().ok().map(aadt_model::EntityId))
                .collect()
        };
        if ids.is_empty() {
            return Err("CHSPACE needs a selection or entity ids".into());
        }
        let space = ctx.doc.active_space();
        let name = ctx
            .doc
            .active_model_space()
            .map(|s| s.name.clone())
            .unwrap_or_else(|| "active".into());
        let mut n = 0usize;
        ctx.doc.transact("CHSPACE", |d| {
            for id in &ids {
                if d.set_entity_space(*id, space) {
                    n += 1;
                }
            }
        });
        Ok(format!("moved {n} object(s) → {name}"))
    }
}

pub struct LayerMapCmd;
impl Command for LayerMapCmd {
    fn name(&self) -> &str {
        "LAYERMAP"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["LMAP", "MAPLAYER"]
    }
    fn help(&self) -> &str {
        "LAYERMAP [layer category] | AUTO | APPLY — map layers to spaces / reassign"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        ctx.doc.ensure_default_spaces();
        if args.is_empty() {
            ctx.doc.seed_layer_map_heuristics();
            let counts = ctx.doc.layer_usage_counts();
            let mut lines = Vec::new();
            let mut layers: Vec<_> = counts.keys().cloned().collect();
            for (layer, cat) in ctx.doc.layer_space_map() {
                if !layers.iter().any(|l| l == layer) {
                    layers.push(layer.clone());
                }
                let _ = cat;
            }
            layers.sort();
            for layer in layers {
                let cat = ctx.doc.category_for_layer(&layer);
                let n = counts.get(&layer).copied().unwrap_or(0);
                lines.push(format!("{layer} → {cat}  ({n})"));
            }
            if lines.is_empty() {
                return Ok("(no layers to map)".into());
            }
            return Ok(lines.join("\n"));
        }
        let head = args[0].to_ascii_uppercase();
        if head == "AUTO" {
            ctx.doc.seed_layer_map_heuristics();
            return Ok(format!(
                "seeded layer map ({} entries)",
                ctx.doc.layer_space_map().len()
            ));
        }
        if head == "APPLY" {
            let floor = ctx
                .doc
                .active_floor_id()
                .ok_or_else(|| "no active floor".to_string())?;
            ctx.doc.reassign_by_layer_map(floor);
            return Ok(format!(
                "yanked all floors onto {} by layer map",
                ctx.doc.floor(floor).map(|f| f.name.as_str()).unwrap_or("?")
            ));
        }
        if args.len() < 2 {
            return Err("LAYERMAP needs layer and category (or AUTO|APPLY)".into());
        }
        let cat = SpaceCategory::parse(&args[args.len() - 1]).ok_or_else(|| {
            "unknown category; try Walls Structural Lintel BeamLayout Furniture".to_string()
        })?;
        let layer = args[..args.len() - 1].join(" ");
        ctx.doc.set_layer_category(layer.clone(), cat);
        Ok(format!("{layer} → {cat}"))
    }
}
