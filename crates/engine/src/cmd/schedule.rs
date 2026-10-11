//! Door and window schedule, the deduction map and the bill (the kit's HCWOPENSTD, HCWOPENSYNC,
//! HCWOPENSCHED, MSCHED and the rates page). Stored in the drawing's project data as `schedule`:
//!
//! ```json
//! { "openings": [{"name","kind":"door|window|ventilator","type","length","height","sill","lintel","block","floor","count"}],
//!   "map": {"FB01-D1": "D1"}, "rates": {"Bricks": {"unit": "cum", "rate": 5200}} }
//! ```
//!
//! Lengths and heights are metres (feet when the drawing is in feet or inches). The deduction map
//! ties each measured opening label to a schedule entry; wall paint then deducts the entry's
//! length times height instead of the measured length times the wall height.

use cadcraft_doc::{EntityKind, Space};
use cadcraft_geom::Vec2;
use serde_json::{Map, Value, json};

use super::*;
use crate::annot::{self as ua, Unit};
use crate::{Result, Session};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("schedule", "Schedules", run_schedule)
            .menu(&["Architecture", "Schedules", "Edit Schedules"])
            .alias(&["msched", "hcwsched"])
            .params("{openings?: [..], map?: {label: name}, rates?: {takeoff: {unit, rate}}} → {openings, map, rates, warnings} (set replaces the given parts)"),
        CommandSpec::new("openings.std", "Standard Openings", run_std)
            .menu(&["Architecture", "Schedules", "Standard Openings"])
            .alias(&["hcwopenstd"])
            .params("{} → {added} (doors D1 800, D2 900, D3 1200; windows W1 600 to W5 2000; ventilator V1 600)"),
        CommandSpec::new("openings.sync", "Update Opening Schedule", run_sync)
            .menu(&["Architecture", "Schedules", "Update Schedule"])
            .alias(&["hcwopensync"])
            .params("{} → {counts, mapped, ambiguous} (counts block references; maps measured openings to the entry of nearest length)"),
        CommandSpec::new("openings.table", "Opening Schedule Table", run_table)
            .menu(&["Architecture", "Schedules", "Draw Opening Schedule"])
            .alias(&["hcwopensched", "mschedtable"])
            .params("{at?: [x,y]} (draws the floors, openings and column tables on MEASURE-TABLE)"),
        CommandSpec::new("takeoff.rates", "Take-off Rates", run_rates)
            .menu(&["Architecture", "Take-off", "Rates"])
            .params("{rates?: {takeoff: {unit, rate}}} → {rates}"),
        CommandSpec::new("takeoff.bill", "Bill of Quantities", run_bill)
            .menu(&["Architecture", "Take-off", "Bill"])
            .params("{path?: csv file} → {lines: [{name, quantity, unit, rate, amount}], total}")
            .noundo(),
    ]
}

fn schedule(d: &cadcraft_doc::Drawing) -> Map<String, Value> {
    match project::get(d, "schedule") {
        Value::Object(o) => o,
        _ => Map::new(),
    }
}

fn openings(o: &Map<String, Value>) -> Vec<Value> {
    o.get("openings").and_then(Value::as_array).cloned().unwrap_or_default()
}

fn text(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or("").to_string()
}

fn num(v: &Value, k: &str) -> Option<f64> {
    v.get(k).and_then(Value::as_f64).filter(|x| x.is_finite())
}

