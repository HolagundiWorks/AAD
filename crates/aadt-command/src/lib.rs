//! # aadt-command — Command interpreter (substrate layer, spec §4.2)
//!
//! The single-line command interpreter that is AADT's primary UI surface (spec §2,
//! principle 2). This crate provides the framework — a [`Command`] trait, a
//! case-insensitive [`Registry`] with aliases, a [`Context`] carrying the working
//! [`Document`], and an [`Interpreter`] that evaluates lines and scripts — plus the
//! generic drawing built-ins that need only the document model.
//!
//! Layering is deliberate: the command **core** depends only on `aadt-model`. The
//! AADT engine and file-I/O commands (`TAKEOFF`, `OPEN`, `SAVE`) are registered by
//! higher layers (the CLI / app) that depend on the engine crates, so the substrate
//! never depends on the flagship engine. The registry is also the seam the M6 LISP
//! engine will register commands through.

use aadt_geometry::{Arc, Circle, Point2, Polyline, Segment};
use aadt_model::{Dimension, DimensionKind, Document, EntityId, Geometry, Hatch, Insert, Text};

/// Strip a single leading UTF-8 byte-order mark (U+FEFF), if present.
///
/// `str::trim` does not consider the BOM whitespace, so scripts and lines saved
/// by editors that prepend a BOM must be cleaned explicitly before tokenizing.
fn strip_bom(s: &str) -> &str {
    s.strip_prefix('\u{feff}').unwrap_or(s)
}

/// A command handler resolvable by name or alias.
pub trait Command {
    /// Canonical command name (matched case-insensitively).
    fn name(&self) -> &str;

    /// Alternate names for the command (matched case-insensitively).
    fn aliases(&self) -> &'static [&'static str] {
        &[]
    }

    /// One-line description for help output.
    fn help(&self) -> &str {
        ""
    }

    /// Execute against the session context. `Ok` carries a line of user feedback.
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String>;
}

/// Session state a command operates on: the working document plus command history.
#[derive(Debug, Default)]
pub struct Context {
    pub doc: Document,
    pub history: Vec<String>,
}

impl Context {
    pub fn new() -> Self {
        Self::default()
    }
}

/// A case-insensitive registry of commands keyed by name and alias.
#[derive(Default)]
pub struct Registry {
    commands: Vec<Box<dyn Command>>,
    index: std::collections::HashMap<String, usize>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a command under its name and every alias. Later registrations
    /// override earlier keys, allowing extensions to shadow built-ins.
    pub fn register(&mut self, command: Box<dyn Command>) {
        let idx = self.commands.len();
        self.index.insert(command.name().to_lowercase(), idx);
        for alias in command.aliases() {
            self.index.insert(alias.to_lowercase(), idx);
        }
        self.commands.push(command);
    }

    /// Resolve a token (case-insensitively) to a command.
    pub fn resolve(&self, token: &str) -> Option<&dyn Command> {
        self.index
            .get(&token.to_lowercase())
            .map(|&i| self.commands[i].as_ref())
    }

    /// All registered commands, in registration order.
    pub fn commands(&self) -> impl Iterator<Item = &dyn Command> {
        self.commands.iter().map(|c| c.as_ref())
    }

    /// Tab-completion candidates for `prefix`: every command name and alias whose
    /// spelling starts with `prefix` (case-insensitively), returned in their
    /// canonical casing, sorted and de-duplicated.
    ///
    /// A pure function of the registry contents — no session state — so the
    /// interactive prompt, a LISP layer, and tests can all share it. An empty
    /// prefix lists every completable token.
    pub fn complete(&self, prefix: &str) -> Vec<String> {
        let needle = prefix.to_lowercase();
        let mut out: Vec<String> = self
            .commands
            .iter()
            .flat_map(|c| std::iter::once(c.name()).chain(c.aliases().iter().copied()))
            .filter(|token| token.to_lowercase().starts_with(&needle))
            .map(str::to_string)
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }
}

/// Build a registry pre-populated with the generic drawing built-ins.
pub fn builtins() -> Registry {
    let mut r = Registry::new();
    // Draw.
    r.register(Box::new(LineCmd));
    r.register(Box::new(PolylineCmd));
    r.register(Box::new(RectangleCmd));
    r.register(Box::new(CircleCmd));
    r.register(Box::new(ArcCmd));
    r.register(Box::new(TextCmd));
    r.register(Box::new(InsertCmd));
    // Modify.
    r.register(Box::new(MoveCmd));
    r.register(Box::new(CopyCmd));
    r.register(Box::new(RotateCmd));
    r.register(Box::new(ScaleCmd));
    r.register(Box::new(MirrorCmd));
    r.register(Box::new(OffsetCmd));
    r.register(Box::new(TrimCmd));
    r.register(Box::new(ExtendCmd));
    r.register(Box::new(FilletCmd));
    r.register(Box::new(ChamferCmd));
    r.register(Box::new(ArrayCmd));
    r.register(Box::new(ExplodeCmd));
    r.register(Box::new(EraseCmd));
    // Organize.
    r.register(Box::new(LayerCmd));
    r.register(Box::new(LayersCmd));
    // Annotate.
    r.register(Box::new(DimCmd));
    r.register(Box::new(HatchCmd));
    // History and inspection.
    r.register(Box::new(UndoCmd));
    r.register(Box::new(RedoCmd));
    r.register(Box::new(ListCmd));
    r.register(Box::new(DistCmd));
    r.register(Box::new(AreaCmd));
    r
}

/// Evaluates command lines and scripts against a [`Context`].
pub struct Interpreter {
    registry: Registry,
}

impl Interpreter {
    pub fn new(registry: Registry) -> Self {
        Self { registry }
    }

    /// An interpreter with the generic drawing built-ins registered.
    pub fn with_builtins() -> Self {
        Self::new(builtins())
    }

    /// Mutable access to the registry, so higher layers can register more commands.
    pub fn registry_mut(&mut self) -> &mut Registry {
        &mut self.registry
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Evaluate one line. Blank lines and comments (`#` or `;`) yield `Ok(None)`.
    ///
    /// A leading UTF-8 BOM (U+FEFF) is stripped before trimming. `str::trim` does
    /// **not** treat the BOM as whitespace, so a `.aadt` script saved by a Windows
    /// editor (which prepends the BOM to line 1) would otherwise fail with
    /// `unknown command "\u{feff}OPEN"`. `run_script` also strips it once up front;
    /// stripping here as well makes single-line evaluation robust on its own.
    pub fn eval_line(&self, ctx: &mut Context, line: &str) -> Result<Option<String>, String> {
        let trimmed = strip_bom(line).trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
            return Ok(None);
        }
        let tokens: Vec<String> = trimmed.split_whitespace().map(str::to_string).collect();
        let (name, args) = tokens.split_first().expect("non-empty after trim check");
        let command = self
            .registry
            .resolve(name)
            .ok_or_else(|| format!("unknown command {name:?}"))?;
        ctx.history.push(trimmed.to_string());
        command.run(ctx, args).map(Some)
    }

    /// Evaluate a whole script, returning each command's feedback line. Aborts on
    /// the first error, reporting the offending 1-based line number.
    ///
    /// A leading UTF-8 BOM on the whole script is stripped once, up front, so it
    /// never reaches tokenization on line 1.
    pub fn run_script(&self, ctx: &mut Context, script: &str) -> Result<Vec<String>, String> {
        let script = strip_bom(script);
        let mut output = Vec::new();
        for (n, line) in script.lines().enumerate() {
            match self.eval_line(ctx, line) {
                Ok(Some(msg)) => output.push(msg),
                Ok(None) => {}
                Err(e) => return Err(format!("line {}: {e}", n + 1)),
            }
        }
        Ok(output)
    }
}

// --- Built-in drawing commands (model-only) ----------------------------------

fn parse_f64(args: &[String], i: usize, flag: &str) -> Result<f64, String> {
    args.get(i)
        .ok_or_else(|| format!("{flag}: missing argument {}", i + 1))?
        .parse::<f64>()
        .map_err(|_| format!("{flag}: argument {} is not a number", i + 1))
}

fn parse_entity_id(raw: &str, flag: &str) -> Result<EntityId, String> {
    raw.parse::<u64>()
        .map(EntityId)
        .map_err(|_| format!("{flag}: {raw:?} is not an entity id"))
}

fn parse_usize(args: &[String], i: usize, flag: &str) -> Result<usize, String> {
    args.get(i)
        .ok_or_else(|| format!("{flag}: missing argument {}", i + 1))?
        .parse::<usize>()
        .map_err(|_| format!("{flag}: argument {} is not a non-negative integer", i + 1))
}

/// Translate a piece of geometry by `(dx, dy)`, returning a moved copy.
///
/// Used by MOVE and COPY. Kept here (not in `aadt-model`, which Track B consumes
/// read-only) so the command layer owns its own edit semantics.
fn translate_geometry(g: &Geometry, dx: f64, dy: f64) -> Geometry {
    let d = aadt_geometry::Vec2::new(dx, dy);
    match g {
        Geometry::Line(s) => Geometry::Line(Segment::new(s.a + d, s.b + d)),
        Geometry::Polyline(p) => Geometry::Polyline(Polyline::new(
            p.vertices.iter().map(|v| *v + d).collect(),
            p.closed,
        )),
        Geometry::Circle(c) => Geometry::Circle(Circle::new(c.center + d, c.radius)),
        Geometry::Arc(a) => {
            Geometry::Arc(Arc::new(a.center + d, a.radius, a.start_angle, a.end_angle))
        }
        Geometry::Text(t) => Geometry::Text(Text::new(
            t.value.clone(),
            t.position + d,
            t.height,
            t.rotation,
        )),
        Geometry::Insert(ins) => Geometry::Insert(Insert::new(
            ins.name.clone(),
            ins.position + d,
            ins.rotation,
        )),
    }
}

struct PolylineCmd;
impl Command for PolylineCmd {
    fn name(&self) -> &str {
        "POLYLINE"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["PLINE", "PL"]
    }
    fn help(&self) -> &str {
        "POLYLINE x1 y1 x2 y2 [...] [CLOSED] [layer] — add a polyline"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        // Optional trailing tokens, in either order: `CLOSED` and a layer name.
        // Everything else is a flat list of x y pairs.
        let mut closed = false;
        let mut layer: Option<String> = None;
        let mut nums: Vec<f64> = Vec::new();
        for tok in args {
            if tok.eq_ignore_ascii_case("CLOSED") {
                closed = true;
            } else if let Ok(n) = tok.parse::<f64>() {
                nums.push(n);
            } else if layer.is_none() {
                layer = Some(tok.clone());
            } else {
                return Err(format!("POLYLINE: unexpected argument {tok:?}"));
            }
        }
        if nums.len() < 4 || !nums.len().is_multiple_of(2) {
            return Err("POLYLINE: need at least two x y pairs".to_string());
        }
        let verts: Vec<Point2> = nums
            .chunks_exact(2)
            .map(|c| Point2::new(c[0], c[1]))
            .collect();
        let n = verts.len();
        let layer = layer.unwrap_or_else(|| "0".to_string());
        let id = ctx.doc.transact("POLYLINE", |d| {
            d.add_geometry(layer, Geometry::Polyline(Polyline::new(verts, closed)))
        });
        Ok(format!(
            "POLYLINE ({n} vertices{}) added as entity {}",
            if closed { ", closed" } else { "" },
            id.0
        ))
    }
}

