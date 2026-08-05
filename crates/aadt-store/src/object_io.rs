//! Persistence for the Object DB + Graph DB + object audit log inside `.aadt`
//! (schema v8+; M14.3 hash columns on audit rows at v10).
//!
//! Objects/edges are replaced wholesale on write (they are derived, promotable
//! state); the audit log is **append-only** — existing rows are never rewritten,
//! only missing sequence numbers are inserted.

use std::collections::BTreeMap;
use std::path::Path;

use aadt_graph::{Edge, Graph, Relation};
use aadt_model::EntityId;
use aadt_object::{AuditEntry, AuditLog, EngObject, ObjectId, ObjectKind, ObjectStore};
use rusqlite::{params, Connection};

use crate::StoreError;

/// Write objects + graph + audit into an open connection.
pub fn write_objects(
    conn: &Connection,
    objects: &ObjectStore,
    graph: &Graph,
    audit: &AuditLog,
) -> Result<(), StoreError> {
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM object_entities", [])?;
    tx.execute("DELETE FROM object_props", [])?;
    tx.execute("DELETE FROM objects", [])?;
    tx.execute("DELETE FROM graph_edges", [])?;
    {
        let mut o = tx.prepare(
            "INSERT INTO objects (id, kind, confidence, revision) VALUES (?1, ?2, ?3, ?4)",
        )?;
        let mut e = tx.prepare(
            "INSERT INTO object_entities (object_id, seq, entity_id) VALUES (?1, ?2, ?3)",
        )?;
        let mut p =
            tx.prepare("INSERT INTO object_props (object_id, key, value) VALUES (?1, ?2, ?3)")?;
        for obj in objects.iter() {
            o.execute(params![
                obj.id.0 as i64,
                obj.kind.as_str(),
                obj.confidence as f64,
                obj.revision as i64,
            ])?;
            for (i, ent) in obj.entities.iter().enumerate() {
                e.execute(params![obj.id.0 as i64, i as i64, ent.0 as i64])?;
            }
            for (k, v) in &obj.props {
                p.execute(params![obj.id.0 as i64, k, v])?;
            }
        }
    }
    {
        let mut g =
            tx.prepare("INSERT INTO graph_edges (from_id, rel, to_id) VALUES (?1, ?2, ?3)")?;
        for edge in graph.edges() {
            g.execute(params![
                edge.from.0 as i64,
                edge.rel.as_str(),
                edge.to.0 as i64
            ])?;
        }
    }
    {
        // Append-only: insert audit rows whose seq is not stored yet.
        let mut a = tx.prepare(
            "INSERT OR IGNORE INTO object_audit
             (seq, at_unix, user, action, object_id, prev, new, ai, approved, reason, prev_hash, hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        )?;
        for e in audit.entries() {
            a.execute(params![
                e.seq as i64,
                e.at_unix as i64,
                e.user,
                e.action,
                e.object.0 as i64,
                e.prev,
                e.new,
                i64::from(e.ai_suggested),
                i64::from(e.approved),
                e.reason,
                e.prev_hash,
                e.hash,
            ])?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// Read objects + graph + audit from an open connection (`None` if the tables
/// are absent or hold no objects and no audit rows).
#[allow(clippy::type_complexity)]
pub fn read_objects(
    conn: &Connection,
) -> Result<Option<(ObjectStore, Graph, AuditLog)>, StoreError> {
    let has: bool = conn
        .prepare("SELECT 1 FROM sqlite_master WHERE type='table' AND name='objects'")?
        .exists([])?;
    if !has {
        return Ok(None);
    }

    let mut store = ObjectStore::new();
    {
        let mut q =
            conn.prepare("SELECT id, kind, confidence, revision FROM objects ORDER BY id")?;
        let rows = q.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)? as u64,
                r.get::<_, String>(1)?,
                r.get::<_, f64>(2)? as f32,
                r.get::<_, i64>(3)? as u64,
            ))
        })?;
        for row in rows {
            let (id, kind_s, confidence, revision) = row?;
            let Some(kind) = ObjectKind::parse(&kind_s) else {
                return Err(StoreError::Format(format!(
                    "unknown object kind {kind_s:?}"
                )));
            };
            let mut entities = Vec::new();
            {
                let mut eq = conn.prepare(
                    "SELECT entity_id FROM object_entities WHERE object_id = ?1 ORDER BY seq",
                )?;
                let ents = eq.query_map(params![id as i64], |r| r.get::<_, i64>(0))?;
                for e in ents {
                    entities.push(EntityId(e? as u64));
                }
            }
            let mut props = BTreeMap::new();
            {
                let mut pq = conn.prepare(
                    "SELECT key, value FROM object_props WHERE object_id = ?1 ORDER BY key",
                )?;
                let ps = pq.query_map(params![id as i64], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })?;
                for p in ps {
                    let (k, v) = p?;
                    props.insert(k, v);
                }
            }
            store.restore(EngObject {
                id: ObjectId(id),
                kind,
                entities,
                props,
                confidence,
                revision,
            });
        }
    }

    let mut graph = Graph::new();
    {
        let mut q = conn
            .prepare("SELECT from_id, rel, to_id FROM graph_edges ORDER BY from_id, rel, to_id")?;
        let rows = q.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)? as u64,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)? as u64,
            ))
        })?;
        for row in rows {
            let (from, rel_s, to) = row?;
            let Some(rel) = Relation::parse(&rel_s) else {
                return Err(StoreError::Format(format!("unknown relation {rel_s:?}")));
            };
            graph.restore(Edge {
                from: ObjectId(from),
                rel,
                to: ObjectId(to),
            });
        }
    }

    let mut audit = AuditLog::new();
    {
        // Ensure M14.3 hash columns exist (no-op when already present).
        let _ = conn.execute(
            "ALTER TABLE object_audit ADD COLUMN prev_hash TEXT NOT NULL DEFAULT ''",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE object_audit ADD COLUMN hash TEXT NOT NULL DEFAULT ''",
            [],
        );
        let mut q = conn.prepare(
            "SELECT seq, at_unix, user, action, object_id, prev, new, ai, approved, reason,
                    prev_hash, hash
             FROM object_audit ORDER BY seq",
        )?;
        let rows = q.query_map([], |r| {
            Ok(AuditEntry {
                seq: r.get::<_, i64>(0)? as u64,
                at_unix: r.get::<_, i64>(1)? as u64,
                user: r.get(2)?,
                action: r.get(3)?,
                object: ObjectId(r.get::<_, i64>(4)? as u64),
                prev: r.get(5)?,
                new: r.get(6)?,
                ai_suggested: r.get::<_, i64>(7)? != 0,
                approved: r.get::<_, i64>(8)? != 0,
                reason: r.get(9)?,
                prev_hash: r.get(10)?,
                hash: r.get(11)?,
            })
        })?;
        for row in rows {
            audit.restore(row?);
        }
    }

    if store.is_empty() && audit.is_empty() {
        return Ok(None);
    }
    Ok(Some((store, graph, audit)))
}

