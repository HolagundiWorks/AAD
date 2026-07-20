//! # aadt-takeoff — Takeoff & Quantity Engine (flagship deliverable, spec §5.2)
//!
//! 2D-native quantity takeoff with **no BIM required**. Computation is deterministic
//! and reproducible independent of the AI layer (spec §5.2, §8): quantities are
//! derived purely from the detected geometry plus a [`TakeoffConfig`] of assumed
//! heights and cost-code mappings.
//!
//! Computed here:
//! - **Walls:** length, thickness; gross area (length × height) and volume
//!   (length × thickness × height) from configured wall height.
//! - **Openings:** door/window area (width × height), subtracted from gross wall
//!   area to give net wall area; counted by type.
//! - **Rooms:** area and perimeter (floor finish).
//! - **Structural:** counted by type.
//! - **Cost codes:** each quantity is mapped to a CSI MasterFormat code (ADR-0005),
//!   overridable per firm, and carried into the CSV for estimator import.
//!
//! The [`LiveTakeoff`] links a computed table to the document: it recomputes the
//! detection + takeoff only when the document's revision changes — editing a wall
//! updates the table, the link-and-update mechanism spec §5.2 calls the "technical
//! heart of the product."
//!
//! Exports: [`Takeoff::to_csv`] and [`Takeoff::to_xlsx`] (a real OOXML workbook,
//! written dependency-free by the [`xlsx`] module) carry identical rows.
//!
//! Deferred (see `docs/ROADMAP.md`): in-drawing schedule tables → later M3 work;
//! AI schedule formatting / anomaly flagging → M4.

pub mod xlsx;

use aadt_detect::{detect, Detection, DetectionConfig, OpeningKind, StructuralKind};
use aadt_model::Document;

/// Number of decimal places used when formatting quantities. Fixed for
/// deterministic, byte-stable CSV output.
const DECIMALS: usize = 3;

/// CSI MasterFormat cost codes per element category (ADR-0005). Firm-overridable.
#[derive(Debug, Clone)]
pub struct CostCodeMap {
    pub wall: String,
    pub door: String,
    pub window: String,
    pub column: String,
    pub beam: String,
    pub room_floor: String,
}

impl Default for CostCodeMap {
    fn default() -> Self {
        Self {
            wall: "09 21 00".to_string(),   // Gypsum Board Assemblies
            door: "08 10 00".to_string(),   // Doors and Frames
            window: "08 50 00".to_string(), // Windows
            column: "05 12 00".to_string(), // Structural Steel Framing
            beam: "05 12 00".to_string(),
            room_floor: "09 60 00".to_string(), // Flooring
        }
    }
}

/// Assumed heights and cost mapping used to turn 2D geometry into quantities.
#[derive(Debug, Clone)]
pub struct TakeoffConfig {
    pub wall_height: f64,
    pub door_height: f64,
    pub window_height: f64,
    /// Width assumed for an opening whose block carries no width attribute.
    pub default_opening_width: f64,
    pub cost_codes: CostCodeMap,
}

impl Default for TakeoffConfig {
    fn default() -> Self {
        Self {
            wall_height: 3.0,
            door_height: 2.1,
            window_height: 1.5,
            default_opening_width: 0.9,
            cost_codes: CostCodeMap::default(),
        }
    }
}

/// Per-wall computed quantities.
#[derive(Debug, Clone, PartialEq)]
pub struct WallTakeoff {
    pub id: usize,
    pub layer: String,
    pub length: f64,
    pub thickness: f64,
    pub height: f64,
    /// Gross face area = length × height (drawing units²).
    pub gross_area: f64,
    /// Volume = length × thickness × height (drawing units³).
    pub volume: f64,
    pub cost_code: String,
}

/// Per-opening computed quantities.
#[derive(Debug, Clone, PartialEq)]
pub struct OpeningTakeoff {
    pub number: u32,
    pub kind: String,
    pub width: f64,
    pub height: f64,
    pub area: f64,
    pub cost_code: String,
}

/// Per-room computed quantities.
#[derive(Debug, Clone, PartialEq)]
pub struct RoomTakeoff {
    pub id: usize,
    pub layer: String,
    pub area: f64,
    pub perimeter: f64,
    pub cost_code: String,
}

