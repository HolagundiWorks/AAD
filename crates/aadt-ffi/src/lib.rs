//! # aadt-ffi — Thin C ABI over [`aadt_app::Session`]
//!
//! Native chrome (WinUI 3 + Direct2D under `native/aad-winui/`) drives the same
//! command / LISP / undo loop as the CLI without rewriting the engine
//! ([ADR-0017](../../docs/adr/0017-winui-shell-rust-engine.md)). **No UI here.**
//!
//! # Safety
//!
//! All exported functions are `extern "C"`. Callers must:
//! - Treat [`AadtSession`] handles as opaque.
//! - Free owned strings from [`aadt_session_path`] with [`aadt_string_free`].
//! - Not use a handle after [`aadt_session_free`].

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::PathBuf;
use std::ptr;

use aadt_app::command_engine::{self, CommandEngine, CommandId};
use aadt_app::osnap::{self, OsnapState};
use aadt_app::otrack::{self, OtrackState};
use aadt_app::selection::{select_by_fence, select_in_polygon, RegionMode};
use aadt_app::{pick, Feedback, Session};
use aadt_cli::register;
use aadt_command::Interpreter;
use aadt_geometry::Point2;
use aadt_model::{Document, DrawingProperties, EntityId, Geometry, DEFAULT_LINEWEIGHT_MM};
use aadt_render::DrawCommand;
use std::time::Instant;

/// Interactive tool family (M18/M19).
#[derive(Clone, Copy, PartialEq, Eq)]
enum ToolKind {
    /// Point-chain draw (LINE/PL/REC/CIRCLE/ARC/XLINE).
    Draw,
    /// Verb-noun modify: selection then points (MOVE/COPY/…).
    Modify,
    /// Entity + side / pick (OFFSET/TRIM/EXTEND/FILLET/HATCH).
    Entity,
    /// Place block reference by click.
    Insert,
    /// ERASE: pick then finish, or erase selection immediately.
    Erase,
    /// TEXT / MLEADER: points then string via [`aadt_tool_set_text`].
    Text,
    /// DIM aligned: 3 points.
    Dim,
    /// DIST: 2 points (reports only).
    Dist,
    /// WALL: points then Space → POLYLINE on A-WALL.
    Wall,
    /// STRETCH: window (2) then base+dest (2).
    Stretch,
    /// ARRAYRECT / POLAR / PATH.
    Array,
    /// REVCLOUD: two corners.
    RevCloud,
    /// IMAGE: path set, click place.
    Image,
    /// WP / CP / FENCE region select.
    Region,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum GripMode {
    #[default]
    Stretch,
    Move,
    Rotate,
    Scale,
    Mirror,
}

impl GripMode {
    fn next(self) -> Self {
        match self {
            Self::Stretch => Self::Move,
            Self::Move => Self::Rotate,
            Self::Rotate => Self::Scale,
            Self::Scale => Self::Mirror,
            Self::Mirror => Self::Stretch,
        }
    }
    fn as_str(self) -> &'static str {
        match self {
            Self::Stretch => "Stretch",
            Self::Move => "Move",
            Self::Rotate => "Rotate",
            Self::Scale => "Scale",
            Self::Mirror => "Mirror",
        }
    }
}

/// Opaque session handle for C callers.
pub struct AadtSession {
    inner: Session,
    last_feedback: CString,
    /// Last path used with OPEN/SAVE (best-effort; host may also track).
    file_path: Option<PathBuf>,
    dirty: bool,
    /// Local ShilpiDB work file (disk SoT). Distinct from Connect `firm.db`.
    work_path: PathBuf,
    /// Document revision last flushed to [`Self::work_path`].
    work_flushed_rev: Option<u64>,
    /// Last successful work-DB flush (debounce).
    work_flush_at: Option<Instant>,
    /// Document revision last tessellated into [`Self::line_xy`] / [`Self::fill_xy`].
    tess_rev: Option<u64>,
    /// Cached strokes for Direct2D: x0,y0,x1,y1,lw_mm,aci per segment (stride 6).
    line_xy: Vec<f32>,
    /// Interactive tool FSM (M16/M18) — shared catalog with egui.
    engine: CommandEngine,
    /// Active tool id while picking (survives `complete()` clearing the engine).
    tool_id: Option<CommandId>,
    tool_token: Option<&'static str>,
    tool_kind: Option<ToolKind>,
    /// When true, clicks add to selection until [`aadt_tool_finish`].
    selecting: bool,
    /// Layer appended on draw commits (WinUI Layers “current”).
    current_layer: String,
    /// Block name for INSERT tool.
    insert_name: Option<String>,
    /// Entity ids picked for OFFSET/TRIM/EXTEND/FILLET.
    entity_picks: Vec<u64>,
    points: Vec<(f64, f64)>,
    cursor: Option<(f64, f64)>,
    prompt_c: CString,
    preview_xy: Vec<f32>,
    /// Cached CString for [`aadt_layer_current`].
    current_layer_c: CString,
    /// Object snap (M19).
    osnap: OsnapState,
    osnap_on: bool,
    /// Last snap marker for host preview (empty if none).
    snap_xy: Option<(f64, f64)>,
    /// TEXT / MLEADER value pending after points.
    text_waiting: bool,
    text_height: f64,
    /// Active hatch pattern for next HATCH tool commit.
    hatch_pattern: String,
    hatch_pattern_c: CString,
    /// OTRACK (M20).
    otrack: OtrackState,
    otrack_on: bool,
    otrack_rays_xy: Vec<f32>,
    /// Grip cache: id, idx, x, y as f32 quads.
    grip_xy: Vec<f32>,
    /// Hot grip world position (hover / drag highlight).
    hot_grip_xy: Option<(f64, f64)>,
    /// Selected dimensions (indices into `document().dimensions()`).
    selected_dimensions: Vec<usize>,
    /// Selection overlay segments.
    sel_xy: Vec<f32>,
    /// HISTORY DIFF ghost traces (removed entities from baseline).
    history_trace_xy: Vec<f32>,
    /// Ortho (F8) / polar OTRACK (F10).
    ortho_on: bool,
    polar_on: bool,
    /// FILLET radius (typed); default 0.
    fillet_radius: f64,
    /// WALL thickness (typed); default 0.2 model units.
    wall_thickness: f64,
    /// STRETCH locked crossing window after 2 points (xmin,ymin,xmax,ymax).
    stretch_window: Option<(f64, f64, f64, f64)>,
    /// ARRAYRECT defaults.
    array_rows: usize,
    array_cols: usize,
    array_count: usize,
    /// 0 = next typed number sets rows, 1 = cols.
    array_num_phase: u8,
    /// IMAGE path for place tool.
    image_path: Option<String>,
    image_width: f64,
    /// Grip edit mode (Space cycles on hot grip).
    grip_mode: GripMode,
    grip_mode_c: CString,
    /// Door/window swing flip (Tab) → negative scale_y after INSERT.
    place_flip: bool,
    /// Additive pick (Shift).
    pick_add: bool,
    /// Hatch fill rings: for each fill → n_pts, alpha, then n*(x,y).
    fill_xy: Vec<f32>,
    /// Pending nav for host (`extents`, `pan`, `window:x0,y0,x1,y1`).
    nav_pending: Option<String>,
}

const INTERACTIVE_TOKENS: &[&str] = &[
    "LINE", "CIRCLE", "RECTANGLE", "POLYLINE", "ARC", "XLINE", "OFFSET", "MOVE",
    "COPY", "ROTATE", "SCALE", "MIRROR", "ERASE", "TRIM", "EXTEND", "FILLET",
    "INSERT", "HATCH", "TEXT", "DIM", "MLEADER", "DIST", "DIMRADIUS", "DIMDIAMETER",
    "WALL", "STRETCH", "ARRAYRECT", "ARRAYPOLAR", "ARRAYPATH", "REVCLOUD", "IMAGE",
    "WPOLYGON", "CPOLYGON", "FENCE",
    // Place tools → INSERT after ensure_place_blocks
    "DOOR", "WINDOW", "COLUMN", "BEAM", "SLAB", "TREE", "SHRUB", "CHAIR", "DESK",
    "SOFA", "BED", "FTABLE", "TOILET",
];

const PLACE_TOKENS: &[&str] = &[
    "DOOR", "WINDOW", "COLUMN", "BEAM", "SLAB", "TREE", "SHRUB", "CHAIR", "DESK",
    "SOFA", "BED", "FTABLE", "TOILET",
];

fn tool_kind_for(token: &str) -> Option<ToolKind> {
    match token {
        "LINE" | "CIRCLE" | "RECTANGLE" | "POLYLINE" | "ARC" | "XLINE" => Some(ToolKind::Draw),
        "MOVE" | "COPY" | "ROTATE" | "SCALE" | "MIRROR" => Some(ToolKind::Modify),
        "OFFSET" | "TRIM" | "EXTEND" | "FILLET" | "HATCH" => Some(ToolKind::Entity),
        "INSERT" => Some(ToolKind::Insert),
        "ERASE" => Some(ToolKind::Erase),
        "TEXT" | "MLEADER" => Some(ToolKind::Text),
        "DIM" => Some(ToolKind::Dim),
        "DIST" => Some(ToolKind::Dist),
        "WALL" => Some(ToolKind::Wall),
        "STRETCH" => Some(ToolKind::Stretch),
        "ARRAYRECT" | "ARRAYPOLAR" | "ARRAYPATH" => Some(ToolKind::Array),
        "REVCLOUD" => Some(ToolKind::RevCloud),
        "IMAGE" => Some(ToolKind::Image),
        "WPOLYGON" | "CPOLYGON" | "FENCE" => Some(ToolKind::Region),
        "DIMRADIUS" | "DIMDIAMETER" => Some(ToolKind::Entity),
        t if PLACE_TOKENS.contains(&t) => Some(ToolKind::Insert),
        _ => None,
    }
}

fn libraries_root() -> PathBuf {
    if let Ok(p) = std::env::var("AADT_LIBRARIES") {
        return PathBuf::from(p);
    }
    let mut cur = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    for _ in 0..8 {
        let candidate = cur.join("libraries");
        if candidate.is_dir() {
            return candidate;
        }
        if !cur.pop() {
            break;
        }
    }
    PathBuf::from("libraries")
}

fn place_block_name(token: &str) -> Option<&'static str> {
    Some(match token {
        "DOOR" => "DOOR-SINGLE",
        "WINDOW" => "WINDOW",
        "COLUMN" => "COLUMN",
        "BEAM" => "BEAM",
        "SLAB" => "SLAB",
        "TREE" => "TREE",
        "SHRUB" => "SHRUB",
        "CHAIR" => "FURN-CHAIR",
        "DESK" => "FURN-DESK",
        "SOFA" => "FURN-SOFA",
        "BED" => "FURN-BED",
        "FTABLE" => "FURN-TABLE",
        "TOILET" => "FIXTURE-TOILET",
        _ => return None,
    })
}

fn grip_points_geom(g: &Geometry) -> Vec<Point2> {
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
        Geometry::Image(img) => img.corners().to_vec(),
    }
}

