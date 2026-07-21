//! # aadt-dxf — DXF interoperability (file I/O layer, spec §3 / §7)
//!
//! DWG/DXF round-trip is non-negotiable for architects exchanging files (spec §7).
//! This crate reads and writes the ASCII DXF entity types AADT currently models:
//!
//! - **Import:** `LINE`, `LWPOLYLINE`, legacy `POLYLINE`/`VERTEX`, `CIRCLE`,
//!   `ARC`, `TEXT`, `INSERT` (block references, with their trailing `ATTRIB`
//!   values), `BLOCK` definitions from the `BLOCKS` section, and `HATCH` /
//!   linear-aligned `DIMENSION` annotation.
//! - **Export:** the `BLOCKS` section, then `LINE`, `LWPOLYLINE`, `CIRCLE`,
//!   `ARC`, `TEXT`, `INSERT` (+`ATTRIB`), `HATCH`, and `DIMENSION`.
//!
//! Coordinates use Rust's shortest round-trip float formatting and insert
//! attributes are written in sorted key order, so an import → export → import cycle
//! reproduces the document exactly.
//!
//! Still bounded for now (see `docs/ROADMAP.md` / ADR-0002): hatches read as a
//! single polygonal boundary (multi-path/edge geometry is not modeled),
//! dimensions cover the linear/aligned subset (radial/angular/ordinate are not),
//! and block content is limited to single-record geometry — a nested `INSERT`
//! inside a block is skipped rather than mis-parsed. Binary
//! **DWG** is a later milestone via the ODA SDK or a LibreDWG FFI binding. The
//! public entry points are stable so a fuller backend (e.g. the `dxf` crate) can
//! replace the internal parser without touching callers.

use std::collections::HashMap;
use std::fmt;
use std::path::Path;

use aadt_geometry::{Arc, Circle, Point2, Polyline, Segment};
use aadt_model::{Block, Dimension, DimensionKind, Document, Geometry, Hatch, Insert, Layer, Text};

/// Error raised while reading or writing a DXF file.
#[derive(Debug)]
pub enum DxfError {
    /// The file could not be read from or written to disk.
    Io(std::io::Error),
    /// The DXF group-code structure was malformed.
    Parse(String),
}

impl fmt::Display for DxfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DxfError::Io(e) => write!(f, "reading DXF: {e}"),
            DxfError::Parse(m) => write!(f, "parsing DXF: {m}"),
        }
    }
}

impl std::error::Error for DxfError {}

impl From<std::io::Error> for DxfError {
    fn from(e: std::io::Error) -> Self {
        DxfError::Io(e)
    }
}

// --- Import ------------------------------------------------------------------

/// Read a DXF file from disk into a fresh [`Document`].
pub fn import_path(path: impl AsRef<Path>) -> Result<Document, DxfError> {
    let text = std::fs::read_to_string(path)?;
    import_str(&text)
}

/// Parse DXF text into a fresh [`Document`].
pub fn import_str(text: &str) -> Result<Document, DxfError> {
    let pairs = parse_pairs(text)?;
    let records = split_records(&pairs);

    let mut doc = Document::new();
    let mut section = Section::None;

    let mut i = 0;
    while i < records.len() {
        let rec = &records[i];
        match rec.kind.as_str() {
            "SECTION" => {
                section = match rec.string(2) {
                    Some("ENTITIES") => Section::Entities,
                    Some("BLOCKS") => Section::Blocks,
                    Some("TABLES") => Section::Tables,
                    _ => Section::Other,
                };
            }
            "ENDSEC" => section = Section::None,
            // A LAYER table record: name, colour (62; negative = off), flags
            // (70 bit 1 = frozen). Read before ENTITIES, so the layer carries its
            // display state before any entity references it.
            "LAYER" if section == Section::Tables => import_layer(&mut doc, rec),
            // A BLOCK definition consumes its content up to ENDBLK.
            "BLOCK" if section == Section::Blocks => {
                i = import_block(&mut doc, &records, i);
                continue;
            }
            "POLYLINE" if section == Section::Entities => {
                // Legacy POLYLINE consumes the following VERTEX records up to SEQEND.
                i = import_polyline(&mut doc, &records, i);
                continue;
            }
            "INSERT" if section == Section::Entities => {
                // A block reference optionally followed by ATTRIB records + SEQEND.
                i = import_insert(&mut doc, &records, i);
                continue;
            }
            "HATCH" if section == Section::Entities => import_hatch(&mut doc, rec),
            "DIMENSION" if section == Section::Entities => import_dimension(&mut doc, rec),
            _ if section == Section::Entities => {
                if let Some((layer, geom)) = simple_geometry(rec) {
                    doc.add_geometry(layer, geom);
                }
            }
            _ => {}
        }
        i += 1;
    }
    Ok(doc)
}

