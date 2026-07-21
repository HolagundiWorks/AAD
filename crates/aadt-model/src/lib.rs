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

use aadt_geometry::{Arc, Circle, Point2, Polyline, Segment};

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

/// A single-line text entity (DXF `TEXT`): room labels, annotation, tag values
/// drawn in the drawing rather than carried as a block attribute.
#[derive(Debug, Clone, PartialEq)]
pub struct Text {
    pub value: String,
    /// Insertion point (DXF group 10/20).
    pub position: Point2,
    /// Cap height in drawing units (DXF group 40).
    pub height: f64,
    /// Rotation in degrees (DXF convention).
    pub rotation: f64,
}

impl Text {
    pub fn new(value: impl Into<String>, position: Point2, height: f64, rotation: f64) -> Self {
        Self {
            value: value.into(),
            position,
            height,
            rotation,
        }
    }
}

/// One piece of geometry inside a [`Block`] definition.
///
/// Deliberately not an [`Entity`]: entity ids are document-scoped and identify
/// something placed in the drawing, whereas block content is a template that may
/// be instantiated many times (or never). Giving template geometry document ids
/// would imply a placement that does not exist.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockEntity {
    pub layer: String,
    pub geometry: Geometry,
}

/// A named block *definition*: the geometry an [`Insert`] refers to.
///
/// Coordinates are relative to `base`, the block's insertion point.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub name: String,
    pub base: Point2,
    pub entities: Vec<BlockEntity>,
}

impl Block {
    pub fn new(name: impl Into<String>, base: Point2) -> Self {
        Self {
            name: name.into(),
            base,
            entities: Vec::new(),
        }
    }

    pub fn push(&mut self, layer: impl Into<String>, geometry: Geometry) {
        self.entities.push(BlockEntity {
            layer: layer.into(),
            geometry,
        });
    }
}

/// A hatched or filled region (DXF `HATCH`): a closed boundary plus a fill
/// pattern.
///
/// Held in its own [`Document`] table rather than as a [`Geometry`] variant.
/// A hatch is annotation — it contributes nothing to wall pairing or takeoff
/// geometry — and keeping it out of the `Geometry` enum spares every consumer a
/// match arm it would only ever ignore. Modeled for **round-trip fidelity**:
/// the boundary and pattern name survive an import → export cycle; full pattern
/// line geometry and multi-path boundaries are deferred.
#[derive(Debug, Clone, PartialEq)]
pub struct Hatch {
    pub layer: String,
    /// The (closed) boundary the fill occupies.
    pub boundary: Polyline,
    /// Pattern name, e.g. `SOLID`, `ANSI31`. Preserved verbatim.
    pub pattern: String,
    /// Whether the fill is solid (DXF group 70).
    pub solid: bool,
}

impl Hatch {
    pub fn new(layer: impl Into<String>, boundary: Polyline, pattern: impl Into<String>) -> Self {
        let pattern = pattern.into();
        let solid = pattern.eq_ignore_ascii_case("SOLID");
        Self {
            layer: layer.into(),
            boundary,
            pattern,
            solid,
        }
    }
}

/// The measured form of a [`Dimension`]. Linear (axis-aligned / rotated) and
/// aligned (parallel to the measured points) are the two AADT models today;
/// radial, angular, and ordinate dimensions are deferred.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DimensionKind {
    Linear,
    Aligned,
}

/// A linear or aligned dimension (DXF `DIMENSION`).
///
/// Like [`Hatch`], held in its own table for round-trip fidelity rather than as
/// a drawable primitive: the measured points, the text placement, any text
/// override, and the stored measurement survive a round-trip. The arrows,
/// extension lines, and text glyphs a CAD app would draw are the renderer's job,
/// not the model's.
#[derive(Debug, Clone, PartialEq)]
pub struct Dimension {
    pub layer: String,
    pub kind: DimensionKind,
    /// First measured point (DXF 13/23).
    pub p1: Point2,
    /// Second measured point (DXF 14/24).
    pub p2: Point2,
    /// Text midpoint (DXF 11/21).
    pub text_anchor: Point2,
    /// Text override (DXF group 1); empty means "use the measured value".
    pub text: String,
    /// The stored measurement (DXF group 42).
    pub measurement: f64,
}