/// Per-structural-element record (count-oriented).
#[derive(Debug, Clone, PartialEq)]
pub struct StructuralTakeoff {
    pub number: u32,
    pub kind: String,
    pub cost_code: String,
}

/// A computed takeoff: the deterministic quantities behind an eventual live table.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TakeoffTable {
    pub walls: Vec<WallTakeoff>,
    pub openings: Vec<OpeningTakeoff>,
    pub rooms: Vec<RoomTakeoff>,
    pub structural: Vec<StructuralTakeoff>,
}

impl TakeoffTable {
    /// Compute quantities from a detection result using default assumptions.
    pub fn from_detection(det: &Detection) -> Self {
        Self::with_config(det, &TakeoffConfig::default())
    }

    /// Compute quantities from a detection result and an explicit config.
    pub fn with_config(det: &Detection, cfg: &TakeoffConfig) -> Self {
        let walls = det
            .walls
            .iter()
            .map(|w| WallTakeoff {
                id: w.id,
                layer: w.layer.clone(),
                length: w.length,
                thickness: w.thickness,
                height: cfg.wall_height,
                gross_area: w.length * cfg.wall_height,
                volume: w.length * w.thickness * cfg.wall_height,
                cost_code: cfg.cost_codes.wall.clone(),
            })
            .collect();

        let openings = det
            .openings
            .iter()
            .map(|o| {
                let width = o.width.unwrap_or(cfg.default_opening_width);
                let (height, cost_code, kind) = match o.kind {
                    OpeningKind::Door => (cfg.door_height, cfg.cost_codes.door.clone(), "Door"),
                    OpeningKind::Window => {
                        (cfg.window_height, cfg.cost_codes.window.clone(), "Window")
                    }
                };
                OpeningTakeoff {
                    number: o.number,
                    kind: kind.to_string(),
                    width,
                    height,
                    area: width * height,
                    cost_code,
                }
            })
            .collect();

        // Explicit polylines plus non-duplicate graph-derived faces, so a
        // walls-only drawing still yields floor area. See `Detection::merged_rooms`.
        let rooms = det
            .merged_rooms()
            .iter()
            .map(|r| RoomTakeoff {
                id: r.id,
                layer: r.layer.clone(),
                area: r.area,
                perimeter: r.perimeter,
                cost_code: cfg.cost_codes.room_floor.clone(),
            })
            .collect();

        let structural = det
            .structural
            .iter()
            .map(|s| {
                let (kind, cost_code) = match s.kind {
                    StructuralKind::Column => ("Column", cfg.cost_codes.column.clone()),
                    StructuralKind::Beam => ("Beam", cfg.cost_codes.beam.clone()),
                };
                StructuralTakeoff {
                    number: s.number,
                    kind: kind.to_string(),
                    cost_code,
                }
            })
            .collect();

        Self {
            walls,
            openings,
            rooms,
            structural,
        }
    }

    pub fn total_wall_length(&self) -> f64 {
        self.walls.iter().map(|w| w.length).sum()
    }

    /// Gross wall face area, before subtracting openings.
    pub fn total_wall_gross_area(&self) -> f64 {
        self.walls.iter().map(|w| w.gross_area).sum()
    }

    pub fn total_wall_volume(&self) -> f64 {
        self.walls.iter().map(|w| w.volume).sum()
    }

    pub fn total_opening_area(&self) -> f64 {
        self.openings.iter().map(|o| o.area).sum()
    }

    /// Net wall face area = gross − openings, clamped at zero.
    pub fn total_wall_net_area(&self) -> f64 {
        (self.total_wall_gross_area() - self.total_opening_area()).max(0.0)
    }

    pub fn total_room_area(&self) -> f64 {
        self.rooms.iter().map(|r| r.area).sum()
    }

