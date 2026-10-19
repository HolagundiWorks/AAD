//! On-disk AAD `.vdb` layout over the ShilpiDB codec.
//!
//! ```text
//! MAGIC ("SHILPIDB")
//! u32  FORMAT_VERSION
//! u32  record_count
//! for each record:
//!   u64 id
//!   f64 min_x, min_y, max_x, max_y
//!   u32 payload_len + bytes
//! SpatialGrid (persisted index over the same boxes)
//! ```

use std::path::Path;

use shilpidb::{Bbox, Error as CodecError, MAGIC, Point, Reader, SpatialGrid, Writer};

use crate::{MAX_PAYLOAD, MAX_RECORDS};

/// Host format version written after [`MAGIC`]. Bump when the record layout changes.
///
/// - **1** — tab-separated `handle\ttype\tlayer` payloads (legacy).
/// - **2** — binary [`crate::payload`] entity payloads (current write format).
pub const FORMAT_VERSION: u32 = 2;
/// Oldest readable host format.
pub const MIN_FORMAT_VERSION: u32 = 1;

/// One spatial record: opaque id, bbox, and host payload.
#[derive(Debug, Clone, PartialEq)]
pub struct VdbRecord {
    pub id: u64,
    pub bbox: Bbox,
    pub payload: Vec<u8>,
}

/// In-memory store with a spatial index.
#[derive(Debug, Clone)]
pub struct VdbStore {
    records: Vec<VdbRecord>,
    grid: SpatialGrid,
    /// Host format version this store was loaded from or will be written as.
    format_version: u32,
}

impl VdbStore {
    /// Empty store (current write format).
    pub fn new() -> Self {
        Self { records: Vec::new(), grid: SpatialGrid::build(Vec::new()), format_version: FORMAT_VERSION }
    }

