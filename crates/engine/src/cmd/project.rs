//! Project data kept in the drawing: details, floors, beam depths and levels (the kit's
//! HCWPROJECT, HCWFLOORS, HCWBEAMS and HCWLEVELS). Stored in `Drawing::project` as JSON text and
//! written to DXF/DWG with the drawing, so schedules and take-off read what is entered once.

use cadcraft_doc::Drawing;
use serde_json::{Map, Value, json};

use super::*;
use crate::{Result, Session};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("project", "Project Details", run_project)
            .menu(&["Architecture", "Project", "Project Details"])
            .alias(&["hcwproject"])
            .params("{details?: {title, owner, architect, engineer, pid, siteArea, plotUse, address, authority, date}} → {details} (set merges the given fields)"),
        CommandSpec::new("floors", "Floors and Heights", run_floors)
            .menu(&["Architecture", "Project", "Floors and Heights"])
            .alias(&["hcwfloors"])
            .params("{count?: 1..60, floors?: [{name, floorToFloor, ceiling, lintel, slab, beam}] (metres; slab and beam in mm)} → {floors}"),
        CommandSpec::new("beams", "Beam Depths", run_beams)
            .menu(&["Architecture", "Project", "Beam Depths"])
            .alias(&["hcwbeams"])
            .params("{depths?: [mm, ...]} → {depths}"),
        CommandSpec::new("levels", "Building Levels", run_levels)
            .menu(&["Architecture", "Project", "Building Levels"])
            .alias(&["hcwlevels"])
            .params("{} → {levels: [{name, level, floorToFloor, ceiling, lintel, slab, beam}]} (finished floor levels in metres, from the plinth)")
            .noundo(),
    ]
}

/// A project entry as JSON, or `Null` when absent or unreadable.
pub(crate) fn get(d: &Drawing, key: &str) -> Value {
    d.project.get(key).and_then(|t| serde_json::from_str(t).ok()).unwrap_or(Value::Null)
}

/// Store a project entry.
pub(crate) fn set(d: &mut Drawing, key: &str, v: &Value) {
    d.project.insert(key.to_string(), v.to_string());
}

const FLOOR_NAMES: [&str; 5] = ["Ground", "First", "Second", "Third", "Fourth"];

fn floor_name(i: usize) -> String {
    FLOOR_NAMES.get(i).map_or_else(|| format!("Floor {i}"), |s| (*s).to_string())
}

fn default_floor(i: usize) -> Value {
    json!({ "name": floor_name(i), "floorToFloor": 3.0, "ceiling": 2.85, "lintel": 2.1, "slab": 150, "beam": 450 })
}

/// The stored floors, or a single default floor.
pub(crate) fn floors(d: &Drawing) -> Vec<Value> {
    match get(d, "floors") {
        Value::Array(a) if !a.is_empty() => a,
        _ => vec![default_floor(0)],
    }
}

fn num(v: &Value, key: &str, default: f64) -> f64 {
    v.get(key).and_then(Value::as_f64).filter(|x| x.is_finite()).unwrap_or(default)
}

fn run_project(s: &mut Session, p: &Value) -> Result<Value> {
    const KEYS: [&str; 10] = ["title", "owner", "architect", "engineer", "pid", "siteArea", "plotUse", "address", "authority", "date"];
    let mut cur = match get(s.doc()?, "details") {
        Value::Object(o) => o,
        _ => Map::new(),
    };
    if let Some(Value::Object(new)) = p.get("details") {
        for (k, v) in new {
            if !KEYS.contains(&k.as_str()) {
                return Err(bad("project", format!("unknown detail `{k}`")));
            }
            let text = v
                .as_str()
                .map(str::to_string)
                .or_else(|| v.as_f64().map(|n| n.to_string()))
                .ok_or_else(|| bad("project", format!("`{k}` must be text")))?;
            if text.len() > 500 {
                return Err(bad("project", format!("`{k}` is too long")));
            }
            cur.insert(k.clone(), json!(text));
        }
        set(s.doc_mut()?, "details", &Value::Object(cur.clone()));
    }
    Ok(json!({ "details": cur }))
}

fn run_floors(s: &mut Session, p: &Value) -> Result<Value> {
    let mut fl = floors(s.doc()?);
    let mut changed = false;
    if let Some(Value::Array(given)) = p.get("floors") {
        if given.is_empty() || given.len() > 60 {
            return Err(bad("floors", "between 1 and 60 floors"));
        }
        fl = given
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let def = default_floor(i);
                json!({
                    "name": f.get("name").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| floor_name(i)),
                    "floorToFloor": num(f, "floorToFloor", num(&def, "floorToFloor", 3.0)),
                    "ceiling": num(f, "ceiling", num(&def, "ceiling", 2.85)),
                    "lintel": num(f, "lintel", num(&def, "lintel", 2.1)),
                    "slab": num(f, "slab", 150.0),
                    "beam": num(f, "beam", 450.0),
                })
            })
            .collect();
        for f in &fl {
            let (ftf, c, l) = (num(f, "floorToFloor", 0.0), num(f, "ceiling", 0.0), num(f, "lintel", 0.0));
            if !(ftf > 0.0 && c > 0.0 && l > 0.0) {
                return Err(bad("floors", "heights must be positive"));
            }
            if c > ftf {
                return Err(bad(
                    "floors",
                    format!("floor `{}`: ceiling height is taller than the floor-to-floor height", f["name"].as_str().unwrap_or("")),
                ));
            }
            if l > c {
                return Err(bad("floors", format!("floor `{}`: lintel bottom is above the ceiling", f["name"].as_str().unwrap_or(""))));
            }
        }
        changed = true;
    }
    if let Some(n) = p.get("count").and_then(Value::as_u64) {
        if !(1..=60).contains(&n) {
            return Err(bad("floors", "`count` must be 1..60"));
        }
        let n = n as usize;
        fl.truncate(n);
        while fl.len() < n {
            let mut next = fl.last().cloned().unwrap_or_else(|| default_floor(0));
            next["name"] = json!(floor_name(fl.len()));
            fl.push(next);
        }
        changed = true;
    }
    if changed {
        set(s.doc_mut()?, "floors", &Value::Array(fl.clone()));
    }
    Ok(json!({ "floors": fl }))
}

fn run_beams(s: &mut Session, p: &Value) -> Result<Value> {
    let mut depths = match get(s.doc()?, "beams") {
        Value::Array(a) => a,
        _ => vec![json!(230), json!(300), json!(375), json!(450), json!(600)],
    };
    if let Some(Value::Array(given)) = p.get("depths") {
        if given.is_empty() || given.len() > 50 || given.iter().any(|v| !v.as_f64().is_some_and(|x| x.is_finite() && x > 0.0 && x < 5000.0)) {
            return Err(bad("beams", "`depths` must be 1..50 positive depths in mm"));
        }
        depths = given.clone();
        set(s.doc_mut()?, "beams", &Value::Array(depths.clone()));
    }
    Ok(json!({ "depths": depths }))
}

fn run_levels(s: &mut Session, _p: &Value) -> Result<Value> {
    let mut level = 0.0;
    let mut out = Vec::new();
    for f in floors(s.doc()?) {
        out.push(json!({
            "name": f["name"], "level": (level * 1000.0_f64).round() / 1000.0, "floorToFloor": f["floorToFloor"],
            "ceiling": f["ceiling"], "lintel": f["lintel"], "slab": f["slab"], "beam": f["beam"],
        }));
        level += num(&f, "floorToFloor", 0.0);
    }
    Ok(json!({ "levels": out }))
}
