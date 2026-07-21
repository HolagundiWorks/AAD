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
use aadt_model::{Document, EntityId, Geometry, Insert, Text};

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
    r.register(Box::new(ArrayCmd));
    r.register(Box::new(ExplodeCmd));
    r.register(Box::new(EraseCmd));
    // Organize.
    r.register(Box::new(LayerCmd));
    r.register(Box::new(LayersCmd));
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
