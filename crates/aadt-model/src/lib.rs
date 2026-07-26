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

use std::collections::{BTreeMap, HashMap};

use aadt_geometry::{Arc, Bbox, Circle, Point2, Polyline, Segment, Vec2};

pub mod floors;
pub mod units;
pub use floors::{
    floors_sorted, DefaultSpaces, Floor, LevelId, ModelSpace, ModelSpaceId, SpaceCategory,
};
pub use units::{
    format_dimension_label, format_length, format_length_pair, parse_length, primary_from_props,
    secondary_enabled, UnitSystem, INCHES_PER_METER, METERS_PER_INCH,
};

/// Stable identifier for an entity within a [`Document`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntityId(pub u64);

/// The default AutoCAD Color Index (ACI) — 7, white/black — a fresh layer takes.
pub const DEFAULT_LAYER_COLOR: u8 = 7;

/// Default plot lineweight in millimetres (ISO 0.25 mm).
pub const DEFAULT_LINEWEIGHT_MM: f32 = 0.25;

/// DXF / plot linetype for a layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineType {
    #[default]
    Continuous,
    Dashed,
    Center,
    Hidden,
    Phantom,
    Dot,
}

impl LineType {
    /// DXF linetype name (uppercase).
    pub fn dxf_name(self) -> &'static str {
        match self {
            LineType::Continuous => "CONTINUOUS",
            LineType::Dashed => "DASHED",
            LineType::Center => "CENTER",
            LineType::Hidden => "HIDDEN",
            LineType::Phantom => "PHANTOM",
            LineType::Dot => "DOT",
        }
    }

    /// Parse a DXF or user token (case-insensitive).
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_uppercase().as_str() {
            "CONTINUOUS" | "CONT" => Some(LineType::Continuous),
            "DASHED" | "DASH" | "HIDDEN2" | "DASHED2" => Some(LineType::Dashed),
            "CENTER" | "CENTRE" | "CENTER2" => Some(LineType::Center),
            "HIDDEN" => Some(LineType::Hidden),
            "PHANTOM" | "PHANTOM2" => Some(LineType::Phantom),
            "DOT" | "DOTTED" => Some(LineType::Dot),
            _ => None,
        }
    }
}

/// A drawing layer. Layer names carry the convention hints AADT detection uses
/// (e.g. `A-WALL`), so they are preserved verbatim from the source file. A layer
/// also carries display/plot state — ACI `color`, `linetype`, `lineweight_mm`,
/// visibility, freeze, and lock — so standards templates and DXF round-trip.
#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    pub name: String,
    /// AutoCAD Color Index (1–255); [`DEFAULT_LAYER_COLOR`] by default.
    pub color: u8,
    /// Plot / screen linetype.
    pub linetype: LineType,
    /// Plot lineweight in millimetres (ISO pen sizes).
    pub lineweight_mm: f32,
    /// A layer that is off is not drawn.
    pub visible: bool,
    /// A frozen layer is neither drawn nor regenerated.
    pub frozen: bool,
    /// A locked layer cannot be selected or edited (drawn if on).
    pub locked: bool,
}

impl Layer {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            color: DEFAULT_LAYER_COLOR,
            linetype: LineType::Continuous,
            lineweight_mm: DEFAULT_LINEWEIGHT_MM,
            visible: true,
            frozen: false,
            locked: false,
        }
    }

    /// Builder: set the ACI colour.
    pub fn with_color(mut self, color: u8) -> Self {
        self.color = color;
        self
    }

    /// Builder: set linetype.
    pub fn with_linetype(mut self, linetype: LineType) -> Self {
        self.linetype = linetype;
        self
    }

    /// Builder: set lineweight in mm.
    pub fn with_lineweight_mm(mut self, mm: f32) -> Self {
        self.lineweight_mm = mm;
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
    /// X scale factor (DXF group 41); default 1.
    pub scale_x: f64,
    /// Y scale factor (DXF group 42); default 1.
    pub scale_y: f64,
}

impl Insert {
    pub fn new(name: impl Into<String>, position: Point2, rotation: f64) -> Self {
        Self {
            name: name.into(),
            position,
            rotation,
            scale_x: 1.0,
            scale_y: 1.0,
        }
    }

    pub fn with_scale(mut self, scale_x: f64, scale_y: f64) -> Self {
        self.scale_x = if scale_x.abs() < 1e-12 { 1.0 } else { scale_x };
        self.scale_y = if scale_y.abs() < 1e-12 { 1.0 } else { scale_y };
        self
    }

    /// Same name/scales with a new placement.
    pub fn at(&self, position: Point2, rotation: f64) -> Self {
        Self {
            name: self.name.clone(),
            position,
            rotation,
            scale_x: self.scale_x,
            scale_y: self.scale_y,
        }
    }
}

/// Prefix applied to layers brought in from DXF/DWG so they stay distinct from
/// AADT AIA layers. Example: `A-WALL` → `ORG-DWG_LAYER-A-WALL`.
pub const IMPORTED_LAYER_PREFIX: &str = "ORG-DWG_LAYER-";

/// Strip AutoCAD MTEXT / TEXT formatting so only drawable glyphs remain.
///
/// Codes like `\fArial|b0|i0|c0|p34;`, `\H2.5;`, `\C1;`, braces, and `\P`
/// paragraph marks are removed — the app always renders with its own sans font.
pub fn sanitize_display_text(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' && i + 1 < chars.len() {
            let cmd = chars[i + 1];
            i += 2;
            match cmd {
                'P' | 'p' => out.push('\n'),
                '~' => out.push(' '),
                '\\' | '{' | '}' => out.push(cmd),
                // Font / height / width / tracking / oblique / color / align / stack:
                // consume through the terminating `;`.
                'F' | 'f' | 'H' | 'h' | 'W' | 'w' | 'T' | 't' | 'Q' | 'q' | 'C' | 'c' | 'A'
                | 'a' | 'S' | 's' => {
                    while i < chars.len() && chars[i] != ';' {
                        i += 1;
                    }
                    if i < chars.len() {
                        i += 1;
                    }
                }
                // Unicode glyph `\U+00B0` (optional trailing `;`).
                'U' | 'u' => {
                    if i < chars.len() && chars[i] == '+' {
                        i += 1;
                        let mut hex = String::new();
                        while i < chars.len() && hex.len() < 8 && chars[i].is_ascii_hexdigit() {
                            hex.push(chars[i]);
                            i += 1;
                        }
                        if i < chars.len() && chars[i] == ';' {
                            i += 1;
                        }
                        if let Ok(cp) = u32::from_str_radix(&hex, 16) {
                            if let Some(ch) = char::from_u32(cp) {
                                out.push(ch);
                            }
                        }
                    }
                }
                // Single-letter toggles (underline / overline / strike) — no `;`.
                'L' | 'l' | 'O' | 'o' | 'K' | 'k' => {}
                _ => {
                    // Unknown escape: skip until `;` if present, else drop the cmd letter only.
                    let start = i;
                    while i < chars.len() && chars[i] != ';' && chars[i] != '\\' && chars[i] != '{'
                    {
                        i += 1;
                        if i - start > 64 {
                            break;
                        }
                    }
                    if i < chars.len() && chars[i] == ';' {
                        i += 1;
                    }
                }
            }
            continue;
        }
        if c == '{' || c == '}' {
            i += 1;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out.trim().to_string()
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
            value: sanitize_display_text(&value.into()),
            position,
            height: height.max(0.01),
            rotation,
        }
    }
}

/// Kind of a parametric block parameter (string tag or numeric value).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockParamKind {
    String,
    Number,
}

impl BlockParamKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Number => "number",
        }
    }

    pub fn parse(s: &str) -> Self {
        if s.eq_ignore_ascii_case("number") || s.eq_ignore_ascii_case("num") {
            Self::Number
        } else {
            Self::String
        }
    }
}

/// Named parameter on a block definition (parametric lite — not AutoCAD dyn-block).
#[derive(Debug, Clone, PartialEq)]
pub struct BlockParam {
    pub name: String,
    pub default: String,
    pub kind: BlockParamKind,
}

/// Visibility state: which definition-entity indices are hidden when active.
#[derive(Debug, Clone, PartialEq)]
pub struct VisibilityState {
    pub name: String,
    /// Indices into [`Block::entities`] that are not drawn for this state.
    pub hidden: Vec<usize>,
}

/// Attribute key on an INSERT for the active visibility state name.
pub const BLOCK_VIS_ATTR: &str = "VISIBILITY";

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
/// Optional `params` / `visibilities` support parametric-lite tagging (e.g. `WNAME`)
/// and show/hide states without geometry morphing.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub name: String,
    pub base: Point2,
    pub entities: Vec<BlockEntity>,
    pub params: Vec<BlockParam>,
    pub visibilities: Vec<VisibilityState>,
}

