//! # aadt-vdb — **ShilpiDB**, a custom on-disk vector database for the drawing
//!
//! ShilpiDB stores a drawing **as vectors**: every entity (line, polyline, circle,
//! arc, text, block insert) is a native binary record in a purpose-built `.vdb`
//! file, alongside layers, blocks, hatches, dimensions, floors, and a built-in
//! **spatial index** over the entities' extents. It is a self-contained database
//! format we own end to end — no SQLite, no `serde`, no external crate,
//! `unsafe`-free.
//!
//! ```
//! use aadt_vdb::ShilpiDb;
//! use aadt_geometry::{Bbox, Point2, Segment};
//! use aadt_model::{Document, Geometry};
//!
//! let mut doc = Document::new();
//! doc.add_geometry("0", Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(4.0, 0.0))));
//! let path = std::env::temp_dir().join("aadt-vdb-doctest.vdb");
//! aadt_vdb::save_path(&doc, &path).unwrap();
//!
//! // Reopen and query the spatial index for entities in a window.
//! let db = ShilpiDb::open(&path).unwrap();
//! let hits = db.query_bbox(Bbox::from_corners(Point2::new(-1.0, -1.0), Point2::new(1.0, 1.0)));
//! assert_eq!(hits.len(), 1);
//! ```

mod buf;
mod client;
mod codec;
mod objects;

use std::path::Path;

use aadt_geometry::{Bbox, Point2};
use aadt_graph::Graph;
use aadt_model::{Document, Entity, EntityId, Geometry};
use aadt_object::{AuditLog, ObjectStore};

pub use buf::VdbError;
pub use client::{VdbClient, VdbClientError};
// The binary codec + spatial index come from the standalone `shilpidb` engine.
// The codec primitives are re-exported so the ShilpiDB wire protocol
// (`aadt-vdb-proto`) frames payloads with the exact encoding used on disk — one
// format for the file and the wire. `PointRead`/`PointWrite` add the `Point2`
// codec that the engine (scalar-only) deliberately omits.
pub use buf::{PointRead, PointWrite, Reader, Writer};
pub use shilpidb::SpatialGrid;
use shilpidb::MAGIC;

/// Pre-rename magic ("VASTUDB" + NUL). Still accepted on read so databases
/// written before the ShilpiDB rename keep opening.
const LEGACY_MAGIC: &[u8; 8] = b"VASTUDB\0";
/// On-disk format version. v2 adds the Object DB / Graph DB / audit sections
/// (AI-native Phase 1); v1 files still load with empty object stores.
pub const FORMAT_VERSION: u32 = 2;

/// The canonical file extension for the vector database.
pub const EXTENSION: &str = "vdb";

/// Write `doc` to `path` as a `.vdb` vector database (document + spatial index).
///
/// Written to a sibling temp file and renamed, so a failed write never truncates
/// an existing database.
pub fn save_path(doc: &Document, path: impl AsRef<Path>) -> Result<(), VdbError> {
    save_path_with_objects(
        doc,
        path,
        &ObjectStore::new(),
        &Graph::new(),
        &AuditLog::new(),
    )
}

/// Write `doc` plus its Object DB / Graph DB / audit log (`.vdb` v2 sections).
pub fn save_path_with_objects(
    doc: &Document,
    path: impl AsRef<Path>,
    objects: &ObjectStore,
    graph: &Graph,
    audit: &AuditLog,
) -> Result<(), VdbError> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let mut w = Writer::new();
    w.raw(MAGIC);
    w.u32(FORMAT_VERSION);
    codec::encode_document(&mut w, doc);
    build_index(doc).write(&mut w);
    objects::write_sections(&mut w, objects, graph, audit);
    let bytes = w.into_bytes();

    let tmp = path.with_extension("vdb.tmp");
    std::fs::write(&tmp, &bytes)?;
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Load a `.vdb` file into a [`Document`] (ignores the stored index).
pub fn load_path(path: impl AsRef<Path>) -> Result<Document, VdbError> {
    let bytes = std::fs::read(path.as_ref())?;
    let mut r = Reader::new(&bytes);
    read_header(&mut r)?;
    codec::decode_document(&mut r)
}

