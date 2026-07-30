//! Little-endian binary read/write primitives for the `.vdb` format.
//!
//! Deliberately hand-rolled (no `serde`, no external crate) — this is a *custom*
//! database format we fully control. Strings are length-prefixed UTF-8; integers
//! and floats are fixed-width little-endian.

/// Errors from `.vdb` encoding / decoding / I/O.
#[derive(Debug)]
pub enum VdbError {
    /// Underlying filesystem error.
    Io(std::io::Error),
    /// The bytes are not a valid `.vdb` payload.
    Format(String),
}

impl std::fmt::Display for VdbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VdbError::Io(e) => write!(f, "aadt vdb i/o: {e}"),
            VdbError::Format(m) => write!(f, "aadt vdb: {m}"),
        }
    }
}

impl std::error::Error for VdbError {}

impl From<std::io::Error> for VdbError {
    fn from(e: std::io::Error) -> Self {
        VdbError::Io(e)
    }
}

/// Append-only little-endian byte writer.
#[derive(Default)]
pub struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    pub fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    pub fn bool(&mut self, v: bool) {
        self.buf.push(v as u8);
    }

    pub fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn i64(&mut self, v: i64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn f64(&mut self, v: f64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn point(&mut self, p: aadt_geometry::Point2) {
        self.f64(p.x);
        self.f64(p.y);
    }

    /// `u32` length prefix + UTF-8 bytes.
    pub fn str(&mut self, s: &str) {
        self.u32(s.len() as u32);
        self.buf.extend_from_slice(s.as_bytes());
    }

    /// Raw magic bytes (no length prefix).
    pub fn raw(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }
}

/// Cursor over a `.vdb` byte slice.
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], VdbError> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|e| *e <= self.data.len())
            .ok_or_else(|| VdbError::Format("unexpected end of vdb data".into()))?;
        let s = &self.data[self.pos..end];
        self.pos = end;
        Ok(s)
    }

    pub fn u8(&mut self) -> Result<u8, VdbError> {
        Ok(self.take(1)?[0])
    }

    pub fn bool(&mut self) -> Result<bool, VdbError> {
        Ok(self.u8()? != 0)
    }

    pub fn u32(&mut self) -> Result<u32, VdbError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn u64(&mut self) -> Result<u64, VdbError> {
        let b = self.take(8)?;
        Ok(u64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    pub fn i64(&mut self) -> Result<i64, VdbError> {
        Ok(self.u64()? as i64)
    }

    pub fn f64(&mut self) -> Result<f64, VdbError> {
        let b = self.take(8)?;
        Ok(f64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    pub fn point(&mut self) -> Result<aadt_geometry::Point2, VdbError> {
        Ok(aadt_geometry::Point2::new(self.f64()?, self.f64()?))
    }

    pub fn str(&mut self) -> Result<String, VdbError> {
        let n = self.u32()? as usize;
        let b = self.take(n)?;
        String::from_utf8(b.to_vec()).map_err(|_| VdbError::Format("invalid utf-8 string".into()))
    }

    pub fn raw(&mut self, n: usize) -> Result<&'a [u8], VdbError> {
        self.take(n)
    }
}
