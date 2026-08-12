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
pub mod glyphs;
pub mod interact;
#[cfg(feature = "gui")]
pub mod view;

pub use glyphs::text_outline_segments;

use std::collections::HashMap;
use std::sync::LazyLock;

use aadt_geometry::{Bbox, Point2};
use aadt_model::{Dimension, DimensionKind, Document, Geometry, Hatch};

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
    /// Filled closed polygon (solid hatch). Drawn under strokes by backends.
    /// `alpha` is 0..1 (presentation fills often use ~0.35).
    Fill {
        layer: String,
        points: Vec<Point2>,
        alpha: f32,
    },
    /// Raster image footprint with source path (GUI may sample a texture).
    Image {
        layer: String,
        path: String,
        /// Quad corners in world space (typically 4).
        corners: Vec<Point2>,
        opacity: f32,
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
            | DrawCommand::Marker { layer, .. }
            | DrawCommand::Fill { layer, .. }
            | DrawCommand::Image { layer, .. } => layer,
        }
    }
}

/// An ordered batch of [`DrawCommand`]s: the frame a [`Renderer`] paints.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DrawList {
    pub commands: Vec<DrawCommand>,
    /// Parallel to `commands`: true = ghost underlay (dimmed, non-editable).
    pub underlay: Vec<bool>,
}

impl DrawList {
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    fn push(&mut self, cmd: DrawCommand, underlay: bool) {
        self.commands.push(cmd);
        self.underlay.push(underlay);
    }

    fn extend(&mut self, cmds: impl IntoIterator<Item = DrawCommand>, underlay: bool) {
        for cmd in cmds {
            self.push(cmd, underlay);
        }
    }
}

/// The placement an [`Insert`](aadt_model::Insert) applies to its block's
/// geometry: shift the block's `base` to the origin, scale, rotate by the insert
/// angle (degrees CCW), then translate to the insert position.
#[derive(Debug, Clone, Copy)]
struct Xform {
    base: Point2,
    rotation_deg: f64,
    position: Point2,
    scale_x: f64,
    scale_y: f64,
}

impl Xform {
    const IDENTITY: Xform = Xform {
        base: Point2::new(0.0, 0.0),
        rotation_deg: 0.0,
        position: Point2::new(0.0, 0.0),
        scale_x: 1.0,
        scale_y: 1.0,
    };

    /// Map a block-space point to world space.
    fn point(&self, p: Point2) -> Point2 {
        let (lx, ly) = (
            (p.x - self.base.x) * self.scale_x,
            (p.y - self.base.y) * self.scale_y,
        );
        let (s, c) = self.rotation_deg.to_radians().sin_cos();
        Point2::new(
            self.position.x + lx * c - ly * s,
            self.position.y + lx * s + ly * c,
        )
    }

    fn radius_scale(&self) -> f64 {
        (self.scale_x.abs() * self.scale_y.abs()).sqrt()
    }
}

/// Turn one piece of (possibly block-space) geometry into draw primitives.
/// Nested inserts expand recursively (depth-limited) when `doc` is provided.
fn push_primitive(
    out: &mut Vec<DrawCommand>,
    doc: &Document,
    geometry: &Geometry,
    xform: &Xform,
    layer: String,
    props: &aadt_model::DrawingProperties,
    depth: u32,
) {
    match geometry {
        Geometry::Line(s) => out.push(DrawCommand::Polyline {
            layer,
            points: vec![xform.point(s.a), xform.point(s.b)],
            closed: false,
        }),
        Geometry::Polyline(p) => out.push(DrawCommand::Polyline {
            layer,
            points: p.vertices.iter().map(|v| xform.point(*v)).collect(),
            closed: p.closed,
        }),
        Geometry::Circle(c) => out.push(DrawCommand::Circle {
            layer,
            center: xform.point(c.center),
            radius: c.radius * xform.radius_scale(),
        }),
        Geometry::Arc(a) => out.push(DrawCommand::Arc {
            layer,
            center: xform.point(a.center),
            radius: a.radius * xform.radius_scale(),
            start_angle: a.start_angle + xform.rotation_deg,
            end_angle: a.end_angle + xform.rotation_deg,
        }),
        Geometry::Text(t) => out.push(DrawCommand::Text {
            layer,
            position: xform.point(t.position),
            height: t.height * xform.radius_scale(),
            rotation: t.rotation + xform.rotation_deg,
            value: aadt_model::resolve_property_text(&t.value, props),
        }),
        Geometry::Insert(ins) => {
            if depth >= 8 {
                out.push(DrawCommand::Marker {
                    layer,
                    position: xform.point(ins.position),
                    label: ins.name.clone(),
                });
                return;
            }
            match doc.block(&ins.name) {
                Some(block) => {
                    for be in &block.entities {
                        let baked = be.geometry.instanced_scaled(
                            block.base,
                            ins.position,
                            ins.rotation,
                            ins.scale_x,
                            ins.scale_y,
                        );
                        let ly = instanced_layer(&be.layer, &layer);
                        push_primitive(out, doc, &baked, xform, ly, props, depth + 1);
                    }
                }
                None => out.push(DrawCommand::Marker {
                    layer,
                    position: xform.point(ins.position),
                    label: ins.name.clone(),
                }),
            }
        }
        Geometry::Image(img) => {
            let corners: Vec<Point2> = img.corners().iter().map(|p| xform.point(*p)).collect();
            out.push(DrawCommand::Image {
                layer: layer.clone(),
                path: img.path.clone(),
                corners: corners.clone(),
                opacity: img.opacity,
            });
            out.push(DrawCommand::Polyline {
                layer,
                points: corners,
                closed: true,
            });
        }
    }
}

