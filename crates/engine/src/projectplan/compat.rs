//! The compatibility manifest `project.aad.json`: a projection of `project.json` for the floor
//! commands (`floor.activate`, `floor.set`, `plot.batch`...), which still read the older, name-keyed
//! format. `project.json` is the one source of truth. This file is generated from it, carries a
//! `compat` block saying so (with a hash of what it was made from), is never edited by hand, and is
//! checked against `project.json` ([`check`]) so a stale or damaged copy is found.
//!
//! The one thing in the old file that is not configuration is the per-floor and per-layout state the
//! floor commands change at run time (`visible`, `frozen`, `locked`, `print`, and `active`). That
//! state lives only here, and regenerating the file keeps it for every floor and layout that still
//! exists; it is not part of the consistency check.
//!
//! Mapping from the ID-based model:
//!
//! | `project.json` | `project.aad.json` |
//! |---|---|
//! | `project.id`, `name`, `type`, `client.name`, `address.city`, `address.line1` | `id`, `name`, `type`, `client`, `location`, `address` |
//! | `project.units.length` (`mm`, `m`) | `units: metric`, `drawingUnits`, `lengthUnit: m` |
//! | floor `code` | floor `key` (and the Xref name) |
//! | floor `name`, `sequence`, resolved elevation, `floor_to_floor` | `name`, `sequence`, `elevation`, `height` (metres; `null` when no height is given) |
//! | floor `kind` | `kind`: basement; terrace (terrace, roof); service (stilt); ground (the lowest other floor when it is at elevation 0); upper |
//! | the floor's drawing in the plan set (code `AR`, else the first per-floor set) | floor `file` |
//! | the floor's drawings in the other per-floor sets | `layouts[]` with `id` = set code in lower case, `name` = set name, `file` |
//! | the master drawing (else the first floor plan) | `main` |
//!
//! Floors are not matched by name or by file name, only by their ids in `project.json`; the legacy
//! `key` is the floor `code`, which the configuration keeps unique.

use serde_json::{Value, json};

use super::{DrawingKind, FloorKind, Issue, Plan, PlannedDrawing, ProjectConfig};

pub const FILE: &str = "project.aad.json";
/// The version of this projection (the `compat.version` field).
pub const COMPAT_VERSION: u32 = 1;

fn fnv(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
}

fn legacy_kind(cfg: &ProjectConfig, ground: Option<&str>, id: &str) -> &'static str {
    let Some(f) = cfg.floors.iter().find(|f| f.id == id) else { return "upper" };
    match f.kind {
        FloorKind::Basement => "basement",
        FloorKind::Terrace | FloorKind::Roof => "terrace",
        FloorKind::Stilt => "service",
        FloorKind::Floor | FloorKind::Podium => {
            if ground == Some(id) {
                "ground"
            } else {
                "upper"
            }
        }
    }
}

/// The set whose floor drawings are the floor plans: code `AR`, else the first per-floor set that
/// has drawings.
fn plan_set<'a>(cfg: &'a ProjectConfig, plan: &Plan) -> Option<&'a str> {
    let has = |set: &str| plan.drawings.iter().any(|d| d.kind == DrawingKind::Floor && d.set_id.as_deref() == Some(set));
    cfg.drawing_sets
        .iter()
        .find(|s| s.code.eq_ignore_ascii_case("AR") && has(&s.id))
        .or_else(|| cfg.drawing_sets.iter().find(|s| has(&s.id)))
        .map(|s| s.id.as_str())
}

