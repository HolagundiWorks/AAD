//! # aadt-model — Document model (substrate, spec §4.1)
//!
//! The drawing database the CAD core produces and the AADT Engine consumes: a store
//! of entities (with layer and attribute data) keyed by a stable [`EntityId`], plus
//! an **atomic undo/redo transaction stack** (spec §4.1).
//!
//! Every user-driven change — manual edits and AADT-driven insertions alike — is
//! grouped into a single undoable [`Document::transact`] unit, indistinguishable in
//! mechanics from a manual CAD command. Bulk machine operations that are not user
//! actions (loading a DXF file, running detection) mutate the store **untracked**,
//! so opening a file does not flood the undo history.
//!
//! The full entity set (arcs, circles, text/mtext, hatches, blocks/inserts,
//! dimensions, tables) continues to grow through M1 (see `docs/ROADMAP.md`).

use std::collections::HashMap;

use aadt_geometry::{Circle, Point2, Polyline, Segment};

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

/// A block reference (DXF `INSERT`): a named block placed at a point. The block's
/// own attributes (door width, window type, tag values) live on the owning
/// [`Entity`]'s attribute map.
#[derive(Debug, Clone, PartialEq)]
pub struct Insert {
    pub name: String,
    pub position: Point2,
    /// Rotation in degrees (DXF convention).
    pub rotation: f64,
}

impl Insert {
    pub fn new(name: impl Into<String>, position: Point2, rotation: f64) -> Self {
        Self {
            name: name.into(),
            position,
            rotation,
        }
    }
}

/// The geometric payload of an entity. A subset of spec §4.1's entity set, growing
/// through M1/M2.
#[derive(Debug, Clone, PartialEq)]
pub enum Geometry {
    Line(Segment),
    Polyline(Polyline),
    Circle(Circle),
    Insert(Insert),
}