/// A single DXF group-code / value pair.
struct Pair {
    code: i32,
    value: String,
}

/// A DXF record: a `0`-code entity/marker plus its trailing fields.
struct Record {
    kind: String,
    fields: Vec<Pair>,
}

impl Record {
    fn string(&self, code: i32) -> Option<&str> {
        self.fields
            .iter()
            .find(|p| p.code == code)
            .map(|p| p.value.as_str())
    }

    fn float(&self, code: i32) -> Option<f64> {
        self.string(code).and_then(|s| s.trim().parse::<f64>().ok())
    }

    fn int(&self, code: i32) -> Option<i64> {
        self.string(code).and_then(|s| s.trim().parse::<i64>().ok())
    }

    fn layer(&self) -> String {
        self.string(8).unwrap_or("0").to_string()
    }
}

/// Turn the raw text into (code, value) pairs. DXF encodes a group code on one
/// line and its value on the next.
fn parse_pairs(text: &str) -> Result<Vec<Pair>, DxfError> {
    let mut lines = text.lines();
    let mut pairs = Vec::new();
    while let Some(code_line) = lines.next() {
        let code_str = code_line.trim();
        if code_str.is_empty() {
            continue;
        }
        let code: i32 = code_str
            .parse()
            .map_err(|_| DxfError::Parse(format!("invalid group code {code_str:?}")))?;
        let value = lines
            .next()
            .ok_or_else(|| DxfError::Parse(format!("group code {code} missing a value")))?;
        pairs.push(Pair {
            code,
            value: value.trim().to_string(),
        });
    }
    Ok(pairs)
}

/// Split a pair stream into records delimited by `0`-code markers.
fn split_records(pairs: &[Pair]) -> Vec<Record> {
    let mut records = Vec::new();
    let mut current: Option<Record> = None;
    for p in pairs {
        if p.code == 0 {
            if let Some(rec) = current.take() {
                records.push(rec);
            }
            current = Some(Record {
                kind: p.value.clone(),
                fields: Vec::new(),
            });
        } else if let Some(rec) = current.as_mut() {
            rec.fields.push(Pair {
                code: p.code,
                value: p.value.clone(),
            });
        }
    }
    if let Some(rec) = current.take() {
        records.push(rec);
    }
    records
}

/// Which DXF section the parser is currently inside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    None,
    Entities,
    Blocks,
    /// The TABLES section — read for its `LAYER` table (colour / on-off / frozen).
    Tables,
    /// A section AADT does not read (HEADER, …).
    Other,
}

/// Parse a single-record entity (`LINE`, `LWPOLYLINE`, `CIRCLE`, `ARC`, `TEXT`)
/// into its layer and geometry. Returns `None` for record kinds that are not
/// single-record geometry — `POLYLINE`/`INSERT` (which span several records) and
/// everything else. Shared by entity and block-content import so both understand
/// exactly the same set.
fn simple_geometry(rec: &Record) -> Option<(String, Geometry)> {
    let geom = match rec.kind.as_str() {
        "LINE" => {
            let (x1, y1, x2, y2) = (
                rec.float(10)?,
                rec.float(20)?,
                rec.float(11)?,
                rec.float(21)?,
            );
            Geometry::Line(Segment::new(Point2::new(x1, y1), Point2::new(x2, y2)))
        }
        "LWPOLYLINE" => {
            let mut vertices = Vec::new();
            let mut pending_x: Option<f64> = None;
            for field in &rec.fields {
                match field.code {
                    10 => pending_x = field.value.trim().parse::<f64>().ok(),
                    20 => {
                        if let (Some(x), Ok(y)) =
                            (pending_x.take(), field.value.trim().parse::<f64>())
                        {
                            vertices.push(Point2::new(x, y));
                        }
                    }
                    _ => {}
                }
            }
            if vertices.len() < 2 {
                return None;
            }
            let closed = rec.int(70).map(|f| f & 1 == 1).unwrap_or(false);
            Geometry::Polyline(Polyline::new(vertices, closed))
        }
        "CIRCLE" => {
            let (cx, cy, r) = (rec.float(10)?, rec.float(20)?, rec.float(40)?);
            Geometry::Circle(Circle::new(Point2::new(cx, cy), r))
        }
        "ARC" => {
            let (cx, cy, r) = (rec.float(10)?, rec.float(20)?, rec.float(40)?);
            let start = rec.float(50).unwrap_or(0.0);
            let end = rec.float(51).unwrap_or(360.0);
            Geometry::Arc(Arc::new(Point2::new(cx, cy), r, start, end))
        }
        "TEXT" => {
            let (x, y) = (rec.float(10)?, rec.float(20)?);
            let value = rec.string(1).unwrap_or("").to_string();
            let height = rec.float(40).unwrap_or(0.0);
            let rotation = rec.float(50).unwrap_or(0.0);
            Geometry::Text(Text::new(value, Point2::new(x, y), height, rotation))
        }
        _ => return None,
    };
    Some((rec.layer(), geom))
}

