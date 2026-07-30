//! Promotion: run the deterministic detector over a drawing and assemble the
//! **Object DB + Graph DB** from its output.
//!
//! Deterministic end to end: same drawing → same objects (ids, properties) and
//! same edges. Detection confidence is `1.0` (rule-based, not AI); every
//! promoted object gets an audit row (`action = "promote"`, approved,
//! not-AI-suggested).

use std::collections::BTreeMap;

use aadt_detect::{detect, DetectionConfig, OpeningKind, StructuralKind};
use aadt_geometry::LENGTH_EPS;
use aadt_model::Document;
use aadt_object::{AuditLog, ObjectId, ObjectKind, ObjectStore};

use crate::{Graph, Relation};

/// The assembled result of a promotion pass.
#[derive(Debug, Clone, Default)]
pub struct Promoted {
    pub objects: ObjectStore,
    pub graph: Graph,
}

fn fmt_num(v: f64) -> String {
    format!("{v:.3}")
}

/// Promote a drawing's detected elements into objects + relationships.
///
/// Objects are created in a fixed order (walls, rooms, openings, structural,
/// furniture — each in detector order) so ids are stable for a given drawing.
pub fn promote_document(doc: &Document) -> Promoted {
    let det = detect(doc, &DetectionConfig::default());
    let rev = doc.revision();
    let mut objects = ObjectStore::new();
    let mut graph = Graph::new();

    // --- Walls -------------------------------------------------------------
    let mut wall_ids: Vec<ObjectId> = Vec::with_capacity(det.walls.len());
    for w in &det.walls {
        let mut props = BTreeMap::new();
        props.insert("length".into(), fmt_num(w.length));
        props.insert("thickness".into(), fmt_num(w.thickness));
        props.insert("layer".into(), w.layer.clone());
        // Wall height, if modelled as a 2D attribute on either face entity.
        for face in [w.faces.0, w.faces.1] {
            if let Some(h) = doc.get(face).and_then(|e| e.attribute("height")) {
                props.insert("height".into(), h.to_string());
                break;
            }
        }
        let id = objects.insert(
            ObjectKind::Wall,
            vec![w.faces.0, w.faces.1],
            props,
            1.0,
            rev,
        );
        wall_ids.push(id);
    }

    // Wall ↔ wall adjacency from the wall-graph topology (shared junctions).
    for (i, &(a1, b1, w1)) in det.graph.edges.iter().enumerate() {
        for &(a2, b2, w2) in det.graph.edges.iter().skip(i + 1) {
            if w1 == w2 {
                continue;
            }
            let shares = a1 == a2 || a1 == b2 || b1 == a2 || b1 == b2;
            if shares {
                if let (Some(&oa), Some(&ob)) = (wall_ids.get(w1), wall_ids.get(w2)) {
                    let (lo, hi) = if oa <= ob { (oa, ob) } else { (ob, oa) };
                    graph.add(lo, Relation::ConnectedTo, hi);
                }
            }
        }
    }

    // --- Rooms -------------------------------------------------------------
    let rooms = det.merged_rooms();
    let mut room_ids: Vec<ObjectId> = Vec::with_capacity(rooms.len());
    for r in &rooms {
        let mut props = BTreeMap::new();
        props.insert("area".into(), fmt_num(r.area));
        props.insert("perimeter".into(), fmt_num(r.perimeter));
        props.insert("layer".into(), r.layer.clone());
        let entities = if r.source.0 != u64::MAX {
            vec![r.source]
        } else {
            Vec::new()
        };
        let id = objects.insert(ObjectKind::Room, entities, props, 1.0, rev);
        room_ids.push(id);
    }

    // Wall bounds room: a wall's centerline midpoint lies on (near) the room
    // boundary — within the wall's thickness.
    for (w, &wid) in det.walls.iter().zip(&wall_ids) {
        let mid = w.centerline.midpoint();
        for (r, &rid) in rooms.iter().zip(&room_ids) {
            let near = r
                .boundary
                .segments()
                .iter()
                .any(|s| s.distance_to_point(mid) <= w.thickness + LENGTH_EPS);
            if near {
                graph.add(wid, Relation::Bounds, rid);
            }
        }
    }

    // --- Openings (doors / windows) ---------------------------------------
    for o in &det.openings {
        let kind = match o.kind {
            OpeningKind::Door => ObjectKind::Door,
            OpeningKind::Window => ObjectKind::Window,
        };
        let mut props = BTreeMap::new();
        props.insert("number".into(), o.number.to_string());
        props.insert("block".into(), o.block.clone());
        if let Some(w) = o.width {
            props.insert("width".into(), fmt_num(w));
        }
        let id = objects.insert(kind, vec![o.source], props, 1.0, rev);

        // Opening belongs to the nearest wall whose centerline passes close by.
        let mut best: Option<(ObjectId, f64)> = None;
        for (w, &wid) in det.walls.iter().zip(&wall_ids) {
            let d = w.centerline.distance_to_point(o.position);
            let reach = w.thickness.max(0.15) * 2.0;
            if d <= reach && best.map(|(_, bd)| d < bd).unwrap_or(true) {
                best = Some((wid, d));
            }
        }
        if let Some((wid, _)) = best {
            graph.add(id, Relation::BelongsTo, wid);
        }
    }

    // --- Structural (columns / beams) --------------------------------------
    for s in &det.structural {
        let kind = match s.kind {
            StructuralKind::Column => ObjectKind::Column,
            StructuralKind::Beam => ObjectKind::Beam,
        };
        let mut props = BTreeMap::new();
        props.insert("number".into(), s.number.to_string());
        props.insert("layer".into(), s.layer.clone());
        let id = objects.insert(kind, vec![s.source], props, 1.0, rev);

        // Rooms contain the columns that stand inside them.
        for (r, &rid) in rooms.iter().zip(&room_ids) {
            if r.boundary.contains_point(s.position) {
                graph.add(rid, Relation::Contains, id);
            }
        }
    }

    // --- Furniture ----------------------------------------------------------
    for f in &det.furniture {
        let mut props = BTreeMap::new();
        props.insert("number".into(), f.number.to_string());
        props.insert("block".into(), f.block.clone());
        let id = objects.insert(ObjectKind::Furniture, vec![f.source], props, 1.0, rev);
        for (r, &rid) in rooms.iter().zip(&room_ids) {
            if r.boundary.contains_point(f.position) {
                graph.add(rid, Relation::Contains, id);
            }
        }
    }

    Promoted { objects, graph }
}