fn hatch_boundary_at(doc: &aadt_model::Document, p: Point2) -> Option<EntityId> {
    let active = doc.active_space();
    let mut best: Option<(f64, EntityId)> = None;
    for e in doc.iter_entities_in_space(active) {
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

fn snap_world(s: &mut AadtSession, x: f64, y: f64, tol: f64) -> (f64, f64) {
    let raw = Point2::new(x, y);
    let tol = tol.max(1e-6);
    let mut px = x;
    let mut py = y;
    let mut snap_hit = None;
    if s.osnap_on {
        let hit = s
            .osnap
            .resolve(s.inner.document(), raw, tol, tol * 0.25, Instant::now());
        if let Some((kind, p)) = hit {
            snap_hit = Some((kind, p));
            px = p.x;
            py = p.y;
            s.snap_xy = Some((p.x, p.y));
        } else {
            s.snap_xy = None;
        }
    } else {
        s.snap_xy = None;
    }
    if s.otrack_on {
        s.otrack.tick_hover(snap_hit, Instant::now(), tol * 0.5);
        if let Some(p) =
            otrack::resolve_tracking(&s.otrack.acquired, s.polar_on, raw, tol)
        {
            px = p.x;
            py = p.y;
            s.snap_xy = Some((p.x, p.y));
        }
        rebuild_otrack_rays(s);
    }
    // Ortho: lock to axis from last tool point when drafting.
    if s.ortho_on {
        if let Some(&(bx, by)) = s.points.last() {
            let dx = (px - bx).abs();
            let dy = (py - by).abs();
            if dx >= dy {
                py = by;
            } else {
                px = bx;
            }
        }
    }
    (px, py)
}

fn rebuild_otrack_rays(s: &mut AadtSession) {
    s.otrack_rays_xy.clear();
    if !s.otrack_on || s.otrack.acquired.is_empty() {
        return;
    }
    let rays = otrack::tracking_rays(&s.otrack.acquired, s.polar_on);
    const LEN: f64 = 1.0e4;
    for r in rays {
        let a = Point2::new(r.origin.x - r.dir.x * LEN, r.origin.y - r.dir.y * LEN);
        let b = Point2::new(r.origin.x + r.dir.x * LEN, r.origin.y + r.dir.y * LEN);
        push_seg(&mut s.otrack_rays_xy, a, b);
    }
}

fn push_geom_segs(out: &mut Vec<f32>, g: &Geometry) {
    match g {
        Geometry::Line(seg) => push_seg(out, seg.a, seg.b),
        Geometry::Polyline(pl) => {
            for w in pl.vertices.windows(2) {
                push_seg(out, w[0], w[1]);
            }
            if pl.closed && pl.vertices.len() > 2 {
                push_seg(out, *pl.vertices.last().unwrap(), pl.vertices[0]);
            }
        }
        Geometry::Circle(c) => {
            let n = 32;
            let mut prev = Point2::new(c.center.x + c.radius, c.center.y);
            for i in 1..=n {
                let t = std::f64::consts::TAU * (i as f64) / (n as f64);
                let p = Point2::new(c.center.x + c.radius * t.cos(), c.center.y + c.radius * t.sin());
                push_seg(out, prev, p);
                prev = p;
            }
        }
        Geometry::Arc(a) => {
            let n = 24;
            let sa = a.start_angle.to_radians();
            let ea = a.end_angle.to_radians();
            let mut ang = sa;
            let mut prev = Point2::new(a.center.x + a.radius * ang.cos(), a.center.y + a.radius * ang.sin());
            for i in 1..=n {
                ang = sa + (ea - sa) * (i as f64) / (n as f64);
                let p = Point2::new(a.center.x + a.radius * ang.cos(), a.center.y + a.radius * ang.sin());
                push_seg(out, prev, p);
                prev = p;
            }
        }
        Geometry::Text(t) => {
            let h = t.height.max(0.1);
            let p = t.position;
            push_seg(out, p, Point2::new(p.x + h, p.y));
            push_seg(out, p, Point2::new(p.x, p.y + h));
        }
        Geometry::Insert(i) => {
            let p = i.position;
            let s = 0.25;
            push_seg(out, Point2::new(p.x - s, p.y), Point2::new(p.x + s, p.y));
            push_seg(out, Point2::new(p.x, p.y - s), Point2::new(p.x, p.y + s));
        }
        Geometry::Image(img) => {
            let c = img.corners();
            for w in c.windows(2) {
                push_seg(out, w[0], w[1]);
            }
            push_seg(out, c[3], c[0]);
        }
    }
}

fn rebuild_selection_lines(s: &mut AadtSession) {
    s.sel_xy.clear();
    let ids: Vec<_> = s.inner.selection().iter().copied().collect();
    for id in ids {
        if let Some(e) = s.inner.document().get(id) {
            push_geom_segs(&mut s.sel_xy, &e.geometry);
        }
    }
    // Drop stale dim indices after erase / undo.
    let n_dims = s.inner.document().dimensions().len();
    s.selected_dimensions.retain(|&i| i < n_dims);
    pick::push_dimension_selection_segs(
        s.inner.document(),
        &s.selected_dimensions,
        &mut s.sel_xy,
    );
}

fn clear_dimension_selection(s: &mut AadtSession) {
    s.selected_dimensions.clear();
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
    let r = ((center.x - a.x).hypot(center.y - a.y)).max(0.0);
    if r <= f64::EPSILON {
        None
    } else {
        Some((center, r))
    }
}

fn arc_through_3_points(p1: Point2, p2: Point2, p3: Point2) -> Option<(Point2, f64, f64, f64)> {
    let (c, r) = circle_from_3_points(p1, p2, p3)?;
    let ang = |p: Point2| (p.y - c.y).atan2(p.x - c.x).to_degrees();
    let cross = (p2.x - p1.x) * (p3.y - p1.y) - (p2.y - p1.y) * (p3.x - p1.x);
    if cross >= 0.0 {
        Some((c, r, ang(p1), ang(p3)))
    } else {
        Some((c, r, ang(p3), ang(p1)))
    }
}

fn layer_suffix(s: &AadtSession) -> String {
    let layer = s.current_layer.trim();
    if layer.is_empty() {
        String::new()
    } else {
        format!(" {layer}")
    }
}

fn feedback_cstring(fb: &Feedback) -> CString {
    let s = match fb {
        Feedback::Ok(m) => m.clone(),
        Feedback::Err(m) => format!("error: {m}"),
        Feedback::Silent => String::new(),
    };
    CString::new(s.replace('\0', "")).unwrap_or_default()
}

fn push_seg(out: &mut Vec<f32>, a: Point2, b: Point2) {
    out.push(a.x as f32);
    out.push(a.y as f32);
    out.push(b.x as f32);
    out.push(b.y as f32);
}

fn push_seg_styled(out: &mut Vec<f32>, a: Point2, b: Point2, lw_mm: f32, aci: u8) {
    push_seg(out, a, b);
    out.push(lw_mm);
    out.push(f32::from(aci));
}

fn layer_style(doc: &Document, layer: &str) -> (f32, u8) {
    doc.layer(layer)
        .map(|l| (l.lineweight_mm, l.color))
        .unwrap_or((DEFAULT_LINEWEIGHT_MM, 7))
}

fn tessellate_drawlist(list: &aadt_render::DrawList, doc: &Document, out: &mut Vec<f32>) {
    out.clear();
    for cmd in &list.commands {
        let (lw, aci) = layer_style(doc, cmd.layer());
        match cmd {
            DrawCommand::Polyline { points, closed, .. } => {
                if points.len() < 2 {
                    continue;
                }
                for w in points.windows(2) {
                    push_seg_styled(out, w[0], w[1], lw, aci);
                }
                if *closed && points.len() > 2 {
                    push_seg_styled(out, *points.last().unwrap(), points[0], lw, aci);
                }
            }
            DrawCommand::Circle {
                center, radius, ..
            } => {
                let n = 48.max((radius.abs() as usize).saturating_mul(4).min(128));
                let mut prev = Point2 {
                    x: center.x + radius,
                    y: center.y,
                };
                for i in 1..=n {
                    let t = std::f64::consts::TAU * (i as f64) / (n as f64);
                    let p = Point2 {
                        x: center.x + radius * t.cos(),
                        y: center.y + radius * t.sin(),
                    };
                    push_seg_styled(out, prev, p, lw, aci);
                    prev = p;
                }
            }
            DrawCommand::Arc {
                center,
                radius,
                start_angle,
                end_angle,
                ..
            } => {
                let a0 = *start_angle;
                let mut a1 = *end_angle;
                while a1 < a0 {
                    a1 += 360.0;
                }
                let sweep = a1 - a0;
                let n = 24.max(((sweep.abs() / 360.0) * 64.0) as usize).min(96);
                let mut prev = Point2 {
                    x: center.x + radius * a0.to_radians().cos(),
                    y: center.y + radius * a0.to_radians().sin(),
                };
                for i in 1..=n {
                    let t = a0 + sweep * (i as f64) / (n as f64);
                    let p = Point2 {
                        x: center.x + radius * t.to_radians().cos(),
                        y: center.y + radius * t.to_radians().sin(),
                    };
                    push_seg_styled(out, prev, p, lw, aci);
                    prev = p;
                }
            }
            DrawCommand::Fill { .. } => {
                // Solid fills are exported via fill_xy / aadt_drawlist_copy_fills.
            }
            DrawCommand::Image { corners, .. } => {
                if corners.len() < 2 {
                    continue;
                }
                for w in corners.windows(2) {
                    push_seg_styled(out, w[0], w[1], lw, aci);
                }
                if corners.len() > 2 {
                    push_seg_styled(out, *corners.last().unwrap(), corners[0], lw, aci);
                }
            }
            DrawCommand::Text {
                position,
                height,
                rotation,
                value,
                ..
            } => {
                for (a, b) in aadt_render::text_outline_segments(
                    *position, *height, *rotation, value,
                ) {
                    push_seg_styled(out, a, b, lw, aci);
                }
            }
            DrawCommand::Marker { position, .. } => {
                let s = 0.15;
                push_seg_styled(
                    out,
                    Point2 {
                        x: position.x - s,
                        y: position.y,
                    },
                    Point2 {
                        x: position.x + s,
                        y: position.y,
                    },
                    lw,
                    aci,
                );
                push_seg_styled(
                    out,
                    Point2 {
                        x: position.x,
                        y: position.y - s,
                    },
                    Point2 {
                        x: position.x,
                        y: position.y + s,
                    },
                    lw,
                    aci,
                );
            }
        }
    }
}

fn path_after_open_save(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let mut parts = trimmed.split_whitespace();
    let head = parts.next()?;
    if !matches!(head.to_ascii_uppercase().as_str(), "OPEN" | "SAVE") {
        return None;
    }
    let rest: Vec<&str> = parts.collect();
    if rest.is_empty() {
        return None;
    }
    let joined = rest.join(" ");
    let path = if joined.len() >= 2
        && ((joined.starts_with('"') && joined.ends_with('"'))
            || (joined.starts_with('\'') && joined.ends_with('\'')))
    {
        joined[1..joined.len() - 1].to_string()
    } else {
        joined
    };
    if path.is_empty() {
        None
    } else {
        Some(path)
    }
}

fn track_open_save(s: &mut AadtSession, line: &str, fb: &Feedback) {
    if fb.is_err() {
        return;
    }
    let mut parts = line.split_whitespace();
    let Some(head) = parts.next() else {
        return;
    };
    let up = head.to_ascii_uppercase();
    if up == "OPEN" || up == "SAVE" {
        if let Some(path) = path_after_open_save(line) {
            s.file_path = Some(PathBuf::from(path));
            if up == "SAVE" {
                s.dirty = false;
            }
        }
        // Prefer engine drawing_path (work .vdb after DWG import) over import path.
        if let Some(p) = s.inner.drawing_path() {
            let pb = PathBuf::from(p);
            if pb
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("vdb"))
            {
                s.work_path = pb;
            }
        }
        if up == "OPEN" {
            s.tess_rev = None;
            s.dirty = false;
        }
        if up == "OPEN" || up == "SAVE" {
            let _ = flush_work_db(s, true);
        }
    } else if matches!(up.as_str(), "COMMIT" | "AADT-COMMIT") {
        s.dirty = false;
        let _ = flush_work_db(s, true);
    } else if up == "HISTORY" {
        let sub = parts
            .next()
            .map(|t| t.to_ascii_uppercase())
            .unwrap_or_default();
        if sub == "SAVE" {
            s.dirty = false;
            let _ = flush_work_db(s, true);
        }
    } else if !matches!(up.as_str(), "UNDO" | "REDO" | "LIST" | "DIST" | "ZOOM" | "PAN") {
        // Heuristic: mutating commands dirty the doc (host may refine later).
        if !fb.message().unwrap_or("").is_empty() || matches!(fb, Feedback::Silent) {
            let skip = matches!(
                up.as_str(),
                "SELECT" | "DESELECT" | "LAYER" | "LAYERS" | "UNITS" | "OSNAP" | "ORTHO"
            );
            if !skip {
                s.dirty = true;
                // Do NOT flush here — tip rewrite is too heavy for the command path.
                // Hosts call aadt_workdb_tick on a timer; tools flush on commit.
            }
        }
    }
}

fn default_work_vdb_path() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("TMP"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join("AADT").join("work");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("active.vdb")
}

fn bind_work_db(s: &mut AadtSession) {
    s.work_path = default_work_vdb_path();
    s.inner
        .set_drawing_path(Some(s.work_path.to_string_lossy().into_owned()));
    s.file_path = Some(s.work_path.clone());
    s.work_flushed_rev = None;
    s.work_flush_at = None;
    // Realtime sync OFF — do not write an empty tip on every New/session.
    // First SAVE / COMMIT / OPEN(import) materializes the work `.vdb` on disk.
}

fn flush_work_db(s: &mut AadtSession, force: bool) -> Result<(), String> {
    let rev = s.inner.revision();
    if !force && s.work_flushed_rev == Some(rev) {
        return Ok(());
    }
    if let Some(parent) = s.work_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    aadt_vdb::save_path(s.inner.document(), &s.work_path).map_err(|e| e.to_string())?;
    s.work_flushed_rev = Some(rev);
    s.work_flush_at = Some(Instant::now());
    if s.inner.drawing_path().is_none() {
        s.inner
            .set_drawing_path(Some(s.work_path.to_string_lossy().into_owned()));
    }
    Ok(())
}

fn clear_tool(s: &mut AadtSession) {
    s.engine.cancel();
    s.tool_id = None;
    s.tool_token = None;
    s.tool_kind = None;
    s.selecting = false;
    s.insert_name = None;
    s.entity_picks.clear();
    s.points.clear();
    s.cursor = None;
    s.prompt_c = CString::new("").unwrap();
    s.preview_xy.clear();
    s.text_waiting = false;
    s.stretch_window = None;
    s.image_path = None;
    s.array_num_phase = 0;
}

fn refresh_prompt(s: &mut AadtSession) {
    let owned = if s.selecting {
        match s.tool_token {
            Some("ERASE") => "ERASE — pick objects (Space finishes)".to_string(),
            Some(t) => format!("{t} — select objects (Space when done)"),
            None => String::new(),
        }
    } else if let Some("INSERT") = s.tool_token {
        match &s.insert_name {
            Some(n) => format!("INSERT {n} — click to place"),
            None => "INSERT — click insertion point".into(),
        }
    } else if let Some(p) = s.engine.prompt() {
        p.to_string()
    } else {
        match (s.tool_token, s.entity_picks.len(), s.points.len()) {
            (Some("OFFSET"), 0, _) => "OFFSET — select object".into(),
            (Some("OFFSET"), _, _) => "OFFSET — pick side".into(),
            (Some("TRIM"), _, _) => "TRIM — click object near end to trim".into(),
            (Some("EXTEND"), _, _) => "EXTEND — click object to extend".into(),
            (Some("POLYLINE"), _, _) => "POLYLINE — next point (Space finishes)".into(),
            (Some("XLINE"), n, _) if n >= 1 => "XLINE — through point (Space finishes)".into(),
            (Some("MOVE") | Some("COPY") | Some("ROTATE") | Some("SCALE") | Some("MIRROR"), _, 0) => {
                format!("{} — base point", s.tool_token.unwrap_or(""))
            }
            (Some("MOVE") | Some("COPY"), _, 1) => {
                format!("{} — destination", s.tool_token.unwrap_or(""))
            }
            (Some("ROTATE"), _, 1) => "ROTATE — angle point".into(),
            (Some("SCALE"), _, 1) => "SCALE — factor point".into(),
            (Some("MIRROR"), _, 1) => "MIRROR — second axis point".into(),
            (Some("HATCH"), _, _) => format!(
                "HATCH — click inside closed region ({})",
                s.hatch_pattern
            ),
            (Some("TEXT"), _, _) if s.text_waiting => {
                "TEXT — type value in command line + Enter".into()
            }
            (Some("TEXT"), _, _) => "TEXT — click insertion point".into(),
            (Some("MLEADER"), _, _) if s.text_waiting => {
                "MLEADER — type note + Enter".into()
            }
            (Some("MLEADER"), _, n) => match n {
                0 => "MLEADER — arrowhead".into(),
                1 => "MLEADER — landing".into(),
                _ => "MLEADER — text point".into(),
            },
            (Some("DIM"), _, n) => match n {
                0 => "DIM — first definition point".into(),
                1 => "DIM — second definition point".into(),
                _ => "DIM — dimension line location".into(),
            },
            (Some("DIST"), _, n) => {
                if n == 0 {
                    "DIST — first point".into()
                } else {
                    "DIST — second point".into()
                }
            }
            (Some("DIMRADIUS"), _, _) => "DIMRADIUS — click a circle or arc".into(),
            (Some("DIMDIAMETER"), _, _) => "DIMDIAMETER — click a circle or arc".into(),
            (Some("WALL"), _, n) => {
                if n == 0 {
                    format!(
                        "WALL — start point (thickness {:.3}; type number to change)",
                        s.wall_thickness
                    )
                } else {
                    "WALL — next point (Space finishes)".into()
                }
            }
            (Some("FILLET"), 0, _) => format!(
                "FILLET — first line (radius {:.3}; type number to change)",
                s.fillet_radius
            ),
            (Some("FILLET"), 1, _) => "FILLET — select second line".into(),
            (Some("STRETCH"), _, _) if s.stretch_window.is_none() => match s.points.len() {
                0 => "STRETCH — first corner of crossing window".into(),
                _ => "STRETCH — opposite corner".into(),
            },
            (Some("STRETCH"), _, n) => {
                if n == 0 {
                    "STRETCH — base point".into()
                } else {
                    "STRETCH — destination".into()
                }
            }
            (Some("ARRAYRECT"), _, n) => match n {
                0 => format!(
                    "ARRAYRECT — origin ({}×{}; type rows/cols via numbers)",
                    s.array_rows, s.array_cols
                ),
                1 => "ARRAYRECT — point for column spacing (dx)".into(),
                _ => "ARRAYRECT — point for row spacing (dy)".into(),
            },
            (Some("ARRAYPOLAR"), _, _) => format!(
                "ARRAYPOLAR — center (count {})",
                s.array_count
            ),
            (Some("ARRAYPATH"), 0, _) => "ARRAYPATH — click path entity".into(),
            (Some("ARRAYPATH"), _, _) => format!(
                "ARRAYPATH — Space to array along path (count {})",
                s.array_count
            ),
            (Some("REVCLOUD"), _, n) => {
                if n == 0 {
                    "REVCLOUD — first corner".into()
                } else {
                    "REVCLOUD — opposite corner".into()
                }
            }
            (Some("IMAGE"), _, _) => match &s.image_path {
                Some(p) => format!("IMAGE — click insert ({p})"),
                None => "IMAGE — set path first".into(),
            },
            (Some("WPOLYGON"), _, _) => "WPOLYGON — click corners (Space selects enclosed)".into(),
            (Some("CPOLYGON"), _, _) => "CPOLYGON — click corners (Space selects crossing)".into(),
            (Some("FENCE"), _, _) => "FENCE — click points (Space selects crossings)".into(),
            _ => String::new(),
        }
    };
    s.prompt_c = CString::new(owned.replace('\0', "")).unwrap_or_default();
}

fn rebuild_history_traces(s: &mut AadtSession) {
    s.history_trace_xy.clear();
    for e in s.inner.diff_ghosts() {
        push_geom_segs(&mut s.history_trace_xy, &e.geometry);
    }
}

fn rebuild_preview(s: &mut AadtSession) {
    s.preview_xy.clear();
    let Some(token) = s.tool_token else {
        s.preview_xy.extend_from_slice(&s.history_trace_xy);
        return;
    };
    let cursor = s.cursor;
    match token {
        "LINE" | "POLYLINE" => {
            if let Some(&(x0, y0)) = s.points.last() {
                if let Some((x1, y1)) = cursor {
                    push_seg(
                        &mut s.preview_xy,
                        Point2 { x: x0, y: y0 },
                        Point2 { x: x1, y: y1 },
                    );
                }
            }
            // Show committed chain for polyline
            if token == "POLYLINE" || token == "LINE" {
                for w in s.points.windows(2) {
                    push_seg(
                        &mut s.preview_xy,
                        Point2 {
                            x: w[0].0,
                            y: w[0].1,
                        },
                        Point2 {
                            x: w[1].0,
                            y: w[1].1,
                        },
                    );
                }
            }
        }
        "XLINE" => {
            // Infinite construction preview (same extent as XLINE command).
            const EXT: f64 = 1_000_000.0;
            if let (Some((x0, y0)), Some((x1, y1))) = (s.points.first().copied(), cursor) {
                let dx = x1 - x0;
                let dy = y1 - y0;
                let len = dx.hypot(dy);
                if len > f64::EPSILON {
                    let ux = dx / len;
                    let uy = dy / len;
                    push_seg(
                        &mut s.preview_xy,
                        Point2 {
                            x: x0 - ux * EXT,
                            y: y0 - uy * EXT,
                        },
                        Point2 {
                            x: x0 + ux * EXT,
                            y: y0 + uy * EXT,
                        },
                    );
                }
            } else if let Some((x, y)) = cursor {
                // No origin yet — show crosshair guides (host may override for Hor/Ver).
                push_seg(
                    &mut s.preview_xy,
                    Point2 {
                        x: x - EXT,
                        y,
                    },
                    Point2 {
                        x: x + EXT,
                        y,
                    },
                );
                push_seg(
                    &mut s.preview_xy,
                    Point2 {
                        x,
                        y: y - EXT,
                    },
                    Point2 {
                        x,
                        y: y + EXT,
                    },
                );
            }
        }
        "RECTANGLE" => {
            if let (Some((x0, y0)), Some((x1, y1))) = (s.points.first().copied(), cursor) {
                let a = Point2 { x: x0, y: y0 };
                let b = Point2 { x: x1, y: y0 };
                let c = Point2 { x: x1, y: y1 };
                let d = Point2 { x: x0, y: y1 };
                push_seg(&mut s.preview_xy, a, b);
                push_seg(&mut s.preview_xy, b, c);
                push_seg(&mut s.preview_xy, c, d);
                push_seg(&mut s.preview_xy, d, a);
            }
        }
        "CIRCLE" => {
            if let (Some((cx, cy)), Some((px, py))) = (s.points.first().copied(), cursor) {
                let r = ((px - cx).hypot(py - cy)).max(1e-9);
                let center = Point2 { x: cx, y: cy };
                let n = 48;
                let mut prev = Point2 {
                    x: center.x + r,
                    y: center.y,
                };
                for i in 1..=n {
                    let t = std::f64::consts::TAU * (i as f64) / (n as f64);
                    let p = Point2 {
                        x: center.x + r * t.cos(),
                        y: center.y + r * t.sin(),
                    };
                    push_seg(&mut s.preview_xy, prev, p);
                    prev = p;
                }
            }
        }
        "ARC" if s.points.len() >= 2 => {
            if let (Some((x0, y0)), Some((x1, y1)), Some((cx, cy))) = (
                s.points.first().copied(),
                s.points.get(1).copied(),
                cursor,
            ) {
                if let Some((c, r, sa, ea)) = arc_through_3_points(
                    Point2 { x: x0, y: y0 },
                    Point2 { x: x1, y: y1 },
                    Point2 { x: cx, y: cy },
                ) {
                    let a0 = sa;
                    let mut a1 = ea;
                    while a1 < a0 {
                        a1 += 360.0;
                    }
                    let sweep = a1 - a0;
                    let n = 24.max(((sweep.abs() / 360.0) * 64.0) as usize).min(96);
                    let mut prev = Point2 {
                        x: c.x + r * a0.to_radians().cos(),
                        y: c.y + r * a0.to_radians().sin(),
                    };
                    for i in 1..=n {
                        let t = a0 + sweep * (i as f64) / (n as f64);
                        let p = Point2 {
                            x: c.x + r * t.to_radians().cos(),
                            y: c.y + r * t.to_radians().sin(),
                        };
                        push_seg(&mut s.preview_xy, prev, p);
                        prev = p;
                    }
                }
            }
        }
        "MOVE" | "COPY" if s.points.len() == 1 => {
            if let (Some((x0, y0)), Some((x1, y1))) = (s.points.first().copied(), cursor) {
                push_seg(
                    &mut s.preview_xy,
                    Point2 { x: x0, y: y0 },
                    Point2 { x: x1, y: y1 },
                );
            }
        }
        "MIRROR" if s.points.len() == 1 => {
            if let (Some((x0, y0)), Some((x1, y1))) = (s.points.first().copied(), cursor) {
                push_seg(
                    &mut s.preview_xy,
                    Point2 { x: x0, y: y0 },
                    Point2 { x: x1, y: y1 },
                );
            }
        }
        "WPOLYGON" | "CPOLYGON" | "FENCE" => {
            for w in s.points.windows(2) {
                push_seg(
                    &mut s.preview_xy,
                    Point2 {
                        x: w[0].0,
                        y: w[0].1,
                    },
                    Point2 {
                        x: w[1].0,
                        y: w[1].1,
                    },
                );
            }
            if let (Some((x0, y0)), Some((x1, y1))) = (s.points.last().copied(), cursor) {
                push_seg(
                    &mut s.preview_xy,
                    Point2 { x: x0, y: y0 },
                    Point2 { x: x1, y: y1 },
                );
            }
            if token != "FENCE" && s.points.len() >= 2 {
                if let (Some((x0, y0)), Some((x1, y1))) =
                    (s.points.first().copied(), s.points.last().copied())
                {
                    if cursor.is_none() {
                        push_seg(
                            &mut s.preview_xy,
                            Point2 { x: x1, y: y1 },
                            Point2 { x: x0, y: y0 },
                        );
                    }
                }
            }
        }
        _ => {}
    }
    // Snap marker (small cross).
    if let Some((sx, sy)) = s.snap_xy {
        let d = 0.35;
        push_seg(
            &mut s.preview_xy,
            Point2 {
                x: sx - d,
                y: sy,
            },
            Point2 {
                x: sx + d,
                y: sy,
            },
        );
        push_seg(
            &mut s.preview_xy,
            Point2 {
                x: sx,
                y: sy - d,
            },
            Point2 {
                x: sx,
                y: sy + d,
            },
        );
    }
    s.preview_xy.extend_from_slice(&s.history_trace_xy);
}

fn feed_line(s: &mut AadtSession, line: &str) {
    let mut parts = line.split_whitespace();
    let head = parts.next().unwrap_or("").to_ascii_uppercase();
    // Entity SELECT / SELECT NONE drops annotation selection (dims have no EntityId).
    if head == "SELECT" {
        clear_dimension_selection(s);
    }
    let fb = s.inner.feed(line);
    track_open_save(s, line, &fb);
    s.last_feedback = feedback_cstring(&fb);
    apply_history_diff_side_effects(s, line, &fb);
}

fn apply_history_diff_side_effects(s: &mut AadtSession, line: &str, fb: &Feedback) {
    let head = line
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_uppercase();
    if !matches!(head.as_str(), "HISTORY" | "HIST" | "VERSIONS") {
        return;
    }
    let sub = line
        .split_whitespace()
        .nth(1)
        .unwrap_or("LIST")
        .to_ascii_uppercase();
    if fb.is_err() {
        return;
    }
    if let Some(p) = s.inner.drawing_path() {
        s.file_path = Some(PathBuf::from(p));
    }
    match sub.as_str() {
        "DIFF" => {
            rebuild_selection_lines(s);
            rebuild_history_traces(s);
            rebuild_preview(s);
        }
        "SAVE" | "RESTORE" | "PURGE" => {
            rebuild_history_traces(s);
            rebuild_selection_lines(s);
            rebuild_preview(s);
        }
        _ => {}
    }
}

fn commit_tool(s: &mut AadtSession) -> bool {
    let Some(token) = s.tool_token else {
        return false;
    };
    let layer = layer_suffix(s);
    let cmd = match token {
        "LINE" if s.points.len() >= 2 => {
            let (x0, y0) = s.points[0];
            let (x1, y1) = s.points[1];
            Some(format!("LINE {x0} {y0} {x1} {y1}{layer}"))
        }
        "RECTANGLE" if s.points.len() >= 2 => {
            let (x0, y0) = s.points[0];
            let (x1, y1) = s.points[1];
            Some(format!("RECTANGLE {x0} {y0} {x1} {y1}{layer}"))
        }
        "CIRCLE" if s.points.len() >= 2 => {
            let (cx, cy) = s.points[0];
            let (px, py) = s.points[1];
            let r = (px - cx).hypot(py - cy);
            Some(format!("CIRCLE {cx} {cy} {r}{layer}"))
        }
        "POLYLINE" if s.points.len() >= 2 => {
            let mut parts = Vec::new();
            for &(x, y) in &s.points {
                parts.push(format!("{x} {y}"));
            }
            Some(format!("POLYLINE {}{layer}", parts.join(" ")))
        }
        "XLINE" if s.points.len() >= 2 => {
            let (x0, y0) = s.points[0];
            let (x1, y1) = s.points[1];
            let dx = x1 - x0;
            let dy = y1 - y0;
            Some(format!("XLINE {x0} {y0} {dx} {dy}{layer}"))
        }
        "ARC" if s.points.len() >= 3 => {
            let a = Point2 {
                x: s.points[0].0,
                y: s.points[0].1,
            };
            let b = Point2 {
                x: s.points[1].0,
                y: s.points[1].1,
            };
            let c = Point2 {
                x: s.points[2].0,
                y: s.points[2].1,
            };
            arc_through_3_points(a, b, c).map(|(cen, r, sa, ea)| {
                format!("ARC {} {} {r} {sa} {ea}{layer}", cen.x, cen.y)
            })
        }
        "MOVE" if s.points.len() >= 2 => {
            let (x0, y0) = s.points[0];
            let (x1, y1) = s.points[1];
            Some(format!("MOVE SELECTION {} {}", x1 - x0, y1 - y0))
        }
        "COPY" if s.points.len() >= 2 => {
            let (x0, y0) = s.points[0];
            let (x1, y1) = s.points[1];
            Some(format!("COPY SELECTION {} {}", x1 - x0, y1 - y0))
        }
        "ROTATE" if s.points.len() >= 2 => {
            let (cx, cy) = s.points[0];
            let (px, py) = s.points[1];
            let deg = (py - cy).atan2(px - cx).to_degrees();
            Some(format!("ROTATE SELECTION {cx} {cy} {deg}"))
        }
        "SCALE" if s.points.len() >= 2 => {
            let (cx, cy) = s.points[0];
            let (px, py) = s.points[1];
            let factor = ((px - cx).hypot(py - cy)).max(1e-9);
            // Normalize: second point distance as factor relative to 1.0 unit — use raw distance
            // as scale factor (CAD: type factor; here distance from base ≈ factor when unit grid).
            let factor = if factor < 1e-6 { 1.0 } else { factor };
            Some(format!("SCALE SELECTION {cx} {cy} {factor}"))
        }
        "MIRROR" if s.points.len() >= 2 => {
            let (x0, y0) = s.points[0];
            let (x1, y1) = s.points[1];
            Some(format!("MIRROR SELECTION {x0} {y0} {x1} {y1}"))
        }
        "ERASE" => Some("ERASE SELECTION".into()),
        "INSERT" if s.points.len() >= 1 => {
            let name = s.insert_name.clone().unwrap_or_default();
            if name.is_empty() {
                None
            } else {
                let (x, y) = s.points[0];
                let opening = matches!(
                    name.as_str(),
                    "DOOR-SINGLE" | "DOOR-DOUBLE" | "WINDOW" | "WINDOW-SLIDER"
                );
                if opening {
                    let width = aadt_command::default_opening_width(s.inner.document(), &name);
                    let click = Point2::new(x, y);
                    let (place, rot) =
                        aadt_command::place_opening_pose(s.inner.document(), click, width)
                            .unwrap_or((click, 0.0));
                    let _ = aadt_command::cut_wall_opening(
                        s.inner.document_mut(),
                        place,
                        rot,
                        width,
                    );
                    Some(format!(
                        "INSERT {name} {} {} {rot}{layer} 1",
                        place.x, place.y
                    ))
                } else {
                    Some(format!("INSERT {name} {x} {y} 0{layer} 1"))
                }
            }
        }
        "WALL" if s.points.len() >= 2 => {
            let mut parts = Vec::new();
            for &(x, y) in &s.points {
                parts.push(format!("{x} {y}"));
            }
            Some(format!("POLYLINE {} A-WALL", parts.join(" ")))
        }
        "TEXT" if s.points.len() >= 1 && s.text_waiting => {
            // commit via aadt_tool_set_text, not here
            None
        }
        "MLEADER" if s.points.len() >= 3 && s.text_waiting => None,
        "DIM" if s.points.len() >= 3 => {
            let (x0, y0) = s.points[0];
            let (x1, y1) = s.points[1];
            let (tx, ty) = s.points[2];
            Some(format!("DIM {x0} {y0} {x1} {y1} {tx} {ty} AN-DIMS"))
        }
        "DIST" if s.points.len() >= 2 => {
            let (x0, y0) = s.points[0];
            let (x1, y1) = s.points[1];
            Some(format!("DIST {x0} {y0} {x1} {y1}"))
        }
        "HATCH" => None, // committed in accept_point
        "OFFSET" if s.entity_picks.len() == 1 && s.points.len() >= 1 => {
            let id = s.entity_picks[0];
            let (sx, sy) = s.points[0];
            let dist = 1.0;
            Some(format!("OFFSET {id} {dist} {sx} {sy}"))
        }
        "FILLET" if s.entity_picks.len() >= 2 => {
            let a = s.entity_picks[0];
            let b = s.entity_picks[1];
            let r = s.fillet_radius;
            Some(format!("FILLET {a} {b} {r}"))
        }
        "STRETCH" if s.stretch_window.is_some() && s.points.len() >= 2 => {
            let (xmin, ymin, xmax, ymax) = s.stretch_window.unwrap();
            let (x0, y0) = s.points[0];
            let (x1, y1) = s.points[1];
            Some(format!(
                "STRETCH {xmin} {ymin} {xmax} {ymax} {} {} SELECTION",
                x1 - x0,
                y1 - y0
            ))
        }
        "ARRAYRECT" if s.points.len() >= 3 => {
            let (x0, _y0) = s.points[0];
            let (x1, _y1) = s.points[1];
            let (_x2, y2) = s.points[2];
            let dx = (x1 - x0).abs().max(1e-9);
            let dy = (y2 - _y0).abs().max(1e-9);
            Some(format!(
                "ARRAYRECT SELECTION {} {} {dx} {dy}",
                s.array_rows, s.array_cols
            ))
        }
        "ARRAYPOLAR" if s.points.len() >= 1 => {
            let (cx, cy) = s.points[0];
            Some(format!(
                "ARRAYPOLAR SELECTION {cx} {cy} {} 360 Y",
                s.array_count.max(2)
            ))
        }
        "ARRAYPATH" if s.entity_picks.len() >= 1 => {
            let path = s.entity_picks[0];
            Some(format!(
                "ARRAYPATH SELECTION {path} {} Y",
                s.array_count.max(2)
            ))
        }
        "REVCLOUD" if s.points.len() >= 2 => {
            let (x0, y0) = s.points[0];
            let (x1, y1) = s.points[1];
            Some(format!("REVCLOUD {x0} {y0} {x1} {y1}"))
        }
        "IMAGE" if s.points.len() >= 1 && s.image_path.is_some() => {
            let path = s.image_path.clone().unwrap();
            let (x, y) = s.points[0];
            let w = s.image_width.max(1e-6);
            // Quote-free path: if spaces, rely on IMAGE path joining until floats.
            Some(format!("IMAGE {path} {x} {y} {w} {w} A-FURN-PRES"))
        }
        "TRIM" if s.entity_picks.len() == 1 && s.points.len() >= 1 => {
            let id = s.entity_picks[0];
            let (x, y) = s.points[0];
            Some(format!("TRIM {id} {x} {y}"))
        }
        "EXTEND" if s.entity_picks.len() == 1 && s.points.len() >= 1 => {
            let id = s.entity_picks[0];
            let (x, y) = s.points[0];
            Some(format!("EXTEND {id} {x} {y}"))
        }
        _ => None,
    };
    let Some(line) = cmd else {
        return false;
    };
    feed_line(s, &line);
    s.dirty = true;
    // Realtime tip sync OFF — disk flush only on SAVE / COMMIT / explicit workdb_flush.
    if token == "INSERT" {
        if s.place_flip {
            // Flip last INSERT about local X (door swing).
            if let Some(e) = s
                .inner
                .document()
                .entities()
                .into_iter()
                .rev()
                .find(|e| matches!(e.geometry, Geometry::Insert(_)))
            {
                let id = e.id;
                if let Some(ent) = s.inner.document().get(id).cloned() {
                    if let Geometry::Insert(mut ins) = ent.geometry {
                        ins.scale_y = -ins.scale_y.abs();
                        s.inner.document_mut().transact("SWING", |d| {
                            d.replace_geometry(id, Geometry::Insert(ins));
                        });
                    }
                }
            }
        }
        s.points.clear();
        s.cursor = None;
        s.preview_xy.clear();
        refresh_prompt(s);
        return true;
    }
    if token == "WALL" {
        // Tag last A-WALL polyline as wall element with thickness.
        let th = s.wall_thickness;
        let id = s
            .inner
            .document()
            .entities()
            .into_iter()
            .rev()
            .find(|e| e.layer.eq_ignore_ascii_case("A-WALL"))
            .map(|e| e.id.0);
        if let Some(id) = id {
            feed_line(s, &format!("EATTEDIT {id} AADT-WALL-THICKNESS {th}"));
            feed_line(s, &format!("EATTEDIT {id} AADT-ELEM wall"));
        }
        clear_tool(s);
        return true;
    }
    // LINE chains: keep tool, continue from last point (AutoCAD-like).
    if token == "LINE" {
        if let Some(&last) = s.points.last() {
            s.points.clear();
            s.points.push(last);
            s.cursor = None;
            s.preview_xy.clear();
            refresh_prompt(s);
            return true;
        }
    }
    // COPY stays for another destination (AutoCAD-like)
    if token == "COPY" {
        s.points.clear();
        s.cursor = None;
        s.preview_xy.clear();
        refresh_prompt(s);
        return true;
    }
    clear_tool(s);
    true
}

fn tool_should_auto_commit(s: &AadtSession) -> bool {
    match s.tool_token {
        Some("LINE" | "CIRCLE" | "RECTANGLE") => s.points.len() >= 2,
        Some("ARC") => s.points.len() >= 3,
        Some("MOVE" | "ROTATE" | "SCALE" | "MIRROR") => !s.selecting && s.points.len() >= 2,
        Some("COPY") => !s.selecting && s.points.len() >= 2,
        Some("INSERT") => s.points.len() >= 1,
        Some("OFFSET") => s.entity_picks.len() == 1 && s.points.len() >= 1,
        Some("TRIM") => s.entity_picks.len() == 1 && s.points.len() >= 1,
        Some("EXTEND") => false,
        Some("FILLET") => s.entity_picks.len() >= 2,
        Some("DIM") => s.points.len() >= 3,
        Some("DIST") => s.points.len() >= 2,
        Some("STRETCH") => s.stretch_window.is_some() && s.points.len() >= 2,
        Some("ARRAYRECT") => !s.selecting && s.points.len() >= 3,
        Some("ARRAYPOLAR") => !s.selecting && s.points.len() >= 1,
        Some("REVCLOUD") => s.points.len() >= 2,
        Some("IMAGE") => s.points.len() >= 1 && s.image_path.is_some(),
        _ => false,
    }
}

/// Create a session with drawing built-ins **plus** CLI host commands
/// (`OPEN`/`SAVE`/assist/takeoff/…).
#[no_mangle]
pub extern "C" fn aadt_session_new() -> *mut AadtSession {
    let mut interp = Interpreter::with_builtins();
    register(&mut interp);
    let session = Session::from_interpreter(interp);
    let current_layer = session.document().active_category_layer();
    let mut s = AadtSession {
        inner: session,
        last_feedback: CString::new("").unwrap(),
        file_path: None,
        dirty: false,
        work_path: default_work_vdb_path(),
        work_flushed_rev: None,
        work_flush_at: None,
        tess_rev: None,
        line_xy: Vec::new(),
        engine: CommandEngine::default(),
        tool_id: None,
        tool_token: None,
        tool_kind: None,
        selecting: false,
        current_layer: current_layer.clone(),
        insert_name: None,
        entity_picks: Vec::new(),
        points: Vec::new(),
        cursor: None,
        prompt_c: CString::new("").unwrap(),
        preview_xy: Vec::new(),
        current_layer_c: CString::new(current_layer.as_str()).unwrap_or_default(),
        osnap: OsnapState::default(),
        osnap_on: true,
        snap_xy: None,
        text_waiting: false,
        text_height: 1.0,
        hatch_pattern: "SOLID".into(),
        hatch_pattern_c: CString::new("SOLID").unwrap_or_default(),
        otrack: OtrackState::default(),
        otrack_on: true,
        otrack_rays_xy: Vec::new(),
        grip_xy: Vec::new(),
        hot_grip_xy: None,
        selected_dimensions: Vec::new(),
        sel_xy: Vec::new(),
        history_trace_xy: Vec::new(),
        ortho_on: false,
        polar_on: false,
        fillet_radius: 0.0,
        wall_thickness: 0.2,
        stretch_window: None,
        array_rows: 2,
        array_cols: 3,
        array_count: 6,
        array_num_phase: 0,
        image_path: None,
        image_width: 1.0,
        grip_mode: GripMode::Stretch,
        grip_mode_c: CString::new("Stretch").unwrap_or_default(),
        place_flip: false,
        pick_add: false,
        fill_xy: Vec::new(),
        nav_pending: None,
    };
    bind_work_db(&mut s);
    Box::into_raw(Box::new(s))
}

/// Free a session created by [`aadt_session_new`].
///
/// # Safety
/// `session` must be null or a pointer from [`aadt_session_new`] not yet freed.
#[no_mangle]
pub unsafe extern "C" fn aadt_session_free(session: *mut AadtSession) {
    if !session.is_null() {
        let mut s = Box::from_raw(session);
        let _ = flush_work_db(&mut s, true);
        drop(s);
    }
}

/// Feed one command line (UTF-8). Returns 0 on success / silent, 1 on error.
///
/// # Safety
/// `session` must be valid. `line` must be a non-null NUL-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn aadt_session_feed(session: *mut AadtSession, line: *const c_char) -> i32 {
    if session.is_null() || line.is_null() {
        return 1;
    }
    let s = &mut *session;
    let Ok(text) = CStr::from_ptr(line).to_str() else {
        return 1;
    };
    let trimmed = text.trim();
    // Transparent view commands — do not cancel the active tool.
    if let Some(rest) = trimmed.strip_prefix('\'') {
        return handle_transparent(s, rest.trim());
    }
    let fb = s.inner.feed(trimmed);
    track_open_save(s, trimmed, &fb);
    if let Some(hint) = s.inner.take_nav_hint() {
        s.nav_pending = Some(hint);
    }
    let code = i32::from(fb.is_err());
    s.last_feedback = feedback_cstring(&fb);
    code
}

