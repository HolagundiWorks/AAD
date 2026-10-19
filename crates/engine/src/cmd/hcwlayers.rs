//! HCW layer-standard tools carried over from the Holgundi Consulting Works AutoLISP toolkit
//! (`hcwtools.lsp`, section 7 and 10): HCWRESET, HCWINFO, HCWLOCK / HCWUNLOCK, HCWAUDIT2,
//! HCWBYBLOCK, HCWMOVE, HCWSCHEDULE and HCWLEGEND, working on the 36-layer HCW standard that
//! `hcwlayers` creates. Purge, audit and layer states already exist as `purge`, `audit` and
//! `layerstate.*`.

use aad_color::Color;
use aad_doc::{HAlign, Handle, Lineweight, VAlign};
use aad_geom::Vec2;
use serde_json::{Value, json};

use super::draw::make_text;
use super::hcw::{HCW_LAYERS, add_on, drawing_units, ensure_layer};
use super::helpers::{line, lwpoly, rect_vertices};
use super::*;
use crate::{Result, Session};

const VERSION: &str = "4.0";
/// Objects one call will look at; input-sized loops stay bounded.
const MAX_OBJECTS: usize = 5_000_000;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("hcwreset", "Reset HCW Layers", run_reset)
            .menu(&["Architecture", "HCW Layers", "Reset to Standard"])
            .params("{} → {created, updated} (creates missing HCW layers and sets colour, linetype, lineweight and description of existing ones to the standard; on, frozen, locked and plot are left alone)"),
        CommandSpec::new("hcwinfo", "HCW Layer Table", run_info)
            .menu(&["Architecture", "HCW Layers", "Layer Table"])
            .params("{} → {version, layers: [{name, aci, linetype, lineweight (mm), description, exists}]}")
            .noundo(),
        CommandSpec::new("hcwlock", "Lock Layers by Prefix", |s, p| run_lock(s, p, true))
            .menu(&["Architecture", "HCW Layers", "Lock by Prefix"])
            .params("{prefix: e.g. \"E-\"} → {changed}"),
        CommandSpec::new("hcwunlock", "Unlock Layers by Prefix", |s, p| run_lock(s, p, false))
            .menu(&["Architecture", "HCW Layers", "Unlock by Prefix"])
            .params("{prefix} → {changed}"),
        CommandSpec::new("hcwaudit2", "Find Non-Standard Layers", run_audit2)
            .menu(&["Architecture", "HCW Layers", "Find Non-Standard Layers"])
            .params("{moveTo?: an existing layer} → {layers: [{layer, count}], total, moved} (objects, in all spaces, that are not on one of the 36 HCW layers; with `moveTo` they all move to that layer)"),
        CommandSpec::new("hcwbyblock", "Find Property Overrides", run_overrides)
            .menu(&["Architecture", "HCW Layers", "Find Property Overrides"])
            .params("{} → {count, layers: [{layer, count}]} (objects with an explicit colour, linetype or lineweight instead of ByLayer)")
            .noundo(),
        CommandSpec::new("hcwmove", "Move to HCW Layer", run_move)
            .menu(&["Architecture", "HCW Layers", "Move to HCW Layer"])
            .params("{handles?, layer: a layer name, or the number (from 0) of an entry in the HCW table} → {moved, layer}"),
        CommandSpec::new("hcwschedule", "HCW Layer Schedule", run_schedule)
            .menu(&["Architecture", "HCW Layers", "Layer Schedule (CSV)"])
            .params("{path?} → {count, csv?, path?} (Layer, ACI, Linetype, LW_mm, Description, InDrawing, ObjectCount for the 36 HCW layers)")
            .noundo(),
        CommandSpec::new("hcwlegend", "HCW Layer Legend", run_legend)
            .menu(&["Architecture", "HCW Layers", "Layer Legend"])
            .params("{at: [x, y] (bottom-left), units?: mm|cm|m|ft|in} → {rows} (swatch, line and name of each HCW layer in two columns, sized as for metres at 1:1)"),
    ]
}