/// Import a `BLOCK` definition at `start`, consuming its content records up to
/// (and including) the `ENDBLK`. Returns the index of the next unconsumed record.
///
/// Block content is limited to single-record geometry: nested `INSERT`s and
/// legacy `POLYLINE`s inside a block are skipped rather than mis-parsed, so an
/// unusual block loses detail but never corrupts the surrounding drawing.
fn import_block(doc: &mut Document, records: &[Record], start: usize) -> usize {
    let header = &records[start];
    let name = header.string(2).unwrap_or("").to_string();
    let base = Point2::new(
        header.float(10).unwrap_or(0.0),
        header.float(20).unwrap_or(0.0),
    );
    let mut block = Block::new(name, base);

    let mut j = start + 1;
    while j < records.len() {
        match records[j].kind.as_str() {
            "ENDBLK" => {
                j += 1;
                break;
            }
            _ => {
                if let Some((layer, geom)) = simple_geometry(&records[j]) {
                    block.push(layer, geom);
                }
                j += 1;
            }
        }
    }
    // A block with no name is malformed; keep only named definitions.
    if !block.name.is_empty() {
        doc.add_block(block);
    }
    j
}

/// Import a legacy `POLYLINE` header at `start`, consuming the following `VERTEX`
/// records up to (and including) the `SEQEND`. Returns the index of the next
/// unconsumed record.
fn import_polyline(doc: &mut Document, records: &[Record], start: usize) -> usize {
    let header = &records[start];
    let layer = header.layer();
    let closed = header.int(70).map(|f| f & 1 == 1).unwrap_or(false);

    let mut vertices = Vec::new();
    let mut j = start + 1;
    while j < records.len() {
        match records[j].kind.as_str() {
            "VERTEX" => {
                if let (Some(x), Some(y)) = (records[j].float(10), records[j].float(20)) {
                    vertices.push(Point2::new(x, y));
                }
                j += 1;
            }
            "SEQEND" => {
                j += 1;
                break;
            }
            _ => break,
        }
    }
    if vertices.len() >= 2 {
        add_polyline(doc, layer, vertices, closed);
    }
    j
}

fn add_polyline(doc: &mut Document, layer: String, vertices: Vec<Point2>, closed: bool) {
    let poly = Polyline::new(vertices, closed);
    let attributes: HashMap<String, String> = HashMap::new();
    doc.add_entity(layer, Geometry::Polyline(poly), attributes);
}

/// Import a `HATCH`: pattern name (2), solid flag (70), and the boundary as the
/// record's `10`/`20` vertex pairs. Real hatches encode multiple boundary paths
/// with edge-type data; AADT reads the vertices as a single closed boundary,
/// which covers the common polygonal case and preserves the region for
/// round-trip. A hatch with fewer than three vertices is skipped.
fn import_hatch(doc: &mut Document, rec: &Record) {
    let mut vertices = Vec::new();
    let mut pending_x: Option<f64> = None;
    for field in &rec.fields {
        match field.code {
            10 => pending_x = field.value.trim().parse::<f64>().ok(),
            20 => {
                if let (Some(x), Ok(y)) = (pending_x.take(), field.value.trim().parse::<f64>()) {
                    vertices.push(Point2::new(x, y));
                }
            }
            _ => {}
        }
    }
    if vertices.len() < 3 {
        return;
    }
    let pattern = rec.string(2).unwrap_or("SOLID").to_string();
    let mut hatch = Hatch::new(rec.layer(), Polyline::new(vertices, true), pattern);
    // An explicit 70 flag overrides the name-derived solid guess.
    if let Some(flag) = rec.int(70) {
        hatch.solid = flag != 0;
    }
    doc.add_hatch(hatch);
}

