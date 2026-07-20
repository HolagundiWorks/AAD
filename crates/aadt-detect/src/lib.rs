//! # aadt-detect — Element Detection & Classification (flagship, spec §5.1)
//!
//! A geometry/topology pipeline, **deterministic by construction** — precision here
//! is what makes downstream takeoff numbers trustworthy (spec §5.1):
//!
//! 1. **Detect walls** — pair parallel `LINE` faces at a consistent offset within a
//!    configurable thickness range and with sufficient overlap; each pair yields a
//!    wall with a centerline, thickness, and length. Layer-name hints (e.g.
//!    `A-WALL`) raise confidence but are not required.
//! 2. **Resolve topology** — merge wall endpoints into a [`WallGraph`] (nodes =
//!    coincident junctions, edges = wall runs), and extract its bounded faces as
//!    graph-derived rooms ([`WallGraph::faces`]).
//! 3. **Rooms** — from explicit closed polylines (area, perimeter, centroid) and,
//!    separately, from the wall graph's faces. [`Detection::merged_rooms`]
//!    reconciles the two sources for takeoff (explicit wins).
//! 4. **Classify** — inserts as doors/windows/furniture by block-name pattern,
//!    circles/structural-layer rectangles as columns/beams; sequential numbering
//!    and tag points ([`Detection::tags`]).
//!
//! Deferred (see `docs/ROADMAP.md`): AI wall-type inference (spec §5.1 step 6) →
//! **M4**.
//!
//! The pipeline is pure and order-independent: it reads a [`Document`] and returns a
//! [`Detection`]; the same input always yields the same output (spec §8).

use std::collections::HashSet;

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
    /// Block-name / layer conventions used to classify inserts and structural
    /// elements (spec §5.1 step 3).
    pub classify: ClassifyConfig,
}

impl Default for DetectionConfig {
    fn default() -> Self {
        // Defaults suit metre-unit residential/commercial plans; overridable per run.
        Self {
            min_thickness: 0.05,
            max_thickness: 1.0,
            min_overlap: 0.1,
            junction_tolerance: 1e-3,
            classify: ClassifyConfig::default(),
        }
    }
}

/// Naming conventions used to classify block inserts and structural elements.
/// Matching is case-insensitive substring matching on block name / layer name;
/// shape-based classification is deliberately avoided as unreliable (spec §5.1).
#[derive(Debug, Clone)]
pub struct ClassifyConfig {
    pub door_patterns: Vec<String>,
    pub window_patterns: Vec<String>,
    pub furniture_patterns: Vec<String>,
    /// Layer-name substrings that mark structural geometry (columns/beams).
    pub structural_layers: Vec<String>,
    /// Attribute key read for an opening's width, if present.
    pub width_attribute: String,
}

impl Default for ClassifyConfig {
    fn default() -> Self {
        let owned = |xs: &[&str]| xs.iter().map(|s| s.to_string()).collect();
        Self {
            door_patterns: owned(&["door", "dr"]),
            window_patterns: owned(&["window", "win"]),
            furniture_patterns: owned(&["furn", "desk", "chair", "sofa", "table", "bed"]),
            structural_layers: owned(&["s-", "struct", "col"]),
            width_attribute: "width".to_string(),
        }
    }
}