fn run_reset(s: &mut Session, _p: &Value) -> Result<Value> {
    let d = s.doc_mut()?;
    let (mut created, mut updated) = (0, 0);
    for (name, aci, linetype, lw, description) in HCW_LAYERS {
        if ensure_layer(d, name, *aci, linetype, *lw, description) {
            created += 1;
            continue;
        }
        // The layer exists: restore the standard look, keep its on/frozen/locked/plot state.
        let linetype = d.linetype(linetype).map(|t| t.name.clone()).unwrap_or_else(|| "Continuous".into());
        if let Some(l) = d.layers.iter_mut().find(|l| l.name == *name) {
            let (color, weight) = (Color::from_aci(*aci), Lineweight::Mm100(*lw));
            if l.color != color || l.linetype != linetype || l.lineweight != weight || l.description != *description {
                l.color = color;
                l.linetype = linetype;
                l.lineweight = weight;
                l.description = (*description).into();
                updated += 1;
            }
        }
    }
    Ok(json!({ "created": created, "updated": updated }))
}

fn run_info(s: &mut Session, _p: &Value) -> Result<Value> {
    let d = s.doc()?;
    let layers: Vec<Value> = HCW_LAYERS
        .iter()
        .map(|(name, aci, lt, lw, desc)| {
            json!({ "name": name, "aci": aci, "linetype": lt, "lineweight": f64::from(*lw) / 100.0, "description": desc, "exists": d.layer(name).is_some() })
        })
        .collect();
    Ok(json!({ "version": VERSION, "layers": layers }))
}

fn run_lock(s: &mut Session, p: &Value, lock: bool) -> Result<Value> {
    let cmd = if lock { "hcwlock" } else { "hcwunlock" };
    let prefix = str_param(p, "prefix").filter(|v| !v.is_empty()).ok_or_else(|| bad(cmd, "`prefix` is required, e.g. \"E-\""))?;
    let mut changed = 0usize;
    for l in s.doc_mut()?.layers.iter_mut().filter(|l| l.name.starts_with(prefix)) {
        if l.locked != lock {
            l.locked = lock;
        }
        changed += 1;
    }
    Ok(json!({ "changed": changed }))
}

/// Every object in every space (model space and layouts), not block definitions.
fn all_objects(s: &Session) -> Result<Vec<Handle>> {
    let d = s.doc()?;
    let mut out: Vec<Handle> = d.model.iter().map(|e| e.handle).collect();
    for l in &d.layouts {
        out.extend(l.entities.iter().map(|e| e.handle));
    }
    out.truncate(MAX_OBJECTS);
    Ok(out)
}

fn per_layer(counts: std::collections::BTreeMap<String, usize>) -> Vec<Value> {
    counts.into_iter().map(|(layer, count)| json!({ "layer": layer, "count": count })).collect()
}

fn run_audit2(s: &mut Session, p: &Value) -> Result<Value> {
    let hs = all_objects(s)?;
    let is_standard = |name: &str| HCW_LAYERS.iter().any(|l| l.0 == name);
    let d = s.doc()?;
    let mut rogue: Vec<Handle> = Vec::new();
    let mut counts = std::collections::BTreeMap::new();
    for e in hs.iter().filter_map(|h| d.entity(*h)) {
        if !is_standard(&e.common.layer) {
            *counts.entry(e.common.layer.clone()).or_insert(0usize) += 1;
            rogue.push(e.handle);
        }
    }
    let mut moved = 0usize;
    if let Some(target) = str_param(p, "moveTo") {
        if s.doc()?.layer(target).is_none() {
            return Err(bad("hcwaudit2", format!("no layer \"{target}\"; nothing was moved")));
        }
        for h in rogue.iter().filter(|h| !curves::is_locked(s, **h)).copied().collect::<Vec<_>>() {
            s.doc_mut()?.modify_entity(h, |e| e.common.layer = target.to_string())?;
            moved += 1;
        }
    }
    if !rogue.is_empty() {
        s.echo(format!("{} object(s) on {} non-standard layer(s).", rogue.len(), counts.len()));
    }
    Ok(json!({ "layers": per_layer(counts), "total": rogue.len(), "moved": moved }))
}

fn run_overrides(s: &mut Session, _p: &Value) -> Result<Value> {
    let hs = all_objects(s)?;
    let d = s.doc()?;
    let mut counts = std::collections::BTreeMap::new();
    let mut count = 0usize;
    for e in hs.iter().filter_map(|h| d.entity(*h)) {
        let c = &e.common;
        let colour = matches!(c.color, Color::Index(_) | Color::True(_));
        let linetype = !["", "bylayer", "byblock"].contains(&c.linetype.to_ascii_lowercase().as_str());
        let weight = matches!(c.lineweight, Lineweight::Mm100(_));
        if colour || linetype || weight {
            *counts.entry(c.layer.clone()).or_insert(0usize) += 1;
            count += 1;
        }
    }
    Ok(json!({ "count": count, "layers": per_layer(counts) }))
}

