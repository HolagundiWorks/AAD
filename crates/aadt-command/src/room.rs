//! Imperial room-label commands (from AutoLISP Room Dimension Tool v2).
//!
//! Two-corner pick → optional rectangle + centered name / WxH / area labels.
//! Short commands (`IBR`, `IKI`, …) and utilities (`IROOM`, `IHELP`, …).

use aadt_geometry::{Point2, Polyline};
use aadt_model::{format_length, primary_from_props, Geometry, Layer, Text, UnitSystem};

use crate::{parse_entity_id, parse_f64, Command, Context, Registry};

/// Session default text height in feet (LISP `*RoomTextHeight*`).
const DEFAULT_ROOM_TEXT_HEIGHT_FT: f64 = 0.15;
const ROOM_LABEL_LINEWEIGHT_MM: f32 = 0.25;
const ROOM_RECT_LINEWEIGHT_MM: f32 = 0.35;

/// Session settings for room labeling (LISP globals).
#[derive(Debug, Clone)]
pub struct RoomSettings {
    /// Text height in feet when drawing units are inches (LISP default 0.15).
    pub text_height_ft: f64,
    /// Cached height in drawing units (inches or metres).
    pub text_height_units: Option<f64>,
    pub draw_rectangle: bool,
    pub label_layer: String,
    pub rect_layer: String,
    /// Optional floor prefix prepended to room names (`MFLOOR` / `GF`, `FF`, …).
    pub floor_prefix: String,
}

impl Default for RoomSettings {
    fn default() -> Self {
        Self {
            text_height_ft: DEFAULT_ROOM_TEXT_HEIGHT_FT,
            text_height_units: None,
            draw_rectangle: true,
            label_layer: "ROOM-LABELS".into(),
            rect_layer: "ROOM-RECTANGLES".into(),
            floor_prefix: String::new(),
        }
    }
}

/// Built-in room type list (IROOM / dialog).
pub const ROOM_TYPES: &[&str] = &[
    "Bedroom",
    "Master Bedroom",
    "Guest Room",
    "Living Room",
    "Dining Room",
    "Kitchen",
    "Pantry",
    "Bathroom",
    "Attached Toilet",
    "Common Toilet",
    "Study Room",
    "Office",
    "Store Room",
    "Pooja Room",
    "Balcony",
    "Terrace",
    "Staircase",
    "Corridor",
    "Entrance",
    "Lobby",
    "Utility",
    "Laundry",
    "Garage",
    "Garden",
    "Courtyard",
    "Wardrobe",
    "Dressing Room",
    "Home Theater",
    "Gym",
];

/// Map a short command token to a room label (uppercase).
pub fn room_type_for_command(cmd: &str) -> Option<&'static str> {
    match cmd.to_ascii_uppercase().as_str() {
        "IBR" | "MBR" => Some("Bedroom"),
        "IDI" | "MDI" => Some("Dining Room"),
        "IKI" | "MKI" => Some("Kitchen"),
        "ILI" | "MLI" => Some("Living Room"),
        "IBA" | "MBA" => Some("Bathroom"),
        "IAT" | "MAT" => Some("Attached Toilet"),
        "ICT" | "MCT" => Some("Common Toilet"),
        "IMB" | "MMB" => Some("Master Bedroom"),
        "IST" | "MST" => Some("Study Room"),
        "ISR" | "MSR" => Some("Store Room"),
        "IPR" | "MPR" => Some("Pooja Room"),
        "IBC" | "MBC" => Some("Balcony"),
        "ITE" | "MTE" => Some("Terrace"),
        "ISC" | "MSC" => Some("Staircase"),
        "ICO" | "MCO" => Some("Corridor"),
        "IEN" | "MEN" => Some("Entrance"),
        "IUT" | "MUT" => Some("Utility"),
        "IGA" | "MGA" => Some("Garage"),
        "IGD" | "MGD" => Some("Garden"),
        "ICY" | "MCY" => Some("Courtyard"),
        "ILO" | "MLO" => Some("Lobby"),
        "IOF" | "MOF" => Some("Office"),
        "IGR" | "MGR" => Some("Guest Room"),
        "IPA" | "MPA" => Some("Pantry"),
        "ILA" | "MLA" => Some("Laundry"),
        "IWR" | "MWR" => Some("Wardrobe"),
        "IDR" | "MDR" => Some("Dressing Room"),
        "IHT" | "MHT" => Some("Home Theater"),
        "IGY" | "MGY" => Some("Gym"),
        _ => None,
    }
}