    /// Build from records (reindexes). Rejects oversized collections or payloads.
    pub fn from_records(records: Vec<VdbRecord>) -> Result<Self, Error> {
        if records.len() > MAX_RECORDS {
            return Err(Error::TooManyRecords { count: records.len(), limit: MAX_RECORDS });
        }
        for record in &records {
            if record.payload.len() > MAX_PAYLOAD {
                return Err(Error::PayloadTooLarge { id: record.id, len: record.payload.len(), limit: MAX_PAYLOAD });
            }
            if !finite_bbox(record.bbox) {
                return Err(Error::BadBbox(record.id));
            }
        }
        let boxes: Vec<(u64, Bbox)> = records.iter().map(|r| (r.id, r.bbox)).collect();
        Ok(Self { grid: SpatialGrid::build(boxes), records, format_version: FORMAT_VERSION })
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn records(&self) -> &[VdbRecord] {
        &self.records
    }

    pub fn grid(&self) -> &SpatialGrid {
        &self.grid
    }

    /// Format version loaded from disk, or [`FORMAT_VERSION`] for a newly built store.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Ids whose box intersects `rect`.
    pub fn query_bbox(&self, rect: Bbox) -> Vec<u64> {
        self.grid.query_bbox(rect)
    }

    /// Ids within `tol` of `(x, y)`.
    pub fn query_point(&self, x: f64, y: f64, tol: f64) -> Vec<u64> {
        self.grid.query_point(Point::new(x, y), tol)
    }
}

impl Default for VdbStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Encode the store to bytes.
pub fn save_bytes(store: &VdbStore) -> Result<Vec<u8>, Error> {
    let count = u32::try_from(store.records.len()).map_err(|_| Error::TooManyRecords { count: store.records.len(), limit: MAX_RECORDS })?;
    let mut w = Writer::new();
    w.raw(MAGIC);
    w.u32(FORMAT_VERSION);
    w.u32(count);
    for record in &store.records {
        write_record(&mut w, record)?;
    }
    store.grid.write(&mut w);
    Ok(w.into_bytes())
}

/// Decode a store from bytes.
pub fn load_bytes(bytes: &[u8]) -> Result<VdbStore, Error> {
    let mut r = Reader::new(bytes);
    let magic = r.raw(MAGIC.len()).map_err(Error::Codec)?;
    if magic != MAGIC {
        return Err(Error::NotVdb);
    }
    let version = r.u32().map_err(Error::Codec)?;
    if version < MIN_FORMAT_VERSION || version > FORMAT_VERSION {
        return Err(Error::UnsupportedVersion(version));
    }
    let count = r.u32().map_err(Error::Codec)? as usize;
    if count > MAX_RECORDS {
        return Err(Error::TooManyRecords { count, limit: MAX_RECORDS });
    }
    let mut records = Vec::with_capacity(count);
    for _ in 0..count {
        records.push(read_record(&mut r)?);
    }
    // Prefer the persisted grid when present; rebuild if the trailer is missing
    // or truncated (still usable for queries from the record list).
    let grid = match SpatialGrid::read(&mut r) {
        Ok(grid) if grid.len() == records.len() => grid,
        _ => SpatialGrid::build(records.iter().map(|rec| (rec.id, rec.bbox)).collect()),
    };
    Ok(VdbStore { records, grid, format_version: version })
}

/// Write a `.vdb` file.
pub fn save_path(path: &Path, store: &VdbStore) -> Result<(), Error> {
    let bytes = save_bytes(store)?;
    std::fs::write(path, bytes).map_err(Error::Io)
}

/// Read a `.vdb` file.
pub fn load_path(path: &Path) -> Result<VdbStore, Error> {
    let bytes = std::fs::read(path).map_err(Error::Io)?;
    load_bytes(&bytes)
}

fn write_record(w: &mut Writer, record: &VdbRecord) -> Result<(), Error> {
    if record.payload.len() > MAX_PAYLOAD {
        return Err(Error::PayloadTooLarge { id: record.id, len: record.payload.len(), limit: MAX_PAYLOAD });
    }
    let len =
        u32::try_from(record.payload.len()).map_err(|_| Error::PayloadTooLarge { id: record.id, len: record.payload.len(), limit: MAX_PAYLOAD })?;
    w.u64(record.id);
    w.f64(record.bbox.min.x);
    w.f64(record.bbox.min.y);
    w.f64(record.bbox.max.x);
    w.f64(record.bbox.max.y);
    w.u32(len);
    w.raw(&record.payload);
    Ok(())
}

fn read_record(r: &mut Reader<'_>) -> Result<VdbRecord, Error> {
    let id = r.u64().map_err(Error::Codec)?;
    let min_x = r.f64().map_err(Error::Codec)?;
    let min_y = r.f64().map_err(Error::Codec)?;
    let max_x = r.f64().map_err(Error::Codec)?;
    let max_y = r.f64().map_err(Error::Codec)?;
    let len = r.u32().map_err(Error::Codec)? as usize;
    if len > MAX_PAYLOAD {
        return Err(Error::PayloadTooLarge { id, len, limit: MAX_PAYLOAD });
    }
    let payload = r.raw(len).map_err(Error::Codec)?.to_vec();
    // Check raw floats before `from_corners`: f64::min/max drops NaN and would
    // silently accept hostile non-finite boxes.
    if ![min_x, min_y, max_x, max_y].iter().all(|v| v.is_finite()) {
        return Err(Error::BadBbox(id));
    }
    let bbox = Bbox::from_corners(min_x, min_y, max_x, max_y);
    Ok(VdbRecord { id, bbox, payload })
}

fn finite_bbox(b: Bbox) -> bool {
    b.min.x.is_finite() && b.min.y.is_finite() && b.max.x.is_finite() && b.max.y.is_finite()
}

/// Errors from reading or writing an AAD `.vdb`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not a ShilpiDB / AAD .vdb file")]
    NotVdb,
    #[error("unsupported .vdb format version {0}")]
    UnsupportedVersion(u32),
    #[error("too many records ({count}); limit is {limit}")]
    TooManyRecords { count: usize, limit: usize },
    #[error("record {id}: payload {len} bytes exceeds limit {limit}")]
    PayloadTooLarge { id: u64, len: usize, limit: usize },
    #[error("record {0}: non-finite bbox")]
    BadBbox(u64),
    #[error(transparent)]
    Codec(#[from] CodecError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(id: u64, x0: f64, y0: f64, x1: f64, y1: f64, payload: &[u8]) -> VdbRecord {
        VdbRecord { id, bbox: Bbox::from_corners(x0, y0, x1, y1), payload: payload.to_vec() }
    }