/// Open `path` and write objects/graph/audit (creating tables if needed).
pub fn write_objects_path(
    path: impl AsRef<Path>,
    objects: &ObjectStore,
    graph: &Graph,
    audit: &AuditLog,
) -> Result<(), StoreError> {
    let conn = Connection::open(path.as_ref())?;
    crate::ensure_extended_tables(&conn)?;
    write_objects(&conn, objects, graph, audit)
}

/// Open `path` and read its stored objects/graph/audit, or `None` if absent.
#[allow(clippy::type_complexity)]
pub fn read_objects_path(
    path: impl AsRef<Path>,
) -> Result<Option<(ObjectStore, Graph, AuditLog)>, StoreError> {
    let conn = Connection::open(path.as_ref())?;
    read_objects(&conn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::save_path;
    use aadt_geometry::{Point2, Segment};
    use aadt_model::{Document, Geometry};

    fn sample() -> (ObjectStore, Graph, AuditLog) {
        let mut store = ObjectStore::new();
        let mut props = BTreeMap::new();
        props.insert("thickness".into(), "0.230".into());
        props.insert("length".into(), "6.000".into());
        let wall = store.insert(
            ObjectKind::Wall,
            vec![EntityId(1), EntityId(2)],
            props,
            1.0,
            5,
        );
        let room = store.insert(ObjectKind::Room, vec![EntityId(3)], BTreeMap::new(), 1.0, 5);
        let mut graph = Graph::new();
        graph.add(wall, Relation::Bounds, room);
        let mut audit = AuditLog::new();
        audit.record("promote", wall, "", "Wall …", false, true, "detect");
        audit.record("promote", room, "", "Room …", false, true, "detect");
        (store, graph, audit)
    }

    #[test]
    fn objects_graph_audit_round_trip() {
        let mut doc = Document::new();
        doc.add_geometry(
            "0",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(1.0, 0.0))),
        );
        let dir = std::env::temp_dir().join("aadt-object-io");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("obj.aadt");
        let _ = std::fs::remove_file(&path);
        save_path(&doc, &path).unwrap();

        let (store, graph, audit) = sample();
        write_objects_path(&path, &store, &graph, &audit).unwrap();

        let (s2, g2, a2) = read_objects_path(&path).unwrap().expect("stored");
        assert_eq!(s2.len(), 2);
        let wall = s2.get(ObjectId(1)).unwrap();
        assert_eq!(wall.kind, ObjectKind::Wall);
        assert_eq!(wall.entities, vec![EntityId(1), EntityId(2)]);
        assert_eq!(wall.prop("thickness"), Some("0.230"));
        assert_eq!(g2.edges().len(), 1);
        assert_eq!(g2.edges()[0].rel, Relation::Bounds);
        assert_eq!(a2.len(), 2);
        assert_eq!(a2.entries()[0].action, "promote");
    }

    #[test]
    fn objects_survive_a_plain_resave_and_audit_appends() {
        let mut doc = Document::new();
        doc.add_geometry(
            "0",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(1.0, 0.0))),
        );
        let dir = std::env::temp_dir().join("aadt-object-io-resave");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("obj.aadt");
        let _ = std::fs::remove_file(&path);
        save_path(&doc, &path).unwrap();
        let (store, graph, mut audit) = sample();
        write_objects_path(&path, &store, &graph, &audit).unwrap();

        // Plain save (no objects passed) must carry the tables forward.
        doc.add_geometry(
            "0",
            Geometry::Line(Segment::new(Point2::new(0.0, 1.0), Point2::new(1.0, 1.0))),
        );
        save_path(&doc, &path).unwrap();
        let (s2, g2, a2) = read_objects_path(&path).unwrap().expect("carried");
        assert_eq!(s2.len(), 2);
        assert_eq!(g2.len(), 1);
        assert_eq!(a2.len(), 2);

        // Appending later audit entries keeps earlier ones (append-only).
        audit.record(
            "update",
            ObjectId(1),
            "Wall …",
            "Wall …2",
            true,
            true,
            "ai ok",
        );
        write_objects_path(&path, &store, &graph, &audit).unwrap();
        let (_, _, a3) = read_objects_path(&path).unwrap().unwrap();
        assert_eq!(a3.len(), 3);
        assert_eq!(a3.entries()[2].action, "update");
    }

    #[test]
    fn absent_objects_read_as_none() {
        let mut doc = Document::new();
        doc.add_geometry(
            "0",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(1.0, 0.0))),
        );
        let dir = std::env::temp_dir().join("aadt-object-io-none");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("noobj.aadt");
        let _ = std::fs::remove_file(&path);
        save_path(&doc, &path).unwrap();
        assert!(read_objects_path(&path).unwrap().is_none());
    }
}