fn ensure_room_layers(ctx: &mut Context) {
    let label = ctx.room.label_layer.clone();
    let rect = ctx.room.rect_layer.clone();
    ctx.doc.transact("ROOM LAYERS", |d| {
        if d.layer(&label).is_none() {
            d.add_layer(Layer::new(&label).with_color(3).with_lineweight_mm(ROOM_LABEL_LINEWEIGHT_MM));
        }
        if d.layer(&rect).is_none() {
            d.add_layer(Layer::new(&rect).with_color(8).with_lineweight_mm(ROOM_RECT_LINEWEIGHT_MM));
        }
    });
}

fn text_height_drawing(ctx: &Context) -> f64 {
    if let Some(h) = ctx.room.text_height_units {
        return h.max(1e-6);
    }
    let primary = primary_from_props(&ctx.doc.properties().units);
    match primary {
        // LISP: height in feet × 12 → inches (drawing units).
        UnitSystem::FeetInches => (ctx.room.text_height_ft * 12.0).max(1e-6),
        UnitSystem::Meters => ctx.room.text_height_ft.max(1e-6),
    }
}

fn format_dim_pair(width: f64, height: f64, units: &str) -> String {
    let primary = primary_from_props(units);
    format!(
        "{} x {}",
        format_length(width, primary),
        format_length(height, primary)
    )
}

fn format_area_label(width: f64, height: f64, units: &str) -> String {
    let primary = primary_from_props(units);
    match primary {
        UnitSystem::FeetInches => {
            let area_sqft = (width / 12.0) * (height / 12.0);
            format!("Area: {area_sqft:.2} sq ft")
        }
        UnitSystem::Meters => {
            let area = width * height;
            format!("Area: {area:.2} m²")
        }
    }
}

/// Create rectangle + three centered texts for a room.
pub fn place_room_label(
    ctx: &mut Context,
    room_type: &str,
    pt1: Point2,
    pt2: Point2,
) -> Result<String, String> {
    let width = (pt2.x - pt1.x).abs();
    let height = (pt2.y - pt1.y).abs();
    if width <= f64::EPSILON || height <= f64::EPSILON {
        return Err("room: corners must span a non-zero width and height".into());
    }
    ensure_room_layers(ctx);
    let cx = (pt1.x + pt2.x) * 0.5;
    let cy = (pt1.y + pt2.y) * 0.5;
    let text_h = text_height_drawing(ctx);
    let units = ctx.doc.properties().units.clone();
    let room_text = {
        let base = room_type.to_ascii_uppercase();
        if ctx.room.floor_prefix.trim().is_empty() {
            base
        } else {
            format!("{} {base}", ctx.room.floor_prefix.trim().to_ascii_uppercase())
        }
    };
    let dim_text = format_dim_pair(width, height, &units);
    let area_text = format_area_label(width, height, &units);
    let label_layer = ctx.room.label_layer.clone();
    let rect_layer = ctx.room.rect_layer.clone();
    let draw_rect = ctx.room.draw_rectangle;

    let dim_center = Point2::new(cx, cy - text_h * 1.5);
    let area_center = Point2::new(cx, cy - text_h * 2.8);
    let center = Point2::new(cx, cy);
    let corners = vec![
        Point2::new(pt1.x, pt1.y),
        Point2::new(pt2.x, pt1.y),
        Point2::new(pt2.x, pt2.y),
        Point2::new(pt1.x, pt2.y),
    ];

    ctx.doc.transact("ROOMLABEL", |d| {
        if draw_rect {
            d.add_geometry(
                rect_layer.clone(),
                Geometry::Polyline(Polyline::new(corners, true)),
            );
        }
        d.add_geometry(
            label_layer.clone(),
            Geometry::Text(Text::new(room_text.clone(), center, text_h, 0.0)),
        );
        d.add_geometry(
            label_layer.clone(),
            Geometry::Text(Text::new(dim_text.clone(), dim_center, text_h * 0.8, 0.0)),
        );
        d.add_geometry(
            label_layer,
            Geometry::Text(Text::new(area_text.clone(), area_center, text_h * 0.7, 0.0)),
        );
    });

    Ok(format!(
        "Created: {room_text} · Size: {dim_text} · {area_text}"
    ))
}

