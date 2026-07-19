//! # aadt-detect — Element Detection & Classification (flagship, spec §5.1)
//!
//! A geometry/topology pipeline, **deterministic by construction** — precision here
//! is what makes downstream takeoff numbers trustworthy (spec §5.1). M0 implements
//! the deterministic core of steps 1–2:
//!
//! 1. **Detect walls** — pair parallel `LINE` faces at a consistent offset within a
//!    configurable thickness range and with sufficient overlap; each pair yields a
//!    wall with a centerline, thickness, and length. Layer-name hints (e.g.
//!    `A-WALL`) raise confidence but are not required.
//! 2. **Resolve topology** — merge wall endpoints into a [`WallGraph`] (nodes =
//!    coincident junctions, edges = wall runs).
//! 3. **Rooms** — extract closed polylines as room boundaries (area, perimeter,
//!    centroid).
//!
//! Deferred (see `docs/ROADMAP.md`):
//! - Openings, structural elements, furniture, tagging, and numbering (spec §5.1
//!   steps 3–5) → **M2**.
//! - Deriving rooms as minimal cycles of the wall graph, and AI wall-type inference
//!   (spec §5.1 step 6) → **M2 / M4**. M0 takes rooms from explicit closed polylines.
//!
//! The pipeline is pure and order-independent: it reads a [`Document`] and returns a
//! [`Detection`]; the same input always yields the same output (spec §8).

use aadt_geometry::{Point2, Polyline, Segment, LENGTH_EPS};
use aadt_model::{Document, EntityId, Geometry};

/// Tunable thresholds for the deterministic detection pass.
#[derive(Debug, Clone)]
pub struct DetectionConfig {
    /// Minimum face-to-face offset counted as a wall (drawing units).
    pub min_thickness: f64,
    /// Maximum face-to-face offset counted as a wall (drawing units).
    pub max_thickness: f64,
    /// Minimum overlap length along the wall run for a pair to count.
    pub min_overlap: f64,
    /// Tolerance for snapping wall endpoints into shared graph nodes.
    pub junction_tolerance: f64,
}

impl Default for DetectionConfig {
    fn default() -> Self {
        // Defaults suit metre-unit residential/commercial plans; overridable per run.
        Self {
            min_thickness: 0.05,
            max_thickness: 1.0,
            min_overlap: 0.1,
            junction_tolerance: 1e-3,
        }
    }
}

/// A detected wall run: a centerline with an associated thickness and the two face
/// entities it was derived from.
#[derive(Debug, Clone)]
pub struct Wall {
    pub id: usize,
    pub centerline: Segment,
    pub thickness: f64,
    pub length: f64,
    pub layer: String,
    pub faces: (EntityId, EntityId),
}

/// A detected room: a closed boundary with computed area and perimeter.
#[derive(Debug, Clone)]
pub struct Room {
    pub id: usize,
    pub boundary: Polyline,
    pub area: f64,
    pub perimeter: f64,
    pub layer: String,
    pub source: EntityId,
}

/// Topology of detected walls: nodes are coincident junctions, edges are wall runs.
#[derive(Debug, Clone, Default)]
pub struct WallGraph {
    pub nodes: Vec<Point2>,
    /// `(node_index_a, node_index_b, wall_id)` per edge.
    pub edges: Vec<(usize, usize, usize)>,
}

/// The full result of a detection pass.
#[derive(Debug, Clone, Default)]
pub struct Detection {
    pub walls: Vec<Wall>,
    pub rooms: Vec<Room>,
    pub graph: WallGraph,
}

/// Run the deterministic detection pipeline over a document.
pub fn detect(doc: &Document, cfg: &DetectionConfig) -> Detection {
    let walls = detect_walls(doc, cfg);
    let rooms = detect_rooms(doc);
    let graph = build_graph(&walls, cfg);
    Detection {
        walls,
        rooms,
        graph,
    }
}

/// One line face gathered from the document, carried with its source id and layer.
struct Face {
    id: EntityId,
    layer: String,
    seg: Segment,
}

fn detect_walls(doc: &Document, cfg: &DetectionConfig) -> Vec<Wall> {
    // M0 treats LINE entities as wall faces; polylines are rooms. Gathered in
    // ascending id order so the greedy pairing below is deterministic.
    let faces: Vec<Face> = doc
        .entities()
        .into_iter()
        .filter_map(|e| match &e.geometry {
            Geometry::Line(s) => Some(Face {
                id: e.id,
                layer: e.layer.clone(),
                seg: *s,
            }),
            Geometry::Polyline(_) => None,
        })
        .collect();

    let mut consumed = vec![false; faces.len()];
    let mut walls = Vec::new();

    for i in 0..faces.len() {
        if consumed[i] {
            continue;
        }
        for j in (i + 1)..faces.len() {
            if consumed[j] {
                continue;
            }
            if let Some(wall) = pair_faces(walls.len(), &faces[i], &faces[j], cfg) {
                consumed[i] = true;
                consumed[j] = true;
                walls.push(wall);
                break; // face i is now used; move on
            }
        }
    }
    walls
}

