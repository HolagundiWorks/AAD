//! Deterministic anomaly flagging — geometry the detector finds suspect.
//!
//! # Why this is not an AI feature
//!
//! Spec §6 lists anomaly flagging among the AI actions, but the flags themselves
//! are computed here, with no model involved, for the reason stated in the
//! product principles: **deterministic core, AI as accelerant**. A takeoff that
//! silently undercounts is the worst failure this product has, and the warning
//! that it might be happening must not depend on whether Ollama is installed.
//! An AI layer can later *narrate* these flags; it does not get to decide them.
//!
//! # What a flag means
//!
//! Every anomaly answers "why might the quantities be wrong?" — not "is this
//! drawing tidy?". Flags point at conditions that change numbers: a gap that
//! stops a room from closing, an opening that subtracts area from no wall, a
//! thickness that suggests two unrelated lines were paired.
//!
//! Advisory only. Nothing here alters detection or takeoff output.

use aadt_geometry::Point2;

use crate::{Detection, WallGraph};

/// How much a flag should worry the reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Worth a look; quantities are probably fine.
    Info,
    /// Quantities are likely affected.
    Warning,
}

/// The kind of suspect condition found. Ordered so flag output is stable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AnomalyKind {
    /// A wall run ends without meeting another wall. Rooms cannot close through
    /// a gap, so floor area goes uncounted.
    DanglingWallEnd,
    /// A door/window sits too far from any wall to be subtracted from one.
    OrphanOpening,
    /// A wall's thickness is a far outlier against the others in the drawing,
    /// which usually means two unrelated lines were paired into a wall.
    ImplausibleThickness,
    /// A room boundary encloses almost no area.
    DegenerateRoom,
}

impl AnomalyKind {
    pub fn as_token(self) -> &'static str {
        match self {
            AnomalyKind::DanglingWallEnd => "dangling-wall-end",
            AnomalyKind::OrphanOpening => "orphan-opening",
            AnomalyKind::ImplausibleThickness => "implausible-thickness",
            AnomalyKind::DegenerateRoom => "degenerate-room",
        }
    }
}

/// One flagged condition.
#[derive(Debug, Clone, PartialEq)]
pub struct Anomaly {
    pub kind: AnomalyKind,
    pub severity: Severity,
    /// Where in the drawing to look.
    pub location: Point2,
    /// Human-readable explanation, including the measured values.
    pub message: String,
}

/// Thresholds for what counts as suspect.
#[derive(Debug, Clone)]
pub struct AnomalyConfig {
    /// How far an opening may sit from a wall centerline before it is orphaned.
    /// Generous by default: an opening is normally *on* its wall, so this only
    /// catches inserts placed nowhere near one.
    pub max_opening_distance: f64,
    /// A room boundary at or below this area is degenerate.
    pub min_room_area: f64,
    /// A wall is flagged when its thickness differs from the drawing's median
    /// by more than this factor. Median rather than mean so a single bad wall
    /// cannot drag the reference and mask itself.
    pub thickness_outlier_factor: f64,
    /// Below this many walls, thickness outlier detection is skipped — a median
    /// over two or three walls is not evidence of anything.
    pub min_walls_for_thickness_check: usize,
    /// How close another wall must come to a loose wall end for it to count as
    /// a corner join. `None` derives it from the drawing as
    /// `median thickness × join_tolerance_factor`, which scales with the units
    /// the plan is drawn in.
    pub join_tolerance: Option<f64>,
    /// Multiplier on median wall thickness when `join_tolerance` is `None`.
    /// A mitred corner leaves its centerline ends about `thickness/√2` apart,
    /// so 1.0 clears a true corner with margin while still catching real gaps.
    pub join_tolerance_factor: f64,
}

impl Default for AnomalyConfig {
    fn default() -> Self {
        Self {
            max_opening_distance: 1.0,
            min_room_area: 1e-6,
            thickness_outlier_factor: 3.0,
            min_walls_for_thickness_check: 4,
            join_tolerance: None,
            join_tolerance_factor: 1.0,
        }
    }
}

