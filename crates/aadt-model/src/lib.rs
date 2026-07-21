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

use aadt_geometry::{Arc, Circle, Point2, Polyline, Segment, Vec2};

/// Stable identifier for an entity within a [`Document`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntityId(pub u64);

/// The default AutoCAD Color Index (ACI) — 7, white/black — a fresh layer takes.
pub const DEFAULT_LAYER_COLOR: u8 = 7;

/// A drawing layer. Layer names carry the convention hints AADT detection uses
/// (e.g. `A-WALL`), so they are preserved verbatim from the source file. A layer
/// also carries display state — an [ACI](https://en.wikipedia.org/wiki/AutoCAD_DXF)
/// `color`, whether it is `visible` (DXF encodes "off" as a negative colour), and
/// whether it is `frozen` (DXF flag 70 bit 1) — so `LAYER … OFF/FREEZE/COLOR`
/// round-trips and the renderer can hide a whole layer at once.
#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    pub name: String,
    /// AutoCAD Color Index (1–255); [`DEFAULT_LAYER_COLOR`] by default.
    pub color: u8,
    /// A layer that is off is not drawn.
    pub visible: bool,
    /// A frozen layer is neither drawn nor regenerated.
    pub frozen: bool,
}

impl Layer {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            color: DEFAULT_LAYER_COLOR,
            visible: true,
            frozen: false,
        }
    }

    /// Builder: set the ACI colour.
    pub fn with_color(mut self, color: u8) -> Self {
        self.color = color;
        self
    }

    /// Whether entities on this layer should be drawn: on and not frozen.
    pub fn shown(&self) -> bool {
        self.visible && !self.frozen
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

/// The measured form of a [`Dimension`]: linear (axis-aligned / rotated), aligned
/// (parallel to the measured points), angular (the angle at a vertex), radius and
/// diameter (a circle/arc size), and ordinate (a single coordinate from a datum).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DimensionKind {
    Linear,
    Aligned,
    /// The angle between two rays from [`Dimension::vertex`] to `p1` and `p2`.
    Angular,
    /// The radius from [`Dimension::vertex`] (the centre) to `p1` (on the circle).
    Radius,
    /// The diameter across a circle, `p1`→`p2` through [`Dimension::vertex`].
    Diameter,
    /// The X coordinate of the feature `p1` relative to the datum [`Dimension::vertex`],
    /// with `p2` the leader endpoint.
    OrdinateX,
    /// The Y coordinate of the feature `p1` relative to the datum [`Dimension::vertex`].
    OrdinateY,
}

/// A dimension annotation (DXF `DIMENSION`).
///
/// Like [`Hatch`], held in its own table for round-trip fidelity rather than as
/// a drawable primitive: the definition points, the text placement, any text
/// override, and the stored measurement survive a round-trip. The arrows,
/// extension lines, and text glyphs a CAD app would draw are the renderer's job,
/// not the model's. Fields are reused per [`kind`](DimensionKind): linear/aligned
/// measure `p1`→`p2`; angular measures the angle at `vertex` between `p1` and
/// `p2`; radius measures `vertex`→`p1`.
#[derive(Debug, Clone, PartialEq)]
pub struct Dimension {
    pub layer: String,
    pub kind: DimensionKind,
    /// First definition point (DXF 13/23).
    pub p1: Point2,
    /// Second definition point (DXF 14/24); for a radius, equal to `p1`.
    pub p2: Point2,
    /// Centre (radius) or angle apex (angular); DXF 15/25. For linear/aligned it
    /// carries no meaning and conventionally equals `p1`.
    pub vertex: Point2,
    /// Text midpoint (DXF 11/21).
    pub text_anchor: Point2,
    /// Text override (DXF group 1); empty means "use the measured value".
    pub text: String,
    /// The stored measurement (DXF group 42): a length (linear/aligned/radius) or
    /// an angle in degrees (angular).
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