fn parse_corners(args: &[String], start: usize, cmd: &str) -> Result<(Point2, Point2), String> {
    let x1 = parse_f64(args, start, cmd)?;
    let y1 = parse_f64(args, start + 1, cmd)?;
    let x2 = parse_f64(args, start + 2, cmd)?;
    let y2 = parse_f64(args, start + 3, cmd)?;
    Ok((Point2::new(x1, y1), Point2::new(x2, y2)))
}

/// Register all room / imperial+metric label commands.
pub fn register_room(r: &mut Registry) {
    r.register(Box::new(RoomLabelCmd));
    r.register(Box::new(IroomCmd));
    r.register(Box::new(IdoCmd));
    r.register(Box::new(IrectCmd));
    r.register(Box::new(IhideRectCmd));
    r.register(Box::new(IthCmd));
    r.register(Box::new(IlayerCmd));
    r.register(Box::new(IsetCmd));
    r.register(Box::new(IresetCmd));
    r.register(Box::new(IarCmd));
    r.register(Box::new(IhelpCmd));
    r.register(Box::new(MfloorCmd));
    r.register(Box::new(MrelabelCmd));
    for (cmd, label) in [
        ("IBR", "Bedroom"),
        ("MBR", "Bedroom"),
        ("IDI", "Dining Room"),
        ("MDI", "Dining Room"),
        ("IKI", "Kitchen"),
        ("MKI", "Kitchen"),
        ("ILI", "Living Room"),
        ("MLI", "Living Room"),
        ("IBA", "Bathroom"),
        ("MBA", "Bathroom"),
        ("IAT", "Attached Toilet"),
        ("MAT", "Attached Toilet"),
        ("ICT", "Common Toilet"),
        ("MCT", "Common Toilet"),
        ("IMB", "Master Bedroom"),
        ("MMB", "Master Bedroom"),
        ("IST", "Study Room"),
        ("MST", "Study Room"),
        ("ISR", "Store Room"),
        ("MSR", "Store Room"),
        ("IPR", "Pooja Room"),
        ("MPR", "Pooja Room"),
        ("IBC", "Balcony"),
        ("MBC", "Balcony"),
        ("ITE", "Terrace"),
        ("MTE", "Terrace"),
        ("ISC", "Staircase"),
        ("MSC", "Staircase"),
        ("ICO", "Corridor"),
        ("MCO", "Corridor"),
        ("IEN", "Entrance"),
        ("MEN", "Entrance"),
        ("IUT", "Utility"),
        ("MUT", "Utility"),
        ("IGA", "Garage"),
        ("MGA", "Garage"),
        ("IGD", "Garden"),
        ("MGD", "Garden"),
        ("ICY", "Courtyard"),
        ("MCY", "Courtyard"),
        ("ILO", "Lobby"),
        ("MLO", "Lobby"),
        ("IOF", "Office"),
        ("MOF", "Office"),
        ("IGR", "Guest Room"),
        ("MGR", "Guest Room"),
        ("IPA", "Pantry"),
        ("MPA", "Pantry"),
        ("ILA", "Laundry"),
        ("MLA", "Laundry"),
        ("IWR", "Wardrobe"),
        ("MWR", "Wardrobe"),
        ("IDR", "Dressing Room"),
        ("MDR", "Dressing Room"),
        ("IHT", "Home Theater"),
        ("MHT", "Home Theater"),
        ("IGY", "Gym"),
        ("MGY", "Gym"),
    ] {
        r.register(Box::new(FixedRoomCmd {
            name: cmd,
            label,
        }));
    }
}

struct FixedRoomCmd {
    name: &'static str,
    label: &'static str,
}

