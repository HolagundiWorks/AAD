//! Thin AAD-facing client over `shilpi-client` (a running `shilpid`).

use std::net::ToSocketAddrs;
use std::time::Duration;

use shilpi_client::{Client, Error as ClientError, Record, Stats};
use shilpidb::Bbox;

use crate::file::{VdbRecord, VdbStore};

/// Connection to a `shilpid` server for shared / multi-client `.vdb` work.
pub struct VdbClient {
    inner: Client,
}

impl VdbClient {
    /// Connect to `addr` (for example `"127.0.0.1:7420"`).
    pub fn connect(addr: impl ToSocketAddrs) -> Result<Self, ClientError> {
        Ok(Self {
            inner: Client::connect(addr)?,
        })
    }

    /// Connect with an explicit per-operation timeout (`None` = block forever).
    pub fn connect_with_timeout(
        addr: impl ToSocketAddrs,
        timeout: Option<Duration>,
    ) -> Result<Self, ClientError> {
        Ok(Self {
            inner: Client::connect_with_timeout(addr, timeout)?,
        })
    }

    pub fn ping(&mut self) -> Result<(), ClientError> {
        self.inner.ping()
    }

    pub fn insert(&mut self, id: u64, bbox: Bbox, payload: Vec<u8>) -> Result<(), ClientError> {
        self.inner.insert(id, bbox, payload)
    }

    pub fn insert_many(&mut self, records: Vec<(u64, Bbox, Vec<u8>)>) -> Result<(), ClientError> {
        self.inner.insert_many(records)
    }

    pub fn get(&mut self, id: u64) -> Result<Option<Record>, ClientError> {
        self.inner.get(id)
    }

    pub fn delete(&mut self, id: u64) -> Result<bool, ClientError> {
        self.inner.delete(id)
    }

    pub fn query_bbox(&mut self, rect: Bbox) -> Result<Vec<u64>, ClientError> {
        self.inner.query_bbox(rect)
    }

    pub fn query_point(&mut self, x: f64, y: f64, tol: f64) -> Result<Vec<u64>, ClientError> {
        self.inner.query_point(x, y, tol)
    }

    pub fn list(&mut self) -> Result<Vec<u64>, ClientError> {
        self.inner.list()
    }

    pub fn list_boxes(&mut self) -> Result<Vec<(u64, Bbox)>, ClientError> {
        self.inner.list_boxes()
    }

    pub fn stats(&mut self) -> Result<Stats, ClientError> {
        self.inner.stats()
    }

    pub fn save(&mut self) -> Result<(), ClientError> {
        self.inner.save()
    }

    /// Push every record from an embedded [`VdbStore`] in one round-trip.
    ///
    /// When `replace` is true, delete every id currently on the server that is not in `store`
    /// (after upsert), so the server mirrors model space instead of keeping orphans.
    pub fn push_store(&mut self, store: &VdbStore, replace: bool) -> Result<(), ClientError> {
        let records: Vec<(u64, Bbox, Vec<u8>)> = store
            .records()
            .iter()
            .map(|r| (r.id, r.bbox, r.payload.clone()))
            .collect();
        self.inner.insert_many(records)?;
        if replace {
            let keep: std::collections::HashSet<u64> = store.records().iter().map(|r| r.id).collect();
            for id in self.inner.list()? {
                if !keep.contains(&id) {
                    let _ = self.inner.delete(id)?;
                }
            }
        }
        Ok(())
    }

    /// Pull all ids and boxes, then fetch payloads into a [`VdbStore`].
    pub fn pull_store(&mut self) -> Result<VdbStore, Error> {
        let boxes = self.inner.list_boxes().map_err(Error::Client)?;
        let mut records = Vec::with_capacity(boxes.len());
        for (id, bbox) in boxes {
            let payload = match self.inner.get(id).map_err(Error::Client)? {
                Some(Record { payload, .. }) => payload,
                None => Vec::new(),
            };
            records.push(VdbRecord { id, bbox, payload });
        }
        VdbStore::from_records(records).map_err(Error::Store)
    }
}

/// Client helpers that combine transport and store errors.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Client(#[from] ClientError),
    #[error(transparent)]
    Store(#[from] crate::file::Error),
}
