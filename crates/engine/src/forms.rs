//! Input forms: what a command needs to be asked, declared as data.
//!
//! The architecture commands take their input as JSON. A menu click has no JSON, so each one has a
//! [`Form`]: the settings are shown in a dialog by the UI (or asked on the command line), and the
//! points are picked on the drawing by [`FormRun`]. Both end in the same JSON call the scripts
//! and the agent make, so a form never knows more than the command does.
//!
//! Input shape (what a dialog or `Session::start_form` gives): an object keyed by field key.
//! Text, number and list fields are strings (or numbers), yes/no fields booleans, a point `[x, y]`,
//! points `[[x, y], ...]`, a table an array of rows (each an array of strings in column order, or an
//! object keyed by column). [`parse_input`] turns that into the command's parameters and refuses
//! anything that is not a finite number where a number is needed.

use serde_json::{Map, Value, json};

use crate::prompt::{Accept, Input, Interactive, Prompt, Step};
use crate::{EngineError, Result, Session};

/// One table column.
#[derive(Clone, Copy, Debug)]
pub struct Col {
    pub key: &'static str,
    pub label: &'static str,
    pub numeric: bool,
}

#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Text,
    Number,
    /// Numbers separated by commas, spaces or semicolons.
    Numbers,
    Bool,
    Choice(&'static [&'static str]),
    /// A room name or two-letter code ([`room_types`]).
    RoomType,
    Point,
    Points,
    /// Rows of values; the field key is the parameter that receives the array.
    Table(&'static [Col]),
}

#[derive(Clone, Copy, Debug)]
pub struct Field {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: Kind,
    pub default: &'static str,
    pub required: bool,
    /// Ask for the points even if optional (Enter skips them).
    pub ask: bool,
    pub help: &'static str,
}