impl Block {
    pub fn new(name: impl Into<String>, base: Point2) -> Self {
        Self {
            name: name.into(),
            base,
            entities: Vec::new(),
            params: Vec::new(),
            visibilities: vec![VisibilityState {
                name: "Standard".into(),
                hidden: Vec::new(),
            }],
        }
    }

    pub fn push(&mut self, layer: impl Into<String>, geometry: Geometry) {
        self.entities.push(BlockEntity {
            layer: layer.into(),
            geometry,
        });
    }

    /// Upsert a parameter definition by name (case-sensitive key as stored).
    pub fn set_param(
        &mut self,
        name: impl Into<String>,
        default: impl Into<String>,
        kind: BlockParamKind,
    ) {
        let name = name.into();
        let default = default.into();
        if let Some(p) = self.params.iter_mut().find(|p| p.name == name) {
            p.default = default;
            p.kind = kind;
        } else {
            self.params.push(BlockParam {
                name,
                default,
                kind,
            });
        }
    }

    /// Look up a parameter by name (case-insensitive).
    pub fn param(&self, name: &str) -> Option<&BlockParam> {
        self.params
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
    }

    /// Upsert a visibility state.
    pub fn set_visibility(&mut self, name: impl Into<String>, hidden: Vec<usize>) {
        let name = name.into();
        if let Some(v) = self.visibilities.iter_mut().find(|v| v.name == name) {
            v.hidden = hidden;
        } else {
            self.visibilities.push(VisibilityState { name, hidden });
        }
    }

    /// Hidden entity indices for `state`, or empty if unknown / Standard.
    pub fn hidden_for(&self, state: &str) -> &[usize] {
        self.visibilities
            .iter()
            .find(|v| v.name.eq_ignore_ascii_case(state))
            .map(|v| v.hidden.as_slice())
            .unwrap_or(&[])
    }

    /// Default attribute map for a new INSERT (params + VISIBILITY=Standard).
    pub fn default_insert_attributes(&self) -> HashMap<String, String> {
        let mut m = HashMap::new();
        for p in &self.params {
            m.insert(p.name.clone(), p.default.clone());
        }
        let vis = self
            .visibilities
            .first()
            .map(|v| v.name.as_str())
            .unwrap_or("Standard");
        m.insert(BLOCK_VIS_ATTR.to_string(), vis.to_string());
        m
    }
}

/// A hatched or filled region (DXF `HATCH`): closed boundary path(s) plus a fill
/// pattern.
///
/// Held in its own [`Document`] table rather than as a [`Geometry`] variant.
/// A hatch is annotation — it contributes nothing to wall pairing or takeoff
/// geometry — and keeping it out of the `Geometry` enum spares every consumer a
/// match arm it would only ever ignore. Modeled for **round-trip fidelity**:
/// outer + island polylines and pattern name survive import → export. Pattern
/// line geometry and arc/ellipse edge types are still approximated as polylines.
#[derive(Debug, Clone, PartialEq)]
pub struct Hatch {
    pub layer: String,
    /// Model space this hatch belongs to (floor × category).
    pub space: ModelSpaceId,
    /// Closed boundary paths: first is the outer loop; any further paths are
    /// islands (holes). Always non-empty after construction via [`Hatch::new`].
    pub boundaries: Vec<Polyline>,
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
            space: ModelSpaceId::default(),
            boundaries: vec![boundary],
            pattern,
            solid,
        }
    }

    /// Outer (first) boundary path.
    pub fn outer(&self) -> &Polyline {
        &self.boundaries[0]
    }

    /// Append an island (hole) path.
    pub fn push_island(&mut self, island: Polyline) {
        self.boundaries.push(island);
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
    /// Model space this dimension belongs to (floor × category).
    pub space: ModelSpaceId,
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
            Geometry::Insert(i) => Geometry::Insert(i.at(shift(i.position), i.rotation)),
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
            Geometry::Insert(i) => Geometry::Insert(i.at(rot(i.position), i.rotation + ddeg)),
        }
    }

    /// This geometry scaled by `factor` about `pivot`. Positions scale about the
    /// pivot; radii and text height scale by `|factor|`; rotations are unchanged.
    pub fn scaled(&self, pivot: Point2, factor: f64) -> Geometry {
        self.scaled_xy(pivot, factor, factor)
    }

    /// Non-uniform scale about `pivot` (insert X/Y scale).
    pub fn scaled_xy(&self, pivot: Point2, scale_x: f64, scale_y: f64) -> Geometry {
        let sc = |p: Point2| {
            Point2::new(
                pivot.x + (p.x - pivot.x) * scale_x,
                pivot.y + (p.y - pivot.y) * scale_y,
            )
        };
        let fr = (scale_x.abs() * scale_y.abs()).sqrt();
        match self {
            Geometry::Line(s) => Geometry::Line(Segment::new(sc(s.a), sc(s.b))),
            Geometry::Polyline(p) => Geometry::Polyline(Polyline::new(
                p.vertices.iter().map(|&v| sc(v)).collect(),
                p.closed,
            )),
            Geometry::Circle(c) => Geometry::Circle(Circle::new(sc(c.center), c.radius * fr)),
            Geometry::Arc(a) => {
                // Negative scales reverse orientation (AutoCAD mirror insert).
                // Reflect angles about the flipped axis and swap start/end so the
                // CCW sweep still tracks the transformed endpoints.
                let mut start = a.start_angle;
                let mut end = a.end_angle;
                if scale_x < 0.0 {
                    start = 180.0 - start;
                    end = 180.0 - end;
                    std::mem::swap(&mut start, &mut end);
                }
                if scale_y < 0.0 {
                    start = -start;
                    end = -end;
                    std::mem::swap(&mut start, &mut end);
                }
                Geometry::Arc(Arc::new(sc(a.center), a.radius * fr, start, end))
            }
            Geometry::Text(t) => {
                let mut rot = t.rotation;
                if scale_x < 0.0 {
                    rot = 180.0 - rot;
                }
                if scale_y < 0.0 {
                    rot = -rot;
                }
                Geometry::Text(Text::new(
                    t.value.clone(),
                    sc(t.position),
                    t.height * fr,
                    rot,
                ))
            }
            Geometry::Insert(i) => Geometry::Insert(
                i.at(sc(i.position), i.rotation)
                    .with_scale(i.scale_x * scale_x, i.scale_y * scale_y),
            ),
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
            Geometry::Insert(i) => Geometry::Insert(i.at(mir(i.position), refl(i.rotation))),
        }
    }

    /// Stretch endpoints (and centers / text positions) that fall inside `window`
    /// by `(dx, dy)`. Points outside the window stay put — classic CAD STRETCH
    /// behaviour for lines and polylines.
    pub fn stretched(&self, window: Bbox, dx: f64, dy: f64) -> Geometry {
        let mv = |p: Point2| {
            if window.contains_point(p) {
                Point2::new(p.x + dx, p.y + dy)
            } else {
                p
            }
        };
        match self {
            Geometry::Line(s) => Geometry::Line(Segment::new(mv(s.a), mv(s.b))),
            Geometry::Polyline(p) => Geometry::Polyline(Polyline::new(
                p.vertices.iter().copied().map(mv).collect(),
                p.closed,
            )),
            Geometry::Circle(c) => Geometry::Circle(Circle::new(mv(c.center), c.radius)),
            Geometry::Arc(a) => {
                Geometry::Arc(Arc::new(mv(a.center), a.radius, a.start_angle, a.end_angle))
            }
            Geometry::Text(t) => Geometry::Text(Text::new(
                t.value.clone(),
                mv(t.position),
                t.height,
                t.rotation,
            )),
            Geometry::Insert(i) => Geometry::Insert(i.at(mv(i.position), i.rotation)),
        }
    }

    /// This (block-space) geometry placed by an insert: shift the block's `base`
    /// to the origin, scale, rotate by `rotation_deg` (CCW), then translate to
    /// `position`. Matches the canvas placement for EXPLODE.
    pub fn instanced(&self, base: Point2, position: Point2, rotation_deg: f64) -> Geometry {
        self.instanced_scaled(base, position, rotation_deg, 1.0, 1.0)
    }

    /// Like [`instanced`](Self::instanced) with non-uniform insert scales.
    pub fn instanced_scaled(
        &self,
        base: Point2,
        position: Point2,
        rotation_deg: f64,
        scale_x: f64,
        scale_y: f64,
    ) -> Geometry {
        self.translated(-base.x, -base.y)
            .scaled_xy(Point2::new(0.0, 0.0), scale_x, scale_y)
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
    /// Model space (floor × category) this entity was drafted into.
    pub space: ModelSpaceId,
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
    /// An entity's layer was changed (undo restores `before`).
    SetEntityLayer {
        id: EntityId,
        before: String,
        after: String,
    },
    /// An entity's model space was changed (undo restores `before`).
    SetEntitySpace {
        id: EntityId,
        before: ModelSpaceId,
        after: ModelSpaceId,
        before_layer: String,
        after_layer: String,
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
    /// Drawing properties were replaced (undo restores `before`). Used for
    /// UNITS / DIMSTYLE / BPLTSTART so one UNDO reverses the whole command.
    /// Boxed so `Op` stays compact (properties are a large struct).
    SetProperties {
        before: Box<DrawingProperties>,
        after: Box<DrawingProperties>,
    },
    /// A block definition was removed (undo re-inserts it, redo removes it) — the
    /// undoable half of `PURGE`.
    RemoveBlock(Block),
    /// A block definition was inserted or replaced (undo restores `before`).
    /// Used by interactive BPARAM / BVIS / BATT / BLOCK-style edits.
    SetBlock { before: Option<Block>, after: Block },
    /// Active model space changed.
    SetActiveSpace {
        before: ModelSpaceId,
        after: ModelSpaceId,
    },
    /// Trace above/below flags changed.
    SetTrace {
        before_below: bool,
        before_above: bool,
        after_below: bool,
        after_above: bool,
    },
    /// Shared reference origin changed.
    SetReferenceOrigin { before: Point2, after: Point2 },
    /// A floor was added (undo removes it and its spaces).
    AddFloor {
        floor: Floor,
        spaces: Vec<ModelSpace>,
    },
    /// A custom model space was added (undo removes it).
    AddModelSpace(ModelSpace),
}

/// A group of primitive changes committed and undone as one atomic unit.
#[derive(Debug, Clone)]
struct Transaction {
    label: String,
    ops: Vec<Op>,
}

/// Drawing-level metadata (AutoCAD-style Summary + title-plate fields).
///
/// Edited via the Drawing Property Manager; untracked outside a transaction
/// (like DWGPROPS). Commands such as UNITS / DIMSTYLE wrap changes in `transact`
/// so they become undoable. Used to auto-fill the title block when plotting to PDF.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DrawingProperties {
    // --- Summary (file properties) ---
    /// Drawing / sheet title (asked again at plot time).
    pub title: String,
    pub subject: String,
    pub author: String,
    pub keywords: String,
    pub comments: String,
    pub hyperlink_base: String,
    // --- Title plate ---
    pub project: String,
    pub client: String,
    pub address: String,
    /// Responsible architect (name and registration).
    pub architect: String,
    pub date: String,
    pub area: String,
    pub scale: String,
    /// Sheet index, e.g. `1/2`.
    pub sheet_number: String,
    pub firm_name: String,
    pub firm_tagline: String,
    pub phone: String,
    pub email: String,
    pub office_address: String,
    /// Absolute or relative path to a logo image (PNG/JPEG/SVG).
    pub logo_path: String,
    /// Project / job number (e.g. `2024-34-43`).
    pub project_number: String,
    /// Discipline label (`Architecture`, `Structural`, …).
    pub discipline: String,
    /// Drawing number in the set (e.g. `A-101`) — large sheet ID.
    pub drawing_number: String,
    /// Total sheets in the set (for `Sheet N of M`).
    pub total_sheets: String,
    /// Drafter initials.
    pub drawn_by: String,
    /// Checker initials.
    pub checked_by: String,
    /// Approver initials.
    pub approved_by: String,
    /// Current revision (e.g. `00`, `Rev 02`).
    pub revision: String,
    /// Issue status (`IFC`, `Issued for Construction`, …).
    pub drawing_status: String,
    /// Firm website / URL shown in the branding cell.
    pub website: String,
    /// Revision table rows: one per line `Rev|Date|Description|By|Checked`.
    pub revision_history: String,
    /// Linear drawing units token: `ft-in` (inches model) or `m` (metres model).
    pub units: String,
    /// Active dimension style name (DIMSTYLE) — follows units by default.
    pub dimstyle: String,
    /// Dimension text height in drawing units.
    pub dim_text_height: String,
    /// Dimension scale factor (string for property round-trip).
    pub dim_scale: String,
    /// `"1"` / `"0"` — show secondary units on dimension labels.
    pub dim_secondary: String,
    /// Auto-number prefix (e.g. `D-` for doors).
    pub autonum_prefix: String,
    /// Next auto-number value (decimal string).
    pub autonum_next: String,
    /// Standard notes body for plot (one note per line).
    pub standard_notes: String,
    /// `"1"` / `"0"` — include notes when plotting to PDF.
    pub plot_include_notes: String,
    /// Nameplate layout preset: `professional` | `compact` | `minimal`.
    pub nameplate_preset: String,
    /// Block name used as the in-drawing nameplate template (`TEMPLATE FROMBLOCK`).
    pub nameplate_block: String,
    /// `"1"` / `"0"` — live takeoff recomputes when the drawing revision changes (default on).
    pub takeoff_live: String,
    /// Typical floor-to-floor height (free text, e.g. `3000 mm` or `3.0 m`) — general notes.
    pub floor_height: String,
    /// Typical lintel height above FFL (free text, e.g. `2100 mm`) — general notes.
    pub lintel_height: String,
}