/// Promote and append one audit row per created object.
pub fn promote_with_audit(doc: &Document, audit: &mut AuditLog) -> Promoted {
    let promoted = promote_document(doc);
    for o in promoted.objects.iter() {
        audit.record(
            "promote",
            o.id,
            "",
            &aadt_object::summarize(o),
            false, // deterministic detection, not an AI suggestion
            true,  // rule-based pipeline is auto-approved
            "deterministic detection promotion",
        );
    }
    promoted
}

#[cfg(test)]
mod tests {
    use super::*;
    use aadt_geometry::{Point2, Polyline, Segment};
    use aadt_model::Geometry;

    /// Two parallel wall faces (a wall), a closed room boundary, and a circle
    /// column inside the room.
    fn sample_doc() -> Document {
        let mut doc = Document::new();
        // Wall: two parallel faces 0.23 apart, 6 m long, on a wall layer.
        doc.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(6.0, 0.0))),
        );
        doc.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.23), Point2::new(6.0, 0.23))),
        );
        // Room: explicit closed boundary polyline.
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
        // Column: small circle inside the room, on a structural layer.
        doc.add_geometry(
            "S-COLS",
            Geometry::Circle(aadt_geometry::Circle::new(Point2::new(3.0, 2.0), 0.15)),
        );
        doc
    }

    #[test]
    fn promotes_walls_rooms_columns_with_props() {
        let doc = sample_doc();
        let p = promote_document(&doc);
        let walls = p.objects.by_kind(ObjectKind::Wall);
        let rooms = p.objects.by_kind(ObjectKind::Room);
        let cols = p.objects.by_kind(ObjectKind::Column);
        assert_eq!(walls.len(), 1, "one wall from the face pair");
        assert!(!rooms.is_empty(), "room detected");
        assert_eq!(cols.len(), 1, "one column");

        let w = walls[0];
        assert_eq!(w.entities.len(), 2, "wall references both face entities");
        assert!((w.prop("thickness").unwrap().parse::<f64>().unwrap() - 0.23).abs() < 0.02);
        assert!((w.prop("length").unwrap().parse::<f64>().unwrap() - 6.0).abs() < 0.1);
        assert_eq!(w.confidence, 1.0);
    }

    #[test]
    fn builds_bounds_and_contains_edges() {
        let doc = sample_doc();
        let p = promote_document(&doc);
        let wall = p.objects.by_kind(ObjectKind::Wall)[0].id;
        let col = p.objects.by_kind(ObjectKind::Column)[0].id;

        // Wall bounds at least one room.
        assert!(
            p.graph.from(wall).iter().any(|e| e.rel == Relation::Bounds),
            "wall → bounds → room edge exists"
        );
        // Some room contains the column.
        assert!(
            p.graph.to(col).iter().any(|e| e.rel == Relation::Contains),
            "room → contains → column edge exists"
        );
    }

    #[test]
    fn promotion_is_deterministic() {
        let doc = sample_doc();
        let a = promote_document(&doc);
        let b = promote_document(&doc);
        assert_eq!(a.objects.len(), b.objects.len());
        assert_eq!(a.graph.edges(), b.graph.edges());
        let ids_a: Vec<u64> = a.objects.iter().map(|o| o.id.0).collect();
        let ids_b: Vec<u64> = b.objects.iter().map(|o| o.id.0).collect();
        assert_eq!(ids_a, ids_b, "same drawing → same object ids");
    }

    #[test]
    fn audit_rows_are_written_per_object() {
        let doc = sample_doc();
        let mut audit = AuditLog::new();
        let p = promote_with_audit(&doc, &mut audit);
        assert_eq!(audit.len(), p.objects.len());
        assert!(audit.entries().iter().all(|e| e.action == "promote"));
        assert!(audit
            .entries()
            .iter()
            .all(|e| e.approved && !e.ai_suggested));
    }
}