/// Layer an instanced block entity draws on. Following the DXF convention,
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

/// Effective X/Y scales for drawing an INSERT: WIDTH param stretch + FLIP mirror.
fn insert_draw_scales(
    ins: &aadt_model::Insert,
    block: &aadt_model::Block,
    entity: &aadt_model::Entity,
) -> (f64, f64) {
    let mut sx = ins.scale_x;
    let mut sy = ins.scale_y;
    // WIDTH stretch: ratio of instance WIDTH attr to block default.
    if let (Some(def), Some(cur)) = (
        block
            .param(aadt_model::BLOCK_WIDTH_PARAM)
            .and_then(|p| p.default.parse::<f64>().ok())
            .filter(|w| *w > 1e-9),
        entity
            .attribute(aadt_model::BLOCK_WIDTH_PARAM)
            .and_then(|s| s.parse::<f64>().ok())
            .filter(|w| *w > 1e-9),
    ) {
        let ratio = cur / def;
        sx *= ratio;
        // Uniform width stretch keeps Y unless already non-uniform.
        if (ins.scale_x - ins.scale_y).abs() < 1e-9 {
            sy *= ratio;
        }
    }
    let flipped = entity
        .attribute(aadt_model::BLOCK_FLIP_ATTR)
        .or_else(|| entity.attribute(aadt_model::BLOCK_FLIP_PARAM))
        .map(|v| {
            v == "1"
                || v.eq_ignore_ascii_case("true")
                || v.eq_ignore_ascii_case("yes")
        })
        .unwrap_or(false);
    if flipped {
        // Mirror about local Y (negative X scale).
        sx = -sx;
    }
    (sx, sy)
}

/// Hatch boundaries as outlines; solid hatches also emit a [`DrawCommand::Fill`].
fn hatch_commands(hatch: &Hatch) -> Vec<DrawCommand> {
    let mut out = Vec::new();
    // Opacities from libraries/materials/catalog.json (L6); defaults if missing.
    let opac = aadt_library::material_opacities();
    let fill_alpha = if hatch.fill_kind == aadt_model::FillKind::Presentation && hatch.solid {
        opac.presentation_solid
    } else if hatch.solid {
        opac.technical_solid
    } else {
        opac.pattern_wash
    };
    // Always fill the outer path (pattern hatches get a light wash + strokes).
    if let Some(outer) = hatch.boundaries.first() {
        if outer.vertices.len() >= 3 {
            out.push(DrawCommand::Fill {
                layer: hatch.layer.clone(),
                points: outer.vertices.clone(),
                alpha: fill_alpha as f32,
            });
        }
        // Mute dense technical patterns on presentation fills (wash only).
        let mute_patterns = hatch.fill_kind == aadt_model::FillKind::Presentation && !hatch.solid;
        if !hatch.solid && !mute_patterns {
            out.extend(pattern_hatch_lines(hatch, outer));
        }
    }
    for b in &hatch.boundaries {
        out.push(DrawCommand::Polyline {
            layer: hatch.layer.clone(),
            points: b.vertices.clone(),
            closed: true,
        });
    }
    out
}