fn read_header(r: &mut Reader) -> Result<u32, VdbError> {
    let magic = r.raw(MAGIC.len())?;
    if magic != MAGIC && magic != LEGACY_MAGIC {
        return Err(VdbError::Format(
            "not a ShilpiDB vector database (bad magic)".into(),
        ));
    }
    let ver = r.u32()?;
    if ver > FORMAT_VERSION {
        return Err(VdbError::Format(format!(
            "unsupported .vdb version {ver} (max {FORMAT_VERSION})"
        )));
    }
    Ok(ver)
}

/// An opened ShilpiDB database: the drawing, its spatial index, and (v2) the
/// Object DB / Graph DB / audit log — ready to query.
pub struct ShilpiDb {
    doc: Document,
    index: SpatialGrid,
    objects: ObjectStore,
    graph: Graph,
    audit: AuditLog,
}

impl ShilpiDb {
    /// Open a `.vdb` file — load the document and its spatial index (rebuilding
    /// the index from the geometry if the stored one is missing or unreadable),
    /// plus the object/graph/audit sections when present (v2).
    pub fn open(path: impl AsRef<Path>) -> Result<ShilpiDb, VdbError> {
        let bytes = std::fs::read(path.as_ref())?;
        let mut r = Reader::new(&bytes);
        let ver = read_header(&mut r)?;
        let doc = codec::decode_document(&mut r)?;
        let index = SpatialGrid::read(&mut r).unwrap_or_else(|_| build_index(&doc));
        let (objects, graph, audit) = if ver >= 2 {
            objects::read_sections(&mut r)?
        } else {
            (ObjectStore::new(), Graph::new(), AuditLog::new())
        };
        Ok(ShilpiDb {
            doc,
            index,
            objects,
            graph,
            audit,
        })
    }

    /// Build an in-memory database from a document already in hand.
    pub fn from_document(doc: Document) -> ShilpiDb {
        let index = build_index(&doc);
        ShilpiDb {
            doc,
            index,
            objects: ObjectStore::new(),
            graph: Graph::new(),
            audit: AuditLog::new(),
        }
    }

    /// The Object DB (empty for v1 files or when never promoted).
    pub fn objects(&self) -> &ObjectStore {
        &self.objects
    }

    /// The Graph DB.
    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    /// The append-only object audit log.
    pub fn audit(&self) -> &AuditLog {
        &self.audit
    }

    /// The loaded drawing.
    pub fn document(&self) -> &Document {
        &self.doc
    }

    /// Take ownership of the loaded drawing.
    pub fn into_document(self) -> Document {
        self.doc
    }

    /// The spatial index.
    pub fn index(&self) -> &SpatialGrid {
        &self.index
    }

    /// Entity ids whose extent intersects `rect`, ascending — served by the index.
    pub fn query_bbox(&self, rect: Bbox) -> Vec<EntityId> {
        self.index
            .query_bbox(to_engine_bbox(rect))
            .into_iter()
            .map(EntityId)
            .collect()
    }

    /// Entity ids within `tol` of `p` — served by the index.
    pub fn query_point(&self, p: Point2, tol: f64) -> Vec<EntityId> {
        self.index
            .query_point(shilpidb::Point::new(p.x, p.y), tol)
            .into_iter()
            .map(EntityId)
            .collect()
    }

    /// The entities whose extent intersects `rect` (resolved against the document).
    pub fn entities_within(&self, rect: Bbox) -> Vec<&Entity> {
        self.query_bbox(rect)
            .into_iter()
            .filter_map(|id| self.doc.get(id))
            .collect()
    }
}

