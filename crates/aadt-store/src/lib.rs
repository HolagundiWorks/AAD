//! # aadt-store — Native `.aadt` SQLite persistence (ADR-0004)
//!
//! The working-file format for AADT drawings. SQLite (bundled) holds layers,
//! entities (stable ids), blocks, hatches, dimensions, **drawing history**
//! (time machine), and **takeoff runs**. Session undo is still RAM-only.
//!
//! Geometry is stored as a compact text payload (no serde). Schema version is in
//! the `meta` table; v4→v5 migrates in place on open.

mod codec;
mod history;
mod takeoff_db;

use std::collections::HashMap;
use std::path::Path;

use aadt_geometry::Point2;
use aadt_model::{
    Block, BlockParam, BlockParamKind, Dimension, DimensionKind, Document, Entity, EntityId,
    Geometry, Hatch, Layer, LineType, VisibilityState,
};
use rusqlite::{params, Connection, OptionalExtension};

use crate::codec::{decode_geometry, encode_geometry};

pub use history::{
    list_checkpoints, purge_auto_checkpoints, restore_checkpoint, save_named_checkpoint,
    CheckpointMeta, MAX_AUTO_CHECKPOINTS,
};
pub use takeoff_db::{
    append_takeoff_delta, append_takeoff_run, latest_delta_summary, latest_published_run_id,
    latest_takeoff_run_id, load_takeoff_run, mark_takeoff_published, TakeoffOpeningRow,
    TakeoffRoomRow, TakeoffRunData, TakeoffStructuralRow, TakeoffWallRow,
};

/// Current on-disk schema version written into every new file.
pub const SCHEMA_VERSION: i64 = 5;

/// File-format / I/O errors for `.aadt` persistence.
pub use codec::StoreError;

/// Save `doc` to `path` as `.aadt` (preserves history/takeoff; appends auto checkpoint).
pub fn save_path(doc: &Document, path: impl AsRef<Path>) -> Result<(), StoreError> {
    save_path_with_checkpoint(doc, path, None).map(|_| ())
}

/// Save with an optional named checkpoint `(label, note, author)` instead of auto.
pub fn save_path_with_checkpoint(
    doc: &Document,
    path: impl AsRef<Path>,
    named: Option<(&str, &str, &str)>,
) -> Result<i64, StoreError> {
    if let Some(parent) = path.as_ref().parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let path = path.as_ref();
    // Copy-then-migrate so a failed upgrade never leaves the live `.aadt` half-written.
    let mut prior: Option<Connection> = None;
    let mut migrate_scratch: Option<std::path::PathBuf> = None;
    if path.exists() {
        let scratch = path.with_extension("aadt.migrate.tmp");
        if scratch.exists() {
            std::fs::remove_file(&scratch)?;
        }
        std::fs::copy(path, &scratch)?;
        let old = Connection::open(&scratch)?;
        let ver: Option<i64> = old
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |r| {
                    let s: String = r.get(0)?;
                    Ok(s.parse::<i64>().unwrap_or(0))
                },
            )
            .optional()?;
        if let Some(v) = ver {
            if v < SCHEMA_VERSION {
                migrate_to_v5(&old, v)?;
            }
            if v <= SCHEMA_VERSION {
                prior = Some(old);
                migrate_scratch = Some(scratch);
            } else {
                drop(old);
                let _ = std::fs::remove_file(&scratch);
            }
        } else {
            drop(old);
            let _ = std::fs::remove_file(&scratch);
        }
    }

    let tmp = path.with_extension("aadt.tmp");
    if tmp.exists() {
        std::fs::remove_file(&tmp)?;
    }
    let conn = Connection::open(&tmp)?;
    init_schema_tables(&conn)?;
    conn.execute(
        "INSERT INTO meta (key, value) VALUES ('schema_version', ?1)",
        params![SCHEMA_VERSION.to_string()],
    )?;
    write_document(&conn, doc)?;
    if let Some(old) = prior.as_ref() {
        history::copy_extended_tables(old, &conn)?;
    }
    let chk_id = if let Some((label, note, author)) = named {
        history::insert_checkpoint_pub(&conn, doc, label, "named", note, author)?
    } else {
        history::append_auto_checkpoint(&conn, doc)?
    };
    drop(conn);
    drop(prior);
    if let Some(scratch) = migrate_scratch {
        let _ = std::fs::remove_file(scratch);
    }
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(chk_id)
}

