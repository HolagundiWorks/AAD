//! Talk to a running `shilpid` server as an AAD host.
//!
//! Where [`crate::ShilpiDb`] embeds the engine to read/write a local `.vdb`
//! file, [`VdbClient`] maps AAD's model onto a **remote** ShilpiDB over TCP —
//! one record per entity:
//!
//! | AAD                        | ShilpiDB record        |
//! |----------------------------|------------------------|
//! | [`EntityId`]               | `u64` id               |
//! | [`entity_bbox`] extent     | `Bbox`                 |
//! | [`encode_entity`] payload  | opaque value bytes     |
//!
//! The payload uses the exact same encoding as an entity inside a `.vdb` file,
//! so the file format and the wire format stay one format.
//!
//! ```no_run
//! use aadt_vdb::VdbClient;
//! use aadt_geometry::{Bbox, Point2};
//!
//! let mut db = VdbClient::connect("127.0.0.1:7420")?;
//! let doc = aadt_model::Document::new();
//! db.push_document(&doc)?;
//! let hits = db.query_bbox(Bbox::from_corners(Point2::new(0.0, 0.0), Point2::new(10.0, 10.0)))?;
//! # Ok::<(), aadt_vdb::VdbClientError>(())
//! ```

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};

use aadt_geometry::{Bbox, Point2};
use aadt_model::{Document, Entity, EntityId};
use shilpi_client::{Client, Error as ClientError};

use crate::codec::{decode_entity, encode_entity};
use crate::{entity_bbox, to_engine_bbox, VdbError};

/// A failure talking to a ShilpiDB server as an AAD host.
#[derive(Debug)]
pub enum VdbClientError {
    /// Transport / protocol / server-side failure from the client library.
    Transport(ClientError),
    /// A record's payload could not be decoded into an [`Entity`].
    Decode(VdbError),
}

impl std::fmt::Display for VdbClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VdbClientError::Transport(e) => write!(f, "shilpid transport: {e}"),
            VdbClientError::Decode(e) => write!(f, "decoding record payload: {e}"),
        }
    }
}

impl std::error::Error for VdbClientError {}

impl From<ClientError> for VdbClientError {
    fn from(e: ClientError) -> Self {
        VdbClientError::Transport(e)
    }
}

/// A connection to a `shilpid` server, speaking AAD's entity model.
///
/// Generic over the stream so it drives a real [`TcpStream`] (via
/// [`VdbClient::connect`]) or an in-memory stream in tests (via
/// [`VdbClient::from_client`]).
pub struct VdbClient<S = TcpStream> {
    inner: Client<S>,
}

impl VdbClient<TcpStream> {
    /// Connect to `shilpid` at `addr` (e.g. `"127.0.0.1:7420"`).
    pub fn connect(addr: impl ToSocketAddrs) -> Result<Self, VdbClientError> {
        Ok(VdbClient {
            inner: Client::connect(addr)?,
        })
    }
}

impl<S: Read + Write> VdbClient<S> {
    /// Wrap an already-connected [`shilpi_client::Client`].
    pub fn from_client(inner: Client<S>) -> Self {
        VdbClient { inner }
    }

    /// Consume the wrapper and return the underlying client.
    pub fn into_inner(self) -> Client<S> {
        self.inner
    }

    /// Liveness check.
    pub fn ping(&mut self) -> Result<(), VdbClientError> {
        Ok(self.inner.ping()?)
    }

    /// Upsert one entity as a record (`id`, bounding box, encoded payload).
    ///
    /// Returns `Ok(false)` for an entity with no computable extent (e.g. empty
    /// geometry) — nothing is stored, since a record needs a box to index.
    pub fn put_entity(&mut self, doc: &Document, entity: &Entity) -> Result<bool, VdbClientError> {
        let Some(bb) = entity_bbox(doc, entity) else {
            return Ok(false);
        };
        self.inner
            .insert(entity.id.0, to_engine_bbox(bb), encode_entity(entity))?;
        Ok(true)
    }

    /// Push every entity of `doc`. Returns how many records were stored (some
    /// entities may be skipped; see [`put_entity`](Self::put_entity)).
    pub fn push_document(&mut self, doc: &Document) -> Result<usize, VdbClientError> {
        let mut stored = 0;
        for entity in doc.entities() {
            if self.put_entity(doc, entity)? {
                stored += 1;
            }
        }
        Ok(stored)
    }

    /// Fetch and decode the entity at `id`, or `None` if it isn't stored.
    pub fn get_entity(&mut self, id: EntityId) -> Result<Option<Entity>, VdbClientError> {
        match self.inner.get(id.0)? {
            Some(record) => Ok(Some(
                decode_entity(&record.payload).map_err(VdbClientError::Decode)?,
            )),
            None => Ok(None),
        }
    }

    /// Delete the record at `id`; returns whether it existed.
    pub fn delete_entity(&mut self, id: EntityId) -> Result<bool, VdbClientError> {
        Ok(self.inner.delete(id.0)?)
    }

    /// Ids of entities whose bounding box intersects `rect`.
    pub fn query_bbox(&mut self, rect: Bbox) -> Result<Vec<EntityId>, VdbClientError> {
        Ok(self
            .inner
            .query_bbox(to_engine_bbox(rect))?
            .into_iter()
            .map(EntityId)
            .collect())
    }