impl Command for FixedRoomCmd {
    fn name(&self) -> &str {
        self.name
    }
    fn help(&self) -> &str {
        "Room label — pick two corners (or pass x1 y1 x2 y2)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        if args.len() >= 4 {
            let (a, b) = parse_corners(args, 0, self.name)?;
            place_room_label(ctx, self.label, a, b)
        } else {
            Ok(format!(
                "{} — pick first corner of {} (or {} x1 y1 x2 y2)",
                self.name, self.label, self.name
            ))
        }
    }
}

struct RoomLabelCmd;
impl Command for RoomLabelCmd {
    fn name(&self) -> &str {
        "ROOMLABEL"
    }
    fn help(&self) -> &str {
        "ROOMLABEL name x1 y1 x2 y2 — room name + arch dims + area (optional rectangle)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let name = args.first().ok_or("ROOMLABEL: missing room name")?;
        if args.len() < 5 {
            return Err("ROOMLABEL: need name x1 y1 x2 y2".into());
        }
        let (a, b) = parse_corners(args, 1, "ROOMLABEL")?;
        place_room_label(ctx, name, a, b)
    }
}

struct IroomCmd;
impl Command for IroomCmd {
    fn name(&self) -> &str {
        "IROOM"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["MROOM"]
    }
    fn help(&self) -> &str {
        "IROOM [type] — list room types, or create with type + x1 y1 x2 y2"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        if args.is_empty() {
            let list = ROOM_TYPES.join(", ");
            return Ok(format!(
                "IROOM — pick a type in the room picker, or IROOM <type> x1 y1 x2 y2. Types: {list}"
            ));
        }
        // Join tokens until we hit a number (type may be multi-word).
        let mut name_parts = Vec::new();
        let mut i = 0;
        while i < args.len() {
            if args[i].parse::<f64>().is_ok() {
                break;
            }
            name_parts.push(args[i].as_str());
            i += 1;
        }
        let name = name_parts.join(" ");
        if name.is_empty() {
            return Err("IROOM: missing room type".into());
        }
        if i + 3 < args.len() {
            let (a, b) = parse_corners(args, i, "IROOM")?;
            place_room_label(ctx, &name, a, b)
        } else {
            Ok(format!(
                "IROOM {name} — pick first corner (or IROOM {name} x1 y1 x2 y2)"
            ))
        }
    }
}

struct IdoCmd;
impl Command for IdoCmd {
    fn name(&self) -> &str {
        "IDO"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["MDO"]
    }
    fn help(&self) -> &str {
        "IDO [name x1 y1 x2 y2] — custom room label"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        if args.is_empty() {
            return Ok("IDO — type room name, then pick two corners".into());
        }
        if args.len() >= 5 {
            let name = &args[0];
            let (a, b) = parse_corners(args, 1, "IDO")?;
            return place_room_label(ctx, name, a, b);
        }
        let name = args.join(" ");
        Ok(format!("IDO {name} — pick first corner (or IDO {name} x1 y1 x2 y2)"))
    }
}

struct IrectCmd;
impl Command for IrectCmd {
    fn name(&self) -> &str {
        "IRECT"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["MRECT"]
    }
    fn help(&self) -> &str {
        "IRECT — toggle drawing the room rectangle"
    }
    fn run(&self, ctx: &mut Context, _args: &[String]) -> Result<String, String> {
        ctx.room.draw_rectangle = !ctx.room.draw_rectangle;
        Ok(format!(
            "Rectangle drawing is now: {}",
            if ctx.room.draw_rectangle { "ON" } else { "OFF" }
        ))
    }
}

struct IhideRectCmd;
impl Command for IhideRectCmd {
    fn name(&self) -> &str {
        "IHIDERECT"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["MHIDERECT"]
    }
    fn help(&self) -> &str {
        "IHIDERECT — freeze/thaw the room rectangle layer"
    }
    fn run(&self, ctx: &mut Context, _args: &[String]) -> Result<String, String> {
        ensure_room_layers(ctx);
        let layer = ctx.room.rect_layer.clone();
        let frozen = ctx
            .doc
            .layer(&layer)
            .map(|l| l.frozen)
            .unwrap_or(false);
        let next = !frozen;
        ctx.doc.transact("IHIDERECT", |d| {
            d.set_layer_frozen(&layer, next);
        });
        Ok(format!(
            "{layer} layer is now {}",
            if next { "HIDDEN" } else { "VISIBLE" }
        ))
    }
}