    /// Render an estimator-friendly CSV. One flat table with a `category` column so
    /// every element type and the totals share a single file. Output is
    /// deterministic.
    pub fn to_csv(&self) -> String {
        let mut out = String::new();
        out.push_str("category,ref,kind,length,thickness,height,area,volume,perimeter,cost_code\n");
        for w in &self.walls {
            out.push_str(&format!(
                "WALL,{},,{},{},{},{},{},,{}\n",
                w.id,
                num(w.length),
                num(w.thickness),
                num(w.height),
                num(w.gross_area),
                num(w.volume),
                w.cost_code,
            ));
        }
        for o in &self.openings {
            out.push_str(&format!(
                "OPENING,{},{},{},,{},{},,,{}\n",
                o.number,
                o.kind,
                num(o.width),
                num(o.height),
                num(o.area),
                o.cost_code,
            ));
        }
        for r in &self.rooms {
            out.push_str(&format!(
                "ROOM,{},,,,,{},,{},{}\n",
                r.id,
                num(r.area),
                num(r.perimeter),
                r.cost_code,
            ));
        }
        for s in &self.structural {
            out.push_str(&format!(
                "STRUCTURAL,{},{},,,,,,,{}\n",
                s.number, s.kind, s.cost_code
            ));
        }
        out.push_str(&format!(
            "TOTAL,walls-gross,,{},,,{},{},,\n",
            num(self.total_wall_length()),
            num(self.total_wall_gross_area()),
            num(self.total_wall_volume()),
        ));
        out.push_str(&format!(
            "TOTAL,openings,,,,,{},,,\n",
            num(self.total_opening_area())
        ));
        out.push_str(&format!(
            "TOTAL,walls-net,,,,,{},,,\n",
            num(self.total_wall_net_area())
        ));
        out.push_str(&format!(
            "TOTAL,rooms,,,,,{},,,\n",
            num(self.total_room_area())
        ));
        out
    }

    /// Render the same table as a real `.xlsx` workbook (OOXML, not a renamed
    /// CSV). Columns and row order match [`to_csv`](Self::to_csv) exactly, and
    /// numbers carry the same rounding, so the two exports can be reconciled
    /// cell for cell.
    ///
    /// Quantities are written as **numbers**, not text — the point of shipping
    /// `.xlsx` over CSV is that an estimator can sum a column without a
    /// text-to-number pass first.
    ///
    /// Output is byte-for-byte deterministic (see the `xlsx` module).
    pub fn to_xlsx(&self) -> Vec<u8> {
        xlsx::workbook(&[self.sheet()])
    }

    /// The takeoff as a spreadsheet sheet, shared by `.xlsx` export and the
    /// in-drawing schedule table.
    fn sheet(&self) -> xlsx::Sheet {
        use xlsx::Cell;

        let mut sheet = xlsx::Sheet::new("Takeoff");
        sheet.push_row(
            [
                "category",
                "ref",
                "kind",
                "length",
                "thickness",
                "height",
                "area",
                "volume",
                "perimeter",
                "cost_code",
            ]
            .iter()
            .map(|h| Cell::text(*h))
            .collect(),
        );

        let n = |v: f64| Cell::Number(num(v));

        for w in &self.walls {
            sheet.push_row(vec![
                Cell::text("WALL"),
                Cell::Number(w.id.to_string()),
                Cell::Empty,
                n(w.length),
                n(w.thickness),
                n(w.height),
                n(w.gross_area),
                n(w.volume),
                Cell::Empty,
                Cell::text(&w.cost_code),
            ]);
        }
        for o in &self.openings {
            sheet.push_row(vec![
                Cell::text("OPENING"),
                Cell::Number(o.number.to_string()),
                Cell::text(&o.kind),
                n(o.width),
                Cell::Empty,
                n(o.height),
                n(o.area),
                Cell::Empty,
                Cell::Empty,
                Cell::text(&o.cost_code),
            ]);
        }
        for r in &self.rooms {
            sheet.push_row(vec![
                Cell::text("ROOM"),
                Cell::Number(r.id.to_string()),
                Cell::Empty,
                Cell::Empty,
                Cell::Empty,
                Cell::Empty,
                n(r.area),
                Cell::Empty,
                n(r.perimeter),
                Cell::text(&r.cost_code),
            ]);
        }
        for s in &self.structural {
            sheet.push_row(vec![
                Cell::text("STRUCTURAL"),
                Cell::Number(s.number.to_string()),
                Cell::text(&s.kind),
                Cell::Empty,
                Cell::Empty,
                Cell::Empty,
                Cell::Empty,
                Cell::Empty,
                Cell::Empty,
                Cell::text(&s.cost_code),
            ]);
        }

        let total = |label: &str, cells: Vec<(usize, Cell)>| {
            let mut row = vec![Cell::Empty; 10];
            row[0] = Cell::text("TOTAL");
            row[1] = Cell::text(label);
            for (i, c) in cells {
                row[i] = c;
            }
            row
        };
        sheet.push_row(total(
            "walls-gross",
            vec![
                (3, n(self.total_wall_length())),
                (6, n(self.total_wall_gross_area())),
                (7, n(self.total_wall_volume())),
            ],
        ));
        sheet.push_row(total("openings", vec![(6, n(self.total_opening_area()))]));
        sheet.push_row(total("walls-net", vec![(6, n(self.total_wall_net_area()))]));
        sheet.push_row(total("rooms", vec![(6, n(self.total_room_area()))]));
        sheet
    }
}

