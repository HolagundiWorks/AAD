//! The confidence-gated mapping engine: embed + classify every element, assign
//! confident ones to their predicted layer + category, and leave the rest pending
//! for human reconciliation. All document mutations run inside a single
//! transaction so the whole pass is one atomic, undoable step.

use aadt_embed::{Classifier, ElementClass};
use aadt_model::{Document, EntityId, ModelSpaceId, SpaceCategory};

use crate::{MappingStatus, VecEntry, VectorDb};

/// Confidence gate for auto-assignment.
#[derive(Debug, Clone, Copy)]
pub struct Thresholds {
    /// Minimum confidence to auto-assign; below this the element is left pending.
    pub auto: f32,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self { auto: 0.75 }
    }
}

/// Outcome of a [`map_metadata`] pass.
#[derive(Debug, Clone, Default)]
pub struct MappingReport {
    /// Elements considered this pass (excludes ones with a manual decision).
    pub total: usize,
    /// Elements auto-assigned because the model was confident.
    pub auto_assigned: usize,
    /// Elements left pending for reconciliation.
    pub pending: usize,
    /// Ids of the pending elements, ascending.
    pub pending_ids: Vec<EntityId>,
    /// Manually-decided elements whose stored layer/category was re-applied
    /// (keeps confirmed/overridden decisions sticking across reopen).
    pub reapplied: usize,
}

/// Assign an element's layer and (optionally) move it into a category model space
/// on its current floor. `set_entity_space` retargets the layer to the category
/// default, so the specific predicted layer is applied last.
fn apply(
    d: &mut Document,
    id: EntityId,
    space: ModelSpaceId,
    layer: &str,
    category: Option<SpaceCategory>,
) {
    if let Some(cat) = category {
        if let Some(floor) = d.model_space(space).map(|ms| ms.floor) {
            if let Some(target) = d.space_on_floor(floor, cat) {
                d.set_entity_space(id, target);
            }
        }
    }
    d.set_entity_layer(id, layer);
}

/// Classify every element and apply the confidence gate. Elements with a manual
/// (`Confirmed`/`Overridden`) status in `db` are preserved untouched. The
/// document mutations are one undoable transaction; `db` is refreshed to match.
pub fn map_metadata(
    doc: &mut Document,
    db: &mut VectorDb,
    clf: &dyn Classifier,
    thr: Thresholds,
) -> MappingReport {
    doc.ensure_default_spaces();

    // Which entities to (re)map: everything except human-decided ones.
    let targets: Vec<(EntityId, ModelSpaceId)> = doc
        .iter_entities()
        .filter(|e| db.get(e.id).map(|x| !x.status.is_manual()).unwrap_or(true))
        .map(|e| (e.id, e.space))
        .collect();

    // Classify (immutable borrow) before mutating.
    let preds: Vec<(EntityId, ModelSpaceId, aadt_embed::Prediction)> = targets
        .iter()
        .filter_map(|(id, space)| doc.get(*id).map(|e| (*id, *space, clf.classify(e, doc))))
        .collect();

    // Manual (confirmed/overridden) decisions to re-apply so they survive reopen
    // even if the document wasn't re-saved after the decision.
    let manual: Vec<(EntityId, ModelSpaceId, String, Option<SpaceCategory>)> = db
        .entries()
        .iter()
        .filter(|e| e.status.is_manual())
        .filter_map(|e| {
            doc.get(e.id)
                .map(|ent| (e.id, ent.space, e.layer.clone(), e.category))
        })
        .collect();

    let auto = thr.auto;
    let (statuses, reapplied) = doc.transact("MAPMETA (auto-classify)", |d| {
        let statuses: Vec<MappingStatus> = preds
            .iter()
            .map(|(id, space, pred)| {
                if pred.confidence >= auto {
                    apply(d, *id, *space, pred.layer, pred.category);
                    MappingStatus::Auto
                } else {
                    MappingStatus::Pending
                }
            })
            .collect();
        let mut reapplied = 0;
        for (id, space, layer, category) in &manual {
            apply(d, *id, *space, layer, *category);
            reapplied += 1;
        }
        (statuses, reapplied)
    });

    let mut report = MappingReport {
        total: preds.len(),
        reapplied,
        ..Default::default()
    };
    for ((id, _space, pred), status) in preds.iter().zip(statuses) {
        match status {
            MappingStatus::Auto => report.auto_assigned += 1,
            MappingStatus::Pending => {
                report.pending += 1;
                report.pending_ids.push(*id);
            }
            _ => {}
        }
        db.upsert(VecEntry {
            id: *id,
            vector: pred.embedding.clone(),
            layer: pred.layer.to_string(),
            category: pred.category,
            confidence: pred.confidence,
            status,
        });
    }
    report
}

