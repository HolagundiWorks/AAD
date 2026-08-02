//! Codec shim over the standalone [`shilpidb`] engine.
//!
//! The little-endian binary primitives now live in `shilpidb`; this module
//! re-exports them under the names AAD already uses (`VdbError`, `Reader`,
//! `Writer`) and adds a geometry-aware `point` for AAD's `Point2` as an
//! extension trait — the engine codec stays scalar-only / domain-agnostic.

use aadt_geometry::Point2;

pub use shilpidb::{Error as VdbError, Reader, Writer};

/// Write a `Point2` as two `f64`s.
pub trait PointWrite {
    fn point(&mut self, p: Point2);
}
impl PointWrite for Writer {
    fn point(&mut self, p: Point2) {
        self.f64(p.x);
        self.f64(p.y);
    }
}

/// Read a `Point2` from two `f64`s.
pub trait PointRead {
    fn point(&mut self) -> Result<Point2, VdbError>;
}
impl PointRead for Reader<'_> {
    fn point(&mut self) -> Result<Point2, VdbError> {
        Ok(Point2::new(self.f64()?, self.f64()?))
    }
}
