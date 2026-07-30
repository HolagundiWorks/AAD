//! Drawing time machine — Sheets-style checkpoints inside `.aadt`.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use aadt_model::Document;
use rusqlite::{params, Connection, OptionalExtension};

use crate::{init_schema_tables, read_document, write_document, StoreError, SCHEMA_VERSION};

/// Max number of `auto` checkpoints retained (named ones are never pruned).
pub const MAX_AUTO_CHECKPOINTS: usize = 20;

/// Metadata for one history checkpoint (blob not included).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointMeta {
    pub id: i64,
    pub created_at: String,
    pub label: String,
    pub kind: String,
    pub doc_revision: u64,
    pub note: String,
    pub author: String,
    pub change_summary: String,
}

fn now_rfc3339_approx() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}

fn change_summary(doc: &Document) -> String {
    format!(
        "{} ents, {} dims, {} hatches, {} layers",
        doc.len(),
        doc.dimensions().len(),
        doc.hatches().len(),
        doc.layers().count()
    )
}

/// Serialize `doc` to a portable SQLite blob (model tables only).
pub fn document_to_blob(doc: &Document) -> Result<Vec<u8>, StoreError> {
    let mem = Connection::open_in_memory()?;
    init_schema_tables(&mem)?;
    write_document(&mem, doc)?;
    let tmp = std::env::temp_dir().join(format!(
        "aadt-chk-{}.aadt",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    // VACUUM INTO produces a compact on-disk DB we can read as bytes.
    let path_str = tmp.to_string_lossy().replace('\'', "''");
    mem.execute_batch(&format!("VACUUM INTO '{path_str}'"))?;
    let bytes = std::fs::read(&tmp)?;
    let _ = std::fs::remove_file(&tmp);
    Ok(bytes)
}

/// Restore a document from a checkpoint blob.
pub fn document_from_blob(blob: &[u8]) -> Result<Document, StoreError> {
    let tmp = std::env::temp_dir().join(format!(
        "aadt-chk-load-{}.aadt",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::write(&tmp, blob)?;
    let conn = Connection::open(&tmp)?;
    let doc = read_document(&conn)?;
    drop(conn);
    let _ = std::fs::remove_file(&tmp);
    Ok(doc)
}

/// Copy all history / takeoff tables from `src` into `dst` (same schema).
pub fn copy_extended_tables(src: &Connection, dst: &Connection) -> Result<(), StoreError> {
    let has_hist: bool = src
        .prepare("SELECT 1 FROM sqlite_master WHERE type='table' AND name='history_checkpoint'")?
        .exists([])?;
    if !has_hist {
        return Ok(());
    }
    // history_checkpoint + blob + change
    {
        let mut q = src.prepare(
            "SELECT id, created_at, label, kind, doc_revision, note, author FROM history_checkpoint ORDER BY id",
        )?;
        let rows: Vec<_> = q
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut ins = dst.prepare(
            "INSERT INTO history_checkpoint (id, created_at, label, kind, doc_revision, note, author)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )?;
        for row in &rows {
            ins.execute(params![row.0, row.1, row.2, row.3, row.4, row.5, row.6])?;
        }
    }
    {
        let mut q = src.prepare("SELECT checkpoint_id, blob FROM history_blob")?;
        let rows: Vec<_> = q
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        let mut ins =
            dst.prepare("INSERT INTO history_blob (checkpoint_id, blob) VALUES (?1, ?2)")?;
        for (id, blob) in rows {
            ins.execute(params![id, blob])?;
        }
    }
    {
        let mut q = src.prepare("SELECT checkpoint_id, op_summary FROM history_change")?;
        let rows: Vec<_> = q
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        let mut ins =
            dst.prepare("INSERT INTO history_change (checkpoint_id, op_summary) VALUES (?1, ?2)")?;
        for (id, s) in rows {
            ins.execute(params![id, s])?;
        }
    }
    copy_takeoff_tables(src, dst)?;
    copy_vec_tables(src, dst)?;
    copy_object_tables(src, dst)?;
    Ok(())
}

/// Carry the Object DB / Graph DB / audit tables (schema v8) across an atomic
/// save rewrite, so a plain `SAVE` preserves promoted objects and the
/// append-only audit trail.
fn copy_object_tables(src: &Connection, dst: &Connection) -> Result<(), StoreError> {
    let has: bool = src
        .prepare("SELECT 1 FROM sqlite_master WHERE type='table' AND name='objects'")?
        .exists([])?;
    if !has {
        return Ok(());
    }
    {
        let mut q = src.prepare("SELECT id, kind, confidence, revision FROM objects")?;
        let rows: Vec<(i64, String, f64, i64)> = q
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        let mut ins = dst.prepare(
            "INSERT INTO objects (id, kind, confidence, revision) VALUES (?1, ?2, ?3, ?4)",
        )?;
        for (a, b, c, d) in rows {
            ins.execute(params![a, b, c, d])?;
        }
    }
    {
        let mut q = src.prepare("SELECT object_id, seq, entity_id FROM object_entities")?;
        let rows: Vec<(i64, i64, i64)> = q
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        let mut ins = dst.prepare(
            "INSERT INTO object_entities (object_id, seq, entity_id) VALUES (?1, ?2, ?3)",
        )?;
        for (a, b, c) in rows {
            ins.execute(params![a, b, c])?;
        }
    }
    {
        let mut q = src.prepare("SELECT object_id, key, value FROM object_props")?;
        let rows: Vec<(i64, String, String)> = q
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        let mut ins =
            dst.prepare("INSERT INTO object_props (object_id, key, value) VALUES (?1, ?2, ?3)")?;
        for (a, b, c) in rows {
            ins.execute(params![a, b, c])?;
        }
    }
    {
        let mut q = src.prepare("SELECT from_id, rel, to_id FROM graph_edges")?;
        let rows: Vec<(i64, String, i64)> = q
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        let mut ins =
            dst.prepare("INSERT INTO graph_edges (from_id, rel, to_id) VALUES (?1, ?2, ?3)")?;
        for (a, b, c) in rows {
            ins.execute(params![a, b, c])?;
        }
    }
    {
        let mut q = src.prepare(
            "SELECT seq, at_unix, user, action, object_id, prev, new, ai, approved, reason
             FROM object_audit",
        )?;
        type AuditRow = (
            i64,
            i64,
            String,
            String,
            i64,
            String,
            String,
            i64,
            i64,
            String,
        );
        let rows: Vec<AuditRow> = q
            .query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                    r.get(8)?,
                    r.get(9)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut ins = dst.prepare(
            "INSERT INTO object_audit
             (seq, at_unix, user, action, object_id, prev, new, ai, approved, reason)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )?;
        for r in rows {
            ins.execute(params![r.0, r.1, r.2, r.3, r.4, r.5, r.6, r.7, r.8, r.9])?;
        }
    }
    Ok(())
}

/// Carry the embeddings vector-db tables forward across an atomic save rewrite,
/// so a plain `SAVE` that doesn't pass a fresh `VectorDb` preserves the stored
/// vectors and reconciliation statuses.
fn copy_vec_tables(src: &Connection, dst: &Connection) -> Result<(), StoreError> {
    let has: bool = src
        .prepare("SELECT 1 FROM sqlite_master WHERE type='table' AND name='vec_embedding'")?
        .exists([])?;
    if !has {
        return Ok(());
    }
    {
        let mut q = src.prepare("SELECT key, value FROM vec_meta")?;
        let rows: Vec<(String, String)> = q
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        let mut ins = dst.prepare(
            "INSERT INTO vec_meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )?;
        for (k, v) in rows {
            ins.execute(params![k, v])?;
        }
    }
    {
        let mut q = src.prepare(
            "SELECT entity_id, vec, pred_layer, pred_category, confidence, status FROM vec_embedding",
        )?;
        let rows: Vec<(i64, Vec<u8>, String, String, f64, String)> = q
            .query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut ins = dst.prepare(
            "INSERT INTO vec_embedding (entity_id, vec, pred_layer, pred_category, confidence, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        for (id, blob, layer, cat, conf, status) in rows {
            ins.execute(params![id, blob, layer, cat, conf, status])?;
        }
    }
    Ok(())
}

fn copy_takeoff_tables(src: &Connection, dst: &Connection) -> Result<(), StoreError> {
    // Only if tables exist (v5).
    let has: bool = src
        .prepare("SELECT 1 FROM sqlite_master WHERE type='table' AND name='takeoff_run'")?
        .exists([])?;
    if !has {
        return Ok(());
    }
    {
        let mut q = src.prepare(
            "SELECT id, at, doc_revision, checkpoint_id, schema_ver, totals_json, published FROM takeoff_run ORDER BY id",
        )?;
        let rows: Vec<_> = q
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, Option<i64>>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, i64>(6)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut ins = dst.prepare(
            "INSERT INTO takeoff_run (id, at, doc_revision, checkpoint_id, schema_ver, totals_json, published)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )?;
        for row in rows {
            ins.execute(params![row.0, row.1, row.2, row.3, row.4, row.5, row.6])?;
        }
    }
    for (table, cols) in [
        (
            "takeoff_wall",
            "run_id, wall_id, layer, length, thickness, height, gross_area, volume, cost_code, wall_type, class_status",
        ),
        (
            "takeoff_room",
            "run_id, room_id, layer, area, perimeter, cost_code",
        ),
        (
            "takeoff_opening",
            "run_id, number, kind, width, height, area, cost_code",
        ),
        (
            "takeoff_structural",
            "run_id, number, kind, cost_code",
        ),
        (
            "takeoff_delta",
            "from_run, to_run, summary",
        ),
    ] {
        let sql = format!("SELECT {cols} FROM {table}");
        let mut q = match src.prepare(&sql) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let col_count = cols.split(',').count();
        let placeholders = (1..=col_count)
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let insert_sql = format!("INSERT INTO {table} ({cols}) VALUES ({placeholders})");
        let mut ins = dst.prepare(&insert_sql)?;
        let mut rows = q.query([])?;
        while let Some(row) = rows.next()? {
            match col_count {
                2 => {
                    let a: i64 = row.get(0)?;
                    let b: String = row.get(1)?;
                    ins.execute(params![a, b])?;
                }
                4 => {
                    ins.execute(params![
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ])?;
                }
                6 => {
                    ins.execute(params![
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, f64>(3)?,
                        row.get::<_, f64>(4)?,
                        row.get::<_, String>(5)?,
                    ])?;
                }
                7 => {
                    ins.execute(params![
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, f64>(3)?,
                        row.get::<_, f64>(4)?,
                        row.get::<_, f64>(5)?,
                        row.get::<_, String>(6)?,
                    ])?;
                }
                11 => {
                    ins.execute(params![
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, f64>(3)?,
                        row.get::<_, f64>(4)?,
                        row.get::<_, f64>(5)?,
                        row.get::<_, f64>(6)?,
                        row.get::<_, f64>(7)?,
                        row.get::<_, String>(8)?,
                        row.get::<_, String>(9)?,
                        row.get::<_, String>(10)?,
                    ])?;
                }
                _ => {}
            }
        }
    }
    Ok(())
}

/// Append a checkpoint (used by save_path_with_checkpoint for named saves).
pub(crate) fn insert_checkpoint_pub(
    conn: &Connection,
    doc: &Document,
    label: &str,
    kind: &str,
    note: &str,
    author: &str,
) -> Result<i64, StoreError> {
    insert_checkpoint(conn, doc, label, kind, note, author)
}

fn insert_checkpoint(
    conn: &Connection,
    doc: &Document,
    label: &str,
    kind: &str,
    note: &str,
    author: &str,
) -> Result<i64, StoreError> {
    let summary = change_summary(doc);
    let blob = document_to_blob(doc)?;
    let created = now_rfc3339_approx();
    conn.execute(
        "INSERT INTO history_checkpoint (created_at, label, kind, doc_revision, note, author)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![created, label, kind, doc.revision() as i64, note, author],
    )?;
    let id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO history_blob (checkpoint_id, blob) VALUES (?1, ?2)",
        params![id, blob],
    )?;
    conn.execute(
        "INSERT INTO history_change (checkpoint_id, op_summary) VALUES (?1, ?2)",
        params![id, summary],
    )?;
    Ok(id)
}

/// Append an auto checkpoint and prune old autos.
pub fn append_auto_checkpoint(conn: &Connection, doc: &Document) -> Result<i64, StoreError> {
    let id = insert_checkpoint(conn, doc, "Auto save", "auto", "", "")?;
    prune_auto(conn, MAX_AUTO_CHECKPOINTS)?;
    Ok(id)
}

fn prune_auto(conn: &Connection, keep: usize) -> Result<(), StoreError> {
    let ids: Vec<i64> = {
        let mut q =
            conn.prepare("SELECT id FROM history_checkpoint WHERE kind = 'auto' ORDER BY id DESC")?;
        let all: Vec<i64> = q
            .query_map([], |r| r.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        if all.len() <= keep {
            return Ok(());
        }
        all.into_iter().skip(keep).collect()
    };
    for id in ids {
        conn.execute(
            "DELETE FROM history_blob WHERE checkpoint_id = ?1",
            params![id],
        )?;
        conn.execute(
            "DELETE FROM history_change WHERE checkpoint_id = ?1",
            params![id],
        )?;
        conn.execute("DELETE FROM history_checkpoint WHERE id = ?1", params![id])?;
    }
    Ok(())
}

/// List checkpoints in a `.aadt` file (newest first).
pub fn list_checkpoints(path: impl AsRef<Path>) -> Result<Vec<CheckpointMeta>, StoreError> {
    let conn = Connection::open(path.as_ref())?;
    ensure_v5_or_migrate(&conn)?;
    let mut q = conn.prepare(
        "SELECT c.id, c.created_at, c.label, c.kind, c.doc_revision, c.note, c.author,
                COALESCE(h.op_summary, '')
         FROM history_checkpoint c
         LEFT JOIN history_change h ON h.checkpoint_id = c.id
         ORDER BY c.id DESC",
    )?;
    let rows = q.query_map([], |r| {
        Ok(CheckpointMeta {
            id: r.get(0)?,
            created_at: r.get(1)?,
            label: r.get(2)?,
            kind: r.get(3)?,
            doc_revision: r.get::<_, i64>(4)? as u64,
            note: r.get(5)?,
            author: r.get(6)?,
            change_summary: r.get(7)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Save a named checkpoint into an existing `.aadt` file (also updates model tables).
pub fn save_named_checkpoint(
    path: impl AsRef<Path>,
    doc: &Document,
    label: &str,
    note: &str,
    author: &str,
) -> Result<i64, StoreError> {
    // Full save with named checkpoint instead of auto.
    crate::save_path_with_checkpoint(doc, path.as_ref(), Some((label, note, author)))
}

/// Load document from a checkpoint id inside `path`.
pub fn restore_checkpoint(path: impl AsRef<Path>, id: i64) -> Result<Document, StoreError> {
    let conn = Connection::open(path.as_ref())?;
    ensure_v5_or_migrate(&conn)?;
    let blob: Vec<u8> = conn
        .query_row(
            "SELECT blob FROM history_blob WHERE checkpoint_id = ?1",
            params![id],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| StoreError::Format(format!("no checkpoint blob for id {id}")))?;
    document_from_blob(&blob)
}

/// Delete all auto checkpoints (named retained). Returns count removed.
pub fn purge_auto_checkpoints(path: impl AsRef<Path>) -> Result<usize, StoreError> {
    let conn = Connection::open(path.as_ref())?;
    ensure_v5_or_migrate(&conn)?;
    let ids: Vec<i64> = {
        let mut q = conn.prepare("SELECT id FROM history_checkpoint WHERE kind = 'auto'")?;
        let collected = q
            .query_map([], |r| r.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        collected
    };
    let n = ids.len();
    for id in ids {
        conn.execute(
            "DELETE FROM history_blob WHERE checkpoint_id = ?1",
            params![id],
        )?;
        conn.execute(
            "DELETE FROM history_change WHERE checkpoint_id = ?1",
            params![id],
        )?;
        conn.execute("DELETE FROM history_checkpoint WHERE id = ?1", params![id])?;
    }
    Ok(n)
}

pub(crate) fn ensure_v5_or_migrate(conn: &Connection) -> Result<(), StoreError> {
    let ver: Option<i64> = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |r| {
                let s: String = r.get(0)?;
                Ok(s.parse::<i64>().unwrap_or(0))
            },
        )
        .optional()?;
    match ver {
        // For these read paths we only need the extended (history / takeoff /
        // vector-db) tables to exist; creating them is additive and version-safe.
        Some(v) if v <= SCHEMA_VERSION => {
            crate::ensure_extended_tables(conn)?;
            Ok(())
        }
        Some(v) => Err(StoreError::Format(format!(
            "unsupported schema_version {v} (want {SCHEMA_VERSION})"
        ))),
        None => Err(StoreError::Format("missing schema_version".into())),
    }
}