impl Detection {
    /// Flag suspect geometry. Advisory only — detection and takeoff are
    /// unaffected.
    ///
    /// Output is deterministic: flags are sorted by kind, then by location, so
    /// the same drawing always reports the same list in the same order.
    pub fn anomalies(&self, cfg: &AnomalyConfig) -> Vec<Anomaly> {
        let mut out = Vec::new();
        self.flag_dangling_ends(cfg, &mut out);
        self.flag_orphan_openings(cfg, &mut out);
        self.flag_thickness_outliers(cfg, &mut out);
        self.flag_degenerate_rooms(cfg, &mut out);

        out.sort_by(|a, b| {
            a.kind.cmp(&b.kind).then_with(|| {
                a.location
                    .x
                    .total_cmp(&b.location.x)
                    .then_with(|| a.location.y.total_cmp(&b.location.y))
            })
        });
        out
    }

    /// Wall ends that meet nothing.
    ///
    /// A shared graph node is too strict a test for "meets another wall".
    /// Centerlines are offset half a thickness from their faces, so at a
    /// properly drafted corner the two centerline ends stop short of each other
    /// — about `thickness/√2` apart — and never merge into one node. Demanding
    /// exact node sharing flags every corner of a perfectly closed plan.
    ///
    /// So a degree-1 node is dangling only when no *other* wall's centerline
    /// comes within [`AnomalyConfig::join_tolerance`] of it.
    fn flag_dangling_ends(&self, cfg: &AnomalyConfig, out: &mut Vec<Anomaly>) {
        let tolerance = cfg
            .join_tolerance
            .unwrap_or_else(|| median_thickness(self).unwrap_or(0.0) * cfg.join_tolerance_factor);

        for (node, degree) in node_degrees(&self.graph).into_iter().enumerate() {
            if degree != 1 {
                continue;
            }
            let p = self.graph.nodes[node];
            // Walls already attached here cannot themselves close the corner.
            let incident: Vec<usize> = self
                .graph
                .edges
                .iter()
                .filter(|(a, b, _)| *a == node || *b == node)
                .map(|(_, _, wall)| *wall)
                .collect();

            let nearest = self
                .walls
                .iter()
                .filter(|w| !incident.contains(&w.id))
                .map(|w| w.centerline.distance_to_point(p))
                .fold(f64::INFINITY, f64::min);

            if nearest > tolerance {
                out.push(Anomaly {
                    kind: AnomalyKind::DanglingWallEnd,
                    severity: Severity::Warning,
                    location: p,
                    message: format!(
                        "wall run ends at ({:.3}, {:.3}) without meeting another wall; \
                         a room cannot close through this gap",
                        p.x, p.y
                    ),
                });
            }
        }
    }

    fn flag_orphan_openings(&self, cfg: &AnomalyConfig, out: &mut Vec<Anomaly>) {
        for o in &self.openings {
            // Distance to the segment, not to its infinite line: an opening past
            // the end of a wall is not on that wall.
            let nearest = self
                .walls
                .iter()
                .map(|w| w.centerline.distance_to_point(o.position))
                .fold(f64::INFINITY, f64::min);
            if nearest > cfg.max_opening_distance {
                let where_ = if self.walls.is_empty() {
                    "there are no detected walls".to_string()
                } else {
                    format!("nearest wall is {nearest:.3} away")
                };
                out.push(Anomaly {
                    kind: AnomalyKind::OrphanOpening,
                    severity: Severity::Warning,
                    location: o.position,
                    message: format!(
                        "opening {} sits clear of every wall ({where_}); \
                         its area is subtracted from nothing",
                        o.number
                    ),
                });
            }
        }
    }

    fn flag_thickness_outliers(&self, cfg: &AnomalyConfig, out: &mut Vec<Anomaly>) {
        if self.walls.len() < cfg.min_walls_for_thickness_check {
            return;
        }
        let Some(median) = median_thickness(self).filter(|m| *m > 0.0) else {
            return;
        };
        for w in &self.walls {
            let ratio = if w.thickness > median {
                w.thickness / median
            } else {
                median / w.thickness.max(f64::MIN_POSITIVE)
            };
            if ratio > cfg.thickness_outlier_factor {
                let p = w.centerline.midpoint();
                out.push(Anomaly {
                    kind: AnomalyKind::ImplausibleThickness,
                    severity: Severity::Info,
                    location: p,
                    message: format!(
                        "wall {} is {:.3} thick against a drawing median of {:.3}; \
                         the paired lines may not be one wall",
                        w.id, w.thickness, median
                    ),
                });
            }
        }
    }