struct CircleCmd;
impl Command for CircleCmd {
    fn name(&self) -> &str {
        "CIRCLE"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["C"]
    }
    fn help(&self) -> &str {
        "CIRCLE x y r [layer] — add a circle"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let x = parse_f64(args, 0, "CIRCLE")?;
        let y = parse_f64(args, 1, "CIRCLE")?;
        let r = parse_f64(args, 2, "CIRCLE")?;
        if r <= 0.0 {
            return Err("CIRCLE: radius must be positive".to_string());
        }
        let layer = args.get(3).cloned().unwrap_or_else(|| "0".to_string());
        let id = ctx.doc.transact("CIRCLE", |d| {
            d.add_geometry(layer, Geometry::Circle(Circle::new(Point2::new(x, y), r)))
        });
        Ok(format!("CIRCLE added as entity {}", id.0))
    }
}

struct ArcCmd;
impl Command for ArcCmd {
    fn name(&self) -> &str {
        "ARC"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["A"]
    }
    fn help(&self) -> &str {
        "ARC cx cy r start end [layer] — add an arc (angles in degrees, CCW)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let cx = parse_f64(args, 0, "ARC")?;
        let cy = parse_f64(args, 1, "ARC")?;
        let r = parse_f64(args, 2, "ARC")?;
        if r <= 0.0 {
            return Err("ARC: radius must be positive".to_string());
        }
        let start = parse_f64(args, 3, "ARC")?;
        let end = parse_f64(args, 4, "ARC")?;
        let layer = args.get(5).cloned().unwrap_or_else(|| "0".to_string());
        let id = ctx.doc.transact("ARC", |d| {
            d.add_geometry(
                layer,
                Geometry::Arc(Arc::new(Point2::new(cx, cy), r, start, end)),
            )
        });
        Ok(format!("ARC added as entity {}", id.0))
    }
}

struct TextCmd;
impl Command for TextCmd {
    fn name(&self) -> &str {
        "TEXT"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["DTEXT", "T"]
    }
    fn help(&self) -> &str {
        "TEXT x y height value... — add single-line text (value is the rest of the line)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let x = parse_f64(args, 0, "TEXT")?;
        let y = parse_f64(args, 1, "TEXT")?;
        let height = parse_f64(args, 2, "TEXT")?;
        if height <= 0.0 {
            return Err("TEXT: height must be positive".to_string());
        }
        // The value is everything after the height, joined — so labels may
        // contain spaces without quoting.
        let value = args.get(3..).unwrap_or(&[]).join(" ");
        if value.is_empty() {
            return Err("TEXT: missing text value".to_string());
        }
        let id = ctx.doc.transact("TEXT", |d| {
            d.add_geometry(
                "0",
                Geometry::Text(Text::new(value, Point2::new(x, y), height, 0.0)),
            )
        });
        Ok(format!("TEXT added as entity {}", id.0))
    }
}

struct InsertCmd;
impl Command for InsertCmd {
    fn name(&self) -> &str {
        "INSERT"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["I"]
    }
    fn help(&self) -> &str {
        "INSERT name x y [rotation] [layer] — place a block reference"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let name = args.first().ok_or("INSERT: missing block name")?.clone();
        let x = parse_f64(args, 1, "INSERT")?;
        let y = parse_f64(args, 2, "INSERT")?;
        let rotation = match args.get(3) {
            Some(r) => r
                .parse::<f64>()
                .map_err(|_| format!("INSERT: rotation {r:?} is not a number"))?,
            None => 0.0,
        };
        let layer = args.get(4).cloned().unwrap_or_else(|| "0".to_string());
        let id = ctx.doc.transact("INSERT", |d| {
            d.add_geometry(
                layer,
                Geometry::Insert(Insert::new(name, Point2::new(x, y), rotation)),
            )
        });
        Ok(format!("INSERT added as entity {}", id.0))
    }
}

struct RectangleCmd;
impl Command for RectangleCmd {
    fn name(&self) -> &str {
        "RECTANGLE"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["REC", "RECTANG"]
    }
    fn help(&self) -> &str {
        "RECTANGLE x1 y1 x2 y2 [layer] — add a closed rectangle by opposite corners"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let x1 = parse_f64(args, 0, "RECTANGLE")?;
        let y1 = parse_f64(args, 1, "RECTANGLE")?;
        let x2 = parse_f64(args, 2, "RECTANGLE")?;
        let y2 = parse_f64(args, 3, "RECTANGLE")?;
        if (x1 - x2).abs() <= f64::EPSILON || (y1 - y2).abs() <= f64::EPSILON {
            return Err("RECTANGLE: corners must span a non-zero width and height".to_string());
        }
        let layer = args.get(4).cloned().unwrap_or_else(|| "0".to_string());
        let corners = vec![
            Point2::new(x1, y1),
            Point2::new(x2, y1),
            Point2::new(x2, y2),
            Point2::new(x1, y2),
        ];
        let id = ctx.doc.transact("RECTANGLE", |d| {
            d.add_geometry(layer, Geometry::Polyline(Polyline::new(corners, true)))
        });
        Ok(format!("RECTANGLE added as entity {}", id.0))
    }
}

struct MoveCmd;
impl Command for MoveCmd {
    fn name(&self) -> &str {
        "MOVE"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["M"]
    }
    fn help(&self) -> &str {
        "MOVE id dx dy — translate an entity in place, keeping its id (UNDO restores)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let raw = args.first().ok_or("MOVE: missing entity id")?;
        let id = parse_entity_id(raw, "MOVE")?;
        let dx = parse_f64(args, 1, "MOVE")?;
        let dy = parse_f64(args, 2, "MOVE")?;
        // `translate_entity` records a single undoable in-place edit that
        // preserves the entity's id, layer, and attributes — no remove+re-add,
        // so any reference to the id survives a move. Returns false for no such id.
        let moved = ctx.doc.transact("MOVE", |d| d.translate_entity(id, dx, dy));
        if moved {
            Ok(format!("moved entity {}", id.0))
        } else {
            Err(format!("MOVE: no entity with id {}", id.0))
        }
    }
}

struct CopyCmd;
impl Command for CopyCmd {
    fn name(&self) -> &str {
        "COPY"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["CP", "CO"]
    }
    fn help(&self) -> &str {
        "COPY id dx dy — duplicate an entity, translated by (dx, dy)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let raw = args.first().ok_or("COPY: missing entity id")?;
        let id = parse_entity_id(raw, "COPY")?;
        let dx = parse_f64(args, 1, "COPY")?;
        let dy = parse_f64(args, 2, "COPY")?;
        let existing = ctx
            .doc
            .get(id)
            .ok_or_else(|| format!("COPY: no entity with id {}", id.0))?;
        let layer = existing.layer.clone();
        let clone = translate_geometry(&existing.geometry, dx, dy);
        let new_id = ctx.doc.transact("COPY", |d| d.add_geometry(layer, clone));
        Ok(format!("copied entity {} as entity {}", id.0, new_id.0))
    }
}

struct RotateCmd;
impl Command for RotateCmd {
    fn name(&self) -> &str {
        "ROTATE"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["RO"]
    }
    fn help(&self) -> &str {
        "ROTATE id cx cy angle — rotate an entity about (cx, cy) by angle degrees (CCW), keeping its id"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let raw = args.first().ok_or("ROTATE: missing entity id")?;
        let id = parse_entity_id(raw, "ROTATE")?;
        let cx = parse_f64(args, 1, "ROTATE")?;
        let cy = parse_f64(args, 2, "ROTATE")?;
        let deg = parse_f64(args, 3, "ROTATE")?;
        let pivot = Point2::new(cx, cy);
        let done = ctx
            .doc
            .transact("ROTATE", |d| d.rotate_entity(id, pivot, deg.to_radians()));
        if done {
            Ok(format!("rotated entity {} by {deg}°", id.0))
        } else {
            Err(format!("ROTATE: no entity with id {}", id.0))
        }
    }
}

struct ScaleCmd;
impl Command for ScaleCmd {
    fn name(&self) -> &str {
        "SCALE"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["SC"]
    }
    fn help(&self) -> &str {
        "SCALE id cx cy factor — scale an entity about (cx, cy) by factor, keeping its id"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let raw = args.first().ok_or("SCALE: missing entity id")?;
        let id = parse_entity_id(raw, "SCALE")?;
        let cx = parse_f64(args, 1, "SCALE")?;
        let cy = parse_f64(args, 2, "SCALE")?;
        let factor = parse_f64(args, 3, "SCALE")?;
        if factor == 0.0 {
            return Err("SCALE: factor must be non-zero".to_string());
        }
        let pivot = Point2::new(cx, cy);
        let done = ctx
            .doc
            .transact("SCALE", |d| d.scale_entity(id, pivot, factor));
        if done {
            Ok(format!("scaled entity {} by {factor}", id.0))
        } else {
            Err(format!("SCALE: no entity with id {}", id.0))
        }
    }
}

struct MirrorCmd;
impl Command for MirrorCmd {
    fn name(&self) -> &str {
        "MIRROR"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["MI"]
    }
    fn help(&self) -> &str {
        "MIRROR id x1 y1 x2 y2 — reflect an entity across the line (x1,y1)-(x2,y2), keeping its id"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let raw = args.first().ok_or("MIRROR: missing entity id")?;
        let id = parse_entity_id(raw, "MIRROR")?;
        let x1 = parse_f64(args, 1, "MIRROR")?;
        let y1 = parse_f64(args, 2, "MIRROR")?;
        let x2 = parse_f64(args, 3, "MIRROR")?;
        let y2 = parse_f64(args, 4, "MIRROR")?;
        let a = Point2::new(x1, y1);
        let b = Point2::new(x2, y2);
        if a.coincident(b) {
            return Err("MIRROR: the two axis points must differ".to_string());
        }
        let done = ctx.doc.transact("MIRROR", |d| d.mirror_entity(id, a, b));
        if done {
            Ok(format!("mirrored entity {}", id.0))
        } else {
            Err(format!("MIRROR: no entity with id {}", id.0))
        }
    }
}

