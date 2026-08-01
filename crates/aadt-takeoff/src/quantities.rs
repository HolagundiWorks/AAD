//! The quantity engine: measured work read off the **Object DB**.
//!
//! This is the Phase-4 difference. The removed M3 engine measured raw geometry
//! and had no way to know which opening pierced which wall — it could compute a
//! gross wall area but not a net one. Objects carry engineering properties, and
//! the Graph DB carries `Door —BelongsTo→ Wall`, so an opening deduction is a
//! graph lookup rather than a geometric guess.
//!
//! Everything is deterministic: objects iterate in id order, so the same drawing
//! yields the same quantities in the same order.

use aadt_graph::{Graph, Relation};
use aadt_object::{EngObject, ObjectId, ObjectKind, ObjectStore};

use crate::costcode::{code_for, CostCode};

/// Assumptions the takeoff needs that 2D geometry cannot supply.
///
/// AADT is a 2D tool (ROADMAP §"no 3D"): a plan has no wall height, so it is an
/// input, not a measurement. Defaults are typical Indian residential values.
#[derive(Debug, Clone, Copy)]
pub struct TakeoffConfig {
    /// Storey height used for wall areas, m.
    pub wall_height: f64,
    /// Assumed door height when the object carries none, m.
    pub door_height: f64,
    /// Assumed window height when the object carries none, m.
    pub window_height: f64,
    /// Assumed door width when the object carries none, m.
    pub door_width: f64,
    /// Assumed window width when the object carries none, m.
    pub window_width: f64,
    /// Plaster thickness per face, m (for mortar/plaster volume).
    pub plaster_thickness: f64,
    /// Nominal brick volume including mortar joint, m³ — 230×110×75 mm plus a
    /// 10 mm joint, the standard modular brick.
    pub brick_volume: f64,
}

impl Default for TakeoffConfig {
    fn default() -> Self {
        Self {
            wall_height: 3.0,
            door_height: 2.1,
            window_height: 1.2,
            door_width: 0.9,
            window_width: 1.2,
            plaster_thickness: 0.012,
            brick_volume: 0.002_16,
        }
    }
}

/// Measured quantities for one wall.
#[derive(Debug, Clone, PartialEq)]
pub struct WallQty {
    pub object: ObjectId,
    pub layer: String,
    pub length: f64,
    pub thickness: f64,
    pub height: f64,
    /// `length × height`, before deductions.
    pub gross_area: f64,
    /// Total area of openings that belong to this wall.
    pub opening_area: f64,
    /// `gross_area − opening_area`, never negative.
    pub net_area: f64,
    /// `net_area × thickness`.
    pub volume: f64,
    /// Bricks required for `volume`, rounded up.
    pub bricks: u64,
    /// Plaster area — both faces of the net wall.
    pub plaster_area: f64,
    pub cost_code: CostCode,
}

/// Measured quantities for one room.
#[derive(Debug, Clone, PartialEq)]
pub struct RoomQty {
    pub object: ObjectId,
    pub layer: String,
    pub floor_area: f64,
    pub perimeter: f64,
    /// Equal to the floor area for a flat ceiling.
    pub ceiling_area: f64,
    /// `perimeter × height` — the internal surface to finish.
    pub wall_finish_area: f64,
    /// Floor + ceiling + walls: everything that gets painted.
    pub paint_area: f64,
    pub cost_code: CostCode,
}

/// One scheduled opening.
#[derive(Debug, Clone, PartialEq)]
pub struct OpeningQty {
    pub object: ObjectId,
    /// `Door` or `Window`.
    pub kind: ObjectKind,
    pub width: f64,
    pub height: f64,
    pub area: f64,
    /// The wall this opening belongs to, when the graph records one.
    pub in_wall: Option<ObjectId>,
    /// Whether `width`/`height` came from the object or from config defaults.
    pub measured: bool,
    pub cost_code: CostCode,
}

/// One structural member.
#[derive(Debug, Clone, PartialEq)]
pub struct StructuralQty {
    pub object: ObjectId,
    pub kind: ObjectKind,
    pub layer: String,
    pub cost_code: CostCode,
}