    /// This geometry rotated `radians` (CCW) about `pivot`. Positions rotate;
    /// radii and sizes are unchanged; an arc's, text's, and insert's own rotation
    /// (stored in degrees) advances by the same angle.
    pub fn rotated(&self, pivot: Point2, radians: f64) -> Geometry {
        let rot = |p: Point2| p.rotate_about(pivot, radians);
        let ddeg = radians.to_degrees();
        match self {
            Geometry::Line(s) => Geometry::Line(Segment::new(rot(s.a), rot(s.b))),
            Geometry::Polyline(p) => Geometry::Polyline(Polyline::new(
                p.vertices.iter().map(|&v| rot(v)).collect(),
                p.closed,
            )),
            Geometry::Circle(c) => Geometry::Circle(Circle::new(rot(c.center), c.radius)),
            Geometry::Arc(a) => Geometry::Arc(Arc::new(
                rot(a.center),
                a.radius,
                a.start_angle + ddeg,
                a.end_angle + ddeg,
            )),
            Geometry::Text(t) => Geometry::Text(Text::new(
                t.value.clone(),
                rot(t.position),
                t.height,
                t.rotation + ddeg,
            )),
            Geometry::Insert(i) => Geometry::Insert(Insert::new(
                i.name.clone(),
                rot(i.position),
                i.rotation + ddeg,
            )),
        }
    }

    /// This geometry scaled by `factor` about `pivot`. Positions scale about the
    /// pivot; radii and text height scale by `|factor|`; rotations are unchanged.
    /// An insert has no scale field, so only its insertion point moves.
    pub fn scaled(&self, pivot: Point2, factor: f64) -> Geometry {
        let sc = |p: Point2| p.scale_about(pivot, factor);
        let f = factor.abs();
        match self {
            Geometry::Line(s) => Geometry::Line(Segment::new(sc(s.a), sc(s.b))),
            Geometry::Polyline(p) => Geometry::Polyline(Polyline::new(
                p.vertices.iter().map(|&v| sc(v)).collect(),
                p.closed,
            )),
            Geometry::Circle(c) => Geometry::Circle(Circle::new(sc(c.center), c.radius * f)),
            Geometry::Arc(a) => Geometry::Arc(Arc::new(
                sc(a.center),
                a.radius * f,
                a.start_angle,
                a.end_angle,
            )),
            Geometry::Text(t) => Geometry::Text(Text::new(
                t.value.clone(),
                sc(t.position),
                t.height * f,
                t.rotation,
            )),
            Geometry::Insert(i) => {
                Geometry::Insert(Insert::new(i.name.clone(), sc(i.position), i.rotation))
            }
        }
    }

    /// This geometry reflected across the line through `a` and `b`. Reflection
    /// reverses orientation, so an arc (stored CCW) has its start/end angles
    /// swapped and reflected about the axis; text and insert rotations reflect
    /// likewise.
    pub fn mirrored(&self, a: Point2, b: Point2) -> Geometry {
        let mir = |p: Point2| p.mirror_across(a, b);
        // Axis angle in degrees; reflecting angle α about the axis gives 2φ − α.
        let phi = (b.y - a.y).atan2(b.x - a.x).to_degrees();
        let refl = |deg: f64| 2.0 * phi - deg;
        match self {
            Geometry::Line(s) => Geometry::Line(Segment::new(mir(s.a), mir(s.b))),
            Geometry::Polyline(p) => Geometry::Polyline(Polyline::new(
                p.vertices.iter().map(|&v| mir(v)).collect(),
                p.closed,
            )),
            Geometry::Circle(c) => Geometry::Circle(Circle::new(mir(c.center), c.radius)),
            Geometry::Arc(arc) => Geometry::Arc(Arc::new(
                mir(arc.center),
                arc.radius,
                refl(arc.end_angle),
                refl(arc.start_angle),
            )),
            Geometry::Text(t) => Geometry::Text(Text::new(
                t.value.clone(),
                mir(t.position),
                t.height,
                refl(t.rotation),
            )),
            Geometry::Insert(i) => Geometry::Insert(Insert::new(
                i.name.clone(),
                mir(i.position),
                refl(i.rotation),
            )),
        }
    }

    /// This (block-space) geometry placed by an insert: shift the block's `base`
    /// to the origin, rotate by `rotation_deg` (CCW), then translate to
    /// `position`. This is the rigid placement `position + Rot(θ)·(p − base)` the
    /// renderer applies to block content, expressed by composing the tested
    /// [`translated`](Self::translated) and [`rotated`](Self::rotated) transforms —
    /// so EXPLODE bakes exactly what the canvas would draw.
    pub fn instanced(&self, base: Point2, position: Point2, rotation_deg: f64) -> Geometry {
        self.translated(-base.x, -base.y)
            .rotated(Point2::new(0.0, 0.0), rotation_deg.to_radians())
            .translated(position.x, position.y)
    }

