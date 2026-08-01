//! Persistence for the local text corpus (schema v9 tables `corpus_source` +
//! `corpus_chunk`). The corpus is replaced wholesale on write; chunks are
//! short paragraphs so the full replace is safe and keeps the schema simple.

use aadt_embed::TEXT_DIM;
use aadt_vecdb::corpus::{CorpusChunk, CorpusDb};
use rusqlite::{params, Connection};

use crate::StoreError;

/// Write the corpus into an open connection, replacing any existing data.
pub fn write_corpus(conn: &Connection, corpus: &CorpusDb) -> Result<(), StoreError> {
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM corpus_chunk", [])?;
    tx.execute("DELETE FROM corpus_source", [])?;

    // Build source → id map (insert each unique source once).
    let mut source_ids: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    {
        let mut ins_src =
            tx.prepare("INSERT INTO corpus_source (path, indexed_at) VALUES (?1, ?2)")?;
        for chunk in corpus.chunks() {
            if !source_ids.contains_key(&chunk.source) {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0) as i64;
                ins_src.execute(params![chunk.source, now])?;
                let id = conn.last_insert_rowid();
                source_ids.insert(chunk.source.clone(), id);
            }
        }
    }
    {
        let mut ins_chunk = tx.prepare(
            "INSERT INTO corpus_chunk (id, source_id, seq, text, vec)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        for chunk in corpus.chunks() {
            let src_id = source_ids[&chunk.source];
            // Serialize the embedding as little-endian f32 bytes.
            let blob = vec_to_blob(&chunk.vector);
            ins_chunk.execute(params![
                chunk.id as i64,
                src_id,
                chunk.seq as i64,
                chunk.text,
                blob,
            ])?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// Read the corpus from an open connection, or `None` if the tables are absent
/// or the corpus is empty.
pub fn read_corpus(conn: &Connection) -> Result<Option<CorpusDb>, StoreError> {
    let has: bool = conn
        .prepare("SELECT 1 FROM sqlite_master WHERE type='table' AND name='corpus_chunk'")?
        .exists([])?;
    if !has {
        return Ok(None);
    }

    let mut db = CorpusDb::new();
    {
        let mut q = conn.prepare(
            "SELECT c.id, s.path, c.seq, c.text, c.vec
             FROM corpus_chunk c
             JOIN corpus_source s ON s.id = c.source_id
             ORDER BY s.id, c.seq",
        )?;
        let rows = q.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)? as u64,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)? as u32,
                r.get::<_, String>(3)?,
                r.get::<_, Vec<u8>>(4)?,
            ))
        })?;
        for row in rows {
            let (id, source, seq, text, blob) = row?;
            let vector = blob_to_vec(&blob);
            db.restore(CorpusChunk {
                id,
                source,
                seq,
                text,
                vector,
            });
        }
    }
    if db.is_empty() {
        return Ok(None);
    }
    Ok(Some(db))
}

/// Open `path` and write the corpus (creating tables if needed).
pub fn write_corpus_path(
    path: impl AsRef<std::path::Path>,
    corpus: &CorpusDb,
) -> Result<(), StoreError> {
    let conn = Connection::open(path.as_ref())?;
    crate::ensure_extended_tables(&conn)?;
    write_corpus(&conn, corpus)
}

/// Open `path` and read the corpus, or `None` if absent or empty.
pub fn read_corpus_path(path: impl AsRef<std::path::Path>) -> Result<Option<CorpusDb>, StoreError> {
    let conn = Connection::open(path.as_ref())?;
    read_corpus(&conn)
}

// ── Blob helpers ──────────────────────────────────────────────────────────────

fn vec_to_blob(v: &[f32; TEXT_DIM]) -> Vec<u8> {
    let mut out = Vec::with_capacity(TEXT_DIM * 4);
    for &x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out
}

fn blob_to_vec(blob: &[u8]) -> [f32; TEXT_DIM] {
    let mut v = [0.0f32; TEXT_DIM];
    for (i, chunk) in blob.chunks_exact(4).enumerate().take(TEXT_DIM) {
        v[i] = f32::from_le_bytes(chunk.try_into().unwrap_or([0; 4]));
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::save_path;
    use aadt_geometry::{Point2, Segment};
    use aadt_model::{Document, Geometry};

    fn sample_corpus() -> CorpusDb {
        let mut db = CorpusDb::new();
        db.index_text(
            "spec.txt",
            "External walls shall be 230 mm brick.\n\nInternal walls shall be 115 mm brick.",
        );
        db
    }

    fn test_path(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("aadt-corpus-io");
        let _ = std::fs::create_dir_all(&dir);
        dir.join(format!("{name}.aadt"))
    }

    fn make_doc() -> Document {
        let mut doc = Document::new();
        doc.add_geometry(
            "0",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(1.0, 0.0))),
        );
        doc
    }

    #[test]
    fn corpus_round_trip() {
        let path = test_path("round-trip");
        let _ = std::fs::remove_file(&path);
        save_path(&make_doc(), &path).unwrap();

        let corpus = sample_corpus();
        write_corpus_path(&path, &corpus).unwrap();

        let loaded = read_corpus_path(&path).unwrap().expect("corpus stored");
        assert_eq!(loaded.len(), 2);
        assert!(loaded.has_source("spec.txt"));
        // Search should work after round-trip.
        let hits = loaded.search("brick wall", 2);
        assert!(!hits.is_empty());
    }

    #[test]
    fn empty_corpus_read_as_none() {
        let path = test_path("empty");
        let _ = std::fs::remove_file(&path);
        save_path(&make_doc(), &path).unwrap();
        assert!(read_corpus_path(&path).unwrap().is_none());
    }
}