struct OffsetCmd;
impl Command for OffsetCmd {
    fn name(&self) -> &str {
        "OFFSET"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["O"]
    }
    fn help(&self) -> &str {
        "OFFSET id dist [sx sy] — copy an entity offset by dist (toward (sx,sy) if given)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let raw = args.first().ok_or("OFFSET: missing entity id")?;
        let id = parse_entity_id(raw, "OFFSET")?;
        let dist = parse_f64(args, 1, "OFFSET")?;
        let side = if args.len() >= 4 {
            Some(Point2::new(
                parse_f64(args, 2, "OFFSET")?,
                parse_f64(args, 3, "OFFSET")?,
            ))
        } else {
            None
        };
        let existing = ctx
            .doc
            .get(id)
            .ok_or_else(|| format!("OFFSET: no entity with id {}", id.0))?;
        let layer = existing.layer.clone();
        let offset = existing.geometry.offset(dist, side).ok_or_else(|| {
            format!(
                "OFFSET: cannot offset entity {} (needs a line, polyline, circle, or arc, and a non-degenerate result)",
                id.0
            )
        })?;
        let new_id = ctx
            .doc
            .transact("OFFSET", |d| d.add_geometry(layer, offset));
        Ok(format!("offset entity {} as entity {}", id.0, new_id.0))
    }
}

/// Fetch an entity that must be a line — CHAMFER/FILLET work on straight
/// segments only.
fn require_line(ctx: &Context, id: EntityId, cmd: &str) -> Result<Segment, String> {
    match ctx.doc.get(id) {
        Some(e) => match &e.geometry {
            Geometry::Line(s) => Ok(*s),
            _ => Err(format!("{cmd}: entity {} is not a line", id.0)),
        },
        None => Err(format!("{cmd}: no entity with id {}", id.0)),
    }
}

/// Fetch a copy of an entity's geometry (TRIM/EXTEND accept lines and arcs).
fn require_geometry(ctx: &Context, id: EntityId, cmd: &str) -> Result<Geometry, String> {
    ctx.doc
        .get(id)
        .map(|e| e.geometry.clone())
        .ok_or_else(|| format!("{cmd}: no entity with id {}", id.0))
}

/// Choose start/end so the CCW arc from start to end is the minor (≤180°) arc —
/// the rounded corner a FILLET wants, not its reflex complement.
fn minor_arc(a1: f64, a2: f64) -> (f64, f64) {
    if (a2 - a1).rem_euclid(360.0) <= 180.0 {
        (a1, a2)
    } else {
        (a2, a1)
    }
}

/// Tolerance for deciding a point lies on a shape.
const HIT_EPS: f64 = 1e-6;

/// The endpoint of `s` farther from `x` — the part FILLET/CHAMFER keep.
fn far_end(x: Point2, s: Segment) -> Point2 {
    if x.distance(s.a) >= x.distance(s.b) {
        s.a
    } else {
        s.b
    }
}

/// Is `p` on the *bounded* shape (line segment, arc sweep, or full circle)?
fn point_on_shape(g: &Geometry, p: Point2) -> bool {
    match g {
        Geometry::Line(s) => s.distance_to_point(p) <= HIT_EPS,
        Geometry::Arc(a) => a.contains_point(p),
        Geometry::Circle(c) => (p.distance(c.center) - c.radius).abs() <= HIT_EPS,
        _ => false,
    }
}

/// Intersection points of the two shapes' *unbounded* curves (a line as its
/// infinite line, an arc/circle as its full circle). Supports line/arc/circle
/// pairs; arc-vs-arc/circle is not modelled and yields nothing.
fn curve_crossings(a: &Geometry, b: &Geometry) -> Vec<Point2> {
    use Geometry::{Arc, Circle, Line};
    match (a, b) {
        (Line(s1), Line(s2)) => s1
            .line_intersection(*s2)
            .map(|(p, _)| p)
            .into_iter()
            .collect(),
        (Line(s), Arc(ar)) | (Arc(ar), Line(s)) => ar.circle().line_intersections(*s),
        (Line(s), Circle(c)) | (Circle(c), Line(s)) => c.line_intersections(*s),
        _ => Vec::new(),
    }
}

/// Crossings that lie on *both* bounded shapes — where a cutter genuinely crosses
/// the target, for TRIM.
fn bounded_crossings(target: &Geometry, cutter: &Geometry) -> Vec<Point2> {
    curve_crossings(target, cutter)
        .into_iter()
        .filter(|&p| point_on_shape(target, p) && point_on_shape(cutter, p))
        .collect()
}

/// Crossings of the target's *unbounded* curve with the bounded boundary — where
/// an extended target would meet the boundary, for EXTEND.
fn boundary_crossings(target: &Geometry, boundary: &Geometry) -> Vec<Point2> {
    curve_crossings(target, boundary)
        .into_iter()
        .filter(|&p| point_on_shape(boundary, p))
        .collect()
}

/// The point in `pts` nearest `p`.
fn nearest_to(pts: &[Point2], p: Point2) -> Option<Point2> {
    pts.iter()
        .copied()
        .min_by(|a, b| a.distance(p).total_cmp(&b.distance(p)))
}

/// Crossings of a single poly-line edge with a cutter shape (bounded on both).
fn segment_crossings(seg: Segment, cutter: &Geometry) -> Vec<Point2> {
    bounded_crossings(&Geometry::Line(seg), cutter)
}

/// Distance along an open poly-line from its first vertex to vertex `k`.
fn length_to_vertex(verts: &[Point2], k: usize) -> f64 {
    verts[..=k].windows(2).map(|w| w[0].distance(w[1])).sum()
}

/// Arc-length position of `p`'s nearest projection onto an open poly-line — used
/// to decide which side of a trim the pick point falls on.
fn path_param(verts: &[Point2], p: Point2) -> f64 {
    let mut cum = 0.0;
    let mut best = (f64::INFINITY, 0.0);
    for w in verts.windows(2) {
        let seg = Segment::new(w[0], w[1]);
        let d = seg.distance_to_point(p);
        if d < best.0 {
            let along = match seg.direction() {
                Some(dir) => (p - w[0]).dot(dir).clamp(0.0, seg.length()),
                None => 0.0,
            };
            best = (d, cum + along);
        }
        cum += seg.length();
    }
    best.1
}

struct TrimCmd;
impl Command for TrimCmd {
    fn name(&self) -> &str {
        "TRIM"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["TR"]
    }
    fn help(&self) -> &str {
        "TRIM id cutter px py — trim a line, arc, or polyline at its crossing with another entity, dropping the piece at (px, py)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let id = parse_entity_id(args.first().ok_or("TRIM: missing entity id")?, "TRIM")?;
        let cutter_id = parse_entity_id(args.get(1).ok_or("TRIM: missing cutter id")?, "TRIM")?;
        let pick = Point2::new(parse_f64(args, 2, "TRIM")?, parse_f64(args, 3, "TRIM")?);
        let target = require_geometry(ctx, id, "TRIM")?;
        let cutter = require_geometry(ctx, cutter_id, "TRIM")?;

        // The crossing nearest the pick, for the whole-shape (line/arc) targets.
        let nearest_crossing = || {
            nearest_to(&bounded_crossings(&target, &cutter), pick)
                .ok_or_else(|| "TRIM: the cutter does not cross the entity".to_string())
        };

        let new = match &target {
            Geometry::Line(s) => {
                let x = nearest_crossing()?;
                // Keep the side away from the pick point.
                let kept = if s.project_param(pick) < s.project_param(x) {
                    Segment::new(x, s.b)
                } else {
                    Segment::new(s.a, x)
                };
                if kept.length() <= HIT_EPS {
                    return Err("TRIM: nothing would be left of the line".to_string());
                }
                Geometry::Line(kept)
            }
            Geometry::Arc(a) => {
                let x = nearest_crossing()?;
                // The crossing splits the sweep in two; drop the half the pick is on.
                let theta = a.angle_of(x);
                let first_half = Arc::new(a.center, a.radius, a.start_angle, theta);
                if first_half.contains_angle(a.angle_of(pick)) {
                    Geometry::Arc(Arc::new(a.center, a.radius, theta, a.end_angle))
                } else {
                    Geometry::Arc(first_half)
                }
            }
            Geometry::Polyline(p) => {
                if p.closed {
                    return Err("TRIM: closed polylines are not supported".to_string());
                }
                let verts = &p.vertices;
                // The crossing (edge index, point) nearest the pick.
                let mut hits: Vec<(usize, Point2)> = Vec::new();
                for (k, w) in verts.windows(2).enumerate() {
                    for x in segment_crossings(Segment::new(w[0], w[1]), &cutter) {
                        hits.push((k, x));
                    }
                }
                let (k, x) = hits
                    .into_iter()
                    .min_by(|a, b| a.1.distance(pick).total_cmp(&b.1.distance(pick)))
                    .ok_or("TRIM: the cutter does not cross the entity")?;
                // Keep the run of the path away from the pick, cut at the crossing.
                let x_pos = length_to_vertex(verts, k) + verts[k].distance(x);
                let new_verts: Vec<Point2> = if path_param(verts, pick) < x_pos {
                    std::iter::once(x)
                        .chain(verts[k + 1..].iter().copied())
                        .collect()
                } else {
                    verts[..=k]
                        .iter()
                        .copied()
                        .chain(std::iter::once(x))
                        .collect()
                };
                if new_verts.len() < 2 {
                    return Err("TRIM: nothing would be left of the polyline".to_string());
                }
                Geometry::Polyline(Polyline::new(new_verts, false))
            }
            _ => {
                return Err(format!(
                    "TRIM: entity {} must be a line, arc, or polyline",
                    id.0
                ))
            }
        };
        ctx.doc.transact("TRIM", |d| d.replace_geometry(id, new));
        Ok(format!("trimmed entity {}", id.0))
    }
}

