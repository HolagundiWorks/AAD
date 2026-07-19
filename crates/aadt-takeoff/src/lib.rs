//! # aadt-takeoff — Takeoff & Quantity Engine (flagship deliverable, spec §5.2)
//!
//! 2D-native quantity takeoff with **no BIM required** — the product's flagship
//! deliverable. Computation is deterministic and reproducible independent of the AI
//! layer (spec §5.2, §8): quantities are derived purely from the detected geometry.
//!
//! M0 computes the deterministic subset:
//! - **Walls:** length, thickness, and footprint area (length × thickness).
//! - **Rooms:** area and perimeter.
//!
//! and exports an estimator-friendly CSV.
//!
//! Deferred (see `docs/ROADMAP.md`):
//! - Wall **area/volume from height** attributes and opening subtraction → **M3**.
//! - **Live-linked** model-bound tables (the link-and-update mechanism, spec §5.2
//!   "technical heart"), **cost-code mapping** (CSI MasterFormat, ADR-0005), and
//!   **Excel / in-drawing schedule** output → **M3**.
//! - AI schedule **formatting** and **anomaly flagging** → **M4**.

use aadt_detect::Detection;

/// Number of decimal places used when formatting quantities. Fixed for
/// deterministic, byte-stable CSV output.
const DECIMALS: usize = 3;

/// Per-wall computed quantities.
#[derive(Debug, Clone, PartialEq)]
pub struct WallTakeoff {
    pub id: usize,
    pub layer: String,
    pub length: f64,
    pub thickness: f64,
    /// Plan footprint area = length × thickness (drawing units²).
    pub footprint_area: f64,
}

/// Per-room computed quantities.
#[derive(Debug, Clone, PartialEq)]
pub struct RoomTakeoff {
    pub id: usize,
    pub layer: String,
    pub area: f64,
    pub perimeter: f64,
}

/// A computed takeoff: the deterministic quantities behind an eventual live table.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TakeoffTable {
    pub walls: Vec<WallTakeoff>,
    pub rooms: Vec<RoomTakeoff>,
}

impl TakeoffTable {
    /// Compute quantities from a detection result.
    pub fn from_detection(det: &Detection) -> Self {
        let walls = det
            .walls
            .iter()
            .map(|w| WallTakeoff {
                id: w.id,
                layer: w.layer.clone(),
                length: w.length,
                thickness: w.thickness,
                footprint_area: w.length * w.thickness,
            })
            .collect();
        let rooms = det
            .rooms
            .iter()
            .map(|r| RoomTakeoff {
                id: r.id,
                layer: r.layer.clone(),
                area: r.area,
                perimeter: r.perimeter,
            })
            .collect();
        Self { walls, rooms }
    }

    pub fn total_wall_length(&self) -> f64 {
        self.walls.iter().map(|w| w.length).sum()
    }

    pub fn total_wall_footprint(&self) -> f64 {
        self.walls.iter().map(|w| w.footprint_area).sum()
    }

    pub fn total_room_area(&self) -> f64 {
        self.rooms.iter().map(|r| r.area).sum()
    }

    /// Render an estimator-friendly CSV. One flat table with a `category` column so
    /// walls, rooms, and totals share a single file. Output is deterministic.
    pub fn to_csv(&self) -> String {
        let mut out = String::new();
        out.push_str("category,id,layer,length,thickness,area,perimeter\n");
        for w in &self.walls {
            out.push_str(&format!(
                "WALL,{},{},{},{},{},\n",
                w.id,
                escape(&w.layer),
                num(w.length),
                num(w.thickness),
                num(w.footprint_area),
            ));
        }
        for r in &self.rooms {
            out.push_str(&format!(
                "ROOM,{},{},,,{},{}\n",
                r.id,
                escape(&r.layer),
                num(r.area),
                num(r.perimeter),
            ));
        }
        out.push_str(&format!(
            "TOTAL,walls,,{},,{},\n",
            num(self.total_wall_length()),
            num(self.total_wall_footprint()),
        ));
        out.push_str(&format!(
            "TOTAL,rooms,,,,{},\n",
            num(self.total_room_area())
        ));
        out
    }
}

/// Format a quantity with a fixed number of decimals (deterministic).
fn num(v: f64) -> String {
    format!("{v:.DECIMALS$}")
}

/// Minimal CSV field escaping for layer names containing commas or quotes.
fn escape(field: &str) -> String {
    if field.contains([',', '"', '\n']) {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aadt_detect::{detect, DetectionConfig};
    use aadt_geometry::{Point2, Polyline, Segment};
    use aadt_model::{Document, Geometry};

    #[test]
    fn computes_quantities_and_csv() {
        let mut doc = Document::new();
        doc.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0))),
        );
        doc.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.2), Point2::new(10.0, 0.2))),
        );
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
        let det = detect(&doc, &DetectionConfig::default());
        let table = TakeoffTable::from_detection(&det);

        assert_eq!(table.walls.len(), 1);
        assert!((table.total_wall_length() - 10.0).abs() < 1e-9);
        assert!((table.total_wall_footprint() - 2.0).abs() < 1e-9);
        assert!((table.total_room_area() - 12.0).abs() < 1e-9);

        let csv = table.to_csv();
        assert!(csv.starts_with("category,id,layer,length,thickness,area,perimeter\n"));
        assert!(csv.contains("WALL,0,A-WALL,10.000,0.200,2.000,\n"));
        assert!(csv.contains("ROOM,0,A-ROOM,,,12.000,14.000\n"));
    }

    #[test]
    fn deterministic_output() {
        let mut doc = Document::new();
        doc.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(5.0, 0.0))),
        );
        doc.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.15), Point2::new(5.0, 0.15))),
        );
        let det = detect(&doc, &DetectionConfig::default());
        let a = TakeoffTable::from_detection(&det).to_csv();
        let b = TakeoffTable::from_detection(&det).to_csv();
        assert_eq!(a, b);
    }
}
