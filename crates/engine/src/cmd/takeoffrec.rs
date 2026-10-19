//! `takeoff.record`: the take-off as ShilpiDB records, for O-CoreOp to read.
//!
//! The contract is `TAKEOFF-SHILPIDB.md` in the O-CoreOp repository, version 1: one header record
//! (`(1 << 56) | 0`) and one record per measured row (`(kind << 56) | n`, kinds 3 wall, 4 opening,
//! 5 column, 6 lintel, 7 room, 8 slab), the payload a UTF-8 JSON object exactly as `bridge.export`
//! writes the row, the bounding box in millimetres. The rows come from the same collection step as
//! `bridge.export`, so the two never disagree. Only measured facts are recorded: no rates, no
//! derived finishes.
//!
//! The database is a `.vdb` file beside the drawing (`house.dxf` → `house.takeoff.vdb`) unless
//! `server` names a running `shilpid`. Recording again makes the database equal the current
//! take-off: rows that are still measured are replaced in place, rows that no longer are, are
//! deleted.

use serde_json::{Map, Value, json};

use super::bridge::{self, Item, KIND_COLUMN, KIND_LINTEL, KIND_OPENING, KIND_ROOM, KIND_SLAB, KIND_WALL};
use super::*;
use crate::{Result, Session};

const FORMAT: &str = "hcw-takeoff-shilpi";
/// The header's record id: kind 1, number 0.
pub(crate) const HEADER_ID: u64 = 1 << 56;
const KIND_HEADER: u8 = 1;
/// The kinds this version writes. Records of any other kind belong to a later version and are left alone.
const OWN_KINDS: [u8; 7] = [KIND_HEADER, KIND_WALL, KIND_OPENING, KIND_COLUMN, KIND_LINTEL, KIND_ROOM, KIND_SLAB];
/// Rows one recording writes; a larger take-off is cut and the result says so.
const MAX_ROWS: usize = 500_000;
/// Records sent to a server in one call.
#[cfg(not(target_arch = "wasm32"))]
const BATCH: usize = 500;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("takeoff.record", "Record Take-off in ShilpiDB", run_record)
            .menu(&["Architecture", "Take-off", "Record in ShilpiDB"])
            .params("{path?: .vdb file (default: <drawing>.takeoff.vdb beside the saved drawing), server?: \"host:port\" of a running shilpid (written instead of a file), level?, bearing?} → {path | server, headerId, counts, written, added, changed, unchanged, deleted, skipped, warnings}")
            .noundo(),
    ]
}

/// The record id of item number `n` of `kind`.
pub(crate) fn record_id(kind: u8, n: u64) -> u64 {
    (u64::from(kind) << 56) | (n & 0x00FF_FFFF_FFFF_FFFF)
}

/// The kind in the top byte of a record id.
pub(crate) fn kind_of(id: u64) -> u8 {
    (id >> 56) as u8
}

fn kind_name(kind: u8) -> &'static str {
    match kind {
        KIND_WALL => "walls",
        KIND_OPENING => "openings",
        KIND_COLUMN => "columns",
        KIND_LINTEL => "lintels",
        KIND_ROOM => "rooms",
        KIND_SLAB => "slabs",
        _ => "other",
    }
}

/// A record as it is written: id, box in millimetres, payload bytes.
pub(crate) struct Rec {
    pub id: u64,
    pub bbox: [f64; 4],
    pub payload: Vec<u8>,
}

/// What a recording would write, and what it left out.
pub(crate) struct Built {
    pub records: Vec<Rec>,
    pub counts: std::collections::BTreeMap<&'static str, usize>,
    pub skipped: Vec<String>,
    pub warnings: Vec<String>,
}