struct IthCmd;
impl Command for IthCmd {
    fn name(&self) -> &str {
        "ITH"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["MTH"]
    }
    fn help(&self) -> &str {
        "ITH [height] — set room text height (feet when ft-in; metres when m)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        if let Some(raw) = args.first() {
            let h: f64 = raw
                .parse()
                .map_err(|_| "ITH: height must be a number".to_string())?;
            if h <= 0.0 {
                return Err("ITH: height must be positive".into());
            }
            ctx.room.text_height_ft = h;
            let primary = primary_from_props(&ctx.doc.properties().units);
            ctx.room.text_height_units = Some(match primary {
                UnitSystem::FeetInches => h * 12.0,
                UnitSystem::Meters => h,
            });
        }
        Ok(format!(
            "Text height set to: {:.2} (drawing units ≈ {:.3})",
            ctx.room.text_height_ft,
            text_height_drawing(ctx)
        ))
    }
}

struct IlayerCmd;
impl Command for IlayerCmd {
    fn name(&self) -> &str {
        "ILAYER"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["MLAYER"]
    }
    fn help(&self) -> &str {
        "ILAYER name — set room label layer"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let name = args
            .first()
            .cloned()
            .ok_or("ILAYER: missing layer name")?;
        ctx.room.label_layer = name.clone();
        ensure_room_layers(ctx);
        Ok(format!("Layer set to: {name}"))
    }
}

struct IsetCmd;
impl Command for IsetCmd {
    fn name(&self) -> &str {
        "ISET"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["MSET"]
    }
    fn help(&self) -> &str {
        "ISET — show room tool settings"
    }
    fn run(&self, ctx: &mut Context, _args: &[String]) -> Result<String, String> {
        Ok(format!(
            "ROOM SETTINGS — text height: {:.2} · draw rect: {} · labels: {} · rects: {}",
            ctx.room.text_height_ft,
            if ctx.room.draw_rectangle { "YES" } else { "NO" },
            ctx.room.label_layer,
            ctx.room.rect_layer
        ))
    }
}

struct IresetCmd;
impl Command for IresetCmd {
    fn name(&self) -> &str {
        "IRESET"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["MRESET"]
    }
    fn help(&self) -> &str {
        "IRESET — reset room tool settings"
    }
    fn run(&self, ctx: &mut Context, _args: &[String]) -> Result<String, String> {
        ctx.room = RoomSettings::default();
        Ok("All room settings reset to default".into())
    }
}

struct IarCmd;
impl Command for IarCmd {
    fn name(&self) -> &str {
        "IAR"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["MAR"]
    }
    fn help(&self) -> &str {
        "IAR id — label area of a closed polyline/circle at its center"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let raw = args.first().ok_or("IAR: missing entity id (or pick in GUI)")?;
        let id = parse_entity_id(raw, "IAR")?;
        let entity = ctx
            .doc
            .get(id)
            .ok_or_else(|| format!("IAR: no entity {}", id.0))?;
        let (area_model, center) = match &entity.geometry {
            Geometry::Polyline(p) if p.closed => {
                let c = p
                    .centroid()
                    .ok_or("IAR: could not compute centroid")?;
                (p.area(), c)
            }
            Geometry::Circle(c) => {
                let a = std::f64::consts::PI * c.radius * c.radius;
                (a, c.center)
            }
            _ => {
                return Err("IAR: select a closed polyline or circle".into());
            }
        };
        ensure_room_layers(ctx);
        let units = ctx.doc.properties().units.clone();
        let primary = primary_from_props(&units);
        let area_text = match primary {
            UnitSystem::FeetInches => format!("Area: {:.2} sq ft", area_model / 144.0),
            UnitSystem::Meters => format!("Area: {:.2} m²", area_model),
        };
        let text_h = text_height_drawing(ctx);
        let layer = ctx.room.label_layer.clone();
        ctx.doc.transact("IAR", |d| {
            d.add_geometry(
                layer,
                Geometry::Text(Text::new(area_text.clone(), center, text_h, 0.0)),
            );
        });
        Ok(format!("{area_text} label created at center"))
    }
}