/// Expand `{field}` tokens in text from drawing properties (unknown tokens left as-is).
pub fn resolve_property_text(s: &str, p: &DrawingProperties) -> String {
    if !s.contains('{') {
        return s.to_string();
    }
    let mut out = s.to_string();
    let pairs: &[(&str, &str)] = &[
        ("{title}", p.title.as_str()),
        ("{subject}", p.subject.as_str()),
        ("{author}", p.author.as_str()),
        ("{project}", p.project.as_str()),
        ("{client}", p.client.as_str()),
        ("{address}", p.address.as_str()),
        ("{architect}", p.architect.as_str()),
        ("{date}", p.date.as_str()),
        ("{area}", p.area.as_str()),
        ("{scale}", p.scale.as_str()),
        ("{sheet_number}", p.sheet_number.as_str()),
        ("{firm_name}", p.firm_name.as_str()),
        ("{firm_tagline}", p.firm_tagline.as_str()),
        ("{phone}", p.phone.as_str()),
        ("{email}", p.email.as_str()),
        ("{office_address}", p.office_address.as_str()),
        ("{project_number}", p.project_number.as_str()),
        ("{discipline}", p.discipline.as_str()),
        ("{drawing_number}", p.drawing_number.as_str()),
        ("{total_sheets}", p.total_sheets.as_str()),
        ("{drawn_by}", p.drawn_by.as_str()),
        ("{checked_by}", p.checked_by.as_str()),
        ("{approved_by}", p.approved_by.as_str()),
        ("{revision}", p.revision.as_str()),
        ("{drawing_status}", p.drawing_status.as_str()),
        ("{website}", p.website.as_str()),
        ("{units}", p.units.as_str()),
        ("{dimstyle}", p.dimstyle.as_str()),
        ("{nameplate_block}", p.nameplate_block.as_str()),
        ("{floor_height}", p.floor_height.as_str()),
        ("{lintel_height}", p.lintel_height.as_str()),
    ];
    for (token, value) in pairs {
        if out.contains(token) {
            out = out.replace(token, value);
        }
    }
    out
}

/// The drawing database. Deterministic iteration order (by id) so downstream
/// detection and takeoff are reproducible regardless of insertion order — a hard
/// requirement from spec §8.
#[derive(Debug, Clone)]
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
    /// Drawing Property Manager fields (title plate / Summary).
    properties: DrawingProperties,
    /// Shared building reference origin (ArchiCAD-style); all floors share XY.
    reference_origin: Point2,
    floors: BTreeMap<LevelId, Floor>,
    spaces: BTreeMap<ModelSpaceId, ModelSpace>,
    /// Explicit layer name → category mapping (import review / LAYERMAP).
    /// When absent, [`SpaceCategory::from_layer_name`] is used.
    layer_space_map: BTreeMap<String, SpaceCategory>,
    active_space: ModelSpaceId,
    trace_below: bool,
    trace_above: bool,
    next_id: u64,
    next_floor_id: u64,
    next_space_id: u64,
    /// The transaction currently being recorded, if any (set only inside `transact`).
    open: Option<Transaction>,
    undo_stack: Vec<Transaction>,
    redo_stack: Vec<Transaction>,
    /// Monotonic change token bumped on every actual mutation. Consumers (e.g. the
    /// live-linked takeoff table) recompute only when this changes.
    revision: u64,
}