fn run_move(s: &mut Session, p: &Value) -> Result<Value> {
    let target = match p.get("layer") {
        Some(Value::String(name)) => match name.parse::<usize>() {
            Ok(i) => HCW_LAYERS.get(i).map(|l| l.0.to_string()).ok_or_else(|| bad("hcwmove", "layer number out of range"))?,
            Err(_) => name.clone(),
        },
        Some(Value::Number(n)) => {
            let i = n.as_u64().and_then(|i| usize::try_from(i).ok());
            i.and_then(|i| HCW_LAYERS.get(i)).map(|l| l.0.to_string()).ok_or_else(|| bad("hcwmove", "layer number out of range"))?
        }
        _ => return Err(bad("hcwmove", "`layer` is required")),
    };
    if s.doc()?.layer(&target).is_none() {
        return Err(bad("hcwmove", format!("no layer \"{target}\"; create the HCW layers with hcwlayers")));
    }
    let hs: Vec<Handle> = targets(s, p)?.into_iter().filter(|h| !curves::is_locked(s, *h)).collect();
    if hs.is_empty() {
        return Err(bad("hcwmove", "select objects that are not on locked layers"));
    }
    let d = s.doc_mut()?;
    let mut moved = 0usize;
    for h in hs {
        if d.modify_entity(h, |e| e.common.layer = target.clone()).is_ok() {
            moved += 1;
        }
    }
    Ok(json!({ "moved": moved, "layer": target }))
}

fn csv_field(v: &str) -> String {
    if v.contains([',', '"', '\n', '\r']) { format!("\"{}\"", v.replace('"', "\"\"")) } else { v.to_string() }
}

fn run_schedule(s: &mut Session, p: &Value) -> Result<Value> {
    let hs = all_objects(s)?;
    let d = s.doc()?;
    let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for e in hs.iter().filter_map(|h| d.entity(*h)) {
        *counts.entry(e.common.layer.as_str()).or_insert(0) += 1;
    }
    let mut csv = String::from("Layer,ACI,Linetype,LW_mm,Description,InDrawing,ObjectCount\n");
    for (name, aci, lt, lw, desc) in HCW_LAYERS {
        let exists = if d.layer(name).is_some() { "Yes" } else { "No" };
        csv.push_str(&format!(
            "{name},{aci},{lt},{:.2},{},{exists},{}\n",
            f64::from(*lw) / 100.0,
            csv_field(desc),
            counts.get(name).copied().unwrap_or(0)
        ));
    }
    match str_param(p, "path") {
        Some(_path) => {
            #[cfg(not(target_arch = "wasm32"))]
            {
                std::fs::write(_path, &csv).map_err(|e| bad("hcwschedule", format!("{_path}: {e}")))?;
                Ok(json!({ "count": HCW_LAYERS.len(), "path": _path }))
            }
            #[cfg(target_arch = "wasm32")]
            Err(bad("hcwschedule", "paths are not available on the web; omit `path` to get the CSV text"))
        }
        None => Ok(json!({ "count": HCW_LAYERS.len(), "csv": csv })),
    }
}

/// Draw one legend row at `(x, y)`: a swatch rectangle and a centre line on the layer, and the
/// layer name with its description beside them.
fn legend_row(s: &mut Session, name: &str, description: &str, at: Vec2, k: f64) -> Result<()> {
    let (swatch_w, swatch_h, text_h, text_dx) = (1.6 * k, 0.55 * k, 0.3 * k, 0.25 * k);
    add_on(s, name, lwpoly(rect_vertices(at, Vec2::new(at.x + swatch_w, at.y + swatch_h)), true))?;
    let mid = at.y + swatch_h / 2.0;
    add_on(s, name, line(Vec2::new(at.x, mid), Vec2::new(at.x + swatch_w, mid)))?;
    let label = make_text(
        s,
        Vec2::new(at.x + swatch_w + text_dx, at.y + (swatch_h - text_h) / 2.0),
        &format!("{name}  {description}"),
        text_h,
        0.0,
        (HAlign::Left, VAlign::Baseline),
    );
    add_on(s, "AN-TEXT", label)?;
    Ok(())
}

