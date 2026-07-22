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

use aadt_geometry::{Bbox, Point2};
use aadt_model::{Document, EntityId, Geometry, DEFAULT_LINEWEIGHT_MM};
use aadt_render::{Camera, DrawCommand, DrawList};
use aadt_standards::aci_to_srgb;
use eframe::egui;

use crate::osnap::{find_osnap, SnapKind};
use crate::Session;

#[path = "command_engine.rs"]
mod command_engine;
#[path = "gui_chrome.rs"]
mod chrome;
#[path = "gui_icons.rs"]
mod icons;

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

/// Pick tolerance in screen pixels.
const PICK_PIXELS: f64 = 6.0;

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

/// Object-snap aperture in screen pixels (slightly larger than pick).
const SNAP_PIXELS: f64 = 10.0;

/// Default plot scale denominator (1:100) for lineweight preview.
const PLOT_SCALE: f32 = 100.0;

/// Open the desktop shell on a configured [`Session`] and run until the window
/// closes. Drivers (the CLI `app` command) register `OPEN`/`SAVE`/`TAKEOFF`/
/// `LISP` on the session before calling this — the GUI never owns those deps.
pub fn run(session: Session) -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("AADT")
            .with_inner_size([1280.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native(
        "AADT",
        options,
        Box::new(move |cc| {
            let mut visuals = egui::Visuals::light();
            // AutoCAD-like light chrome around a white paper canvas.
            visuals.panel_fill = egui::Color32::from_rgb(236, 236, 236);
            visuals.window_fill = egui::Color32::from_rgb(248, 248, 248);
            visuals.extreme_bg_color = egui::Color32::from_rgb(225, 225, 225);
            visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(220, 220, 220);
            visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(200, 210, 230);
            visuals.widgets.active.bg_fill = egui::Color32::from_rgb(170, 190, 220);
            cc.egui_ctx.set_visuals(visuals);
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
    Line {
        last: Option<Point2>,
    },
    Circle {
        center: Option<Point2>,
    },
    Arc {
        center: Option<Point2>,
        start: Option<Point2>,
    },
    Rectangle {
        first: Option<Point2>,
    },
    Move {
        base: Option<Point2>,
    },
    Copy {
        base: Option<Point2>,
    },
    Rotate {
        pivot: Option<Point2>,
    },
    Scale {
        pivot: Option<Point2>,
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
    /// Stretch: window corner1 → corner2 → base → destination.
    Stretch {
        w0: Option<Point2>,
        w1: Option<Point2>,
        base: Option<Point2>,
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
    /// Open polyline: click vertices; Enter finishes.
    Polyline {
        points: Vec<Point2>,
    },
    /// Hatch: click a closed polyline or circle.
    Hatch,
    /// Trim: cutter first, then click object (side to discard).
    Trim {
        cutter: Option<EntityId>,
    },
    /// Extend: boundary first, then click object near the end to grow.
    Extend {
        boundary: Option<EntityId>,
    },
    /// Mirror selection across two axis points.
    Mirror {
        a: Option<Point2>,
    },
    /// Explode: click an insert (or explode selection via command).
    Explode,
    /// Aligned dimension: two points.
    Dim {
        first: Option<Point2>,
    },
    /// Match properties: source entity, then targets.
    MatchProp {
        source: Option<EntityId>,
    },
    /// Zoom window: two corners.
    ZoomWindow {
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
}

impl PlaceKind {
    fn layer(self) -> &'static str {
        match self {
            Self::Door => "A-DOOR",
            Self::Window => "A-GLAZ",
            Self::Column => "S-COLS",
            Self::Beam => "S-BEAM",
            Self::Slab => "A-FLOR",
        }
    }

    fn elem(self) -> &'static str {
        match self {
            Self::Door => "door",
            Self::Window => "window",
            Self::Column => "column",
            Self::Beam => "beam",
            Self::Slab => "slab",
        }
    }

    fn block_name(self) -> &'static str {
        match self {
            Self::Door => "DOOR-SINGLE",
            Self::Window => "WINDOW",
            Self::Column => "COLUMN",
            Self::Beam => "BEAM",
            Self::Slab => "SLAB",
        }
    }
}

/// Map a bare command token to a tool. `None` means "not an interactive command"
/// (the caller feeds it to the session as a typed command instead).
pub(super) fn tool_for(token: &str, has_selection: bool) -> Option<Tool> {
    match token.to_ascii_uppercase().as_str() {
        "L" | "LINE" => Some(Tool::Line { last: None }),
        "C" | "CIRCLE" => Some(Tool::Circle { center: None }),
        "A" | "ARC" => Some(Tool::Arc {
            center: None,
            start: None,
        }),
        "REC" | "RECTANG" | "RECTANGLE" => Some(Tool::Rectangle { first: None }),
        "PL" | "PLINE" | "POLYLINE" => Some(Tool::Polyline { points: Vec::new() }),
        "H" | "HATCH" => Some(Tool::Hatch),
        "TR" | "TRIM" => Some(Tool::Trim { cutter: None }),
        "EX" | "EXTEND" => Some(Tool::Extend { boundary: None }),
        "MI" | "MIRROR" if has_selection => Some(Tool::Mirror { a: None }),
        "X" | "EXPLODE" => Some(Tool::Explode),
        "DIM" | "DIMLINEAR" => Some(Tool::Dim { first: None }),
        "MA" | "MATCHPROP" => Some(Tool::MatchProp { source: None }),
        "Z" | "ZOOM" => Some(Tool::ZoomWindow { first: None }),
        "O" | "OFFSET" => Some(Tool::Offset {
            dist: None,
            entity: None,
        }),
        "T" | "TEXT" | "DTEXT" => Some(Tool::Text { origin: None }),
        "MLEADER" | "LEADER" | "MLD" | "LE" => Some(Tool::Mleader {
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
        "S" | "STRETCH" if has_selection => Some(Tool::Stretch {
            w0: None,
            w1: None,
            base: None,
        }),
        "BRUSH" | "VB" => Some(Tool::Brush { points: None }),
        "F" | "FILLET" => Some(Tool::Fillet {
            radius: None,
            first: None,
        }),
        "I" | "INSERT" => None, // needs a block name — use Blocks Manager
        "M" | "MOVE" if has_selection => Some(Tool::Move { base: None }),
        "CO" | "CP" | "COPY" if has_selection => Some(Tool::Copy { base: None }),
        "RO" | "ROTATE" if has_selection => Some(Tool::Rotate { pivot: None }),
        "SC" | "SCALE" if has_selection => Some(Tool::Scale { pivot: None }),
        "W" | "WALL" => Some(Tool::Wall {
            thickness: None,
            points: Vec::new(),
        }),
        "D" | "DO" | "DOOR" => Some(Tool::PlaceElem {
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
        _ => None,
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
    /// Layer new interactive geometry is drawn on.
    current_layer: String,
    /// Screen anchor of an in-progress box selection.
    box_start: Option<egui::Pos2>,
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
    /// Camera before last ZOOM, for ZOOM P.
    zoom_prev: Option<Camera>,
    /// Active snap under the cursor (for the marker), if any.
    snap_hit: Option<(SnapKind, Point2)>,
    /// Active layer-set name for the side panel.
    layer_set: String,
    /// Right-dock tab: 0 = Layers, 1 = Blocks, 2 = Takeoff.
    right_tab: usize,
    /// Block name queued for INSERT from the Blocks Manager.
    insert_block: String,
    /// Filter text in the Layer Manager.
    layer_filter: String,
    /// Show the right-hand Layer Properties Manager.
    show_layer_mgr: bool,
    /// Status-bar: display lineweights in the viewport.
    show_lweights: bool,
    /// Local copy of Drawing Property Manager fields (applied on Save in the panel).
    props_edit: aadt_model::DrawingProperties,
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
    /// Spacebar-primary command engine.
    engine: CommandEngine,
    space_gesture: SpaceGesture,
    /// Hold / Ctrl+Space command palette.
    palette_open: bool,
    palette_filter: String,
}

impl AadtApp {
    fn from_session(session: Session) -> Self {
        Self {
            session,
            command: String::new(),
            camera: Camera::new((800.0, 600.0)),
            fitted: false,
            tool: Tool::Select,
            selection: Vec::new(),
            current_layer: "A-WALL".to_string(),
            box_start: None,
            cursor: None,
            cursor_screen: None,
            dyn_input: String::new(),
            last_line: None,
            note: String::new(),
            refocus: false,
            ortho: false,
            osnap: true,
            polar: false,
            osnap_track: false,
            zoom_prev: None,
            snap_hit: None,
            layer_set: "CONSTRUCTION".to_string(),
            right_tab: 0,
            insert_block: String::new(),
            layer_filter: String::new(),
            show_layer_mgr: true,
            show_lweights: true,
            props_edit: aadt_model::DrawingProperties::default(),
            plot_open: false,
            plot_drawing_name: String::new(),
            plot_paper: "A3".into(),
            plot_notes: String::new(),
            plot_include_notes: true,
            plot_nameplate: "professional".into(),
            dyn_x: String::new(),
            dyn_y: String::new(),
            dyn_focus: CoordFocus::X,
            hatch_pattern: "ANSI31".into(),
            autonum_prefix: "A-".into(),
            autonum_start: "1".into(),
            table_rows: "5".into(),
            table_cols: "4".into(),
            engine: CommandEngine::default(),
            space_gesture: SpaceGesture::default(),
            palette_open: false,
            palette_filter: String::new(),
        }
    }

    /// The anchor point of the active tool (first click), if it has one — the
    /// reference for direct distance entry and the length/angle readout.
    fn tool_anchor(&self) -> Option<Point2> {
        match &self.tool {
            Tool::Line { last } => *last,
            Tool::Circle { center } => *center,
            Tool::Arc { center, start } => start.or(*center),
            Tool::Rectangle { first } => *first,
            Tool::Move { base } | Tool::Copy { base } => *base,
            Tool::Rotate { pivot } | Tool::Scale { pivot } => *pivot,
            Tool::Text { origin } => *origin,
            Tool::Mleader {
                arrow,
                landing,
                text_pt,
            } => text_pt.or(*landing).or(*arrow),
            Tool::Stretch { w0, w1, base } => base.or(*w1).or(*w0),
            Tool::Brush { points } => points.as_ref().and_then(|v| v.last().copied()),
            Tool::Polyline { points } => points.last().copied(),
            Tool::Wall { points, .. } => points.last().copied(),
            Tool::Dim { first } => *first,
            Tool::Mirror { a } => *a,
            Tool::ZoomWindow { first } => *first,
            Tool::SectionLine { first } => *first,
            Tool::Callout { arrow, center } => center.or(*arrow),
            Tool::RevCloud { first } => *first,
            Tool::Insert { .. }
            | Tool::Offset { .. }
            | Tool::Fillet { .. }
            | Tool::Hatch
            | Tool::Trim { .. }
            | Tool::Extend { .. }
            | Tool::Explode
            | Tool::MatchProp { .. }
            | Tool::RevMark
            | Tool::TablePlace
            | Tool::PlaceElem { .. }
            | Tool::Select => None,
        }
    }

    /// Apply Ortho (F8), Polar (F10), or temporary Shift-constrain while drafting.
    fn constrain_pick(&self, p: Point2, shift: bool) -> Point2 {
        if !(self.ortho || self.polar || shift) {
            return p;
        }
        let Some(a) = self.tool_anchor() else {
            return p;
        };
        let dx = (p.x - a.x).abs();
        let dy = (p.y - a.y).abs();
        if dx >= dy {
            Point2::new(p.x, a.y)
        } else {
            Point2::new(a.x, p.y)
        }
    }

    /// Resolve a world cursor: object snap first, then Ortho/Shift.
    fn resolve_pick(&mut self, raw: Point2, shift: bool) -> Point2 {
        let mut p = raw;
        self.snap_hit = None;
        if self.osnap {
            let tol = SNAP_PIXELS / self.camera.scale;
            if let Some(hit) = find_osnap(self.session.document(), raw, tol) {
                self.snap_hit = Some(hit);
                p = hit.1;
            }
        }
        if self.drafting() {
            self.constrain_pick(p, shift)
        } else {
            p
        }
    }

    /// Commit the dynamic-input buffer: empty finishes/repeats; a number during a
    /// tool is direct distance entry (along the cursor direction); otherwise it is
    /// a command token, dispatched like a command line.
    fn commit_dyn(&mut self) {
        // Absolute X/Y from Tab fields while drafting a point.
        if self.expects_point_coords() {
            if let Some(p) = parse_dyn_xy(&self.dyn_x, &self.dyn_y) {
                self.clear_dyn_coords();
                self.dyn_input.clear();
                self.on_pick(p);
                return;
            }
            let combined = if !self.dyn_input.is_empty() {
                self.dyn_input.clone()
            } else if !self.dyn_x.is_empty() && self.dyn_y.is_empty() {
                self.dyn_x.clone()
            } else {
                format!("{},{}", self.dyn_x, self.dyn_y)
            };
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
            if let Tool::Brush {
                points: Some(ref pts),
            } = self.tool
            {
                if pts.len() >= 2 {
                    let coords: Vec<String> = pts
                        .iter()
                        .flat_map(|q| [q.x.to_string(), q.y.to_string()])
                        .collect();
                    self.session.feed(&format!(
                        "POLYLINE {} A-PRES-MARK",
                        coords.join(" ")
                    ));
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
            } else if self.drafting() {
                self.tool = Tool::Select;
            } else if let Some(l) = self.last_line.clone() {
                self.dispatch(&l);
            }
            return;
        }
        // OFFSET / FILLET wait for a typed distance/radius before picks.
        if let Tool::Offset {
            dist: None,
            entity: None,
        } = self.tool
        {
            if let Ok(d) = text.parse::<f64>() {
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
        if let Tool::Fillet { radius: None, first: None } = self.tool {
            if let Ok(r) = text.parse::<f64>() {
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
            if let Ok(t) = text.parse::<f64>() {
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
        if let Tool::Mleader {
            arrow: Some(a),
            landing: Some(l),
            text_pt: Some(t),
        } = self.tool
        {
            self.session.feed(&format!(
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
            self.session.feed(&format!(
                "CALLOUT {} {} {} {} {}",
                a.x, a.y, c.x, c.y, label
            ));
            self.tool = Tool::Select;
            return;
        }
        // Direct distance entry: a bare number while drawing places the next point
        // that far along the crosshair direction from the anchor.
        if self.drafting() {
            if let Ok(dist) = text.parse::<f64>() {
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
            self.plot_notes =
                aadt_docs::notes_to_text(&aadt_docs::NotesLibrary::standard().suggest(&det));
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
        if let Some(def) = self.engine.start(id) {
            let tool_token = def.tool_token;
            let cmd_id = def.id;
            let has_sel = !self.selection.is_empty();
            match tool_for(tool_token, has_sel) {
                Some(t) => {
                    if matches!(
                        t,
                        Tool::Move { .. }
                            | Tool::Copy { .. }
                            | Tool::Rotate { .. }
                            | Tool::Scale { .. }
                            | Tool::Mirror { .. }
                            | Tool::Stretch { .. }
                    ) {
                        self.sync_selection();
                    }
                    self.tool = t;
                    self.last_line = Some(tool_token.to_string());
                    self.clear_dyn_coords();
                    self.dyn_input.clear();
                    self.note = format!(
                        "{} — {}",
                        cmd_id.to_ascii_uppercase(),
                        self.engine.prompt().unwrap_or("ready")
                    );
                }
                None => {
                    self.engine.cancel();
                    self.note = format!("{} — select objects first", cmd_id.to_ascii_uppercase());
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
                    self.session
                        .feed(&format!("EATTEDIT {id} AADT-ELEM wall"));
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
            Tool::Brush {
                points: Some(pts),
            } if pts.len() >= 2 => {
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
        if self.has_typed_dyn() {
            self.commit_dyn();
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
            resolve_alias(&filter)
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
                    .map(|d| (d.id, d.aliases.first().copied().unwrap_or(d.id)))
                    .collect();
                for (id, alias) in candidates {
                    let label = format!("{}  ({})", id.to_ascii_uppercase(), alias);
                    if ui.selectable_label(false, label).clicked() {
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
        let l = self.current_layer.trim();
        if l.is_empty() {
            "0".to_string()
        } else {
            l.to_string()
        }
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
        self.session.feed(&format!("SELECT {}", ids.join(" ")));
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
        if !line.contains(' ') {
            let up = line.to_ascii_uppercase();
            if matches!(up.as_str(), "E" | "ERASE") {
                self.erase_selection();
                return;
            }
            if matches!(up.as_str(), "LA" | "LAYER") {
                self.show_layer_mgr = true;
                self.right_tab = 0;
                self.note = "Layer Properties Manager".into();
                return;
            }
            if matches!(up.as_str(), "PR" | "PROPERTIES") {
                self.show_layer_mgr = true;
                self.right_tab = 3;
                self.props_edit = self.session.document().properties().clone();
                self.note = "Drawing Property Manager".into();
                return;
            }
            if matches!(up.as_str(), "Z" | "ZOOM") {
                // Bare Z → zoom extents (window zoom via Z W or tool after ZOOM W).
                self.apply_nav_hint("extents");
                self.note = "Zoom extents (type ZOOM W for window)".into();
                return;
            }
            if matches!(up.as_str(), "P" | "PAN") {
                self.note = "Pan — drag with the middle mouse button".into();
                return;
            }
            if let Some(id) = resolve_alias(line) {
                self.start_command_id(id);
                return;
            }
            if let Some(t) = tool_for(line, !self.selection.is_empty()) {
                if matches!(
                    t,
                    Tool::Move { .. }
                        | Tool::Copy { .. }
                        | Tool::Rotate { .. }
                        | Tool::Scale { .. }
                        | Tool::Mirror { .. }
                        | Tool::Stretch { .. }
                ) {
                    self.sync_selection();
                }
                self.tool = t;
                return;
            }
            if matches!(
                up.as_str(),
                "M" | "MOVE" | "CO" | "CP" | "COPY" | "RO" | "ROTATE" | "SC" | "SCALE"
                    | "S" | "STRETCH" | "MI" | "MIRROR"
            ) {
                self.note = "Select objects first, then run the edit.".to_string();
                return;
            }
        }
        let fb = self.session.feed(line);
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
            self.zoom_prev = Some(self.camera.clone());
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
                self.zoom_prev = Some(self.camera.clone());
                self.camera.scale *= f;
            }
            return;
        }
        if let Some(rest) = hint.strip_prefix("window:") {
            let parts: Vec<f64> = rest.split(',').filter_map(|s| s.parse().ok()).collect();
            if parts.len() == 4 {
                self.zoom_prev = Some(self.camera.clone());
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
        if self.selection.is_empty() {
            self.note = "Select objects first, then E.".to_string();
            return;
        }
        self.sync_selection();
        self.session.feed("ERASE SELECTION");
        self.selection.clear();
    }

    /// Advance the active tool with a picked world point.
    fn on_pick(&mut self, p: Point2) {
        let layer = self.layer();
        let tool = self.tool.clone();
        match tool {
            Tool::Line { last } => {
                if let Some(a) = last {
                    self.session
                        .feed(&format!("LINE {} {} {} {} {}", a.x, a.y, p.x, p.y, layer));
                }
                self.tool = Tool::Line { last: Some(p) };
                self.engine.accept_point();
            }
            Tool::Circle { center } => match center {
                Some(c) => {
                    let r = c.distance(p);
                    if r > f64::EPSILON {
                        self.session
                            .feed(&format!("CIRCLE {} {} {} {}", c.x, c.y, r, layer));
                    }
                    self.tool = Tool::Select;
                    self.engine.accept_point();
                    self.engine.complete();
                }
                None => {
                    self.tool = Tool::Circle { center: Some(p) };
                    self.engine.accept_point();
                }
            },
            Tool::Arc { center, start } => match (center, start) {
                (None, _) => {
                    self.tool = Tool::Arc {
                        center: Some(p),
                        start: None,
                    }
                }
                (Some(c), None) => {
                    self.tool = Tool::Arc {
                        center: Some(c),
                        start: Some(p),
                    }
                }
                (Some(c), Some(s)) => {
                    let r = c.distance(s);
                    if r > f64::EPSILON {
                        let start_ang = (s.y - c.y).atan2(s.x - c.x).to_degrees();
                        let end_ang = (p.y - c.y).atan2(p.x - c.x).to_degrees();
                        self.session.feed(&format!(
                            "ARC {} {} {} {} {} {}",
                            c.x, c.y, r, start_ang, end_ang, layer
                        ));
                    }
                    self.tool = Tool::Select;
                }
            },
            Tool::Rectangle { first } => match first {
                Some(a) => {
                    self.session.feed(&format!(
                        "RECTANGLE {} {} {} {} {}",
                        a.x, a.y, p.x, p.y, layer
                    ));
                    self.tool = Tool::Select;
                    self.engine.accept_point();
                    self.engine.complete();
                }
                None => {
                    self.tool = Tool::Rectangle { first: Some(p) };
                    self.engine.accept_point();
                }
            },
            Tool::Move { base } => match base {
                Some(b) => {
                    self.session
                        .feed(&format!("MOVE SELECTION {} {}", p.x - b.x, p.y - b.y));
                    self.tool = Tool::Select;
                    self.engine.accept_point();
                    self.engine.complete();
                }
                None => {
                    self.tool = Tool::Move { base: Some(p) };
                    self.engine.accept_point();
                }
            },
            Tool::Copy { base } => match base {
                // COPY repeats: each click drops another copy until Esc/Space.
                Some(b) => {
                    self.session
                        .feed(&format!("COPY SELECTION {} {}", p.x - b.x, p.y - b.y));
                    self.engine.accept_point();
                }
                None => {
                    self.tool = Tool::Copy { base: Some(p) };
                    self.engine.accept_point();
                }
            },
            Tool::Rotate { pivot } => match pivot {
                Some(piv) => {
                    let ang = (p.y - piv.y).atan2(p.x - piv.x).to_degrees();
                    self.session
                        .feed(&format!("ROTATE SELECTION {} {} {}", piv.x, piv.y, ang));
                    self.tool = Tool::Select;
                }
                None => self.tool = Tool::Rotate { pivot: Some(p) },
            },
            Tool::Scale { pivot } => match pivot {
                Some(piv) => {
                    let factor = piv.distance(p);
                    if factor > f64::EPSILON {
                        self.session
                            .feed(&format!("SCALE SELECTION {} {} {}", piv.x, piv.y, factor));
                    }
                    self.tool = Tool::Select;
                }
                None => self.tool = Tool::Scale { pivot: Some(p) },
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
            Tool::Stretch { w0, w1, base } => match (w0, w1, base) {
                (None, _, _) => {
                    self.tool = Tool::Stretch {
                        w0: Some(p),
                        w1: None,
                        base: None,
                    }
                }
                (Some(a), None, _) => {
                    self.tool = Tool::Stretch {
                        w0: Some(a),
                        w1: Some(p),
                        base: None,
                    }
                }
                (Some(a), Some(b), None) => {
                    self.tool = Tool::Stretch {
                        w0: Some(a),
                        w1: Some(b),
                        base: Some(p),
                    }
                }
                (Some(a), Some(b), Some(c)) => {
                    self.sync_selection();
                    let lo = Point2::new(a.x.min(b.x), a.y.min(b.y));
                    let hi = Point2::new(a.x.max(b.x), a.y.max(b.y));
                    self.session.feed(&format!(
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
                match first {
                    None => {
                        self.tool = Tool::Fillet {
                            radius: Some(r),
                            first: Some(id),
                        }
                    }
                    Some(a) => {
                        self.session
                            .feed(&format!("FILLET {} {} {}", a.0, id.0, r));
                        self.tool = Tool::Select;
                    }
                }
            }
            Tool::Insert { name } => {
                let layer = self.layer();
                self.session.feed(&format!(
                    "INSERT {} {} {} 0 {}",
                    name, p.x, p.y, layer
                ));
                // Stay in insert mode for multiple placements (AutoCAD-like).
                self.note = format!("INSERT {name} — click to place another (Esc finishes)");
            }
            Tool::Polyline { mut points } => {
                points.push(p);
                self.tool = Tool::Polyline { points };
                self.engine.accept_point();
                self.note = "POLYLINE — click next vertex (Space finishes)".into();
            }
            Tool::Wall {
                thickness: None,
                ..
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
                let layer = kind.layer();
                let block = kind.block_name();
                let fb = self.session.feed(&format!(
                    "INSERT {} {} {} 0 {}",
                    block, p.x, p.y, layer
                ));
                let id = parse_entity_id_from_feedback(fb.message()).or_else(|| {
                    // Fallback: small rectangle if INSERT failed.
                    let s = 0.45;
                    let fb2 = self.session.feed(&format!(
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
                let tol = PICK_PIXELS / self.camera.scale;
                if let Some(id) = pick_entity(self.session.document(), p, tol) {
                    let pat = self.hatch_pattern.clone();
                    self.session.feed(&format!("HATCH {} {}", id.0, pat));
                    self.tool = Tool::Select;
                } else {
                    self.note = format!(
                        "HATCH ({}) — click a closed polyline or circle",
                        self.hatch_pattern
                    );
                }
            }
            Tool::Trim { cutter } => {
                let tol = PICK_PIXELS / self.camera.scale;
                let Some(id) = pick_entity(self.session.document(), p, tol) else {
                    self.note = "TRIM — click an object".into();
                    return;
                };
                match cutter {
                    None => {
                        self.tool = Tool::Trim {
                            cutter: Some(id),
                        };
                        self.note = "TRIM — click the object to trim (on the side to remove)".into();
                    }
                    Some(c) => {
                        self.session
                            .feed(&format!("TRIM {} {} {} {}", id.0, c.0, p.x, p.y));
                        self.tool = Tool::Select;
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
                        self.tool = Tool::Extend {
                            boundary: Some(id),
                        };
                        self.note = "EXTEND — click near the end to lengthen".into();
                    }
                    Some(b) => {
                        self.session
                            .feed(&format!("EXTEND {} {} {} {}", id.0, b.0, p.x, p.y));
                        self.tool = Tool::Select;
                    }
                }
            }
            Tool::Mirror { a } => match a {
                None => self.tool = Tool::Mirror { a: Some(p) },
                Some(a0) => {
                    self.sync_selection();
                    self.session.feed(&format!(
                        "MIRROR SELECTION {} {} {} {}",
                        a0.x, a0.y, p.x, p.y
                    ));
                    self.tool = Tool::Select;
                }
            },
            Tool::Explode => {
                let tol = PICK_PIXELS / self.camera.scale;
                if let Some(id) = pick_entity(self.session.document(), p, tol) {
                    self.session.feed(&format!("EXPLODE {}", id.0));
                    self.tool = Tool::Select;
                } else {
                    self.note = "EXPLODE — click a block reference".into();
                }
            }
            Tool::Dim { first } => match first {
                None => self.tool = Tool::Dim { first: Some(p) },
                Some(a) => {
                    self.session.feed(&format!(
                        "DIM {} {} {} {} A-ANNO-DIMS",
                        a.x, a.y, p.x, p.y
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
                        self.tool = Tool::MatchProp {
                            source: Some(id),
                        };
                        self.note = "MATCHPROP — click target object(s)".into();
                    }
                    Some(src) => {
                        self.session
                            .feed(&format!("MATCHPROP {} {}", src.0, id.0));
                        self.tool = Tool::MatchProp {
                            source: Some(src),
                        };
                        self.note = "MATCHPROP — click another target (Esc finishes)".into();
                    }
                }
            }
            Tool::ZoomWindow { first } => match first {
                None => self.tool = Tool::ZoomWindow { first: Some(p) },
                Some(a) => {
                    self.apply_nav_hint(&format!("window:{},{},{},{}", a.x, a.y, p.x, p.y));
                    self.tool = Tool::Select;
                    self.note = "Zoom window".into();
                }
            },
            Tool::SectionLine { first } => match first {
                None => self.tool = Tool::SectionLine { first: Some(p) },
                Some(a) => {
                    self.session.feed(&format!(
                        "SECTIONLINE {} {} {} {} A",
                        a.x, a.y, p.x, p.y
                    ));
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
                    self.session.feed(&format!(
                        "REVCLOUD {} {} {} {}",
                        a.x, a.y, p.x, p.y
                    ));
                    self.tool = Tool::Select;
                }
            },
            Tool::TablePlace => {
                let rows = self.table_rows.parse::<u32>().unwrap_or(5).clamp(1, 200);
                let cols = self.table_cols.parse::<u32>().unwrap_or(4).clamp(1, 50);
                self.session.feed(&format!(
                    "TABLE {} {} {} {}",
                    p.x, p.y, rows, cols
                ));
                self.tool = Tool::Select;
            }
            Tool::Offset { .. } | Tool::Select => {}
        }
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

    /// The prompt shown while a tool is active.
    fn tool_prompt(&self) -> Option<&'static str> {
        if let Some(p) = self.engine.prompt() {
            return Some(p);
        }
        Some(match &self.tool {
            Tool::Line { last: None } => "LINE — click the start point (Esc cancels)",
            Tool::Line { last: Some(_) } => "LINE — next point (Space finish · Double-Space repeat)",
            Tool::Circle { center: None } => "CIRCLE — click the centre",
            Tool::Circle { center: Some(_) } => "CIRCLE — click a point on the circle",
            Tool::Arc { center: None, .. } => "ARC — click the centre",
            Tool::Arc {
                center: Some(_),
                start: None,
            } => "ARC — click the start point",
            Tool::Arc {
                center: Some(_),
                start: Some(_),
            } => "ARC — click the end point",
            Tool::Rectangle { first: None } => "RECTANGLE — click the first corner",
            Tool::Rectangle { first: Some(_) } => "RECTANGLE — click the opposite corner",
            Tool::Move { base: None } => "MOVE — click the base point",
            Tool::Move { base: Some(_) } => "MOVE — click the destination",
            Tool::Copy { base: None } => "COPY — click the base point",
            Tool::Copy { base: Some(_) } => "COPY — click each destination (Space finishes)",
            Tool::Rotate { pivot: None } => "ROTATE — click the pivot",
            Tool::Rotate { pivot: Some(_) } => "ROTATE — click to set the angle",
            Tool::Scale { pivot: None } => "SCALE — click the base point",
            Tool::Scale { pivot: Some(_) } => "SCALE — click to set the factor (distance)",
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
            Tool::Stretch { w0: None, .. } => "STRETCH — click first window corner",
            Tool::Stretch {
                w0: Some(_),
                w1: None,
                ..
            } => "STRETCH — click opposite window corner",
            Tool::Stretch {
                w1: Some(_),
                base: None,
                ..
            } => "STRETCH — click the base point",
            Tool::Stretch { base: Some(_), .. } => "STRETCH — click the destination",
            Tool::Brush { .. } => "BRUSH — click/drag freehand; Space finishes (A-PRES-MARK)",
            Tool::Fillet { radius: None, .. } => "FILLET — type radius and Space (0 = sharp)",
            Tool::Fillet {
                radius: Some(_),
                first: None,
            } => "FILLET — click the first line",
            Tool::Fillet {
                radius: Some(_),
                first: Some(_),
            } => "FILLET — click the second line",
            Tool::Insert { .. } => "INSERT — click to place the block",
            Tool::Polyline { .. } => "POLYLINE — click vertices (Space finishes)",
            Tool::Wall { thickness: None, .. } => "WALL — type thickness and Space",
            Tool::Wall {
                thickness: Some(_), ..
            } => "WALL — pick points (Space finish · Double-Space repeat)",
            Tool::PlaceElem { kind } => match kind {
                PlaceKind::Door => "DOOR — click insertion point",
                PlaceKind::Window => "WINDOW — click insertion point",
                PlaceKind::Column => "COLUMN — click insertion point",
                PlaceKind::Beam => "BEAM — click insertion point",
                PlaceKind::Slab => "SLAB — click insertion point",
            },
            Tool::Hatch => "HATCH — click a closed boundary",
            Tool::Trim { cutter: None } => "TRIM — click the cutting edge",
            Tool::Trim { cutter: Some(_) } => "TRIM — click object on the side to remove",
            Tool::Extend { boundary: None } => "EXTEND — click the boundary edge",
            Tool::Extend {
                boundary: Some(_),
            } => "EXTEND — click near the end to lengthen",
            Tool::Mirror { a: None } => "MIRROR — click first axis point",
            Tool::Mirror { a: Some(_) } => "MIRROR — click second axis point",
            Tool::Explode => "EXPLODE — click a block reference",
            Tool::Dim { first: None } => "DIM — click first point",
            Tool::Dim { first: Some(_) } => "DIM — click second point",
            Tool::MatchProp { source: None } => "MATCHPROP — click source object",
            Tool::MatchProp { source: Some(_) } => "MATCHPROP — click target object",
            Tool::ZoomWindow { first: None } => "ZOOM — click first corner",
            Tool::ZoomWindow { first: Some(_) } => "ZOOM — click opposite corner",
            Tool::SectionLine { first: None } => "SECTIONLINE — click first end",
            Tool::SectionLine { first: Some(_) } => "SECTIONLINE — click second end",
            Tool::Callout { arrow: None, .. } => "CALLOUT — click arrow point",
            Tool::Callout {
                arrow: Some(_),
                center: None,
            } => "CALLOUT — click bubble centre",
            Tool::Callout {
                arrow: Some(_),
                center: Some(_),
            } => "CALLOUT — type text and Space",
            Tool::RevMark => "REVMARK — click insertion point",
            Tool::RevCloud { first: None } => "REVCLOUD — click first corner",
            Tool::RevCloud { first: Some(_) } => "REVCLOUD — click opposite corner",
            Tool::TablePlace => "TABLE — click upper-left corner",
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
            lines.push(match kind {
                SnapKind::Endpoint => "END".to_string(),
                SnapKind::Midpoint => "MID".to_string(),
            });
        }
        lines
    }
}

impl eframe::App for AadtApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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
                                    if let Some(path) = rfd::FileDialog::new()
                                        .add_filter("AADT", &["aadt"])
                                        .add_filter("DXF", &["dxf"])
                                        .set_file_name("drawing.aadt")
                                        .save_file()
                                    {
                                        self.session.feed(&format!("SAVE {}", path.display()));
                                    }
                                }
                                egui::Key::O => {
                                    if let Some(path) = rfd::FileDialog::new()
                                        .add_filter("AADT / DXF", &["aadt", "dxf"])
                                        .pick_file()
                                    {
                                        self.session.feed(&format!("OPEN {}", path.display()));
                                        self.fitted = false;
                                        self.selection.clear();
                                    }
                                }
                                egui::Key::N => {
                                    let mut d = aadt_model::Document::new();
                                    aadt_standards::apply_standard_layers(&mut d);
                                    self.session.set_document(d);
                                    self.selection.clear();
                                    self.fitted = false;
                                    self.note = "New drawing".into();
                                }
                                egui::Key::Z => {
                                    self.session.feed("UNDO");
                                }
                                egui::Key::Y => {
                                    self.session.feed("REDO");
                                }
                                egui::Key::C => {
                                    if !self.selection.is_empty() {
                                        self.sync_selection();
                                        self.session.feed("COPYCLIP SELECTION");
                                    }
                                }
                                egui::Key::X => {
                                    if !self.selection.is_empty() {
                                        self.sync_selection();
                                        self.session.feed("CUTCLIP SELECTION");
                                        self.selection.clear();
                                    }
                                }
                                egui::Key::V => {
                                    self.session.feed("PASTECLIP");
                                    self.selection =
                                        self.session.selection().to_vec();
                                }
                                egui::Key::P => {
                                    self.props_edit =
                                        self.session.document().properties().clone();
                                    self.seed_plot_dialog();
                                }
                                egui::Key::Num1 => {
                                    self.show_layer_mgr = true;
                                    self.right_tab = 3;
                                    self.props_edit =
                                        self.session.document().properties().clone();
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
                            } else {
                                self.pop_dyn_char();
                            }
                        }
                        egui::Key::Escape => {
                            self.dyn_input.clear();
                            self.clear_dyn_coords();
                            self.palette_open = false;
                            self.engine.cancel();
                            if self.drafting() {
                                self.tool = Tool::Select;
                            } else {
                                self.selection.clear();
                            }
                        }
                        egui::Key::F8 => {
                            self.ortho = !self.ortho;
                            self.note = if self.ortho {
                                "Ortho ON (F8)".to_string()
                            } else {
                                "Ortho OFF (F8)".to_string()
                            };
                        }
                        egui::Key::F3 => {
                            self.osnap = !self.osnap;
                            self.snap_hit = None;
                            self.note = if self.osnap {
                                "Object snap ON (F3) — endpoint / midpoint".to_string()
                            } else {
                                "Object snap OFF (F3)".to_string()
                            };
                        }
                        egui::Key::F10 => {
                            self.polar = !self.polar;
                            self.note = if self.polar {
                                "Polar tracking ON (F10)".into()
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
                            self.note = if self.osnap_track {
                                "Object-snap tracking ON (F11)".into()
                            } else {
                                "Object-snap tracking OFF (F11)".into()
                            };
                        }
                        _ => {}
                    }
                    }
                    _ => {}
                }
            }
        }

        // AutoCAD-like chrome: QAT → layer mgr → command+status → canvas.
        self.draw_top_bar(ctx);
        self.draw_bottom_bar(ctx);
        self.draw_right_panel(ctx);
        self.draw_plot_dialog(ctx);
        self.draw_command_palette(ctx);

        // --- Central viewport (white paper = print preview) ---
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(egui::Color32::from_rgb(250, 250, 248)))
            .show(ctx, |ui| {
            // Viewport label (AutoCAD-style).
            ui.painter().text(
                ui.min_rect().left_top() + egui::vec2(8.0, 6.0),
                egui::Align2::LEFT_TOP,
                "[-][Top][2D Wireframe]",
                egui::FontId::proportional(11.0),
                egui::Color32::from_rgb(80, 80, 80),
            );

            let (rect, resp) =
                ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
            self.camera.viewport = (rect.width() as f64, rect.height() as f64);

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
                    if resp.drag_started() {
                        self.box_start = resp.interact_pointer_pos();
                    }
                    if resp.drag_stopped() {
                        if let (Some(s), Some(e)) = (self.box_start, resp.interact_pointer_pos()) {
                            self.finish_box(s, e, &to_world, ui);
                        }
                        self.box_start = None;
                    }
                    if resp.clicked() {
                        if let Some(pos) = resp.interact_pointer_pos() {
                            self.click_select(to_world(&self.camera, pos), ui);
                        }
                    }
                }
                _ => {
                    if resp.secondary_clicked() {
                        self.apply_space_single();
                    }
                    let place = if resp.clicked() || resp.drag_stopped() {
                        resp.interact_pointer_pos()
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

            let list = self.session.drawlist();
            let painter = ui.painter_at(rect);
            paint(
                &painter,
                rect,
                &self.camera,
                &list,
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

            // Rubber-band preview for the in-progress tool.
            if let Some(pos) = self.cursor {
                paint_preview(&painter, rect, &self.camera, &self.tool, pos);
            }
            // Object-snap marker at the acquired point.
            if let Some((kind, sp)) = self.snap_hit {
                paint_snap_marker(&painter, rect, &self.camera, kind, sp);
            }
            // The selection box while dragging.
            if let (Some(s), Some(cur)) = (self.box_start, resp.hover_pos()) {
                paint_box(&painter, s, cur);
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
            // Full-viewport crosshair + the dynamic-input box, both painted (not
            // interactive) so they follow the cursor every frame and never
            // intercept a selection click.
            if let Some(sp) = resp.hover_pos() {
                let cross = egui::Stroke::new(0.5_f32, egui::Color32::from_gray(180));
                painter.line_segment(
                    [egui::pos2(rect.min.x, sp.y), egui::pos2(rect.max.x, sp.y)],
                    cross,
                );
                painter.line_segment(
                    [egui::pos2(sp.x, rect.min.y), egui::pos2(sp.x, rect.max.y)],
                    cross,
                );
                paint_dyn_box(&painter, sp, &self.dyn_lines());
            }
            // Keep the crosshair, rubber-band, and box tracking the cursor.
            if resp.hovered() {
                ctx.request_repaint();
            }
        });
    }
}

impl AadtApp {
    /// Pick or toggle the entity under `world`; an empty click clears (unless
    /// Shift is held, which only adds/removes).
    fn click_select(&mut self, world: Point2, ui: &egui::Ui) {
        let shift = ui.input(|i| i.modifiers.shift);
        let tol = PICK_PIXELS / self.camera.scale;
        match pick_entity(self.session.document(), world, tol) {
            Some(id) => {
                if shift {
                    if let Some(i) = self.selection.iter().position(|&x| x == id) {
                        self.selection.remove(i);
                    } else {
                        self.selection.push(id);
                    }
                } else {
                    self.selection = vec![id];
                }
            }
            None => {
                if !shift {
                    self.selection.clear();
                }
            }
        }
    }

    /// Finalise a box selection from screen anchor `s` to release `e`: left→right
    /// is a window (encloses), right→left a crossing (touches). Shift adds.
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
        }
    }
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

/// Shortest distance from `p` to the entity's drawn geometry (world units).
fn entity_distance(g: &Geometry, p: Point2) -> f64 {
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

/// The nearest entity within `tol` of `p`, if any.
fn pick_entity(doc: &Document, p: Point2, tol: f64) -> Option<EntityId> {
    let mut best: Option<(f64, EntityId)> = None;
    for e in doc.entities() {
        let d = entity_distance(&e.geometry, p);
        if d <= tol && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, e.id));
        }
    }
    best.map(|(_, id)| id)
}

/// A bounding box around a geometry's defining points.
fn entity_bbox(g: &Geometry) -> Option<Bbox> {
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

/// Entities selected by a window (fully enclosed) or crossing (touching) box.
fn box_select(doc: &Document, lo: Point2, hi: Point2, crossing: bool) -> Vec<EntityId> {
    doc.entities()
        .iter()
        .filter_map(|e| {
            let bb = entity_bbox(&e.geometry)?;
            let enclosed =
                bb.min.x >= lo.x && bb.min.y >= lo.y && bb.max.x <= hi.x && bb.max.y <= hi.y;
            let touches =
                !(bb.max.x < lo.x || bb.min.x > hi.x || bb.max.y < lo.y || bb.min.y > hi.y);
            ((crossing && touches) || (!crossing && enclosed)).then_some(e.id)
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

/// Paint a small END (square) or MID (triangle) marker at the snap point.
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
    }
}

// --- Painting ----------------------------------------------------------------

fn world_to_screen(camera: &Camera, rect: egui::Rect, p: Point2) -> egui::Pos2 {
    let (sx, sy) = camera.world_to_screen(p);
    egui::pos2(rect.min.x + sx as f32, rect.min.y + sy as f32)
}

/// Paint the draw list into `rect` with egui's 2-D painter using layer ACI
/// colours and print lineweights scaled for the white-paper preview.
fn paint(
    painter: &egui::Painter,
    rect: egui::Rect,
    camera: &Camera,
    list: &DrawList,
    doc: &Document,
    show_lweights: bool,
) {
    for cmd in &list.commands {
        let (color, stroke) = layer_stroke(doc, cmd.layer(), camera, show_lweights);
        draw_command(painter, rect, camera, cmd, stroke, color);
    }
}

/// Highlight the selected entities and draw their grips.
fn paint_selection(
    painter: &egui::Painter,
    rect: egui::Rect,
    camera: &Camera,
    doc: &Document,
    selection: &[EntityId],
) {
    let hi = egui::Color32::from_rgb(255, 210, 70);
    let grip = egui::Color32::from_rgb(90, 170, 255);
    for id in selection {
        let Some(e) = doc.get(*id) else { continue };
        draw_geometry(
            painter,
            rect,
            camera,
            &e.geometry,
            egui::Stroke::new(2.0_f32, hi),
            hi,
        );
        for gp in grip_points(&e.geometry) {
            let c = world_to_screen(camera, rect, gp);
            let s = 3.0;
            painter.rect_filled(
                egui::Rect::from_center_size(c, egui::vec2(s * 2.0, s * 2.0)),
                0.0,
                grip,
            );
        }
    }
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
            painter.text(
                to_screen(t.position),
                egui::Align2::LEFT_BOTTOM,
                &t.value,
                egui::FontId::proportional(11.0),
                color,
            );
        }
        Geometry::Insert(i) => {
            let c = to_screen(i.position);
            let d = 4.0;
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
            position, value, ..
        } => {
            painter.text(
                to_screen(*position),
                egui::Align2::LEFT_BOTTOM,
                value,
                egui::FontId::proportional(11.0),
                color,
            );
        }
        DrawCommand::Marker { position, .. } => {
            let c = to_screen(*position);
            let d = 4.0;
            painter.line_segment([c + egui::vec2(-d, 0.0), c + egui::vec2(d, 0.0)], stroke);
            painter.line_segment([c + egui::vec2(0.0, -d), c + egui::vec2(0.0, d)], stroke);
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
) {
    let to_screen = |p: Point2| world_to_screen(camera, rect, p);
    let stroke = egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(30, 90, 180));
    match tool {
        Tool::Line { last: Some(a) } => {
            painter.line_segment([to_screen(*a), to_screen(cursor)], stroke);
        }
        Tool::Polyline { points } if !points.is_empty() => {
            for pair in points.windows(2) {
                painter.line_segment([to_screen(pair[0]), to_screen(pair[1])], stroke);
            }
            if let Some(&last) = points.last() {
                painter.line_segment([to_screen(last), to_screen(cursor)], stroke);
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
        Tool::Brush {
            points: Some(pts),
        } => {
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
        Tool::Circle { center: Some(c) } => {
            painter.circle_stroke(
                to_screen(*c),
                (c.distance(cursor) * camera.scale) as f32,
                stroke,
            );
        }
        Tool::Rectangle { first: Some(a) } => {
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
        Tool::Arc {
            center: Some(c),
            start: None,
        } => {
            painter.line_segment([to_screen(*c), to_screen(cursor)], stroke);
        }
        Tool::Arc {
            center: Some(c),
            start: Some(s),
        } => {
            let r = c.distance(*s);
            painter.circle_stroke(to_screen(*c), (r * camera.scale) as f32, stroke);
            painter.line_segment([to_screen(*c), to_screen(cursor)], stroke);
        }
        Tool::Move { base: Some(b) }
        | Tool::Copy { base: Some(b) }
        | Tool::Rotate { pivot: Some(b) }
        | Tool::Scale { pivot: Some(b) }
        | Tool::Mirror { a: Some(b) }
        | Tool::Dim { first: Some(b) }
        | Tool::ZoomWindow { first: Some(b) }
        | Tool::SectionLine { first: Some(b) }
        | Tool::RevCloud { first: Some(b) } => {
            painter.line_segment([to_screen(*b), to_screen(cursor)], stroke);
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
        _ => {}
    }
}

/// The selection rectangle while dragging: blue (window) or green (crossing).
fn paint_box(painter: &egui::Painter, start: egui::Pos2, cur: egui::Pos2) {
    let crossing = cur.x < start.x;
    let (edge, fill) = if crossing {
        (
            egui::Color32::from_rgb(120, 220, 120),
            egui::Color32::from_rgba_unmultiplied(120, 220, 120, 30),
        )
    } else {
        (
            egui::Color32::from_rgb(120, 170, 255),
            egui::Color32::from_rgba_unmultiplied(120, 170, 255, 30),
        )
    };
    let r = egui::Rect::from_two_pos(start, cur);
    painter.rect_filled(r, 0.0, fill);
    painter.rect_stroke(r, 0.0, egui::Stroke::new(1.0_f32, edge));
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
    use super::{parse_comma_xy, parse_dyn_xy};
    use aadt_geometry::Point2;

    #[test]
    fn parse_dyn_xy_both_fields() {
        let p = parse_dyn_xy("10.5", "-2").unwrap();
        assert!((p.x - 10.5).abs() < 1e-9);
        assert!((p.y + 2.0).abs() < 1e-9);
    }

    #[test]
    fn parse_comma_xy_accepts_pair() {
        assert_eq!(
            parse_comma_xy("3,4"),
            Some(Point2::new(3.0, 4.0))
        );
        assert!(parse_comma_xy("3").is_none());
        assert!(parse_comma_xy("a,b").is_none());
    }
}
