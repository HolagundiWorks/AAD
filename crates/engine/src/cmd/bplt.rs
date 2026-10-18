//! Building Permission Layer Tool (BPLT), carried over from the Holgundi Consulting Works
//! AutoLISP toolkit (`hcwtools.lsp`, sections 6 and 9) for the BBMP BPAS / AutoPlan submission
//! workflow: BPLTSTART (metres, layers, styles), BPLTLAYERS (legend), BPLTCOPY, BPLTCHECK,
//! BPLTAREA, BPLTREPORT and BPLTTITLEBLOCK.
//!
//! The drawing is kept in metres at 1:1. `BP-` layers are drafted and plotted; `AP-` layers hold
//! the closed polylines AutoPlan marks and never plot.

use aad_doc::{Drawing, EntityKind, HAlign, Handle, Layer, Lineweight, MText, TextStyle, VAlign};
use aad_geom::{Mat3, Polyline, Vec2};
use serde_json::{Value, json};

use super::draw::make_text;
use super::hcw::{add_on, drawing_units};
use super::helpers::{line, lwpoly, rect_vertices, v3};
use super::*;
use crate::{Result, Session};

/// Name, ACI colour, linetype, lineweight (hundredths of mm), legend text. In legend order: the
/// first `COLUMN_1` rows are the first legend column, the next up to `COLUMN_2` the second, the
/// rest (the `AP-` layers) the third.
const LAYERS: &[(&str, i16, &str, u16, &str)] = &[
    ("BP-SITE-BOUNDARY", 1, "Continuous", 50, "Plot / site outer boundary"),
    ("BP-SITE-SETBACK", 3, "DASHED2", 25, "Setback / margin lines"),
    ("BP-SITE-SURRENDER", 3, "DASHED", 18, "Area surrendered for road"),
    ("BP-ROAD-EDGE", 8, "Continuous", 35, "Road carriageway edge"),
    ("BP-ROAD-CL", 8, "CENTER2", 18, "Road centreline"),
    ("BP-ROAD-WIDENING", 8, "DASHED", 18, "Proposed road widening"),
    ("BP-DRIVEWAY", 52, "Continuous", 25, "Vehicular driveway"),
    ("BP-COMP-WALL", 9, "Continuous", 18, "Compound / boundary wall"),
    ("BP-BUILDING-CUT", 7, "Continuous", 50, "Walls at cut plane"),
    ("BP-BUILDING-VIEW", 7, "Continuous", 25, "Elements visible below cut"),
    ("BP-BUILDING-HATCH", 7, "Continuous", 13, "Section poche / hatch"),
    ("BP-DOOR", 252, "Continuous", 18, "Door openings"),
    ("BP-WINDOW", 252, "Continuous", 13, "Window openings"),
    ("BP-STAIR", 142, "Continuous", 25, "Staircase"),
    ("BP-LIFT", 141, "Continuous", 18, "Lift / elevator shaft"),
    ("BP-RAMP", 140, "Continuous", 18, "Vehicular / accessible ramp"),
    ("BP-ESCALATOR", 140, "Continuous", 13, "Escalator"),
    ("BP-CORRIDOR", 143, "Continuous", 18, "Corridor / passage"),
    ("BP-ROOM", 153, "Continuous", 18, "Room outlines"),
    ("BP-TOILET", 153, "Continuous", 13, "Toilet / bathroom"),
    ("BP-BALCONY", 144, "Continuous", 18, "Balcony slab"),
    ("BP-TERRACE", 96, "Continuous", 18, "Roof terrace"),
    ("BP-PORCH", 22, "Continuous", 18, "Entrance porch"),
    ("BP-LOFT", 33, "DASHED", 13, "Loft (DASHED = below slab)"),
    ("BP-MEZZANINE", 33, "Continuous", 18, "Mezzanine / service floor"),
    ("BP-VENT-SHAFT", 132, "Continuous", 18, "Ventilation shaft"),
    ("BP-DUCT", 4, "Continuous", 18, "Service duct"),
    ("BP-CHOWK", 4, "Continuous", 18, "Internal light well / chowk"),
    ("BP-OPEN-SPACE", 3, "Continuous", 18, "Organised open space"),
    ("BP-SETBACK-ZONE", 3, "DASHED2", 13, "Setback shading reference"),
    ("BP-PARKING", 151, "Continuous", 18, "Car parking spaces"),
    ("BP-PARKING-2W", 151, "Continuous", 13, "Two-wheeler parking"),
    ("BP-PARKING-AISLE", 151, "DASHED", 13, "Manoeuvring aisle"),
    ("BP-RWH", 133, "Continuous", 18, "Rainwater harvesting pit/tank"),
    ("BP-WATER-SUPPLY", 134, "Continuous", 13, "Water supply line"),
    ("BP-SEWAGE", 10, "DASHED", 13, "Sewage / drainage line"),
    ("BP-WATER-TANK", 93, "Continuous", 18, "Overhead / underground tank"),
    ("BP-FIRE-EXIT", 1, "DASHED", 25, "Fire exit route"),
    ("BP-FIRE-EQUIP", 1, "Continuous", 13, "Fire extinguisher / hose reel"),
    ("BP-LEVEL-LINE", 6, "Continuous", 13, "Floor / slab level lines"),
    ("BP-GROUND-LINE", 6, "Continuous", 25, "Ground Line GL (height datum)"),
    ("BP-PLANTATION", 62, "Continuous", 13, "Plantation / trees"),
    ("BP-GARAGE", 152, "Continuous", 18, "Garage"),
    ("BP-ACCESSORY", 152, "Continuous", 13, "Accessory building"),
    ("BP-PwD", 94, "Continuous", 13, "Facilities - physically challenged"),
    ("BP-FAR-ZONE", 6, "Continuous", 13, "FSI / FAR zone overlay"),
    ("BP-COVERAGE", 4, "Continuous", 13, "Ground coverage overlay"),
    ("BP-HEIGHT-LIMIT", 5, "DASHED", 13, "Height restriction reference"),
    ("BP-SHEET-BORDER", 8, "Continuous", 50, "Drawing sheet border"),
    ("BP-TITLE-BLOCK", 8, "Continuous", 25, "Title block lines"),
    ("BP-NORTH", 2, "Continuous", 25, "North point"),
    ("BP-SURVEY-SKETCH", 9, "Continuous", 13, "Survey reference sketch"),
    ("BP-SECTION", 30, "Continuous", 35, "Building section"),
    ("BP-ELEVATION", 30, "Continuous", 25, "Building elevation"),
    ("BP-DIM", 2, "Continuous", 13, "Dimensions"),
    ("BP-DIM-CHAIN", 2, "Continuous", 13, "Chain dimensioning"),
    ("BP-TEXT-GENERAL", 2, "Continuous", 13, "General annotation text"),
    ("BP-TEXT-ROOM", 2, "Continuous", 13, "Room labels and areas"),
    ("BP-TEXT-LEVEL", 2, "Continuous", 13, "Level annotations"),
    ("BP-AREA-TABLE", 2, "Continuous", 13, "Area statement table"),
    ("BP-NOTES", 2, "Continuous", 13, "General notes"),
    ("BP-STRUC-REF", 3, "Continuous", 13, "Structural reference lines"),
    ("BP-REVISION", 1, "Continuous", 25, "Revision clouds / marks"),
    ("BP-EXISTING", 251, "Continuous", 13, "Existing structure (to retain)"),
    ("BP-DEMOLISH", 251, "DASHED", 13, "Existing structure (to demolish)"),
    ("AP-DRAWING-BOUND", 253, "Continuous", 13, "AutoPlan: Mark Drawing Bound"),
    ("AP-SITE-GROSS", 253, "Continuous", 13, "AutoPlan: Mark Gross Site Area"),
    ("AP-SITE-NET", 253, "Continuous", 13, "AutoPlan: Mark Net Site Area"),
    ("AP-SITE-SURR", 253, "Continuous", 13, "AutoPlan: Mark Surrendered Area"),
    ("AP-ROAD", 253, "Continuous", 13, "AutoPlan: Mark Road Side1/Side2/CL"),
    ("AP-MEANS-ACCESS", 253, "Continuous", 13, "AutoPlan: Mark Means of Access"),
    ("AP-PARAPET", 253, "Continuous", 13, "AutoPlan: Mark Parapet levels"),
    ("AP-BLDG-BOUNDARY", 253, "Continuous", 13, "AutoPlan: Mark Building Boundary"),
    ("AP-FLOOR-LEVEL", 253, "Continuous", 13, "AutoPlan: Mark Floor Level lines"),
    ("AP-BLDG-AREA", 253, "Continuous", 13, "AutoPlan: Mark Building Area (FAR)"),
    ("AP-ROOM", 253, "Continuous", 13, "AutoPlan: Mark Room Size"),
    ("AP-PREMISES", 253, "Continuous", 13, "AutoPlan: Mark Premises / units"),
    ("AP-STAIRCASE", 253, "Continuous", 13, "AutoPlan: Mark Staircase"),
    ("AP-RAMP", 253, "Continuous", 13, "AutoPlan: Mark Ramp"),
    ("AP-LIFT", 253, "Continuous", 13, "AutoPlan: Mark Lift"),
    ("AP-PARKING", 253, "Continuous", 13, "AutoPlan: Mark Parking Spaces"),
    ("AP-PROJECTION", 253, "Continuous", 13, "AutoPlan: Mark Projection in Open Space"),
    ("AP-RWH", 253, "Continuous", 13, "AutoPlan: Mark Rain Water Harvesting"),
    ("AP-OPEN-SPACE", 253, "Continuous", 13, "AutoPlan: Mark Open Space / Vent Shaft"),
    ("AP-COMP-WALL", 253, "Continuous", 13, "AutoPlan: Mark Boundary Wall"),
    ("AP-MEZZANINE", 253, "Continuous", 13, "AutoPlan: Mark Service / Mezzanine Floor"),
    ("AP-PORCH", 253, "Continuous", 13, "AutoPlan: Mark Porch"),
    ("AP-PLANTATION", 253, "Continuous", 13, "AutoPlan: Mark Plantation"),
    ("AP-GARAGE", 253, "Continuous", 13, "AutoPlan: Mark Garage"),
    ("AP-PwD", 253, "Continuous", 13, "AutoPlan: Mark Facilities for PwD"),
    ("AP-ACCESSORY", 253, "Continuous", 13, "AutoPlan: Mark Accessory Building"),
    ("AP-DRIVEWAY", 253, "Continuous", 13, "AutoPlan: Mark Driveway"),
];
const COLUMN_1: usize = 30;
const COLUMN_2: usize = 65;

