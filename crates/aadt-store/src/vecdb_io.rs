//! Persistence for the embeddings vector database inside `.aadt` (schema v7).
//!
//! Two tables hold it: `vec_meta` (a small key/value store for the model id and
//! embedding dimension) and `vec_embedding` (one row per entity — the raw
//! little-endian `f32` vector plus the predicted layer / category / confidence /
//! reconciliation status). Embeddings are a recomputable cache; the meaningful
//! persisted state is the metadata mapping and its status.

use std::path::Path;

use aadt_model::{EntityId, SpaceCategory};
use aadt_vecdb::{MappingStatus, VecEntry, VectorDb};
use rusqlite::{params, Connection};

use crate::StoreError;

fn vec_to_blob(v: &[f32]) -> Vec<u8> {
    let mut b = Vec::with_capacity(v.len() * 4);
    for x in v {
        b.extend_from_slice(&x.to_le_bytes());
    }
    b
}

fn blob_to_vec(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn table_exists(conn: &Connection, name: &str) -> Result<bool, StoreError> {
    Ok(conn
        .prepare("SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1")?
        .exists(params![name])?)
}

/// Write `db` into an open connection's vector tables, replacing prior rows.
pub fn write_vecdb(conn: &Connection, db: &VectorDb) -> Result<(), StoreError> {
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM vec_embedding", [])?;
    {
        let mut m = tx.prepare(
            "INSERT INTO vec_meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )?;
        m.execute(params!["model_id", db.model_id()])?;
        m.execute(params!["dim", db.dim().to_string()])?;
    }
    {
        let mut ins = tx.prepare(
            "INSERT INTO vec_embedding (entity_id, vec, pred_layer, pred_category, confidence, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        for e in db.entries() {
            let cat = e.category.map(|c| c.as_str()).unwrap_or("");
            ins.execute(params![
                e.id.0 as i64,
                vec_to_blob(&e.vector),
                e.layer,
                cat,
                e.confidence as f64,
                e.status.as_str(),
            ])?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// Read the vector database from an open connection, or `None` if none was stored.
pub fn read_vecdb(conn: &Connection) -> Result<Option<VectorDb>, StoreError> {
    if !table_exists(conn, "vec_embedding")? {
        return Ok(None);
    }
    let model_id: Option<String> = conn
        .query_row(
            "SELECT value FROM vec_meta WHERE key = 'model_id'",
            [],
            |r| r.get(0),
        )
        .ok();
    let Some(model_id) = model_id else {
        return Ok(None);
    };
    let dim: usize = conn
        .query_row("SELECT value FROM vec_meta WHERE key = 'dim'", [], |r| {
            r.get::<_, String>(0)
        })
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let mut db = VectorDb::new(model_id, dim);
    let mut q = conn.prepare(
        "SELECT entity_id, vec, pred_layer, pred_category, confidence, status
         FROM vec_embedding ORDER BY entity_id",
    )?;
    let rows = q.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)? as u64,
            r.get::<_, Vec<u8>>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, f64>(4)? as f32,
            r.get::<_, String>(5)?,
        ))
    })?;
    for row in rows {
        let (id, blob, layer, cat, confidence, status) = row?;
        db.upsert(VecEntry {
            id: EntityId(id),
            vector: blob_to_vec(&blob),
            layer,
            category: SpaceCategory::parse(&cat),
            confidence,
            status: MappingStatus::parse(&status),
        });
    }
    Ok(Some(db))
}

/// Open `path` and write `db` into its vector tables (creating them if needed).
pub fn write_vecdb_path(path: impl AsRef<Path>, db: &VectorDb) -> Result<(), StoreError> {
    let conn = Connection::open(path.as_ref())?;
    crate::ensure_extended_tables(&conn)?;
    write_vecdb(&conn, db)
}