/// Everything a takeoff run measured.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Quantities {
    pub walls: Vec<WallQty>,
    pub rooms: Vec<RoomQty>,
    pub openings: Vec<OpeningQty>,
    pub structural: Vec<StructuralQty>,
}

impl Quantities {
    /// Total net wall area across the drawing, m².
    pub fn total_wall_area(&self) -> f64 {
        nonneg(self.walls.iter().map(|w| w.net_area).sum())
    }

    /// Total wall volume, m³.
    pub fn total_wall_volume(&self) -> f64 {
        nonneg(self.walls.iter().map(|w| w.volume).sum())
    }

    /// Total floor area, m².
    pub fn total_floor_area(&self) -> f64 {
        nonneg(self.rooms.iter().map(|r| r.floor_area).sum())
    }

    /// Total bricks.
    pub fn total_bricks(&self) -> u64 {
        self.walls.iter().map(|w| w.bricks).sum()
    }

    /// Total paintable area, m².
    pub fn total_paint_area(&self) -> f64 {
        nonneg(self.rooms.iter().map(|r| r.paint_area).sum())
    }

    /// Whether nothing at all was measured.
    pub fn is_empty(&self) -> bool {
        self.walls.is_empty()
            && self.rooms.is_empty()
            && self.openings.is_empty()
            && self.structural.is_empty()
    }
}

/// Read a numeric property, falling back to `default` when absent or unparsable.
fn num(o: &EngObject, key: &str, default: f64) -> f64 {
    o.prop(key)
        .and_then(|v| v.trim().parse::<f64>().ok())
        .unwrap_or(default)
}

/// Clamp a measured quantity to a non-negative value, normalising signed zero.
///
/// Rust's `Sum for f64` folds from `-0.0` (so signed zeros compose correctly),
/// which means summing *no* openings yields `-0.0` — and `(-0.0).max(0.0)` is
/// still `-0.0`. Arithmetically that is zero, but a bill of quantities printing
/// `-0.00 m²` reads as a defect and undermines every other figure on the page.
fn nonneg(x: f64) -> f64 {
    if x <= 0.0 {
        0.0
    } else {
        x
    }
}

fn layer_of(o: &EngObject) -> String {
    o.prop("layer").unwrap_or("").to_string()
}

/// Measure `objects` into quantities, using `graph` to attribute openings to
/// their walls.
pub fn measure(objects: &ObjectStore, graph: &Graph, cfg: &TakeoffConfig) -> Quantities {
    let mut q = Quantities::default();

    // Openings first: walls need their deductions.
    for o in objects.iter() {
        let (kind, default_w, default_h) = match o.kind {
            ObjectKind::Door => (o.kind, cfg.door_width, cfg.door_height),
            ObjectKind::Window => (o.kind, cfg.window_width, cfg.window_height),
            _ => continue,
        };
        let measured = o.prop("width").is_some();
        let width = num(o, "width", default_w);
        let height = num(o, "height", default_h);
        q.openings.push(OpeningQty {
            object: o.id,
            kind,
            width,
            height,
            area: width * height,
            in_wall: host_wall(objects, graph, o.id),
            measured,
            cost_code: code_for(o),
        });
    }

    for o in objects.iter() {
        match o.kind {
            ObjectKind::Wall => {
                let length = num(o, "length", 0.0);
                let thickness = num(o, "thickness", 0.0);
                let height = num(o, "height", cfg.wall_height);
                let gross_area = nonneg(length * height);
                let opening_area: f64 = nonneg(
                    q.openings
                        .iter()
                        .filter(|op| op.in_wall == Some(o.id))
                        .map(|op| op.area)
                        .sum(),
                );
                // An over-deduction means the openings as measured exceed the
                // wall; clamp rather than report negative work.
                let net_area = nonneg(gross_area - opening_area);
                let volume = nonneg(net_area * thickness);
                let bricks = if cfg.brick_volume > 0.0 {
                    (volume / cfg.brick_volume).ceil().max(0.0) as u64
                } else {
                    0
                };
                q.walls.push(WallQty {
                    object: o.id,
                    layer: layer_of(o),
                    length,
                    thickness,
                    height,
                    gross_area,
                    opening_area,
                    net_area,
                    volume,
                    bricks,
                    plaster_area: net_area * 2.0,
                    cost_code: code_for(o),
                });
            }
            ObjectKind::Room => {
                let floor_area = num(o, "area", 0.0);
                let perimeter = num(o, "perimeter", 0.0);
                let wall_finish_area = perimeter * cfg.wall_height;
                q.rooms.push(RoomQty {
                    object: o.id,
                    layer: layer_of(o),
                    floor_area,
                    perimeter,
                    ceiling_area: floor_area,
                    wall_finish_area,
                    paint_area: floor_area + wall_finish_area,
                    cost_code: code_for(o),
                });
            }
            ObjectKind::Column | ObjectKind::Beam => {
                q.structural.push(StructuralQty {
                    object: o.id,
                    kind: o.kind,
                    layer: layer_of(o),
                    cost_code: code_for(o),
                });
            }
            _ => {}
        }
    }
    q
}

