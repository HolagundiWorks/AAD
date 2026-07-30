//! # aadt-graph — the Graph Database (AI-native Phase 1, ADR-0012)
//!
//! Persisted **typed engineering relationships** between objects:
//! Door → *belongs to* → Wall · Room → *contains* → Furniture ·
//! Wall → *bounds* → Room · Wall → *connected to* → Wall. Forward and reverse
//! queries plus breadth-first traversal, all deterministic.
//!
//! The [`promote`] module assembles the Object DB + Graph DB from a drawing:
//! it runs the deterministic detector ([`aadt_detect`]) and promotes walls,
//! rooms, openings, structural elements, and furniture into intelligent
//! objects with engineering properties and relationship edges.

pub mod promote;

use aadt_object::ObjectId;

/// A typed engineering relationship.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Relation {
    /// Structural support: Beam → *supports* → Slab.
    Supports,
    /// Membership: Door → *belongs to* → Wall.
    BelongsTo,
    /// Adjacency/topology: Wall → *connected to* → Wall.
    ConnectedTo,
    /// Containment: Room → *contains* → Furniture.
    Contains,
    /// Enclosure: Wall → *bounds* → Room.
    Bounds,
}

impl Relation {
    pub const ALL: [Relation; 5] = [
        Relation::Supports,
        Relation::BelongsTo,
        Relation::ConnectedTo,
        Relation::Contains,
        Relation::Bounds,
    ];

    /// Stable persistence/display token.
    pub fn as_str(self) -> &'static str {
        match self {
            Relation::Supports => "Supports",
            Relation::BelongsTo => "BelongsTo",
            Relation::ConnectedTo => "ConnectedTo",
            Relation::Contains => "Contains",
            Relation::Bounds => "Bounds",
        }
    }

    pub fn parse(s: &str) -> Option<Relation> {
        Relation::ALL
            .into_iter()
            .find(|r| r.as_str().eq_ignore_ascii_case(s.trim()))
    }
}

impl std::fmt::Display for Relation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One directed, typed edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Edge {
    pub from: ObjectId,
    pub rel: Relation,
    pub to: ObjectId,
}

/// The graph database: a deduplicated, sorted set of typed edges.
#[derive(Debug, Clone, Default)]
pub struct Graph {
    edges: Vec<Edge>,
}

impl Graph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an edge (idempotent; edges stay sorted for deterministic queries).
    pub fn add(&mut self, from: ObjectId, rel: Relation, to: ObjectId) {
        let e = Edge { from, rel, to };
        if let Err(i) = self.edges.binary_search(&e) {
            self.edges.insert(i, e);
        }
    }

    /// Restore a persisted edge (same semantics as [`add`](Self::add)).
    pub fn restore(&mut self, edge: Edge) {
        self.add(edge.from, edge.rel, edge.to);
    }

    pub fn len(&self) -> usize {
        self.edges.len()
    }

    pub fn is_empty(&self) -> bool {
        self.edges.is_empty()
    }

    /// All edges, sorted.
    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    /// Outgoing edges of `id`.
    pub fn from(&self, id: ObjectId) -> Vec<Edge> {
        self.edges
            .iter()
            .filter(|e| e.from == id)
            .copied()
            .collect()
    }

    /// Incoming edges of `id`.
    pub fn to(&self, id: ObjectId) -> Vec<Edge> {
        self.edges.iter().filter(|e| e.to == id).copied().collect()
    }

    /// Every edge touching `id` (outgoing first, then incoming), deterministic.
    pub fn relations_of(&self, id: ObjectId) -> Vec<Edge> {
        let mut out = self.from(id);
        out.extend(self.to(id).into_iter().filter(|e| e.from != id));
        out
    }

    /// Distinct neighbours of `id` (either direction), ascending.
    pub fn neighbors(&self, id: ObjectId) -> Vec<ObjectId> {
        let mut v: Vec<ObjectId> = self
            .edges
            .iter()
            .filter_map(|e| {
                if e.from == id {
                    Some(e.to)
                } else if e.to == id {
                    Some(e.from)
                } else {
                    None
                }
            })
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    /// Objects reachable from `id` over any relation (BFS, excludes `id`),
    /// ascending — the "context" of an object.
    pub fn reachable(&self, id: ObjectId) -> Vec<ObjectId> {
        let mut seen = std::collections::BTreeSet::new();
        let mut queue = std::collections::VecDeque::from([id]);
        while let Some(cur) = queue.pop_front() {
            for n in self.neighbors(cur) {
                if n != id && seen.insert(n) {
                    queue.push_back(n);
                }
            }
        }
        seen.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: ObjectId = ObjectId(1);
    const B: ObjectId = ObjectId(2);
    const C: ObjectId = ObjectId(3);
    const D: ObjectId = ObjectId(4);

    #[test]
    fn add_is_idempotent_and_sorted() {
        let mut g = Graph::new();
        g.add(B, Relation::Contains, C);
        g.add(A, Relation::Bounds, B);
        g.add(B, Relation::Contains, C);
        assert_eq!(g.len(), 2);
        assert_eq!(g.edges()[0].from, A, "edges sorted by (from, rel, to)");
    }

    #[test]
    fn forward_reverse_and_neighbor_queries() {
        let mut g = Graph::new();
        g.add(A, Relation::ConnectedTo, B); // wall-wall
        g.add(C, Relation::BelongsTo, A); // door in wall A
        g.add(A, Relation::Bounds, D); // wall bounds room
        assert_eq!(g.from(A).len(), 2);
        assert_eq!(g.to(A).len(), 1);
        assert_eq!(g.relations_of(A).len(), 3);
        assert_eq!(g.neighbors(A), vec![B, C, D]);
    }

    #[test]
    fn reachable_is_transitive_bfs() {
        let mut g = Graph::new();
        g.add(A, Relation::ConnectedTo, B);
        g.add(B, Relation::Bounds, C);
        g.add(D, Relation::Contains, D); // self-loop elsewhere, ignored from A
        assert_eq!(g.reachable(A), vec![B, C]);
        assert!(g.reachable(C).contains(&A), "reachability is undirected");
    }

    #[test]
    fn relation_tokens_round_trip() {
        for r in Relation::ALL {
            assert_eq!(Relation::parse(r.as_str()), Some(r));
        }
        assert_eq!(Relation::parse("belongsto"), Some(Relation::BelongsTo));
        assert_eq!(Relation::parse("x"), None);
    }
}