/// Import a linear/aligned `DIMENSION`: measured points (13/23, 14/24), text
/// anchor (11/21), text override (1), stored measurement (42), and type (70).
/// Type flag bit 0..2 == 1 is aligned; everything else is read as linear.
/// Radial, angular, and ordinate dimensions are not modeled and are skipped
/// (no measured points to read in the 13/14 slots).
fn import_dimension(doc: &mut Document, rec: &Record) {
    let (Some(x1), Some(y1), Some(x2), Some(y2)) =
        (rec.float(13), rec.float(23), rec.float(14), rec.float(24))
    else {
        return;
    };
    let kind = match rec.int(70).map(|f| f & 7) {
        Some(1) => DimensionKind::Aligned,
        _ => DimensionKind::Linear,
    };
    let anchor = Point2::new(rec.float(11).unwrap_or(0.0), rec.float(21).unwrap_or(0.0));
    doc.add_dimension(Dimension {
        layer: rec.layer(),
        kind,
        p1: Point2::new(x1, y1),
        p2: Point2::new(x2, y2),
        text_anchor: anchor,
        text: rec.string(1).unwrap_or("").to_string(),
        measurement: rec.float(42).unwrap_or(0.0),
    });
}

/// Import an `INSERT` (block reference) at `start`, gathering any following
/// `ATTRIB` records (tag → value) up to the `SEQEND` into the entity's attributes.
/// Returns the index of the next unconsumed record.
fn import_insert(doc: &mut Document, records: &[Record], start: usize) -> usize {
    let header = &records[start];
    let name = header.string(2).unwrap_or("").to_string();
    let position = Point2::new(
        header.float(10).unwrap_or(0.0),
        header.float(20).unwrap_or(0.0),
    );
    let rotation = header.float(50).unwrap_or(0.0);

    let mut attributes: HashMap<String, String> = HashMap::new();
    let mut j = start + 1;
    while j < records.len() {
        match records[j].kind.as_str() {
            "ATTRIB" => {
                if let (Some(tag), Some(value)) = (records[j].string(2), records[j].string(1)) {
                    attributes.insert(tag.to_string(), value.to_string());
                }
                j += 1;
            }
            "SEQEND" => {
                j += 1;
                break;
            }
            _ => break,
        }
    }

    let insert = Insert::new(name, position, rotation);
    doc.add_entity(header.layer(), Geometry::Insert(insert), attributes);
    j
}

// --- Export ------------------------------------------------------------------

/// Serialize a document to DXF text. A `BLOCKS` section (definitions, by name)
/// precedes the `ENTITIES` section, matching DXF ordering so a reader sees a
/// block defined before any `INSERT` references it. Everything is written in a
/// deterministic order (blocks by name, entities by id).
pub fn export_str(doc: &Document) -> String {
    let mut out = String::new();

    // TABLES (layer state) precede BLOCKS and ENTITIES, per DXF ordering.
    write_layer_table(&mut out, doc);

    let blocks = doc.blocks();
    if !blocks.is_empty() {
        out.push_str("0\nSECTION\n2\nBLOCKS\n");
        for b in blocks {
            out.push_str(&format!(
                "0\nBLOCK\n2\n{}\n10\n{}\n20\n{}\n",
                b.name, b.base.x, b.base.y
            ));
            for be in &b.entities {
                write_geometry(&mut out, &be.layer, &be.geometry, None);
            }
            out.push_str("0\nENDBLK\n");
        }
        out.push_str("0\nENDSEC\n");
    }

    out.push_str("0\nSECTION\n2\nENTITIES\n");
    for e in doc.entities() {
        write_geometry(&mut out, &e.layer, &e.geometry, Some(&e.attributes));
    }
    // Annotation follows the drawn entities, in insertion order.
    for h in doc.hatches() {
        write_hatch(&mut out, h);
    }
    for d in doc.dimensions() {
        write_dimension(&mut out, d);
    }
    out.push_str("0\nENDSEC\n0\nEOF\n");
    out
}

/// Read a `LAYER` table record into the document's layer table. Colour is DXF
/// group 62 (negative encodes "off"), frozen is flag 70 bit 1.
fn import_layer(doc: &mut Document, rec: &Record) {
    let Some(name) = rec.string(2) else { return };
    let mut layer = Layer::new(name);
    if let Some(c) = rec.int(62) {
        layer.visible = c >= 0;
        let aci = c.unsigned_abs();
        if (1..=255).contains(&aci) {
            layer.color = aci as u8;
        }
    }
    if let Some(flags) = rec.int(70) {
        layer.frozen = flags & 1 != 0;
    }
    doc.add_layer(layer);
}