    fn flag_degenerate_rooms(&self, cfg: &AnomalyConfig, out: &mut Vec<Anomaly>) {
        for r in self.merged_rooms() {
            if r.area <= cfg.min_room_area {
                let p = r.boundary.centroid().unwrap_or(Point2::new(0.0, 0.0));
                out.push(Anomaly {
                    kind: AnomalyKind::DegenerateRoom,
                    severity: Severity::Warning,
                    location: p,
                    message: format!(
                        "room {} encloses {:.6} area; it contributes no usable floor quantity",
                        r.id, r.area
                    ),
                });
            }
        }
    }
}

/// Median wall thickness, the drawing's own sense of scale. Median rather than
/// mean so one badly paired wall cannot drag the reference and mask itself.
fn median_thickness(det: &Detection) -> Option<f64> {
    if det.walls.is_empty() {
        return None;
    }
    let mut t: Vec<f64> = det.walls.iter().map(|w| w.thickness).collect();
    t.sort_by(f64::total_cmp);
    Some(t[t.len() / 2])
}

/// How many wall edges touch each graph node.
fn node_degrees(graph: &WallGraph) -> Vec<usize> {
    let mut degrees = vec![0usize; graph.nodes.len()];
    for &(a, b, _) in &graph.edges {
        if a < degrees.len() {
            degrees[a] += 1;
        }
        if b < degrees.len() {
            degrees[b] += 1;
        }
    }
    degrees
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{detect, DetectionConfig};
    use aadt_geometry::{Polyline, Segment};
    use aadt_model::{Document, Geometry, Insert};

    fn line(doc: &mut Document, layer: &str, x1: f64, y1: f64, x2: f64, y2: f64) {
        doc.add_geometry(
            layer,
            Geometry::Line(Segment::new(Point2::new(x1, y1), Point2::new(x2, y2))),
        );
    }

    fn closed_square(doc: &mut Document) {
        line(doc, "A-WALL", 0.0, -0.1, 10.0, -0.1);
        line(doc, "A-WALL", 0.0, 0.1, 10.0, 0.1);
        line(doc, "A-WALL", 0.0, 9.9, 10.0, 9.9);
        line(doc, "A-WALL", 0.0, 10.1, 10.0, 10.1);
        line(doc, "A-WALL", -0.1, 0.0, -0.1, 10.0);
        line(doc, "A-WALL", 0.1, 0.0, 0.1, 10.0);
        line(doc, "A-WALL", 9.9, 0.0, 9.9, 10.0);
        line(doc, "A-WALL", 10.1, 0.0, 10.1, 10.0);
    }

    fn anomalies_of(doc: &Document) -> Vec<Anomaly> {
        detect(doc, &DetectionConfig::default()).anomalies(&AnomalyConfig::default())
    }

    #[test]
    fn a_closed_plan_is_clean() {
        let mut doc = Document::new();
        closed_square(&mut doc);
        let flags = anomalies_of(&doc);
        assert!(flags.is_empty(), "expected no flags, got {flags:#?}");
    }

    /// Regression: the shipped sample plan is closed, and must report clean.
    ///
    /// Its wall faces span the full extent (0..10, 0..8), so each centerline is
    /// offset half a thickness and the two ends at a corner sit ~0.141 apart in
    /// separate graph nodes. An exact-node-sharing test flagged all eight
    /// corners of a correctly drafted plan.
    #[test]
    fn mitred_corners_are_not_dangling_ends() {
        let mut doc = Document::new();
        // Faces as drawn in fixtures/sample-floorplan.dxf.
        line(&mut doc, "A-WALL", 0.0, 0.0, 10.0, 0.0);
        line(&mut doc, "A-WALL", 0.0, 0.2, 10.0, 0.2);
        line(&mut doc, "A-WALL", 0.0, 8.0, 10.0, 8.0);
        line(&mut doc, "A-WALL", 0.0, 7.8, 10.0, 7.8);
        line(&mut doc, "A-WALL", 0.0, 0.0, 0.0, 8.0);
        line(&mut doc, "A-WALL", 0.2, 0.0, 0.2, 8.0);
        line(&mut doc, "A-WALL", 10.0, 0.0, 10.0, 8.0);
        line(&mut doc, "A-WALL", 9.8, 0.0, 9.8, 8.0);

        let det = detect(&doc, &DetectionConfig::default());
        assert_eq!(det.walls.len(), 4);
        assert_eq!(det.graph.nodes.len(), 8, "corners do not share nodes");

        let flags = det.anomalies(&AnomalyConfig::default());
        assert!(
            !flags.iter().any(|a| a.kind == AnomalyKind::DanglingWallEnd),
            "a closed plan must not report dangling ends, got {flags:#?}"
        );
    }

    #[test]
    fn a_real_gap_is_still_flagged_despite_the_tolerance() {
        // Same plan, but the right wall is pulled far clear of the top and
        // bottom runs. The tolerance must not swallow a genuine gap.
        let mut doc = Document::new();
        line(&mut doc, "A-WALL", 0.0, 0.0, 10.0, 0.0);
        line(&mut doc, "A-WALL", 0.0, 0.2, 10.0, 0.2);
        line(&mut doc, "A-WALL", 0.0, 8.0, 10.0, 8.0);
        line(&mut doc, "A-WALL", 0.0, 7.8, 10.0, 7.8);
        line(&mut doc, "A-WALL", 0.0, 0.0, 0.0, 8.0);
        line(&mut doc, "A-WALL", 0.2, 0.0, 0.2, 8.0);
        line(&mut doc, "A-WALL", 30.0, 0.0, 30.0, 8.0);
        line(&mut doc, "A-WALL", 29.8, 0.0, 29.8, 8.0);

        let flags = detect(&doc, &DetectionConfig::default()).anomalies(&AnomalyConfig::default());
        assert!(
            flags.iter().any(|a| a.kind == AnomalyKind::DanglingWallEnd),
            "a 20-unit gap must still be reported, got {flags:#?}"
        );
    }

    #[test]
    fn flags_a_wall_run_that_stops_short() {
        // Three sides of a square: the run has two loose ends.
        let mut doc = Document::new();
        line(&mut doc, "A-WALL", 0.0, -0.1, 10.0, -0.1);
        line(&mut doc, "A-WALL", 0.0, 0.1, 10.0, 0.1);
        line(&mut doc, "A-WALL", -0.1, 0.0, -0.1, 10.0);
        line(&mut doc, "A-WALL", 0.1, 0.0, 0.1, 10.0);

        let flags = anomalies_of(&doc);
        let dangling: Vec<_> = flags
            .iter()
            .filter(|a| a.kind == AnomalyKind::DanglingWallEnd)
            .collect();
        assert_eq!(dangling.len(), 2, "two loose ends, got {flags:#?}");
        assert!(dangling.iter().all(|a| a.severity == Severity::Warning));
    }

    #[test]
    fn flags_an_opening_placed_away_from_every_wall() {
        let mut doc = Document::new();
        closed_square(&mut doc);
        // A door out in the middle of the room, on no wall.
        doc.add_geometry(
            "A-DOOR",
            Geometry::Insert(Insert::new("DOOR-SINGLE", Point2::new(5.0, 5.0), 0.0)),
        );

        let flags = anomalies_of(&doc);
        assert_eq!(
            flags
                .iter()
                .filter(|a| a.kind == AnomalyKind::OrphanOpening)
                .count(),
            1,
            "got {flags:#?}"
        );
    }

    #[test]
    fn an_opening_on_its_wall_is_not_flagged() {
        let mut doc = Document::new();
        closed_square(&mut doc);
        // On the bottom wall's centerline.
        doc.add_geometry(
            "A-DOOR",
            Geometry::Insert(Insert::new("DOOR-SINGLE", Point2::new(5.0, 0.0), 0.0)),
        );
        let flags = anomalies_of(&doc);
        assert!(
            !flags.iter().any(|a| a.kind == AnomalyKind::OrphanOpening),
            "got {flags:#?}"
        );
    }

    #[test]
    fn an_opening_beyond_a_wall_end_is_still_orphaned() {
        // Guards the clamped-distance choice: this point is collinear with the
        // bottom wall, so an infinite-line test would call it attached.
        let mut doc = Document::new();
        closed_square(&mut doc);
        doc.add_geometry(
            "A-DOOR",
            Geometry::Insert(Insert::new("DOOR-SINGLE", Point2::new(40.0, 0.0), 0.0)),
        );
        let flags = anomalies_of(&doc);
        assert_eq!(
            flags
                .iter()
                .filter(|a| a.kind == AnomalyKind::OrphanOpening)
                .count(),
            1,
            "got {flags:#?}"
        );
    }

    #[test]
    fn flags_a_thickness_outlier_against_the_median() {
        let mut doc = Document::new();
        closed_square(&mut doc);
        // A fifth "wall": two lines 0.9 apart, far off the 0.2 median.
        line(&mut doc, "A-WALL", 20.0, 0.0, 30.0, 0.0);
        line(&mut doc, "A-WALL", 20.0, 0.9, 30.0, 0.9);

        let flags = anomalies_of(&doc);
        let thick: Vec<_> = flags
            .iter()
            .filter(|a| a.kind == AnomalyKind::ImplausibleThickness)
            .collect();
        assert_eq!(thick.len(), 1, "got {flags:#?}");
        assert!(thick[0].message.contains("0.900"));
    }

    #[test]
    fn thickness_check_is_skipped_on_tiny_drawings() {
        // Two walls of wildly different thickness: with no meaningful median,
        // flagging either would be noise.
        let mut doc = Document::new();
        line(&mut doc, "A-WALL", 0.0, 0.0, 10.0, 0.0);
        line(&mut doc, "A-WALL", 0.0, 0.1, 10.0, 0.1);
        line(&mut doc, "A-WALL", 0.0, 5.0, 10.0, 5.0);
        line(&mut doc, "A-WALL", 0.0, 5.9, 10.0, 5.9);

        let flags = anomalies_of(&doc);
        assert!(
            !flags
                .iter()
                .any(|a| a.kind == AnomalyKind::ImplausibleThickness),
            "got {flags:#?}"
        );
    }

    #[test]
    fn flags_a_room_with_no_area() {
        let mut doc = Document::new();
        // A "closed" polyline whose vertices are collinear.
        doc.add_geometry(
            "A-ROOM",
            Geometry::Polyline(Polyline::new(
                vec![
                    Point2::new(0.0, 0.0),
                    Point2::new(4.0, 0.0),
                    Point2::new(8.0, 0.0),
                ],
                true,
            )),
        );
        let flags = anomalies_of(&doc);
        assert_eq!(
            flags
                .iter()
                .filter(|a| a.kind == AnomalyKind::DegenerateRoom)
                .count(),
            1,
            "got {flags:#?}"
        );
    }

    #[test]
    fn output_is_deterministic_and_sorted() {
        let mut doc = Document::new();
        line(&mut doc, "A-WALL", 0.0, -0.1, 10.0, -0.1);
        line(&mut doc, "A-WALL", 0.0, 0.1, 10.0, 0.1);
        doc.add_geometry(
            "A-DOOR",
            Geometry::Insert(Insert::new("DOOR-SINGLE", Point2::new(50.0, 50.0), 0.0)),
        );

        let a = anomalies_of(&doc);
        let b = anomalies_of(&doc);
        assert_eq!(a, b);
        // Kinds are grouped in declaration order.
        let kinds: Vec<_> = a.iter().map(|x| x.kind).collect();
        let mut sorted = kinds.clone();
        sorted.sort();
        assert_eq!(kinds, sorted);
    }
}