/// The geometric payload of an entity. A subset of spec §4.1's entity set, growing
/// through M1/M2.
#[derive(Debug, Clone, PartialEq)]
pub enum Geometry {
    Line(Segment),
    Polyline(Polyline),
    Circle(Circle),
    Arc(Arc),
    Text(Text),
    Insert(Insert),
}

impl Geometry {
    /// The straight segments this geometry contributes.
    ///
    /// Lines and polylines contribute their own edges. **Arcs contribute
    /// nothing**: chord approximation is a rendering concern with a
    /// caller-chosen tolerance, and silently returning chords here would let
    /// them reach wall detection, where a curved wall would be paired as a run
    /// of short straight faces. Consumers that want chords call
    /// [`Arc::to_polyline`] explicitly. Circles, text, and inserts are likewise
    /// empty.
    pub fn segments(&self) -> Vec<Segment> {
        match self {
            Geometry::Line(s) => vec![*s],
            Geometry::Polyline(p) => p.segments(),
            Geometry::Circle(_) | Geometry::Arc(_) | Geometry::Text(_) | Geometry::Insert(_) => {
                Vec::new()
            }
        }
    }

    /// This geometry shifted by `(dx, dy)`. A rigid translation: sizes, radii,
    /// and arc angles are unchanged, only positions move.
    pub fn translated(&self, dx: f64, dy: f64) -> Geometry {
        let shift = |p: Point2| Point2::new(p.x + dx, p.y + dy);
        match self {
            Geometry::Line(s) => Geometry::Line(Segment::new(shift(s.a), shift(s.b))),
            Geometry::Polyline(p) => Geometry::Polyline(Polyline::new(
                p.vertices.iter().map(|&v| shift(v)).collect(),
                p.closed,
            )),
            Geometry::Circle(c) => Geometry::Circle(Circle::new(shift(c.center), c.radius)),
            Geometry::Arc(a) => Geometry::Arc(Arc::new(
                shift(a.center),
                a.radius,
                a.start_angle,
                a.end_angle,
            )),
            Geometry::Text(t) => Geometry::Text(Text::new(
                t.value.clone(),
                shift(t.position),
                t.height,
                t.rotation,
            )),
            Geometry::Insert(i) => {
                Geometry::Insert(Insert::new(i.name.clone(), shift(i.position), i.rotation))
            }
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
    /// An entity's geometry was replaced in place, its id preserved (undo
    /// restores `before`, redo reapplies `after`).
    ReplaceGeometry {
        id: EntityId,
        before: Geometry,
        after: Geometry,
    },
    /// An entity attribute was set or cleared (undo restores `before`, redo
    /// reapplies `after`); `None` means the key was absent.
    SetAttribute {
        id: EntityId,
        key: String,
        before: Option<String>,
        after: Option<String>,
    },
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
    /// Block definitions by name. Held apart from `entities` because a
    /// definition is a template, not something placed in the drawing.
    blocks: HashMap<String, Block>,
    /// Hatched regions. Annotation, held apart from `entities` (see [`Hatch`]).
    hatches: Vec<Hatch>,
    /// Dimensions. Annotation, held apart from `entities` (see [`Dimension`]).
    dimensions: Vec<Dimension>,
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
            Op::ReplaceGeometry { id, after, .. } => {
                if let Some(e) = self.entities.get_mut(id) {
                    e.geometry = after.clone();
                }
            }
            Op::SetAttribute { id, key, after, .. } => {
                Self::write_attribute(&mut self.entities, id, key, after.clone());
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
            Op::ReplaceGeometry { id, before, .. } => {
                if let Some(e) = self.entities.get_mut(id) {
                    e.geometry = before.clone();
                }
            }
            Op::SetAttribute {
                id, key, before, ..
            } => {
                Self::write_attribute(&mut self.entities, id, key, before.clone());
            }
        }
    }

    /// Set or clear an entity attribute in the store (`None` clears it).
    fn write_attribute(
        entities: &mut HashMap<EntityId, Entity>,
        id: &EntityId,
        key: &str,
        value: Option<String>,
    ) {
        if let Some(e) = entities.get_mut(id) {
            match value {
                Some(v) => {
                    e.attributes.insert(key.to_string(), v);
                }
                None => {
                    e.attributes.remove(key);
                }
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

    /// Replace an entity's geometry in place, keeping its id, layer, and
    /// attributes. Returns `false` if no entity has that id.
    ///
    /// Recorded as a single [`Op::ReplaceGeometry`] so undo/redo is exact — the
    /// id-preserving alternative to remove-then-add, which would mint a new id
    /// and break any reference held to the old one.
    pub fn replace_geometry(&mut self, id: EntityId, geometry: Geometry) -> bool {
        let Some(entity) = self.entities.get_mut(&id) else {
            return false;
        };
        let before = entity.geometry.clone();
        entity.geometry = geometry.clone();
        self.record(Op::ReplaceGeometry {
            id,
            before,
            after: geometry,
        });
        self.revision += 1;
        true
    }

    /// Translate an entity by `(dx, dy)`, keeping its id. Returns `false` if no
    /// entity has that id. A convenience over [`replace_geometry`](Self::replace_geometry)
    /// with a rigid [`Geometry::translated`].
    pub fn translate_entity(&mut self, id: EntityId, dx: f64, dy: f64) -> bool {
        let Some(entity) = self.entities.get(&id) else {
            return false;
        };
        let moved = entity.geometry.translated(dx, dy);
        self.replace_geometry(id, moved)
    }

    /// Set an entity attribute, keeping its id and geometry. Returns `false` if
    /// no entity has that id, or if the value already equals `value` (a no-op is
    /// not recorded, so re-applying the same metadata leaves no undo entry).
    ///
    /// Recorded as a single [`Op::SetAttribute`] so undo restores exactly the
    /// prior value — the mechanism AI-derived metadata (wall types, flags) is
    /// applied through, non-destructively and reversibly (spec §6.1).
    pub fn set_attribute(&mut self, id: EntityId, key: &str, value: impl Into<String>) -> bool {
        let value = value.into();
        let Some(entity) = self.entities.get_mut(&id) else {
            return false;
        };
        let before = entity.attributes.get(key).cloned();
        if before.as_deref() == Some(value.as_str()) {
            return false; // unchanged
        }
        entity.attributes.insert(key.to_string(), value.clone());
        self.record(Op::SetAttribute {
            id,
            key: key.to_string(),
            before,
            after: Some(value),
        });
        self.revision += 1;
        true
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

    // --- Block definitions ---------------------------------------------------

    /// Define a block, replacing any existing definition of the same name.
    ///
    /// **Untracked**, like a file load: block definitions arrive with the
    /// drawing rather than as a user edit, so they do not flood the undo
    /// history. An interactive `BLOCK` command would wrap this in a transaction
    /// of its own once M1 grows one.
    pub fn add_block(&mut self, block: Block) {
        self.blocks.insert(block.name.clone(), block);
        self.revision += 1;
    }

    /// The definition an [`Insert`] of `name` refers to, if the drawing carries
    /// one. Inserts may legitimately reference an undefined block — a DXF can be
    /// exported without its `BLOCKS` section — so callers must handle `None`
    /// rather than assume resolution succeeds.
    pub fn block(&self, name: &str) -> Option<&Block> {
        self.blocks.get(name)
    }

    /// All block definitions in ascending name order (deterministic).
    pub fn blocks(&self) -> Vec<&Block> {
        let mut v: Vec<&Block> = self.blocks.values().collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }

    // --- Annotation (hatches, dimensions) ------------------------------------

    /// Add a hatch. **Untracked**, like block definitions and file loads:
    /// annotation arrives with the drawing rather than as a tracked user edit.
    pub fn add_hatch(&mut self, hatch: Hatch) {
        self.hatches.push(hatch);
        self.revision += 1;
    }

    /// Hatches in insertion order (deterministic).
    pub fn hatches(&self) -> &[Hatch] {
        &self.hatches
    }

    /// Add a dimension. **Untracked**, for the same reason as [`add_hatch`](Self::add_hatch).
    pub fn add_dimension(&mut self, dimension: Dimension) {
        self.dimensions.push(dimension);
        self.revision += 1;
    }

    /// Dimensions in insertion order (deterministic).
    pub fn dimensions(&self) -> &[Dimension] {
        &self.dimensions
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
                // Cardinal extremes, not just the centre: a bbox built from
                // centres alone clips every circle by its radius, which would
                // cut a zoom-to-extents short.
                Geometry::Circle(c) => {
                    pts.push(Point2::new(c.center.x - c.radius, c.center.y));
                    pts.push(Point2::new(c.center.x + c.radius, c.center.y));
                    pts.push(Point2::new(c.center.x, c.center.y - c.radius));
                    pts.push(Point2::new(c.center.x, c.center.y + c.radius));
                }
                // Sampled along the sweep, so a bulging arc is not understated
                // by its endpoints alone.
                Geometry::Arc(a) => pts.extend(a.to_polyline(16).vertices),
                // The insertion point only: text extent needs font metrics AADT
                // does not model.
                Geometry::Text(t) => pts.push(t.position),
                Geometry::Insert(ins) => pts.push(ins.position),
            }
        }
        // Annotation lives outside the entity store but still bounds the drawing.
        for h in &self.hatches {
            pts.extend(h.boundary.vertices.iter().copied());
        }
        for d in &self.dimensions {
            pts.push(d.p1);
            pts.push(d.p2);
            pts.push(d.text_anchor);
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
    fn translate_entity_moves_geometry_and_keeps_the_id() {
        let mut doc = Document::new();
        let id = doc.add_geometry("A-WALL", line(0.0, 0.0, 10.0, 0.0));

        let moved = doc.transact("move", |d| d.translate_entity(id, 3.0, 4.0));
        assert!(moved);

        let e = doc.get(id).expect("id is preserved, not re-minted");
        match &e.geometry {
            Geometry::Line(s) => {
                assert_eq!(s.a, Point2::new(3.0, 4.0));
                assert_eq!(s.b, Point2::new(13.0, 4.0));
            }
            other => panic!("expected line, got {other:?}"),
        }
    }

    #[test]
    fn translate_is_one_undoable_step_that_restores_exactly() {
        let mut doc = Document::new();
        let id = doc.add_geometry("A-WALL", line(1.0, 1.0, 2.0, 2.0));
        let original = doc.get(id).unwrap().geometry.clone();

        doc.transact("move", |d| d.translate_entity(id, 5.0, -5.0));
        assert_ne!(doc.get(id).unwrap().geometry, original);

        doc.undo();
        assert_eq!(
            doc.get(id).unwrap().geometry,
            original,
            "undo restores the pre-move geometry under the same id"
        );

        doc.redo();
        assert_ne!(doc.get(id).unwrap().geometry, original, "redo re-applies");
    }

    #[test]
    fn replace_geometry_preserves_layer_and_attributes() {
        let mut doc = Document::new();
        let mut attrs = HashMap::new();
        attrs.insert("width".to_string(), "0.9".to_string());
        let id = doc.add_entity("A-DOOR", line(0.0, 0.0, 1.0, 0.0), attrs);

        let ok = doc.transact("edit", |d| {
            d.replace_geometry(
                id,
                Geometry::Circle(Circle::new(Point2::new(5.0, 5.0), 2.0)),
            )
        });
        assert!(ok);

        let e = doc.get(id).unwrap();
        assert!(matches!(e.geometry, Geometry::Circle(_)));
        assert_eq!(e.layer, "A-DOOR", "layer is untouched");
        assert_eq!(e.attribute("width"), Some("0.9"), "attributes survive");
    }

    #[test]
    fn set_attribute_is_recorded_and_exactly_undone() {
        let mut doc = Document::new();
        let id = doc.add_geometry("A-WALL", line(0.0, 0.0, 10.0, 0.0));

        let set = doc.transact("classify", |d| {
            d.set_attribute(id, "wall-type", "structural")
        });
        assert!(set);
        assert_eq!(
            doc.get(id).unwrap().attribute("wall-type"),
            Some("structural")
        );

        doc.undo();
        assert_eq!(
            doc.get(id).unwrap().attribute("wall-type"),
            None,
            "undo clears an attribute that was absent before"
        );
        doc.redo();
        assert_eq!(
            doc.get(id).unwrap().attribute("wall-type"),
            Some("structural")
        );
    }

    #[test]
    fn set_attribute_undo_restores_a_prior_value() {
        let mut doc = Document::new();
        let mut attrs = HashMap::new();
        attrs.insert("wall-type".to_string(), "partition".to_string());
        let id = doc.add_entity("A-WALL", line(0.0, 0.0, 1.0, 0.0), attrs);

        doc.transact("reclassify", |d| {
            d.set_attribute(id, "wall-type", "structural")
        });
        assert_eq!(
            doc.get(id).unwrap().attribute("wall-type"),
            Some("structural")
        );
        doc.undo();
        assert_eq!(
            doc.get(id).unwrap().attribute("wall-type"),
            Some("partition"),
            "undo restores the prior value, not absence"
        );
    }

    #[test]
    fn setting_the_same_attribute_value_is_a_no_op() {
        let mut doc = Document::new();
        let id = doc.add_geometry("A-WALL", line(0.0, 0.0, 1.0, 0.0));
        doc.transact("a", |d| d.set_attribute(id, "k", "v"));
        let r = doc.revision();
        let changed = doc.transact("b", |d| d.set_attribute(id, "k", "v"));
        assert!(!changed, "re-setting the same value changes nothing");
        assert_eq!(doc.revision(), r, "and does not bump the revision");
    }

    #[test]
    fn translate_reports_missing_ids_without_recording() {
        let mut doc = Document::new();
        let r0 = doc.revision();
        let moved = doc.transact("move", |d| d.translate_entity(EntityId(999), 1.0, 1.0));
        assert!(!moved, "no such entity");
        assert_eq!(doc.revision(), r0, "a no-op must not bump the revision");
        assert!(!doc.can_undo(), "and must leave no undo entry");
    }

    #[test]
    fn block_definitions_live_apart_from_entities() {
        let mut doc = Document::new();
        doc.add_geometry("A-WALL", line(0.0, 0.0, 1.0, 0.0));

        let mut block = Block::new("DOOR", Point2::new(0.0, 0.0));
        block.push("0", line(0.0, 0.0, 0.9, 0.0));
        doc.add_block(block);

        // A definition is not an entity: the drawing still holds one wall.
        assert_eq!(doc.len(), 1);
        assert!(doc.block("DOOR").is_some());
        assert!(doc.block("MISSING").is_none());
        assert_eq!(doc.blocks().len(), 1);
    }

    #[test]
    fn redefining_a_block_replaces_it_and_lists_sorted() {
        let mut doc = Document::new();
        doc.add_block(Block::new("B", Point2::new(0.0, 0.0)));
        doc.add_block(Block::new("A", Point2::new(0.0, 0.0)));
        // Redefine B with different base.
        doc.add_block(Block::new("B", Point2::new(5.0, 5.0)));

        let names: Vec<&str> = doc.blocks().iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, vec!["A", "B"], "listed by name, deduplicated");
        assert_eq!(doc.block("B").unwrap().base, Point2::new(5.0, 5.0));
    }

    #[test]
    fn hatches_and_dimensions_live_apart_from_entities() {
        let mut doc = Document::new();
        doc.add_geometry("A-WALL", line(0.0, 0.0, 10.0, 0.0));

        let boundary = Polyline::new(
            vec![
                Point2::new(0.0, 0.0),
                Point2::new(4.0, 0.0),
                Point2::new(4.0, 3.0),
                Point2::new(0.0, 3.0),
            ],
            true,
        );
        doc.add_hatch(Hatch::new("A-FLOR", boundary, "ANSI31"));
        doc.add_dimension(Dimension {
            layer: "A-DIMS".into(),
            kind: DimensionKind::Linear,
            p1: Point2::new(0.0, 0.0),
            p2: Point2::new(10.0, 0.0),
            text_anchor: Point2::new(5.0, -1.0),
            text: String::new(),
            measurement: 10.0,
        });

        // Annotation is not an entity: the drawing still holds just the wall.
        assert_eq!(doc.len(), 1);
        assert_eq!(doc.hatches().len(), 1);
        assert_eq!(doc.dimensions().len(), 1);
        assert!(!doc.hatches()[0].solid, "ANSI31 is a pattern, not solid");

        // But annotation still bounds the drawing (points feed zoom-to-extents).
        let pts = doc.points();
        assert!(pts.iter().any(|p| p.x == 4.0 && p.y == 3.0), "hatch corner");
        assert!(
            pts.iter().any(|p| (p.y + 1.0).abs() < 1e-9),
            "dim text anchor"
        );
    }

    #[test]
    fn hatch_solid_flag_follows_the_pattern_name() {
        let sq = Polyline::new(
            vec![
                Point2::new(0.0, 0.0),
                Point2::new(1.0, 0.0),
                Point2::new(1.0, 1.0),
            ],
            true,
        );
        assert!(Hatch::new("0", sq.clone(), "SOLID").solid);
        assert!(
            Hatch::new("0", sq.clone(), "solid").solid,
            "case-insensitive"
        );
        assert!(!Hatch::new("0", sq, "ANSI31").solid);
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