/// Validate an opening list; returns the cleaned list and the warnings.
fn check(list: &[Value], lintel: f64) -> Result<(Vec<Value>, Vec<String>)> {
    if list.len() > 500 {
        return Err(bad("schedule", "at most 500 openings"));
    }
    let mut out: Vec<Value> = Vec::new();
    let mut warnings = Vec::new();
    for o in list {
        let name = text(o, "name").trim().to_string();
        if name.is_empty() || name.len() > 40 {
            return Err(bad("schedule", "every opening needs a name of 1..40 characters"));
        }
        if out.iter().any(|x| text(x, "name").eq_ignore_ascii_case(&name)) {
            return Err(bad("schedule", format!("the name `{name}` is used twice")));
        }
        let kind = match text(o, "kind").to_ascii_lowercase().as_str() {
            "door" => "door",
            "window" => "window",
            "ventilator" => "ventilator",
            "" if name.to_ascii_uppercase().starts_with('W') => "window",
            "" if name.to_ascii_uppercase().starts_with('V') => "ventilator",
            "" => "door",
            other => return Err(bad("schedule", format!("`{name}`: kind must be door, window or ventilator, not `{other}`"))),
        };
        let (len, h) = (num(o, "length"), num(o, "height"));
        let (Some(len), Some(h)) = (len.filter(|x| *x > 0.0), h.filter(|x| *x > 0.0)) else {
            return Err(bad("schedule", format!("`{name}`: length and height must be positive")));
        };
        let sill = num(o, "sill");
        let top = num(o, "lintel").unwrap_or(lintel);
        if h > top + 1e-9 {
            warnings.push(format!("`{name}`: height {h} is taller than the lintel bottom {top}"));
        }
        if let Some(sl) = sill
            && (sl + h - top).abs() > 0.05 + 1e-9
        {
            warnings.push(format!("`{name}`: sill {sl} + height {h} does not reach the lintel bottom {top}"));
        }
        let mut c = Map::new();
        c.insert("name".into(), json!(name));
        c.insert("kind".into(), json!(kind));
        c.insert("type".into(), json!(text(o, "type")));
        c.insert("length".into(), json!(len));
        c.insert("height".into(), json!(h));
        if let Some(sl) = sill {
            c.insert("sill".into(), json!(sl));
        }
        if let Some(l) = num(o, "lintel") {
            c.insert("lintel".into(), json!(l));
        }
        c.insert("block".into(), json!(text(o, "block")));
        c.insert("floor".into(), json!(text(o, "floor")));
        c.insert("count".into(), json!(o.get("count").and_then(Value::as_u64).unwrap_or(0)));
        out.push(Value::Object(c));
    }
    Ok((out, warnings))
}

fn first_lintel(s: &Session) -> Result<f64> {
    Ok(project::floors(s.doc()?).first().and_then(|f| f["lintel"].as_f64()).unwrap_or(2.1))
}

fn run_schedule(s: &mut Session, p: &Value) -> Result<Value> {
    let mut o = schedule(s.doc()?);
    let lintel = first_lintel(s)?;
    let mut changed = false;
    if let Some(Value::Array(list)) = p.get("openings") {
        let (clean, _) = check(list, lintel)?;
        o.insert("openings".into(), Value::Array(clean));
        changed = true;
    }
    if let Some(Value::Object(m)) = p.get("map") {
        if m.len() > 5000 || m.iter().any(|(k, v)| k.len() > 80 || !v.is_string()) {
            return Err(bad("schedule", "`map` must be at most 5000 label to name entries"));
        }
        let names: Vec<String> = openings(&o).iter().map(|x| text(x, "name")).collect();
        if let Some((k, v)) = m.iter().find(|(_, v)| !names.iter().any(|n| Some(n.as_str()) == v.as_str())) {
            return Err(bad("schedule", format!("the map sends `{k}` to `{v}`, which is not in the schedule")));
        }
        o.insert("map".into(), Value::Object(m.clone()));
        changed = true;
    }
    if let Some(Value::Object(r)) = p.get("rates") {
        o.insert("rates".into(), Value::Object(r.clone()));
        changed = true;
    }
    let (_, warnings) = check(&openings(&o), lintel)?;
    if changed {
        project::set(s.doc_mut()?, "schedule", &Value::Object(o.clone()));
    }
    Ok(
        json!({ "openings": o.get("openings").cloned().unwrap_or(json!([])), "map": o.get("map").cloned().unwrap_or(json!({})), "rates": o.get("rates").cloned().unwrap_or(json!({})), "warnings": warnings }),
    )
}

fn run_std(s: &mut Session, _p: &Value) -> Result<Value> {
    let lintel = first_lintel(s)?;
    let mut o = schedule(s.doc()?);
    let mut list = openings(&o);
    let std: [(&str, &str, f64, f64, Option<f64>); 9] = [
        ("D1", "door", 0.8, lintel, None),
        ("D2", "door", 0.9, lintel, None),
        ("D3", "door", 1.2, lintel, None),
        ("W1", "window", 0.6, 1.2, Some(lintel - 1.2)),
        ("W2", "window", 0.9, 1.2, Some(lintel - 1.2)),
        ("W3", "window", 1.2, 1.2, Some(lintel - 1.2)),
        ("W4", "window", 1.5, 1.2, Some(lintel - 1.2)),
        ("W5", "window", 2.0, 1.2, Some(lintel - 1.2)),
        ("V1", "ventilator", 0.6, 0.45, Some(lintel - 0.45)),
    ];
    let mut added = 0;
    for (name, kind, len, h, sill) in std {
        if list.iter().any(|x| text(x, "name") == name) {
            continue;
        }
        let mut e = json!({ "name": name, "kind": kind, "length": len, "height": h, "type": "", "block": "", "floor": "", "count": 0 });
        if let Some(sl) = sill {
            e["sill"] = json!((sl * 1000.0).round() / 1000.0);
        }
        list.push(e);
        added += 1;
    }
    let (clean, _) = check(&list, lintel)?;
    o.insert("openings".into(), Value::Array(clean));
    project::set(s.doc_mut()?, "schedule", &Value::Object(o));
    Ok(json!({ "added": added }))
}