/// `AP-` layers a submission needs at least one closed polyline on.
const REQUIRED_AP: &[&str] =
    &["AP-DRAWING-BOUND", "AP-SITE-GROSS", "AP-SITE-NET", "AP-ROAD", "AP-BLDG-BOUNDARY", "AP-FLOOR-LEVEL", "AP-BLDG-AREA", "AP-ROOM"];

const TEXT_STYLES: &[&str] = &["BPLT-TEXT", "BPLT-TITLE", "BPLT-DIM-TXT"];
const MAX_OBJECTS: usize = 5_000_000;
const MAX_COPIES: usize = 100_000;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("bpltstart", "BPLT Start", run_start)
            .menu(&["Architecture", "Building Permission", "Start (Metres, Layers, Styles)"])
            .params("{rescale?: bool (default true: scale the model to metres from the drawing's units)} → {factor, scaled, created, updated} (metres at 1:1; the 92 BP-/AP- layers; BPLT text styles and the BPLT-DIM dimension style; current layer BP-BUILDING-CUT)"),
        CommandSpec::new("bpltlayers", "BPLT Layer Legend", run_legend)
            .menu(&["Architecture", "Building Permission", "Layer Legend"])
            .params("{at: [x, y], units?} → {rows} (the BP- and AP- layers with swatch and description, three columns, and the AutoPlan usage notes)"),
        CommandSpec::new("bpltcopy", "Copy to AP- Layers", run_copy)
            .menu(&["Architecture", "Building Permission", "Copy to AP- Layers"])
            .params("{handles?} → {copied, skipped, handles} (each object on BP-name is copied to AP-name when that layer exists)"),
        CommandSpec::new("bpltcheck", "Check Before AutoPlan", run_check)
            .menu(&["Architecture", "Building Permission", "Check Before AutoPlan"])
            .params("{} → {result: PASS|WARN|FAIL, required: [{layer, closed}], open: [{layer, count}]} (the eight required AP- layers each need a closed polyline; open polylines on AP- layers are a warning)")
            .noundo(),
        CommandSpec::new("bpltarea", "Area Statement", run_area)
            .menu(&["Architecture", "Building Permission", "Area Statement"])
            .params("{at?: [x, y] (places the table when given), units?} → {layers: [{layer, area}], total, unit} (closed polylines on AP- layers, in square metres)"),
        CommandSpec::new("bpltreport", "BPLT Layer Report", run_report)
            .menu(&["Architecture", "Building Permission", "Layer Report (CSV)"])
            .params("{path?, units?} → {count, csv?, path?} (Layer, Type, ObjectCount, ClosedPolyArea_m2, Plottable)")
            .noundo(),
        CommandSpec::new("bplttitleblock", "BPLT Title Block", run_titleblock)
            .menu(&["Architecture", "Building Permission", "Title Block"])
            .params("{at: [x, y] (bottom-left), size?: A0|A1|A2 (default A1)} → {size} (sheet border and title strip, in metres)"),
    ]
}