/// The header payload: the bridge file's `source`, `project`, `levels`, `beam_depths_mm` and `opening_schedule`.
fn header(root: &Map<String, Value>) -> Value {
    let mut source = Map::new();
    source.insert("app".into(), json!("O-Draft"));
    if let Some(src) = root.get("source") {
        for key in ["drawing", "drawing_id", "exported"] {
            if let Some(v) = src.get(key) {
                source.insert(key.into(), v.clone());
            }
        }
        if let Some(sha) = src.get("revision").and_then(|r| r.get("sha256")) {
            source.insert("revision_sha256".into(), sha.clone());
        }
    }
    let mut h = Map::new();
    h.insert("format".into(), json!(FORMAT));
    h.insert("version".into(), json!(1));
    h.insert("units".into(), json!("mm"));
    h.insert("source".into(), Value::Object(source));
    for key in ["project", "levels", "beam_depths_mm", "opening_schedule"] {
        if let Some(v) = root.get(key) {
            h.insert(key.into(), v.clone());
        }
    }
    Value::Object(h)
}

pub(crate) fn build(s: &mut Session, p: &Value) -> Result<Built> {
    let c = bridge::collect(s, p)?;
    let mut warnings = c.warnings;
    let mut skipped = Vec::new();
    let mut counts = std::collections::BTreeMap::new();
    let header_bytes = serde_json::to_vec(&header(&c.root)).map_err(|e| bad("takeoff.record", e.to_string()))?;
    let mut records = vec![Rec { id: HEADER_ID, bbox: [0.0; 4], payload: header_bytes }];
    let mut seen = std::collections::HashSet::new();
    seen.insert(HEADER_ID);
    let mut cut = false;
    for Item { kind, n, row, bbox } in c.items {
        if records.len() > MAX_ROWS {
            cut = true;
            break;
        }
        let id = record_id(kind, n);
        if !seen.insert(id) {
            let name = row.get("ref").and_then(Value::as_str).unwrap_or("?");
            skipped.push(format!("{} {name}: another row has the same entity handle", kind_name(kind)));
            continue;
        }
        let payload = serde_json::to_vec(&row).map_err(|e| bad("takeoff.record", e.to_string()))?;
        *counts.entry(kind_name(kind)).or_insert(0usize) += 1;
        records.push(Rec { id, bbox, payload });
    }
    if cut {
        warnings.push(format!("more than {MAX_ROWS} rows: the rest were not recorded"));
    }
    records.sort_by_key(|r| r.id);
    Ok(Built { records, counts, skipped, warnings })
}

fn run_record(s: &mut Session, p: &Value) -> Result<Value> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (s, p);
        Err(bad("takeoff.record", "ShilpiDB files cannot be written on the web"))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let built = build(s, p)?;
        let mut out = match str_param(p, "server") {
            Some(addr) => write_server(addr, &built)?,
            None => {
                let path = match str_param(p, "path") {
                    Some(t) => std::path::PathBuf::from(t),
                    None => match s.state()?.path.as_ref() {
                        Some(drawing) => std::path::Path::new(drawing).with_extension("takeoff.vdb"),
                        None => return Err(bad("takeoff.record", "save the drawing first, or give `path` or `server`")),
                    },
                };
                write_file(&path, &built)?
            }
        };
        out["headerId"] = json!(HEADER_ID);
        out["counts"] = json!(built.counts);
        out["skipped"] = json!(built.skipped);
        out["warnings"] = json!(built.warnings);
        let text = built.counts.iter().map(|(k, n)| format!("{n} {k}")).collect::<Vec<_>>().join(", ");
        s.echo(format!("Take-off recorded: {}.", if text.is_empty() { "no measured rows".to_string() } else { text }));
        Ok(out)
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn vdb_record(r: &Rec) -> aad_vdb::VdbRecord {
    aad_vdb::VdbRecord { id: r.id, bbox: aad_vdb::Bbox::from_corners(r.bbox[0], r.bbox[1], r.bbox[2], r.bbox[3]), payload: r.payload.clone() }
}