/// The structural part of the projection (no run-time flags), as JSON.
pub fn project(cfg: &ProjectConfig, plan: &Plan, existing: Option<&Value>) -> Result<Value, Vec<Issue>> {
    let mut issues = Vec::new();
    let p = &cfg.project;
    // Metres, by dividing (3300 / 1000 is exactly 3.3; 3300 * 0.001 is not).
    let per_m = if p.units.length == "m" { 1.0 } else { 1000.0 };
    let Some(plan_set) = plan_set(cfg, plan) else {
        return Err(vec![Issue::new("drawing_sets", "the compatibility manifest needs a set with a drawing for each floor")]);
    };
    let ground = cfg
        .floors_in_order()
        .into_iter()
        .filter(|f| matches!(f.kind, FloorKind::Floor | FloorKind::Podium | FloorKind::Stilt))
        .find(|f| plan.elevations.iter().any(|(id, z)| id == &f.id && *z == 0.0))
        .map(|f| f.id.clone());
    let by = |floor: &str, set: &str| -> Option<&PlannedDrawing> {
        plan.drawings.iter().find(|d| d.kind == DrawingKind::Floor && d.floor_id.as_deref() == Some(floor) && d.set_id.as_deref() == Some(set))
    };
    let old_state = |floor_key: &str, layout: Option<&str>, field: &str, default: bool| -> bool {
        let Some(old) = existing else { return default };
        let Some(f) = old["floors"].as_array().and_then(|a| a.iter().find(|f| f["key"] == floor_key)) else { return default };
        let node = match layout {
            None => f,
            Some(l) => match f["layouts"].as_array().and_then(|a| a.iter().find(|x| x["id"] == l)) {
                Some(n) => n,
                None => return default,
            },
        };
        node[field].as_bool().unwrap_or(default)
    };
    let mut floors = Vec::new();
    for (i, fl) in cfg.floors_in_order().into_iter().enumerate() {
        let Some(file) = by(&fl.id, plan_set) else {
            issues.push(Issue::new(
                format!("floors[{i}]"),
                format!("`{}` has no drawing in the plan set, which the compatibility manifest needs", fl.name),
            ));
            continue;
        };
        let z = plan.elevations.iter().find(|(id, _)| id == &fl.id).map(|(_, z)| z / per_m);
        let mut layouts = Vec::new();
        for set in cfg.drawing_sets.iter().filter(|s| s.enabled && s.id != plan_set) {
            if let Some(d) = by(&fl.id, &set.id) {
                let id = set.code.to_ascii_lowercase();
                layouts.push(json!({
                    "id": id, "name": set.name, "file": d.rel_path,
                    "visible": old_state(&fl.code, Some(&id), "visible", false), "frozen": old_state(&fl.code, Some(&id), "frozen", false),
                    "locked": old_state(&fl.code, Some(&id), "locked", true), "print": old_state(&fl.code, Some(&id), "print", true),
                }));
            }
        }
        let kind = legacy_kind(cfg, ground.as_deref(), &fl.id);
        floors.push(json!({
            "name": fl.name, "key": fl.code, "kind": kind, "sequence": fl.sequence, "elevation": z, "height": fl.floor_to_floor.map(|h| h / per_m),
            "file": file.rel_path,
            "visible": old_state(&fl.code, None, "visible", true), "frozen": old_state(&fl.code, None, "frozen", false),
            "locked": old_state(&fl.code, None, "locked", kind != "ground"), "print": old_state(&fl.code, None, "print", true),
            "layouts": layouts,
        }));
    }
    let main = plan
        .drawings
        .iter()
        .find(|d| d.kind == DrawingKind::Master)
        .or_else(|| plan.drawings.iter().find(|d| d.kind == DrawingKind::Floor))
        .map(|d| d.rel_path.clone());
    let Some(main) = main else {
        issues.push(Issue::new("drawings", "there is no drawing to be the main drawing"));
        return Err(issues);
    };
    if !issues.is_empty() {
        return Err(issues);
    }
    let categories: Vec<Value> = cfg
        .drawing_sets
        .iter()
        .filter(|s| s.enabled && s.id != plan_set && plan.drawings.iter().any(|d| d.set_id.as_deref() == Some(&s.id) && d.kind == DrawingKind::Floor))
        .map(|s| json!({"id": s.code.to_ascii_lowercase(), "name": s.name}))
        .collect();
    let active = floors.iter().find(|f| f["kind"] == "ground").or(floors.first()).and_then(|f| f["name"].as_str()).unwrap_or("").to_string();
    let active = existing.and_then(|o| o["active"].as_str()).filter(|a| floors.iter().any(|f| f["name"] == *a)).map_or(active, str::to_string);
    let mut m = json!({
        "format": "aad-project", "version": 1, "id": p.id, "name": p.name, "client": p.client.name, "location": p.address.city, "address": p.address.line1,
        "type": p.kind, "units": "metric", "drawingUnits": p.units.length, "scale": 100.0, "lengthUnit": "m",
        "main": main, "categories": categories, "floors": floors, "active": active,
        "activeCategory": existing.map_or(Value::Null, |o| o["activeCategory"].clone()),
    });
    let hash = fnv(&structural(&m).to_string());
    m["compat"] = json!({"source": "project.json", "version": COMPAT_VERSION, "sourceHash": format!("{hash:016x}"), "generatedBy": "project.generate", "note": "generated from project.json: do not edit; run project.generate to refresh"});
    Ok(m)
}