struct ExtendCmd;
impl Command for ExtendCmd {
    fn name(&self) -> &str {
        "EXTEND"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["EX"]
    }
    fn help(&self) -> &str {
        "EXTEND id boundary px py — extend a line or arc to meet a boundary entity, lengthening the end near (px, py)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let id = parse_entity_id(args.first().ok_or("EXTEND: missing entity id")?, "EXTEND")?;
        let bound_id =
            parse_entity_id(args.get(1).ok_or("EXTEND: missing boundary id")?, "EXTEND")?;
        let pick = Point2::new(parse_f64(args, 2, "EXTEND")?, parse_f64(args, 3, "EXTEND")?);
        let target = require_geometry(ctx, id, "EXTEND")?;
        let boundary = require_geometry(ctx, bound_id, "EXTEND")?;

        // Where the target's *extended* curve meets the boundary (whole-shape
        // targets; the polyline arm crosses only its terminal edge).
        let crossings = boundary_crossings(&target, &boundary);
        let eps = 1e-9;

        let new = match &target {
            Geometry::Line(s) => {
                let near_a = pick.distance(s.a) <= pick.distance(s.b);
                // The meeting point must be beyond the end being extended.
                let reach = crossings.iter().copied().filter(|&x| {
                    let t = s.project_param(x);
                    if near_a {
                        t < -eps
                    } else {
                        t > 1.0 + eps
                    }
                });
                let x = nearest_to(&reach.collect::<Vec<_>>(), pick)
                    .ok_or("EXTEND: the boundary is not beyond that end")?;
                if near_a {
                    Geometry::Line(Segment::new(x, s.b))
                } else {
                    Geometry::Line(Segment::new(s.a, x))
                }
            }
            Geometry::Arc(a) => {
                // Grow the sweep at the end nearer the pick out to the boundary.
                let extend_end = pick.distance(a.end_point()) <= pick.distance(a.start_point());
                let grown = crossings.iter().copied().filter_map(|x| {
                    let theta = a.angle_of(x);
                    let new_sweep = if extend_end {
                        (theta - a.start_angle).rem_euclid(360.0)
                    } else {
                        (a.end_angle - theta).rem_euclid(360.0)
                    };
                    // Only accept a genuine extension (the sweep must grow).
                    if new_sweep > a.sweep() + eps {
                        let arc = if extend_end {
                            Arc::new(a.center, a.radius, a.start_angle, theta)
                        } else {
                            Arc::new(a.center, a.radius, theta, a.end_angle)
                        };
                        Some((new_sweep, arc))
                    } else {
                        None
                    }
                });
                // Take the smallest growth (nearest reachable boundary).
                let arc = grown
                    .min_by(|x, y| x.0.total_cmp(&y.0))
                    .ok_or("EXTEND: the boundary is not beyond that end")?
                    .1;
                Geometry::Arc(arc)
            }
            Geometry::Polyline(p) => {
                if p.closed {
                    return Err("EXTEND: closed polylines have no free end".to_string());
                }
                let verts = &p.vertices;
                if verts.len() < 2 {
                    return Err("EXTEND: the polyline has no edge".to_string());
                }
                let n = verts.len();
                // Extend the terminal edge nearer the pick, moving its free vertex.
                let extend_end = pick.distance(verts[n - 1]) <= pick.distance(verts[0]);
                let edge = if extend_end {
                    Segment::new(verts[n - 2], verts[n - 1])
                } else {
                    Segment::new(verts[1], verts[0])
                };
                // Crossings of the terminal edge's line with the boundary, beyond
                // the free vertex.
                let reach: Vec<Point2> = boundary_crossings(&Geometry::Line(edge), &boundary)
                    .into_iter()
                    .filter(|&x| edge.project_param(x) > 1.0 + eps)
                    .collect();
                let x = nearest_to(&reach, pick)
                    .ok_or("EXTEND: the boundary is not beyond that end")?;
                let mut nv = verts.clone();
                if extend_end {
                    nv[n - 1] = x;
                } else {
                    nv[0] = x;
                }
                Geometry::Polyline(Polyline::new(nv, false))
            }
            _ => {
                return Err(format!(
                    "EXTEND: entity {} must be a line, arc, or polyline",
                    id.0
                ))
            }
        };
        ctx.doc.transact("EXTEND", |d| d.replace_geometry(id, new));
        Ok(format!("extended entity {}", id.0))
    }
}

struct FilletCmd;
impl Command for FilletCmd {
    fn name(&self) -> &str {
        "FILLET"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["F"]
    }
    fn help(&self) -> &str {
        "FILLET id1 id2 radius — round the corner between two lines with an arc (radius 0 just trims to the corner)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let id1 = parse_entity_id(args.first().ok_or("FILLET: missing first id")?, "FILLET")?;
        let id2 = parse_entity_id(args.get(1).ok_or("FILLET: missing second id")?, "FILLET")?;
        let radius = parse_f64(args, 2, "FILLET")?;
        if radius < 0.0 {
            return Err("FILLET: radius must be non-negative".to_string());
        }
        let l1 = require_line(ctx, id1, "FILLET")?;
        let l2 = require_line(ctx, id2, "FILLET")?;
        let layer1 = ctx.doc.get(id1).expect("line exists").layer.clone();

        let (x, _) = l1
            .line_intersection(l2)
            .ok_or("FILLET: the lines are parallel")?;
        // The kept part of each line runs from its far end to the tangent point.
        let far1 = far_end(x, l1);
        let far2 = far_end(x, l2);
        let u1 = (far1 - x)
            .normalized()
            .ok_or("FILLET: a line is degenerate")?;
        let u2 = (far2 - x)
            .normalized()
            .ok_or("FILLET: a line is degenerate")?;
        let half = u1.dot(u2).clamp(-1.0, 1.0).acos() / 2.0;
        if half.sin().abs() < 1e-9 {
            return Err("FILLET: the lines are collinear".to_string());
        }

        // Radius 0 is a plain corner: trim both lines to the intersection.
        if radius <= aadt_geometry::LENGTH_EPS {
            ctx.doc.transact("FILLET", |d| {
                d.replace_geometry(id1, Geometry::Line(Segment::new(far1, x)));
                d.replace_geometry(id2, Geometry::Line(Segment::new(far2, x)));
            });
            return Ok(format!(
                "filleted entities {} and {} (corner)",
                id1.0, id2.0
            ));
        }

        let d_tan = radius / half.tan();
        if d_tan > x.distance(far1) + 1e-9 || d_tan > x.distance(far2) + 1e-9 {
            return Err("FILLET: radius is too large for these segments".to_string());
        }
        let t1 = x + u1 * d_tan;
        let t2 = x + u2 * d_tan;
        let bisector = (u1 + u2)
            .normalized()
            .ok_or("FILLET: the lines are collinear")?;
        let center = x + bisector * (radius / half.sin());
        let ang = |p: Point2| {
            let v = p - center;
            v.y.atan2(v.x).to_degrees()
        };
        let (start, end) = minor_arc(ang(t1), ang(t2));

        ctx.doc.transact("FILLET", |d| {
            d.replace_geometry(id1, Geometry::Line(Segment::new(far1, t1)));
            d.replace_geometry(id2, Geometry::Line(Segment::new(far2, t2)));
            d.add_geometry(layer1, Geometry::Arc(Arc::new(center, radius, start, end)));
        });
        Ok(format!(
            "filleted entities {} and {} with r={radius}",
            id1.0, id2.0
        ))
    }
}

struct ChamferCmd;
impl Command for ChamferCmd {
    fn name(&self) -> &str {
        "CHAMFER"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["CHA"]
    }
    fn help(&self) -> &str {
        "CHAMFER id1 id2 dist1 [dist2] — bevel the corner between two lines (dist2 defaults to dist1)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let id1 = parse_entity_id(args.first().ok_or("CHAMFER: missing first id")?, "CHAMFER")?;
        let id2 = parse_entity_id(args.get(1).ok_or("CHAMFER: missing second id")?, "CHAMFER")?;
        let dist1 = parse_f64(args, 2, "CHAMFER")?;
        let dist2 = if args.len() >= 4 {
            parse_f64(args, 3, "CHAMFER")?
        } else {
            dist1
        };
        if dist1 < 0.0 || dist2 < 0.0 {
            return Err("CHAMFER: distances must be non-negative".to_string());
        }
        let l1 = require_line(ctx, id1, "CHAMFER")?;
        let l2 = require_line(ctx, id2, "CHAMFER")?;
        let layer1 = ctx.doc.get(id1).expect("line exists").layer.clone();

        let (x, _) = l1
            .line_intersection(l2)
            .ok_or("CHAMFER: the lines are parallel")?;
        let far1 = far_end(x, l1);
        let far2 = far_end(x, l2);
        let u1 = (far1 - x)
            .normalized()
            .ok_or("CHAMFER: a line is degenerate")?;
        let u2 = (far2 - x)
            .normalized()
            .ok_or("CHAMFER: a line is degenerate")?;
        if u1.dot(u2).abs() >= 1.0 - 1e-9 {
            return Err("CHAMFER: the lines are collinear".to_string());
        }
        if dist1 > x.distance(far1) + 1e-9 || dist2 > x.distance(far2) + 1e-9 {
            return Err("CHAMFER: a distance is too large for its segment".to_string());
        }
        // Trim each line to its bevel point, then join the two with a straight edge.
        let t1 = x + u1 * dist1;
        let t2 = x + u2 * dist2;
        ctx.doc.transact("CHAMFER", |d| {
            d.replace_geometry(id1, Geometry::Line(Segment::new(far1, t1)));
            d.replace_geometry(id2, Geometry::Line(Segment::new(far2, t2)));
            d.add_geometry(layer1, Geometry::Line(Segment::new(t1, t2)));
        });
        Ok(format!("chamfered entities {} and {}", id1.0, id2.0))
    }
}

struct ArrayCmd;
impl Command for ArrayCmd {
    fn name(&self) -> &str {
        "ARRAY"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["AR"]
    }
    fn help(&self) -> &str {
        "ARRAY id rows cols dx dy — rectangular array of copies (rows×cols, spacing dx/dy)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let raw = args.first().ok_or("ARRAY: missing entity id")?;
        let id = parse_entity_id(raw, "ARRAY")?;
        let rows = parse_usize(args, 1, "ARRAY")?;
        let cols = parse_usize(args, 2, "ARRAY")?;
        let dx = parse_f64(args, 3, "ARRAY")?;
        let dy = parse_f64(args, 4, "ARRAY")?;
        if rows == 0 || cols == 0 {
            return Err("ARRAY: rows and cols must be at least 1".to_string());
        }
        let existing = ctx
            .doc
            .get(id)
            .ok_or_else(|| format!("ARRAY: no entity with id {}", id.0))?;
        let layer = existing.layer.clone();
        let geometry = existing.geometry.clone();
        // Every grid cell except the original's (0,0) gets a translated copy.
        let offsets: Vec<(f64, f64)> = (0..rows)
            .flat_map(|i| (0..cols).map(move |j| (i, j)))
            .filter(|&(i, j)| !(i == 0 && j == 0))
            .map(|(i, j)| (j as f64 * dx, i as f64 * dy))
            .collect();
        let count = offsets.len();
        ctx.doc.transact("ARRAY", |d| {
            for (ox, oy) in &offsets {
                d.add_geometry(layer.clone(), translate_geometry(&geometry, *ox, *oy));
            }
        });
        Ok(format!("arrayed entity {} into {count} copies", id.0))
    }
}