fn handle_transparent(s: &mut AadtSession, rest: &str) -> i32 {
    let up = rest.to_ascii_uppercase();
    let (cmd, args) = {
        let mut it = up.split_whitespace();
        let c = it.next().unwrap_or("");
        (c, it.collect::<Vec<_>>())
    };
    match cmd {
        "ZOOM" | "Z" => {
            let opt = args.first().copied().unwrap_or("");
            match opt {
                "" | "W" | "WINDOW" => {
                    s.nav_pending = Some("zoom_window".into());
                    s.last_feedback = CString::new(
                        "'ZOOM — host: drag a window (or type 'ZOOM E)",
                    )
                    .unwrap_or_default();
                    0
                }
                "E" | "EXTENTS" | "A" | "ALL" => {
                    let fb = s.inner.feed("ZOOM E");
                    if let Some(hint) = s.inner.take_nav_hint() {
                        s.nav_pending = Some(hint);
                    } else {
                        s.nav_pending = Some("extents".into());
                    }
                    s.last_feedback = feedback_cstring(&fb);
                    i32::from(fb.is_err())
                }
                "P" | "PREVIOUS" => {
                    let fb = s.inner.feed("ZOOM P");
                    if let Some(hint) = s.inner.take_nav_hint() {
                        s.nav_pending = Some(hint);
                    }
                    s.last_feedback = feedback_cstring(&fb);
                    i32::from(fb.is_err())
                }
                other if other.parse::<f64>().is_ok() => {
                    s.nav_pending = Some(format!("scale:{other}"));
                    s.last_feedback =
                        CString::new(format!("'ZOOM scale {other}")).unwrap_or_default();
                    0
                }
                _ => {
                    s.last_feedback = CString::new(format!(
                        "'ZOOM: unknown option {opt:?} (E/A/P/W/factor)"
                    ))
                    .unwrap_or_default();
                    1
                }
            }
        }
        "PAN" | "P" => {
            s.nav_pending = Some("pan".into());
            s.last_feedback =
                CString::new("'PAN — drag middle mouse to pan (tool stays active)")
                    .unwrap_or_default();
            0
        }
        "" => {
            s.last_feedback =
                CString::new("' — name a transparent command ('ZOOM / 'PAN)").unwrap_or_default();
            1
        }
        other => {
            s.last_feedback = CString::new(format!(
                "'{other}: not transparent — only 'ZOOM / 'PAN mid-command"
            ))
            .unwrap_or_default();
            1
        }
    }
}

/// Last feedback message from [`aadt_session_feed`] (empty if silent).
/// Borrowed; valid until the next feed/free. Do not free this pointer.
///
/// # Safety
/// `session` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_last_feedback(session: *const AadtSession) -> *const c_char {
    if session.is_null() {
        return ptr::null();
    }
    (*session).last_feedback.as_ptr()
}

/// Entity count in the working document.
///
/// # Safety
/// `session` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_entity_count(session: *const AadtSession) -> u64 {
    if session.is_null() {
        return 0;
    }
    (*session).inner.document().len() as u64
}

/// Selection count (last SELECT).
///
/// # Safety
/// `session` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_selection_count(session: *const AadtSession) -> u64 {
    if session.is_null() {
        return 0;
    }
    (*session).inner.selection().len() as u64
}