/// Accept a pending/auto prediction: apply its stored metadata (idempotent) and
/// mark it `Confirmed`. Returns `false` if the entity or its entry is missing.
pub fn accept(doc: &mut Document, db: &mut VectorDb, id: EntityId) -> bool {
    let Some(mut entry) = db.get(id).cloned() else {
        return false;
    };
    let Some(space) = doc.get(id).map(|e| e.space) else {
        return false;
    };
    let layer = entry.layer.clone();
    let category = entry.category;
    doc.transact("MAPMETA accept", |d| apply(d, id, space, &layer, category));
    entry.status = MappingStatus::Confirmed;
    db.upsert(entry);
    true
}

/// Override an element to a specific class's layer + category and mark it
/// `Overridden`. Returns `false` if the entity is missing.
pub fn set_class(doc: &mut Document, db: &mut VectorDb, id: EntityId, class: ElementClass) -> bool {
    let Some(space) = doc.get(id).map(|e| e.space) else {
        return false;
    };
    let layer = class.layer();
    let category = class.category();
    doc.transact("MAPMETA set", |d| apply(d, id, space, layer, category));
    let (vector, confidence) = db
        .get(id)
        .map(|e| (e.vector.clone(), e.confidence))
        .unwrap_or((Vec::new(), 0.0));
    db.upsert(VecEntry {
        id,
        vector,
        layer: layer.to_string(),
        category,
        confidence,
        status: MappingStatus::Overridden,
    });
    true
}