impl ClassifyConfig {
    fn matches(patterns: &[String], haystack: &str) -> bool {
        let h = haystack.to_lowercase();
        patterns.iter().any(|p| h.contains(&p.to_lowercase()))
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

impl WallGraph {
    /// Extract the bounded interior faces of the planar wall graph as cycles of
    /// node indices (spec §5.1 step 2: the wall graph yields room boundaries).
    ///
    /// Uses the standard next-half-edge traversal: at each node the incident edges
    /// are sorted by angle, and a face is traced by always taking the next edge
    /// clockwise from the reverse of the arriving edge. Every directed half-edge
    /// belongs to exactly one traced face; the single unbounded outer face is
    /// discarded by keeping only clockwise (negative-signed-area) cycles.
    pub fn faces(&self) -> Vec<Vec<usize>> {
        let n = self.nodes.len();
        if n == 0 {
            return Vec::new();
        }

        // Adjacency, each neighbour list sorted counter-clockwise by angle.
        let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
        for &(a, b, _) in &self.edges {
            if a != b {
                adj[a].push(b);
                adj[b].push(a);
            }
        }
        for (v, list) in adj.iter_mut().enumerate() {
            let vp = self.nodes[v];
            list.sort_by(|&x, &y| {
                let ax = (self.nodes[x].y - vp.y).atan2(self.nodes[x].x - vp.x);
                let ay = (self.nodes[y].y - vp.y).atan2(self.nodes[y].x - vp.x);
                ax.total_cmp(&ay)
            });
            list.dedup();
        }

        let position = |v: usize, u: usize| adj[v].iter().position(|&w| w == u);

        let mut visited: HashSet<(usize, usize)> = HashSet::new();
        let mut raw_faces: Vec<Vec<usize>> = Vec::new();
        let safety_cap = self.edges.len() * 4 + 8;

        for &(a, b, _) in &self.edges {
            for &(sa, sb) in &[(a, b), (b, a)] {
                if visited.contains(&(sa, sb)) {
                    continue;
                }
                let (mut u, mut v) = (sa, sb);
                let mut cycle = Vec::new();
                loop {
                    visited.insert((u, v));
                    cycle.push(u);
                    let Some(k) = position(v, u) else { break };
                    let m = adj[v].len();
                    let w = adj[v][(k + m - 1) % m];
                    u = v;
                    v = w;
                    if (u, v) == (sa, sb) {
                        break;
                    }
                    if cycle.len() > safety_cap {
                        break;
                    }
                }
                if cycle.len() >= 3 {
                    raw_faces.push(cycle);
                }
            }
        }

        // Keep clockwise cycles (negative signed area) — the bounded interior faces.
        raw_faces
            .into_iter()
            .filter(|cycle| {
                let verts: Vec<Point2> = cycle.iter().map(|&i| self.nodes[i]).collect();
                Polyline::new(verts, true).signed_area() < -LENGTH_EPS
            })
            .collect()
    }
}

/// The kind of a detected opening.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpeningKind {
    Door,
    Window,
}

/// A detected opening (door/window) from a block insert.
#[derive(Debug, Clone)]
pub struct Opening {
    pub number: u32,
    pub kind: OpeningKind,
    pub position: Point2,
    /// Opening width from the block's width attribute, if present.
    pub width: Option<f64>,
    pub block: String,
    pub source: EntityId,
}

/// The kind of a detected structural element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructuralKind {
    Column,
    Beam,
}

/// A detected structural element (column/beam) from a circle or structural-layer
/// rectangle.
#[derive(Debug, Clone)]
pub struct Structural {
    pub number: u32,
    pub kind: StructuralKind,
    pub position: Point2,
    pub layer: String,
    pub source: EntityId,
}

/// A detected furniture item from a block insert.
#[derive(Debug, Clone)]
pub struct Furniture {
    pub number: u32,
    pub block: String,
    pub position: Point2,
    pub source: EntityId,
}

/// The full result of a detection pass.
#[derive(Debug, Clone, Default)]
pub struct Detection {
    pub walls: Vec<Wall>,
    /// Rooms from explicit closed polylines in the drawing.
    pub rooms: Vec<Room>,
    pub graph: WallGraph,
    /// Rooms derived from the bounded faces of the wall graph (spec §5.1 step 2).
    /// Kept separate from [`rooms`](Self::rooms) so the two sources stay
    /// individually inspectable; [`merged_rooms`](Self::merged_rooms) reconciles
    /// them into the set a takeoff should count.
    pub graph_rooms: Vec<Room>,
    pub openings: Vec<Opening>,
    pub structural: Vec<Structural>,
    pub furniture: Vec<Furniture>,
}

/// A tag marker point for a detected element, at its centroid/insertion point
/// (spec §5.1 step 4). Deterministic block insertion into the document is a later
/// concern; detection only computes where tags would go.
#[derive(Debug, Clone)]
pub struct Tag {
    pub point: Point2,
    pub label: String,
}

