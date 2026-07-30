//! The egui desktop shell — command line, docked panels, and the drawing
//! viewport (ADR-0001: egui chrome over the custom canvas).
//!
//! Feature-gated (`--features gui`). This is the one part of `aadt-app` that
//! **cannot be verified headlessly** — it opens a window. It is a thin driver of
//! [`Session`]: every edit flows through [`Session::feed`], so it is undoable and
//! identical to the tested headless path; the viewport only adds interaction.
//!
//! # Interaction model (Spacebar-primary)
//!
//! - **Mouse** = position · **Keyboard** = intent · **Space** = execute.
//! - Type `L` then Space (or click) to start LINE; digits + Space accept typed
//!   values; empty Space finishes; Double-Space finishes and repeats.
//! - Hold Space / Ctrl+Space opens the command palette. Esc cancels.
//! - **Pan** middle-drag · **Zoom** scroll · **Ortho** F8 · **Osnap** F3.
//! - Right-click confirms like empty Space while drafting.

use std::path::{Path, PathBuf};
use std::time::Instant;

use aadt_geometry::{Bbox, Point2, Segment};
use aadt_model::{Document, EntityId, Geometry, Hatch, DEFAULT_LINEWEIGHT_MM};
use aadt_render::{Camera, DrawCommand, DrawList};
use aadt_standards::aci_to_srgb;
use eframe::egui;

use crate::osnap::{OsnapState, SnapKind};
use crate::selection::{select_by_fence, select_in_polygon, RegionMode};
use crate::prefs::UserPrefs;
use crate::{Feedback, Session};

#[path = "gui_chrome.rs"]
mod chrome;
#[path = "command_engine.rs"]
mod command_engine;
#[path = "gui_icons.rs"]
mod icons;
#[path = "gui_theme.rs"]
mod theme;

use command_engine::{catalog, resolve_alias, CommandEngine, SpaceAction, SpaceGesture};

fn parse_entity_id_from_feedback(msg: Option<&str>) -> Option<u64> {
    let msg = msg?;
    for (i, w) in msg.split_whitespace().enumerate() {
        if w.eq_ignore_ascii_case("entity") {
            return msg.split_whitespace().nth(i + 1)?.parse().ok();
        }
    }
    None
}

/// Pick tolerance in screen pixels (also drives pickbox half-size).
const PICK_PIXELS: f64 = 6.0;

/// Cached Layers-panel row: name, ACI, visible, frozen, locked, lineweight_mm, linetype.
type LayerRowCache = (String, u8, bool, bool, bool, f32, String);

/// Which layers the Layer Manager lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LayerMgrScope {
    /// Layers related to the active model space (default).
    ActiveSpace,
    /// Layers related to any space on the active floor.
    ActiveFloor,
    /// Imported CAD layers (`ORG-DWG_LAYER-*`).
    Import,
    /// Every layer in the drawing.
    All,
}

impl LayerMgrScope {
    fn label(self) -> &'static str {
        match self {
            Self::ActiveSpace => "Space",
            Self::ActiveFloor => "Floor",
            Self::Import => "Import",
            Self::All => "All",
        }
    }

    fn cache_key(self) -> &'static str {
        self.label()
    }
}

/// Crosshair total size in screen pixels (AutoCAD-style fixed cursor).
const CROSSHAIR_SIZE_PX: f32 = 100.0;

/// Pickbox half-extent in screen pixels (square at crosshair centre).
const PICKBOX_HALF_PX: f32 = 5.0;

/// Which drafting coordinate field receives typed digits (Tab toggles).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum CoordFocus {
    #[default]
    X,
    Y,
}

/// Parse absolute X/Y from the dyn coord fields.
fn parse_dyn_xy(x: &str, y: &str) -> Option<Point2> {
    let x = x.trim().parse::<f64>().ok()?;
    let y = y.trim().parse::<f64>().ok()?;
    Some(Point2::new(x, y))
}

/// Parse `x,y` typed into a single field / buffer.
fn parse_comma_xy(text: &str) -> Option<Point2> {
    let mut parts = text.splitn(2, ',');
    let x = parts.next()?.trim();
    let y = parts.next()?.trim();
    if y.is_empty() {
        return None;
    }
    parse_dyn_xy(x, y)
}

/// Parse `@dx,dy` relative to `anchor` (AutoCAD-style relative coords).
fn parse_relative_xy(text: &str, anchor: Point2) -> Option<Point2> {
    let rest = text.trim().strip_prefix('@')?;
    let delta = parse_comma_xy(rest)?;
    Some(Point2::new(anchor.x + delta.x, anchor.y + delta.y))
}

/// Object-snap aperture in screen pixels (slightly larger than pick).
const SNAP_PIXELS: f64 = 10.0;

/// Click vs micro-drag threshold (screen px) for accepting a drafting point.
const PLACE_DRAG_PX: f32 = 3.0;

/// Default plot scale denominator (1:100) for lineweight preview.
const PLOT_SCALE: f32 = 100.0;

/// Open the desktop shell on a configured [`Session`] and run until the window
/// closes. Drivers (the CLI `app` command) register `OPEN`/`SAVE`/`TAKEOFF`/
/// `LISP` on the session before calling this — the GUI never owns those deps.
pub fn run(session: Session) -> eframe::Result<()> {
    let icon =
        eframe::icon_data::from_png_bytes(include_bytes!("../assets/branding/app_icon_256.png"))
            .unwrap_or_default();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("AAD")
            .with_inner_size([1280.0, 800.0])
            .with_icon(icon),
        ..Default::default()
    };
    eframe::run_native(
        "AAD",
        options,
        Box::new(move |cc| {
            // Windows 11 / Fluent neutrals + AADT brand orange.
            theme::apply_fluent_visuals(&cc.egui_ctx);
            theme::install_drawing_fonts(&cc.egui_ctx);
            // Keep OS DPI; density comes from compact text/spacing (Cursor-like).
            icons::install(&cc.egui_ctx);
            Ok(Box::new(AadtApp::from_session(session)))
        }),
    )
}

/// The active tool: idle selection, an interactive draw, or a noun-verb edit of
/// the current selection.
#[derive(Debug, Clone)]
pub(super) enum Tool {
    Select,
    /// Continuous line — vertices accumulate; Space finishes; U/C undo/close.
    Line {
        points: Vec<Point2>,
    },
    Circle {
        mode: CircleMode,
        /// Center (Center mode), or first diameter/circumference point.
        p1: Option<Point2>,
        /// Second point (2P / 3P).
        p2: Option<Point2>,
        /// Center mode only: typed/pick value is diameter.
        diameter: bool,
    },
    /// ARC — `mode` picks 3-point (default) or centre-start-end; `a`/`b` are the
    /// first two collected points.
    Arc {
        mode: ArcMode,
        a: Option<Point2>,
        b: Option<Point2>,
    },
    Rectangle {
        first: Option<Point2>,
        /// Corners vs Dimensions workflow (AutoCAD-like).
        phase: RectPhase,
        dim_length: Option<f64>,
        dim_width: Option<f64>,
    },
    /// Construction line — through / Hor / Ver / Ang (AutoCAD XLINE).
    Xline {
        origin: Option<Point2>,
        mode: XlineMode,
        ang_deg: Option<f64>,
    },
    Move {
        /// Verb-noun: accumulate selection until Space.
        collecting: bool,
        base: Option<Point2>,
    },
    Copy {
        collecting: bool,
        base: Option<Point2>,
    },
    Rotate {
        collecting: bool,
        pivot: Option<Point2>,
        /// After `R`: measuring a reference angle then a new angle.
        reference: bool,
        /// First point of the current angle pair (reference or new).
        ref_a: Option<Point2>,
        /// Reference angle in degrees (direction of the first pair).
        ref_ang: Option<f64>,
    },
    Scale {
        collecting: bool,
        pivot: Option<Point2>,
        /// First point of a reference length (Reference mode).
        ref_a: Option<Point2>,
        /// Measured reference length; waiting for the new length.
        old_len: Option<f64>,
    },
    /// Offset: distance first (typed), then entity, then side point.
    Offset {
        dist: Option<f64>,
        entity: Option<EntityId>,
    },
    /// Single-line text: click insertion point, then type value in dyn-input.
    Text {
        origin: Option<Point2>,
    },
    /// Multileader: arrow → landing → text point, then type value.
    Mleader {
        arrow: Option<Point2>,
        landing: Option<Point2>,
        text_pt: Option<Point2>,
    },
    /// Stretch: select objects → crossing window → base → destination.
    Stretch {
        collecting: bool,
        w0: Option<Point2>,
        w1: Option<Point2>,
        base: Option<Point2>,
    },
    /// Distance inquiry: two clicks (DI / DIST).
    Dist {
        first: Option<Point2>,
    },
    /// Vector freehand brush on A-PRES-MARK (polyline strokes).
    Brush {
        points: Option<Vec<Point2>>,
    },
    /// Fillet: pick two lines (radius typed first, default 0).
    Fillet {
        radius: Option<f64>,
        first: Option<EntityId>,
    },
    /// Place a named block reference (INSERT) at the click point.
    Insert {
        name: String,
    },
    /// Create a block definition from the selection (BLOCK / WBLOCK).
    MakeBlock {
        /// True → WBLOCK (erase sources after defining).
        erase: bool,
        collecting: bool,
        /// Typed block name; None until the user enters it.
        name: Option<String>,
    },
    /// Open polyline: click vertices; Enter finishes.
    Polyline {
        points: Vec<Point2>,
    },
    /// Region selection: click region corners (or a fence chain); Space selects.
    /// `crossing` picks window vs crossing membership; `fence` uses the open
    /// poly-line "crosses" rule instead of a closed polygon.
    RegionSelect {
        points: Vec<Point2>,
        crossing: bool,
        fence: bool,
    },
    /// Hatch: click a closed polyline or circle.
    Hatch,
    /// Trim: cutting edges → Space → objects to trim (AutoCAD-like).
    Trim {
        cutters: Vec<EntityId>,
        phase: TrimPhase,
    },
    /// Extend: boundary first, then objects near the end (Space finishes).
    Extend {
        boundary: Option<EntityId>,
    },
    /// Mirror selection across two axis points; then Y/N erase source.
    Mirror {
        collecting: bool,
        a: Option<Point2>,
        b: Option<Point2>,
    },
    /// Rectangular array: rows → cols → dx → dy.
    ArrayRect {
        collecting: bool,
        rows: Option<usize>,
        cols: Option<usize>,
        /// First point when measuring a spacing by two clicks.
        spacing_a: Option<Point2>,
        dx: Option<f64>,
    },
    /// Polar array: center → count → fill angle → rotate items Y/N.
    ArrayPolar {
        collecting: bool,
        center: Option<Point2>,
        count: Option<usize>,
        fill: Option<f64>,
    },
    /// Path array: path entity → count → align Y/N.
    ArrayPath {
        collecting: bool,
        path: Option<EntityId>,
        count: Option<usize>,
    },
    /// Erase: select objects (verb-noun or noun-verb), Space confirms.
    Erase,
    /// Hot-grip edit: `mode` (cycled with Space) transforms the selection about
    /// the grip `origin`. `ref_dist` is the SCALE reference (base→cursor when
    /// Scale mode was entered) so dragging scales relative to the grab.
    GripEdit {
        id: EntityId,
        grip_idx: usize,
        origin: Point2,
        mode: GripMode,
        ref_dist: f64,
    },
    /// Explode: click an insert (or explode selection via command).
    Explode,
    /// Room label (imperial tool): optional name, then two opposite corners.
    RoomLabel {
        room_type: String,
        first: Option<Point2>,
    },
    /// IAR — pick a closed polyline/circle to label its area.
    RoomArea,
    /// Aligned dimension: definition pt1 → pt2 → dim-line/text location.
    Dim {
        first: Option<Point2>,
        second: Option<Point2>,
    },
    /// Quick dim: selection ready → pick chain side / dim-line location.
    QDim,
    /// Continue from last aligned/linear dim: each click adds the next segment.
    DimContinue,
    /// Radius dimension: pick a circle or arc.
    DimRadius,
    /// Diameter dimension: pick a circle or arc.
    DimDiameter,
    /// Angular dimension: vertex → first ray → second ray.
    DimAngular {
        vertex: Option<Point2>,
        p1: Option<Point2>,
    },
    /// Match properties: source entity, then targets.
    MatchProp {
        source: Option<EntityId>,
    },
    /// Zoom window: two corners.
    ZoomWindow {
        first: Option<Point2>,
    },
    /// Plot window: two corners for the area to plot to PDF.
    PlotWindow {
        first: Option<Point2>,
    },
    /// Section cut line on plan (two clicks).
    SectionLine {
        first: Option<Point2>,
    },
    /// Callout: arrow then bubble centre, then type text.
    Callout {
        arrow: Option<Point2>,
        center: Option<Point2>,
    },
    /// Revision mark (triangle) at click.
    RevMark,
    /// Revision cloud: two corners of a box.
    RevCloud {
        first: Option<Point2>,
    },
    /// Empty table: click origin (uses panel rows/cols).
    TablePlace,
    /// Wall: thickness typed first, then polyline points on A-WALL + attrs.
    Wall {
        thickness: Option<f64>,
        points: Vec<Point2>,
    },
    /// Placeholder place (door/window/column/beam/slab) at click.
    PlaceElem {
        kind: PlaceKind,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PlaceKind {
    Door,
    Window,
    Column,
    Beam,
    Slab,
    Tree,
    Shrub,
    Chair,
    Desk,
    Sofa,
    Bed,
    Table,
    Toilet,
}

/// CIRCLE creation mode (AutoCAD Center / 2P / 3P / TTR).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum CircleMode {
    #[default]
    Center,
    TwoPoint,
    ThreePoint,
    /// Tangent-tangent-radius: pick two entities, then a radius.
    Ttr,
}

/// RECTANGLE step (corners or Dimensions).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum RectPhase {
    #[default]
    FirstCorner,
    OppositeCorner,
    DimLength,
    DimWidth,
}

/// TRIM phases — cutting edges, then objects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum TrimPhase {
    #[default]
    CuttingEdges,
    Objects,
}

/// Hot-grip edit modes, cycled with Space (AutoCAD: Stretch → Move → Rotate →
/// Scale → Mirror). Stretch moves the picked grip; the others transform the
/// whole selection about the hot grip as base point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum GripMode {
    #[default]
    Stretch,
    Move,
    Rotate,
    Scale,
    Mirror,
}

impl GripMode {
    /// Next mode in the Space cycle.
    pub(super) fn next(self) -> Self {
        match self {
            GripMode::Stretch => GripMode::Move,
            GripMode::Move => GripMode::Rotate,
            GripMode::Rotate => GripMode::Scale,
            GripMode::Scale => GripMode::Mirror,
            GripMode::Mirror => GripMode::Stretch,
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            GripMode::Stretch => "STRETCH",
            GripMode::Move => "MOVE",
            GripMode::Rotate => "ROTATE",
            GripMode::Scale => "SCALE",
            GripMode::Mirror => "MIRROR",
        }
    }
}

/// ARC construction modes. AutoCAD's default is three points *on* the arc
/// (start, second, end); `[C]enter` switches to centre → start → end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum ArcMode {
    #[default]
    ThreePoint,
    CenterStartEnd,
}

/// RECTANGLE corner treatment (AutoCAD Chamfer / Fillet). The value is the
/// chamfer distance or fillet radius.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum RectCorner {
    Square,
    Chamfer(f64),
    Fillet(f64),
}

impl Default for RectCorner {
    fn default() -> Self {
        RectCorner::Square
    }
}

/// Closed-polyline vertices for a rectangle from opposite corners `a`,`c`, with
/// each corner squared, chamfered by distance `d`, or filleted (rounded) by
/// radius `d`. Fillet arcs are densified. `d` is clamped to half the short side.
fn rect_corner_polyline(a: Point2, c: Point2, corner: RectCorner) -> Vec<Point2> {
    let (minx, maxx) = (a.x.min(c.x), a.x.max(c.x));
    let (miny, maxy) = (a.y.min(c.y), a.y.max(c.y));
    let square = vec![
        Point2::new(minx, miny),
        Point2::new(maxx, miny),
        Point2::new(maxx, maxy),
        Point2::new(minx, maxy),
    ];
    let d = match corner {
        RectCorner::Square => return square,
        RectCorner::Chamfer(d) | RectCorner::Fillet(d) => d,
    };
    let d = d.min((maxx - minx) * 0.5).min((maxy - miny) * 0.5);
    if d <= 1e-9 {
        return square;
    }
    // Eight corner-cut points, CCW from the bottom edge.
    let v = vec![
        Point2::new(minx + d, miny),
        Point2::new(maxx - d, miny),
        Point2::new(maxx, miny + d),
        Point2::new(maxx, maxy - d),
        Point2::new(maxx - d, maxy),
        Point2::new(minx + d, maxy),
        Point2::new(minx, maxy - d),
        Point2::new(minx, miny + d),
    ];
    match corner {
        // Corner segments (odd indices) become 90° arcs; edges stay straight.
        RectCorner::Fillet(_) => {
            let b = std::f64::consts::FRAC_PI_8.tan(); // tan(90°/4) = tan(22.5°)
            let bulges = [0.0, b, 0.0, b, 0.0, b, 0.0, b];
            aadt_geometry::Polyline::densify_bulges(&v, &bulges, true).vertices
        }
        _ => v, // chamfer: straight cuts
    }
}

/// Arc parameters `(center, radius, start_deg, end_deg)` for the CCW arc through
/// `p1`→`p2`→`p3`. The sweep direction is chosen so the arc passes through `p2`.
fn arc_through_3_points(p1: Point2, p2: Point2, p3: Point2) -> Option<(Point2, f64, f64, f64)> {
    let (c, r) = circle_from_3_points(p1, p2, p3)?;
    let ang = |p: Point2| (p.y - c.y).atan2(p.x - c.x).to_degrees();
    // Orientation of p1→p2→p3: CCW (>0) sweeps start→end directly; CW flips so
    // the stored CCW arc still passes through p2.
    let cross = (p2.x - p1.x) * (p3.y - p1.y) - (p2.y - p1.y) * (p3.x - p1.x);
    if cross >= 0.0 {
        Some((c, r, ang(p1), ang(p3)))
    } else {
        Some((c, r, ang(p3), ang(p1)))
    }
}

/// XLINE mode keywords (Hor / Ver / Ang / Through).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum XlineMode {
    #[default]
    Through,
    Hor,
    Ver,
    Ang,
}

impl PlaceKind {
    fn layer(self) -> &'static str {
        match self {
            Self::Door => "A-DOOR",
            Self::Window => "A-GLAZ",
            Self::Column => "S-COLS",
            Self::Beam => "S-BEAM",
            Self::Slab | Self::Toilet => "A-FLOR",
            Self::Tree | Self::Shrub => "S-TREE",
            Self::Chair | Self::Desk | Self::Sofa | Self::Bed | Self::Table => "A-FURN",
        }
    }

    /// Model-space category this place tool belongs to.
    #[allow(dead_code)]
    pub(super) fn space_category(self) -> aadt_model::SpaceCategory {
        use aadt_model::SpaceCategory;
        match self {
            Self::Door | Self::Window => SpaceCategory::Walls,
            Self::Column | Self::Slab => SpaceCategory::Structural,
            Self::Beam => SpaceCategory::BeamLayout,
            Self::Chair
            | Self::Desk
            | Self::Sofa
            | Self::Bed
            | Self::Table
            | Self::Toilet
            | Self::Tree
            | Self::Shrub => SpaceCategory::Furniture,
        }
    }

    fn elem(self) -> &'static str {
        match self {
            Self::Door => "door",
            Self::Window => "window",
            Self::Column => "column",
            Self::Beam => "beam",
            Self::Slab => "slab",
            Self::Tree => "tree",
            Self::Shrub => "shrub",
            Self::Chair => "chair",
            Self::Desk => "desk",
            Self::Sofa => "sofa",
            Self::Bed => "bed",
            Self::Table => "table",
            Self::Toilet => "toilet",
        }
    }

    pub(super) fn block_name(self) -> &'static str {
        match self {
            Self::Door => "DOOR-SINGLE",
            Self::Window => "WINDOW",
            Self::Column => "COLUMN",
            Self::Beam => "BEAM",
            Self::Slab => "SLAB",
            Self::Tree => "TREE",
            Self::Shrub => "SHRUB",
            Self::Chair => "FURN-CHAIR",
            Self::Desk => "FURN-DESK",
            Self::Sofa => "FURN-SOFA",
            Self::Bed => "FURN-BED",
            Self::Table => "FURN-TABLE",
            Self::Toilet => "FIXTURE-TOILET",
        }
    }
}

/// Place tools shown for the active model-space category.
pub(super) fn place_kinds_for_category(
    cat: aadt_model::SpaceCategory,
) -> &'static [(&'static str, PlaceKind)] {
    use aadt_model::SpaceCategory;
    use PlaceKind::*;
    match cat {
        SpaceCategory::Walls => &[("Door", Door), ("Window", Window)],
        SpaceCategory::Structural => &[("Column", Column), ("Slab", Slab)],
        SpaceCategory::Lintel => &[],
        SpaceCategory::BeamLayout => &[("Beam", Beam)],
        SpaceCategory::Furniture => &[
            ("Chair", Chair),
            ("Desk", Desk),
            ("Sofa", Sofa),
            ("Bed", Bed),
            ("Table", Table),
            ("Toilet", Toilet),
            ("Tree", Tree),
            ("Shrub", Shrub),
        ],
        SpaceCategory::Custom => &[
            ("Door", Door),
            ("Window", Window),
            ("Column", Column),
            ("Beam", Beam),
            ("Slab", Slab),
            ("Chair", Chair),
            ("Desk", Desk),
            ("Sofa", Sofa),
            ("Bed", Bed),
            ("Table", Table),
            ("Toilet", Toilet),
            ("Tree", Tree),
            ("Shrub", Shrub),
        ],
    }
}

/// Whether a block name belongs to the active space category.
pub(super) fn block_name_for_category(name: &str, cat: aadt_model::SpaceCategory) -> bool {
    use aadt_model::SpaceCategory;
    if matches!(cat, SpaceCategory::Custom) {
        return true;
    }
    let u = name.trim().to_ascii_uppercase();
    match cat {
        SpaceCategory::Walls => {
            u.contains("DOOR")
                || u.contains("WINDOW")
                || u.contains("GLAZ")
                || u.contains("OPENING")
        }
        SpaceCategory::Structural => {
            u.contains("COL") || u.contains("SLAB") || u.contains("FOUND") || u.contains("PIER")
        }
        SpaceCategory::Lintel => u.contains("LINT"),
        SpaceCategory::BeamLayout => u.contains("BEAM"),
        SpaceCategory::Furniture => {
            u.contains("FURN")
                || u.contains("FIXTURE")
                || u.contains("TREE")
                || u.contains("SHRUB")
                || u.contains("TOILET")
                || u.contains("CHAIR")
                || u.contains("DESK")
                || u.contains("SOFA")
                || u.contains("BED")
                || u.contains("TABLE")
                || u.contains("SINK")
                || u.contains("SHOWER")
                || u.contains("BATHTUB")
        }
        SpaceCategory::Custom => true,
    }
}

/// Library folder / path hint → space category filter.
pub(super) fn library_path_for_category(
    path: &std::path::Path,
    cat: aadt_model::SpaceCategory,
) -> bool {
    use aadt_model::SpaceCategory;
    if matches!(cat, SpaceCategory::Custom) {
        return true;
    }
    let s = path.to_string_lossy().to_ascii_lowercase();
    let name = path
        .file_stem()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_uppercase();
    if block_name_for_category(&name, cat) {
        return true;
    }
    match cat {
        SpaceCategory::Walls => s.contains("door") || s.contains("window"),
        SpaceCategory::Structural => {
            s.contains("column") || s.contains("struct") || s.contains("slab")
        }
        SpaceCategory::Lintel => s.contains("lintel"),
        SpaceCategory::BeamLayout => s.contains("beam"),
        SpaceCategory::Furniture => {
            s.contains("furn") || s.contains("fixture") || s.contains("tree") || s.contains("plant")
        }
        SpaceCategory::Custom => true,
    }
}

/// Map a bare command token to a tool. `None` means "not an interactive command"
/// (the caller feeds it to the session as a typed command instead).
/// Map a typed command token to an interactive tool.
/// Edit transforms always start (verb-noun): `collecting` is true when selection is empty.
pub(super) fn tool_for(token: &str, has_selection: bool) -> Option<Tool> {
    let collect = !has_selection;
    match token.to_ascii_uppercase().as_str() {
        "L" | "LINE" => Some(Tool::Line { points: Vec::new() }),
        "C" | "CIRCLE" => Some(Tool::Circle {
            mode: CircleMode::Center,
            p1: None,
            p2: None,
            diameter: false,
        }),
        "A" | "ARC" => Some(Tool::Arc {
            mode: ArcMode::ThreePoint,
            a: None,
            b: None,
        }),
        "REC" | "RECTANG" | "RECTANGLE" => Some(Tool::Rectangle {
            first: None,
            phase: RectPhase::FirstCorner,
            dim_length: None,
            dim_width: None,
        }),
        "XL" | "XLINE" => Some(Tool::Xline {
            origin: None,
            mode: XlineMode::Through,
            ang_deg: None,
        }),
        "PL" | "PLINE" | "POLYLINE" => Some(Tool::Polyline { points: Vec::new() }),
        "WP" | "WPOLYGON" => Some(Tool::RegionSelect {
            points: Vec::new(),
            crossing: false,
            fence: false,
        }),
        "CPOLYGON" => Some(Tool::RegionSelect {
            points: Vec::new(),
            crossing: true,
            fence: false,
        }),
        "FEN" | "FENCE" => Some(Tool::RegionSelect {
            points: Vec::new(),
            crossing: true,
            fence: true,
        }),
        "H" | "HATCH" => Some(Tool::Hatch),
        "TR" | "TRIM" => Some(Tool::Trim {
            cutters: Vec::new(),
            // AutoCAD Quick mode: all geometry are cutting edges; click objects to trim.
            phase: TrimPhase::Objects,
        }),
        "EX" | "EXTEND" => Some(Tool::Extend { boundary: None }),
        "MI" | "MIRROR" => Some(Tool::Mirror {
            collecting: collect,
            a: None,
            b: None,
        }),
        "AR" | "ARRAY" | "ARRAYRECT" => Some(Tool::ArrayRect {
            collecting: collect,
            rows: None,
            cols: None,
            spacing_a: None,
            dx: None,
        }),
        "ARRAYPOLAR" | "ARRAYPOL" => Some(Tool::ArrayPolar {
            collecting: collect,
            center: None,
            count: None,
            fill: None,
        }),
        "ARRAYPATH" | "APATH" => Some(Tool::ArrayPath {
            collecting: collect,
            path: None,
            count: None,
        }),
        "X" | "EXPLODE" => Some(Tool::Explode),
        "DIM" | "DIMLINEAR" => Some(Tool::Dim {
            first: None,
            second: None,
        }),
        "QDIM" | "QUICKDIM" => Some(Tool::QDim),
        "DIMCONTINUE" | "DIMCONT" | "DCO" => Some(Tool::DimContinue),
        "DIMRADIUS" | "DIMRAD" => Some(Tool::DimRadius),
        "DIMDIAMETER" | "DIMDIA" => Some(Tool::DimDiameter),
        "DIMANGULAR" | "DIMANG" => Some(Tool::DimAngular {
            vertex: None,
            p1: None,
        }),
        "MA" | "MATCHPROP" => Some(Tool::MatchProp { source: None }),
        "Z" | "ZOOM" => Some(Tool::ZoomWindow { first: None }),
        "O" | "OFFSET" => Some(Tool::Offset {
            dist: None,
            entity: None,
        }),
        "T" | "TEXT" | "DTEXT" | "DT" => Some(Tool::Text { origin: None }),
        "MLEADER" | "LEADER" | "MLD" => Some(Tool::Mleader {
            arrow: None,
            landing: None,
            text_pt: None,
        }),
        "TABLE" | "TB" => Some(Tool::TablePlace),
        "SECTIONLINE" | "SECLINE" | "CUTLINE" => Some(Tool::SectionLine { first: None }),
        "CALLOUT" | "COUT" | "BUBBLE" => Some(Tool::Callout {
            arrow: None,
            center: None,
        }),
        "REVMARK" | "REVT" => Some(Tool::RevMark),
        "REVCLOUD" | "RCLOUD" => Some(Tool::RevCloud { first: None }),
        "S" | "STRETCH" => Some(Tool::Stretch {
            collecting: collect,
            w0: None,
            w1: None,
            base: None,
        }),
        "DI" | "DIST" | "DISTANCE" => Some(Tool::Dist { first: None }),
        "BRUSH" | "VB" => Some(Tool::Brush { points: None }),
        "F" | "FILLET" => Some(Tool::Fillet {
            radius: None,
            first: None,
        }),
        "E" | "ERASE" => Some(Tool::Erase),
        "I" | "INSERT" => None, // opens Blocks Manager in dispatch
        "B" | "BLOCK" => Some(Tool::MakeBlock {
            erase: false,
            collecting: collect,
            name: None,
        }),
        "W" | "WBLOCK" => Some(Tool::MakeBlock {
            erase: true,
            collecting: collect,
            name: None,
        }),
        "M" | "MOVE" => Some(Tool::Move {
            collecting: collect,
            base: None,
        }),
        "CO" | "CP" | "COPY" => Some(Tool::Copy {
            collecting: collect,
            base: None,
        }),
        "RO" | "ROTATE" => Some(Tool::Rotate {
            collecting: collect,
            pivot: None,
            reference: false,
            ref_a: None,
            ref_ang: None,
        }),
        "SC" | "SCALE" => Some(Tool::Scale {
            collecting: collect,
            pivot: None,
            ref_a: None,
            old_len: None,
        }),
        "WA" | "WALL" => Some(Tool::Wall {
            thickness: None,
            points: Vec::new(),
        }),
        "DO" | "DOOR" => Some(Tool::PlaceElem {
            kind: PlaceKind::Door,
        }),
        "WI" | "WINDOW" => Some(Tool::PlaceElem {
            kind: PlaceKind::Window,
        }),
        "COL" | "COLUMN" => Some(Tool::PlaceElem {
            kind: PlaceKind::Column,
        }),
        "BE" | "BEAM" => Some(Tool::PlaceElem {
            kind: PlaceKind::Beam,
        }),
        "SL" | "SLAB" => Some(Tool::PlaceElem {
            kind: PlaceKind::Slab,
        }),
        "TREE" => Some(Tool::PlaceElem {
            kind: PlaceKind::Tree,
        }),
        "SHRUB" | "BUSH" => Some(Tool::PlaceElem {
            kind: PlaceKind::Shrub,
        }),
        "CHAIR" => Some(Tool::PlaceElem {
            kind: PlaceKind::Chair,
        }),
        "DESK" => Some(Tool::PlaceElem {
            kind: PlaceKind::Desk,
        }),
        "SOFA" | "COUCH" => Some(Tool::PlaceElem {
            kind: PlaceKind::Sofa,
        }),
        "BED" => Some(Tool::PlaceElem {
            kind: PlaceKind::Bed,
        }),
        "FTABLE" | "RDTABLE" => Some(Tool::PlaceElem {
            kind: PlaceKind::Table,
        }),
        "TOILET" | "WC" => Some(Tool::PlaceElem {
            kind: PlaceKind::Toilet,
        }),
        "IROOM" | "MROOM" => None, // opens picker in dispatch
        "IAR" | "MAR" => Some(Tool::RoomArea),
        "IDO" | "MDO" => Some(Tool::RoomLabel {
            room_type: String::new(),
            first: None,
        }),
        other => {
            if let Some(label) = aadt_command::room_type_for_command(other) {
                Some(Tool::RoomLabel {
                    room_type: label.to_string(),
                    first: None,
                })
            } else if other.eq_ignore_ascii_case("ROOMLABEL") {
                Some(Tool::RoomLabel {
                    room_type: String::new(),
                    first: None,
                })
            } else {
                None
            }
        }
    }
}

struct AadtApp {
    session: Session,
    command: String,
    camera: Camera,
    fitted: bool,
    tool: Tool,
    /// Entities highlighted in the viewport; mirrored into the session's
    /// `SELECTION` before a noun-verb edit runs.
    selection: Vec<EntityId>,
    /// Selected hatches, by index into `document().hatches()`. Held apart from
    /// `selection` because hatches live in their own table (no `EntityId`).
    selected_hatches: Vec<usize>,
    /// Layer new interactive geometry is drawn on.
    current_layer: String,
    /// Screen anchor of an in-progress box selection.
    box_start: Option<egui::Pos2>,
    /// Screen path of an in-progress drag select; a near-straight drag is a box,
    /// a meandering one is a lasso (free-drag region select).
    drag_path: Vec<egui::Pos2>,
    /// Entities already trimmed by the current TRIM drag stroke (AutoCAD
    /// freehand quick-trim), so a single stroke never trims one twice.
    trim_stroke_hits: Vec<EntityId>,
    /// ROTATE/SCALE `[C]opy` option: transform a clone, keep the source.
    /// Set by the `C` keyword; consumed (and cleared) when the transform runs.
    xform_copy: bool,
    /// RECTANGLE corner treatment (Square / Chamfer / Fillet), armed by the
    /// C/F keywords and applied to the next rectangle.
    rect_corner: RectCorner,
    /// RECTANGLE is waiting for a typed chamfer distance (`Some(false)`) or
    /// fillet radius (`Some(true)`) after the C/F keyword.
    rect_corner_pending: Option<bool>,
    /// FILLET `[T]rim` toggle: when set, keep both source lines full-length
    /// (AutoCAD No-trim). Persists across corners until toggled or the command ends.
    fillet_notrim: bool,
    /// CIRCLE TTR: the tangent entities picked so far, with the pick point (used
    /// to choose which of the four tangent solutions to keep).
    ttr_lines: Vec<(EntityId, Point2)>,
    /// Cursor position in world coordinates, for the readout.
    cursor: Option<Point2>,
    /// Cursor position in screen pixels, for the crosshair and dynamic-input box.
    cursor_screen: Option<egui::Pos2>,
    /// The dynamic-input buffer typed at the crosshair (command or value).
    dyn_input: String,
    /// The last submitted command line, for Enter-repeats.
    last_line: Option<String>,
    /// A transient prompt (e.g. "select objects first").
    note: String,
    refocus: bool,
    /// Ortho mode (F8): constrain the second pick of a tool to the nearer axis.
    ortho: bool,
    /// Object snap (F3): pull picks to nearby endpoints / midpoints.
    osnap: bool,
    /// Polar tracking (F10): same soft axis constrain as Ortho while drafting.
    polar: bool,
    /// Object-snap tracking (F11): keep osnap active while polar is on.
    osnap_track: bool,
    /// Dynamic Input near-cursor prompts (F12).
    dyn_mode: bool,
    /// Camera before last ZOOM, for ZOOM P.
    zoom_prev: Option<Camera>,
    /// Active snap under the cursor (for the marker), if any.
    snap_hit: Option<(SnapKind, Point2)>,
    /// One-shot snap override (typed `PER`/`TAN`/… or the SHIFT+right-click
    /// menu): forces a single kind for the next pick, then clears. Works even
    /// with running osnap (F3) off, and supplies PER/TAN via the drag anchor.
    snap_override: Option<SnapKind>,
    /// Screen position of the SHIFT+right-click snap menu while it is open.
    snap_menu_at: Option<egui::Pos2>,
    /// Active layer-set name for the side panel.
    layer_set: String,
    /// Left-dock tab: Layers / Blocks.
    left_tab: usize,
    /// Right-dock tab: Hatches / Annotate / Templates / Units / Dims / History / Properties.
    right_tab: usize,
    /// Block name queued for INSERT from the Blocks Manager.
    insert_block: String,
    /// Filter text in the Layer Manager.
    layer_filter: String,
    /// Layer Manager list scope (Space / Floor / Import / All).
    layer_mgr_scope: LayerMgrScope,
    /// Map-layers-to-spaces review dialog.
    show_layer_map: bool,
    /// Draft map edited in the dialog (layer → category).
    layer_map_draft: std::collections::BTreeMap<String, aadt_model::SpaceCategory>,
    /// Show the right-hand tools manager.
    show_layer_mgr: bool,
    /// Show the left-hand structure manager (Layers / Blocks).
    show_left_mgr: bool,
    /// Status-bar: display lineweights in the viewport.
    show_lweights: bool,
    /// Local copy of Drawing Property Manager fields (applied on Save in the panel).
    props_edit: aadt_model::DrawingProperties,
    /// Draft name for Add space in Drawing Properties / levels.
    space_add_draft: String,
    /// Footer Drawing Properties panel open.
    show_drawing_props: bool,
    /// Plot-to-PDF dialog open.
    plot_open: bool,
    /// Drawing name asked at plot time.
    plot_drawing_name: String,
    /// Paper size name for plot (A4 / A3 / A1…).
    plot_paper: String,
    /// Editable standard notes in the plot dialog.
    plot_notes: String,
    /// Include notes on the plotted sheet.
    plot_include_notes: bool,
    /// Nameplate preset token (`professional` / `compact` / `minimal`).
    plot_nameplate: String,
    /// Plot content: 0 = extents, 1 = selection, 2 = window.
    plot_area_mode: u8,
    /// World-space window from PlotWindow tool (when mode = 2).
    plot_window: Option<aadt_geometry::Bbox>,
    /// Drafting coordinate fields (Tab cycles X ↔ Y).
    dyn_x: String,
    dyn_y: String,
    dyn_focus: CoordFocus,
    /// Active hatch pattern for Hatch Manager / HATCH tool.
    hatch_pattern: String,
    /// Editable autonumber prefix in Annotate panel.
    autonum_prefix: String,
    /// Editable autonumber start.
    autonum_start: String,
    /// Table size for TABLE place tool.
    table_rows: String,
    table_cols: String,
    /// Label for HISTORY SAVE from the History panel.
    history_label: String,
    /// Spacebar-primary command engine.
    engine: CommandEngine,
    space_gesture: SpaceGesture,
    /// Hold / Ctrl+Space command palette.
    palette_open: bool,
    palette_filter: String,
    /// Path of the last successful OPEN/SAVE (.aadt, .dxf, or .dwg).
    file_path: Option<PathBuf>,
    /// True when the document has unsaved edits.
    dirty: bool,
    /// Last successful autosave wall time.
    last_autosave: Option<Instant>,
    /// Autosave interval from prefs (seconds).
    autosave_secs: u64,
    /// Pending recovery file found at launch.
    recovery_prompt: Option<PathBuf>,
    /// F1 shortcut sheet.
    show_shortcuts: bool,
    /// IROOM room-type picker dialog.
    show_room_picker: bool,
    /// Object-snap throttle / scratch buffers.
    osnap_state: OsnapState,
    /// Right panel width (persisted).
    right_panel_width: f32,
    /// Left panel width (persisted).
    left_panel_width: f32,
    /// Cached layer rows: (doc revision, filter+scope key, rows).
    layer_rows_cache: Option<(u64, String, Vec<LayerRowCache>)>,
    /// Cached drawing blocks for the Blocks tab.
    blocks_cache_rev: Option<u64>,
    blocks_cache: Vec<(String, usize)>,
    /// Last pointer screen pos used for meaningful-repaint gating.
    last_pointer_screen: Option<egui::Pos2>,
    /// Cached window title — avoid ViewportCommand every frame.
    last_window_title: String,
    /// Library block listing cache (name, path) — avoid FS walk every frame.
    library_cache: Vec<(String, PathBuf)>,
    library_cache_at: Option<Instant>,
    /// Shift held on the last canvas interaction (TRIM↔EXTEND).
    pointer_shift: bool,
}

