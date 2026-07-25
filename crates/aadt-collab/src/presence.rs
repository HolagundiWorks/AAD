//! Ephemeral presence / cursor state (not part of the markup op log).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use aadt_geometry::Point2;

/// One peer's last-known cursor position.
#[derive(Debug, Clone, PartialEq)]
pub struct Presence {
    pub site: String,
    pub at: Point2,
    pub updated: Instant,
}

/// Live presence map keyed by site id.
#[derive(Debug, Default, Clone)]
pub struct PresenceMap {
    peers: HashMap<String, Presence>,
}

impl PresenceMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn upsert(&mut self, site: impl Into<String>, at: Point2) {
        let site = site.into();
        self.peers.insert(
            site.clone(),
            Presence {
                site,
                at,
                updated: Instant::now(),
            },
        );
    }

    pub fn remove(&mut self, site: &str) {
        self.peers.remove(site);
    }

    /// Snapshot of all peers (unordered).
    pub fn peers(&self) -> Vec<Presence> {
        self.peers.values().cloned().collect()
    }

    pub fn get(&self, site: &str) -> Option<&Presence> {
        self.peers.get(site)
    }

    /// Drop peers not updated within `max_age`.
    pub fn prune_older_than(&mut self, max_age: Duration) {
        let now = Instant::now();
        self.peers
            .retain(|_, p| now.duration_since(p.updated) <= max_age);
    }
}