impl Detection {
    /// The rooms a takeoff should count: explicit polyline rooms, plus every
    /// graph-derived room that does not describe a space an explicit room
    /// already covers (spec §5.1 step 2).
    ///
    /// **Policy — explicit wins.** A graph room is dropped when its centroid
    /// lies inside an explicit room, *or* when it encloses an explicit room's
    /// centroid. The second half matters: walls bounding a suite that is
    /// subdivided into explicitly drawn rooms yield one large face enclosing
    /// them all, and counting it would double-count the floor area.
    ///
    /// Only walls-only drawings — no explicit polylines — gain rooms here.
    /// Where an author drew the boundary, their intent is authoritative: the
    /// polyline follows the finished face, whereas a graph face follows wall
    /// centerlines and so overstates area by half the wall thickness all round.
    ///
    /// Deterministic: explicit rooms keep their order, then surviving graph
    /// rooms in face order. Ids are reassigned sequentially across the merged
    /// set so downstream numbering stays stable.
    pub fn merged_rooms(&self) -> Vec<Room> {
        let explicit_centroids: Vec<Point2> = self
            .rooms
            .iter()
            .filter_map(|r| r.boundary.centroid())
            .collect();

        let mut merged: Vec<Room> = self.rooms.clone();
        for gr in &self.graph_rooms {
            let covered = gr
                .boundary
                .centroid()
                .is_some_and(|c| self.rooms.iter().any(|r| r.boundary.contains_point(c)))
                || explicit_centroids
                    .iter()
                    .any(|&c| gr.boundary.contains_point(c));
            if !covered {
                merged.push(gr.clone());
            }
        }
        for (i, r) in merged.iter_mut().enumerate() {
            r.id = i;
        }
        merged
    }

    /// Tag points for every numbered element, deterministically ordered
    /// (walls, rooms, openings, structural, furniture).
    pub fn tags(&self) -> Vec<Tag> {
        let mut tags = Vec::new();
        for w in &self.walls {
            tags.push(Tag {
                point: w.centerline.midpoint(),
                label: format!("W{}", w.id + 1),
            });
        }
        for (i, r) in self.rooms.iter().enumerate() {
            if let Some(c) = r.boundary.centroid() {
                tags.push(Tag {
                    point: c,
                    label: format!("R{}", i + 1),
                });
            }
        }
        for o in &self.openings {
            let prefix = match o.kind {
                OpeningKind::Door => "D",
                OpeningKind::Window => "WN",
            };
            tags.push(Tag {
                point: o.position,
                label: format!("{prefix}{}", o.number),
            });
        }
        for s in &self.structural {
            let prefix = match s.kind {
                StructuralKind::Column => "C",
                StructuralKind::Beam => "B",
            };
            tags.push(Tag {
                point: s.position,
                label: format!("{prefix}{}", s.number),
            });
        }
        for fitem in &self.furniture {
            tags.push(Tag {
                point: fitem.position,
                label: format!("F{}", fitem.number),
            });
        }
        tags
    }
}

/// Run the deterministic detection pipeline over a document.
pub fn detect(doc: &Document, cfg: &DetectionConfig) -> Detection {
    let walls = detect_walls(doc, cfg);
    let rooms = detect_rooms(doc);
    let graph = build_graph(&walls, cfg);
    let graph_rooms = derive_graph_rooms(&graph);
    let (openings, structural, furniture) = classify(doc, &cfg.classify);
    Detection {
        walls,
        rooms,
        graph,
        graph_rooms,
        openings,
        structural,
        furniture,
    }
}

/// Build rooms from the bounded faces of the wall graph. Graph-derived rooms carry
/// no source entity or layer, so `source` is a sentinel and `layer` is empty.
fn derive_graph_rooms(graph: &WallGraph) -> Vec<Room> {
    graph
        .faces()
        .into_iter()
        .enumerate()
        .filter_map(|(i, cycle)| {
            let verts: Vec<Point2> = cycle.iter().map(|&idx| graph.nodes[idx]).collect();
            let boundary = Polyline::new(verts, true);
            let area = boundary.area();
            if area <= LENGTH_EPS {
                return None;
            }
            let perimeter = boundary.perimeter();
            Some(Room {
                id: i,
                boundary,
                area,
                perimeter,
                layer: String::new(),
                source: EntityId(u64::MAX),
            })
        })
        .collect()
}

