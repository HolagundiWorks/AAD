//! The AQC bridge: a JSON file ("hcw-aqc-bridge", version 1, millimetres) that carries a
//! drawing's measured data to the AQC estimation program and AQC's project details, floors and
//! beam depths back.
//!
//! `bridge.export` writes what the drawing knows: project details, floors, beam depths, the
//! opening schedule and the door and window blocks placed in the drawing, and the saved take-off
//! results for walls, columns, lintels and slabs. Take-off sections that were never run are
//! reported, not guessed. Not exported: rooms (there is no room log yet) and wall thickness
//! (the take-off measures centre lines).
//!
//! `bridge.import` reads AQC's file. It never changes geometry. Without `apply` it only reports what
//! would change; with `apply` it updates the project data. A project field already filled in is
//! left alone unless `overwrite` is set.

use aad_doc::EntityKind;
use serde_json::{Map, Value, json};

use super::project;
use super::*;
use crate::{Result, Session};

const FORMAT: &str = "hcw-aqc-bridge";
const MAX_FILE: u64 = 5_000_000;
const MAX_FLOORS: usize = 60;
const STORE: &str = "bridge";
/// AAD project detail → bridge project key.
const PROJECT_MAP: [(&str, &str); 6] = [
    ("title", "name"),
    ("owner", "client_name"),
    ("architect", "company_name"),
    ("engineer", "prepared_by_name"),
    ("location", "location"),
    ("address", "address"),
];

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("bridge.export", "Export for AQC", run_export)
            .menu(&["Architecture", "AQC Bridge", "Export for AQC"])
            .alias(&["aqcexport"])
            .params("{path?: file (default: beside the saved drawing), write?: false, level?: floor name for the measured items, bearing?: lintel bearing in mm (150)} → {json, path?, counts, warnings}"),
        CommandSpec::new("bridge.import", "Import from AQC", run_import)
            .menu(&["Architecture", "AQC Bridge", "Import from AQC"])
            .alias(&["aqcimport"])
            .params("{path | json, apply?: bool, overwrite?: bool, acceptOtherDrawing?: bool} → {items, summary, applied}"),
    ]
}

