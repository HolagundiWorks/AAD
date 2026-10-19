//! AAD's `.vdb` store on top of [ShilpiDB](https://github.com/HolagundiWorks/shilpidb).
//!
//! - **Embedded:** [`file`] composes `MAGIC` + format version + records + a [`SpatialGrid`].
//! - **Server:** [`VdbClient`] wraps `shilpi-client` for a running `shilpid`.
//!
//! Records are opaque `u64` ids with a bbox and payload bytes. Mapping to AAD entities
//! (handles, layers, geometry) is done by the host (`aad-io` / engine commands).
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

mod client;
mod file;
mod payload;

pub use client::VdbClient;
pub use file::{FORMAT_VERSION, MIN_FORMAT_VERSION, VdbRecord, VdbStore, load_bytes, load_path, save_bytes, save_path};
pub use payload::{
    EntityGeom, EntityPayload, KindTag, MAX_BLOCK_CHARS, MAX_LAYER_CHARS, MAX_POLY_VERTICES, MAX_TEXT_CHARS, MAX_TYPE_CHARS, PAYLOAD_VERSION,
    decode as decode_payload, encode as encode_payload,
};
pub use shilpi_client::{Error as ClientError, Record as ClientRecord, Stats as ClientStats};
pub use shilpidb::{Bbox, Error as CodecError, MAGIC, Point, Reader, SpatialGrid, Writer};

/// Cap on records read from a hostile or oversized `.vdb` (allocations and loops).
pub const MAX_RECORDS: usize = 1_000_000;
/// Cap on a single payload (bytes).
pub const MAX_PAYLOAD: usize = 16 * 1024 * 1024;