/// Classify block inserts (doors/windows/furniture) and structural geometry
/// (columns/beams), numbering each category sequentially in ascending entity id.
fn classify(
    doc: &Document,
    cfg: &ClassifyConfig,
) -> (Vec<Opening>, Vec<Structural>, Vec<Furniture>) {
    let mut openings = Vec::new();
    let mut structural = Vec::new();
    let mut furniture = Vec::new();

    let is_structural_layer = |layer: &str| ClassifyConfig::matches(&cfg.structural_layers, layer);

    for e in doc.entities() {
        match &e.geometry {
            Geometry::Insert(ins) => {
                if ClassifyConfig::matches(&cfg.door_patterns, &ins.name) {
                    openings.push(Opening {
                        number: openings.len() as u32 + 1,
                        kind: OpeningKind::Door,
                        position: ins.position,
                        width: e
                            .attribute(&cfg.width_attribute)
                            .and_then(|w| w.parse().ok()),
                        block: ins.name.clone(),
                        source: e.id,
                    });
                } else if ClassifyConfig::matches(&cfg.window_patterns, &ins.name) {
                    openings.push(Opening {
                        number: openings.len() as u32 + 1,
                        kind: OpeningKind::Window,
                        position: ins.position,
                        width: e
                            .attribute(&cfg.width_attribute)
                            .and_then(|w| w.parse().ok()),
                        block: ins.name.clone(),
                        source: e.id,
                    });
                } else if ClassifyConfig::matches(&cfg.furniture_patterns, &ins.name) {
                    furniture.push(Furniture {
                        number: furniture.len() as u32 + 1,
                        block: ins.name.clone(),
                        position: ins.position,
                        source: e.id,
                    });
                }
            }
            // Circles are treated as round columns (structural).
            Geometry::Circle(c) => {
                structural.push(Structural {
                    number: structural.len() as u32 + 1,
                    kind: StructuralKind::Column,
                    position: c.center,
                    layer: e.layer.clone(),
                    source: e.id,
                });
            }
            // Closed rectangles on a structural layer are columns (near-square) or
            // beams (elongated).
            Geometry::Polyline(p) if p.closed && is_structural_layer(&e.layer) => {
                if let Some(kind) = rectangle_structural_kind(p) {
                    let position = p.centroid().unwrap_or(Point2::new(0.0, 0.0));
                    structural.push(Structural {
                        number: structural.len() as u32 + 1,
                        kind,
                        position,
                        layer: e.layer.clone(),
                        source: e.id,
                    });
                }
            }
            _ => {}
        }
    }

    (openings, structural, furniture)
}