impl AadtApp {

    fn from_session(session: Session) -> Self {
        let prefs = UserPrefs::load();
        let recovery_prompt = Self::detect_recovery(None);
        Self {
            session,
            command: String::new(),
            camera: Camera::new((800.0, 600.0)),
            fitted: false,
            tool: Tool::Select,
            selection: Vec::new(),
            selected_hatches: Vec::new(),
            current_layer: "A-WALL".to_string(),
            box_start: None,
            drag_path: Vec::new(),
            trim_stroke_hits: Vec::new(),
            xform_copy: false,
            rect_corner: RectCorner::Square,
            rect_corner_pending: None,
            fillet_notrim: false,
            ttr_lines: Vec::new(),
            cursor: None,
            cursor_screen: None,
            dyn_input: String::new(),
            last_line: None,
            note: String::new(),
            refocus: false,
            ortho: prefs.ortho,
            osnap: prefs.osnap,
            polar: prefs.polar,
            osnap_track: prefs.osnap_track,
            dyn_mode: prefs.dyn_mode,
            zoom_prev: None,
            snap_hit: None,
            snap_override: None,
            snap_menu_at: None,
            layer_set: "CONSTRUCTION".to_string(),
            left_tab: 0,
            right_tab: 1,
            insert_block: String::new(),
            layer_filter: String::new(),
            layer_mgr_scope: LayerMgrScope::ActiveSpace,
            show_layer_map: false,
            layer_map_draft: std::collections::BTreeMap::new(),
            show_layer_mgr: prefs.show_layer_mgr,
            show_left_mgr: prefs.show_left_mgr,
            show_lweights: prefs.show_lweights,
            props_edit: aadt_model::DrawingProperties::default(),
            space_add_draft: String::new(),
            show_drawing_props: false,
            plot_open: false,
            plot_drawing_name: String::new(),
            plot_paper: "A3".into(),
            plot_notes: String::new(),
            plot_include_notes: true,
            plot_nameplate: "professional".into(),
            plot_area_mode: 0,
            plot_window: None,
            dyn_x: String::new(),
            dyn_y: String::new(),
            dyn_focus: CoordFocus::X,
            hatch_pattern: "ANSI31".into(),
            autonum_prefix: "A-".into(),
            autonum_start: "1".into(),
            table_rows: "5".into(),
            table_cols: "4".into(),
            history_label: String::new(),
            engine: CommandEngine::default(),
            space_gesture: SpaceGesture::default(),
            palette_open: false,
            palette_filter: String::new(),
            file_path: None,
            dirty: false,
            last_autosave: None,
            autosave_secs: prefs.autosave_secs.max(30),
            recovery_prompt,
            show_shortcuts: false,
            show_room_picker: false,
            osnap_state: OsnapState::default(),
            right_panel_width: prefs.right_panel_width.clamp(200.0, 340.0),
            left_panel_width: prefs.left_panel_width.clamp(180.0, 260.0),
            layer_rows_cache: None,
            blocks_cache_rev: None,
            blocks_cache: Vec::new(),
            last_pointer_screen: None,
            last_window_title: String::new(),
            library_cache: Vec::new(),
            library_cache_at: None,
            pointer_shift: false,
        }
    }

    fn current_prefs(&self) -> UserPrefs {
        UserPrefs {
            ortho: self.ortho,
            osnap: self.osnap,
            polar: self.polar,
            osnap_track: self.osnap_track,
            dyn_mode: self.dyn_mode,
            show_layer_mgr: self.show_layer_mgr,
            show_left_mgr: self.show_left_mgr,
            show_lweights: self.show_lweights,
            right_panel_width: self.right_panel_width,
            left_panel_width: self.left_panel_width,
            autosave_secs: self.autosave_secs,
        }
    }

    fn persist_prefs(&self) {
        self.current_prefs().save();
    }

    /// Sibling autosave path: `drawing.aadt` → `drawing.autosave.aadt`.
    fn autosave_path(path: &Path) -> PathBuf {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("drawing");
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("aadt");
        path.with_file_name(format!("{stem}.autosave.{ext}"))
    }

    fn detect_recovery(main: Option<&Path>) -> Option<PathBuf> {
        let main = main?;
        let auto = Self::autosave_path(main);
        if !auto.is_file() {
            return None;
        }
        let Ok(am) = auto.metadata().and_then(|m| m.modified()) else {
            return Some(auto);
        };
        match main.metadata().and_then(|m| m.modified()) {
            Ok(mm) if am > mm => Some(auto),
            Ok(_) => None,
            Err(_) => Some(auto),
        }
    }

    /// Feed a command line; tracks dirty + file path for OPEN/SAVE.
    pub(super) fn app_feed(&mut self, line: &str) -> Feedback {
        let rev0 = self.session.document().revision();
        let fb = self.session.feed(line);
        let ok = matches!(fb, Feedback::Ok(_));
        let head = line.split_whitespace().next().unwrap_or("");
        let up = head.to_ascii_uppercase();
        if ok && (up == "OPEN" || up == "SAVE") {
            if let Some(path) = line.split_once(char::is_whitespace).map(|(_, p)| p.trim()) {
                if !path.is_empty() {
                    let pb = PathBuf::from(path);
                    self.file_path = Some(pb.clone());
                    self.dirty = false;
                    if up == "OPEN" {
                        self.recovery_prompt = Self::detect_recovery(Some(&pb));
                        // OPEN retags CAD layers under Import Layers (`IMPORT` set).
                        self.layer_set = "IMPORT".to_string();
                        self.layer_mgr_scope = LayerMgrScope::Import;
                        self.sync_layer_from_active_space();
                        self.open_layer_map_dialog();
                        self.show_left_mgr = true;
                        self.left_tab = 0;
                        // Confidence-gated auto-classification: map elements to
                        // their AIA layer + model-space category via the
                        // embeddings vector db. Confident ones are assigned;
                        // low-confidence ones stay pending (MAPMETA RECONCILE).
                        let rev_before = self.session.document().revision();
                        let mfb = self.session.feed("MAPMETA");
                        if self.session.document().revision() != rev_before {
                            self.dirty = true;
                            self.fitted = false;
                        }
                        if let Some(m) = mfb.message() {
                            self.note = m.to_string();
                        }
                    } else if up == "SAVE" {
                        let auto = Self::autosave_path(&pb);
                        let _ = std::fs::remove_file(auto);
                    }
                }
            }
        } else if self.session.document().revision() != rev0 {
            self.dirty = true;
            // New geometry (ASK / LINE / …) — reframe so it isn't off-camera.
            self.fitted = false;
        }
        if ok
            && matches!(
                up.as_str(),
                "FLOOR" | "FLOORADD" | "CATEGORY" | "SPACEADD" | "CHSPACE"
            )
        {
            // Floor/space context change — drop selection that may live elsewhere.
            // CHSPACE moves selection into the active space; still clear highlight.
            if up != "CHSPACE" {
                self.selection.clear();
            }
            self.sync_layer_from_active_space();
            self.layer_rows_cache = None;
        }
        fb
    }

    fn quick_save(&mut self) {
        if let Some(path) = self.file_path.clone() {
            let fb = self.app_feed(&format!("SAVE {}", path.display()));
            if let Some(m) = fb.message() {
                self.note = m.to_string();
            }
        } else {
            self.save_as_dialog();
        }
    }