/// The fields that come from `project.json` (everything except run-time state and the `compat` block).
fn structural(m: &Value) -> Value {
    let floors: Vec<Value> = m["floors"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|f| {
            let layouts: Vec<Value> = f["layouts"].as_array().into_iter().flatten().map(|l| json!({"id": l["id"], "name": l["name"], "file": l["file"]})).collect();
            json!({"name": f["name"], "key": f["key"], "kind": f["kind"], "sequence": f["sequence"], "elevation": f["elevation"], "height": f["height"], "file": f["file"], "layouts": layouts})
        })
        .collect();
    json!({
        "id": m["id"], "name": m["name"], "client": m["client"], "location": m["location"], "address": m["address"], "type": m["type"],
        "units": m["units"], "drawingUnits": m["drawingUnits"], "lengthUnit": m["lengthUnit"], "main": m["main"], "categories": m["categories"], "floors": floors,
    })
}

/// Compare the compatibility file at `root` with what `project.json` says it should be.
pub fn check(root: &std::path::Path, cfg: &ProjectConfig, plan: &Plan) -> Vec<Issue> {
    let path = root.join(FILE);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return vec![Issue::new(FILE, "the compatibility manifest is missing: run project.generate to create it")];
    };
    let Ok(on_disk) = serde_json::from_str::<Value>(&text) else {
        return vec![Issue::new(FILE, "the compatibility manifest is not valid JSON: run project.generate to recreate it")];
    };
    if on_disk["format"] != "aad-project" || on_disk["version"] != 1 {
        return vec![Issue::new(FILE, "the compatibility manifest is not an AAD project file of version 1")];
    }
    if on_disk["compat"]["source"] != "project.json" {
        return vec![Issue::new(FILE, "the compatibility manifest does not say it was generated from project.json: it must not be kept by hand")];
    }
    let expected = match project(cfg, plan, Some(&on_disk)) {
        Ok(m) => m,
        Err(e) => return e,
    };
    let (a, b) = (structural(&expected), structural(&on_disk));
    let mut v = Vec::new();
    if a != b {
        for key in a.as_object().into_iter().flat_map(|o| o.keys()) {
            if a[key] != b[key] {
                v.push(Issue::new(format!("{FILE}: {key}"), "differs from project.json (the compatibility manifest is stale)"));
            }
        }
    }
    if v.is_empty() && on_disk["compat"]["sourceHash"] != expected["compat"]["sourceHash"] {
        v.push(Issue::new(format!("{FILE}: compat.sourceHash"), "does not match what project.json produces"));
    }
    v
}

/// Write the compatibility manifest for `cfg` and `plan` into `root`, keeping the run-time state of
/// an existing one.
pub fn write(root: &std::path::Path, cfg: &ProjectConfig, plan: &Plan) -> Result<(), Vec<Issue>> {
    let existing = std::fs::read_to_string(root.join(FILE)).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok());
    let m = project(cfg, plan, existing.as_ref())?;
    let text = serde_json::to_string_pretty(&m).map_err(|e| vec![Issue::new(FILE, e.to_string())])?;
    super::company::write_atomic(&root.join(FILE), text.as_bytes()).map_err(|e| vec![Issue::new(FILE, e)])
}