fn is_ap(name: &str) -> bool {
    name.get(..3).is_some_and(|p| p.eq_ignore_ascii_case("AP-"))
}

fn is_bp(name: &str) -> bool {
    name.get(..3).is_some_and(|p| p.eq_ignore_ascii_case("BP-"))
}

/// Create or update every BPLT layer; `AP-` layers do not plot. Returns (created, updated).
fn make_layers(d: &mut Drawing) -> (usize, usize) {
    let (mut created, mut updated) = (0, 0);
    for (name, aci, lt, lw, description) in LAYERS {
        if d.linetype(lt).is_none()
            && let Some(l) = aad_doc::library::standard_linetypes_for(d).into_iter().find(|l| l.name.eq_ignore_ascii_case(lt))
        {
            d.linetypes.push(l);
        }
        let linetype = d.linetype(lt).map(|t| t.name.clone()).unwrap_or_else(|| "Continuous".into());
        let plot = !is_ap(name);
        let color = aad_color::Color::from_aci(*aci);
        let weight = Lineweight::Mm100(*lw);
        match d.layers.iter_mut().find(|l| l.name == *name) {
            Some(l) => {
                if l.color != color || l.linetype != linetype || l.lineweight != weight || l.plot != plot || l.description != *description {
                    updated += 1;
                }
                (l.color, l.linetype, l.lineweight, l.plot, l.description) = (color, linetype, weight, plot, (*description).into());
            }
            None => {
                let mut l = Layer::new(name);
                (l.color, l.linetype, l.lineweight, l.plot, l.description) = (color, linetype, weight, plot, (*description).into());
                d.layers.push(l);
                created += 1;
            }
        }
    }
    (created, updated)
}

