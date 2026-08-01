//! # aadt-takeoff — quantity takeoff over the Object DB (AI-native Phase 4)
//!
//! Reinstates the quantity engine removed in ROADMAP M3, rebuilt against the
//! **Object DB** rather than raw geometry (ADR-0014).
//!
//! That change is the whole point. The M3 engine measured lines and polylines,
//! so it could compute a *gross* wall area but had no way to know which opening
//! pierced which wall. Objects carry engineering properties, and the Graph DB
//! carries `Door —BelongsTo→ Wall`, so an opening deduction is now a graph
//! lookup rather than a geometric guess — and a wall whose opening the graph
//! does not attribute is simply not deducted, rather than guessed at.
//!
//! A run is:
//!
//! 1. promote the document into objects + a graph ([`aadt_graph::promote`]),
//! 2. [`measure`](quantities::measure) the objects into [`Quantities`],
//! 3. [`build`](boq::build) a [`Boq`] aggregated by CSI MasterFormat cost code.
//!
//! Everything is deterministic: objects iterate in id order and bill lines sort
//! by cost code, so two runs of one drawing produce identical output — which is
//! what makes revision-to-revision diffing meaningful.
//!
//! **What 2D cannot tell you.** AADT is a 2D tool: a plan has no storey height,
//! so wall height is a [`TakeoffConfig`] *input*, not a measurement. Openings
//! that carry no dimensions fall back to configured defaults and are flagged
//! `measured: false` so a reader can tell an assumption from a measurement.

pub mod boq;
pub mod costcode;
pub mod quantities;

use aadt_model::Document;

pub use boq::{build, Boq, BoqLine, Unit};
pub use costcode::{code_for, CostCode};
pub use quantities::{
    measure, OpeningQty, Quantities, RoomQty, StructuralQty, TakeoffConfig, WallQty,
};

/// A complete takeoff run: what was measured and the bill built from it.
#[derive(Debug, Clone, Default)]
pub struct TakeoffRun {
    pub quantities: Quantities,
    pub boq: Boq,
    /// `Document::revision` the run measured, so a stale run is detectable.
    pub doc_revision: u64,
}

/// Run a full takeoff on `doc`: promote to objects, measure, and bill.
pub fn run(doc: &Document, cfg: &TakeoffConfig) -> TakeoffRun {
    let promoted = aadt_graph::promote::promote_document(doc);
    let quantities = measure(&promoted.objects, &promoted.graph, cfg);
    let boq = build(&quantities);
    TakeoffRun {
        quantities,
        boq,
        doc_revision: doc.revision(),
    }
}

/// Totals worth showing at the head of a report.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Totals {
    pub wall_area: f64,
    pub wall_volume: f64,
    pub floor_area: f64,
    pub paint_area: f64,
    pub bricks: u64,
    pub walls: usize,
    pub rooms: usize,
    pub openings: usize,
}

impl TakeoffRun {
    /// Headline totals for this run.
    pub fn totals(&self) -> Totals {
        let q = &self.quantities;
        Totals {
            wall_area: q.total_wall_area(),
            wall_volume: q.total_wall_volume(),
            floor_area: q.total_floor_area(),
            paint_area: q.total_paint_area(),
            bricks: q.total_bricks(),
            walls: q.walls.len(),
            rooms: q.rooms.len(),
            openings: q.openings.len(),
        }
    }

    /// Totals as the compact JSON the `.aadt` run tables store.
    ///
    /// Hand-rolled rather than serde: the workspace stays dependency-frugal, and
    /// this is a fixed six-field object, not a general serializer.
    pub fn totals_json(&self) -> String {
        let t = self.totals();
        format!(
            "{{\"wall_area\":{:.4},\"wall_volume\":{:.4},\"floor_area\":{:.4},\
             \"paint_area\":{:.4},\"bricks\":{},\"walls\":{},\"rooms\":{},\"openings\":{}}}",
            t.wall_area,
            t.wall_volume,
            t.floor_area,
            t.paint_area,
            t.bricks,
            t.walls,
            t.rooms,
            t.openings
        )
    }

    /// Openings measured from assumed rather than drawn dimensions — the figures
    /// a reviewer should confirm before pricing.
    pub fn assumed_openings(&self) -> Vec<&OpeningQty> {
        self.quantities
            .openings
            .iter()
            .filter(|o| !o.measured)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aadt_geometry::{Point2, Polyline, Segment};
    use aadt_model::Geometry;

    /// A closed 6 × 4 room bounded by a wall pair.
    fn plan() -> Document {
        let mut doc = Document::new();
        doc.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(6.0, 0.0))),
        );
        doc.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.23), Point2::new(6.0, 0.23))),
        );
        doc.add_geometry(
            "A-AREA",
            Geometry::Polyline(Polyline::new(
                vec![
                    Point2::new(0.0, 0.23),
                    Point2::new(6.0, 0.23),
                    Point2::new(6.0, 4.0),
                    Point2::new(0.0, 4.0),
                ],
                true,
            )),
        );
        doc
    }

    #[test]
    fn a_plan_measures_and_bills() {
        let r = run(&plan(), &TakeoffConfig::default());
        let t = r.totals();
        assert_eq!(t.walls, 1, "one wall from the face pair");
        assert!(t.wall_area > 0.0);
        assert!(t.wall_volume > 0.0);
        assert!(t.bricks > 0);
        assert!(!r.boq.is_empty(), "the bill should have lines");
    }

    #[test]
    fn a_run_is_deterministic() {
        let doc = plan();
        let cfg = TakeoffConfig::default();
        let a = run(&doc, &cfg);
        let b = run(&doc, &cfg);
        assert_eq!(a.quantities, b.quantities);
        assert_eq!(a.boq, b.boq);
        assert_eq!(a.totals_json(), b.totals_json());
    }

    #[test]
    fn wall_height_is_an_input_not_a_measurement() {
        let doc = plan();
        let low = run(
            &doc,
            &TakeoffConfig {
                wall_height: 2.4,
                ..Default::default()
            },
        );
        let high = run(
            &doc,
            &TakeoffConfig {
                wall_height: 3.6,
                ..Default::default()
            },
        );
        assert!(
            high.totals().wall_area > low.totals().wall_area,
            "a taller storey must measure more wall"
        );
    }

    #[test]
    fn totals_json_is_well_formed_and_carries_every_field() {
        let j = run(&plan(), &TakeoffConfig::default()).totals_json();
        assert!(j.starts_with('{') && j.ends_with('}'));
        for key in [
            "wall_area",
            "wall_volume",
            "floor_area",
            "paint_area",
            "bricks",
            "walls",
            "rooms",
            "openings",
        ] {
            assert!(j.contains(key), "totals JSON missing {key}: {j}");
        }
    }

    #[test]
    fn an_empty_drawing_measures_nothing() {
        let r = run(&Document::new(), &TakeoffConfig::default());
        assert!(r.quantities.is_empty());
        assert!(r.boq.is_empty());
        assert_eq!(r.totals().walls, 0);
    }

    #[test]
    fn run_records_the_revision_it_measured() {
        let doc = plan();
        let r = run(&doc, &TakeoffConfig::default());
        assert_eq!(r.doc_revision, doc.revision());
    }
}