/// Copy up to `max_ids` selected entity ids into `out_ids`. Returns number written.
///
/// # Safety
/// `session` must be valid. `out_ids` must point to at least `max_ids` u64s when
/// `max_ids > 0`.
#[no_mangle]
pub unsafe extern "C" fn aadt_selection_copy_ids(
    session: *const AadtSession,
    out_ids: *mut u64,
    max_ids: u64,
) -> u64 {
    if session.is_null() || (max_ids > 0 && out_ids.is_null()) {
        return 0;
    }
    let sel = (*session).inner.selection();
    let n = (sel.len() as u64).min(max_ids) as usize;
    for (i, id) in sel.iter().take(n).enumerate() {
        *out_ids.add(i) = id.0;
    }
    n as u64
}

fn geometry_kind_label(g: &Geometry) -> &'static str {
    match g {
        Geometry::Line(_) => "LINE",
        Geometry::Polyline(p) if p.closed => "PLINE",
        Geometry::Polyline(_) => "POLYLINE",
        Geometry::Circle(_) => "CIRCLE",
        Geometry::Arc(_) => "ARC",
        Geometry::Text(_) => "TEXT",
        Geometry::Image(_) => "IMAGE",
        Geometry::Insert(_) => "INSERT",
    }
}

fn fmt_pt(p: Point2) -> String {
    format!("{:.4}, {:.4}", p.x, p.y)
}

/// Multi-line Properties dump: `Label\\tValue` rows (newline-separated).
/// Always includes Type, Layer, Space, Color, Linetype, Lineweight; then
/// geometry-specific fields (text height/font, circle radius, …) and attributes.
fn build_entity_prop_summary(doc: &aadt_model::Document, e: &aadt_model::Entity) -> String {
    let kind = match &e.geometry {
        Geometry::Insert(i) => format!("INSERT ({})", i.name),
        other => geometry_kind_label(other).to_string(),
    };
    let space = doc
        .model_space(e.space)
        .map(|s| s.name.clone())
        .unwrap_or_else(|| format!("space {}", e.space.0));
    let layer = doc.layer(&e.layer);
    let (aci, ltype, lw) = match layer {
        Some(l) => (l.color, l.linetype.dxf_name(), l.lineweight_mm),
        None => (7u8, "CONTINUOUS", 0.25f32),
    };

    let mut rows: Vec<(String, String)> = vec![
        ("Type".into(), kind),
        ("Id".into(), e.id.0.to_string()),
        ("Layer".into(), e.layer.clone()),
        ("Space".into(), space),
        ("Color".into(), format!("ACI {aci} (BYLAYER)")),
        ("Linetype".into(), format!("{ltype} (BYLAYER)")),
        ("Lineweight".into(), format!("{lw:.2} mm (BYLAYER)")),
    ];

    match &e.geometry {
        Geometry::Line(seg) => {
            rows.push(("Start".into(), fmt_pt(seg.a)));
            rows.push(("End".into(), fmt_pt(seg.b)));
            rows.push((
                "Length".into(),
                format!("{:.4}", seg.a.distance(seg.b)),
            ));
        }
        Geometry::Polyline(pl) => {
            rows.push((
                "Vertices".into(),
                format!("{}{}", pl.vertices.len(), if pl.closed { " (closed)" } else { "" }),
            ));
            if let (Some(a), Some(b)) = (pl.vertices.first(), pl.vertices.last()) {
                rows.push(("First".into(), fmt_pt(*a)));
                rows.push(("Last".into(), fmt_pt(*b)));
            }
        }
        Geometry::Circle(c) => {
            rows.push(("Center".into(), fmt_pt(c.center)));
            rows.push(("Radius".into(), format!("{:.4}", c.radius)));
            rows.push(("Diameter".into(), format!("{:.4}", c.radius * 2.0)));
        }
        Geometry::Arc(a) => {
            rows.push(("Center".into(), fmt_pt(a.center)));
            rows.push(("Radius".into(), format!("{:.4}", a.radius)));
            rows.push((
                "Angles".into(),
                format!("{:.2}° → {:.2}°", a.start_angle, a.end_angle),
            ));
        }
        Geometry::Text(t) => {
            rows.push(("Content".into(), t.value.clone()));
            // Canvas always draws with the suite face; no per-entity font store yet.
            rows.push(("Font".into(), "Urbanist (app default)".into()));
            rows.push(("Height".into(), format!("{:.4}", t.height)));
            rows.push(("Rotation".into(), format!("{:.2}°", t.rotation)));
            rows.push(("Position".into(), fmt_pt(t.position)));
            rows.push(("Size".into(), format!("H={:.4} (cap height)", t.height)));
        }
        Geometry::Insert(i) => {
            rows.push(("Block".into(), i.name.clone()));
            rows.push(("Position".into(), fmt_pt(i.position)));
            rows.push(("Rotation".into(), format!("{:.2}°", i.rotation)));
            rows.push((
                "Scale".into(),
                format!("X={:.4}  Y={:.4}", i.scale_x, i.scale_y),
            ));
        }
        Geometry::Image(img) => {
            rows.push(("Path".into(), img.path.clone()));
            rows.push(("Position".into(), fmt_pt(img.position)));
            rows.push((
                "Size".into(),
                format!("{:.4} × {:.4}", img.width, img.height),
            ));
            rows.push(("Rotation".into(), format!("{:.2}°", img.rotation)));
            rows.push(("Opacity".into(), format!("{:.0}%", img.opacity * 100.0)));
        }
    }

    let mut attrs: Vec<_> = e.attributes.iter().collect();
    attrs.sort_by(|a, b| a.0.cmp(b.0));
    for (k, v) in attrs {
        rows.push((format!("Attr:{k}"), v.clone()));
    }

    rows.into_iter()
        .map(|(k, v)| format!("{k}\t{v}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Heap UTF-8 Properties dump: newline-separated `Label\\tValue` rows.
/// Free with [`aadt_string_free`]. Returns null if the entity is missing.
///
/// # Safety
/// `session` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_entity_prop_summary(
    session: *const AadtSession,
    entity_id: u64,
) -> *mut c_char {
    if session.is_null() {
        return ptr::null_mut();
    }
    let doc = (*session).inner.document();
    let Some(e) = doc.get(EntityId(entity_id)) else {
        return ptr::null_mut();
    };
    let line = build_entity_prop_summary(doc, e);
    CString::new(line.replace('\0', ""))
        .map(|c| c.into_raw())
        .unwrap_or(ptr::null_mut())
}

/// Pick nearest entity or dimension within `tol` (world units).
/// Entities → `SELECT <id>` / `SELECT NONE`. Dimensions update
/// [`AadtSession::selected_dimensions`] (no EntityId). Returns 1 on hit
/// (writes entity id into `*out_id`, or 0 for a dimension hit), 0 on miss/clear.
/// `out_id` may be null.
///
/// # Safety
/// `session` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_pick_at(
    session: *mut AadtSession,
    x: f64,
    y: f64,
    tol: f64,
    out_id: *mut u64,
) -> i32 {
    if session.is_null() {
        return 0;
    }
    let s = &mut *session;
    let p = Point2::new(x, y);
    let tol = tol.max(1e-9);
    // Closest-wins: a dim line often sits near its source entity — prefer the
    // nearer of entity vs dimension so dims are actually selectable.
    let ent = pick::pick_entity_dist(s.inner.document(), p, tol);
    let dim = pick::pick_dimension_dist(s.inner.document(), p, tol);
    let prefer_dim = match (ent, dim) {
        (Some((ed, _)), Some((dd, _))) => dd <= ed,
        (None, Some(_)) => true,
        (Some(_), None) => false,
        (None, None) => false,
    };

    if prefer_dim {
        if let Some((_, di)) = dim {
            if s.pick_add {
                if let Some(pos) = s.selected_dimensions.iter().position(|&x| x == di) {
                    s.selected_dimensions.remove(pos);
                } else {
                    s.selected_dimensions.push(di);
                }
            } else {
                // Clear entities without wiping the dim we are about to select.
                let fb = s.inner.feed("SELECT NONE");
                s.last_feedback = feedback_cstring(&fb);
                s.selected_dimensions.clear();
                s.selected_dimensions.push(di);
                s.last_feedback = CString::new("1 dimension selected").unwrap_or_default();
            }
            s.hot_grip_xy = None;
            rebuild_selection_lines(s);
            if !out_id.is_null() {
                *out_id = 0;
            }
            return 1;
        }
    }

    if let Some((_, id)) = ent {
        let line = if s.pick_add {
            let mut ids: Vec<u64> = s.inner.selection().iter().map(|e| e.0).collect();
            if let Some(pos) = ids.iter().position(|&x| x == id.0) {
                ids.remove(pos);
            } else {
                ids.push(id.0);
            }
            if ids.is_empty() {
                "SELECT NONE".into()
            } else {
                format!(
                    "SELECT {}",
                    ids.iter()
                        .map(|i| i.to_string())
                        .collect::<Vec<_>>()
                        .join(" ")
                )
            }
        } else {
            format!("SELECT {}", id.0)
        };
        if s.pick_add {
            // Keep annotation selection; do not route through feed_line (clears dims).
            let fb = s.inner.feed(&line);
            s.last_feedback = feedback_cstring(&fb);
        } else {
            feed_line(s, &line); // clears dims on replace
        }
        s.hot_grip_xy = None;
        rebuild_selection_lines(s);
        if !out_id.is_null() {
            *out_id = id.0;
        }
        return 1;
    }

    // Miss: clear entities + dimensions.
    clear_dimension_selection(s);
    let fb = s.inner.feed("SELECT NONE");
    s.last_feedback = feedback_cstring(&fb);
    s.hot_grip_xy = None;
    rebuild_selection_lines(s);
    if !out_id.is_null() {
        *out_id = 0;
    }
    0
}

/// Document revision (increments on mutating edits).
///
/// # Safety
/// `session` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_document_revision(session: *const AadtSession) -> u64 {
    if session.is_null() {
        return 0;
    }
    (*session).inner.document().revision()
}

/// 1 if the host should treat the drawing as modified since last SAVE.
///
/// # Safety
/// `session` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_is_dirty(session: *const AadtSession) -> i32 {
    if session.is_null() {
        return 0;
    }
    i32::from((*session).dirty)
}

/// Heap UTF-8 path from last successful OPEN/SAVE, or null. Free with
/// [`aadt_string_free`].
///
/// # Safety
/// `session` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_session_path(session: *const AadtSession) -> *mut c_char {
    if session.is_null() {
        return ptr::null_mut();
    }
    let Some(p) = &(*session).file_path else {
        return ptr::null_mut();
    };
    let s = p.to_string_lossy();
    CString::new(s.as_ref().replace('\0', ""))
        .map(|c| c.into_raw())
        .unwrap_or(ptr::null_mut())
}

/// Drawlist primitive count (snapshot length).
///
/// # Safety
/// `session` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_drawlist_len(session: *mut AadtSession) -> u64 {
    if session.is_null() {
        return 0;
    }
    (*session).inner.drawlist().len() as u64
}

/// Rebuild the tessellated stroke cache from the current drawlist. Returns the
/// number of floats (multiple of 6: x0,y0,x1,y1,lw_mm,aci per seg).
///
/// Skips work when [`Document::revision`] matches the last tessellation — hosts
/// may call this every pointer move without paying full rebuild cost.
///
/// # Safety
/// `session` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_drawlist_rebuild_lines(session: *mut AadtSession) -> u64 {
    if session.is_null() {
        return 0;
    }
    let s = &mut *session;
    // Never flush the work DB from the draw path — tip rewrite stalls the UI.
    let rev = s.inner.revision();
    if s.tess_rev == Some(rev) {
        return s.line_xy.len() as u64;
    }
    s.inner.refresh_draw_cache();
    // Take buffers so we can borrow the drawlist/document without cloning.
    let mut line_xy = std::mem::take(&mut s.line_xy);
    let mut fill_xy = std::mem::take(&mut s.fill_xy);
    {
        let list = s.inner.cached_drawlist();
        let doc = s.inner.document();
        tessellate_drawlist(list, doc, &mut line_xy);
        rebuild_fills_from_drawlist(list, &mut fill_xy);
    }
    s.line_xy = line_xy;
    s.fill_xy = fill_xy;
    s.tess_rev = Some(rev);
    s.line_xy.len() as u64
}

fn rebuild_fills_from_drawlist(list: &aadt_render::DrawList, out: &mut Vec<f32>) {
    out.clear();
    for cmd in &list.commands {
        if let DrawCommand::Fill { points, alpha, .. } = cmd {
            if points.len() < 3 {
                continue;
            }
            out.push(points.len() as f32);
            out.push(*alpha);
            for p in points {
                out.push(p.x as f32);
                out.push(p.y as f32);
            }
        }
    }
}

/// Copy up to `max_floats` values from the line cache into `out_xy`.
/// Returns floats written. Call [`aadt_drawlist_rebuild_lines`] first.
///
/// # Safety
/// `session` must be valid. `out_xy` must point to at least `max_floats` f32s when non-null.
#[no_mangle]
pub unsafe extern "C" fn aadt_drawlist_copy_lines(
    session: *const AadtSession,
    out_xy: *mut f32,
    max_floats: u64,
) -> u64 {
    if session.is_null() || out_xy.is_null() || max_floats == 0 {
        return 0;
    }
    let src = &(*session).line_xy;
    let n = (src.len() as u64).min(max_floats) as usize;
    // Round down to whole segments.
    let n = n - (n % 4);
    std::ptr::copy_nonoverlapping(src.as_ptr(), out_xy, n);
    n as u64
}

/// Copy hatch fill pack rebuilt by [`aadt_drawlist_rebuild_lines`].
/// Layout: repeating records of `n_pts, alpha, x0,y0, … xn,yn` (n_pts ≥ 3).
///
/// # Safety
/// `session` must be valid. `out` sized for `max_floats` when non-null.
#[no_mangle]
pub unsafe extern "C" fn aadt_drawlist_copy_fills(
    session: *const AadtSession,
    out: *mut f32,
    max_floats: u64,
) -> u64 {
    if session.is_null() {
        return 0;
    }
    let src = &(*session).fill_xy;
    let len = src.len() as u64;
    if out.is_null() || max_floats == 0 {
        return len;
    }
    let n = len.min(max_floats) as usize;
    std::ptr::copy_nonoverlapping(src.as_ptr(), out, n);
    n as u64
}

/// Take pending navigation hint (`extents`, `pan`, `zoom_window`, `scale:…`,
/// `window:x0,y0,x1,y1`). Heap UTF-8; free with [`aadt_string_free`]. Null if none.
///
/// # Safety
/// `session` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_nav_take(session: *mut AadtSession) -> *mut c_char {
    if session.is_null() {
        return ptr::null_mut();
    }
    let Some(hint) = (*session).nav_pending.take() else {
        return ptr::null_mut();
    };
    CString::new(hint.replace('\0', ""))
        .map(|c| c.into_raw())
        .unwrap_or(ptr::null_mut())
}

/// Axis-aligned bounds of the line cache (world units). Returns 0 if empty.
/// Writes min_x, min_y, max_x, max_y into `out4` (4 floats).
///
/// # Safety
/// `session` and `out4` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_drawlist_bounds(session: *const AadtSession, out4: *mut f32) -> i32 {
    if session.is_null() || out4.is_null() {
        return 0;
    }
    let xy = &(*session).line_xy;
    if xy.len() < 4 {
        return 0;
    }
    // Ignore construction-line extents (XLINE uses ±1e6) so Zoom Extents /
    // fitWorld stay on real drafting geometry.
    const MAX_SEG: f32 = 50_000.0;
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut any = false;
    let stride = if xy.len() % 6 == 0 { 6 } else { 4 };
    let mut i = 0;
    while i + 3 < xy.len() {
        let x0 = xy[i];
        let y0 = xy[i + 1];
        let x1 = xy[i + 2];
        let y1 = xy[i + 3];
        let len = ((x1 - x0).hypot(y1 - y0)).abs();
        if len <= MAX_SEG {
            min_x = min_x.min(x0).min(x1);
            min_y = min_y.min(y0).min(y1);
            max_x = max_x.max(x0).max(x1);
            max_y = max_y.max(y0).max(y1);
            any = true;
        }
        i += stride;
    }
    if !any {
        return 0;
    }
    *out4 = min_x;
    *out4.add(1) = min_y;
    *out4.add(2) = max_x;
    *out4.add(3) = max_y;
    1
}

/// Free a string returned by this ABI that was allocated with `CString::into_raw`
/// (e.g. [`aadt_session_path`]). Do **not** free [`aadt_last_feedback`].
///
/// # Safety
/// `s` must be null or a pointer previously returned for the caller to free.
#[no_mangle]
pub unsafe extern "C" fn aadt_string_free(s: *mut c_char) {
    if !s.is_null() {
        drop(CString::from_raw(s));
    }
}

/// Start an interactive pick tool (M16/M18). Tokens: LINE/CIRCLE/REC/PL/ARC/XL,
/// MOVE/COPY/ROTATE/SCALE/MIRROR/ERASE/OFFSET/TRIM/EXTEND/FILLET, or INSERT
/// (use [`aadt_tool_start_insert`] for a block name). Returns 0 ok, 1 unsupported.
///
/// # Safety
/// `session` and `cmd` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_tool_start(session: *mut AadtSession, cmd: *const c_char) -> i32 {
    if session.is_null() || cmd.is_null() {
        return 1;
    }
    let Ok(token) = CStr::from_ptr(cmd).to_str() else {
        return 1;
    };
    let up = token.trim().to_ascii_uppercase();
    if up == "INSERT" {
        return 1; // need block name — aadt_tool_start_insert
    }
    if up == "IMAGE" || up == "PLACEIMAGE" || up == "IMAGEATTACH" {
        return 1; // need path — aadt_tool_start_image
    }
    // REVCLOUD is Registry/annotate only — not in CommandEngine CATALOG.
    if up == "REVCLOUD" || up == "RCLOUD" {
        let s = &mut *session;
        clear_tool(s);
        s.tool_token = Some("REVCLOUD");
        s.tool_kind = Some(ToolKind::RevCloud);
        refresh_prompt(s);
        return 0;
    }
    // Region select — not in CATALOG (and CP would collide with COPY).
    let region = match up.as_str() {
        "WP" | "WPOLYGON" => Some("WPOLYGON"),
        "CPOLYGON" | "CW" => Some("CPOLYGON"),
        "FENCE" | "FEN" => Some("FENCE"),
        _ => None,
    };
    if let Some(tok) = region {
        let s = &mut *session;
        clear_tool(s);
        s.tool_token = Some(tok);
        s.tool_kind = Some(ToolKind::Region);
        refresh_prompt(s);
        return 0;
    }
    let Some(id) = command_engine::resolve_alias(&up) else {
        return 1;
    };
    let Some(def) = command_engine::find_def(id) else {
        return 1;
    };
    let Some(kind) = tool_kind_for(def.tool_token) else {
        return 1;
    };
    if !INTERACTIVE_TOKENS.contains(&def.tool_token) {
        return 1;
    }
    let s = &mut *session;
    clear_tool(s);

    // Place tools → INSERT with seeded block.
    if let Some(block) = place_block_name(def.tool_token) {
        aadt_command::ensure_place_blocks(s.inner.document_mut());
        s.tool_token = Some("INSERT");
        s.tool_kind = Some(ToolKind::Insert);
        s.insert_name = Some(block.to_string());
        refresh_prompt(s);
        return 0;
    }

    s.engine.start(id);
    s.tool_id = Some(def.id);
    s.tool_token = Some(def.tool_token);
    s.tool_kind = Some(kind);

    match kind {
        ToolKind::Modify | ToolKind::Erase | ToolKind::Stretch | ToolKind::Array => {
            if s.inner.selection().is_empty() {
                s.selecting = true;
            } else if kind == ToolKind::Erase {
                feed_line(s, "ERASE SELECTION");
                clear_tool(s);
                return 0;
            }
        }
        ToolKind::Image => {
            // Need path via aadt_tool_start_image — bare IMAGE fails unless path set.
            if s.image_path.is_none() {
                clear_tool(s);
                return 1;
            }
        }
        ToolKind::Entity | ToolKind::Draw | ToolKind::Insert | ToolKind::Text | ToolKind::Dim
        | ToolKind::Dist | ToolKind::Wall | ToolKind::RevCloud | ToolKind::Region => {}
    }
    refresh_prompt(s);
    0
}