/// The wall an opening belongs to, via `Door —BelongsTo→ Wall`.
fn host_wall(objects: &ObjectStore, graph: &Graph, opening: ObjectId) -> Option<ObjectId> {
    graph
        .relations_of(opening)
        .iter()
        .filter(|e| e.rel == Relation::BelongsTo && e.from == opening)
        .map(|e| e.to)
        .find(|id| objects.get(*id).is_some_and(|o| o.kind == ObjectKind::Wall))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn props(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    /// A wall 6 m × 0.23 m with one 0.9 × 2.1 door belonging to it.
    fn wall_with_door() -> (ObjectStore, Graph) {
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
        let mut g = Graph::new();
        g.add(door, Relation::BelongsTo, wall);
        (s, g)
    }

    #[test]
    fn openings_are_deducted_through_the_graph() {
        let (s, g) = wall_with_door();
        let q = measure(&s, &g, &TakeoffConfig::default());
        assert_eq!(q.walls.len(), 1);
        let w = &q.walls[0];
        assert!((w.gross_area - 18.0).abs() < 1e-9, "6 × 3 = 18");
        assert!((w.opening_area - 1.89).abs() < 1e-9, "0.9 × 2.1 = 1.89");
        assert!(
            (w.net_area - 16.11).abs() < 1e-9,
            "18 − 1.89 = 16.11, got {}",
            w.net_area
        );
        assert!((w.volume - 16.11 * 0.23).abs() < 1e-9);
    }

    #[test]
    fn an_unattached_opening_is_not_deducted_from_any_wall() {
        let (mut s, _) = wall_with_door();
        // Same objects, but no graph edge: the engine must not guess a host.
        let g = Graph::new();
        let extra = s.insert(ObjectKind::Window, vec![], props(&[]), 1.0, 1);
        let _ = extra;
        let q = measure(&s, &g, &TakeoffConfig::default());
        assert!(
            (q.walls[0].opening_area).abs() < 1e-9,
            "no edge → no deduction"
        );
        assert!(q.openings.iter().all(|o| o.in_wall.is_none()));
    }

    #[test]
    fn openings_without_dimensions_use_config_defaults_and_are_flagged() {
        let mut s = ObjectStore::new();
        let door = s.insert(ObjectKind::Door, vec![], props(&[]), 1.0, 1);
        let _ = door;
        let q = measure(&s, &Graph::new(), &TakeoffConfig::default());
        assert_eq!(q.openings.len(), 1);
        let o = &q.openings[0];
        assert!(!o.measured, "flagged as assumed, not measured");
        assert!((o.width - 0.9).abs() < 1e-9);
        assert!((o.height - 2.1).abs() < 1e-9);
    }

    #[test]
    fn over_deduction_clamps_at_zero() {
        let mut s = ObjectStore::new();
        let wall = s.insert(
            ObjectKind::Wall,
            vec![],
            props(&[("length", "1.0"), ("thickness", "0.23")]),
            1.0,
            1,
        );
        // A 5 × 5 opening in a 1 × 3 wall: nonsense input, must not go negative.
        let door = s.insert(
            ObjectKind::Door,
            vec![],
            props(&[("width", "5.0"), ("height", "5.0")]),
            1.0,
            1,
        );
        let mut g = Graph::new();
        g.add(door, Relation::BelongsTo, wall);
        let q = measure(&s, &g, &TakeoffConfig::default());
        assert_eq!(q.walls[0].net_area, 0.0);
        assert_eq!(q.walls[0].volume, 0.0);
        assert_eq!(q.walls[0].bricks, 0);
    }

    #[test]
    fn room_finishes_are_measured() {
        let mut s = ObjectStore::new();
        s.insert(
            ObjectKind::Room,
            vec![],
            props(&[("area", "20.0"), ("perimeter", "18.0"), ("layer", "A-AREA")]),
            1.0,
            1,
        );
        let q = measure(&s, &Graph::new(), &TakeoffConfig::default());
        let r = &q.rooms[0];
        assert!((r.floor_area - 20.0).abs() < 1e-9);
        assert!((r.ceiling_area - 20.0).abs() < 1e-9);
        assert!((r.wall_finish_area - 54.0).abs() < 1e-9, "18 × 3");
        assert!((r.paint_area - 74.0).abs() < 1e-9, "20 + 54");
    }

    #[test]
    fn brick_count_follows_volume() {
        let (s, g) = wall_with_door();
        let cfg = TakeoffConfig::default();
        let q = measure(&s, &g, &cfg);
        let w = &q.walls[0];
        let expected = (w.volume / cfg.brick_volume).ceil() as u64;
        assert_eq!(w.bricks, expected);
        assert!(w.bricks > 0);
    }

    #[test]
    fn measurement_is_deterministic() {
        let (s, g) = wall_with_door();
        let cfg = TakeoffConfig::default();
        assert_eq!(measure(&s, &g, &cfg), measure(&s, &g, &cfg));
    }

    #[test]
    fn quantities_never_display_as_negative_zero() {
        // A wall with no openings: the deduction sum is an empty f64 sum, which
        // Rust folds from -0.0. Nothing measured may ever render as "-0.00".
        let mut s = ObjectStore::new();
        s.insert(
            ObjectKind::Wall,
            vec![],
            props(&[("length", "6.0"), ("thickness", "0.23")]),
            1.0,
            1,
        );
        let q = measure(&s, &Graph::new(), &TakeoffConfig::default());
        let w = &q.walls[0];
        assert_eq!(
            format!("{:.2}", w.opening_area),
            "0.00",
            "an empty deduction must not print as negative zero"
        );
        assert!(!w.opening_area.is_sign_negative());

        // The same applies to totals summed over empty collections.
        let empty = measure(
            &ObjectStore::new(),
            &Graph::new(),
            &TakeoffConfig::default(),
        );
        for (label, v) in [
            ("wall area", empty.total_wall_area()),
            ("wall volume", empty.total_wall_volume()),
            ("floor area", empty.total_floor_area()),
            ("paint area", empty.total_paint_area()),
        ] {
            assert_eq!(format!("{v:.2}"), "0.00", "{label} printed as {v:.2}");
        }
    }

    #[test]
    fn empty_store_measures_nothing() {
        let q = measure(
            &ObjectStore::new(),
            &Graph::new(),
            &TakeoffConfig::default(),
        );
        assert!(q.is_empty());
    }

    #[test]
    fn structural_members_are_scheduled() {
        let mut s = ObjectStore::new();
        s.insert(
            ObjectKind::Column,
            vec![],
            props(&[("layer", "S-COLS")]),
            1.0,
            1,
        );
        s.insert(
            ObjectKind::Beam,
            vec![],
            props(&[("layer", "S-BEAM")]),
            1.0,
            1,
        );
        let q = measure(&s, &Graph::new(), &TakeoffConfig::default());
        assert_eq!(q.structural.len(), 2);
    }
}
