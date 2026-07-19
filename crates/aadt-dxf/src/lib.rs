//! # aadt-dxf — DXF interoperability (file I/O layer, spec §3 / §7)
//!
//! DWG/DXF round-trip is non-negotiable for architects exchanging files (spec §7).
//! M0 ships a small, dependency-free reader for the ASCII DXF entity types the
//! takeoff vertical slice needs — `LINE` and `LWPOLYLINE` — parsing group-code
//! pairs directly into an [`aadt_model::Document`].
//!
//! Scope deliberately bounded for M0 (see `docs/ROADMAP.md` / ADR-0002):
//! - The **full DXF entity set** (legacy `POLYLINE`/`VERTEX`, arcs, circles, text,
//!   hatches, blocks/inserts, dimensions) and **DXF export** land in **M1**.
//! - **DWG** (binary, reverse-engineered) is a later milestone via the ODA SDK or a
//!   LibreDWG FFI binding.
//!
//! The public entry points ([`import_str`], [`import_path`]) are stable so a
//! fuller backend (e.g. the `dxf` crate) can replace the internal parser without
//! touching callers.

use std::collections::HashMap;
use std::fmt;
use std::path::Path;

use aadt_geometry::{Point2, Polyline, Segment};
use aadt_model::{Document, Geometry};

/// Error raised while reading a DXF file.
#[derive(Debug)]
pub enum DxfError {
    /// The file could not be read from disk.
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
    let mut in_entities = false;

    for rec in &records {
        match rec.kind.as_str() {
            "SECTION" => in_entities = rec.string(2).map(|s| s == "ENTITIES").unwrap_or(false),
            "ENDSEC" => in_entities = false,
            "LINE" if in_entities => import_line(&mut doc, rec),
            "LWPOLYLINE" if in_entities => import_lwpolyline(&mut doc, rec),
            _ => {}
        }
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

fn import_line(doc: &mut Document, rec: &Record) {
    let (Some(x1), Some(y1), Some(x2), Some(y2)) =
        (rec.float(10), rec.float(20), rec.float(11), rec.float(21))
    else {
        return; // Skip malformed lines rather than aborting the whole import.
    };
    let seg = Segment::new(Point2::new(x1, y1), Point2::new(x2, y2));
    doc.add_geometry(rec.layer(), Geometry::Line(seg));
}

fn import_lwpolyline(doc: &mut Document, rec: &Record) {
    // Vertices arrive as interleaved 10 (x) / 20 (y) fields, possibly with 42
    // bulge values between them (ignored in M0). Pair each x with the next y.
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
    if vertices.len() < 2 {
        return;
    }
    let closed = rec.int(70).map(|f| f & 1 == 1).unwrap_or(false);
    let poly = Polyline::new(vertices, closed);
    let attributes: HashMap<String, String> = HashMap::new();
    doc.add_entity(rec.layer(), Geometry::Polyline(poly), attributes);
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
}