/// Write the take-off as a `.vdb` file. An existing file must be a take-off database (it has the header
/// record) or be empty, so an entity store is never overwritten by mistake.
#[cfg(not(target_arch = "wasm32"))]
fn write_file(path: &std::path::Path, built: &Built) -> Result<Value> {
    let old = if path.exists() {
        let store = aad_vdb::load_path(path).map_err(|e| bad("takeoff.record", format!("{}: {e}", path.display())))?;
        if !store.is_empty() && !store.records().iter().any(|r| r.id == HEADER_ID) {
            return Err(bad(
                "takeoff.record",
                format!("{} is not a take-off database (it has no header record); choose another file", path.display()),
            ));
        }
        Some(store)
    } else {
        None
    };
    let new_ids: std::collections::HashMap<u64, &Rec> = built.records.iter().map(|r| (r.id, r)).collect();
    let (mut added, mut changed, mut unchanged, mut deleted) = (0usize, 0usize, 0usize, 0usize);
    let mut all: Vec<aad_vdb::VdbRecord> = Vec::new();
    let old_records: Vec<aad_vdb::VdbRecord> = old.map(|s| s.records().to_vec()).unwrap_or_default();
    for r in &old_records {
        if OWN_KINDS.contains(&kind_of(r.id)) {
            match new_ids.get(&r.id) {
                None => deleted += 1,
                Some(n) if n.payload == r.payload && vdb_record(n).bbox == r.bbox => unchanged += 1,
                Some(_) => changed += 1,
            }
        } else {
            // A kind from a later version of the contract: kept as it is.
            all.push(r.clone());
        }
    }
    let old_ids: std::collections::HashSet<u64> = old_records.iter().map(|r| r.id).collect();
    added += built.records.iter().filter(|r| !old_ids.contains(&r.id)).count();
    all.extend(built.records.iter().map(vdb_record));
    all.sort_by_key(|r| r.id);
    let store = aad_vdb::VdbStore::from_records(all).map_err(|e| bad("takeoff.record", e.to_string()))?;
    aad_vdb::save_path(path, &store).map_err(|e| bad("takeoff.record", format!("{}: {e}", path.display())))?;
    Ok(
        json!({ "path": path.display().to_string(), "written": built.records.len(), "added": added, "changed": changed, "unchanged": unchanged, "deleted": deleted }),
    )
}

/// Write the take-off to a running `shilpid`: insert every record (an existing id is replaced), delete
/// the take-off records that are no longer measured, then ask the server to save.
#[cfg(not(target_arch = "wasm32"))]
fn write_server(addr: &str, built: &Built) -> Result<Value> {
    let fail = |e: aad_vdb::ClientError| bad("takeoff.record", format!("{addr}: {e}"));
    let mut client = aad_vdb::VdbClient::connect_with_timeout(addr, Some(std::time::Duration::from_secs(10))).map_err(fail)?;
    let existing: std::collections::HashSet<u64> = client.list().map_err(fail)?.into_iter().collect();
    let new_ids: std::collections::HashSet<u64> = built.records.iter().map(|r| r.id).collect();
    let stale: Vec<u64> = existing.iter().copied().filter(|id| OWN_KINDS.contains(&kind_of(*id)) && !new_ids.contains(id)).collect();
    let added = built.records.iter().filter(|r| !existing.contains(&r.id)).count();
    for chunk in built.records.chunks(BATCH) {
        let batch =
            chunk.iter().map(|r| (r.id, aad_vdb::Bbox::from_corners(r.bbox[0], r.bbox[1], r.bbox[2], r.bbox[3]), r.payload.clone())).collect();
        client.insert_many(batch).map_err(fail)?;
    }
    let mut deleted = 0usize;
    for id in stale {
        if client.delete(id).map_err(fail)? {
            deleted += 1;
        }
    }
    client.save().map_err(fail)?;
    Ok(json!({ "server": addr, "written": built.records.len(), "added": added, "deleted": deleted }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_carry_the_kind_in_the_top_byte_and_the_header_is_fixed() {
        assert_eq!(HEADER_ID, 72_057_594_037_927_936);
        assert_eq!(record_id(1, 0), HEADER_ID);
        assert_eq!(record_id(3, 0x1A2), (3u64 << 56) | 0x1A2);
        assert_eq!(kind_of(record_id(8, 77)), 8);
        // A number wider than 56 bits cannot spill into the kind.
        assert_eq!(kind_of(record_id(3, u64::MAX)), 3);
    }
}