/// A takeoff table linked to a document: it recomputes only when the document
/// changes (spec §5.2, the link-and-update mechanism). Hold one per view and call
/// [`table`](LiveTakeoff::table) whenever the current quantities are needed; edits
/// to the document are reflected automatically on the next call.
#[derive(Debug, Clone, Default)]
pub struct LiveTakeoff {
    detect_cfg: DetectionConfig,
    takeoff_cfg: TakeoffConfig,
    /// Cached `(document revision, table)`; `None` until first computed.
    cache: Option<(u64, TakeoffTable)>,
}

impl LiveTakeoff {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_configs(detect_cfg: DetectionConfig, takeoff_cfg: TakeoffConfig) -> Self {
        Self {
            detect_cfg,
            takeoff_cfg,
            cache: None,
        }
    }

    /// Whether the cached table is up to date with `doc` (no recompute needed).
    pub fn is_current(&self, doc: &Document) -> bool {
        self.cache
            .as_ref()
            .map(|(rev, _)| *rev == doc.revision())
            .unwrap_or(false)
    }

    /// The current takeoff table for `doc`, recomputing detection + takeoff only if
    /// the document changed since the last call.
    pub fn table(&mut self, doc: &Document) -> &TakeoffTable {
        if !self.is_current(doc) {
            let detection = detect(doc, &self.detect_cfg);
            let table = TakeoffTable::with_config(&detection, &self.takeoff_cfg);
            self.cache = Some((doc.revision(), table));
        }
        &self.cache.as_ref().expect("cache populated above").1
    }
}