struct IhelpCmd;
impl Command for IhelpCmd {
    fn name(&self) -> &str {
        "IHELP"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["MHELP"]
    }
    fn help(&self) -> &str {
        "IHELP / MHELP — room dimension tool help"
    }
    fn run(&self, _ctx: &mut Context, _args: &[String]) -> Result<String, String> {
        Ok(
            "ROOM TOOL — IROOM/MROOM picker · IDO/MDO custom · IBR/MBR… · \
             IRECT/MRECT · IHIDERECT · ITH/MTH · MFLOOR · IAR/MAR · Format follows UNITS (ft-in or m)"
                .into(),
        )
    }
}

struct MfloorCmd;
impl Command for MfloorCmd {
    fn name(&self) -> &str {
        "MFLOOR"
    }
    fn help(&self) -> &str {
        "MFLOOR [prefix] — set floor prefix for room labels (empty clears)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let prefix = args.join(" ");
        ctx.room.floor_prefix = prefix.clone();
        if prefix.is_empty() {
            Ok("MFLOOR: floor prefix cleared".into())
        } else {
            Ok(format!(
                "MFLOOR: labels will be prefixed '{prefix}' (e.g. '{prefix} LIVING ROOM')"
            ))
        }
    }
}

struct MrelabelCmd;
impl Command for MrelabelCmd {
    fn name(&self) -> &str {
        "MRELABEL"
    }
    fn help(&self) -> &str {
        "MRELABEL id newname… — replace TEXT value of a room label"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let id = parse_entity_id(args.first().ok_or("MRELABEL: missing id")?, "MRELABEL")?;
        let name = args.get(1..).unwrap_or(&[]).join(" ");
        if name.is_empty() {
            return Err("MRELABEL: missing new room type".into());
        }
        let new_str = {
            let base = name.to_ascii_uppercase();
            if ctx.room.floor_prefix.trim().is_empty() {
                base
            } else {
                format!(
                    "{} {base}",
                    ctx.room.floor_prefix.trim().to_ascii_uppercase()
                )
            }
        };
        let old = match &ctx
            .doc
            .get(id)
            .ok_or_else(|| format!("MRELABEL: no entity {}", id.0))?
            .geometry
        {
            Geometry::Text(t) => t.value.clone(),
            _ => return Err("MRELABEL: pick a TEXT object".into()),
        };
        ctx.doc.transact("MRELABEL", |d| {
            if let Some(e) = d.get(id) {
                if let Geometry::Text(t) = &e.geometry {
                    let mut t2 = t.clone();
                    t2.value = new_str.clone();
                    d.replace_geometry(id, Geometry::Text(t2));
                }
            }
        });
        Ok(format!("MRELABEL: '{old}' -> '{new_str}'"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Interpreter;

    #[test]
    fn roomlabel_creates_rect_and_texts() {
        let i = Interpreter::with_builtins();
        let mut ctx = Context::new();
        let msg = i
            .eval_line(&mut ctx, "ROOMLABEL Kitchen 0 0 120 96")
            .unwrap()
            .unwrap();
        assert!(msg.contains("KITCHEN"));
        assert!(msg.contains("sq ft") || msg.contains("'"));
        // rect + 3 texts
        assert_eq!(ctx.doc.len(), 4);
        assert!(ctx.doc.layer("ROOM-LABELS").is_some());
        assert!(ctx.doc.layer("ROOM-RECTANGLES").is_some());
    }

    #[test]
    fn ibr_alias_places_bedroom() {
        let i = Interpreter::with_builtins();
        let mut ctx = Context::new();
        let msg = i
            .eval_line(&mut ctx, "IBR 0 0 144 120")
            .unwrap()
            .unwrap();
        assert!(msg.contains("BEDROOM"));
    }

    #[test]
    fn irect_toggles() {
        let i = Interpreter::with_builtins();
        let mut ctx = Context::new();
        assert!(ctx.room.draw_rectangle);
        i.eval_line(&mut ctx, "IRECT").unwrap();
        assert!(!ctx.room.draw_rectangle);
    }
}
