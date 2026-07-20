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
//!   each tagged with its source layer so a backend can colour by layer.
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

use aadt_geometry::{Bbox, Point2};
use aadt_model::{Document, Geometry};

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

/// Translate a read-only [`Document`] snapshot into a [`DrawList`].
///
/// Entities are visited in the document's deterministic id order, so the same
/// document always yields byte-identical draw lists — a property the tests lean
/// on and a future frame-diff optimiser will too. Geometry maps one-to-one to a
/// primitive, except a [`Geometry::Insert`], which becomes a [`DrawCommand::Marker`]
/// at its insertion point (instantiating a block's own geometry, with rotation, is
/// a later increment; the marker keeps inserts visible meanwhile).
pub fn build_drawlist(doc: &Document) -> DrawList {
    let mut commands = Vec::with_capacity(doc.len());
    for e in doc.entities() {
        let layer = e.layer.clone();
        let cmd = match &e.geometry {
            Geometry::Line(s) => DrawCommand::Polyline {
                layer,
                points: vec![s.a, s.b],
                closed: false,
            },
            Geometry::Polyline(p) => DrawCommand::Polyline {
                layer,
                points: p.vertices.clone(),
                closed: p.closed,
            },
            Geometry::Circle(c) => DrawCommand::Circle {
                layer,
                center: c.center,
                radius: c.radius,
            },
            Geometry::Arc(a) => DrawCommand::Arc {
                layer,
                center: a.center,
                radius: a.radius,
                start_angle: a.start_angle,
                end_angle: a.end_angle,
            },
            Geometry::Text(t) => DrawCommand::Text {
                layer,
                position: t.position,
                height: t.height,
                rotation: t.rotation,
                value: t.value.clone(),
            },
            Geometry::Insert(ins) => DrawCommand::Marker {
                layer,
                position: ins.position,
                label: ins.name.clone(),
            },
        };
        commands.push(cmd);
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
    use aadt_model::{Insert, Text};

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
    fn insert_becomes_a_named_marker() {
        let mut doc = Document::new();
        doc.add_geometry(
            "A-DOOR",
            Geometry::Insert(Insert::new("DOOR-36", Point2::new(2.0, 2.0), 0.0)),
        );
        match &build_drawlist(&doc).commands[0] {
            DrawCommand::Marker {
                position, label, ..
            } => {
                assert_eq!(*position, Point2::new(2.0, 2.0));
                assert_eq!(label, "DOOR-36");
            }
            other => panic!("expected a marker, got {other:?}"),
        }
    }

    #[test]
    fn drawlist_is_deterministic() {
        let doc = sample_doc();
        assert_eq!(build_drawlist(&doc), build_drawlist(&doc));
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
