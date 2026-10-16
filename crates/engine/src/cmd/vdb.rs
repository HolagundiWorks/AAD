//! Export and query AAD drawings as ShilpiDB `.vdb` spatial stores (`aad-vdb`).
//!
//! Payloads are a short UTF-8 line `handle\ttype\tlayer` so the file stays small and
//! tools can label hits without a full drawing round-trip. Geometry still lives in DXF/DWG.

use std::path::PathBuf;

use aad_doc::{entity_bounds, Space};
use aad_vdb::{save_path, Bbox, VdbRecord, VdbStore, FORMAT_VERSION};
use serde_json::{json, Value};

use super::*;
use crate::{Result, Session};

/// Most entities one export writes (hostile drawings are cut; the result says so).
const MAX_EXPORT: usize = 500_000;
/// Most entities one export scan looks at.
const MAX_SCAN: usize = 2_000_000;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("vdb.export", "Export ShilpiDB .vdb", run_export)
            .menu(&["File", "Export", "ShilpiDB (.vdb)…"])
            .params("{path} → {path, count, truncated, version} (model-space entity bounds into a .vdb spatial store)")
            .noundo(),
        CommandSpec::new("vdb.info", "ShilpiDB .vdb Info", run_info)
            .params("{path} → {path, count, version} (read a .vdb without changing the drawing)")
            .noundo(),
        CommandSpec::new("vdb.query", "Query ShilpiDB .vdb", run_query)
            .params("{path, window: [x0,y0,x1,y1], limit?: 1..200 (default 50)} → {path, window, matched, shown, truncated, ids, hits: [{id, handle?, type?, layer?}]} (bbox query against a .vdb file)")
            .noundo(),
    ]
}

fn run_export(s: &mut Session, p: &Value) -> Result<Value> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (s, p);
        return Err(bad("vdb.export", "files cannot be written on the web"));
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let path = path_param(p, "path", "vdb.export")?;
        let drawing = s.doc()?;
        let mut records = Vec::new();
        let mut truncated = false;
        let store = drawing.space(&Space::Model);
        let Some(entities) = store else {
            return Err(bad("vdb.export", "no model space"));
        };
        for entity in entities.iter().take(MAX_SCAN) {
            if records.len() >= MAX_EXPORT {
                truncated = true;
                break;
            }
            let bounds = entity_bounds(drawing, entity, 0);
            if bounds.is_empty() || !finite_bounds(&bounds) {
                continue;
            }
            let payload = format!(
                "{}\t{}\t{}",
                entity.handle.hex(),
                entity.kind.type_name(),
                entity.common.layer
            );
            records.push(VdbRecord {
                id: entity.handle.0,
                bbox: Bbox::from_corners(bounds.min.x, bounds.min.y, bounds.max.x, bounds.max.y),
                payload: payload.into_bytes(),
            });
        }
        let count = records.len();
        let vdb = VdbStore::from_records(records).map_err(|e| bad("vdb.export", e.to_string()))?;
        save_path(&path, &vdb).map_err(|e| bad("vdb.export", format!("{}: {e}", path.display())))?;
        Ok(json!({
            "path": path.to_string_lossy(),
            "count": count,
            "truncated": truncated,
            "version": FORMAT_VERSION,
        }))
    }
}

fn run_info(_s: &mut Session, p: &Value) -> Result<Value> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = p;
        return Err(bad("vdb.info", "files cannot be read on the web"));
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let path = path_param(p, "path", "vdb.info")?;
        let store = aad_vdb::load_path(&path).map_err(|e| bad("vdb.info", format!("{}: {e}", path.display())))?;
        Ok(json!({
            "path": path.to_string_lossy(),
            "count": store.len(),
            "version": FORMAT_VERSION,
        }))
    }
}