/// Try to form a wall from two candidate faces. `None` if they don't qualify.
fn pair_faces(next_id: usize, a: &Face, b: &Face, cfg: &DetectionConfig) -> Option<Wall> {
    if !a.seg.is_parallel(b.seg) {
        return None;
    }
    // Constant perpendicular offset between the two parallel lines.
    let offset = a.seg.perpendicular_distance(b.seg.midpoint());
    if offset < cfg.min_thickness || offset > cfg.max_thickness {
        return None;
    }
    let (lo, hi) = a.seg.overlap_interval(b.seg)?;
    let overlap = hi - lo;
    if overlap < cfg.min_overlap {
        return None;
    }

    // Centerline: midpoints between overlap endpoints on `a` and their feet on `b`.
    let p_lo = a.seg.point_at(lo);
    let p_hi = a.seg.point_at(hi);
    let c_lo = p_lo.midpoint(b.seg.foot_of(p_lo));
    let c_hi = p_hi.midpoint(b.seg.foot_of(p_hi));
    let centerline = Segment::new(c_lo, c_hi);

    Some(Wall {
        id: next_id,
        centerline,
        thickness: offset,
        length: centerline.length(),
        layer: a.layer.clone(),
        faces: (a.id, b.id),
    })
}

fn detect_rooms(doc: &Document) -> Vec<Room> {
    let mut rooms = Vec::new();
    for e in doc.entities() {
        if let Geometry::Polyline(p) = &e.geometry {
            if p.closed && p.vertices.len() >= 3 {
                rooms.push(Room {
                    id: rooms.len(),
                    boundary: p.clone(),
                    area: p.area(),
                    perimeter: p.perimeter(),
                    layer: e.layer.clone(),
                    source: e.id,
                });
            }
        }
    }
    rooms
}

fn build_graph(walls: &[Wall], cfg: &DetectionConfig) -> WallGraph {
    let mut graph = WallGraph::default();
    for wall in walls {
        let a = intern_node(&mut graph.nodes, wall.centerline.a, cfg.junction_tolerance);
        let b = intern_node(&mut graph.nodes, wall.centerline.b, cfg.junction_tolerance);
        graph.edges.push((a, b, wall.id));
    }
    graph
}

/// Return the index of an existing node within tolerance, or append a new one.
fn intern_node(nodes: &mut Vec<Point2>, p: Point2, tol: f64) -> usize {
    let tol = tol.max(LENGTH_EPS);
    if let Some(idx) = nodes.iter().position(|n| n.distance(p) <= tol) {
        idx
    } else {
        nodes.push(p);
        nodes.len() - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(doc: &mut Document, layer: &str, x1: f64, y1: f64, x2: f64, y2: f64) {
        doc.add_geometry(
            layer,
            Geometry::Line(Segment::new(Point2::new(x1, y1), Point2::new(x2, y2))),
        );
    }

    #[test]
    fn pairs_two_parallel_faces_into_one_wall() {
        let mut doc = Document::new();
        // Two faces 0.2 apart over a 10-unit run -> one wall.
        line(&mut doc, "A-WALL", 0.0, 0.0, 10.0, 0.0);
        line(&mut doc, "A-WALL", 0.0, 0.2, 10.0, 0.2);
        let det = detect(&doc, &DetectionConfig::default());
        assert_eq!(det.walls.len(), 1);
        let w = &det.walls[0];
        assert!((w.thickness - 0.2).abs() < 1e-9);
        assert!((w.length - 10.0).abs() < 1e-9);
        // Centerline sits halfway between the faces.
        assert!((w.centerline.a.y - 0.1).abs() < 1e-9);
        assert_eq!(det.graph.nodes.len(), 2);
        assert_eq!(det.graph.edges.len(), 1);
    }

    #[test]
    fn ignores_faces_beyond_thickness_range() {
        let mut doc = Document::new();
        // 3.0 apart -> a room width, not a wall.
        line(&mut doc, "A-WALL", 0.0, 0.0, 10.0, 0.0);
        line(&mut doc, "A-WALL", 0.0, 3.0, 10.0, 3.0);
        let det = detect(&doc, &DetectionConfig::default());
        assert_eq!(det.walls.len(), 0);
    }

    #[test]
    fn extracts_closed_polyline_room() {
        let mut doc = Document::new();
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
        assert_eq!(det.rooms.len(), 1);
        assert!((det.rooms[0].area - 12.0).abs() < 1e-9);
        assert!((det.rooms[0].perimeter - 14.0).abs() < 1e-9);
    }
}
