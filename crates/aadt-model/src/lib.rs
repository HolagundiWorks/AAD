//! # aadt-model — Document model (substrate, spec §4.1)
//!
//! The drawing database the CAD core produces and the AADT Engine consumes: a store
//! of entities (with layer and attribute data) keyed by a stable [`EntityId`].
//!
//! M0 models the entity types the takeoff vertical slice needs — lines and
//! polylines on named layers. The full entity set (arcs, circles, text/mtext,
//! hatches, blocks/inserts, dimensions, tables) and the **atomic undo/redo
//! transaction stack** (spec §4.1) land in **M1 — CAD Core Substrate** (see
//! `docs/ROADMAP.md`). The store's API is intentionally id-based so that undoable
//! transactions can be layered on without changing callers.

use std::collections::HashMap;

use aadt_geometry::{Point2, Polyline, Segment};

/// Stable identifier for an entity within a [`Document`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntityId(pub u64);

/// A drawing layer. Layer names carry the convention hints AADT detection uses
/// (e.g. `A-WALL`), so they are preserved verbatim from the source file.
#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    pub name: String,
}

impl Layer {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

/// The geometric payload of an entity. A small M0 subset of spec §4.1's entity set.
#[derive(Debug, Clone, PartialEq)]
pub enum Geometry {
    Line(Segment),
    Polyline(Polyline),
}

impl Geometry {
    /// The segments this geometry contributes (one for a line, many for a polyline).
    pub fn segments(&self) -> Vec<Segment> {
        match self {
            Geometry::Line(s) => vec![*s],
            Geometry::Polyline(p) => p.segments(),
        }
    }
}

/// A single drawing entity: geometry plus the layer it lives on and free-form
/// attribute key/value pairs (block attributes, wall-height overrides, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct Entity {
    pub id: EntityId,
    pub layer: String,
    pub geometry: Geometry,
    pub attributes: HashMap<String, String>,
}

impl Entity {
    /// Convenience accessor for an attribute value.
    pub fn attribute(&self, key: &str) -> Option<&str> {
        self.attributes.get(key).map(String::as_str)
    }
}

/// The drawing database. Deterministic iteration order (by id) so downstream
/// detection and takeoff are reproducible regardless of insertion order — a hard
/// requirement from spec §8.
#[derive(Debug, Default, Clone)]
pub struct Document {
    entities: HashMap<EntityId, Entity>,
    layers: HashMap<String, Layer>,
    next_id: u64,
}

impl Document {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a layer (idempotent on name).
    pub fn add_layer(&mut self, layer: Layer) {
        self.layers.entry(layer.name.clone()).or_insert(layer);
    }

    pub fn layer(&self, name: &str) -> Option<&Layer> {
        self.layers.get(name)
    }

    pub fn layers(&self) -> impl Iterator<Item = &Layer> {
        self.layers.values()
    }

    /// Insert geometry on a layer, auto-registering the layer if unseen, and
    /// return the new entity's id. Attributes default to empty.
    pub fn add_geometry(&mut self, layer: impl Into<String>, geometry: Geometry) -> EntityId {
        self.add_entity(layer, geometry, HashMap::new())
    }

    /// Insert geometry on a layer with explicit attributes.
    pub fn add_entity(
        &mut self,
        layer: impl Into<String>,
        geometry: Geometry,
        attributes: HashMap<String, String>,
    ) -> EntityId {
        let layer = layer.into();
        self.add_layer(Layer::new(layer.clone()));
        let id = EntityId(self.next_id);
        self.next_id += 1;
        self.entities.insert(
            id,
            Entity {
                id,
                layer,
                geometry,
                attributes,
            },
        );
        id
    }

    pub fn get(&self, id: EntityId) -> Option<&Entity> {
        self.entities.get(&id)
    }

    pub fn len(&self) -> usize {
        self.entities.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    /// Entities in ascending id order (deterministic).
    pub fn entities(&self) -> Vec<&Entity> {
        let mut v: Vec<&Entity> = self.entities.values().collect();
        v.sort_by_key(|e| e.id);
        v
    }

    /// Every endpoint/vertex in the document, for a quick overall bounding box.
    pub fn points(&self) -> Vec<Point2> {
        let mut pts = Vec::new();
        for e in self.entities() {
            match &e.geometry {
                Geometry::Line(s) => {
                    pts.push(s.a);
                    pts.push(s.b);
                }
                Geometry::Polyline(p) => pts.extend(p.vertices.iter().copied()),
            }
        }
        pts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aadt_geometry::Point2;

    #[test]
    fn insert_and_iterate_deterministically() {
        let mut doc = Document::new();
        let a = doc.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(1.0, 0.0))),
        );
        let b = doc.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(Point2::new(0.0, 1.0), Point2::new(1.0, 1.0))),
        );
        assert_eq!(doc.len(), 2);
        assert_eq!(doc.layer("A-WALL").map(|l| l.name.as_str()), Some("A-WALL"));
        let ids: Vec<EntityId> = doc.entities().iter().map(|e| e.id).collect();
        assert_eq!(ids, vec![a, b]);
    }
}