    fn save_as_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("AAD", &["aadt"])
            .add_filter("DXF", &["dxf"])
            .add_filter("DWG", &["dwg"])
            .add_filter("VastuDB", &["vdb"])
            .set_file_name("drawing.aadt")
            .save_file()
        {
            let fb = self.app_feed(&format!("SAVE {}", path.display()));
            if let Some(m) = fb.message() {
                self.note = m.to_string();
            }
        }
    }

    fn new_drawing(&mut self) {
        let mut d = Document::new();
        aadt_standards::apply_standard_layers(&mut d);
        self.session.set_document(d);
        self.selection.clear();
        self.fitted = false;
        self.file_path = None;
        self.dirty = false;
        self.recovery_prompt = None;
        self.layer_set = "CONSTRUCTION".to_string();
        self.sync_layer_from_active_space();
        self.note = "New drawing — Ground · Walls".into();
    }

    fn refresh_window_title(&mut self, ctx: &egui::Context) {
        let title = match &self.file_path {
            Some(p) => {
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("drawing");
                if self.dirty {
                    format!("AAD — {name}*")
                } else {
                    format!("AAD — {name}")
                }
            }
            None => {
                if self.dirty {
                    "AAD — untitled*".into()
                } else {
                    "AAD".into()
                }
            }
        };
        if title != self.last_window_title {
            self.last_window_title = title.clone();
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        }
    }

    /// Refresh library file list at most every 2s (or on first open / invalidate).
    pub(super) fn ensure_library_cache(&mut self, force: bool) {
        let stale = force
            || self
                .library_cache_at
                .is_none_or(|t| t.elapsed() > std::time::Duration::from_secs(2));
        if !stale {
            return;
        }
        let root = aadt_library::default_library_root();
        self.library_cache = aadt_library::list_blocks(&root)
            .unwrap_or_default()
            .into_iter()
            .map(|b| (b.name, b.path))
            .collect();
        self.library_cache_at = Some(Instant::now());
    }

    pub(super) fn invalidate_library_cache(&mut self) {
        self.library_cache_at = None;
        self.library_cache.clear();
    }

    fn tick_autosave(&mut self) {
        if !self.dirty {
            return;
        }
        let Some(path) = self.file_path.clone() else {
            return;
        };
        let now = Instant::now();
        if let Some(last) = self.last_autosave {
            if now.duration_since(last).as_secs() < self.autosave_secs {
                return;
            }
        }
        let auto = Self::autosave_path(&path);
        let fb = self.session.feed(&format!("SAVE {}", auto.display()));
        // Autosave must not clear dirty or rewrite file_path.
        if matches!(fb, Feedback::Ok(_)) {
            self.last_autosave = Some(now);
            self.note = format!("Autosaved {}", auto.display());
        }
    }

    fn ensure_blocks_cache(&mut self) {
        let rev = self.session.document().revision();
        if self.blocks_cache_rev == Some(rev) {
            return;
        }
        self.blocks_cache = self
            .session
            .document()
            .blocks()
            .into_iter()
            .map(|b| (b.name.clone(), b.entities.len()))
            .collect();
        self.blocks_cache_rev = Some(rev);
    }

    fn layer_rows_snapshot(&mut self) -> Vec<(String, u8, bool, bool, bool, f32, String)> {
        let rev = self.session.document().revision();
        let filter = self.layer_filter.to_ascii_uppercase();
        let scope_key = format!("{}|{}", self.layer_mgr_scope.cache_key(), filter);
        let stale = match &self.layer_rows_cache {
            Some((r, f, _)) => *r != rev || *f != scope_key,
            None => true,
        };
        if stale {
            let allowed: Option<std::collections::HashSet<String>> = match self.layer_mgr_scope {
                LayerMgrScope::All => None,
                LayerMgrScope::Import => Some(
                    self.session
                        .document()
                        .layers()
                        .filter(|l| l.name.starts_with(aadt_model::IMPORTED_LAYER_PREFIX))
                        .map(|l| l.name.clone())
                        .collect(),
                ),
                LayerMgrScope::ActiveSpace => {
                    let sid = self.session.document().active_space();
                    Some(
                        self.session
                            .document()
                            .layers_related_to_space(sid)
                            .into_iter()
                            .collect(),
                    )
                }
                LayerMgrScope::ActiveFloor => {
                    let names = self
                        .session
                        .document()
                        .active_floor_id()
                        .map(|fid| self.session.document().layers_related_to_floor(fid))
                        .unwrap_or_default();
                    Some(names.into_iter().collect())
                }
            };
            let mut rows: Vec<(String, u8, bool, bool, bool, f32, String)> = self
                .session
                .document()
                .layers()
                .filter(|l| {
                    if let Some(set) = &allowed {
                        if !set.contains(&l.name) {
                            return false;
                        }
                    }
                    filter.is_empty() || l.name.to_ascii_uppercase().contains(&filter)
                })
                .map(|l| {
                    (
                        l.name.clone(),
                        l.color,
                        l.visible,
                        l.frozen,
                        l.locked,
                        l.lineweight_mm,
                        l.linetype.dxf_name().to_string(),
                    )
                })
                .collect();
            rows.sort_by(|a, b| a.0.cmp(&b.0));
            self.layer_rows_cache = Some((rev, scope_key, rows));
        }
        self.layer_rows_cache
            .as_ref()
            .map(|(_, _, rows)| rows.clone())
            .unwrap_or_default()
    }

    /// Seed and open the layer→space mapping dialog.
    pub(super) fn open_layer_map_dialog(&mut self) {
        self.session.document_mut().seed_layer_map_heuristics();
        let counts = self.session.document().layer_usage_counts();
        let mut draft = self.session.document().layer_space_map().clone();
        // Prefer import layers; fall back to all used layers.
        let mut layers: Vec<String> = counts
            .keys()
            .filter(|n| n.starts_with(aadt_model::IMPORTED_LAYER_PREFIX))
            .cloned()
            .collect();
        if layers.is_empty() {
            layers = counts.keys().cloned().collect();
        }
        for layer in &layers {
            draft
                .entry(layer.clone())
                .or_insert_with(|| aadt_model::SpaceCategory::from_layer_name(layer));
        }
        // Keep only layers we are reviewing.
        draft.retain(|k, _| layers.iter().any(|l| l == k));
        self.layer_map_draft = draft;
        self.show_layer_map = !self.layer_map_draft.is_empty();
        if self.show_layer_map {
            self.note = "Map imported layers to model spaces".into();
        }
    }

    pub(super) fn apply_layer_map_draft(&mut self) {
        let map = self.layer_map_draft.clone();
        self.session.document_mut().set_layer_space_map(map);
        let floor = self
            .session
            .document()
            .active_floor_id()
            .or_else(|| self.session.document().floors().first().map(|f| f.id));
        if let Some(floor) = floor {
            self.session.document_mut().reassign_by_layer_map(floor);
        }
        self.layer_rows_cache = None;
        self.dirty = true;
        self.show_layer_map = false;
        self.layer_mgr_scope = LayerMgrScope::ActiveSpace;
        self.sync_layer_from_active_space();
        self.note = "Layer → space map applied (all floors yanked onto active floor)".into();
    }

    pub(super) fn draw_recovery_prompt(&mut self, ctx: &egui::Context) {
        let Some(auto) = self.recovery_prompt.clone() else {
            return;
        };
        egui::Window::new("Autosave recovery")
            .collapsible(false)
            .resizable(false)
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(theme::color::LAYER_CARD)
                    .stroke(egui::Stroke::new(theme::stroke::THIN, theme::color::BORDER))
                    .rounding(theme::radius::DIALOG),
            )
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(format!("A newer autosave was found:\n{}", auto.display()));
                ui.horizontal(|ui| {
                    if ui.button("Restore").clicked() {
                        let p = auto.display().to_string();
                        self.app_feed(&format!("OPEN {p}"));
                        self.dirty = true;
                        self.fitted = false;
                        self.recovery_prompt = None;
                        self.note = "Restored from autosave — save to keep".into();
                    }
                    if ui.button("Discard").clicked() {
                        let _ = std::fs::remove_file(&auto);
                        self.recovery_prompt = None;
                    }
                });
            });
    }

    pub(super) fn draw_shortcuts_sheet(&mut self, ctx: &egui::Context) {
        if !self.show_shortcuts {
            return;
        }
        egui::Window::new("Keyboard & Space workflow")
            .open(&mut self.show_shortcuts)
            .default_width(420.0)
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(theme::color::LAYER_CARD)
                    .stroke(egui::Stroke::new(theme::stroke::THIN, theme::color::BORDER))
                    .rounding(theme::radius::DIALOG),
            )
            .show(ctx, |ui| {
                ui.label(egui::RichText::new("Spacebar command engine").strong());
                ui.label("Space — accept / next / finish");
                ui.label("Double-Space — finish and repeat");
                ui.label("Hold Space / Ctrl+Space — command palette");
                ui.label("Enter / right-click — same as Space");
                ui.label("Esc — cancel to idle");
                ui.label("Tab — cycle X ↔ Y while drafting");
                ui.label("Shift+Space — clear typed input");
                ui.separator();
                ui.label(egui::RichText::new("Modes & file").strong());
                ui.label("F1 — this sheet");
                ui.label("F3 — object snap (END/MID/CEN/QUA/INT/NEA) · F8 — ortho · F10 — polar 45° · F11 — osnap track · F12 — dyn input");
                ui.label("Snap override — type END/MID/INT/CEN/QUA/NEA/PER/TAN at a point, or SHIFT+right-click");
                ui.label("Delete / Backspace — erase selection · W — WBLOCK · WA — WALL");
                ui.label("Z Enter — ZOOM · A Enter — All (extents) · W — Window · P — Previous");
                ui.label("Ctrl+S — quick save · Ctrl+Shift+S — Save As");
                ui.label("Ctrl+O — open · Ctrl+N — new · Ctrl+Z/Y — undo/redo");
                ui.separator();
                ui.label(egui::RichText::new("Command steps (AutoCAD-like)").strong());
                ui.label("MOVE/COPY/STRETCH/… — verb-noun: pick objects · Space · points (noun-verb if pre-selected)");
                ui.label("LINE — points · [U]ndo vertex · [C]lose · Space finish");
                ui.label("CIRCLE — center · radius or [D]iameter · or [2P/3P]");
                ui.label("XLINE — point · through (or H/V/A) · Space ends");
                ui.label("RECTANGLE — corner · corner (or D length/width, @W,H)");
                ui.label("Typed points — x,y absolute · @dx,dy relative to last point");
                ui.label("TRIM — Quick: click object to trim · [C]utting edges · Space ends");
                ui.label("DIM — pt1 · pt2 · dim-line location (AutoCAD-like)");
                ui.label("MIRROR — axis · Erase source? [Y/N] <N>");
                ui.label("ARRAYRECT / AR — rows · cols · dx · dy");
                ui.label("ARRAYPOLAR — center · count · fill° · rotate? [Y/N] <Y>");
                ui.label("ARRAYPATH — path · count · align? [Y/N] <Y>");
                ui.label("Grips — select · click grip · new location");
                ui.label("SCALE — select · base · factor (or R = Reference length)");
                ui.label("ROTATE — select · base · angle (or R = Reference angle)");
                ui.label("IROOM/MROOM / IBR/MBR / IKI/MKI / … — room labels (two corners · dims · area)");
                ui.label("HCWLAYERS · BPLTSTART · FIXTXT · TEXTINCREMENT — layer / BP / text tools");
                ui.label("IDO — custom room · IRECT toggle box · IHIDERECT · IHELP");
                ui.label("UNITS ft-in|m [SECONDARY on|off] — drawing units & alt labels");
            });
    }


    pub(super) fn draw_room_picker(&mut self, ctx: &egui::Context) {
        if !self.show_room_picker {
            return;
        }
        let mut open = self.show_room_picker;
        let mut picked: Option<String> = None;
        let mut custom = false;
        egui::Window::new("Select Room Type")
            .open(&mut open)
            .default_width(280.0)
            .default_height(420.0)
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(theme::color::LAYER_CARD)
                    .stroke(egui::Stroke::new(theme::stroke::THIN, theme::color::BORDER))
                    .rounding(theme::radius::DIALOG),
            )
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new("Imperial room labels — pick two corners after choosing")
                        .small()
                        .weak(),
                );
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for &name in aadt_command::ROOM_TYPES {
                        if ui.selectable_label(false, name).clicked() {
                            picked = Some(name.to_string());
                        }
                    }
                    ui.separator();
                    if ui.selectable_label(false, "Custom…").clicked() {
                        custom = true;
                    }
                });
            });
        self.show_room_picker = open;
        if let Some(name) = picked {
            self.tool = Tool::RoomLabel {
                room_type: name.clone(),
                first: None,
            };
            self.show_room_picker = false;
            self.note = format!("{name} — pick first corner");
        } else if custom {
            self.tool = Tool::RoomLabel {
                room_type: String::new(),
                first: None,
            };
            self.show_room_picker = false;
            self.note = "ROOM — type room name and Space".into();
        }
    }

    /// The anchor point of the active tool (first click), if it has one — the
    /// reference for direct distance entry and the length/angle readout.
    fn tool_anchor(&self) -> Option<Point2> {
        match &self.tool {
            Tool::Line { points } => points.last().copied(),
            Tool::RegionSelect { points, .. } => points.last().copied(),
            Tool::Circle { p1, p2, .. } => p2.or(*p1),
            Tool::Arc { a, b, .. } => b.or(*a),
            Tool::GripEdit { origin, .. } => Some(*origin),
            Tool::Rectangle { first, .. } => *first,
            Tool::RoomLabel { first, .. } => *first,
            Tool::Xline { origin, .. } => *origin,
            Tool::Move {
                collecting: false,
                base,
            }
            | Tool::Copy {
                collecting: false,
                base,
            } => *base,
            Tool::Rotate {
                collecting: false,
                pivot,
                ..
            } => *pivot,
            Tool::Scale {
                collecting: false,
                pivot,
                ref_a,
                ..
            } => ref_a.or(*pivot),
            Tool::Text { origin } => *origin,
            Tool::Mleader {
                arrow,
                landing,
                text_pt,
            } => text_pt.or(*landing).or(*arrow),
            Tool::Stretch { w0, w1, base, .. } => base.or(*w1).or(*w0),
            Tool::Dist { first } => *first,
            Tool::Brush { points } => points.as_ref().and_then(|v| v.last().copied()),
            Tool::Polyline { points } => points.last().copied(),
            Tool::Wall { points, .. } => points.last().copied(),
            Tool::Dim { first, second } => second.or(*first),
            Tool::DimAngular { vertex, p1 } => p1.or(*vertex),
            Tool::Mirror {
                collecting: false,
                a,
                b,
            } => b.or(*a),
            Tool::ArrayPolar {
                collecting: false,
                center,
                ..
            } => *center,
            Tool::ArrayRect {
                collecting: false,
                spacing_a,
                ..
            } => *spacing_a,
            Tool::ZoomWindow { first } => *first,
            Tool::PlotWindow { first } => *first,
            Tool::SectionLine { first } => *first,
            Tool::Callout { arrow, center } => center.or(*arrow),
            Tool::RevCloud { first } => *first,
            Tool::Insert { .. }
            | Tool::MakeBlock { .. }
            | Tool::Offset { .. }
            | Tool::Fillet { .. }
            | Tool::Hatch
            | Tool::Trim { .. }
            | Tool::Extend { .. }
            | Tool::Explode
            | Tool::DimRadius
            | Tool::DimDiameter
            | Tool::QDim
            | Tool::DimContinue
            | Tool::MatchProp { .. }
            | Tool::RevMark
            | Tool::TablePlace
            | Tool::PlaceElem { .. }
            | Tool::Move {
                collecting: true, ..
            }
            | Tool::Copy {
                collecting: true, ..
            }
            | Tool::Rotate {
                collecting: true, ..
            }
            | Tool::Scale {
                collecting: true, ..
            }
            | Tool::Mirror {
                collecting: true, ..
            }
            | Tool::ArrayRect {
                collecting: true, ..
            }
            | Tool::ArrayPolar {
                collecting: true, ..
            }
            | Tool::ArrayPath {
                collecting: true, ..
            }
            | Tool::ArrayPath {
                collecting: false, ..
            }
            | Tool::Erase
            | Tool::RoomArea
            | Tool::Select => None,
        }
    }

    /// Parse a typed length using the drawing's active units (`'` / `"` → feet & inches;
    /// bare number = inches when ft-in, metres when m).
    fn parse_model_length(&self, text: &str) -> Result<f64, String> {
        let units = &self.session.document().properties().units;
        let sys = aadt_model::primary_from_props(units);
        aadt_model::parse_length(text, sys)
    }

    /// Apply Ortho (F8), Polar (F10), or temporary Shift-constrain while drafting.
    fn constrain_pick(&self, p: Point2, shift: bool) -> Point2 {
        if !(self.ortho || self.polar || shift) {
            return p;
        }
        // Opposite corner needs free X and Y; H/V lock would collapse the rectangle.
        if matches!(
            self.tool,
            Tool::Rectangle {
                phase: RectPhase::OppositeCorner,
                first: Some(_),
                ..
            } | Tool::RoomLabel { first: Some(_), .. }
        ) {
            return p;
        }
        let Some(a) = self.tool_anchor() else {
            return p;
        };
        // Ortho / Shift: lock to nearer world axis (H/V).
        if self.ortho || shift {
            let dx = (p.x - a.x).abs();
            let dy = (p.y - a.y).abs();
            return if dx >= dy {
                Point2::new(p.x, a.y)
            } else {
                Point2::new(a.x, p.y)
            };
        }
        // Polar (without Ortho): snap direction to nearest 45° increment.
        if self.polar {
            let dx = p.x - a.x;
            let dy = p.y - a.y;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist < 1e-12 {
                return p;
            }
            let ang = dy.atan2(dx);
            let step = std::f64::consts::FRAC_PI_4;
            let snapped = (ang / step).round() * step;
            return Point2::new(a.x + dist * snapped.cos(), a.y + dist * snapped.sin());
        }
        p
    }

    /// Resolve a world cursor: object snap first (active + TRACE/underlay), then Ortho/Shift.
    fn resolve_pick(&mut self, raw: Point2, shift: bool) -> Point2 {
        let mut p = raw;
        self.snap_hit = None;
        // One-shot override (PER/TAN or a forced running kind) wins over the
        // running snap and ignores Ortho/Polar for that pick, like AutoCAD.
        if let Some(kind) = self.snap_override {
            let tol = SNAP_PIXELS / self.camera.scale;
            let base = self.tool_anchor();
            let spaces = self.session.document().snap_candidate_spaces();
            if let Some(sp) = crate::osnap::find_override_in_spaces(
                self.session.document(),
                Some(spaces.as_slice()),
                kind,
                base,
                raw,
                tol,
            ) {
                self.snap_hit = Some((kind, sp));
                return sp;
            }
            // No qualifying object under the aperture: fall back to raw + Ortho.
            return if self.drafting() {
                self.constrain_pick(raw, shift)
            } else {
                raw
            };
        }
        if self.osnap {
            let tol = SNAP_PIXELS / self.camera.scale;
            let move_tol = 2.0 / self.camera.scale;
            let spaces = self.session.document().snap_candidate_spaces();
            if let Some(hit) = self.osnap_state.resolve_in_spaces(
                self.session.document(),
                Some(spaces.as_slice()),
                raw,
                tol,
                move_tol,
                Instant::now(),
            ) {
                self.snap_hit = Some(hit);
                p = hit.1;
            }
        } else {
            self.osnap_state.clear();
        }
        if self.drafting() {
            self.constrain_pick(p, shift)
        } else {
            p
        }
    }

    /// If a point is expected and `token` is a snap-override keyword (`PER`,
    /// `END`, `MID`, `INT`, `TAN`, …), arm the one-shot override and return
    /// `true` so the caller stops treating the token as a command.
    fn try_snap_override(&mut self, token: &str) -> bool {
        if !self.expects_point_coords() {
            return false;
        }
        let Some(kind) = crate::osnap::parse_snap_keyword(token) else {
            return false;
        };
        self.arm_snap_override(kind);
        true
    }

    /// Arm a one-shot snap override for the next pick (from a keyword or the
    /// SHIFT+right-click menu) and describe it in the status note.
    fn arm_snap_override(&mut self, kind: SnapKind) {
        self.snap_override = Some(kind);
        let label = crate::osnap::snap_label(kind);
        let needs_base = matches!(kind, SnapKind::Perpendicular | SnapKind::Tangent);
        self.note = if needs_base && self.tool_anchor().is_none() {
            format!("Snap override {label} — set a start point first")
        } else {
            format!("Snap override {label} — pick object for the next point")
        };
    }

    /// Render the SHIFT+right-click object-snap override menu while
    /// [`Self::snap_menu_at`] is set. A choice arms a one-shot override; a click
    /// outside, Escape, or "None" dismisses it.
    fn show_snap_menu(&mut self, ctx: &egui::Context) {
        let Some(pos) = self.snap_menu_at else {
            return;
        };
        const ITEMS: &[(&str, SnapKind)] = &[
            ("Endpoint", SnapKind::Endpoint),
            ("Midpoint", SnapKind::Midpoint),
            ("Center", SnapKind::Center),
            ("Quadrant", SnapKind::Quadrant),
            ("Intersection", SnapKind::Intersection),
            ("Perpendicular", SnapKind::Perpendicular),
            ("Tangent", SnapKind::Tangent),
            ("Nearest", SnapKind::Nearest),
        ];
        let mut chosen: Option<SnapKind> = None;
        let mut clear = false;
        let area = egui::Area::new(egui::Id::new("snap_override_menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos)
            .show(ctx, |ui| {
                egui::Frame::menu(ui.style()).show(ui, |ui| {
                    ui.set_min_width(160.0);
                    ui.label(egui::RichText::new("Object Snap").small().weak());
                    for &(label, kind) in ITEMS {
                        let tag = crate::osnap::snap_label(kind);
                        if ui.button(format!("{label}  ({tag})")).clicked() {
                            chosen = Some(kind);
                        }
                    }
                    ui.separator();
                    if ui.button("None").clicked() {
                        clear = true;
                    }
                });
            });
        if let Some(kind) = chosen {
            self.arm_snap_override(kind);
            self.snap_menu_at = None;
            return;
        }
        if clear {
            self.snap_override = None;
            self.note = "Snap override cleared".into();
            self.snap_menu_at = None;
            return;
        }
        // Dismiss on Escape or a press outside the menu.
        let dismissed = ctx.input(|i| i.key_pressed(egui::Key::Escape))
            || (ctx.input(|i| i.pointer.any_pressed()) && !area.response.contains_pointer());
        if dismissed {
            self.snap_menu_at = None;
        }
    }

    /// Commit the dynamic-input buffer: empty finishes/repeats; a number during a
    /// tool is direct distance entry (along the cursor direction); otherwise it is
    /// a command token, dispatched like a command line.
    fn commit_dyn(&mut self) {
        // Absolute / relative X/Y from Tab fields while drafting a point.
        if self.expects_point_coords() {
            let combined = if !self.dyn_input.is_empty() {
                self.dyn_input.clone()
            } else if !self.dyn_x.is_empty() && self.dyn_y.is_empty() {
                self.dyn_x.clone()
            } else {
                format!("{},{}", self.dyn_x, self.dyn_y)
            };
            // `@dx,dy` relative to the tool anchor (or origin when starting a tool).
            if combined.trim_start().starts_with('@') {
                let anchor = self.tool_anchor().unwrap_or_else(|| Point2::new(0.0, 0.0));
                if let Some(p) = parse_relative_xy(combined.trim(), anchor) {
                    self.clear_dyn_coords();
                    self.dyn_input.clear();
                    self.on_pick(p);
                    return;
                }
            }
            if let Some(p) = parse_dyn_xy(&self.dyn_x, &self.dyn_y) {
                self.clear_dyn_coords();
                self.dyn_input.clear();
                self.on_pick(p);
                return;
            }
            if let Some(p) = parse_comma_xy(&combined) {
                self.clear_dyn_coords();
                self.dyn_input.clear();
                self.on_pick(p);
                return;
            }
        }

        let text = if self.expects_point_coords() && self.dyn_input.is_empty() {
            // Prefer X field as the single typed value (distance or command).
            self.dyn_x.trim().to_string()
        } else {
            self.dyn_input.trim().to_string()
        };
        self.dyn_input.clear();
        self.clear_dyn_coords();
        if text.is_empty() {
            // MIRROR erase prompt: empty = No (keep source).
            if let Tool::Mirror {
                collecting: false,
                a: Some(a0),
                b: Some(b0),
            } = self.tool
            {
                self.sync_selection();
                self.app_feed(&format!(
                    "MIRROR SELECTION {} {} {} {} N",
                    a0.x, a0.y, b0.x, b0.y
                ));
                self.tool = Tool::Select;
                self.engine.complete();
                return;
            }
            // ARRAYPOLAR: empty fill → 360°, then ask rotate; empty rotate → Y.
            if let Tool::ArrayPolar {
                collecting: false,
                center: Some(c),
                count: Some(n),
                fill: None,
            } = self.tool
            {
                self.tool = Tool::ArrayPolar {
                    collecting: false,
                    center: Some(c),
                    count: Some(n),
                    fill: Some(360.0),
                };
                self.note = "ARRAYPOLAR — Rotate items as copied? [Yes/No] <Y>".into();
                return;
            }
            if let Tool::ArrayPolar {
                collecting: false,
                center: Some(c),
                count: Some(n),
                fill: Some(fill),
            } = self.tool
            {
                self.finish_array_polar(c, n, fill, true);
                return;
            }
            // ARRAYPATH: empty align → Y.
            if let Tool::ArrayPath {
                collecting: false,
                path: Some(pid),
                count: Some(n),
            } = self.tool
            {
                self.finish_array_path(pid, n, true);
                return;
            }
            if let Tool::Brush {
                points: Some(ref pts),
            } = self.tool
            {
                if pts.len() >= 2 {
                    let coords: Vec<String> = pts
                        .iter()
                        .flat_map(|q| [q.x.to_string(), q.y.to_string()])
                        .collect();
                    self.app_feed(&format!("POLYLINE {} A-PRES-MARK", coords.join(" ")));
                }
                self.tool = Tool::Select;
            } else if let Tool::Polyline { ref points } = self.tool {
                if points.len() >= 2 {
                    let layer = self.layer();
                    let coords: Vec<String> = points
                        .iter()
                        .flat_map(|q| [q.x.to_string(), q.y.to_string()])
                        .collect();
                    self.session
                        .feed(&format!("POLYLINE {} {}", coords.join(" "), layer));
                }
                self.tool = Tool::Select;
            } else if let Tool::Trim {
                phase: TrimPhase::CuttingEdges,
                cutters,
            } = &self.tool
            {
                let cutters = cutters.clone();
                self.tool = Tool::Trim {
                    cutters,
                    phase: TrimPhase::Objects,
                };
                self.engine.advance_step();
                self.note =
                    "TRIM — click or drag across objects to trim (Shift extends; Space finishes)".into();
            } else if matches!(
                self.tool,
                Tool::Trim {
                    phase: TrimPhase::Objects,
                    ..
                } | Tool::Xline { .. }
            ) {
                self.tool = Tool::Select;
                self.engine.complete();
                self.note = "Command complete".into();
            } else if self.drafting() {
                self.tool = Tool::Select;
            } else if let Some(l) = self.last_line.clone() {
                self.dispatch(&l);
            }
            return;
        }
        let up = text.to_ascii_uppercase();
        // Snap-override keyword typed at the crosshair (PER/END/MID/INT/TAN/…):
        // arm the one-shot override for the next pick, then stop.
        if self.try_snap_override(&text) {
            return;
        }
        // TRIM — AutoCAD [C]utting edges option (Standard mode).
        if matches!(
            self.tool,
            Tool::Trim {
                phase: TrimPhase::Objects,
                ..
            }
        ) && (up == "C" || up == "CUTTING" || up == "CUTTINGEDGES" || up == "CUTTING EDGES")
        {
            self.tool = Tool::Trim {
                cutters: Vec::new(),
                phase: TrimPhase::CuttingEdges,
            };
            self.note = "TRIM — select cutting edges (Space when done)".into();
            return;
        }
        // ROTATE/SCALE [C]opy — arm "transform a clone, keep the source".
        if matches!(self.tool, Tool::Rotate { .. } | Tool::Scale { .. })
            && (up == "C" || up == "COPY")
        {
            self.xform_copy = !self.xform_copy;
            let verb = if matches!(self.tool, Tool::Rotate { .. }) {
                "ROTATE"
            } else {
                "SCALE"
            };
            self.note = if self.xform_copy {
                format!("{verb} Copy — source will be kept (C toggles off)")
            } else {
                format!("{verb} Copy off")
            };
            return;
        }
        // FILLET [T]rim — toggle whether the source lines are trimmed.
        if matches!(self.tool, Tool::Fillet { .. }) && (up == "T" || up == "TRIM") {
            self.fillet_notrim = !self.fillet_notrim;
            self.note = if self.fillet_notrim {
                "FILLET — No-trim: lines kept full-length (T toggles)".into()
            } else {
                "FILLET — Trim: lines trimmed to the arc (T toggles)".into()
            };
            return;
        }
        // ARC [C]enter — switch to centre-start-end before the first point.
        if let Tool::Arc { a: None, .. } = self.tool {
            if up == "C" || up == "CENTER" || up == "CENTRE" {
                self.tool = Tool::Arc {
                    mode: ArcMode::CenterStartEnd,
                    a: None,
                    b: None,
                };
                self.note = "ARC — center point".into();
                return;
            }
        }
        // RECTANGLE keywords / Dimensions / @W,H
        if matches!(self.tool, Tool::Rectangle { .. }) {
            // Waiting for a typed chamfer distance / fillet radius.
            if let Some(fillet) = self.rect_corner_pending {
                if let Ok(d) = self.parse_model_length(&text) {
                    if d > 0.0 {
                        self.rect_corner = if fillet {
                            RectCorner::Fillet(d)
                        } else {
                            RectCorner::Chamfer(d)
                        };
                        self.rect_corner_pending = None;
                        self.note = format!(
                            "RECTANGLE — first corner ({} {d})",
                            if fillet { "fillet r" } else { "chamfer" }
                        );
                        return;
                    }
                }
            }
            // Arm Chamfer / Fillet corner treatment (asks for the distance).
            if up == "C" || up == "CHAMFER" {
                self.rect_corner_pending = Some(false);
                self.note = "RECTANGLE — type chamfer distance and Space".into();
                return;
            }
            if up == "F" || up == "FILLET" {
                self.rect_corner_pending = Some(true);
                self.note = "RECTANGLE — type fillet radius and Space".into();
                return;
            }
            if up == "D" || up == "DIM" || up == "DIMENSIONS" {
                self.tool = Tool::Rectangle {
                    first: None,
                    phase: RectPhase::DimLength,
                    dim_length: None,
                    dim_width: None,
                };
                self.note = "RECTANGLE — type length and Space".into();
                return;
            }
            if let Tool::Rectangle {
                first: Some(a),
                phase: RectPhase::OppositeCorner,
                ..
            } = self.tool
            {
                let rel = text.strip_prefix('@').unwrap_or(text.as_str());
                if let Some(b) = parse_comma_xy(rel) {
                    let corner = if text.starts_with('@') || text.contains(',') {
                        Point2::new(a.x + b.x, a.y + b.y)
                    } else {
                        b
                    };
                    let layer = self.layer();
                    self.place_rectangle(a, corner, &layer);
                    return;
                }
            }
            if let Tool::Rectangle {
                first,
                phase: RectPhase::DimLength,
                dim_width,
                ..
            } = self.tool
            {
                if let Ok(len) = self.parse_model_length(&text) {
                    if len.abs() > f64::EPSILON {
                        self.tool = Tool::Rectangle {
                            first,
                            phase: RectPhase::DimWidth,
                            dim_length: Some(len),
                            dim_width,
                        };
                        self.note = "RECTANGLE — type width and Space".into();
                        return;
                    }
                }
            }
            if let Tool::Rectangle {
                first,
                phase: RectPhase::DimWidth,
                dim_length: Some(len),
                ..
            } = self.tool
            {
                if let Ok(w) = self.parse_model_length(&text) {
                    if w.abs() > f64::EPSILON {
                        let layer = self.layer();
                        if let Some(a) = first {
                            self.place_rectangle_dims(a, len, w, &layer);
                        } else {
                            self.tool = Tool::Rectangle {
                                first: None,
                                phase: RectPhase::DimWidth,
                                dim_length: Some(len),
                                dim_width: Some(w),
                            };
                            self.note = "RECTANGLE — click first corner to place".into();
                        }
                        return;
                    }
                }
            }
        }
        // ZOOM options (AutoCAD: Z Enter, then A = All / E = Extents / W / P / S).
        if matches!(self.tool, Tool::ZoomWindow { first: None }) {
            match up.as_str() {
                "A" | "ALL" | "E" | "EXTENTS" => {
                    self.apply_nav_hint("extents");
                    self.tool = Tool::Select;
                    self.note = if matches!(up.as_str(), "A" | "ALL") {
                        "Zoom All".into()
                    } else {
                        "Zoom Extents".into()
                    };
                    return;
                }
                "W" | "WINDOW" => {
                    self.note = "ZOOM Window — click first corner".into();
                    return;
                }
                "P" | "PREVIOUS" => {
                    self.apply_nav_hint("previous");
                    self.tool = Tool::Select;
                    self.note = "Zoom Previous".into();
                    return;
                }
                "S" | "SCALE" => {
                    self.note = "ZOOM Scale — type factor and Space (e.g. 2 or 0.5x)".into();
                    return;
                }
                _ => {
                    // Bare number or "2x" / "0.5X" as scale factor.
                    let factor = if let Some(rest) = up.strip_suffix('X') {
                        rest.parse::<f64>().ok()
                    } else {
                        text.parse::<f64>().ok()
                    };
                    if let Some(f) = factor {
                        if f.is_finite() && f.abs() > f64::EPSILON {
                            self.apply_nav_hint(&format!("scale:{f}"));
                            self.tool = Tool::Select;
                            self.note = format!("Zoom scale ×{f}");
                            return;
                        }
                    }
                }
            }
        }
        // XLINE mode keywords
        if matches!(self.tool, Tool::Xline { .. }) {
            if matches!(up.as_str(), "H" | "HOR" | "HORIZONTAL") {
                self.tool = Tool::Xline {
                    origin: None,
                    mode: XlineMode::Hor,
                    ang_deg: None,
                };
                self.note = "XLINE Hor — specify a point".into();
                return;
            }
            if matches!(up.as_str(), "V" | "VER" | "VERTICAL") {
                self.tool = Tool::Xline {
                    origin: None,
                    mode: XlineMode::Ver,
                    ang_deg: None,
                };
                self.note = "XLINE Ver — specify a point".into();
                return;
            }
            if matches!(up.as_str(), "A" | "ANG" | "ANGLE") {
                self.tool = Tool::Xline {
                    origin: None,
                    mode: XlineMode::Ang,
                    ang_deg: None,
                };
                self.note = "XLINE Ang — type angle in degrees and Space".into();
                return;
            }
            if let Tool::Xline {
                mode: XlineMode::Ang,
                origin,
                ..
            } = self.tool
            {
                if let Ok(deg) = text.parse::<f64>() {
                    self.tool = Tool::Xline {
                        origin,
                        mode: XlineMode::Ang,
                        ang_deg: Some(deg),
                    };
                    self.note = "XLINE Ang — specify a point".into();
                    return;
                }
            }
        }
        // LINE / POLYLINE undo & close keywords.
        if up == "C" || up == "CLOSE" {
            if let Tool::Line { points } = &self.tool {
                if points.len() >= 3 {
                    let layer = self.layer();
                    if let (Some(&first), Some(&last)) = (points.first(), points.last()) {
                        self.session.feed(&format!(
                            "LINE {} {} {} {} {}",
                            last.x, last.y, first.x, first.y, layer
                        ));
                    }
                    self.tool = Tool::Select;
                    self.engine.complete();
                    return;
                }
            }
            if let Tool::Polyline { points } = &self.tool {
                if points.len() >= 3 {
                    let layer = self.layer();
                    let coords: Vec<String> = points
                        .iter()
                        .flat_map(|q| [q.x.to_string(), q.y.to_string()])
                        .collect();
                    self.session
                        .feed(&format!("POLYLINE {} CLOSED {}", coords.join(" "), layer));
                    self.tool = Tool::Select;
                    self.engine.complete();
                    return;
                }
            }
        }
        if let Tool::Line { points } = &mut self.tool {
            if up == "U" || up == "UNDO" {
                if points.len() >= 2 {
                    points.pop();
                    self.session.feed("UNDO");
                } else if points.len() == 1 {
                    points.clear();
                }
                return;
            }
        }
        if let Tool::Polyline { points } = &mut self.tool {
            if up == "U" || up == "UNDO" {
                // Vertices live only in tool state until commit — never document UNDO.
                if !points.is_empty() {
                    points.pop();
                }
                return;
            }
        }
        // CIRCLE mode keywords / diameter / typed radius.
        if matches!(self.tool, Tool::Circle { .. }) {
            if up == "2P" || up == "TWO" {
                self.tool = Tool::Circle {
                    mode: CircleMode::TwoPoint,
                    p1: None,
                    p2: None,
                    diameter: false,
                };
                self.note = "CIRCLE 2P — first point of diameter".into();
                return;
            }
            if up == "3P" || up == "THREE" {
                self.tool = Tool::Circle {
                    mode: CircleMode::ThreePoint,
                    p1: None,
                    p2: None,
                    diameter: false,
                };
                self.note = "CIRCLE 3P — first point on circumference".into();
                return;
            }
            if up == "T" || up == "TTR" || up == "TAN" {
                self.tool = Tool::Circle {
                    mode: CircleMode::Ttr,
                    p1: None,
                    p2: None,
                    diameter: false,
                };
                self.ttr_lines.clear();
                self.note = "CIRCLE TTR — pick the first tangent line".into();
                return;
            }
            // CIRCLE TTR: a typed radius once both tangent lines are picked.
            if matches!(
                self.tool,
                Tool::Circle {
                    mode: CircleMode::Ttr,
                    ..
                }
            ) && self.ttr_lines.len() == 2
            {
                if let Ok(r) = self.parse_model_length(&text) {
                    if r > 0.0 {
                        self.finish_ttr_circle(r);
                    }
                    return;
                }
            }
            if let Tool::Circle {
                mode: CircleMode::Center,
                p1: Some(_),
                diameter,
                ..
            } = &mut self.tool
            {
                if up == "D" || up == "DIAMETER" {
                    *diameter = true;
                    self.note = "CIRCLE — diameter (type + Space)".into();
                    return;
                }
            }
            if let Tool::Circle {
                mode: CircleMode::Center,
                p1: Some(c),
                diameter,
                ..
            } = self.tool
            {
                if let Ok(len) = self.parse_model_length(&text) {
                    let r = if diameter { len * 0.5 } else { len };
                    if r > f64::EPSILON {
                        let layer = self.layer();
                        self.session
                            .feed(&format!("CIRCLE {} {} {} {}", c.x, c.y, r, layer));
                        self.tool = Tool::Select;
                        self.engine.complete();
                        return;
                    }
                }
            }
        }
        // MIRROR erase-source prompt (after both axis points).
        if let Tool::Mirror {
            collecting: false,
            a: Some(a0),
            b: Some(b0),
        } = self.tool
        {
            let erase = matches!(up.as_str(), "Y" | "YES" | "ERASE");
            let keep = matches!(up.as_str(), "N" | "NO") || text.is_empty();
            if erase || keep {
                self.sync_selection();
                let flag = if erase { "Y" } else { "N" };
                self.app_feed(&format!(
                    "MIRROR SELECTION {} {} {} {} {}",
                    a0.x, a0.y, b0.x, b0.y, flag
                ));
                self.tool = Tool::Select;
                self.engine.complete();
                return;
            }
        }
        // ARRAYRECT typed rows / cols / spacings.
        if let Tool::ArrayRect {
            collecting: false,
            rows,
            cols,
            spacing_a: None,
            dx,
        } = self.tool
        {
            if let Ok(n) = text.parse::<usize>() {
                if rows.is_none() {
                    if n == 0 {
                        self.note = "ARRAYRECT — rows must be at least 1".into();
                        return;
                    }
                    self.tool = Tool::ArrayRect {
                        collecting: false,
                        rows: Some(n),
                        cols: None,
                        spacing_a: None,
                        dx: None,
                    };
                    self.note = "ARRAYRECT — columns (type + Space)".into();
                    return;
                }
                if cols.is_none() {
                    if n == 0 {
                        self.note = "ARRAYRECT — columns must be at least 1".into();
                        return;
                    }
                    self.tool = Tool::ArrayRect {
                        collecting: false,
                        rows,
                        cols: Some(n),
                        spacing_a: None,
                        dx: None,
                    };
                    self.note = "ARRAYRECT — column spacing dx (type or pick two points)".into();
                    return;
                }
            }
            if let Ok(v) = text.parse::<f64>() {
                if rows.is_some() && cols.is_some() && dx.is_none() {
                    self.tool = Tool::ArrayRect {
                        collecting: false,
                        rows,
                        cols,
                        spacing_a: None,
                        dx: Some(v),
                    };
                    self.note = "ARRAYRECT — row spacing dy (type or pick two points)".into();
                    return;
                }
                if let (Some(r), Some(c), Some(dxv)) = (rows, cols, dx) {
                    self.finish_array_rect(r, c, dxv, v);
                    return;
                }
            }
        }
        // ARRAYPOLAR typed count / fill / rotate flag.
        if let Tool::ArrayPolar {
            collecting: false,
            center: Some(c),
            count,
            fill,
        } = self.tool
        {
            match (count, fill) {
                (None, _) => {
                    if let Ok(n) = text.parse::<usize>() {
                        if n < 2 {
                            self.note = "ARRAYPOLAR — count must be at least 2".into();
                            return;
                        }
                        self.tool = Tool::ArrayPolar {
                            collecting: false,
                            center: Some(c),
                            count: Some(n),
                            fill: None,
                        };
                        self.note = "ARRAYPOLAR — fill angle degrees <360>".into();
                        return;
                    }
                }
                (Some(n), None) => {
                    if let Ok(deg) = text.parse::<f64>() {
                        self.tool = Tool::ArrayPolar {
                            collecting: false,
                            center: Some(c),
                            count: Some(n),
                            fill: Some(deg),
                        };
                        self.note = "ARRAYPOLAR — Rotate items as copied? [Yes/No] <Y>".into();
                        return;
                    }
                }
                (Some(n), Some(f)) => {
                    let yes = matches!(up.as_str(), "Y" | "YES") || text.is_empty();
                    let no = matches!(up.as_str(), "N" | "NO");
                    if yes || no {
                        self.finish_array_polar(c, n, f, yes);
                        return;
                    }
                }
            }
        }
        // ARRAYPATH typed count / align flag.
        if let Tool::ArrayPath {
            collecting: false,
            path: Some(pid),
            count,
        } = self.tool
        {
            match count {
                None => {
                    if let Ok(n) = text.parse::<usize>() {
                        if n < 2 {
                            self.note = "ARRAYPATH — count must be at least 2".into();
                            return;
                        }
                        self.tool = Tool::ArrayPath {
                            collecting: false,
                            path: Some(pid),
                            count: Some(n),
                        };
                        self.note = "ARRAYPATH — Align items to path? [Yes/No] <Y>".into();
                        return;
                    }
                }
                Some(n) => {
                    let yes = matches!(up.as_str(), "Y" | "YES") || text.is_empty();
                    let no = matches!(up.as_str(), "N" | "NO");
                    if yes || no {
                        self.finish_array_path(pid, n, yes);
                        return;
                    }
                }
            }
        }
        // ROTATE: Reference keyword or typed degrees after pivot.
        if let Tool::Rotate {
            collecting: false,
            pivot: Some(piv),
            reference,
            ref_a: _,
            ref_ang,
        } = self.tool.clone()
        {
            if (up == "R" || up == "REFERENCE" || up == "REF") && !reference && ref_ang.is_none() {
                self.tool = Tool::Rotate {
                    collecting: false,
                    pivot: Some(piv),
                    reference: true,
                    ref_a: None,
                    ref_ang: None,
                };
                self.note = "ROTATE Reference — click first point of reference angle".into();
                return;
            }
            if let Ok(deg) = text.parse::<f64>() {
                let angle = if !reference && ref_ang.is_none() {
                    deg
                } else if reference && ref_ang.is_some() {
                    deg - ref_ang.unwrap()
                } else {
                    return;
                };
                let sfx = self.take_copy_suffix();
                self.sync_selection();
                self.session
                    .feed(&format!("ROTATE SELECTION {} {} {}{}", piv.x, piv.y, angle, sfx));
                self.tool = Tool::Select;
                self.engine.complete();
                return;
            }
        }
        // SCALE: typed factor (unitless) or Reference keyword / new length.
        if let Tool::Scale {
            collecting: false,
            pivot: Some(piv),
            ref_a: None,
            old_len: None,
            ..
        } = self.tool
        {
            if up == "R" || up == "REFERENCE" || up == "REF" {
                self.note = "SCALE Reference — click first point of reference length".into();
                return;
            }
            if let Ok(factor) = text.parse::<f64>() {
                if factor.abs() > f64::EPSILON {
                    let sfx = self.take_copy_suffix();
                    self.sync_selection();
                    self.session
                        .feed(&format!("SCALE SELECTION {} {} {}{}", piv.x, piv.y, factor, sfx));
                    self.tool = Tool::Select;
                    self.engine.complete();
                    return;
                }
            }
        }
        if let Tool::Scale {
            collecting: false,
            pivot: Some(piv),
            ref_a: None,
            old_len: Some(old),
            ..
        } = self.tool
        {
            if let Ok(new_len) = self.parse_model_length(&text) {
                if old > f64::EPSILON && new_len.abs() > f64::EPSILON {
                    let factor = new_len / old;
                    let sfx = self.take_copy_suffix();
                    self.sync_selection();
                    self.session
                        .feed(&format!("SCALE SELECTION {} {} {}{}", piv.x, piv.y, factor, sfx));
                    self.tool = Tool::Select;
                    self.engine.complete();
                    return;
                }
            }
        }
        // OFFSET / FILLET wait for a typed distance/radius before picks.
        if let Tool::Offset {
            dist: None,
            entity: None,
        } = self.tool
        {
            if let Ok(d) = self.parse_model_length(&text) {
                if d > 0.0 {
                    self.tool = Tool::Offset {
                        dist: Some(d),
                        entity: None,
                    };
                    self.engine.advance_step();
                    return;
                }
            }
        }
        if let Tool::Fillet {
            radius: None,
            first: None,
        } = self.tool
        {
            if let Ok(r) = self.parse_model_length(&text) {
                if r >= 0.0 {
                    self.tool = Tool::Fillet {
                        radius: Some(r),
                        first: None,
                    };
                    return;
                }
            }
        }
        if let Tool::Wall {
            thickness: None,
            points,
        } = &self.tool
        {
            if let Ok(t) = self.parse_model_length(&text) {
                if t > 0.0 {
                    let pts = points.clone();
                    self.tool = Tool::Wall {
                        thickness: Some(t),
                        points: pts,
                    };
                    self.engine.advance_step();
                    self.note = format!("WALL thickness {t}");
                    return;
                }
            }
        }
        // TEXT / MLEADER: after the insertion point, Enter commits the label.
        if let Tool::Text { origin: Some(o) } = self.tool {
            let layer = self.layer();
            self.session
                .feed(&format!("TEXT {} {} 0.25 {} {}", o.x, o.y, layer, text));
            self.tool = Tool::Select;
            return;
        }
        // Room label: name first when custom / ROOMLABEL with empty type.
        if let Tool::RoomLabel {
            room_type,
            first: None,
        } = &self.tool
        {
            if room_type.is_empty() && !text.is_empty() {
                self.tool = Tool::RoomLabel {
                    room_type: text,
                    first: None,
                };
                self.note = "ROOM — pick first corner".into();
                return;
            }
        }
        // BLOCK / WBLOCK: type name after selection.
        if let Tool::MakeBlock {
            erase,
            collecting: false,
            name: None,
        } = self.tool
        {
            let name = text.split_whitespace().next().unwrap_or("").to_string();
            if !name.is_empty() {
                self.tool = Tool::MakeBlock {
                    erase,
                    collecting: false,
                    name: Some(name.clone()),
                };
                self.note = format!(
                    "{} {name} — pick base point",
                    if erase { "WBLOCK" } else { "BLOCK" }
                );
                return;
            }
        }
        if let Tool::Mleader {
            arrow: Some(a),
            landing: Some(l),
            text_pt: Some(t),
        } = self.tool
        {
            self.app_feed(&format!(
                "MLEADER {} {} {} {} {} {} 0.25 {}",
                a.x, a.y, l.x, l.y, t.x, t.y, text
            ));
            self.tool = Tool::Select;
            return;
        }
        if let Tool::Callout {
            arrow: Some(a),
            center: Some(c),
        } = self.tool
        {
            let label = if text.trim().is_empty() {
                "1".to_string()
            } else {
                text
            };
            self.app_feed(&format!(
                "CALLOUT {} {} {} {} {}",
                a.x, a.y, c.x, c.y, label
            ));
            self.tool = Tool::Select;
            return;
        }
        // Direct distance entry: typed length along the crosshair from the anchor
        // (supports `12'-6"` / bare inches when units are ft-in).
        if self.drafting() {
            if let Ok(dist) = self.parse_model_length(&text) {
                if let (Some(a), Some(c)) = (self.tool_anchor(), self.cursor) {
                    if let Some(dir) = (c - a).normalized() {
                        self.on_pick(a + dir * dist);
                        return;
                    }
                }
            }
        }
        self.last_line = Some(text.clone());
        self.dispatch(&text);
    }

    fn clear_dyn_coords(&mut self) {
        self.dyn_x.clear();
        self.dyn_y.clear();
        self.dyn_focus = CoordFocus::X;
    }

    /// True while a draw tool wants an absolute typed point (not label text).
    fn expects_point_coords(&self) -> bool {
        if !self.drafting() {
            return false;
        }
        !matches!(
            self.tool,
            Tool::Text { origin: Some(_) }
                | Tool::Mleader {
                    arrow: Some(_),
                    landing: Some(_),
                    text_pt: Some(_),
                }
                | Tool::Callout {
                    arrow: Some(_),
                    center: Some(_),
                }
                | Tool::Offset {
                    dist: None,
                    entity: None,
                }
                | Tool::Fillet {
                    radius: None,
                    first: None,
                }
                | Tool::Wall {
                    thickness: None,
                    ..
                }
                | Tool::MakeBlock {
                    collecting: false,
                    name: None,
                    ..
                }
                | Tool::ArrayRect {
                    collecting: false,
                    spacing_a: None,
                    ..
                }
                | Tool::ArrayPolar {
                    collecting: false,
                    center: Some(_),
                    ..
                }
                | Tool::ArrayPath {
                    collecting: false,
                    path: Some(_),
                    ..
                }
        )
    }

    fn push_dyn_char(&mut self, ch: char) {
        if self.expects_point_coords() {
            match self.dyn_focus {
                CoordFocus::X => self.dyn_x.push(ch),
                CoordFocus::Y => self.dyn_y.push(ch),
            }
            // Keep dyn_input in sync for comma-entry convenience on the active field.
            self.dyn_input.clear();
            self.dyn_input.push_str(&self.dyn_x);
            if !self.dyn_y.is_empty() || self.dyn_focus == CoordFocus::Y {
                self.dyn_input.push(',');
                self.dyn_input.push_str(&self.dyn_y);
            }
        } else {
            self.dyn_input.push(ch);
        }
    }

    fn pop_dyn_char(&mut self) {
        if self.expects_point_coords() {
            match self.dyn_focus {
                CoordFocus::X => {
                    self.dyn_x.pop();
                }
                CoordFocus::Y => {
                    self.dyn_y.pop();
                }
            }
            self.dyn_input.clear();
            self.dyn_input.push_str(&self.dyn_x);
            if !self.dyn_y.is_empty() || self.dyn_focus == CoordFocus::Y {
                self.dyn_input.push(',');
                self.dyn_input.push_str(&self.dyn_y);
            }
        } else {
            self.dyn_input.pop();
        }
    }

    /// Seed plot dialog fields from drawing properties / suggested notes.
    pub(super) fn seed_plot_dialog(&mut self) {
        let props = self.session.document().properties().clone();
        self.props_edit = props.clone();
        self.plot_drawing_name = if props.title.trim().is_empty() {
            "Drawing".into()
        } else {
            props.title.clone()
        };
        self.plot_include_notes = props.plot_include_notes.trim() != "0";
        self.plot_nameplate = if props.nameplate_preset.trim().is_empty() {
            "professional".into()
        } else {
            props.nameplate_preset.clone()
        };
        if props.standard_notes.trim().is_empty() {
            let det = aadt_detect::detect(
                self.session.document(),
                &aadt_detect::DetectionConfig::default(),
            );
            let floors = self.session.document().floors();
            self.plot_notes = aadt_docs::notes_to_text(
                &aadt_docs::NotesLibrary::standard().suggest_for_drawing(&det, &props, &floors),
            );
        } else {
            self.plot_notes = props.standard_notes.clone();
        }
        self.plot_open = true;
    }

    fn has_typed_dyn(&self) -> bool {
        !self.dyn_input.trim().is_empty()
            || !self.dyn_x.trim().is_empty()
            || !self.dyn_y.trim().is_empty()
    }

    fn start_command_id(&mut self, id: command_engine::CommandId) {
        // Fresh command: clear any armed ROTATE/SCALE copy toggle and RECTANGLE
        // corner treatment.
        self.xform_copy = false;
        self.rect_corner = RectCorner::Square;
        self.rect_corner_pending = None;
        self.fillet_notrim = false;
        self.ttr_lines.clear();
        if let Some(def) = self.engine.start(id) {
            let tool_token = def.tool_token;
            let cmd_id = def.id;
            let has_sel = !self.selection.is_empty();
            match tool_for(tool_token, has_sel) {
                Some(t) => {
                    let skip_selection = matches!(
                        t,
                        Tool::Move {
                            collecting: false,
                            ..
                        } | Tool::Copy {
                            collecting: false,
                            ..
                        } | Tool::Rotate {
                            collecting: false,
                            ..
                        } | Tool::Scale {
                            collecting: false,
                            ..
                        } | Tool::Mirror {
                            collecting: false,
                            ..
                        } | Tool::ArrayRect {
                            collecting: false,
                            ..
                        } | Tool::ArrayPolar {
                            collecting: false,
                            ..
                        } | Tool::ArrayPath {
                            collecting: false,
                            ..
                        } | Tool::Stretch {
                            collecting: false,
                            ..
                        }
                    );
                    if matches!(
                        t,
                        Tool::Move { .. }
                            | Tool::Copy { .. }
                            | Tool::Rotate { .. }
                            | Tool::Scale { .. }
                            | Tool::Mirror { .. }
                            | Tool::ArrayRect { .. }
                            | Tool::ArrayPolar { .. }
                            | Tool::ArrayPath { .. }
                            | Tool::Stretch { .. }
                    ) {
                        self.sync_selection();
                    }
                    self.tool = t;
                    self.last_line = Some(tool_token.to_string());
                    self.clear_dyn_coords();
                    self.dyn_input.clear();
                    if skip_selection {
                        self.engine.advance_step();
                    }
                    self.note = format!(
                        "{} — {}",
                        cmd_id.to_ascii_uppercase(),
                        self.engine.prompt().unwrap_or("ready")
                    );
                }
                None => {
                    // Inquiry / manager / CLI-backed commands with no interactive Tool yet.
                    if cmd_id == "list" {
                        let fb = self.app_feed("LIST");
                        self.engine.complete();
                        self.tool = Tool::Select;
                        self.note = fb
                            .message()
                            .map(|m| m.to_string())
                            .unwrap_or_else(|| "LIST complete".into());
                        return;
                    }
                    if cmd_id == "dimstyle" {
                        self.engine.complete();
                        self.show_layer_mgr = true;
                        self.right_tab = 5;
                        self.props_edit = self.session.document().properties().clone();
                        self.note = "Dimension style / Dims manager".into();
                        self.persist_prefs();
                        return;
                    }
                    self.engine.cancel();
                    self.note = format!(
                        "{} — not available interactively",
                        cmd_id.to_ascii_uppercase()
                    );
                }
            }
        }
    }

    fn open_palette(&mut self) {
        self.palette_open = true;
        self.palette_filter.clear();
        self.space_gesture.cancel_press();
    }

    fn flush_open_geometry(&mut self) {
        match &self.tool {
            Tool::Wall {
                thickness: Some(th),
                points,
            } if points.len() >= 2 => {
                let pts = points.clone();
                let th = *th;
                let coords: Vec<String> = pts
                    .iter()
                    .flat_map(|q| [q.x.to_string(), q.y.to_string()])
                    .collect();
                let fb = self
                    .session
                    .feed(&format!("POLYLINE {} A-WALL", coords.join(" ")));
                if let Some(id) = parse_entity_id_from_feedback(fb.message()) {
                    self.session
                        .feed(&format!("EATTEDIT {id} AADT-WALL-THICKNESS {th}"));
                    self.session.feed(&format!("EATTEDIT {id} AADT-ELEM wall"));
                }
            }
            Tool::Polyline { points } if points.len() >= 2 => {
                let layer = self.layer();
                let coords: Vec<String> = points
                    .iter()
                    .flat_map(|q| [q.x.to_string(), q.y.to_string()])
                    .collect();
                self.session
                    .feed(&format!("POLYLINE {} {}", coords.join(" "), layer));
            }
            Tool::RegionSelect {
                points,
                crossing,
                fence,
            } => {
                let doc = self.session.document();
                let hits = if *fence {
                    select_by_fence(doc, points)
                } else {
                    let mode = if *crossing {
                        RegionMode::Crossing
                    } else {
                        RegionMode::Window
                    };
                    select_in_polygon(doc, points, mode)
                };
                let n = hits.len();
                self.selection = hits;
                self.sync_selection();
                self.note = format!("{n} object(s) selected");
                self.tool = Tool::Select;
            }
            Tool::Brush { points: Some(pts) } if pts.len() >= 2 => {
                let coords: Vec<String> = pts
                    .iter()
                    .flat_map(|q| [q.x.to_string(), q.y.to_string()])
                    .collect();
                self.session
                    .feed(&format!("POLYLINE {} A-PRES-MARK", coords.join(" ")));
            }
            _ => {}
        }
    }

    fn apply_space_single(&mut self) {
        if self.palette_open {
            self.activate_palette_selection();
            return;
        }
        // MIRROR erase prompt: empty Space = No (keep source).
        if matches!(
            self.tool,
            Tool::Mirror {
                collecting: false,
                a: Some(_),
                b: Some(_),
            }
        ) {
            self.commit_dyn();
            return;
        }
        // ARRAYPOLAR / ARRAYPATH Y-N or fill defaults via commit_dyn empty path.
        if matches!(
            self.tool,
            Tool::ArrayPolar {
                collecting: false,
                center: Some(_),
                count: Some(_),
                ..
            } | Tool::ArrayPath {
                collecting: false,
                path: Some(_),
                count: Some(_),
            }
        ) {
            self.commit_dyn();
            return;
        }
        if self.has_typed_dyn() {
            self.commit_dyn();
            return;
        }
        // Hot grip: Space cycles the edit mode (Stretch→Move→Rotate→Scale→Mirror).
        if let Tool::GripEdit {
            id,
            grip_idx,
            origin,
            mode,
            ..
        } = self.tool
        {
            let mode = mode.next();
            // Entering Scale: capture the base→cursor grab as the 1.0 reference.
            let ref_dist = if mode == GripMode::Scale {
                self.cursor.map(|c| origin.distance(c)).unwrap_or(0.0)
            } else {
                0.0
            };
            self.tool = Tool::GripEdit {
                id,
                grip_idx,
                origin,
                mode,
                ref_dist,
            };
            self.note = format!(
                "GRIP {} — specify point (Space cycles mode; Esc cancels)",
                mode.label()
            );
            return;
        }
        if self.tool_is_collecting() {
            let _ = self.end_collecting_phase();
            return;
        }
        // AutoCAD-style phase advance before engine finish rules.
        if let Tool::Trim {
            phase: TrimPhase::CuttingEdges,
            cutters,
        } = &self.tool
        {
            let cutters = cutters.clone();
            self.tool = Tool::Trim {
                cutters,
                phase: TrimPhase::Objects,
            };
            self.engine.advance_step();
            self.note = "TRIM — click or drag across objects to trim (Shift extends; Space finishes)".into();
            return;
        }
        if matches!(
            self.tool,
            Tool::Copy {
                collecting: false,
                base: Some(_),
                ..
            }
        ) {
            self.tool = Tool::Select;
            self.engine.complete();
            self.note = "Command complete".into();
            return;
        }
        if matches!(self.tool, Tool::Extend { boundary: Some(_) }) {
            self.tool = Tool::Select;
            self.engine.complete();
            self.note = "Command complete".into();
            return;
        }
        if matches!(
            self.tool,
            Tool::Offset {
                dist: Some(_),
                entity: None,
            }
        ) {
            self.tool = Tool::Select;
            self.engine.complete();
            self.note = "Command complete".into();
            return;
        }
        if matches!(self.tool, Tool::DimContinue) {
            self.tool = Tool::Select;
            self.engine.complete();
            self.note = "DIMCONTINUE complete".into();
            return;
        }
        if matches!(
            self.tool,
            Tool::Trim {
                phase: TrimPhase::Objects,
                ..
            } | Tool::Xline { .. }
        ) {
            self.tool = Tool::Select;
            self.engine.complete();
            self.note = "Command complete".into();
            return;
        }
        if self.engine.is_active() {
            let can_finish = self
                .engine
                .current_id()
                .and_then(command_engine::find_def)
                .map(|d| command_engine::can_finish(d, self.engine.point_count))
                .unwrap_or(false);
            if can_finish {
                self.flush_open_geometry();
                self.engine.empty_space_finish();
                self.tool = Tool::Select;
                self.note = "Command complete".into();
                return;
            }
            self.commit_dyn();
            if matches!(self.tool, Tool::Select) {
                self.engine.complete();
            }
            return;
        }
        if let Some(def) = self.engine.repeat_previous() {
            let id = def.id;
            self.start_command_id(id);
        } else {
            self.commit_dyn();
        }
    }

    fn apply_space_double(&mut self) {
        if self.palette_open {
            self.palette_open = false;
            return;
        }
        self.flush_open_geometry();
        self.tool = Tool::Select;
        self.clear_dyn_coords();
        self.dyn_input.clear();
        if let Some(def) = self.engine.finish_and_repeat() {
            let id = def.id;
            self.start_command_id(id);
            self.note = format!("Repeat {}", id.to_ascii_uppercase());
        } else {
            self.engine.cancel();
            self.note = "Nothing to repeat".into();
        }
    }

    fn activate_palette_selection(&mut self) {
        let filter = self.palette_filter.clone();
        let id = if filter.trim().is_empty() {
            self.engine.history.front().copied()
        } else {
            command_engine::resolve_fuzzy(&filter).map(|hit| match hit {
                command_engine::ResolveHit::Exact(id) | command_engine::ResolveHit::Fuzzy(id) => id,
            })
        };
        self.palette_open = false;
        self.palette_filter.clear();
        if let Some(id) = id {
            self.start_command_id(id);
        }
    }

    fn palette_candidates(&self) -> Vec<&'static command_engine::CommandDef> {
        let filter = self.palette_filter.trim().to_ascii_uppercase();
        let mut out: Vec<&command_engine::CommandDef> = Vec::new();
        for id in &self.engine.history {
            if let Some(d) = command_engine::find_def(id) {
                if filter.is_empty()
                    || d.id.to_ascii_uppercase().starts_with(&filter)
                    || d.aliases
                        .iter()
                        .any(|a| a.to_ascii_uppercase().starts_with(&filter))
                {
                    out.push(d);
                }
            }
        }
        for d in catalog() {
            if out.iter().any(|x| x.id == d.id) {
                continue;
            }
            if filter.is_empty()
                || d.id.to_ascii_uppercase().starts_with(&filter)
                || d.aliases
                    .iter()
                    .any(|a| a.to_ascii_uppercase().starts_with(&filter))
            {
                out.push(d);
            }
            if out.len() >= 12 {
                break;
            }
        }
        out
    }

    pub(super) fn draw_command_palette(&mut self, ctx: &egui::Context) {
        if !self.palette_open {
            return;
        }
        let mut open = true;
        egui::Window::new("Commands")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(theme::color::LAYER_CARD)
                    .stroke(egui::Stroke::new(theme::stroke::THIN, theme::color::BORDER))
                    .rounding(theme::radius::DIALOG),
            )
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new("Space = run · Esc = close · type to filter")
                        .small()
                        .weak(),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut self.palette_filter)
                        .hint_text("L, WA, OFF…")
                        .desired_width(280.0),
                );
                ui.separator();
                let candidates: Vec<_> = self
                    .palette_candidates()
                    .into_iter()
                    .map(|d| {
                        (
                            d.id,
                            d.aliases.first().copied().unwrap_or(d.id),
                            d.capabilities().summary(),
                        )
                    })
                    .collect();
                for (id, alias, caps) in candidates {
                    let label = format!("{}  ({})", id.to_ascii_uppercase(), alias);
                    // Hover reveals the §33 capability contract for the command.
                    if ui
                        .selectable_label(false, label)
                        .on_hover_text(&caps)
                        .clicked()
                    {
                        self.palette_open = false;
                        self.palette_filter.clear();
                        self.start_command_id(id);
                        return;
                    }
                }
            });
        if !open {
            self.palette_open = false;
        }
    }

    fn layer(&self) -> String {
        // Drafting always targets the active model-space category (layer is a shim).
        self.session.document().active_category_layer()
    }

    fn drafting(&self) -> bool {
        !matches!(self.tool, Tool::Select)
    }

    /// Push the GUI selection into the session as `SELECTION` so noun-verb edits
    /// (which target `SELECTION`) act on exactly the highlighted set.
    fn sync_selection(&mut self) {
        if self.selection.is_empty() {
            return;
        }
        let ids: Vec<String> = self.selection.iter().map(|id| id.0.to_string()).collect();
        self.app_feed(&format!("SELECT {}", ids.join(" ")));
    }

    /// AutoCAD ROTATE/SCALE `[C]opy`: return the trailing `" COPY"` argument when
    /// the copy option is armed, clearing it so it applies to exactly one run.
    fn take_copy_suffix(&mut self) -> &'static str {
        if self.xform_copy {
            self.xform_copy = false;
            " COPY"
        } else {
            ""
        }
    }

    /// Apply a hot-grip edit at the confirming point `p`. Stretch moves the
    /// picked grip on its entity; the other modes transform the whole selection
    /// about the grip `origin` (base point), reusing the MOVE/ROTATE/SCALE/MIRROR
    /// commands so each is a single UNDO. AutoCAD hot-grip semantics.
    fn apply_grip_edit(
        &mut self,
        id: EntityId,
        grip_idx: usize,
        origin: Point2,
        mode: GripMode,
        ref_dist: f64,
        p: Point2,
    ) {
        match mode {
            GripMode::Stretch => {
                self.app_feed(&format!("GRIPEDIT {} {} {} {}", id.0, grip_idx, p.x, p.y));
            }
            GripMode::Move => {
                self.sync_selection();
                let (dx, dy) = (p.x - origin.x, p.y - origin.y);
                self.app_feed(&format!("MOVE SELECTION {dx} {dy}"));
            }
            GripMode::Rotate => {
                self.sync_selection();
                let angle = (p.y - origin.y).atan2(p.x - origin.x).to_degrees();
                self.app_feed(&format!(
                    "ROTATE SELECTION {} {} {angle}",
                    origin.x, origin.y
                ));
            }
            GripMode::Scale => {
                self.sync_selection();
                // Factor relative to the grab distance captured on entering Scale
                // mode. If that was ~0 (cursor near the grip), fall back to the
                // entity's grip spread so a drag still scales sensibly.
                let reference = if ref_dist > 1e-9 {
                    ref_dist
                } else {
                    self.session
                        .document()
                        .get(id)
                        .map(|e| {
                            grip_points(&e.geometry)
                                .iter()
                                .map(|g| origin.distance(*g))
                                .fold(0.0_f64, f64::max)
                        })
                        .filter(|d| *d > 1e-9)
                        .unwrap_or(1.0)
                };
                let factor = origin.distance(p) / reference;
                if factor > 1e-9 && (factor - 1.0).abs() > 1e-9 {
                    self.app_feed(&format!(
                        "SCALE SELECTION {} {} {factor}",
                        origin.x, origin.y
                    ));
                } else {
                    self.note = "GRIP SCALE — move away from the grip to set a factor".into();
                }
            }
            GripMode::Mirror => {
                self.sync_selection();
                // Mirror across the base→point axis, keeping the source (N).
                self.app_feed(&format!(
                    "MIRROR SELECTION {} {} {} {} N",
                    origin.x, origin.y, p.x, p.y
                ));
            }
        }
    }

    fn tool_is_collecting(&self) -> bool {
        matches!(
            self.tool,
            Tool::Move {
                collecting: true,
                ..
            } | Tool::Copy {
                collecting: true,
                ..
            } | Tool::Rotate {
                collecting: true,
                ..
            } | Tool::Scale {
                collecting: true,
                ..
            } | Tool::Mirror {
                collecting: true,
                ..
            } | Tool::ArrayRect {
                collecting: true,
                ..
            } | Tool::ArrayPolar {
                collecting: true,
                ..
            } | Tool::ArrayPath {
                collecting: true,
                ..
            } | Tool::MakeBlock {
                collecting: true,
                ..
            } | Tool::Stretch {
                collecting: true,
                ..
            } | Tool::Erase
        )
    }

    fn collect_selection_pick(&mut self, world: Point2) {
        let tol = PICK_PIXELS / self.camera.scale;
        if let Some(id) = pick_entity(self.session.document(), world, tol) {
            if let Some(i) = self.selection.iter().position(|x| *x == id) {
                self.selection.remove(i);
            } else {
                self.selection.push(id);
            }
            self.note = format!("{} object(s) — Space when done", self.selection.len());
        }
    }

    fn end_collecting_phase(&mut self) -> bool {
        if self.selection.is_empty() {
            self.note = "Select objects, then Space".into();
            return false;
        }
        self.sync_selection();
        match &self.tool {
            Tool::Move { .. } => {
                self.tool = Tool::Move {
                    collecting: false,
                    base: None,
                };
                self.engine.advance_step();
                self.note = "MOVE — base point".into();
            }
            Tool::Copy { .. } => {
                self.tool = Tool::Copy {
                    collecting: false,
                    base: None,
                };
                self.engine.advance_step();
                self.note = "COPY — base point".into();
            }
            Tool::Rotate { .. } => {
                self.tool = Tool::Rotate {
                    collecting: false,
                    pivot: None,
                    reference: false,
                    ref_a: None,
                    ref_ang: None,
                };
                self.engine.advance_step();
                self.note = "ROTATE — base point".into();
            }
            Tool::Scale { .. } => {
                self.tool = Tool::Scale {
                    collecting: false,
                    pivot: None,
                    ref_a: None,
                    old_len: None,
                };
                self.engine.advance_step();
                self.note = "SCALE — base point".into();
            }
            Tool::Mirror { .. } => {
                self.tool = Tool::Mirror {
                    collecting: false,
                    a: None,
                    b: None,
                };
                self.engine.advance_step();
                self.note = "MIRROR — first axis point".into();
            }
            Tool::ArrayRect { .. } => {
                self.tool = Tool::ArrayRect {
                    collecting: false,
                    rows: None,
                    cols: None,
                    spacing_a: None,
                    dx: None,
                };
                self.engine.advance_step();
                self.note = "ARRAYRECT — rows (type + Space)".into();
            }
            Tool::ArrayPolar { .. } => {
                self.tool = Tool::ArrayPolar {
                    collecting: false,
                    center: None,
                    count: None,
                    fill: None,
                };
                self.engine.advance_step();
                self.note = "ARRAYPOLAR — center point".into();
            }
            Tool::ArrayPath { .. } => {
                self.tool = Tool::ArrayPath {
                    collecting: false,
                    path: None,
                    count: None,
                };
                self.engine.advance_step();
                self.note = "ARRAYPATH — select path (line/pline/arc/circle)".into();
            }
            Tool::Stretch { .. } => {
                self.tool = Tool::Stretch {
                    collecting: false,
                    w0: None,
                    w1: None,
                    base: None,
                };
                self.engine.advance_step();
                self.note = "STRETCH — click first crossing-window corner".into();
            }
            Tool::MakeBlock { erase, .. } => {
                let erase = *erase;
                self.tool = Tool::MakeBlock {
                    erase,
                    collecting: false,
                    name: None,
                };
                self.engine.advance_step();
                self.note = if erase {
                    "WBLOCK — type block name and Space".into()
                } else {
                    "BLOCK — type block name and Space".into()
                };
            }
            Tool::Erase => {
                self.erase_selection();
                self.tool = Tool::Select;
                self.engine.complete();
            }
            _ => {}
        }
        true
    }

    /// Run the command line: start a tool for a bare interactive alias, run a
    /// noun-verb edit, or feed a full typed command. Empty submits repeat the
    /// last command.
    fn submit_command_line(&mut self) {
        let line = self.command.trim().to_string();
        self.command.clear();
        self.refocus = true;
        if line.is_empty() {
            if let Some(last) = self.last_line.clone() {
                self.dispatch(&last);
            }
            return;
        }
        self.last_line = Some(line.clone());
        self.dispatch(&line);
    }

    /// Route a command line: bare interactive alias → tool; `E` → erase the
    /// selection; anything else → feed to the session.
    fn dispatch(&mut self, line: &str) {
        self.note.clear();
        // Snap-override keyword typed at a point prompt (PER/END/MID/INT/TAN/…):
        // arm the one-shot override instead of dispatching a command.
        if !line.contains(' ') && self.try_snap_override(line) {
            return;
        }
        // While ZOOM is active, A/E/W/P/S are options — not new commands (A≠ARC).
        if matches!(self.tool, Tool::ZoomWindow { first: None }) && !line.contains(' ') {
            let up = line.to_ascii_uppercase();
            match up.as_str() {
                "A" | "ALL" | "E" | "EXTENTS" => {
                    self.apply_nav_hint("extents");
                    self.tool = Tool::Select;
                    self.note = if matches!(up.as_str(), "A" | "ALL") {
                        "Zoom All".into()
                    } else {
                        "Zoom Extents".into()
                    };
                    return;
                }
                "W" | "WINDOW" => {
                    self.note = "ZOOM Window — click first corner".into();
                    return;
                }
                "P" | "PREVIOUS" => {
                    self.apply_nav_hint("previous");
                    self.tool = Tool::Select;
                    self.note = "Zoom Previous".into();
                    return;
                }
                "S" | "SCALE" => {
                    self.note = "ZOOM Scale — type factor and Space (e.g. 2 or 0.5x)".into();
                    return;
                }
                _ => {}
            }
        }
        if !line.contains(' ') {
            let up = line.to_ascii_uppercase();
            if matches!(up.as_str(), "E" | "ERASE")
                && !(self.selection.is_empty() && self.selected_hatches.is_empty())
            {
                self.erase_selection();
                return;
            }
            if matches!(up.as_str(), "LA" | "LAYER") {
                self.show_left_mgr = true;
                self.left_tab = 0;
                self.note = "Layer Properties Manager".into();
                return;
            }
            if matches!(up.as_str(), "BLOCKS" | "I" | "INSERT") {
                self.show_left_mgr = true;
                self.left_tab = 1;
                self.note = "Blocks Manager — pick a block to INSERT".into();
                return;
            }
            if matches!(up.as_str(), "D" | "DIMSTYLE") {
                self.show_layer_mgr = true;
                self.right_tab = 5;
                self.props_edit = self.session.document().properties().clone();
                self.note = "Dimension style / Dims manager".into();
                self.persist_prefs();
                return;
            }
            if matches!(up.as_str(), "PR" | "PROPERTIES") {
                self.open_drawing_properties();
                return;
            }
            if matches!(up.as_str(), "Z" | "ZOOM") {
                // AutoCAD: Z Enter starts ZOOM; then A/E/W/P/S (not immediate extents).
                self.tool = Tool::ZoomWindow { first: None };
                self.note =
                    "ZOOM — [A]ll [E]xtents [W]indow [P]revious [S]cale · or click window".into();
                return;
            }
            if matches!(up.as_str(), "P" | "PAN") {
                self.note = "Pan — drag with the middle mouse button".into();
                return;
            }
            if matches!(up.as_str(), "IROOM" | "MROOM") {
                self.show_room_picker = true;
                self.note = format!("{up} — select a room type");
                return;
            }
            if let Some(id) = resolve_alias(line) {
                self.start_command_id(id);
                return;
            }
            if let Some(t) = tool_for(line, !self.selection.is_empty()) {
                let skip_selection = matches!(
                    t,
                    Tool::Move {
                        collecting: false,
                        ..
                    } | Tool::Copy {
                        collecting: false,
                        ..
                    } | Tool::Rotate {
                        collecting: false,
                        ..
                    } | Tool::Scale {
                        collecting: false,
                        ..
                    } | Tool::Mirror {
                        collecting: false,
                        ..
                    } | Tool::ArrayRect {
                        collecting: false,
                        ..
                    } | Tool::ArrayPolar {
                        collecting: false,
                        ..
                    } | Tool::ArrayPath {
                        collecting: false,
                        ..
                    } | Tool::Stretch {
                        collecting: false,
                        ..
                    } | Tool::MakeBlock {
                        collecting: false,
                        ..
                    }
                );
                if matches!(
                    t,
                    Tool::Move { .. }
                        | Tool::Copy { .. }
                        | Tool::Rotate { .. }
                        | Tool::Scale { .. }
                        | Tool::Mirror { .. }
                        | Tool::ArrayRect { .. }
                        | Tool::ArrayPolar { .. }
                        | Tool::ArrayPath { .. }
                        | Tool::Stretch { .. }
                ) {
                    self.sync_selection();
                }
                self.tool = t;
                if skip_selection {
                    self.engine.advance_step();
                }
                // Seed door/window/etc. blocks so PlaceElem rubber-band ghosts work.
                if matches!(self.tool, Tool::PlaceElem { .. }) {
                    aadt_command::ensure_place_blocks(self.session.document_mut());
                }
                if let Tool::ZoomWindow { first: None } = self.tool {
                    self.note =
                        "ZOOM — [A]ll [E]xtents [W]indow [P]revious [S]cale · or click window"
                            .into();
                }
                if let Tool::MakeBlock {
                    erase,
                    collecting: false,
                    name: None,
                } = &self.tool
                {
                    self.note = if *erase {
                        "WBLOCK — type block name and Space".into()
                    } else {
                        "BLOCK — type block name and Space".into()
                    };
                }
                return;
            }
        }
        let fb = self.app_feed(line);
        if let Some(msg) = fb.message() {
            if fb.is_err() {
                self.note = msg.to_string();
            }
        }
        if let Some(hint) = self.session.take_nav_hint() {
            self.apply_nav_hint(&hint);
        }
    }

    fn apply_nav_hint(&mut self, hint: &str) {
        if hint == "extents" || hint == "previous" && self.zoom_prev.is_none() {
            self.zoom_prev = Some(self.camera);
            self.fitted = false;
            return;
        }
        if hint == "previous" {
            if let Some(prev) = self.zoom_prev.take() {
                self.camera = prev;
            }
            return;
        }
        if let Some(rest) = hint.strip_prefix("scale:") {
            if let Ok(f) = rest.parse::<f64>() {
                self.zoom_prev = Some(self.camera);
                self.camera.scale *= f;
            }
            return;
        }
        if let Some(rest) = hint.strip_prefix("window:") {
            let parts: Vec<f64> = rest.split(',').filter_map(|s| s.parse().ok()).collect();
            if parts.len() == 4 {
                self.zoom_prev = Some(self.camera);
                let (x1, y1, x2, y2) = (parts[0], parts[1], parts[2], parts[3]);
                let min = Point2::new(x1.min(x2), y1.min(y2));
                let max = Point2::new(x1.max(x2), y1.max(y2));
                let bw = (max.x - min.x).max(1e-9);
                let bh = (max.y - min.y).max(1e-9);
                let (vw, vh) = self.camera.viewport;
                let sx = vw / bw;
                let sy = vh / bh;
                self.camera.scale = sx.min(sy) * 0.9;
                self.camera.center = Point2::new((min.x + max.x) * 0.5, (min.y + max.y) * 0.5);
            }
            return;
        }
        if let Some(rest) = hint.strip_prefix("pan:") {
            let parts: Vec<f64> = rest.split(',').filter_map(|s| s.parse().ok()).collect();
            if parts.len() == 2 {
                self.camera.center.x -= parts[0];
                self.camera.center.y -= parts[1];
            }
        }
    }

    fn erase_selection(&mut self) {
        if self.selection.is_empty() && self.selected_hatches.is_empty() {
            self.note = "Select objects first, then E.".to_string();
            return;
        }
        if !self.selection.is_empty() {
            self.sync_selection();
            self.app_feed("ERASE SELECTION");
            self.selection.clear();
        }
        if !self.selected_hatches.is_empty() {
            // Remove by descending index so earlier removals don't shift later
            // ones; one transaction so a single UNDO restores every hatch.
            let mut idxs = std::mem::take(&mut self.selected_hatches);
            idxs.sort_unstable();
            idxs.dedup();
            let n = idxs.len();
            self.session.document_mut().transact("Erase hatch", |d| {
                for &i in idxs.iter().rev() {
                    d.remove_hatch(i);
                }
            });
            self.note = format!("Erased {n} hatch{}", if n == 1 { "" } else { "es" });
        }
    }

    /// Apply the current Hatches-dock pattern to every selected hatch (the
    /// hatch "edit" flow: pick a hatch, choose a pattern, apply). Undoable.
    pub(super) fn apply_pattern_to_selected_hatches(&mut self) {
        if self.selected_hatches.is_empty() {
            self.note = "Select a hatch first (click inside its fill).".into();
            return;
        }
        let pattern = self.hatch_pattern.clone();
        let idxs = self.selected_hatches.clone();
        let mut changed = 0;
        self.session.document_mut().transact("Edit hatch", |d| {
            for &i in &idxs {
                if d.set_hatch_pattern(i, pattern.clone()) {
                    changed += 1;
                }
            }
        });
        self.note = format!("Set {changed} hatch{} to {pattern}", if changed == 1 { "" } else { "es" });
    }

    /// Advance the active tool with a picked world point.
    fn on_pick(&mut self, p: Point2) {
        // A one-shot snap override applies to exactly this pick; consume it now
        // (`p` already reflects it via `resolve_pick`).
        self.snap_override = None;
        if self.tool_is_collecting() {
            self.collect_selection_pick(p);
            return;
        }
        let layer = self.layer();
        let tool = self.tool.clone();
        match tool {
            Tool::Line { points } => {
                let mut pts = points;
                if let Some(&a) = pts.last() {
                    self.session
                        .feed(&format!("LINE {} {} {} {} {}", a.x, a.y, p.x, p.y, layer));
                }
                pts.push(p);
                self.tool = Tool::Line { points: pts };
                self.engine.accept_point();
            }
            Tool::Circle {
                mode,
                p1,
                p2,
                diameter,
            } => match mode {
                CircleMode::Center => match p1 {
                    Some(c) => {
                        let mut r = c.distance(p);
                        if diameter {
                            r *= 0.5;
                        }
                        if r > f64::EPSILON {
                            self.session
                                .feed(&format!("CIRCLE {} {} {} {}", c.x, c.y, r, layer));
                        }
                        self.tool = Tool::Select;
                        self.engine.accept_point();
                        self.engine.complete();
                    }
                    None => {
                        self.tool = Tool::Circle {
                            mode: CircleMode::Center,
                            p1: Some(p),
                            p2: None,
                            diameter,
                        };
                        self.engine.accept_point();
                        self.note = "CIRCLE — radius or [D]iameter (type + Space)".into();
                    }
                },
                CircleMode::TwoPoint => match (p1, p2) {
                    (None, _) => {
                        self.tool = Tool::Circle {
                            mode: CircleMode::TwoPoint,
                            p1: Some(p),
                            p2: None,
                            diameter: false,
                        };
                        self.note = "CIRCLE 2P — second point of diameter".into();
                    }
                    (Some(a), None) => {
                        let c = a.midpoint(p);
                        let r = a.distance(p) * 0.5;
                        if r > f64::EPSILON {
                            self.session
                                .feed(&format!("CIRCLE {} {} {} {}", c.x, c.y, r, layer));
                        }
                        self.tool = Tool::Select;
                        self.engine.complete();
                    }
                    _ => {}
                },
                CircleMode::ThreePoint => match (p1, p2) {
                    (None, _) => {
                        self.tool = Tool::Circle {
                            mode: CircleMode::ThreePoint,
                            p1: Some(p),
                            p2: None,
                            diameter: false,
                        };
                        self.note = "CIRCLE 3P — second point".into();
                    }
                    (Some(a), None) => {
                        self.tool = Tool::Circle {
                            mode: CircleMode::ThreePoint,
                            p1: Some(a),
                            p2: Some(p),
                            diameter: false,
                        };
                        self.note = "CIRCLE 3P — third point".into();
                    }
                    (Some(a), Some(b)) => {
                        if let Some((c, r)) = circle_from_3_points(a, b, p) {
                            self.session
                                .feed(&format!("CIRCLE {} {} {} {}", c.x, c.y, r, layer));
                            self.tool = Tool::Select;
                            self.engine.complete();
                        } else {
                            self.note = "CIRCLE 3P — points are collinear; pick again".into();
                        }
                    }
                },
                // TTR: collect two tangent lines, then a typed radius finishes.
                CircleMode::Ttr => {
                    let tol = PICK_PIXELS / self.camera.scale;
                    match pick_entity(self.session.document(), p, tol) {
                        Some(id) if !self.ttr_lines.iter().any(|(e, _)| *e == id) => {
                            self.ttr_lines.push((id, p));
                            self.note = if self.ttr_lines.len() < 2 {
                                "CIRCLE TTR — pick the second tangent line".into()
                            } else {
                                "CIRCLE TTR — type the radius and Space".into()
                            };
                        }
                        _ => {
                            self.note = "CIRCLE TTR — pick a line tangent to the circle".into();
                        }
                    }
                }
            },
            Tool::GripEdit {
                id,
                grip_idx,
                origin,
                mode,
                ref_dist,
            } => {
                self.apply_grip_edit(id, grip_idx, origin, mode, ref_dist, p);
                self.tool = Tool::Select;
            }
            Tool::Arc { mode, a, b } => match (a, b) {
                (None, _) => {
                    self.tool = Tool::Arc {
                        mode,
                        a: Some(p),
                        b: None,
                    };
                    self.engine.accept_point();
                }
                (Some(a0), None) => {
                    self.tool = Tool::Arc {
                        mode,
                        a: Some(a0),
                        b: Some(p),
                    };
                    self.engine.accept_point();
                }
                (Some(a0), Some(b0)) => {
                    let arc = match mode {
                        // 3-point: a=start, b=second-on-arc, p=end.
                        ArcMode::ThreePoint => arc_through_3_points(a0, b0, p),
                        // Centre-start-end: a=centre, b=start, p=end direction.
                        ArcMode::CenterStartEnd => {
                            let r = a0.distance(b0);
                            (r > f64::EPSILON).then(|| {
                                let sa = (b0.y - a0.y).atan2(b0.x - a0.x).to_degrees();
                                let ea = (p.y - a0.y).atan2(p.x - a0.x).to_degrees();
                                (a0, r, sa, ea)
                            })
                        }
                    };
                    if let Some((c, r, sa, ea)) = arc {
                        self.app_feed(&format!("ARC {} {} {} {} {} {}", c.x, c.y, r, sa, ea, layer));
                    }
                    self.tool = Tool::Select;
                    self.engine.accept_point();
                    self.engine.complete();
                }
            },
            Tool::Rectangle {
                first,
                phase,
                dim_length,
                dim_width,
            } => match (phase, first, dim_length, dim_width) {
                (RectPhase::DimWidth, None, Some(len), Some(w)) => {
                    self.place_rectangle_dims(p, len, w, &layer);
                }
                (RectPhase::FirstCorner, None, _, _) => {
                    self.tool = Tool::Rectangle {
                        first: Some(p),
                        phase: RectPhase::OppositeCorner,
                        dim_length: None,
                        dim_width: None,
                    };
                    self.engine.accept_point();
                    self.note = "RECTANGLE — opposite corner or @W,H (D = dimensions)".into();
                }
                (RectPhase::OppositeCorner, Some(a), _, _)
                | (RectPhase::DimWidth, Some(a), _, _) => {
                    self.place_rectangle(a, p, &layer);
                }
                (RectPhase::DimLength, _, _, _) | (RectPhase::DimWidth, None, _, _) => {
                    self.tool = Tool::Rectangle {
                        first: Some(p),
                        phase,
                        dim_length,
                        dim_width,
                    };
                    self.note = if phase == RectPhase::DimLength {
                        "RECTANGLE — type length and Space"
                    } else {
                        "RECTANGLE — type width and Space"
                    }
                    .into();
                }
                _ => {
                    self.tool = Tool::Rectangle {
                        first: Some(p),
                        phase: RectPhase::OppositeCorner,
                        dim_length: None,
                        dim_width: None,
                    };
                }
            },
            Tool::Xline {
                origin,
                mode,
                ang_deg,
            } => {
                let layer = layer.clone();
                self.on_xline_pick(p, origin, mode, ang_deg, &layer);
            }
            Tool::Move {
                collecting: false,
                base,
            } => match base {
                Some(b) => {
                    self.session
                        .feed(&format!("MOVE SELECTION {} {}", p.x - b.x, p.y - b.y));
                    self.tool = Tool::Select;
                    self.engine.accept_point();
                    self.engine.complete();
                }
                None => {
                    self.tool = Tool::Move {
                        collecting: false,
                        base: Some(p),
                    };
                    self.engine.accept_point();
                }
            },
            Tool::Copy {
                collecting: false,
                base,
            } => match base {
                // COPY repeats: each click drops another copy until Esc/Space.
                Some(b) => {
                    self.session
                        .feed(&format!("COPY SELECTION {} {}", p.x - b.x, p.y - b.y));
                    self.engine.accept_point();
                }
                None => {
                    self.tool = Tool::Copy {
                        collecting: false,
                        base: Some(p),
                    };
                    self.engine.accept_point();
                }
            },
            Tool::Rotate {
                collecting: false,
                pivot,
                reference,
                ref_a,
                ref_ang,
            } => match (pivot, reference, ref_a, ref_ang) {
                (None, _, _, _) => {
                    self.tool = Tool::Rotate {
                        collecting: false,
                        pivot: Some(p),
                        reference: false,
                        ref_a: None,
                        ref_ang: None,
                    };
                    self.engine.accept_point();
                    self.note = "ROTATE — type angle or [R]eference".into();
                }
                (Some(piv), false, _, _) => {
                    let ang = (p.y - piv.y).atan2(p.x - piv.x).to_degrees();
                    let sfx = self.take_copy_suffix();
                    self.sync_selection();
                    self.session
                        .feed(&format!("ROTATE SELECTION {} {} {}{}", piv.x, piv.y, ang, sfx));
                    self.tool = Tool::Select;
                    self.engine.accept_point();
                    self.engine.complete();
                }
                (Some(piv), true, None, None) => {
                    self.tool = Tool::Rotate {
                        collecting: false,
                        pivot: Some(piv),
                        reference: true,
                        ref_a: Some(p),
                        ref_ang: None,
                    };
                    self.note = "ROTATE Reference — click second point of reference angle".into();
                }
                (Some(piv), true, Some(a), None) => {
                    let ref_ang_val = (p.y - a.y).atan2(p.x - a.x).to_degrees();
                    self.tool = Tool::Rotate {
                        collecting: false,
                        pivot: Some(piv),
                        reference: true,
                        ref_a: None,
                        ref_ang: Some(ref_ang_val),
                    };
                    self.note =
                        "ROTATE Reference — type new angle or click first point of new angle"
                            .into();
                }
                (Some(piv), true, None, Some(ref_ang_val)) => {
                    self.tool = Tool::Rotate {
                        collecting: false,
                        pivot: Some(piv),
                        reference: true,
                        ref_a: Some(p),
                        ref_ang: Some(ref_ang_val),
                    };
                    self.note = "ROTATE Reference — click second point of new angle".into();
                }
                (Some(piv), true, Some(a), Some(ref_ang_val)) => {
                    let new_ang = (p.y - a.y).atan2(p.x - a.x).to_degrees();
                    let deg = new_ang - ref_ang_val;
                    let sfx = self.take_copy_suffix();
                    self.sync_selection();
                    self.session
                        .feed(&format!("ROTATE SELECTION {} {} {}{}", piv.x, piv.y, deg, sfx));
                    self.tool = Tool::Select;
                    self.engine.complete();
                }
            },
            Tool::Scale {
                collecting: false,
                pivot,
                ref_a,
                old_len,
            } => match (pivot, ref_a, old_len) {
                (None, _, _) => {
                    self.tool = Tool::Scale {
                        collecting: false,
                        pivot: Some(p),
                        ref_a: None,
                        old_len: None,
                    };
                    self.engine.accept_point();
                    self.note = "SCALE — type factor and Space (or R / click for Reference)".into();
                }
                (Some(piv), None, None) => {
                    // Click after base starts Reference (first point of old length).
                    self.tool = Tool::Scale {
                        collecting: false,
                        pivot: Some(piv),
                        ref_a: Some(p),
                        old_len: None,
                    };
                    self.note = "SCALE Reference — click second point of reference length".into();
                }
                (Some(piv), Some(a), None) => {
                    let old = a.distance(p);
                    if old > f64::EPSILON {
                        self.tool = Tool::Scale {
                            collecting: false,
                            pivot: Some(piv),
                            ref_a: None,
                            old_len: Some(old),
                        };
                        self.note =
                            "SCALE Reference — type new length and Space, or click first point"
                                .into();
                    }
                }
                (Some(piv), None, Some(old)) => {
                    self.tool = Tool::Scale {
                        collecting: false,
                        pivot: Some(piv),
                        ref_a: Some(p),
                        old_len: Some(old),
                    };
                    self.note = "SCALE Reference — click second point of new length".into();
                }
                (Some(piv), Some(a), Some(old)) => {
                    let new_len = a.distance(p);
                    if old > f64::EPSILON && new_len > f64::EPSILON {
                        let factor = new_len / old;
                        let sfx = self.take_copy_suffix();
                        self.sync_selection();
                        self.session
                            .feed(&format!("SCALE SELECTION {} {} {}{}", piv.x, piv.y, factor, sfx));
                        self.engine.complete();
                    }
                    self.tool = Tool::Select;
                }
            },
            Tool::Offset {
                dist: Some(d),
                entity: Some(id),
            } => {
                self.session
                    .feed(&format!("OFFSET {} {} {} {}", id.0, d, p.x, p.y));
                self.tool = Tool::Offset {
                    dist: Some(d),
                    entity: None,
                };
            }
            Tool::Text { origin: None } => {
                self.tool = Tool::Text { origin: Some(p) };
                self.note = "TEXT — type the value and Enter".into();
            }
            Tool::Text { origin: Some(_) } => {
                self.note = "TEXT — type the value and Enter".into();
            }
            Tool::Mleader {
                arrow,
                landing,
                text_pt,
            } => match (arrow, landing, text_pt) {
                (None, _, _) => {
                    self.tool = Tool::Mleader {
                        arrow: Some(p),
                        landing: None,
                        text_pt: None,
                    }
                }
                (Some(a), None, _) => {
                    self.tool = Tool::Mleader {
                        arrow: Some(a),
                        landing: Some(p),
                        text_pt: None,
                    }
                }
                (Some(a), Some(l), None) => {
                    self.tool = Tool::Mleader {
                        arrow: Some(a),
                        landing: Some(l),
                        text_pt: Some(p),
                    };
                    self.note = "MLEADER — type the note and Enter".into();
                }
                (Some(_), Some(_), Some(_)) => {
                    self.note = "MLEADER — type the note and Enter".into();
                }
            },
            Tool::Stretch {
                collecting,
                w0,
                w1,
                base,
            } => {
                if collecting {
                    return; // picks handled by collect_selection_pick
                }
                match (w0, w1, base) {
                    (None, _, _) => {
                        self.tool = Tool::Stretch {
                            collecting: false,
                            w0: Some(p),
                            w1: None,
                            base: None,
                        };
                        self.note = "STRETCH — click opposite crossing-window corner".into();
                    }
                    (Some(a), None, _) => {
                        self.tool = Tool::Stretch {
                            collecting: false,
                            w0: Some(a),
                            w1: Some(p),
                            base: None,
                        };
                        self.note = "STRETCH — click the base point".into();
                    }
                    (Some(a), Some(b), None) => {
                        self.tool = Tool::Stretch {
                            collecting: false,
                            w0: Some(a),
                            w1: Some(b),
                            base: Some(p),
                        };
                        self.note = "STRETCH — click the destination".into();
                    }
                    (Some(a), Some(b), Some(c)) => {
                        self.sync_selection();
                        let lo = Point2::new(a.x.min(b.x), a.y.min(b.y));
                        let hi = Point2::new(a.x.max(b.x), a.y.max(b.y));
                        self.app_feed(&format!(
                            "STRETCH {} {} {} {} {} {} SELECTION",
                            lo.x,
                            lo.y,
                            hi.x,
                            hi.y,
                            p.x - c.x,
                            p.y - c.y
                        ));
                        self.tool = Tool::Select;
                    }
                }
            }
            Tool::Dist { first } => match first {
                None => {
                    self.tool = Tool::Dist { first: Some(p) };
                    self.note = "DIST — click second point".into();
                }
                Some(a) => {
                    let fb = self.app_feed(&format!("DIST {} {} {} {}", a.x, a.y, p.x, p.y));
                    if let Some(m) = fb.message() {
                        self.note = m.to_string();
                    }
                    self.tool = Tool::Select;
                }
            },
            Tool::Brush { points } => {
                let mut pts = points.clone().unwrap_or_default();
                if pts.last().map(|q| q.distance(p) > 1e-6).unwrap_or(true) {
                    pts.push(p);
                }
                self.tool = Tool::Brush { points: Some(pts) };
            }
            Tool::Fillet { radius, first } => {
                let r = radius.unwrap_or(0.0);
                let tol = PICK_PIXELS / self.camera.scale;
                let Some(id) = pick_entity(self.session.document(), p, tol) else {
                    self.note = "FILLET — click a line".into();
                    return;
                };
                // A polyline is filleted whole (all corners) — AutoCAD Polyline.
                let is_polyline = matches!(
                    self.session.document().get(id).map(|e| &e.geometry),
                    Some(Geometry::Polyline(_))
                );
                if first.is_none() && is_polyline {
                    if r > f64::EPSILON {
                        self.session.feed(&format!("FILLET P {} {}", id.0, r));
                        self.note =
                            "FILLET — polyline corners rounded; pick another (Esc finishes)".into();
                    } else {
                        self.note = "FILLET — set a non-zero radius before a polyline".into();
                    }
                    self.tool = Tool::Fillet {
                        radius: Some(r),
                        first: None,
                    };
                    return;
                }
                match first {
                    None => {
                        self.tool = Tool::Fillet {
                            radius: Some(r),
                            first: Some(id),
                        }
                    }
                    Some(a) => {
                        let trim = if self.fillet_notrim { " NOTRIM" } else { "" };
                        self.session
                            .feed(&format!("FILLET {} {} {}{}", a.0, id.0, r, trim));
                        // Stay active for more corners (AutoCAD Multiple); radius
                        // and trim mode are retained. Esc finishes.
                        self.tool = Tool::Fillet {
                            radius: Some(r),
                            first: None,
                        };
                        self.note =
                            "FILLET — click the first line of the next corner (Esc finishes)".into();
                    }
                }
            }
            Tool::Insert { name } => {
                let layer = self.layer();
                self.app_feed(&format!("INSERT {} {} {} 0 {}", name, p.x, p.y, layer));
                // Stay in insert mode for multiple placements (AutoCAD-like).
                self.note = format!("INSERT {name} — click to place another (Esc finishes)");
            }
            Tool::MakeBlock {
                erase,
                collecting: false,
                name: Some(ref n),
            } => {
                if self.selection.is_empty() {
                    self.note = "BLOCK — select objects first".into();
                    return;
                }
                self.sync_selection();
                let cmd = if erase { "WBLOCK" } else { "BLOCK" };
                let ids: String = self
                    .selection
                    .iter()
                    .map(|id| id.0.to_string())
                    .collect::<Vec<_>>()
                    .join(" ");
                let fb = self.app_feed(&format!("{cmd} {n} {} {} {ids}", p.x, p.y));
                if let Some(m) = fb.message() {
                    self.note = m.to_string();
                }
                if erase {
                    self.selection.clear();
                }
                self.tool = Tool::Select;
                self.engine.complete();
            }
            Tool::MakeBlock {
                name: None,
                collecting: false,
                ..
            } => {
                self.note = "BLOCK — type block name and Space first".into();
            }
            Tool::RoomLabel { room_type, first } => {
                if room_type.is_empty() {
                    self.note = "ROOM — type the room name and Space first".into();
                    return;
                }
                match first {
                    None => {
                        self.tool = Tool::RoomLabel {
                            room_type: room_type.clone(),
                            first: Some(p),
                        };
                        self.note =
                            format!("{} — pick opposite corner", room_type.to_ascii_uppercase());
                    }
                    Some(a) => {
                        let name = room_type.clone();
                        let fb = self.app_feed(&format!(
                            "ROOMLABEL {} {} {} {} {}",
                            name, a.x, a.y, p.x, p.y
                        ));
                        if let Some(m) = fb.message() {
                            self.note = m.to_string();
                        }
                        // Stay in tool for another room of the same type.
                        self.tool = Tool::RoomLabel {
                            room_type: name,
                            first: None,
                        };
                    }
                }
            }
            Tool::RoomArea => {
                let tol = PICK_PIXELS / self.camera.scale;
                let Some(id) = pick_entity(self.session.document(), p, tol) else {
                    self.note = "IAR — click a closed polyline or circle".into();
                    return;
                };
                let fb = self.app_feed(&format!("IAR {}", id.0));
                if let Some(m) = fb.message() {
                    self.note = m.to_string();
                }
                self.tool = Tool::Select;
            }
            Tool::Polyline { mut points } => {
                points.push(p);
                self.tool = Tool::Polyline { points };
                self.engine.accept_point();
                self.note = "POLYLINE — click next vertex (Space finishes)".into();
            }
            Tool::RegionSelect {
                mut points,
                crossing,
                fence,
            } => {
                points.push(p);
                let n = points.len();
                self.tool = Tool::RegionSelect {
                    points,
                    crossing,
                    fence,
                };
                self.engine.accept_point();
                self.note = if fence {
                    format!("FENCE — click next point ({n}); Space selects")
                } else {
                    format!("{} — click next corner ({n}); Space selects",
                        if crossing { "CPOLYGON" } else { "WPOLYGON" })
                };
            }
            Tool::Wall {
                thickness: None, ..
            } => {
                self.note = "WALL — type thickness and Space first".into();
            }
            Tool::Wall {
                thickness: Some(th),
                mut points,
            } => {
                points.push(p);
                self.tool = Tool::Wall {
                    thickness: Some(th),
                    points,
                };
                self.engine.accept_point();
                self.note = "WALL — next point (Space finish · Double-Space repeat)".into();
            }
            Tool::PlaceElem { kind } => {
                aadt_command::ensure_place_blocks(self.session.document_mut());
                let layer = kind.layer();
                let block = kind.block_name();
                let fb = self.app_feed(&format!("INSERT {} {} {} 0 {}", block, p.x, p.y, layer));
                let id = parse_entity_id_from_feedback(fb.message()).or_else(|| {
                    let s = 0.45;
                    let fb2 = self.app_feed(&format!(
                        "RECTANGLE {} {} {} {} {}",
                        p.x - s,
                        p.y - s,
                        p.x + s,
                        p.y + s,
                        layer
                    ));
                    parse_entity_id_from_feedback(fb2.message())
                });
                if let Some(id) = id {
                    self.session
                        .feed(&format!("EATTEDIT {id} AADT-ELEM {}", kind.elem()));
                }
                self.tool = Tool::Select;
                self.engine.accept_point();
                self.engine.complete();
            }
            Tool::Hatch => {
                // AutoCAD: prefer an internal point (region under the cursor);
                // fall back to a direct hit on the boundary entity itself.
                let tol = PICK_PIXELS / self.camera.scale;
                let target = hatch_boundary_at(self.session.document(), p)
                    .or_else(|| pick_entity(self.session.document(), p, tol));
                if let Some(id) = target {
                    let pat = self.hatch_pattern.clone();
                    let fb = self.app_feed(&format!("HATCH {} {}", id.0, pat));
                    if fb.is_err() {
                        self.note = format!(
                            "HATCH ({}) — click inside a closed region",
                            self.hatch_pattern
                        );
                    } else {
                        self.tool = Tool::Select;
                        self.engine.accept_point();
                        self.engine.complete();
                    }
                } else {
                    self.note = format!(
                        "HATCH ({}) — pick a point inside a closed region (or its boundary)",
                        self.hatch_pattern
                    );
                }
            }
            Tool::Trim { cutters, phase } => {
                let tol = PICK_PIXELS / self.camera.scale;
                let Some(id) = pick_entity(self.session.document(), p, tol) else {
                    self.note = match phase {
                        TrimPhase::CuttingEdges => "TRIM — click a cutting edge".into(),
                        TrimPhase::Objects => "TRIM — click object on the side to remove".into(),
                    };
                    return;
                };
                match phase {
                    TrimPhase::CuttingEdges => {
                        let mut cutters = cutters.clone();
                        if !cutters.contains(&id) {
                            cutters.push(id);
                        }
                        let n = cutters.len();
                        self.tool = Tool::Trim {
                            cutters,
                            phase: TrimPhase::CuttingEdges,
                        };
                        self.note = format!(
                            "TRIM — {n} cutting edge(s); click more or Space to trim objects"
                        );
                    }
                    TrimPhase::Objects => {
                        let cutters = cutters.clone();
                        // AutoCAD: Shift+select extends instead of trims.
                        if self.pointer_shift {
                            if self.extend_within_trim(id, &cutters, p) {
                                self.note =
                                    "TRIM — select object to trim or Shift+extend (Space finishes)"
                                        .into();
                            } else {
                                self.note =
                                    "TRIM — Shift+click: nothing to extend at that pick".into();
                            }
                            return;
                        }
                        let ok = if cutters.is_empty() {
                            self.trim_object(id, &[], p)
                        } else {
                            self.trim_object(id, &cutters, p)
                        };
                        if ok {
                            self.note =
                                "TRIM — click or drag across more objects (Shift extends; Space finishes)"
                                    .into();
                        }
                    }
                }
            }
            Tool::Extend { boundary } => {
                let tol = PICK_PIXELS / self.camera.scale;
                let Some(id) = pick_entity(self.session.document(), p, tol) else {
                    self.note = "EXTEND — click an object".into();
                    return;
                };
                match boundary {
                    None => {
                        self.tool = Tool::Extend { boundary: Some(id) };
                        self.note = "EXTEND — click near the end to lengthen".into();
                    }
                    Some(b) => {
                        self.session
                            .feed(&format!("EXTEND {} {} {} {}", id.0, b.0, p.x, p.y));
                        self.tool = Tool::Extend { boundary: Some(b) };
                        self.note =
                            "EXTEND — click near the end to lengthen (Space finishes)".into();
                    }
                }
            }
            Tool::Mirror {
                collecting: false,
                a,
                b,
            } => match (a, b) {
                (None, _) => {
                    self.tool = Tool::Mirror {
                        collecting: false,
                        a: Some(p),
                        b: None,
                    };
                    self.engine.accept_point();
                    self.note = "MIRROR — second axis point".into();
                }
                (Some(a0), None) => {
                    self.tool = Tool::Mirror {
                        collecting: false,
                        a: Some(a0),
                        b: Some(p),
                    };
                    self.engine.accept_point();
                    self.note = "MIRROR — Erase source objects? [Yes/No] <N>".into();
                }
                (Some(_), Some(_)) => {
                    self.note = "MIRROR — type Y or N (Space = No)".into();
                }
            },
            Tool::ArrayRect {
                collecting: false,
                rows,
                cols,
                spacing_a,
                dx,
            } => {
                if rows.is_none() || cols.is_none() {
                    self.note = "ARRAYRECT — type rows and columns first".into();
                    return;
                }
                match (spacing_a, dx) {
                    (None, None) => {
                        self.tool = Tool::ArrayRect {
                            collecting: false,
                            rows,
                            cols,
                            spacing_a: Some(p),
                            dx: None,
                        };
                        self.note = "ARRAYRECT — second point for column spacing".into();
                    }
                    (Some(a0), None) => {
                        let d = a0.distance(p);
                        self.tool = Tool::ArrayRect {
                            collecting: false,
                            rows,
                            cols,
                            spacing_a: None,
                            dx: Some(d),
                        };
                        self.note = "ARRAYRECT — row spacing dy (type or pick two points)".into();
                    }
                    (None, Some(dxv)) => {
                        self.tool = Tool::ArrayRect {
                            collecting: false,
                            rows,
                            cols,
                            spacing_a: Some(p),
                            dx: Some(dxv),
                        };
                        self.note = "ARRAYRECT — second point for row spacing".into();
                    }
                    (Some(a0), Some(dxv)) => {
                        let dy = a0.distance(p);
                        self.finish_array_rect(rows.unwrap(), cols.unwrap(), dxv, dy);
                    }
                }
            }
            Tool::ArrayPolar {
                collecting: false,
                center,
                count,
                fill,
            } => {
                if center.is_none() {
                    self.tool = Tool::ArrayPolar {
                        collecting: false,
                        center: Some(p),
                        count: None,
                        fill: None,
                    };
                    self.engine.accept_point();
                    self.note = "ARRAYPOLAR — count (type + Space)".into();
                } else if count.is_none() {
                    self.note = "ARRAYPOLAR — type count and Space".into();
                } else if fill.is_none() {
                    self.note = "ARRAYPOLAR — type fill angle or Space for 360".into();
                } else {
                    self.note = "ARRAYPOLAR — type Y or N (Space = Yes)".into();
                }
            }
            Tool::ArrayPath {
                collecting: false,
                path,
                count,
            } => {
                if path.is_none() {
                    let tol = PICK_PIXELS / self.camera.scale;
                    match pick_entity(self.session.document(), p, tol) {
                        Some(id) => {
                            self.tool = Tool::ArrayPath {
                                collecting: false,
                                path: Some(id),
                                count: None,
                            };
                            self.note = "ARRAYPATH — count (type + Space)".into();
                        }
                        None => {
                            self.note = "ARRAYPATH — click a line, polyline, arc, or circle".into();
                        }
                    }
                } else if count.is_none() {
                    self.note = "ARRAYPATH — type count and Space".into();
                } else {
                    self.note = "ARRAYPATH — type Y or N (Space = Yes)".into();
                }
            }
            Tool::Explode => {
                let tol = PICK_PIXELS / self.camera.scale;
                if let Some(id) = pick_entity(self.session.document(), p, tol) {
                    self.app_feed(&format!("EXPLODE {}", id.0));
                    self.tool = Tool::Select;
                } else {
                    self.note = "EXPLODE — click a block reference".into();
                }
            }
            Tool::Dim { first, second } => match (first, second) {
                (None, _) => {
                    self.tool = Tool::Dim {
                        first: Some(p),
                        second: None,
                    };
                    self.note = "DIM — click second definition point".into();
                }
                (Some(a), None) => {
                    self.tool = Tool::Dim {
                        first: Some(a),
                        second: Some(p),
                    };
                    self.note = "DIM — click dimension line / text location".into();
                }
                (Some(a), Some(b)) => {
                    self.app_feed(&format!(
                        "DIM {} {} {} {} {} {} AN-DIMS",
                        a.x, a.y, b.x, b.y, p.x, p.y
                    ));
                    self.tool = Tool::Select;
                }
            },
            Tool::QDim => {
                if self.selection.is_empty() {
                    self.note = "QDIM — select objects first, then pick dim-line side".into();
                } else {
                    self.sync_selection();
                    self.app_feed(&format!("QDIM SELECTION {} {} AN-DIMS", p.x, p.y));
                    self.tool = Tool::Select;
                }
            }
            Tool::DimContinue => {
                if self.session.document().dimensions().iter().rev().any(|d| {
                    matches!(
                        d.kind,
                        aadt_model::DimensionKind::Aligned | aadt_model::DimensionKind::Linear
                    )
                }) {
                    self.app_feed(&format!("DIMCONTINUE {} {}", p.x, p.y));
                    self.note = "DIMCONTINUE — next point (Space finishes)".into();
                    if let Some(fb) = self.session.transcript().last() {
                        if let Some(msg) = fb.message() {
                            self.note = format!("{msg} — Space finishes");
                        }
                    }
                } else {
                    self.note = "DIMCONTINUE — place a DIM first".into();
                    self.tool = Tool::Select;
                }
            }
            Tool::DimRadius => {
                let tol = PICK_PIXELS / self.camera.scale;
                if let Some(id) = pick_entity(self.session.document(), p, tol) {
                    self.app_feed(&format!("DIMRADIUS {} AN-DIMS", id.0));
                    self.tool = Tool::Select;
                } else {
                    self.note = "DIMRADIUS — click a circle or arc".into();
                }
            }
            Tool::DimDiameter => {
                let tol = PICK_PIXELS / self.camera.scale;
                if let Some(id) = pick_entity(self.session.document(), p, tol) {
                    self.app_feed(&format!("DIMDIAMETER {} AN-DIMS", id.0));
                    self.tool = Tool::Select;
                } else {
                    self.note = "DIMDIAMETER — click a circle or arc".into();
                }
            }
            Tool::DimAngular { vertex, p1 } => match (vertex, p1) {
                (None, _) => {
                    self.tool = Tool::DimAngular {
                        vertex: Some(p),
                        p1: None,
                    };
                    self.note = "DIMANGULAR — click first ray point".into();
                }
                (Some(v), None) => {
                    self.tool = Tool::DimAngular {
                        vertex: Some(v),
                        p1: Some(p),
                    };
                    self.note = "DIMANGULAR — click second ray point".into();
                }
                (Some(v), Some(a)) => {
                    self.app_feed(&format!(
                        "DIMANGULAR {} {} {} {} {} {} AN-DIMS",
                        v.x, v.y, a.x, a.y, p.x, p.y
                    ));
                    self.tool = Tool::Select;
                }
            },
            Tool::MatchProp { source } => {
                let tol = PICK_PIXELS / self.camera.scale;
                let Some(id) = pick_entity(self.session.document(), p, tol) else {
                    self.note = "MATCHPROP — click an object".into();
                    return;
                };
                match source {
                    None => {
                        self.tool = Tool::MatchProp { source: Some(id) };
                        self.note = "MATCHPROP — click target object(s)".into();
                    }
                    Some(src) => {
                        self.session.feed(&format!("MATCHPROP {} {}", src.0, id.0));
                        self.tool = Tool::MatchProp { source: Some(src) };
                        self.note = "MATCHPROP — click another target (Esc finishes)".into();
                    }
                }
            }
            Tool::ZoomWindow { first } => match first {
                None => {
                    self.tool = Tool::ZoomWindow { first: Some(p) };
                    self.note = "ZOOM Window — click opposite corner".into();
                }
                Some(a) => {
                    self.apply_nav_hint(&format!("window:{},{},{},{}", a.x, a.y, p.x, p.y));
                    self.tool = Tool::Select;
                    self.note = "Zoom Window".into();
                }
            },
            Tool::PlotWindow { first } => match first {
                None => {
                    self.tool = Tool::PlotWindow { first: Some(p) };
                    self.note = "PLOT WINDOW — click opposite corner".into();
                }
                Some(a) => {
                    self.plot_window = Some(aadt_geometry::Bbox::from_corners(a, p));
                    self.plot_area_mode = 2;
                    self.tool = Tool::Select;
                    self.seed_plot_dialog();
                    self.note = "Plot window set — confirm Plot to PDF".into();
                }
            },
            Tool::SectionLine { first } => match first {
                None => self.tool = Tool::SectionLine { first: Some(p) },
                Some(a) => {
                    self.app_feed(&format!("SECTIONLINE {} {} {} {} A", a.x, a.y, p.x, p.y));
                    self.tool = Tool::Select;
                }
            },
            Tool::Callout { arrow, center } => match (arrow, center) {
                (None, _) => {
                    self.tool = Tool::Callout {
                        arrow: Some(p),
                        center: None,
                    };
                }
                (Some(a), None) => {
                    self.tool = Tool::Callout {
                        arrow: Some(a),
                        center: Some(p),
                    };
                    self.note = "CALLOUT — type bubble text and Enter".into();
                }
                (Some(_), Some(_)) => {
                    self.note = "CALLOUT — type bubble text and Enter".into();
                }
            },
            Tool::RevMark => {
                let letter = self
                    .session
                    .document()
                    .properties()
                    .autonum_prefix
                    .chars()
                    .next()
                    .filter(|c| c.is_ascii_alphabetic())
                    .map(|c| c.to_ascii_uppercase().to_string())
                    .unwrap_or_else(|| "A".into());
                self.session
                    .feed(&format!("REVMARK {} {} {} 0.5", p.x, p.y, letter));
                self.tool = Tool::Select;
            }
            Tool::RevCloud { first } => match first {
                None => self.tool = Tool::RevCloud { first: Some(p) },
                Some(a) => {
                    self.app_feed(&format!("REVCLOUD {} {} {} {}", a.x, a.y, p.x, p.y));
                    self.tool = Tool::Select;
                }
            },
            Tool::TablePlace => {
                let rows = self.table_rows.parse::<u32>().unwrap_or(5).clamp(1, 200);
                let cols = self.table_cols.parse::<u32>().unwrap_or(4).clamp(1, 50);
                self.app_feed(&format!("TABLE {} {} {} {}", p.x, p.y, rows, cols));
                self.tool = Tool::Select;
            }
            Tool::Offset { .. }
            | Tool::Select
            | Tool::Erase
            | Tool::Move {
                collecting: true, ..
            }
            | Tool::Copy {
                collecting: true, ..
            }
            | Tool::Rotate {
                collecting: true, ..
            }
            | Tool::Scale {
                collecting: true, ..
            }
            | Tool::Mirror {
                collecting: true, ..
            }
            | Tool::ArrayRect {
                collecting: true, ..
            }
            | Tool::ArrayPolar {
                collecting: true, ..
            }
            | Tool::ArrayPath {
                collecting: true, ..
            }
            | Tool::MakeBlock {
                collecting: true, ..
            } => {}
        }
    }

    fn finish_array_rect(&mut self, rows: usize, cols: usize, dx: f64, dy: f64) {
        self.sync_selection();
        self.app_feed(&format!("ARRAYRECT SELECTION {rows} {cols} {dx} {dy}"));
        self.tool = Tool::Select;
        self.engine.complete();
    }

    fn finish_array_polar(&mut self, c: Point2, count: usize, fill: f64, rotate: bool) {
        self.sync_selection();
        let flag = if rotate { "Y" } else { "N" };
        self.app_feed(&format!(
            "ARRAYPOLAR SELECTION {} {} {count} {fill} {flag}",
            c.x, c.y
        ));
        self.tool = Tool::Select;
        self.engine.complete();
    }

    fn finish_array_path(&mut self, path: EntityId, count: usize, align: bool) {
        self.sync_selection();
        let flag = if align { "Y" } else { "N" };
        self.app_feed(&format!("ARRAYPATH SELECTION {} {count} {flag}", path.0));
        self.tool = Tool::Select;
        self.engine.complete();
    }

    /// Click while OFFSET is waiting for an entity: pick under the cursor.
    fn offset_pick_entity(&mut self, world: Point2) {
        let Tool::Offset {
            dist: Some(d),
            entity: None,
        } = self.tool
        else {
            return;
        };
        let tol = PICK_PIXELS / self.camera.scale;
        match pick_entity(self.session.document(), world, tol) {
            Some(id) => {
                self.tool = Tool::Offset {
                    dist: Some(d),
                    entity: Some(id),
                };
            }
            None => {
                self.note = "OFFSET — click a line, polyline, circle, or arc".to_string();
            }
        }
    }

    fn place_rectangle(&mut self, a: Point2, b: Point2, layer: &str) {
        match self.rect_corner {
            RectCorner::Square => {
                self.app_feed(&format!(
                    "RECTANGLE {} {} {} {} {}",
                    a.x, a.y, b.x, b.y, layer
                ));
            }
            // Chamfered / filleted corners: draw a closed polyline instead.
            corner => {
                let verts = rect_corner_polyline(a, b, corner);
                let coords: Vec<String> = verts
                    .iter()
                    .flat_map(|p| [p.x.to_string(), p.y.to_string()])
                    .collect();
                self.app_feed(&format!("POLYLINE {} CLOSED {}", coords.join(" "), layer));
            }
        }
        self.rect_corner = RectCorner::Square;
        self.rect_corner_pending = None;
        self.tool = Tool::Select;
        self.engine.accept_point();
        self.engine.complete();
    }

    /// Finish CIRCLE TTR: solve for the radius-`r` circle tangent to the two
    /// picked lines, on the side the picks indicate, and add it.
    fn finish_ttr_circle(&mut self, r: f64) {
        let layer = self.layer();
        let lines: Vec<(Segment, Point2)> = self
            .ttr_lines
            .iter()
            .filter_map(
                |(id, pick)| match self.session.document().get(*id).map(|e| &e.geometry) {
                    Some(Geometry::Line(s)) => Some((*s, *pick)),
                    _ => None,
                },
            )
            .collect();
        if lines.len() != 2 {
            self.note = "CIRCLE TTR — both tangents must be straight lines".into();
            self.ttr_lines.clear();
            self.tool = Tool::Select;
            return;
        }
        let near = lines[0].1.midpoint(lines[1].1);
        if let Some((c, radius)) = tangent_circle_center(lines[0].0, lines[1].0, r, near) {
            self.session
                .feed(&format!("CIRCLE {} {} {} {}", c.x, c.y, radius, layer));
            self.engine.complete();
        } else {
            self.note = "CIRCLE TTR — the two lines are parallel".into();
        }
        self.ttr_lines.clear();
        self.tool = Tool::Select;
    }

    fn place_rectangle_dims(&mut self, a: Point2, length: f64, width: f64, layer: &str) {
        self.place_rectangle(a, Point2::new(a.x + length, a.y + width), layer);
    }

    fn feed_xline(&mut self, origin: Point2, dx: f64, dy: f64, layer: &str) {
        self.app_feed(&format!(
            "XLINE {} {} {} {} {}",
            origin.x, origin.y, dx, dy, layer
        ));
    }

    fn on_xline_pick(
        &mut self,
        p: Point2,
        origin: Option<Point2>,
        mode: XlineMode,
        ang_deg: Option<f64>,
        layer: &str,
    ) {
        match mode {
            XlineMode::Hor => {
                self.feed_xline(p, 1.0, 0.0, layer);
                self.tool = Tool::Xline {
                    origin: None,
                    mode: XlineMode::Hor,
                    ang_deg: None,
                };
                self.note = "XLINE Hor — specify a point (Space finishes)".into();
            }
            XlineMode::Ver => {
                self.feed_xline(p, 0.0, 1.0, layer);
                self.tool = Tool::Xline {
                    origin: None,
                    mode: XlineMode::Ver,
                    ang_deg: None,
                };
                self.note = "XLINE Ver — specify a point (Space finishes)".into();
            }
            XlineMode::Ang => {
                if let Some(deg) = ang_deg {
                    let rad = deg.to_radians();
                    self.feed_xline(p, rad.cos(), rad.sin(), layer);
                    self.tool = Tool::Xline {
                        origin: None,
                        mode: XlineMode::Ang,
                        ang_deg: Some(deg),
                    };
                    self.note =
                        "XLINE Ang — specify a point (Space finishes · type new angle)".into();
                } else {
                    self.tool = Tool::Xline {
                        origin: Some(p),
                        mode: XlineMode::Ang,
                        ang_deg: None,
                    };
                    self.note = "XLINE Ang — type angle in degrees and Space".into();
                }
            }
            XlineMode::Through => match origin {
                None => {
                    self.tool = Tool::Xline {
                        origin: Some(p),
                        mode: XlineMode::Through,
                        ang_deg: None,
                    };
                    self.engine.accept_point();
                    self.note = "XLINE — specify through point".into();
                }
                Some(o) => {
                    let d = p - o;
                    if d.length() <= f64::EPSILON {
                        self.note = "XLINE — through point must differ from origin".into();
                        return;
                    }
                    self.feed_xline(o, d.x, d.y, layer);
                    self.tool = Tool::Xline {
                        origin: Some(o),
                        mode: XlineMode::Through,
                        ang_deg: None,
                    };
                    self.engine.accept_point();
                    self.note = "XLINE — specify through point (Space finishes)".into();
                }
            },
        }
    }

    /// AutoCAD-like TRIM: one shot against all cutting edges at the pick.
    /// Returns whether a trim applied.
    fn trim_object(&mut self, target: EntityId, cutters: &[EntityId], pick: Point2) -> bool {
        let cutter_ids: Vec<EntityId> = cutters.iter().copied().filter(|c| *c != target).collect();
        let fb = if cutter_ids.is_empty() {
            // Quick mode — every other entity is a cutting edge.
            self.app_feed(&format!("TRIM {} {} {}", target.0, pick.x, pick.y))
        } else {
            let mut line = format!("TRIM {} {} {}", target.0, pick.x, pick.y);
            for c in &cutter_ids {
                line.push(' ');
                line.push_str(&c.0.to_string());
            }
            self.app_feed(&line)
        };
        if fb.is_err() {
            self.note = "TRIM — no crossing with cutting edges at that pick".into();
            return false;
        }
        true
    }

    /// AutoCAD Shift-select-to-extend inside TRIM: lengthen `target` at `pick`
    /// to the first boundary that yields a result. `cutters` are the active
    /// cutting edges (empty ⇒ every other entity is a candidate boundary).
    fn extend_within_trim(&mut self, target: EntityId, cutters: &[EntityId], pick: Point2) -> bool {
        let mut boundaries: Vec<EntityId> =
            cutters.iter().copied().filter(|b| *b != target).collect();
        if boundaries.is_empty() {
            boundaries = self
                .session
                .document()
                .entities()
                .iter()
                .map(|e| e.id)
                .filter(|&eid| eid != target)
                .collect();
        }
        for b in boundaries {
            let fb = self.app_feed(&format!("EXTEND {} {} {} {}", target.0, b.0, pick.x, pick.y));
            if !fb.is_err() {
                return true;
            }
        }
        false
    }

    /// The geometry the TRIM tool would remove at `pick` right now, for the
    /// hover ghost. `cutters` are the active cutting edges (empty ⇒ Quick mode,
    /// every other entity). `None` when nothing under the cursor can be trimmed.
    fn trim_hover_ghost(&self, pick: Point2, cutters: &[EntityId]) -> Option<Geometry> {
        let tol = PICK_PIXELS / self.camera.scale;
        let id = pick_entity(self.session.document(), pick, tol)?;
        let doc = self.session.document();
        let target = doc.get(id)?.geometry.clone();
        let cutter_geoms: Vec<Geometry> = if cutters.is_empty() {
            doc.entities()
                .iter()
                .filter(|e| e.id != id)
                .map(|e| e.geometry.clone())
                .collect()
        } else {
            cutters
                .iter()
                .filter(|c| **c != id)
                .filter_map(|c| doc.get(*c))
                .map(|e| e.geometry.clone())
                .collect()
        };
        aadt_command::trim_removed_preview(&target, &cutter_geoms, pick)
    }

    /// The prompt shown while a tool is active.
    fn tool_prompt(&self) -> Option<&'static str> {
        // Multi-phase tools own step prompts (AutoCAD-style), not the engine catalog.
        match &self.tool {
            Tool::Rectangle {
                first: None,
                phase: RectPhase::FirstCorner,
                ..
            } => return Some("RECTANGLE — first corner or [D]imensions / [C]hamfer / [F]illet"),
            Tool::Rectangle {
                phase: RectPhase::OppositeCorner,
                ..
            } => return Some("RECTANGLE — opposite corner or @W,H"),
            Tool::Rectangle {
                phase: RectPhase::DimLength,
                ..
            } => return Some("RECTANGLE — type length and Space"),
            Tool::Rectangle {
                phase: RectPhase::DimWidth,
                ..
            } => return Some("RECTANGLE — type width and Space"),
            Tool::Trim {
                phase: TrimPhase::CuttingEdges,
                ..
            } => return Some("TRIM — select cutting edges (Space when done)"),
            Tool::Trim {
                phase: TrimPhase::Objects,
                ..
            } => {
                return Some(
                    "TRIM — click or drag across objects to trim; Shift extends, [C]utting edges (Space finishes)",
                )
            }
            Tool::Xline {
                mode: XlineMode::Through,
                origin: None,
                ..
            } => return Some("XLINE — point or [H]or/[V]er/[A]ng"),
            Tool::Xline {
                mode: XlineMode::Through,
                origin: Some(_),
                ..
            } => return Some("XLINE — through point (Space finishes)"),
            Tool::Xline {
                mode: XlineMode::Hor,
                ..
            } => return Some("XLINE Hor — specify a point (Space finishes)"),
            Tool::Xline {
                mode: XlineMode::Ver,
                ..
            } => return Some("XLINE Ver — specify a point (Space finishes)"),
            Tool::Xline {
                mode: XlineMode::Ang,
                ang_deg: None,
                ..
            } => return Some("XLINE Ang — type angle (degrees) and Space"),
            Tool::Xline {
                mode: XlineMode::Ang,
                ang_deg: Some(_),
                ..
            } => return Some("XLINE Ang — specify a point"),
            Tool::Move {
                collecting: true, ..
            } => return Some("MOVE — select objects (Space when done)"),
            Tool::Copy {
                collecting: true, ..
            } => return Some("COPY — select objects (Space when done)"),
            Tool::Rotate {
                collecting: true, ..
            } => return Some("ROTATE — select objects (Space when done)"),
            Tool::Scale {
                collecting: true, ..
            } => return Some("SCALE — select objects (Space when done)"),
            Tool::Mirror {
                collecting: true, ..
            } => return Some("MIRROR — select objects (Space when done)"),
            Tool::ArrayRect {
                collecting: true, ..
            } => return Some("ARRAYRECT — select objects (Space when done)"),
            Tool::ArrayPolar {
                collecting: true, ..
            } => return Some("ARRAYPOLAR — select objects (Space when done)"),
            Tool::ArrayPath {
                collecting: true, ..
            } => return Some("ARRAYPATH — select objects (Space when done)"),
            Tool::MakeBlock {
                collecting: true, ..
            } => return Some("BLOCK — select objects (Space when done)"),
            Tool::Erase => return Some("ERASE — select objects (Space erases)"),
            _ => {}
        }
        if let Some(p) = self.engine.prompt() {
            return Some(p);
        }
        Some(match &self.tool {
            Tool::Line { points } if points.is_empty() => {
                "LINE — specify first point or [U]ndo/[C]lose"
            }
            Tool::Line { .. } => "LINE — next point or [U]ndo/[C]lose (Space finish)",
            Tool::Circle {
                mode: CircleMode::Center,
                p1: None,
                ..
            } => "CIRCLE — center or [2P/3P/T]TR/[D]iameter",
            Tool::Circle {
                mode: CircleMode::Center,
                p1: Some(_),
                diameter: false,
                ..
            } => "CIRCLE — radius or [D]iameter (type + Space)",
            Tool::Circle {
                mode: CircleMode::Center,
                p1: Some(_),
                diameter: true,
                ..
            } => "CIRCLE — diameter (type + Space)",
            Tool::Circle {
                mode: CircleMode::TwoPoint,
                p1: None,
                ..
            } => "CIRCLE 2P — first point of diameter",
            Tool::Circle {
                mode: CircleMode::TwoPoint,
                p1: Some(_),
                ..
            } => "CIRCLE 2P — second point of diameter",
            Tool::Circle {
                mode: CircleMode::ThreePoint,
                p1: None,
                ..
            } => "CIRCLE 3P — first point",
            Tool::Circle {
                mode: CircleMode::ThreePoint,
                p1: Some(_),
                p2: None,
                ..
            } => "CIRCLE 3P — second point",
            Tool::Circle {
                mode: CircleMode::ThreePoint,
                p2: Some(_),
                ..
            } => "CIRCLE 3P — third point",
            Tool::Circle {
                mode: CircleMode::Ttr,
                ..
            } => "CIRCLE TTR — pick two tangent lines, then type the radius",
            Tool::Arc {
                mode: ArcMode::ThreePoint,
                a: None,
                ..
            } => "ARC — start point or [C]enter",
            Tool::Arc {
                mode: ArcMode::ThreePoint,
                b: None,
                ..
            } => "ARC — second point on arc",
            Tool::Arc {
                mode: ArcMode::ThreePoint,
                ..
            } => "ARC — end point",
            Tool::Arc {
                mode: ArcMode::CenterStartEnd,
                a: None,
                ..
            } => "ARC — center point",
            Tool::Arc {
                mode: ArcMode::CenterStartEnd,
                b: None,
                ..
            } => "ARC — start point",
            Tool::Arc {
                mode: ArcMode::CenterStartEnd,
                ..
            } => "ARC — end point",
            Tool::Rectangle { .. } => "RECTANGLE",
            Tool::Xline { .. } => "XLINE",
            Tool::Move {
                collecting: false,
                base: None,
            } => "MOVE — specify base point",
            Tool::Move {
                collecting: false,
                base: Some(_),
            } => "MOVE — specify destination",
            Tool::Copy {
                collecting: false,
                base: None,
            } => "COPY — specify base point",
            Tool::Copy {
                collecting: false,
                base: Some(_),
            } => "COPY — specify destination (Space finishes)",
            Tool::Rotate {
                collecting: false,
                pivot: None,
                ..
            } => "ROTATE — specify base point",
            Tool::Rotate {
                collecting: false,
                pivot: Some(_),
                reference: false,
                ..
            } => "ROTATE — angle (type degrees, pick point, or [R]eference / [C]opy)",
            Tool::Rotate {
                collecting: false,
                pivot: Some(_),
                reference: true,
                ref_a: None,
                ref_ang: None,
            } => "ROTATE Reference — first point of reference angle",
            Tool::Rotate {
                collecting: false,
                pivot: Some(_),
                reference: true,
                ref_a: Some(_),
                ref_ang: None,
            } => "ROTATE Reference — second point of reference angle",
            Tool::Rotate {
                collecting: false,
                pivot: Some(_),
                reference: true,
                ref_a: None,
                ref_ang: Some(_),
            } => "ROTATE Reference — type new angle or first point of new angle",
            Tool::Rotate {
                collecting: false,
                pivot: Some(_),
                reference: true,
                ref_a: Some(_),
                ref_ang: Some(_),
            } => "ROTATE Reference — second point of new angle",
            Tool::Scale {
                collecting: false,
                pivot: None,
                ..
            } => "SCALE — specify base point",
            Tool::Scale {
                collecting: false,
                pivot: Some(_),
                ref_a: None,
                old_len: None,
            } => "SCALE — factor (type) or [R]eference / [C]opy",
            Tool::Scale {
                ref_a: Some(_),
                old_len: None,
                ..
            } => "SCALE Reference — second point of reference length",
            Tool::Scale {
                old_len: Some(_),
                ref_a: None,
                ..
            } => "SCALE Reference — type new length or click first point",
            Tool::Scale {
                old_len: Some(_),
                ref_a: Some(_),
                ..
            } => "SCALE Reference — second point of new length",
            Tool::Offset { dist: None, .. } => "OFFSET — type the distance and Space",
            Tool::Offset {
                dist: Some(_),
                entity: None,
            } => "OFFSET — click the object to offset",
            Tool::Offset {
                dist: Some(_),
                entity: Some(_),
            } => "OFFSET — click a point on the side to offset toward",
            Tool::Text { origin: None } => "TEXT — click the insertion point",
            Tool::Text { origin: Some(_) } => "TEXT — type the value and Space",
            Tool::RoomLabel {
                room_type,
                first: None,
            } if room_type.is_empty() => "ROOM — type room name and Space",
            Tool::RoomLabel { first: None, .. } => "ROOM — pick first corner",
            Tool::RoomLabel { first: Some(_), .. } => "ROOM — pick opposite corner",
            Tool::RoomArea => "IAR — click a closed polyline or circle",
            Tool::Mleader { arrow: None, .. } => "MLEADER — click the arrowhead",
            Tool::Mleader {
                arrow: Some(_),
                landing: None,
                ..
            } => "MLEADER — click the landing",
            Tool::Mleader {
                arrow: Some(_),
                landing: Some(_),
                text_pt: None,
            } => "MLEADER — click the text point",
            Tool::Mleader {
                text_pt: Some(_), ..
            } => "MLEADER — type the note and Space",
            Tool::Stretch {
                collecting: true, ..
            } => "STRETCH — select objects (Space when done)",
            Tool::Stretch {
                collecting: false,
                w0: None,
                ..
            } => "STRETCH — click first crossing-window corner",
            Tool::Stretch {
                collecting: false,
                w0: Some(_),
                w1: None,
                ..
            } => "STRETCH — click opposite crossing-window corner",
            Tool::Stretch {
                collecting: false,
                w1: Some(_),
                base: None,
                ..
            } => "STRETCH — click the base point",
            Tool::Stretch {
                collecting: false,
                base: Some(_),
                ..
            } => "STRETCH — click the destination",
            Tool::Dist { first: None } => "DIST — click first point",
            Tool::Dist { first: Some(_) } => "DIST — click second point",
            Tool::Brush { .. } => "BRUSH — click/drag freehand; Space finishes (A-PRES-MARK)",
            Tool::Fillet { radius: None, .. } => "FILLET — type radius and Space (0 = sharp)",
            Tool::Fillet {
                radius: Some(_),
                first: None,
            } => "FILLET — click the first line (or [T]rim toggle)",
            Tool::Fillet {
                radius: Some(_),
                first: Some(_),
            } => "FILLET — click the second line",
            Tool::Insert { .. } => "INSERT — click to place the block",
            Tool::MakeBlock {
                erase: false,
                collecting: false,
                name: None,
            } => "BLOCK — type block name and Space",
            Tool::MakeBlock {
                erase: true,
                collecting: false,
                name: None,
            } => "WBLOCK — type block name and Space",
            Tool::MakeBlock {
                erase: false,
                name: Some(_),
                ..
            } => "BLOCK — pick base point",
            Tool::MakeBlock {
                erase: true,
                name: Some(_),
                ..
            } => "WBLOCK — pick base point",
            Tool::Polyline { .. } => "POLYLINE — click vertices (Space finishes)",
            Tool::RegionSelect { fence: true, .. } => "FENCE — click points; Space selects crossings",
            Tool::RegionSelect { crossing: true, .. } => "CPOLYGON — click corners; Space selects (crossing)",
            Tool::RegionSelect { .. } => "WPOLYGON — click corners; Space selects (enclosed)",
            Tool::Wall {
                thickness: None, ..
            } => "WALL — type thickness and Space",
            Tool::Wall {
                thickness: Some(_), ..
            } => "WALL — pick points (Space finish · Double-Space repeat)",
            Tool::PlaceElem { kind } => match kind {
                PlaceKind::Door => "DOOR — click insertion point",
                PlaceKind::Window => "WINDOW — click insertion point",
                PlaceKind::Column => "COLUMN — click insertion point",
                PlaceKind::Beam => "BEAM — click insertion point",
                PlaceKind::Slab => "SLAB — click insertion point",
                PlaceKind::Tree => "TREE — click insertion point",
                PlaceKind::Shrub => "SHRUB — click insertion point",
                PlaceKind::Chair => "CHAIR — click insertion point",
                PlaceKind::Desk => "DESK — click insertion point",
                PlaceKind::Sofa => "SOFA — click insertion point",
                PlaceKind::Bed => "BED — click insertion point",
                PlaceKind::Table => "TABLE — click insertion point",
                PlaceKind::Toilet => "TOILET — click insertion point",
            },
            Tool::Hatch => "HATCH — pick a point inside a closed region",
            Tool::Trim { .. } => "TRIM",
            Tool::Extend { boundary: None } => "EXTEND — click the boundary edge",
            Tool::Extend { boundary: Some(_) } => "EXTEND — click near the end to lengthen",
            Tool::Mirror {
                collecting: false,
                a: None,
                ..
            } => "MIRROR — first axis point",
            Tool::Mirror {
                collecting: false,
                a: Some(_),
                b: None,
            } => "MIRROR — second axis point",
            Tool::Mirror {
                collecting: false,
                a: Some(_),
                b: Some(_),
            } => "MIRROR — Erase source objects? [Yes/No] <N>",
            Tool::ArrayRect {
                collecting: false,
                rows: None,
                ..
            } => "ARRAYRECT — rows (type + Space)",
            Tool::ArrayRect {
                collecting: false,
                rows: Some(_),
                cols: None,
                ..
            } => "ARRAYRECT — columns (type + Space)",
            Tool::ArrayRect {
                collecting: false,
                cols: Some(_),
                dx: None,
                spacing_a: None,
                ..
            } => "ARRAYRECT — column spacing dx (type or pick)",
            Tool::ArrayRect {
                collecting: false,
                dx: None,
                spacing_a: Some(_),
                ..
            } => "ARRAYRECT — second point for column spacing",
            Tool::ArrayRect {
                collecting: false,
                dx: Some(_),
                spacing_a: None,
                ..
            } => "ARRAYRECT — row spacing dy (type or pick)",
            Tool::ArrayRect {
                collecting: false,
                dx: Some(_),
                spacing_a: Some(_),
                ..
            } => "ARRAYRECT — second point for row spacing",
            Tool::ArrayPolar {
                collecting: false,
                center: None,
                ..
            } => "ARRAYPOLAR — center point",
            Tool::ArrayPolar {
                collecting: false,
                center: Some(_),
                count: None,
                ..
            } => "ARRAYPOLAR — count (type + Space)",
            Tool::ArrayPolar {
                collecting: false,
                count: Some(_),
                fill: None,
                ..
            } => "ARRAYPOLAR — fill angle degrees <360>",
            Tool::ArrayPolar {
                collecting: false,
                fill: Some(_),
                ..
            } => "ARRAYPOLAR — Rotate items as copied? [Yes/No] <Y>",
            Tool::ArrayPath {
                collecting: false,
                path: None,
                ..
            } => "ARRAYPATH — select path (line/pline/arc/circle)",
            Tool::ArrayPath {
                collecting: false,
                path: Some(_),
                count: None,
            } => "ARRAYPATH — count (type + Space)",
            Tool::ArrayPath {
                collecting: false,
                path: Some(_),
                count: Some(_),
            } => "ARRAYPATH — Align items to path? [Yes/No] <Y>",
            Tool::GripEdit { .. } => {
                "GRIP — new point; Space cycles Stretch/Move/Rotate/Scale/Mirror (Esc cancels)"
            }
            Tool::Explode => "EXPLODE — click a block reference",
            Tool::Dim { first: None, .. } => "DIM — click first definition point",
            Tool::Dim {
                first: Some(_),
                second: None,
            } => "DIM — click second definition point",
            Tool::Dim {
                second: Some(_), ..
            } => "DIM — click dimension line / text location",
            Tool::QDim => "QDIM — click dim-line side (select objects first)",
            Tool::DimContinue => "DIMCONTINUE — next definition point (Space finishes)",
            Tool::DimRadius => "DIMRADIUS — click a circle or arc",
            Tool::DimDiameter => "DIMDIAMETER — click a circle or arc",
            Tool::DimAngular { vertex: None, .. } => "DIMANGULAR — click vertex",
            Tool::DimAngular {
                vertex: Some(_),
                p1: None,
            } => "DIMANGULAR — click first ray point",
            Tool::DimAngular { p1: Some(_), .. } => "DIMANGULAR — click second ray point",
            Tool::MatchProp { source: None } => "MATCHPROP — click source",
            Tool::MatchProp { source: Some(_) } => "MATCHPROP — click targets",
            Tool::ZoomWindow { first: None } => {
                "ZOOM — [A]ll [E]xtents [W]indow [P]revious [S]cale · or click window"
            }
            Tool::ZoomWindow { first: Some(_) } => "ZOOM Window — click opposite corner",
            Tool::PlotWindow { first: None } => "PLOT WINDOW — click first corner",
            Tool::PlotWindow { first: Some(_) } => "PLOT WINDOW — click opposite corner",
            Tool::SectionLine { first: None } => "SECTIONLINE — first point",
            Tool::SectionLine { first: Some(_) } => "SECTIONLINE — second point",
            Tool::Callout { arrow: None, .. } => "CALLOUT — arrow",
            Tool::Callout {
                arrow: Some(_),
                center: None,
            } => "CALLOUT — bubble centre",
            Tool::Callout {
                center: Some(_), ..
            } => "CALLOUT — type text and Space",
            Tool::RevMark => "REVMARK — click insertion",
            Tool::RevCloud { first: None } => "REVCLOUD — first corner",
            Tool::RevCloud { first: Some(_) } => "REVCLOUD — opposite corner",
            Tool::TablePlace => "TABLE — click origin",
            Tool::Move {
                collecting: true, ..
            }
            | Tool::Copy {
                collecting: true, ..
            }
            | Tool::Rotate {
                collecting: true, ..
            }
            | Tool::Scale {
                collecting: true, ..
            }
            | Tool::Mirror {
                collecting: true, ..
            }
            | Tool::ArrayRect {
                collecting: true, ..
            }
            | Tool::ArrayPolar {
                collecting: true, ..
            }
            | Tool::ArrayPath {
                collecting: true, ..
            }
            | Tool::MakeBlock {
                collecting: true, ..
            } => "select objects (Space when done)",
            Tool::Erase => "ERASE — select objects (Space erases)",
            Tool::Select => return None,
        })
    }

    /// The lines shown in the floating dynamic-input box: the prompt, what is
    /// being typed, and the live length/angle while drawing. Empty (no box) when
    /// idle with nothing typed — no bare coordinate readout at the cursor.
    fn dyn_lines(&self) -> Vec<String> {
        let mut lines: Vec<String> = Vec::new();
        if let Some(prompt) = self.tool_prompt() {
            lines.push(prompt.to_string());
        }
        if self.expects_point_coords()
            && (!self.dyn_x.is_empty() || !self.dyn_y.is_empty() || self.dyn_focus == CoordFocus::Y)
        {
            let xmark = if self.dyn_focus == CoordFocus::X {
                ">"
            } else {
                " "
            };
            let ymark = if self.dyn_focus == CoordFocus::Y {
                ">"
            } else {
                " "
            };
            let xshow = if self.dyn_x.is_empty() {
                "_"
            } else {
                self.dyn_x.as_str()
            };
            let yshow = if self.dyn_y.is_empty() {
                "_"
            } else {
                self.dyn_y.as_str()
            };
            lines.push(format!("{xmark}X: {xshow}   {ymark}Y: {yshow}   (Tab)"));
        } else if !self.dyn_input.is_empty() {
            lines.push(format!("» {}", self.dyn_input));
        } else if let (Some(a), Some(c)) = (self.tool_anchor(), self.cursor) {
            let d = a.distance(c);
            let ang = (c.y - a.y).atan2(c.x - a.x).to_degrees().rem_euclid(360.0);
            lines.push(format!("len {d:.2}   ang {ang:.1}°"));
        }
        if let Some((kind, _)) = self.snap_hit {
            lines.push(crate::osnap::snap_label(kind).to_string());
        }
        lines
    }
}