/// Build the spatial index over a document's entities (block-aware bounding
/// boxes), mapping to the engine's `(u64, Bbox)` records.
pub fn build_index(doc: &Document) -> SpatialGrid {
    let boxes: Vec<(u64, shilpidb::Bbox)> = doc
        .entities()
        .into_iter()
        .filter_map(|e| entity_bbox(doc, e).map(|bb| (e.id.0, to_engine_bbox(bb))))
        .collect();
    SpatialGrid::build(boxes)
}

/// Convert an AAD geometry bounding box to the engine's plain [`shilpidb::Bbox`].
pub(crate) fn to_engine_bbox(b: Bbox) -> shilpidb::Bbox {
    shilpidb::Bbox::new(
        shilpidb::Point::new(b.min.x, b.min.y),
        shilpidb::Point::new(b.max.x, b.max.y),
    )
}

/// Query a built [`SpatialGrid`] with an AAD bounding box, mapping the engine's
/// `u64` ids back to [`EntityId`]. Convenience for callers holding a raw index
/// (e.g. the LISP `aadt-within`) rather than an opened [`ShilpiDb`].
pub fn query_index_bbox(index: &SpatialGrid, rect: Bbox) -> Vec<EntityId> {
    index
        .query_bbox(to_engine_bbox(rect))
        .into_iter()
        .map(EntityId)
        .collect()
}

/// The axis-aligned bounding box of an entity, expanding block inserts.
pub fn entity_bbox(doc: &Document, entity: &Entity) -> Option<Bbox> {
    Bbox::of_points(geometry_points(doc, &entity.geometry))
}

fn geometry_points(doc: &Document, g: &Geometry) -> Vec<Point2> {
    match g {
        Geometry::Line(s) => vec![s.a, s.b],
        Geometry::Polyline(p) => p.vertices.clone(),
        Geometry::Circle(c) => vec![
            Point2::new(c.center.x - c.radius, c.center.y),
            Point2::new(c.center.x + c.radius, c.center.y),
            Point2::new(c.center.x, c.center.y - c.radius),
            Point2::new(c.center.x, c.center.y + c.radius),
        ],
        Geometry::Arc(a) => a.to_polyline(8).vertices,
        Geometry::Text(t) => {
            let w = (t.value.chars().count().max(1) as f64) * t.height * 0.6;
            vec![
                t.position,
                Point2::new(t.position.x + w, t.position.y + t.height),
            ]
        }
        Geometry::Insert(ins) => insert_points(doc, ins),
    }
}

/// Transformed corner/vertex points of a block insert's footprint.
fn insert_points(doc: &Document, ins: &aadt_model::Insert) -> Vec<Point2> {
    let Some(block) = doc.block(&ins.name) else {
        return vec![ins.position];
    };
    let (sin, cos) = ins.rotation.to_radians().sin_cos();
    let mut out = Vec::new();
    for be in &block.entities {
        for p in raw_points(&be.geometry) {
            let lx = (p.x - block.base.x) * ins.scale_x;
            let ly = (p.y - block.base.y) * ins.scale_y;
            out.push(Point2::new(
                ins.position.x + lx * cos - ly * sin,
                ins.position.y + lx * sin + ly * cos,
            ));
        }
    }
    if out.is_empty() {
        out.push(ins.position);
    }
    out
}