/// Load a `.aadt` file into a fresh [`Document`] (empty undo history).
pub fn load_path(path: impl AsRef<Path>) -> Result<Document, StoreError> {
    let path = path.as_ref();
    let conn = Connection::open(path)?;
    let ver: i64 = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |r| {
                let s: String = r.get(0)?;
                s.parse::<i64>().map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        Box::new(e),
                    )
                })
            },
        )
        .optional()?
        .ok_or_else(|| StoreError::Format("missing schema_version".into()))?;
    if ver > SCHEMA_VERSION {
        return Err(StoreError::Format(format!(
            "unsupported schema_version {ver} (want {SCHEMA_VERSION})"
        )));
    }
    if ver < SCHEMA_VERSION {
        // Migrate a copy, then atomically replace — never leave the live file mid-upgrade.
        drop(conn);
        let scratch = path.with_extension("aadt.migrate.tmp");
        if scratch.exists() {
            std::fs::remove_file(&scratch)?;
        }
        std::fs::copy(path, &scratch)?;
        {
            let mig = Connection::open(&scratch)?;
            migrate_to_v5(&mig, ver)?;
        }
        let bak = path.with_extension("aadt.bak");
        if bak.exists() {
            std::fs::remove_file(&bak)?;
        }
        std::fs::rename(path, &bak)?;
        match std::fs::rename(&scratch, path) {
            Ok(()) => {
                let _ = std::fs::remove_file(&bak);
            }
            Err(e) => {
                let _ = std::fs::rename(&bak, path);
                let _ = std::fs::remove_file(&scratch);
                return Err(e.into());
            }
        }
        let conn = Connection::open(path)?;
        return read_document(&conn);
    }
    read_document(&conn)
}