/// Classify a closed polyline's structural role by its bounding-box aspect ratio,
/// or `None` if it is not a plausible rectangle (not 4 vertices).
fn rectangle_structural_kind(p: &Polyline) -> Option<StructuralKind> {
    if p.vertices.len() != 4 {
        return None;
    }
    let bbox = aadt_geometry::Bbox::of_points(p.vertices.iter().copied())?;
    let (w, h) = (bbox.width(), bbox.height());
    let (long, short) = if w >= h { (w, h) } else { (h, w) };
    if short <= LENGTH_EPS {
        return None;
    }
    // Aspect ratio >= 3 reads as a beam; squarer reads as a column.
    Some(if long / short >= 3.0 {
        StructuralKind::Beam
    } else {
        StructuralKind::Column
    })
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
            // Arcs are deliberately not wall faces: a curved wall needs arc-pair
            // detection, not chord approximation, or it would be reported as a
            // run of short straight walls. Tracked for a later milestone.
            Geometry::Polyline(_)
            | Geometry::Circle(_)
            | Geometry::Arc(_)
            | Geometry::Text(_)
            | Geometry::Insert(_) => None,
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

    fn insert(doc: &mut Document, layer: &str, name: &str, x: f64, y: f64) -> EntityId {
        doc.add_geometry(
            layer,
            Geometry::Insert(aadt_model::Insert::new(name, Point2::new(x, y), 0.0)),
        )
    }

    #[test]
    fn classifies_doors_windows_and_furniture() {
        let mut doc = Document::new();
        insert(&mut doc, "A-DOOR", "DOOR-SINGLE", 1.0, 0.0);
        insert(&mut doc, "A-GLAZ", "WINDOW-FIX", 2.0, 0.0);
        insert(&mut doc, "A-FURN", "DESK-1200", 3.0, 0.0);
        insert(&mut doc, "A-MISC", "SYMBOL-NORTH", 4.0, 0.0); // unclassified

        let det = detect(&doc, &DetectionConfig::default());
        assert_eq!(det.openings.len(), 2);
        assert_eq!(det.openings[0].kind, OpeningKind::Door);
        assert_eq!(det.openings[0].number, 1);
        assert_eq!(det.openings[1].kind, OpeningKind::Window);
        assert_eq!(det.furniture.len(), 1);
        assert_eq!(det.furniture[0].block, "DESK-1200");
    }

    #[test]
    fn reads_opening_width_attribute() {
        let mut doc = Document::new();
        let mut attrs = std::collections::HashMap::new();
        attrs.insert("width".to_string(), "0.9".to_string());
        doc.add_entity(
            "A-DOOR",
            Geometry::Insert(aadt_model::Insert::new("DOOR", Point2::new(0.0, 0.0), 0.0)),
            attrs,
        );
        let det = detect(&doc, &DetectionConfig::default());
        assert_eq!(det.openings[0].width, Some(0.9));
    }

    #[test]
    fn classifies_structural_circle_and_beam() {
        let mut doc = Document::new();
        // Round column anywhere.
        doc.add_geometry(
            "A-COLS",
            Geometry::Circle(aadt_geometry::Circle::new(Point2::new(0.0, 0.0), 0.3)),
        );
        // Elongated rectangle on a structural layer -> beam.
        doc.add_geometry(
            "S-BEAM",
            Geometry::Polyline(Polyline::new(
                vec![
                    Point2::new(0.0, 0.0),
                    Point2::new(6.0, 0.0),
                    Point2::new(6.0, 0.3),
                    Point2::new(0.0, 0.3),
                ],
                true,
            )),
        );
        let det = detect(&doc, &DetectionConfig::default());
        assert_eq!(det.structural.len(), 2);
        assert_eq!(det.structural[0].kind, StructuralKind::Column);
        assert_eq!(det.structural[1].kind, StructuralKind::Beam);
    }

    #[test]
    fn derives_room_from_closed_wall_loop() {
        // Four walls whose centerlines meet at the corners of a 10×10 square.
        // Each wall is a pair of faces 0.2 apart straddling the centerline.
        let mut doc = Document::new();
        // Bottom (centerline y=0), top (y=10), left (x=0), right (x=10).
        line(&mut doc, "A-WALL", 0.0, -0.1, 10.0, -0.1);
        line(&mut doc, "A-WALL", 0.0, 0.1, 10.0, 0.1);
        line(&mut doc, "A-WALL", 0.0, 9.9, 10.0, 9.9);
        line(&mut doc, "A-WALL", 0.0, 10.1, 10.0, 10.1);
        line(&mut doc, "A-WALL", -0.1, 0.0, -0.1, 10.0);
        line(&mut doc, "A-WALL", 0.1, 0.0, 0.1, 10.0);
        line(&mut doc, "A-WALL", 9.9, 0.0, 9.9, 10.0);
        line(&mut doc, "A-WALL", 10.1, 0.0, 10.1, 10.0);

        let det = detect(&doc, &DetectionConfig::default());
        assert_eq!(det.walls.len(), 4, "expected four walls");
        assert_eq!(
            det.graph.nodes.len(),
            4,
            "centerlines should share four corners"
        );
        assert_eq!(det.graph_rooms.len(), 1, "one enclosed room");
        assert!((det.graph_rooms[0].area - 100.0).abs() < 1e-6);
        assert!((det.graph_rooms[0].perimeter - 40.0).abs() < 1e-6);
        // No explicit polyline rooms in this drawing.
        assert!(det.rooms.is_empty());
    }

    /// The four walls of `derives_room_from_closed_wall_loop`: centerlines on a
    /// 10×10 square, each wall a pair of faces 0.2 apart.
    fn closed_wall_square(doc: &mut Document) {
        line(doc, "A-WALL", 0.0, -0.1, 10.0, -0.1);
        line(doc, "A-WALL", 0.0, 0.1, 10.0, 0.1);
        line(doc, "A-WALL", 0.0, 9.9, 10.0, 9.9);
        line(doc, "A-WALL", 0.0, 10.1, 10.0, 10.1);
        line(doc, "A-WALL", -0.1, 0.0, -0.1, 10.0);
        line(doc, "A-WALL", 0.1, 0.0, 0.1, 10.0);
        line(doc, "A-WALL", 9.9, 0.0, 9.9, 10.0);
        line(doc, "A-WALL", 10.1, 0.0, 10.1, 10.0);
    }

    fn room_polyline(doc: &mut Document, x1: f64, y1: f64, x2: f64, y2: f64) {
        doc.add_geometry(
            "A-ROOM",
            Geometry::Polyline(Polyline::new(
                vec![
                    Point2::new(x1, y1),
                    Point2::new(x2, y1),
                    Point2::new(x2, y2),
                    Point2::new(x1, y2),
                ],
                true,
            )),
        );
    }

    #[test]
    fn walls_only_drawing_counts_graph_rooms() {
        let mut doc = Document::new();
        closed_wall_square(&mut doc);
        let det = detect(&doc, &DetectionConfig::default());
        assert!(det.rooms.is_empty());

        // Without a merge the takeoff would report zero floor area here.
        let merged = det.merged_rooms();
        assert_eq!(merged.len(), 1);
        assert!((merged[0].area - 100.0).abs() < 1e-6);
        assert_eq!(merged[0].id, 0, "ids are reassigned across the merged set");
    }

    #[test]
    fn graph_room_enclosing_explicit_rooms_is_dropped() {
        // Walls bound a 10x10 suite; the author drew the two rooms inside it.
        // Counting the enclosing face too would double-count the floor.
        let mut doc = Document::new();
        closed_wall_square(&mut doc);
        room_polyline(&mut doc, 0.1, 0.1, 4.9, 9.9);
        room_polyline(&mut doc, 5.1, 0.1, 9.9, 9.9);

        let det = detect(&doc, &DetectionConfig::default());
        assert_eq!(det.rooms.len(), 2);
        assert_eq!(det.graph_rooms.len(), 1);

        let merged = det.merged_rooms();
        assert_eq!(merged.len(), 2, "the enclosing graph face is dropped");
        let total: f64 = merged.iter().map(|r| r.area).sum();
        assert!(
            total < 100.0,
            "merged area {total} must not exceed the 100.0 envelope"
        );
    }

    #[test]
    fn graph_room_coincident_with_explicit_room_is_dropped() {
        // The author traced the same space the walls enclose. Explicit wins:
        // the polyline follows the finished face, the graph face the centerlines.
        let mut doc = Document::new();
        closed_wall_square(&mut doc);
        room_polyline(&mut doc, 0.1, 0.1, 9.9, 9.9);

        let det = detect(&doc, &DetectionConfig::default());
        let merged = det.merged_rooms();
        assert_eq!(merged.len(), 1);
        assert!(
            (merged[0].area - 96.04).abs() < 1e-6,
            "the explicit 9.8x9.8 room survives, not the 100.0 graph face"
        );
    }

    #[test]
    fn merged_rooms_are_deterministic() {
        let mut doc = Document::new();
        closed_wall_square(&mut doc);
        room_polyline(&mut doc, 0.1, 0.1, 4.9, 9.9);
        let det = detect(&doc, &DetectionConfig::default());
        let a: Vec<f64> = det.merged_rooms().iter().map(|r| r.area).collect();
        let b: Vec<f64> = det.merged_rooms().iter().map(|r| r.area).collect();
        assert_eq!(a, b);
    }

    #[test]
    fn no_graph_rooms_when_walls_do_not_close() {
        let mut doc = Document::new();
        // Two disjoint walls -> no cycle.
        line(&mut doc, "A-WALL", 0.0, 0.0, 10.0, 0.0);
        line(&mut doc, "A-WALL", 0.0, 0.2, 10.0, 0.2);
        let det = detect(&doc, &DetectionConfig::default());
        assert_eq!(det.walls.len(), 1);
        assert!(det.graph_rooms.is_empty());
    }

    #[test]
    fn tags_cover_all_numbered_elements() {
        let mut doc = Document::new();
        line(&mut doc, "A-WALL", 0.0, 0.0, 10.0, 0.0);
        line(&mut doc, "A-WALL", 0.0, 0.2, 10.0, 0.2);
        insert(&mut doc, "A-DOOR", "DOOR", 5.0, 0.1);
        let det = detect(&doc, &DetectionConfig::default());
        // 1 wall + 1 door = 2 tags.
        assert_eq!(det.tags().len(), 2);
        assert!(det.tags().iter().any(|t| t.label == "W1"));
        assert!(det.tags().iter().any(|t| t.label == "D1"));
    }
}