/// Points of a flat block-content geometry (no nested-insert expansion).
fn raw_points(g: &Geometry) -> Vec<Point2> {
    match g {
        Geometry::Line(s) => vec![s.a, s.b],
        Geometry::Polyline(p) => p.vertices.clone(),
        Geometry::Circle(c) => vec![
            Point2::new(c.center.x - c.radius, c.center.y - c.radius),
            Point2::new(c.center.x + c.radius, c.center.y + c.radius),
        ],
        Geometry::Arc(a) => a.to_polyline(4).vertices,
        Geometry::Text(t) => vec![t.position],
        Geometry::Insert(ins) => vec![ins.position],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aadt_geometry::{Arc, Circle, Point2, Polyline, Segment};
    use aadt_model::{
        Block, Dimension, DimensionKind, Entity, Geometry, Hatch, Insert, Layer, LineType, Text,
    };
    use std::collections::HashMap;

    fn tmp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("aadt-vdb-tests");
        let _ = std::fs::create_dir_all(&dir);
        dir.join(name)
    }

    #[test]
    fn legacy_vastudb_magic_still_opens() {
        // A database written before the ShilpiDB rename (magic "VASTUDB\0") must
        // still open. Save a normal file, then rewrite its 8-byte magic.
        let mut doc = Document::new();
        doc.add_geometry(
            "0",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(4.0, 0.0))),
        );
        let path = tmp("legacy-magic.vdb");
        save_path(&doc, &path).unwrap();

        let mut bytes = std::fs::read(&path).unwrap();
        bytes[..LEGACY_MAGIC.len()].copy_from_slice(LEGACY_MAGIC);
        std::fs::write(&path, &bytes).unwrap();

        let db = ShilpiDb::open(&path).unwrap();
        assert_eq!(db.document().len(), 1);
    }

    #[test]
    fn round_trips_a_rich_document() {
        let mut doc = Document::new();
        doc.ensure_default_spaces();
        doc.restore_layer(
            Layer::new("A-WALL")
                .with_color(7)
                .with_linetype(LineType::Dashed)
                .with_lineweight_mm(0.35),
        );
        let mut block = Block::new("DOOR", Point2::new(0.0, 0.0));
        block.push(
            "0",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(1.0, 0.0))),
        );
        doc.add_block(block);
        doc.restore_entity(Entity {
            id: EntityId(3),
            layer: "A-WALL".into(),
            space: Default::default(),
            geometry: Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0))),
            attributes: HashMap::from([("height".into(), "2.7".into())]),
        });
        doc.restore_entity(Entity {
            id: EntityId(5),
            layer: "0".into(),
            space: Default::default(),
            geometry: Geometry::Arc(Arc::new(Point2::new(1.0, 2.0), 0.5, 0.0, 90.0)),
            attributes: HashMap::new(),
        });
        doc.restore_entity(Entity {
            id: EntityId(6),
            layer: "0".into(),
            space: Default::default(),
            geometry: Geometry::Insert(Insert::new("DOOR", Point2::new(5.0, 5.0), 90.0)),
            attributes: HashMap::new(),
        });
        doc.restore_entity(Entity {
            id: EntityId(7),
            layer: "0".into(),
            space: Default::default(),
            geometry: Geometry::Text(Text::new("ROOM 1", Point2::new(2.0, 2.0), 0.25, 0.0)),
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
            space: Default::default(),
            layer: "0".into(),
            kind: DimensionKind::Aligned,
            p1: Point2::new(0.0, 0.0),
            p2: Point2::new(4.0, 0.0),
            vertex: Point2::new(0.0, 0.0),
            text_anchor: Point2::new(2.0, 0.5),
            text: String::new(),
            measurement: 4.0,
        });
        let mut props = doc.properties().clone();
        props.title = "Plan".into();
        props.standard_notes = "Note A\nNote B".into();
        doc.set_properties(props);

        let path = tmp("rich.vdb");
        save_path(&doc, &path).unwrap();
        let loaded = load_path(&path).unwrap();

        assert_eq!(loaded.len(), 4);
        let e = loaded.get(EntityId(3)).unwrap();
        assert_eq!(e.attribute("height"), Some("2.7"));
        assert!(matches!(e.geometry, Geometry::Line(_)));
        assert_eq!(loaded.layer("A-WALL").unwrap().color, 7);
        assert!(loaded.block("DOOR").is_some());
        assert_eq!(loaded.hatches().len(), 1);
        assert_eq!(loaded.dimensions().len(), 1);
        assert_eq!(loaded.properties().title, "Plan");
        assert_eq!(loaded.properties().standard_notes, "Note A\nNote B");
        // id sequence advanced past the max restored id.
        let mut m = loaded;
        let nid = m.add_geometry(
            "0",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(1.0, 1.0))),
        );
        assert!(nid.0 >= 8, "next id was {}", nid.0);
    }

    #[test]
    fn spatial_index_narrows_a_window_query() {
        let mut doc = Document::new();
        // A grid of far-apart circles; a small window should hit only one.
        for i in 0..25 {
            let x = (i % 5) as f64 * 100.0;
            let y = (i / 5) as f64 * 100.0;
            doc.add_geometry("0", Geometry::Circle(Circle::new(Point2::new(x, y), 1.0)));
        }
        let path = tmp("grid.vdb");
        save_path(&doc, &path).unwrap();
        let vdb = ShilpiDb::open(&path).unwrap();
        assert_eq!(vdb.index().len(), 25);

        let hits = vdb.query_bbox(Bbox::from_corners(
            Point2::new(199.0, 99.0),
            Point2::new(201.0, 101.0),
        ));
        assert_eq!(hits.len(), 1, "only the circle at (200,100)");
        let e = vdb.document().get(hits[0]).unwrap();
        assert!(matches!(e.geometry, Geometry::Circle(_)));

        // An empty region hits nothing.
        assert!(vdb
            .query_bbox(Bbox::from_corners(
                Point2::new(40.0, 40.0),
                Point2::new(45.0, 45.0)
            ))
            .is_empty());
    }

    #[test]
    fn atomic_save_does_not_corrupt_on_resave() {
        let mut doc = Document::new();
        doc.add_geometry(
            "0",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(1.0, 0.0))),
        );
        let path = tmp("resave.vdb");
        save_path(&doc, &path).unwrap();
        doc.add_geometry(
            "0",
            Geometry::Line(Segment::new(Point2::new(0.0, 1.0), Point2::new(1.0, 1.0))),
        );
        save_path(&doc, &path).unwrap();
        assert_eq!(load_path(&path).unwrap().len(), 2);
    }

    #[test]
    fn objects_graph_audit_round_trip_in_v2() {
        use aadt_graph::Relation;
        use aadt_object::ObjectKind;
        use std::collections::BTreeMap;

        let mut doc = Document::new();
        let e = doc.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(6.0, 0.0))),
        );
        let mut store = ObjectStore::new();
        let mut props = BTreeMap::new();
        props.insert("length".into(), "6.000".into());
        let wall = store.insert(ObjectKind::Wall, vec![e], props, 1.0, doc.revision());
        let room = store.insert(ObjectKind::Room, vec![], BTreeMap::new(), 1.0, 0);
        let mut graph = Graph::new();
        graph.add(wall, Relation::Bounds, room);
        let mut audit = AuditLog::new();
        audit.record("promote", wall, "", "Wall …", false, true, "detect");

        let path = tmp("objects.vdb");
        save_path_with_objects(&doc, &path, &store, &graph, &audit).unwrap();
        let db = ShilpiDb::open(&path).unwrap();
        assert_eq!(db.objects().len(), 2);
        let w = db.objects().get(wall).unwrap();
        assert_eq!(w.kind, ObjectKind::Wall);
        assert_eq!(w.entities, vec![e]);
        assert_eq!(w.prop("length"), Some("6.000"));
        assert_eq!(db.graph().edges().len(), 1);
        assert_eq!(db.graph().edges()[0].rel, Relation::Bounds);
        assert_eq!(db.audit().len(), 1);
        assert_eq!(db.audit().entries()[0].action, "promote");

        // A plain save (no objects) still opens with empty stores.
        let path2 = tmp("noobjects.vdb");
        save_path(&doc, &path2).unwrap();
        let db2 = ShilpiDb::open(&path2).unwrap();
        assert!(db2.objects().is_empty() && db2.graph().is_empty());
    }

    #[test]
    fn rejects_foreign_files() {
        let path = tmp("garbage.vdb");
        std::fs::write(&path, b"not a vdb file at all").unwrap();
        assert!(load_path(&path).is_err());
    }
}