fn run_legend(s: &mut Session, p: &Value) -> Result<Value> {
    let at = point_req("hcwlegend", p, "at")?;
    // The legend is laid out in metres; scale it to the drawing's units.
    let k = 1.0 / drawing_units(s.doc()?, "hcwlegend", p)?.metres();
    {
        let d = s.doc_mut()?;
        for (name, aci, lt, lw, desc) in HCW_LAYERS {
            ensure_layer(d, name, *aci, lt, *lw, desc);
        }
    }
    let first = |name: &str| name.starts_with("AN-") || name.starts_with("A-") || name.starts_with("I-");
    let left: Vec<_> = HCW_LAYERS.iter().filter(|l| first(l.0)).collect();
    let right: Vec<_> = HCW_LAYERS.iter().filter(|l| !first(l.0)).collect();
    let (row_h, col_gap) = (0.8 * k, 18.0 * k);
    let rows = left.len().max(right.len());
    let header_y = at.y + rows as f64 * row_h + row_h;
    let title_h = 0.3 * k * 1.4;
    for (title, x) in [("HCW LAYER STANDARD v4.0 - AN / A / I", at.x), ("HCW LAYER STANDARD v4.0 - E / P / S / PR", at.x + col_gap)] {
        let t = make_text(s, Vec2::new(x, header_y), title, title_h, 0.0, (HAlign::Left, VAlign::Baseline));
        add_on(s, "AN-TEXT", t)?;
    }
    for (column, x) in [(&left, at.x), (&right, at.x + col_gap)] {
        for (r, layer) in column.iter().enumerate() {
            legend_row(s, layer.0, layer.4, Vec2::new(x, at.y + (column.len() - 1 - r) as f64 * row_h), k)?;
        }
    }
    let (x0, y0) = (at.x - 0.5 * k, at.y - 0.5 * k);
    let (x1, y1) = (at.x + col_gap + 18.0 * k + 0.5 * k, header_y + row_h);
    add_on(s, "AN-REF", lwpoly(rect_vertices(Vec2::new(x0, y0), Vec2::new(x1, y1)), true))?;
    add_on(s, "AN-REF", line(Vec2::new(at.x + col_gap - 0.3 * k, y0 + 0.2 * k), Vec2::new(at.x + col_gap - 0.3 * k, y1 - 0.2 * k)))?;
    Ok(json!({ "rows": left.len() + right.len() }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> Session {
        let mut s = Session::new();
        s.execute("hcwlayers", &json!({})).unwrap();
        s
    }

    fn line_on(s: &mut Session, layer: &str) -> String {
        let r = s.execute("line", &json!({ "points": [[0, 0], [10, 0]] })).unwrap();
        let h = r["handles"][0].as_str().unwrap().to_string();
        let d = s.doc_mut().unwrap();
        d.ensure_layer(layer);
        d.modify_entity(Handle::parse_hex(&h).unwrap(), |e| e.common.layer = layer.to_string()).unwrap();
        h
    }

    #[test]
    fn info_lists_the_36_standard_layers() {
        let mut s = session();
        let r = s.execute("hcwinfo", &json!({})).unwrap();
        assert_eq!(r["layers"].as_array().unwrap().len(), 36);
        assert_eq!(r["version"], json!("4.0"));
        assert!(r["layers"].as_array().unwrap().iter().all(|l| l["exists"] == json!(true)));
    }

    #[test]
    fn reset_repairs_changed_layers_and_keeps_their_state() {
        let mut s = session();
        {
            let l = s.doc_mut().unwrap().layers.iter_mut().find(|l| l.name == "A-WALL").unwrap();
            l.color = Color::from_aci(5);
            l.locked = true;
        }
        s.doc_mut().unwrap().layers.retain(|l| l.name != "S-TREE");
        let r = s.execute("hcwreset", &json!({})).unwrap();
        assert_eq!(r["created"], json!(1));
        assert_eq!(r["updated"], json!(1));
        let wall = s.doc().unwrap().layer("A-WALL").unwrap();
        assert_eq!(wall.color, Color::from_aci(1));
        assert!(wall.locked, "the lock stays");
        let again = s.execute("hcwreset", &json!({})).unwrap();
        assert_eq!((again["created"].clone(), again["updated"].clone()), (json!(0), json!(0)));
    }

    #[test]
    fn lock_by_prefix_needs_a_prefix_and_counts_layers() {
        let mut s = session();
        assert!(s.execute("hcwlock", &json!({})).is_err());
        assert!(s.execute("hcwlock", &json!({ "prefix": "" })).is_err());
        let r = s.execute("hcwlock", &json!({ "prefix": "E-" })).unwrap();
        assert_eq!(r["changed"], json!(3));
        assert!(s.doc().unwrap().layer("E-WIRE").unwrap().locked);
        s.execute("hcwunlock", &json!({ "prefix": "E-" })).unwrap();
        assert!(!s.doc().unwrap().layer("E-WIRE").unwrap().locked);
    }

    #[test]
    fn audit2_finds_objects_off_the_standard_and_moves_them_on_request() {
        let mut s = session();
        line_on(&mut s, "A-WALL");
        line_on(&mut s, "MY-LAYER");
        line_on(&mut s, "MY-LAYER");
        let r = s.execute("hcwaudit2", &json!({})).unwrap();
        assert_eq!(r["total"], json!(2));
        assert_eq!(r["layers"][0]["layer"], json!("MY-LAYER"));
        assert_eq!(r["moved"], json!(0));
        assert!(s.execute("hcwaudit2", &json!({ "moveTo": "NOPE" })).is_err());
        let r = s.execute("hcwaudit2", &json!({ "moveTo": "A-WALL" })).unwrap();
        assert_eq!(r["moved"], json!(2));
        assert_eq!(s.execute("hcwaudit2", &json!({})).unwrap()["total"], json!(0));
    }

    #[test]
    fn move_takes_a_name_or_a_table_number() {
        let mut s = session();
        let h = line_on(&mut s, "A-WALL");
        let r = s.execute("hcwmove", &json!({ "handles": [h], "layer": "6" })).unwrap();
        assert_eq!(r["layer"], json!("A-WALL"));
        assert!(s.execute("hcwmove", &json!({ "handles": [h], "layer": "99" })).is_err());
        assert!(s.execute("hcwmove", &json!({ "handles": [h], "layer": "UNKNOWN" })).is_err());
        assert!(s.execute("hcwmove", &json!({ "handles": [h] })).is_err());
        let r = s.execute("hcwmove", &json!({ "handles": [h], "layer": 0 })).unwrap();
        assert_eq!(r["layer"], json!("AN-REF"));
    }

    #[test]
    fn overrides_are_counted_per_layer() {
        let mut s = session();
        let h = line_on(&mut s, "A-WALL");
        line_on(&mut s, "A-DOOR");
        let hd = Handle::parse_hex(&h).unwrap();
        s.doc_mut().unwrap().modify_entity(hd, |e| e.common.color = Color::Index(1)).unwrap();
        let r = s.execute("hcwbyblock", &json!({})).unwrap();
        assert_eq!(r["count"], json!(1));
        assert_eq!(r["layers"][0]["layer"], json!("A-WALL"));
    }

    #[test]
    fn schedule_counts_objects_per_layer() {
        let mut s = session();
        line_on(&mut s, "A-WALL");
        line_on(&mut s, "A-WALL");
        let r = s.execute("hcwschedule", &json!({})).unwrap();
        let csv = r["csv"].as_str().unwrap();
        assert!(csv.starts_with("Layer,ACI,Linetype,LW_mm,Description,InDrawing,ObjectCount\n"));
        assert!(csv.contains("A-WALL,1,Continuous,0.35,Walls - new and existing (cut),Yes,2"));
        assert_eq!(csv.lines().count(), 37);
    }

    #[test]
    fn legend_draws_every_layer_once_and_needs_a_point() {
        let mut s = Session::new();
        assert!(s.execute("hcwlegend", &json!({})).is_err());
        let before = s.doc().unwrap().model.iter().count();
        let r = s.execute("hcwlegend", &json!({ "at": [0, 0], "units": "m" })).unwrap();
        assert_eq!(r["rows"], json!(36));
        let added = s.doc().unwrap().model.iter().count() - before;
        // per row: swatch + line + label; plus two titles, a border and a divider
        assert_eq!(added, 36 * 3 + 2 + 2);
        assert!(s.doc().unwrap().layer("A-WALL").is_some(), "the layers are created so the rows can use them");
    }
}
