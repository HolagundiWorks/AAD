//! Bill of Quantities: measured work aggregated by cost code.
//!
//! A BOQ line is `(cost code, description, unit, quantity)`. Lines are emitted
//! in ascending cost-code order so two runs of the same drawing produce
//! byte-identical output — a BOQ that reordered itself between runs would be
//! useless for diffing revisions.

use std::collections::BTreeMap;

use crate::costcode::CostCode;
use crate::quantities::Quantities;

/// The unit a quantity is measured in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Unit {
    /// Square metres.
    SquareMetre,
    /// Cubic metres.
    CubicMetre,
    /// Linear metres.
    Metre,
    /// A count of discrete items.
    Each,
}

impl Unit {
    pub fn as_str(self) -> &'static str {
        match self {
            Unit::SquareMetre => "m²",
            Unit::CubicMetre => "m³",
            Unit::Metre => "m",
            Unit::Each => "no.",
        }
    }
}

impl std::fmt::Display for Unit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One line of the bill.
#[derive(Debug, Clone, PartialEq)]
pub struct BoqLine {
    pub cost_code: CostCode,
    /// What the line covers, e.g. `"Brick masonry in walls"`.
    pub description: &'static str,
    pub unit: Unit,
    pub quantity: f64,
}

/// A complete bill of quantities.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Boq {
    pub lines: Vec<BoqLine>,
}

impl Boq {
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// The quantity billed under one code and description, if present.
    pub fn line(&self, code: &str, description: &str) -> Option<&BoqLine> {
        self.lines
            .iter()
            .find(|l| l.cost_code.code == code && l.description == description)
    }
}

/// Key a BOQ line is accumulated under before sorting.
type LineKey = (&'static str, &'static str, Unit);

/// Aggregate measured quantities into a bill.
///
/// Zero-quantity lines are dropped: a bill listing "0 m² painting" invites a
/// reader to price nothing, or worse, to think painting was measured and found
/// absent when in fact no room was detected.
pub fn build(q: &Quantities) -> Boq {
    let mut acc: BTreeMap<LineKey, (CostCode, f64)> = BTreeMap::new();
    let mut add = |code: CostCode, description: &'static str, unit: Unit, qty: f64| {
        let e = acc
            .entry((code.code, description, unit))
            .or_insert((code, 0.0));
        e.1 += qty;
    };

    for w in &q.walls {
        add(w.cost_code, "Masonry in walls", Unit::CubicMetre, w.volume);
        add(
            w.cost_code,
            "Wall area (net of openings)",
            Unit::SquareMetre,
            w.net_area,
        );
        add(
            crate::costcode::PLASTER,
            "Cement plaster to wall faces",
            Unit::SquareMetre,
            w.plaster_area,
        );
        add(w.cost_code, "Bricks", Unit::Each, w.bricks as f64);
    }

    for r in &q.rooms {
        add(r.cost_code, "Floor finish", Unit::SquareMetre, r.floor_area);
        add(
            crate::costcode::PAINTING,
            "Painting to walls and ceilings",
            Unit::SquareMetre,
            r.paint_area,
        );
    }

    for o in &q.openings {
        add(o.cost_code, "Openings", Unit::Each, 1.0);
        add(o.cost_code, "Opening area", Unit::SquareMetre, o.area);
    }

    for s in &q.structural {
        add(s.cost_code, "Structural members", Unit::Each, 1.0);
    }

    let mut lines: Vec<BoqLine> = acc
        .into_iter()
        .filter(|(_, (_, qty))| *qty > 0.0)
        .map(|((_, description, unit), (cost_code, quantity))| BoqLine {
            cost_code,
            description,
            unit,
            quantity,
        })
        .collect();
    // BTreeMap already orders by (code, description, unit); make it explicit so
    // the guarantee survives a change of accumulator.
    lines.sort_by(|a, b| {
        a.cost_code
            .code
            .cmp(b.cost_code.code)
            .then(a.description.cmp(b.description))
            .then(a.unit.cmp(&b.unit))
    });
    Boq { lines }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quantities::{measure, TakeoffConfig};
    use aadt_graph::{Graph, Relation};
    use aadt_object::{ObjectKind, ObjectStore};
    use std::collections::BTreeMap;

    fn props(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn sample() -> Quantities {
        let mut s = ObjectStore::new();
        let wall = s.insert(
            ObjectKind::Wall,
            vec![],
            props(&[
                ("length", "6.0"),
                ("thickness", "0.23"),
                ("layer", "A-WALL"),
            ]),
            1.0,
            1,
        );
        let door = s.insert(
            ObjectKind::Door,
            vec![],
            props(&[("width", "0.9"), ("height", "2.1")]),
            1.0,
            1,
        );
        s.insert(
            ObjectKind::Room,
            vec![],
            props(&[("area", "20.0"), ("perimeter", "18.0"), ("layer", "A-AREA")]),
            1.0,
            1,
        );
        let mut g = Graph::new();
        g.add(door, Relation::BelongsTo, wall);
        measure(&s, &g, &TakeoffConfig::default())
    }

    #[test]
    fn bill_is_sorted_by_cost_code() {
        let boq = build(&sample());
        assert!(!boq.is_empty());
        let codes: Vec<&str> = boq.lines.iter().map(|l| l.cost_code.code).collect();
        let mut sorted = codes.clone();
        sorted.sort();
        assert_eq!(codes, sorted, "lines must come out in cost-code order");
    }

    #[test]
    fn masonry_volume_reaches_the_bill() {
        let q = sample();
        let boq = build(&q);
        let line = boq
            .line("04 20 00", "Masonry in walls")
            .expect("masonry line");
        assert_eq!(line.unit, Unit::CubicMetre);
        assert!((line.quantity - q.total_wall_volume()).abs() < 1e-9);
    }

    #[test]
    fn painting_is_billed_against_the_painting_code() {
        let boq = build(&sample());
        let line = boq
            .line("09 91 00", "Painting to walls and ceilings")
            .expect("painting line");
        assert!((line.quantity - 74.0).abs() < 1e-9, "20 floor + 54 walls");
    }

    #[test]
    fn zero_quantity_lines_are_dropped() {
        // A drawing with only a room: no wall lines should appear at all.
        let mut s = ObjectStore::new();
        s.insert(
            ObjectKind::Room,
            vec![],
            props(&[("area", "12.0"), ("perimeter", "14.0")]),
            1.0,
            1,
        );
        let q = measure(&s, &Graph::new(), &TakeoffConfig::default());
        let boq = build(&q);
        assert!(boq.line("04 20 00", "Masonry in walls").is_none());
        assert!(boq.lines.iter().all(|l| l.quantity > 0.0));
    }

    #[test]
    fn aggregation_sums_across_objects() {
        let mut s = ObjectStore::new();
        for _ in 0..3 {
            s.insert(
                ObjectKind::Wall,
                vec![],
                props(&[
                    ("length", "4.0"),
                    ("thickness", "0.23"),
                    ("layer", "A-WALL"),
                ]),
                1.0,
                1,
            );
        }
        let q = measure(&s, &Graph::new(), &TakeoffConfig::default());
        let boq = build(&q);
        let line = boq.line("04 20 00", "Wall area (net of openings)").unwrap();
        assert!(
            (line.quantity - 36.0).abs() < 1e-9,
            "3 walls × 4 m × 3 m = 36 m², got {}",
            line.quantity
        );
    }

    #[test]
    fn building_is_deterministic() {
        assert_eq!(build(&sample()), build(&sample()));
    }

    #[test]
    fn empty_quantities_give_an_empty_bill() {
        assert!(build(&Quantities::default()).is_empty());
    }
}