/// Override an element to an explicit layer (category left unchanged) and mark it
/// `Overridden`. Returns `false` if the entity is missing.
pub fn set_layer(doc: &mut Document, db: &mut VectorDb, id: EntityId, layer: &str) -> bool {
    if doc.get(id).is_none() {
        return false;
    }
    let layer_owned = layer.to_string();
    doc.transact("MAPMETA set layer", |d| {
        d.set_entity_layer(id, layer_owned.clone());
    });
    let (vector, confidence, category) = db
        .get(id)
        .map(|e| (e.vector.clone(), e.confidence, e.category))
        .unwrap_or((Vec::new(), 0.0, None));
    db.upsert(VecEntry {
        id,
        vector,
        layer: layer_owned,
        category,
        confidence,
        status: MappingStatus::Overridden,
    });
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use aadt_embed::{default_classifier, EMBED_DIM, MODEL_ID};
    use aadt_geometry::{Circle, Point2};
    use aadt_model::Geometry;

    fn column_doc() -> (Document, EntityId) {
        let mut doc = Document::new();
        doc.ensure_default_spaces();
        let id = doc.add_geometry(
            "0",
            Geometry::Circle(Circle::new(Point2::new(0.0, 0.0), 0.2)),
        );
        (doc, id)
    }

    fn category_of(doc: &Document, id: EntityId) -> SpaceCategory {
        let space = doc.get(id).unwrap().space;
        doc.model_space(space).unwrap().category
    }

    #[test]
    fn low_threshold_auto_assigns_layer_and_category() {
        let (mut doc, id) = column_doc();
        let mut db = VectorDb::new(MODEL_ID, EMBED_DIM);
        let report = map_metadata(
            &mut doc,
            &mut db,
            &default_classifier(),
            Thresholds { auto: 0.0 },
        );

        assert_eq!(report.auto_assigned, 1);
        assert_eq!(report.pending, 0);
        // A small circle reads as a column → A-COLS on the Structural space.
        assert_eq!(doc.get(id).unwrap().layer, "A-COLS");
        assert_eq!(category_of(&doc, id), SpaceCategory::Structural);
        let entry = db.get(id).unwrap();
        assert_eq!(entry.status, MappingStatus::Auto);
        assert_eq!(entry.vector.len(), EMBED_DIM);
    }

    #[test]
    fn mapping_is_undoable_as_one_step() {
        let (mut doc, id) = column_doc();
        let mut db = VectorDb::new(MODEL_ID, EMBED_DIM);
        map_metadata(
            &mut doc,
            &mut db,
            &default_classifier(),
            Thresholds { auto: 0.0 },
        );
        assert_eq!(doc.get(id).unwrap().layer, "A-COLS");
        doc.undo();
        assert_eq!(
            doc.get(id).unwrap().layer,
            "0",
            "undo restores the original layer"
        );
    }

    #[test]
    fn high_threshold_routes_to_pending_without_mutating() {
        let (mut doc, id) = column_doc();
        let mut db = VectorDb::new(MODEL_ID, EMBED_DIM);
        let report = map_metadata(
            &mut doc,
            &mut db,
            &default_classifier(),
            Thresholds { auto: 1.01 },
        );
        assert_eq!(report.pending, 1);
        assert_eq!(report.pending_ids, vec![id]);
        assert_eq!(
            doc.get(id).unwrap().layer,
            "0",
            "pending elements are untouched"
        );
        assert_eq!(db.get(id).unwrap().status, MappingStatus::Pending);
    }

    #[test]
    fn accept_applies_and_confirms_a_pending_element() {
        let (mut doc, id) = column_doc();
        let mut db = VectorDb::new(MODEL_ID, EMBED_DIM);
        map_metadata(
            &mut doc,
            &mut db,
            &default_classifier(),
            Thresholds { auto: 1.01 },
        );
        assert!(accept(&mut doc, &mut db, id));
        assert_eq!(doc.get(id).unwrap().layer, "A-COLS");
        assert_eq!(db.get(id).unwrap().status, MappingStatus::Confirmed);
    }

    #[test]
    fn manual_status_is_preserved_on_remap() {
        let (mut doc, id) = column_doc();
        let mut db = VectorDb::new(MODEL_ID, EMBED_DIM);
        // Override to Furniture, then re-map: the override must survive.
        assert!(set_class(&mut doc, &mut db, id, ElementClass::Furniture));
        assert_eq!(doc.get(id).unwrap().layer, "A-FURN");
        map_metadata(
            &mut doc,
            &mut db,
            &default_classifier(),
            Thresholds { auto: 0.0 },
        );
        assert_eq!(db.get(id).unwrap().status, MappingStatus::Overridden);
        assert_eq!(
            doc.get(id).unwrap().layer,
            "A-FURN",
            "override not clobbered"
        );
    }

    #[test]
    fn reapplies_manual_decision_after_doc_reverted() {
        // Simulates reopening a drawing whose confirmed decision was in the vector
        // db but not re-saved into the geometry: MAPMETA must restore it.
        let (mut doc, id) = column_doc();
        let mut db = VectorDb::new(MODEL_ID, EMBED_DIM);
        assert!(set_class(&mut doc, &mut db, id, ElementClass::Furniture));
        // Pretend the geometry came back on its original layer.
        doc.set_entity_layer(id, "0");
        assert_eq!(doc.get(id).unwrap().layer, "0");

        let report = map_metadata(
            &mut doc,
            &mut db,
            &default_classifier(),
            Thresholds { auto: 0.0 },
        );
        assert_eq!(report.reapplied, 1);
        assert_eq!(
            doc.get(id).unwrap().layer,
            "A-FURN",
            "manual decision restored"
        );
    }
}