impl eframe::App for AadtApp {
    /// Clear the framebuffer with the neutral chrome background, not eframe's
    /// dark default — otherwise any region a panel doesn't opaquely cover (e.g.
    /// a transient gap beside the left dock while its width changes) shows black.
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        theme::color::CANVAS.to_normalized_gamma_f32()
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Prefer light OS chrome; avoid spamming the viewport every frame.
        if self.last_window_title.is_empty() {
            ctx.send_viewport_cmd(egui::ViewportCommand::SetTheme(egui::SystemTheme::Light));
        }
        ctx.request_repaint_after(std::time::Duration::from_secs(2));
        // Dynamic input: unless a text field is focused, keystrokes are typed at
        // the crosshair — a command token, a value, or edit/finish keys.
        if !ctx.wants_keyboard_input() {
            let now = std::time::Instant::now();
            let space_down = ctx.input(|i| i.key_down(egui::Key::Space));
            if space_down {
                ctx.request_repaint();
            }
            if let Some(SpaceAction::HoldPalette) = self.space_gesture.poll_hold(now, space_down) {
                self.open_palette();
            }
            let events = ctx.input(|i| i.events.clone());
            for ev in &events {
                match ev {
                    egui::Event::Text(t) => {
                        for ch in t.chars() {
                            // Space is owned by SpaceGesture (Key::Space) — ignore text ' '.
                            if ch == ' ' {
                                continue;
                            } else if self.palette_open {
                                if !ch.is_control() {
                                    self.palette_filter.push(ch);
                                }
                            } else if !ch.is_control() {
                                self.push_dyn_char(ch);
                            }
                        }
                    }
                    egui::Event::Key {
                        key,
                        pressed,
                        modifiers,
                        ..
                    } => {
                        if *key == egui::Key::Space {
                            if modifiers.ctrl {
                                if *pressed {
                                    self.open_palette();
                                }
                                continue;
                            }
                            if modifiers.shift {
                                if *pressed {
                                    self.clear_dyn_coords();
                                    self.dyn_input.clear();
                                    self.engine.previous_input();
                                    self.note = "Cleared current typed input".into();
                                }
                                continue;
                            }
                            if *pressed {
                                self.space_gesture.on_press(now);
                            } else {
                                match self.space_gesture.on_release(now) {
                                    Some(SpaceAction::Single) => self.apply_space_single(),
                                    Some(SpaceAction::Double) => self.apply_space_double(),
                                    _ => {}
                                }
                            }
                            continue;
                        }
                        if !*pressed {
                            continue;
                        }
                        // Ctrl shortcuts (file / edit / plot / palettes).
                        if modifiers.ctrl {
                            match key {
                                egui::Key::S => {
                                    if modifiers.shift {
                                        self.save_as_dialog();
                                    } else {
                                        self.quick_save();
                                    }
                                }
                                egui::Key::O => {
                                    if let Some(path) = rfd::FileDialog::new()
                                        .add_filter("AAD / DXF / DWG / VDB", &["aadt", "dxf", "dwg", "vdb"])
                                        .add_filter("DWG", &["dwg"])
                                        .add_filter("DXF", &["dxf"])
                                        .add_filter("AAD", &["aadt"])
                                        .add_filter("VastuDB", &["vdb"])
                                        .pick_file()
                                    {
                                        let fb = self.app_feed(&format!("OPEN {}", path.display()));
                                        if let Some(m) = fb.message() {
                                            self.note = m.to_string();
                                        }
                                        self.fitted = false;
                                        self.selection.clear();
                                    }
                                }
                                egui::Key::N => {
                                    self.new_drawing();
                                }
                                egui::Key::Z => {
                                    self.app_feed("UNDO");
                                }
                                egui::Key::Y => {
                                    self.app_feed("REDO");
                                }
                                egui::Key::C => {
                                    if !self.selection.is_empty() {
                                        self.sync_selection();
                                        self.app_feed("COPYCLIP SELECTION");
                                    }
                                }
                                egui::Key::X => {
                                    if !self.selection.is_empty() {
                                        self.sync_selection();
                                        self.app_feed("CUTCLIP SELECTION");
                                        self.selection.clear();
                                    }
                                }
                                egui::Key::V => {
                                    self.app_feed("PASTECLIP");
                                    self.selection = self.session.selection().to_vec();
                                }
                                egui::Key::P => {
                                    self.props_edit = self.session.document().properties().clone();
                                    self.seed_plot_dialog();
                                }
                                egui::Key::Num1 => {
                                    self.open_drawing_properties();
                                }
                                egui::Key::Num9 => {
                                    self.refocus = true;
                                }
                                _ => {}
                            }
                            continue;
                        }
                        match key {
                            egui::Key::Enter => self.apply_space_single(),
                            egui::Key::Tab => {
                                if self.expects_point_coords() {
                                    self.dyn_focus = match self.dyn_focus {
                                        CoordFocus::X => CoordFocus::Y,
                                        CoordFocus::Y => CoordFocus::X,
                                    };
                                }
                            }
                            egui::Key::Backspace => {
                                if self.palette_open {
                                    self.palette_filter.pop();
                                } else if matches!(self.tool, Tool::Select)
                                    && !self.selection.is_empty()
                                    && self.dyn_input.is_empty()
                                    && self.dyn_x.is_empty()
                                    && self.dyn_y.is_empty()
                                {
                                    self.erase_selection();
                                } else {
                                    self.pop_dyn_char();
                                }
                            }
                            egui::Key::Delete => {
                                if matches!(self.tool, Tool::Select)
                                    && !(self.selection.is_empty()
                                        && self.selected_hatches.is_empty())
                                {
                                    self.erase_selection();
                                }
                            }
                            egui::Key::Escape => {
                                self.dyn_input.clear();
                                self.clear_dyn_coords();
                                self.palette_open = false;
                                self.snap_override = None;
                                self.snap_menu_at = None;
                                self.engine.cancel();
                                if self.drafting() {
                                    self.tool = Tool::Select;
                                } else {
                                    self.selection.clear();
                                    self.selected_hatches.clear();
                                }
                            }
                            egui::Key::F8 => {
                                self.ortho = !self.ortho;
                                self.persist_prefs();
                                self.note = if self.ortho {
                                    "Ortho ON (F8)".to_string()
                                } else {
                                    "Ortho OFF (F8)".to_string()
                                };
                            }
                            egui::Key::F3 => {
                                self.osnap = !self.osnap;
                                self.snap_hit = None;
                                self.persist_prefs();
                                self.note = if self.osnap {
                                    "Object snap ON (F3) — END/MID/CEN/QUA/INT/NEA (SHIFT+right-click for PER/TAN)".to_string()
                                } else {
                                    "Object snap OFF (F3)".to_string()
                                };
                            }
                            egui::Key::F10 => {
                                self.polar = !self.polar;
                                self.persist_prefs();
                                self.note = if self.polar {
                                    "Polar tracking ON (F10) — 45° increments".into()
                                } else {
                                    "Polar tracking OFF (F10)".into()
                                };
                            }
                            egui::Key::F11 => {
                                self.osnap_track = !self.osnap_track;
                                if self.osnap_track {
                                    self.osnap = true;
                                    self.polar = true;
                                }
                                self.persist_prefs();
                                self.note = if self.osnap_track {
                                    "Object-snap tracking ON (F11)".into()
                                } else {
                                    "Object-snap tracking OFF (F11)".into()
                                };
                            }
                            egui::Key::F12 => {
                                self.dyn_mode = !self.dyn_mode;
                                self.persist_prefs();
                                self.note = if self.dyn_mode {
                                    "Dynamic Input ON (F12)".into()
                                } else {
                                    "Dynamic Input OFF (F12)".into()
                                };
                            }
                            egui::Key::F1 => {
                                self.show_shortcuts = true;
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }

        // AutoCAD-like chrome: QAT → layer mgr → command+status → canvas.
        self.refresh_window_title(ctx);
        self.tick_autosave();
        self.draw_recovery_prompt(ctx);
        self.draw_shortcuts_sheet(ctx);
        self.draw_room_picker(ctx);
        self.draw_top_bar(ctx);
        self.draw_bottom_bar(ctx);
        self.draw_drawing_props_panel(ctx);
        self.draw_left_panel(ctx);
        self.draw_right_panel(ctx);
        self.draw_plot_dialog(ctx);
        self.draw_layer_map_dialog(ctx);
        self.draw_command_palette(ctx);

        // --- Central viewport (white paper = print preview) ---
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(theme::color::CANVAS))
            .show(ctx, |ui| {
                let (rect, resp) =
                    ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
                self.camera.viewport = (rect.width() as f64, rect.height() as f64);

                // Paint the paper background over the whole viewport explicitly.
                ui.painter_at(rect)
                    .rect_filled(rect, 0.0, theme::color::CANVAS);

                if !self.fitted {
                    self.camera.fit_document(self.session.document(), 0.08);
                    self.fitted = true;
                }

                let to_world = |camera: &Camera, pos: egui::Pos2| {
                    camera.screen_to_world((pos.x - rect.min.x) as f64, (pos.y - rect.min.y) as f64)
                };

                // Pan (middle-drag) and zoom (scroll).
                let (mid_down, delta) = ui.input(|i| (i.pointer.middle_down(), i.pointer.delta()));
                if mid_down && resp.hovered() {
                    self.camera.center.x -= delta.x as f64 / self.camera.scale;
                    self.camera.center.y += delta.y as f64 / self.camera.scale;
                }
                handle_zoom(&mut self.camera, &resp, rect, ui);

                self.cursor_screen = resp.hover_pos();
                let shift = ui.input(|i| i.modifiers.shift);
                self.pointer_shift = shift;
                let raw_cursor = resp.hover_pos().map(|pos| to_world(&self.camera, pos));
                self.cursor = raw_cursor.map(|p| self.resolve_pick(p, shift));

                // Brush samples while dragging.
                if matches!(self.tool, Tool::Brush { .. }) && resp.dragged() {
                    if let Some(pos) = resp.hover_pos() {
                        let world = to_world(&self.camera, pos);
                        let p = self.resolve_pick(world, shift);
                        self.on_pick(p);
                    }
                }

                match self.tool {
                    Tool::Select => {
                        // Box selection on a primary drag; a plain click picks.
                        // Use press origin so window/crossing direction matches the
                        // true drag (AutoCAD: L→R window, R→L crossing).
                        if resp.drag_started() {
                            let start = ui
                                .input(|i| i.pointer.press_origin())
                                .or_else(|| resp.interact_pointer_pos());
                            self.box_start = start;
                            self.drag_path.clear();
                            if let Some(p) = start {
                                self.drag_path.push(p);
                            }
                        }
                        // Trail the drag so a curved gesture reads as a lasso.
                        if resp.dragged() && self.box_start.is_some() {
                            if let Some(pos) = resp.interact_pointer_pos() {
                                if self
                                    .drag_path
                                    .last()
                                    .map(|p| (*p - pos).length() > 2.0)
                                    .unwrap_or(true)
                                {
                                    self.drag_path.push(pos);
                                }
                            }
                        }
                        let mut box_done = false;
                        if resp.drag_stopped() {
                            if let (Some(s), Some(e)) =
                                (self.box_start, resp.interact_pointer_pos())
                            {
                                if drag_is_lasso(&self.drag_path) {
                                    self.finish_lasso(&to_world, ui);
                                    box_done = true;
                                } else if (s - e).length() >= 3.0 {
                                    self.finish_box(s, e, &to_world, ui);
                                    box_done = true;
                                }
                            }
                            self.box_start = None;
                            self.drag_path.clear();
                        }
                        // Skip click-select after a real box so release doesn't clear.
                        if resp.clicked() && !box_done {
                            if let Some(pos) = resp.interact_pointer_pos() {
                                self.click_select(to_world(&self.camera, pos), ui);
                            }
                        }
                    }
                    _ => {
                        if resp.secondary_clicked() {
                            // SHIFT+right-click opens the object-snap override menu
                            // (AutoCAD); plain right-click stays Enter/confirm.
                            if shift {
                                self.snap_menu_at = resp
                                    .interact_pointer_pos()
                                    .or(self.cursor_screen);
                            } else {
                                self.apply_space_single();
                            }
                        }
                        // AutoCAD freehand quick-trim: drag a stroke and every
                        // object the path crosses is trimmed at the crossing
                        // (Shift-drag extends instead). Each entity trims once
                        // per stroke. A plain click still falls through to the
                        // single-pick `on_pick` path below.
                        if matches!(
                            self.tool,
                            Tool::Trim {
                                phase: TrimPhase::Objects,
                                ..
                            }
                        ) {
                            if resp.drag_started() {
                                self.trim_stroke_hits.clear();
                                self.drag_path.clear();
                                if let Some(pos) = ui
                                    .input(|i| i.pointer.press_origin())
                                    .or_else(|| resp.interact_pointer_pos())
                                {
                                    self.drag_path.push(pos);
                                }
                            }
                            if resp.dragged() {
                                if let Some(pos) = resp.interact_pointer_pos() {
                                    let moved = self
                                        .drag_path
                                        .last()
                                        .map(|p| (*p - pos).length() > 2.0)
                                        .unwrap_or(true);
                                    if moved {
                                        self.drag_path.push(pos);
                                        let world = to_world(&self.camera, pos);
                                        let tol = PICK_PIXELS / self.camera.scale;
                                        if let Some(id) =
                                            pick_entity(self.session.document(), world, tol)
                                        {
                                            if !self.trim_stroke_hits.contains(&id) {
                                                let cutters = match &self.tool {
                                                    Tool::Trim { cutters, .. } => cutters.clone(),
                                                    _ => Vec::new(),
                                                };
                                                let acted = if shift {
                                                    self.extend_within_trim(id, &cutters, world)
                                                } else {
                                                    self.trim_object(id, &cutters, world)
                                                };
                                                if acted {
                                                    self.trim_stroke_hits.push(id);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            if resp.drag_stopped() {
                                self.drag_path.clear();
                                let n = self.trim_stroke_hits.len();
                                self.trim_stroke_hits.clear();
                                if n > 0 {
                                    self.note = format!(
                                        "TRIM — {n} trimmed; drag or click more (Space finishes)"
                                    );
                                }
                            }
                        }
                        // One place per gesture: prefer click; accept micro-drag
                        // release only (not both) so we don't double-fire on_pick.
                        // TRIM owns its own drag gesture (freehand quick-trim
                        // above), so exclude it from the micro-drag release path
                        // — otherwise a looping stroke would trim once more here.
                        let trim_active = matches!(
                            self.tool,
                            Tool::Trim {
                                phase: TrimPhase::Objects,
                                ..
                            }
                        );
                        let place = if resp.clicked() {
                            resp.interact_pointer_pos()
                        } else if resp.drag_stopped() && !trim_active {
                            let origin = ui.input(|i| i.pointer.press_origin());
                            let end = resp.interact_pointer_pos();
                            match (origin, end) {
                                (Some(s), Some(e)) if (s - e).length() < PLACE_DRAG_PX => Some(e),
                                _ => None,
                            }
                        } else {
                            None
                        };
                        if let Some(pos) = place {
                            let world = to_world(&self.camera, pos);
                            match self.tool {
                                Tool::Offset {
                                    dist: Some(_),
                                    entity: None,
                                } => self.offset_pick_entity(world),
                                Tool::Offset { dist: None, .. } => {
                                    self.note = "OFFSET — type the distance and Space first".into();
                                }
                                _ => {
                                    let p = self.resolve_pick(world, shift);
                                    self.on_pick(p);
                                }
                            }
                        }
                    }
                }

                self.session.refresh_draw_cache();
                let list = self.session.cached_drawlist();
                let painter = ui.painter_at(rect);
                paint(
                    &painter,
                    rect,
                    &self.camera,
                    list,
                    self.session.document(),
                    self.show_lweights,
                );
                paint_selection(
                    &painter,
                    rect,
                    &self.camera,
                    self.session.document(),
                    &self.selection,
                );
                paint_selected_hatches(
                    &painter,
                    rect,
                    &self.camera,
                    self.session.document(),
                    &self.selected_hatches,
                );

                // Rubber-band preview for the in-progress tool.
                if let Some(pos) = self.cursor {
                    paint_preview(
                        &painter,
                        rect,
                        &self.camera,
                        &self.tool,
                        pos,
                        self.session.document(),
                        &self.selection,
                    );
                }
                // Object-snap marker at the acquired point.
                if let Some((kind, sp)) = self.snap_hit {
                    paint_snap_marker(&painter, rect, &self.camera, kind, sp);
                }
                // TRIM: freehand stroke trail while dragging, else a ghost of the
                // segment a click would remove under the cursor (AutoCAD preview).
                if matches!(
                    self.tool,
                    Tool::Trim {
                        phase: TrimPhase::Objects,
                        ..
                    }
                ) {
                    let ghost = egui::Color32::from_rgba_unmultiplied(230, 70, 70, 210);
                    if self.drag_path.len() >= 2 && self.box_start.is_none() {
                        let lstroke = egui::Stroke::new(1.5_f32, ghost);
                        for pair in self.drag_path.windows(2) {
                            painter.line_segment([pair[0], pair[1]], lstroke);
                        }
                    } else if let Some(pos) = self.cursor {
                        let cutters = match &self.tool {
                            Tool::Trim { cutters, .. } => cutters.clone(),
                            _ => Vec::new(),
                        };
                        if let Some(g) = self.trim_hover_ghost(pos, &cutters) {
                            draw_geometry(
                                &painter,
                                rect,
                                &self.camera,
                                &g,
                                egui::Stroke::new(3.0_f32, ghost),
                                ghost,
                            );
                        }
                    }
                }
                // HATCH: live preview of the region under the cursor (AutoCAD
                // internal-point pick) — translucent fill + accent boundary.
                if matches!(self.tool, Tool::Hatch) {
                    if let Some(pos) = self.cursor {
                        if let Some(id) = hatch_boundary_at(self.session.document(), pos) {
                            if let Some(e) = self.session.document().get(id) {
                                paint_hatch_region(&painter, rect, &self.camera, &e.geometry);
                            }
                        }
                    }
                }
                // The selection box (straight drag) or lasso trail (curved drag).
                if let (Some(s), Some(cur)) = (self.box_start, resp.hover_pos()) {
                    if drag_is_lasso(&self.drag_path) {
                        let col = if self.drag_path.first().map(|p| cur.x < p.x).unwrap_or(false) {
                            egui::Color32::from_rgb(90, 160, 90) // crossing (green)
                        } else {
                            egui::Color32::from_rgb(80, 130, 200) // window (blue)
                        };
                        let lstroke = egui::Stroke::new(1.0_f32, col);
                        for pair in self.drag_path.windows(2) {
                            painter.line_segment([pair[0], pair[1]], lstroke);
                        }
                        if let (Some(&first), Some(&last)) =
                            (self.drag_path.first(), self.drag_path.last())
                        {
                            painter.line_segment([last, first], lstroke);
                        }
                    } else {
                        paint_box(&painter, s, cur);
                    }
                }
                // UCS marker (bottom-left of viewport).
                {
                    let origin = egui::pos2(rect.min.x + 28.0, rect.max.y - 28.0);
                    let x_end = origin + egui::vec2(36.0, 0.0);
                    let y_end = origin + egui::vec2(0.0, -36.0);
                    painter.arrow(
                        origin,
                        egui::vec2(36.0, 0.0),
                        egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(200, 40, 40)),
                    );
                    painter.arrow(
                        origin,
                        egui::vec2(0.0, -36.0),
                        egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(40, 160, 40)),
                    );
                    painter.text(
                        x_end + egui::vec2(4.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        "X",
                        egui::FontId::proportional(11.0),
                        egui::Color32::from_rgb(200, 40, 40),
                    );
                    painter.text(
                        y_end + egui::vec2(0.0, -4.0),
                        egui::Align2::CENTER_BOTTOM,
                        "Y",
                        egui::FontId::proportional(11.0),
                        egui::Color32::from_rgb(40, 160, 40),
                    );
                    let _ = x_end;
                }
                // Shared building reference origin (all floors share this XY).
                {
                    let o = self.session.document().reference_origin();
                    let sp = world_to_screen(&self.camera, rect, o);
                    let mark = egui::Color32::from_rgb(180, 80, 20);
                    let stroke = egui::Stroke::new(1.5_f32, mark);
                    painter.line_segment(
                        [sp + egui::vec2(-8.0, 0.0), sp + egui::vec2(8.0, 0.0)],
                        stroke,
                    );
                    painter.line_segment(
                        [sp + egui::vec2(0.0, -8.0), sp + egui::vec2(0.0, 8.0)],
                        stroke,
                    );
                    painter.circle_stroke(sp, 5.0, stroke);
                }
                // AutoCAD-style cursor: 100×100 crosshair + pickbox; selection box
                // while dragging a window/crossing. Hide the OS arrow over the canvas.
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                }
                if let Some(sp) = resp.hover_pos() {
                    if self.box_start.is_none() {
                        paint_crosshair_and_pickbox(&painter, sp, self.drafting());
                    }
                    if self.dyn_mode {
                        paint_dyn_box(&painter, sp, &self.dyn_lines());
                    }
                }
                // Keep the crosshair / rubber-band live when the pointer moves,
                // a box is dragged, or Space is held. Idle drafting tools do not
                // spin the CPU at display rate — throttle rubber-band updates.
                let pointer_moved = resp.hover_pos().is_some_and(|p| {
                    self.last_pointer_screen
                        .is_none_or(|prev| (prev - p).length() > 0.5)
                });
                if let Some(p) = resp.hover_pos() {
                    self.last_pointer_screen = Some(p);
                }
                if pointer_moved || self.box_start.is_some() {
                    ctx.request_repaint();
                } else if self.drafting() && self.cursor.is_some() {
                    ctx.request_repaint_after(std::time::Duration::from_millis(50));
                }
            });
        // SHIFT+right-click object-snap override menu (floats over the viewport).
        self.show_snap_menu(ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.persist_prefs();
    }
}

impl AadtApp {
    /// Pick or toggle the entity under `world`; an empty click clears (unless
    /// Shift is held, which only adds/removes). Hot grips on the selection start
    /// GripEdit (AutoCAD-like).
    fn click_select(&mut self, world: Point2, ui: &egui::Ui) {
        let shift = ui.input(|i| i.modifiers.shift);
        let grip_tol = GRIP_PICK_PIXELS / self.camera.scale;
        if !shift && !self.selection.is_empty() {
            for &id in &self.selection.clone() {
                let Some(e) = self.session.document().get(id) else {
                    continue;
                };
                let grips = grip_points(&e.geometry);
                if let Some((idx, gp)) = grips
                    .iter()
                    .enumerate()
                    .find(|(_, g)| world.distance(**g) <= grip_tol)
                {
                    self.tool = Tool::GripEdit {
                        id,
                        grip_idx: idx,
                        origin: *gp,
                        mode: GripMode::Stretch,
                        ref_dist: 0.0,
                    };
                    self.note =
                        "GRIP STRETCH — new location (Space cycles Move/Rotate/Scale/Mirror; Esc cancels)".into();
                    return;
                }
            }
        }
        let tol = PICK_PIXELS / self.camera.scale;
        // Entities (incl. hatch boundary polylines) win on a direct hit.
        if let Some(id) = pick_entity(self.session.document(), world, tol) {
            if shift {
                if let Some(i) = self.selection.iter().position(|&x| x == id) {
                    self.selection.remove(i);
                } else {
                    self.selection.push(id);
                }
            } else {
                self.selection = vec![id];
                self.selected_hatches.clear();
            }
            return;
        }
        // No entity under the cursor: a click inside a fill selects the hatch.
        if let Some(hi) = pick_hatch(self.session.document(), world) {
            if shift {
                if let Some(i) = self.selected_hatches.iter().position(|&x| x == hi) {
                    self.selected_hatches.remove(i);
                } else {
                    self.selected_hatches.push(hi);
                }
            } else {
                self.selected_hatches = vec![hi];
                self.selection.clear();
            }
            self.note = "Hatch selected — Del/E to erase, or apply a pattern in Hatches".into();
            return;
        }
        // Empty click clears both sets (Shift keeps the current selection).
        if !shift {
            self.selection.clear();
            self.selected_hatches.clear();
        }
    }

    /// Finalise a box selection from screen anchor `s` to release `e`.
    ///
    /// AutoCAD convention (screen space):
    /// - left → right (top-left → bottom-right): **window** — fully inside only
    /// - right → left (bottom-right → top-left): **crossing** — touching or inside
    /// Shift adds to the current set.
    fn finish_box(
        &mut self,
        s: egui::Pos2,
        e: egui::Pos2,
        to_world: &impl Fn(&Camera, egui::Pos2) -> Point2,
        ui: &egui::Ui,
    ) {
        if (s - e).length() < 3.0 {
            return; // too small to be a box; treat as a (missed) click
        }
        let (ws, we) = (to_world(&self.camera, s), to_world(&self.camera, e));
        let lo = Point2::new(ws.x.min(we.x), ws.y.min(we.y));
        let hi = Point2::new(ws.x.max(we.x), ws.y.max(we.y));
        // Direction is screen X only (AutoCAD); Y can go either way.
        let crossing = e.x < s.x;
        let hits = box_select(self.session.document(), lo, hi, crossing);
        if ui.input(|i| i.modifiers.shift) {
            for id in hits {
                if !self.selection.contains(&id) {
                    self.selection.push(id);
                }
            }
        } else {
            self.selection = hits;
            self.selected_hatches.clear();
        }
        let n = self.selection.len();
        self.note = if crossing {
            format!("Crossing: {n} selected")
        } else {
            format!("Window: {n} selected")
        };
    }

    /// Free-drag lasso select: the drag path is the region polygon. Direction
    /// (screen L→R vs R→L) picks window vs crossing, like the box.
    fn finish_lasso(
        &mut self,
        to_world: &impl Fn(&Camera, egui::Pos2) -> Point2,
        ui: &egui::Ui,
    ) {
        if self.drag_path.len() < 3 {
            return;
        }
        let polygon: Vec<Point2> = self
            .drag_path
            .iter()
            .map(|p| to_world(&self.camera, *p))
            .collect();
        let (a, b) = (self.drag_path[0], self.drag_path[self.drag_path.len() - 1]);
        let crossing = b.x < a.x;
        let mode = if crossing {
            RegionMode::Crossing
        } else {
            RegionMode::Window
        };
        let hits = select_in_polygon(self.session.document(), &polygon, mode);
        if ui.input(|i| i.modifiers.shift) {
            for id in hits {
                if !self.selection.contains(&id) {
                    self.selection.push(id);
                }
            }
        } else {
            self.selection = hits;
            self.selected_hatches.clear();
        }
        self.sync_selection();
        let n = self.selection.len();
        self.note = format!(
            "Lasso {}: {n} selected",
            if crossing { "crossing" } else { "window" }
        );
    }
}

/// True when a drag path curves enough to be a lasso rather than a box: some
/// point lies well off the straight line from start to end (or it loops back).
fn drag_is_lasso(path: &[egui::Pos2]) -> bool {
    if path.len() < 4 {
        return false;
    }
    let (a, b) = (path[0], path[path.len() - 1]);
    let ab = b - a;
    let len = ab.length();
    if len < 6.0 {
        return true; // returned near the start → a loop, not a box
    }
    let normal = egui::vec2(-ab.y, ab.x) / len;
    let max_dev = path
        .iter()
        .map(|p| (*p - a).dot(normal).abs())
        .fold(0.0_f32, f32::max);
    max_dev > 12.0
}

/// Zoom about the cursor on scroll.
fn handle_zoom(camera: &mut Camera, resp: &egui::Response, rect: egui::Rect, ui: &egui::Ui) {
    let scroll = ui.input(|i| i.raw_scroll_delta.y) as f64;
    if scroll != 0.0 {
        if let Some(pos) = resp.hover_pos() {
            let (cx, cy) = ((pos.x - rect.min.x) as f64, (pos.y - rect.min.y) as f64);
            let anchor = camera.screen_to_world(cx, cy);
            let factor = 1.0015_f64.powf(scroll);
            camera.scale = (camera.scale * factor).clamp(1e-6, 1e9);
            let (w, h) = camera.viewport;
            camera.center = Point2::new(
                anchor.x - (cx - w * 0.5) / camera.scale,
                anchor.y - (h * 0.5 - cy) / camera.scale,
            );
        }
    }
}

// --- Hit testing -------------------------------------------------------------

/// True when an entity may be picked / box-selected (visible + unlocked layer).
fn entity_pickable(doc: &Document, layer: &str) -> bool {
    if !doc.layer_shown(layer) {
        return false;
    }
    !doc.layer(layer).map(|l| l.locked).unwrap_or(false)
}

/// Shortest distance from `p` to drawn geometry (inserts expand to block content).
fn entity_distance(doc: &Document, g: &Geometry, p: Point2) -> f64 {
    match g {
        Geometry::Insert(_) => {
            let geoms = selection_geometries(doc, g);
            let mut best = f64::INFINITY;
            for sg in &geoms {
                best = best.min(entity_distance_primitive(sg, p));
            }
            if best.is_finite() {
                best
            } else if let Geometry::Insert(i) = g {
                p.distance(i.position)
            } else {
                f64::INFINITY
            }
        }
        other => entity_distance_primitive(other, p),
    }
}

fn entity_distance_primitive(g: &Geometry, p: Point2) -> f64 {
    match g {
        Geometry::Line(s) => s.distance_to_point(p),
        Geometry::Polyline(pl) => pl
            .segments()
            .iter()
            .map(|s| s.distance_to_point(p))
            .fold(f64::INFINITY, f64::min),
        Geometry::Circle(c) => (p.distance(c.center) - c.radius).abs(),
        Geometry::Arc(a) => {
            if a.contains_angle(a.angle_of(p)) {
                (p.distance(a.center) - a.radius).abs()
            } else {
                p.distance(a.start_point()).min(p.distance(a.end_point()))
            }
        }
        Geometry::Text(t) => p.distance(t.position),
        Geometry::Insert(i) => p.distance(i.position),
    }
}

/// The nearest entity within `tol` of `p` in the **active model space**, if any.
fn pick_entity(doc: &Document, p: Point2, tol: f64) -> Option<EntityId> {
    let active = doc.active_space();
    let mut best: Option<(f64, EntityId)> = None;
    for e in doc.iter_entities_in_space(active) {
        if !entity_pickable(doc, &e.layer) {
            continue;
        }
        let d = entity_distance(doc, &e.geometry, p);
        if d <= tol && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, e.id));
        }
    }
    best.map(|(_, id)| id)
}

/// Topmost hatch in the active space whose fill contains `p` (later-drawn wins).
/// Hatches have no `EntityId`, so this returns an index into `doc.hatches()`.
fn pick_hatch(doc: &Document, p: Point2) -> Option<usize> {
    let active = doc.active_space();
    doc.hatches()
        .iter()
        .enumerate()
        .rev()
        .find(|(_, h)| h.space == active && entity_pickable(doc, &h.layer) && hatch_contains(h, p))
        .map(|(i, _)| i)
}

/// Point-in-hatch: inside the outer boundary and outside every island (hole).
fn hatch_contains(h: &Hatch, p: Point2) -> bool {
    h.outer().contains_point(p) && !h.boundaries.iter().skip(1).any(|isl| isl.contains_point(p))
}

/// Smallest-area hatchable closed entity whose interior contains `p` — AutoCAD
/// internal-point boundary detection. Innermost wins, so clicking a nested
/// region hatches its own loop. `None` when the pick is in open space.
fn hatch_boundary_at(doc: &Document, p: Point2) -> Option<EntityId> {
    let active = doc.active_space();
    let mut best: Option<(f64, EntityId)> = None;
    for e in doc.iter_entities_in_space(active) {
        if !entity_pickable(doc, &e.layer) {
            continue;
        }
        let area = match &e.geometry {
            Geometry::Polyline(pl) if pl.closed && pl.contains_point(p) => pl.area(),
            Geometry::Circle(c) if c.center.distance(p) <= c.radius => {
                std::f64::consts::PI * c.radius * c.radius
            }
            _ => continue,
        };
        if best.is_none_or(|(ba, _)| area < ba) {
            best = Some((area, e.id));
        }
    }
    best.map(|(_, id)| id)
}

/// A bounding box around a geometry's defining points (inserts expand to block).
fn entity_bbox(doc: &Document, g: &Geometry) -> Option<Bbox> {
    match g {
        Geometry::Insert(_) => {
            let mut pts = Vec::new();
            for sg in selection_geometries(doc, g) {
                if let Some(b) = entity_bbox_primitive(&sg) {
                    pts.push(b.min);
                    pts.push(b.max);
                }
            }
            if pts.is_empty() {
                if let Geometry::Insert(i) = g {
                    return Bbox::of_points([i.position]);
                }
            }
            Bbox::of_points(pts)
        }
        other => entity_bbox_primitive(other),
    }
}

fn entity_bbox_primitive(g: &Geometry) -> Option<Bbox> {
    let pts: Vec<Point2> = match g {
        Geometry::Line(s) => vec![s.a, s.b],
        Geometry::Polyline(p) => p.vertices.clone(),
        Geometry::Circle(c) => vec![
            Point2::new(c.center.x - c.radius, c.center.y - c.radius),
            Point2::new(c.center.x + c.radius, c.center.y + c.radius),
        ],
        Geometry::Arc(a) => vec![
            Point2::new(a.center.x - a.radius, a.center.y - a.radius),
            Point2::new(a.center.x + a.radius, a.center.y + a.radius),
        ],
        Geometry::Text(t) => vec![t.position],
        Geometry::Insert(i) => vec![i.position],
    };
    Bbox::of_points(pts)
}

fn point_in_sel(p: Point2, lo: Point2, hi: Point2) -> bool {
    p.x >= lo.x && p.x <= hi.x && p.y >= lo.y && p.y <= hi.y
}

/// Selection-rectangle edges (world), for crossing tests.
fn sel_edges(lo: Point2, hi: Point2) -> [Segment; 4] {
    [
        Segment::new(Point2::new(lo.x, lo.y), Point2::new(hi.x, lo.y)),
        Segment::new(Point2::new(hi.x, lo.y), Point2::new(hi.x, hi.y)),
        Segment::new(Point2::new(hi.x, hi.y), Point2::new(lo.x, hi.y)),
        Segment::new(Point2::new(lo.x, hi.y), Point2::new(lo.x, lo.y)),
    ]
}

fn segment_crosses_sel(s: Segment, lo: Point2, hi: Point2) -> bool {
    if point_in_sel(s.a, lo, hi) || point_in_sel(s.b, lo, hi) {
        return true;
    }
    sel_edges(lo, hi)
        .iter()
        .any(|e| s.segment_intersection(*e).is_some())
}

fn circle_crosses_sel(center: Point2, radius: f64, lo: Point2, hi: Point2) -> bool {
    if radius <= 0.0 {
        return point_in_sel(center, lo, hi);
    }
    // Circumference intersects the window if any sample lies inside, or the
    // circle crosses a selection edge (AutoCAD: interior-only window does not
    // pick a surrounding circle).
    const N: usize = 32;
    for i in 0..N {
        let ang = (i as f64) * std::f64::consts::TAU / (N as f64);
        let p = Point2::new(center.x + radius * ang.cos(), center.y + radius * ang.sin());
        if point_in_sel(p, lo, hi) {
            return true;
        }
    }
    let circ = aadt_geometry::Circle::new(center, radius);
    for edge in sel_edges(lo, hi) {
        for p in circ.line_intersections(edge) {
            let t = edge.project_param(p);
            if (-1e-9..=1.0 + 1e-9).contains(&t) {
                return true;
            }
        }
    }
    false
}

fn arc_crosses_sel(a: &aadt_geometry::Arc, lo: Point2, hi: Point2) -> bool {
    if point_in_sel(a.start_point(), lo, hi) || point_in_sel(a.end_point(), lo, hi) {
        return true;
    }
    const N: usize = 24;
    for i in 0..=N {
        let t = i as f64 / N as f64;
        let p = a.point_at(t);
        if point_in_sel(p, lo, hi) {
            return true;
        }
    }
    let circ = aadt_geometry::Circle::new(a.center, a.radius);
    for edge in sel_edges(lo, hi) {
        for p in circ.line_intersections(edge) {
            let t = edge.project_param(p);
            if (-1e-9..=1.0 + 1e-9).contains(&t) && a.contains_angle(a.angle_of(p)) {
                return true;
            }
        }
    }
    false
}

/// True when the entire entity lies inside the selection window (AutoCAD window).
fn geometry_fully_inside(doc: &Document, g: &Geometry, lo: Point2, hi: Point2) -> bool {
    match g {
        Geometry::Insert(_) => {
            let geoms = selection_geometries(doc, g);
            if geoms.is_empty() {
                if let Geometry::Insert(i) = g {
                    return point_in_sel(i.position, lo, hi);
                }
            }
            geoms
                .iter()
                .all(|sg| geometry_fully_inside_primitive(sg, lo, hi))
        }
        other => geometry_fully_inside_primitive(other, lo, hi),
    }
}

fn geometry_fully_inside_primitive(g: &Geometry, lo: Point2, hi: Point2) -> bool {
    match g {
        Geometry::Line(s) => point_in_sel(s.a, lo, hi) && point_in_sel(s.b, lo, hi),
        Geometry::Polyline(p) => {
            !p.vertices.is_empty() && p.vertices.iter().all(|v| point_in_sel(*v, lo, hi))
        }
        Geometry::Circle(c) => {
            // Circumscribed AABB of the circle must sit inside the window.
            point_in_sel(
                Point2::new(c.center.x - c.radius, c.center.y - c.radius),
                lo,
                hi,
            ) && point_in_sel(
                Point2::new(c.center.x + c.radius, c.center.y + c.radius),
                lo,
                hi,
            )
        }
        Geometry::Arc(a) => {
            const N: usize = 24;
            (0..=N).all(|i| {
                let t = i as f64 / N as f64;
                point_in_sel(a.point_at(t), lo, hi)
            })
        }
        Geometry::Text(t) => point_in_sel(t.position, lo, hi),
        Geometry::Insert(i) => point_in_sel(i.position, lo, hi),
    }
}

/// True when the entity touches or lies inside the selection area (AutoCAD crossing).
fn geometry_touches_sel(doc: &Document, g: &Geometry, lo: Point2, hi: Point2) -> bool {
    // Fast reject via AABB.
    let Some(bb) = entity_bbox(doc, g) else {
        return false;
    };
    if bb.max.x < lo.x || bb.min.x > hi.x || bb.max.y < lo.y || bb.min.y > hi.y {
        return false;
    }
    match g {
        Geometry::Insert(_) => {
            let geoms = selection_geometries(doc, g);
            if geoms.is_empty() {
                if let Geometry::Insert(i) = g {
                    return point_in_sel(i.position, lo, hi);
                }
            }
            geoms
                .iter()
                .any(|sg| geometry_touches_sel_primitive(sg, lo, hi))
        }
        other => geometry_touches_sel_primitive(other, lo, hi),
    }
}

fn geometry_touches_sel_primitive(g: &Geometry, lo: Point2, hi: Point2) -> bool {
    match g {
        Geometry::Line(s) => segment_crosses_sel(*s, lo, hi),
        Geometry::Polyline(p) => p.segments().iter().any(|s| segment_crosses_sel(*s, lo, hi)),
        Geometry::Circle(c) => circle_crosses_sel(c.center, c.radius, lo, hi),
        Geometry::Arc(a) => arc_crosses_sel(a, lo, hi),
        Geometry::Text(t) => point_in_sel(t.position, lo, hi),
        Geometry::Insert(i) => point_in_sel(i.position, lo, hi),
    }
}

/// Entities selected by a window (fully enclosed) or crossing (touching) box.
/// Only entities in the active model space on visible unlocked layers are selectable.
fn box_select(doc: &Document, lo: Point2, hi: Point2, crossing: bool) -> Vec<EntityId> {
    let active = doc.active_space();
    doc.iter_entities_in_space(active)
        .filter_map(|e| {
            if !entity_pickable(doc, &e.layer) {
                return None;
            }
            let hit = if crossing {
                geometry_touches_sel(doc, &e.geometry, lo, hi)
            } else {
                geometry_fully_inside(doc, &e.geometry, lo, hi)
            };
            hit.then_some(e.id)
        })
        .collect()
}

/// Grip points (endpoints, centre, vertices) for a selected entity.
fn grip_points(g: &Geometry) -> Vec<Point2> {
    match g {
        Geometry::Line(s) => vec![s.a, s.midpoint(), s.b],
        Geometry::Polyline(p) => p.vertices.clone(),
        Geometry::Circle(c) => vec![
            c.center,
            Point2::new(c.center.x + c.radius, c.center.y),
            Point2::new(c.center.x - c.radius, c.center.y),
            Point2::new(c.center.x, c.center.y + c.radius),
            Point2::new(c.center.x, c.center.y - c.radius),
        ],
        Geometry::Arc(a) => vec![a.center, a.start_point(), a.end_point()],
        Geometry::Text(t) => vec![t.position],
        Geometry::Insert(i) => vec![i.position],
    }
}

/// Circumcircle through three non-collinear points.
/// Center of a circle of radius `r` tangent to both infinite lines `l1`,`l2`
/// (AutoCAD CIRCLE TTR). Each line offset by ±r gives four candidate centers;
/// the one nearest `near` (the midpoint of the two pick points) is chosen so the
/// result sits in the quadrant the user pointed at. `None` for parallel lines.
fn tangent_circle_center(l1: Segment, l2: Segment, r: f64, near: Point2) -> Option<(Point2, f64)> {
    let n1 = (l1.b - l1.a).normalized()?.perp();
    let n2 = (l2.b - l2.a).normalized()?.perp();
    let mut best: Option<(f64, Point2)> = None;
    for s1 in [-1.0_f64, 1.0] {
        for s2 in [-1.0_f64, 1.0] {
            let o1 = Segment::new(l1.a + n1 * (s1 * r), l1.b + n1 * (s1 * r));
            let o2 = Segment::new(l2.a + n2 * (s2 * r), l2.b + n2 * (s2 * r));
            if let Some((c, _)) = o1.line_intersection(o2) {
                let dist = c.distance(near);
                if best.map(|(bd, _)| dist < bd).unwrap_or(true) {
                    best = Some((dist, c));
                }
            }
        }
    }
    best.map(|(_, c)| (c, r))
}

fn circle_from_3_points(a: Point2, b: Point2, c: Point2) -> Option<(Point2, f64)> {
    let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
    if d.abs() < 1e-12 {
        return None;
    }
    let a2 = a.x * a.x + a.y * a.y;
    let b2 = b.x * b.x + b.y * b.y;
    let c2 = c.x * c.x + c.y * c.y;
    let ux = (a2 * (b.y - c.y) + b2 * (c.y - a.y) + c2 * (a.y - b.y)) / d;
    let uy = (a2 * (c.x - b.x) + b2 * (a.x - c.x) + c2 * (b.x - a.x)) / d;
    let center = Point2::new(ux, uy);
    let r = center.distance(a);
    if r <= f64::EPSILON {
        None
    } else {
        Some((center, r))
    }
}

/// Local copy of grip-move math for rubber-band preview (mirrors GRIPEDIT command).
fn geometry_with_grip_moved_local(
    g: &Geometry,
    grip_idx: usize,
    new_pt: Point2,
) -> Option<Geometry> {
    match g {
        Geometry::Line(s) => match grip_idx {
            0 => Some(Geometry::Line(aadt_geometry::Segment::new(new_pt, s.b))),
            1 => {
                let mid = s.midpoint();
                let dx = new_pt.x - mid.x;
                let dy = new_pt.y - mid.y;
                Some(Geometry::Line(aadt_geometry::Segment::new(
                    Point2::new(s.a.x + dx, s.a.y + dy),
                    Point2::new(s.b.x + dx, s.b.y + dy),
                )))
            }
            2 => Some(Geometry::Line(aadt_geometry::Segment::new(s.a, new_pt))),
            _ => None,
        },
        Geometry::Polyline(p) => {
            if grip_idx >= p.vertices.len() {
                return None;
            }
            let mut verts = p.vertices.clone();
            verts[grip_idx] = new_pt;
            Some(Geometry::Polyline(aadt_geometry::Polyline::new(
                verts, p.closed,
            )))
        }
        Geometry::Circle(c) => match grip_idx {
            0 => Some(Geometry::Circle(aadt_geometry::Circle::new(
                new_pt, c.radius,
            ))),
            1..=4 => {
                let r = c.center.distance(new_pt);
                (r > f64::EPSILON)
                    .then_some(Geometry::Circle(aadt_geometry::Circle::new(c.center, r)))
            }
            _ => None,
        },
        Geometry::Arc(a) => match grip_idx {
            0 => {
                let dx = new_pt.x - a.center.x;
                let dy = new_pt.y - a.center.y;
                Some(Geometry::Arc(aadt_geometry::Arc::new(
                    Point2::new(a.center.x + dx, a.center.y + dy),
                    a.radius,
                    a.start_angle,
                    a.end_angle,
                )))
            }
            1 => Some(Geometry::Arc(aadt_geometry::Arc::new(
                a.center,
                a.radius,
                a.angle_of(new_pt),
                a.end_angle,
            ))),
            2 => Some(Geometry::Arc(aadt_geometry::Arc::new(
                a.center,
                a.radius,
                a.start_angle,
                a.angle_of(new_pt),
            ))),
            _ => None,
        },
        Geometry::Text(t) => Some(Geometry::Text(aadt_model::Text::new(
            t.value.clone(),
            new_pt,
            t.height,
            t.rotation,
        ))),
        Geometry::Insert(i) => Some(Geometry::Insert(i.at(new_pt, i.rotation))),
    }
}

const GRIP_PICK_PIXELS: f64 = 8.0;

/// Paint a small END (square), MID (triangle), or CEN (circle) marker at the snap point.
fn paint_snap_marker(
    painter: &egui::Painter,
    rect: egui::Rect,
    camera: &Camera,
    kind: SnapKind,
    world: Point2,
) {
    let c = world_to_screen(camera, rect, world);
    let color = egui::Color32::from_rgb(255, 200, 60);
    let stroke = egui::Stroke::new(1.5_f32, color);
    match kind {
        SnapKind::Endpoint => {
            let s = 5.0_f32;
            let r = egui::Rect::from_center_size(c, egui::vec2(s * 2.0, s * 2.0));
            painter.rect_stroke(r, 0.0, stroke);
        }
        SnapKind::Midpoint => {
            let s = 6.0_f32;
            let pts = [
                c + egui::vec2(0.0, -s),
                c + egui::vec2(s, s * 0.6),
                c + egui::vec2(-s, s * 0.6),
            ];
            painter.line_segment([pts[0], pts[1]], stroke);
            painter.line_segment([pts[1], pts[2]], stroke);
            painter.line_segment([pts[2], pts[0]], stroke);
        }
        SnapKind::Center => {
            painter.circle_stroke(c, 5.0, stroke);
            painter.line_segment(
                [c + egui::vec2(-3.0, 0.0), c + egui::vec2(3.0, 0.0)],
                stroke,
            );
            painter.line_segment(
                [c + egui::vec2(0.0, -3.0), c + egui::vec2(0.0, 3.0)],
                stroke,
            );
        }
        SnapKind::Intersection => {
            // X cross (AutoCAD intersection marker).
            let s = 5.0_f32;
            painter.line_segment([c + egui::vec2(-s, -s), c + egui::vec2(s, s)], stroke);
            painter.line_segment([c + egui::vec2(-s, s), c + egui::vec2(s, -s)], stroke);
        }
        SnapKind::Quadrant => {
            // Diamond (AutoCAD quadrant marker).
            let s = 6.0_f32;
            let pts = [
                c + egui::vec2(0.0, -s),
                c + egui::vec2(s, 0.0),
                c + egui::vec2(0.0, s),
                c + egui::vec2(-s, 0.0),
            ];
            painter.line_segment([pts[0], pts[1]], stroke);
            painter.line_segment([pts[1], pts[2]], stroke);
            painter.line_segment([pts[2], pts[3]], stroke);
            painter.line_segment([pts[3], pts[0]], stroke);
        }
        SnapKind::Perpendicular => {
            // Perpendicular tack ⊥ (AutoCAD perpendicular marker).
            let s = 6.0_f32;
            painter.line_segment([c + egui::vec2(-s, s), c + egui::vec2(s, s)], stroke);
            painter.line_segment([c + egui::vec2(0.0, s), c + egui::vec2(0.0, -s)], stroke);
            // Small square at the foot to read as a right angle.
            painter.line_segment([c + egui::vec2(0.0, s * 0.4), c + egui::vec2(s * 0.4, s * 0.4)], stroke);
            painter.line_segment([c + egui::vec2(s * 0.4, s * 0.4), c + egui::vec2(s * 0.4, s)], stroke);
        }
        SnapKind::Tangent => {
            // Circle with a tangent line across the top (AutoCAD tangent marker).
            let r = 4.5_f32;
            painter.circle_stroke(c, r, stroke);
            painter.line_segment(
                [c + egui::vec2(-r - 1.5, -r), c + egui::vec2(r + 1.5, -r)],
                stroke,
            );
        }
        SnapKind::Nearest => {
            // Hourglass / bowtie (AutoCAD nearest marker).
            let s = 5.0_f32;
            let tl = c + egui::vec2(-s, -s);
            let tr = c + egui::vec2(s, -s);
            let bl = c + egui::vec2(-s, s);
            let br = c + egui::vec2(s, s);
            painter.line_segment([tl, tr], stroke);
            painter.line_segment([bl, br], stroke);
            painter.line_segment([tl, br], stroke);
            painter.line_segment([tr, bl], stroke);
        }
    }
}

// --- Painting ----------------------------------------------------------------

fn world_to_screen(camera: &Camera, rect: egui::Rect, p: Point2) -> egui::Pos2 {
    let (sx, sy) = camera.world_to_screen(p);
    egui::pos2(rect.min.x + sx as f32, rect.min.y + sy as f32)
}

/// Paint the draw list into `rect` with egui's 2-D painter using layer ACI
/// colours and print lineweights scaled for the white-paper preview.
/// Underlay commands (trace / other categories) are dimmed.
/// Off-screen commands are skipped (viewport cull).
fn paint(
    painter: &egui::Painter,
    rect: egui::Rect,
    camera: &Camera,
    list: &DrawList,
    doc: &Document,
    show_lweights: bool,
) {
    let view = world_view_bbox(camera).expanded(32.0 / camera.scale.max(1e-9));
    let mut stroke_cache: std::collections::HashMap<String, (egui::Color32, egui::Stroke)> =
        std::collections::HashMap::new();
    for (i, cmd) in list.commands.iter().enumerate() {
        if !cmd_intersects_view(cmd, view) {
            continue;
        }
        let underlay = list.underlay.get(i).copied().unwrap_or(false);
        let layer = cmd.layer();
        let (mut color, mut stroke) = if let Some(cached) = stroke_cache.get(layer) {
            *cached
        } else {
            let s = layer_stroke(doc, layer, camera, show_lweights);
            stroke_cache.insert(layer.to_owned(), s);
            s
        };
        if underlay {
            color = egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 90);
            stroke = egui::Stroke::new(stroke.width.max(0.6), color);
        }
        draw_command(painter, rect, camera, cmd, stroke, color);
    }
}

/// World AABB of the current camera viewport.
fn world_view_bbox(camera: &Camera) -> Bbox {
    let (w, h) = camera.viewport;
    let a = camera.screen_to_world(0.0, 0.0);
    let b = camera.screen_to_world(w, h);
    Bbox::from_corners(a, b)
}

trait BboxExpand {
    fn expanded(self, pad: f64) -> Bbox;
}

impl BboxExpand for Bbox {
    fn expanded(self, pad: f64) -> Bbox {
        Bbox {
            min: Point2::new(self.min.x - pad, self.min.y - pad),
            max: Point2::new(self.max.x + pad, self.max.y + pad),
        }
    }
}

fn cmd_intersects_view(cmd: &DrawCommand, view: Bbox) -> bool {
    match cmd {
        DrawCommand::Polyline { points, .. } | DrawCommand::Fill { points, .. } => {
            Bbox::of_points(points.iter().copied())
                .map(|b| b.intersects(view))
                .unwrap_or(false)
        }
        DrawCommand::Circle { center, radius, .. } | DrawCommand::Arc { center, radius, .. } => {
            let r = *radius;
            Bbox {
                min: Point2::new(center.x - r, center.y - r),
                max: Point2::new(center.x + r, center.y + r),
            }
            .intersects(view)
        }
        DrawCommand::Text {
            position, height, ..
        } => {
            let pad = height.max(0.1) * 8.0;
            Bbox {
                min: Point2::new(position.x - pad, position.y - pad),
                max: Point2::new(position.x + pad, position.y + pad),
            }
            .intersects(view)
        }
        DrawCommand::Marker { position, .. } => {
            let pad = 1.0;
            Bbox {
                min: Point2::new(position.x - pad, position.y - pad),
                max: Point2::new(position.x + pad, position.y + pad),
            }
            .intersects(view)
        }
    }
}

/// Highlight the selected entities and draw their grips.
///
/// AutoCAD-like: bright blue overlay on top of the normal draw (thick soft
/// underglow + crisp stroke) so the pick stays obvious even when layer
/// lineweights are heavy. Block inserts expand to their definition geometry.
fn paint_selection(
    painter: &egui::Painter,
    rect: egui::Rect,
    camera: &Camera,
    doc: &Document,
    selection: &[EntityId],
) {
    if selection.is_empty() {
        return;
    }
    let hi = egui::Color32::from_rgb(30, 120, 255);
    let glow = egui::Color32::from_rgba_unmultiplied(40, 140, 255, 110);
    let grip = egui::Color32::from_rgb(255, 255, 255);
    let grip_edge = egui::Color32::from_rgb(30, 120, 255);
    let n = selection.len();
    for (i, id) in selection.iter().enumerate() {
        let Some(e) = doc.get(*id) else { continue };
        let active = i + 1 == n;
        let glow_w = if active { 8.0_f32 } else { 6.0_f32 };
        let core_w = if active { 3.0_f32 } else { 2.5_f32 };

        for g in selection_geometries(doc, &e.geometry) {
            // Soft underglow, then crisp highlight on top.
            draw_geometry(
                painter,
                rect,
                camera,
                &g,
                egui::Stroke::new(glow_w, glow),
                glow,
            );
            draw_geometry(painter, rect, camera, &g, egui::Stroke::new(core_w, hi), hi);
        }

        for gp in grip_points(&e.geometry) {
            let c = world_to_screen(camera, rect, gp);
            let s = if active { 4.0 } else { 3.5 };
            let r = egui::Rect::from_center_size(c, egui::vec2(s * 2.0, s * 2.0));
            painter.rect_filled(r, 0.0, grip);
            painter.rect_stroke(r, 0.0, egui::Stroke::new(1.5_f32, grip_edge));
        }
    }
}

/// Highlight the selected hatches by outlining their boundary loops in the
/// selection blue (hatches have no grips). Indices are into `doc.hatches()`.
fn paint_selected_hatches(
    painter: &egui::Painter,
    rect: egui::Rect,
    camera: &Camera,
    doc: &Document,
    selected: &[usize],
) {
    if selected.is_empty() {
        return;
    }
    let hi = egui::Color32::from_rgb(30, 120, 255);
    let glow = egui::Color32::from_rgba_unmultiplied(40, 140, 255, 110);
    let hatches = doc.hatches();
    for &idx in selected {
        let Some(h) = hatches.get(idx) else { continue };
        for boundary in &h.boundaries {
            let g = Geometry::Polyline(boundary.clone());
            draw_geometry(painter, rect, camera, &g, egui::Stroke::new(6.0_f32, glow), glow);
            draw_geometry(painter, rect, camera, &g, egui::Stroke::new(2.5_f32, hi), hi);
        }
    }
}

/// Live HATCH preview: translucent accent fill of the region under the cursor
/// plus an accent boundary outline. `g` is the detected closed boundary entity.
fn paint_hatch_region(painter: &egui::Painter, rect: egui::Rect, camera: &Camera, g: &Geometry) {
    let to_screen = |p: Point2| world_to_screen(camera, rect, p);
    let pts: Vec<egui::Pos2> = match g {
        Geometry::Polyline(pl) if pl.closed => pl.vertices.iter().map(|v| to_screen(*v)).collect(),
        Geometry::Circle(c) => (0..48)
            .map(|i| {
                let a = i as f64 / 48.0 * std::f64::consts::TAU;
                to_screen(Point2::new(
                    c.center.x + c.radius * a.cos(),
                    c.center.y + c.radius * a.sin(),
                ))
            })
            .collect(),
        _ => return,
    };
    if pts.len() < 3 {
        return;
    }
    let acc = theme::color::ACCENT;
    let fill = egui::Color32::from_rgba_unmultiplied(acc.r(), acc.g(), acc.b(), 44);
    // Path fill supports concave boundaries; convex_polygon does not.
    painter.add(egui::Shape::Path(egui::epaint::PathShape {
        points: pts.clone(),
        closed: true,
        fill,
        stroke: egui::Stroke::NONE.into(),
    }));
    let outline = egui::Stroke::new(2.0_f32, acc);
    for pair in pts.windows(2) {
        painter.line_segment([pair[0], pair[1]], outline);
    }
    if let (Some(&a), Some(&b)) = (pts.first(), pts.last()) {
        painter.line_segment([b, a], outline);
    }
}

/// World-space geometry used for selection highlight (inserts expand to block content).
fn selection_geometries(doc: &Document, g: &Geometry) -> Vec<Geometry> {
    match g {
        Geometry::Insert(ins) => match doc.block(&ins.name) {
            Some(block) => block
                .entities
                .iter()
                .map(|be| instantiate_block_geom(&be.geometry, block.base, ins))
                .collect(),
            None => vec![g.clone()],
        },
        other => vec![other.clone()],
    }
}

/// Map block-space geometry to world via insert transform (incl. scale).
fn instantiate_block_geom(g: &Geometry, base: Point2, ins: &aadt_model::Insert) -> Geometry {
    g.instanced_scaled(
        base,
        ins.position,
        ins.rotation,
        ins.scale_x,
        ins.scale_y,
    )
}

/// Draw one raw [`Geometry`] with `stroke` (an insert shows a small marker).
fn draw_geometry(
    painter: &egui::Painter,
    rect: egui::Rect,
    camera: &Camera,
    g: &Geometry,
    stroke: egui::Stroke,
    color: egui::Color32,
) {
    let to_screen = |p: Point2| world_to_screen(camera, rect, p);
    match g {
        Geometry::Line(s) => {
            painter.line_segment([to_screen(s.a), to_screen(s.b)], stroke);
        }
        Geometry::Polyline(p) => {
            for pair in p.vertices.windows(2) {
                painter.line_segment([to_screen(pair[0]), to_screen(pair[1])], stroke);
            }
            if p.closed && p.vertices.len() > 2 {
                painter.line_segment(
                    [
                        to_screen(p.vertices[p.vertices.len() - 1]),
                        to_screen(p.vertices[0]),
                    ],
                    stroke,
                );
            }
        }
        Geometry::Circle(c) => {
            painter.circle_stroke(
                to_screen(c.center),
                (c.radius * camera.scale) as f32,
                stroke,
            );
        }
        Geometry::Arc(a) => {
            let sweep = normalize_sweep(a.start_angle, a.end_angle);
            let steps = ((sweep / 360.0 * 48.0).ceil() as usize).max(2);
            let mut prev: Option<egui::Pos2> = None;
            for i in 0..=steps {
                let ang = (a.start_angle + sweep * i as f64 / steps as f64).to_radians();
                let p = to_screen(Point2::new(
                    a.center.x + a.radius * ang.cos(),
                    a.center.y + a.radius * ang.sin(),
                ));
                if let Some(q) = prev {
                    painter.line_segment([q, p], stroke);
                }
                prev = Some(p);
            }
        }
        Geometry::Text(t) => {
            if t.value.trim().is_empty() {
                return;
            }
            let pos = to_screen(t.position);
            let px = ((t.height * camera.scale) as f32).clamp(10.0, 36.0);
            let galley = painter.layout_no_wrap(t.value.clone(), theme::drawing_font(px), color);
            if galley.size().x <= 0.5 || galley.size().y <= 0.5 {
                return;
            }
            // Highlight plate behind text so labels stay readable when selected.
            if color.r() < 80 && color.b() > 180 {
                let pad = 3.0;
                let r = egui::Rect::from_min_size(
                    pos - egui::vec2(pad, galley.size().y + pad),
                    galley.size() + egui::vec2(pad * 2.0, pad * 2.0),
                );
                painter.rect_filled(
                    r,
                    2.0,
                    egui::Color32::from_rgba_unmultiplied(30, 120, 255, 60),
                );
            }
            let mut text_shape = egui::epaint::TextShape::new(pos, galley, color);
            text_shape.angle = (t.rotation as f32).to_radians();
            painter.add(text_shape);
        }
        Geometry::Insert(i) => {
            let c = to_screen(i.position);
            let d = 7.0;
            painter.circle_stroke(c, d, stroke);
            painter.line_segment([c + egui::vec2(-d, 0.0), c + egui::vec2(d, 0.0)], stroke);
            painter.line_segment([c + egui::vec2(0.0, -d), c + egui::vec2(0.0, d)], stroke);
        }
    }
}

/// Draw one [`DrawCommand`] (the already-instantiated draw list).
fn draw_command(
    painter: &egui::Painter,
    rect: egui::Rect,
    camera: &Camera,
    cmd: &DrawCommand,
    stroke: egui::Stroke,
    color: egui::Color32,
) {
    let to_screen = |p: Point2| world_to_screen(camera, rect, p);
    match cmd {
        DrawCommand::Polyline { points, closed, .. } => {
            for pair in points.windows(2) {
                painter.line_segment([to_screen(pair[0]), to_screen(pair[1])], stroke);
            }
            if *closed && points.len() > 2 {
                painter.line_segment(
                    [to_screen(points[points.len() - 1]), to_screen(points[0])],
                    stroke,
                );
            }
        }
        DrawCommand::Circle { center, radius, .. } => {
            painter.circle_stroke(to_screen(*center), (*radius * camera.scale) as f32, stroke);
        }
        DrawCommand::Arc {
            center,
            radius,
            start_angle,
            end_angle,
            ..
        } => {
            let sweep = normalize_sweep(*start_angle, *end_angle);
            let steps = ((sweep / 360.0 * 48.0).ceil() as usize).max(2);
            let mut prev: Option<egui::Pos2> = None;
            for i in 0..=steps {
                let a = (*start_angle + sweep * i as f64 / steps as f64).to_radians();
                let p = to_screen(Point2::new(
                    center.x + radius * a.cos(),
                    center.y + radius * a.sin(),
                ));
                if let Some(q) = prev {
                    painter.line_segment([q, p], stroke);
                }
                prev = Some(p);
            }
        }
        DrawCommand::Text {
            position,
            height,
            rotation,
            value,
            ..
        } => {
            if value.trim().is_empty() {
                return;
            }
            // Keep labels readable at any zoom; CAD heights vary wildly.
            let px = ((*height * camera.scale) as f32).clamp(8.0, 72.0);
            let galley = painter.layout_no_wrap(value.clone(), theme::drawing_font(px), color);
            if galley.size().x <= 0.5 || galley.size().y <= 0.5 {
                return;
            }
            let mut text_shape = egui::epaint::TextShape::new(to_screen(*position), galley, color);
            text_shape.angle = (*rotation as f32).to_radians();
            painter.add(text_shape);
        }
        DrawCommand::Marker { position, .. } => {
            let c = to_screen(*position);
            let d = 4.0;
            painter.line_segment([c + egui::vec2(-d, 0.0), c + egui::vec2(d, 0.0)], stroke);
            painter.line_segment([c + egui::vec2(0.0, -d), c + egui::vec2(0.0, d)], stroke);
        }
        DrawCommand::Fill { points, .. } => {
            if points.len() >= 3 {
                let shape: Vec<egui::Pos2> = points.iter().map(|p| to_screen(*p)).collect();
                // Path fill supports concave hatch boundaries; convex_polygon does not.
                let fill =
                    egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 96);
                painter.add(egui::Shape::Path(egui::epaint::PathShape {
                    points: shape,
                    closed: true,
                    fill,
                    stroke: egui::Stroke::NONE.into(),
                }));
            }
        }
    }
}

/// The in-progress interactive geometry as a dim rubber band to the cursor.
fn paint_preview(
    painter: &egui::Painter,
    rect: egui::Rect,
    camera: &Camera,
    tool: &Tool,
    cursor: Point2,
    doc: &Document,
    selection: &[EntityId],
) {
    let to_screen = |p: Point2| world_to_screen(camera, rect, p);
    let stroke = egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(30, 90, 180));
    let ghost_stroke = egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(80, 140, 220));
    let ghost_color = egui::Color32::from_rgb(80, 140, 220);

    let draw_ghosts = |transform: &dyn Fn(&Geometry) -> Geometry| {
        let mut count = 0usize;
        'outer: for id in selection {
            let Some(e) = doc.get(*id) else { continue };
            for g in selection_geometries(doc, &e.geometry) {
                if count >= 40 {
                    break 'outer;
                }
                draw_geometry(
                    painter,
                    rect,
                    camera,
                    &transform(&g),
                    ghost_stroke,
                    ghost_color,
                );
                count += 1;
            }
        }
    };
    match tool {
        Tool::Line { points } if points.last().is_some() => {
            if let Some(&a) = points.last() {
                painter.line_segment([to_screen(a), to_screen(cursor)], stroke);
            }
        }
        Tool::Polyline { points } if !points.is_empty() => {
            for pair in points.windows(2) {
                painter.line_segment([to_screen(pair[0]), to_screen(pair[1])], stroke);
            }
            if let Some(&last) = points.last() {
                painter.line_segment([to_screen(last), to_screen(cursor)], stroke);
            }
        }
        Tool::RegionSelect { points, fence, .. } if !points.is_empty() => {
            for pair in points.windows(2) {
                painter.line_segment([to_screen(pair[0]), to_screen(pair[1])], stroke);
            }
            if let Some(&last) = points.last() {
                painter.line_segment([to_screen(last), to_screen(cursor)], stroke);
            }
            // Polygon: hint the closing edge back to the first corner.
            if !*fence {
                if let Some(&first) = points.first() {
                    painter.line_segment([to_screen(cursor), to_screen(first)], stroke);
                }
            }
        }
        Tool::Wall { points, .. } if !points.is_empty() => {
            for pair in points.windows(2) {
                painter.line_segment([to_screen(pair[0]), to_screen(pair[1])], stroke);
            }
            if let Some(&last) = points.last() {
                painter.line_segment([to_screen(last), to_screen(cursor)], stroke);
            }
        }
        Tool::Brush { points: Some(pts) } => {
            for pair in pts.windows(2) {
                painter.line_segment([to_screen(pair[0]), to_screen(pair[1])], stroke);
            }
            if let Some(&last) = pts.last() {
                painter.line_segment([to_screen(last), to_screen(cursor)], stroke);
            }
        }
        Tool::Mleader {
            arrow: Some(a),
            landing: None,
            ..
        } => {
            painter.line_segment([to_screen(*a), to_screen(cursor)], stroke);
        }
        Tool::Mleader {
            arrow: Some(a),
            landing: Some(l),
            text_pt: None,
            ..
        } => {
            painter.line_segment([to_screen(*a), to_screen(*l)], stroke);
            painter.line_segment([to_screen(*l), to_screen(cursor)], stroke);
        }
        Tool::Stretch {
            w0: Some(a),
            w1: None,
            ..
        } => {
            paint_box(painter, to_screen(*a), to_screen(cursor));
        }
        Tool::Stretch { base: Some(b), .. } => {
            painter.line_segment([to_screen(*b), to_screen(cursor)], stroke);
        }
        Tool::Dist { first: Some(a) } => {
            painter.line_segment([to_screen(*a), to_screen(cursor)], stroke);
        }
        Tool::Circle {
            mode: CircleMode::Center,
            p1: Some(c),
            diameter,
            ..
        } => {
            let r = c.distance(cursor) * if *diameter { 0.5 } else { 1.0 };
            if r > f64::EPSILON {
                painter.circle_stroke(to_screen(*c), (r * camera.scale) as f32, stroke);
            }
        }
        Tool::Circle {
            mode: CircleMode::TwoPoint,
            p1: Some(a),
            ..
        } => {
            painter.line_segment([to_screen(*a), to_screen(cursor)], stroke);
            let c = a.midpoint(cursor);
            let r = a.distance(cursor) * 0.5;
            if r > f64::EPSILON {
                painter.circle_stroke(to_screen(c), (r * camera.scale) as f32, stroke);
            }
        }
        Tool::Circle {
            mode: CircleMode::ThreePoint,
            p1: Some(a),
            p2: None,
            ..
        } => {
            painter.line_segment([to_screen(*a), to_screen(cursor)], stroke);
        }
        Tool::Circle {
            mode: CircleMode::ThreePoint,
            p1: Some(a),
            p2: Some(b),
            ..
        } => {
            painter.line_segment([to_screen(*a), to_screen(*b)], stroke);
            painter.line_segment([to_screen(*b), to_screen(cursor)], stroke);
            if let Some((c, r)) = circle_from_3_points(*a, *b, cursor) {
                painter.circle_stroke(to_screen(c), (r * camera.scale) as f32, stroke);
            }
        }
        Tool::GripEdit {
            origin,
            id,
            grip_idx,
            mode,
            ..
        } => {
            // Base→cursor reference line for every mode.
            painter.line_segment([to_screen(*origin), to_screen(cursor)], stroke);
            // Stretch previews the moved grip; other modes show only the axis
            // (the transform preview would duplicate MOVE/ROTATE/… math).
            if matches!(mode, GripMode::Stretch) {
                if let Some(e) = doc.get(*id) {
                    if let Some(g) = geometry_with_grip_moved_local(&e.geometry, *grip_idx, cursor) {
                        draw_geometry(
                            painter,
                            rect,
                            camera,
                            &g,
                            egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(80, 140, 220)),
                            egui::Color32::from_rgb(80, 140, 220),
                        );
                    }
                }
            }
        }
        Tool::Rectangle { first: Some(a), .. } | Tool::RoomLabel { first: Some(a), .. } => {
            let corners = [
                *a,
                Point2::new(cursor.x, a.y),
                cursor,
                Point2::new(a.x, cursor.y),
            ];
            for i in 0..4 {
                painter.line_segment(
                    [to_screen(corners[i]), to_screen(corners[(i + 1) % 4])],
                    stroke,
                );
            }
        }
        Tool::Xline {
            origin: Some(o),
            mode: XlineMode::Through,
            ..
        } => {
            let d = cursor - *o;
            if let Some(u) = d.normalized() {
                let ext = 1.0e4;
                let a = *o + aadt_geometry::Vec2::new(-u.x * ext, -u.y * ext);
                let b = *o + aadt_geometry::Vec2::new(u.x * ext, u.y * ext);
                painter.line_segment([to_screen(a), to_screen(b)], stroke);
            }
        }
        // 3-point: after two points, preview the arc through both + cursor.
        Tool::Arc {
            mode: ArcMode::ThreePoint,
            a: Some(a0),
            b: Some(b0),
        } => {
            if let Some((c, r, sa, ea)) = arc_through_3_points(*a0, *b0, cursor) {
                draw_geometry(
                    painter,
                    rect,
                    camera,
                    &Geometry::Arc(aadt_geometry::Arc::new(c, r, sa, ea)),
                    stroke,
                    egui::Color32::from_rgb(30, 90, 180),
                );
            } else {
                painter.line_segment([to_screen(*a0), to_screen(cursor)], stroke);
            }
        }
        // 3-point: first segment guide to the second point.
        Tool::Arc {
            mode: ArcMode::ThreePoint,
            a: Some(a0),
            b: None,
        } => {
            painter.line_segment([to_screen(*a0), to_screen(cursor)], stroke);
        }
        Tool::Arc {
            mode: ArcMode::CenterStartEnd,
            a: Some(c),
            b: None,
        } => {
            painter.line_segment([to_screen(*c), to_screen(cursor)], stroke);
        }
        Tool::Arc {
            mode: ArcMode::CenterStartEnd,
            a: Some(c),
            b: Some(s),
        } => {
            let r = c.distance(*s);
            if r > f64::EPSILON {
                let start_ang = (s.y - c.y).atan2(s.x - c.x).to_degrees();
                let end_ang = (cursor.y - c.y).atan2(cursor.x - c.x).to_degrees();
                // Radius guide + true CCW arc (not a full circle).
                painter.line_segment([to_screen(*c), to_screen(*s)], stroke);
                painter.line_segment([to_screen(*c), to_screen(cursor)], stroke);
                draw_geometry(
                    painter,
                    rect,
                    camera,
                    &Geometry::Arc(aadt_geometry::Arc::new(*c, r, start_ang, end_ang)),
                    stroke,
                    egui::Color32::from_rgb(30, 90, 180),
                );
            }
        }
        Tool::Insert { name } => {
            paint_insert_ghost(painter, rect, camera, doc, name, cursor, stroke);
        }
        Tool::PlaceElem { kind } => {
            paint_insert_ghost(
                painter,
                rect,
                camera,
                doc,
                kind.block_name(),
                cursor,
                stroke,
            );
        }
        Tool::Move {
            collecting: false,
            base: Some(b),
        }
        | Tool::Copy {
            collecting: false,
            base: Some(b),
        } => {
            let dx = cursor.x - b.x;
            let dy = cursor.y - b.y;
            draw_ghosts(&|g| g.translated(dx, dy));
            painter.line_segment([to_screen(*b), to_screen(cursor)], stroke);
        }
        Tool::Rotate {
            collecting: false,
            pivot: Some(piv),
            reference: false,
            ..
        } => {
            let ang_rad = (cursor.y - piv.y).atan2(cursor.x - piv.x);
            let piv = *piv;
            draw_ghosts(&|g| g.rotated(piv, ang_rad));
            painter.line_segment([to_screen(piv), to_screen(cursor)], stroke);
        }
        Tool::Rotate {
            collecting: false,
            pivot: Some(_piv),
            reference: true,
            ref_a: Some(a),
            ref_ang: None,
        } => {
            painter.line_segment([to_screen(*a), to_screen(cursor)], stroke);
        }
        Tool::Rotate {
            collecting: false,
            pivot: Some(piv),
            reference: true,
            ref_a,
            ref_ang: Some(ref_ang),
        } => {
            if let Some(a) = ref_a {
                let delta = (cursor.y - a.y).atan2(cursor.x - a.x).to_degrees() - ref_ang;
                let piv = *piv;
                draw_ghosts(&|g| g.rotated(piv, delta.to_radians()));
                painter.line_segment([to_screen(*a), to_screen(cursor)], stroke);
            } else {
                let delta = (cursor.y - piv.y).atan2(cursor.x - piv.x).to_degrees() - ref_ang;
                let piv = *piv;
                draw_ghosts(&|g| g.rotated(piv, delta.to_radians()));
                painter.line_segment([to_screen(piv), to_screen(cursor)], stroke);
            }
        }
        Tool::Mirror {
            collecting: false,
            a: Some(b),
            b: None,
        }
        | Tool::ZoomWindow { first: Some(b) }
        | Tool::PlotWindow { first: Some(b) }
        | Tool::SectionLine { first: Some(b) }
        | Tool::RevCloud { first: Some(b) } => {
            painter.line_segment([to_screen(*b), to_screen(cursor)], stroke);
        }
        Tool::Mirror {
            collecting: false,
            a: Some(a0),
            b: Some(b0),
        } => {
            painter.line_segment([to_screen(*a0), to_screen(*b0)], stroke);
        }
        Tool::Scale {
            pivot: Some(b),
            ref_a: None,
            ..
        }
        | Tool::Scale {
            pivot: Some(_),
            ref_a: Some(b),
            ..
        } => {
            painter.line_segment([to_screen(*b), to_screen(cursor)], stroke);
        }
        Tool::Dim {
            first: Some(a),
            second: None,
        } => {
            painter.line_segment([to_screen(*a), to_screen(cursor)], stroke);
        }
        Tool::Dim {
            first: Some(a),
            second: Some(b),
        } => {
            // Ghost extension + dim line following the cursor (AutoCAD-like).
            painter.line_segment(
                [to_screen(*a), to_screen(*b)],
                egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(120, 120, 120)),
            );
            if let Some(u) = (*b - *a).normalized() {
                let n = u.perp();
                let offset = (cursor - *a).dot(n);
                let d1 = *a + n * offset;
                let d2 = *b + n * offset;
                painter.line_segment([to_screen(*a), to_screen(d1)], stroke);
                painter.line_segment([to_screen(*b), to_screen(d2)], stroke);
                painter.line_segment([to_screen(d1), to_screen(d2)], stroke);
            }
        }
        Tool::DimContinue => {
            if let Some(base) = doc.dimensions().iter().rev().find(|d| {
                matches!(
                    d.kind,
                    aadt_model::DimensionKind::Aligned | aadt_model::DimensionKind::Linear
                )
            }) {
                let a = base.p2;
                painter.line_segment([to_screen(a), to_screen(cursor)], stroke);
                let v = base.p2 - base.p1;
                if let Some(u) = v.normalized() {
                    let n = u.perp();
                    let offset = (base.text_anchor - base.p1).dot(n);
                    let u2 = (cursor - a).normalized();
                    if let Some(u2) = u2 {
                        let n2 = u2.perp();
                        let d1 = a + n2 * offset;
                        let d2 = cursor + n2 * offset;
                        painter.line_segment([to_screen(a), to_screen(d1)], stroke);
                        painter.line_segment([to_screen(cursor), to_screen(d2)], stroke);
                        painter.line_segment([to_screen(d1), to_screen(d2)], stroke);
                    }
                }
            }
        }
        Tool::DimAngular {
            vertex: Some(v),
            p1: None,
        } => {
            painter.line_segment([to_screen(*v), to_screen(cursor)], stroke);
        }
        Tool::DimAngular {
            vertex: Some(v),
            p1: Some(a),
        } => {
            painter.line_segment([to_screen(*v), to_screen(*a)], stroke);
            painter.line_segment([to_screen(*v), to_screen(cursor)], stroke);
        }
        Tool::Callout {
            arrow: Some(a),
            center: None,
        } => {
            painter.line_segment([to_screen(*a), to_screen(cursor)], stroke);
        }
        Tool::Callout {
            arrow: Some(a),
            center: Some(c),
        } => {
            painter.line_segment([to_screen(*a), to_screen(*c)], stroke);
            painter.circle_stroke(to_screen(*c), 12.0, stroke);
        }
        Tool::Polyline { points } if !points.is_empty() => {
            for pair in points.windows(2) {
                painter.line_segment([to_screen(pair[0]), to_screen(pair[1])], stroke);
            }
            if let Some(&last) = points.last() {
                painter.line_segment([to_screen(last), to_screen(cursor)], stroke);
            }
        }
        Tool::RegionSelect { points, fence, .. } if !points.is_empty() => {
            for pair in points.windows(2) {
                painter.line_segment([to_screen(pair[0]), to_screen(pair[1])], stroke);
            }
            if let Some(&last) = points.last() {
                painter.line_segment([to_screen(last), to_screen(cursor)], stroke);
            }
            // Polygon: hint the closing edge back to the first corner.
            if !*fence {
                if let Some(&first) = points.first() {
                    painter.line_segment([to_screen(cursor), to_screen(first)], stroke);
                }
            }
        }
        _ => {}
    }
}