struct ExplodeCmd;
impl Command for ExplodeCmd {
    fn name(&self) -> &str {
        "EXPLODE"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["X"]
    }
    fn help(&self) -> &str {
        "EXPLODE id — replace a block reference with its instantiated geometry"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let raw = args.first().ok_or("EXPLODE: missing entity id")?;
        let id = parse_entity_id(raw, "EXPLODE")?;
        let entity = ctx
            .doc
            .get(id)
            .ok_or_else(|| format!("EXPLODE: no entity with id {}", id.0))?;
        let Geometry::Insert(ins) = &entity.geometry else {
            return Err(format!("EXPLODE: entity {} is not a block reference", id.0));
        };
        let block = ctx
            .doc
            .block(&ins.name)
            .ok_or_else(|| format!("EXPLODE: block {:?} is not defined", ins.name))?;
        // Bake each block entity to world space, inheriting the insert's layer for
        // block-local layer "0" (the DXF convention the renderer also follows).
        let placed: Vec<(String, Geometry)> = block
            .entities
            .iter()
            .map(|be| {
                let layer = if be.layer == "0" {
                    entity.layer.clone()
                } else {
                    be.layer.clone()
                };
                (
                    layer,
                    be.geometry
                        .instanced(block.base, ins.position, ins.rotation),
                )
            })
            .collect();
        let count = placed.len();
        // One atomic, undoable step: drop the insert, add its baked geometry.
        ctx.doc.transact("EXPLODE", |d| {
            d.remove_entity(id);
            for (layer, geometry) in placed {
                d.add_geometry(layer, geometry);
            }
        });
        Ok(format!("exploded entity {} into {count} entities", id.0))
    }
}

struct LayersCmd;
impl Command for LayersCmd {
    fn name(&self) -> &str {
        "LAYERS"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["LAYERLIST"]
    }
    fn help(&self) -> &str {
        "LAYERS — list the drawing's layers"
    }
    fn run(&self, ctx: &mut Context, _args: &[String]) -> Result<String, String> {
        let mut layers: Vec<_> = ctx.doc.layers().collect();
        layers.sort_by(|a, b| a.name.cmp(&b.name));
        if layers.is_empty() {
            return Ok("no layers".to_string());
        }
        let mut lines = vec![format!("{} layer(s):", layers.len())];
        for l in layers {
            let mut flags = format!("color {}", l.color);
            if !l.visible {
                flags.push_str(", off");
            }
            if l.frozen {
                flags.push_str(", frozen");
            }
            lines.push(format!("  {} [{flags}]", l.name));
        }
        Ok(lines.join("\n"))
    }
}

struct LineCmd;
impl Command for LineCmd {
    fn name(&self) -> &str {
        "LINE"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["L"]
    }
    fn help(&self) -> &str {
        "LINE x1 y1 x2 y2 [layer] — add a line segment"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let x1 = parse_f64(args, 0, "LINE")?;
        let y1 = parse_f64(args, 1, "LINE")?;
        let x2 = parse_f64(args, 2, "LINE")?;
        let y2 = parse_f64(args, 3, "LINE")?;
        let layer = args.get(4).cloned().unwrap_or_else(|| "0".to_string());
        let seg = Segment::new(Point2::new(x1, y1), Point2::new(x2, y2));
        let id = ctx
            .doc
            .transact("LINE", |d| d.add_geometry(layer, Geometry::Line(seg)));
        Ok(format!("LINE added as entity {}", id.0))
    }
}

struct LayerCmd;
impl Command for LayerCmd {
    fn name(&self) -> &str {
        "LAYER"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["LA"]
    }
    fn help(&self) -> &str {
        "LAYER name [COLOR n] [ON|OFF] [FREEZE|THAW] — create or restyle a layer"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let name = args.first().ok_or("LAYER: missing name")?.clone();

        // Parse the optional state changes, in any order.
        let mut color: Option<u8> = None;
        let mut visible: Option<bool> = None;
        let mut frozen: Option<bool> = None;
        let mut it = args[1..].iter();
        while let Some(tok) = it.next() {
            match tok.to_ascii_uppercase().as_str() {
                "COLOR" | "COL" => {
                    let v = it.next().ok_or("LAYER: COLOR needs a number (1-255)")?;
                    color = Some(
                        v.parse::<u8>()
                            .ok()
                            .filter(|c| *c >= 1)
                            .ok_or_else(|| format!("LAYER: invalid color {v:?} (1-255)"))?,
                    );
                }
                "ON" => visible = Some(true),
                "OFF" => visible = Some(false),
                "FREEZE" => frozen = Some(true),
                "THAW" => frozen = Some(false),
                other => return Err(format!("LAYER: unknown option {other:?}")),
            }
        }

        ctx.doc.transact("LAYER", |d| {
            d.add_layer(aadt_model::Layer::new(name.clone()));
            if let Some(c) = color {
                d.set_layer_color(&name, c);
            }
            if let Some(v) = visible {
                d.set_layer_visible(&name, v);
            }
            if let Some(fr) = frozen {
                d.set_layer_frozen(&name, fr);
            }
        });

        // Report the resulting state.
        let l = ctx.doc.layer(&name).expect("layer exists after LAYER");
        let state = format!(
            "color {}, {}{}",
            l.color,
            if l.visible { "on" } else { "off" },
            if l.frozen { ", frozen" } else { "" },
        );
        Ok(format!("layer {name:?} ready ({state})"))
    }
}

struct EraseCmd;
impl Command for EraseCmd {
    fn name(&self) -> &str {
        "ERASE"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["E"]
    }
    fn help(&self) -> &str {
        "ERASE id — delete an entity by id"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let raw = args.first().ok_or("ERASE: missing entity id")?;
        let id = raw
            .parse::<u64>()
            .map_err(|_| format!("ERASE: {raw:?} is not an entity id"))?;
        let removed = ctx.doc.transact("ERASE", |d| d.remove_entity(EntityId(id)));
        match removed {
            Some(_) => Ok(format!("erased entity {id}")),
            None => Err(format!("no entity with id {id}")),
        }
    }
}

struct DistCmd;
impl Command for DistCmd {
    fn name(&self) -> &str {
        "DIST"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["DI"]
    }
    fn help(&self) -> &str {
        "DIST x1 y1 x2 y2 — report the distance between two points"
    }
    fn run(&self, _ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let a = Point2::new(parse_f64(args, 0, "DIST")?, parse_f64(args, 1, "DIST")?);
        let b = Point2::new(parse_f64(args, 2, "DIST")?, parse_f64(args, 3, "DIST")?);
        let d = a.distance(b);
        Ok(format!("distance {d} (dx {}, dy {})", b.x - a.x, b.y - a.y))
    }
}

struct AreaCmd;
impl Command for AreaCmd {
    fn name(&self) -> &str {
        "AREA"
    }
    fn help(&self) -> &str {
        "AREA id — report the enclosed area of a closed polyline or circle"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let raw = args.first().ok_or("AREA: missing entity id")?;
        let id = parse_entity_id(raw, "AREA")?;
        let entity = ctx
            .doc
            .get(id)
            .ok_or_else(|| format!("AREA: no entity with id {}", id.0))?;
        let area = match &entity.geometry {
            Geometry::Polyline(p) if p.closed => p.area(),
            Geometry::Circle(c) => std::f64::consts::PI * c.radius * c.radius,
            Geometry::Polyline(_) => {
                return Err(format!("AREA: entity {} is an open polyline", id.0))
            }
            _ => {
                return Err(format!(
                    "AREA: entity {} has no enclosed area (not a closed polyline or circle)",
                    id.0
                ))
            }
        };
        Ok(format!("area {area}"))
    }
}

/// Approximate a circle as a closed poly-line of `segments` chords — the boundary
/// a HATCH of a circular region fills.
fn circle_polygon(c: Circle, segments: usize) -> Polyline {
    let n = segments.max(3);
    let verts = (0..n)
        .map(|i| {
            let a = (i as f64 / n as f64) * std::f64::consts::TAU;
            Point2::new(
                c.center.x + c.radius * a.cos(),
                c.center.y + c.radius * a.sin(),
            )
        })
        .collect();
    Polyline::new(verts, true)
}

struct DimCmd;
impl Command for DimCmd {
    fn name(&self) -> &str {
        "DIM"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["DIMLINEAR"]
    }
    fn help(&self) -> &str {
        "DIM x1 y1 x2 y2 [layer] — add an aligned dimension between two points (undoable)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let p1 = Point2::new(parse_f64(args, 0, "DIM")?, parse_f64(args, 1, "DIM")?);
        let p2 = Point2::new(parse_f64(args, 2, "DIM")?, parse_f64(args, 3, "DIM")?);
        if p1.coincident(p2) {
            return Err("DIM: the two points must differ".to_string());
        }
        let layer = args.get(4).cloned().unwrap_or_else(|| "A-DIMS".to_string());
        let measurement = p1.distance(p2);
        let dim = Dimension {
            layer,
            kind: DimensionKind::Aligned,
            p1,
            p2,
            text_anchor: p1.midpoint(p2),
            text: String::new(),
            measurement,
        };
        ctx.doc.transact("DIM", |d| d.add_dimension(dim));
        Ok(format!("added dimension measuring {measurement}"))
    }
}

struct HatchCmd;
impl Command for HatchCmd {
    fn name(&self) -> &str {
        "HATCH"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["H"]
    }
    fn help(&self) -> &str {
        "HATCH id [pattern] [layer] — fill a closed polyline or circle (pattern default SOLID; undoable)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let raw = args.first().ok_or("HATCH: missing entity id")?;
        let id = parse_entity_id(raw, "HATCH")?;
        let entity = ctx
            .doc
            .get(id)
            .ok_or_else(|| format!("HATCH: no entity with id {}", id.0))?;
        let boundary = match &entity.geometry {
            Geometry::Polyline(p) if p.closed => p.clone(),
            Geometry::Circle(c) => circle_polygon(*c, 64),
            _ => {
                return Err(format!(
                    "HATCH: entity {} must be a closed polyline or a circle",
                    id.0
                ))
            }
        };
        let pattern = args.get(1).cloned().unwrap_or_else(|| "SOLID".to_string());
        let layer = args.get(2).cloned().unwrap_or_else(|| entity.layer.clone());
        let vertices = boundary.vertices.len();
        ctx.doc.transact("HATCH", |d| {
            d.add_hatch(Hatch::new(layer, boundary, pattern))
        });
        Ok(format!(
            "hatched entity {} ({vertices}-vertex boundary)",
            id.0
        ))
    }
}

struct UndoCmd;
impl Command for UndoCmd {
    fn name(&self) -> &str {
        "UNDO"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["U"]
    }
    fn help(&self) -> &str {
        "UNDO — revert the last command"
    }
    fn run(&self, ctx: &mut Context, _args: &[String]) -> Result<String, String> {
        match ctx.doc.undo() {
            Some(label) => Ok(format!("undo: {label}")),
            None => Err("nothing to undo".to_string()),
        }
    }
}

struct RedoCmd;
impl Command for RedoCmd {
    fn name(&self) -> &str {
        "REDO"
    }
    fn help(&self) -> &str {
        "REDO — replay the last undone command"
    }
    fn run(&self, ctx: &mut Context, _args: &[String]) -> Result<String, String> {
        match ctx.doc.redo() {
            Some(label) => Ok(format!("redo: {label}")),
            None => Err("nothing to redo".to_string()),
        }
    }
}

