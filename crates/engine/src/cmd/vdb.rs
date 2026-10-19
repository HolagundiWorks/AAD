//! Export, import and sync AAD drawings with ShilpiDB `.vdb` stores (`aad-vdb`).
//!
//! DXF/DWG remain the editable source of truth. `.vdb` holds spatial records with
//! format-v2 geometry payloads for the kinds listed in `aad_vdb::EntityGeom`.

use std::path::{Path, PathBuf};

use aad_doc::{Arc, Circle, Common, EntityKind, Insert, Line, LwPolyline, MText, Space, Text, entity_bounds};
use aad_geom::{PolyVertex, Vec2, Vec3};
use aad_vdb::{
    Bbox, EntityGeom, EntityPayload, FORMAT_VERSION, MAX_BLOCK_CHARS, MAX_POLY_VERTICES, MAX_TEXT_CHARS, VdbClient, VdbRecord, VdbStore,
    decode_payload, encode_payload, load_path, save_path,
};
use serde_json::{Value, json};

use super::*;
use crate::{Result, Session};

const MAX_EXPORT: usize = 500_000;
const MAX_SCAN: usize = 2_000_000;
const DEFAULT_SHILPID: &str = "127.0.0.1:7420";

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("vdb.export", "Export ShilpiDB .vdb", run_export)
            .menu(&["File", "Export", "ShilpiDB (.vdb)…"])
            .params("{path} → {path, count, truncated, version, skipped} (model-space entities into a .vdb spatial store, format v2 payloads)")
            .noundo(),
        CommandSpec::new("vdb.import", "Import ShilpiDB .vdb", run_import)
            .menu(&["File", "Import", "ShilpiDB (.vdb)…"])
            .params("{path, replace?: bool} → {path, added, skipped, version} (best-effort geometry into model space; one undo step)"),
        CommandSpec::new("vdb.info", "ShilpiDB .vdb Info", run_info)
            .params("{path} → {path, count, version} (read a .vdb without changing the drawing)")
            .noundo(),
        CommandSpec::new("vdb.query", "Query ShilpiDB .vdb", run_query)
            .params("{path, window: [x0,y0,x1,y1], limit?: 1..200 (default 50)} → {path, window, matched, shown, truncated, ids, hits}")
            .noundo(),
        CommandSpec::new("vdb.connect", "Connect ShilpiDB server", run_connect)
            .menu(&["File", "ShilpiDB", "Connect…"])
            .params("{addr?: \"127.0.0.1:7420\"} → {addr} (ping a local shilpid; session remembers the address)")
            .noundo(),
        CommandSpec::new("vdb.disconnect", "Disconnect ShilpiDB server", run_disconnect)
            .menu(&["File", "ShilpiDB", "Disconnect"])
            .params("{} → {ok: true}")
            .noundo(),
        CommandSpec::new("vdb.push", "Push drawing to ShilpiDB", run_push)
            .menu(&["File", "ShilpiDB", "Push drawing"])
            .params("{replace?: bool} → {addr, count, truncated, replaced} (upsert model space onto the connected shilpid; replace=true deletes server ids not in the drawing)")
            .noundo(),
        CommandSpec::new("vdb.pull", "Pull ShilpiDB into drawing", run_pull)
            .menu(&["File", "ShilpiDB", "Pull into drawing"])
            .params("{replace?: bool} → {addr, added, skipped} (pull records from the connected shilpid into model space; one undo step)"),
        CommandSpec::new("vdb.sidecar", "ShilpiDB sidecar on save", run_sidecar)
            .menu(&["File", "ShilpiDB", "Sidecar on save"])
            .params("{enabled?: bool, toggle?: bool} → {enabled} (when true, DXF/DWG save also writes `<stem>.vdb`; default false; toggle flips; omit both to read)")
            .noundo(),
        CommandSpec::new("vdb.status", "ShilpiDB status", run_status)
            .params("{} → {connected, addr?, sidecar}")
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
        let (store, truncated, skipped) = store_from_drawing(s)?;
        let count = store.len();
        save_path(&path, &store).map_err(|e| bad("vdb.export", format!("{}: {e}", path.display())))?;
        Ok(json!({
            "path": path.to_string_lossy(),
            "count": count,
            "truncated": truncated,
            "skipped": skipped,
            "version": FORMAT_VERSION,
        }))
    }
}