fn pattern_hatch_lines(hatch: &Hatch, outer: &aadt_geometry::Polyline) -> Vec<DrawCommand> {
    let xs: Vec<f64> = outer.vertices.iter().map(|p| p.x).collect();
    let ys: Vec<f64> = outer.vertices.iter().map(|p| p.y).collect();
    let (Some(min_x), Some(max_x), Some(min_y), Some(max_y)) = (
        xs.iter().cloned().reduce(f64::min),
        xs.iter().cloned().reduce(f64::max),
        ys.iter().cloned().reduce(f64::min),
        ys.iter().cloned().reduce(f64::max),
    ) else {
        return Vec::new();
    };
    let span = (max_x - min_x).max(max_y - min_y).max(1.0);
    // Faithful multi-family rendering for embedded .pat library patterns.
    if let Some(families) = embedded_pattern(&hatch.pattern) {
        return render_pat_families(hatch, outer, families, min_x, max_x, min_y, max_y);
    }
    let (angle_deg, density) = hatch_pattern_params(&hatch.pattern);
    let step = (span / density).clamp(0.05, span);
    let angle = angle_deg.to_radians();
    let (dir_x, dir_y) = (angle.cos(), angle.sin());
    let (nx, ny) = (-dir_y, dir_x);
    let mut cmds = Vec::new();
    let mid_x = (min_x + max_x) * 0.5;
    let mid_y = (min_y + max_y) * 0.5;
    // Offset primary family lines along the normal through the AABB extent.
    let mut o = -span * 1.5;
    while o < span * 1.5 {
        let cx = mid_x + nx * o;
        let cy = mid_y + ny * o;
        let a = Point2::new(cx - dir_x * span * 2.0, cy - dir_y * span * 2.0);
        let b = Point2::new(cx + dir_x * span * 2.0, cy + dir_y * span * 2.0);
        cmds.extend(clipped_hatch_run(hatch, outer, a, b));
        o += step;
    }
    // Cross/grid: second family offset independently along `dir` (not the same
    // normal offsets — otherwise every perpendicular stroke collapses).
    if hatch_pattern_is_cross(&hatch.pattern) {
        let mut o2 = -span * 1.5;
        while o2 < span * 1.5 {
            let cx = mid_x + dir_x * o2;
            let cy = mid_y + dir_y * o2;
            let a2 = Point2::new(cx - nx * span * 2.0, cy - ny * span * 2.0);
            let b2 = Point2::new(cx + nx * span * 2.0, cy + ny * span * 2.0);
            cmds.extend(clipped_hatch_run(hatch, outer, a2, b2));
            o2 += step;
        }
    }
    cmds
}

fn hatch_pattern_params(name: &str) -> (f64, f64) {
    let n = name.trim().to_ascii_uppercase();
    match n.as_str() {
        "ANSI31" | "ANSI37" => (45.0, 18.0),
        "ANSI32" | "STEEL" => (45.0, 12.0),
        "ANSI33" => (0.0, 16.0),
        "ANSI34" | "BRICK" => (0.0, 10.0),
        "AR-SAND" | "DOTS" | "GRAVEL" => (45.0, 28.0),
        "EARTH" | "GRASS" => (60.0, 14.0),
        "CROSS" | "ANSI38" | "NET" | "GRID" => (0.0, 14.0),
        "DASH" | "DASHED" => (0.0, 12.0),
        _ => (45.0, 18.0),
    }
}

fn hatch_pattern_is_cross(name: &str) -> bool {
    matches!(
        name.trim().to_ascii_uppercase().as_str(),
        "CROSS" | "ANSI38" | "NET" | "GRID"
    )
}

// --- Embedded .pat pattern library ------------------------------------------

/// One line family of a hatch pattern (AutoCAD `.pat` semantics): a set of
/// parallel lines at `angle_deg`, offset by `delta_perp` perpendicular spacing
/// and `delta_along` stagger along the line. Dash arrays are parsed but not yet
/// rendered (solid approximation — see M9 hatch notes).
#[derive(Debug, Clone)]
struct PatFamily {
    angle_deg: f64,
    delta_along: f64,
    delta_perp: f64,
}

/// Parse a `.pat` text (possibly many `*NAME` blocks) into families by name.
fn parse_pat_families(text: &str) -> HashMap<String, Vec<PatFamily>> {
    let mut map = HashMap::new();
    let mut current: Option<(String, Vec<PatFamily>)> = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        if let Some(rest) = line.strip_prefix('*') {
            if let Some((name, fams)) = current.take() {
                map.insert(name, fams);
            }
            let name = rest
                .split(',')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_uppercase();
            current = Some((name, Vec::new()));
        } else if let Some((_, fams)) = current.as_mut() {
            let nums: Vec<f64> = line
                .split(',')
                .filter_map(|s| s.trim().parse::<f64>().ok())
                .collect();
            // angle, x-base, y-base, delta-x, delta-y [, dashes...]
            if nums.len() >= 5 {
                fams.push(PatFamily {
                    angle_deg: nums[0],
                    delta_along: nums[3],
                    delta_perp: nums[4],
                });
            }
        }
    }
    if let Some((name, fams)) = current.take() {
        map.insert(name, fams);
    }
    map
}

/// The embedded pattern library (parsed once). Source: `libraries/hatches/*.pat`.
static PATTERN_LIB: LazyLock<HashMap<String, Vec<PatFamily>>> =
    LazyLock::new(|| parse_pat_families(include_str!("hatch_patterns.pat")));

/// Line families for a named pattern, if it is in the embedded library.
fn embedded_pattern(name: &str) -> Option<&'static [PatFamily]> {
    PATTERN_LIB
        .get(name.trim().to_ascii_uppercase().as_str())
        .filter(|v| !v.is_empty())
        .map(|v| v.as_slice())
}