/// Block references of each name in the model and layouts.
fn block_counts(d: &cadcraft_doc::Drawing) -> std::collections::BTreeMap<String, u64> {
    let mut out = std::collections::BTreeMap::new();
    for e in std::iter::once(&d.model).chain(d.layouts.iter().map(|l| &l.entities)).flat_map(|s| s.iter()) {
        if let EntityKind::Insert(i) = &e.kind {
            *out.entry(i.block.to_ascii_uppercase()).or_insert(0u64) += 1;
        }
    }
    out
}

fn run_sync(s: &mut Session, _p: &Value) -> Result<Value> {
    let lintel = first_lintel(s)?;
    let mut o = schedule(s.doc()?);
    let mut list = openings(&o);
    let counts = block_counts(s.doc()?);
    let mut map: Map<String, Value> = o.get("map").and_then(Value::as_object).cloned().unwrap_or_default();
    // Block counts: an entry's `block` field may name several blocks separated by `;`.
    let mut by_name = Map::new();
    for e in &mut list {
        let n: u64 = text(e, "block")
            .split(';')
            .map(str::trim)
            .filter(|b| !b.is_empty())
            .map(|b| counts.get(&b.to_ascii_uppercase()).copied().unwrap_or(0))
            .sum();
        e["count"] = json!(n);
        by_name.insert(text(e, "name"), json!(n));
    }
    // Measured openings from the saved take-offs, mapped to the entry of nearest length.
    let mut mapped = 0;
    let mut ambiguous = Vec::new();
    let mut unmatched = Vec::new();
    for key in ["Bricks", "Linear", "Beams"] {
        let t = project::get(s.doc()?, &format!("takeoff:{key}"));
        let imperial = t["lengthUnit"].as_str() == Some("ft");
        let tol = if imperial { 2.0 / 12.0 } else { 0.05 };
        for item in t["items"].as_array().into_iter().flatten() {
            for op in item["openings"].as_array().into_iter().flatten() {
                let (Some(label), Some(len)) = (op["name"].as_str(), op["length"].as_f64()) else { continue };
                if map.contains_key(label) {
                    continue;
                }
                let near: Vec<(&Value, f64)> =
                    list.iter().filter_map(|e| Some((e, (num(e, "length")? - len).abs()))).filter(|(_, dd)| *dd <= tol + 1e-9).collect();
                let best = near.iter().map(|(_, dd)| *dd).fold(f64::INFINITY, f64::min);
                let winners: Vec<&(&Value, f64)> = near.iter().filter(|(_, dd)| (*dd - best).abs() < 1e-9).collect();
                match winners.as_slice() {
                    [one] => {
                        map.insert(label.to_string(), json!(text(one.0, "name")));
                        mapped += 1;
                    }
                    [] => unmatched.push(label.to_string()),
                    _ => ambiguous.push(label.to_string()),
                }
            }
        }
    }
    let (clean, warnings) = check(&list, lintel)?;
    o.insert("openings".into(), Value::Array(clean));
    o.insert("map".into(), Value::Object(map));
    project::set(s.doc_mut()?, "schedule", &Value::Object(o));
    Ok(json!({ "counts": by_name, "mapped": mapped, "ambiguous": ambiguous, "unmatched": unmatched, "warnings": warnings }))
}

