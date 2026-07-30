//! `.vdb` v2 sections: the Object DB, Graph DB, and object audit log
//! (AI-native Phase 1). Written after the spatial index; absent in v1 files,
//! which load with empty stores.

use std::collections::BTreeMap;

use aadt_graph::{Edge, Graph, Relation};
use aadt_model::EntityId;
use aadt_object::{AuditEntry, AuditLog, EngObject, ObjectId, ObjectKind, ObjectStore};

use crate::buf::{Reader, VdbError, Writer};

/// Serialize objects + graph + audit.
pub fn write_sections(w: &mut Writer, objects: &ObjectStore, graph: &Graph, audit: &AuditLog) {
    w.u32(objects.len() as u32);
    for o in objects.iter() {
        w.u64(o.id.0);
        w.str(o.kind.as_str());
        w.f64(o.confidence as f64);
        w.u64(o.revision);
        w.u32(o.entities.len() as u32);
        for e in &o.entities {
            w.u64(e.0);
        }
        w.u32(o.props.len() as u32);
        for (k, v) in &o.props {
            w.str(k);
            w.str(v);
        }
    }

    w.u32(graph.len() as u32);
    for e in graph.edges() {
        w.u64(e.from.0);
        w.str(e.rel.as_str());
        w.u64(e.to.0);
    }

    w.u32(audit.len() as u32);
    for a in audit.entries() {
        w.u64(a.seq);
        w.u64(a.at_unix);
        w.str(&a.user);
        w.str(&a.action);
        w.u64(a.object.0);
        w.str(&a.prev);
        w.str(&a.new);
        w.bool(a.ai_suggested);
        w.bool(a.approved);
        w.str(&a.reason);
    }
}

/// Deserialize sections produced by [`write_sections`].
pub fn read_sections(r: &mut Reader) -> Result<(ObjectStore, Graph, AuditLog), VdbError> {
    let mut objects = ObjectStore::new();
    let n = r.u32()? as usize;
    for _ in 0..n {
        let id = ObjectId(r.u64()?);
        let kind_s = r.str()?;
        let kind = ObjectKind::parse(&kind_s)
            .ok_or_else(|| VdbError::Format(format!("unknown object kind {kind_s:?}")))?;
        let confidence = r.f64()? as f32;
        let revision = r.u64()?;
        let ne = r.u32()? as usize;
        let mut entities = Vec::with_capacity(ne);
        for _ in 0..ne {
            entities.push(EntityId(r.u64()?));
        }
        let np = r.u32()? as usize;
        let mut props = BTreeMap::new();
        for _ in 0..np {
            let k = r.str()?;
            let v = r.str()?;
            props.insert(k, v);
        }
        objects.restore(EngObject {
            id,
            kind,
            entities,
            props,
            confidence,
            revision,
        });
    }

    let mut graph = Graph::new();
    let n = r.u32()? as usize;
    for _ in 0..n {
        let from = ObjectId(r.u64()?);
        let rel_s = r.str()?;
        let rel = Relation::parse(&rel_s)
            .ok_or_else(|| VdbError::Format(format!("unknown relation {rel_s:?}")))?;
        let to = ObjectId(r.u64()?);
        graph.restore(Edge { from, rel, to });
    }

    let mut audit = AuditLog::new();
    let n = r.u32()? as usize;
    for _ in 0..n {
        audit.restore(AuditEntry {
            seq: r.u64()?,
            at_unix: r.u64()?,
            user: r.str()?,
            action: r.str()?,
            object: ObjectId(r.u64()?),
            prev: r.str()?,
            new: r.str()?,
            ai_suggested: r.bool()?,
            approved: r.bool()?,
            reason: r.str()?,
        });
    }

    Ok((objects, graph, audit))
}