/// Format a quantity with a fixed number of decimals (deterministic). Negative
/// zero is normalized to positive zero so output is stable.
fn num(v: f64) -> String {
    let v = if v == 0.0 { 0.0 } else { v };
    format!("{v:.DECIMALS$}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use aadt_detect::{detect, DetectionConfig};
    use aadt_geometry::{Point2, Polyline, Segment};
    use aadt_model::{Document, Geometry, Insert};

    fn sample_doc() -> Document {
        let mut doc = Document::new();
        // One 10m wall (two faces 0.2 apart).
        doc.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0))),
        );
        doc.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.2), Point2::new(10.0, 0.2))),
        );
        // A room.
        doc.add_geometry(
            "A-ROOM",
            Geometry::Polyline(Polyline::new(
                vec![
                    Point2::new(0.0, 0.0),
                    Point2::new(4.0, 0.0),
                    Point2::new(4.0, 3.0),
                    Point2::new(0.0, 3.0),
                ],
                true,
            )),
        );
        // A door with an explicit width.
        let mut attrs = std::collections::HashMap::new();
        attrs.insert("width".to_string(), "0.9".to_string());
        doc.add_entity(
            "A-DOOR",
            Geometry::Insert(Insert::new("DOOR-SINGLE", Point2::new(5.0, 0.1), 0.0)),
            attrs,
        );
        doc
    }

    #[test]
    fn computes_areas_volumes_and_opening_subtraction() {
        let doc = sample_doc();
        let det = detect(&doc, &DetectionConfig::default());
        let table = TakeoffTable::from_detection(&det);

        assert_eq!(table.walls.len(), 1);
        // 10 length × 3.0 height = 30 gross; volume 10 × 0.2 × 3.0 = 6.
        assert!((table.total_wall_gross_area() - 30.0).abs() < 1e-9);
        assert!((table.total_wall_volume() - 6.0).abs() < 1e-9);
        // Door 0.9 × 2.1 = 1.89 opening area.
        assert_eq!(table.openings.len(), 1);
        assert!((table.total_opening_area() - 1.89).abs() < 1e-9);
        assert!((table.total_wall_net_area() - (30.0 - 1.89)).abs() < 1e-9);
        assert!((table.total_room_area() - 12.0).abs() < 1e-9);
    }

    /// The `.xlsx` and `.csv` exports must never disagree — an estimator who
    /// opens both should reconcile them cell for cell. This compares the sheet
    /// grid against the CSV text rather than trusting the two writers to stay
    /// in step by inspection.
    #[test]
    fn xlsx_grid_matches_the_csv_exactly() {
        let doc = sample_doc();
        let det = detect(&doc, &DetectionConfig::default());
        let table = TakeoffTable::from_detection(&det);

        let csv = table.to_csv();
        let csv_rows: Vec<Vec<&str>> = csv
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| l.split(',').collect())
            .collect();
        let sheet = table.sheet();

        assert_eq!(
            sheet.rows.len(),
            csv_rows.len(),
            "row count differs between xlsx and csv"
        );
        for (i, (srow, crow)) in sheet.rows.iter().zip(&csv_rows).enumerate() {
            assert_eq!(srow.len(), crow.len(), "row {i} has a different width");
            for (j, (cell, want)) in srow.iter().zip(crow).enumerate() {
                let got = match cell {
                    xlsx::Cell::Empty => "",
                    xlsx::Cell::Text(t) => t.as_str(),
                    xlsx::Cell::Number(n) => n.as_str(),
                };
                assert_eq!(got, *want, "row {i} column {j} differs");
            }
        }
    }

    #[test]
    fn xlsx_quantities_are_numeric_cells() {
        // The reason to ship .xlsx rather than CSV: a column must sum without
        // a text-to-number conversion pass first.
        let doc = sample_doc();
        let det = detect(&doc, &DetectionConfig::default());
        let sheet = TakeoffTable::from_detection(&det).sheet();

        let header = &sheet.rows[0];
        assert!(
            matches!(header[6], xlsx::Cell::Text(_)),
            "header stays text"
        );
        let area_col = sheet.rows[1][6].clone();
        assert!(
            matches!(area_col, xlsx::Cell::Number(_)),
            "wall area must be a number cell, got {area_col:?}"
        );
    }

    #[test]
    fn live_takeoff_recomputes_only_on_change() {
        let mut doc = sample_doc();
        let mut live = LiveTakeoff::new();

        assert!(!live.is_current(&doc), "nothing computed yet");
        assert_eq!(live.table(&doc).walls.len(), 1);
        assert!(live.is_current(&doc), "cache is fresh after compute");

        // Editing the document invalidates the cached table.
        doc.transact("add wall", |d| {
            d.add_geometry(
                "A-WALL",
                Geometry::Line(Segment::new(Point2::new(0.0, 5.0), Point2::new(10.0, 5.0))),
            );
            d.add_geometry(
                "A-WALL",
                Geometry::Line(Segment::new(Point2::new(0.0, 5.2), Point2::new(10.0, 5.2))),
            );
        });
        assert!(!live.is_current(&doc), "edit invalidates the cache");
        assert_eq!(
            live.table(&doc).walls.len(),
            2,
            "table reflects the new wall"
        );

        // Undo brings it back to one wall.
        doc.undo();
        assert_eq!(live.table(&doc).walls.len(), 1);
    }

    #[test]
    fn csv_has_costcodes_and_is_deterministic() {
        let doc = sample_doc();
        let det = detect(&doc, &DetectionConfig::default());
        let table = TakeoffTable::from_detection(&det);
        let csv = table.to_csv();

        assert!(csv.starts_with(
            "category,ref,kind,length,thickness,height,area,volume,perimeter,cost_code\n"
        ));
        assert!(csv.contains("WALL,0,,10.000,0.200,3.000,30.000,6.000,,09 21 00\n"));
        assert!(csv.contains("OPENING,1,Door,0.900,,2.100,1.890,,,08 10 00\n"));
        assert!(csv.contains("ROOM,0,,,,,12.000,,14.000,09 60 00\n"));
        assert_eq!(csv, table.to_csv());
    }
}