/// Open `path` and read its stored vector database, or `None` if absent.
pub fn read_vecdb_path(path: impl AsRef<Path>) -> Result<Option<VectorDb>, StoreError> {
    let conn = Connection::open(path.as_ref())?;
    read_vecdb(&conn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{load_path, save_path};
    use aadt_geometry::{Point2, Segment};
    use aadt_model::{Document, Geometry};

    fn sample_db(model_id: &str) -> VectorDb {
        let mut db = VectorDb::new(model_id, 3);
        db.upsert(VecEntry {
            id: EntityId(7),
            vector: vec![0.1, 0.2, 0.3],
            layer: "A-WALL".into(),
            category: Some(SpaceCategory::Walls),
            confidence: 0.82,
            status: MappingStatus::Auto,
        });
        db.upsert(VecEntry {
            id: EntityId(9),
            vector: vec![-0.5, 0.5, 0.0],
            layer: "A-COLS".into(),
            category: Some(SpaceCategory::Structural),
            confidence: 0.4,
            status: MappingStatus::Pending,
        });
        db.upsert(VecEntry {
            id: EntityId(11),
            vector: vec![0.0, 0.0, 1.0],
            layer: "A-FURN".into(),
            category: None,
            confidence: 0.0,
            status: MappingStatus::Overridden,
        });
        db
    }

    #[test]
    fn vecdb_round_trips_through_aadt() {
        let mut doc = Document::new();
        doc.add_geometry(
            "0",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(1.0, 0.0))),
        );
        let dir = std::env::temp_dir().join("aadt-vecdb-io");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("vec.aadt");
        let _ = std::fs::remove_file(&path);

        save_path(&doc, &path).unwrap();
        let db = sample_db("aadt-embed-mlp-1/f25-h32-c10");
        write_vecdb_path(&path, &db).unwrap();

        let loaded = read_vecdb_path(&path).unwrap().expect("vecdb present");
        assert_eq!(loaded.model_id(), db.model_id());
        assert_eq!(loaded.dim(), 3);
        assert_eq!(loaded.len(), 3);

        let e = loaded.get(EntityId(7)).unwrap();
        assert_eq!(e.layer, "A-WALL");
        assert_eq!(e.category, Some(SpaceCategory::Walls));
        assert!((e.confidence - 0.82).abs() < 1e-6);
        assert_eq!(e.status, MappingStatus::Auto);
        assert_eq!(e.vector, vec![0.1, 0.2, 0.3]);

        // Category None and Pending/Overridden statuses survive.
        assert_eq!(loaded.get(EntityId(11)).unwrap().category, None);
        assert_eq!(
            loaded.get(EntityId(9)).unwrap().status,
            MappingStatus::Pending
        );
        assert_eq!(
            loaded.get(EntityId(11)).unwrap().status,
            MappingStatus::Overridden
        );
    }

    #[test]
    fn vectors_survive_a_plain_resave() {
        // A normal save_path (no VectorDb) must carry the vec tables forward.
        let mut doc = Document::new();
        doc.add_geometry(
            "0",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(1.0, 0.0))),
        );
        let dir = std::env::temp_dir().join("aadt-vecdb-io-resave");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("vec.aadt");
        let _ = std::fs::remove_file(&path);

        save_path(&doc, &path).unwrap();
        write_vecdb_path(&path, &sample_db("m")).unwrap();

        // Mutate + save again without touching the vec db.
        doc.add_geometry(
            "0",
            Geometry::Line(Segment::new(Point2::new(0.0, 1.0), Point2::new(1.0, 1.0))),
        );
        save_path(&doc, &path).unwrap();

        let reloaded = read_vecdb_path(&path).unwrap().expect("vecdb preserved");
        assert_eq!(reloaded.len(), 3, "vectors carried across the resave");
        let _ = load_path(&path).unwrap();
    }

    #[test]
    fn absent_vecdb_reads_as_none() {
        let mut doc = Document::new();
        doc.add_geometry(
            "0",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(1.0, 0.0))),
        );
        let dir = std::env::temp_dir().join("aadt-vecdb-io-none");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("novec.aadt");
        let _ = std::fs::remove_file(&path);
        save_path(&doc, &path).unwrap();
        assert!(read_vecdb_path(&path).unwrap().is_none());
    }
}