    /// A parallel copy of this geometry, `distance` away. With a `side` point the
    /// copy moves toward it; without, a positive `distance` moves a line/polyline
    /// to its left (the `+` normal) and grows a circle/arc. Returns `None` for
    /// geometry with no parallel offset (text, inserts) or a degenerate result (a
    /// zero-length line, a radius driven to zero). A poly-line is mitred: adjacent
    /// offset edges are extended to their intersection.
    pub fn offset(&self, distance: f64, side: Option<Point2>) -> Option<Geometry> {
        match self {
            Geometry::Line(s) => {
                let n = s.direction()?.perp();
                let off = n * signed_offset(distance, n, s.a, side);
                Some(Geometry::Line(Segment::new(s.a + off, s.b + off)))
            }
            Geometry::Circle(c) => {
                let r = c.radius + radial_offset(distance, c.center, c.radius, side);
                (r > aadt_geometry::LENGTH_EPS).then(|| Geometry::Circle(Circle::new(c.center, r)))
            }
            Geometry::Arc(a) => {
                let r = a.radius + radial_offset(distance, a.center, a.radius, side);
                (r > aadt_geometry::LENGTH_EPS)
                    .then(|| Geometry::Arc(Arc::new(a.center, r, a.start_angle, a.end_angle)))
            }
            Geometry::Polyline(p) => offset_polyline(p, distance, side),
            Geometry::Text(_) | Geometry::Insert(_) => None,
        }
    }
}

/// Signed offset for a straight edge: positive moves along `normal` (the edge's
/// left side); a `side` point flips the sign to move the copy toward it.
fn signed_offset(distance: f64, normal: Vec2, base: Point2, side: Option<Point2>) -> f64 {
    match side {
        Some(sp) if (sp - base).dot(normal) < 0.0 => -distance.abs(),
        Some(_) => distance.abs(),
        None => distance,
    }
}

/// Signed radial delta for a circle/arc: positive grows the radius; a `side`
/// point outside the ring grows it, inside shrinks it.
fn radial_offset(distance: f64, center: Point2, radius: f64, side: Option<Point2>) -> f64 {
    match side {
        Some(sp) if sp.distance(center) < radius => -distance.abs(),
        Some(_) => distance.abs(),
        None => distance,
    }
}

