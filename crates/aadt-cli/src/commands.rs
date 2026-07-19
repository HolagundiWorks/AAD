//! CLI-registered commands that bridge the substrate command interpreter to the
//! file-I/O and AADT-engine layers.
//!
//! These live in the CLI (not `aadt-command`) so the substrate command core stays
//! free of engine dependencies: `OPEN`/`SAVE` use `aadt-dxf`, and `TAKEOFF` uses
//! `aadt-detect` + `aadt-takeoff`. They are registered onto an interpreter that
//! already carries the generic drawing built-ins.

use aadt_command::{Command, Context, Interpreter};
use aadt_detect::{detect, DetectionConfig};
use aadt_takeoff::TakeoffTable;

/// Register the file-I/O and engine commands onto an interpreter.
pub fn register(interp: &mut Interpreter) {
    let registry = interp.registry_mut();
    registry.register(Box::new(OpenCmd));
    registry.register(Box::new(SaveCmd));
    registry.register(Box::new(TakeoffCmd));
}

struct OpenCmd;
impl Command for OpenCmd {
    fn name(&self) -> &str {
        "OPEN"
    }
    fn help(&self) -> &str {
        "OPEN path.dxf — replace the document with an imported DXF"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let path = args.first().ok_or("OPEN: missing path")?;
        ctx.doc = aadt_dxf::import_path(path).map_err(|e| e.to_string())?;
        Ok(format!("opened {path} ({} entities)", ctx.doc.len()))
    }
}

struct SaveCmd;
impl Command for SaveCmd {
    fn name(&self) -> &str {
        "SAVE"
    }
    fn help(&self) -> &str {
        "SAVE path.dxf — write the document to a DXF file"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let path = args.first().ok_or("SAVE: missing path")?;
        aadt_dxf::export_path(&ctx.doc, path).map_err(|e| e.to_string())?;
        Ok(format!("saved {} entities to {path}", ctx.doc.len()))
    }
}

struct TakeoffCmd;
impl Command for TakeoffCmd {
    fn name(&self) -> &str {
        "TAKEOFF"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["AADT-TAKEOFF"]
    }
    fn help(&self) -> &str {
        "TAKEOFF [out.csv] — detect elements and compute quantities"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let detection = detect(&ctx.doc, &DetectionConfig::default());
        let table = TakeoffTable::from_detection(&detection);
        if let Some(path) = args.first() {
            std::fs::write(path, table.to_csv()).map_err(|e| format!("writing {path}: {e}"))?;
        }
        Ok(format!(
            "{} walls, {} rooms; wall length {:.3}, room area {:.3}",
            table.walls.len(),
            table.rooms.len(),
            table.total_wall_length(),
            table.total_room_area(),
        ))
    }
}