fn mm(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

fn text_of(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or("").trim().to_string()
}

// ---------------- identity and time ----------------

fn fnv(seed: u64, bytes: &[u8]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325 ^ seed;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// A new drawing id in the shape of a version-4 UUID. It only has to be unlikely to repeat.
fn new_id(salt: &str) -> String {
    #[cfg(not(target_arch = "wasm32"))]
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    #[cfg(target_arch = "wasm32")]
    let nanos = 0u128;
    let a = fnv(nanos as u64, salt.as_bytes());
    let b = fnv((nanos >> 64) as u64 ^ a, std::process::id().to_le_bytes().as_slice());
    format!("{:08x}-{:04x}-4{:03x}-8{:03x}-{:012x}", (a >> 32) as u32, (a >> 16) as u16, a & 0xfff, (b >> 52) & 0xfff, b & 0xffff_ffff_ffff)
}

fn iso_time() -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        None
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
        Some(iso_from_unix(secs))
    }
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
fn iso_from_unix(secs: u64) -> String {
    let (days, rem) = (secs / 86_400, secs % 86_400);
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

fn stored_id(d: &aad_doc::Drawing) -> Option<String> {
    project::get(d, STORE).get("drawingId").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string)
}

// ---------------- export ----------------

fn saved(d: &aad_doc::Drawing, name: &str) -> Option<Value> {
    let v = project::get(d, &format!("takeoff:{name}"));
    if v.is_null() { None } else { Some(v) }
}

/// (millimetres per length unit, square metres per area unit) of a saved take-off.
fn factors(t: &Value) -> (f64, f64) {
    let len = if t["lengthUnit"].as_str() == Some("ft") { 304.8 } else { 1000.0 };
    let area = if t["areaUnit"].as_str().is_some_and(|u| u.contains("ft")) { 0.092_903_04 } else { 1.0 };
    (len, area)
}

fn run_export(s: &mut Session, p: &Value) -> Result<Value> {
    let bearing = f64_or(p, "bearing", 150.0);
    if !(bearing.is_finite() && bearing > 0.0 && bearing < 2000.0) {
        return Err(bad("bridge.export", "`bearing` must be between 0 and 2000 mm"));
    }
    let id = match stored_id(s.doc()?) {
        Some(id) => id,
        None => {
            let id = new_id(&format!("{:?}", s.state()?.path));
            project::set(s.doc_mut()?, STORE, &json!({ "drawingId": id }));
            id
        }
    };
    let d = s.doc()?;
    let floors = project::floors(d);
    let level = str_param(p, "level")
        .map(str::to_string)
        .or_else(|| floors.first().and_then(|f| f["name"].as_str()).map(str::to_string))
        .unwrap_or_else(|| "Ground".into());
    let ceiling_mm = floors
        .iter()
        .find(|f| f["name"].as_str() == Some(level.as_str()))
        .or(floors.first())
        .and_then(|f| f["ceiling"].as_f64())
        .map_or(2850.0, |c| c * 1000.0);
    let mut warnings: Vec<String> = Vec::new();
    let mut root = Map::new();
    root.insert("format".into(), json!(FORMAT));
    root.insert("version".into(), json!(1));
    let mut source = json!({ "app": "AAD", "host": "AAD", "drawing_id": id });
    if let Some(path) = &s.state()?.path {
        source["drawing"] = json!(std::path::Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or(path));
    }
    if let Some(t) = iso_time() {
        source["exported"] = json!(t);
    }
    root.insert("source".into(), source);
    root.insert("units".into(), json!("mm"));

    // Project details.
    let details = project::get(d, "details");
    let mut proj = Map::new();
    for (ours, theirs) in PROJECT_MAP {
        let v = text_of(&details, ours);
        if !v.is_empty() {
            proj.insert(theirs.into(), json!(v));
        }
    }
    for (ours, theirs) in [("pid", "pid"), ("plotUse", "plot_use")] {
        let v = text_of(&details, ours);
        if !v.is_empty() {
            proj.insert(theirs.into(), json!(v));
        }
    }
    if let Ok(a) = text_of(&details, "siteArea").parse::<f64>()
        && a.is_finite()
    {
        proj.insert("site_area_m2".into(), json!(a));
    }
    if !proj.is_empty() {
        root.insert("project".into(), Value::Object(proj));
    }

    // Floors and beam depths.
    let levels: Vec<Value> = floors
        .iter()
        .map(|f| {
            json!({
                "name": f["name"], "height_mm": mm(f["floorToFloor"].as_f64().unwrap_or(0.0) * 1000.0),
                "slab_thickness_mm": f["slab"], "beam_depth_mm": f["beam"],
                "ceiling_mm": mm(f["ceiling"].as_f64().unwrap_or(0.0) * 1000.0), "lintel_bottom_mm": mm(f["lintel"].as_f64().unwrap_or(0.0) * 1000.0),
            })
        })
        .collect();
    root.insert("levels".into(), Value::Array(levels));
    let beams = project::get(d, "beams");
    if beams.as_array().is_some_and(|a| !a.is_empty()) {
        root.insert("beam_depths_mm".into(), beams);
    }

    // The opening schedule and the blocks placed in the drawing.
    let sched = project::get(d, "schedule");
    let entries: Vec<Value> = sched["openings"].as_array().cloned().unwrap_or_default();
    let kind_name = |k: &str| match k {
        "door" => "Door",
        "window" => "Window",
        _ => "Ventilator",
    };
    let mut counts_by_block = std::collections::BTreeMap::<String, u64>::new();
    let mut inserts = Vec::new();
    for e in d.model.iter().filter(|e| d.is_visible(e)) {
        if let EntityKind::Insert(i) = &e.kind {
            let b = i.block.to_ascii_uppercase();
            *counts_by_block.entry(b.clone()).or_insert(0) += 1;
            inserts.push(b);
        }
    }
    let entry_of = |block: &str| -> Option<&Value> {
        entries.iter().find(|e| text_of(e, "block").split(';').map(str::trim).any(|b| !b.is_empty() && b.eq_ignore_ascii_case(block)))
    };
    let mut schedule_rows = Vec::new();
    for e in &entries {
        let nos: u64 = text_of(e, "block")
            .split(';')
            .map(str::trim)
            .filter(|b| !b.is_empty())
            .map(|b| counts_by_block.get(&b.to_ascii_uppercase()).copied().unwrap_or(0))
            .sum::<u64>()
            .max(e["count"].as_u64().unwrap_or(0));
        let mut row = json!({
            "mark": text_of(e, "name"), "kind": kind_name(&text_of(e, "kind")),
            "width_mm": mm(e["length"].as_f64().unwrap_or(0.0) * 1000.0), "height_mm": mm(e["height"].as_f64().unwrap_or(0.0) * 1000.0),
            "sill_mm": mm(e["sill"].as_f64().unwrap_or(0.0) * 1000.0), "nos": nos,
        });
        if !text_of(e, "type").is_empty() {
            row["type"] = json!(text_of(e, "type"));
        }
        schedule_rows.push(row);
    }
    let mut opening_rows = Vec::new();
    for b in &inserts {
        let Some(e) = entry_of(b) else { continue };
        let mut row = json!({
            "ref": format!("O{:04}", opening_rows.len() + 1), "kind": kind_name(&text_of(e, "kind")), "mark": text_of(e, "name"), "level": level,
            "width_mm": mm(e["length"].as_f64().unwrap_or(0.0) * 1000.0), "height_mm": mm(e["height"].as_f64().unwrap_or(0.0) * 1000.0),
            "sill_mm": mm(e["sill"].as_f64().unwrap_or(0.0) * 1000.0),
        });
        if !text_of(e, "type").is_empty() {
            row["type"] = json!(text_of(e, "type"));
        }
        opening_rows.push(row);
    }
    if schedule_rows.is_empty() {
        warnings.push("no opening schedule: run `openings.std` and place doors and windows to include openings".into());
    }
    root.insert("opening_schedule".into(), Value::Array(schedule_rows.clone()));
    root.insert("openings".into(), Value::Array(opening_rows.clone()));

    // Walls from the brick take-off.
    let mut walls = Vec::new();
    match saved(d, "Bricks") {
        Some(t) => {
            let (lf, _) = factors(&t);
            for it in t["items"].as_array().into_iter().flatten() {
                let (Some(mark), Some(len)) = (it["mark"].as_str(), it["length"].as_f64()) else { continue };
                let layer = if it["group"].as_str().is_some_and(|g| g.starts_with("HB")) { "MEASURE-HALFBRICK" } else { "MEASURE-FULLBRICK" };
                walls.push(json!({ "ref": mark, "level": level, "length_mm": mm(len * lf), "height_mm": mm(ceiling_mm), "layer": layer }));
            }
        }
        None => warnings.push("no saved brick take-off: run `takeoff.brick` to include walls".into()),
    }
    root.insert("walls".into(), Value::Array(walls.clone()));

    // Columns.
    let mut columns = Vec::new();
    match saved(d, "Columns") {
        Some(t) => {
            let (lf, _) = factors(&t);
            for (n, it) in t["items"].as_array().into_iter().flatten().enumerate() {
                let (Some(mark), Some(l), Some(b)) = (it["mark"].as_str(), it["length"].as_f64(), it["breadth"].as_f64()) else { continue };
                let (w, dep) = (l.min(b) * lf, l.max(b) * lf);
                columns.push(json!({ "ref": format!("C{:04}", n + 1), "mark": mark, "level": level, "width_mm": mm(w), "depth_mm": mm(dep) }));
            }
        }
        None => warnings.push("no saved column take-off: run `takeoff.columns` to include columns".into()),
    }
    root.insert("columns".into(), Value::Array(columns.clone()));

    // Lintels: the take-off group letters become marks L1, L2 ...
    let mut lintels = Vec::new();
    match saved(d, "Beams") {
        Some(t) => {
            let (lf, _) = factors(&t);
            let mut groups: Vec<String> = Vec::new();
            for it in t["items"].as_array().into_iter().flatten() {
                let (Some(group), Some(len)) = (it["group"].as_str(), it["length"].as_f64()) else { continue };
                if !group.starts_with("LT") {
                    continue;
                }
                let g = groups.iter().position(|x| x == group).unwrap_or_else(|| {
                    groups.push(group.to_string());
                    groups.len() - 1
                });
                lintels.push(json!({
                    "ref": format!("L{:04}", lintels.len() + 1), "mark": format!("L{}", g + 1), "level": level,
                    "opening_mm": mm((len * lf - 2.0 * bearing).max(0.0)), "bearing_mm": bearing,
                }));
            }
        }
        None => warnings.push("no saved beam and lintel take-off: run `takeoff.beams` to include lintels".into()),
    }
    root.insert("lintels".into(), Value::Array(lintels.clone()));

    // Slabs.
    let slab_mm =
        floors.iter().find(|f| f["name"].as_str() == Some(level.as_str())).or(floors.first()).and_then(|f| f["slab"].as_f64()).unwrap_or(150.0);
    let mut slabs = Vec::new();
    match saved(d, "Slab") {
        Some(t) => {
            let (_, af) = factors(&t);
            for (n, it) in t["items"].as_array().into_iter().flatten().enumerate() {
                let Some(gross) = it["gross"].as_f64() else { continue };
                slabs.push(json!({
                    "ref": format!("S{:04}", n + 1), "level": level, "area_m2": (gross * af * 1000.0).round() / 1000.0, "thickness_mm": slab_mm,
                }));
            }
        }
        None => warnings.push("no saved slab take-off: run `takeoff.slab` to include slabs".into()),
    }
    root.insert("slabs".into(), Value::Array(slabs.clone()));
    warnings.push("rooms are not exported: AAD has no room log yet".into());

    let text = serde_json::to_string_pretty(&Value::Object(root)).map_err(|e| bad("bridge.export", e.to_string()))?;
    #[cfg_attr(target_arch = "wasm32", allow(unused_mut))]
    let mut out = json!({
        "json": text,
        "drawingId": id,
        "counts": { "walls": walls.len(), "openings": opening_rows.len(), "openingSchedule": schedule_rows.len(), "columns": columns.len(), "lintels": lintels.len(), "slabs": slabs.len() },
        "warnings": warnings,
    });
    #[cfg(target_arch = "wasm32")]
    {
        if str_param(p, "path").is_some() {
            return Err(bad("bridge.export", "files cannot be written on the web; read `json` from the result"));
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        // Beside the saved drawing (`house.dxf` → `house.aqc.json`) unless `path` says otherwise or
        // `write` is false.
        let target = match str_param(p, "path") {
            Some(t) => Some(std::path::PathBuf::from(t)),
            None if bool_or(p, "write", true) => s.state()?.path.as_ref().map(|b| std::path::Path::new(b).with_extension("aqc.json")),
            None => None,
        };
        if let Some(path) = target {
            std::fs::write(&path, out["json"].as_str().unwrap_or(""))
                .map_err(|e| bad("bridge.export", format!("cannot write `{}`: {e}", path.display())))?;
            out["path"] = json!(path.display().to_string());
            out["message"] = json!(format!("Exported for AQC: {}", path.display()));
        }
    }
    Ok(out)
}

// ---------------- import ----------------

struct Plan {
    items: Vec<Value>,
}

impl Plan {
    fn add(&mut self, section: &str, action: &str, field: &str, from: Value, to: Value, reason: &str) {
        self.items.push(json!({ "section": section, "action": action, "field": field, "from": from, "to": to, "reason": reason }));
    }
    fn count(&self, action: &str) -> usize {
        self.items.iter().filter(|i| i["action"] == action).count()
    }
}

fn read_input(p: &Value) -> Result<String> {
    if let Some(j) = str_param(p, "json") {
        if j.len() as u64 > MAX_FILE {
            return Err(bad("bridge.import", "the bridge text is too large"));
        }
        return Ok(j.to_string());
    }
    let Some(path) = str_param(p, "path") else {
        return Err(bad("bridge.import", "give `path` (an AQC export file) or `json`"));
    };
    #[cfg(target_arch = "wasm32")]
    {
        let _ = path;
        Err(bad("bridge.import", "files cannot be read on the web; pass the text as `json`"))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let meta = std::fs::metadata(path).map_err(|e| bad("bridge.import", format!("cannot read `{path}`: {e}")))?;
        if meta.len() > MAX_FILE {
            return Err(bad("bridge.import", "the file is too large for a bridge file"));
        }
        std::fs::read_to_string(path).map_err(|e| bad("bridge.import", format!("cannot read `{path}`: {e}")))
    }
}

fn pos(v: &Value, k: &str) -> Option<f64> {
    v.get(k).and_then(Value::as_f64).filter(|x| x.is_finite() && *x > 0.0)
}

fn run_import(s: &mut Session, p: &Value) -> Result<Value> {
    let src = read_input(p)?;
    let root: Value = serde_json::from_str(&src).map_err(|e| bad("bridge.import", format!("not valid JSON: {e}")))?;
    if root["format"].as_str() != Some(FORMAT) {
        return Err(bad("bridge.import", "not an hcw-aqc-bridge file"));
    }
    if root["version"].as_f64() != Some(1.0) {
        return Err(bad("bridge.import", "unsupported bridge version (this reads version 1)"));
    }
    if !root["units"].as_str().is_some_and(|u| u.eq_ignore_ascii_case("mm")) {
        return Err(bad("bridge.import", "unsupported units (expected mm)"));
    }
    let file_id = root["source"]["drawing_id"]
        .as_str()
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .ok_or_else(|| bad("bridge.import", "source.drawing_id is missing"))?
        .to_string();
    let apply = bool_or(p, "apply", false);
    let overwrite = bool_or(p, "overwrite", false);
    let known = stored_id(s.doc()?);
    let other = known.as_ref().is_some_and(|k| *k != file_id);
    if apply && other && !bool_or(p, "acceptOtherDrawing", false) {
        return Err(bad("bridge.import", "this file came from a different drawing; check it, then set `acceptOtherDrawing`"));
    }

    let d = s.doc()?;
    let mut plan = Plan { items: Vec::new() };

    // Project details.
    let mut details = match project::get(d, "details") {
        Value::Object(o) => o,
        _ => Map::new(),
    };
    if let Some(proj) = root["project"].as_object() {
        for (ours, theirs) in PROJECT_MAP {
            let Some(new) = proj.get(theirs).and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty()) else { continue };
            if new.len() > 500 {
                plan.add("project", "flagged", ours, Value::Null, json!(new.len()), "too long, skipped");
                continue;
            }
            let old = details.get(ours).and_then(Value::as_str).unwrap_or("").to_string();
            if old == new {
                plan.add("project", "unchanged", ours, json!(old), json!(new), "");
            } else if old.is_empty() {
                plan.add("project", "add", ours, Value::Null, json!(new), "");
                details.insert(ours.into(), json!(new));
            } else if overwrite {
                plan.add("project", "update", ours, json!(old), json!(new), "overwritten");
                details.insert(ours.into(), json!(new));
            } else {
                plan.add("project", "flagged", ours, json!(old), json!(new), "already filled in AAD; set `overwrite` to replace it");
            }
        }
    }

    // Floors.
    let mut floors = project::floors(d);
    if let Some(levels) = root["levels"].as_array() {
        for l in levels.iter().take(MAX_FLOORS) {
            let name = text_of(l, "name");
            if name.is_empty() || name.chars().count() > 60 {
                plan.add("floors", "flagged", &name, Value::Null, Value::Null, "needs a name of up to 60 characters");
                continue;
            }
            let (Some(h), slab, beam) = (pos(l, "height_mm").filter(|h| *h <= 20_000.0), pos(l, "slab_thickness_mm"), pos(l, "beam_depth_mm")) else {
                plan.add("floors", "flagged", &name, Value::Null, Value::Null, "height must be between 0 and 20 000 mm");
                continue;
            };
            if slab.is_some_and(|x| x > 1000.0) || beam.is_some_and(|x| x > 3000.0) {
                plan.add("floors", "flagged", &name, Value::Null, Value::Null, "slab or beam depth is out of range");
                continue;
            }
            let height = h / 1000.0;
            match floors.iter().position(|f| f["name"].as_str().is_some_and(|n| n.eq_ignore_ascii_case(&name))) {
                Some(i) => {
                    let ceiling = floors[i]["ceiling"].as_f64().unwrap_or(0.0);
                    if height <= ceiling {
                        plan.add(
                            "floors",
                            "flagged",
                            &name,
                            floors[i]["floorToFloor"].clone(),
                            json!(height),
                            "floor-to-floor height would be below the ceiling height",
                        );
                        continue;
                    }
                    let mut next = floors[i].clone();
                    next["floorToFloor"] = json!(height);
                    if let Some(x) = slab {
                        next["slab"] = json!(x);
                    }
                    if let Some(x) = beam {
                        next["beam"] = json!(x);
                    }
                    if next == floors[i] {
                        plan.add("floors", "unchanged", &name, floors[i]["floorToFloor"].clone(), json!(height), "");
                    } else {
                        plan.add("floors", "update", &name, floors[i]["floorToFloor"].clone(), json!(height), "");
                        floors[i] = next;
                    }
                }
                None => {
                    if floors.len() >= MAX_FLOORS {
                        plan.add("floors", "flagged", &name, Value::Null, json!(height), "too many floors");
                        continue;
                    }
                    let slab_m = slab.unwrap_or(150.0) / 1000.0;
                    let ceiling = ((height - slab_m) * 100.0).round() / 100.0;
                    floors.push(json!({ "name": name, "floorToFloor": height, "ceiling": ceiling, "lintel": ceiling.min(2.1), "slab": slab.unwrap_or(150.0), "beam": beam.unwrap_or(450.0) }));
                    plan.add("floors", "add", &name, Value::Null, json!(height), "");
                }
            }
        }
    }

    // Beam depths.
    let mut beams: Option<Vec<Value>> = None;
    if let Some(list) = root["beam_depths_mm"].as_array() {
        let depths: Vec<f64> = list.iter().filter_map(Value::as_f64).filter(|x| x.is_finite() && *x > 0.0 && *x <= 3000.0).take(50).collect();
        if depths.is_empty() {
            plan.add("beams", "flagged", "beam_depths_mm", Value::Null, Value::Null, "no valid depths");
        } else {
            let new: Vec<Value> = depths.iter().map(|x| json!(x)).collect();
            let old = project::get(d, "beams");
            if old == Value::Array(new.clone()) {
                plan.add("beams", "unchanged", "beam_depths_mm", old, Value::Array(new), "");
            } else {
                plan.add("beams", "update", "beam_depths_mm", old, Value::Array(new.clone()), "replaces the saved depths");
                beams = Some(new);
            }
        }
    }

    // Geometry in the file is never applied.
    let ignored: Vec<&str> = ["walls", "openings", "columns", "lintels", "rooms", "slabs"]
        .into_iter()
        .filter(|k| root[*k].as_array().is_some_and(|a| !a.is_empty()))
        .collect();
    if !ignored.is_empty() {
        plan.add("geometry", "skipped", &ignored.join(", "), Value::Null, Value::Null, "measured data is not applied to the drawing");
    }

    let changes = plan.count("add") + plan.count("update");
    let mut applied = false;
    if apply && (changes > 0 || known.is_none()) {
        let dm = s.doc_mut()?;
        project::set(dm, "details", &Value::Object(details));
        project::set(dm, "floors", &Value::Array(floors));
        if let Some(b) = beams {
            project::set(dm, "beams", &Value::Array(b));
        }
        if known.is_none() {
            project::set(dm, STORE, &json!({ "drawingId": file_id }));
        }
        applied = true;
    }
    let summary = format!(
        "{} to add, {} to update, {} unchanged, {} flagged, {} skipped",
        plan.count("add"),
        plan.count("update"),
        plan.count("unchanged"),
        plan.count("flagged"),
        plan.count("skipped")
    );
    Ok(json!({
        "drawingId": file_id, "otherDrawing": other, "items": plan.items, "summary": summary, "applied": applied,
        "message": if applied { format!("Applied from AQC: {summary}") } else { format!("AQC import preview (nothing changed): {summary}") },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_time_matches_known_dates() {
        assert_eq!(iso_from_unix(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso_from_unix(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(iso_from_unix(1_700_000_000), "2023-11-14T22:13:20Z");
    }

    #[test]
    fn ids_have_the_shape_of_a_uuid() {
        let id = new_id("x");
        assert_eq!(id.len(), 36, "{id}");
        assert_eq!(id.matches('-').count(), 4);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit() || c == '-'));
    }
}
