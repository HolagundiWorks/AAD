//! # aadt-render — 2D rendering layer (substrate, spec §3 / §4.3)
//!
//! The seam between the document model and a pixel-pushing backend. A
//! GPU-accelerated `wgpu` canvas (see ADR-0001) targeting 60fps on 100k+ entity
//! drawings is the eventual backend, but the parts that are *verifiable without a
//! display* live here and land first:
//!
//! - [`DrawList`] / [`DrawCommand`] — a backend-agnostic snapshot of what to draw,
//!   built from a read-only [`Document`] by [`build_drawlist`]. Entity geometry is
//!   flattened into primitives (poly-lines, circles, arcs, text anchors, markers),
//!   each tagged with its source layer so a backend can colour by layer. A block
//!   [`Insert`](aadt_model::Insert) is *instantiated* — the referenced definition's
//!   geometry is expanded, transformed by the insert's base/rotation/position —
//!   falling back to a marker only when the block is undefined.
//! - [`Camera`] — the world→screen transform (pan/zoom), a pure function with a
//!   [`Camera::fit`] zoom-to-extents. No GPU needed to test the math.
//! - [`Renderer`] — the one-method trait a real `wgpu` canvas will implement. A
//!   [`RecordingRenderer`] stands in headlessly so the pipeline is exercised in
//!   tests today.
//!
//! **What is deliberately absent:** `wgpu`, `egui`, any windowing crate. This
//! environment has no display, so a live canvas cannot be verified here; pulling
//! the GPU stack in unverified is exactly the risk this seam defers. The real
//! backend is *another `impl Renderer`*, dropped in when there is a screen to
//! prove it on.

#[cfg(feature = "gpu")]
pub mod gpu;
pub mod interact;
#[cfg(feature = "gui")]
pub mod view;

use aadt_geometry::{Bbox, Point2};
use aadt_model::{Dimension, Document, Geometry, Hatch};

/// A dimension's value text is drawn at this fraction of the measured span, so it
/// scales sensibly with the drawing. A dimension style would override it; the
/// model carries no text height, so this is the renderer's default.
const DIM_TEXT_FRACTION: f64 = 0.05;

/// Performance target from spec §4.3: entities renderable at 60fps.
pub const TARGET_ENTITY_COUNT: u32 = 100_000;

/// A single primitive to draw, in world coordinates. Backend-agnostic: a `wgpu`
/// canvas, an SVG dump, or the test [`RecordingRenderer`] all consume the same
/// list. Every command carries the `layer` it came from so a backend can resolve
/// a colour.
#[derive(Debug, Clone, PartialEq)]
pub enum DrawCommand {
    /// A connected run of vertices; `closed` joins the last back to the first.
    Polyline {
        layer: String,
        points: Vec<Point2>,
        closed: bool,
    },
    /// A true circle (backend may tessellate as it likes).
    Circle {
        layer: String,
        center: Point2,
        radius: f64,
    },
    /// A circular arc, angles in degrees CCW (DXF convention).
    Arc {
        layer: String,
        center: Point2,
        radius: f64,
        start_angle: f64,
        end_angle: f64,
    },
    /// A text anchor. The string and cap height travel with it; glyph layout is
    /// the backend's job (AADT models no font metrics).
    Text {
        layer: String,
        position: Point2,
        height: f64,
        rotation: f64,
        value: String,
    },
    /// A point marker (e.g. a block insertion point), drawn as a small glyph by
    /// the backend. Carries the block name for labelling.
    Marker {
        layer: String,
        position: Point2,
        label: String,
    },
}

impl DrawCommand {
    /// The layer this primitive belongs to.
    pub fn layer(&self) -> &str {
        match self {
            DrawCommand::Polyline { layer, .. }
            | DrawCommand::Circle { layer, .. }
            | DrawCommand::Arc { layer, .. }
            | DrawCommand::Text { layer, .. }
            | DrawCommand::Marker { layer, .. } => layer,
        }
    }
}

/// An ordered batch of [`DrawCommand`]s: the frame a [`Renderer`] paints.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DrawList {
    pub commands: Vec<DrawCommand>,
}