impl Default for Document {
    fn default() -> Self {
        let mut doc = Self {
            entities: HashMap::new(),
            layers: HashMap::new(),
            blocks: HashMap::new(),
            hatches: Vec::new(),
            dimensions: Vec::new(),
            properties: DrawingProperties::default(),
            reference_origin: Point2::new(0.0, 0.0),
            floors: BTreeMap::new(),
            spaces: BTreeMap::new(),
            layer_space_map: BTreeMap::new(),
            active_space: ModelSpaceId(0),
            trace_below: false,
            trace_above: false,
            next_id: 0,
            next_floor_id: 1,
            next_space_id: 1,
            open: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            revision: 0,
        };
        doc.ensure_default_spaces();
        doc
    }
}

impl Document {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ensure Ground floor + five category spaces exist. Idempotent.
    /// Returns the Ground floor id and Ground·Walls space id.
    pub fn ensure_default_spaces(&mut self) -> DefaultSpaces {
        if self.floors.is_empty() {
            let ground = LevelId(self.next_floor_id);
            self.next_floor_id += 1;
            self.floors.insert(
                ground,
                Floor {
                    id: ground,
                    name: "Ground".into(),
                    index: 0,
                    elevation_m: 0.0,
                },
            );
            for cat in SpaceCategory::ALL {
                let id = ModelSpaceId(self.next_space_id);
                self.next_space_id += 1;
                let name = ModelSpace::display_name("Ground", cat);
                self.spaces.insert(
                    id,
                    ModelSpace {
                        id,
                        floor: ground,
                        category: cat,
                        name,
                    },
                );
                self.add_layer(Layer::new(cat.default_layer()));
            }
            if let Some(walls) = self.space_on_floor(ground, SpaceCategory::Walls) {
                self.active_space = walls;
            }
        }
        let ground = floors_sorted(&self.floors)
            .first()
            .map(|f| f.id)
            .unwrap_or(LevelId(1));
        let walls = self
            .space_on_floor(ground, SpaceCategory::Walls)
            .unwrap_or(ModelSpaceId(1));
        DefaultSpaces { ground, walls }
    }

    pub fn reference_origin(&self) -> Point2 {
        self.reference_origin
    }

    pub fn set_reference_origin(&mut self, origin: Point2) {
        if self.reference_origin == origin {
            return;
        }
        let before = self.reference_origin;
        self.reference_origin = origin;
        self.record(Op::SetReferenceOrigin {
            before,
            after: origin,
        });
        self.revision += 1;
    }

    pub fn floors(&self) -> Vec<&Floor> {
        floors_sorted(&self.floors)
    }

    pub fn floor(&self, id: LevelId) -> Option<&Floor> {
        self.floors.get(&id)
    }

    pub fn model_spaces(&self) -> impl Iterator<Item = &ModelSpace> {
        self.spaces.values()
    }

    pub fn model_space(&self, id: ModelSpaceId) -> Option<&ModelSpace> {
        self.spaces.get(&id)
    }

    pub fn active_space(&self) -> ModelSpaceId {
        self.active_space
    }

    pub fn active_model_space(&self) -> Option<&ModelSpace> {
        self.spaces.get(&self.active_space)
    }

    pub fn active_floor_id(&self) -> Option<LevelId> {
        self.active_model_space().map(|s| s.floor)
    }

    pub fn trace_below(&self) -> bool {
        self.trace_below
    }

    pub fn trace_above(&self) -> bool {
        self.trace_above
    }

    pub fn space_on_floor(&self, floor: LevelId, category: SpaceCategory) -> Option<ModelSpaceId> {
        self.spaces
            .values()
            .find(|s| s.floor == floor && s.category == category)
            .map(|s| s.id)
    }

    pub fn spaces_on_floor(&self, floor: LevelId) -> Vec<&ModelSpace> {
        let mut v: Vec<&ModelSpace> = self.spaces.values().filter(|s| s.floor == floor).collect();
        v.sort_by_key(|s| s.category);
        v
    }

    pub fn floor_below(&self, floor: LevelId) -> Option<LevelId> {
        let sorted = floors_sorted(&self.floors);
        let idx = sorted.iter().position(|f| f.id == floor)?;
        if idx == 0 {
            None
        } else {
            Some(sorted[idx - 1].id)
        }
    }

    pub fn floor_above(&self, floor: LevelId) -> Option<LevelId> {
        let sorted = floors_sorted(&self.floors);
        let idx = sorted.iter().position(|f| f.id == floor)?;
        sorted.get(idx + 1).map(|f| f.id)
    }

    /// Model spaces that participate in object snap: the active space, same-floor
    /// soft underlays, and TRACE below/above floors. Picking/editing stay active-only.
    pub fn snap_candidate_spaces(&self) -> Vec<ModelSpaceId> {
        let mut out = Vec::new();
        let active = self.active_space();
        out.push(active);
        let Some(floor) = self.active_floor_id() else {
            return out;
        };
        for s in self.spaces_on_floor(floor) {
            if s.id != active {
                out.push(s.id);
            }
        }
        if self.trace_below() {
            if let Some(below) = self.floor_below(floor) {
                for s in self.spaces_on_floor(below) {
                    out.push(s.id);
                }
            }
        }
        if self.trace_above() {
            if let Some(above) = self.floor_above(floor) {
                for s in self.spaces_on_floor(above) {
                    out.push(s.id);
                }
            }
        }
        out
    }

    pub fn set_active_space(&mut self, id: ModelSpaceId) -> Result<(), String> {
        if !self.spaces.contains_key(&id) {
            return Err(format!("unknown model space {}", id.0));
        }
        if self.active_space == id {
            return Ok(());
        }
        let before = self.active_space;
        self.active_space = id;
        self.record(Op::SetActiveSpace { before, after: id });
        self.revision += 1;
        Ok(())
    }

    pub fn set_active_floor(&mut self, floor: LevelId) -> Result<(), String> {
        if !self.floors.contains_key(&floor) {
            return Err(format!("unknown floor {}", floor.0));
        }
        let cat = self
            .active_model_space()
            .map(|s| s.category)
            .unwrap_or(SpaceCategory::Walls);
        let space = self
            .space_on_floor(floor, cat)
            .or_else(|| self.space_on_floor(floor, SpaceCategory::Walls))
            .ok_or_else(|| format!("floor {} has no model spaces", floor.0))?;
        self.set_active_space(space)
    }

    pub fn set_active_category(&mut self, category: SpaceCategory) -> Result<(), String> {
        let floor = self
            .active_floor_id()
            .ok_or_else(|| "no active floor".to_string())?;
        let space = self
            .space_on_floor(floor, category)
            .ok_or_else(|| format!("no {category} space on active floor"))?;
        self.set_active_space(space)
    }

    pub fn set_trace(&mut self, below: bool, above: bool) {
        if self.trace_below == below && self.trace_above == above {
            return;
        }
        let before_below = self.trace_below;
        let before_above = self.trace_above;
        self.trace_below = below;
        self.trace_above = above;
        self.record(Op::SetTrace {
            before_below,
            before_above,
            after_below: below,
            after_above: above,
        });
        self.revision += 1;
    }

    /// Append a floor above the topmost, creating five category spaces.
    pub fn add_floor(&mut self, name: impl Into<String>, elevation_m: f64) -> LevelId {
        let name = name.into();
        let index = self
            .floors
            .values()
            .map(|f| f.index)
            .max()
            .map(|i| i + 1)
            .unwrap_or(0);
        let id = LevelId(self.next_floor_id);
        self.next_floor_id += 1;
        let floor = Floor {
            id,
            name: name.clone(),
            index,
            elevation_m,
        };
        let mut spaces = Vec::with_capacity(5);
        for cat in SpaceCategory::ALL {
            let sid = ModelSpaceId(self.next_space_id);
            self.next_space_id += 1;
            let ms = ModelSpace {
                id: sid,
                floor: id,
                category: cat,
                name: ModelSpace::display_name(&name, cat),
            };
            self.spaces.insert(sid, ms.clone());
            spaces.push(ms);
            self.add_layer(Layer::new(cat.default_layer()));
        }
        self.floors.insert(id, floor.clone());
        self.record(Op::AddFloor { floor, spaces });
        self.revision += 1;
        id
    }

    pub fn rename_floor(&mut self, id: LevelId, name: impl Into<String>) -> bool {
        let name = name.into();
        let Some(floor) = self.floors.get_mut(&id) else {
            return false;
        };
        floor.name = name.clone();
        for s in self.spaces.values_mut() {
            if s.floor == id {
                if s.category == SpaceCategory::Custom {
                    // Keep the custom label after the floor prefix if present.
                    let label = s
                        .name
                        .rsplit_once('·')
                        .map(|(_, rest)| rest.trim().to_string())
                        .unwrap_or_else(|| s.name.clone());
                    s.name = format!("{name} · {label}");
                } else {
                    s.name = ModelSpace::display_name(&name, s.category);
                }
            }
        }
        self.revision += 1;
        true
    }