fn make_text_styles(d: &mut Drawing) {
    for name in TEXT_STYLES {
        if !d.text_styles.iter().any(|t| t.name == *name) {
            d.text_styles.push(TextStyle { name: (*name).into(), font: "Arial".into(), ..TextStyle::default() });
        }
    }
}

/// The scale to metres for an `INSUNITS` value; unset or unknown is taken as millimetres.
fn metres_per_unit(insunits: i64) -> f64 {
    match insunits {
        1 => 0.0254,
        2 => 0.3048,
        5 => 0.01,
        6 => 1.0,
        _ => 0.001,
    }
}

fn run_start(s: &mut Session, p: &Value) -> Result<Value> {
    let factor = metres_per_unit(s.doc()?.header.i64("INSUNITS", 0));
    let mut scaled = 0usize;
    let mut skipped = 0usize;
    if factor != 1.0 && bool_or(p, "rescale", true) {
        let m = Mat3::scale_about(Vec2::new(0.0, 0.0), factor);
        let hs: Vec<Handle> = s.doc()?.model.iter().map(|e| e.handle).take(MAX_OBJECTS).collect();
        let free: Vec<Handle> = hs.iter().copied().filter(|h| !curves::is_locked(s, *h)).collect();
        skipped = hs.len() - free.len();
        scaled = modify::transform_entities(s, &free, &m, false)?.len();
    }
    let d = s.doc_mut()?;
    for (k, v) in
        [("LUNITS", 2), ("LUPREC", 3), ("AUNITS", 0), ("AUPREC", 2), ("INSUNITS", 6), ("MEASUREMENT", 1), ("PSLTSCALE", 1), ("MSLTSCALE", 1)]
    {
        d.header.set_i64(k, v);
    }
    d.header.set_f64("LTSCALE", 1.0);
    d.header.set_f64("TEXTSIZE", 0.20);
    let (created, updated) = make_layers(d);
    make_text_styles(d);
    let style = aad_doc::DimStyle {
        name: "BPLT-DIM".into(),
        text_height: 0.20,
        arrow_size: 0.10,
        text_gap: 0.05,
        ext_offset: 0.10,
        ext_extend: 0.10,
        dim_line_extend: 0.0,
        baseline_spacing: 0.50,
        decimals: 3,
        linear_factor: 1.0,
        linear_unit: 2,
        text_above: 1,
        decimal_separator: ".".into(),
        text_style: "BPLT-DIM-TXT".into(),
        ..aad_doc::DimStyle::default()
    };
    match d.dim_styles.iter_mut().find(|x| x.name == "BPLT-DIM") {
        Some(existing) => *existing = style,
        None => d.dim_styles.push(style),
    }
    d.header.set_str("DIMSTYLE", "BPLT-DIM");
    d.header.set_str("CLAYER", "BP-BUILDING-CUT");
    s.echo("BPLT set up: metres at 1:1, BP-/AP- layers, BPLT-DIM. Unlock all layers before opening AutoPlan.".to_string());
    Ok(json!({ "factor": factor, "scaled": scaled, "skippedLocked": skipped, "created": created, "updated": updated }))
}

/// One legend row: a swatch and a mid-height line on the layer, and the layer name and meaning.
fn legend_row(s: &mut Session, name: &str, description: &str, at: Vec2, k: f64) -> Result<()> {
    let (w, h, text_h, dx) = (1.6 * k, 0.55 * k, 0.3 * k, 0.25 * k);
    add_on(s, name, lwpoly(rect_vertices(at, Vec2::new(at.x + w, at.y + h)), true))?;
    add_on(s, name, line(Vec2::new(at.x, at.y + h / 2.0), Vec2::new(at.x + w, at.y + h / 2.0)))?;
    let label = make_text(
        s,
        Vec2::new(at.x + w + dx, at.y + (h - text_h) / 2.0),
        &format!("{name}  |  {description}"),
        text_h,
        0.0,
        (HAlign::Left, VAlign::Baseline),
    );
    add_on(s, "BP-TEXT-GENERAL", label)?;
    Ok(())
}

