//! Takeoff run persistence inside `.aadt` (schema v5).

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};

use crate::{history::ensure_v5_or_migrate, StoreError};

/// One persisted takeoff run (rows + totals JSON).
#[derive(Debug, Clone, PartialEq)]
pub struct TakeoffRunData {
    pub doc_revision: u64,
    pub checkpoint_id: Option<i64>,
    pub schema_ver: String,
    pub totals_json: String,
    pub published: bool,
    pub walls: Vec<TakeoffWallRow>,
    pub rooms: Vec<TakeoffRoomRow>,
    pub openings: Vec<TakeoffOpeningRow>,
    pub structural: Vec<TakeoffStructuralRow>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TakeoffWallRow {
    pub wall_id: i64,
    pub layer: String,
    pub length: f64,
    pub thickness: f64,
    pub height: f64,
    pub gross_area: f64,
    pub volume: f64,
    pub cost_code: String,
    pub wall_type: String,
    pub class_status: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TakeoffRoomRow {
    pub room_id: i64,
    pub layer: String,
    pub area: f64,
    pub perimeter: f64,
    pub cost_code: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TakeoffOpeningRow {
    pub number: i64,
    pub kind: String,
    pub width: f64,
    pub height: f64,
    pub area: f64,
    pub cost_code: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TakeoffStructuralRow {
    pub number: i64,
    pub kind: String,
    pub cost_code: String,
}

fn now_secs() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".into())
}

/// Append a takeoff run to `path`. Returns the new run id.
pub fn append_takeoff_run(
    path: impl AsRef<Path>,
    data: &TakeoffRunData,
) -> Result<i64, StoreError> {
    let conn = Connection::open(path.as_ref())?;
    ensure_v5_or_migrate(&conn)?;
    append_takeoff_run_conn(&conn, data)
}

pub fn append_takeoff_run_conn(
    conn: &Connection,
    data: &TakeoffRunData,
) -> Result<i64, StoreError> {
    conn.execute(
        "INSERT INTO takeoff_run (at, doc_revision, checkpoint_id, schema_ver, totals_json, published)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            now_secs(),
            data.doc_revision as i64,
            data.checkpoint_id,
            data.schema_ver,
            data.totals_json,
            if data.published { 1i64 } else { 0i64 }
        ],
    )?;
    let run_id = conn.last_insert_rowid();
    {
        let mut ins = conn.prepare(
            "INSERT INTO takeoff_wall
             (run_id, wall_id, layer, length, thickness, height, gross_area, volume, cost_code, wall_type, class_status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        )?;
        for w in &data.walls {
            ins.execute(params![
                run_id,
                w.wall_id,
                w.layer,
                w.length,
                w.thickness,
                w.height,
                w.gross_area,
                w.volume,
                w.cost_code,
                w.wall_type,
                w.class_status
            ])?;
        }
    }
    {
        let mut ins = conn.prepare(
            "INSERT INTO takeoff_room (run_id, room_id, layer, area, perimeter, cost_code)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        for r in &data.rooms {
            ins.execute(params![
                run_id,
                r.room_id,
                r.layer,
                r.area,
                r.perimeter,
                r.cost_code
            ])?;
        }
    }
    {
        let mut ins = conn.prepare(
            "INSERT INTO takeoff_opening (run_id, number, kind, width, height, area, cost_code)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )?;
        for o in &data.openings {
            ins.execute(params![
                run_id,
                o.number,
                o.kind,
                o.width,
                o.height,
                o.area,
                o.cost_code
            ])?;
        }
    }
    {
        let mut ins = conn.prepare(
            "INSERT INTO takeoff_structural (run_id, number, kind, cost_code)
             VALUES (?1, ?2, ?3, ?4)",
        )?;
        for s in &data.structural {
            ins.execute(params![run_id, s.number, s.kind, s.cost_code])?;
        }
    }
    Ok(run_id)
}

/// Mark a run as published.
pub fn mark_takeoff_published(path: impl AsRef<Path>, run_id: i64) -> Result<(), StoreError> {
    let conn = Connection::open(path.as_ref())?;
    ensure_v5_or_migrate(&conn)?;
    conn.execute(
        "UPDATE takeoff_run SET published = 1 WHERE id = ?1",
        params![run_id],
    )?;
    Ok(())
}

/// Store a delta summary between two runs.
pub fn append_takeoff_delta(
    path: impl AsRef<Path>,
    from_run: i64,
    to_run: i64,
    summary: &str,
) -> Result<(), StoreError> {
    let conn = Connection::open(path.as_ref())?;
    ensure_v5_or_migrate(&conn)?;
    conn.execute(
        "INSERT INTO takeoff_delta (from_run, to_run, summary) VALUES (?1, ?2, ?3)",
        params![from_run, to_run, summary],
    )?;
    Ok(())
}

/// Latest takeoff run id, if any.
pub fn latest_takeoff_run_id(path: impl AsRef<Path>) -> Result<Option<i64>, StoreError> {
    let conn = Connection::open(path.as_ref())?;
    ensure_v5_or_migrate(&conn)?;
    let id: Option<i64> = conn
        .query_row(
            "SELECT id FROM takeoff_run ORDER BY id DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()?;
    Ok(id)
}

/// Latest published takeoff run id.
pub fn latest_published_run_id(path: impl AsRef<Path>) -> Result<Option<i64>, StoreError> {
    let conn = Connection::open(path.as_ref())?;
    ensure_v5_or_migrate(&conn)?;
    let id: Option<i64> = conn
        .query_row(
            "SELECT id FROM takeoff_run WHERE published = 1 ORDER BY id DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()?;
    Ok(id)
}

/// Load a takeoff run by id.
pub fn load_takeoff_run(
    path: impl AsRef<Path>,
    run_id: i64,
) -> Result<Option<TakeoffRunData>, StoreError> {
    let conn = Connection::open(path.as_ref())?;
    ensure_v5_or_migrate(&conn)?;
    let meta: Option<(i64, Option<i64>, String, String, i64)> = conn
        .query_row(
            "SELECT doc_revision, checkpoint_id, schema_ver, totals_json, published FROM takeoff_run WHERE id = ?1",
            params![run_id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                ))
            },
        )
        .optional()?;
    let Some((doc_rev, chk, schema, totals, published)) = meta else {
        return Ok(None);
    };
    let mut data = TakeoffRunData {
        doc_revision: doc_rev as u64,
        checkpoint_id: chk,
        schema_ver: schema,
        totals_json: totals,
        published: published != 0,
        walls: Vec::new(),
        rooms: Vec::new(),
        openings: Vec::new(),
        structural: Vec::new(),
    };
    {
        let mut q = conn.prepare(
            "SELECT wall_id, layer, length, thickness, height, gross_area, volume, cost_code, wall_type, class_status
             FROM takeoff_wall WHERE run_id = ?1",
        )?;
        data.walls = q
            .query_map(params![run_id], |r| {
                Ok(TakeoffWallRow {
                    wall_id: r.get(0)?,
                    layer: r.get(1)?,
                    length: r.get(2)?,
                    thickness: r.get(3)?,
                    height: r.get(4)?,
                    gross_area: r.get(5)?,
                    volume: r.get(6)?,
                    cost_code: r.get(7)?,
                    wall_type: r.get(8)?,
                    class_status: r.get(9)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
    }
    {
        let mut q = conn.prepare(
            "SELECT room_id, layer, area, perimeter, cost_code FROM takeoff_room WHERE run_id = ?1",
        )?;
        data.rooms = q
            .query_map(params![run_id], |r| {
                Ok(TakeoffRoomRow {
                    room_id: r.get(0)?,
                    layer: r.get(1)?,
                    area: r.get(2)?,
                    perimeter: r.get(3)?,
                    cost_code: r.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
    }
    {
        let mut q = conn.prepare(
            "SELECT number, kind, width, height, area, cost_code FROM takeoff_opening WHERE run_id = ?1",
        )?;
        data.openings = q
            .query_map(params![run_id], |r| {
                Ok(TakeoffOpeningRow {
                    number: r.get(0)?,
                    kind: r.get(1)?,
                    width: r.get(2)?,
                    height: r.get(3)?,
                    area: r.get(4)?,
                    cost_code: r.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
    }
    {
        let mut q = conn
            .prepare("SELECT number, kind, cost_code FROM takeoff_structural WHERE run_id = ?1")?;
        data.structural = q
            .query_map(params![run_id], |r| {
                Ok(TakeoffStructuralRow {
                    number: r.get(0)?,
                    kind: r.get(1)?,
                    cost_code: r.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
    }
    Ok(Some(data))
}

/// Latest delta summary involving `to_run`, if any.
pub fn latest_delta_summary(
    path: impl AsRef<Path>,
    to_run: i64,
) -> Result<Option<String>, StoreError> {
    let conn = Connection::open(path.as_ref())?;
    ensure_v5_or_migrate(&conn)?;
    let s: Option<String> = conn
        .query_row(
            "SELECT summary FROM takeoff_delta WHERE to_run = ?1 ORDER BY rowid DESC LIMIT 1",
            params![to_run],
            |r| r.get(0),
        )
        .optional()?;
    Ok(s)
}

// ── Bridge from the Phase-4 quantity engine ───────────────────────────────────

/// Convert a [`aadt_takeoff::TakeoffRun`] into the persisted row shape.
///
/// The row tables predate the Object DB and key rows by a plain integer, so the
/// object id doubles as the row id — objects carry stable ids, which is exactly
/// what the tables wanted and geometry could not provide.
pub fn run_to_row_data(
    run: &aadt_takeoff::TakeoffRun,
    checkpoint_id: Option<i64>,
) -> TakeoffRunData {
    let q = &run.quantities;
    TakeoffRunData {
        doc_revision: run.doc_revision,
        checkpoint_id,
        schema_ver: "takeoff-2".into(),
        totals_json: run.totals_json(),
        published: false,
        walls: q
            .walls
            .iter()
            .map(|w| TakeoffWallRow {
                wall_id: w.object.0 as i64,
                layer: w.layer.clone(),
                length: w.length,
                thickness: w.thickness,
                height: w.height,
                gross_area: w.net_area,
                volume: w.volume,
                cost_code: w.cost_code.code.to_string(),
                wall_type: w.cost_code.title.to_string(),
                class_status: "promoted".into(),
            })
            .collect(),
        rooms: q
            .rooms
            .iter()
            .map(|r| TakeoffRoomRow {
                room_id: r.object.0 as i64,
                layer: r.layer.clone(),
                area: r.floor_area,
                perimeter: r.perimeter,
                cost_code: r.cost_code.code.to_string(),
            })
            .collect(),
        openings: q
            .openings
            .iter()
            .map(|o| TakeoffOpeningRow {
                number: o.object.0 as i64,
                kind: o.kind.as_str().to_string(),
                width: o.width,
                height: o.height,
                area: o.area,
                cost_code: o.cost_code.code.to_string(),
            })
            .collect(),
        structural: q
            .structural
            .iter()
            .map(|s| TakeoffStructuralRow {
                number: s.object.0 as i64,
                kind: s.kind.as_str().to_string(),
                cost_code: s.cost_code.code.to_string(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod phase4_tests {
    use super::*;
    use aadt_geometry::{Point2, Segment};
    use aadt_model::{Document, Geometry};

    fn plan() -> Document {
        let mut doc = Document::new();
        doc.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(6.0, 0.0))),
        );
        doc.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.23), Point2::new(6.0, 0.23))),
        );
        doc
    }

    #[test]
    fn a_takeoff_run_round_trips_through_aadt() {
        let doc = plan();
        let dir = std::env::temp_dir().join("aadt-takeoff-io");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("takeoff.aadt");
        let _ = std::fs::remove_file(&path);
        crate::save_path(&doc, &path).unwrap();

        let run = aadt_takeoff::run(&doc, &aadt_takeoff::TakeoffConfig::default());
        let data = run_to_row_data(&run, None);
        assert_eq!(data.walls.len(), 1, "the plan has one wall");

        let id = append_takeoff_run(&path, &data).unwrap();
        let loaded = load_takeoff_run(&path, id).unwrap().expect("run stored");
        assert_eq!(loaded.walls.len(), 1);
        assert_eq!(loaded.walls[0].cost_code, "04 20 00");
        assert_eq!(loaded.doc_revision, run.doc_revision);
        assert_eq!(loaded.totals_json, run.totals_json());
    }

    #[test]
    fn wall_rows_key_off_stable_object_ids() {
        let run = aadt_takeoff::run(&plan(), &aadt_takeoff::TakeoffConfig::default());
        let data = run_to_row_data(&run, None);
        // Object ids start at 1 and are stable across runs — the row id is the
        // object id, so a row can be traced back to the object it measured.
        assert_eq!(
            data.walls[0].wall_id,
            run.quantities.walls[0].object.0 as i64
        );
        assert!(data.walls[0].wall_id > 0);
    }
}