/// Render each `.pat` family as parallel lines at its native spacing, clipped to
/// the boundary. Perpendicular spacing is `|delta_perp|`; line count is capped so
/// dense patterns over large regions stay bounded.
fn render_pat_families(
    hatch: &Hatch,
    outer: &aadt_geometry::Polyline,
    families: &[PatFamily],
    min_x: f64,
    max_x: f64,
    min_y: f64,
    max_y: f64,
) -> Vec<DrawCommand> {
    const MAX_LINES_PER_FAMILY: f64 = 600.0;
    let diag = ((max_x - min_x).powi(2) + (max_y - min_y).powi(2))
        .sqrt()
        .max(1.0);
    let mid = Point2::new((min_x + max_x) * 0.5, (min_y + max_y) * 0.5);
    let mut cmds = Vec::new();
    for fam in families {
        let mut spacing = fam.delta_perp.abs();
        if spacing < 1e-6 {
            continue; // degenerate family (coincident lines)
        }
        // Keep the line count bounded by widening spacing if needed.
        if diag / spacing > MAX_LINES_PER_FAMILY {
            spacing = diag / MAX_LINES_PER_FAMILY;
        }
        let a = fam.angle_deg.to_radians();
        let (dx, dy) = (a.cos(), a.sin());
        let (nx, ny) = (-dy, dx);
        let half = (diag / spacing).ceil() as i64 + 2;
        for k in -half..=half {
            let step = k as f64;
            let ox = mid.x + nx * (step * spacing) + dx * (step * fam.delta_along);
            let oy = mid.y + ny * (step * spacing) + dy * (step * fam.delta_along);
            let pa = Point2::new(ox - dx * diag, oy - dy * diag);
            let pb = Point2::new(ox + dx * diag, oy + dy * diag);
            cmds.extend(clipped_hatch_run(hatch, outer, pa, pb));
        }
    }
    cmds
}

fn point_in_hatch_fill(hatch: &Hatch, outer: &aadt_geometry::Polyline, p: Point2) -> bool {
    if !outer.contains_point(p) {
        return false;
    }
    // Punch out islands (boundaries after the outer loop).
    !hatch
        .boundaries
        .iter()
        .skip(1)
        .any(|island| island.contains_point(p))
}

/// Clip the infinite line through `a`→`b` to the hatch fill (outer boundary minus
/// islands) **analytically**: intersect the line with every boundary edge, sort
/// the crossing parameters, and keep each sub-span whose midpoint is inside the
/// fill. Exact regardless of how thin the region is or how dense the pattern —
/// unlike fixed-step sampling, which dropped short crossings of thin regions.
fn clipped_hatch_run(
    hatch: &Hatch,
    outer: &aadt_geometry::Polyline,
    a: Point2,
    b: Point2,
) -> Vec<DrawCommand> {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    if dx * dx + dy * dy < 1e-12 {
        return Vec::new();
    }
    // Crossing parameters (t along a→b) with every boundary edge, outer + islands.
    let mut ts: Vec<f64> = Vec::new();
    for ring in &hatch.boundaries {
        collect_ring_crossings(ring, a, dx, dy, &mut ts);
    }
    if ts.len() < 2 {
        return Vec::new();
    }
    ts.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));

    let mut cmds = Vec::new();
    for w in ts.windows(2) {
        let (t0, t1) = (w[0], w[1]);
        if t1 - t0 < 1e-9 {
            continue; // zero-length span (e.g. a crossing through a vertex)
        }
        let tm = 0.5 * (t0 + t1);
        let mid = Point2::new(a.x + dx * tm, a.y + dy * tm);
        if point_in_hatch_fill(hatch, outer, mid) {
            cmds.push(DrawCommand::Polyline {
                layer: hatch.layer.clone(),
                points: vec![
                    Point2::new(a.x + dx * t0, a.y + dy * t0),
                    Point2::new(a.x + dx * t1, a.y + dy * t1),
                ],
                closed: false,
            });
        }
    }
    cmds
}

/// Append the parameters `t` (along the line `a + t·(dx,dy)`) at which the line
/// crosses each edge of `ring`, treated as a closed loop.
fn collect_ring_crossings(
    ring: &aadt_geometry::Polyline,
    a: Point2,
    dx: f64,
    dy: f64,
    out: &mut Vec<f64>,
) {
    let v = &ring.vertices;
    let n = v.len();
    if n < 2 {
        return;
    }
    for i in 0..n {
        let p = v[i];
        let q = v[(i + 1) % n];
        let (ex, ey) = (q.x - p.x, q.y - p.y);
        // Solve a + t·d = p + s·e for t (line param) and s (edge param, [0,1]).
        let denom = dx * ey - dy * ex;
        if denom.abs() < 1e-12 {
            continue; // parallel to this edge
        }
        let (difx, dify) = (p.x - a.x, p.y - a.y);
        let s = (difx * dy - dify * dx) / denom;
        if (0.0..=1.0).contains(&s) {
            out.push((difx * ey - dify * ex) / denom);
        }
    }
}