fn run_legend(s: &mut Session, p: &Value) -> Result<Value> {
    let at = point_req("bpltlayers", p, "at")?;
    let k = 1.0 / drawing_units(s.doc()?, "bpltlayers", p)?.metres();
    {
        let d = s.doc_mut()?;
        make_layers(d);
        make_text_styles(d);
    }
    let columns = [&LAYERS[..COLUMN_1], &LAYERS[COLUMN_1..COLUMN_2], &LAYERS[COLUMN_2..]];
    let titles = [
        "BP- DRAFTING LAYERS (Site / Building / Circulation)",
        "BP- DRAFTING LAYERS (Services / Annotations / Admin)",
        "AP- AUTOPLAN MARKING HELPERS  (non-plotting)",
    ];
    let (row_h, col_w, text_h) = (0.8 * k, 16.0 * k, 0.3 * k);
    let rows = columns.iter().map(|c| c.len()).max().unwrap_or(0);
    let header_y = at.y + rows as f64 * row_h + row_h;
    for (i, (column, title)) in columns.iter().zip(titles).enumerate() {
        let x = at.x + i as f64 * col_w;
        add_on(s, "BP-TITLE-BLOCK", line(Vec2::new(x, header_y + 0.65 * k), Vec2::new(x + col_w + 0.15 * k, header_y + 0.65 * k)))?;
        let t = make_text(s, Vec2::new(x, header_y), title, text_h * 1.4, 0.0, (HAlign::Left, VAlign::Baseline));
        add_on(s, "BP-TITLE-BLOCK", t)?;
        for (r, layer) in column.iter().enumerate() {
            legend_row(s, layer.0, layer.4, Vec2::new(x, at.y + (column.len() - 1 - r) as f64 * row_h), k)?;
        }
    }
    let (x1, y1) = (at.x + 3.0 * col_w + 0.5 * k, header_y + row_h);
    add_on(s, "BP-SHEET-BORDER", lwpoly(rect_vertices(Vec2::new(at.x - 0.5 * k, at.y - 0.5 * k), Vec2::new(x1, y1)), true))?;
    for i in 1..3 {
        let x = at.x + i as f64 * col_w - 0.3 * k;
        add_on(s, "BP-TITLE-BLOCK", line(Vec2::new(x, at.y - 0.3 * k), Vec2::new(x, y1 + 0.3 * k)))?;
    }
    let title = make_text(
        s,
        Vec2::new(at.x, y1 + 0.4 * k),
        "Building Permission Layer Tool - Layer Reference Legend v1.0",
        text_h * 1.8,
        0.0,
        (HAlign::Left, VAlign::Baseline),
    );
    add_on(s, "BP-TITLE-BLOCK", title)?;
    let notes = [
        "HOW TO USE WITH AUTOPLAN AUTHOR:",
        "  1. LAYISO on the required AP- layer, picking any object on it",
        "  2. The matching swatch polyline becomes the only selectable object on screen",
        "  3. In AutoPlan Author click Mark, then pick the swatch (or your closed polyline on the same AP- layer)",
        "  4. Run LAYUNISO to restore all layers after marking is complete",
        "  NOTE: AP- layers are non-plotting; they never appear on printed drawings",
    ];
    for (i, note) in notes.iter().enumerate() {
        let t = make_text(s, Vec2::new(at.x, at.y - (i + 1) as f64 * row_h - 0.5 * k), note, text_h * 0.85, 0.0, (HAlign::Left, VAlign::Baseline));
        add_on(s, "BP-NOTES", t)?;
    }
    Ok(json!({ "rows": LAYERS.len() }))
}

fn run_copy(s: &mut Session, p: &Value) -> Result<Value> {
    let hs = targets(s, p)?;
    if hs.is_empty() {
        return Err(bad("bpltcopy", "select objects on BP- layers"));
    }
    if hs.len() > MAX_COPIES {
        return Err(bad("bpltcopy", format!("select at most {MAX_COPIES} objects")));
    }
    let (mut copied, mut skipped) = (Vec::new(), 0usize);
    let d = s.doc_mut()?;
    for h in hs {
        let Some(e) = d.entity(h).map(|e| (**e).clone()) else {
            skipped += 1;
            continue;
        };
        let ap = format!("AP-{}", e.common.layer.get(3..).unwrap_or(""));
        if !is_bp(&e.common.layer) || d.layer(&ap).is_none() {
            skipped += 1;
            continue;
        }
        let Some(space) = d.space_of(h) else {
            skipped += 1;
            continue;
        };
        let nh = d.new_handle();
        let mut common = e.common.clone();
        common.layer = ap;
        if let Some(store) = d.space_mut(&space) {
            store.push(aad_doc::Entity { handle: nh, common, kind: e.kind.clone() });
            copied.push(nh.hex());
        }
    }
    Ok(json!({ "copied": copied.len(), "skipped": skipped, "handles": copied }))
}

/// Closed and open polyline counts per `AP-` layer, over every space.
fn ap_polylines(d: &Drawing) -> (std::collections::BTreeMap<String, usize>, std::collections::BTreeMap<String, usize>) {
    let (mut closed, mut open) = (std::collections::BTreeMap::new(), std::collections::BTreeMap::new());
    let stores = std::iter::once(&d.model).chain(d.layouts.iter().map(|l| &l.entities));
    for e in stores.flat_map(|st| st.iter()).take(MAX_OBJECTS) {
        if let (true, EntityKind::LwPolyline(pl)) = (is_ap(&e.common.layer), &e.kind) {
            *(if pl.closed { &mut closed } else { &mut open }).entry(e.common.layer.clone()).or_insert(0usize) += 1;
        }
    }
    (closed, open)
}

