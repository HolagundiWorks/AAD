//! Pluggable takeoff publish sinks (CSV / JSON / SQL script for Postgres).

use std::path::{Path, PathBuf};

use crate::TakeoffTable;

/// Metadata attached when publishing a takeoff run.
#[derive(Debug, Clone, Default)]
pub struct TakeoffPushMeta {
    pub project_id: String,
    pub drawing_id: String,
    pub run_id: String,
    pub doc_revision: u64,
    pub title_block_revision: String,
    pub checkpoint_label: String,
}

/// Destination for published takeoff quantities.
pub trait TakeoffSink {
    fn push(&mut self, run: &TakeoffTable, meta: &TakeoffPushMeta) -> Result<(), String>;
}

/// Discard / no-op sink.
#[derive(Debug, Default, Clone)]
pub struct NullSink;

impl TakeoffSink for NullSink {
    fn push(&mut self, _run: &TakeoffTable, _meta: &TakeoffPushMeta) -> Result<(), String> {
        Ok(())
    }
}

/// Write `takeoff_<run>.csv` into a folder.
#[derive(Debug, Clone)]
pub struct CsvFolderSink {
    pub folder: PathBuf,
}

impl TakeoffSink for CsvFolderSink {
    fn push(&mut self, run: &TakeoffTable, meta: &TakeoffPushMeta) -> Result<(), String> {
        std::fs::create_dir_all(&self.folder).map_err(|e| e.to_string())?;
        let name = if meta.run_id.is_empty() {
            format!("takeoff_r{}.csv", meta.doc_revision)
        } else {
            format!("takeoff_{}.csv", meta.run_id)
        };
        let path = self.folder.join(name);
        std::fs::write(&path, run.to_csv()).map_err(|e| format!("csv {}: {e}", path.display()))?;
        Ok(())
    }
}

/// Write `takeoff_<run>.json` into a folder.
#[derive(Debug, Clone)]
pub struct JsonFolderSink {
    pub folder: PathBuf,
}

impl TakeoffSink for JsonFolderSink {
    fn push(&mut self, run: &TakeoffTable, meta: &TakeoffPushMeta) -> Result<(), String> {
        std::fs::create_dir_all(&self.folder).map_err(|e| e.to_string())?;
        let name = if meta.run_id.is_empty() {
            format!("takeoff_r{}.json", meta.doc_revision)
        } else {
            format!("takeoff_{}.json", meta.run_id)
        };
        let path = self.folder.join(name);
        std::fs::write(&path, run.to_json()).map_err(|e| format!("json {}: {e}", path.display()))?;
        Ok(())
    }
}

/// Emit a PostgreSQL-compatible upsert script (apply with `psql -f`).
///
/// Live network Postgres is opt-in later; this keeps drafting offline while still
/// producing a DB-ready artifact for estimators.
#[derive(Debug, Clone)]
pub struct PostgresSqlSink {
    pub path: PathBuf,
}

impl TakeoffSink for PostgresSqlSink {
    fn push(&mut self, run: &TakeoffTable, meta: &TakeoffPushMeta) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut sql = String::new();
        sql.push_str("-- AADT takeoff publish (PostgreSQL)\n");
        sql.push_str("CREATE TABLE IF NOT EXISTS aadt_takeoff_run (\n");
        sql.push_str("  project_id TEXT NOT NULL,\n");
        sql.push_str("  drawing_id TEXT NOT NULL,\n");
        sql.push_str("  run_id TEXT NOT NULL,\n");
        sql.push_str("  doc_revision BIGINT NOT NULL,\n");
        sql.push_str("  title_rev TEXT,\n");
        sql.push_str("  checkpoint_label TEXT,\n");
        sql.push_str("  wall_length DOUBLE PRECISION,\n");
        sql.push_str("  wall_net_area DOUBLE PRECISION,\n");
        sql.push_str("  room_area DOUBLE PRECISION,\n");
        sql.push_str("  PRIMARY KEY (project_id, drawing_id, run_id)\n");
        sql.push_str(");\n");
        let esc = |s: &str| s.replace('\'', "''");
        sql.push_str(&format!(
            "INSERT INTO aadt_takeoff_run \
             (project_id, drawing_id, run_id, doc_revision, title_rev, checkpoint_label, \
              wall_length, wall_net_area, room_area) VALUES \
             ('{}','{}','{}',{},'{}','{}',{:.6},{:.6},{:.6})\n\
             ON CONFLICT (project_id, drawing_id, run_id) DO UPDATE SET \
             doc_revision = EXCLUDED.doc_revision,\
             wall_length = EXCLUDED.wall_length,\
             wall_net_area = EXCLUDED.wall_net_area,\
             room_area = EXCLUDED.room_area;\n",
            esc(&meta.project_id),
            esc(&meta.drawing_id),
            esc(&meta.run_id),
            meta.doc_revision,
            esc(&meta.title_block_revision),
            esc(&meta.checkpoint_label),
            run.total_wall_length(),
            run.total_wall_net_area(),
            run.total_room_area(),
        ));
        std::fs::write(&self.path, sql).map_err(|e| e.to_string())?;
        Ok(())
    }
}

/// Build a sink from prefs token: `none` | `csv:<dir>` | `json:<dir>` | `postgres:<sqlpath>`.
pub fn sink_from_spec(spec: &str) -> Result<Box<dyn TakeoffSink>, String> {
    let spec = spec.trim();
    if spec.is_empty() || spec.eq_ignore_ascii_case("none") {
        return Ok(Box::new(NullSink));
    }
    if let Some(dir) = spec.strip_prefix("csv:") {
        return Ok(Box::new(CsvFolderSink {
            folder: PathBuf::from(dir),
        }));
    }
    if let Some(dir) = spec.strip_prefix("json:") {
        return Ok(Box::new(JsonFolderSink {
            folder: PathBuf::from(dir),
        }));
    }
    if let Some(path) = spec
        .strip_prefix("postgres:")
        .or_else(|| spec.strip_prefix("pgsql:"))
    {
        return Ok(Box::new(PostgresSqlSink {
            path: PathBuf::from(path),
        }));
    }
    // Bare path → CSV folder
    if Path::new(spec).is_dir() || !spec.contains(':') {
        return Ok(Box::new(CsvFolderSink {
            folder: PathBuf::from(spec),
        }));
    }
    Err(format!(
        "unknown takeoff sink {spec:?} (use none|csv:dir|json:dir|postgres:file.sql)"
    ))
}