    /// Ids of entities within `tol` of point `p`.
    pub fn query_point(&mut self, p: Point2, tol: f64) -> Result<Vec<EntityId>, VdbClientError> {
        Ok(self
            .inner
            .query_point(p.x, p.y, tol)?
            .into_iter()
            .map(EntityId)
            .collect())
    }

    /// Every entity id currently stored on the server.
    pub fn list(&mut self) -> Result<Vec<EntityId>, VdbClientError> {
        Ok(self.inner.list()?.into_iter().map(EntityId).collect())
    }

    /// Pull and decode every stored entity.
    ///
    /// The record store holds entities only — floors, spaces, layers, and
    /// blocks aren't part of the record model — so this returns bare
    /// [`Entity`] values. Use [`restore_into`](Self::restore_into) to fold them
    /// back into a document.
    pub fn pull_entities(&mut self) -> Result<Vec<Entity>, VdbClientError> {
        let ids = self.inner.list()?;
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(record) = self.inner.get(id)? {
                out.push(decode_entity(&record.payload).map_err(VdbClientError::Decode)?);
            }
        }
        Ok(out)
    }

    /// Pull every stored entity and restore it into `doc`. Returns the count.
    pub fn restore_into(&mut self, doc: &mut Document) -> Result<usize, VdbClientError> {
        let entities = self.pull_entities()?;
        let n = entities.len();
        for entity in entities {
            doc.restore_entity(entity);
        }
        Ok(n)
    }

    /// Ask the server to flush its in-memory database to disk.
    pub fn save(&mut self) -> Result<(), VdbClientError> {
        Ok(self.inner.save()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aadt_geometry::Segment;
    use aadt_model::Geometry;
    use shilpi_protocol::{recv_request, write_message, Request, Response};
    use std::io::{self, Cursor};

    /// A fake duplex: reads served from a preloaded buffer of canned response
    /// frames; writes (the requests we send) captured for inspection.
    struct Fake {
        read: Cursor<Vec<u8>>,
        wrote: Vec<u8>,
    }

    impl Read for Fake {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.read.read(buf)
        }
    }

    impl Write for Fake {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.wrote.extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn client_with(responses: &[Response]) -> VdbClient<Fake> {
        let mut buf = Vec::new();
        for r in responses {
            write_message(&mut buf, &r.encode()).unwrap();
        }
        VdbClient::from_client(Client::from_stream(Fake {
            read: Cursor::new(buf),
            wrote: Vec::new(),
        }))
    }

    fn line_doc() -> (Document, EntityId) {
        let mut doc = Document::new();
        let id = doc.add_geometry(
            "0",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(4.0, 2.0))),
        );
        (doc, id)
    }

    #[test]
    fn put_entity_sends_insert_with_mapped_id_bbox_payload() {
        let (doc, id) = line_doc();
        let entity = doc.get(id).unwrap();
        let mut client = client_with(&[Response::Ok]);
        assert!(client.put_entity(&doc, entity).unwrap());

        // Decode the request we actually sent.
        let mut cur = Cursor::new(client.into_inner().into_inner().wrote);
        let req = recv_request(&mut cur).unwrap().unwrap();
        match req {
            Request::Insert {
                id: rid,
                bbox,
                payload,
            } => {
                assert_eq!(rid, id.0);
                // Box spans the segment endpoints.
                assert_eq!(
                    bbox,
                    to_engine_bbox(Bbox::from_corners(
                        Point2::new(0.0, 0.0),
                        Point2::new(4.0, 2.0),
                    ))
                );
                // Payload decodes back to the same entity.
                assert_eq!(decode_entity(&payload).unwrap(), *entity);
            }
            other => panic!("expected Insert, got {other:?}"),
        }
    }

    #[test]
    fn get_entity_decodes_the_payload() {
        let (doc, id) = line_doc();
        let entity = doc.get(id).unwrap().clone();
        let mut client = client_with(&[Response::Found {
            bbox: to_engine_bbox(Bbox::from_corners(
                Point2::new(0.0, 0.0),
                Point2::new(4.0, 2.0),
            )),
            payload: encode_entity(&entity),
        }]);
        assert_eq!(client.get_entity(id).unwrap(), Some(entity));
    }

    #[test]
    fn missing_entity_is_none() {
        let mut client = client_with(&[Response::NotFound]);
        assert_eq!(client.get_entity(EntityId(7)).unwrap(), None);
    }

    #[test]
    fn query_bbox_maps_ids_to_entity_ids() {
        let mut client = client_with(&[Response::Ids(vec![1, 2, 3])]);
        let hits = client
            .query_bbox(Bbox::from_corners(
                Point2::new(0.0, 0.0),
                Point2::new(9.0, 9.0),
            ))
            .unwrap();
        assert_eq!(hits, vec![EntityId(1), EntityId(2), EntityId(3)]);
    }

    #[test]
    fn server_error_surfaces_as_transport() {
        let (doc, id) = line_doc();
        let entity = doc.get(id).unwrap();
        let mut client = client_with(&[Response::Err("server is read-only".into())]);
        match client.put_entity(&doc, entity) {
            Err(VdbClientError::Transport(ClientError::Server(msg))) => {
                assert_eq!(msg, "server is read-only")
            }
            other => panic!("expected Transport(Server), got {other:?}"),
        }
    }
}