/// Write a TABLES section with the LAYER table — but only for layers whose state
/// differs from the default (colour 7, on, thawed). A drawing that never touched
/// layer state emits no TABLES section, so existing all-default fixtures are
/// byte-for-byte unchanged. Layers are written sorted by name for determinism.
fn write_layer_table(out: &mut String, doc: &Document) {
    let mut layers: Vec<&Layer> = doc
        .layers()
        .filter(|l| l.color != aadt_model::DEFAULT_LAYER_COLOR || !l.visible || l.frozen)
        .collect();
    if layers.is_empty() {
        return;
    }
    layers.sort_by(|a, b| a.name.cmp(&b.name));
    out.push_str("0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nLAYER\n");
    for l in layers {
        let color = if l.visible {
            i32::from(l.color)
        } else {
            -i32::from(l.color)
        };
        let flags = i32::from(l.frozen);
        out.push_str(&format!(
            "0\nLAYER\n2\n{}\n70\n{}\n62\n{}\n",
            l.name, flags, color
        ));
    }
    out.push_str("0\nENDTAB\n0\nENDSEC\n");
}

fn write_hatch(out: &mut String, h: &aadt_model::Hatch) {
    out.push_str(&format!(
        "0\nHATCH\n8\n{}\n2\n{}\n70\n{}\n91\n1\n93\n{}\n",
        h.layer,
        h.pattern,
        i32::from(h.solid),
        h.boundary.vertices.len(),
    ));
    for v in &h.boundary.vertices {
        out.push_str(&format!("10\n{}\n20\n{}\n", v.x, v.y));
    }
}

fn write_dimension(out: &mut String, d: &Dimension) {
    let ty = match d.kind {
        DimensionKind::Linear => 0,
        DimensionKind::Aligned => 1,
    };
    out.push_str(&format!(
        "0\nDIMENSION\n8\n{}\n70\n{}\n13\n{}\n23\n{}\n14\n{}\n24\n{}\n11\n{}\n21\n{}\n1\n{}\n42\n{}\n",
        d.layer,
        ty,
        d.p1.x,
        d.p1.y,
        d.p2.x,
        d.p2.y,
        d.text_anchor.x,
        d.text_anchor.y,
        d.text,
        d.measurement,
    ));
}

/// Write one geometry as a DXF entity. `attributes` carries `INSERT` block
/// attributes; block *content* passes `None` since a definition's geometry has
/// none of its own.
fn write_geometry(
    out: &mut String,
    layer: &str,
    geometry: &Geometry,
    attributes: Option<&HashMap<String, String>>,
) {
    match geometry {
        Geometry::Line(s) => {
            out.push_str(&format!(
                "0\nLINE\n8\n{}\n10\n{}\n20\n{}\n11\n{}\n21\n{}\n",
                layer, s.a.x, s.a.y, s.b.x, s.b.y
            ));
        }
        Geometry::Polyline(p) => {
            out.push_str(&format!(
                "0\nLWPOLYLINE\n8\n{}\n90\n{}\n70\n{}\n",
                layer,
                p.vertices.len(),
                i32::from(p.closed)
            ));
            for v in &p.vertices {
                out.push_str(&format!("10\n{}\n20\n{}\n", v.x, v.y));
            }
        }
        Geometry::Circle(c) => {
            out.push_str(&format!(
                "0\nCIRCLE\n8\n{}\n10\n{}\n20\n{}\n40\n{}\n",
                layer, c.center.x, c.center.y, c.radius
            ));
        }
        Geometry::Arc(a) => {
            out.push_str(&format!(
                "0\nARC\n8\n{}\n10\n{}\n20\n{}\n40\n{}\n50\n{}\n51\n{}\n",
                layer, a.center.x, a.center.y, a.radius, a.start_angle, a.end_angle
            ));
        }
        Geometry::Text(t) => {
            out.push_str(&format!(
                "0\nTEXT\n8\n{}\n10\n{}\n20\n{}\n40\n{}\n1\n{}\n50\n{}\n",
                layer, t.position.x, t.position.y, t.height, t.value, t.rotation
            ));
        }
        Geometry::Insert(ins) => {
            // Attributes are emitted in sorted key order so export is
            // deterministic and round-trips exactly.
            let attrs = attributes.filter(|a| !a.is_empty());
            out.push_str(&format!(
                "0\nINSERT\n8\n{}\n2\n{}\n10\n{}\n20\n{}\n50\n{}\n66\n{}\n",
                layer,
                ins.name,
                ins.position.x,
                ins.position.y,
                ins.rotation,
                i32::from(attrs.is_some()),
            ));
            if let Some(attrs) = attrs {
                let mut tags: Vec<(&String, &String)> = attrs.iter().collect();
                tags.sort_by(|a, b| a.0.cmp(b.0));
                for (tag, value) in tags {
                    out.push_str(&format!("0\nATTRIB\n1\n{value}\n2\n{tag}\n"));
                }
                out.push_str("0\nSEQEND\n");
            }
        }
    }
}