    /// Add a custom model space on the active floor (or `floor` if given).
    pub fn add_custom_space(
        &mut self,
        floor: LevelId,
        label: impl Into<String>,
    ) -> Result<ModelSpaceId, String> {
        if !self.floors.contains_key(&floor) {
            return Err(format!("unknown floor {}", floor.0));
        }
        let label = label.into().trim().to_string();
        if label.is_empty() {
            return Err("custom space needs a name".into());
        }
        let floor_name = self
            .floors
            .get(&floor)
            .map(|f| f.name.clone())
            .unwrap_or_else(|| "Floor".into());
        let sid = ModelSpaceId(self.next_space_id);
        self.next_space_id += 1;
        let ms = ModelSpace {
            id: sid,
            floor,
            category: SpaceCategory::Custom,
            name: format!("{floor_name} · {label}"),
        };
        self.spaces.insert(sid, ms.clone());
        self.add_layer(Layer::new(SpaceCategory::Custom.default_layer()));
        self.record(Op::AddModelSpace(ms));
        self.revision += 1;
        Ok(sid)
    }

    /// Layer name derived from the active model space category (compat shim).
    pub fn active_category_layer(&self) -> String {
        self.active_model_space()
            .map(|s| s.category.default_layer().to_string())
            .unwrap_or_else(|| "A-WALL".into())
    }

    /// Assign every entity / hatch / dim to `space` (used after OPEN/import).
    pub fn assign_all_to_space(&mut self, space: ModelSpaceId) {
        for e in self.entities.values_mut() {
            e.space = space;
        }
        for h in &mut self.hatches {
            h.space = space;
        }
        for d in &mut self.dimensions {
            d.space = space;
        }
        self.revision += 1;
    }

    /// Assign each item to the Ground space matching its layer→category map
    /// (or [`SpaceCategory::from_layer_name`] when unmapped).
    pub fn assign_imported_by_layer(&mut self) {
        let defaults = self.ensure_default_spaces();
        self.seed_layer_map_heuristics();
        self.reassign_by_layer_map(defaults.ground);
        let _ = self.set_active_space(defaults.walls);
    }

    /// Resolve the category for a layer: explicit map first, then heuristic.
    pub fn category_for_layer(&self, layer: &str) -> SpaceCategory {
        self.layer_space_map
            .get(layer)
            .copied()
            .unwrap_or_else(|| SpaceCategory::from_layer_name(layer))
    }

    pub fn layer_space_map(&self) -> &BTreeMap<String, SpaceCategory> {
        &self.layer_space_map
    }

    /// Replace the whole layer→category map (untracked outside a transaction).
    pub fn set_layer_space_map(&mut self, map: BTreeMap<String, SpaceCategory>) {
        if self.layer_space_map == map {
            return;
        }
        self.layer_space_map = map;
        self.revision += 1;
    }

    pub fn set_layer_category(&mut self, layer: impl Into<String>, category: SpaceCategory) {
        let layer = layer.into();
        if self.layer_space_map.get(&layer) == Some(&category) {
            return;
        }
        self.layer_space_map.insert(layer, category);
        self.revision += 1;
    }

    /// Seed map entries for every distinct layer currently used (heuristic fill).
    /// Does not overwrite existing map entries.
    pub fn seed_layer_map_heuristics(&mut self) {
        let mut layers: BTreeMap<String, ()> = BTreeMap::new();
        for e in self.entities.values() {
            layers.insert(e.layer.clone(), ());
        }
        for h in &self.hatches {
            layers.insert(h.layer.clone(), ());
        }
        for d in &self.dimensions {
            layers.insert(d.layer.clone(), ());
        }
        for name in self.layers.keys() {
            if name.starts_with(IMPORTED_LAYER_PREFIX) {
                layers.insert(name.clone(), ());
            }
        }
        let mut changed = false;
        for layer in layers.into_keys() {
            if self.layer_space_map.contains_key(&layer) {
                continue;
            }
            self.layer_space_map
                .insert(layer.clone(), SpaceCategory::from_layer_name(&layer));
            changed = true;
        }
        if changed {
            self.revision += 1;
        }
    }

    /// Reassign **all** entities / hatches / dims (every floor) onto category
    /// spaces of `floor` using the layer→category map. LAYERMAP APPLY yanks the
    /// whole drawing onto the active floor.
    pub fn reassign_by_layer_map(&mut self, floor: LevelId) {
        self.ensure_default_spaces();
        let walls = self
            .space_on_floor(floor, SpaceCategory::Walls)
            .unwrap_or(self.active_space);
        let resolve = |doc: &Document, layer: &str| -> ModelSpaceId {
            let cat = doc.category_for_layer(layer);
            doc.space_on_floor(floor, cat).unwrap_or(walls)
        };
        let entity_spaces: Vec<(EntityId, ModelSpaceId)> = self
            .entities
            .values()
            .map(|e| (e.id, resolve(self, &e.layer)))
            .collect();
        for (id, space) in entity_spaces {
            if let Some(e) = self.entities.get_mut(&id) {
                e.space = space;
            }
        }
        let hatch_spaces: Vec<ModelSpaceId> = self
            .hatches
            .iter()
            .map(|h| resolve(self, &h.layer))
            .collect();
        for (h, space) in self.hatches.iter_mut().zip(hatch_spaces) {
            h.space = space;
        }
        let dim_spaces: Vec<ModelSpaceId> = self
            .dimensions
            .iter()
            .map(|d| resolve(self, &d.layer))
            .collect();
        for (d, space) in self.dimensions.iter_mut().zip(dim_spaces) {
            d.space = space;
        }
        self.revision += 1;
    }

    /// Layers used by geometry in `space`, plus the category shim and mapped layers.
    pub fn layers_related_to_space(&self, space: ModelSpaceId) -> Vec<String> {
        let mut set = std::collections::BTreeSet::new();
        if let Some(ms) = self.model_space(space) {
            set.insert(ms.category.default_layer().to_string());
            for (layer, cat) in &self.layer_space_map {
                if *cat == ms.category {
                    set.insert(layer.clone());
                }
            }
        }
        for e in self.entities.values().filter(|e| e.space == space) {
            set.insert(e.layer.clone());
        }
        for h in self.hatches.iter().filter(|h| h.space == space) {
            set.insert(h.layer.clone());
        }
        for d in self.dimensions.iter().filter(|d| d.space == space) {
            set.insert(d.layer.clone());
        }
        set.into_iter().collect()
    }

    /// Layers related to any model space on `floor`.
    pub fn layers_related_to_floor(&self, floor: LevelId) -> Vec<String> {
        let mut set = std::collections::BTreeSet::new();
        for s in self.spaces_on_floor(floor) {
            for name in self.layers_related_to_space(s.id) {
                set.insert(name);
            }
        }
        set.into_iter().collect()
    }

    /// Distinct layers used by geometry, with entity counts (entities only).
    pub fn layer_usage_counts(&self) -> BTreeMap<String, usize> {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for e in self.entities.values() {
            *counts.entry(e.layer.clone()).or_default() += 1;
        }
        for h in &self.hatches {
            *counts.entry(h.layer.clone()).or_default() += 1;
        }
        for d in &self.dimensions {
            *counts.entry(d.layer.clone()).or_default() += 1;
        }
        counts
    }

    /// Restore a previously saved layer→space map (file load; untracked).
    pub fn restore_layer_space_map(&mut self, map: BTreeMap<String, SpaceCategory>) {
        self.layer_space_map = map;
    }

    pub fn entities_in_space(&self, space: ModelSpaceId) -> Vec<&Entity> {
        let mut v: Vec<&Entity> = self
            .entities
            .values()
            .filter(|e| e.space == space)
            .collect();
        v.sort_by_key(|e| e.id);
        v
    }

    pub fn entities_on_floor(&self, floor: LevelId) -> Vec<&Entity> {
        let ids: std::collections::HashSet<ModelSpaceId> = self
            .spaces
            .values()
            .filter(|s| s.floor == floor)
            .map(|s| s.id)
            .collect();
        let mut v: Vec<&Entity> = self
            .entities
            .values()
            .filter(|e| ids.contains(&e.space))
            .collect();
        v.sort_by_key(|e| e.id);
        v
    }