/// A [`Dimension`] flattened to AutoCAD-like draw primitives: extension lines,
/// dimension line, arrowheads, and value text (oriented along the dim line for
/// linear/aligned). `dim_scale` / `dim_text_height` size the glyphs.
///
/// Public so front-ends can hit-test / highlight a dimension against exactly the
/// geometry that is drawn, rather than re-deriving it.
pub fn dimension_commands(
    dim: &Dimension,
    props: &aadt_model::DrawingProperties,
) -> Vec<DrawCommand> {
    let label = aadt_model::format_dimension_label_ex(
        dim.kind,
        dim.measurement,
        &dim.text,
        &props.units,
        &props.dim_secondary,
        &props.dim_precision,
        &props.dim_fraction,
    );
    let dim_scale = props
        .dim_scale
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|s| *s > 0.0)
        .unwrap_or(1.0);

    match dim.kind {
        DimensionKind::Linear | DimensionKind::Aligned => {
            aligned_dimension_commands(dim, props, &label, dim_scale)
        }
        DimensionKind::Angular => {
            let scale_ref = dim.vertex.distance(dim.p1).max(dim.vertex.distance(dim.p2));
            let text_height = dim_text_height(props, scale_ref, dim_scale);
            let mut out = vec![
                poly_open(&dim.layer, vec![dim.vertex, dim.p1]),
                poly_open(&dim.layer, vec![dim.vertex, dim.p2]),
            ];
            out.push(DrawCommand::Text {
                layer: dim.layer.clone(),
                position: dim.text_anchor,
                height: text_height,
                rotation: 0.0,
                value: label,
            });
            out
        }
        DimensionKind::Radius => {
            let scale_ref = dim.vertex.distance(dim.p1);
            let text_height = dim_text_height(props, scale_ref, dim_scale);
            let mut out = vec![poly_open(&dim.layer, vec![dim.vertex, dim.p1])];
            // Arrow at the rim point, pointing outward from centre.
            if let Some(u) = (dim.p1 - dim.vertex).normalized() {
                out.push(arrowhead(
                    &dim.layer,
                    dim.p1,
                    u,
                    text_height * 0.45 * dim_scale,
                ));
            }
            out.push(DrawCommand::Text {
                layer: dim.layer.clone(),
                position: dim.text_anchor,
                height: text_height,
                rotation: 0.0,
                value: label,
            });
            out
        }
        DimensionKind::Diameter => {
            let scale_ref = dim.p1.distance(dim.p2);
            let text_height = dim_text_height(props, scale_ref, dim_scale);
            let mut out = vec![poly_open(&dim.layer, vec![dim.p1, dim.p2])];
            if let Some(u) = (dim.p2 - dim.p1).normalized() {
                let asz = text_height * 0.45 * dim_scale;
                out.push(arrowhead(&dim.layer, dim.p1, u * -1.0, asz));
                out.push(arrowhead(&dim.layer, dim.p2, u, asz));
            }
            out.push(DrawCommand::Text {
                layer: dim.layer.clone(),
                position: dim.text_anchor,
                height: text_height,
                rotation: 0.0,
                value: label,
            });
            out
        }
        DimensionKind::OrdinateX | DimensionKind::OrdinateY => {
            let scale_ref = dim.p1.distance(dim.p2).max(1.0);
            let text_height = dim_text_height(props, scale_ref, dim_scale);
            let mut out = vec![poly_open(&dim.layer, vec![dim.p1, dim.p2])];
            out.push(DrawCommand::Text {
                layer: dim.layer.clone(),
                position: dim.text_anchor,
                height: text_height,
                rotation: 0.0,
                value: label,
            });
            out
        }
    }
}

fn dim_text_height(props: &aadt_model::DrawingProperties, scale_ref: f64, dim_scale: f64) -> f64 {
    props
        .dim_text_height
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|h| *h > 0.0)
        .unwrap_or_else(|| (scale_ref * DIM_TEXT_FRACTION).max(f64::MIN_POSITIVE))
        * dim_scale.max(f64::MIN_POSITIVE)
}

fn poly_open(layer: &str, points: Vec<Point2>) -> DrawCommand {
    DrawCommand::Polyline {
        layer: layer.to_string(),
        points,
        closed: false,
    }
}

/// Filled-looking arrowhead as a small closed triangle at `tip`, pointing along `dir`.
fn arrowhead(layer: &str, tip: Point2, dir: aadt_geometry::Vec2, size: f64) -> DrawCommand {
    let u = dir
        .normalized()
        .unwrap_or(aadt_geometry::Vec2::new(1.0, 0.0));
    let n = u.perp();
    let base = tip + u * (-size);
    let left = base + n * (size * 0.35);
    let right = base + n * (-size * 0.35);
    DrawCommand::Polyline {
        layer: layer.to_string(),
        points: vec![tip, left, right],
        closed: true,
    }
}