/// Write a document to a DXF file on disk.
pub fn export_path(doc: &Document, path: impl AsRef<Path>) -> Result<(), DxfError> {
    std::fs::write(path, export_str(doc))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
0
SECTION
2
ENTITIES
0
LINE
8
A-WALL
10
0.0
20
0.0
11
10.0
21
0.0
0
LWPOLYLINE
8
A-ROOM
90
4
70
1
10
0.0
20
0.0
10
4.0
20
0.0
10
4.0
20
3.0
10
0.0
20
3.0
0
ENDSEC
0
EOF
";

    const LEGACY: &str = "\
0
SECTION
2
ENTITIES
0
POLYLINE
8
A-ROOM
70
1
0
VERTEX
10
0.0
20
0.0
0
VERTEX
10
2.0
20
0.0
0
VERTEX
10
2.0
20
2.0
0
SEQEND
0
ENDSEC
0
EOF
";

    #[test]
    fn imports_line_and_lwpolyline() {
        let doc = import_str(SAMPLE).unwrap();
        assert_eq!(doc.len(), 2);
        let entities = doc.entities();
        match &entities[0].geometry {
            Geometry::Line(s) => assert!((s.length() - 10.0).abs() < 1e-9),
            _ => panic!("expected line"),
        }
        match &entities[1].geometry {
            Geometry::Polyline(p) => {
                assert!(p.closed);
                assert!((p.area() - 12.0).abs() < 1e-9);
            }
            _ => panic!("expected polyline"),
        }
        assert_eq!(entities[0].layer, "A-WALL");
        assert_eq!(entities[1].layer, "A-ROOM");
    }

    #[test]
    fn imports_legacy_polyline() {
        let doc = import_str(LEGACY).unwrap();
        assert_eq!(doc.len(), 1);
        match &doc.entities()[0].geometry {
            Geometry::Polyline(p) => {
                assert!(p.closed);
                assert_eq!(p.vertices.len(), 3);
            }
            _ => panic!("expected polyline"),
        }
    }

    #[test]
    fn round_trip_preserves_entities() {
        let doc1 = import_str(SAMPLE).unwrap();
        let text = export_str(&doc1);
        let doc2 = import_str(&text).unwrap();

        let e1: Vec<_> = doc1.entities().into_iter().cloned().collect();
        let e2: Vec<_> = doc2.entities().into_iter().cloned().collect();
        assert_eq!(e1, e2, "import -> export -> import changed the document");
    }

    const BLOCKS: &str = "\
0
SECTION
2
ENTITIES
0
CIRCLE
8
S-COLS
10
1.0
20
2.0
40
0.25
0
INSERT
8
A-DOOR
2
DOOR-SINGLE
10
3.0
20
0.0
50
90.0
66
1
0
ATTRIB
1
0.9
2
width
0
SEQEND
0
ENDSEC
0
EOF
";

    #[test]
    fn imports_circle_and_insert_with_attributes() {
        let doc = import_str(BLOCKS).unwrap();
        assert_eq!(doc.len(), 2);
        let entities = doc.entities();
        match &entities[0].geometry {
            Geometry::Circle(c) => {
                assert!((c.radius - 0.25).abs() < 1e-9);
                assert_eq!(c.center, Point2::new(1.0, 2.0));
            }
            _ => panic!("expected circle"),
        }
        match &entities[1].geometry {
            Geometry::Insert(ins) => {
                assert_eq!(ins.name, "DOOR-SINGLE");
                assert_eq!(ins.position, Point2::new(3.0, 0.0));
                assert!((ins.rotation - 90.0).abs() < 1e-9);
            }
            _ => panic!("expected insert"),
        }
        assert_eq!(entities[1].attribute("width"), Some("0.9"));
    }

    #[test]
    fn round_trip_preserves_blocks() {
        let doc1 = import_str(BLOCKS).unwrap();
        let doc2 = import_str(&export_str(&doc1)).unwrap();
        let e1: Vec<_> = doc1.entities().into_iter().cloned().collect();
        let e2: Vec<_> = doc2.entities().into_iter().cloned().collect();
        assert_eq!(e1, e2, "circle/insert round-trip changed the document");
    }

    /// A door swing (ARC) and a room label (TEXT) — the two annotation entities
    /// a real floor plan carries alongside its walls.
    const ANNOTATED: &str = "\
0
SECTION
2
ENTITIES
0
ARC
8
A-DOOR
10
3.0
20
0.0
40
0.9
50
0.0
51
90.0
0
TEXT
8
A-ANNO
10
2.5
20
1.5
40
0.25
1
LIVING ROOM
50
0.0
0
ENDSEC
0
EOF
";

    #[test]
    fn imports_arc_and_text() {
        let doc = import_str(ANNOTATED).unwrap();
        assert_eq!(doc.len(), 2);
        let entities = doc.entities();
        match &entities[0].geometry {
            Geometry::Arc(a) => {
                assert_eq!(a.center, Point2::new(3.0, 0.0));
                assert!((a.radius - 0.9).abs() < 1e-9);
                assert!((a.sweep() - 90.0).abs() < 1e-9);
            }
            other => panic!("expected arc, got {other:?}"),
        }
        assert_eq!(entities[0].layer, "A-DOOR");
        match &entities[1].geometry {
            Geometry::Text(t) => {
                // The value must survive verbatim, spaces included.
                assert_eq!(t.value, "LIVING ROOM");
                assert_eq!(t.position, Point2::new(2.5, 1.5));
                assert!((t.height - 0.25).abs() < 1e-9);
            }
            other => panic!("expected text, got {other:?}"),
        }
        assert_eq!(entities[1].layer, "A-ANNO");
    }

    #[test]
    fn round_trip_preserves_arc_and_text() {
        let doc1 = import_str(ANNOTATED).unwrap();
        let doc2 = import_str(&export_str(&doc1)).unwrap();
        let e1: Vec<_> = doc1.entities().into_iter().cloned().collect();
        let e2: Vec<_> = doc2.entities().into_iter().cloned().collect();
        assert_eq!(e1, e2, "arc/text round-trip changed the document");
    }

    #[test]
    fn arcs_and_text_are_not_wall_faces() {
        // Guards the layering decision: annotation entities must not leak into
        // the segment stream that wall detection consumes.
        let doc = import_str(ANNOTATED).unwrap();
        for e in doc.entities() {
            assert!(
                e.geometry.segments().is_empty(),
                "arc/text contributed segments to detection"
            );
        }
    }

    /// A BLOCKS section defining a door symbol (a line + an arc swing), then an
    /// INSERT of it in the ENTITIES section.
    const WITH_BLOCK: &str = "\
0
SECTION
2
BLOCKS
0
BLOCK
2
DOOR-SINGLE
10
0.0
20
0.0
0
LINE
8
0
10
0.0
20
0.0
11
0.9
21
0.0
0
ARC
8
0
10
0.0
20
0.0
40
0.9
50
0.0
51
90.0
0
ENDBLK
0
ENDSEC
0
SECTION
2
ENTITIES
0
INSERT
8
A-DOOR
2
DOOR-SINGLE
10
5.0
20
3.0
50
0.0
66
0
0
ENDSEC
0
EOF
";

    #[test]
    fn imports_a_block_definition_and_its_insert() {
        let doc = import_str(WITH_BLOCK).unwrap();
        // The definition is not an entity — only the INSERT is placed.
        assert_eq!(doc.len(), 1, "one placed entity (the insert)");

        let block = doc.block("DOOR-SINGLE").expect("block defined");
        assert_eq!(block.base, Point2::new(0.0, 0.0));
        assert_eq!(block.entities.len(), 2, "line + arc swing");
        assert!(matches!(block.entities[0].geometry, Geometry::Line(_)));
        assert!(matches!(block.entities[1].geometry, Geometry::Arc(_)));

        // The insert resolves to that definition by name.
        match &doc.entities()[0].geometry {
            Geometry::Insert(ins) => {
                assert_eq!(ins.name, "DOOR-SINGLE");
                assert!(doc.block(&ins.name).is_some(), "insert resolves");
            }
            other => panic!("expected insert, got {other:?}"),
        }
    }

    #[test]
    fn round_trip_preserves_block_definitions() {
        let doc1 = import_str(WITH_BLOCK).unwrap();
        let doc2 = import_str(&export_str(&doc1)).unwrap();

        let e1: Vec<_> = doc1.entities().into_iter().cloned().collect();
        let e2: Vec<_> = doc2.entities().into_iter().cloned().collect();
        assert_eq!(e1, e2, "entities changed across block round-trip");

        let b1: Vec<_> = doc1.blocks().into_iter().cloned().collect();
        let b2: Vec<_> = doc2.blocks().into_iter().cloned().collect();
        assert_eq!(b1, b2, "block definitions changed across round-trip");
    }

    #[test]
    fn an_insert_may_reference_an_undefined_block() {
        // A DXF exported without its BLOCKS section is common; the insert must
        // still import, with resolution simply returning None.
        let doc = import_str(BLOCKS).unwrap();
        match &doc.entities()[1].geometry {
            Geometry::Insert(ins) => assert!(doc.block(&ins.name).is_none()),
            other => panic!("expected insert, got {other:?}"),
        }
    }

    /// A HATCH (ANSI31 pattern over a triangle) and a linear DIMENSION.
    const ANNOTATION: &str = "\
0
SECTION
2
ENTITIES
0
HATCH
8
A-FLOR
2
ANSI31
70
0
91
1
93
3
10
0.0
20
0.0
10
4.0
20
0.0
10
4.0
20
3.0
0
DIMENSION
8
A-DIMS
70
0
13
0.0
23
0.0
14
10.0
24
0.0
11
5.0
21
-1.0
1
10.00
42
10.0
0
ENDSEC
0
EOF
";

    #[test]
    fn imports_hatch_and_dimension() {
        let doc = import_str(ANNOTATION).unwrap();
        // Neither is a placed entity.
        assert_eq!(doc.len(), 0);

        assert_eq!(doc.hatches().len(), 1);
        let h = &doc.hatches()[0];
        assert_eq!(h.layer, "A-FLOR");
        assert_eq!(h.pattern, "ANSI31");
        assert!(!h.solid);
        assert_eq!(h.boundary.vertices.len(), 3);
        assert!(h.boundary.closed);

        assert_eq!(doc.dimensions().len(), 1);
        let d = &doc.dimensions()[0];
        assert_eq!(d.kind, DimensionKind::Linear);
        assert_eq!(d.p1, Point2::new(0.0, 0.0));
        assert_eq!(d.p2, Point2::new(10.0, 0.0));
        assert_eq!(d.text_anchor, Point2::new(5.0, -1.0));
        assert_eq!(d.text, "10.00");
        assert!((d.measurement - 10.0).abs() < 1e-9);
    }

    #[test]
    fn round_trip_preserves_hatch_and_dimension() {
        let doc1 = import_str(ANNOTATION).unwrap();
        let doc2 = import_str(&export_str(&doc1)).unwrap();
        assert_eq!(
            doc1.hatches(),
            doc2.hatches(),
            "hatch changed on round-trip"
        );
        assert_eq!(
            doc1.dimensions(),
            doc2.dimensions(),
            "dimension changed on round-trip"
        );
    }

    #[test]
    fn round_trip_preserves_layer_state() {
        let mut doc = Document::new();
        let seg = |x, y| Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(x, y)));
        doc.add_geometry("A-WALL", seg(1.0, 0.0));
        doc.add_geometry("A-GRID", seg(0.0, 1.0));
        doc.add_geometry("A-PLAIN", seg(1.0, 1.0));
        doc.set_layer_color("A-WALL", 5);
        doc.set_layer_visible("A-WALL", false);
        doc.set_layer_frozen("A-GRID", true);

        let dxf = export_str(&doc);
        assert!(dxf.contains("\nTABLES\n"), "a TABLES section is emitted");

        let back = import_str(&dxf).unwrap();
        let wall = back.layer("A-WALL").expect("A-WALL survives");
        assert_eq!(wall.color, 5);
        assert!(!wall.visible, "off layer stays off");
        assert!(back.layer("A-GRID").unwrap().frozen, "frozen stays frozen");
        // The all-default layer is shown and needs no explicit record to be so.
        assert!(back.layer_shown("A-PLAIN"));
    }

    #[test]
    fn all_default_layers_emit_no_tables_section() {
        let mut doc = Document::new();
        doc.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(1.0, 0.0))),
        );
        let dxf = export_str(&doc);
        assert!(
            !dxf.contains("\nTABLES\n"),
            "default-only layers add no TABLES section, keeping existing output byte-stable"
        );
    }

    #[test]
    fn solid_hatch_survives_and_an_undersized_boundary_is_skipped() {
        let solid = "\
0
SECTION
2
ENTITIES
0
HATCH
8
0
2
SOLID
70
1
10
0.0
20
0.0
10
2.0
20
0.0
10
1.0
20
2.0
0
ENDSEC
0
EOF
";
        let doc = import_str(solid).unwrap();
        assert_eq!(doc.hatches().len(), 1);
        assert!(doc.hatches()[0].solid);

        // A two-vertex "hatch" cannot bound a region and is dropped.
        let degenerate = "\
0
SECTION
2
ENTITIES
0
HATCH
8
0
2
SOLID
70
1
10
0.0
20
0.0
10
2.0
20
0.0
0
ENDSEC
0
EOF
";
        let doc2 = import_str(degenerate).unwrap();
        assert!(doc2.hatches().is_empty(), "under-3-vertex hatch skipped");
    }
}