impl Geometry {
    /// The straight segments this geometry contributes (lines and polylines); empty
    /// for circles and block inserts.
    pub fn segments(&self) -> Vec<Segment> {
        match self {
            Geometry::Line(s) => vec![*s],
            Geometry::Polyline(p) => p.segments(),
            Geometry::Circle(_) | Geometry::Insert(_) => Vec::new(),
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

/// A reversible primitive change. Each variant stores enough to both apply and
/// invert itself, so undo and redo are exact.
#[derive(Debug, Clone)]
enum Op {
    /// An entity was inserted (undo removes it, redo reinserts it).
    AddEntity(Entity),
    /// An entity was removed (undo reinserts it, redo removes it).
    RemoveEntity(Entity),
    /// A new layer was created (undo removes it, redo recreates it).
    AddLayer(String),
}

/// A group of primitive changes committed and undone as one atomic unit.
#[derive(Debug, Clone)]
struct Transaction {
    label: String,
    ops: Vec<Op>,
}

/// The drawing database. Deterministic iteration order (by id) so downstream
/// detection and takeoff are reproducible regardless of insertion order — a hard
/// requirement from spec §8.
#[derive(Debug, Default, Clone)]
pub struct Document {
    entities: HashMap<EntityId, Entity>,
    layers: HashMap<String, Layer>,
    next_id: u64,
    /// The transaction currently being recorded, if any (set only inside `transact`).
    open: Option<Transaction>,
    undo_stack: Vec<Transaction>,
    redo_stack: Vec<Transaction>,
    /// Monotonic change token bumped on every actual mutation. Consumers (e.g. the
    /// live-linked takeoff table) recompute only when this changes.
    revision: u64,
}

impl Document {
    pub fn new() -> Self {
        Self::default()
    }

    /// Monotonic revision token, bumped on every mutation. A consumer that caches
    /// derived data (detection, takeoff) can compare this to know when to recompute.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    // --- Transactions (spec §4.1) --------------------------------------------

    /// Run `f` as a single atomic, undoable transaction labelled `label`. Mutations
    /// made through `&mut Document` inside `f` are recorded; an empty transaction is
    /// discarded (no undo entry). Committing a transaction clears the redo stack.
    ///
    /// Transactions do not nest; calling `transact` inside `transact` panics in
    /// debug builds.
    pub fn transact<R>(
        &mut self,
        label: impl Into<String>,
        f: impl FnOnce(&mut Document) -> R,
    ) -> R {
        debug_assert!(self.open.is_none(), "nested transactions are not supported");
        self.open = Some(Transaction {
            label: label.into(),
            ops: Vec::new(),
        });
        let result = f(self);
        let tx = self.open.take().expect("transaction is open");
        if !tx.ops.is_empty() {
            self.undo_stack.push(tx);
            self.redo_stack.clear();
        }
        result
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Label of the transaction that [`undo`](Self::undo) would revert, if any.
    pub fn undo_label(&self) -> Option<&str> {
        self.undo_stack.last().map(|t| t.label.as_str())
    }

    /// Label of the transaction that [`redo`](Self::redo) would replay, if any.
    pub fn redo_label(&self) -> Option<&str> {
        self.redo_stack.last().map(|t| t.label.as_str())
    }

    /// Revert the most recent transaction. Returns its label, or `None` if there is
    /// nothing to undo.
    pub fn undo(&mut self) -> Option<String> {
        let tx = self.undo_stack.pop()?;
        for op in tx.ops.iter().rev() {
            self.invert(op);
        }
        self.revision += 1;
        let label = tx.label.clone();
        self.redo_stack.push(tx);
        Some(label)
    }

    /// Replay the most recently undone transaction. Returns its label, or `None` if
    /// there is nothing to redo.
    pub fn redo(&mut self) -> Option<String> {
        let tx = self.redo_stack.pop()?;
        for op in &tx.ops {
            self.apply(op);
        }
        self.revision += 1;
        let label = tx.label.clone();
        self.undo_stack.push(tx);
        Some(label)
    }

    fn apply(&mut self, op: &Op) {
        match op {
            Op::AddEntity(e) => {
                self.entities.insert(e.id, e.clone());
            }
            Op::RemoveEntity(e) => {
                self.entities.remove(&e.id);
            }
            Op::AddLayer(name) => {
                self.layers.insert(name.clone(), Layer::new(name.clone()));
            }
        }
    }

    fn invert(&mut self, op: &Op) {
        match op {
            Op::AddEntity(e) => {
                self.entities.remove(&e.id);
            }
            Op::RemoveEntity(e) => {
                self.entities.insert(e.id, e.clone());
            }
            Op::AddLayer(name) => {
                self.layers.remove(name);
            }
        }
    }

    /// Record an op onto the open transaction, if one is being recorded.
    fn record(&mut self, op: Op) {
        if let Some(tx) = self.open.as_mut() {
            tx.ops.push(op);
        }
    }

    // --- Mutation ------------------------------------------------------------

    /// Register a layer (idempotent on name). Recorded on the open transaction only
    /// when it creates a genuinely new layer.
    pub fn add_layer(&mut self, layer: Layer) {
        if !self.layers.contains_key(&layer.name) {
            let name = layer.name.clone();
            self.layers.insert(name.clone(), layer);
            self.record(Op::AddLayer(name));
            self.revision += 1;
        }
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
        let entity = Entity {
            id,
            layer,
            geometry,
            attributes,
        };
        self.entities.insert(id, entity.clone());
        self.record(Op::AddEntity(entity));
        self.revision += 1;
        id
    }

    /// Remove an entity by id, returning it if present. Recorded for undo.
    pub fn remove_entity(&mut self, id: EntityId) -> Option<Entity> {
        let entity = self.entities.remove(&id)?;
        self.record(Op::RemoveEntity(entity.clone()));
        self.revision += 1;
        Some(entity)
    }

    // --- Query ---------------------------------------------------------------

    pub fn get(&self, id: EntityId) -> Option<&Entity> {
        self.entities.get(&id)
    }

    pub fn layer(&self, name: &str) -> Option<&Layer> {
        self.layers.get(name)
    }

    pub fn layers(&self) -> impl Iterator<Item = &Layer> {
        self.layers.values()
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
                Geometry::Circle(c) => pts.push(c.center),
                Geometry::Insert(ins) => pts.push(ins.position),
            }
        }
        pts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aadt_geometry::Point2;

    fn line(x1: f64, y1: f64, x2: f64, y2: f64) -> Geometry {
        Geometry::Line(Segment::new(Point2::new(x1, y1), Point2::new(x2, y2)))
    }

    #[test]
    fn insert_and_iterate_deterministically() {
        let mut doc = Document::new();
        let a = doc.add_geometry("A-WALL", line(0.0, 0.0, 1.0, 0.0));
        let b = doc.add_geometry("A-WALL", line(0.0, 1.0, 1.0, 1.0));
        assert_eq!(doc.len(), 2);
        assert_eq!(doc.layer("A-WALL").map(|l| l.name.as_str()), Some("A-WALL"));
        let ids: Vec<EntityId> = doc.entities().iter().map(|e| e.id).collect();
        assert_eq!(ids, vec![a, b]);
    }

    #[test]
    fn untracked_edits_leave_no_undo_history() {
        let mut doc = Document::new();
        doc.add_geometry("A-WALL", line(0.0, 0.0, 1.0, 0.0));
        assert!(!doc.can_undo());
        assert_eq!(doc.undo(), None);
    }

    #[test]
    fn revision_bumps_on_mutation_and_undo() {
        let mut doc = Document::new();
        let r0 = doc.revision();
        doc.add_geometry("A-WALL", line(0.0, 0.0, 1.0, 0.0));
        let r1 = doc.revision();
        assert!(r1 > r0, "adding an entity must bump the revision");
        // A pure query does not change the revision.
        let _ = doc.entities();
        assert_eq!(doc.revision(), r1);
        doc.transact("erase", |d| {
            d.remove_entity(EntityId(0));
        });
        let r2 = doc.revision();
        assert!(r2 > r1);
        doc.undo();
        assert!(
            doc.revision() > r2,
            "undo is a change and must bump the revision"
        );
    }

    #[test]
    fn transaction_undo_and_redo_roundtrip() {
        let mut doc = Document::new();
        doc.transact("Draw two walls", |d| {
            d.add_geometry("A-WALL", line(0.0, 0.0, 4.0, 0.0));
            d.add_geometry("A-WALL", line(4.0, 0.0, 4.0, 3.0));
        });
        assert_eq!(doc.len(), 2);
        assert_eq!(doc.undo_label(), Some("Draw two walls"));

        assert_eq!(doc.undo().as_deref(), Some("Draw two walls"));
        assert_eq!(doc.len(), 0);
        // The layer created by the transaction is gone too.
        assert!(doc.layer("A-WALL").is_none());

        assert_eq!(doc.redo().as_deref(), Some("Draw two walls"));
        assert_eq!(doc.len(), 2);
        assert!(doc.layer("A-WALL").is_some());
    }

    #[test]
    fn new_transaction_clears_redo() {
        let mut doc = Document::new();
        doc.transact("first", |d| {
            d.add_geometry("L", line(0.0, 0.0, 1.0, 0.0));
        });
        doc.undo();
        assert!(doc.can_redo());
        doc.transact("second", |d| {
            d.add_geometry("L", line(0.0, 0.0, 2.0, 0.0));
        });
        assert!(!doc.can_redo());
        assert_eq!(doc.len(), 1);
    }

    #[test]
    fn remove_is_undoable() {
        let mut doc = Document::new();
        let id = doc.add_geometry("L", line(0.0, 0.0, 1.0, 0.0));
        doc.transact("erase", |d| {
            d.remove_entity(id);
        });
        assert_eq!(doc.len(), 0);
        doc.undo();
        assert_eq!(doc.len(), 1);
        assert!(doc.get(id).is_some());
    }

    #[test]
    fn empty_transaction_is_discarded() {
        let mut doc = Document::new();
        doc.transact("noop", |_d| {});
        assert!(!doc.can_undo());
    }
}
