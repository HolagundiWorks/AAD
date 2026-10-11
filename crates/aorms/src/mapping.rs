//! Pure mapping between what the portal holds and AAD's project data (`engine::cmd::project`). No I/O; fully unit-tested.
//!
//! Rule (AORMS-SUITE-SIGNIN.md): portal data reaches the drawing only on an explicit pull, and never touches drawn geometry —
//! these functions only produce project details and floors.
use serde_json::{Map, Value, json};

fn text(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::trim).filter(|t| !t.is_empty()).map(|t| t.chars().take(500).collect())
}

/// `GET /seed/{project}` → AAD project details (`title`, `owner`, `architect`, `address`, `pid`). Only fields the portal knows are returned,
/// so a pull never blanks something the user typed in AAD.
pub fn details_from_seed(seed: &Value) -> Map<String, Value> {
    let mut out = Map::new();
    let project = seed.get("project").cloned().unwrap_or(Value::Null);
    let office = seed.get("projectOffice").cloned().unwrap_or(Value::Null);
    let mut put = |k: &str, v: Option<String>| {
        if let Some(v) = v {
            out.insert(k.to_string(), json!(v));
        }
    };
    put("title", text(&project, "name").or_else(|| text(&office, "title")));
    put("owner", text(&project, "client_name"));
    put("architect", text(&project, "company_name").or_else(|| text(&project, "prepared_by_name")));
    put("address", text(&project, "location"));
    put("pid", text(&office, "ref"));
    out
}

fn mm(v: &Value, k: &str) -> Option<f64> {
    v.get(k).and_then(Value::as_f64).filter(|x| x.is_finite() && *x > 0.0 && *x < 100_000.0)
}