/// 100×100 px crosshair with a pickbox at the centre (AutoCAD-like).
/// While drafting, the crosshair is slightly stronger; idle select keeps the
/// same size with a clearer pickbox.
fn paint_crosshair_and_pickbox(painter: &egui::Painter, sp: egui::Pos2, drafting: bool) {
    let half = CROSSHAIR_SIZE_PX * 0.5;
    let color = if drafting {
        egui::Color32::from_rgb(40, 40, 40)
    } else {
        egui::Color32::from_rgb(60, 60, 60)
    };
    let stroke = egui::Stroke::new(1.0_f32, color);
    // Horizontal / vertical arms — total 100×100.
    painter.line_segment(
        [egui::pos2(sp.x - half, sp.y), egui::pos2(sp.x + half, sp.y)],
        stroke,
    );
    painter.line_segment(
        [egui::pos2(sp.x, sp.y - half), egui::pos2(sp.x, sp.y + half)],
        stroke,
    );
    // Pickbox (selection aperture) at the intersection.
    let pb = PICKBOX_HALF_PX;
    let box_rect = egui::Rect::from_min_max(
        egui::pos2(sp.x - pb, sp.y - pb),
        egui::pos2(sp.x + pb, sp.y + pb),
    );
    painter.rect_stroke(
        box_rect,
        0.0,
        egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(20, 20, 20)),
    );
}