    #[test]
    fn round_trip_bytes_and_query() {
        let store = VdbStore::from_records(vec![rec(1, 0.0, 0.0, 1.0, 1.0, b"wall"), rec(2, 5.0, 5.0, 6.0, 6.0, b"door")]).expect("build");
        let bytes = save_bytes(&store).expect("save");
        assert!(bytes.starts_with(MAGIC));
        let back = load_bytes(&bytes).expect("load");
        assert_eq!(back.len(), 2);
        assert_eq!(back.format_version(), FORMAT_VERSION);
        assert_eq!(back.query_bbox(Bbox::from_corners(0.2, 0.2, 0.8, 0.8)), vec![1]);
        assert_eq!(back.records()[1].payload, b"door");
    }

    #[test]
    fn path_round_trip() {
        let dir = tempfile::tempdir().expect("temp");
        let path = dir.path().join("sample.vdb");
        let store = VdbStore::from_records(vec![rec(9, -1.0, -1.0, 1.0, 1.0, b"x")]).expect("build");
        save_path(&path, &store).expect("save");
        let back = load_path(&path).expect("load");
        assert_eq!(back.records()[0].id, 9);
    }

    #[test]
    fn rejects_bad_magic() {
        assert!(matches!(load_bytes(b"NOTVDB!!"), Err(Error::NotVdb)));
        assert!(load_bytes(b"short").is_err());
        // Same length as MAGIC but wrong bytes.
        assert!(matches!(load_bytes(b"XXXXXXXX"), Err(Error::NotVdb)));
    }

    #[test]
    fn rejects_huge_record_count() {
        let mut w = Writer::new();
        w.raw(MAGIC);
        w.u32(FORMAT_VERSION);
        w.u32(u32::try_from(MAX_RECORDS + 1).expect("count fits u32"));
        let bytes = w.into_bytes();
        assert!(matches!(load_bytes(&bytes), Err(Error::TooManyRecords { .. })));
    }

    #[test]
    fn rejects_nan_bbox_on_build_and_load() {
        // Build a NaN box without `Bbox::from_corners` (that API drops NaNs via f64::min/max).
        let nan_rec = VdbRecord { id: 1, bbox: Bbox { min: Point::new(f64::NAN, 0.0), max: Point::new(1.0, 1.0) }, payload: b"x".to_vec() };
        assert!(matches!(VdbStore::from_records(vec![nan_rec]), Err(Error::BadBbox(1))));
        assert!(matches!(VdbStore::from_records(vec![rec(2, 0.0, 0.0, f64::INFINITY, 1.0, b"x")]), Err(Error::BadBbox(2))));

        let mut w = Writer::new();
        w.raw(MAGIC);
        w.u32(FORMAT_VERSION);
        w.u32(1);
        w.u64(7);
        w.f64(f64::NAN);
        w.f64(0.0);
        w.f64(1.0);
        w.f64(1.0);
        w.u32(1);
        w.raw(b"x");
        let bytes = w.into_bytes();
        assert!(matches!(load_bytes(&bytes), Err(Error::BadBbox(7))));
    }

    #[test]
    fn rejects_oversized_payload_len() {
        let mut w = Writer::new();
        w.raw(MAGIC);
        w.u32(FORMAT_VERSION);
        w.u32(1);
        w.u64(1);
        w.f64(0.0);
        w.f64(0.0);
        w.f64(1.0);
        w.f64(1.0);
        w.u32(u32::try_from(MAX_PAYLOAD + 1).expect("len fits u32"));
        let bytes = w.into_bytes();
        assert!(matches!(load_bytes(&bytes), Err(Error::PayloadTooLarge { .. })));
    }

    #[test]
    fn empty_store_round_trips() {
        let bytes = save_bytes(&VdbStore::new()).expect("save");
        let back = load_bytes(&bytes).expect("load");
        assert!(back.is_empty());
    }
}