fn run_import(s: &mut Session, p: &Value) -> Result<Value> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (s, p);
        return Err(bad("vdb.import", "files cannot be read on the web"));
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let path = path_param(p, "path", "vdb.import")?;
        let replace = bool_or(p, "replace", false);
        let store = load_path(&path).map_err(|e| bad("vdb.import", format!("{}: {e}", path.display())))?;
        let version = store.format_version();
        let (added, skipped) = import_store(s, &store, replace)?;
        Ok(json!({
            "path": path.to_string_lossy(),
            "added": added,
            "skipped": skipped,
            "version": version,
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
        let store = load_path(&path).map_err(|e| bad("vdb.info", format!("{}: {e}", path.display())))?;
        Ok(json!({
            "path": path.to_string_lossy(),
            "count": store.len(),
            "version": store.format_version(),
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
        let store = load_path(&path).map_err(|e| bad("vdb.query", format!("{}: {e}", path.display())))?;
        let ids = store.query_bbox(window);
        let matched = ids.len();
        let truncated = matched > limit;
        let shown_ids: Vec<u64> = ids.into_iter().take(limit).collect();
        let mut hits = Vec::with_capacity(shown_ids.len());
        for id in &shown_ids {
            let mut hit = json!({ "id": id });
            if let Some(record) = store.records().iter().find(|r| r.id == *id)
                && let Ok(payload) = decode_payload(&record.payload)
            {
                hit["type"] = json!(payload.geom.type_name());
                hit["layer"] = json!(payload.layer);
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

fn run_connect(s: &mut Session, p: &Value) -> Result<Value> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (s, p);
        return Err(bad("vdb.connect", "ShilpiDB server is not available on the web"));
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let addr = str_param(p, "addr").unwrap_or(DEFAULT_SHILPID).trim().to_string();
        if addr.is_empty() || addr.len() > 260 {
            return Err(bad("vdb.connect", "`addr` must be a non-empty host:port"));
        }
        let mut client = VdbClient::connect(&addr).map_err(|e| bad("vdb.connect", e.to_string()))?;
        client.ping().map_err(|e| bad("vdb.connect", e.to_string()))?;
        s.vdb_addr = Some(addr.clone());
        Ok(json!({ "addr": addr }))
    }
}

fn run_disconnect(s: &mut Session, _p: &Value) -> Result<Value> {
    s.vdb_addr = None;
    Ok(json!({ "ok": true }))
}

fn run_push(s: &mut Session, p: &Value) -> Result<Value> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (s, p);
        return Err(bad("vdb.push", "ShilpiDB server is not available on the web"));
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let addr = s.vdb_addr.clone().ok_or_else(|| bad("vdb.push", "not connected; run vdb.connect first"))?;
        let replace = bool_or(p, "replace", false);
        let (store, truncated, _skipped) = store_from_drawing(s)?;
        let count = store.len();
        let mut client = VdbClient::connect(&addr).map_err(|e| bad("vdb.push", e.to_string()))?;
        client.push_store(&store, replace).map_err(|e| bad("vdb.push", e.to_string()))?;
        client.save().map_err(|e| bad("vdb.push", e.to_string()))?;
        Ok(json!({ "addr": addr, "count": count, "truncated": truncated, "replaced": replace }))
    }
}

fn run_pull(s: &mut Session, p: &Value) -> Result<Value> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (s, p);
        return Err(bad("vdb.pull", "ShilpiDB server is not available on the web"));
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let addr = s.vdb_addr.clone().ok_or_else(|| bad("vdb.pull", "not connected; run vdb.connect first"))?;
        let replace = bool_or(p, "replace", false);
        let mut client = VdbClient::connect(&addr).map_err(|e| bad("vdb.pull", e.to_string()))?;
        let store = client.pull_store().map_err(|e| bad("vdb.pull", e.to_string()))?;
        let (added, skipped) = import_store(s, &store, replace)?;
        Ok(json!({ "addr": addr, "added": added, "skipped": skipped }))
    }
}

fn run_sidecar(s: &mut Session, p: &Value) -> Result<Value> {
    if let Some(v) = p.get("enabled").and_then(Value::as_bool) {
        s.settings.vdb_sidecar = v;
    } else if p.get("toggle").and_then(Value::as_bool) == Some(true) {
        s.settings.vdb_sidecar = !s.settings.vdb_sidecar;
    }
    Ok(json!({ "enabled": s.settings.vdb_sidecar }))
}

fn run_status(s: &mut Session, _p: &Value) -> Result<Value> {
    match &s.vdb_addr {
        Some(addr) => Ok(json!({ "connected": true, "addr": addr, "sidecar": s.settings.vdb_sidecar })),
        None => Ok(json!({ "connected": false, "sidecar": s.settings.vdb_sidecar })),
    }
}

/// After a DXF/DWG save, write `<stem>.vdb` when the sidecar preference is on.
pub fn write_sidecar_if_enabled(s: &Session, drawing_path: &Path) -> Result<()> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (s, drawing_path);
        return Ok(());
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        if !s.settings.vdb_sidecar {
            return Ok(());
        }
        let Some(stem) = drawing_path.file_stem().and_then(|s| s.to_str()) else {
            return Ok(());
        };
        let parent = drawing_path.parent().unwrap_or_else(|| Path::new("."));
        let vdb_path = parent.join(format!("{stem}.vdb"));
        let (store, _, _) = store_from_drawing_ref(s)?;
        save_path(&vdb_path, &store).map_err(|e| bad("vdb.sidecar", format!("{}: {e}", vdb_path.display())))?;
        Ok(())
    }
}