/// Window (L→R, solid blue) or crossing (R→L, dashed green) selection box.
fn paint_box(painter: &egui::Painter, start: egui::Pos2, cur: egui::Pos2) {
    let crossing = cur.x < start.x;
    let (edge, fill) = if crossing {
        (
            egui::Color32::from_rgb(40, 180, 60),
            egui::Color32::from_rgba_unmultiplied(40, 180, 60, 28),
        )
    } else {
        (
            egui::Color32::from_rgb(40, 100, 220),
            egui::Color32::from_rgba_unmultiplied(40, 100, 220, 28),
        )
    };
    let r = egui::Rect::from_two_pos(start, cur);
    painter.rect_filled(r, 0.0, fill);
    if crossing {
        paint_dashed_rect(painter, r, egui::Stroke::new(1.0_f32, edge), 6.0, 4.0);
    } else {
        painter.rect_stroke(r, 0.0, egui::Stroke::new(1.0_f32, edge));
    }
}

fn paint_dashed_rect(
    painter: &egui::Painter,
    r: egui::Rect,
    stroke: egui::Stroke,
    dash: f32,
    gap: f32,
) {
    let corners = [
        (r.left_top(), r.right_top()),
        (r.right_top(), r.right_bottom()),
        (r.right_bottom(), r.left_bottom()),
        (r.left_bottom(), r.left_top()),
    ];
    for (a, b) in corners {
        paint_dashed_segment(painter, a, b, stroke, dash, gap);
    }
}