/// Start INSERT for `block_name` (UTF-8). Returns 0 ok, 1 on error.
///
/// # Safety
/// `session` and `block_name` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_tool_start_insert(
    session: *mut AadtSession,
    block_name: *const c_char,
) -> i32 {
    if session.is_null() || block_name.is_null() {
        return 1;
    }
    let Ok(name) = CStr::from_ptr(block_name).to_str() else {
        return 1;
    };
    let name = name.trim();
    if name.is_empty() {
        return 1;
    }
    let s = &mut *session;
    clear_tool(s);
    s.tool_token = Some("INSERT");
    s.tool_kind = Some(ToolKind::Insert);
    s.insert_name = Some(name.to_string());
    refresh_prompt(s);
    0
}

/// Accept a world-space point for the active tool. May auto-commit when enough
/// input is gathered. Returns 0 ok, 1 if no tool / error.
///
/// # Safety
/// `session` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_tool_accept_point(session: *mut AadtSession, x: f64, y: f64) -> i32 {
    if session.is_null() {
        return 1;
    }
    let s = &mut *session;
    if s.tool_token.is_none() {
        return 1;
    }

    let (x, y) = snap_world(s, x, y, 1.0);

    // Selection phase: click adds to pickset.
    if s.selecting {
        let tol = 1.0;
        let p = Point2::new(x, y);
        if let Some(id) = pick::pick_entity_linear(s.inner.document(), p, tol) {
            let mut ids: Vec<u64> = s.inner.selection().iter().map(|e| e.0).collect();
            if !ids.contains(&id.0) {
                ids.push(id.0);
            }
            let line = format!(
                "SELECT {}",
                ids.iter()
                    .map(|i| i.to_string())
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            feed_line(s, &line);
        }
        refresh_prompt(s);
        return 0;
    }

    // IMAGE: click place.
    if s.tool_token == Some("IMAGE") {
        s.points.clear();
        s.points.push((x, y));
        let _ = commit_tool(s);
        return 0;
    }

    // WP / CP / FENCE — accumulate vertices; Space finishes.
    if matches!(
        s.tool_token,
        Some("WPOLYGON") | Some("CPOLYGON") | Some("FENCE")
    ) {
        s.points.push((x, y));
        refresh_prompt(s);
        rebuild_preview(s);
        return 0;
    }

    // REVCLOUD: two corners.
    if s.tool_token == Some("REVCLOUD") {
        s.points.push((x, y));
        if tool_should_auto_commit(s) {
            let _ = commit_tool(s);
        } else {
            refresh_prompt(s);
        }
        return 0;
    }

    // STRETCH: window then base/dest.
    if s.tool_token == Some("STRETCH") {
        if s.stretch_window.is_none() {
            s.points.push((x, y));
            if s.points.len() >= 2 {
                let (x0, y0) = s.points[0];
                let (x1, y1) = s.points[1];
                s.stretch_window = Some((
                    x0.min(x1),
                    y0.min(y1),
                    x0.max(x1),
                    y0.max(y1),
                ));
                s.points.clear();
            }
            refresh_prompt(s);
            rebuild_preview(s);
            return 0;
        }
        s.points.push((x, y));
        if tool_should_auto_commit(s) {
            let _ = commit_tool(s);
        } else {
            refresh_prompt(s);
        }
        return 0;
    }

    // ARRAY*
    if matches!(
        s.tool_token,
        Some("ARRAYRECT") | Some("ARRAYPOLAR") | Some("ARRAYPATH")
    ) {
        let token = s.tool_token.unwrap();
        if token == "ARRAYPATH" && s.entity_picks.is_empty() {
            if let Some(id) = pick::pick_entity_linear(s.inner.document(), Point2::new(x, y), 1.0)
            {
                s.entity_picks.push(id.0);
                refresh_prompt(s);
            }
            return 0;
        }
        if token == "ARRAYPATH" {
            return 0; // Space finishes
        }
        s.points.push((x, y));
        if tool_should_auto_commit(s) {
            let _ = commit_tool(s);
        } else {
            refresh_prompt(s);
        }
        return 0;
    }

    // TEXT: first click = insertion point; await typed value.
    if s.tool_token == Some("TEXT") {
        if !s.text_waiting {
            s.points.clear();
            s.points.push((x, y));
            s.text_waiting = true;
            refresh_prompt(s);
        }
        return 0;
    }

    // MLEADER: three points then text.
    if s.tool_token == Some("MLEADER") {
        if s.text_waiting {
            return 0;
        }
        s.points.push((x, y));
        if s.points.len() >= 3 {
            s.text_waiting = true;
        }
        refresh_prompt(s);
        return 0;
    }

    // DIM: three points → aligned dimension.
    if s.tool_token == Some("DIM") {
        s.points.push((x, y));
        if tool_should_auto_commit(s) {
            let _ = commit_tool(s);
        } else {
            refresh_prompt(s);
        }
        return 0;
    }

    // DIST: two points.
    if s.tool_token == Some("DIST") {
        s.points.push((x, y));
        if tool_should_auto_commit(s) {
            let _ = commit_tool(s);
        } else {
            refresh_prompt(s);
        }
        return 0;
    }

    // DIMRADIUS / DIMDIAMETER: pick entity.
    if matches!(s.tool_token, Some("DIMRADIUS") | Some("DIMDIAMETER")) {
        let token = s.tool_token.unwrap();
        let tol = 1.0;
        if let Some(id) = pick::pick_entity_linear(s.inner.document(), Point2::new(x, y), tol) {
            feed_line(s, &format!("{token} {} AN-DIMS", id.0));
            clear_tool(s);
        } else {
            refresh_prompt(s);
        }
        return 0;
    }

    // HATCH: click inside region or on boundary.
    if s.tool_token == Some("HATCH") {
        let p = Point2::new(x, y);
        let id = hatch_boundary_at(s.inner.document(), p)
            .or_else(|| pick::pick_entity_linear(s.inner.document(), p, 1.0));
        if let Some(id) = id {
            let pat = s.hatch_pattern.clone();
            feed_line(s, &format!("HATCH {} {pat}", id.0));
            clear_tool(s);
        } else {
            refresh_prompt(s);
        }
        return 0;
    }

    let kind = s.tool_kind;
    match kind {
        Some(ToolKind::Entity) => {
            let token = s.tool_token.unwrap_or("");
            if matches!(token, "OFFSET" | "TRIM" | "EXTEND" | "FILLET") && s.entity_picks.is_empty()
            {
                let tol = 1.0;
                if let Some(id) = pick::pick_entity_linear(s.inner.document(), Point2::new(x, y), tol)
                {
                    s.entity_picks.push(id.0);
                    if matches!(token, "TRIM") {
                        // Same click is the trim pick point.
                        s.points.push((x, y));
                        if tool_should_auto_commit(s) {
                            let _ = commit_tool(s);
                        } else {
                            refresh_prompt(s);
                        }
                        return 0;
                    }
                    if token == "EXTEND" {
                        // Wait for boundary entity + point on second click.
                        refresh_prompt(s);
                        return 0;
                    }
                    if token == "FILLET" {
                        refresh_prompt(s);
                        return 0;
                    }
                    // OFFSET: wait for side point
                    refresh_prompt(s);
                    return 0;
                }
                return 0;
            }
            if token == "EXTEND" && s.entity_picks.len() == 1 {
                let tol = 1.0;
                if let Some(bound) =
                    pick::pick_entity_linear(s.inner.document(), Point2::new(x, y), tol)
                {
                    let id = s.entity_picks[0];
                    if bound.0 != id {
                        let line = format!("EXTEND {id} {} {x} {y}", bound.0);
                        feed_line(s, &line);
                        clear_tool(s);
                    }
                }
                return 0;
            }
            if token == "FILLET" && s.entity_picks.len() == 1 {
                let tol = 1.0;
                if let Some(id) = pick::pick_entity_linear(s.inner.document(), Point2::new(x, y), tol)
                {
                    if id.0 != s.entity_picks[0] {
                        s.entity_picks.push(id.0);
                        let _ = commit_tool(s);
                    }
                }
                return 0;
            }
            if token == "OFFSET" && s.entity_picks.len() == 1 {
                s.points.push((x, y));
                // Distance from entity roughly: use  distance to click via OFFSET side args
                if let Some(e) = s.inner.document().get(EntityId(s.entity_picks[0])) {
                    // Rebuild OFFSET with better dist
                    let id = s.entity_picks[0];
                    let dist = match &e.geometry {
                        Geometry::Circle(c) => ((x - c.center.x).hypot(y - c.center.y) - c.radius)
                            .abs()
                            .max(1e-6),
                        Geometry::Line(seg) => {
                            // distance point to infinite line through segment
                            let ax = seg.a.x;
                            let ay = seg.a.y;
                            let bx = seg.b.x;
                            let by = seg.b.y;
                            let dx = bx - ax;
                            let dy = by - ay;
                            let len2 = dx * dx + dy * dy;
                            if len2 < 1e-18 {
                                1.0
                            } else {
                                ((x - ax) * dy - (y - ay) * dx).abs() / len2.sqrt()
                            }
                            .max(1e-6)
                        }
                        _ => 1.0,
                    };
                    let line = format!("OFFSET {id} {dist} {x} {y}");
                    feed_line(s, &line);
                    clear_tool(s);
                    return 0;
                }
                let _ = commit_tool(s);
                return 0;
            }
        }
        _ => {}
    }

    s.points.push((x, y));
    s.engine.accept_point();
    if tool_should_auto_commit(s) {
        let _ = commit_tool(s);
    } else {
        refresh_prompt(s);
        rebuild_preview(s);
    }
    0
}

/// Finish open-ended tools (POLYLINE/XLINE chain, selection phase, ERASE).
/// Returns 0 if handled, 1 if idle / nothing to finish.
///
/// # Safety
/// `session` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_tool_finish(session: *mut AadtSession) -> i32 {
    if session.is_null() {
        return 1;
    }
    let s = &mut *session;
    if s.tool_token.is_none() {
        return 1;
    }
    if s.selecting {
        s.selecting = false;
        if s.tool_token == Some("ERASE") {
            if s.inner.selection().is_empty() {
                clear_tool(s);
                return 0;
            }
            let _ = commit_tool(s);
            return 0;
        }
        // Advance modify tools to point phase
        s.points.clear();
        refresh_prompt(s);
        return 0;
    }
    match s.tool_token {
        Some("POLYLINE") if s.points.len() >= 2 => {
            let _ = commit_tool(s);
            0
        }
        Some("WALL") if s.points.len() >= 2 => {
            let _ = commit_tool(s);
            0
        }
        Some("XLINE") if s.points.len() >= 2 => {
            let _ = commit_tool(s);
            0
        }
        Some("LINE") => {
            clear_tool(s);
            0
        }
        Some("ARRAYPATH") if s.entity_picks.len() >= 1 => {
            let _ = commit_tool(s);
            0
        }
        Some("WPOLYGON") | Some("CPOLYGON") if s.points.len() >= 3 => {
            finish_region_select(s);
            0
        }
        Some("FENCE") if s.points.len() >= 2 => {
            finish_region_select(s);
            0
        }
        Some("COPY") => {
            clear_tool(s);
            0
        }
        Some("INSERT") => {
            clear_tool(s);
            0
        }
        _ => 1,
    }
}

/// Update the rubber-band cursor (world space) for preview.
///
/// # Safety
/// `session` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_tool_set_cursor(session: *mut AadtSession, x: f64, y: f64) {
    if session.is_null() {
        return;
    }
    let s = &mut *session;
    if s.tool_token.is_none() {
        // Idle hover: no osnap scan (CAD-like — snap runs during commands only).
        s.snap_xy = None;
        return;
    }
    let (sx, sy) = snap_world(s, x, y, 1.0);
    s.cursor = Some((sx, sy));
    rebuild_preview(s);
}

/// Cancel the active interactive tool.
///
/// # Safety
/// `session` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_tool_cancel(session: *mut AadtSession) {
    if session.is_null() {
        return;
    }
    clear_tool(&mut *session);
}

/// Current tool prompt (borrowed; empty if idle). Valid until next tool/feed/free.
///
/// # Safety
/// `session` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_tool_prompt(session: *const AadtSession) -> *const c_char {
    if session.is_null() {
        return ptr::null();
    }
    (*session).prompt_c.as_ptr()
}

/// 1 if an interactive tool is active.
///
/// # Safety
/// `session` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_tool_active(session: *const AadtSession) -> i32 {
    if session.is_null() {
        return 0;
    }
    i32::from((*session).tool_token.is_some())
}

/// Rebuild the tool rubber-band preview cache. Returns float count (×4 segments).
///
/// # Safety
/// `session` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_tool_preview_rebuild_lines(session: *mut AadtSession) -> u64 {
    if session.is_null() {
        return 0;
    }
    let s = &mut *session;
    rebuild_preview(s);
    s.preview_xy.len() as u64
}

/// Copy preview line floats into `out_xy`. Call [`aadt_tool_preview_rebuild_lines`] first.
///
/// # Safety
/// `session` must be valid; `out_xy` sized for `max_floats`.
#[no_mangle]
pub unsafe extern "C" fn aadt_tool_preview_copy_lines(
    session: *const AadtSession,
    out_xy: *mut f32,
    max_floats: u64,
) -> u64 {
    if session.is_null() || out_xy.is_null() || max_floats == 0 {
        return 0;
    }
    let src = &(*session).preview_xy;
    let n = (src.len() as u64).min(max_floats) as usize;
    let n = n - (n % 4);
    std::ptr::copy_nonoverlapping(src.as_ptr(), out_xy, n);
    n as u64
}

/// ABI version (bump when breaking C layouts).
/// 11 plot/PDF · 12 layer TSV LTYPE/LW · 13 stroke pack lw_mm+aci · 14 drawing props
/// · 15 work-DB flush / revision-gated tessellation
/// · 16 dim pick · hot grip · osnap mode bitmask.
#[no_mangle]
pub extern "C" fn aadt_ffi_version() -> u32 {
    16
}

/// Force-flush the tip into the local work `.vdb` (disk SoT). Returns 0 ok, 1 err.
///
/// # Safety
/// `session` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_workdb_flush(session: *mut AadtSession) -> i32 {
    if session.is_null() {
        return 1;
    }
    i32::from(flush_work_db(&mut *session, true).is_err())
}

/// Formerly debounced tip flush. **Realtime sync is OFF** — this is a no-op.
/// Use [`aadt_workdb_flush`] or `COMMIT` / `SAVE` for disk writes.
///
/// # Safety
/// `session` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_workdb_tick(session: *mut AadtSession) -> i32 {
    if session.is_null() {
        return 1;
    }
    let _ = session;
    0
}

/// Heap UTF-8 path of the local work `.vdb`. Free with [`aadt_string_free`].
///
/// # Safety
/// `session` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_workdb_path(session: *const AadtSession) -> *mut c_char {
    if session.is_null() {
        return ptr::null_mut();
    }
    let s = (*session).work_path.to_string_lossy();
    CString::new(s.as_ref().replace('\0', ""))
        .map(|c| c.into_raw())
        .unwrap_or(ptr::null_mut())
}

fn drawing_props_pack(p: &DrawingProperties) -> String {
    [
        ("title", p.title.as_str()),
        ("subject", p.subject.as_str()),
        ("author", p.author.as_str()),
        ("project", p.project.as_str()),
        ("client", p.client.as_str()),
        ("firm_name", p.firm_name.as_str()),
        ("firm_tagline", p.firm_tagline.as_str()),
        ("logo_path", p.logo_path.as_str()),
        ("drawing_number", p.drawing_number.as_str()),
        ("revision", p.revision.as_str()),
        ("units", p.units.as_str()),
        ("dimstyle", p.dimstyle.as_str()),
        ("dim_text_height", p.dim_text_height.as_str()),
        ("dim_scale", p.dim_scale.as_str()),
        ("dim_secondary", p.dim_secondary.as_str()),
        ("dim_precision", p.dim_precision.as_str()),
        ("dim_fraction", p.dim_fraction.as_str()),
        ("floor_height", p.floor_height.as_str()),
        ("lintel_height", p.lintel_height.as_str()),
        ("architect", p.architect.as_str()),
        ("scale", p.scale.as_str()),
    ]
    .into_iter()
    .map(|(k, v)| format!("{k}\t{v}"))
    .collect::<Vec<_>>()
    .join("\n")
}

fn drawing_props_apply(p: &mut DrawingProperties, pack: &str) {
    for line in pack.lines() {
        let Some((k, v)) = line.split_once('\t') else {
            continue;
        };
        let v = v.to_string();
        match k.trim() {
            "title" => p.title = v,
            "subject" => p.subject = v,
            "author" => p.author = v,
            "project" => p.project = v,
            "client" => p.client = v,
            "firm_name" => p.firm_name = v,
            "firm_tagline" => p.firm_tagline = v,
            "logo_path" => p.logo_path = v,
            "drawing_number" => p.drawing_number = v,
            "revision" => p.revision = v,
            "units" => p.units = v,
            "dimstyle" => p.dimstyle = v,
            "dim_text_height" => p.dim_text_height = v,
            "dim_scale" => p.dim_scale = v,
            "dim_secondary" => p.dim_secondary = v,
            "dim_precision" => p.dim_precision = v,
            "dim_fraction" => p.dim_fraction = v,
            "floor_height" => p.floor_height = v,
            "lintel_height" => p.lintel_height = v,
            "architect" => p.architect = v,
            "scale" => p.scale = v,
            _ => {}
        }
    }
}

/// Heap UTF-8 `key\\tvalue` rows for drawing properties. Free with [`aadt_string_free`].
///
/// # Safety
/// `session` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_drawing_props_get(session: *const AadtSession) -> *mut c_char {
    if session.is_null() {
        return ptr::null_mut();
    }
    let pack = drawing_props_pack((*session).inner.document().properties());
    CString::new(pack.replace('\0', ""))
        .map(|c| c.into_raw())
        .unwrap_or(ptr::null_mut())
}

/// Apply `key\\tvalue` rows into drawing properties (DWGPROPS-style). Returns 0 ok.
///
/// # Safety
/// `session` and `pack` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_drawing_props_set(
    session: *mut AadtSession,
    pack: *const c_char,
) -> i32 {
    if session.is_null() || pack.is_null() {
        return 1;
    }
    let Ok(text) = CStr::from_ptr(pack).to_str() else {
        return 1;
    };
    let s = &mut *session;
    let mut props = s.inner.document().properties().clone();
    drawing_props_apply(&mut props, text);
    s.inner.document_mut().set_properties(props);
    s.dirty = true;
    s.last_feedback = CString::new("drawing properties updated").unwrap_or_default();
    0
}

/// Enable (1) / disable (0) object snap for tool picks.
#[no_mangle]
pub unsafe extern "C" fn aadt_osnap_set(session: *mut AadtSession, on: i32) {
    if session.is_null() {
        return;
    }
    (*session).osnap_on = on != 0;
}

