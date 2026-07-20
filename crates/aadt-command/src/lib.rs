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

use aadt_geometry::{Arc, Circle, Point2, Polyline, Segment, Vec2};
use aadt_model::{Document, EntityId, Geometry, Insert, Text};

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
}

/// Build a registry pre-populated with the generic drawing built-ins.
pub fn builtins() -> Registry {
    let mut r = Registry::new();
    // Draw
    r.register(Box::new(LineCmd));
    r.register(Box::new(CircleCmd));
    r.register(Box::new(ArcCmd));
    r.register(Box::new(RectangCmd));
    r.register(Box::new(PlineCmd));
    // Annotate
    r.register(Box::new(TextCmd));
    // Modify
    r.register(Box::new(CopyCmd));
    r.register(Box::new(MoveCmd));
    r.register(Box::new(EraseCmd));
    // Organize
    r.register(Box::new(LayerCmd));
    // History / query
    r.register(Box::new(UndoCmd));
    r.register(Box::new(RedoCmd));
    r.register(Box::new(ListCmd));
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
    pub fn eval_line(&self, ctx: &mut Context, line: &str) -> Result<Option<String>, String> {
        let trimmed = line.trim();
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
    pub fn run_script(&self, ctx: &mut Context, script: &str) -> Result<Vec<String>, String> {
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

/// Parse the first argument as an entity id.
fn parse_entity_id(args: &[String], flag: &str) -> Result<EntityId, String> {
    let raw = args
        .first()
        .ok_or_else(|| format!("{flag}: missing entity id"))?;
    raw.parse::<u64>()
        .map(EntityId)
        .map_err(|_| format!("{flag}: {raw:?} is not an entity id"))
}

/// Return a copy of `geometry` translated by `(dx, dy)`. Used by COPY and MOVE.
fn translate(geometry: &Geometry, dx: f64, dy: f64) -> Geometry {
    let d = Vec2::new(dx, dy);
    match geometry {
        Geometry::Line(s) => Geometry::Line(Segment::new(s.a + d, s.b + d)),
        Geometry::Polyline(p) => {
            let vertices = p.vertices.iter().map(|&v| v + d).collect();
            Geometry::Polyline(Polyline::new(vertices, p.closed))
        }
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

struct CircleCmd;
impl Command for CircleCmd {
    fn name(&self) -> &str {
        "CIRCLE"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["C"]
    }
    fn help(&self) -> &str {
        "CIRCLE cx cy radius [layer] — add a circle"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let cx = parse_f64(args, 0, "CIRCLE")?;
        let cy = parse_f64(args, 1, "CIRCLE")?;
        let r = parse_f64(args, 2, "CIRCLE")?;
        if r <= 0.0 {
            return Err("CIRCLE: radius must be positive".to_string());
        }
        let layer = args.get(3).cloned().unwrap_or_else(|| "0".to_string());
        let circle = Circle::new(Point2::new(cx, cy), r);
        let id = ctx.doc.transact("CIRCLE", |d| {
            d.add_geometry(layer, Geometry::Circle(circle))
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
        "ARC cx cy radius start_deg end_deg [layer] — add an arc (degrees, CCW)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let cx = parse_f64(args, 0, "ARC")?;
        let cy = parse_f64(args, 1, "ARC")?;
        let r = parse_f64(args, 2, "ARC")?;
        let start = parse_f64(args, 3, "ARC")?;
        let end = parse_f64(args, 4, "ARC")?;
        if r <= 0.0 {
            return Err("ARC: radius must be positive".to_string());
        }
        let layer = args.get(5).cloned().unwrap_or_else(|| "0".to_string());
        let arc = Arc::new(Point2::new(cx, cy), r, start, end);
        let id = ctx
            .doc
            .transact("ARC", |d| d.add_geometry(layer, Geometry::Arc(arc)));
        Ok(format!("ARC added as entity {}", id.0))
    }
}

struct RectangCmd;
impl Command for RectangCmd {
    fn name(&self) -> &str {
        "RECTANG"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["REC"]
    }
    fn help(&self) -> &str {
        "RECTANG x1 y1 x2 y2 [layer] — add a closed rectangle from two corners"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let x1 = parse_f64(args, 0, "RECTANG")?;
        let y1 = parse_f64(args, 1, "RECTANG")?;
        let x2 = parse_f64(args, 2, "RECTANG")?;
        let y2 = parse_f64(args, 3, "RECTANG")?;
        let layer = args.get(4).cloned().unwrap_or_else(|| "0".to_string());
        let poly = Polyline::new(
            vec![
                Point2::new(x1, y1),
                Point2::new(x2, y1),
                Point2::new(x2, y2),
                Point2::new(x1, y2),
            ],
            true,
        );
        let id = ctx.doc.transact("RECTANG", |d| {
            d.add_geometry(layer, Geometry::Polyline(poly))
        });
        Ok(format!("RECTANG added as entity {}", id.0))
    }
}

struct PlineCmd;
impl Command for PlineCmd {
    fn name(&self) -> &str {
        "PLINE"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["PL"]
    }
    fn help(&self) -> &str {
        "PLINE x1 y1 x2 y2 [x3 y3 ...] [closed] [layer] — add a polyline"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        // Numeric tokens are vertex coordinates; a `closed`/`c` token sets closure;
        // any other token is the layer name.
        let mut coords: Vec<f64> = Vec::new();
        let mut closed = false;
        let mut layer = "0".to_string();
        for arg in args {
            if let Ok(n) = arg.parse::<f64>() {
                coords.push(n);
            } else if arg.eq_ignore_ascii_case("closed") || arg.eq_ignore_ascii_case("c") {
                closed = true;
            } else {
                layer = arg.clone();
            }
        }
        if coords.len() < 4 || !coords.len().is_multiple_of(2) {
            return Err("PLINE: need at least two x y vertex pairs".to_string());
        }
        let vertices: Vec<Point2> = coords
            .chunks_exact(2)
            .map(|c| Point2::new(c[0], c[1]))
            .collect();
        let poly = Polyline::new(vertices, closed);
        let id = ctx
            .doc
            .transact("PLINE", |d| d.add_geometry(layer, Geometry::Polyline(poly)));
        Ok(format!("PLINE added as entity {}", id.0))
    }
}

struct TextCmd;
impl Command for TextCmd {
    fn name(&self) -> &str {
        "TEXT"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["T"]
    }
    fn help(&self) -> &str {
        "TEXT x y height content... — add text (content is the remaining words)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let x = parse_f64(args, 0, "TEXT")?;
        let y = parse_f64(args, 1, "TEXT")?;
        let height = parse_f64(args, 2, "TEXT")?;
        if height <= 0.0 {
            return Err("TEXT: height must be positive".to_string());
        }
        let content = args.get(3..).unwrap_or(&[]).join(" ");
        if content.is_empty() {
            return Err("TEXT: missing content".to_string());
        }
        let text = Text::new(content.clone(), Point2::new(x, y), height, 0.0);
        let id = ctx
            .doc
            .transact("TEXT", |d| d.add_geometry("0", Geometry::Text(text)));
        Ok(format!("TEXT {content:?} added as entity {}", id.0))
    }
}

struct CopyCmd;
impl Command for CopyCmd {
    fn name(&self) -> &str {
        "COPY"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["CP"]
    }
    fn help(&self) -> &str {
        "COPY id dx dy — duplicate an entity, offset by (dx, dy)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let id = parse_entity_id(args, "COPY")?;
        let dx = parse_f64(args, 1, "COPY")?;
        let dy = parse_f64(args, 2, "COPY")?;
        let source = ctx
            .doc
            .get(id)
            .ok_or_else(|| format!("COPY: no entity with id {}", id.0))?;
        let layer = source.layer.clone();
        let attributes = source.attributes.clone();
        let geometry = translate(&source.geometry, dx, dy);
        let new_id = ctx
            .doc
            .transact("COPY", |d| d.add_entity(layer, geometry, attributes));
        Ok(format!("COPY of {} added as entity {}", id.0, new_id.0))
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
        "MOVE id dx dy — translate an entity by (dx, dy)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let id = parse_entity_id(args, "MOVE")?;
        let dx = parse_f64(args, 1, "MOVE")?;
        let dy = parse_f64(args, 2, "MOVE")?;
        let source = ctx
            .doc
            .get(id)
            .ok_or_else(|| format!("MOVE: no entity with id {}", id.0))?;
        let layer = source.layer.clone();
        let attributes = source.attributes.clone();
        let geometry = translate(&source.geometry, dx, dy);
        // Remove-then-add in one transaction: atomic and undoable. The moved entity
        // takes a new id (the model has no in-place update yet — see Track A request).
        let new_id = ctx.doc.transact("MOVE", |d| {
            d.remove_entity(id);
            d.add_entity(layer, geometry, attributes)
        });
        Ok(format!("MOVE {} -> entity {}", id.0, new_id.0))
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
        "LAYER name — create a layer"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let name = args.first().ok_or("LAYER: missing name")?.clone();
        ctx.doc.transact("LAYER", |d| {
            d.add_layer(aadt_model::Layer::new(name.clone()))
        });
        Ok(format!("layer {name:?} ready"))
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
    fn draws_each_geometry_kind() {
        let interp = Interpreter::with_builtins();
        let mut ctx = Context::new();
        interp
            .run_script(
                &mut ctx,
                "CIRCLE 1 2 0.5 S-COLS\n\
                 ARC 0 0 2 0 90\n\
                 RECTANG 0 0 4 3 A-ROOM\n\
                 PLINE 0 0 1 0 1 1 closed A-WALL\n\
                 TEXT 2 2 0.25 Room 101\n",
            )
            .unwrap();
        assert_eq!(ctx.doc.len(), 5);
        let kinds: Vec<_> = ctx
            .doc
            .entities()
            .iter()
            .map(|e| match &e.geometry {
                Geometry::Circle(_) => "circle",
                Geometry::Arc(_) => "arc",
                Geometry::Polyline(p) if p.closed => "rect-or-closed",
                Geometry::Polyline(_) => "pline",
                Geometry::Text(_) => "text",
                _ => "other",
            })
            .collect();
        assert_eq!(
            kinds,
            vec!["circle", "arc", "rect-or-closed", "rect-or-closed", "text"]
        );
        // Text content preserves spaces.
        if let Geometry::Text(t) = &ctx.doc.entities()[4].geometry {
            assert_eq!(t.value, "Room 101");
        } else {
            panic!("expected text entity");
        }
    }

    #[test]
    fn copy_duplicates_translated_and_is_undoable() {
        let interp = Interpreter::with_builtins();
        let mut ctx = Context::new();
        interp.eval_line(&mut ctx, "CIRCLE 0 0 1").unwrap();
        interp.eval_line(&mut ctx, "COPY 0 5 0").unwrap();
        assert_eq!(ctx.doc.len(), 2);
        // The copy is offset by (5, 0).
        if let Geometry::Circle(c) = &ctx.doc.entities()[1].geometry {
            assert_eq!(c.center, Point2::new(5.0, 0.0));
        } else {
            panic!("expected circle copy");
        }
        interp.eval_line(&mut ctx, "UNDO").unwrap();
        assert_eq!(ctx.doc.len(), 1);
    }

    #[test]
    fn move_translates_and_undo_restores_original() {
        let interp = Interpreter::with_builtins();
        let mut ctx = Context::new();
        interp.eval_line(&mut ctx, "LINE 0 0 2 0 A-WALL").unwrap();
        interp.eval_line(&mut ctx, "MOVE 0 0 3").unwrap();
        assert_eq!(ctx.doc.len(), 1);
        if let Geometry::Line(s) = &ctx.doc.entities()[0].geometry {
            assert_eq!(s.a, Point2::new(0.0, 3.0));
            assert_eq!(s.b, Point2::new(2.0, 3.0));
        } else {
            panic!("expected moved line");
        }
        // One atomic undo restores the original line at its old position.
        interp.eval_line(&mut ctx, "UNDO").unwrap();
        assert_eq!(ctx.doc.len(), 1);
        if let Geometry::Line(s) = &ctx.doc.entities()[0].geometry {
            assert_eq!(s.a, Point2::new(0.0, 0.0));
        } else {
            panic!("expected restored line");
        }
    }
}