fn paint_dashed_segment(
    painter: &egui::Painter,
    a: egui::Pos2,
    b: egui::Pos2,
    stroke: egui::Stroke,
    dash: f32,
    gap: f32,
) {
    let delta = b - a;
    let len = delta.length();
    if len < 0.5 {
        return;
    }
    let dir = delta / len;
    let mut t = 0.0_f32;
    let period = dash + gap;
    while t < len {
        let t1 = (t + dash).min(len);
        painter.line_segment([a + dir * t, a + dir * t1], stroke);
        t += period;
    }
}

/// The dynamic-input box painted at the crosshair — non-interactive, so it tracks
/// the cursor every frame and never intercepts a selection click.
fn paint_dyn_box(painter: &egui::Painter, sp: egui::Pos2, lines: &[String]) {
    if lines.is_empty() {
        return;
    }
    let font = egui::FontId::monospace(13.0);
    let color = egui::Color32::from_gray(20);
    let galleys: Vec<_> = lines
        .iter()
        .map(|l| painter.layout_no_wrap(l.clone(), font.clone(), color))
        .collect();
    let pad = 6.0_f32;
    let w = galleys.iter().map(|g| g.size().x).fold(0.0_f32, f32::max);
    let h = galleys.iter().map(|g| g.size().y).sum::<f32>();
    let origin = sp + egui::vec2(18.0, 18.0);
    let bg = egui::Rect::from_min_size(origin, egui::vec2(w + pad * 2.0, h + pad * 2.0));
    painter.rect_filled(
        bg,
        4.0,
        egui::Color32::from_rgba_unmultiplied(255, 255, 255, 230),
    );
    painter.rect_stroke(
        bg,
        4.0,
        egui::Stroke::new(1.0_f32, egui::Color32::from_gray(160)),
    );
    let mut y = origin.y + pad;
    for g in galleys {
        let dy = g.size().y;
        painter.galley(egui::pos2(origin.x + pad, y), g, color);
        y += dy;
    }
}