impl DrawList {
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

/// The rigid placement an [`Insert`](aadt_model::Insert) applies to its block's
/// geometry: shift the block's `base` to the origin, rotate by the insert angle
/// (degrees CCW, the DXF convention shared with [`Arc`](aadt_geometry::Arc) and
/// [`Text`](aadt_model::Text)), then translate to the insert position. No scale —
/// [`Insert`](aadt_model::Insert) carries none. The identity transform (base and
/// position at the origin, zero rotation) leaves top-level geometry untouched, so
/// one code path serves both placed entities and instanced block content.
#[derive(Debug, Clone, Copy)]
struct Xform {
    base: Point2,
    rotation_deg: f64,
    position: Point2,
}

impl Xform {
    const IDENTITY: Xform = Xform {
        base: Point2::new(0.0, 0.0),
        rotation_deg: 0.0,
        position: Point2::new(0.0, 0.0),
    };

    /// Map a block-space point to world space: `position + Rot(θ)·(p − base)`.
    fn point(&self, p: Point2) -> Point2 {
        let (lx, ly) = (p.x - self.base.x, p.y - self.base.y);
        let (s, c) = self.rotation_deg.to_radians().sin_cos();
        Point2::new(
            self.position.x + lx * c - ly * s,
            self.position.y + lx * s + ly * c,
        )
    }
}

/// Turn one piece of (possibly block-space) geometry into a draw primitive,
/// transformed by `xform` and tagged with `layer`. A nested `Insert` inside a
/// block becomes a [`DrawCommand::Marker`] at its transformed insertion point —
/// recursive block instantiation is a later increment, and a marker keeps it
/// visible without risking unbounded recursion on a cyclic definition.
fn primitive(geometry: &Geometry, xform: &Xform, layer: String) -> DrawCommand {
    match geometry {
        Geometry::Line(s) => DrawCommand::Polyline {
            layer,
            points: vec![xform.point(s.a), xform.point(s.b)],
            closed: false,
        },
        Geometry::Polyline(p) => DrawCommand::Polyline {
            layer,
            points: p.vertices.iter().map(|v| xform.point(*v)).collect(),
            closed: p.closed,
        },
        Geometry::Circle(c) => DrawCommand::Circle {
            layer,
            center: xform.point(c.center),
            radius: c.radius,
        },
        Geometry::Arc(a) => DrawCommand::Arc {
            layer,
            center: xform.point(a.center),
            radius: a.radius,
            start_angle: a.start_angle + xform.rotation_deg,
            end_angle: a.end_angle + xform.rotation_deg,
        },
        Geometry::Text(t) => DrawCommand::Text {
            layer,
            position: xform.point(t.position),
            height: t.height,
            rotation: t.rotation + xform.rotation_deg,
            value: t.value.clone(),
        },
        Geometry::Insert(ins) => DrawCommand::Marker {
            layer,
            position: xform.point(ins.position),
            label: ins.name.clone(),
        },
    }
}

/// The layer an instanced block entity draws on. Following the DXF convention,
/// geometry drawn on layer `"0"` inside a block *inherits the insert's layer*
/// (so a door swing colours with its `A-DOOR` insert); geometry on any named
/// layer keeps that layer. This matches how [`build_drawlist`] tags top-level
/// entities — by the layer the drawn thing effectively belongs to.
fn instanced_layer(block_entity_layer: &str, insert_layer: &str) -> String {
    if block_entity_layer == "0" {
        insert_layer.to_string()
    } else {
        block_entity_layer.to_string()
    }
}

/// The boundary of a [`Hatch`] as a closed poly-line. The fill pattern itself is
/// a backend concern (solid fills and pattern lines are the renderer's to draw);
/// the drawlist carries the outline, tagged with the hatch's layer.
fn hatch_command(hatch: &Hatch) -> DrawCommand {
    DrawCommand::Polyline {
        layer: hatch.layer.clone(),
        points: hatch.boundary.vertices.clone(),
        closed: true,
    }
}

/// A [`Dimension`] flattened to draw primitives: the dimension line between the
/// measured points, and the value text at its anchor. Extension lines, arrowheads
/// and a dimension-line offset belong to a dimension style the model does not yet
/// carry — the measured span and the value are the parts that render faithfully.
fn dimension_commands(dim: &Dimension) -> Vec<DrawCommand> {
    let span = dim.p1.distance(dim.p2);
    let label = if dim.text.trim().is_empty() {
        format!("{}", dim.measurement)
    } else {
        dim.text.clone()
    };
    vec![
        DrawCommand::Polyline {
            layer: dim.layer.clone(),
            points: vec![dim.p1, dim.p2],
            closed: false,
        },
        DrawCommand::Text {
            layer: dim.layer.clone(),
            position: dim.text_anchor,
            // A default height proportional to the span (see [`DIM_TEXT_FRACTION`]),
            // floored so a zero-length dimension still yields a visible glyph.
            height: (span * DIM_TEXT_FRACTION).max(f64::MIN_POSITIVE),
            rotation: 0.0,
            value: label,
        },
    ]
}

/// Translate a read-only [`Document`] snapshot into a [`DrawList`].
///
/// Entities are visited in the document's deterministic id order, then hatches
/// and dimensions in their table order, so the same document always yields
/// byte-identical draw lists — a property the tests lean on and a future
/// frame-diff optimiser will too. Non-insert geometry maps one-to-one to a
/// primitive.
///
/// An [`Geometry::Insert`] is *instantiated*: its referenced block definition
/// (looked up read-only via [`Document::block`]) is expanded, each block entity
/// transformed by the insert's base/rotation/position (see [`Xform`]) and tagged
/// via [`instanced_layer`]. When the block is **undefined** — a legitimate case,
/// e.g. a DXF exported without its `BLOCKS` section — the insert falls back to a
/// single [`DrawCommand::Marker`] at its insertion point rather than being dropped.
///
/// [`Hatch`]es contribute their boundary outline and [`Dimension`]s their
/// dimension line plus value text (see [`hatch_command`], [`dimension_commands`]).
pub fn build_drawlist(doc: &Document) -> DrawList {
    let mut commands = Vec::with_capacity(doc.len());
    for e in doc.entities() {
        // A hidden layer (off or frozen) draws nothing.
        if !doc.layer_shown(&e.layer) {
            continue;
        }
        match &e.geometry {
            Geometry::Insert(ins) => match doc.block(&ins.name) {
                Some(block) => {
                    let xform = Xform {
                        base: block.base,
                        rotation_deg: ins.rotation,
                        position: ins.position,
                    };
                    for be in &block.entities {
                        let layer = instanced_layer(&be.layer, &e.layer);
                        commands.push(primitive(&be.geometry, &xform, layer));
                    }
                }
                None => commands.push(DrawCommand::Marker {
                    layer: e.layer.clone(),
                    position: ins.position,
                    label: ins.name.clone(),
                }),
            },
            other => commands.push(primitive(other, &Xform::IDENTITY, e.layer.clone())),
        }
    }
    for hatch in doc.hatches() {
        if doc.layer_shown(&hatch.layer) {
            commands.push(hatch_command(hatch));
        }
    }
    for dim in doc.dimensions() {
        if doc.layer_shown(&dim.layer) {
            commands.extend(dimension_commands(dim));
        }
    }
    DrawList { commands }
}

/// The world→screen transform for a 2D CAD viewport: a centre point in world
/// space, a uniform `scale` (screen pixels per world unit), and the screen size.
///
/// Y is flipped (CAD is Y-up, screens are Y-down). Pure math; no GPU. `scale`
/// carries pan/zoom and [`Camera::fit`] derives a zoom-to-extents from a bounding
/// box, which is the headless-verifiable half of the B1 canvas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    /// World-space point mapped to the centre of the viewport.
    pub center: Point2,
    /// Screen pixels per world unit.
    pub scale: f64,
    /// Viewport size in pixels (width, height).
    pub viewport: (f64, f64),
}