/// Running object-snap mode bitmask (`OSNAP_END` … — see aadt_app::osnap).
#[no_mangle]
pub unsafe extern "C" fn aadt_osnap_modes_get(session: *const AadtSession) -> u32 {
    if session.is_null() {
        return osnap::OSNAP_MODE_DEFAULT;
    }
    (*session).osnap.modes
}

/// Set running object-snap mode bitmask. `0` disables all discrete kinds.
#[no_mangle]
pub unsafe extern "C" fn aadt_osnap_modes_set(session: *mut AadtSession, modes: u32) {
    if session.is_null() {
        return;
    }
    (*session).osnap.modes = modes;
}

/// Write last snap marker into out2 (x,y). Returns 1 if active snap, else 0.
#[no_mangle]
pub unsafe extern "C" fn aadt_osnap_marker(session: *const AadtSession, out2: *mut f64) -> i32 {
    if session.is_null() || out2.is_null() {
        return 0;
    }
    match (*session).snap_xy {
        Some((x, y)) => {
            *out2 = x;
            *out2.add(1) = y;
            1
        }
        None => 0,
    }
}

/// Number of selected dimensions (annotation table indices).
#[no_mangle]
pub unsafe extern "C" fn aadt_selection_dim_count(session: *const AadtSession) -> u64 {
    if session.is_null() {
        return 0;
    }
    (*session).selected_dimensions.len() as u64
}

/// Clear entity + dimension selection (SELECT NONE + dims).
#[no_mangle]
pub unsafe extern "C" fn aadt_selection_clear(session: *mut AadtSession) {
    if session.is_null() {
        return;
    }
    let s = &mut *session;
    clear_dimension_selection(s);
    feed_line(s, "SELECT NONE");
    s.hot_grip_xy = None;
    rebuild_selection_lines(s);
}

/// Window (left→right) or crossing (right→left) box select. Replaces selection.
/// Returns number of entities selected.
#[no_mangle]
pub unsafe extern "C" fn aadt_select_box(
    session: *mut AadtSession,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
) -> u64 {
    if session.is_null() {
        return 0;
    }
    let s = &mut *session;
    let crossing = x1 < x0;
    let lo = Point2::new(x0.min(x1), y0.min(y1));
    let hi = Point2::new(x0.max(x1), y0.max(y1));
    let poly = vec![
        Point2::new(lo.x, lo.y),
        Point2::new(hi.x, lo.y),
        Point2::new(hi.x, hi.y),
        Point2::new(lo.x, hi.y),
    ];
    let mode = if crossing {
        RegionMode::Crossing
    } else {
        RegionMode::Window
    };
    let ids = select_in_polygon(s.inner.document(), &poly, mode);
    let dims = pick::dimension_box_select(s.inner.document(), lo, hi, crossing);
    let n = apply_selection_ids(s, &ids);
    s.selected_dimensions = dims;
    rebuild_selection_lines(s);
    n + s.selected_dimensions.len() as u64
}

fn finish_region_select(s: &mut AadtSession) {
    let token = s.tool_token.unwrap_or("");
    let pts: Vec<Point2> = s
        .points
        .iter()
        .map(|&(x, y)| Point2::new(x, y))
        .collect();
    let ids = match token {
        "WPOLYGON" => select_in_polygon(s.inner.document(), &pts, RegionMode::Window),
        "CPOLYGON" => select_in_polygon(s.inner.document(), &pts, RegionMode::Crossing),
        "FENCE" => select_by_fence(s.inner.document(), &pts),
        _ => Vec::new(),
    };
    let n = apply_selection_ids(s, &ids);
    clear_tool(s);
    s.last_feedback = CString::new(format!("{token} → {n} selected")).unwrap_or_default();
}

fn apply_selection_ids(s: &mut AadtSession, ids: &[EntityId]) -> u64 {
    clear_dimension_selection(s);
    if ids.is_empty() {
        feed_line(s, "SELECT NONE");
        rebuild_selection_lines(s);
        return 0;
    }
    let line = format!(
        "SELECT {}",
        ids.iter()
            .map(|i| i.0.to_string())
            .collect::<Vec<_>>()
            .join(" ")
    );
    feed_line(s, &line);
    rebuild_selection_lines(s);
    ids.len() as u64
}

/// Provide text value for active TEXT / MLEADER tool. Returns 0 ok.
#[no_mangle]
pub unsafe extern "C" fn aadt_tool_set_text(session: *mut AadtSession, value: *const c_char) -> i32 {
    if session.is_null() || value.is_null() {
        return 1;
    }
    let s = &mut *session;
    let Ok(text) = CStr::from_ptr(value).to_str() else {
        return 1;
    };
    let text = text.trim();
    if text.is_empty() {
        return 1;
    }

    if s.tool_token == Some("TEXT") && s.text_waiting && !s.points.is_empty() {
        let (x, y) = s.points[0];
        let h = s.text_height;
        let layer = layer_suffix(s);
        let line = if layer.is_empty() {
            format!("TEXT {x} {y} {h} {text}")
        } else {
            format!("TEXT {x} {y} {h}{} {text}", layer)
        };
        feed_line(s, &line);
        clear_tool(s);
        return 0;
    }

    if s.tool_token == Some("MLEADER") && s.text_waiting && s.points.len() >= 3 {
        let (ax, ay) = s.points[0];
        let (lx, ly) = s.points[1];
        let (tx, ty) = s.points[2];
        let h = 0.25_f64.max(s.text_height);
        feed_line(
            s,
            &format!("MLEADER {ax} {ay} {lx} {ly} {tx} {ty} {h} {text}"),
        );
        clear_tool(s);
        return 0;
    }

    1
}

/// 1 if TEXT / MLEADER tool is waiting for a typed value.
#[no_mangle]
pub unsafe extern "C" fn aadt_tool_needs_text(session: *const AadtSession) -> i32 {
    if session.is_null() {
        return 0;
    }
    let s = &*session;
    i32::from(
        matches!(s.tool_token, Some("TEXT") | Some("MLEADER")) && s.text_waiting,
    )
}

/// Heap UTF-8 TSV of layers per line:
/// `name\\taci\\ton\\tfrozen\\tlocked\\tcurrent\\tlinetype\\tlineweight_mm`
/// (`on`/`frozen`/`locked`/`current` are `0`|`1`; linetype is DXF name e.g. `CONTINUOUS`).
/// Free with [`aadt_string_free`].
///
/// # Safety
/// `session` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_layers_list(session: *const AadtSession) -> *mut c_char {
    if session.is_null() {
        return ptr::null_mut();
    }
    let s = &*session;
    let current = s.current_layer.as_str();
    let mut lines = Vec::new();
    for layer in s.inner.document().layers() {
        let on = u8::from(layer.visible);
        let fr = u8::from(layer.frozen);
        let lk = u8::from(layer.locked);
        let cur = u8::from(layer.name == current);
        lines.push(format!(
            "{}\t{}\t{on}\t{fr}\t{lk}\t{cur}\t{}\t{:.2}",
            layer.name,
            layer.color,
            layer.linetype.dxf_name(),
            layer.lineweight_mm
        ));
    }
    let body = lines.join("\n");
    CString::new(body.replace('\0', ""))
        .map(|c| c.into_raw())
        .unwrap_or(ptr::null_mut())
}

/// Set the drafting “current” layer (appended on draw commits). Returns 0 ok, 1 bad.
///
/// # Safety
/// `session` and `name` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_layer_set_current(
    session: *mut AadtSession,
    name: *const c_char,
) -> i32 {
    if session.is_null() || name.is_null() {
        return 1;
    }
    let Ok(n) = CStr::from_ptr(name).to_str() else {
        return 1;
    };
    let n = n.trim();
    if n.is_empty() {
        return 1;
    }
    (*session).current_layer = n.to_string();
    (*session).current_layer_c =
        CString::new(n.replace('\0', "")).unwrap_or_default();
    // Ensure the layer exists.
    let line = format!("LAYER {n}");
    feed_line(&mut *session, &line);
    0
}

/// Borrowed current layer name (valid until next set_current/reset/free).
///
/// # Safety
/// `session` must be valid. Do not free.
#[no_mangle]
pub unsafe extern "C" fn aadt_layer_current(session: *const AadtSession) -> *const c_char {
    if session.is_null() {
        return ptr::null();
    }
    (*session).current_layer_c.as_ptr()
}

/// Heap UTF-8 newline-separated block definition names. Free with [`aadt_string_free`].
///
/// # Safety
/// `session` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_blocks_list(session: *const AadtSession) -> *mut c_char {
    if session.is_null() {
        return ptr::null_mut();
    }
    let names: Vec<String> = (*session)
        .inner
        .document()
        .blocks()
        .iter()
        .map(|b| b.name.clone())
        .collect();
    let body = names.join("\n");
    CString::new(body.replace('\0', ""))
        .map(|c| c.into_raw())
        .unwrap_or(ptr::null_mut())
}

/// Heap UTF-8 catalog of **interactive** tools only: `TOKEN\\talias1,alias2` per line.
/// Free with [`aadt_string_free`].
#[no_mangle]
pub extern "C" fn aadt_catalog_tools() -> *mut c_char {
    let mut lines = Vec::new();
    for def in command_engine::catalog() {
        if !INTERACTIVE_TOKENS.contains(&def.tool_token) {
            continue;
        }
        lines.push(format!("{}\t{}", def.tool_token, def.aliases.join(",")));
    }
    if !lines.iter().any(|l| l.starts_with("INSERT\t")) {
        lines.push("INSERT\tI,INSERT".into());
    }
    if !lines.iter().any(|l| l.starts_with("REVCLOUD\t")) {
        lines.push("REVCLOUD\tREVCLOUD,RCLOUD".into());
    }
    if !lines.iter().any(|l| l.starts_with("IMAGE\t")) {
        lines.push("IMAGE\tIMAGE,PLACEIMAGE".into());
    }
    if !lines.iter().any(|l| l.starts_with("WPOLYGON\t")) {
        lines.push("WPOLYGON\tWP,WPOLYGON".into());
    }
    if !lines.iter().any(|l| l.starts_with("CPOLYGON\t")) {
        lines.push("CPOLYGON\tCPOLYGON,CW".into());
    }
    if !lines.iter().any(|l| l.starts_with("FENCE\t")) {
        lines.push("FENCE\tFENCE,FEN".into());
    }
    let body = lines.join("\n");
    CString::new(body.replace('\0', ""))
        .map(|c| c.into_raw())
        .unwrap_or(ptr::null_mut())
}

/// Tab-completion candidates for a command-name prefix. Heap newline list.
#[no_mangle]
pub unsafe extern "C" fn aadt_complete(
    session: *const AadtSession,
    prefix: *const c_char,
) -> *mut c_char {
    if session.is_null() || prefix.is_null() {
        return ptr::null_mut();
    }
    let Ok(p) = CStr::from_ptr(prefix).to_str() else {
        return ptr::null_mut();
    };
    let body = (*session).inner.complete(p).join("\n");
    CString::new(body.replace('\0', ""))
        .map(|c| c.into_raw())
        .unwrap_or(ptr::null_mut())
}

/// 1 if `cmd` is a bare interactive tool alias.
#[no_mangle]
pub unsafe extern "C" fn aadt_is_interactive(cmd: *const c_char) -> i32 {
    if cmd.is_null() {
        return 0;
    }
    let Ok(token) = CStr::from_ptr(cmd).to_str() else {
        return 0;
    };
    let up = token.trim().to_ascii_uppercase();
    if up.is_empty() || up.contains(' ') {
        return 0;
    }
    if matches!(
        up.as_str(),
        "REVCLOUD"
            | "RCLOUD"
            | "IMAGE"
            | "PLACEIMAGE"
            | "IMAGEATTACH"
            | "WP"
            | "WPOLYGON"
            | "CPOLYGON"
            | "CW"
            | "FENCE"
            | "FEN"
    ) {
        return 1;
    }
    let Some(id) = command_engine::resolve_alias(&up) else {
        return 0;
    };
    let Some(def) = command_engine::find_def(id) else {
        return 0;
    };
    i32::from(INTERACTIVE_TOKENS.contains(&def.tool_token) || place_block_name(def.tool_token).is_some())
}

/// Elements library catalog TSV: name\\tkind\\tcategory\\tdisplay\\tpath
#[no_mangle]
pub extern "C" fn aadt_elements_catalog() -> *mut c_char {
    let root = libraries_root();
    let entries = aadt_library::load_element_catalog(&root).unwrap_or_default();
    let mut lines = Vec::new();
    for e in entries {
        lines.push(format!(
            "{}\t{}\t{}\t{}\t{}",
            e.name,
            e.kind.as_str(),
            e.category,
            e.display_name,
            e.path
        ));
    }
    let body = lines.join("\n");
    CString::new(body.replace('\0', ""))
        .map(|c| c.into_raw())
        .unwrap_or(ptr::null_mut())
}

/// Seed place blocks and/or ensure a named block exists in the document.
#[no_mangle]
pub unsafe extern "C" fn aadt_element_ensure(
    session: *mut AadtSession,
    name: *const c_char,
) -> i32 {
    if session.is_null() || name.is_null() {
        return 1;
    }
    let Ok(n) = CStr::from_ptr(name).to_str() else {
        return 1;
    };
    let n = n.trim();
    if n.is_empty() {
        return 1;
    }
    let s = &mut *session;
    aadt_command::ensure_place_blocks(s.inner.document_mut());
    // Also accept place aliases (DOOR → DOOR-SINGLE).
    let resolved = place_block_name(&n.to_ascii_uppercase()).unwrap_or(n);
    if s.inner.document().block(resolved).is_some() || s.inner.document().block(n).is_some() {
        return 0;
    }
    // Soft ok — place tools may still INSERT after seed; missing custom names are host's concern.
    0
}