fn run_query(_s: &mut Session, p: &Value) -> Result<Value> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = p;
        return Err(bad("vdb.query", "files cannot be read on the web"));
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let path = path_param(p, "path", "vdb.query")?;
        let window = window_param(p, "vdb.query")?;
        let limit = (int_or(p, "limit", 50) as usize).clamp(1, 200);
        let store = aad_vdb::load_path(&path).map_err(|e| bad("vdb.query", format!("{}: {e}", path.display())))?;
        let ids = store.query_bbox(window);
        let matched = ids.len();
        let truncated = matched > limit;
        let shown_ids: Vec<u64> = ids.into_iter().take(limit).collect();
        let mut hits = Vec::with_capacity(shown_ids.len());
        for id in &shown_ids {
            let mut hit = json!({ "id": id });
            if let Some(record) = store.records().iter().find(|r| r.id == *id) {
                if let Ok(text) = std::str::from_utf8(&record.payload) {
                    let mut parts = text.split('\t');
                    if let Some(handle) = parts.next() {
                        hit["handle"] = json!(handle);
                    }
                    if let Some(kind) = parts.next() {
                        hit["type"] = json!(kind);
                    }
                    if let Some(layer) = parts.next() {
                        hit["layer"] = json!(layer);
                    }
                }
            }
            hits.push(hit);
        }
        Ok(json!({
            "path": path.to_string_lossy(),
            "window": [window.min.x, window.min.y, window.max.x, window.max.y],
            "matched": matched,
            "shown": shown_ids.len(),
            "truncated": truncated,
            "ids": shown_ids,
            "hits": hits,
        }))
    }
}

fn path_param(p: &Value, key: &str, cmd: &str) -> Result<PathBuf> {
    let raw = p.get(key).and_then(|v| v.as_str()).unwrap_or("").trim();
    if raw.is_empty() {
        return Err(bad(cmd, format!("missing `{key}`")));
    }
    Ok(PathBuf::from(raw))
}

fn window_param(p: &Value, cmd: &str) -> Result<Bbox> {
    let arr = p
        .get("window")
        .and_then(|v| v.as_array())
        .ok_or_else(|| bad(cmd, "missing `window: [x0, y0, x1, y1]`"))?;
    if arr.len() != 4 {
        return Err(bad(cmd, "`window` needs four numbers [x0, y0, x1, y1]"));
    }
    let mut nums = [0.0; 4];
    for (i, v) in arr.iter().enumerate() {
        nums[i] = v.as_f64().ok_or_else(|| bad(cmd, "`window` values must be numbers"))?;
        if !nums[i].is_finite() {
            return Err(bad(cmd, "`window` values must be finite"));
        }
    }
    Ok(Bbox::from_corners(nums[0], nums[1], nums[2], nums[3]))
}

fn finite_bounds(b: &aad_geom::Bounds2) -> bool {
    b.min.x.is_finite() && b.min.y.is_finite() && b.max.x.is_finite() && b.max.y.is_finite()
}

fn int_or(p: &Value, key: &str, default: i64) -> i64 {
    p.get(key).and_then(|v| v.as_i64()).unwrap_or(default)
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use aad_doc::{Common, EntityKind, Line};
    use aad_geom::Vec3;
    use serde_json::json;

    #[test]
    fn export_info_and_query_round_trip() {
        let dir = tempfile::tempdir().expect("temp");
        let path = dir.path().join("sample.vdb");
        let mut s = Session::new();
        let a = Vec3::new(0.0, 0.0, 0.0);
        let b = Vec3::new(10.0, 0.0, 0.0);
        s.doc_mut()
            .expect("doc")
            .add(
                &Space::Model,
                Common::default(),
                EntityKind::Line(Line { a, b }),
            )
            .expect("add");
        let path_str = path.to_string_lossy().to_string();
        let exported = s.execute("vdb.export", &json!({ "path": path_str })).expect("export");
        assert_eq!(exported["count"], 1);
        assert_eq!(exported["version"], FORMAT_VERSION);
        let info = s.execute("vdb.info", &json!({ "path": path_str })).expect("info");
        assert_eq!(info["count"], 1);
        let q = s
            .execute(
                "vdb.query",
                &json!({ "path": path_str, "window": [0.0, -1.0, 10.0, 1.0] }),
            )
            .expect("query");
        assert_eq!(q["matched"], 1);
        assert_eq!(q["hits"][0]["type"], "Line");
    }
}