fn store_from_drawing(s: &Session) -> Result<(VdbStore, bool, usize)> {
    store_from_drawing_ref(s)
}

fn store_from_drawing_ref(s: &Session) -> Result<(VdbStore, bool, usize)> {
    let drawing = s.doc()?;
    let mut records = Vec::new();
    let mut truncated = false;
    let mut skipped = 0usize;
    let Some(entities) = drawing.space(&Space::Model) else {
        return Err(bad("vdb.export", "no model space"));
    };
    for entity in entities.iter().take(MAX_SCAN) {
        if records.len() >= MAX_EXPORT {
            truncated = true;
            break;
        }
        let bounds = entity_bounds(drawing, entity, 0);
        if bounds.is_empty() || !finite_bounds(&bounds) {
            skipped += 1;
            continue;
        }
        let Some(payload) = entity_to_payload(entity) else {
            skipped += 1;
            continue;
        };
        let bytes = match encode_payload(&payload) {
            Ok(b) => b,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        records.push(VdbRecord {
            id: entity.handle.0,
            bbox: Bbox::from_corners(bounds.min.x, bounds.min.y, bounds.max.x, bounds.max.y),
            payload: bytes,
        });
    }
    let store = VdbStore::from_records(records).map_err(|e| bad("vdb.export", e.to_string()))?;
    Ok((store, truncated, skipped))
}

fn import_store(s: &mut Session, store: &VdbStore, replace: bool) -> Result<(usize, usize)> {
    let drawing = s.doc_mut()?;
    if replace && let Some(model) = drawing.space_mut(&Space::Model) {
        *model = aad_doc::EntityStore::new();
    }
    let mut added = 0usize;
    let mut skipped = 0usize;
    for record in store.records().iter().take(MAX_EXPORT) {
        let Ok(payload) = decode_payload(&record.payload) else {
            skipped += 1;
            continue;
        };
        let Some(kind) = payload_to_kind(&payload.geom) else {
            skipped += 1;
            continue;
        };
        let mut common = Common::default();
        if !payload.layer.is_empty() {
            common.layer = payload.layer.clone();
        }
        match drawing.add(&Space::Model, common, kind) {
            Ok(_) => added += 1,
            Err(_) => skipped += 1,
        }
    }
    Ok((added, skipped))
}

fn entity_to_payload(entity: &aad_doc::Entity) -> Option<EntityPayload> {
    let layer = entity.common.layer.clone();
    let geom = match &entity.kind {
        EntityKind::Line(l) => EntityGeom::Line { x0: l.a.x, y0: l.a.y, x1: l.b.x, y1: l.b.y },
        EntityKind::Circle(c) => EntityGeom::Circle { cx: c.center.x, cy: c.center.y, radius: c.radius },
        EntityKind::Arc(a) => EntityGeom::Arc { cx: a.center.x, cy: a.center.y, radius: a.radius, start: a.start, end: a.end },
        EntityKind::LwPolyline(p) => {
            let vertices: Vec<(f64, f64)> = p.vertices.iter().take(MAX_POLY_VERTICES).map(|v| (v.p.x, v.p.y)).collect();
            EntityGeom::LwPolyline { closed: p.closed, vertices }
        }
        EntityKind::Text(t) => {
            EntityGeom::Text { x: t.insert.x, y: t.insert.y, height: t.height, rotation: t.rotation, value: truncate_chars(&t.value, MAX_TEXT_CHARS) }
        }
        EntityKind::MText(t) => {
            EntityGeom::MText { x: t.insert.x, y: t.insert.y, height: t.height, contents: truncate_chars(&t.contents, MAX_TEXT_CHARS) }
        }
        EntityKind::Insert(i) => EntityGeom::Insert {
            x: i.insert.x,
            y: i.insert.y,
            rotation: i.rotation,
            sx: i.scale.x,
            sy: i.scale.y,
            block: truncate_chars(&i.block, MAX_BLOCK_CHARS),
        },
        other => EntityGeom::Stub { type_name: other.type_name().to_string() },
    };
    Some(EntityPayload { layer, geom })
}

fn payload_to_kind(geom: &EntityGeom) -> Option<EntityKind> {
    Some(match geom {
        EntityGeom::Stub { .. } => return None,
        EntityGeom::Line { x0, y0, x1, y1 } => EntityKind::Line(Line { a: Vec3::new(*x0, *y0, 0.0), b: Vec3::new(*x1, *y1, 0.0) }),
        EntityGeom::Circle { cx, cy, radius } => EntityKind::Circle(Circle { center: Vec3::new(*cx, *cy, 0.0), radius: *radius }),
        EntityGeom::Arc { cx, cy, radius, start, end } => {
            EntityKind::Arc(Arc { center: Vec3::new(*cx, *cy, 0.0), radius: *radius, start: *start, end: *end })
        }
        EntityGeom::LwPolyline { closed, vertices } => EntityKind::LwPolyline(LwPolyline {
            vertices: vertices.iter().map(|(x, y)| PolyVertex::new(Vec2::new(*x, *y))).collect(),
            closed: *closed,
            const_width: 0.0,
            elevation: 0.0,
            plinegen: false,
        }),
        EntityGeom::Text { x, y, height, rotation, value } => EntityKind::Text(Text {
            insert: Vec3::new(*x, *y, 0.0),
            align_pt: None,
            height: (*height).max(1e-9),
            value: value.clone(),
            rotation: *rotation,
            width_factor: 1.0,
            oblique: 0.0,
            style: "Standard".into(),
            halign: Default::default(),
            valign: Default::default(),
        }),
        EntityGeom::MText { x, y, height, contents } => EntityKind::MText(MText {
            insert: Vec3::new(*x, *y, 0.0),
            height: (*height).max(1e-9),
            width: 0.0,
            attach: 1,
            rotation: 0.0,
            style: "Standard".into(),
            contents: contents.clone(),
            line_spacing: 1.0,
            line_spacing_exact: false,
        }),
        EntityGeom::Insert { x, y, rotation, sx, sy, block } => EntityKind::Insert(Insert {
            block: block.clone(),
            insert: Vec3::new(*x, *y, 0.0),
            scale: Vec3::new(*sx, *sy, 1.0),
            rotation: *rotation,
            attribs: Vec::new(),
            cols: 1,
            rows: 1,
            col_spacing: 0.0,
            row_spacing: 0.0,
        }),
    })
}

fn truncate_chars(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

fn path_param(p: &Value, key: &str, cmd: &str) -> Result<PathBuf> {
    let raw = p.get(key).and_then(|v| v.as_str()).unwrap_or("").trim();
    if raw.is_empty() {
        return Err(bad(cmd, format!("missing `{key}`")));
    }
    Ok(PathBuf::from(raw))
}

fn window_param(p: &Value, cmd: &str) -> Result<Bbox> {
    let arr = p.get("window").and_then(|v| v.as_array()).ok_or_else(|| bad(cmd, "missing `window: [x0, y0, x1, y1]`"))?;
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
    use serde_json::json;

    #[test]
    fn export_import_round_trip_line() {
        let dir = tempfile::tempdir().expect("temp");
        let path = dir.path().join("sample.vdb");
        let mut s = Session::new();
        let a = Vec3::new(0.0, 0.0, 0.0);
        let b = Vec3::new(10.0, 0.0, 0.0);
        s.doc_mut().expect("doc").add(&Space::Model, Common::default(), EntityKind::Line(Line { a, b })).expect("add");
        let path_str = path.to_string_lossy().to_string();
        let exported = s.execute("vdb.export", &json!({ "path": path_str })).expect("export");
        assert_eq!(exported["count"], 1);
        assert_eq!(exported["version"], FORMAT_VERSION);

        let mut t = Session::new();
        let imported = t.execute("vdb.import", &json!({ "path": path_str, "replace": true })).expect("import");
        assert_eq!(imported["added"], 1);
        let n = t.doc().expect("doc").space(&Space::Model).map(|m| m.len()).unwrap_or(0);
        assert_eq!(n, 1);
    }

    #[test]
    fn sidecar_pref_toggle_and_query_hits() {
        let dir = tempfile::tempdir().expect("temp");
        let path = dir.path().join("q.vdb");
        let mut s = Session::new();
        assert_eq!(s.execute("vdb.sidecar", &json!({})).unwrap()["enabled"], false);
        assert_eq!(s.execute("vdb.sidecar", &json!({ "enabled": true })).unwrap()["enabled"], true);
        assert!(s.settings.vdb_sidecar);
        assert_eq!(s.execute("vdb.sidecar", &json!({ "toggle": true })).unwrap()["enabled"], false);

        s.doc_mut()
            .expect("doc")
            .add(&Space::Model, Common::default(), EntityKind::Line(Line { a: Vec3::new(0.0, 0.0, 0.0), b: Vec3::new(5.0, 0.0, 0.0) }))
            .expect("add");
        let path_str = path.to_string_lossy().to_string();
        s.execute("vdb.export", &json!({ "path": path_str })).expect("export");
        let q = s.execute("vdb.query", &json!({ "path": path_str, "window": [-1.0, -1.0, 6.0, 1.0], "limit": 10 })).expect("query");
        assert_eq!(q["matched"], 1);
        assert_eq!(q["hits"].as_array().expect("hits").len(), 1);
        assert_eq!(q["hits"][0]["type"], "Line");

        let info = s.execute("vdb.info", &json!({ "path": path_str })).expect("info");
        assert_eq!(info["version"], FORMAT_VERSION);
        assert_eq!(info["count"], 1);
    }

    #[test]
    fn connect_without_server_reports_error() {
        let mut s = Session::new();
        let err = s.execute("vdb.connect", &json!({ "addr": "127.0.0.1:1" })).expect_err("nothing listens on port 1");
        assert!(err.to_string().contains("vdb.connect"), "{err}");
        assert_eq!(s.execute("vdb.status", &json!({})).unwrap()["connected"], false);
    }
}