/// AutoCAD-style aligned/linear: extension lines, dim line offset to `text_anchor`,
/// arrows, text above the dim line rotated to match.
fn aligned_dimension_commands(
    dim: &Dimension,
    props: &aadt_model::DrawingProperties,
    label: &str,
    dim_scale: f64,
) -> Vec<DrawCommand> {
    let span = dim.p1.distance(dim.p2);
    let text_height = dim_text_height(props, span.max(1.0), 1.0); // height before overall scale
    let text_height = text_height * dim_scale;
    let asz = (text_height * 0.45).max(span * 0.02 * dim_scale);
    let exo = (text_height * 0.2).max(asz * 0.25); // gap from definition point
    let exe = (text_height * 0.2).max(asz * 0.25); // past dim line

    let Some(u) = (dim.p2 - dim.p1).normalized() else {
        return vec![DrawCommand::Text {
            layer: dim.layer.clone(),
            position: dim.text_anchor,
            height: text_height.max(f64::MIN_POSITIVE),
            rotation: 0.0,
            value: label.to_string(),
        }];
    };
    let n = u.perp();
    let mut offset = (dim.text_anchor - dim.p1).dot(n);
    if offset.abs() < span * 1e-6 {
        // No third point / coincident with span — park the dim line above.
        offset = (text_height * 2.5).max(asz * 3.0);
    }
    let ext_dir = if offset >= 0.0 {
        n
    } else {
        aadt_geometry::Vec2::new(-n.x, -n.y)
    };
    let abs_off = offset.abs();

    let d1 = dim.p1 + n * offset;
    let d2 = dim.p2 + n * offset;
    let e1_start = dim.p1 + ext_dir * exo;
    let e1_end = dim.p1 + ext_dir * (abs_off + exe);
    let e2_start = dim.p2 + ext_dir * exo;
    let e2_end = dim.p2 + ext_dir * (abs_off + exe);

    let mut out = vec![
        poly_open(&dim.layer, vec![e1_start, e1_end]),
        poly_open(&dim.layer, vec![e2_start, e2_end]),
        poly_open(&dim.layer, vec![d1, d2]),
        arrowhead(&dim.layer, d1, u * -1.0, asz),
        arrowhead(&dim.layer, d2, u, asz),
    ];

    // Text sits just above the dim line (toward the side opposite the defs when
    // offset is positive — i.e. further along ext_dir).
    let mid = d1.midpoint(d2);
    let text_pos = mid + ext_dir * (text_height * 0.65);
    let mut rot = (dim.p2.y - dim.p1.y)
        .atan2(dim.p2.x - dim.p1.x)
        .to_degrees();
    // Keep text readable (never upside-down).
    if rot > 90.0 {
        rot -= 180.0;
    } else if rot < -90.0 {
        rot += 180.0;
    }
    out.push(DrawCommand::Text {
        layer: dim.layer.clone(),
        position: text_pos,
        height: text_height.max(f64::MIN_POSITIVE),
        rotation: rot,
        value: label.to_string(),
    });
    out
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
/// dimension line plus value text (see [`hatch_commands`], [`dimension_commands`]).
/// Build the draw list with floor trace underlays (ArchiCAD-style).
///
/// Pass order: floor below (if traced) → floor above (if traced) → other
/// categories on the active floor (soft underlay) → active model space (full).
pub fn build_drawlist(doc: &Document) -> DrawList {
    build_drawlist_with_trace(doc)
}

/// Explicit entry point for the floor/category draw pipeline.
pub fn build_drawlist_with_trace(doc: &Document) -> DrawList {
    let mut list = DrawList::default();
    let active = doc.active_space();
    let active_floor = doc.active_floor_id();

    let push_space = |list: &mut DrawList, space: aadt_model::ModelSpaceId, underlay: bool| {
        append_space_commands(list, doc, space, underlay);
    };

    if let Some(floor) = active_floor {
        if doc.trace_below() {
            if let Some(below) = doc.floor_below(floor) {
                for s in doc.spaces_on_floor(below) {
                    push_space(&mut list, s.id, true);
                }
            }
        }
        if doc.trace_above() {
            if let Some(above) = doc.floor_above(floor) {
                for s in doc.spaces_on_floor(above) {
                    push_space(&mut list, s.id, true);
                }
            }
        }
        if let Some(extra) = doc.trace_floor() {
            if extra != floor {
                for s in doc.spaces_on_floor(extra) {
                    push_space(&mut list, s.id, true);
                }
            }
        }
        // Same-floor context (non-active categories) as a soft underlay.
        for s in doc.spaces_on_floor(floor) {
            if s.id != active {
                push_space(&mut list, s.id, true);
            }
        }
    }

    push_space(&mut list, active, false);
    list
}

fn append_space_commands(
    list: &mut DrawList,
    doc: &Document,
    space: aadt_model::ModelSpaceId,
    underlay: bool,
) {
    let props = doc.properties();
    for e in doc.entities_in_space(space) {
        if !doc.layer_shown(&e.layer) {
            continue;
        }
        match &e.geometry {
            Geometry::Insert(ins) => match doc.block(&ins.name) {
                Some(block) => {
                    let (sx, sy) = insert_draw_scales(ins, block, e);
                    let xform = Xform {
                        base: block.base,
                        rotation_deg: ins.rotation,
                        position: ins.position,
                        scale_x: sx,
                        scale_y: sy,
                    };
                    let vis = e
                        .attribute(aadt_model::BLOCK_VIS_ATTR)
                        .unwrap_or("Standard");
                    let hidden = block.hidden_for(vis);
                    for (i, be) in block.entities.iter().enumerate() {
                        if hidden.contains(&i) {
                            continue;
                        }
                        let layer = instanced_layer(&be.layer, &e.layer);
                        let mut cmds = Vec::new();
                        push_primitive(&mut cmds, doc, &be.geometry, &xform, layer, props, 1);
                        list.extend(cmds, underlay);
                    }
                }
                None => list.push(
                    DrawCommand::Marker {
                        layer: e.layer.clone(),
                        position: ins.position,
                        label: ins.name.clone(),
                    },
                    underlay,
                ),
            },
            other => {
                let mut cmds = Vec::new();
                push_primitive(
                    &mut cmds,
                    doc,
                    other,
                    &Xform::IDENTITY,
                    e.layer.clone(),
                    props,
                    0,
                );
                list.extend(cmds, underlay);
            }
        }
    }
    for hatch in doc.hatches() {
        if hatch.space == space && doc.layer_shown(&hatch.layer) {
            list.extend(hatch_commands(hatch), underlay);
        }
    }
    for dim in doc.dimensions() {
        if dim.space == space && doc.layer_shown(&dim.layer) {
            list.extend(dimension_commands(dim, doc.properties()), underlay);
        }
    }
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
            space: Default::default(),
            layer: "A-DIMS".to_string(),
            kind: DimensionKind::Linear,
            p1: Point2::new(0.0, 0.0),
            p2: Point2::new(10.0, 0.0),
            vertex: Point2::new(0.0, 0.0),
            text_anchor: Point2::new(5.0, -1.0),
            text: String::new(),
            measurement: 10.0,
        });

        let list = build_drawlist(&doc);
        // 1 entity + 1 hatch + dim glyphs (extensions, dim line, arrows, text).
        assert!(
            list.len() >= 4,
            "expected entity + hatch + dimension primitives, got {}",
            list.len()
        );

        // Pattern hatch (ANSI31) emits diagonal strokes + closed boundary.
        let hatch = list
            .commands
            .iter()
            .find(|c| {
                matches!(
                    c,
                    DrawCommand::Polyline {
                        layer,
                        closed: true,
                        points,
                        ..
                    } if layer == "A-FLOR" && points.len() == 4
                )
            })
            .expect("hatch boundary present");
        assert!(matches!(
            hatch,
            DrawCommand::Polyline { closed: true, points, .. } if points.len() == 4
        ));
        assert!(
            list.commands
                .iter()
                .any(|c| matches!(c, DrawCommand::Polyline { layer, closed: false, .. } if layer == "A-FLOR")),
            "pattern hatch should emit diagonal strokes"
        );

        // The dimension renders as a span line plus its measured value as text.
        let dim_text = list.commands.iter().find_map(|c| match c {
            DrawCommand::Text { layer, value, .. } if layer == "A-DIMS" => Some(value.clone()),
            _ => None,
        });
        assert_eq!(
            dim_text.as_deref(),
            Some("10\""),
            "empty override → formatted measurement (default ft-in inches)"
        );
    }

    #[test]
    fn cross_hatch_emits_both_stroke_families() {
        let mut doc = Document::new();
        doc.add_hatch(Hatch::new(
            "A-FLOR",
            Polyline::new(
                vec![
                    Point2::new(0.0, 0.0),
                    Point2::new(10.0, 0.0),
                    Point2::new(10.0, 10.0),
                    Point2::new(0.0, 10.0),
                ],
                true,
            ),
            "CROSS",
        ));
        let list = build_drawlist(&doc);
        let strokes: Vec<_> = list
            .commands
            .iter()
            .filter(|c| {
                matches!(
                    c,
                    DrawCommand::Polyline {
                        layer,
                        closed: false,
                        points,
                        ..
                    } if layer == "A-FLOR" && points.len() >= 2
                )
            })
            .collect();
        assert!(
            strokes.len() >= 4,
            "CROSS should emit multiple H and V strokes, got {}",
            strokes.len()
        );
        let mut has_h = false;
        let mut has_v = false;
        for c in &strokes {
            if let DrawCommand::Polyline { points, .. } = c {
                let dx = (points[1].x - points[0].x).abs();
                let dy = (points[1].y - points[0].y).abs();
                if dy < 1e-6 && dx > 1e-6 {
                    has_h = true;
                }
                if dx < 1e-6 && dy > 1e-6 {
                    has_v = true;
                }
            }
        }
        assert!(
            has_h && has_v,
            "CROSS needs both horizontal and vertical runs"
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

    #[test]
    fn visibility_hides_block_entities() {
        use aadt_model::BLOCK_VIS_ATTR;
        use std::collections::HashMap;

        let mut doc = Document::new();
        let mut block = Block::new("WIN", Point2::new(0.0, 0.0));
        block.push(
            "0",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(1.0, 0.0))),
        );
        block.push(
            "0",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(0.0, 1.0))),
        );
        block.set_visibility("Swing", vec![1]);
        doc.add_block(block);

        let mut attrs = HashMap::new();
        attrs.insert(BLOCK_VIS_ATTR.to_string(), "Swing".to_string());
        doc.add_entity(
            "0",
            Geometry::Insert(Insert::new("WIN", Point2::new(0.0, 0.0), 0.0)),
            attrs,
        );
        let list = build_drawlist(&doc);
        assert_eq!(list.len(), 1, "one of two block ents hidden");
    }

    #[test]
    fn embedded_pat_library_parses_expected_patterns() {
        // Two-family grid at 0°/90°, 600 mm spacing.
        let grid = embedded_pattern("FLOOR-600-GRID").expect("grid pattern present");
        assert_eq!(grid.len(), 2);
        assert!(grid
            .iter()
            .any(|f| f.angle_deg == 0.0 && (f.delta_perp - 600.0).abs() < 1e-9));
        assert!(grid.iter().any(|f| f.angle_deg == 90.0));
        // Crosshatch at 45°/135°.
        let block = embedded_pattern("ar-conc-block").expect("case-insensitive lookup");
        assert_eq!(block.len(), 2);
        assert!(block.iter().any(|f| f.angle_deg == 45.0));
        assert!(block.iter().any(|f| f.angle_deg == 135.0));
        // Single-family diagonal.
        assert_eq!(embedded_pattern("AR-METAL-LINES").unwrap().len(), 1);
        // Unknown name falls through to the legacy param model.
        assert!(embedded_pattern("NOT-A-PATTERN").is_none());
    }

    #[test]
    fn pat_pattern_renders_multiple_families() {
        // 3 m square region hatched with the 600 mm grid → several strokes per family.
        let outer = Polyline::new(
            vec![
                Point2::new(0.0, 0.0),
                Point2::new(3000.0, 0.0),
                Point2::new(3000.0, 3000.0),
                Point2::new(0.0, 3000.0),
            ],
            true,
        );
        let hatch = Hatch::new("0", outer.clone(), "FLOOR-600-GRID");
        let cmds = pattern_hatch_lines(&hatch, &outer);
        let lines = cmds
            .iter()
            .filter(|c| matches!(c, DrawCommand::Polyline { .. }))
            .count();
        // Two families of ~5 lines across 3 m at 600 mm spacing.
        assert!(lines >= 6, "expected grid pattern lines, got {lines}");
    }

    #[test]
    fn thin_region_still_renders_diagonal_hatch_lines() {
        // A very thin, wide rectangle (aspect ~20:1) — the case where fixed-step
        // sampling dropped nearly every 45° crossing and the fill read as solid.
        // Analytic clipping must still emit many diagonal strokes.
        let outer = Polyline::new(
            vec![
                Point2::new(0.0, 0.0),
                Point2::new(2000.0, 0.0),
                Point2::new(2000.0, 100.0),
                Point2::new(0.0, 100.0),
            ],
            true,
        );
        let hatch = Hatch::new("0", outer.clone(), "ANSI31");
        let cmds = pattern_hatch_lines(&hatch, &outer);
        // Count the diagonal strokes (open 2-point polylines) that lie inside.
        let strokes = cmds
            .iter()
            .filter(|c| matches!(c, DrawCommand::Polyline { closed: false, points, .. } if points.len() == 2))
            .count();
        // Old fixed-step sampling dropped nearly every crossing here (≈0 strokes);
        // analytic clipping keeps the full family that crosses the strip.
        assert!(
            strokes >= 8,
            "thin ANSI31 region should still show diagonal hatching, got {strokes} strokes"
        );
    }

    #[test]
    fn clipped_run_matches_a_known_rectangle_crossing() {
        // Horizontal line across a unit square from x=-1..2 at y=0.5 clips to x∈[0,1].
        let outer = Polyline::new(
            vec![
                Point2::new(0.0, 0.0),
                Point2::new(1.0, 0.0),
                Point2::new(1.0, 1.0),
                Point2::new(0.0, 1.0),
            ],
            true,
        );
        let hatch = Hatch::new("0", outer.clone(), "ANSI31");
        let cmds = clipped_hatch_run(
            &hatch,
            &outer,
            Point2::new(-1.0, 0.5),
            Point2::new(2.0, 0.5),
        );
        assert_eq!(cmds.len(), 1);
        if let DrawCommand::Polyline { points, .. } = &cmds[0] {
            assert!((points[0].x - 0.0).abs() < 1e-9 && (points[1].x - 1.0).abs() < 1e-9);
        } else {
            panic!("expected a clipped polyline");
        }
    }
}
