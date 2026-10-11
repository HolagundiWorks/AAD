//! The allow-listed tool registry. Each tool is one existing engine command with a strict
//! argument schema; the model never reaches anything outside this list.

use cadcraft_engine::Session;
use serde_json::{Map, Value, json};

/// How much a call can change, which decides whether the user must confirm it first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Risk {
    /// Reads the drawing only.
    Read,
    /// Opens another drawing (a repaired copy of a file); nothing existing is changed.
    Open,
    /// Changes the open drawing (one undo step).
    Mutating,
    /// Removes definitions from the open drawing (one undo step).
    Destructive,
}

impl Risk {
    /// Whether the application must get the user's confirmation before running the call.
    pub fn needs_confirmation(self) -> bool {
        self >= Risk::Mutating
    }
}

type RiskFn = fn(&Map<String, Value>) -> Risk;
type RunFn = fn(&mut Session, &Map<String, Value>) -> Result<Value, String>;

/// A registered tool.
pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    /// JSON Schema (draft 2020-12 subset, see [`crate::validate`]) of the arguments.
    pub schema: fn() -> Value,
    pub risk: RiskFn,
    /// A short description of what a confirmation would allow, shown to the user.
    pub summary: fn(&Map<String, Value>) -> String,
    pub run: RunFn,
}

/// Tools that are planned but not built. They are not offered to the model and a call to one is
/// refused as not implemented.
pub const PLANNED: &[&str] =
    &["find_openings", "generate_schedule", "run_quantity_takeoff", "get_takeoff_result", "check_missing_attributes", "export_table"];

fn exec(s: &mut Session, id: &str, args: Value) -> Result<Value, String> {
    s.execute(id, &args).map_err(|e| e.to_string())
}

fn empty_schema() -> Value {
    json!({ "type": "object", "properties": {}, "additionalProperties": false })
}

fn purge_types() -> Vec<&'static str> {
    vec!["all", "blocks", "dimstyles", "groups", "layers", "linetypes", "mleaderstyles", "tablestyles", "textstyles", "zerolength", "emptytext"]
}

/// The registered tools, in a stable order.
pub fn registry() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "get_drawing_summary",
            description: "Report the open drawing's units, layers, blocks, object counts and what flattening did when it was opened.",
            schema: empty_schema,
            risk: |_| Risk::Read,
            summary: |_| "Read the drawing summary".into(),
            run: |s, _| {
                let mut r = exec(s, "drawing.inspect", json!({}))?;
                if let (Some(o), Some(f)) = (r.as_object_mut(), s.last_flatten.as_ref()) {
                    o.insert("flattenOnImport".into(), f.to_json());
                }
                Ok(r)
            },
        },
        ToolDef {
            name: "flatten_drawing",
            description: "Flatten the whole open drawing to 2D (elevations, thickness and z values removed). Reports 3D objects that cannot be flattened. One undo step.",
            schema: empty_schema,
            risk: |_| Risk::Mutating,
            summary: |_| "Flatten every object of the open drawing to 2D".into(),
            run: |s, _| exec(s, "flatten", json!({ "all": true })),
        },
        ToolDef {
            name: "audit_drawing",
            description: "Check the open drawing for database errors. With fix=true, repair them (one undo step); without it, only report.",
            schema: || {
                json!({
                    "type": "object",
                    "properties": { "fix": { "type": "boolean", "description": "repair the errors found (default false: report only)" } },
                    "additionalProperties": false
                })
            },
            risk: |a| if a.get("fix") == Some(&Value::Bool(true)) { Risk::Mutating } else { Risk::Read },
            summary: |a| {
                if a.get("fix") == Some(&Value::Bool(true)) {
                    "Audit the drawing and repair the errors found".into()
                } else {
                    "Audit the drawing (report only)".into()
                }
            },
            run: |s, a| exec(s, "audit", json!({ "fix": a.get("fix").and_then(Value::as_bool).unwrap_or(false) })),
        },
        ToolDef {
            name: "purge_unused",
            description: "Remove unused named definitions (blocks, layers, linetypes, styles, groups) from the open drawing. One undo step.",
            schema: || {
                json!({
                    "type": "object",
                    "properties": {
                        "type": { "type": "string", "enum": purge_types(), "description": "kind of definition to purge (default all)" },
                        "names": { "type": "string", "maxLength": 256, "description": "name pattern such as \"A*,B\" (default *)" }
                    },
                    "additionalProperties": false
                })
            },
            risk: |_| Risk::Destructive,
            summary: |a| {
                format!(
                    "Remove unused {} definitions matching `{}`",
                    a.get("type").and_then(Value::as_str).unwrap_or("all"),
                    a.get("names").and_then(Value::as_str).unwrap_or("*")
                )
            },
            run: |s, a| exec(s, "purge", Value::Object(a.clone())),
        },
        ToolDef {
            name: "recover_file",
            description: "Open a damaged DXF or DWG file as a repaired copy in a new tab. The file on disk is not changed.",
            schema: || {
                json!({
                    "type": "object",
                    "properties": { "path": { "type": "string", "minLength": 1, "maxLength": 1024, "description": "path of the file to recover" } },
                    "required": ["path"],
                    "additionalProperties": false
                })
            },
            risk: |_| Risk::Open,
            summary: |a| format!("Recover {}", a.get("path").and_then(Value::as_str).unwrap_or("a file")),
            run: |s, a| exec(s, "recover", Value::Object(a.clone())),
        },
    ]
}

/// The tool named `name`.
pub fn find(name: &str) -> Option<ToolDef> {
    registry().into_iter().find(|t| t.name == name)
}

/// The tools in the function-calling format of the model's chat template.
pub fn function_schemas() -> Value {
    Value::Array(
        registry()
            .iter()
            .map(|t| json!({ "type": "function", "function": { "name": t.name, "description": t.description, "parameters": (t.schema)() } }))
            .collect(),
    )
}