/// The project's `settings.levels` (AQC's levels: `Lvl0` is the plinth, then floors) → AAD floors, in metres/mm as `floors` expects.
/// `Lvl0` is AQC's and is skipped (the floors are those above the plinth). The portal carries no ceiling or lintel height, so those are
/// taken from `existing` floors of the same position, else derived (ceiling = floor-to-floor − slab, lintel 2.1 m or the ceiling if lower).
/// Returns `None` when the settings hold no usable floors, so the caller leaves AAD's floors alone.
pub fn floors_from_settings(settings: &Value, existing: &[Value]) -> Option<Vec<Value>> {
    let levels = settings.get("levels")?.as_array()?;
    let mut floors = Vec::new();
    for (i, lv) in levels.iter().filter(|l| l.get("id").and_then(Value::as_str) != Some("Lvl0")).take(60).enumerate() {
        let ftf = mm(lv, "height_mm")? / 1000.0;
        let slab = mm(lv, "slab_thickness_mm").unwrap_or(150.0);
        let beam = mm(lv, "beam_depth_mm").unwrap_or(450.0);
        let prev = existing.get(i);
        let derived = ((ftf - slab / 1000.0) * 1000.0).round() / 1000.0;
        let ceiling = prev.and_then(|p| p.get("ceiling")).and_then(Value::as_f64).filter(|c| *c > 0.0 && *c <= ftf).unwrap_or(derived);
        let lintel = prev.and_then(|p| p.get("lintel")).and_then(Value::as_f64).filter(|l| *l > 0.0 && *l <= ceiling).unwrap_or(2.1_f64.min(ceiling));
        let name = text(lv, "name").unwrap_or_else(|| format!("Floor {}", i + 1));
        floors.push(json!({ "name": name, "floorToFloor": ftf, "ceiling": ceiling, "lintel": lintel, "slab": slab, "beam": beam }));
    }
    if floors.is_empty() || floors.iter().any(|f| f["ceiling"].as_f64().is_none_or(|c| c <= 0.0)) {
        return None;
    }
    Some(floors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_fills_only_what_the_portal_knows() {
        let seed = json!({
            "project": { "name": "Lakeview Clubhouse", "client_name": "Lakeview RA", "company_name": "Aurelia Design", "location": "Hospet", "prepared_by_name": "A. Rao" },
            "projectOffice": { "ref": "DEMO-PRJ-04", "title": "Lakeview Clubhouse Redevelopment" },
        });
        let d = details_from_seed(&seed);
        assert_eq!(d["title"], "Lakeview Clubhouse");
        assert_eq!(d["owner"], "Lakeview RA");
        assert_eq!(d["architect"], "Aurelia Design");
        assert_eq!(d["address"], "Hospet");
        assert_eq!(d["pid"], "DEMO-PRJ-04");
        assert!(!d.contains_key("engineer") && !d.contains_key("siteArea"));
    }

    #[test]
    fn empty_and_odd_seed_values_are_skipped_and_long_ones_capped() {
        let long = "x".repeat(900);
        let d = details_from_seed(
            &json!({ "project": { "name": "  ", "client_name": 5, "location": long }, "projectOffice": { "title": "From office" } }),
        );
        assert_eq!(d["title"], "From office"); // blank project name falls back to the office title
        assert!(!d.contains_key("owner"));
        assert_eq!(d["address"].as_str().map(str::len), Some(500));
        assert!(details_from_seed(&Value::Null).is_empty());
    }

    #[test]
    fn levels_skip_the_plinth_and_convert_units() {
        let s = json!({ "levels": [
            { "id": "Lvl0", "name": "Plinth", "height_mm": 3200, "slab_thickness_mm": 150, "beam_depth_mm": 450 },
            { "id": "Lvl1", "name": "First floor", "height_mm": 3000, "slab_thickness_mm": 150, "beam_depth_mm": 400 },
            { "id": "Lvl2", "name": "Second floor", "height_mm": 3150, "slab_thickness_mm": 125, "beam_depth_mm": 450 } ] });
        let f = floors_from_settings(&s, &[]).unwrap_or_default();
        assert_eq!(f.len(), 2);
        assert_eq!(f[0]["name"], "First floor");
        assert_eq!(f[0]["floorToFloor"], 3.0);
        assert_eq!(f[0]["ceiling"], 2.85); // 3.0 − 0.150
        assert_eq!(f[0]["lintel"], 2.1);
        assert_eq!(f[0]["beam"], 400.0);
        assert_eq!(f[1]["slab"], 125.0);
    }

    #[test]
    fn existing_ceiling_and_lintel_are_kept_when_still_valid() {
        let s = json!({ "levels": [{ "id": "Lvl1", "name": "G", "height_mm": 3000, "slab_thickness_mm": 150, "beam_depth_mm": 450 }] });
        let f = floors_from_settings(&s, &[json!({ "ceiling": 2.7, "lintel": 2.0 })]).unwrap_or_default();
        assert_eq!((f[0]["ceiling"].as_f64(), f[0]["lintel"].as_f64()), (Some(2.7), Some(2.0)));
        // an existing ceiling taller than the new floor-to-floor is not kept
        let g = floors_from_settings(&s, &[json!({ "ceiling": 3.4, "lintel": 2.0 })]).unwrap_or_default();
        assert_eq!(g[0]["ceiling"].as_f64(), Some(2.85));
    }

    #[test]
    fn unusable_levels_leave_the_floors_alone() {
        assert!(floors_from_settings(&json!({}), &[]).is_none());
        assert!(floors_from_settings(&json!({ "levels": [] }), &[]).is_none());
        assert!(floors_from_settings(&json!({ "levels": [{ "id": "Lvl0", "height_mm": 3000 }] }), &[]).is_none()); // plinth only
        assert!(floors_from_settings(&json!({ "levels": [{ "id": "Lvl1", "height_mm": -5 }] }), &[]).is_none());
        assert!(floors_from_settings(&json!({ "levels": [{ "id": "Lvl1", "height_mm": f64::NAN }] }), &[]).is_none());
        assert!(floors_from_settings(&json!({ "levels": "nope" }), &[]).is_none());
    }

    #[test]
    fn hostile_level_lists_are_bounded() {
        let many: Vec<Value> = (1..500).map(|i| json!({ "id": format!("Lvl{i}"), "name": "F", "height_mm": 3000 })).collect();
        assert_eq!(floors_from_settings(&json!({ "levels": many }), &[]).map(|f| f.len()), Some(60));
    }
}