/// Offset a poly-line by mitring: shift each edge's supporting line along its
/// left normal, then take consecutive intersections as the new vertices (the
/// endpoints of an open poly-line just shift). Returns `None` if any edge is
/// zero-length.
fn offset_polyline(p: &Polyline, distance: f64, side: Option<Point2>) -> Option<Geometry> {
    let verts = &p.vertices;
    if verts.len() < 2 {
        return None;
    }
    let segs = p.segments();
    // Sign the distance once, from the first edge's left normal, so the whole
    // poly-line offsets consistently to one side.
    let first = segs.first()?;
    let d = signed_offset(distance, first.direction()?.perp(), first.a, side);

    let mut lines = Vec::with_capacity(segs.len());
    for s in &segs {
        let off = s.direction()?.perp() * d;
        lines.push(Segment::new(s.a + off, s.b + off));
    }

    let m = verts.len();
    let mut out = Vec::with_capacity(m);
    let meet = |x: Segment, y: Segment, fallback: Point2| {
        x.line_intersection(y).map(|(pt, _)| pt).unwrap_or(fallback)
    };
    if p.closed {
        let n = lines.len();
        for i in 0..n {
            out.push(meet(lines[(i + n - 1) % n], lines[i], lines[i].a));
        }
    } else {
        out.push(lines[0].a);
        for i in 1..m - 1 {
            out.push(meet(lines[i - 1], lines[i], lines[i].a));
        }
        out.push(lines[lines.len() - 1].b);
    }
    Some(Geometry::Polyline(Polyline::new(out, p.closed)))
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
    /// A layer's properties were set (undo restores `before`, redo reapplies
    /// `after`); `None` means the layer was absent, so create/delete round-trips
    /// through the same op.
    SetLayer {
        name: String,
        before: Option<Layer>,
        after: Option<Layer>,
    },
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
    /// A hatch was appended (undo pops it, redo re-appends). Undo is strictly
    /// LIFO, so the hatch this reverts is always the last in the table.
    AddHatch(Hatch),
    /// A dimension was appended (undo pops it, redo re-appends).
    AddDimension(Dimension),
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
            Op::SetLayer { name, after, .. } => {
                Self::write_layer(&mut self.layers, name, after.clone());
            }
            Op::ReplaceGeometry { id, after, .. } => {
                if let Some(e) = self.entities.get_mut(id) {
                    e.geometry = after.clone();
                }
            }
            Op::SetAttribute { id, key, after, .. } => {
                Self::write_attribute(&mut self.entities, id, key, after.clone());
            }
            Op::AddHatch(h) => {
                self.hatches.push(h.clone());
            }
            Op::AddDimension(d) => {
                self.dimensions.push(d.clone());
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
            Op::SetLayer { name, before, .. } => {
                Self::write_layer(&mut self.layers, name, before.clone());
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
            // Undo is LIFO, so the appended annotation is the last in its table.
            Op::AddHatch(_) => {
                self.hatches.pop();
            }
            Op::AddDimension(_) => {
                self.dimensions.pop();
            }
        }
    }

    /// Set or remove a layer in the store (`None` removes it) — the primitive
    /// [`Op::SetLayer`] apply/invert share.
    fn write_layer(layers: &mut HashMap<String, Layer>, name: &str, value: Option<Layer>) {
        match value {
            Some(l) => {
                layers.insert(name.to_string(), l);
            }
            None => {
                layers.remove(name);
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

    /// Modify a layer's display state through `f`, creating it first if it does
    /// not exist. Recorded as one reversible [`Op::SetLayer`] (a fresh layer's
    /// `before` is `None`, so undo removes it). Returns `false` — recording
    /// nothing — when `f` leaves the layer unchanged.
    fn update_layer(&mut self, name: &str, f: impl FnOnce(&mut Layer)) -> bool {
        let before = self.layers.get(name).cloned();
        let mut candidate = before.clone().unwrap_or_else(|| Layer::new(name));
        f(&mut candidate);
        if before.as_ref() == Some(&candidate) {
            return false; // no change
        }
        self.layers.insert(name.to_string(), candidate.clone());
        self.record(Op::SetLayer {
            name: name.to_string(),
            before,
            after: Some(candidate),
        });
        self.revision += 1;
        true
    }

    /// Set a layer's ACI colour, creating the layer if needed. Returns whether it
    /// changed. Undoable.
    pub fn set_layer_color(&mut self, name: &str, color: u8) -> bool {
        self.update_layer(name, |l| l.color = color)
    }

    /// Turn a layer on or off (an off layer is not drawn). Undoable.
    pub fn set_layer_visible(&mut self, name: &str, visible: bool) -> bool {
        self.update_layer(name, |l| l.visible = visible)
    }

    /// Freeze or thaw a layer (a frozen layer is not drawn). Undoable.
    pub fn set_layer_frozen(&mut self, name: &str, frozen: bool) -> bool {
        self.update_layer(name, |l| l.frozen = frozen)
    }

    /// Whether entities on `layer` should be drawn: a layer unknown to the
    /// document is shown (an entity may reference a layer with no explicit
    /// record), and a known layer is shown when it is on and not frozen.
    pub fn layer_shown(&self, layer: &str) -> bool {
        self.layers.get(layer).is_none_or(Layer::shown)
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

    /// Rotate an entity `radians` (CCW) about `pivot`, keeping its id. Returns
    /// `false` if no entity has that id. Undoable via [`replace_geometry`](Self::replace_geometry).
    pub fn rotate_entity(&mut self, id: EntityId, pivot: Point2, radians: f64) -> bool {
        let Some(entity) = self.entities.get(&id) else {
            return false;
        };
        let rotated = entity.geometry.rotated(pivot, radians);
        self.replace_geometry(id, rotated)
    }

    /// Scale an entity by `factor` about `pivot`, keeping its id. Returns `false`
    /// if no entity has that id.
    pub fn scale_entity(&mut self, id: EntityId, pivot: Point2, factor: f64) -> bool {
        let Some(entity) = self.entities.get(&id) else {
            return false;
        };
        let scaled = entity.geometry.scaled(pivot, factor);
        self.replace_geometry(id, scaled)
    }

    /// Reflect an entity across the line through `a` and `b`, keeping its id.
    /// Returns `false` if no entity has that id.
    pub fn mirror_entity(&mut self, id: EntityId, a: Point2, b: Point2) -> bool {
        let Some(entity) = self.entities.get(&id) else {
            return false;
        };
        let mirrored = entity.geometry.mirrored(a, b);
        self.replace_geometry(id, mirrored)
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

    /// Add a hatch. Recorded as an undoable [`Op::AddHatch`] when a transaction
    /// is open (an interactive `HATCH`), and left **untracked** otherwise — so a
    /// bulk file load or block instantiation does not flood the undo history,
    /// exactly like [`add_geometry`](Self::add_geometry).
    pub fn add_hatch(&mut self, hatch: Hatch) {
        if self.open.is_some() {
            self.record(Op::AddHatch(hatch.clone()));
        }
        self.hatches.push(hatch);
        self.revision += 1;
    }

    /// Hatches in insertion order (deterministic).
    pub fn hatches(&self) -> &[Hatch] {
        &self.hatches
    }

    /// Add a dimension. Undoable when a transaction is open, untracked otherwise
    /// — see [`add_hatch`](Self::add_hatch).
    pub fn add_dimension(&mut self, dimension: Dimension) {
        if self.open.is_some() {
            self.record(Op::AddDimension(dimension.clone()));
        }
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
    fn rotate_scale_mirror_entity_keep_the_id_and_undo() {
        let mut doc = Document::new();
        let id = doc.add_geometry("A", line(0.0, 0.0, 2.0, 0.0));
        let original = doc.get(id).unwrap().geometry.clone();

        // Rotate 90° CCW about the origin: (0,0)->(0,0), (2,0)->(0,2).
        doc.transact("rotate", |d| {
            d.rotate_entity(id, Point2::new(0.0, 0.0), std::f64::consts::FRAC_PI_2)
        });
        if let Geometry::Line(s) = &doc.get(id).unwrap().geometry {
            assert!(s.a.distance(Point2::new(0.0, 0.0)) < 1e-9);
            assert!(s.b.distance(Point2::new(0.0, 2.0)) < 1e-9);
        } else {
            panic!("expected line");
        }
        doc.undo();
        assert_eq!(
            doc.get(id).unwrap().geometry,
            original,
            "rotate undoes exactly"
        );

        // Scale ×2 about the origin: endpoint (2,0)->(4,0).
        doc.transact("scale", |d| d.scale_entity(id, Point2::new(0.0, 0.0), 2.0));
        if let Geometry::Line(s) = &doc.get(id).unwrap().geometry {
            assert!(s.b.distance(Point2::new(4.0, 0.0)) < 1e-9);
        } else {
            panic!("expected line");
        }
        doc.undo();

        // Mirror across the x-axis: a horizontal line on y=0 is unchanged in shape.
        let id2 = doc.add_geometry("A", line(0.0, 1.0, 2.0, 1.0));
        doc.transact("mirror", |d| {
            d.mirror_entity(id2, Point2::new(0.0, 0.0), Point2::new(1.0, 0.0))
        });
        if let Geometry::Line(s) = &doc.get(id2).unwrap().geometry {
            assert!(s.a.distance(Point2::new(0.0, -1.0)) < 1e-9);
            assert!(s.b.distance(Point2::new(2.0, -1.0)) < 1e-9);
        } else {
            panic!("expected line");
        }
    }

    #[test]
    fn instanced_matches_base_rotation_and_position() {
        // A block-space segment (0,0)-(1,0), base (1,1), placed at (10,10) with no
        // rotation draws from (10,10)-(1,1)=(9,9) to (10,9) — the renderer's rule.
        let g = line(0.0, 0.0, 1.0, 0.0);
        let placed = g.instanced(Point2::new(1.0, 1.0), Point2::new(10.0, 10.0), 0.0);
        if let Geometry::Line(s) = placed {
            assert!(s.a.distance(Point2::new(9.0, 9.0)) < 1e-9);
            assert!(s.b.distance(Point2::new(10.0, 9.0)) < 1e-9);
        } else {
            panic!("expected line");
        }
        // With 90° rotation about its own base at the origin, (1,0) -> (0,1).
        let g2 = line(0.0, 0.0, 1.0, 0.0);
        let placed2 = g2.instanced(Point2::new(0.0, 0.0), Point2::new(0.0, 0.0), 90.0);
        if let Geometry::Line(s) = placed2 {
            assert!(s.b.distance(Point2::new(0.0, 1.0)) < 1e-9, "got {:?}", s.b);
        } else {
            panic!("expected line");
        }
    }

    #[test]
    fn offset_moves_a_line_perpendicular() {
        // A horizontal line offset by 2 with no side point moves to its left (+y).
        let g = line(0.0, 0.0, 10.0, 0.0);
        let Geometry::Line(s) = g.offset(2.0, None).unwrap() else {
            panic!("expected line");
        };
        assert!(s.a.distance(Point2::new(0.0, 2.0)) < 1e-9);
        assert!(s.b.distance(Point2::new(10.0, 2.0)) < 1e-9);

        // A side point below the line flips it to -y regardless of sign.
        let Geometry::Line(s) = g.offset(2.0, Some(Point2::new(5.0, -3.0))).unwrap() else {
            panic!("expected line");
        };
        assert!(
            s.a.distance(Point2::new(0.0, -2.0)) < 1e-9,
            "toward the side point"
        );
    }

    #[test]
    fn offset_grows_or_shrinks_a_circle_by_side() {
        let c = Geometry::Circle(Circle::new(Point2::new(0.0, 0.0), 5.0));
        // No side, positive distance grows.
        let Geometry::Circle(g) = c.offset(2.0, None).unwrap() else {
            panic!();
        };
        assert!((g.radius - 7.0).abs() < 1e-9);
        // A side point inside shrinks.
        let Geometry::Circle(g) = c.offset(2.0, Some(Point2::new(0.0, 0.0))).unwrap() else {
            panic!();
        };
        assert!((g.radius - 3.0).abs() < 1e-9);
        // Shrinking past the centre is degenerate → None.
        assert!(c.offset(10.0, Some(Point2::new(0.0, 0.0))).is_none());
    }

    #[test]
    fn offset_mitres_a_polyline_corner() {
        // An L: (0,0)-(4,0)-(4,4). Offsetting inward (right side) by 1 mitres the
        // corner to (3,1); endpoints shift by 1 along each leg's normal.
        let poly = Geometry::Polyline(Polyline::new(
            vec![
                Point2::new(0.0, 0.0),
                Point2::new(4.0, 0.0),
                Point2::new(4.0, 4.0),
            ],
            false,
        ));
        let Geometry::Polyline(p) = poly.offset(1.0, Some(Point2::new(2.0, 2.0))).unwrap() else {
            panic!("expected polyline");
        };
        assert_eq!(p.vertices.len(), 3);
        // The shared corner moves to the inside mitre point (3,1).
        assert!(
            p.vertices[1].distance(Point2::new(3.0, 1.0)) < 1e-9,
            "got {:?}",
            p.vertices[1]
        );
    }

    #[test]
    fn offset_of_text_is_none() {
        let t = Geometry::Text(Text::new("X", Point2::new(0.0, 0.0), 1.0, 0.0));
        assert!(t.offset(1.0, None).is_none());
    }

    #[test]
    fn scale_grows_a_circle_radius() {
        let mut doc = Document::new();
        let id = doc.add_geometry(
            "A",
            Geometry::Circle(Circle::new(Point2::new(0.0, 0.0), 2.0)),
        );
        doc.transact("scale", |d| d.scale_entity(id, Point2::new(0.0, 0.0), 3.0));
        if let Geometry::Circle(c) = &doc.get(id).unwrap().geometry {
            assert!((c.radius - 6.0).abs() < 1e-9);
        } else {
            panic!("expected circle");
        }
    }

    #[test]
    fn layer_state_is_editable_and_undoable() {
        let mut doc = Document::new();
        // A used-but-unrecorded layer is shown by default.
        doc.add_geometry("A-WALL", line(0.0, 0.0, 1.0, 0.0));
        assert!(doc.layer_shown("A-WALL"));
        assert!(doc.layer("A-WALL").unwrap().shown());

        // Colour it, then turn it off, each an undoable step.
        doc.transact("color", |d| d.set_layer_color("A-WALL", 5));
        assert_eq!(doc.layer("A-WALL").unwrap().color, 5);

        doc.transact("off", |d| d.set_layer_visible("A-WALL", false));
        assert!(!doc.layer_shown("A-WALL"), "an off layer is not shown");

        doc.undo(); // thaw the "off"
        assert!(doc.layer_shown("A-WALL"));
        assert_eq!(doc.layer("A-WALL").unwrap().color, 5, "colour survives");

        doc.undo(); // revert the colour
        assert_eq!(doc.layer("A-WALL").unwrap().color, DEFAULT_LAYER_COLOR);

        // Freezing hides too.
        doc.transact("freeze", |d| d.set_layer_frozen("A-WALL", true));
        assert!(!doc.layer_shown("A-WALL"));
    }

    #[test]
    fn setting_a_layer_property_to_its_current_value_records_nothing() {
        let mut doc = Document::new();
        doc.add_geometry("A", line(0.0, 0.0, 1.0, 0.0));
        // A brand-new layer already has the default colour; setting it again is a
        // no-op that leaves nothing to undo.
        let changed = doc.transact("noop", |d| d.set_layer_color("A", DEFAULT_LAYER_COLOR));
        assert!(!changed);
        assert!(!doc.can_undo(), "an unchanged set records no transaction");
    }

    #[test]
    fn add_hatch_and_dimension_are_undoable_when_transacted() {
        let mut doc = Document::new();
        let boundary = Polyline::new(
            vec![
                Point2::new(0.0, 0.0),
                Point2::new(4.0, 0.0),
                Point2::new(4.0, 3.0),
                Point2::new(0.0, 3.0),
            ],
            true,
        );
        doc.transact("hatch", |d| {
            d.add_hatch(Hatch::new("A-FLOR", boundary.clone(), "ANSI31"))
        });
        assert_eq!(doc.hatches().len(), 1);
        doc.undo();
        assert_eq!(doc.hatches().len(), 0, "undo pops the hatch");
        doc.redo();
        assert_eq!(doc.hatches().len(), 1, "redo re-appends it");

        doc.transact("dim", |d| {
            d.add_dimension(Dimension {
                layer: "A-DIMS".to_string(),
                kind: DimensionKind::Aligned,
                p1: Point2::new(0.0, 0.0),
                p2: Point2::new(4.0, 0.0),
                vertex: Point2::new(0.0, 0.0),
                text_anchor: Point2::new(2.0, -1.0),
                text: String::new(),
                measurement: 4.0,
            })
        });
        assert_eq!(doc.dimensions().len(), 1);
        doc.undo();
        assert_eq!(doc.dimensions().len(), 0, "undo pops the dimension");
    }

    #[test]
    fn bulk_added_annotation_is_untracked() {
        // No open transaction (a file load) → no undo entry, matching add_geometry.
        let mut doc = Document::new();
        let boundary = Polyline::new(
            vec![
                Point2::new(0.0, 0.0),
                Point2::new(1.0, 0.0),
                Point2::new(1.0, 1.0),
            ],
            true,
        );
        doc.add_hatch(Hatch::new("A", boundary, "SOLID"));
        assert_eq!(doc.hatches().len(), 1);
        assert!(!doc.can_undo(), "an untracked add records no undo entry");
    }

    #[test]
    fn transforming_a_missing_id_is_false() {
        let mut doc = Document::new();
        let ghost = EntityId(999);
        assert!(!doc.rotate_entity(ghost, Point2::new(0.0, 0.0), 1.0));
        assert!(!doc.scale_entity(ghost, Point2::new(0.0, 0.0), 2.0));
        assert!(!doc.mirror_entity(ghost, Point2::new(0.0, 0.0), Point2::new(1.0, 0.0)));
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
            vertex: Point2::new(0.0, 0.0),
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