impl Camera {
    /// A camera centred on the origin at 1:1 scale for a given viewport.
    pub fn new(viewport: (f64, f64)) -> Self {
        Self {
            center: Point2::new(0.0, 0.0),
            scale: 1.0,
            viewport,
        }
    }

    /// Map a world point to screen pixels (origin top-left, Y down).
    pub fn world_to_screen(&self, p: Point2) -> (f64, f64) {
        let (w, h) = self.viewport;
        let sx = (p.x - self.center.x) * self.scale + w * 0.5;
        let sy = h * 0.5 - (p.y - self.center.y) * self.scale;
        (sx, sy)
    }

    /// Map screen pixels back to a world point (inverse of [`world_to_screen`]).
    ///
    /// [`world_to_screen`]: Camera::world_to_screen
    pub fn screen_to_world(&self, sx: f64, sy: f64) -> Point2 {
        let (w, h) = self.viewport;
        Point2::new(
            (sx - w * 0.5) / self.scale + self.center.x,
            (h * 0.5 - sy) / self.scale + self.center.y,
        )
    }

    /// Frame `bbox` in the viewport with a margin (fraction of the viewport, per
    /// side): centre on the box and choose the scale that just fits it. A
    /// zero-area box (single point, empty drawing) keeps the current scale and
    /// only re-centres, avoiding a divide-by-zero blow-up to infinite zoom.
    pub fn fit(&mut self, bbox: Bbox, margin: f64) {
        self.center = bbox.min.midpoint(bbox.max);
        let (w, h) = self.viewport;
        let usable_w = w * (1.0 - 2.0 * margin);
        let usable_h = h * (1.0 - 2.0 * margin);
        let bw = bbox.width();
        let bh = bbox.height();
        let sx = if bw > aadt_geometry::LENGTH_EPS {
            usable_w / bw
        } else {
            f64::INFINITY
        };
        let sy = if bh > aadt_geometry::LENGTH_EPS {
            usable_h / bh
        } else {
            f64::INFINITY
        };
        let s = sx.min(sy);
        if s.is_finite() && s > 0.0 {
            self.scale = s;
        }
    }