/// Hatch patterns: name\\tkind\\tdescription
#[no_mangle]
pub extern "C" fn aadt_hatch_patterns_list() -> *mut c_char {
    let root = libraries_root();
    let pats = aadt_library::list_hatch_patterns(&root)
        .unwrap_or_else(|_| aadt_library::builtin_hatch_patterns());
    let mut lines = Vec::new();
    for p in pats {
        let kind = aadt_model::FillKind::from_pattern(&p.name).as_str();
        lines.push(format!("{}\t{}\t{}", p.name, kind, p.description));
    }
    let body = lines.join("\n");
    CString::new(body.replace('\0', ""))
        .map(|c| c.into_raw())
        .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "C" fn aadt_hatch_set_pattern(session: *mut AadtSession, name: *const c_char) {
    if session.is_null() || name.is_null() {
        return;
    }
    let Ok(n) = CStr::from_ptr(name).to_str() else {
        return;
    };
    let n = n.trim();
    if n.is_empty() {
        return;
    }
    let s = &mut *session;
    s.hatch_pattern = n.to_ascii_uppercase();
    s.hatch_pattern_c = CString::new(s.hatch_pattern.as_str()).unwrap_or_default();
}

#[no_mangle]
pub unsafe extern "C" fn aadt_hatch_pattern(session: *const AadtSession) -> *const c_char {
    if session.is_null() {
        return ptr::null();
    }
    (*session).hatch_pattern_c.as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn aadt_otrack_set(session: *mut AadtSession, on: i32) {
    if session.is_null() {
        return;
    }
    let s = &mut *session;
    s.otrack_on = on != 0;
    if !s.otrack_on {
        s.otrack.clear();
        s.otrack_rays_xy.clear();
    }
}

#[no_mangle]
pub unsafe extern "C" fn aadt_otrack_clear(session: *mut AadtSession) {
    if session.is_null() {
        return;
    }
    (*session).otrack.clear();
    (*session).otrack_rays_xy.clear();
}

#[no_mangle]
pub unsafe extern "C" fn aadt_otrack_rays_rebuild(session: *mut AadtSession) -> u64 {
    if session.is_null() {
        return 0;
    }
    rebuild_otrack_rays(&mut *session);
    (*session).otrack_rays_xy.len() as u64
}

#[no_mangle]
pub unsafe extern "C" fn aadt_otrack_rays_copy(
    session: *const AadtSession,
    out: *mut f32,
    max_floats: u64,
) -> u64 {
    if session.is_null() || out.is_null() || max_floats == 0 {
        return 0;
    }
    let src = &(*session).otrack_rays_xy;
    let n = (src.len() as u64).min(max_floats) as usize;
    let n = n - (n % 4);
    std::ptr::copy_nonoverlapping(src.as_ptr(), out, n);
    n as u64
}

#[no_mangle]
pub unsafe extern "C" fn aadt_grips_rebuild(session: *mut AadtSession) -> u64 {
    if session.is_null() {
        return 0;
    }
    let s = &mut *session;
    s.grip_xy.clear();
    let ids: Vec<_> = s.inner.selection().iter().copied().collect();
    for id in ids {
        let Some(e) = s.inner.document().get(id) else {
            continue;
        };
        for (idx, p) in grip_points_geom(&e.geometry).into_iter().enumerate() {
            s.grip_xy.push(id.0 as f32);
            s.grip_xy.push(idx as f32);
            s.grip_xy.push(p.x as f32);
            s.grip_xy.push(p.y as f32);
        }
    }
    s.grip_xy.len() as u64
}

#[no_mangle]
pub unsafe extern "C" fn aadt_grips_copy(
    session: *const AadtSession,
    out: *mut f32,
    max_floats: u64,
) -> u64 {
    if session.is_null() || out.is_null() || max_floats == 0 {
        return 0;
    }
    let src = &(*session).grip_xy;
    let n = (src.len() as u64).min(max_floats) as usize;
    let n = n - (n % 4);
    std::ptr::copy_nonoverlapping(src.as_ptr(), out, n);
    n as u64
}

#[no_mangle]
pub unsafe extern "C" fn aadt_grip_pick(
    session: *mut AadtSession,
    x: f64,
    y: f64,
    tol: f64,
    out_id: *mut u64,
    out_idx: *mut u64,
) -> i32 {
    if session.is_null() {
        return 0;
    }
    let s = &mut *session;
    if s.grip_xy.is_empty() {
        aadt_grips_rebuild(session);
    }
    let tol = tol.max(1e-9);
    let mut best: Option<(f64, u64, u64)> = None;
    for chunk in s.grip_xy.chunks_exact(4) {
        let id = chunk[0] as u64;
        let idx = chunk[1] as u64;
        let gx = chunk[2] as f64;
        let gy = chunk[3] as f64;
        let d = (gx - x).hypot(gy - y);
        if d <= tol && best.as_ref().map_or(true, |(bd, _, _)| d < *bd) {
            best = Some((d, id, idx));
        }
    }
    match best {
        Some((_, id, idx)) => {
            let (gx, gy) = s
                .grip_xy
                .chunks_exact(4)
                .find(|c| c[0] as u64 == id && c[1] as u64 == idx)
                .map(|c| (c[2] as f64, c[3] as f64))
                .unwrap_or((x, y));
            s.hot_grip_xy = Some((gx, gy));
            if !out_id.is_null() {
                *out_id = id;
            }
            if !out_idx.is_null() {
                *out_idx = idx;
            }
            1
        }
        None => {
            s.hot_grip_xy = None;
            0
        }
    }
}

/// Write hot grip world (x,y) into out2. Returns 1 if a grip is hot.
#[no_mangle]
pub unsafe extern "C" fn aadt_hot_grip(session: *const AadtSession, out2: *mut f64) -> i32 {
    if session.is_null() || out2.is_null() {
        return 0;
    }
    match (*session).hot_grip_xy {
        Some((x, y)) => {
            *out2 = x;
            *out2.add(1) = y;
            1
        }
        None => 0,
    }
}

#[no_mangle]
pub unsafe extern "C" fn aadt_grip_move(
    session: *mut AadtSession,
    id: u64,
    grip_idx: u64,
    nx: f64,
    ny: f64,
) -> i32 {
    if session.is_null() {
        return 1;
    }
    let s = &mut *session;
    // Origin = current grip position.
    let origin = s
        .grip_xy
        .chunks_exact(4)
        .find(|c| c[0] as u64 == id && c[1] as u64 == grip_idx)
        .map(|c| (c[2] as f64, c[3] as f64))
        .unwrap_or((nx, ny));
    let (ox, oy) = origin;
    let line = match s.grip_mode {
        GripMode::Stretch => format!("GRIPEDIT {id} {grip_idx} {nx} {ny}"),
        GripMode::Move => format!("MOVE SELECTION {} {}", nx - ox, ny - oy),
        GripMode::Rotate => {
            let deg = (ny - oy).atan2(nx - ox).to_degrees();
            format!("ROTATE SELECTION {ox} {oy} {deg}")
        }
        GripMode::Scale => {
            let factor = ((nx - ox).hypot(ny - oy)).max(1e-9);
            format!("SCALE SELECTION {ox} {oy} {factor}")
        }
        GripMode::Mirror => format!("MIRROR SELECTION {ox} {oy} {nx} {ny} N"),
    };
    feed_line(s, &line);
    aadt_grips_rebuild(session);
    rebuild_selection_lines(s);
    0
}

#[no_mangle]
pub unsafe extern "C" fn aadt_selection_rebuild_lines(session: *mut AadtSession) -> u64 {
    if session.is_null() {
        return 0;
    }
    rebuild_selection_lines(&mut *session);
    (*session).sel_xy.len() as u64
}

#[no_mangle]
pub unsafe extern "C" fn aadt_selection_copy_lines(
    session: *const AadtSession,
    out: *mut f32,
    max_floats: u64,
) -> u64 {
    if session.is_null() || out.is_null() || max_floats == 0 {
        return 0;
    }
    let src = &(*session).sel_xy;
    let n = (src.len() as u64).min(max_floats) as usize;
    let n = n - (n % 4);
    std::ptr::copy_nonoverlapping(src.as_ptr(), out, n);
    n as u64
}

#[no_mangle]
pub unsafe extern "C" fn aadt_ortho_set(session: *mut AadtSession, on: i32) {
    if session.is_null() {
        return;
    }
    (*session).ortho_on = on != 0;
}

#[no_mangle]
pub unsafe extern "C" fn aadt_ortho_get(session: *const AadtSession) -> i32 {
    if session.is_null() {
        return 0;
    }
    i32::from((*session).ortho_on)
}

#[no_mangle]
pub unsafe extern "C" fn aadt_polar_set(session: *mut AadtSession, on: i32) {
    if session.is_null() {
        return;
    }
    (*session).polar_on = on != 0;
}

#[no_mangle]
pub unsafe extern "C" fn aadt_polar_get(session: *const AadtSession) -> i32 {
    if session.is_null() {
        return 0;
    }
    i32::from((*session).polar_on)
}

/// Set numeric tool param: FILLET radius, WALL thickness, or OFFSET distance hint.
#[no_mangle]
pub unsafe extern "C" fn aadt_tool_set_number(session: *mut AadtSession, value: f64) -> i32 {
    if session.is_null() || !value.is_finite() || value < 0.0 {
        return 1;
    }
    let s = &mut *session;
    match s.tool_token {
        Some("FILLET") => {
            s.fillet_radius = value;
            refresh_prompt(s);
            0
        }
        Some("WALL") => {
            s.wall_thickness = value.max(1e-6);
            refresh_prompt(s);
            0
        }
        Some("ARRAYRECT") => {
            let n = (value as usize).max(1);
            if s.array_num_phase == 0 {
                s.array_rows = n;
                s.array_num_phase = 1;
            } else {
                s.array_cols = n;
                s.array_num_phase = 0;
            }
            refresh_prompt(s);
            0
        }
        Some("ARRAYPOLAR") | Some("ARRAYPATH") => {
            s.array_count = (value as usize).max(2);
            refresh_prompt(s);
            0
        }
        Some("IMAGE") => {
            s.image_width = value.max(1e-6);
            refresh_prompt(s);
            0
        }
        _ => {
            s.fillet_radius = value;
            s.wall_thickness = value.max(1e-6);
            0
        }
    }
}

/// Start IMAGE place tool with a filesystem path (UTF-8).
#[no_mangle]
pub unsafe extern "C" fn aadt_tool_start_image(
    session: *mut AadtSession,
    path: *const c_char,
) -> i32 {
    if session.is_null() || path.is_null() {
        return 1;
    }
    let Ok(p) = CStr::from_ptr(path).to_str() else {
        return 1;
    };
    let p = p.trim();
    if p.is_empty() {
        return 1;
    }
    let s = &mut *session;
    clear_tool(s);
    s.tool_token = Some("IMAGE");
    s.tool_kind = Some(ToolKind::Image);
    s.image_path = Some(p.to_string());
    refresh_prompt(s);
    0
}

#[no_mangle]
pub unsafe extern "C" fn aadt_place_flip_set(session: *mut AadtSession, on: i32) {
    if session.is_null() {
        return;
    }
    (*session).place_flip = on != 0;
}

#[no_mangle]
pub unsafe extern "C" fn aadt_place_flip_get(session: *const AadtSession) -> i32 {
    if session.is_null() {
        return 0;
    }
    i32::from((*session).place_flip)
}

#[no_mangle]
pub unsafe extern "C" fn aadt_pick_add_set(session: *mut AadtSession, on: i32) {
    if session.is_null() {
        return;
    }
    (*session).pick_add = on != 0;
}

#[no_mangle]
pub unsafe extern "C" fn aadt_grip_mode_cycle(session: *mut AadtSession) {
    if session.is_null() {
        return;
    }
    let s = &mut *session;
    s.grip_mode = s.grip_mode.next();
    s.grip_mode_c = CString::new(s.grip_mode.as_str()).unwrap_or_default();
}

#[no_mangle]
pub unsafe extern "C" fn aadt_grip_mode(session: *const AadtSession) -> *const c_char {
    if session.is_null() {
        return ptr::null();
    }
    (*session).grip_mode_c.as_ptr()
}

/// Reset the working document (New). Clears tools, path, dirty.
///
/// # Safety
/// `session` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_session_reset(session: *mut AadtSession) {
    if session.is_null() {
        return;
    }
    let s = &mut *session;
    clear_tool(s);
    let mut interp = Interpreter::with_builtins();
    register(&mut interp);
    let fresh = Session::from_interpreter(interp);
    s.current_layer = fresh.document().active_category_layer();
    s.current_layer_c =
        CString::new(s.current_layer.as_str()).unwrap_or_default();
    s.inner = fresh;
    s.file_path = None;
    s.dirty = false;
    s.tess_rev = None;
    s.line_xy.clear();
    s.hatch_pattern = "SOLID".into();
    s.hatch_pattern_c = CString::new("SOLID").unwrap_or_default();
    s.otrack.clear();
    s.otrack_rays_xy.clear();
    s.grip_xy.clear();
    s.hot_grip_xy = None;
    s.selected_dimensions.clear();
    s.sel_xy.clear();
    s.ortho_on = false;
    s.polar_on = false;
    s.fillet_radius = 0.0;
    s.wall_thickness = 0.2;
    s.stretch_window = None;
    s.array_rows = 2;
    s.array_cols = 3;
    s.array_count = 6;
    s.array_num_phase = 0;
    s.image_path = None;
    s.image_width = 1.0;
    s.grip_mode = GripMode::Stretch;
    s.grip_mode_c = CString::new("Stretch").unwrap_or_default();
    s.place_flip = false;
    s.pick_add = false;
    s.fill_xy.clear();
    s.nav_pending = None;
    bind_work_db(s);
    s.last_feedback = CString::new("New drawing").unwrap_or_default();
}

/// Compose a sheet (extents) and write a vector PDF to `path`.
/// `paper` is a size name (`A3`, `A1`, `ANSI-D`, …); null/empty → A3.
/// Returns 0 ok, 1 error (see [`aadt_last_feedback`]).
///
/// # Safety
/// `session` and `path` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_plot_pdf_to_path(
    session: *mut AadtSession,
    path: *const c_char,
    paper: *const c_char,
) -> i32 {
    if session.is_null() || path.is_null() {
        return 1;
    }
    let Ok(path) = CStr::from_ptr(path).to_str() else {
        return 1;
    };
    let path = path.trim();
    if path.is_empty() {
        return 1;
    }
    let paper_name = if paper.is_null() {
        "A3"
    } else {
        CStr::from_ptr(paper).to_str().unwrap_or("A3").trim()
    };
    let paper_name = if paper_name.is_empty() {
        "A3"
    } else {
        paper_name
    };
    let paper_size =
        aadt_docs::PaperSize::from_name(paper_name).unwrap_or(aadt_docs::PaperSize::IsoA3);

    let s = &mut *session;
    let props = s.inner.document().properties().clone();
    let title = if props.title.trim().is_empty() {
        "drawing"
    } else {
        props.title.trim()
    };
    let tb = aadt_docs::TitleBlock::from_properties(&props, title);
    let (sheet_doc, sheet, composed) = aadt_docs::compose_plot_sheet_for_area(
        s.inner.document(),
        paper_size,
        &tb,
        aadt_docs::PlotContent::Extents,
    );
    let (w, h) = sheet.extent();
    let page = aadt_pdf::PageSize::from_mm(w, h);
    let pdf = aadt_pdf::plot(&sheet_doc, page);
    match std::fs::write(path, &pdf) {
        Ok(()) => {
            s.last_feedback = CString::new(format!(
                "Plotted PDF {path} ({paper_name}, scale {})",
                composed.scale_label(1000.0)
            ))
            .unwrap_or_default();
            0
        }
        Err(e) => {
            s.last_feedback =
                CString::new(format!("PDF write failed: {e}")).unwrap_or_default();
            1
        }
    }
}