fn run_check(s: &mut Session, _p: &Value) -> Result<Value> {
    let (closed, open) = ap_polylines(s.doc()?);
    let required: Vec<Value> = REQUIRED_AP.iter().map(|l| json!({ "layer": l, "closed": closed.get(*l).copied().unwrap_or(0) })).collect();
    let missing = REQUIRED_AP.iter().filter(|l| !closed.contains_key(**l)).count();
    let (result, message) = if missing > 0 {
        ("FAIL", "missing required layers")
    } else if !open.is_empty() {
        ("WARN", "open polylines present")
    } else {
        ("PASS", "ready for AutoPlan")
    };
    let open: Vec<Value> = open.into_iter().map(|(layer, count)| json!({ "layer": layer, "count": count })).collect();
    s.echo(format!("BPLTCHECK {result}: {message}."));
    Ok(json!({ "result": result, "message": message, "required": required, "open": open }))
}

/// Area of closed polylines per layer, in square metres.
fn closed_areas(d: &Drawing, keep: impl Fn(&str) -> bool, m2_per_unit2: f64) -> std::collections::BTreeMap<String, f64> {
    let mut out = std::collections::BTreeMap::new();
    let stores = std::iter::once(&d.model).chain(d.layouts.iter().map(|l| &l.entities));
    for e in stores.flat_map(|st| st.iter()).take(MAX_OBJECTS) {
        if let EntityKind::LwPolyline(pl) = &e.kind
            && pl.closed
            && keep(&e.common.layer)
        {
            let a = Polyline { vertices: pl.vertices.clone(), closed: true }.area().abs() * m2_per_unit2;
            if a.is_finite() {
                *out.entry(e.common.layer.clone()).or_insert(0.0) += a;
            }
        }
    }
    out
}

fn run_area(s: &mut Session, p: &Value) -> Result<Value> {
    let u = drawing_units(s.doc()?, "bpltarea", p)?.metres();
    let areas = closed_areas(s.doc()?, is_ap, u * u);
    if areas.is_empty() {
        return Err(bad("bpltarea", "no closed polylines on AP- layers"));
    }
    let total: f64 = areas.values().sum();
    if p.get("at").is_some_and(|v| !v.is_null()) {
        let at = point_req("bpltarea", p, "at")?;
        make_layers(s.doc_mut()?);
        let mut text = String::from("AREA STATEMENT\\P");
        text.push_str(&format!("{:<26}{:<14}\\P{}\\P", "Layer", "Area (m2)", "-".repeat(38)));
        for (layer, a) in &areas {
            text.push_str(&format!("{layer:<26}{a:<14.3}\\P"));
        }
        text.push_str(&format!("{}\\P{:<26}{total:.3} m2", "-".repeat(38), "TOTAL"));
        let style = s
            .doc()?
            .text_style("BPLT-TEXT")
            .map(|t| t.name.clone())
            .unwrap_or_else(|| s.doc().map(|d| d.header.str("TEXTSTYLE", "Standard")).unwrap_or_else(|_| "Standard".into()));
        let k = EntityKind::MText(MText {
            insert: v3(at),
            height: 0.20 / u,
            width: 12.0 / u,
            attach: 1,
            rotation: 0.0,
            style,
            contents: text,
            line_spacing: 1.0,
            line_spacing_exact: false,
        });
        add_on(s, "BP-AREA-TABLE", k)?;
    }
    let layers: Vec<Value> = areas.iter().map(|(layer, area)| json!({ "layer": layer, "area": area })).collect();
    Ok(json!({ "layers": layers, "total": total, "unit": "m2" }))
}

fn csv_field(v: &str) -> String {
    if v.contains([',', '"', '\n', '\r']) { format!("\"{}\"", v.replace('"', "\"\"")) } else { v.to_string() }
}

fn run_report(s: &mut Session, p: &Value) -> Result<Value> {
    let u = drawing_units(s.doc()?, "bpltreport", p)?.metres();
    let d = s.doc()?;
    let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    let stores = std::iter::once(&d.model).chain(d.layouts.iter().map(|l| &l.entities));
    for e in stores.flat_map(|st| st.iter()).take(MAX_OBJECTS) {
        if is_bp(&e.common.layer) || is_ap(&e.common.layer) {
            *counts.entry(e.common.layer.clone()).or_insert(0) += 1;
        }
    }
    let areas = closed_areas(d, |l| is_bp(l) || is_ap(l), u * u);
    let mut csv = String::from("Layer,Type,ObjectCount,ClosedPolyArea_m2,Plottable\n");
    for (layer, n) in &counts {
        let plottable = d.layer(layer).map_or("?", |l| if l.plot { "Yes" } else { "No" });
        let kind = layer.get(..2).unwrap_or("").to_ascii_uppercase();
        csv.push_str(&format!("{},{kind},{n},{:.4},{plottable}\n", csv_field(layer), areas.get(layer).copied().unwrap_or(0.0)));
    }
    match str_param(p, "path") {
        Some(_path) => {
            #[cfg(not(target_arch = "wasm32"))]
            {
                std::fs::write(_path, &csv).map_err(|e| bad("bpltreport", format!("{_path}: {e}")))?;
                return Ok(json!({ "count": counts.len(), "path": _path }));
            }
            #[cfg(target_arch = "wasm32")]
            Err(bad("bpltreport", "paths are not available on the web; omit `path` to get the CSV text"))
        }
        None => Ok(json!({ "count": counts.len(), "csv": csv })),
    }
}