struct ListCmd;
impl Command for ListCmd {
    fn name(&self) -> &str {
        "LIST"
    }
    fn help(&self) -> &str {
        "LIST — summarize entities in the document"
    }
    fn run(&self, ctx: &mut Context, _args: &[String]) -> Result<String, String> {
        let mut lines = vec![format!("{} entities:", ctx.doc.len())];
        for e in ctx.doc.entities() {
            let kind = match &e.geometry {
                Geometry::Line(_) => "LINE",
                Geometry::Polyline(_) => "POLYLINE",
                Geometry::Circle(_) => "CIRCLE",
                Geometry::Arc(_) => "ARC",
                Geometry::Text(_) => "TEXT",
                Geometry::Insert(_) => "INSERT",
            };
            lines.push(format!("  {} {} [{}]", e.id.0, kind, e.layer));
        }
        Ok(lines.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_script_with_undo_redo() {
        let interp = Interpreter::with_builtins();
        let mut ctx = Context::new();
        let out = interp
            .run_script(
                &mut ctx,
                "# draw a wall\nLINE 0 0 10 0 A-WALL\nlayer A-ROOM\nLINE 0 0 0 8 A-WALL\n",
            )
            .unwrap();
        assert_eq!(ctx.doc.len(), 2);
        assert!(out.iter().any(|l| l.contains("entity 0")));

        // Aliases and case-insensitivity.
        interp.eval_line(&mut ctx, "u").unwrap();
        assert_eq!(ctx.doc.len(), 1);
        interp.eval_line(&mut ctx, "REDO").unwrap();
        assert_eq!(ctx.doc.len(), 2);
    }

    #[test]
    fn erase_then_undo() {
        let interp = Interpreter::with_builtins();
        let mut ctx = Context::new();
        interp.eval_line(&mut ctx, "LINE 0 0 1 0").unwrap();
        interp.eval_line(&mut ctx, "ERASE 0").unwrap();
        assert_eq!(ctx.doc.len(), 0);
        interp.eval_line(&mut ctx, "UNDO").unwrap();
        assert_eq!(ctx.doc.len(), 1);
    }

    #[test]
    fn unknown_command_reports_line_number() {
        let interp = Interpreter::with_builtins();
        let mut ctx = Context::new();
        let err = interp
            .run_script(&mut ctx, "LINE 0 0 1 0\nBOGUS 1 2\n")
            .unwrap_err();
        assert!(err.starts_with("line 2:"), "got {err:?}");
    }

    #[test]
    fn bom_prefixed_script_runs() {
        // A Windows editor prepends a UTF-8 BOM to line 1. Before the fix this
        // failed with `unknown command "\u{feff}LAYER"`.
        let interp = Interpreter::with_builtins();
        let mut ctx = Context::new();
        let script = "\u{feff}LAYER A-WALL\nLINE 0 0 10 0 A-WALL\n";
        let out = interp
            .run_script(&mut ctx, script)
            .expect("BOM script runs");
        assert_eq!(ctx.doc.len(), 1);
        assert!(out.iter().any(|l| l.contains("A-WALL")));
    }

    #[test]
    fn bom_prefixed_single_line_evaluates() {
        let interp = Interpreter::with_builtins();
        let mut ctx = Context::new();
        interp
            .eval_line(&mut ctx, "\u{feff}LINE 0 0 1 0")
            .expect("BOM line evaluates");
        assert_eq!(ctx.doc.len(), 1);
    }

    #[test]
    fn draws_circle_arc_text_polyline_insert() {
        let interp = Interpreter::with_builtins();
        let mut ctx = Context::new();
        let out = interp
            .run_script(
                &mut ctx,
                "CIRCLE 5 5 2 A-COL\n\
                 ARC 0 0 3 0 90\n\
                 TEXT 1 1 0.25 KITCHEN 12\n\
                 POLYLINE 0 0 4 0 4 3 CLOSED A-ROOM\n\
                 INSERT DOOR 2 2 90 A-DOOR\n",
            )
            .unwrap();
        assert_eq!(ctx.doc.len(), 5);
        assert_eq!(out.len(), 5);

        let kinds: Vec<&str> = ctx
            .doc
            .entities()
            .iter()
            .map(|e| match &e.geometry {
                Geometry::Circle(_) => "circle",
                Geometry::Arc(_) => "arc",
                Geometry::Text(_) => "text",
                Geometry::Polyline(_) => "polyline",
                Geometry::Insert(_) => "insert",
                Geometry::Line(_) => "line",
            })
            .collect();
        assert_eq!(kinds, ["circle", "arc", "text", "polyline", "insert"]);

        // Text keeps its multi-word value.
        let text = ctx
            .doc
            .entities()
            .into_iter()
            .find_map(|e| match &e.geometry {
                Geometry::Text(t) => Some(t.value.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(text, "KITCHEN 12");
    }

    #[test]
    fn move_keeps_the_id_and_is_undoable() {
        let interp = Interpreter::with_builtins();
        let mut ctx = Context::new();
        interp.eval_line(&mut ctx, "CIRCLE 0 0 1 L").unwrap();
        let id = ctx.doc.entities()[0].id;

        let msg = interp.eval_line(&mut ctx, "MOVE 0 10 5").unwrap().unwrap();
        assert!(
            !msg.contains("new id"),
            "id is preserved, not re-minted: {msg:?}"
        );
        assert_eq!(ctx.doc.len(), 1, "move keeps a single entity");

        // The entity is still reachable under its original id, now translated.
        let moved = ctx.doc.get(id).expect("id survives the move");
        if let Geometry::Circle(c) = &moved.geometry {
            assert_eq!(c.center, Point2::new(10.0, 5.0));
        } else {
            panic!("expected a circle");
        }

        // One UNDO restores the original geometry under that same id.
        interp.eval_line(&mut ctx, "UNDO").unwrap();
        assert_eq!(ctx.doc.len(), 1);
        let restored = ctx.doc.get(id).expect("id is stable across undo too");
        if let Geometry::Circle(c) = &restored.geometry {
            assert_eq!(c.center, Point2::new(0.0, 0.0));
        } else {
            panic!("expected a circle");
        }
    }

    #[test]
    fn copy_duplicates_translated() {
        let interp = Interpreter::with_builtins();
        let mut ctx = Context::new();
        interp.eval_line(&mut ctx, "LINE 0 0 1 0 L").unwrap();
        interp.eval_line(&mut ctx, "COPY 0 0 5").unwrap();
        assert_eq!(ctx.doc.len(), 2);
        let ys: Vec<f64> = ctx
            .doc
            .entities()
            .iter()
            .filter_map(|e| match &e.geometry {
                Geometry::Line(s) => Some(s.a.y),
                _ => None,
            })
            .collect();
        assert_eq!(ys, [0.0, 5.0]);
    }

    #[test]
    fn rotate_scale_mirror_transform_in_place_and_undo() {
        let interp = Interpreter::with_builtins();
        let mut ctx = Context::new();
        interp.eval_line(&mut ctx, "LINE 0 0 2 0 A").unwrap();
        let id = ctx.doc.entities()[0].id;

        // ROTATE 90° about origin: (2,0) -> (0,2).
        interp.eval_line(&mut ctx, "ROTATE 0 0 0 90").unwrap();
        assert_eq!(ctx.doc.len(), 1, "rotate keeps a single entity");
        if let Geometry::Line(s) = &ctx.doc.get(id).unwrap().geometry {
            assert!(s.b.distance(Point2::new(0.0, 2.0)) < 1e-9, "got {:?}", s.b);
        } else {
            panic!("expected line");
        }
        interp.eval_line(&mut ctx, "UNDO").unwrap();
        if let Geometry::Line(s) = &ctx.doc.get(id).unwrap().geometry {
            assert!(s.b.distance(Point2::new(2.0, 0.0)) < 1e-9, "undo restores");
        }

        // SCALE ×2 about origin: (2,0) -> (4,0).
        interp.eval_line(&mut ctx, "SCALE 0 0 0 2").unwrap();
        if let Geometry::Line(s) = &ctx.doc.get(id).unwrap().geometry {
            assert!(s.b.distance(Point2::new(4.0, 0.0)) < 1e-9);
        }

        // MIRROR across the x-axis: y flips.
        interp.eval_line(&mut ctx, "MIRROR 0 0 0 1 0").unwrap();
        if let Geometry::Line(s) = &ctx.doc.get(id).unwrap().geometry {
            assert!(s.a.y <= 0.0 && s.b.y <= 0.0, "mirrored below the axis");
        }
    }

    #[test]
    fn rectangle_is_a_closed_four_corner_polyline() {
        let interp = Interpreter::with_builtins();
        let mut ctx = Context::new();
        interp
            .eval_line(&mut ctx, "RECTANGLE 0 0 4 3 A-ROOM")
            .unwrap();
        let e = &ctx.doc.entities()[0];
        assert_eq!(e.layer, "A-ROOM");
        if let Geometry::Polyline(p) = &e.geometry {
            assert!(p.closed);
            assert_eq!(p.vertices.len(), 4);
            assert!((p.area() - 12.0).abs() < 1e-9, "4×3 rectangle area");
        } else {
            panic!("expected polyline");
        }
        // A degenerate rectangle is rejected.
        assert!(interp.eval_line(&mut ctx, "RECTANGLE 0 0 0 3").is_err());
        // Aliases resolve.
        assert!(interp.eval_line(&mut ctx, "REC 0 0 1 1").is_ok());
    }

    #[test]
    fn transform_commands_reject_missing_ids() {
        let interp = Interpreter::with_builtins();
        let mut ctx = Context::new();
        assert!(interp.eval_line(&mut ctx, "ROTATE 42 0 0 90").is_err());
        assert!(interp.eval_line(&mut ctx, "SCALE 42 0 0 2").is_err());
        assert!(interp.eval_line(&mut ctx, "MIRROR 42 0 0 1 0").is_err());
        // Bad axis / factor guards.
        interp.eval_line(&mut ctx, "LINE 0 0 1 0").unwrap();
        assert!(interp.eval_line(&mut ctx, "SCALE 0 0 0 0").is_err());
        assert!(interp.eval_line(&mut ctx, "MIRROR 0 1 1 1 1").is_err());
    }

    #[test]
    fn offset_adds_a_parallel_copy() {
        let interp = Interpreter::with_builtins();
        let mut ctx = Context::new();
        interp.eval_line(&mut ctx, "LINE 0 0 10 0 A-WALL").unwrap();
        // Offset toward (5, 5) → the copy sits at y = 3 on the same layer.
        let msg = interp
            .eval_line(&mut ctx, "OFFSET 0 3 5 5")
            .unwrap()
            .unwrap();
        assert!(msg.contains("as entity 1"), "{msg:?}");
        assert_eq!(ctx.doc.len(), 2);
        let copy = ctx.doc.get(EntityId(1)).unwrap();
        assert_eq!(copy.layer, "A-WALL", "offset keeps the source layer");
        if let Geometry::Line(s) = &copy.geometry {
            assert!(s.a.distance(Point2::new(0.0, 3.0)) < 1e-9, "got {:?}", s.a);
        } else {
            panic!("expected line");
        }
        // Text cannot be offset.
        interp.eval_line(&mut ctx, "TEXT 0 0 1 HELLO").unwrap();
        assert!(interp.eval_line(&mut ctx, "OFFSET 2 1").is_err());
    }

    #[test]
    fn trim_keeps_the_side_away_from_the_pick() {
        let interp = Interpreter::with_builtins();
        let mut ctx = Context::new();
        interp.eval_line(&mut ctx, "LINE 0 0 10 0").unwrap(); // id 0
        interp.eval_line(&mut ctx, "LINE 4 -5 4 5").unwrap(); // id 1, cutter at x=4
                                                              // Pick near the a-end: that piece is dropped, [X, b] survives.
        interp.eval_line(&mut ctx, "TRIM 0 1 0 0").unwrap();
        if let Geometry::Line(s) = &ctx.doc.get(EntityId(0)).unwrap().geometry {
            assert!(
                s.a.distance(Point2::new(4.0, 0.0)) < 1e-9,
                "trimmed to the cut"
            );
            assert!(s.b.distance(Point2::new(10.0, 0.0)) < 1e-9);
        } else {
            panic!("expected line");
        }
        // A cutter that does not cross is rejected.
        interp.eval_line(&mut ctx, "LINE 40 -5 40 5").unwrap(); // id 2, far away
        assert!(interp.eval_line(&mut ctx, "TRIM 0 2 5 0").is_err());
    }

    #[test]
    fn extend_lengthens_a_line_to_a_boundary() {
        let interp = Interpreter::with_builtins();
        let mut ctx = Context::new();
        interp.eval_line(&mut ctx, "LINE 0 0 5 0").unwrap(); // id 0
        interp.eval_line(&mut ctx, "LINE 8 -5 8 5").unwrap(); // id 1, boundary at x=8
                                                              // Pick near the b-end (5,0): it stretches out to x=8.
        interp.eval_line(&mut ctx, "EXTEND 0 1 5 0").unwrap();
        if let Geometry::Line(s) = &ctx.doc.get(EntityId(0)).unwrap().geometry {
            assert!(
                s.b.distance(Point2::new(8.0, 0.0)) < 1e-9,
                "extended to the boundary"
            );
        } else {
            panic!("expected line");
        }
        // A boundary the line already reaches past cannot extend it.
        assert!(interp.eval_line(&mut ctx, "EXTEND 0 1 5 0").is_err());
    }

    #[test]
    fn fillet_rounds_a_right_angle_corner() {
        let interp = Interpreter::with_builtins();
        let mut ctx = Context::new();
        interp.eval_line(&mut ctx, "LINE 0 0 10 0 A-WALL").unwrap(); // id 0
        interp.eval_line(&mut ctx, "LINE 0 0 0 10 A-WALL").unwrap(); // id 1
        interp.eval_line(&mut ctx, "FILLET 0 1 2").unwrap();

        // The two lines are trimmed back to the tangent points, and an arc is added.
        assert_eq!(ctx.doc.len(), 3);
        if let Geometry::Line(s) = &ctx.doc.get(EntityId(0)).unwrap().geometry {
            assert!(
                s.b.distance(Point2::new(2.0, 0.0)) < 1e-9,
                "trimmed to tangent"
            );
        } else {
            panic!("expected line");
        }
        let arc = ctx
            .doc
            .entities()
            .into_iter()
            .find_map(|e| match &e.geometry {
                Geometry::Arc(a) => Some(*a),
                _ => None,
            });
        let arc = arc.expect("a fillet arc was added");
        assert!((arc.radius - 2.0).abs() < 1e-9);
        assert!(
            arc.center.distance(Point2::new(2.0, 2.0)) < 1e-9,
            "center at (2,2)"
        );

        // Parallel lines cannot be filleted.
        let interp2 = Interpreter::with_builtins();
        let mut c2 = Context::new();
        interp2.eval_line(&mut c2, "LINE 0 0 10 0").unwrap();
        interp2.eval_line(&mut c2, "LINE 0 5 10 5").unwrap();
        assert!(interp2.eval_line(&mut c2, "FILLET 0 1 1").is_err());
    }

    #[test]
    fn trim_truncates_a_polyline_at_a_cutter() {
        let interp = Interpreter::with_builtins();
        let mut ctx = Context::new();
        // An L-path (0,0)-(4,0)-(4,4), cut by a vertical line at x=2.
        interp
            .eval_line(&mut ctx, "POLYLINE 0 0 4 0 4 4 A-ROOM")
            .unwrap(); // id 0
        interp.eval_line(&mut ctx, "LINE 2 -5 2 5").unwrap(); // id 1, crosses edge 0 at (2,0)
                                                              // Pick near the origin end → that stub is dropped.
        interp.eval_line(&mut ctx, "TRIM 0 1 0 0").unwrap();
        if let Geometry::Polyline(p) = &ctx.doc.get(EntityId(0)).unwrap().geometry {
            assert_eq!(p.vertices.len(), 3);
            assert!(
                p.vertices[0].distance(Point2::new(2.0, 0.0)) < 1e-9,
                "cut at (2,0)"
            );
            assert!(
                p.vertices[2].distance(Point2::new(4.0, 4.0)) < 1e-9,
                "far end kept"
            );
        } else {
            panic!("expected polyline");
        }
    }

    #[test]
    fn extend_stretches_a_polyline_end_to_a_boundary() {
        let interp = Interpreter::with_builtins();
        let mut ctx = Context::new();
        interp
            .eval_line(&mut ctx, "POLYLINE 0 0 4 0 4 4 A-ROOM")
            .unwrap(); // id 0
        interp.eval_line(&mut ctx, "LINE 0 8 10 8").unwrap(); // id 1, boundary y=8
                                                              // Pick near the (4,4) end → its terminal edge stretches up to y=8.
        interp.eval_line(&mut ctx, "EXTEND 0 1 4 4").unwrap();
        if let Geometry::Polyline(p) = &ctx.doc.get(EntityId(0)).unwrap().geometry {
            assert_eq!(p.vertices.len(), 3);
            assert!(
                p.vertices[2].distance(Point2::new(4.0, 8.0)) < 1e-9,
                "end stretched to (4,8), got {:?}",
                p.vertices[2]
            );
        } else {
            panic!("expected polyline");
        }
    }

    #[test]
    fn chamfer_bevels_a_right_angle_corner() {
        let interp = Interpreter::with_builtins();
        let mut ctx = Context::new();
        interp.eval_line(&mut ctx, "LINE 0 0 10 0 A-WALL").unwrap(); // id 0
        interp.eval_line(&mut ctx, "LINE 0 0 0 10 A-WALL").unwrap(); // id 1
        interp.eval_line(&mut ctx, "CHAMFER 0 1 2").unwrap();

        assert_eq!(ctx.doc.len(), 3, "two trimmed lines plus the bevel");
        if let Geometry::Line(s) = &ctx.doc.get(EntityId(0)).unwrap().geometry {
            assert!(
                s.b.distance(Point2::new(2.0, 0.0)) < 1e-9,
                "trimmed to the bevel"
            );
        } else {
            panic!("expected line");
        }
        // The bevel joins the two trim points (2,0)-(0,2).
        let bevel = ctx.doc.get(EntityId(2)).unwrap();
        if let Geometry::Line(s) = &bevel.geometry {
            let ends = [s.a, s.b];
            assert!(ends
                .iter()
                .any(|p| p.distance(Point2::new(2.0, 0.0)) < 1e-9));
            assert!(ends
                .iter()
                .any(|p| p.distance(Point2::new(0.0, 2.0)) < 1e-9));
        } else {
            panic!("expected the bevel line");
        }
        // Parallel lines cannot be chamfered.
        let interp2 = Interpreter::with_builtins();
        let mut c2 = Context::new();
        interp2.eval_line(&mut c2, "LINE 0 0 10 0").unwrap();
        interp2.eval_line(&mut c2, "LINE 0 5 10 5").unwrap();
        assert!(interp2.eval_line(&mut c2, "CHAMFER 0 1 1").is_err());
    }

    #[test]
    fn trim_cuts_an_arc_at_a_line() {
        let interp = Interpreter::with_builtins();
        let mut ctx = Context::new();
        // Upper semicircle 0°→180°, and a vertical line through the centre.
        interp.eval_line(&mut ctx, "ARC 0 0 5 0 180").unwrap(); // id 0
        interp.eval_line(&mut ctx, "LINE 0 -10 0 10").unwrap(); // id 1, crosses at (0,5)=90°
                                                                // Pick on the left half → drop it, keep the right quarter [0°, 90°].
        interp.eval_line(&mut ctx, "TRIM 0 1 -4 1").unwrap();
        if let Geometry::Arc(a) = &ctx.doc.get(EntityId(0)).unwrap().geometry {
            assert!(
                (a.start_angle - 0.0).abs() < 1e-6,
                "start {}",
                a.start_angle
            );
            assert!((a.end_angle - 90.0).abs() < 1e-6, "end {}", a.end_angle);
        } else {
            panic!("expected arc");
        }
    }

    #[test]
    fn extend_grows_an_arc_to_a_boundary() {
        let interp = Interpreter::with_builtins();
        let mut ctx = Context::new();
        // A right-upper quarter 0°→90°, and a line tangent to the circle at (-5,0)=180°.
        interp.eval_line(&mut ctx, "ARC 0 0 5 0 90").unwrap(); // id 0
        interp.eval_line(&mut ctx, "LINE -5 -10 -5 10").unwrap(); // id 1, tangent at 180°
                                                                  // Pick near the arc's end point (0,5) → grow that end out to 180°.
        interp.eval_line(&mut ctx, "EXTEND 0 1 0 5").unwrap();
        if let Geometry::Arc(a) = &ctx.doc.get(EntityId(0)).unwrap().geometry {
            assert!((a.start_angle - 0.0).abs() < 1e-6);
            assert!(
                (a.end_angle - 180.0).abs() < 1e-6,
                "grown to 180°, got {}",
                a.end_angle
            );
        } else {
            panic!("expected arc");
        }
    }

    #[test]
    fn array_creates_a_rectangular_grid_of_copies() {
        let interp = Interpreter::with_builtins();
        let mut ctx = Context::new();
        interp.eval_line(&mut ctx, "CIRCLE 0 0 1").unwrap();
        // 2 rows × 3 cols → 6 cells, 5 new copies plus the original = 6 entities.
        interp.eval_line(&mut ctx, "ARRAY 0 2 3 10 5").unwrap();
        assert_eq!(ctx.doc.len(), 6);
        // The centers span the grid {0,10,20} × {0,5}.
        let mut centers: Vec<(i64, i64)> = ctx
            .doc
            .entities()
            .iter()
            .filter_map(|e| match &e.geometry {
                Geometry::Circle(c) => Some((c.center.x as i64, c.center.y as i64)),
                _ => None,
            })
            .collect();
        centers.sort_unstable();
        assert_eq!(
            centers,
            [(0, 0), (0, 5), (10, 0), (10, 5), (20, 0), (20, 5)]
        );
        // One UNDO removes all copies atomically.
        interp.eval_line(&mut ctx, "UNDO").unwrap();
        assert_eq!(ctx.doc.len(), 1);
    }

    #[test]
    fn explode_bakes_a_block_reference_into_geometry() {
        use aadt_model::Block;

        let interp = Interpreter::with_builtins();
        let mut ctx = Context::new();
        // Define a two-segment block, then reference it.
        let mut b = Block::new("TEE", Point2::new(0.0, 0.0));
        b.push(
            "0",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(2.0, 0.0))),
        );
        b.push(
            "STEEL",
            Geometry::Line(Segment::new(Point2::new(1.0, 0.0), Point2::new(1.0, 2.0))),
        );
        ctx.doc.add_block(b);
        interp
            .eval_line(&mut ctx, "INSERT TEE 10 10 0 A-COL")
            .unwrap();
        let insert_id = ctx.doc.entities()[0].id;

        interp.eval_line(&mut ctx, "EXPLODE 0").unwrap();
        // The insert is gone, replaced by the block's two segments.
        assert!(ctx.doc.get(insert_id).is_none(), "insert removed");
        assert_eq!(ctx.doc.len(), 2);

        // Layer "0" content inherited the insert's layer; named layer kept.
        let layers: std::collections::BTreeSet<&str> = ctx
            .doc
            .entities()
            .iter()
            .map(|e| e.layer.as_str())
            .collect();
        assert!(layers.contains("A-COL"));
        assert!(layers.contains("STEEL"));

        // Geometry is baked at the insertion point (10,10).
        let baked = ctx.doc.entities().iter().any(|e| {
            matches!(
                &e.geometry,
                Geometry::Line(s) if s.a.distance(Point2::new(10.0, 10.0)) < 1e-9
            )
        });
        assert!(baked, "first segment starts at the insertion point");

        // One UNDO restores the single insert.
        interp.eval_line(&mut ctx, "UNDO").unwrap();
        assert_eq!(ctx.doc.len(), 1);
        assert!(matches!(
            ctx.doc.entities()[0].geometry,
            Geometry::Insert(_)
        ));
    }

    #[test]
    fn explode_rejects_non_inserts_and_undefined_blocks() {
        let interp = Interpreter::with_builtins();
        let mut ctx = Context::new();
        interp.eval_line(&mut ctx, "LINE 0 0 1 0").unwrap();
        assert!(
            interp.eval_line(&mut ctx, "EXPLODE 0").is_err(),
            "a line is not a block ref"
        );
        interp.eval_line(&mut ctx, "INSERT GHOST 0 0").unwrap();
        assert!(
            interp.eval_line(&mut ctx, "EXPLODE 1").is_err(),
            "an undefined block cannot be exploded"
        );
    }

    #[test]
    fn dist_and_area_report_without_mutating() {
        let interp = Interpreter::with_builtins();
        let mut ctx = Context::new();
        let d = interp.eval_line(&mut ctx, "DIST 0 0 3 4").unwrap().unwrap();
        assert!(d.contains("distance 5"), "3-4-5 triangle: {d:?}");

        interp.eval_line(&mut ctx, "RECTANGLE 0 0 4 3").unwrap();
        let a = interp.eval_line(&mut ctx, "AREA 0").unwrap().unwrap();
        assert!(a.contains("area 12"), "4×3 rectangle: {a:?}");

        interp.eval_line(&mut ctx, "CIRCLE 0 0 2").unwrap();
        let ac = interp.eval_line(&mut ctx, "AREA 1").unwrap().unwrap();
        assert!(ac.contains("area 12.56"), "π·2²: {ac:?}");

        // An open polyline has no enclosed area.
        interp.eval_line(&mut ctx, "POLYLINE 0 0 1 0 2 2").unwrap();
        assert!(interp.eval_line(&mut ctx, "AREA 2").is_err());
        // Inspection left the document unchanged in size.
        assert_eq!(ctx.doc.len(), 3);
    }

    #[test]
    fn layer_command_sets_color_and_visibility_and_undoes() {
        let interp = Interpreter::with_builtins();
        let mut ctx = Context::new();
        let msg = interp
            .eval_line(&mut ctx, "LAYER A-WALL COLOR 5 OFF FREEZE")
            .unwrap()
            .unwrap();
        assert!(msg.contains("color 5") && msg.contains("off") && msg.contains("frozen"));
        let l = ctx.doc.layer("A-WALL").unwrap();
        assert_eq!(l.color, 5);
        assert!(!l.visible && l.frozen && !l.shown());

        // Back-compat: a bare LAYER just creates a default layer.
        interp.eval_line(&mut ctx, "LAYER A-ROOM").unwrap();
        let r = ctx.doc.layer("A-ROOM").unwrap();
        assert!(r.shown() && r.color == 7);

        // Turn A-WALL back on and thaw it in one command.
        interp.eval_line(&mut ctx, "LAYER A-WALL ON THAW").unwrap();
        assert!(ctx.doc.layer("A-WALL").unwrap().shown());
        // One UNDO reverts that whole restyle.
        interp.eval_line(&mut ctx, "UNDO").unwrap();
        assert!(
            !ctx.doc.layer("A-WALL").unwrap().shown(),
            "undo restores off+frozen"
        );

        // A bad colour and unknown option are rejected.
        assert!(interp.eval_line(&mut ctx, "LAYER X COLOR 999").is_err());
        assert!(interp.eval_line(&mut ctx, "LAYER X BOGUS").is_err());
    }

    #[test]
    fn dim_adds_an_undoable_dimension() {
        let interp = Interpreter::with_builtins();
        let mut ctx = Context::new();
        let msg = interp
            .eval_line(&mut ctx, "DIM 0 0 4 3 A-DIMS")
            .unwrap()
            .unwrap();
        assert!(msg.contains("measuring 5"), "3-4-5: {msg:?}");
        assert_eq!(ctx.doc.dimensions().len(), 1);
        let d = &ctx.doc.dimensions()[0];
        assert_eq!(d.layer, "A-DIMS");
        assert!((d.measurement - 5.0).abs() < 1e-9);

        interp.eval_line(&mut ctx, "UNDO").unwrap();
        assert!(
            ctx.doc.dimensions().is_empty(),
            "undo removes the dimension"
        );
        interp.eval_line(&mut ctx, "REDO").unwrap();
        assert_eq!(ctx.doc.dimensions().len(), 1, "redo restores it");

        // Coincident points are rejected.
        assert!(interp.eval_line(&mut ctx, "DIM 1 1 1 1").is_err());
    }

    #[test]
    fn hatch_fills_a_closed_region_and_undoes() {
        let interp = Interpreter::with_builtins();
        let mut ctx = Context::new();
        interp
            .eval_line(&mut ctx, "RECTANGLE 0 0 4 3 A-ROOM")
            .unwrap(); // id 0
        interp.eval_line(&mut ctx, "HATCH 0 ANSI31 A-FLOR").unwrap();
        assert_eq!(ctx.doc.hatches().len(), 1);
        let h = &ctx.doc.hatches()[0];
        assert_eq!(h.pattern, "ANSI31");
        assert_eq!(h.layer, "A-FLOR");
        assert_eq!(h.boundary.vertices.len(), 4);

        interp.eval_line(&mut ctx, "UNDO").unwrap();
        assert!(ctx.doc.hatches().is_empty(), "undo removes the hatch");

        // A circle hatches as a polygon; pattern defaults to SOLID, layer inherited.
        interp.eval_line(&mut ctx, "CIRCLE 5 5 2 A-COL").unwrap(); // id 1
        interp.eval_line(&mut ctx, "HATCH 1").unwrap();
        let h = ctx.doc.hatches().last().unwrap();
        assert!(h.solid, "default pattern SOLID is a solid fill");
        assert_eq!(h.layer, "A-COL", "inherits the entity's layer");
        assert!(h.boundary.vertices.len() >= 3);

        // An open shape (a line) cannot be hatched.
        interp.eval_line(&mut ctx, "LINE 0 0 1 0").unwrap(); // id 2
        assert!(interp.eval_line(&mut ctx, "HATCH 2").is_err());
    }

    #[test]
    fn layers_command_lists_sorted() {
        let interp = Interpreter::with_builtins();
        let mut ctx = Context::new();
        interp.eval_line(&mut ctx, "LINE 0 0 1 0 B-LAYER").unwrap();
        interp.eval_line(&mut ctx, "LAYER A-LAYER").unwrap();
        let out = interp.eval_line(&mut ctx, "LAYERS").unwrap().unwrap();
        let a = out.find("A-LAYER").unwrap();
        let b = out.find("B-LAYER").unwrap();
        assert!(a < b, "layers listed sorted: {out:?}");
    }

    #[test]
    fn bad_arguments_report_errors() {
        let interp = Interpreter::with_builtins();
        let mut ctx = Context::new();
        assert!(interp.eval_line(&mut ctx, "CIRCLE 0 0 -1").is_err());
        assert!(interp.eval_line(&mut ctx, "MOVE 999 1 1").is_err());
        assert!(interp.eval_line(&mut ctx, "POLYLINE 0 0").is_err());
        assert_eq!(ctx.doc.len(), 0, "failed commands leave no entities");
    }

    #[test]
    fn completion_matches_names_and_aliases_sorted() {
        let reg = builtins();
        // Prefix "L" hits LINE, LAYER, LAYERS, LAYERLIST, LIST, and the alias L/LA.
        let c = reg.complete("l");
        assert_eq!(c, {
            let mut v = c.clone();
            v.sort();
            v
        });
        assert!(c.contains(&"LINE".to_string()));
        assert!(c.contains(&"LAYER".to_string()));
        assert!(c.contains(&"LAYERS".to_string()));
        assert!(c.contains(&"LIST".to_string()));
        // Case-insensitive, and aliases participate.
        assert!(reg.complete("ci").contains(&"CIRCLE".to_string()));
        assert!(reg.complete("pl").contains(&"PLINE".to_string()));
        // No match yields an empty list; empty prefix lists everything.
        assert!(reg.complete("zzz").is_empty());
        assert!(reg.complete("").len() >= 14);
    }
}