    /// Convenience: fit the whole document's extents into the viewport. Leaves the
    /// camera unchanged for an empty document.
    pub fn fit_document(&mut self, doc: &Document, margin: f64) {
        if let Some(bbox) = Bbox::of_points(doc.points()) {
            self.fit(bbox, margin);
        }
    }
}

/// The single seam a real backend implements. A `wgpu` canvas, an SVG exporter,
/// and the headless [`RecordingRenderer`] are all just `impl Renderer`.
///
/// One method by design: the render layer's whole contract is "given the frame's
/// draw list (and the camera that positions it), paint it". Everything else —
/// buffers, pipelines, tessellation — is the backend's private concern.
pub trait Renderer {
    /// Paint one frame. Called once per redraw with a fresh (or cached) list.
    fn render(&mut self, camera: &Camera, list: &DrawList);
}

/// A headless [`Renderer`] that records the frames it is asked to paint instead
/// of drawing them. Lets the model→drawlist→renderer pipeline be exercised end to
/// end in tests with no display, and doubles as a golden-output fixture.
#[derive(Debug, Default)]
pub struct RecordingRenderer {
    /// Every frame handed to [`render`](Renderer::render), in order.
    pub frames: Vec<DrawList>,
    /// The camera from the most recent frame, if any.
    pub last_camera: Option<Camera>,
}

impl RecordingRenderer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Total primitives across every recorded frame.
    pub fn total_commands(&self) -> usize {
        self.frames.iter().map(DrawList::len).sum()
    }
}