fn run_table(s: &mut Session, p: &Value) -> Result<Value> {
    let o = schedule(s.doc()?);
    let list = openings(&o);
    if list.is_empty() {
        return Err(bad("openings.table", "the schedule is empty: add openings with `openings.std` or `schedule`"));
    }
    let imperial = ua::drawing_unit(s.doc()?).is_some_and(|u| matches!(u, Unit::In | Unit::Ft));
    let unit = if imperial { "ft" } else { "m" };
    let th = ua::drawing_unit(s.doc()?).map_or(0.125, |u| 0.125 / u.metres());
    let ext = s.doc()?.extents(&Space::Model);
    let origin = point_param(p, "at").unwrap_or(if ext.is_empty() { Vec2::ZERO } else { Vec2::new(ext.min.x, ext.min.y - 6.0 * th) });
    let c = takeoff::table_ctx(th);
    let floors = project::floors(s.doc()?);
    let frows: Vec<Vec<String>> = floors
        .iter()
        .map(|f| {
            vec![
                text(f, "name"),
                format!("{:.2}", num(f, "floorToFloor").unwrap_or(0.0)),
                format!("{:.2}", num(f, "ceiling").unwrap_or(0.0)),
                format!("{:.2}", num(f, "lintel").unwrap_or(0.0)),
                format!("{:.0}", num(f, "slab").unwrap_or(0.0)),
            ]
        })
        .collect();
    let fh: Vec<String> = ["Floor", "Floor to floor", "Ceiling", "Lintel bottom", "Slab (mm)"].iter().map(|x| (*x).to_string()).collect();
    let mut y = origin;
    takeoff::table(s, &c, y, "FLOOR HEIGHTS", &fh, &frows)?;
    y.y -= th * 1.8 * (frows.len() as f64 + 3.0);
    let mut orows: Vec<Vec<String>> = Vec::new();
    for kind in ["door", "window", "ventilator"] {
        let mut total = 0;
        for e in list.iter().filter(|e| text(e, "kind") == kind) {
            let n = e["count"].as_u64().unwrap_or(0);
            total += n;
            orows.push(vec![
                text(e, "name"),
                text(e, "type"),
                format!("{:.2}", num(e, "length").unwrap_or(0.0)),
                format!("{:.2}", num(e, "height").unwrap_or(0.0)),
                num(e, "sill").map_or(String::new(), |x| format!("{x:.2}")),
                n.to_string(),
            ]);
        }
        if list.iter().any(|e| text(e, "kind") == kind) {
            orows.push(vec![format!("TOTAL {}", kind.to_uppercase()), String::new(), String::new(), String::new(), String::new(), total.to_string()]);
        }
    }
    let oh: Vec<String> = ["Name", "Type", &format!("Length ({unit})"), "Height", "Sill", "Count"].iter().map(|x| (*x).to_string()).collect();
    takeoff::table(s, &c, y, "DOOR AND WINDOW SCHEDULE", &oh, &orows)?;
    Ok(json!({ "openings": list.len(), "floors": floors.len() }))
}

fn run_rates(s: &mut Session, p: &Value) -> Result<Value> {
    let mut o = schedule(s.doc()?);
    if let Some(Value::Object(r)) = p.get("rates") {
        for (k, v) in r {
            let ok = v.get("rate").and_then(Value::as_f64).is_some_and(|x| x.is_finite() && x >= 0.0)
                && v.get("unit").and_then(Value::as_str).is_some_and(|u| !u.is_empty() && u.len() <= 16);
            if !ok {
                return Err(bad("takeoff.rates", format!("`{k}` needs a unit and a non-negative rate")));
            }
        }
        o.insert("rates".into(), Value::Object(r.clone()));
        project::set(s.doc_mut()?, "schedule", &Value::Object(o.clone()));
    }
    Ok(json!({ "rates": o.get("rates").cloned().unwrap_or(json!({})) }))
}

/// The quantity that prices a saved take-off.
fn quantity(name: &str, t: &Value) -> Option<f64> {
    let k = match name {
        "Linear" | "Bricks" | "Beams" | "WallPaint" | "Slab" => "net",
        "Ceiling" | "Floor" | "Areas" => "area",
        "Columns" | "Rectangles" => "count",
        _ => return None,
    };
    t["totals"][k].as_f64()
}

fn run_bill(s: &mut Session, p: &Value) -> Result<Value> {
    let o = schedule(s.doc()?);
    let rates = o.get("rates").and_then(Value::as_object).cloned().unwrap_or_default();
    if rates.is_empty() {
        return Err(bad("takeoff.bill", "no rates: set them with `takeoff.rates`"));
    }
    let mut lines = Vec::new();
    let mut total = 0.0;
    let mut csv = String::from("Item,Quantity,Unit,Rate,Amount\n");
    for (name, r) in &rates {
        let t = project::get(s.doc()?, &format!("takeoff:{name}"));
        let Some(q) = quantity(name, &t) else { continue };
        let rate = r["rate"].as_f64().unwrap_or(0.0);
        let amount = (q * rate * 100.0).round() / 100.0;
        total += amount;
        let unit = r["unit"].as_str().unwrap_or("");
        csv += &format!("{name},{q},{unit},{rate},{amount}\n");
        lines.push(json!({ "name": name, "quantity": q, "unit": unit, "rate": rate, "amount": amount }));
    }
    if lines.is_empty() {
        return Err(bad("takeoff.bill", "none of the rated take-offs has been run"));
    }
    let total = (total * 100.0).round() / 100.0;
    csv += &format!("TOTAL,,,,{total}\n");
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(path) = str_param(p, "path") {
        std::fs::write(path, &csv).map_err(|e| bad("takeoff.bill", format!("{path}: {e}")))?;
    }
    #[cfg(target_arch = "wasm32")]
    let _ = p;
    Ok(json!({ "lines": lines, "total": total, "csv": csv }))
}