fn normalize_sweep(start: f64, end: f64) -> f64 {
    let mut d = end - start;
    while d <= 0.0 {
        d += 360.0;
    }
    d
}

/// Rubber-band ghost for INSERT / place-tool blocks at the cursor.
fn paint_insert_ghost(
    painter: &egui::Painter,
    rect: egui::Rect,
    camera: &Camera,
    doc: &Document,
    name: &str,
    cursor: Point2,
    stroke: egui::Stroke,
) {
    let color = egui::Color32::from_rgb(30, 90, 180);
    match doc.block(name) {
        Some(block) => {
            let ins = aadt_model::Insert::new(name, cursor, 0.0);
            let mut n = 0usize;
            for be in &block.entities {
                if n >= 48 {
                    break;
                }
                let g = instantiate_block_geom(&be.geometry, block.base, &ins);
                draw_geometry(painter, rect, camera, &g, stroke, color);
                n += 1;
            }
            if n == 0 {
                let c = world_to_screen(camera, rect, cursor);
                painter.line_segment([c + egui::vec2(-5.0, 0.0), c + egui::vec2(5.0, 0.0)], stroke);
                painter.line_segment([c + egui::vec2(0.0, -5.0), c + egui::vec2(0.0, 5.0)], stroke);
            }
        }
        None => {
            let c = world_to_screen(camera, rect, cursor);
            painter.line_segment([c + egui::vec2(-5.0, 0.0), c + egui::vec2(5.0, 0.0)], stroke);
            painter.line_segment([c + egui::vec2(0.0, -5.0), c + egui::vec2(0.0, 5.0)], stroke);
        }
    }
}

/// Colour + print-preview stroke for a layer name.
pub(super) fn layer_stroke(
    doc: &Document,
    layer: &str,
    camera: &Camera,
    show_lweights: bool,
) -> (egui::Color32, egui::Stroke) {
    let (aci, lw) = doc
        .layer(layer)
        .map(|l| (l.color, l.lineweight_mm))
        .unwrap_or((7, DEFAULT_LINEWEIGHT_MM));
    let (r, g, b) = aci_to_srgb(aci);
    let color = egui::Color32::from_rgb(r, g, b);
    let px = if show_lweights {
        (lw * (camera.scale as f32) * (25.4 / PLOT_SCALE) * 0.15).clamp(0.8, 10.0)
    } else {
        1.0
    };
    (color, egui::Stroke::new(px, color))
}

#[cfg(test)]
mod dyn_coord_tests {
    use super::{parse_comma_xy, parse_dyn_xy, parse_relative_xy};
    use aadt_geometry::Point2;

    #[test]
    fn parse_dyn_xy_both_fields() {
        let p = parse_dyn_xy("10.5", "-2").unwrap();
        assert!((p.x - 10.5).abs() < 1e-9);
        assert!((p.y + 2.0).abs() < 1e-9);
    }

    #[test]
    fn parse_comma_xy_accepts_pair() {
        assert_eq!(parse_comma_xy("3,4"), Some(Point2::new(3.0, 4.0)));
        assert!(parse_comma_xy("3").is_none());
        assert!(parse_comma_xy("a,b").is_none());
    }

    #[test]
    fn parse_relative_xy_offsets_anchor() {
        let a = Point2::new(10.0, 20.0);
        assert_eq!(parse_relative_xy("@3,4", a), Some(Point2::new(13.0, 24.0)));
        assert!(parse_relative_xy("3,4", a).is_none());
    }
}

#[cfg(test)]
mod arc_tests {
    use super::arc_through_3_points;
    use aadt_geometry::Point2;

    #[test]
    fn three_point_arc_through_upper_semicircle() {
        // p1=(1,0), p2=(0,1) top, p3=(-1,0): unit circle centered at origin.
        let (c, r, sa, ea) = arc_through_3_points(
            Point2::new(1.0, 0.0),
            Point2::new(0.0, 1.0),
            Point2::new(-1.0, 0.0),
        )
        .unwrap();
        assert!(c.x.abs() < 1e-9 && c.y.abs() < 1e-9, "center at origin: {c:?}");
        assert!((r - 1.0).abs() < 1e-9, "radius 1: {r}");
        // CCW from p1 (0°) to p3 (180°) passes through the top (p2).
        assert!((sa - 0.0).abs() < 1e-6, "start 0°: {sa}");
        assert!((ea - 180.0).abs() < 1e-6, "end 180°: {ea}");
    }

    #[test]
    fn three_point_arc_flips_for_clockwise_points() {
        // Same endpoints but p2 at the bottom → the CCW-stored arc must run
        // start=180°→end=360°(0°) so it passes through the bottom.
        let (_c, _r, sa, ea) = arc_through_3_points(
            Point2::new(1.0, 0.0),
            Point2::new(0.0, -1.0),
            Point2::new(-1.0, 0.0),
        )
        .unwrap();
        assert!((sa - 180.0).abs() < 1e-6, "start 180°: {sa}");
        assert!((ea - 0.0).abs() < 1e-6, "end 0°: {ea}");
    }

    #[test]
    fn collinear_points_have_no_arc() {
        assert!(arc_through_3_points(
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(2.0, 0.0),
        )
        .is_none());
    }
}

#[cfg(test)]
mod ttr_tests {
    use super::tangent_circle_center;
    use aadt_geometry::{Point2, Segment};

    #[test]
    fn tangent_to_the_two_axes_in_the_picked_quadrant() {
        let x_axis = Segment::new(Point2::new(-5.0, 0.0), Point2::new(5.0, 0.0));
        let y_axis = Segment::new(Point2::new(0.0, -5.0), Point2::new(0.0, 5.0));
        // r=1, pointing into the first quadrant → center (1,1).
        let (c, r) =
            tangent_circle_center(x_axis, y_axis, 1.0, Point2::new(3.0, 3.0)).unwrap();
        assert!((r - 1.0).abs() < 1e-9);
        assert!(c.distance(Point2::new(1.0, 1.0)) < 1e-9, "got {c:?}");
        // Pointing into the third quadrant → center (-1,-1).
        let (c2, _) =
            tangent_circle_center(x_axis, y_axis, 1.0, Point2::new(-3.0, -3.0)).unwrap();
        assert!(c2.distance(Point2::new(-1.0, -1.0)) < 1e-9, "got {c2:?}");
    }

    #[test]
    fn parallel_lines_have_no_tangent_circle() {
        let a = Segment::new(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0));
        let b = Segment::new(Point2::new(0.0, 4.0), Point2::new(10.0, 4.0));
        assert!(tangent_circle_center(a, b, 1.0, Point2::new(5.0, 2.0)).is_none());
    }
}

#[cfg(test)]
mod rect_corner_tests {
    use super::{rect_corner_polyline, RectCorner};
    use aadt_geometry::Point2;

    #[test]
    fn square_is_four_corners() {
        let v = rect_corner_polyline(
            Point2::new(0.0, 0.0),
            Point2::new(10.0, 6.0),
            RectCorner::Square,
        );
        assert_eq!(v.len(), 4);
    }

    #[test]
    fn chamfer_cuts_each_corner_to_eight_points() {
        let v = rect_corner_polyline(
            Point2::new(0.0, 0.0),
            Point2::new(10.0, 6.0),
            RectCorner::Chamfer(1.0),
        );
        assert_eq!(v.len(), 8, "chamfer = 8 straight vertices");
        // First cut point sits 1.0 in from the bottom-left along the bottom edge.
        assert!(v[0].distance(Point2::new(1.0, 0.0)) < 1e-9, "got {:?}", v[0]);
        // No vertex escapes the rectangle bounds.
        assert!(v.iter().all(|p| p.x >= -1e-9 && p.x <= 10.0 + 1e-9));
    }

    #[test]
    fn fillet_densifies_into_a_rounded_ring() {
        let v = rect_corner_polyline(
            Point2::new(0.0, 0.0),
            Point2::new(10.0, 6.0),
            RectCorner::Fillet(1.0),
        );
        // Corner arcs are sampled, so far more than 8 vertices, all in bounds.
        assert!(v.len() > 8, "fillet densifies corners: {}", v.len());
        assert!(v
            .iter()
            .all(|p| p.x >= -1e-9 && p.x <= 10.0 + 1e-9 && p.y >= -1e-9 && p.y <= 6.0 + 1e-9));
    }

    #[test]
    fn oversized_corner_clamps_to_square() {
        // d larger than half the short side clamps; still a valid ring.
        let v = rect_corner_polyline(
            Point2::new(0.0, 0.0),
            Point2::new(10.0, 6.0),
            RectCorner::Chamfer(100.0),
        );
        assert!(v.len() >= 4);
    }
}

#[cfg(test)]
mod box_select_tests {
    use super::{box_select, geometry_fully_inside, geometry_touches_sel};
    use aadt_geometry::{Point2, Segment};
    use aadt_model::{Document, EntityId, Geometry, Layer};

    struct Sample {
        doc: Document,
        inside: EntityId,
        crossing: EntityId,
        outside: EntityId,
    }

    fn sample() -> Sample {
        let mut doc = Document::new();
        let inside = doc.add_geometry(
            "0",
            Geometry::Line(Segment::new(Point2::new(1.0, 1.0), Point2::new(2.0, 2.0))),
        );
        let crossing = doc.add_geometry(
            "0",
            Geometry::Line(Segment::new(Point2::new(-5.0, 5.0), Point2::new(15.0, 5.0))),
        );
        let outside = doc.add_geometry(
            "0",
            Geometry::Line(Segment::new(
                Point2::new(20.0, 20.0),
                Point2::new(21.0, 21.0),
            )),
        );
        Sample {
            doc,
            inside,
            crossing,
            outside,
        }
    }

    #[test]
    fn window_selects_only_fully_inside() {
        let s = sample();
        let lo = Point2::new(0.0, 0.0);
        let hi = Point2::new(10.0, 10.0);
        let hits = box_select(&s.doc, lo, hi, false);
        assert_eq!(hits, vec![s.inside]);
    }

    #[test]
    fn crossing_selects_inside_and_touching() {
        let s = sample();
        let lo = Point2::new(0.0, 0.0);
        let hi = Point2::new(10.0, 10.0);
        let hits = box_select(&s.doc, lo, hi, true);
        assert_eq!(hits.len(), 2);
        assert!(hits.contains(&s.inside));
        assert!(hits.contains(&s.crossing));
        assert!(!hits.contains(&s.outside));
    }

    #[test]
    fn crossing_rejects_aabb_overlap_without_geometry_hit() {
        let mut doc = Document::new();
        doc.add_geometry(
            "0",
            Geometry::Line(Segment::new(
                Point2::new(-5.0, 15.0),
                Point2::new(15.0, 12.0),
            )),
        );
        let lo = Point2::new(0.0, 0.0);
        let hi = Point2::new(10.0, 10.0);
        let g = Geometry::Line(Segment::new(
            Point2::new(-5.0, 15.0),
            Point2::new(15.0, 12.0),
        ));
        assert!(!geometry_touches_sel(&doc, &g, lo, hi));
        assert!(box_select(&doc, lo, hi, true).is_empty());
        assert!(box_select(&doc, lo, hi, false).is_empty());
    }

    #[test]
    fn partial_line_not_in_window() {
        let doc = Document::new();
        let g = Geometry::Line(Segment::new(Point2::new(-5.0, 5.0), Point2::new(15.0, 5.0)));
        let lo = Point2::new(0.0, 0.0);
        let hi = Point2::new(10.0, 10.0);
        assert!(!geometry_fully_inside(&doc, &g, lo, hi));
        assert!(geometry_touches_sel(&doc, &g, lo, hi));
    }

    #[test]
    fn off_layer_not_pickable() {
        let mut doc = Document::new();
        doc.transact("setup", |d| {
            d.add_layer(Layer::new("HIDDEN"));
            d.add_geometry(
                "HIDDEN",
                Geometry::Line(Segment::new(Point2::new(1.0, 1.0), Point2::new(2.0, 2.0))),
            );
            d.set_layer_visible("HIDDEN", false);
        });
        let lo = Point2::new(0.0, 0.0);
        let hi = Point2::new(10.0, 10.0);
        assert!(box_select(&doc, lo, hi, true).is_empty());
        assert!(super::pick_entity(&doc, Point2::new(1.5, 1.5), 1.0).is_none());
    }

    #[test]
    fn locked_layer_not_pickable() {
        let mut doc = Document::new();
        doc.transact("setup", |d| {
            d.add_layer(Layer::new("LOCK"));
            d.add_geometry(
                "LOCK",
                Geometry::Line(Segment::new(Point2::new(1.0, 1.0), Point2::new(2.0, 2.0))),
            );
            d.set_layer_locked("LOCK", true);
        });
        assert!(super::pick_entity(&doc, Point2::new(1.5, 1.5), 1.0).is_none());
        assert!(box_select(&doc, Point2::new(0.0, 0.0), Point2::new(10.0, 10.0), true).is_empty());
    }
}