impl Renderer for RecordingRenderer {
    fn render(&mut self, camera: &Camera, list: &DrawList) {
        self.last_camera = Some(*camera);
        self.frames.push(list.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aadt_geometry::{Arc, Circle, Point2, Polyline, Segment};
    use aadt_model::{Block, Insert, Text};

    fn sample_doc() -> Document {
        let mut doc = Document::new();
        doc.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0))),
        );
        doc.add_geometry(
            "A-ROOM",
            Geometry::Polyline(Polyline::new(
                vec![
                    Point2::new(0.0, 0.0),
                    Point2::new(4.0, 0.0),
                    Point2::new(4.0, 3.0),
                ],
                true,
            )),
        );
        doc.add_geometry(
            "A-COL",
            Geometry::Circle(Circle::new(Point2::new(5.0, 5.0), 2.0)),
        );
        doc.add_geometry(
            "A-SWING",
            Geometry::Arc(Arc::new(Point2::new(0.0, 0.0), 3.0, 0.0, 90.0)),
        );
        doc.add_geometry(
            "A-TEXT",
            Geometry::Text(Text::new("KITCHEN", Point2::new(1.0, 1.0), 0.25, 0.0)),
        );
        doc.add_geometry(
            "A-DOOR",
            Geometry::Insert(Insert::new("DOOR", Point2::new(2.0, 2.0), 90.0)),
        );
        doc
    }

    #[test]
    fn every_geometry_maps_to_a_primitive() {
        let doc = sample_doc();
        let list = build_drawlist(&doc);
        assert_eq!(list.len(), 6, "one primitive per entity");

        // Order follows the document's deterministic id order.
        assert!(matches!(
            list.commands[0],
            DrawCommand::Polyline { closed: false, .. }
        ));
        assert!(matches!(
            list.commands[1],
            DrawCommand::Polyline { closed: true, .. }
        ));
        assert!(matches!(list.commands[2], DrawCommand::Circle { .. }));
        assert!(matches!(list.commands[3], DrawCommand::Arc { .. }));
        assert!(matches!(list.commands[4], DrawCommand::Text { .. }));
        assert!(matches!(list.commands[5], DrawCommand::Marker { .. }));
    }

    #[test]
    fn line_becomes_two_point_polyline_and_layer_is_preserved() {
        let mut doc = Document::new();
        doc.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(Point2::new(1.0, 2.0), Point2::new(3.0, 4.0))),
        );
        let list = build_drawlist(&doc);
        match &list.commands[0] {
            DrawCommand::Polyline {
                layer,
                points,
                closed,
            } => {
                assert_eq!(layer, "A-WALL");
                assert_eq!(points, &[Point2::new(1.0, 2.0), Point2::new(3.0, 4.0)]);
                assert!(!closed);
            }
            other => panic!("expected a polyline, got {other:?}"),
        }
        assert_eq!(list.commands[0].layer(), "A-WALL");
    }

    #[test]
    fn insert_of_an_undefined_block_falls_back_to_a_marker() {
        // No block "DOOR-36" is defined — a legitimate case (DXF without its
        // BLOCKS section). The insert must survive as a marker, not be dropped.
        let mut doc = Document::new();
        doc.add_geometry(
            "A-DOOR",
            Geometry::Insert(Insert::new("DOOR-36", Point2::new(2.0, 2.0), 0.0)),
        );
        let list = build_drawlist(&doc);
        assert_eq!(list.len(), 1);
        match &list.commands[0] {
            DrawCommand::Marker {
                position, label, ..
            } => {
                assert_eq!(*position, Point2::new(2.0, 2.0));
                assert_eq!(label, "DOOR-36");
            }
            other => panic!("expected a marker, got {other:?}"),
        }
    }

    /// A block whose base is the origin, holding a line and a circle, both on
    /// layer "0" (so they inherit the insert's layer).
    fn door_block() -> Block {
        let mut b = Block::new("DOOR", Point2::new(0.0, 0.0));
        b.push(
            "0",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(1.0, 0.0))),
        );
        b.push(
            "0",
            Geometry::Circle(Circle::new(Point2::new(0.0, 0.0), 0.5)),
        );
        b
    }

    #[test]
    fn insert_of_a_defined_block_expands_to_its_primitives() {
        let mut doc = Document::new();
        doc.add_block(door_block());
        // No rotation, translated to (10, 5). Block base is the origin.
        doc.add_geometry(
            "A-DOOR",
            Geometry::Insert(Insert::new("DOOR", Point2::new(10.0, 5.0), 0.0)),
        );
        let list = build_drawlist(&doc);
        assert_eq!(
            list.len(),
            2,
            "the insert expands into the block's 2 entities"
        );

        match &list.commands[0] {
            DrawCommand::Polyline { layer, points, .. } => {
                assert_eq!(
                    layer, "A-DOOR",
                    "layer-0 block geometry inherits the insert layer"
                );
                assert_eq!(points, &[Point2::new(10.0, 5.0), Point2::new(11.0, 5.0)]);
            }
            other => panic!("expected a polyline, got {other:?}"),
        }
        match &list.commands[1] {
            DrawCommand::Circle {
                layer,
                center,
                radius,
            } => {
                assert_eq!(layer, "A-DOOR");
                assert_eq!(*center, Point2::new(10.0, 5.0));
                assert_eq!(*radius, 0.5);
            }
            other => panic!("expected a circle, got {other:?}"),
        }
    }

    #[test]
    fn insert_rotation_rotates_the_instanced_geometry() {
        let mut doc = Document::new();
        doc.add_block(door_block());
        // 90° CCW about the block base, then place at the origin. The block's
        // (1, 0) endpoint should rotate to (0, 1).
        doc.add_geometry(
            "A-DOOR",
            Geometry::Insert(Insert::new("DOOR", Point2::new(0.0, 0.0), 90.0)),
        );
        let list = build_drawlist(&doc);
        match &list.commands[0] {
            DrawCommand::Polyline { points, .. } => {
                assert!((points[0].x).abs() < 1e-9 && (points[0].y).abs() < 1e-9);
                assert!(
                    (points[1].x).abs() < 1e-9 && (points[1].y - 1.0).abs() < 1e-9,
                    "endpoint (1,0) rotates 90° to (0,1): got {:?}",
                    points[1]
                );
            }
            other => panic!("expected a polyline, got {other:?}"),
        }
    }

    #[test]
    fn block_base_offset_is_applied_before_placement() {
        // Base at (1, 1): the block's own origin sits one unit down-left of its
        // base, so an unrotated insert at (10, 10) draws the line from
        // (10,10)-(1,1) = (9, 9) to (10, 9).
        let mut doc = Document::new();
        let mut b = Block::new("SEG", Point2::new(1.0, 1.0));
        b.push(
            "STEEL",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(1.0, 0.0))),
        );
        doc.add_block(b);
        doc.add_geometry(
            "A-COL",
            Geometry::Insert(Insert::new("SEG", Point2::new(10.0, 10.0), 0.0)),
        );
        match &build_drawlist(&doc).commands[0] {
            DrawCommand::Polyline { layer, points, .. } => {
                assert_eq!(layer, "STEEL", "a named block layer is kept, not inherited");
                assert_eq!(points, &[Point2::new(9.0, 9.0), Point2::new(10.0, 9.0)]);
            }
            other => panic!("expected a polyline, got {other:?}"),
        }
    }

    #[test]
    fn instanced_arc_angles_shift_by_the_insert_rotation() {
        let mut doc = Document::new();
        let mut b = Block::new("SWING", Point2::new(0.0, 0.0));
        b.push(
            "0",
            Geometry::Arc(Arc::new(Point2::new(0.0, 0.0), 2.0, 0.0, 90.0)),
        );
        doc.add_block(b);
        doc.add_geometry(
            "A-DOOR",
            Geometry::Insert(Insert::new("SWING", Point2::new(0.0, 0.0), 45.0)),
        );
        match &build_drawlist(&doc).commands[0] {
            DrawCommand::Arc {
                start_angle,
                end_angle,
                ..
            } => {
                assert!((start_angle - 45.0).abs() < 1e-9);
                assert!((end_angle - 135.0).abs() < 1e-9);
            }
            other => panic!("expected an arc, got {other:?}"),
        }
    }

    #[test]
    fn hatches_and_dimensions_join_the_drawlist() {
        use aadt_model::DimensionKind;

        let mut doc = Document::new();
        doc.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0))),
        );
        doc.add_hatch(Hatch::new(
            "A-FLOR",
            Polyline::new(
                vec![
                    Point2::new(0.0, 0.0),
                    Point2::new(4.0, 0.0),
                    Point2::new(4.0, 3.0),
                    Point2::new(0.0, 3.0),
                ],
                true,
            ),
            "ANSI31",
        ));
        doc.add_dimension(Dimension {
            layer: "A-DIMS".to_string(),
            kind: DimensionKind::Linear,
            p1: Point2::new(0.0, 0.0),
            p2: Point2::new(10.0, 0.0),
            text_anchor: Point2::new(5.0, -1.0),
            text: String::new(),
            measurement: 10.0,
        });

        let list = build_drawlist(&doc);
        // 1 entity + 1 hatch outline + (1 dim line + 1 dim text) = 4.
        assert_eq!(list.len(), 4);

        // The hatch renders as a closed boundary on its layer.
        let hatch = list
            .commands
            .iter()
            .find(|c| c.layer() == "A-FLOR")
            .expect("hatch boundary present");
        assert!(matches!(
            hatch,
            DrawCommand::Polyline { closed: true, points, .. } if points.len() == 4
        ));

        // The dimension renders as a span line plus its measured value as text.
        let dim_text = list.commands.iter().find_map(|c| match c {
            DrawCommand::Text { layer, value, .. } if layer == "A-DIMS" => Some(value.clone()),
            _ => None,
        });
        assert_eq!(
            dim_text.as_deref(),
            Some("10"),
            "empty override → measurement"
        );
    }

    #[test]
    fn drawlist_is_deterministic() {
        let mut doc = sample_doc();
        doc.add_hatch(Hatch::new(
            "A-FLOR",
            Polyline::new(vec![Point2::new(0.0, 0.0), Point2::new(1.0, 0.0)], true),
            "SOLID",
        ));
        assert_eq!(build_drawlist(&doc), build_drawlist(&doc));
    }

    #[test]
    fn hidden_layers_are_culled_from_the_drawlist() {
        let mut doc = Document::new();
        doc.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(1.0, 0.0))),
        );
        doc.add_geometry(
            "A-GRID",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(0.0, 1.0))),
        );
        assert_eq!(build_drawlist(&doc).len(), 2);

        // Turning A-GRID off drops its entity from the frame.
        doc.set_layer_visible("A-GRID", false);
        let list = build_drawlist(&doc);
        assert_eq!(list.len(), 1);
        assert_eq!(list.commands[0].layer(), "A-WALL");

        // Freezing hides too; thawing/turning back on restores it.
        doc.set_layer_visible("A-GRID", true);
        doc.set_layer_frozen("A-WALL", true);
        let list = build_drawlist(&doc);
        assert_eq!(list.len(), 1);
        assert_eq!(list.commands[0].layer(), "A-GRID");
    }

    #[test]
    fn empty_document_yields_empty_list() {
        assert!(build_drawlist(&Document::new()).is_empty());
    }

    #[test]
    fn camera_maps_center_to_viewport_centre_with_y_flip() {
        let cam = Camera {
            center: Point2::new(0.0, 0.0),
            scale: 2.0,
            viewport: (800.0, 600.0),
        };
        // Centre of the world maps to the centre of the screen.
        assert_eq!(cam.world_to_screen(Point2::new(0.0, 0.0)), (400.0, 300.0));
        // +x is right; +y is *up*, i.e. a smaller screen-y.
        assert_eq!(cam.world_to_screen(Point2::new(10.0, 0.0)), (420.0, 300.0));
        assert_eq!(cam.world_to_screen(Point2::new(0.0, 10.0)), (400.0, 280.0));
    }

    #[test]
    fn camera_roundtrips_screen_and_world() {
        let cam = Camera {
            center: Point2::new(3.0, -7.0),
            scale: 1.75,
            viewport: (1280.0, 800.0),
        };
        let p = Point2::new(12.5, 4.25);
        let (sx, sy) = cam.world_to_screen(p);
        let back = cam.screen_to_world(sx, sy);
        assert!((back.x - p.x).abs() < 1e-9 && (back.y - p.y).abs() < 1e-9);
    }

    #[test]
    fn fit_centres_and_scales_to_extents() {
        let mut cam = Camera::new((1000.0, 1000.0));
        let bbox = Bbox {
            min: Point2::new(0.0, 0.0),
            max: Point2::new(100.0, 50.0),
        };
        cam.fit(bbox, 0.1); // 10% margin per side -> 800px usable of 1000.
        assert_eq!(cam.center, Point2::new(50.0, 25.0));
        // Width is the binding dimension: 800 usable / 100 world = 8.
        assert!((cam.scale - 8.0).abs() < 1e-9, "scale {}", cam.scale);
        // The extents fall inside the viewport after fitting.
        let (x0, _) = cam.world_to_screen(bbox.min);
        let (x1, _) = cam.world_to_screen(bbox.max);
        assert!(x0 >= 0.0 && x1 <= 1000.0);
    }

    #[test]
    fn fit_of_a_degenerate_box_only_recentres() {
        let mut cam = Camera::new((640.0, 480.0));
        let before = cam.scale;
        let p = Point2::new(9.0, 9.0);
        cam.fit(Bbox { min: p, max: p }, 0.05);
        assert_eq!(cam.center, p, "still recentres on the point");
        assert_eq!(cam.scale, before, "no infinite zoom on a zero-area box");
    }

    #[test]
    fn recording_renderer_captures_the_pipeline() {
        let doc = sample_doc();
        let list = build_drawlist(&doc);
        let mut cam = Camera::new((800.0, 600.0));
        cam.fit_document(&doc, 0.1);

        let mut renderer = RecordingRenderer::new();
        renderer.render(&cam, &list);
        renderer.render(&cam, &list);

        assert_eq!(renderer.frames.len(), 2);
        assert_eq!(renderer.total_commands(), 12);
        assert_eq!(renderer.last_camera, Some(cam));
    }
}