/// Create all tables (model + history + takeoff). Does not set schema_version.
pub(crate) fn init_schema_tables(conn: &Connection) -> Result<(), StoreError> {
    conn.execute_batch(
        "
        CREATE TABLE meta (
            key TEXT PRIMARY KEY NOT NULL,
            value TEXT NOT NULL
        );
        CREATE TABLE layers (
            name TEXT PRIMARY KEY NOT NULL,
            color INTEGER NOT NULL,
            linetype TEXT NOT NULL,
            lineweight_mm REAL NOT NULL,
            visible INTEGER NOT NULL,
            frozen INTEGER NOT NULL,
            locked INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE blocks (
            name TEXT PRIMARY KEY NOT NULL,
            base_x REAL NOT NULL,
            base_y REAL NOT NULL
        );
        CREATE TABLE block_entities (
            block TEXT NOT NULL REFERENCES blocks(name) ON DELETE CASCADE,
            seq INTEGER NOT NULL,
            layer TEXT NOT NULL,
            geom TEXT NOT NULL,
            PRIMARY KEY (block, seq)
        );
        CREATE TABLE block_params (
            block TEXT NOT NULL REFERENCES blocks(name) ON DELETE CASCADE,
            name TEXT NOT NULL,
            default_value TEXT NOT NULL,
            kind TEXT NOT NULL,
            PRIMARY KEY (block, name)
        );
        CREATE TABLE block_vis (
            block TEXT NOT NULL REFERENCES blocks(name) ON DELETE CASCADE,
            state TEXT NOT NULL,
            hidden TEXT NOT NULL,
            PRIMARY KEY (block, state)
        );
        CREATE TABLE entities (
            id INTEGER PRIMARY KEY NOT NULL,
            layer TEXT NOT NULL,
            geom TEXT NOT NULL
        );
        CREATE TABLE entity_attrs (
            entity_id INTEGER NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
            key TEXT NOT NULL,
            value TEXT NOT NULL,
            PRIMARY KEY (entity_id, key)
        );
        CREATE TABLE hatches (
            seq INTEGER PRIMARY KEY NOT NULL,
            layer TEXT NOT NULL,
            pattern TEXT NOT NULL,
            solid INTEGER NOT NULL,
            geom TEXT NOT NULL
        );
        CREATE TABLE dimensions (
            seq INTEGER PRIMARY KEY NOT NULL,
            layer TEXT NOT NULL,
            kind TEXT NOT NULL,
            p1x REAL NOT NULL, p1y REAL NOT NULL,
            p2x REAL NOT NULL, p2y REAL NOT NULL,
            vx REAL NOT NULL, vy REAL NOT NULL,
            tx REAL NOT NULL, ty REAL NOT NULL,
            text TEXT NOT NULL,
            measurement REAL NOT NULL
        );
        ",
    )?;
    ensure_extended_tables(conn)?;
    Ok(())
}

pub(crate) fn ensure_extended_tables(conn: &Connection) -> Result<(), StoreError> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS history_checkpoint (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            created_at TEXT NOT NULL,
            label TEXT NOT NULL,
            kind TEXT NOT NULL,
            doc_revision INTEGER NOT NULL,
            note TEXT NOT NULL DEFAULT '',
            author TEXT NOT NULL DEFAULT ''
        );
        CREATE TABLE IF NOT EXISTS history_blob (
            checkpoint_id INTEGER PRIMARY KEY NOT NULL
                REFERENCES history_checkpoint(id) ON DELETE CASCADE,
            blob BLOB NOT NULL
        );
        CREATE TABLE IF NOT EXISTS history_change (
            checkpoint_id INTEGER PRIMARY KEY NOT NULL
                REFERENCES history_checkpoint(id) ON DELETE CASCADE,
            op_summary TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS takeoff_run (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            at TEXT NOT NULL,
            doc_revision INTEGER NOT NULL,
            checkpoint_id INTEGER,
            schema_ver TEXT NOT NULL,
            totals_json TEXT NOT NULL,
            published INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS takeoff_wall (
            run_id INTEGER NOT NULL REFERENCES takeoff_run(id) ON DELETE CASCADE,
            wall_id INTEGER NOT NULL,
            layer TEXT NOT NULL,
            length REAL NOT NULL,
            thickness REAL NOT NULL,
            height REAL NOT NULL,
            gross_area REAL NOT NULL,
            volume REAL NOT NULL,
            cost_code TEXT NOT NULL,
            wall_type TEXT NOT NULL,
            class_status TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS takeoff_room (
            run_id INTEGER NOT NULL REFERENCES takeoff_run(id) ON DELETE CASCADE,
            room_id INTEGER NOT NULL,
            layer TEXT NOT NULL,
            area REAL NOT NULL,
            perimeter REAL NOT NULL,
            cost_code TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS takeoff_opening (
            run_id INTEGER NOT NULL REFERENCES takeoff_run(id) ON DELETE CASCADE,
            number INTEGER NOT NULL,
            kind TEXT NOT NULL,
            width REAL NOT NULL,
            height REAL NOT NULL,
            area REAL NOT NULL,
            cost_code TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS takeoff_structural (
            run_id INTEGER NOT NULL REFERENCES takeoff_run(id) ON DELETE CASCADE,
            number INTEGER NOT NULL,
            kind TEXT NOT NULL,
            cost_code TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS takeoff_delta (
            from_run INTEGER NOT NULL,
            to_run INTEGER NOT NULL,
            summary TEXT NOT NULL
        );
        ",
    )?;
    Ok(())
}

pub(crate) fn migrate_to_v5(conn: &Connection, from: i64) -> Result<(), StoreError> {
    if from > SCHEMA_VERSION {
        return Err(StoreError::Format(format!(
            "cannot migrate from future schema {from}"
        )));
    }
    ensure_extended_tables(conn)?;
    conn.execute(
        "INSERT INTO meta (key, value) VALUES ('schema_version', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![SCHEMA_VERSION.to_string()],
    )?;
    Ok(())
}


pub(crate) fn write_document(conn: &Connection, doc: &Document) -> Result<(), StoreError> {
    let tx = conn.unchecked_transaction()?;

    {
        let mut stmt = tx.prepare(
            "INSERT INTO layers (name, color, linetype, lineweight_mm, visible, frozen, locked) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )?;
        for layer in doc.layers() {
            stmt.execute(params![
                layer.name,
                layer.color as i64,
                layer.linetype.dxf_name(),
                layer.lineweight_mm as f64,
                i64::from(layer.visible),
                i64::from(layer.frozen),
                i64::from(layer.locked),
            ])?;
        }
    }

    {
        let mut bstmt =
            tx.prepare("INSERT INTO blocks (name, base_x, base_y) VALUES (?1, ?2, ?3)")?;
        let mut estmt = tx.prepare(
            "INSERT INTO block_entities (block, seq, layer, geom) VALUES (?1, ?2, ?3, ?4)",
        )?;
        let mut pstmt = tx.prepare(
            "INSERT INTO block_params (block, name, default_value, kind) VALUES (?1, ?2, ?3, ?4)",
        )?;
        let mut vstmt =
            tx.prepare("INSERT INTO block_vis (block, state, hidden) VALUES (?1, ?2, ?3)")?;
        for block in doc.blocks() {
            bstmt.execute(params![block.name, block.base.x, block.base.y])?;
            for (i, be) in block.entities.iter().enumerate() {
                estmt.execute(params![
                    block.name,
                    i as i64,
                    be.layer,
                    encode_geometry(&be.geometry)?
                ])?;
            }
            for p in &block.params {
                pstmt.execute(params![
                    block.name,
                    p.name,
                    p.default,
                    p.kind.as_str()
                ])?;
            }
            for v in &block.visibilities {
                let hidden = v
                    .hidden
                    .iter()
                    .map(|i| i.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                vstmt.execute(params![block.name, v.name, hidden])?;
            }
        }
    }

    {
        let mut estmt = tx.prepare("INSERT INTO entities (id, layer, geom) VALUES (?1, ?2, ?3)")?;
        let mut astmt =
            tx.prepare("INSERT INTO entity_attrs (entity_id, key, value) VALUES (?1, ?2, ?3)")?;
        for e in doc.entities() {
            estmt.execute(params![
                e.id.0 as i64,
                e.layer,
                encode_geometry(&e.geometry)?
            ])?;
            let mut keys: Vec<_> = e.attributes.keys().collect();
            keys.sort();
            for k in keys {
                if let Some(v) = e.attributes.get(k) {
                    astmt.execute(params![e.id.0 as i64, k, v])?;
                }
            }
        }
    }

    {
        let mut stmt = tx.prepare(
            "INSERT INTO hatches (seq, layer, pattern, solid, geom) VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        for (i, h) in doc.hatches().iter().enumerate() {
            let geom = Geometry::Polyline(h.boundary.clone());
            stmt.execute(params![
                i as i64,
                h.layer,
                h.pattern,
                i64::from(h.solid),
                encode_geometry(&geom)?
            ])?;
        }
    }

    {
        let mut stmt = tx.prepare(
            "INSERT INTO dimensions (seq, layer, kind, p1x, p1y, p2x, p2y, vx, vy, tx, ty, text, measurement)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
        )?;
        for (i, d) in doc.dimensions().iter().enumerate() {
            stmt.execute(params![
                i as i64,
                d.layer,
                dim_kind_name(d.kind),
                d.p1.x,
                d.p1.y,
                d.p2.x,
                d.p2.y,
                d.vertex.x,
                d.vertex.y,
                d.text_anchor.x,
                d.text_anchor.y,
                d.text,
                d.measurement,
            ])?;
        }
    }

    // Keep properties in the same SQLite transaction as geometry (atomic save).
    write_properties(&*tx, doc.properties())?;

    tx.commit()?;

    Ok(())
}

fn write_properties(
    conn: &rusqlite::Connection,
    p: &aadt_model::DrawingProperties,
) -> Result<(), StoreError> {
    let pairs = [
        ("prop.title", p.title.as_str()),
        ("prop.subject", p.subject.as_str()),
        ("prop.author", p.author.as_str()),
        ("prop.keywords", p.keywords.as_str()),
        ("prop.comments", p.comments.as_str()),
        ("prop.hyperlink_base", p.hyperlink_base.as_str()),
        ("prop.project", p.project.as_str()),
        ("prop.client", p.client.as_str()),
        ("prop.address", p.address.as_str()),
        ("prop.architect", p.architect.as_str()),
        ("prop.date", p.date.as_str()),
        ("prop.area", p.area.as_str()),
        ("prop.scale", p.scale.as_str()),
        ("prop.sheet_number", p.sheet_number.as_str()),
        ("prop.firm_name", p.firm_name.as_str()),
        ("prop.firm_tagline", p.firm_tagline.as_str()),
        ("prop.phone", p.phone.as_str()),
        ("prop.email", p.email.as_str()),
        ("prop.office_address", p.office_address.as_str()),
        ("prop.logo_path", p.logo_path.as_str()),
        ("prop.project_number", p.project_number.as_str()),
        ("prop.discipline", p.discipline.as_str()),
        ("prop.drawing_number", p.drawing_number.as_str()),
        ("prop.total_sheets", p.total_sheets.as_str()),
        ("prop.drawn_by", p.drawn_by.as_str()),
        ("prop.checked_by", p.checked_by.as_str()),
        ("prop.approved_by", p.approved_by.as_str()),
        ("prop.revision", p.revision.as_str()),
        ("prop.drawing_status", p.drawing_status.as_str()),
        ("prop.website", p.website.as_str()),
        ("prop.revision_history", p.revision_history.as_str()),
        ("prop.units", p.units.as_str()),
        ("prop.dimstyle", p.dimstyle.as_str()),
        ("prop.dim_text_height", p.dim_text_height.as_str()),
        ("prop.dim_scale", p.dim_scale.as_str()),
        ("prop.dim_secondary", p.dim_secondary.as_str()),
        ("prop.autonum_prefix", p.autonum_prefix.as_str()),
        ("prop.autonum_next", p.autonum_next.as_str()),
        ("prop.standard_notes", p.standard_notes.as_str()),
        ("prop.plot_include_notes", p.plot_include_notes.as_str()),
        ("prop.nameplate_preset", p.nameplate_preset.as_str()),
        ("prop.nameplate_block", p.nameplate_block.as_str()),
        ("prop.takeoff_live", p.takeoff_live.as_str()),
    ];
    let mut stmt = conn.prepare(
        "INSERT INTO meta (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )?;
    for (k, v) in pairs {
        if !v.is_empty() {
            stmt.execute(params![k, v])?;
        }
    }
    Ok(())
}

fn read_properties(conn: &Connection) -> Result<aadt_model::DrawingProperties, StoreError> {
    let mut p = aadt_model::DrawingProperties::default();
    let mut stmt = conn.prepare("SELECT key, value FROM meta WHERE key LIKE 'prop.%'")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    for row in rows {
        let (k, v) = row?;
        match k.as_str() {
            "prop.title" => p.title = v,
            "prop.subject" => p.subject = v,
            "prop.author" => p.author = v,
            "prop.keywords" => p.keywords = v,
            "prop.comments" => p.comments = v,
            "prop.hyperlink_base" => p.hyperlink_base = v,
            "prop.project" => p.project = v,
            "prop.client" => p.client = v,
            "prop.address" => p.address = v,
            "prop.architect" => p.architect = v,
            "prop.date" => p.date = v,
            "prop.area" => p.area = v,
            "prop.scale" => p.scale = v,
            "prop.sheet_number" => p.sheet_number = v,
            "prop.firm_name" => p.firm_name = v,
            "prop.firm_tagline" => p.firm_tagline = v,
            "prop.phone" => p.phone = v,
            "prop.email" => p.email = v,
            "prop.office_address" => p.office_address = v,
            "prop.logo_path" => p.logo_path = v,
            "prop.project_number" => p.project_number = v,
            "prop.discipline" => p.discipline = v,
            "prop.drawing_number" => p.drawing_number = v,
            "prop.total_sheets" => p.total_sheets = v,
            "prop.drawn_by" => p.drawn_by = v,
            "prop.checked_by" => p.checked_by = v,
            "prop.approved_by" => p.approved_by = v,
            "prop.revision" => p.revision = v,
            "prop.drawing_status" => p.drawing_status = v,
            "prop.website" => p.website = v,
            "prop.revision_history" => p.revision_history = v,
            "prop.units" => p.units = v,
            "prop.dimstyle" => p.dimstyle = v,
            "prop.dim_text_height" => p.dim_text_height = v,
            "prop.dim_scale" => p.dim_scale = v,
            "prop.dim_secondary" => p.dim_secondary = v,
            "prop.autonum_prefix" => p.autonum_prefix = v,
            "prop.autonum_next" => p.autonum_next = v,
            "prop.standard_notes" => p.standard_notes = v,
            "prop.plot_include_notes" => p.plot_include_notes = v,
            "prop.nameplate_preset" => p.nameplate_preset = v,
            "prop.nameplate_block" => p.nameplate_block = v,
            "prop.takeoff_live" => p.takeoff_live = v,
            _ => {}
        }
    }
    Ok(p)
}

pub(crate) fn read_document(conn: &Connection) -> Result<Document, StoreError> {
    let mut doc = Document::new();

    {
        // Prefer schema v4 (locked); fall back for older files without the column.
        let load = |with_locked: bool| -> Result<Vec<Layer>, StoreError> {
            let sql = if with_locked {
                "SELECT name, color, linetype, lineweight_mm, visible, frozen, locked FROM layers ORDER BY name"
            } else {
                "SELECT name, color, linetype, lineweight_mm, visible, frozen FROM layers ORDER BY name"
            };
            let mut stmt = conn.prepare(sql)?;
            let rows = stmt.query_map([], move |r| {
                let lt_name: String = r.get(2)?;
                Ok(Layer {
                    name: r.get(0)?,
                    color: r.get::<_, i64>(1)? as u8,
                    linetype: LineType::parse(&lt_name).unwrap_or(LineType::Continuous),
                    lineweight_mm: r.get::<_, f64>(3)? as f32,
                    visible: r.get::<_, i64>(4)? != 0,
                    frozen: r.get::<_, i64>(5)? != 0,
                    locked: if with_locked {
                        r.get::<_, i64>(6)? != 0
                    } else {
                        false
                    },
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(StoreError::from)
        };
        let layers = load(true).or_else(|_| load(false))?;
        for layer in layers {
            doc.restore_layer(layer);
        }
    }

    {
        let mut bstmt = conn.prepare("SELECT name, base_x, base_y FROM blocks ORDER BY name")?;
        let blocks: Vec<(String, Point2)> = bstmt
            .query_map([], |r| {
                Ok((r.get::<_, String>(0)?, Point2::new(r.get(1)?, r.get(2)?)))
            })?
            .collect::<Result<_, _>>()?;

        for (name, base) in blocks {
            let mut block = Block::new(name.clone(), base);
            let mut estmt = conn
                .prepare("SELECT layer, geom FROM block_entities WHERE block = ?1 ORDER BY seq")?;
            let ents = estmt.query_map(params![name], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?;
            for row in ents {
                let (layer, geom) = row?;
                block.push(layer, decode_geometry(&geom)?);
            }
            // Schema v3+: params / visibility (ignore if tables absent on older files).
            if let Ok(mut pstmt) = conn.prepare(
                "SELECT name, default_value, kind FROM block_params WHERE block = ?1 ORDER BY name",
            ) {
                let params_ok = pstmt.query_map(params![name], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                });
                if let Ok(prows) = params_ok {
                    block.params.clear();
                    for row in prows {
                        let (pname, def, kind) = row?;
                        block.params.push(BlockParam {
                            name: pname,
                            default: def,
                            kind: BlockParamKind::parse(&kind),
                        });
                    }
                }
            }
            if let Ok(mut vstmt) =
                conn.prepare("SELECT state, hidden FROM block_vis WHERE block = ?1 ORDER BY state")
            {
                let vis_ok = vstmt.query_map(params![name], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                });
                if let Ok(vrows) = vis_ok {
                    let mut loaded = Vec::new();
                    for row in vrows {
                        let (state, hidden_s) = row?;
                        let hidden: Vec<usize> = if hidden_s.trim().is_empty() {
                            Vec::new()
                        } else {
                            hidden_s
                                .split(',')
                                .filter_map(|s| s.trim().parse().ok())
                                .collect()
                        };
                        loaded.push(VisibilityState {
                            name: state,
                            hidden,
                        });
                    }
                    if !loaded.is_empty() {
                        block.visibilities = loaded;
                    }
                }
            }
            doc.add_block(block);
        }
    }

    {
        let mut stmt = conn.prepare("SELECT id, layer, geom FROM entities ORDER BY id")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)? as u64,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        for row in rows {
            let (id, layer, geom) = row?;
            let mut attrs = HashMap::new();
            let mut astmt =
                conn.prepare("SELECT key, value FROM entity_attrs WHERE entity_id = ?1")?;
            let arows = astmt.query_map(params![id as i64], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?;
            for a in arows {
                let (k, v) = a?;
                attrs.insert(k, v);
            }
            doc.restore_entity(Entity {
                id: EntityId(id),
                layer,
                geometry: decode_geometry(&geom)?,
                attributes: attrs,
            });
        }
    }

    {
        let mut stmt =
            conn.prepare("SELECT layer, pattern, solid, geom FROM hatches ORDER BY seq")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)? != 0,
                r.get::<_, String>(3)?,
            ))
        })?;
        for row in rows {
            let (layer, pattern, solid, geom) = row?;
            let Geometry::Polyline(boundary) = decode_geometry(&geom)? else {
                return Err(StoreError::Format("hatch geom must be a polyline".into()));
            };
            doc.add_hatch(Hatch {
                layer,
                boundary,
                pattern,
                solid,
            });
        }
    }

    {
        let mut stmt = conn.prepare(
            "SELECT layer, kind, p1x, p1y, p2x, p2y, vx, vy, tx, ty, text, measurement
             FROM dimensions ORDER BY seq",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                Point2::new(r.get(2)?, r.get(3)?),
                Point2::new(r.get(4)?, r.get(5)?),
                Point2::new(r.get(6)?, r.get(7)?),
                Point2::new(r.get(8)?, r.get(9)?),
                r.get::<_, String>(10)?,
                r.get::<_, f64>(11)?,
            ))
        })?;
        for row in rows {
            let (layer, kind, p1, p2, vertex, text_anchor, text, measurement) = row?;
            doc.add_dimension(Dimension {
                layer,
                kind: parse_dim_kind(&kind)?,
                p1,
                p2,
                vertex,
                text_anchor,
                text,
                measurement,
            });
        }
    }

    doc.set_properties(read_properties(conn)?);
    Ok(doc)
}

fn dim_kind_name(k: DimensionKind) -> &'static str {
    match k {
        DimensionKind::Linear => "Linear",
        DimensionKind::Aligned => "Aligned",
        DimensionKind::Angular => "Angular",
        DimensionKind::Radius => "Radius",
        DimensionKind::Diameter => "Diameter",
        DimensionKind::OrdinateX => "OrdinateX",
        DimensionKind::OrdinateY => "OrdinateY",
    }
}

fn parse_dim_kind(s: &str) -> Result<DimensionKind, StoreError> {
    Ok(match s {
        "Linear" => DimensionKind::Linear,
        "Aligned" => DimensionKind::Aligned,
        "Angular" => DimensionKind::Angular,
        "Radius" => DimensionKind::Radius,
        "Diameter" => DimensionKind::Diameter,
        "OrdinateX" => DimensionKind::OrdinateX,
        "OrdinateY" => DimensionKind::OrdinateY,
        other => {
            return Err(StoreError::Format(format!(
                "unknown dimension kind {other:?}"
            )))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use aadt_geometry::{Circle, Point2, Polyline, Segment};
    use aadt_model::{Insert, Text};

    #[test]
    fn round_trip_entities_layers_and_attrs() {
        let mut doc = Document::new();
        doc.restore_layer(
            Layer::new("A-WALL")
                .with_color(7)
                .with_linetype(LineType::Continuous)
                .with_lineweight_mm(0.35),
        );
        doc.restore_entity(Entity {
            id: EntityId(3),
            layer: "A-WALL".into(),
            geometry: Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0))),
            attributes: HashMap::from([("height".into(), "2.7".into())]),
        });
        doc.restore_entity(Entity {
            id: EntityId(5),
            layer: "A-WALL".into(),
            geometry: Geometry::Circle(Circle::new(Point2::new(1.0, 2.0), 0.5)),
            attributes: HashMap::new(),
        });

        let dir = std::env::temp_dir().join("aadt-store-rt");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("roundtrip.aadt");
        save_path(&doc, &path).unwrap();
        let mut loaded = load_path(&path).unwrap();
        let e = loaded.get(EntityId(3)).unwrap();
        assert_eq!(e.attribute("height"), Some("2.7"));
        assert!(matches!(e.geometry, Geometry::Line(_)));
        assert!(loaded.get(EntityId(5)).is_some());
        // next_id advanced past 5 so a new add gets 6+
        let nid = loaded.add_geometry(
            "0",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(1.0, 1.0))),
        );
        assert!(nid.0 >= 6, "got {}", nid.0);
        let layer = loaded.layer("A-WALL").unwrap();
        assert_eq!(layer.color, 7);
    }

    #[test]
    fn history_checkpoint_survives_resave() {
        let mut doc = Document::new();
        doc.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0))),
        );
        let dir = std::env::temp_dir().join("aadt-store-hist");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("hist.aadt");
        let _ = std::fs::remove_file(&path);
        save_path(&doc, &path).unwrap();
        let list = list_checkpoints(&path).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].kind, "auto");

        save_named_checkpoint(&path, &doc, "Issued for construction", "", "me").unwrap();
        let list = list_checkpoints(&path).unwrap();
        assert!(list.iter().any(|c| c.kind == "named"));
        assert!(list.len() >= 2);

        let id = list.iter().find(|c| c.kind == "named").unwrap().id;
        let restored = restore_checkpoint(&path, id).unwrap();
        assert_eq!(restored.len(), 1);
    }

    #[test]
    fn round_trip_plot_notes_and_nameplate_props() {
        let mut doc = Document::new();
        let mut props = doc.properties().clone();
        props.standard_notes = "Note A\nNote B".into();
        props.plot_include_notes = "1".into();
        props.nameplate_preset = "compact".into();
        doc.set_properties(props);

        let dir = std::env::temp_dir().join("aadt-store-props");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("notes-props.aadt");
        save_path(&doc, &path).unwrap();
        let loaded = load_path(&path).unwrap();
        let p = loaded.properties();
        assert_eq!(p.standard_notes, "Note A\nNote B");
        assert_eq!(p.plot_include_notes, "1");
        assert_eq!(p.nameplate_preset, "compact");
    }

    #[test]
    fn round_trip_blocks_hatches_dims() {
        let mut doc = Document::new();
        let mut block = Block::new("DOOR", Point2::new(0.0, 0.0));
        block.push(
            "0",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(1.0, 0.0))),
        );
        doc.add_block(block);
        doc.restore_entity(Entity {
            id: EntityId(1),
            layer: "0".into(),
            geometry: Geometry::Insert(Insert::new("DOOR", Point2::new(5.0, 5.0), 90.0)),
            attributes: HashMap::new(),
        });
        doc.restore_entity(Entity {
            id: EntityId(2),
            layer: "0".into(),
            geometry: Geometry::Text(Text::new("Room", Point2::new(0.0, 0.0), 0.25, 0.0)),
            attributes: HashMap::new(),
        });
        doc.add_hatch(Hatch::new(
            "0",
            Polyline::new(
                vec![
                    Point2::new(0.0, 0.0),
                    Point2::new(1.0, 0.0),
                    Point2::new(1.0, 1.0),
                ],
                true,
            ),
            "SOLID",
        ));
        doc.add_dimension(Dimension {
            layer: "0".into(),
            kind: DimensionKind::Aligned,
            p1: Point2::new(0.0, 0.0),
            p2: Point2::new(4.0, 0.0),
            vertex: Point2::new(0.0, 0.0),
            text_anchor: Point2::new(2.0, 0.5),
            text: String::new(),
            measurement: 4.0,
        });

        let path = std::env::temp_dir().join("aadt-store-blocks.aadt");
        save_path(&doc, &path).unwrap();
        let loaded = load_path(&path).unwrap();
        assert!(loaded.block("DOOR").is_some());
        assert_eq!(loaded.hatches().len(), 1);
        assert_eq!(loaded.dimensions().len(), 1);
        assert!(matches!(
            loaded.get(EntityId(2)).unwrap().geometry,
            Geometry::Text(_)
        ));
    }
}