fn run_titleblock(s: &mut Session, p: &Value) -> Result<Value> {
    let at = point_req("bplttitleblock", p, "at")?;
    let size = str_param(p, "size").unwrap_or("A1").to_ascii_uppercase();
    // Sheet sizes in metres.
    let (w, h) = match size.as_str() {
        "A0" => (1.189, 0.841),
        "A1" => (0.841, 0.594),
        "A2" => (0.594, 0.420),
        _ => return Err(bad("bplttitleblock", "`size` must be A0, A1 or A2")),
    };
    let k = 1.0 / drawing_units(s.doc()?, "bplttitleblock", p)?.metres();
    make_layers(s.doc_mut()?);
    make_text_styles(s.doc_mut()?);
    let (w, h, strip) = (w * k, h * k, 0.060 * k);
    let border = add_on(s, "BP-SHEET-BORDER", lwpoly(rect_vertices(at, Vec2::new(at.x + w, at.y + h)), true))?;
    let title = add_on(s, "BP-TITLE-BLOCK", lwpoly(rect_vertices(at, Vec2::new(at.x + w, at.y + strip)), true))?;
    let d = s.doc_mut()?;
    d.modify_entity(border, |e| e.common.lineweight = Lineweight::Mm100(50))?;
    d.modify_entity(title, |e| e.common.lineweight = Lineweight::Mm100(25))?;
    let labels = [
        ("PROJECT NAME".to_string(), Vec2::new(at.x + 0.010 * k, at.y + strip + 0.005 * k), 0.012 * k),
        ("DRAWING TITLE".to_string(), Vec2::new(at.x + 0.010 * k, at.y + strip + 0.020 * k), 0.010 * k),
        (format!("Sheet: {size}"), Vec2::new(at.x + w - 0.120 * k, at.y + 0.010 * k), 0.008 * k),
        ("Rev: A0".to_string(), Vec2::new(at.x + w - 0.060 * k, at.y + 0.010 * k), 0.008 * k),
    ];
    for (text, pos, height) in labels {
        let t = make_text(s, pos, &text, height, 0.0, (HAlign::Left, VAlign::Baseline));
        add_on(s, "BP-TITLE-BLOCK", t)?;
    }
    Ok(json!({ "size": size }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metres() -> Session {
        let mut s = Session::new();
        s.doc_mut().unwrap().header.set_i64("INSUNITS", 6);
        s
    }

    fn poly_on(s: &mut Session, layer: &str, closed: bool, side: f64) -> String {
        let r = s.execute("rectang", &json!({ "p1": [0, 0], "p2": [side, side] })).unwrap();
        let h = r["handle"].as_str().or_else(|| r["handles"][0].as_str()).unwrap().to_string();
        let handle = Handle::parse_hex(&h).unwrap();
        let d = s.doc_mut().unwrap();
        d.ensure_layer(layer);
        d.modify_entity(handle, |e| {
            e.common.layer = layer.to_string();
            if let EntityKind::LwPolyline(pl) = &mut e.kind {
                pl.closed = closed;
            }
        })
        .unwrap();
        h
    }

    #[test]
    fn the_table_has_92_layers_in_three_columns() {
        assert_eq!(LAYERS.len(), 92);
        assert_eq!(LAYERS.iter().filter(|l| is_ap(l.0)).count(), 27);
        assert!(LAYERS[COLUMN_2..].iter().all(|l| is_ap(l.0)));
        assert!(LAYERS[..COLUMN_2].iter().all(|l| is_bp(l.0)));
        let mut names: Vec<_> = LAYERS.iter().map(|l| l.0).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 92, "no duplicate layer names");
    }

    #[test]
    fn start_sets_metres_layers_styles_and_the_dimension_style() {
        let mut s = metres();
        let r = s.execute("bpltstart", &json!({})).unwrap();
        assert_eq!(r["created"], json!(92));
        let d = s.doc().unwrap();
        assert!(!d.layer("AP-ROOM").unwrap().plot, "AP- layers never plot");
        assert!(d.layer("BP-ROOM").unwrap().plot);
        assert_eq!(d.header.str("CLAYER", ""), "BP-BUILDING-CUT");
        assert_eq!(d.header.str("DIMSTYLE", ""), "BPLT-DIM");
        assert!(d.text_style("BPLT-TEXT").is_some() && d.dim_style("BPLT-DIM").is_some());
        let again = s.execute("bpltstart", &json!({})).unwrap();
        assert_eq!((again["created"].clone(), again["updated"].clone()), (json!(0), json!(0)));
    }

    #[test]
    fn start_rescales_a_millimetre_drawing_to_metres_once() {
        let mut s = Session::new();
        s.doc_mut().unwrap().header.set_i64("INSUNITS", 4);
        s.execute("line", &json!({ "points": [[0, 0], [5000, 0]] })).unwrap();
        let r = s.execute("bpltstart", &json!({})).unwrap();
        assert_eq!(r["scaled"], json!(1));
        let EntityKind::Line(l) = &s.doc().unwrap().model.iter().next().unwrap().kind else { panic!("a line") };
        assert!((l.b.x - 5.0).abs() < 1e-9);
        assert_eq!(s.doc().unwrap().header.i64("INSUNITS", 0), 6);
        let again = s.execute("bpltstart", &json!({})).unwrap();
        assert_eq!(again["scaled"], json!(0), "already metres: nothing is scaled twice");
    }

    #[test]
    fn rescale_can_be_turned_off() {
        let mut s = Session::new();
        s.doc_mut().unwrap().header.set_i64("INSUNITS", 4);
        s.execute("line", &json!({ "points": [[0, 0], [5000, 0]] })).unwrap();
        let r = s.execute("bpltstart", &json!({ "rescale": false })).unwrap();
        assert_eq!(r["scaled"], json!(0));
    }

    #[test]
    fn copy_puts_bp_objects_on_the_matching_ap_layer() {
        let mut s = metres();
        s.execute("bpltstart", &json!({})).unwrap();
        let a = poly_on(&mut s, "BP-ROOM", true, 3.0);
        let b = poly_on(&mut s, "BP-NORTH", true, 1.0);
        let c = poly_on(&mut s, "OTHER", true, 1.0);
        let r = s.execute("bpltcopy", &json!({ "handles": [a, b, c] })).unwrap();
        assert_eq!(r["copied"], json!(1), "BP-NORTH has no AP- twin and OTHER is not BP-");
        assert_eq!(r["skipped"], json!(2));
        let (closed, _) = ap_polylines(s.doc().unwrap());
        assert_eq!(closed.get("AP-ROOM"), Some(&1));
        assert!(s.execute("bpltcopy", &json!({ "handles": [] })).is_err());
    }

    #[test]
    fn check_reports_fail_warn_and_pass() {
        let mut s = metres();
        s.execute("bpltstart", &json!({})).unwrap();
        assert_eq!(s.execute("bpltcheck", &json!({})).unwrap()["result"], json!("FAIL"));
        for l in REQUIRED_AP {
            poly_on(&mut s, l, true, 2.0);
        }
        assert_eq!(s.execute("bpltcheck", &json!({})).unwrap()["result"], json!("PASS"));
        poly_on(&mut s, "AP-ROOM", false, 2.0);
        let r = s.execute("bpltcheck", &json!({})).unwrap();
        assert_eq!(r["result"], json!("WARN"));
        assert_eq!(r["open"][0]["layer"], json!("AP-ROOM"));
    }

    #[test]
    fn area_sums_closed_ap_polylines_and_can_place_a_table() {
        let mut s = metres();
        s.execute("bpltstart", &json!({})).unwrap();
        assert!(s.execute("bpltarea", &json!({})).is_err());
        poly_on(&mut s, "AP-ROOM", true, 3.0);
        poly_on(&mut s, "AP-ROOM", true, 2.0);
        poly_on(&mut s, "AP-ROOM", false, 9.0);
        poly_on(&mut s, "BP-ROOM", true, 9.0);
        let r = s.execute("bpltarea", &json!({})).unwrap();
        assert!((r["total"].as_f64().unwrap() - 13.0).abs() < 1e-9);
        let before = s.doc().unwrap().model.iter().count();
        s.execute("bpltarea", &json!({ "at": [0, 20] })).unwrap();
        assert_eq!(s.doc().unwrap().model.iter().count(), before + 1);
    }

    #[test]
    fn report_counts_bp_and_ap_layers() {
        let mut s = metres();
        s.execute("bpltstart", &json!({})).unwrap();
        poly_on(&mut s, "AP-ROOM", true, 3.0);
        poly_on(&mut s, "NOT-BPLT", true, 3.0);
        let r = s.execute("bpltreport", &json!({})).unwrap();
        let csv = r["csv"].as_str().unwrap();
        assert!(csv.contains("AP-ROOM,AP,1,9.0000,No"));
        assert!(!csv.contains("NOT-BPLT"));
    }

    #[test]
    fn title_block_and_legend_draw_and_reject_bad_input() {
        let mut s = metres();
        assert!(s.execute("bplttitleblock", &json!({ "at": [0, 0], "size": "A5" })).is_err());
        assert!(s.execute("bplttitleblock", &json!({ "size": "A1" })).is_err());
        let before = s.doc().unwrap().model.iter().count();
        s.execute("bplttitleblock", &json!({ "at": [0, 0] })).unwrap();
        assert_eq!(s.doc().unwrap().model.iter().count(), before + 6, "border, strip and four labels");
        let r = s.execute("bpltlayers", &json!({ "at": [0, 50] })).unwrap();
        assert_eq!(r["rows"], json!(92));
        assert!(s.execute("bpltlayers", &json!({})).is_err());
    }
}