const fn field(key: &'static str, label: &'static str, kind: Kind) -> Field {
    Field { key, label, kind, default: "", required: false, ask: false, help: "" }
}
impl Field {
    const fn req(mut self) -> Self {
        self.required = true;
        self
    }
    const fn def(mut self, d: &'static str) -> Self {
        self.default = d;
        self
    }
    const fn ask(mut self) -> Self {
        self.ask = true;
        self
    }
    const fn help(mut self, h: &'static str) -> Self {
        self.help = h;
        self
    }
    /// A point or points: picked on the drawing, not typed in a dialog.
    pub fn is_pick(&self) -> bool {
        matches!(self.kind, Kind::Point | Kind::Points)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Form {
    pub id: &'static str,
    pub title: &'static str,
    /// The parameters go inside this key (`project` takes `{details: {...}}`).
    pub wrap: Option<&'static str>,
    pub fields: &'static [Field],
}

impl Form {
    /// Fields shown in a dialog.
    pub fn dialog_fields(&self) -> impl Iterator<Item = &Field> {
        self.fields.iter().filter(|f| !f.is_pick())
    }
    /// Whether the command needs points picked.
    pub fn has_picks(&self) -> bool {
        self.fields.iter().any(|f| f.is_pick() && (f.required || f.ask))
    }
}

const UNITS: &[&str] = &["", "mm", "cm", "m", "in", "ft"];
const FLOOR_COLS: &[Col] = &[
    Col { key: "name", label: "Name", numeric: false },
    Col { key: "floorToFloor", label: "Floor to floor (m)", numeric: true },
    Col { key: "ceiling", label: "Ceiling (m)", numeric: true },
    Col { key: "lintel", label: "Lintel bottom (m)", numeric: true },
    Col { key: "slab", label: "Slab (mm)", numeric: true },
    Col { key: "beam", label: "Beam (mm)", numeric: true },
];
const OPENING_COLS: &[Col] = &[
    Col { key: "name", label: "Mark", numeric: false },
    Col { key: "kind", label: "Kind (door, window, ventilator)", numeric: false },
    Col { key: "type", label: "Type", numeric: false },
    Col { key: "length", label: "Width (m)", numeric: true },
    Col { key: "height", label: "Height (m)", numeric: true },
    Col { key: "sill", label: "Sill (m)", numeric: true },
    Col { key: "block", label: "Blocks", numeric: false },
    Col { key: "floor", label: "Floor", numeric: false },
];
const RATE_COLS: &[Col] = &[
    Col { key: "takeoff", label: "Item", numeric: false },
    Col { key: "unit", label: "Unit", numeric: false },
    Col { key: "rate", label: "Rate", numeric: true },
];

const TAKEOFF: &[Field] = &[
    field("units", "Units (blank: the drawing's)", Kind::Choice(UNITS)),
    field("table", "Draw the table", Kind::Bool).def("true"),
    field("label", "Label the objects", Kind::Bool).def("true"),
    field("tolerance", "Matching distance (drawing units)", Kind::Number).help("Blank: about 10 mm"),
];
const TAKEOFF_PAINT: &[Field] = &[
    field("floor", "Floor name (blank: the first)", Kind::Text),
    field("units", "Units (blank: the drawing's)", Kind::Choice(UNITS)),
    field("table", "Draw the table", Kind::Bool).def("true"),
    field("label", "Label the objects", Kind::Bool).def("true"),
];
const TAKEOFF_COLUMNS: &[Field] = &[
    field("layer", "Layer of the column outlines", Kind::Text).def("MEASURE-COLUMN"),
    field("units", "Units (blank: the drawing's)", Kind::Choice(UNITS)),
    field("table", "Draw the table", Kind::Bool).def("true"),
    field("label", "Label the objects", Kind::Bool).def("true"),
];

/// Every form. Order is not significant.
pub static FORMS: &[Form] = &[
    Form {
        id: "project",
        title: "Project Details",
        wrap: Some("details"),
        fields: &[
            field("title", "Project title", Kind::Text),
            field("owner", "Owner / client", Kind::Text),
            field("architect", "Architect / company", Kind::Text),
            field("engineer", "Engineer / prepared by", Kind::Text),
            field("pid", "Property ID", Kind::Text),
            field("siteArea", "Site area (m²)", Kind::Text),
            field("plotUse", "Plot use", Kind::Text),
            field("location", "Location", Kind::Text),
            field("address", "Address", Kind::Text),
            field("authority", "Approving authority", Kind::Text),
            field("date", "Date", Kind::Text),
        ],
    },
    Form { id: "floors", title: "Floors and Heights", wrap: None, fields: &[field("floors", "Floors", Kind::Table(FLOOR_COLS)).req()] },
    Form {
        id: "beams",
        title: "Beam Depths",
        wrap: None,
        fields: &[field("depths", "Beam depths in mm (for example 300, 375, 450)", Kind::Numbers).req()],
    },
    Form {
        id: "schedule",
        title: "Opening Schedule",
        wrap: None,
        fields: &[field("openings", "Doors, windows and ventilators", Kind::Table(OPENING_COLS)).req()],
    },
    Form { id: "takeoff.rates", title: "Take-off Rates", wrap: None, fields: &[field("rates", "Rates", Kind::Table(RATE_COLS)).req()] },
    Form {
        id: "room",
        title: "Room",
        wrap: None,
        fields: &[
            field("name", "Room type", Kind::RoomType).req(),
            field("floorPrefix", "Number prefix (for example G for G1, G2)", Kind::Text),
            field("textHeight", "Text height (drawing units)", Kind::Number),
            field("rectangle", "Draw the room rectangle", Kind::Bool).def("true"),
            field("units", "Units of the size text (blank: the drawing's)", Kind::Choice(UNITS)),
            field("p1", "First corner", Kind::Point).req(),
            field("p2", "Opposite corner", Kind::Point).req(),
        ],
    },
    Form { id: "hcwlayers", title: "Layer Sets", wrap: None, fields: &[field("set", "Layer set", Kind::Choice(&["hcw", "bp", "both"])).def("hcw")] },
    Form { id: "takeoff.linear", title: "Linear Take-off", wrap: None, fields: TAKEOFF },
    Form { id: "takeoff.brick", title: "Brickwork Take-off", wrap: None, fields: TAKEOFF },
    Form { id: "takeoff.beams", title: "Beams and Lintels Take-off", wrap: None, fields: TAKEOFF },
    Form { id: "takeoff.columns", title: "Columns Take-off", wrap: None, fields: TAKEOFF_COLUMNS },
    Form { id: "takeoff.rects", title: "Rectangles Take-off", wrap: None, fields: TAKEOFF },
    Form { id: "takeoff.paint", title: "Wall Paint Take-off", wrap: None, fields: TAKEOFF_PAINT },
    Form { id: "takeoff.ceiling", title: "Ceiling Take-off", wrap: None, fields: TAKEOFF },
    Form { id: "takeoff.floor", title: "Flooring Take-off", wrap: None, fields: TAKEOFF },
    Form { id: "takeoff.area", title: "Area Take-off", wrap: None, fields: TAKEOFF },
    Form { id: "takeoff.slab", title: "Slab Take-off", wrap: None, fields: TAKEOFF },
    Form {
        id: "wall.draw",
        title: "Draw Wall",
        wrap: None,
        fields: &[
            field("thickness", "Thickness (drawing units; blank: 230 mm)", Kind::Number),
            field("closed", "Close the path", Kind::Bool),
            field("layer", "Layer", Kind::Text).def("A-WALL"),
            field("points", "Wall centre line: pick points, Enter to finish", Kind::Points).req(),
        ],
    },
    Form {
        id: "wall.door",
        title: "Door",
        wrap: None,
        fields: &[
            field("name", "Schedule mark (for example D2)", Kind::Text),
            field("width", "Width (drawing units; used when no mark)", Kind::Number),
            field("swing", "Swing", Kind::Choice(&["left", "right"])).def("left"),
            field("side", "Side", Kind::Choice(&["up", "down"])).def("up"),
            field("thickness", "Wall thickness (blank: read from the wall)", Kind::Number),
            field("deduct", "Deduct from the wall in the take-off", Kind::Bool).def("true"),
            field("at", "Door centre on the wall", Kind::Point).req(),
        ],
    },
    Form {
        id: "wall.window",
        title: "Window",
        wrap: None,
        fields: &[
            field("name", "Schedule mark (for example W3)", Kind::Text),
            field("width", "Width (drawing units; used when no mark)", Kind::Number),
            field("thickness", "Wall thickness (blank: read from the wall)", Kind::Number),
            field("deduct", "Deduct from the wall in the take-off", Kind::Bool).def("true"),
            field("at", "Window centre on the wall", Kind::Point).req(),
        ],
    },
    Form {
        id: "grid.axis",
        title: "Axis Grid",
        wrap: None,
        fields: &[
            field("bays", "Bay widths along x (for example 4000, 5000)", Kind::Numbers).req(),
            field("rows", "Row depths along y (blank: none)", Kind::Numbers),
            field("extend", "Line extension beyond the grid", Kind::Number),
            field("origin", "Grid origin", Kind::Point).ask(),
        ],
    },
    Form {
        id: "column.place",
        title: "Place Columns",
        wrap: None,
        fields: &[
            field("size", "Column size: one number for square, or width, depth", Kind::Numbers).req(),
            field("points", "Pick column positions (Enter: every grid intersection)", Kind::Points).ask(),
        ],
    },
    Form {
        id: "lintel.draw",
        title: "Generate Lintel",
        wrap: None,
        fields: &[field("bearing", "Bearing each side (drawing units; blank: 150 mm)", Kind::Number)],
    },
    Form {
        id: "stair.plan",
        title: "Stair",
        wrap: None,
        fields: &[
            field("kind", "Kind", Kind::Choice(&["straight", "dogleg"])).def("straight"),
            field("rise", "Floor-to-floor rise (metres)", Kind::Number).req(),
            field("width", "Flight width (drawing units)", Kind::Number).req(),
            field("going", "Going (mm; blank: 250)", Kind::Number),
            field("riserMax", "Largest riser (mm; blank: 175)", Kind::Number),
            field("landing", "Landing depth (dog-leg; drawing units)", Kind::Number),
            field("waist", "Waist thickness (mm; blank: 150)", Kind::Number),
            field("at", "Start of the stair", Kind::Point).req(),
        ],
    },
    Form {
        id: "lift.shaft",
        title: "Lift Shaft",
        wrap: None,
        fields: &[
            field("width", "Clear shaft width (drawing units)", Kind::Number).req(),
            field("depth", "Clear shaft depth (drawing units)", Kind::Number).req(),
            field("wall", "Wall thickness (blank: 230 mm)", Kind::Number),
            field("at", "Lower-left corner of the clear shaft", Kind::Point).req(),
        ],
    },
    Form {
        id: "rail.draw",
        title: "Handrail",
        wrap: None,
        fields: &[
            field("post", "Post spacing (blank: 1.2 m)", Kind::Number),
            field("size", "Post size (blank: 50 mm)", Kind::Number),
            field("points", "Rail path: pick points, Enter to finish", Kind::Points).req(),
        ],
    },
    Form {
        id: "level.mark",
        title: "Level Mark",
        wrap: None,
        fields: &[
            field("value", "Level in metres (or use a floor below)", Kind::Number),
            field("floor", "Floor name (its finished level)", Kind::Text),
            field("offset", "Offset in metres", Kind::Number),
            field("at", "Mark position", Kind::Point).req(),
        ],
    },
    Form {
        id: "section.marker",
        title: "Section Marker",
        wrap: None,
        fields: &[
            field("label", "Label", Kind::Text).def("A"),
            field("side", "The section looks", Kind::Choice(&["left", "right"])).def("left"),
            field("p1", "First end of the section line", Kind::Point).req(),
            field("p2", "Second end", Kind::Point).req(),
        ],
    },
    Form {
        id: "elevation.marker",
        title: "Elevation Marker",
        wrap: None,
        fields: &[
            field("number", "Elevation number", Kind::Text).req(),
            field("sheet", "Sheet", Kind::Text),
            field("angle", "Direction you look (degrees)", Kind::Number).def("90"),
            field("at", "Marker position", Kind::Point).req(),
        ],
    },
    Form {
        id: "north.arrow",
        title: "North Arrow",
        wrap: None,
        fields: &[
            field("angle", "Direction of north (degrees; 90 is up the sheet)", Kind::Number).def("90"),
            field("size", "Size (drawing units; blank: 1.5 m of real size)", Kind::Number),
            field("at", "Position", Kind::Point).req(),
        ],
    },
    Form {
        id: "slope.arrow",
        title: "Slope Arrow",
        wrap: None,
        fields: &[
            field("text", "Slope text (for example 1:100 or 2%)", Kind::Text).req(),
            field("p1", "Top of the slope", Kind::Point).req(),
            field("p2", "Bottom of the slope", Kind::Point).req(),
        ],
    },
    Form {
        id: "electrical.connect",
        title: "Connect Circuits",
        wrap: None,
        fields: &[
            field("snap", "Wire end snap distance (drawing units; blank: 20 mm)", Kind::Number),
            field("flag", "Flag unwired points", Kind::Bool).def("true"),
        ],
    },
    Form {
        id: "electrical.number",
        title: "Number Electrical Points",
        wrap: None,
        fields: &[field("renumber", "Renumber ids already given", Kind::Bool)],
    },
    Form {
        id: "electrical.schedule",
        title: "Electrical Schedule",
        wrap: None,
        fields: &[field("mode", "Schedule", Kind::Choice(&["matrix", "point"])).def("matrix")],
    },
    Form {
        id: "bridge.export",
        title: "Export for AQC",
        wrap: None,
        fields: &[
            field("level", "Floor the measured items belong to (blank: the first)", Kind::Text),
            field("bearing", "Lintel bearing each side (mm)", Kind::Number).def("150"),
            field("write", "Write the file beside the drawing", Kind::Bool).def("true"),
        ],
    },
];

pub fn get(id: &str) -> Option<&'static Form> {
    FORMS.iter().find(|f| f.id == id)
}

/// `(code, name)` of the room types the ROOM command knows.
pub fn room_types() -> Vec<(&'static str, &'static str)> {
    crate::cmd::room_types()
}

fn text_of(v: &Value) -> String {
    match v {
        Value::String(s) => s.trim().to_string(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

fn number_of(f: &Field, v: &Value) -> Result<Option<f64>> {
    let t = text_of(v);
    if t.is_empty() {
        return Ok(None);
    }
    t.parse::<f64>().ok().filter(|x| x.is_finite()).map(Some).ok_or_else(|| bad(f, format!("`{t}` is not a number")))
}

fn bad(f: &Field, msg: String) -> EngineError {
    EngineError::BadParams { cmd: "form".into(), msg: format!("{}: {msg}", f.label) }
}

fn point_of(f: &Field, v: &Value) -> Result<Value> {
    let a = v.as_array().filter(|a| a.len() >= 2).ok_or_else(|| bad(f, "a point is needed".into()))?;
    let (x, y) = (a[0].as_f64(), a[1].as_f64());
    match (x, y) {
        (Some(x), Some(y)) if x.is_finite() && y.is_finite() => Ok(json!([x, y])),
        _ => Err(bad(f, "a point needs two numbers".into())),
    }
}

/// Turn what a dialog (or the prompts) collected into the command's parameters.
pub fn parse_input(form: &Form, input: &Value) -> Result<Value> {
    parse_fields(form, input, true)
}

/// Like [`parse_input`] but only the settings: points are left to be picked on the drawing.
pub fn parse_settings(form: &Form, input: &Value) -> Result<Value> {
    parse_fields(form, input, false)
}

fn parse_fields(form: &Form, input: &Value, picks: bool) -> Result<Value> {
    let mut out = Map::new();
    for f in form.fields {
        if !picks && f.is_pick() {
            continue;
        }
        let v = input.get(f.key).cloned().unwrap_or(Value::Null);
        let empty = match &v {
            Value::Null => true,
            Value::String(s) => s.trim().is_empty(),
            Value::Array(a) => a.is_empty(),
            _ => false,
        };
        if empty {
            if f.required {
                return Err(bad(f, "is required".into()));
            }
            continue;
        }
        match f.kind {
            Kind::Text | Kind::RoomType => {
                let t = text_of(&v);
                if t.chars().count() > 500 {
                    return Err(bad(f, "is too long".into()));
                }
                out.insert(f.key.into(), json!(t));
            }
            Kind::Choice(list) => {
                let t = text_of(&v);
                let hit = list
                    .iter()
                    .find(|c| !c.is_empty() && c.eq_ignore_ascii_case(&t))
                    .ok_or_else(|| bad(f, format!("`{t}` is not one of the choices")))?;
                out.insert(f.key.into(), json!(hit));
            }
            Kind::Bool => {
                let b = v.as_bool().or_else(|| text_of(&v).parse::<bool>().ok()).ok_or_else(|| bad(f, "must be yes or no".into()))?;
                out.insert(f.key.into(), json!(b));
            }
            Kind::Number => {
                if let Some(n) = number_of(f, &v)? {
                    out.insert(f.key.into(), json!(n));
                }
            }
            Kind::Numbers => {
                let t = match &v {
                    Value::Array(a) => a.iter().map(text_of).collect::<Vec<_>>().join(" "),
                    other => text_of(other),
                };
                let nums: Vec<f64> = t
                    .split([',', ';', ' '])
                    .filter(|s| !s.is_empty())
                    .map(|s| s.parse::<f64>().ok().filter(|x| x.is_finite()).ok_or_else(|| bad(f, format!("`{s}` is not a number"))))
                    .collect::<Result<_>>()?;
                if nums.len() > 200 {
                    return Err(bad(f, "too many numbers".into()));
                }
                if f.key == "size" {
                    match nums.as_slice() {
                        [one] => {
                            out.insert("size".into(), json!(one));
                        }
                        [w, h] => {
                            out.insert("size".into(), json!([w, h]));
                        }
                        _ => return Err(bad(f, "give one number or two (width, depth)".into())),
                    }
                } else if !nums.is_empty() {
                    out.insert(f.key.into(), json!(nums));
                }
            }
            Kind::Point => {
                out.insert(f.key.into(), point_of(f, &v)?);
            }
            Kind::Points => {
                let pts: Vec<Value> = v.as_array().into_iter().flatten().take(10_000).map(|p| point_of(f, p)).collect::<Result<_>>()?;
                out.insert(f.key.into(), Value::Array(pts));
            }
            Kind::Table(cols) => {
                let mut rows = Vec::new();
                for row in v.as_array().into_iter().flatten().take(500) {
                    let cell = |i: usize, c: &Col| -> Value {
                        match row {
                            Value::Array(a) => a.get(i).cloned().unwrap_or(Value::Null),
                            Value::Object(o) => o.get(c.key).cloned().unwrap_or(Value::Null),
                            _ => Value::Null,
                        }
                    };
                    if cols.iter().enumerate().all(|(i, c)| text_of(&cell(i, c)).is_empty()) {
                        continue;
                    }
                    let mut o = Map::new();
                    for (i, c) in cols.iter().enumerate() {
                        let cv = cell(i, c);
                        let t = text_of(&cv);
                        if t.is_empty() {
                            continue;
                        }
                        if c.numeric {
                            let n = t
                                .parse::<f64>()
                                .ok()
                                .filter(|x| x.is_finite())
                                .ok_or_else(|| bad(f, format!("{}: `{t}` is not a number", c.label)))?;
                            o.insert(c.key.into(), json!(n));
                        } else {
                            o.insert(c.key.into(), json!(t));
                        }
                    }
                    rows.push(Value::Object(o));
                }
                if f.key == "rates" {
                    // [{takeoff, unit, rate}] → {takeoff: {unit, rate}}
                    let mut m = Map::new();
                    for r in &rows {
                        let Some(name) = r["takeoff"].as_str() else { continue };
                        m.insert(name.into(), json!({ "unit": r["unit"], "rate": r["rate"] }));
                    }
                    out.insert("rates".into(), Value::Object(m));
                } else {
                    out.insert(f.key.into(), Value::Array(rows));
                }
            }
        }
    }
    Ok(match form.wrap {
        Some(k) => json!({ k: Value::Object(out) }),
        None => Value::Object(out),
    })
}

/// What the dialog starts with: the drawing's current values, else each field's default.
pub fn prefill(s: &Session, form: &Form) -> Value {
    let mut o = Map::new();
    for f in form.fields {
        if !f.default.is_empty() {
            o.insert(f.key.into(), if matches!(f.kind, Kind::Bool) { json!(f.default == "true") } else { json!(f.default) });
        }
    }
    let Ok(d) = s.doc() else { return Value::Object(o) };
    let cur = |k: &str| crate::cmd::project_value(d, k);
    match form.id {
        "project" => {
            if let Value::Object(det) = cur("details") {
                for (k, v) in det {
                    o.insert(k, v);
                }
            }
        }
        "floors" => {
            o.insert("floors".into(), Value::Array(crate::cmd::project_floors(d)));
        }
        "beams" => {
            if let Value::Array(a) = cur("beams") {
                o.insert("depths".into(), json!(a.iter().map(text_of).collect::<Vec<_>>().join(", ")));
            }
        }
        "schedule" => {
            if let Some(a) = cur("schedule").get("openings").cloned() {
                o.insert("openings".into(), a);
            }
        }
        "takeoff.rates" => {
            let rows: Vec<Value> = cur("schedule")
                .get("rates")
                .and_then(Value::as_object)
                .map(|m| m.iter().map(|(k, v)| json!({ "takeoff": k, "unit": v["unit"], "rate": v["rate"] })).collect())
                .unwrap_or_default();
            o.insert("rates".into(), Value::Array(rows));
        }
        _ => {}
    }
    Value::Object(o)
}

// ---------------- the prompt machine ----------------

/// Collects what a form still needs (settings typed on the command line, points picked on the
/// drawing), then makes the command's JSON call. Values already given (`preset`, from a dialog)
/// are not asked again.
pub struct FormRun {
    form: &'static Form,
    values: Map<String, Value>,
    /// Index into `form.fields` of the field being asked.
    at: usize,
    /// Points collected so far for a `Points` field.
    points: Vec<Value>,
}

impl FormRun {
    pub fn new(form: &'static Form, preset: &Value) -> Self {
        let values = preset.as_object().cloned().unwrap_or_default();
        let mut r = FormRun { form, values, at: 0, points: Vec::new() };
        r.skip_known();
        r
    }

    fn has(&self, f: &Field) -> bool {
        self.values.get(f.key).is_some_and(|v| !matches!(v, Value::Null) && !(v.is_string() && text_of(v).is_empty()))
    }

    /// Move to the next field that must be asked.
    fn skip_known(&mut self) {
        while let Some(f) = self.form.fields.get(self.at) {
            let ask = !self.has(f) && (f.required || f.ask);
            if ask {
                break;
            }
            self.at += 1;
        }
    }

    fn finish(&mut self, s: &mut Session) -> Result<Step> {
        let spec = crate::cmd::find_command(self.form.id).ok_or_else(|| EngineError::UnknownCommand(self.form.id.into()))?;
        // The running command records the single undo step. A refusal ends the command with the
        // reason shown, instead of leaving a prompt with nothing to answer.
        let run = parse_input(self.form, &Value::Object(self.values.clone())).and_then(|params| (spec.run)(s, &params));
        let r = match run {
            Ok(r) => r,
            Err(e) => {
                s.echo(e.to_string());
                return Ok(Step::Done);
            }
        };
        if let Some(m) = r.get("message").and_then(Value::as_str) {
            for l in m.lines() {
                s.echo(l.to_string());
            }
        }
        Ok(Step::Done)
    }

    fn advance(&mut self, s: &mut Session) -> Result<Step> {
        self.at += 1;
        self.skip_known();
        if self.at >= self.form.fields.len() { self.finish(s) } else { Ok(Step::Continue) }
    }
}

impl Interactive for FormRun {
    fn name(&self) -> &'static str {
        self.form.title
    }

    fn begin(&mut self, s: &mut Session) -> Result<Step> {
        if self.at >= self.form.fields.len() { self.finish(s) } else { Ok(Step::Continue) }
    }

    fn prompt(&self, _s: &Session) -> Prompt {
        let Some(f) = self.form.fields.get(self.at) else { return Prompt::new("Done", Accept::TEXT) };
        let mut p = match f.kind {
            Kind::Point => Prompt::new(f.label, Accept::POINT),
            Kind::Points => {
                let mut p = Prompt::new(format!("{} ({} picked)", f.label, self.points.len()), Accept::POINT);
                if let Some(Value::Array(last)) = self.points.last() {
                    p.base = Some(cadcraft_geom::Vec2::new(last[0].as_f64().unwrap_or(0.0), last[1].as_f64().unwrap_or(0.0)));
                }
                p
            }
            Kind::Number => Prompt::new(f.label, Accept::NUMBER),
            Kind::Choice(list) => Prompt::new(f.label, Accept::TEXT).kw(&list.iter().copied().filter(|c| !c.is_empty()).collect::<Vec<_>>()),
            _ => Prompt::new(f.label, Accept::TEXT),
        };
        if !f.default.is_empty() {
            p.default = Some(f.default.to_string());
        }
        p
    }

    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        let Some(f) = self.form.fields.get(self.at).copied() else { return Ok(Step::Done) };
        match (f.kind, i) {
            (Kind::Point, Input::Point(p)) => {
                self.values.insert(f.key.into(), json!([p.x, p.y]));
                self.advance(s)
            }
            (Kind::Points, Input::Point(p)) => {
                self.points.push(json!([p.x, p.y]));
                Ok(Step::Continue)
            }
            (Kind::Points, Input::Enter) => {
                if self.points.is_empty() && f.required {
                    return Ok(Step::Cancel);
                }
                if !self.points.is_empty() {
                    self.values.insert(f.key.into(), Value::Array(std::mem::take(&mut self.points)));
                }
                self.advance(s)
            }
            (_, Input::Enter) if !f.default.is_empty() => {
                self.values.insert(f.key.into(), json!(f.default));
                self.advance(s)
            }
            (_, Input::Enter) if f.required => Ok(Step::Cancel),
            (_, Input::Enter) => self.advance(s),
            (_, Input::Text(t) | Input::Keyword(t)) if !f.is_pick() => {
                self.values.insert(f.key.into(), json!(t));
                // Refuse a bad entry now, not after the last question.
                let all: &'static [Field] = self.form.fields;
                let one = Form { id: self.form.id, title: self.form.title, wrap: None, fields: all.get(self.at..=self.at).unwrap_or(&[]) };
                parse_input(&one, &Value::Object(self.values.clone())).inspect_err(|_| {
                    self.values.remove(f.key);
                })?;
                self.advance(s)
            }
            _ => Ok(Step::Continue),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_form_names_a_command_and_its_fields_are_unique() {
        for f in FORMS {
            assert!(crate::cmd::find_command(f.id).is_some(), "{}", f.id);
            let mut keys: Vec<&str> = f.fields.iter().map(|x| x.key).collect();
            keys.sort_unstable();
            keys.dedup();
            assert_eq!(keys.len(), f.fields.len(), "duplicate field in {}", f.id);
        }
        assert_eq!(FORMS.iter().map(|f| f.id).collect::<std::collections::BTreeSet<_>>().len(), FORMS.len());
    }

    #[test]
    fn parse_input_types_values_and_refuses_bad_ones() {
        let f = get("wall.door").unwrap();
        let p = parse_input(f, &json!({"name": "D2", "swing": "RIGHT", "deduct": false, "at": [1, 2], "width": ""})).unwrap();
        assert_eq!(p, json!({"name": "D2", "swing": "right", "deduct": false, "at": [1.0, 2.0]}));
        assert!(parse_input(f, &json!({"at": [1, 2], "swing": "sideways"})).is_err());
        assert!(parse_input(f, &json!({"at": [1, 2], "width": "abc"})).is_err());
        assert!(parse_input(f, &json!({"width": "900"})).is_err(), "the position is required");
        assert!(parse_input(f, &json!({"at": [1, "x"]})).is_err());
        let g = get("column.place").unwrap();
        assert_eq!(parse_input(g, &json!({"size": "230, 450"})).unwrap(), json!({"size": [230.0, 450.0]}));
        assert_eq!(parse_input(g, &json!({"size": "300"})).unwrap(), json!({"size": 300.0}));
        assert!(parse_input(g, &json!({"size": "1 2 3"})).is_err());
        assert!(parse_input(g, &json!({"size": "1,inf"})).is_err());
        let p = get("project").unwrap();
        assert_eq!(parse_input(p, &json!({"title": " House ", "owner": ""})).unwrap(), json!({"details": {"title": "House"}}));
    }

    #[test]
    fn tables_become_arrays_of_objects_and_rates_a_map() {
        let f = get("floors").unwrap();
        let p = parse_input(
            f,
            &json!({"floors": [["Ground", "3.15", "3.0", "2.1", "150", "450"], ["", "", "", "", "", ""], {"name": "First", "floorToFloor": 3}]}),
        )
        .unwrap();
        assert_eq!(p["floors"].as_array().unwrap().len(), 2, "blank rows are dropped");
        assert_eq!(p["floors"][0]["floorToFloor"], 3.15);
        assert!(parse_input(f, &json!({"floors": [["G", "tall"]]})).is_err());
        assert!(parse_input(f, &json!({"floors": []})).is_err());
        let r = get("takeoff.rates").unwrap();
        let p = parse_input(r, &json!({"rates": [["Bricks", "m3", "5200"], ["", "", ""]]})).unwrap();
        assert_eq!(p, json!({"rates": {"Bricks": {"unit": "m3", "rate": 5200.0}}}));
    }
}