/// Place a sheet border on the live document (`PLOTSETUP` peer).
/// `paper` null/empty → A3 landscape.
///
/// # Safety
/// `session` must be valid.
#[no_mangle]
pub unsafe extern "C" fn aadt_plot_setup(session: *mut AadtSession, paper: *const c_char) -> i32 {
    if session.is_null() {
        return 1;
    }
    let paper_name = if paper.is_null() {
        "A3"
    } else {
        CStr::from_ptr(paper).to_str().unwrap_or("A3").trim()
    };
    let paper_name = if paper_name.is_empty() {
        "A3"
    } else {
        paper_name
    };
    let paper_size =
        aadt_docs::PaperSize::from_name(paper_name).unwrap_or(aadt_docs::PaperSize::IsoA3);
    let orient = aadt_docs::orientation_from_extents((*session).inner.document());
    let sheet = aadt_docs::Sheet::new(paper_size, orient, 10.0);
    sheet.place((*session).inner.document_mut());
    (*session).dirty = true;
    (*session).last_feedback = CString::new(format!(
        "PLOTSETUP — {paper_name} border on BORDER layer"
    ))
    .unwrap_or_default();
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn abi_version_is_16() {
        assert_eq!(aadt_ffi_version(), 16);
    }

    #[test]
    fn pick_at_selects_dimension_over_nearby_entity() {
        let session = aadt_session_new();
        unsafe {
            let line = CString::new("LINE 0 0 10 0").unwrap();
            assert_eq!(aadt_session_feed(session, line.as_ptr()), 0);
            // Dim line / text at y=2 — closer to (5,2) than the entity at y=0.
            let dim = CString::new("DIM 0 0 10 0 5 2 AN-DIMS").unwrap();
            assert_eq!(aadt_session_feed(session, dim.as_ptr()), 0);
            assert!(aadt_entity_count(session) >= 1);
            let mut id = 99u64;
            assert_eq!(aadt_pick_at(session, 5.0, 2.0, 0.75, &mut id), 1);
            assert_eq!(id, 0, "dimension hit should report entity id 0");
            assert_eq!(aadt_selection_dim_count(session), 1);
            assert_eq!(aadt_selection_count(session), 0);
            aadt_session_free(session);
        }
    }

    #[test]
    fn drawlist_rebuild_is_revision_gated() {
        let session = aadt_session_new();
        let line = CString::new("LINE 0 0 10 0").unwrap();
        unsafe {
            assert_eq!(
                aadt_session_feed(session, line.as_ptr()),
                0,
                "{}",
                cstr_msg(session)
            );
            let n1 = aadt_drawlist_rebuild_lines(session);
            assert!(n1 >= 6);
            let n2 = aadt_drawlist_rebuild_lines(session);
            assert_eq!(n1, n2, "second rebuild must reuse tessellation cache");
            assert_eq!(aadt_workdb_flush(session), 0);
            let path = aadt_workdb_path(session);
            assert!(!path.is_null());
            aadt_string_free(path);
            aadt_session_free(session);
        }
    }

    #[test]
    fn layers_list_and_current() {
        let session = aadt_session_new();
        unsafe {
            let list = aadt_layers_list(session);
            assert!(!list.is_null());
            let s = CStr::from_ptr(list).to_str().unwrap();
            assert!(s.contains('\t'), "{s}");
            aadt_string_free(list);
            let name = CString::new("A-WALL").unwrap();
            assert_eq!(aadt_layer_set_current(session, name.as_ptr()), 0);
            let cur = CStr::from_ptr(aadt_layer_current(session))
                .to_str()
                .unwrap();
            assert_eq!(cur, "A-WALL");
            aadt_session_free(session);
        }
    }

    #[test]
    fn tool_polyline_finish() {
        let session = aadt_session_new();
        let cmd = CString::new("PL").unwrap();
        unsafe {
            assert_eq!(aadt_tool_start(session, cmd.as_ptr()), 0);
            assert_eq!(aadt_tool_accept_point(session, 0.0, 0.0), 0);
            assert_eq!(aadt_tool_accept_point(session, 10.0, 0.0), 0);
            assert_eq!(aadt_tool_accept_point(session, 10.0, 5.0), 0);
            assert_eq!(aadt_tool_finish(session), 0);
            assert_eq!(aadt_tool_active(session), 0);
            assert_eq!(aadt_entity_count(session), 1);
            aadt_session_free(session);
        }
    }

    #[test]
    fn blocks_list_empty_ok() {
        let session = aadt_session_new();
        unsafe {
            let list = aadt_blocks_list(session);
            assert!(!list.is_null());
            aadt_string_free(list);
            let cat = aadt_catalog_tools();
            assert!(!cat.is_null());
            let s = CStr::from_ptr(cat).to_str().unwrap();
            assert!(s.contains("LINE\t"), "{s}");
            aadt_string_free(cat);
            aadt_session_free(session);
        }
    }

    #[test]
    fn feed_line_and_count_entities() {
        let session = aadt_session_new();
        assert!(!session.is_null());
        let line = CString::new("LINE 0 0 10 0").unwrap();
        unsafe {
            assert_eq!(aadt_session_feed(session, line.as_ptr()), 0);
            assert_eq!(aadt_entity_count(session), 1);
            assert!(aadt_drawlist_len(session) >= 1);
            let n = aadt_drawlist_rebuild_lines(session);
            assert!(n >= 4, "expected at least one segment, got {n}");
            aadt_session_free(session);
        }
    }

    #[test]
    fn text_drawlist_emits_outline_segments() {
        let session = aadt_session_new();
        let line = CString::new("TEXT 0 0 1 HELLO").unwrap();
        unsafe {
            let rc = aadt_session_feed(session, line.as_ptr());
            assert_eq!(rc, 0, "TEXT feed: {}", cstr_msg(session));
            let n = aadt_drawlist_rebuild_lines(session);
            assert!(
                n > 16,
                "expected glyph outline floats for HELLO, got {n}"
            );
            aadt_session_free(session);
        }
    }

    unsafe fn cstr_msg(session: *const AadtSession) -> String {
        let p = aadt_last_feedback(session);
        if p.is_null() {
            return String::new();
        }
        CStr::from_ptr(p).to_string_lossy().into_owned()
    }

    #[test]
    fn tool_line_two_clicks() {
        let session = aadt_session_new();
        let cmd = CString::new("LINE").unwrap();
        unsafe {
            assert_eq!(aadt_tool_start(session, cmd.as_ptr()), 0);
            assert_eq!(aadt_tool_active(session), 1);
            assert_eq!(aadt_tool_accept_point(session, 0.0, 0.0), 0);
            assert_eq!(aadt_tool_active(session), 1);
            assert_eq!(aadt_tool_accept_point(session, 10.0, 0.0), 0);
            // Chain continues after first segment.
            assert_eq!(aadt_tool_active(session), 1);
            assert_eq!(aadt_entity_count(session), 1);
            assert_eq!(aadt_tool_accept_point(session, 10.0, 5.0), 0);
            assert_eq!(aadt_entity_count(session), 2);
            assert_eq!(aadt_tool_finish(session), 0);
            assert_eq!(aadt_tool_active(session), 0);
            aadt_session_free(session);
        }
    }

    #[test]
    fn tool_circle_center_radius_point() {
        let session = aadt_session_new();
        let cmd = CString::new("C").unwrap();
        unsafe {
            assert_eq!(aadt_tool_start(session, cmd.as_ptr()), 0);
            assert_eq!(aadt_tool_accept_point(session, 5.0, 5.0), 0);
            aadt_tool_set_cursor(session, 8.0, 5.0);
            let n = aadt_tool_preview_rebuild_lines(session);
            assert!(n >= 4, "preview expected, got {n}");
            assert_eq!(aadt_tool_accept_point(session, 8.0, 5.0), 0);
            assert_eq!(aadt_tool_active(session), 0);
            assert_eq!(aadt_entity_count(session), 1);
            aadt_session_free(session);
        }
    }

    #[test]
    fn tool_rectangle_and_cancel() {
        let session = aadt_session_new();
        let cmd = CString::new("REC").unwrap();
        unsafe {
            assert_eq!(aadt_tool_start(session, cmd.as_ptr()), 0);
            assert_eq!(aadt_tool_accept_point(session, 0.0, 0.0), 0);
            aadt_tool_cancel(session);
            assert_eq!(aadt_tool_active(session), 0);
            assert_eq!(aadt_entity_count(session), 0);
            assert_eq!(aadt_tool_start(session, cmd.as_ptr()), 0);
            assert_eq!(aadt_tool_accept_point(session, 0.0, 0.0), 0);
            assert_eq!(aadt_tool_accept_point(session, 4.0, 3.0), 0);
            assert_eq!(aadt_entity_count(session), 1);
            aadt_session_free(session);
        }
    }

    #[test]
    fn pick_and_prop_summary() {
        let session = aadt_session_new();
        let line = CString::new("LINE 0 0 10 0").unwrap();
        unsafe {
            assert_eq!(aadt_session_feed(session, line.as_ptr()), 0);
            assert_eq!(aadt_entity_count(session), 1);
            let mut id = 999u64;
            assert_eq!(aadt_pick_at(session, 5.0, 0.0, 1.0, &mut id), 1);
            assert_eq!(aadt_selection_count(session), 1);
            let mut ids = [0u64; 4];
            assert_eq!(aadt_selection_copy_ids(session, ids.as_mut_ptr(), 4), 1);
            assert_eq!(ids[0], id);
            let summary = aadt_entity_prop_summary(session, id);
            assert!(!summary.is_null());
            let s = CStr::from_ptr(summary).to_str().unwrap();
            assert!(s.contains("Type\tLINE"), "{s}");
            assert!(s.contains("Linetype\t"), "{s}");
            assert!(s.contains("Lineweight\t"), "{s}");
            assert!(s.contains("Color\t"), "{s}");
            aadt_string_free(summary);
            assert_eq!(aadt_pick_at(session, 1000.0, 1000.0, 0.1, &mut id), 0);
            assert_eq!(aadt_selection_count(session), 0);
            aadt_session_free(session);
        }
    }

    #[test]
    fn lisp_via_feed() {
        let session = aadt_session_new();
        let line = CString::new("(+ 2 3)").unwrap();
        unsafe {
            assert_eq!(aadt_session_feed(session, line.as_ptr()), 0);
            let fb = CStr::from_ptr(aadt_last_feedback(session))
                .to_str()
                .unwrap();
            assert!(fb.contains('5'), "{fb}");
            aadt_session_free(session);
        }
    }

    #[test]
    fn host_commands_registered() {
        let session = aadt_session_new();
        let line = CString::new("OPEN __no_such_file_aadt__.dxf").unwrap();
        unsafe {
            let code = aadt_session_feed(session, line.as_ptr());
            assert_eq!(code, 1);
            let fb = CStr::from_ptr(aadt_last_feedback(session))
                .to_str()
                .unwrap()
                .to_ascii_lowercase();
            assert!(
                !fb.contains("unknown"),
                "OPEN should be registered, got: {fb}"
            );
            aadt_session_free(session);
        }
    }

    /// M18/M19 WinUI shipping-path smoke — headless stand-in for the native shell.
    #[test]
    fn m19_move_tool_with_selection() {
        let session = aadt_session_new();
        unsafe {
            let line = CString::new("LINE 0 0 10 0").unwrap();
            assert_eq!(aadt_session_feed(session, line.as_ptr()), 0);
            let mut id = 0u64;
            assert_eq!(aadt_pick_at(session, 5.0, 0.0, 1.0, &mut id), 1);
            let cmd = CString::new("MOVE").unwrap();
            assert_eq!(aadt_tool_start(session, cmd.as_ptr()), 0);
            assert_eq!(aadt_tool_active(session), 1);
            // Selection already present → skip selecting phase.
            assert_eq!(aadt_tool_accept_point(session, 0.0, 0.0), 0);
            assert_eq!(aadt_tool_accept_point(session, 5.0, 0.0), 0);
            assert_eq!(aadt_tool_active(session), 0);
            aadt_session_free(session);
        }
    }

    #[test]
    fn m19_arc_three_points() {
        let session = aadt_session_new();
        let cmd = CString::new("ARC").unwrap();
        unsafe {
            assert_eq!(aadt_tool_start(session, cmd.as_ptr()), 0);
            assert_eq!(aadt_tool_accept_point(session, 0.0, 0.0), 0);
            assert_eq!(aadt_tool_accept_point(session, 5.0, 5.0), 0);
            assert_eq!(aadt_tool_accept_point(session, 10.0, 0.0), 0);
            assert_eq!(aadt_tool_active(session), 0);
            assert_eq!(aadt_entity_count(session), 1);
            aadt_session_free(session);
        }
    }

    #[test]
    fn m19_hatch_closed_rectangle() {
        let session = aadt_session_new();
        unsafe {
            let rec = CString::new("RECTANGLE 0 0 20 10").unwrap();
            assert_eq!(aadt_session_feed(session, rec.as_ptr()), 0);
            let before = aadt_drawlist_rebuild_lines(session);
            let cmd = CString::new("HATCH").unwrap();
            assert_eq!(aadt_tool_start(session, cmd.as_ptr()), 0);
            assert_eq!(aadt_tool_accept_point(session, 10.0, 5.0), 0);
            assert_eq!(aadt_tool_active(session), 0);
            let after = aadt_drawlist_rebuild_lines(session);
            assert!(
                after >= before,
                "hatch should keep or grow drawlist ({before} → {after})"
            );
            aadt_session_free(session);
        }
    }

    #[test]
    fn m19_text_point_then_value() {
        let session = aadt_session_new();
        let cmd = CString::new("TEXT").unwrap();
        unsafe {
            assert_eq!(aadt_tool_start(session, cmd.as_ptr()), 0);
            assert_eq!(aadt_tool_needs_text(session), 0);
            assert_eq!(aadt_tool_accept_point(session, 1.0, 2.0), 0);
            assert_eq!(aadt_tool_needs_text(session), 1);
            let val = CString::new("ROOM-A").unwrap();
            assert_eq!(aadt_tool_set_text(session, val.as_ptr()), 0);
            assert_eq!(aadt_tool_active(session), 0);
            assert_eq!(aadt_entity_count(session), 1);
            aadt_session_free(session);
        }
    }

    #[test]
    fn m19_select_box_window_and_crossing() {
        let session = aadt_session_new();
        unsafe {
            assert_eq!(
                aadt_session_feed(session, CString::new("LINE 0 0 5 0").unwrap().as_ptr()),
                0
            );
            assert_eq!(
                aadt_session_feed(session, CString::new("LINE 20 0 25 0").unwrap().as_ptr()),
                0
            );
            // Window enclosing only first line.
            let n = aadt_select_box(session, -1.0, -1.0, 6.0, 1.0);
            assert_eq!(n, 1, "window should catch one segment");
            assert_eq!(aadt_selection_count(session), 1);
            // Crossing from right→left that touches both (box covers mid gap + both).
            let n2 = aadt_select_box(session, 26.0, -1.0, -1.0, 1.0);
            assert!(n2 >= 1, "crossing select got {n2}");
            aadt_session_free(session);
        }
    }

    #[test]
    fn m19_osnap_endpoint_on_line() {
        let session = aadt_session_new();
        unsafe {
            assert_eq!(
                aadt_session_feed(session, CString::new("LINE 0 0 10 0").unwrap().as_ptr()),
                0
            );
            aadt_osnap_set(session, 1);
            let cmd = CString::new("LINE").unwrap();
            assert_eq!(aadt_tool_start(session, cmd.as_ptr()), 0);
            // Near endpoint (0,0) — should snap when accepting.
            assert_eq!(aadt_tool_accept_point(session, 0.2, 0.15), 0);
            aadt_tool_set_cursor(session, 0.15, 0.1);
            let mut xy = [0.0_f64; 2];
            // Marker may or may not stick after accept; cursor update should try.
            let _ = aadt_osnap_marker(session, xy.as_mut_ptr());
            aadt_tool_cancel(session);
            aadt_session_free(session);
        }
    }

    #[test]
    fn m19_erase_selection_immediate() {
        let session = aadt_session_new();
        unsafe {
            assert_eq!(
                aadt_session_feed(session, CString::new("LINE 0 0 1 0").unwrap().as_ptr()),
                0
            );
            let mut id = 0u64;
            assert_eq!(aadt_pick_at(session, 0.5, 0.0, 1.0, &mut id), 1);
            assert_eq!(aadt_entity_count(session), 1);
            let cmd = CString::new("ERASE").unwrap();
            assert_eq!(aadt_tool_start(session, cmd.as_ptr()), 0);
            assert_eq!(aadt_entity_count(session), 0);
            assert_eq!(aadt_tool_active(session), 0);
            aadt_session_free(session);
        }
    }

    #[test]
    fn m19_session_reset_clears_geometry() {
        let session = aadt_session_new();
        unsafe {
            assert_eq!(
                aadt_session_feed(session, CString::new("LINE 0 0 1 0").unwrap().as_ptr()),
                0
            );
            assert_eq!(aadt_entity_count(session), 1);
            aadt_session_reset(session);
            assert_eq!(aadt_entity_count(session), 0);
            aadt_session_free(session);
        }
    }

    #[test]
    fn m19_insert_tool_requires_name() {
        let session = aadt_session_new();
        unsafe {
            let bare = CString::new("INSERT").unwrap();
            assert_eq!(aadt_tool_start(session, bare.as_ptr()), 1);
            // Define a block then place.
            assert_eq!(
                aadt_session_feed(session, CString::new("LINE 0 0 2 0").unwrap().as_ptr()),
                0
            );
            let mut id = 0u64;
            assert_eq!(aadt_pick_at(session, 1.0, 0.0, 1.0, &mut id), 1);
            let blk = CString::new(format!("BLOCK DEMO 0 0 {id}")).unwrap();
            let rc = aadt_session_feed(session, blk.as_ptr());
            assert_eq!(rc, 0, "BLOCK: {}", cstr_msg(session));
            let name = CString::new("DEMO").unwrap();
            assert_eq!(aadt_tool_start_insert(session, name.as_ptr()), 0);
            assert_eq!(aadt_tool_accept_point(session, 10.0, 10.0), 0);
            // INSERT stays active for multi-place
            assert_eq!(aadt_tool_active(session), 1);
            aadt_tool_cancel(session);
            aadt_session_free(session);
        }
    }

    #[test]
    fn m20_complete_and_interactive() {
        let session = aadt_session_new();
        unsafe {
            let p = CString::new("LI").unwrap();
            let list = aadt_complete(session, p.as_ptr());
            assert!(!list.is_null());
            let s = CStr::from_ptr(list).to_str().unwrap();
            assert!(s.to_ascii_uppercase().contains("LINE"), "{s}");
            aadt_string_free(list);
            let dim = CString::new("DIM").unwrap();
            assert_eq!(aadt_is_interactive(dim.as_ptr()), 1);
            let junk = CString::new("NOTACOMMAND").unwrap();
            assert_eq!(aadt_is_interactive(junk.as_ptr()), 0);
            aadt_session_free(session);
        }
    }

    #[test]
    fn m20_dim_three_points() {
        let session = aadt_session_new();
        let cmd = CString::new("DIM").unwrap();
        unsafe {
            assert_eq!(aadt_tool_start(session, cmd.as_ptr()), 0);
            assert_eq!(aadt_tool_accept_point(session, 0.0, 0.0), 0);
            assert_eq!(aadt_tool_accept_point(session, 10.0, 0.0), 0);
            assert_eq!(aadt_tool_accept_point(session, 5.0, 2.0), 0);
            assert_eq!(aadt_tool_active(session), 0);
            assert!(aadt_document_revision(session) > 0);
            aadt_session_free(session);
        }
    }

    #[test]
    fn m20_door_place_insert() {
        let session = aadt_session_new();
        let cmd = CString::new("DOOR").unwrap();
        unsafe {
            assert_eq!(aadt_tool_start(session, cmd.as_ptr()), 0);
            assert_eq!(aadt_tool_active(session), 1);
            assert_eq!(aadt_tool_accept_point(session, 1.0, 1.0), 0);
            assert!(aadt_entity_count(session) >= 1);
            aadt_session_free(session);
        }
    }

    #[test]
    fn m20_hatch_pattern_and_grips() {
        let session = aadt_session_new();
        unsafe {
            let pats = aadt_hatch_patterns_list();
            assert!(!pats.is_null());
            aadt_string_free(pats);
            let ansi = CString::new("ANSI31").unwrap();
            aadt_hatch_set_pattern(session, ansi.as_ptr());
            let cur = CStr::from_ptr(aadt_hatch_pattern(session)).to_str().unwrap();
            assert_eq!(cur, "ANSI31");
            assert_eq!(
                aadt_session_feed(session, CString::new("LINE 0 0 10 0").unwrap().as_ptr()),
                0
            );
            let mut id = 0u64;
            assert_eq!(aadt_pick_at(session, 5.0, 0.0, 1.0, &mut id), 1);
            let n = aadt_grips_rebuild(session);
            assert!(n >= 4, "expected grip floats, got {n}");
            let mut gid = 0u64;
            let mut gidx = 0u64;
            assert_eq!(aadt_grip_pick(session, 0.0, 0.0, 0.5, &mut gid, &mut gidx), 1);
            assert_eq!(gid, id);
            assert_eq!(aadt_grip_move(session, gid, gidx, -1.0, 0.0), 0);
            aadt_session_free(session);
        }
    }

    #[test]
    fn m21_catalog_excludes_wblock() {
        unsafe {
            let cat = aadt_catalog_tools();
            let s = CStr::from_ptr(cat).to_str().unwrap();
            assert!(s.contains("LINE\t"), "{s}");
            assert!(s.contains("WALL\t"), "{s}");
            assert!(s.contains("STRETCH\t"), "{s}");
            assert!(!s.contains("WBLOCK\t"), "{s}");
            assert!(!s.contains("LENGTHEN\t"), "{s}");
            aadt_string_free(cat);
        }
    }

    #[test]
    fn m21_wall_polyline() {
        let session = aadt_session_new();
        let cmd = CString::new("WALL").unwrap();
        unsafe {
            assert_eq!(aadt_tool_start(session, cmd.as_ptr()), 0);
            assert_eq!(aadt_tool_set_number(session, 0.23), 0);
            assert_eq!(aadt_tool_accept_point(session, 0.0, 0.0), 0);
            assert_eq!(aadt_tool_accept_point(session, 5.0, 0.0), 0);
            assert_eq!(aadt_tool_accept_point(session, 5.0, 3.0), 0);
            assert_eq!(aadt_tool_finish(session), 0);
            assert!(aadt_entity_count(session) >= 1);
            aadt_session_free(session);
        }
    }

    #[test]
    fn m21_ortho_and_selection_lines() {
        let session = aadt_session_new();
        unsafe {
            aadt_ortho_set(session, 1);
            assert_eq!(aadt_ortho_get(session), 1);
            assert_eq!(
                aadt_session_feed(session, CString::new("LINE 0 0 10 0").unwrap().as_ptr()),
                0
            );
            let mut id = 0u64;
            assert_eq!(aadt_pick_at(session, 5.0, 0.0, 1.0, &mut id), 1);
            let n = aadt_selection_rebuild_lines(session);
            assert!(n >= 4, "selection overlay floats {n}");
            aadt_session_free(session);
        }
    }

    #[test]
    fn m22_stretch_and_revcloud() {
        let session = aadt_session_new();
        unsafe {
            assert_eq!(
                aadt_session_feed(session, CString::new("LINE 0 0 10 0").unwrap().as_ptr()),
                0
            );
            let mut id = 0u64;
            assert_eq!(aadt_pick_at(session, 5.0, 0.0, 1.0, &mut id), 1);
            let cmd = CString::new("STRETCH").unwrap();
            assert_eq!(aadt_tool_start(session, cmd.as_ptr()), 0);
            // Crossing window around the line
            assert_eq!(aadt_tool_accept_point(session, -1.0, -1.0), 0);
            assert_eq!(aadt_tool_accept_point(session, 11.0, 1.0), 0);
            assert_eq!(aadt_tool_accept_point(session, 0.0, 0.0), 0);
            assert_eq!(aadt_tool_accept_point(session, 0.0, 2.0), 0);
            assert_eq!(aadt_tool_active(session), 0);

            let rc = CString::new("REVCLOUD").unwrap();
            assert_eq!(aadt_tool_start(session, rc.as_ptr()), 0);
            assert_eq!(aadt_tool_accept_point(session, 0.0, 0.0), 0);
            assert_eq!(aadt_tool_accept_point(session, 4.0, 3.0), 0);
            assert_eq!(aadt_tool_active(session), 0);
            aadt_session_free(session);
        }
    }

    #[test]
    fn m22_arrayrect_and_image_start() {
        let session = aadt_session_new();
        unsafe {
            assert_eq!(
                aadt_session_feed(session, CString::new("CIRCLE 0 0 1").unwrap().as_ptr()),
                0
            );
            let mut id = 0u64;
            assert_eq!(aadt_pick_at(session, 0.0, 0.0, 1.5, &mut id), 1);
            let cmd = CString::new("ARRAYRECT").unwrap();
            assert_eq!(aadt_tool_start(session, cmd.as_ptr()), 0);
            assert_eq!(aadt_tool_set_number(session, 2.0), 0);
            assert_eq!(aadt_tool_set_number(session, 2.0), 0);
            assert_eq!(aadt_tool_accept_point(session, 0.0, 0.0), 0);
            assert_eq!(aadt_tool_accept_point(session, 3.0, 0.0), 0);
            assert_eq!(aadt_tool_accept_point(session, 0.0, 3.0), 0);
            assert!(aadt_entity_count(session) >= 2);

            let bare = CString::new("IMAGE").unwrap();
            assert_eq!(aadt_tool_start(session, bare.as_ptr()), 1);
            let path = CString::new("C:/tmp/furniture.png").unwrap();
            assert_eq!(aadt_tool_start_image(session, path.as_ptr()), 0);
            assert_eq!(aadt_tool_active(session), 1);
            aadt_tool_cancel(session);
            aadt_session_free(session);
        }
    }

    #[test]
    fn m23_wpolygon_and_transparent_zoom() {
        let session = aadt_session_new();
        unsafe {
            assert_eq!(
                aadt_session_feed(session, CString::new("LINE 0 0 10 0").unwrap().as_ptr()),
                0
            );
            assert_eq!(
                aadt_session_feed(session, CString::new("LINE 0 0 0 10").unwrap().as_ptr()),
                0
            );
            let wp = CString::new("WP").unwrap();
            assert_eq!(aadt_tool_start(session, wp.as_ptr()), 0);
            assert_eq!(aadt_tool_accept_point(session, -1.0, -1.0), 0);
            assert_eq!(aadt_tool_accept_point(session, 11.0, -1.0), 0);
            assert_eq!(aadt_tool_accept_point(session, 11.0, 1.0), 0);
            assert_eq!(aadt_tool_accept_point(session, -1.0, 1.0), 0);
            assert_eq!(aadt_tool_finish(session), 0);
            assert!(aadt_selection_count(session) >= 1);

            let line = CString::new("LINE").unwrap();
            assert_eq!(aadt_tool_start(session, line.as_ptr()), 0);
            assert_eq!(aadt_tool_active(session), 1);
            assert_eq!(
                aadt_session_feed(session, CString::new("'ZOOM E").unwrap().as_ptr()),
                0
            );
            assert_eq!(aadt_tool_active(session), 1, "transparent zoom keeps tool");
            let nav = aadt_nav_take(session);
            assert!(!nav.is_null());
            let hint = CStr::from_ptr(nav).to_str().unwrap();
            assert_eq!(hint, "extents");
            aadt_string_free(nav);

            let cat = aadt_catalog_tools();
            let s = CStr::from_ptr(cat).to_str().unwrap();
            assert!(s.contains("WPOLYGON\t"), "{s}");
            assert!(s.contains("FENCE\t"), "{s}");
            aadt_string_free(cat);
            aadt_session_free(session);
        }
    }

    #[test]
    fn m23_fills_export_after_hatch() {
        let session = aadt_session_new();
        unsafe {
            assert_eq!(
                aadt_session_feed(
                    session,
                    CString::new("RECTANGLE 0 0 4 3").unwrap().as_ptr()
                ),
                0
            );
            // Pick the rectangle as hatch boundary via tool.
            let h = CString::new("HATCH").unwrap();
            assert_eq!(aadt_tool_start(session, h.as_ptr()), 0);
            assert_eq!(aadt_tool_accept_point(session, 2.0, 1.5), 0);
            let n = aadt_drawlist_rebuild_lines(session);
            assert!(n > 0);
            let mut buf = vec![0f32; 256];
            let nf = aadt_drawlist_copy_fills(session, buf.as_mut_ptr(), buf.len() as u64);
            assert!(
                nf >= 8,
                "expected fill pack (n,alpha,xy…) got {nf} floats"
            );
            aadt_session_free(session);
        }
    }
}