    /// Restore floor table from a native-file load (untracked).
    pub fn restore_floors(
        &mut self,
        floors: Vec<Floor>,
        spaces: Vec<ModelSpace>,
        active: ModelSpaceId,
        origin: Point2,
        trace_below: bool,
        trace_above: bool,
        next_floor_id: u64,
        next_space_id: u64,
    ) {
        self.floors.clear();
        self.spaces.clear();
        for f in floors {
            self.floors.insert(f.id, f);
        }
        for s in spaces {
            self.spaces.insert(s.id, s);
        }
        self.active_space = active;
        self.reference_origin = origin;
        self.trace_below = trace_below;
        self.trace_above = trace_above;
        self.next_floor_id = next_floor_id.max(1);
        self.next_space_id = next_space_id.max(1);
        if self.floors.is_empty() {
            self.ensure_default_spaces();
        }
        self.revision += 1;
    }

    /// Monotonic revision token, bumped on every mutation. A consumer that caches
    /// derived data (detection, takeoff) can compare this to know when to recompute.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Drawing Property Manager fields (title, project, firm, logo path, …).
    pub fn properties(&self) -> &DrawingProperties {
        &self.properties
    }

    /// Replace drawing properties.
    ///
    /// When called **inside** [`transact`](Self::transact), the change is recorded
    /// as [`Op::SetProperties`] and is undoable (UNITS / DIMSTYLE / BPLTSTART).
    /// Outside a transaction it remains untracked (Drawing Property Manager /
    /// DWGPROPS-style edits).
    pub fn set_properties(&mut self, properties: DrawingProperties) {
        if self.properties == properties {
            return;
        }
        if self.open.is_some() {
            let before = self.properties.clone();
            self.properties = properties.clone();
            self.record(Op::SetProperties {
                before: Box::new(before),
                after: Box::new(properties),
            });
        } else {
            self.properties = properties;
        }
        self.revision += 1;
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
            Op::SetEntityLayer { id, after, .. } => {
                if let Some(e) = self.entities.get_mut(id) {
                    e.layer = after.clone();
                }
            }
            Op::SetEntitySpace {
                id,
                after,
                after_layer,
                ..
            } => {
                if let Some(e) = self.entities.get_mut(id) {
                    e.space = *after;
                    e.layer = after_layer.clone();
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
            Op::SetProperties { after, .. } => {
                self.properties = (**after).clone();
            }
            Op::RemoveBlock(b) => {
                self.blocks.remove(&b.name);
            }
            Op::SetBlock { after, .. } => {
                self.blocks.insert(after.name.clone(), after.clone());
            }
            Op::SetActiveSpace { after, .. } => {
                self.active_space = *after;
            }
            Op::SetTrace {
                after_below,
                after_above,
                ..
            } => {
                self.trace_below = *after_below;
                self.trace_above = *after_above;
            }
            Op::SetReferenceOrigin { after, .. } => {
                self.reference_origin = *after;
            }
            Op::AddFloor { floor, spaces } => {
                self.floors.insert(floor.id, floor.clone());
                for s in spaces {
                    self.spaces.insert(s.id, s.clone());
                }
            }
            Op::AddModelSpace(s) => {
                self.spaces.insert(s.id, s.clone());
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
            Op::SetEntityLayer { id, before, .. } => {
                if let Some(e) = self.entities.get_mut(id) {
                    e.layer = before.clone();
                }
            }
            Op::SetEntitySpace {
                id,
                before,
                before_layer,
                ..
            } => {
                if let Some(e) = self.entities.get_mut(id) {
                    e.space = *before;
                    e.layer = before_layer.clone();
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
            Op::SetProperties { before, .. } => {
                self.properties = (**before).clone();
            }
            Op::RemoveBlock(b) => {
                self.blocks.insert(b.name.clone(), b.clone());
            }
            Op::SetBlock { before, after } => {
                if let Some(b) = before {
                    self.blocks.insert(b.name.clone(), b.clone());
                } else {
                    self.blocks.remove(&after.name);
                }
            }
            Op::SetActiveSpace { before, .. } => {
                self.active_space = *before;
            }
            Op::SetTrace {
                before_below,
                before_above,
                ..
            } => {
                self.trace_below = *before_below;
                self.trace_above = *before_above;
            }
            Op::SetReferenceOrigin { before, .. } => {
                self.reference_origin = *before;
            }
            Op::AddFloor { floor, spaces } => {
                for s in spaces {
                    self.spaces.remove(&s.id);
                }
                self.floors.remove(&floor.id);
            }
            Op::AddModelSpace(s) => {
                self.spaces.remove(&s.id);
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

    /// Replace a layer record during native-file load (untracked). Creates or
    /// overwrites by name so colour / visibility / freeze survive OPEN.
    pub fn restore_layer(&mut self, layer: Layer) {
        self.layers.insert(layer.name.clone(), layer);
        self.revision += 1;
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

    /// Lock or unlock a layer (locked layers are drawn but not editable). Undoable.
    pub fn set_layer_locked(&mut self, name: &str, locked: bool) -> bool {
        self.update_layer(name, |l| l.locked = locked)
    }

    /// Set a layer's linetype. Undoable.
    pub fn set_layer_linetype(&mut self, name: &str, linetype: LineType) -> bool {
        self.update_layer(name, |l| l.linetype = linetype)
    }

    /// Set a layer's plot lineweight in millimetres. Undoable.
    pub fn set_layer_lineweight_mm(&mut self, name: &str, mm: f32) -> bool {
        self.update_layer(name, |l| l.lineweight_mm = mm)
    }

    /// Whether entities on `layer` should be drawn: a layer unknown to the
    /// document is shown (an entity may reference a layer with no explicit
    /// record), and a known layer is shown when it is on and not frozen.
    pub fn layer_shown(&self, layer: &str) -> bool {
        self.layers.get(layer).is_none_or(Layer::shown)
    }

    /// Remove a layer record, returning whether one existed. Undoable via
    /// [`Op::SetLayer`] (undo restores the layer with its properties). The
    /// caller is responsible for the layer being unused — this is the primitive
    /// `PURGE` drives.
    pub fn remove_layer(&mut self, name: &str) -> bool {
        let Some(before) = self.layers.remove(name) else {
            return false;
        };
        self.record(Op::SetLayer {
            name: name.to_string(),
            before: Some(before),
            after: None,
        });
        self.revision += 1;
        true
    }

    /// Insert geometry on a layer, auto-registering the layer if unseen, and
    /// return the new entity's id. Attributes default to empty.
    ///
    /// Stamps the active model space. When `layer` is empty, uses the category's
    /// default AIA shim layer.
    pub fn add_geometry(&mut self, layer: impl Into<String>, geometry: Geometry) -> EntityId {
        self.add_entity(layer, geometry, HashMap::new())
    }

    /// Insert geometry into the active model space using the category layer shim.
    pub fn add_geometry_in_active(&mut self, geometry: Geometry) -> EntityId {
        let layer = self.active_category_layer();
        self.add_entity(layer, geometry, HashMap::new())
    }

    /// Insert geometry on a layer with explicit attributes.
    pub fn add_entity(
        &mut self,
        layer: impl Into<String>,
        geometry: Geometry,
        attributes: HashMap<String, String>,
    ) -> EntityId {
        let mut layer = layer.into();
        if layer.trim().is_empty() {
            layer = self.active_category_layer();
        }
        self.ensure_default_spaces();
        self.add_layer(Layer::new(layer.clone()));
        let id = EntityId(self.next_id);
        self.next_id += 1;
        let entity = Entity {
            id,
            layer,
            space: self.active_space,
            geometry,
            attributes,
        };
        self.entities.insert(id, entity.clone());
        self.record(Op::AddEntity(entity));
        self.revision += 1;
        id
    }

    /// Insert an entity with a predetermined id during native-file load.
    /// Untracked (no undo). Advances `next_id` past this id so later adds do not
    /// collide. Overwrites any existing entity with the same id.
    pub fn restore_entity(&mut self, entity: Entity) {
        self.add_layer(Layer::new(entity.layer.clone()));
        self.next_id = self.next_id.max(entity.id.0.saturating_add(1));
        self.entities.insert(entity.id, entity);
        self.revision += 1;
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

    /// Change an entity's layer in place (id preserved). Returns `false` if missing.
    pub fn set_entity_layer(&mut self, id: EntityId, layer: impl Into<String>) -> bool {
        let layer = layer.into();
        self.add_layer(Layer::new(layer.clone()));
        let Some(entity) = self.entities.get_mut(&id) else {
            return false;
        };
        if entity.layer == layer {
            return true;
        }
        let before = entity.layer.clone();
        entity.layer = layer.clone();
        self.record(Op::SetEntityLayer {
            id,
            before,
            after: layer,
        });
        self.revision += 1;
        true
    }

    /// Move an entity into a model space and retarget its layer shim.
    pub fn set_entity_space(&mut self, id: EntityId, space: ModelSpaceId) -> bool {
        let Some(ms) = self.spaces.get(&space).cloned() else {
            return false;
        };
        let layer = ms.category.default_layer().to_string();
        self.add_layer(Layer::new(layer.clone()));
        let Some(entity) = self.entities.get_mut(&id) else {
            return false;
        };
        if entity.space == space && entity.layer == layer {
            return true;
        }
        let before = entity.space;
        let before_layer = entity.layer.clone();
        entity.space = space;
        entity.layer = layer.clone();
        self.record(Op::SetEntitySpace {
            id,
            before,
            after: space,
            before_layer,
            after_layer: layer,
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

    /// Stretch an entity: move geometry points inside `window` by `(dx, dy)`.
    /// Returns `false` if no entity has that id, or the geometry is unchanged.
    pub fn stretch_entity(&mut self, id: EntityId, window: Bbox, dx: f64, dy: f64) -> bool {
        let Some(entity) = self.entities.get(&id) else {
            return false;
        };
        let after = entity.geometry.stretched(window, dx, dy);
        if after == entity.geometry {
            return false;
        }
        self.replace_geometry(id, after)
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
    /// Insert or replace a block definition.
    ///
    /// **Untracked** outside a transaction (file load / DXF import). Inside
    /// [`transact`](Self::transact), recorded as [`Op::SetBlock`] so BPARAM /
    /// BVIS / BATT / interactive `BLOCK` edits undo cleanly.
    pub fn add_block(&mut self, block: Block) {
        if self.open.is_some() {
            let before = self.blocks.get(&block.name).cloned();
            if before.as_ref() == Some(&block) {
                return;
            }
            self.blocks.insert(block.name.clone(), block.clone());
            self.record(Op::SetBlock {
                before,
                after: block,
            });
        } else {
            self.blocks.insert(block.name.clone(), block);
        }
        self.revision += 1;
    }

    /// The definition an [`Insert`] of `name` refers to, if the drawing carries
    /// one. Inserts may legitimately reference an undefined block — a DXF can be
    /// exported without its `BLOCKS` section — so callers must handle `None`
    /// rather than assume resolution succeeds.
    pub fn block(&self, name: &str) -> Option<&Block> {
        self.blocks.get(name)
    }

    /// Mutable access to a block definition (library import scaling / params).
    pub fn block_mut(&mut self, name: &str) -> Option<&mut Block> {
        self.blocks.get_mut(name)
    }

    /// All block definitions in ascending name order (deterministic).
    pub fn blocks(&self) -> Vec<&Block> {
        let mut v: Vec<&Block> = self.blocks.values().collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }

    /// Remove a block definition, returning it if present. Recorded as an
    /// undoable [`Op::RemoveBlock`] when a transaction is open (interactive
    /// `PURGE`); untracked otherwise. The caller ensures no [`Insert`] still
    /// references it.
    pub fn remove_block(&mut self, name: &str) -> Option<Block> {
        let removed = self.blocks.remove(name);
        if let Some(block) = &removed {
            if self.open.is_some() {
                self.record(Op::RemoveBlock(block.clone()));
            }
            self.revision += 1;
        }
        removed
    }

    // --- Annotation (hatches, dimensions) ------------------------------------

    /// Add a hatch. Recorded as an undoable [`Op::AddHatch`] when a transaction
    /// is open (an interactive `HATCH`), and left **untracked** otherwise — so a
    /// bulk file load or block instantiation does not flood the undo history,
    /// exactly like [`add_geometry`](Self::add_geometry).
    pub fn add_hatch(&mut self, mut hatch: Hatch) {
        self.ensure_default_spaces();
        if hatch.space.0 == 0 {
            hatch.space = self.active_space;
        }
        if hatch.layer.trim().is_empty() {
            hatch.layer = self.active_category_layer();
        }
        self.add_layer(Layer::new(hatch.layer.clone()));
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

    /// Rename every current layer to [`IMPORTED_LAYER_PREFIX`]`{name}` and remap
    /// entity / hatch / dim / block-content layer references. Call **before**
    /// seeding AIA layers so imported CAD layers stay intact and distinct.
    pub fn retag_imported_layers(&mut self) {
        let mut names: std::collections::HashSet<String> = self.layers.keys().cloned().collect();
        for e in self.entities.values() {
            names.insert(e.layer.clone());
        }
        for h in &self.hatches {
            names.insert(h.layer.clone());
        }
        for d in &self.dimensions {
            names.insert(d.layer.clone());
        }
        for b in self.blocks.values() {
            for be in &b.entities {
                names.insert(be.layer.clone());
            }
        }
        let mut renames: Vec<(String, String)> = Vec::new();
        for name in names {
            if name.is_empty() || name.starts_with(IMPORTED_LAYER_PREFIX) {
                continue;
            }
            renames.push((name.clone(), format!("{IMPORTED_LAYER_PREFIX}{name}")));
        }
        for (from, to) in &renames {
            if let Some(mut layer) = self.layers.remove(from) {
                layer.name = to.clone();
                self.layers.insert(to.clone(), layer);
            } else {
                self.layers.insert(to.clone(), Layer::new(to.clone()));
            }
        }
        let map = |s: &str| -> String {
            renames
                .iter()
                .find(|(f, _)| f == s)
                .map(|(_, t)| t.clone())
                .unwrap_or_else(|| s.to_string())
        };
        if !renames.is_empty() {
            for e in self.entities.values_mut() {
                e.layer = map(&e.layer);
            }
            for h in &mut self.hatches {
                h.layer = map(&h.layer);
            }
            for d in &mut self.dimensions {
                d.layer = map(&d.layer);
            }
            for b in self.blocks.values_mut() {
                for be in &mut b.entities {
                    be.layer = map(&be.layer);
                }
            }
        }
        // Always scrub MTEXT/TEXT formatting so the canvas never shows font names.
        for d in &mut self.dimensions {
            d.text = sanitize_display_text(&d.text);
        }
        for b in self.blocks.values_mut() {
            for be in &mut b.entities {
                if let Geometry::Text(t) = &mut be.geometry {
                    t.value = sanitize_display_text(&t.value);
                }
            }
        }
        for e in self.entities.values_mut() {
            if let Geometry::Text(t) = &mut e.geometry {
                t.value = sanitize_display_text(&t.value);
            }
        }
        self.revision += 1;
    }

    /// Add a dimension. Undoable when a transaction is open, untracked otherwise
    /// — see [`add_hatch`](Self::add_hatch).
    pub fn add_dimension(&mut self, mut dimension: Dimension) {
        self.ensure_default_spaces();
        if dimension.space.0 == 0 {
            dimension.space = self.active_space;
        }
        if dimension.layer.trim().is_empty() {
            dimension.layer = self.active_category_layer();
        }
        self.add_layer(Layer::new(dimension.layer.clone()));
        if self.open.is_some() {
            self.record(Op::AddDimension(dimension.clone()));
        }
        self.dimensions.push(dimension);
        self.revision += 1;
    }

    pub fn next_floor_id(&self) -> u64 {
        self.next_floor_id
    }

    pub fn next_space_id(&self) -> u64 {
        self.next_space_id
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

    /// Unordered entity iterator (no alloc) — prefer for hit-tests / osnap.
    pub fn iter_entities(&self) -> impl Iterator<Item = &Entity> {
        self.entities.values()
    }

    /// Unordered entities in one model space (no alloc / sort).
    pub fn iter_entities_in_space(&self, space: ModelSpaceId) -> impl Iterator<Item = &Entity> {
        self.entities.values().filter(move |e| e.space == space)
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
            pts.extend(h.outer().vertices.iter().copied());
            for island in h.boundaries.iter().skip(1) {
                pts.extend(island.vertices.iter().copied());
            }
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
    fn negative_x_scale_flips_arc_sweep() {
        // 0°→90° quarter-circle under sx=-1 becomes 90°→180° (CCW after X-flip).
        let g = Geometry::Arc(Arc::new(Point2::new(0.0, 0.0), 1.0, 0.0, 90.0));
        let Geometry::Arc(a) = g.scaled_xy(Point2::new(0.0, 0.0), -1.0, 1.0) else {
            panic!("expected arc");
        };
        assert!((a.start_angle - 90.0).abs() < 1e-9, "got {}", a.start_angle);
        assert!((a.end_angle - 180.0).abs() < 1e-9, "got {}", a.end_angle);
        let s = a.start_point();
        let e = a.end_point();
        assert!(s.distance(Point2::new(0.0, 1.0)) < 1e-9, "start {s:?}");
        assert!(e.distance(Point2::new(-1.0, 0.0)) < 1e-9, "end {e:?}");
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
                space: Default::default(),
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
    fn set_properties_is_undoable_inside_transact() {
        let mut doc = Document::new();
        let before = doc.properties().clone();
        let mut after = before.clone();
        after.units = "m".into();
        after.dimstyle = "METRIC".into();
        doc.transact("UNITS", |d| d.set_properties(after.clone()));
        assert_eq!(doc.properties().units, "m");
        doc.undo();
        assert_eq!(doc.properties(), &before);
        doc.redo();
        assert_eq!(doc.properties().dimstyle, "METRIC");

        // Outside a transaction remains untracked (property-manager style).
        let mut doc2 = Document::new();
        let mut p = doc2.properties().clone();
        p.title = "t".into();
        doc2.set_properties(p);
        assert!(!doc2.can_undo());
    }

    #[test]
    fn remove_layer_and_block_are_undoable() {
        let mut doc = Document::new();
        // A layer with non-default state, and a block definition.
        doc.transact("color", |d| d.set_layer_color("A-GRID", 5));
        let mut block = Block::new("DOOR", Point2::new(0.0, 0.0));
        block.push("0", line(0.0, 0.0, 1.0, 0.0));
        doc.add_block(block);

        // Remove both in one transaction.
        doc.transact("purge", |d| {
            assert!(d.remove_layer("A-GRID"));
            assert!(d.remove_block("DOOR").is_some());
        });
        assert!(doc.layer("A-GRID").is_none());
        assert!(doc.block("DOOR").is_none());

        // One UNDO restores both, the layer with its colour.
        doc.undo();
        assert_eq!(doc.layer("A-GRID").unwrap().color, 5, "colour restored");
        assert!(doc.block("DOOR").is_some(), "block restored");

        // Removing a missing layer/block is a no-op.
        assert!(!doc.remove_layer("NOPE"));
        assert!(doc.remove_block("NOPE").is_none());
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
            space: Default::default(),
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
        let id = doc.add_geometry("A-WALL", line(0.0, 0.0, 1.0, 0.0));
        let r1 = doc.revision();
        assert!(r1 > r0, "adding an entity must bump the revision");
        // A pure query does not change the revision.
        let _ = doc.entities();
        assert_eq!(doc.revision(), r1);
        doc.transact("erase", |d| {
            d.remove_entity(id);
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
            d.add_geometry("TEMP-WALL", line(0.0, 0.0, 4.0, 0.0));
            d.add_geometry("TEMP-WALL", line(4.0, 0.0, 4.0, 3.0));
        });
        assert_eq!(doc.len(), 2);
        assert_eq!(doc.undo_label(), Some("Draw two walls"));

        assert_eq!(doc.undo().as_deref(), Some("Draw two walls"));
        assert_eq!(doc.len(), 0);
        // The layer created by the transaction is gone too.
        assert!(doc.layer("TEMP-WALL").is_none());

        assert_eq!(doc.redo().as_deref(), Some("Draw two walls"));
        assert_eq!(doc.len(), 2);
        assert!(doc.layer("TEMP-WALL").is_some());
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

    #[test]
    fn resolve_property_text_expands_known_tokens() {
        let p = DrawingProperties {
            title: "Floor Plan".into(),
            project: "Tower".into(),
            ..Default::default()
        };
        assert_eq!(
            resolve_property_text("{title} — {project}", &p),
            "Floor Plan — Tower"
        );
        assert_eq!(resolve_property_text("static", &p), "static");
        assert_eq!(
            resolve_property_text("{unknown}", &p),
            "{unknown}",
            "unknown tokens left intact"
        );
    }

    #[test]
    fn default_document_has_ground_and_five_spaces() {
        let doc = Document::new();
        assert_eq!(doc.floors().len(), 1);
        assert_eq!(doc.floors()[0].name, "Ground");
        assert_eq!(doc.model_spaces().count(), 5);
        let active = doc.active_model_space().unwrap();
        assert_eq!(active.category, SpaceCategory::Walls);
        assert_eq!(active.name, "Ground · Walls");
    }

    #[test]
    fn add_floor_creates_spaces_and_trace_neighbours() {
        let mut doc = Document::new();
        let ground = doc.active_floor_id().unwrap();
        let l1 = doc.add_floor("Level 1", 3.0);
        assert_eq!(doc.floors().len(), 2);
        assert_eq!(doc.spaces_on_floor(l1).len(), 5);
        assert_eq!(doc.floor_above(ground), Some(l1));
        assert_eq!(doc.floor_below(l1), Some(ground));
        doc.set_active_floor(l1).unwrap();
        doc.set_active_category(SpaceCategory::Furniture).unwrap();
        assert_eq!(
            doc.active_model_space().unwrap().category,
            SpaceCategory::Furniture
        );
        let id = doc.add_geometry_in_active(line(0.0, 0.0, 1.0, 0.0));
        let e = doc.get(id).unwrap();
        assert_eq!(e.layer, "A-FURN");
        assert_eq!(e.space, doc.active_space());
        doc.set_trace(true, true);
        assert!(doc.trace_below() && doc.trace_above());
    }

    #[test]
    fn add_custom_space_on_floor() {
        let mut doc = Document::new();
        let ground = doc.active_floor_id().unwrap();
        let id = doc.add_custom_space(ground, "MEP").unwrap();
        assert_eq!(doc.spaces_on_floor(ground).len(), 6);
        let ms = doc.model_space(id).unwrap();
        assert_eq!(ms.category, SpaceCategory::Custom);
        assert!(ms.name.contains("MEP"));
        assert_eq!(ms.category.default_layer(), "A-ANNO");
    }

    #[test]
    fn layer_space_map_overrides_heuristic_on_reassign() {
        let mut doc = Document::new();
        let id = doc.add_geometry(
            "ORG-DWG_LAYER-MISC",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(1.0, 0.0))),
        );
        doc.set_layer_category("ORG-DWG_LAYER-MISC", SpaceCategory::Furniture);
        let ground = doc.active_floor_id().unwrap();
        doc.reassign_by_layer_map(ground);
        let furn = doc
            .space_on_floor(ground, SpaceCategory::Furniture)
            .unwrap();
        assert_eq!(doc.get(id).unwrap().space, furn);
        assert!(doc
            .layers_related_to_space(furn)
            .iter()
            .any(|n| n == "ORG-DWG_LAYER-MISC"));
    }

    #[test]
    fn layermap_apply_yanks_all_floors_onto_active() {
        let mut doc = Document::new();
        let ground = doc.active_floor_id().unwrap();
        let level1 = doc.add_floor("Level 1", 3.0);
        let walls_l1 = doc.space_on_floor(level1, SpaceCategory::Walls).unwrap();
        let furn_g = doc
            .space_on_floor(ground, SpaceCategory::Furniture)
            .unwrap();
        // Entity lives on Level 1 walls; keep an import layer that maps to Furniture.
        let id = doc.add_geometry(
            "ORG-DWG_LAYER-FURN",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(1.0, 0.0))),
        );
        doc.set_entity_space(id, walls_l1);
        // set_entity_space retargets the shim layer — restore the import layer name.
        assert!(doc.set_entity_layer(id, "ORG-DWG_LAYER-FURN"));
        assert_eq!(doc.get(id).unwrap().space, walls_l1);
        doc.set_layer_category("ORG-DWG_LAYER-FURN", SpaceCategory::Furniture);
        let _ = doc.set_active_floor(ground);
        doc.reassign_by_layer_map(ground);
        assert_eq!(doc.get(id).unwrap().space, furn_g);
    }

    #[test]
    fn sanitize_display_text_strips_mtext_font_and_size() {
        let raw = r"{\fArial|b0|i0|c0|p34;\H2.5000;ROOM 101}";
        assert_eq!(sanitize_display_text(raw), "ROOM 101");
        assert_eq!(sanitize_display_text(r"Line A\PLine B"), "Line A\nLine B");
        let t = Text::new(raw, Point2::new(0.0, 0.0), 2.5, 0.0);
        assert_eq!(t.value, "ROOM 101");
    }

    #[test]
    fn retag_imported_layers_prefixes_and_sanitizes() {
        let mut doc = Document::new();
        // Construct without `Text::new` so retag must scrub formatting itself.
        doc.add_geometry(
            "WALLS",
            Geometry::Text(Text {
                value: r"{\fArial|b0;\H1.5;Label}".into(),
                position: Point2::new(0.0, 0.0),
                height: 1.5,
                rotation: 0.0,
            }),
        );
        doc.retag_imported_layers();
        assert!(doc.layer("ORG-DWG_LAYER-WALLS").is_some());
        assert!(doc.layer("WALLS").is_none());
        let e = doc.entities().into_iter().next().unwrap();
        assert_eq!(e.layer, "ORG-DWG_LAYER-WALLS");
        match &e.geometry {
            Geometry::Text(t) => assert_eq!(t.value, "Label"),
            other => panic!("expected text, got {other:?}"),
        }
    }
}
